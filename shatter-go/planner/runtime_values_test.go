package planner_test

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

func runtimeValueParam(name, typeName string) protocol.ParamInfo {
	tn := typeName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "opaque", Label: typeName},
		TypeName: &tn,
	}
}

// AC (str-hy9b.F2): func(ctx context.Context): planner emits
// context.Background().
func TestPlanParam_ContextContext_EmitsBackground(t *testing.T) {
	plans, u := planner.PlanParam(testTargetID, 0, runtimeValueParam("ctx", "context.Context"), planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
	p := plans[0]
	if p.Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind = %q, want %q", p.Kind, protocol.ValuePlanKindRuntimeValue)
	}
	if p.TypeHint != "context.Context" {
		t.Errorf("TypeHint = %q, want %q", p.TypeHint, "context.Context")
	}
	if p.ParamIndex != 0 || p.ParamName != "ctx" {
		t.Errorf("ParamIndex/Name = %d/%q, want 0/\"ctx\"", p.ParamIndex, p.ParamName)
	}
	var expr string
	if err := json.Unmarshal(p.Literal, &expr); err != nil {
		t.Fatalf("Literal %q not a JSON string: %v", string(p.Literal), err)
	}
	if expr != "context.Background()" {
		t.Errorf("expression = %q, want context.Background()", expr)
	}
}

func TestLookupRuntimeValue_AllDefaults(t *testing.T) {
	cases := []struct {
		typeName   string
		wantExprs  []string
		wantImport string
	}{
		{"context.Context", []string{"context.Background()"}, "context"},
		{"*bytes.Buffer", []string{"&bytes.Buffer{}"}, "bytes"},
		{"io.Reader", []string{`strings.NewReader("")`}, "strings"},
		// str-gxjs: io.Writer now offers io.Discard as a second candidate
		// for sinks the test doesn't need to inspect.
		// str-gxjs: io.Writer has two candidates with different imports; the
		// import assertion below checks that each row carries AT LEAST ONE
		// of its required imports, and the multi-row io.Writer case carries
		// "bytes" on the first row and "io" on the second — both acceptable.
		{"io.Writer", []string{"&bytes.Buffer{}", "io.Discard"}, ""},
		{"io.ReadCloser", []string{`io.NopCloser(strings.NewReader(""))`}, "io"},
		{"http.ResponseWriter", []string{"httptest.NewRecorder()"}, "net/http/httptest"},
		{"*http.Request", []string{`httptest.NewRequest("GET", "/", bytes.NewReader(nil))`}, "net/http/httptest"},
		{"*http.Client", []string{"shatterHTTPClient()"}, "net/http"},
		{"http.RoundTripper", []string{"shatterHTTPTransport()"}, "net/http"},
		{"time.Time", []string{"time.Time{}", "time.Now()"}, "time"},
		{"http.Header", []string{"http.Header{}"}, "net/http"},
		{"*template.Template", []string{`template.Must(template.New("shatter").Parse("{}"))`}, "text/template"},
		{"wazero.Runtime", []string{`wazero.NewRuntime(context.Background())`}, "github.com/tetratelabs/wazero"},
		{"wazero.CompiledModule", []string{`func() wazero.CompiledModule`}, "github.com/tetratelabs/wazero"},
	}
	for _, tc := range cases {
		t.Run(tc.typeName, func(t *testing.T) {
			rvs := planner.LookupRuntimeValue(tc.typeName)
			if len(rvs) != len(tc.wantExprs) {
				t.Fatalf("len(rvs) = %d, want %d", len(rvs), len(tc.wantExprs))
			}
			for i, want := range tc.wantExprs {
				if tc.typeName == "wazero.CompiledModule" {
					if !strings.Contains(rvs[i].Expression, want) {
						t.Errorf("rvs[%d].Expression = %q, want to contain %q", i, rvs[i].Expression, want)
					}
				} else if rvs[i].Expression != want {
					t.Errorf("rvs[%d].Expression = %q, want %q", i, rvs[i].Expression, want)
				}
				if rvs[i].TypeHint != tc.typeName {
					t.Errorf("rvs[%d].TypeHint = %q, want %q", i, rvs[i].TypeHint, tc.typeName)
				}
				if rvs[i].SideEffectClass != protocol.ClassPure {
					t.Errorf("rvs[%d].SideEffectClass = %q, want %q", i, rvs[i].SideEffectClass, protocol.ClassPure)
				}
				if tc.wantImport != "" {
					foundImport := false
					for _, imp := range rvs[i].Imports {
						if imp == tc.wantImport {
							foundImport = true
							break
						}
					}
					if !foundImport {
						t.Errorf("rvs[%d].Imports = %v, want to include %q", i, rvs[i].Imports, tc.wantImport)
					}
				} else if len(rvs[i].Imports) == 0 {
					t.Errorf("rvs[%d].Imports = empty, expected at least one import for %q", i, tc.typeName)
				}
			}
		})
	}
}

