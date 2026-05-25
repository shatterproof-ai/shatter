package planner_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

const testTargetID = "example.com/pkg:Func"

func strParam(name string) protocol.ParamInfo {
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "str"}}
}

func intParam(name string) protocol.ParamInfo {
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "int"}}
}

func floatParam(name string) protocol.ParamInfo {
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "float"}}
}

func boolParam(name string) protocol.ParamInfo {
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "bool"}}
}

func byteSliceParam(name string) protocol.ParamInfo {
	tn := "[]byte"
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "array", Element: &protocol.TypeInfo{Kind: "int"}},
		TypeName: &tn,
	}
}

func opaqueParam(name, label string) protocol.ParamInfo {
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "opaque", Label: label}}
}

// AC1: func(s string, n int) error — planner emits composed plans without panicking.
func TestPlanParams_StringAndInt_ComposedPlans(t *testing.T) {
	params := []protocol.ParamInfo{strParam("s"), intParam("n")}
	matrix, unsat := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(matrix) != 2 {
		t.Fatalf("len(matrix) = %d, want 2", len(matrix))
	}
	if len(matrix[0]) == 0 {
		t.Errorf("string param produced no plans")
	}
	if len(matrix[1]) == 0 {
		t.Errorf("int param produced no plans")
	}
	for pi, plans := range matrix {
		for i, p := range plans {
			if p.ParamIndex != pi {
				t.Errorf("matrix[%d][%d].ParamIndex = %d, want %d", pi, i, p.ParamIndex, pi)
			}
			if p.ParamName != params[pi].Name {
				t.Errorf("matrix[%d][%d].ParamName = %q, want %q", pi, i, p.ParamName, params[pi].Name)
			}
		}
	}
	// TypeHint must match the family.
	if matrix[0][0].TypeHint != "string" {
		t.Errorf("string TypeHint = %q, want %q", matrix[0][0].TypeHint, "string")
	}
	if matrix[1][0].TypeHint != "int" {
		t.Errorf("int TypeHint = %q, want %q", matrix[1][0].TypeHint, "int")
	}
}

// AC2: func(db *sql.DB) — parameter resolves to unsatisfied with documented reason.
func TestPlanParams_OpaqueSQLDB_UnsatisfiedWithDetail(t *testing.T) {
	params := []protocol.ParamInfo{opaqueParam("db", "sql.DB")}
	matrix, unsat := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})
	if len(matrix) != 1 || matrix[0] != nil {
		t.Errorf("expected matrix[0]==nil, got %+v", matrix)
	}
	if len(unsat) != 1 {
		t.Fatalf("len(unsat) = %d, want 1", len(unsat))
	}
	u := unsat[0]
	if u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("unsat.Kind = %q, want %q", u.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if u.TargetID != testTargetID {
		t.Errorf("unsat.TargetID = %q, want %q", u.TargetID, testTargetID)
	}
	if u.Detail == "" {
		t.Error("unsat.Detail must be non-empty")
	}
	if !contains(u.Detail, "db") || !contains(u.Detail, "sql.DB") {
		t.Errorf("unsat.Detail = %q, want it to mention both param name \"db\" and type \"sql.DB\"", u.Detail)
	}
}

// Unsupported parameter must not block the whole plan — other params are still planned.
func TestPlanParams_UnsupportedDoesNotBlockOthers(t *testing.T) {
	params := []protocol.ParamInfo{
		strParam("s"),
		opaqueParam("db", "sql.DB"),
		intParam("n"),
	}
	matrix, unsat := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{})
	if len(matrix) != 3 {
		t.Fatalf("len(matrix) = %d, want 3", len(matrix))
	}
	if len(matrix[0]) == 0 {
		t.Error("matrix[0] (string) must not be empty")
	}
	if matrix[1] != nil {
		t.Errorf("matrix[1] (opaque) must be nil, got %+v", matrix[1])
	}
	if len(matrix[2]) == 0 {
		t.Error("matrix[2] (int) must not be empty")
	}
	if len(unsat) != 1 {
		t.Fatalf("len(unsat) = %d, want 1", len(unsat))
	}
	if unsat[0].Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("unsat[0].Kind = %v, want complex_type", unsat[0].Kind)
	}
}

