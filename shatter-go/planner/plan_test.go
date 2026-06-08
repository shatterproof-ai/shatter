package planner

import (
	"strings"
	"testing"

	"pgregory.net/rapid"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

func strPtr(s string) *string { return &s }

// analysisLookup wraps a `func(targetID) *FunctionAnalysis` into the new
// `TargetLookup` shape, populating only the Analysis field. Tests that only
// exercise free-function or method-shaped-by-name paths use this adapter to
// stay focused on the planner branch under test; tests that need the receiver
// planner pathway pass a richer TargetContext directly via richLookup below.
func analysisLookup(fn func(string) *protocol.FunctionAnalysis) TargetLookup {
	return func(id string) *protocol.TargetContext {
		analysis := fn(id)
		if analysis == nil {
			return nil
		}
		return &protocol.TargetContext{Analysis: analysis}
	}
}

// richLookup wraps a TargetContext-returning closure for tests that need to
// drive the receiver planner pathway with explicit DiscoveredTarget +
// constructor candidates.
func richLookup(fn func(string) *protocol.TargetContext) TargetLookup {
	return TargetLookup(fn)
}

func TestPlanRequirements_FreeFunctionProducesPlans(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "Add",
		Params: []protocol.ParamInfo{
			{Name: "a", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
			{Name: "b", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := analysisLookup(func(id string) *protocol.FunctionAnalysis {
		if id == "example.com/pkg:Add" {
			return analysis
		}
		return nil
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "example.com/pkg:Add"}},
		lookup,
		PlanRequirementsOptions{},
	)

	if len(unsat) != 0 {
		t.Fatalf("expected no unsatisfied requirements, got %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan for free function with int params")
	}
	for _, p := range plans {
		if p.TargetID != "example.com/pkg:Add" {
			t.Errorf("plan target_id mismatch: %q", p.TargetID)
		}
		if p.ReceiverKind != "" {
			t.Errorf("free-function plan should have empty receiver_kind, got %q", p.ReceiverKind)
		}
		if len(p.ArgumentPlans) != 2 {
			t.Errorf("expected 2 argument plans, got %d", len(p.ArgumentPlans))
		}
	}
}

func TestPlanRequirements_GenericFreeFunctionProducesInstantiatedPlans(t *testing.T) {
	t.Parallel()
	const targetID = "example.com/pkg:Identity"
	analysis := &protocol.FunctionAnalysis{
		Name: "Identity",
		Params: []protocol.ParamInfo{
			{Name: "v", Type: protocol.TypeInfo{Kind: "unknown"}, TypeName: strPtr("T")},
		},
	}
	target := &protocol.DiscoveredTarget{
		ID:            targetID,
		PackagePath:   "example.com/pkg",
		PackageName:   "pkg",
		SymbolName:    "Identity",
		QualifiedName: "Identity",
		Kind:          protocol.TargetKindFunction,
		Parameters:    analysis.Params,
		TypeParams:    []protocol.TypeParamInfo{{Name: "T", Constraint: "any"}},
		HasTypeParams: true,
	}
	lookup := richLookup(func(id string) *protocol.TargetContext {
		if id != targetID {
			return nil
		}
		return &protocol.TargetContext{Analysis: analysis, Target: target}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)

	if len(unsat) != 0 {
		t.Fatalf("expected no unsatisfied requirements, got %+v", unsat)
	}
	if len(plans) != 5 {
		t.Fatalf("len(plans) = %d, want 5; plans=%+v", len(plans), plans)
	}
	seen := map[string]bool{}
	for _, p := range plans {
		if len(p.GenericTypeArgs) != 1 {
			t.Fatalf("plan %+v GenericTypeArgs len = %d, want 1", p, len(p.GenericTypeArgs))
		}
		seen[p.GenericTypeArgs[0]] = true
		if len(p.ArgumentPlans) == 0 {
			t.Fatalf("plan %+v missing argument plans", p)
		}
		if p.ArgumentPlans[0].TypeHint != p.GenericTypeArgs[0] {
			t.Errorf("TypeHint = %q, want generic type arg %q", p.ArgumentPlans[0].TypeHint, p.GenericTypeArgs[0])
		}
	}
	for _, want := range []string{"string", "int", "bool", "int64", "float64"} {
		if !seen[want] {
			t.Errorf("missing generic instantiation %q in plans %+v", want, plans)
		}
	}
}

func TestPlanRequirements_TargetNotAnalyzed(t *testing.T) {
	t.Parallel()
	lookup := analysisLookup(func(string) *protocol.FunctionAnalysis { return nil })

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "example.com/pkg:Missing"}},
		lookup,
		PlanRequirementsOptions{},
	)

	if len(plans) != 0 {
		t.Fatalf("expected no plans, got %d", len(plans))
	}
	if len(unsat) != 1 || unsat[0].Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Fatalf("expected one ComplexType unsatisfied, got %+v", unsat)
	}
	if unsat[0].Detail != "target not analyzed" {
		t.Errorf("unexpected detail: %q", unsat[0].Detail)
	}
}

