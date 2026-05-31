package planner_test

import (
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

func errorParam(name string) protocol.ParamInfo {
	tn := "error"
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "complex", ComplexKind: "error"},
		TypeName: &tn,
	}
}

func chanParam(name, typeName string) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "opaque", Label: typeName},
		TypeName: &tn,
	}
}

func funcParam(name, typeName string) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "unknown"},
		TypeName: &tn,
	}
}

// AC1: error param emits at least two ValuePlans: nil and fmt.Errorf("err").
func TestPlanParam_Error_EmitsNilAndFmtErrorf(t *testing.T) {
	plans, u := planner.PlanParam(testTargetID, 0, errorParam("err"), planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) < 2 {
		t.Fatalf("len(plans) = %d, want >= 2; plans=%+v", len(plans), plans)
	}
	var nilFound, errorfFound bool
	for _, p := range plans {
		if p.Kind != protocol.ValuePlanKindRuntimeValue {
			t.Errorf("plan Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
		}
		if p.TypeHint != "error" {
			t.Errorf("plan TypeHint = %q, want %q", p.TypeHint, "error")
		}
		if p.ParamIndex != 0 || p.ParamName != "err" {
			t.Errorf("plan identity = (%d,%q), want (0,%q)", p.ParamIndex, p.ParamName, "err")
		}
		expr, err := decodeExpression(p.Literal)
		if err != nil {
			t.Errorf("Literal is not a JSON string: %v (%s)", err, string(p.Literal))
			continue
		}
		switch expr {
		case "nil":
			nilFound = true
		case `fmt.Errorf("err")`:
			errorfFound = true
		}
	}
	if !nilFound {
		t.Errorf("missing nil ValuePlan")
	}
	if !errorfFound {
		t.Errorf("missing fmt.Errorf ValuePlan")
	}
}

// AC1 variant: error recognized without a TypeName, via ComplexKind.
func TestPlanParam_Error_NoTypeName_StillPlans(t *testing.T) {
	p := protocol.ParamInfo{
		Name: "err",
		Type: protocol.TypeInfo{Kind: "complex", ComplexKind: "error"},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) < 2 {
		t.Fatalf("len(plans) = %d, want >= 2", len(plans))
	}
}

// AC2: chan T param emits nil and make(chan T).
func TestPlanParam_Chan_EmitsNilAndMake(t *testing.T) {
	cases := []struct {
		name     string
		param    protocol.ParamInfo
		wantMake string
	}{
		{
			name:     "chan_int",
			param:    chanParam("ch", "chan int"),
			wantMake: "make(chan int)",
		},
		{
			name:     "chan_string",
			param:    chanParam("ch", "chan string"),
			wantMake: "make(chan string)",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, tc.param, planner.ParamPlanOptions{})
			if u != nil {
				t.Fatalf("unexpected unsatisfied: %+v", u)
			}
			if len(plans) < 2 {
				t.Fatalf("len(plans) = %d, want >= 2; plans=%+v", len(plans), plans)
			}
			var nilFound, makeFound bool
			for _, p := range plans {
				if p.Kind != protocol.ValuePlanKindRuntimeValue {
					t.Errorf("plan Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
				}
				if p.TypeHint != *tc.param.TypeName {
					t.Errorf("plan TypeHint = %q, want %q", p.TypeHint, *tc.param.TypeName)
				}
				expr, err := decodeExpression(p.Literal)
				if err != nil {
					continue
				}
				switch expr {
				case "nil":
					nilFound = true
				case tc.wantMake:
					makeFound = true
				}
			}
			if !nilFound {
				t.Errorf("missing nil ValuePlan")
			}
			if !makeFound {
				t.Errorf("missing %q ValuePlan", tc.wantMake)
			}
		})
	}
}

// AC2 variant: directional channels (chan<- T, <-chan T) cannot be make'd
// with the directional spelling; only a nil ValuePlan is emitted.
func TestPlanParam_DirectionalChan_NilOnly(t *testing.T) {
	cases := []string{"chan<- int", "<-chan string"}
	for _, tn := range cases {
		t.Run(tn, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, chanParam("ch", tn), planner.ParamPlanOptions{})
			if u != nil {
				t.Fatalf("unexpected unsatisfied: %+v", u)
			}
			if len(plans) != 1 {
				t.Fatalf("len(plans) = %d, want 1; plans=%+v", len(plans), plans)
			}
			expr, err := decodeExpression(plans[0].Literal)
			if err != nil || expr != "nil" {
				t.Errorf("plan expression = %q, want \"nil\"", expr)
			}
		})
	}
}