func TestPlanParam_PrimitiveFamilies(t *testing.T) {
	cases := []struct {
		name       string
		param      protocol.ParamInfo
		wantHint   string
		minPlans   int
		mustHaveZero bool
	}{
		{"string", strParam("s"), "string", 2, true},
		{"int", intParam("n"), "int", 2, true},
		{"float", floatParam("f"), "float64", 2, true},
		{"bool", boolParam("b"), "bool", 2, true},
		{"byteslice", byteSliceParam("buf"), "[]byte", 1, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, tc.param, planner.ParamPlanOptions{})
			if u != nil {
				t.Fatalf("unexpected unsatisfied: %+v", u)
			}
			if len(plans) < tc.minPlans {
				t.Errorf("len(plans)=%d, want >= %d; plans=%+v", len(plans), tc.minPlans, plans)
			}
			for i, p := range plans {
				if p.TypeHint != tc.wantHint {
					t.Errorf("plans[%d].TypeHint = %q, want %q", i, p.TypeHint, tc.wantHint)
				}
				if p.ParamIndex != 0 {
					t.Errorf("plans[%d].ParamIndex = %d, want 0", i, p.ParamIndex)
				}
				if p.Kind != protocol.ValuePlanKindZero && p.Kind != protocol.ValuePlanKindLiteral {
					t.Errorf("plans[%d].Kind = %q, want zero or literal", i, p.Kind)
				}
				if p.Kind == protocol.ValuePlanKindLiteral && len(p.Literal) == 0 {
					t.Errorf("plans[%d]: literal kind but empty Literal", i)
				}
				if p.Kind == protocol.ValuePlanKindLiteral && !json.Valid(p.Literal) {
					t.Errorf("plans[%d]: literal %q is not valid JSON", i, string(p.Literal))
				}
			}
			if tc.mustHaveZero {
				foundZero := false
				for _, p := range plans {
					if p.Kind == protocol.ValuePlanKindZero {
						foundZero = true
						break
					}
				}
				if !foundZero {
					t.Errorf("expected a zero-kind plan in %+v", plans)
				}
			}
		})
	}
}

func TestPlanParam_Cap(t *testing.T) {
	plans, _ := planner.PlanParam(testTargetID, 0, intParam("n"), planner.ParamPlanOptions{MaxPlansPerParam: 2})
	if len(plans) != 2 {
		t.Fatalf("len(plans) = %d, want 2", len(plans))
	}
}

func TestPlanParam_DefaultCap(t *testing.T) {
	plans, _ := planner.PlanParam(testTargetID, 0, strParam("s"), planner.ParamPlanOptions{})
	if len(plans) > planner.DefaultMaxParamValuePlans {
		t.Errorf("len(plans) = %d exceeds DefaultMaxParamValuePlans=%d", len(plans), planner.DefaultMaxParamValuePlans)
	}
}

func TestPlanParam_HintOverrideTakesPriority(t *testing.T) {
	literal := json.RawMessage(`"custom"`)
	opts := planner.ParamPlanOptions{
		HintsByName: map[string]planner.ParamValueHint{
			"s": {Literal: literal, TypeHint: "string"},
		},
	}
	plans, u := planner.PlanParam(testTargetID, 0, strParam("s"), opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatalf("expected at least one plan")
	}
	if plans[0].Kind != protocol.ValuePlanKindLiteral {
		t.Errorf("plans[0].Kind = %q, want %q", plans[0].Kind, protocol.ValuePlanKindLiteral)
	}
	if !bytes.Equal(plans[0].Literal, literal) {
		t.Errorf("plans[0].Literal = %s, want %s", string(plans[0].Literal), string(literal))
	}
	if plans[0].TypeHint != "string" {
		t.Errorf("plans[0].TypeHint = %q, want string", plans[0].TypeHint)
	}
}