// TestPlanRequirements_MethodWithoutDiscoveredTargetReturnsNoConstructor covers
// the legacy/fallback path: a caller that hands the planner a method-shaped
// `FunctionAnalysis` (qualified name like `(*Service).Run`) without a
// `DiscoveredTarget` cannot drive receiver planning. The planner emits
// `NoConstructor` with a "missing DiscoveredTarget context" detail so the
// caller can distinguish a planner-context gap from a true no-constructor
// outcome (cf. AC #4 distinguishability).
func TestPlanRequirements_MethodWithoutDiscoveredTargetReturnsNoConstructor(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "(*Service).Run",
		Params: []protocol.ParamInfo{
			{Name: "x", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := analysisLookup(func(string) *protocol.FunctionAnalysis { return analysis })

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "example.com/pkg:(*Service).Run"}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(plans) != 0 {
		t.Fatalf("method target without DiscoveredTarget should yield no plans, got %d", len(plans))
	}
	if len(unsat) != 1 || unsat[0].Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Fatalf("expected NoConstructor unsatisfied, got %+v", unsat)
	}
	if !strings.Contains(unsat[0].Detail, "missing DiscoveredTarget context") {
		t.Errorf("expected detail to name the missing-context gap, got %q", unsat[0].Detail)
	}
}

