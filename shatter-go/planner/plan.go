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

// TargetLookup resolves a target_id to the per-target context the planner
// needs. The handler supplies this closure so Plan stays agnostic of the
// caller's analysis cache layout.
//
// Callers must populate `Analysis` whenever the target is known. For method
// targets, callers must additionally populate `Target` (with Receiver shape
// and HasTypeParams) and `Constructors` (same-package constructor candidates
// whose TargetType matches the receiver type) to enable receiver-aware
// planning. Callers that only need free-function planning may leave Target
// and Constructors nil; PlanRequirements will fall back to the legacy free-
// function path for any non-method analysis.
//
// A nil return means the target was not analyzed and PlanRequirements should
// emit UnsatisfiedRequirementKindComplexType with detail "target not analyzed".
type TargetLookup func(targetID string) *protocol.TargetContext

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
	// MaxReceiverPlans caps the receiver plan count for a single method
	// target. Zero means DefaultMaxReceiverPlans (3). Free-function
	// requirements ignore this knob.
	MaxReceiverPlans int
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
// substitution scoped to a target. It is a planner output describing the
// intended mock substitution: a code generator is expected to paste
// Expression at every call site of QualifiedFunction inside the harness
// wrapping the target. MockSpec is a Go-internal artifact — it does not
// appear on the protocol wire because hint_config_v1 is a Go-only
// capability.
//
// IMPORTANT: emitting a MockSpec is the planner-side half of mock
// substitution. The execute-time half — a build-time symbol swap or
// launcher-level shim that actually replaces the call site at runtime — is
// NOT IMPLEMENTED in str-hy9b.G3 (str-ruw0). Anyone relying on user-
// supplied mocks taking effect at runtime today is unsupported. The
// substitution mechanism is tracked under follow-up issue str-8v66
// ("hint_config_v1 mocks substitution mechanism", blocked by str-ruw0).
type MockSpec struct {
	// TargetID is the planner-scoped target the mock applies to (e.g.
	// "example.com/pkg:Func").
	TargetID string
	// QualifiedFunction is the call site intended for replacement (e.g.
	// "fmt.Println"). Comes verbatim from the user's hint config.
	QualifiedFunction string
	// Expression is the Go source expression intended to replace the call
	// site once the executor-side substitution mechanism (str-8v66) is
	// wired.
	Expression string
}

// ResolveMockSpecs flattens the mocks map carried by hints into a sorted
// slice of MockSpec entries scoped to targetID. Returns an empty slice when
// hints carry no mocks. Ordering is alphabetical by QualifiedFunction so
// callers and tests see deterministic output.
//
// This is the planner-output half of str-hy9b.G3 AC2 — the artifact a
// future code generator will consume to substitute mocks at execute time.
// The substitution itself (build-time symbol swap or launcher-level shim)
// is NOT IMPLEMENTED yet; it is tracked under str-8v66 ("hint_config_v1
// mocks substitution mechanism", blocked by str-ruw0). Until str-8v66
// ships, calling this function and observing its output is safe and
// deterministic, but the resulting MockSpec entries do not affect runtime
// behaviour. Callers MUST NOT assume that a mock declared in the user's
// `.shatter/config.yaml` takes effect when execute is invoked today.
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

// PlanRequirements fans out PlanReceivers + PlanParams + Compose for every
// requirement.
//
// For each requirement the planner consults `lookup` for the per-target
// context. Free functions take the existing parameter-only path. Method
// targets — distinguished by `TargetContext.Target.Kind == TargetKindMethod`,
// or as a fallback by the legacy `(*Type).Method` qualified-name shape when
// only `Analysis` is populated — invoke `PlanReceivers` with the supplied
// constructor candidates and compose them with the parameter plans.
//
// Returns aggregated plans (ordered by requirement index) and aggregated
// unsatisfied requirements. Callers that need deterministic ordering across
// requirements should pass an already-ordered slice.
func PlanRequirements(
	requirements []protocol.InvocationRequirement,
	lookup TargetLookup,
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
	lookup TargetLookup,
	opts PlanRequirementsOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	ctx := lookup(req.TargetID)
	if ctx == nil || ctx.Analysis == nil {
		return nil, []protocol.UnsatisfiedRequirement{{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: req.TargetID,
			Detail:   "target not analyzed",
		}}
	}

	if isMethodTarget(ctx) {
		return planMethod(req, ctx, opts)
	}

	paramOpts := paramOptionsForRequirement(req.TargetID, opts)
	// str-4v9h: propagate interface impl candidates from TargetContext.
	if len(ctx.InterfaceImplsByParam) > 0 {
		paramOpts.InterfaceImplsByParam = ctx.InterfaceImplsByParam
	}
	return planWithGenericArgs(req, ctx, nil, false, paramOpts, opts)
}