// AC3: top-level func-typed param emits a nil ValuePlan.
func TestPlanParam_Func_EmitsNil(t *testing.T) {
	cases := []protocol.ParamInfo{
		funcParam("cb", "func()"),
		funcParam("cb", "func(int) error"),
		funcParam("cb", "func(context.Context, string) (bool, error)"),
	}
	for _, p := range cases {
		t.Run(*p.TypeName, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
			if u != nil {
				t.Fatalf("unexpected unsatisfied: %+v", u)
			}
			if len(plans) != 1 {
				t.Fatalf("len(plans) = %d, want 1; plans=%+v", len(plans), plans)
			}
			if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
				t.Errorf("Kind = %q, want %q", plans[0].Kind, protocol.ValuePlanKindRuntimeValue)
			}
			if plans[0].TypeHint != *p.TypeName {
				t.Errorf("TypeHint = %q, want %q", plans[0].TypeHint, *p.TypeName)
			}
			expr, err := decodeExpression(plans[0].Literal)
			if err != nil || expr != "nil" {
				t.Errorf("plan expression = %q, want \"nil\"", expr)
			}
		})
	}
}

// AC4: an unsupported compound (here a chan with no TypeName spelling) emits
// a complex_type UnsatisfiedRequirement and does not block sibling params.
func TestPlanParams_UnsupportedCompound_DoesNotBlockSiblings(t *testing.T) {
	// Sibling 0: unsupported chan shape (opaque with chan label but no TypeName).
	unsupportedChan := protocol.ParamInfo{
		Name: "ch",
		Type: protocol.TypeInfo{Kind: "opaque", Label: "chan struct{}"},
	}
	// Sibling 1: supported error param.
	okError := errorParam("err")
	// Sibling 2: supported primitive int.
	okInt := protocol.ParamInfo{Name: "n", Type: protocol.TypeInfo{Kind: "int"}}

	params := []protocol.ParamInfo{unsupportedChan, okError, okInt}
	matrix, unsatisfied := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})

	if len(unsatisfied) != 1 {
		t.Fatalf("unsatisfied = %d, want 1; %+v", len(unsatisfied), unsatisfied)
	}
	if unsatisfied[0].Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("unsatisfied kind = %q, want %q", unsatisfied[0].Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if matrix[0] != nil {
		t.Errorf("matrix[0] (unsupported) = %+v, want nil", matrix[0])
	}
	if len(matrix[1]) < 2 {
		t.Errorf("matrix[1] (error) has %d plans, want >= 2", len(matrix[1]))
	}
	if len(matrix[2]) == 0 {
		t.Errorf("matrix[2] (int) has no plans")
	}
}

func TestPlanParam_InterfaceJSONEncodeCandidatesRequireHint(t *testing.T) {
	p := protocol.ParamInfo{
		Name: "v",
		Type: protocol.TypeInfo{Kind: "opaque", Label: "interface"},
	}

	if plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{}); u == nil {
		t.Fatalf("plain interface{} planned without JSON encode hint: plans=%+v", plans)
	}

	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{
		JSONEncodeInterfaceParams: map[string]bool{"v": true},
	})
	if u != nil {
		t.Fatalf("unexpected unsatisfied for hinted interface{}: %+v", u)
	}
	if len(plans) < 4 {
		t.Fatalf("len(plans) = %d, want bounded JSON candidate family; plans=%+v", len(plans), plans)
	}

	wantLiterals := map[string]bool{
		`{"key":"value"}`: false,
		`["value"]`:      false,
		`"value"`:        false,
		`true`:           false,
	}
	for _, plan := range plans {
		if plan.Kind != protocol.ValuePlanKindLiteral {
			t.Errorf("plan Kind = %q, want %q", plan.Kind, protocol.ValuePlanKindLiteral)
		}
		if plan.TypeHint != "interface{}" {
			t.Errorf("plan TypeHint = %q, want interface{}", plan.TypeHint)
		}
		if _, ok := wantLiterals[string(plan.Literal)]; ok {
			wantLiterals[string(plan.Literal)] = true
		}
	}
	for literal, found := range wantLiterals {
		if !found {
			t.Errorf("missing JSON interface candidate literal %s; plans=%+v", literal, plans)
		}
	}
}

// AC4 variant: a recognized chan + an unsupported-chan sibling still lets the
// recognized one plan.
func TestPlanParams_MixedChanSupport(t *testing.T) {
	params := []protocol.ParamInfo{
		chanParam("a", "chan int"),
		{Name: "b", Type: protocol.TypeInfo{Kind: "opaque", Label: "chan struct{}"}}, // no TypeName
	}
	matrix, unsatisfied := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})
	if len(matrix[0]) < 2 {
		t.Errorf("matrix[0] (chan int) has %d plans, want >= 2", len(matrix[0]))
	}
	if len(unsatisfied) != 1 {
		t.Fatalf("unsatisfied = %d, want 1", len(unsatisfied))
	}
}

