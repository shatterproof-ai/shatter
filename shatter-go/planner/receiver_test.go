package planner_test

import (
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

func methodTarget(typeName string, isPointer bool) protocol.DiscoveredTarget {
	return protocol.DiscoveredTarget{
		ID:            "example.com/pkg:(*" + typeName + ").Method",
		PackagePath:   "example.com/pkg",
		PackageName:   "pkg",
		FilePath:      "/src/pkg/file.go",
		SymbolName:    "Method",
		QualifiedName: "(*" + typeName + ").Method",
		Kind:          protocol.TargetKindMethod,
		Receiver:      &protocol.ReceiverShape{TypeName: typeName, IsPointer: isPointer},
		Parameters:    []protocol.ParamInfo{},
		Results:       []protocol.TypeInfo{},
		Visibility:    "exported",
	}
}

func ctor(funcName, targetType string) protocol.ConstructorCandidate {
	return protocol.ConstructorCandidate{FuncName: funcName, TargetType: targetType}
}

// AC1: *Service with New() *Service -> top plan uses label "new_service".
func TestPlanReceivers_ServiceWithNewConstructor_TopIsNewService(t *testing.T) {
	target := methodTarget("Service", true)
	opts := planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{ctor("New", "Service")},
	}
	plans, unsat := planner.PlanReceivers(target, opts)
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatalf("expected at least one plan")
	}
	top := plans[0]
	if top.Kind != planner.ReceiverPlanKindSamePackageConstructor {
		t.Errorf("top.Kind = %v, want %v", top.Kind, planner.ReceiverPlanKindSamePackageConstructor)
	}
	if top.Label != "new_service" {
		t.Errorf("top.Label = %q, want %q", top.Label, "new_service")
	}
	if top.ReceiverKind != "constructor:New" {
		t.Errorf("top.ReceiverKind = %q, want %q", top.ReceiverKind, "constructor:New")
	}
	if top.Priority != 0 {
		t.Errorf("top.Priority = %d, want 0", top.Priority)
	}
}

// str-qo1.9: pointer receiver with no usable constructor falls back to a
// zero-value plan instead of producing a NoConstructor unsatisfied
// requirement. The wrapper's `zero_value` switch case unconditionally
// compiles for non-interface receivers (`var _recv T; _recv := &_recvVal`),
// so the planner emits an executable last-resort plan.
func TestPlanReceivers_NoConstructor_FallbackZeroValue(t *testing.T) {
	target := methodTarget("Counter", true)
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("plans = %+v, want exactly one fallback plan", plans)
	}
	top := plans[0]
	if top.Kind != planner.ReceiverPlanKindFallbackZeroValue {
		t.Errorf("top.Kind = %v, want %v", top.Kind, planner.ReceiverPlanKindFallbackZeroValue)
	}
	if top.ReceiverKind != planner.WrapperReceiverKindZeroValue {
		t.Errorf("top.ReceiverKind = %q, want %q", top.ReceiverKind, planner.WrapperReceiverKindZeroValue)
	}
	if top.Label != "fallback_zero_value_counter" {
		t.Errorf("top.Label = %q, want %q", top.Label, "fallback_zero_value_counter")
	}
}

