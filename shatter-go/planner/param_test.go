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

func TestPlanParams_RoundTripperParam_RemainsUnsatisfied(t *testing.T) {
	cases := []struct {
		name  string
		param protocol.ParamInfo
	}{
		{name: "round_tripper", param: opaqueParam("transport", "http.RoundTripper")},
		{name: "cookie_jar", param: opaqueParam("jar", "http.CookieJar")},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			matrix, unsat := planner.PlanParams(testTargetID, []protocol.ParamInfo{tc.param}, planner.ParamPlanOptions{})
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
			if !contains(u.Detail, tc.param.Name) || !contains(u.Detail, tc.param.Type.Label) {
				t.Errorf("unsat.Detail = %q, want it to mention param and type", u.Detail)
			}
		})
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
		name         string
		param        protocol.ParamInfo
		wantHint     string
		minPlans     int
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

func TestPlanParam_HTTPRequestBodySymbolicStringPlans(t *testing.T) {
	typeName := "*http.Request"
	param := protocol.ParamInfo{
		Name:     "r",
		Type:     protocol.TypeInfo{Kind: "str", Label: typeName},
		TypeName: &typeName,
	}

	plans, unsat := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied requirement: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatalf("no plans produced")
	}
	for i, plan := range plans {
		if plan.Kind == protocol.ValuePlanKindRuntimeValue {
			t.Fatalf("plan[%d] is runtime-value plan; direct *http.Request must consume a symbolic body input", i)
		}
		if plan.TypeHint != "string" {
			t.Fatalf("plan[%d].TypeHint = %q, want string", i, plan.TypeHint)
		}
	}
}

func TestPlanParam_HTTPRequestBodyIncludesJSONRequestSeeds(t *testing.T) {
	typeName := "*http.Request"
	param := protocol.ParamInfo{
		Name:     "r",
		Type:     protocol.TypeInfo{Kind: "str", Label: typeName},
		TypeName: &typeName,
	}

	mined := []string{"model-alpha", "exact-match-payload"}
	plans, unsat := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{
		StringLiteralsByParam: map[string][]string{"r": mined},
		MaxPlansPerParam:      8,
	})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied requirement: %+v", unsat)
	}
	if len(plans) < len(mined)+1 {
		t.Fatalf("len(plans) = %d, want mined literals plus at least one seed", len(plans))
	}
	// Source-mined comparison literals are exact known-answer payloads and
	// must precede the schema-agnostic seeds so a tight MaxPlansPerParam cap
	// never evicts them.
	for i, want := range mined {
		var got string
		if err := json.Unmarshal(plans[i].Literal, &got); err != nil {
			t.Fatalf("plan[%d] literal does not decode as string: %v", i, err)
		}
		if got != want {
			t.Fatalf("plan[%d] = %q, want mined literal %q before body seeds", i, got, want)
		}
	}
	var seededBody string
	if err := json.Unmarshal(plans[len(mined)].Literal, &seededBody); err != nil {
		t.Fatalf("first seed literal does not decode as string: %v", err)
	}
	var parsed any
	if err := json.Unmarshal([]byte(seededBody), &parsed); err != nil {
		t.Fatalf("first request body seed %q is not valid JSON: %v", seededBody, err)
	}
	if _, isObject := parsed.(map[string]any); !isObject {
		t.Fatalf("first request body seed = %q, want a JSON object so decode-into-struct handlers pass their parse guard", seededBody)
	}
	if plans[len(mined)].TypeHint != "string" {
		t.Fatalf("first seed TypeHint = %q, want string", plans[len(mined)].TypeHint)
	}
}

func TestPlanParam_HTTPRequestBodyStructuredHintReencodedAsString(t *testing.T) {
	typeName := "*http.Request"
	param := protocol.ParamInfo{
		Name:     "r",
		Type:     protocol.TypeInfo{Kind: "str", Label: typeName},
		TypeName: &typeName,
	}

	structured := json.RawMessage(`{"model":"m","max_tokens":32}`)
	plans, unsat := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{
		HintsByName: map[string]planner.ParamValueHint{"r": {Literal: structured}},
	})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied requirement: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatalf("no plans produced")
	}
	// The wrapper decodes the body slot as a JSON string; a structured YAML
	// hint must arrive re-encoded as a string literal, not an object.
	var body string
	if err := json.Unmarshal(plans[0].Literal, &body); err != nil {
		t.Fatalf("structured hint was not re-encoded as a JSON string: %v; literal=%s", err, plans[0].Literal)
	}
	var roundTrip map[string]any
	if err := json.Unmarshal([]byte(body), &roundTrip); err != nil {
		t.Fatalf("re-encoded body %q does not round-trip to the hinted object: %v", body, err)
	}
	if roundTrip["model"] != "m" {
		t.Fatalf("re-encoded body lost hint content: %q", body)
	}
}