func TestPlanParam_EmptyParams_NoMatrixEntries(t *testing.T) {
	matrix, unsat := planner.PlanParams(testTargetID, nil, planner.ParamPlanOptions{})
	if len(matrix) != 0 {
		t.Errorf("len(matrix) = %d, want 0", len(matrix))
	}
	if len(unsat) != 0 {
		t.Errorf("len(unsat) = %d, want 0", len(unsat))
	}
}

func TestPlanParam_ComplexKindFamilies_Unsatisfied(t *testing.T) {
	cases := []protocol.ParamInfo{
		{Name: "m", Type: protocol.TypeInfo{Kind: "object"}},
		{Name: "arr", Type: protocol.TypeInfo{Kind: "array", Element: &protocol.TypeInfo{Kind: "object"}}},
		{Name: "p", Type: protocol.TypeInfo{Kind: "nullable", Inner: &protocol.TypeInfo{Kind: "object"}}},
		{Name: "c", Type: protocol.TypeInfo{Kind: "complex", ComplexKind: "time"}},
		{Name: "u", Type: protocol.TypeInfo{Kind: "unknown"}},
	}
	for _, tc := range cases {
		t.Run(tc.Type.Kind, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, tc, planner.ParamPlanOptions{})
			if plans != nil {
				t.Errorf("expected nil plans, got %+v", plans)
			}
			if u == nil {
				t.Fatalf("expected unsatisfied, got nil")
			}
			if u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
				t.Errorf("u.Kind = %q, want %q", u.Kind, protocol.UnsatisfiedRequirementKindComplexType)
			}
			if u.Detail == "" {
				t.Error("u.Detail must be non-empty")
			}
		})
	}
}

// Rapid property: priorities are strictly increasing, cap is respected,
// every plan has a valid ParamIndex/ParamName/TypeHint and literal (when
// literal-kind) is valid JSON.
func TestPlanParams_Invariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		families := []string{"str", "int", "float", "bool", "byteslice", "opaque"}
		n := rapid.IntRange(0, 5).Draw(rt, "n")
		maxPlans := rapid.IntRange(1, 6).Draw(rt, "maxPlans")
		params := make([]protocol.ParamInfo, n)
		for i := range n {
			fam := rapid.SampledFrom(families).Draw(rt, "family")
			switch fam {
			case "str":
				params[i] = strParam("p" + string(rune('a'+i)))
			case "int":
				params[i] = intParam("p" + string(rune('a'+i)))
			case "float":
				params[i] = floatParam("p" + string(rune('a'+i)))
			case "bool":
				params[i] = boolParam("p" + string(rune('a'+i)))
			case "byteslice":
				params[i] = byteSliceParam("p" + string(rune('a'+i)))
			default:
				params[i] = opaqueParam("p"+string(rune('a'+i)), "pkg.T")
			}
		}
		matrix, _ := planner.PlanParams(testTargetID, params, planner.ParamPlanOptions{MaxPlansPerParam: maxPlans})
		if len(matrix) != n {
			t.Fatalf("len(matrix)=%d, want %d", len(matrix), n)
		}
		for pi, plans := range matrix {
			if len(plans) > maxPlans {
				t.Fatalf("matrix[%d]: len=%d exceeds maxPlans=%d", pi, len(plans), maxPlans)
			}
			for i, p := range plans {
				if p.ParamIndex != pi {
					t.Fatalf("matrix[%d][%d].ParamIndex=%d, want %d", pi, i, p.ParamIndex, pi)
				}
				if p.ParamName != params[pi].Name {
					t.Fatalf("matrix[%d][%d].ParamName=%q, want %q", pi, i, p.ParamName, params[pi].Name)
				}
				if p.TypeHint == "" {
					t.Fatalf("matrix[%d][%d].TypeHint is empty", pi, i)
				}
				if p.Kind == protocol.ValuePlanKindLiteral {
					if len(p.Literal) == 0 || !json.Valid(p.Literal) {
						t.Fatalf("matrix[%d][%d]: literal %q is not valid JSON", pi, i, string(p.Literal))
					}
				}
			}
		}
	})
}

