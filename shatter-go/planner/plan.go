package planner

import (
	"fmt"
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