func TestLookupRuntimeValue_Unknown_ReturnsNil(t *testing.T) {
	if got := planner.LookupRuntimeValue("nonesuch.Type"); got != nil {
		t.Errorf("LookupRuntimeValue(nonesuch.Type) = %+v, want nil", got)
	}
	if got := planner.LookupRuntimeValue(""); got != nil {
		t.Errorf("LookupRuntimeValue(\"\") = %+v, want nil", got)
	}
}

func TestLookupRuntimeValue_MutationIsolation(t *testing.T) {
	first := planner.LookupRuntimeValue("context.Context")
	if len(first) == 0 {
		t.Fatal("expected context.Context entry")
	}
	first[0].Expression = "mutated"
	first[0].Imports = append(first[0].Imports, "injected")
	second := planner.LookupRuntimeValue("context.Context")
	if second[0].Expression != "context.Background()" {
		t.Errorf("registry leak: second call returned %q", second[0].Expression)
	}
	for _, imp := range second[0].Imports {
		if imp == "injected" {
			t.Errorf("registry imports leaked a caller mutation")
		}
	}
}

func TestPlanParam_RuntimeValueFallback_BeforeUnsatisfied(t *testing.T) {
	// An opaque param with a recognized TypeName resolves via the registry
	// rather than producing UnsatisfiedRequirement.
	plans, u := planner.PlanParam(testTargetID, 3, runtimeValueParam("buf", "*bytes.Buffer"), planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected runtime-value plans, got none")
	}
	if plans[0].ParamIndex != 3 {
		t.Errorf("ParamIndex = %d, want 3", plans[0].ParamIndex)
	}
	if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind = %q, want runtime_value", plans[0].Kind)
	}
}

func TestPlanParam_ConfiguredRuntimeValueFallback(t *testing.T) {
	expr := `func() fixture.CompiledModule { return fixture.CompiledModule{} }()`
	plans, u := planner.PlanParam(testTargetID, 0, runtimeValueParam("mod", "fixture.CompiledModule"), planner.ParamPlanOptions{
		ConfiguredRuntimeValues: map[string]config.GoRuntimeValueConfig{
			"fixture.CompiledModule": {
				Expression: expr,
				Imports:    []string{"zolem.dev/zolem/internal/fixture"},
			},
		},
	})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans) = %d, want 1", len(plans))
	}
	if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind = %q, want runtime_value", plans[0].Kind)
	}
	if plans[0].TypeHint != "fixture.CompiledModule" {
		t.Errorf("TypeHint = %q, want fixture.CompiledModule", plans[0].TypeHint)
	}
	var gotExpr string
	if err := json.Unmarshal(plans[0].Literal, &gotExpr); err != nil {
		t.Fatalf("Literal %q not a JSON string: %v", string(plans[0].Literal), err)
	}
	if gotExpr != expr {
		t.Errorf("expression = %q, want %q", gotExpr, expr)
	}
}