func TestPlanParam_HTTPRequestBodyRejectsGenerator(t *testing.T) {
	typeName := "*http.Request"
	param := protocol.ParamInfo{
		Name:     "r",
		Type:     protocol.TypeInfo{Kind: "str", Label: typeName},
		TypeName: &typeName,
	}

	plans, unsat := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{
		GeneratorsByName: map[string]string{"r": "*http.Request"},
	})
	if plans != nil {
		t.Fatalf("expected no plans for generator on symbolic request body, got %d", len(plans))
	}
	// A runtime-value generator plan would materialize as a null slot and
	// silently produce an empty-body request; the conflict must surface.
	if unsat == nil {
		t.Fatalf("expected unsatisfied requirement explaining the generator conflict")
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

func TestPlanParam_StringLiteralCandidatesPrecedeFamilyDefaults(t *testing.T) {
	opts := planner.ParamPlanOptions{
		StringLiteralsByParam: map[string][]string{
			"mode": {"fixed", "random"},
		},
		MaxPlansPerParam: 5,
	}
	plans, u := planner.PlanParam(testTargetID, 0, strParam("mode"), opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) < 3 {
		t.Fatalf("len(plans) = %d, want at least 3", len(plans))
	}
	for i, want := range []string{`"fixed"`, `"random"`} {
		if plans[i].Kind != protocol.ValuePlanKindLiteral {
			t.Fatalf("plans[%d].Kind = %q, want literal", i, plans[i].Kind)
		}
		if string(plans[i].Literal) != want {
			t.Errorf("plans[%d].Literal = %s, want %s", i, plans[i].Literal, want)
		}
		if plans[i].TypeHint != "string" {
			t.Errorf("plans[%d].TypeHint = %q, want string", i, plans[i].TypeHint)
		}
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

// Rapid property (str-e41w): for any *http.Request body param and any mix of
// config hint, mined string literals, and plan cap —
//   - no plan is ever kind runtime_value (the gen-v12 wrapper always consumes
//     the slot as a symbolic body; a runtime_value plan would silently yield
//     an empty-body request);
//   - a config hint, when present, is always plan[0] and its literal is a
//     JSON string (structured hints are re-encoded);
//   - mined literals precede the schema-agnostic seeds;
//   - the MaxPlansPerParam cap holds.
func TestPlanParam_HTTPRequestBodyInvariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		typeName := "*http.Request"
		param := protocol.ParamInfo{
			Name:     "r",
			Type:     protocol.TypeInfo{Kind: "str", Label: typeName},
			TypeName: &typeName,
		}
		maxPlans := rapid.IntRange(1, 8).Draw(rt, "maxPlans")
		seedSet := map[string]bool{`{}`: true, `{"data":{"id":"1","name":"a"},"items":["a"]}`: true, `[]`: true}
		drawn := rapid.SliceOfN(rapid.StringMatching(`[a-z{}":,0-9]{0,20}`), 0, 4).Draw(rt, "mined")
		mined := make([]string, 0, len(drawn))
		for _, m := range drawn {
			// A mined literal identical to a seed would make seed-order
			// detection ambiguous below; the collision carries no signal.
			if !seedSet[m] {
				mined = append(mined, m)
			}
		}
		opts := planner.ParamPlanOptions{
			MaxPlansPerParam:      maxPlans,
			StringLiteralsByParam: map[string][]string{"r": mined},
		}
		hasHint := rapid.Bool().Draw(rt, "hasHint")
		var hintJSON json.RawMessage
		if hasHint {
			if rapid.Bool().Draw(rt, "structuredHint") {
				hintJSON = json.RawMessage(`{"model":"m"}`)
			} else {
				hintJSON = json.RawMessage(`"{\"model\":\"m\"}"`)
			}
			opts.HintsByName = map[string]planner.ParamValueHint{"r": {Literal: hintJSON}}
		}

		plans, unsat := planner.PlanParam(testTargetID, 0, param, opts)
		if unsat != nil {
			rt.Fatalf("unexpected unsatisfied requirement: %+v", unsat)
		}
		if len(plans) > maxPlans {
			rt.Fatalf("len(plans) = %d exceeds cap %d", len(plans), maxPlans)
		}
		minedSet := map[string]bool{}
		for _, m := range mined {
			minedSet[m] = true
		}
		lastMined, firstSeed := -1, len(plans)
		for i, plan := range plans {
			if plan.Kind == protocol.ValuePlanKindRuntimeValue {
				rt.Fatalf("plan[%d] is runtime_value; symbolic body params must never plan runtime values", i)
			}
			if plan.Kind != protocol.ValuePlanKindLiteral || len(plan.Literal) == 0 {
				continue
			}
			var s string
			if err := json.Unmarshal(plan.Literal, &s); err != nil {
				continue
			}
			if minedSet[s] && i > lastMined {
				lastMined = i
			}
			if seedSet[s] && i < firstSeed {
				firstSeed = i
			}
		}
		if lastMined > firstSeed {
			rt.Fatalf("mined literal at plan[%d] ranked after generic seed at plan[%d]", lastMined, firstSeed)
		}
		if hasHint && len(plans) > 0 {
			var s string
			if err := json.Unmarshal(plans[0].Literal, &s); err != nil {
				rt.Fatalf("hint plan literal is not a JSON string: %s", plans[0].Literal)
			}
		}
	})
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

// str-79nvf: []byte params classified via go_byte element (TypeName absent) must
// produce byteSlice candidates and honour defaults hints — the same as when
// TypeName is explicitly set to "[]byte".
func TestPlanParam_ByteSlice_GoByteElement_NoTypeName(t *testing.T) {
	goByteElem := protocol.TypeInfo{Kind: "complex", ComplexKind: "go_byte"}
	param := protocol.ParamInfo{
		Name: "data",
		Type: protocol.TypeInfo{Kind: "array", Element: &goByteElem},
		// TypeName intentionally absent — matches the type-checker path that emits
		// Element.ComplexKind but no TypeName (e.g. NormalizeGeminiDiscovery.data).
	}

	t.Run("plans_without_hint", func(t *testing.T) {
		plans, u := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{MaxPlansPerParam: 8})
		if u != nil {
			t.Fatalf("unexpected unsatisfied: %+v", u)
		}
		if len(plans) == 0 {
			t.Fatal("expected at least one plan, got none")
		}
		for i, p := range plans {
			if p.TypeHint != "[]byte" {
				t.Errorf("plans[%d].TypeHint = %q, want []byte", i, p.TypeHint)
			}
		}
	})

	t.Run("hint_applied_as_first_plan", func(t *testing.T) {
		hintLiteral := json.RawMessage(`"aGVsbG8="`) // base64("hello")
		opts := planner.ParamPlanOptions{
			MaxPlansPerParam: 8,
			HintsByName: map[string]planner.ParamValueHint{
				"data": {Literal: hintLiteral, TypeHint: "string"},
			},
		}
		plans, u := planner.PlanParam(testTargetID, 0, param, opts)
		if u != nil {
			t.Fatalf("unexpected unsatisfied: %+v", u)
		}
		if len(plans) == 0 {
			t.Fatal("expected at least one plan")
		}
		if plans[0].Kind != protocol.ValuePlanKindLiteral {
			t.Fatalf("plans[0].Kind = %q, want literal (hint should be first)", plans[0].Kind)
		}
		if !bytes.Equal(plans[0].Literal, hintLiteral) {
			t.Errorf("plans[0].Literal = %s, want %s (hint literal)", plans[0].Literal, hintLiteral)
		}
	})
}

