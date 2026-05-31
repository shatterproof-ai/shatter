package planner_test

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

func sliceParam(name, typeName string, elem protocol.TypeInfo) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "array", Element: &elem},
		TypeName: &tn,
	}
}

func mapParam(name, typeName string, key, value protocol.TypeInfo) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name: name,
		Type: protocol.TypeInfo{
			Kind: "object",
			Fields: []protocol.ObjectField{
				{Name: "_key", Type: key},
				{Name: "_value", Type: value},
			},
		},
		TypeName: &tn,
	}
}

func structParam(name, typeName string, fields ...protocol.ObjectField) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "object", Fields: fields},
		TypeName: &tn,
	}
}

// AC1: Slice of primitive element emits at least two ValuePlans including
// zero-length and a one-element literal.
func TestPlanParam_SliceOfPrimitive_EmitsZeroAndOneElement(t *testing.T) {
	cases := []struct {
		name     string
		param    protocol.ParamInfo
		wantZero string
		wantOne  string
	}{
		{
			name:     "int_slice",
			param:    sliceParam("xs", "[]int", protocol.TypeInfo{Kind: "int"}),
			wantZero: "[]int{}",
			wantOne:  "[]int{0}",
		},
		{
			name:     "string_slice",
			param:    sliceParam("names", "[]string", protocol.TypeInfo{Kind: "str"}),
			wantZero: "[]string{}",
			wantOne:  `[]string{""}`,
		},
		{
			name:     "bool_slice",
			param:    sliceParam("flags", "[]bool", protocol.TypeInfo{Kind: "bool"}),
			wantZero: "[]bool{}",
			wantOne:  "[]bool{false}",
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
			var zeroFound, oneFound bool
			for _, p := range plans {
				if p.Kind != protocol.ValuePlanKindRuntimeValue {
					t.Errorf("plan Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
				}
				if p.TypeHint != *tc.param.TypeName {
					t.Errorf("plan TypeHint = %q, want %q", p.TypeHint, *tc.param.TypeName)
				}
				expr, err := decodeExpression(p.Literal)
				if err != nil {
					t.Errorf("plan Literal is not a JSON string: %v (%s)", err, string(p.Literal))
					continue
				}
				if expr == tc.wantZero {
					zeroFound = true
				}
				if expr == tc.wantOne {
					oneFound = true
				}
			}
			if !zeroFound {
				t.Errorf("expected a %q expression in %+v", tc.wantZero, plans)
			}
			if !oneFound {
				t.Errorf("expected a %q expression in %+v", tc.wantOne, plans)
			}
		})
	}
}

// AC2: Map with primitive key/value types emits at least one ValuePlan with
// literal entries.
func TestPlanParam_MapOfPrimitives_EmitsOneEntryLiteral(t *testing.T) {
	param := mapParam("m", "map[string]int",
		protocol.TypeInfo{Kind: "str"},
		protocol.TypeInfo{Kind: "int"},
	)
	plans, u := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatalf("expected at least one plan")
	}
	var oneEntryFound bool
	for _, p := range plans {
		if p.Kind != protocol.ValuePlanKindRuntimeValue {
			t.Errorf("plan Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
		}
		if p.TypeHint != "map[string]int" {
			t.Errorf("plan TypeHint = %q, want %q", p.TypeHint, "map[string]int")
		}
		expr, err := decodeExpression(p.Literal)
		if err != nil {
			t.Errorf("plan Literal is not a JSON string: %v (%s)", err, string(p.Literal))
			continue
		}
		if expr == `map[string]int{"": 0}` {
			oneEntryFound = true
		}
	}
	if !oneEntryFound {
		t.Errorf("expected a map[string]int{\"\": 0} expression in %+v", plans)
	}
}