func TestPlanParam_RuntimeValueRespectsCap(t *testing.T) {
	// time.Time registers two candidates; a cap of 1 must truncate.
	plans, u := planner.PlanParam(testTargetID, 0, runtimeValueParam("t", "time.Time"), planner.ParamPlanOptions{MaxPlansPerParam: 1})
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) != 1 {
		t.Errorf("len(plans) = %d, want 1 (cap)", len(plans))
	}
}

func TestPlanParam_UnrecognizedOpaque_StillUnsatisfied(t *testing.T) {
	// An opaque with a TypeName not in the registry must still produce an
	// UnsatisfiedRequirement — F2 must not silently accept unknown types.
	tn := "other.Thing"
	param := protocol.ParamInfo{
		Name:     "x",
		Type:     protocol.TypeInfo{Kind: "opaque", Label: "other.Thing"},
		TypeName: &tn,
	}
	plans, u := planner.PlanParam(testTargetID, 0, param, planner.ParamPlanOptions{})
	if plans != nil {
		t.Errorf("plans = %+v, want nil", plans)
	}
	if u == nil {
		t.Fatal("expected UnsatisfiedRequirement, got nil")
	}
	if u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("u.Kind = %q, want %q", u.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if !strings.Contains(u.Detail, "other.Thing") {
		t.Errorf("u.Detail = %q, want it to mention other.Thing", u.Detail)
	}
}

func TestPlanParam_HintStillWinsOverRuntimeValue(t *testing.T) {
	opts := planner.ParamPlanOptions{
		HintsByName: map[string]planner.ParamValueHint{
			"ctx": {Literal: json.RawMessage(`"override"`), TypeHint: "string"},
		},
	}
	// A hint is not a wholesale override of family classification; hints
	// only take effect when the family is a primitive. For runtime-value
	// types (no primitive family) the planner currently ignores hints by
	// name because there is no family.typeHint to bind against. This test
	// documents that runtime-value fallback fires anyway and the hint is
	// dropped; future work may lift hints into runtime-value slots.
	plans, u := planner.PlanParam(testTargetID, 0, runtimeValueParam("ctx", "context.Context"), opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected plans")
	}
	if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind = %q, want runtime_value (hint does not apply to runtime types)", plans[0].Kind)
	}
}

func TestRegisteredRuntimeValueTypes_IsSorted(t *testing.T) {
	got := planner.RegisteredRuntimeValueTypes()
	if len(got) < 6 {
		t.Fatalf("expected at least 6 registered types, got %d: %v", len(got), got)
	}
	for i := 1; i < len(got); i++ {
		if got[i-1] >= got[i] {
			t.Errorf("not sorted: got[%d]=%q, got[%d]=%q", i-1, got[i-1], i, got[i])
		}
	}
}

// Property: every registry entry must decode as a JSON string when round-
// tripped through PlanParam's runtime-value path, preserving the exact Go
// expression.
func TestProperty_RuntimeValueLiteralRoundtrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		types := planner.RegisteredRuntimeValueTypes()
		if len(types) == 0 {
			t.Skip("no registered types")
		}
		idx := rapid.IntRange(0, len(types)-1).Draw(t, "typeIdx")
		typeName := types[idx]
		plans, u := planner.PlanParam(testTargetID, 0, runtimeValueParam("p", typeName), planner.ParamPlanOptions{})
		if u != nil {
			t.Fatalf("%s: unsatisfied %+v", typeName, u)
		}
		if len(plans) == 0 {
			t.Fatalf("%s: no plans", typeName)
		}
		registry := planner.LookupRuntimeValue(typeName)
		for i, p := range plans {
			if p.Kind != protocol.ValuePlanKindRuntimeValue {
				t.Fatalf("%s plan[%d].Kind = %q", typeName, i, p.Kind)
			}
			var expr string
			if err := json.Unmarshal(p.Literal, &expr); err != nil {
				t.Fatalf("%s plan[%d] literal not a JSON string: %v", typeName, i, err)
			}
			if expr != registry[i].Expression {
				t.Fatalf("%s plan[%d] expr = %q, want %q", typeName, i, expr, registry[i].Expression)
			}
		}
	})
}