// TestPlanRequirements_MethodWithSamePackageCtor exercises the receiver-aware
// planner path end-to-end: a method target with a same-package constructor
// produces an InvocationPlan whose ReceiverKind is "constructor:<FuncName>"
// AND whose ArgumentPlans cover every parameter (str-hy9b.H5 AC #2).
func TestPlanRequirements_MethodWithSamePackageCtor(t *testing.T) {
	t.Parallel()
	const targetID = "example.com/pkg:(*Service).DoIt"
	analysis := &protocol.FunctionAnalysis{
		Name: "(*Service).DoIt",
		Params: []protocol.ParamInfo{
			{Name: "x", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "DoIt",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "Service", IsPointer: true},
	}
	constructors := []protocol.ConstructorCandidate{
		{FuncName: "New", TargetType: "Service"},
	}
	lookup := richLookup(func(id string) *protocol.TargetContext {
		if id != targetID {
			return nil
		}
		return &protocol.TargetContext{
			Analysis:     analysis,
			Target:       target,
			Constructors: constructors,
		}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)

	if len(unsat) != 0 {
		t.Fatalf("expected no unsatisfied requirements, got %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan for method target with same-package ctor")
	}
	var sawCtor, sawParam bool
	for _, p := range plans {
		if p.TargetID != targetID {
			t.Errorf("plan target_id mismatch: %q", p.TargetID)
		}
		if p.ReceiverKind == "" {
			t.Errorf("method plan should have non-empty receiver_kind, got empty (label=%q)", p.Label)
		}
		if p.ReceiverKind == "constructor:New" {
			sawCtor = true
		}
		if len(p.ArgumentPlans) >= 1 {
			sawParam = true
		}
	}
	if !sawCtor {
		t.Errorf("expected at least one plan with receiver_kind=\"constructor:New\", got %+v", plans)
	}
	if !sawParam {
		t.Errorf("expected at least one plan with ≥1 argument plan, got %+v", plans)
	}
}

// TestPlanRequirements_MethodInterfaceReceiverShortCircuits regresses the
// guard that interface receivers cannot be constructed.
func TestPlanRequirements_MethodInterfaceReceiverShortCircuits(t *testing.T) {
	t.Parallel()
	const targetID = "example.com/pkg:(Greeter).Greet"
	analysis := &protocol.FunctionAnalysis{
		Name:   "(Greeter).Greet",
		Params: []protocol.ParamInfo{{Name: "name", Type: protocol.TypeInfo{Kind: "string"}, TypeName: strPtr("string")}},
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "Greet",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "Greeter", IsInterface: true},
	}
	lookup := richLookup(func(string) *protocol.TargetContext {
		return &protocol.TargetContext{Analysis: analysis, Target: target}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(plans) != 0 {
		t.Fatalf("interface receiver should yield no plans, got %d", len(plans))
	}
	if len(unsat) != 1 || unsat[0].Kind != protocol.UnsatisfiedRequirementKindInterfaceReceiver {
		t.Fatalf("expected InterfaceReceiver unsatisfied, got %+v", unsat)
	}
}

// TestPlanRequirements_MethodNoCtorFallsBackToZeroValue: a pointer-receiver
// method target with a known DiscoveredTarget but no matching constructor
// candidates produces a fallback zero-value plan (str-qo1.9). The wrapper's
// `zero_value` switch case unconditionally compiles for non-interface
// receivers, so the planner emits an executable last-resort plan rather
// than a NoConstructor unsatisfied requirement. Callers that need to
// distinguish "synthesized zero value" from "discovered constructor" can
// inspect plan.ReceiverKind ("zero_value" vs "constructor:Name").
func TestPlanRequirements_MethodNoCtorFallsBackToZeroValue(t *testing.T) {
	t.Parallel()
	const targetID = "example.com/pkg:(*Orphan).Run"
	analysis := &protocol.FunctionAnalysis{
		Name: "(*Orphan).Run",
		Params: []protocol.ParamInfo{
			{Name: "x", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "Run",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "Orphan", IsPointer: true},
	}
	lookup := richLookup(func(string) *protocol.TargetContext {
		return &protocol.TargetContext{Analysis: analysis, Target: target}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(unsat) != 0 {
		t.Fatalf("expected no unsatisfied, got %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatalf("expected fallback zero-value plan for pointer receiver without ctor")
	}
	for _, p := range plans {
		if p.ReceiverKind != "zero_value" {
			t.Errorf("plan.ReceiverKind=%q, want zero_value (fallback)", p.ReceiverKind)
		}
	}
}

// TestPlanRequirements_MethodInvariants is the formal-methods-policy property
// test for the receiver-aware planner pathway. For any method target with at
// least one same-package constructor and a primitive parameter list, every
// emitted InvocationPlan must:
//   - carry a non-empty ReceiverKind (no free-function bypass leaks),
//   - have ArgumentPlans whose length equals len(Analysis.Params),
//   - reference the supplied target_id verbatim.
//
// This locks the receiver-planner / parameter-planner / Compose composition
// against silent regressions where, e.g., a refactor drops the IsMethod flag
// and falls through to the free-function path.
func TestPlanRequirements_MethodInvariants(t *testing.T) {
	t.Parallel()
	rapid.Check(t, func(rt *rapid.T) {
		// Param kinds the planner's primitive family classifier accepts.
		// Mirrors planner/param.go::classifyParamFamily; values must match
		// protocol.TypeInfo.Kind, not Go source spellings.
		type kindSpelling struct{ kind, typeName string }
		paramKinds := []kindSpelling{
			{"int", "int"},
			{"str", "string"},
			{"bool", "bool"},
			{"float", "float64"},
		}
		paramCount := rapid.IntRange(0, 3).Draw(rt, "paramCount")
		ctorCount := rapid.IntRange(1, 3).Draw(rt, "ctorCount")
		isPointer := rapid.Bool().Draw(rt, "isPointer")

		params := make([]protocol.ParamInfo, paramCount)
		for i := range params {
			ks := rapid.SampledFrom(paramKinds).Draw(rt, "kind")
			params[i] = protocol.ParamInfo{
				Name:     "p" + string(rune('a'+i)),
				Type:     protocol.TypeInfo{Kind: ks.kind},
				TypeName: strPtr(ks.typeName),
			}
		}

		ctors := make([]protocol.ConstructorCandidate, ctorCount)
		for i := range ctors {
			ctors[i] = protocol.ConstructorCandidate{
				FuncName:   "New" + string(rune('A'+i)),
				TargetType: "Service",
			}
		}

		const targetID = "example.com/pkg:(*Service).Run"
		analysis := &protocol.FunctionAnalysis{Name: "(*Service).Run", Params: params}
		target := &protocol.DiscoveredTarget{
			ID:         targetID,
			SymbolName: "Run",
			Kind:       protocol.TargetKindMethod,
			Receiver:   &protocol.ReceiverShape{TypeName: "Service", IsPointer: isPointer},
		}
		lookup := richLookup(func(string) *protocol.TargetContext {
			return &protocol.TargetContext{Analysis: analysis, Target: target, Constructors: ctors}
		})

		plans, _ := PlanRequirements(
			[]protocol.InvocationRequirement{{TargetID: targetID}},
			lookup,
			PlanRequirementsOptions{},
		)
		if len(plans) == 0 {
			rt.Fatalf("expected ≥1 plan for method with %d ctors and %d primitive params", ctorCount, paramCount)
		}
		for i, p := range plans {
			if p.TargetID != targetID {
				rt.Fatalf("plans[%d].TargetID=%q want %q", i, p.TargetID, targetID)
			}
			if p.ReceiverKind == "" {
				rt.Fatalf("plans[%d].ReceiverKind is empty (label=%q)", i, p.Label)
			}
			if len(p.ArgumentPlans) != paramCount {
				rt.Fatalf("plans[%d].ArgumentPlans len=%d, want %d", i, len(p.ArgumentPlans), paramCount)
			}
		}
	})
}

func TestPlanRequirements_AggregatesAcrossRequirements(t *testing.T) {
	t.Parallel()
	add := &protocol.FunctionAnalysis{
		Name: "Add",
		Params: []protocol.ParamInfo{
			{Name: "a", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := analysisLookup(func(id string) *protocol.FunctionAnalysis {
		if id == "example.com/pkg:Add" {
			return add
		}
		return nil
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{
			{TargetID: "example.com/pkg:Add"},
			{TargetID: "example.com/pkg:Missing"},
		},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(plans) == 0 {
		t.Fatal("expected plans for the satisfied requirement")
	}
	if len(unsat) != 1 || unsat[0].TargetID != "example.com/pkg:Missing" {
		t.Fatalf("expected one unsatisfied for Missing, got %+v", unsat)
	}
}

// str-9b1q: a parameterized constructor with satisfiable primitive params
// must produce a plan with non-empty ConstructorArgPlans.
func TestPlanRequirements_ParameterizedConstructor_EmitsConstructorArgPlans(t *testing.T) {
	t.Parallel()

	const targetID = "example.com/pkg:(*Loader).WithNamespace"
	analysis := &protocol.FunctionAnalysis{
		Name:   "(*Loader).WithNamespace",
		Params: []protocol.ParamInfo{{Name: "ns", Type: protocol.TypeInfo{Kind: "str"}, TypeName: strPtr("string")}},
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "WithNamespace",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "Loader", IsPointer: true},
	}
	ctors := []protocol.ConstructorCandidate{{
		FuncName:       "NewLoader",
		TargetType:     "Loader",
		Parameters:     []protocol.ParamInfo{{Name: "dir", Type: protocol.TypeInfo{Kind: "str"}, TypeName: strPtr("string")}},
		ReturnsPointer: true,
	}}
	lookup := richLookup(func(string) *protocol.TargetContext {
		return &protocol.TargetContext{Analysis: analysis, Target: target, Constructors: ctors}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(unsat) > 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan")
	}

	// Find the constructor-backed plan.
	var ctorPlan *protocol.InvocationPlan
	for i, p := range plans {
		if p.ReceiverKind == "constructor:NewLoader" {
			ctorPlan = &plans[i]
			break
		}
	}
	if ctorPlan == nil {
		t.Fatalf("no plan with ReceiverKind=constructor:NewLoader found; plans=%+v", plans)
	}
	if len(ctorPlan.ConstructorArgPlans) != 1 {
		t.Fatalf("expected 1 ConstructorArgPlan, got %d; plan=%+v", len(ctorPlan.ConstructorArgPlans), ctorPlan)
	}
	cap := ctorPlan.ConstructorArgPlans[0]
	if cap.Kind != protocol.ValuePlanKindZero {
		t.Errorf("expected Zero kind for ctor arg plan, got %s", cap.Kind)
	}
	if cap.TypeHint != "string" {
		t.Errorf("expected type_hint=string, got %q", cap.TypeHint)
	}
}

func TestPlanRequirements_ConstructorRuntimeValueParamsDoNotConsumeInputPrefix(t *testing.T) {
	t.Parallel()

	const targetID = "example.com/pkg:(*SSEWriter).WriteEvent"
	analysis := &protocol.FunctionAnalysis{
		Name:   "(*SSEWriter).WriteEvent",
		Params: []protocol.ParamInfo{{Name: "event", Type: protocol.TypeInfo{Kind: "str"}, TypeName: strPtr("string")}},
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "WriteEvent",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "SSEWriter", IsPointer: true},
	}
	ctors := []protocol.ConstructorCandidate{{
		FuncName:   "NewSSEWriter",
		TargetType: "SSEWriter",
		Parameters: []protocol.ParamInfo{
			{Name: "w", Type: protocol.TypeInfo{Kind: "opaque"}, TypeName: strPtr("http.ResponseWriter")},
			{Name: "label", Type: protocol.TypeInfo{Kind: "str"}, TypeName: strPtr("string")},
		},
		ReturnsPointer: true,
	}}
	lookup := richLookup(func(string) *protocol.TargetContext {
		return &protocol.TargetContext{Analysis: analysis, Target: target, Constructors: ctors}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(unsat) > 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}

	var ctorPlan *protocol.InvocationPlan
	for i, p := range plans {
		if p.ReceiverKind == "constructor:NewSSEWriter" {
			ctorPlan = &plans[i]
			break
		}
	}
	if ctorPlan == nil {
		t.Fatalf("no plan with ReceiverKind=constructor:NewSSEWriter found; plans=%+v", plans)
	}
	if len(ctorPlan.ConstructorArgPlans) != 1 {
		t.Fatalf("expected 1 JSON-backed ConstructorArgPlan, got %d; plan=%+v", len(ctorPlan.ConstructorArgPlans), ctorPlan)
	}
	cap := ctorPlan.ConstructorArgPlans[0]
	if cap.ParamName != "label" {
		t.Errorf("constructor arg plan param_name=%q, want label", cap.ParamName)
	}
	if cap.ParamIndex != 1 {
		t.Errorf("constructor arg plan param_index=%d, want original constructor index 1", cap.ParamIndex)
	}
	if cap.TypeHint != "string" {
		t.Errorf("constructor arg plan type_hint=%q, want string", cap.TypeHint)
	}
}

// str-9b1q: a constructor with unsatisfiable params (interface, complex types)
// must NOT produce a constructor plan — fall back to zero value.
func TestPlanRequirements_UnsatisfiableConstructorParams_FallsBack(t *testing.T) {
	t.Parallel()

	const targetID = "example.com/pkg:(*Server).Listen"
	analysis := &protocol.FunctionAnalysis{
		Name: "(*Server).Listen",
	}
	target := &protocol.DiscoveredTarget{
		ID:         targetID,
		SymbolName: "Listen",
		Kind:       protocol.TargetKindMethod,
		Receiver:   &protocol.ReceiverShape{TypeName: "Server", IsPointer: true},
	}
	// Constructor with a complex/opaque param that is NOT satisfiable.
	ctors := []protocol.ConstructorCandidate{{
		FuncName:       "NewServer",
		TargetType:     "Server",
		Parameters:     []protocol.ParamInfo{{Name: "handler", Type: protocol.TypeInfo{Kind: "complex"}}},
		ReturnsPointer: true,
	}}
	lookup := richLookup(func(string) *protocol.TargetContext {
		return &protocol.TargetContext{Analysis: analysis, Target: target, Constructors: ctors}
	})

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: targetID}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(unsat) > 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	// Should have at least a fallback zero-value plan.
	if len(plans) == 0 {
		t.Fatal("expected at least a fallback plan")
	}
	// Must NOT have a constructor:NewServer plan.
	for _, p := range plans {
		if p.ReceiverKind == "constructor:NewServer" {
			t.Fatalf("should not emit constructor:NewServer for unsatisfiable params; plans=%+v", plans)
		}
	}
}