// str-qo1.9: pointer receiver method whose only same-package constructor
// requires arguments must NOT yield a `constructor:NewFoo` plan — the
// wrapper template (str-qo1.14) drops parametric constructor switch cases,
// so emitting that plan would surface as a runtime "unknown receiver kind"
// failure. Instead the planner falls back to a zero-value plan.
func TestPlanReceivers_ParameterfulConstructor_FallsBackToZeroValue(t *testing.T) {
	target := methodTarget("Adapter", true)
	parameterfulCtor := protocol.ConstructorCandidate{
		FuncName:   "NewAdapter",
		TargetType: "Adapter",
		Parameters: []protocol.ParamInfo{{
			Name: "cfg",
			Type: protocol.TypeInfo{Kind: "struct"},
		}},
	}
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{parameterfulCtor},
	})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	for _, p := range plans {
		if p.ReceiverKind == "constructor:NewAdapter" {
			t.Fatalf("planner emitted constructor:NewAdapter for a parameterful constructor; the wrapper drops this case (str-qo1.14) and dispatch would fail with \"unknown receiver kind\"; plans=%+v", plans)
		}
		if p.Kind == planner.ReceiverPlanKindSamePackageConstructor {
			t.Fatalf("planner emitted same-package constructor plan despite arity mismatch; plans=%+v", plans)
		}
	}
	if len(plans) != 1 || plans[0].Kind != planner.ReceiverPlanKindFallbackZeroValue {
		t.Fatalf("expected single fallback zero-value plan, got %+v", plans)
	}
}

// str-qo1.9: parameterful constructor mixed with parameterless constructor —
// only the parameterless one is plan-eligible.
func TestPlanReceivers_MixedConstructors_KeepsOnlyParameterless(t *testing.T) {
	target := methodTarget("Adapter", true)
	withArgs := protocol.ConstructorCandidate{
		FuncName:   "NewAdapter",
		TargetType: "Adapter",
		Parameters: []protocol.ParamInfo{{Name: "cfg", Type: protocol.TypeInfo{Kind: "struct"}}},
	}
	noArgs := protocol.ConstructorCandidate{
		FuncName:   "DefaultAdapter",
		TargetType: "Adapter",
	}
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{withArgs, noArgs},
	})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("plans=%+v, want exactly one (only the parameterless ctor)", plans)
	}
	if plans[0].ReceiverKind != "constructor:DefaultAdapter" {
		t.Errorf("plans[0].ReceiverKind=%q, want constructor:DefaultAdapter", plans[0].ReceiverKind)
	}
}

// AC3: *bytes.Buffer receiver -> zero-value strategy selected as the top plan.
func TestPlanReceivers_BytesBuffer_ZeroValueSelected(t *testing.T) {
	target := methodTarget("bytes.Buffer", true)
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("plans = %+v, want exactly one", plans)
	}
	top := plans[0]
	if top.Kind != planner.ReceiverPlanKindUsefulZeroValue {
		t.Errorf("top.Kind = %v, want %v", top.Kind, planner.ReceiverPlanKindUsefulZeroValue)
	}
	if top.ReceiverKind != planner.WrapperReceiverKindZeroValue {
		t.Errorf("top.ReceiverKind = %q, want %q", top.ReceiverKind, planner.WrapperReceiverKindZeroValue)
	}
	if top.Label != "zero_value_bytes_buffer" {
		t.Errorf("top.Label = %q, want %q", top.Label, "zero_value_bytes_buffer")
	}
}

func TestPlanReceivers_StrategyOrder(t *testing.T) {
	target := methodTarget("bytes.Buffer", false)
	opts := planner.PlanOptions{
		Adapter:                        &planner.ReceiverHint{ReceiverKind: "adapter:httptest", Label: "http_test"},
		SamePackageConstructors:        []protocol.ConstructorCandidate{ctor("NewBuffer", "bytes.Buffer")},
		NearbyPackageConstructors:      []protocol.ConstructorCandidate{ctor("Make", "bytes.Buffer")},
		ReceiverIsCompositeLiteralSafe: true,
		Hint:                           &planner.ReceiverHint{ReceiverKind: "zero_value", Label: "operator_hint"},
		MaxPlans:                       10,
	}
	plans, unsat := planner.PlanReceivers(target, opts)
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	want := []planner.ReceiverPlanKind{
		planner.ReceiverPlanKindAdapter,
		planner.ReceiverPlanKindSamePackageConstructor,
		planner.ReceiverPlanKindNearbyPackageConstructor,
		planner.ReceiverPlanKindCompositeLiteral,
		planner.ReceiverPlanKindUsefulZeroValue,
		planner.ReceiverPlanKindHint,
	}
	if len(plans) != len(want) {
		t.Fatalf("len(plans) = %d, want %d; plans=%+v", len(plans), len(want), plans)
	}
	for i, kind := range want {
		if plans[i].Kind != kind {
			t.Errorf("plans[%d].Kind = %v, want %v", i, plans[i].Kind, kind)
		}
		if plans[i].Priority != i {
			t.Errorf("plans[%d].Priority = %d, want %d", i, plans[i].Priority, i)
		}
	}
}