// AC5: rapid invariant — for any single param, fallback ValuePlans never
// collide with primitive-family ValuePlans; i.e. the dispatch is mutually
// exclusive. A primitive-family param yields primitive candidates only
// (Kind=zero or Kind=literal), while a fallback param (error/chan/func)
// yields Kind=runtime_value.
func TestPlanParam_FallbackNeverCollidesWithPrimitive(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		primitiveKinds := []string{"str", "int", "float", "bool"}
		fallbackChoices := []struct {
			builder  func() protocol.ParamInfo
			typeHint string
		}{
			{func() protocol.ParamInfo { return errorParam("err") }, "error"},
			{func() protocol.ParamInfo { return chanParam("ch", "chan int") }, "chan int"},
			{func() protocol.ParamInfo { return funcParam("cb", "func()") }, "func()"},
		}

		if rapid.Bool().Draw(rt, "is_primitive") {
			kind := rapid.SampledFrom(primitiveKinds).Draw(rt, "kind")
			p := protocol.ParamInfo{Name: "x", Type: protocol.TypeInfo{Kind: kind}}
			plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
			if u != nil {
				rt.Fatalf("primitive %q unexpectedly unsatisfied: %+v", kind, u)
			}
			for _, pl := range plans {
				if pl.Kind == protocol.ValuePlanKindRuntimeValue {
					rt.Fatalf("primitive %q produced runtime_value kind: %+v", kind, pl)
				}
			}
		} else {
			choice := rapid.SampledFrom(fallbackChoices).Draw(rt, "fallback")
			plans, u := planner.PlanParam(testTargetID, 0, choice.builder(), planner.ParamPlanOptions{})
			if u != nil {
				rt.Fatalf("fallback %q unexpectedly unsatisfied: %+v", choice.typeHint, u)
			}
			if len(plans) == 0 {
				rt.Fatalf("fallback %q produced no plans", choice.typeHint)
			}
			for _, pl := range plans {
				if pl.Kind != protocol.ValuePlanKindRuntimeValue {
					rt.Fatalf("fallback %q plan has kind %q, want runtime_value", choice.typeHint, pl.Kind)
				}
				if pl.TypeHint != choice.typeHint {
					rt.Fatalf("fallback %q plan TypeHint = %q, want %q", choice.typeHint, pl.TypeHint, choice.typeHint)
				}
			}
		}
	})
}

// Regression: hints on an error parameter still take top priority — the
// hint path runs in the primitive-family branch and produces a literal
// ValuePlan, but an error param has no primitive family, so a hint alone
// cannot produce a ValuePlan; instead the fallback still applies.
// This test documents that behaviour: hints are silently ignored for
// fallback-shaped params today.
func TestPlanParam_ErrorHint_IgnoredButStillPlans(t *testing.T) {
	p := errorParam("err")
	opts := planner.ParamPlanOptions{
		HintsByName: map[string]planner.ParamValueHint{
			"err": {Literal: []byte(`"io.EOF"`)},
		},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) < 2 {
		t.Fatalf("len(plans) = %d, want >= 2", len(plans))
	}
	for _, pl := range plans {
		if pl.Kind != protocol.ValuePlanKindRuntimeValue {
			t.Errorf("plan Kind = %q, want runtime_value", pl.Kind)
		}
	}
}

// Regression: maxPlans cap applies to fallback plans.
func TestPlanParam_Fallback_RespectsMaxPlans(t *testing.T) {
	opts := planner.ParamPlanOptions{MaxPlansPerParam: 1}
	plans, u := planner.PlanParam(testTargetID, 0, errorParam("err"), opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
	expr, _ := decodeExpression(plans[0].Literal)
	if expr != "nil" {
		t.Errorf("first plan expression = %q, want \"nil\"", expr)
	}
}

// Regression: fallback detection does not shadow the runtime-value registry
// for context.Context (the TypeName does not match any fallback shape).
func TestPlanParam_ContextParam_StillUsesRegistry(t *testing.T) {
	tn := "context.Context"
	p := protocol.ParamInfo{
		Name:     "ctx",
		Type:     protocol.TypeInfo{Kind: "unknown"},
		TypeName: &tn,
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatalf("no plans")
	}
	expr, _ := decodeExpression(plans[0].Literal)
	if !strings.Contains(expr, "context.Background") {
		t.Errorf("first plan expression = %q, want context.Background()", expr)
	}
}