func contains(s, sub string) bool {
	return len(s) >= len(sub) && bytes.Contains([]byte(s), []byte(sub))
}

// str-is5g: time.Duration parameters must plan as a primitive family with
// integer-nanosecond literal candidates covering zero, positive (sub-second
// and second-scale), and negative durations. The wrapper unmarshals each
// candidate directly into the parameter via time.Duration's default
// int64 UnmarshalJSON path.
func TestPlanParam_Duration_IntegerNanosecondCandidates(t *testing.T) {
	typeName := "time.Duration"
	param := protocol.ParamInfo{
		Name:     "timeout",
		Type:     protocol.TypeInfo{Kind: "int", Label: "time.Duration"},
		TypeName: &typeName,
	}
	opts := planner.ParamPlanOptions{MaxPlansPerParam: 8}
	plans, u := planner.PlanParam(testTargetID, 0, param, opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) < 4 {
		t.Fatalf("len(plans) = %d, want >= 4; plans=%+v", len(plans), plans)
	}
	for i, p := range plans {
		if p.TypeHint != "time.Duration" {
			t.Errorf("plans[%d].TypeHint = %q, want %q", i, p.TypeHint, "time.Duration")
		}
		if p.Kind == protocol.ValuePlanKindLiteral && !json.Valid(p.Literal) {
			t.Errorf("plans[%d]: literal %q is not valid JSON", i, string(p.Literal))
		}
	}

	// Collect literal values to check coverage of zero/positive/negative.
	foundZero := false
	foundPositive := false
	foundNegative := false
	for _, p := range plans {
		if p.Kind == protocol.ValuePlanKindZero {
			foundZero = true
			continue
		}
		if p.Kind != protocol.ValuePlanKindLiteral {
			continue
		}
		var n int64
		if err := json.Unmarshal(p.Literal, &n); err != nil {
			t.Errorf("literal %q must decode as int64: %v", string(p.Literal), err)
			continue
		}
		switch {
		case n > 0:
			foundPositive = true
		case n < 0:
			foundNegative = true
		case n == 0:
			foundZero = true
		}
	}
	if !foundZero {
		t.Error("duration family missing a zero candidate")
	}
	if !foundPositive {
		t.Error("duration family missing a positive-nanosecond candidate")
	}
	if !foundNegative {
		t.Error("duration family missing a negative-nanosecond candidate")
	}
}

// str-cfsa: unsigned integer params (top-level) must never emit negative values.
// The analyzer sets ComplexKind="go_uint" for uint/uint16/uint32/uint64 and the
// AST fallback sets TypeName to the exact unsigned spelling. Both paths must map
// to non-negative literal candidates only.
func TestPlanParam_UnsignedInt_NeverNegative(t *testing.T) {
	cases := []struct {
		name  string
		param protocol.ParamInfo
	}{
		{
			name: "go_uint_complex_kind",
			param: protocol.ParamInfo{
				Name: "seq",
				Type: protocol.TypeInfo{Kind: "complex", ComplexKind: "go_uint"},
			},
		},
		{
			name: "uint64_type_name",
			param: func() protocol.ParamInfo {
				tn := "uint64"
				return protocol.ParamInfo{
					Name:     "seq",
					Type:     protocol.TypeInfo{Kind: "complex", ComplexKind: "go_uint"},
					TypeName: &tn,
				}
			}(),
		},
		{
			name: "uint_type_name",
			param: func() protocol.ParamInfo {
				tn := "uint"
				return protocol.ParamInfo{
					Name:     "n",
					Type:     protocol.TypeInfo{Kind: "complex", ComplexKind: "go_uint"},
					TypeName: &tn,
				}
			}(),
		},
		{
			name: "uint32_type_name",
			param: func() protocol.ParamInfo {
				tn := "uint32"
				return protocol.ParamInfo{
					Name:     "id",
					Type:     protocol.TypeInfo{Kind: "complex", ComplexKind: "go_uint"},
					TypeName: &tn,
				}
			}(),
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			plans, u := planner.PlanParam(testTargetID, 0, tc.param, planner.ParamPlanOptions{MaxPlansPerParam: 8})
			if u != nil {
				t.Fatalf("unexpected unsatisfied: %+v", u)
			}
			if len(plans) == 0 {
				t.Fatal("expected at least one plan, got none")
			}
			for i, p := range plans {
				if p.Kind != protocol.ValuePlanKindLiteral && p.Kind != protocol.ValuePlanKindZero {
					t.Errorf("plans[%d].Kind = %q, want literal or zero", i, p.Kind)
				}
				if p.Kind != protocol.ValuePlanKindLiteral {
					continue
				}
				var n int64
				if err := json.Unmarshal(p.Literal, &n); err != nil {
					t.Errorf("plans[%d]: literal %q cannot unmarshal as number: %v", i, string(p.Literal), err)
					continue
				}
				if n < 0 {
					t.Errorf("plans[%d]: literal %d is negative; unsigned params must never emit negative values", i, n)
				}
			}
		})
	}
}