func TestPlanReceivers_CapMaxPlans(t *testing.T) {
	target := methodTarget("Service", true)
	opts := planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{
			ctor("New", "Service"),
			ctor("NewWithConfig", "Service"),
			ctor("NewDefault", "Service"),
			ctor("MustNew", "Service"),
		},
		// No explicit MaxPlans -> uses DefaultMaxReceiverPlans (3).
	}
	plans, _ := planner.PlanReceivers(target, opts)
	if len(plans) != planner.DefaultMaxReceiverPlans {
		t.Fatalf("len(plans) = %d, want %d", len(plans), planner.DefaultMaxReceiverPlans)
	}
}

func TestPlanReceivers_ExplicitMaxPlans(t *testing.T) {
	target := methodTarget("Service", true)
	opts := planner.PlanOptions{
		Adapter:                 &planner.ReceiverHint{ReceiverKind: "adapter:x"},
		SamePackageConstructors: []protocol.ConstructorCandidate{ctor("New", "Service")},
		MaxPlans:                1,
	}
	plans, _ := planner.PlanReceivers(target, opts)
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
	if plans[0].Kind != planner.ReceiverPlanKindAdapter {
		t.Errorf("plans[0].Kind = %v, want adapter", plans[0].Kind)
	}
}

func TestPlanReceivers_InterfaceReceiver_Unsatisfied(t *testing.T) {
	target := methodTarget("Reader", false)
	target.Receiver.IsInterface = true
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{ctor("New", "Reader")},
	})
	if plans != nil {
		t.Errorf("plans = %+v, want nil", plans)
	}
	if unsat == nil || unsat.Kind != protocol.UnsatisfiedRequirementKindInterfaceReceiver {
		t.Errorf("unsat = %+v, want kind=%v", unsat, protocol.UnsatisfiedRequirementKindInterfaceReceiver)
	}
}

func TestPlanReceivers_GenericUnconstrained_Unsatisfied(t *testing.T) {
	target := methodTarget("Box", true)
	target.HasTypeParams = true
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{ctor("New", "Box")},
	})
	if plans != nil {
		t.Errorf("plans = %+v, want nil", plans)
	}
	if unsat == nil || unsat.Kind != protocol.UnsatisfiedRequirementKindGenericUnconstrained {
		t.Errorf("unsat = %+v, want kind=%v", unsat, protocol.UnsatisfiedRequirementKindGenericUnconstrained)
	}
}

func TestPlanReceivers_NonMethod_NoPlan(t *testing.T) {
	target := protocol.DiscoveredTarget{
		ID:          "example.com/pkg:Free",
		PackagePath: "example.com/pkg",
		PackageName: "pkg",
		SymbolName:  "Free",
		Kind:        protocol.TargetKindFunction,
	}
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{})
	if plans != nil || unsat != nil {
		t.Errorf("free function should yield (nil, nil); got (%+v, %+v)", plans, unsat)
	}
}