// planMethod composes receiver plans (from PlanReceivers) with parameter
// plans for a method target. Returns NoConstructor when no receiver strategy
// applies — callers map this to AC #4's "planner gap" diagnostic.
func planMethod(
	req protocol.InvocationRequirement,
	ctx *protocol.TargetContext,
	opts PlanRequirementsOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	target := ctx.Target
	if target == nil {
		// Caller surfaced a method-shaped analysis but did not provide the
		// Go-internal DiscoveredTarget. Without Receiver shape we cannot
		// invoke PlanReceivers; emit NoConstructor with a detail that names
		// the upstream gap so producers can debug the lookup.
		return nil, []protocol.UnsatisfiedRequirement{{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: req.TargetID,
			Detail:   fmt.Sprintf("method target %s missing DiscoveredTarget context", ctx.Analysis.Name),
		}}
	}

	receiverPlans, receiverUnsat := PlanReceivers(*target, PlanOptions{
		SamePackageConstructors:        ctx.Constructors,
		ReceiverIsCompositeLiteralSafe: false,
		ReceiverRequiresConstruction:   ctx.ReceiverRequiresConstruction,
		MaxPlans:                       opts.MaxReceiverPlans,
	})
	if receiverUnsat != nil {
		return nil, []protocol.UnsatisfiedRequirement{*receiverUnsat}
	}

	paramOpts := ParamPlanOptions{MaxPlansPerParam: opts.MaxPlansPerParam}
	// str-4v9h: propagate interface impl candidates for method targets too.
	if len(ctx.InterfaceImplsByParam) > 0 {
		paramOpts.InterfaceImplsByParam = ctx.InterfaceImplsByParam
	}
	return planWithGenericArgs(req, ctx, receiverPlans, true, paramOpts, opts)
}

func paramOptionsForRequirement(targetID string, opts PlanRequirementsOptions) ParamPlanOptions {
	paramOpts := ParamPlanOptions{MaxPlansPerParam: opts.MaxPlansPerParam}
	if opts.PerTargetHints != nil {
		hints := opts.PerTargetHints(targetID)
		if len(hints.Defaults) > 0 {
			paramOpts.HintsByName = hints.Defaults
		}
		if len(hints.Generators) > 0 {
			paramOpts.GeneratorsByName = hints.Generators
		}
	}
	return paramOpts
}

func planWithGenericArgs(
	req protocol.InvocationRequirement,
	ctx *protocol.TargetContext,
	receiverPlans []ReceiverPlan,
	isMethod bool,
	paramOpts ParamPlanOptions,
	opts PlanRequirementsOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	typeArgSets, unsat := typeArgSetsForTarget(req.TargetID, ctx.Target)
	if unsat != nil {
		return nil, []protocol.UnsatisfiedRequirement{*unsat}
	}

	composeOpts := ComposeOptions{
		MaxPlans:  opts.MaxPlansPerTarget,
		BeamWidth: opts.MaxPlansPerTarget,
		IsMethod:  isMethod,
	}

	groups := make([][]protocol.InvocationPlan, 0, len(typeArgSets))
	var allUnsat []protocol.UnsatisfiedRequirement
	for _, typeArgs := range typeArgSets {
		params := substituteGenericParams(ctx.Analysis.Params, ctx.Target, typeArgs)
		paramMatrix, paramUnsat := PlanParams(req.TargetID, params, paramOpts)
		plans, groupUnsat := Compose(req.TargetID, receiverPlans, paramMatrix, paramUnsat, composeOpts)
		if len(groupUnsat) > 0 {
			allUnsat = append(allUnsat, groupUnsat...)
			continue
		}
		for i := range plans {
			plans[i].GenericTypeArgs = genericTypeNamesToStrings(typeArgs)
		}
		groups = append(groups, plans)
	}
	if len(groups) == 0 && len(allUnsat) > 0 {
		return nil, allUnsat
	}

	plans := interleaveGenericPlanGroups(groups)
	maxPlans := opts.MaxPlansPerTarget
	if maxPlans <= 0 {
		maxPlans = DefaultMaxPlansPerRequirement
	}
	if len(plans) > maxPlans {
		plans = plans[:maxPlans]
	}
	for i := range plans {
		plans[i].Priority = i
	}
	return plans, nil
}

