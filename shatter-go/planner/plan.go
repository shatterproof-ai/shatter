package planner

import (
	"fmt"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// DefaultMaxPlansPerRequirement caps the InvocationPlan count emitted for one
// InvocationRequirement when PlanOptions.MaxPlansPerTarget is zero.
const DefaultMaxPlansPerRequirement = 5

// AnalysisLookup resolves the analyzed parameter metadata for a single
// target_id. The handler supplies this closure so Plan can stay agnostic of
// the caller's analysis cache layout. A nil return means the target has not
// been analyzed and Plan should emit UnsatisfiedRequirementKindComplexType
// with a "not analyzed" detail.
type AnalysisLookup func(targetID string) *protocol.FunctionAnalysis

// PlanRequirementsOptions bundles Compose-level knobs.
type PlanRequirementsOptions struct {
	// MaxPlansPerTarget caps the number of InvocationPlans emitted per
	// requirement. Zero means DefaultMaxPlansPerRequirement.
	MaxPlansPerTarget int
	// MaxPlansPerParam caps the ValuePlan count per parameter. Zero means
	// DefaultMaxParamValuePlans.
	MaxPlansPerParam int
	// PerTargetHints, when non-nil, returns the resolved hint_config_v1
	// entry for the supplied target_id. PlanRequirements consults it once
	// per requirement and threads the result into the per-parameter
	// planner. A nil resolver disables hint config consumption (the
	// behaviour before str-hy9b.G3).
	PerTargetHints func(targetID string) PerTargetHints
}

// PerTargetHints is the resolved hint_config_v1 entry for a single target.
// Fields mirror the FunctionConfig sections parsed by config.Load and are
// supplied by the caller's PerTargetHints resolver.
//
// Per the hint_config_v1 capability declaration in protocol/parity-matrix.yaml
// ("consumed internally by the Go .shatter/config.yaml loader; no wire
// probe"), PerTargetHints stays a Go-internal planner type — it does not
// flow over the protocol wire.
type PerTargetHints struct {
	// Defaults supplies per-parameter literal overrides (parameter name →
	// hint). Consumed by PlanParam as top-priority ValuePlans, taking
	// precedence over classifyParamFamily defaults.
	Defaults map[string]ParamValueHint
	// Generators names a runtime-value registry entry per parameter. The
	// planner consults the named generator before falling back to primitive
	// families.
	Generators map[string]string
	// Mocks maps a qualified function name (e.g. "fmt.Println") to the Go
	// source expression that should replace it at execute time. Consumed
	// via ResolveMockSpecs.
	Mocks map[string]string
}

// MockSpec is the planner's representation of a single hint_config_v1 mock
// substitution scoped to a target. A code generator pastes Expression at
// every call site of QualifiedFunction inside the harness wrapping the
// target. MockSpec is a Go-internal artifact — it does not appear on the
// protocol wire because hint_config_v1 is a Go-only capability.
type MockSpec struct {
	// TargetID is the planner-scoped target the mock applies to (e.g.
	// "example.com/pkg:Func").
	TargetID string
	// QualifiedFunction is the call site to be replaced (e.g.
	// "fmt.Println"). Comes verbatim from the user's hint config.
	QualifiedFunction string
	// Expression is the Go source expression that replaces the call site.
	Expression string
}

// ResolveMockSpecs flattens the mocks map carried by hints into a sorted
// slice of MockSpec entries scoped to targetID. Returns an empty slice when
// hints carry no mocks. Ordering is alphabetical by QualifiedFunction so
// callers and tests see deterministic output.
//
// This is the public planner artifact called out in str-hy9b.G3 AC2 — the
// "ValuePlan or adapter hook" the planner emits in response to user mock
// hints. Code generators consume MockSpec entries when constructing the
// harness for an InvocationPlan; runtime-time substitution lives in the
// executor/codegen path, which this planner-side artifact feeds.
func ResolveMockSpecs(targetID string, hints PerTargetHints) []MockSpec {
	if len(hints.Mocks) == 0 {
		return nil
	}
	specs := make([]MockSpec, 0, len(hints.Mocks))
	for qualified, expression := range hints.Mocks {
		specs = append(specs, MockSpec{
			TargetID:          targetID,
			QualifiedFunction: qualified,
			Expression:        expression,
		})
	}
	sort.Slice(specs, func(i, j int) bool {
		return specs[i].QualifiedFunction < specs[j].QualifiedFunction
	})
	return specs
}

// PlanRequirements fans out PlanParams + Compose for every requirement.
//
// For each requirement, Plan looks up the target's FunctionAnalysis via lookup
// and plans parameters only — this release handles free functions only;
// method targets get an UnsatisfiedRequirementKindNoConstructor with a
// "receiver planning deferred" detail (planner receiver wiring is tracked by
// str-hy9b.H2 follow-ups).
//
// Returns aggregated plans (ordered by requirement index) and aggregated
// unsatisfied requirements. Callers that need deterministic ordering across
// requirements should pass an already-ordered slice.
func PlanRequirements(
	requirements []protocol.InvocationRequirement,
	lookup AnalysisLookup,
	opts PlanRequirementsOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	var plans []protocol.InvocationPlan
	var unsatisfied []protocol.UnsatisfiedRequirement
	for _, req := range requirements {
		reqPlans, reqUnsat := planOne(req, lookup, opts)
		plans = append(plans, reqPlans...)
		unsatisfied = append(unsatisfied, reqUnsat...)
	}
	return plans, unsatisfied
}

func planOne(
	req protocol.InvocationRequirement,
	lookup AnalysisLookup,
	opts PlanRequirementsOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	analysis := lookup(req.TargetID)
	if analysis == nil {
		return nil, []protocol.UnsatisfiedRequirement{{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: req.TargetID,
			Detail:   "target not analyzed",
		}}
	}

	if isMethodQualifiedName(analysis.Name) {
		return nil, []protocol.UnsatisfiedRequirement{{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: req.TargetID,
			Detail:   fmt.Sprintf("method receiver planning is not wired yet for %s", analysis.Name),
		}}
	}

	paramOpts := ParamPlanOptions{MaxPlansPerParam: opts.MaxPlansPerParam}
	if opts.PerTargetHints != nil {
		hints := opts.PerTargetHints(req.TargetID)
		if len(hints.Defaults) > 0 {
			paramOpts.HintsByName = hints.Defaults
		}
		if len(hints.Generators) > 0 {
			paramOpts.GeneratorsByName = hints.Generators
		}
	}
	paramMatrix, paramUnsat := PlanParams(req.TargetID, analysis.Params, paramOpts)

	composeOpts := ComposeOptions{
		MaxPlans:  opts.MaxPlansPerTarget,
		BeamWidth: opts.MaxPlansPerTarget,
		IsMethod:  false,
	}
	return Compose(req.TargetID, nil, paramMatrix, paramUnsat, composeOpts)
}

// isMethodQualifiedName returns true when name is formatted like a Go method
// qualified name, e.g. "(*Service).Run" or "Service.Run".
func isMethodQualifiedName(name string) bool {
	if strings.HasPrefix(name, "(") {
		return true
	}
	return strings.Contains(name, ".")
}