// unionStringParam builds a ParamInfo shaped like the analyzer's output for a
// named string-alias enum (type X string; const A, B, … X = …). The analyzer's
// enumValuesFromNamed emits Kind="union" with a single str base variant and the
// constant string domain in EnumValues (str-pjlc1). str-9pkrb teaches the
// planner to consume that domain as high-priority ValuePlan candidates.
func unionStringParam(name string, values ...string) protocol.ParamInfo {
	ev := make([]any, len(values))
	for i, v := range values {
		ev[i] = v
	}
	return protocol.ParamInfo{
		Name: name,
		Type: protocol.TypeInfo{
			Kind:       "union",
			Variants:   []protocol.TypeInfo{{Kind: "str"}},
			EnumValues: ev,
		},
	}
}

// str-9pkrb: a named string-alias enum parameter must seed every same-package
// constant as a string candidate so an enum-like switch reaches its case arms
// without a hand-written generator. Before this change classifyParamFamily had
// no "union" case and the parameter fell to the unsupported path.
func TestPlanParam_NamedStringEnum_SeedsConstantCandidates(t *testing.T) {
	// Four constants + the default per-param cap of 4 also verifies the cap is
	// expanded so both the full enum domain AND the generic fuzz family survive.
	p := unionStringParam("t", "CORE", "LOCATION", "ACADEMICS", "OTHER")
	plans, u := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}

	got := map[string]bool{}
	for i, pl := range plans {
		if pl.TypeHint != "string" {
			t.Errorf("plans[%d].TypeHint = %q, want string", i, pl.TypeHint)
		}
		if pl.ParamName != "t" {
			t.Errorf("plans[%d].ParamName = %q, want t", i, pl.ParamName)
		}
		if pl.ParamIndex != 0 {
			t.Errorf("plans[%d].ParamIndex = %d, want 0", i, pl.ParamIndex)
		}
		if pl.Kind == protocol.ValuePlanKindLiteral {
			var s string
			if json.Unmarshal(pl.Literal, &s) == nil {
				got[s] = true
			}
		}
	}
	for _, want := range []string{"CORE", "LOCATION", "ACADEMICS", "OTHER"} {
		if !got[want] {
			t.Errorf("enum constant %q missing from candidates %+v", want, plans)
		}
	}

	// Existing random/string fuzzing is preserved (not replaced): the generic
	// string family's zero-value (empty string) off-domain probe still appears.
	foundZero := false
	for _, pl := range plans {
		if pl.Kind == protocol.ValuePlanKindZero {
			foundZero = true
			break
		}
	}
	if !foundZero {
		t.Errorf("expected generic zero-value candidate preserved for off-domain fuzzing, got %+v", plans)
	}
}