func typeArgSetsForTarget(targetID string, target *protocol.DiscoveredTarget) ([][]GenericTypeName, *protocol.UnsatisfiedRequirement) {
	if target == nil || !target.HasTypeParams {
		return [][]GenericTypeName{{}}, nil
	}
	if len(target.TypeParams) == 0 {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindGenericUnconstrained,
			TargetID: targetID,
			Detail:   "target has type parameters but no constraints were discovered",
		}
	}
	return PlanGenericTypeArgSets(targetID, target.TypeParams)
}

func substituteGenericParams(params []protocol.ParamInfo, target *protocol.DiscoveredTarget, typeArgs []GenericTypeName) []protocol.ParamInfo {
	if target == nil || len(typeArgs) == 0 || len(target.TypeParams) == 0 {
		return params
	}
	subst := make(map[string]GenericTypeName, len(target.TypeParams))
	for i, tp := range target.TypeParams {
		if i < len(typeArgs) {
			subst[tp.Name] = typeArgs[i]
		}
	}

	out := make([]protocol.ParamInfo, len(params))
	for i, p := range params {
		out[i] = p
		if p.TypeName == nil {
			continue
		}
		typeArg, ok := subst[*p.TypeName]
		if !ok {
			continue
		}
		typeName := string(typeArg)
		out[i].TypeName = &typeName
		out[i].Type = typeInfoForGenericTypeArg(typeArg)
	}
	return out
}

func typeInfoForGenericTypeArg(typeArg GenericTypeName) protocol.TypeInfo {
	switch typeArg {
	case genericTypeString:
		return protocol.TypeInfo{Kind: "str"}
	case genericTypeInt, genericTypeInt64:
		return protocol.TypeInfo{Kind: "int"}
	case genericTypeFloat64:
		return protocol.TypeInfo{Kind: "float"}
	case genericTypeBool:
		return protocol.TypeInfo{Kind: "bool"}
	default:
		return protocol.TypeInfo{Kind: "unknown"}
	}
}

func genericTypeNamesToStrings(typeArgs []GenericTypeName) []string {
	if len(typeArgs) == 0 {
		return nil
	}
	out := make([]string, len(typeArgs))
	for i, typeArg := range typeArgs {
		out[i] = string(typeArg)
	}
	return out
}

func interleaveGenericPlanGroups(groups [][]protocol.InvocationPlan) []protocol.InvocationPlan {
	var plans []protocol.InvocationPlan
	for depth := 0; ; depth++ {
		added := false
		for _, group := range groups {
			if depth >= len(group) {
				continue
			}
			plans = append(plans, group[depth])
			added = true
		}
		if !added {
			break
		}
	}
	return plans
}

// isMethodTarget reports whether the planner should follow the method path.
// Prefers the explicit DiscoveredTarget.Kind when available (handler-built
// contexts always populate this for methods); falls back to the legacy
// qualified-name heuristic for callers that only carry FunctionAnalysis.
func isMethodTarget(ctx *protocol.TargetContext) bool {
	if ctx.Target != nil {
		return ctx.Target.Kind == protocol.TargetKindMethod
	}
	return isMethodQualifiedName(ctx.Analysis.Name)
}

// isMethodQualifiedName returns true when name is formatted like a Go method
// qualified name, e.g. "(*Service).Run" or "Service.Run".
func isMethodQualifiedName(name string) bool {
	if strings.HasPrefix(name, "(") {
		return true
	}
	return strings.Contains(name, ".")
}