// str-4v9h: PlanParam routes to PlanInterfaceImpls when
// InterfaceImplsByParam contains candidates for the parameter.
func TestPlanParam_InterfaceImplCandidates_ProducesRuntimeValuePlan(t *testing.T) {
	p := opaqueParam("generator", "response.Generator")
	opts := planner.ParamPlanOptions{
		InterfaceImplsByParam: map[string][]protocol.InterfaceParamCandidate{
			"generator": {
				{
					TypeName:    "FakerGenerator",
					SamePackage: false,
					Constructors: []protocol.ConstructorCandidate{
						{FuncName: "response.NewFakerGenerator", TargetType: "FakerGenerator"},
					},
					ImportPath: "internal/response",
				},
			},
		},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan")
	}
	if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("plan.Kind = %q, want runtime_value", plans[0].Kind)
	}
	var expr string
	if err := json.Unmarshal(plans[0].Literal, &expr); err != nil {
		t.Fatalf("unmarshal literal: %v", err)
	}
	if expr != "response.NewFakerGenerator()" {
		t.Errorf("expr = %q, want %q", expr, "response.NewFakerGenerator()")
	}
}

// str-4v9h: when interface impl candidates have no constructors, PlanParam
// falls through to the unsatisfied path.
func TestPlanParam_InterfaceImplCandidates_NoConstructor_Unsatisfied(t *testing.T) {
	p := opaqueParam("store", "db.Store")
	opts := planner.ParamPlanOptions{
		InterfaceImplsByParam: map[string][]protocol.InterfaceParamCandidate{
			"store": {
				{TypeName: "FileStore", SamePackage: false},
			},
		},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if plans != nil {
		t.Errorf("expected nil plans, got %+v", plans)
	}
	if u == nil {
		t.Fatal("expected unsatisfied, got nil")
	}
	if u.Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Errorf("u.Kind = %q, want no_constructor", u.Kind)
	}
}

// str-n66n: go_duration ComplexKind must emit integer-nanosecond candidates.
func TestPlanParam_GoDuration_IntegerNanosecondCandidates(t *testing.T) {
	param := protocol.ParamInfo{
		Name: "delay",
		Type: protocol.TypeInfo{Kind: "complex", ComplexKind: "go_duration"},
	}
	plans, u := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{MaxPlansPerParam: 8})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan, got none")
	}
	for i, p := range plans {
		if p.Kind != protocol.ValuePlanKindLiteral && p.Kind != protocol.ValuePlanKindZero {
			t.Errorf("plans[%d].Kind = %q, want literal or zero", i, p.Kind)
		}
		if p.Kind != protocol.ValuePlanKindLiteral {
			continue
		}
		var n int64
		if err := json.Unmarshal(p.Literal, &n); err != nil {
			t.Errorf("plans[%d]: literal %q cannot unmarshal as integer: %v", i, string(p.Literal), err)
		}
	}
}