// AC3: Struct-typed parameter emits one ValuePlan built by PlanComposite.
func TestPlanParam_Struct_EmitsCompositeLiteralPlan(t *testing.T) {
	param := structParam("req", "pkg.Req",
		protocol.ObjectField{Name: "Name", Type: protocol.TypeInfo{Kind: "str"}},
		protocol.ObjectField{Name: "N", Type: protocol.TypeInfo{Kind: "int"}},
	)
	plans, u := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
	p := plans[0]
	if p.Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("plan Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
	}
	if p.TypeHint != "pkg.Req" {
		t.Errorf("plan TypeHint = %q, want %q", p.TypeHint, "pkg.Req")
	}
	expr, err := decodeExpression(p.Literal)
	if err != nil {
		t.Fatalf("plan Literal is not a JSON string: %v (%s)", err, string(p.Literal))
	}
	if !strings.HasPrefix(expr, "pkg.Req{") {
		t.Errorf("expr = %q, want it to start with pkg.Req{", expr)
	}
	if !strings.Contains(expr, `Name: ""`) || !strings.Contains(expr, "N: 0") {
		t.Errorf("expr = %q, want it to mention Name and N fields", expr)
	}
}

// AC4: Unsupported element/field types fall through to complex_type
// UnsatisfiedRequirement without blocking sibling params.
func TestPlanParam_UnsupportedAggregate_DoesNotBlockSiblings(t *testing.T) {
	// []chan int — array of a kind PlanComposite cannot synthesize.
	badSlice := sliceParam("ch", "[]chan int", protocol.TypeInfo{Kind: "unknown"})
	// map[string]*sql.DB — map value is an opaque-inside-pointer that
	// PlanComposite rejects (see the pointer-to-opaque rule in composite.go).
	badMap := mapParam("m", "map[string]*sql.DB",
		protocol.TypeInfo{Kind: "str"},
		protocol.TypeInfo{Kind: "nullable", Inner: &protocol.TypeInfo{Kind: "opaque", Label: "sql.DB"}},
	)
	// struct with an opaque field.
	badStruct := structParam("r", "pkg.Req",
		protocol.ObjectField{Name: "DB", Type: protocol.TypeInfo{Kind: "opaque", Label: "sql.DB"}},
	)
	params := []protocol.ParamInfo{
		strParam("s"),
		badSlice,
		badMap,
		badStruct,
		intParam("n"),
	}
	matrix, unsat := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})
	if len(matrix) != 5 {
		t.Fatalf("len(matrix) = %d, want 5", len(matrix))
	}
	if len(matrix[0]) == 0 {
		t.Error("matrix[0] (string) must not be empty")
	}
	if matrix[1] != nil {
		t.Errorf("matrix[1] (bad slice) must be nil, got %+v", matrix[1])
	}
	if matrix[2] != nil {
		t.Errorf("matrix[2] (bad map) must be nil, got %+v", matrix[2])
	}
	if matrix[3] != nil {
		t.Errorf("matrix[3] (bad struct) must be nil, got %+v", matrix[3])
	}
	if len(matrix[4]) == 0 {
		t.Error("matrix[4] (int) must not be empty")
	}
	if len(unsat) != 3 {
		t.Fatalf("len(unsat) = %d, want 3; got %+v", len(unsat), unsat)
	}
	for i, u := range unsat {
		if u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
			t.Errorf("unsat[%d].Kind = %q, want %q", i, u.Kind, protocol.UnsatisfiedRequirementKindComplexType)
		}
		if u.Detail == "" {
			t.Errorf("unsat[%d].Detail must be non-empty", i)
		}
	}
}

func TestPlanParam_StructWithTemplateFieldUsesNilField(t *testing.T) {
	p := structParam("holder", "fixture.TemplateHolder",
		protocol.ObjectField{Name: "Name", Type: protocol.TypeInfo{Kind: "str"}},
		protocol.ObjectField{Name: "Template", Type: protocol.TypeInfo{Kind: "unknown", Label: "*template.Template"}},
	)

	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected aggregate plan")
	}
	expr, err := decodeExpression(plans[0].Literal)
	if err != nil {
		t.Fatalf("plan literal is not expression string: %v", err)
	}
	if !strings.Contains(expr, "Template: nil") {
		t.Fatalf("template field was not synthesized as nil: %q", expr)
	}
}