func TestPlanReceivers_CompositeLiteral_OnlyForNonPointerStruct(t *testing.T) {
	// Pointer receiver should skip composite-literal even if flag is set.
	target := methodTarget("Config", true)
	plans, _ := planner.PlanReceivers(target, planner.PlanOptions{
		ReceiverIsCompositeLiteralSafe: true,
	})
	for _, p := range plans {
		if p.Kind == planner.ReceiverPlanKindCompositeLiteral {
			t.Errorf("composite-literal plan should not appear for pointer receiver; got %+v", p)
		}
	}

	// Non-pointer receiver should accept composite-literal.
	target = methodTarget("Config", false)
	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		ReceiverIsCompositeLiteralSafe: true,
	})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 || plans[0].Kind != planner.ReceiverPlanKindCompositeLiteral {
		t.Errorf("want single composite-literal plan, got %+v", plans)
	}
}

func TestLabelForConstructor(t *testing.T) {
	cases := []struct {
		funcName, targetType, want string
	}{
		{"New", "Service", "new_service"},
		{"NewCounter", "Counter", "new_counter"},
		{"MustNew", "Buffer", "must_new_buffer"},
		{"MustNewBuffer", "Buffer", "must_new_buffer"},
		{"Default", "Config", "default_config"},
		{"DefaultClient", "Client", "default_client"},
	}
	for _, tc := range cases {
		target := methodTarget(tc.targetType, true)
		plans, _ := planner.PlanReceivers(target, planner.PlanOptions{
			SamePackageConstructors: []protocol.ConstructorCandidate{ctor(tc.funcName, tc.targetType)},
		})
		if len(plans) == 0 {
			t.Errorf("%s/%s: expected plan", tc.funcName, tc.targetType)
			continue
		}
		if plans[0].Label != tc.want {
			t.Errorf("%s/%s: label=%q want %q", tc.funcName, tc.targetType, plans[0].Label, tc.want)
		}
	}
}

// Rapid property: priorities are strictly increasing 0..n-1 and len never
// exceeds MaxPlans.
func TestPlanReceivers_PriorityAndCapInvariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		typeName := rapid.SampledFrom([]string{"Service", "bytes.Buffer", "Counter", "Config"}).Draw(rt, "typeName")
		isPointer := rapid.Bool().Draw(rt, "isPointer")
		maxPlans := rapid.IntRange(1, 6).Draw(rt, "maxPlans")
		ctorCount := rapid.IntRange(0, 5).Draw(rt, "ctorCount")
		ctors := make([]protocol.ConstructorCandidate, ctorCount)
		for i := range ctorCount {
			ctors[i] = protocol.ConstructorCandidate{
				FuncName:   "New" + string(rune('A'+i)),
				TargetType: typeName,
			}
		}

		target := methodTarget(typeName, isPointer)
		opts := planner.PlanOptions{
			Adapter:                        adapterHintOrNil(rt),
			SamePackageConstructors:        ctors,
			ReceiverIsCompositeLiteralSafe: rapid.Bool().Draw(rt, "compositeSafe"),
			Hint:                           hintOrNil(rt),
			MaxPlans:                       maxPlans,
		}
		plans, unsat := planner.PlanReceivers(target, opts)
		if unsat != nil {
			return // valid outcome
		}
		if len(plans) > maxPlans {
			t.Fatalf("len(plans)=%d exceeds MaxPlans=%d", len(plans), maxPlans)
		}
		for i, p := range plans {
			if p.Priority != i {
				t.Fatalf("plans[%d].Priority=%d, want %d", i, p.Priority, i)
			}
			if p.ReceiverKind == "" {
				t.Fatalf("plans[%d].ReceiverKind is empty", i)
			}
			if p.Label == "" {
				t.Fatalf("plans[%d].Label is empty", i)
			}
		}
	})
}

func adapterHintOrNil(rt *rapid.T) *planner.ReceiverHint {
	if rapid.Bool().Draw(rt, "hasAdapter") {
		return &planner.ReceiverHint{ReceiverKind: "adapter:x"}
	}
	return nil
}

func hintOrNil(rt *rapid.T) *planner.ReceiverHint {
	if rapid.Bool().Draw(rt, "hasHint") {
		return &planner.ReceiverHint{ReceiverKind: "zero_value"}
	}
	return nil
}