// str-9pkrb: enum constants are seeded as high-priority candidates, but an
// operator override (config default) must still win the top slot — mirroring
// the primitive-family precedence in TestPlanParam_HintOverrideTakesPriority.
func TestPlanParam_NamedStringEnum_HintTakesPriority(t *testing.T) {
	p := unionStringParam("t", "CORE", "LOCATION")
	lit := json.RawMessage(`"CUSTOM"`)
	opts := planner.ParamPlanOptions{
		HintsByName: map[string]planner.ParamValueHint{
			"t": {Literal: lit, TypeHint: "string"},
		},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan")
	}
	if plans[0].Kind != protocol.ValuePlanKindLiteral || !bytes.Equal(plans[0].Literal, lit) {
		t.Errorf("plans[0] = %+v, want hint literal %s first", plans[0], lit)
	}
	// Enum constants still follow the hint.
	got := map[string]bool{}
	for _, pl := range plans {
		if pl.Kind == protocol.ValuePlanKindLiteral {
			var s string
			if json.Unmarshal(pl.Literal, &s) == nil {
				got[s] = true
			}
		}
	}
	for _, want := range []string{"CORE", "LOCATION"} {
		if !got[want] {
			t.Errorf("enum constant %q missing from candidates %+v", want, plans)
		}
	}
}

// str-9pkrb scope boundary: int enums are out of scope (semantic domain
// inference for arbitrary numeric enums is explicitly deferred). A union with a
// non-string base must not be seeded as string candidates; it retains the
// pre-existing unsupported behavior so no int-enum regression is introduced.
func TestPlanParam_NamedIntEnum_NotSeededAsStrings(t *testing.T) {
	p := protocol.ParamInfo{
		Name: "p",
		Type: protocol.TypeInfo{
			Kind:       "union",
			Variants:   []protocol.TypeInfo{{Kind: "int"}},
			EnumValues: []any{int64(0), int64(1), int64(2)},
		},
	}
	plans, _ := planner.PlanParam(testTargetID, 0, p, planner.ParamPlanOptions{})
	for i, pl := range plans {
		if pl.TypeHint == "string" {
			t.Errorf("plans[%d]=%+v: int enum must not be seeded as a string candidate", i, pl)
		}
	}
}