// AC5 extension: determinism. The aggregate planner returns the same plan
// slice for identical inputs across repeated calls.
func TestPlanAggregate_Deterministic(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		kinds := []string{"slice_int", "slice_string", "slice_bool", "map_str_int", "struct_simple"}
		shape := rapid.SampledFrom(kinds).Draw(rt, "shape")
		maxPlans := rapid.IntRange(1, 5).Draw(rt, "maxPlans")
		var p protocol.ParamInfo
		switch shape {
		case "slice_int":
			p = sliceParam("xs", "[]int", protocol.TypeInfo{Kind: "int"})
		case "slice_string":
			p = sliceParam("xs", "[]string", protocol.TypeInfo{Kind: "str"})
		case "slice_bool":
			p = sliceParam("xs", "[]bool", protocol.TypeInfo{Kind: "bool"})
		case "map_str_int":
			p = mapParam("m", "map[string]int",
				protocol.TypeInfo{Kind: "str"},
				protocol.TypeInfo{Kind: "int"})
		case "struct_simple":
			p = structParam("r", "pkg.T",
				protocol.ObjectField{Name: "A", Type: protocol.TypeInfo{Kind: "int"}},
				protocol.ObjectField{Name: "B", Type: protocol.TypeInfo{Kind: "str"}},
			)
		}
		a, aUnsat := planner.PlanAggregate(testTargetID, 0, p, maxPlans)
		b, bUnsat := planner.PlanAggregate(testTargetID, 0, p, maxPlans)
		if (aUnsat == nil) != (bUnsat == nil) {
			t.Fatalf("determinism broken: aUnsat=%v bUnsat=%v", aUnsat, bUnsat)
		}
		if len(a) != len(b) {
			t.Fatalf("determinism broken: len(a)=%d len(b)=%d", len(a), len(b))
		}
		for i := range a {
			if a[i].Kind != b[i].Kind ||
				a[i].TypeHint != b[i].TypeHint ||
				string(a[i].Literal) != string(b[i].Literal) {
				t.Fatalf("determinism broken at %d: %+v vs %+v", i, a[i], b[i])
			}
		}
	})
}

// A parameter whose TypeInfo is aggregate-shaped but missing a TypeName must
// not be classified as an aggregate (we cannot spell the Go type).
func TestPlanParam_AggregateWithoutTypeName_FallsThrough(t *testing.T) {
	// Array with an object element and no TypeName: not enough information
	// to emit an aggregate literal. Must hit complex_type unsatisfied.
	p := protocol.ParamInfo{
		Name: "xs",
		Type: protocol.TypeInfo{Kind: "array", Element: &protocol.TypeInfo{Kind: "int"}},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if plans != nil {
		t.Errorf("expected nil plans, got %+v", plans)
	}
	if u == nil || u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("expected complex_type unsat, got %+v", u)
	}
}

// Cap must bound the number of aggregate ValuePlans returned.
func TestPlanParam_SliceRespectsCap(t *testing.T) {
	p := sliceParam("xs", "[]int", protocol.TypeInfo{Kind: "int"})
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{MaxPlansPerParam: 1})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
}

// Byte slices must continue to route through the primitive family, not the
// aggregate planner.
func TestPlanParam_ByteSlice_StillPrimitive(t *testing.T) {
	p := byteSliceParam("buf")
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("byte slice produced no plans")
	}
	for _, pl := range plans {
		if pl.Kind == protocol.ValuePlanKindRuntimeValue {
			t.Errorf("byte slice must not use runtime_value kind; plan=%+v", pl)
		}
	}
}

func decodeExpression(raw json.RawMessage) (string, error) {
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return "", err
	}
	return s, nil
}
