package planner_test

import (
	"encoding/json"
	"sort"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

// strPtr returns &s.
func strPtr(s string) *string { return &s }

// AC3: a named generator on a parameter overrides primitive-family
// classification. Here `n` is an int (would normally yield literal/zero
// plans), but the user explicitly maps it to "context.Context" via the
// generators section. PlanParam must consult the registry instead.
func TestPlanParam_GeneratorOverridesPrimitiveFamily(t *testing.T) {
	t.Parallel()
	p := protocol.ParamInfo{
		Name:     "ctx",
		Type:     protocol.TypeInfo{Kind: "int"},
		TypeName: strPtr("int"),
	}
	opts := planner.ParamPlanOptions{
		GeneratorsByName: map[string]string{"ctx": "context.Context"},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one runtime-value plan from the generator")
	}
	if plans[0].Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("plan[0].Kind = %q, want %q", plans[0].Kind, protocol.ValuePlanKindRuntimeValue)
	}
	if plans[0].TypeHint != "context.Context" {
		t.Errorf("plan[0].TypeHint = %q, want context.Context", plans[0].TypeHint)
	}
	var expr string
	if err := json.Unmarshal(plans[0].Literal, &expr); err != nil {
		t.Fatalf("plan[0].Literal not a JSON string: %v", err)
	}
	if expr != "context.Background()" {
		t.Errorf("expression = %q, want context.Background()", expr)
	}
}

// AC3 corner case: a generator name that is NOT in the runtime-value
// registry must produce an UnsatisfiedRequirement so configuration typos
// surface rather than silently falling through to family planning.
func TestPlanParam_UnknownGenerator_Unsatisfied(t *testing.T) {
	t.Parallel()
	p := protocol.ParamInfo{
		Name:     "ctx",
		Type:     protocol.TypeInfo{Kind: "int"},
		TypeName: strPtr("int"),
	}
	opts := planner.ParamPlanOptions{
		GeneratorsByName: map[string]string{"ctx": "no.SuchType"},
	}
	plans, u := planner.PlanParam(testTargetID, 0, p, opts)
	if plans != nil {
		t.Errorf("plans = %+v, want nil", plans)
	}
	if u == nil {
		t.Fatal("expected UnsatisfiedRequirement")
	}
	if u.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("u.Kind = %q, want %q", u.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if !strings.Contains(u.Detail, "no.SuchType") {
		t.Errorf("detail %q must mention generator name", u.Detail)
	}
}

// AC1: HintsByName entries (the planner-side translation of
// FunctionConfig.Defaults) take priority over classifyParamFamily defaults.
// The first plan is the literal hint and the family candidates fill the
// remaining slots.
func TestPlanParam_DefaultHintBeatsFamilyDefaults(t *testing.T) {
	t.Parallel()
	hint := planner.ParamValueHint{Literal: json.RawMessage(`42`), TypeHint: "int"}
	opts := planner.ParamPlanOptions{HintsByName: map[string]planner.ParamValueHint{"n": hint}}
	plans, u := planner.PlanParam(testTargetID, 0, intParam("n"), opts)
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if len(plans) == 0 {
		t.Fatal("expected plans")
	}
	if plans[0].Kind != protocol.ValuePlanKindLiteral {
		t.Errorf("plans[0].Kind = %q, want %q", plans[0].Kind, protocol.ValuePlanKindLiteral)
	}
	if string(plans[0].Literal) != "42" {
		t.Errorf("plans[0].Literal = %s, want 42", string(plans[0].Literal))
	}
	if plans[0].TypeHint != "int" {
		t.Errorf("plans[0].TypeHint = %q, want int", plans[0].TypeHint)
	}
	// Family defaults must still appear so the planner has fallback values
	// to compose alongside the hint.
	if len(plans) < 2 {
		t.Errorf("expected hint + family defaults, got only %d plans", len(plans))
	}
}

// PlanRequirements wires PerTargetHints into PlanParams. A free-function
// target with a defaults entry has its hint-driven literal at the head of
// each parameter's ValuePlan slice.
func TestPlanRequirements_PerTargetHints_DefaultsApplied(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "Add",
		Params: []protocol.ParamInfo{
			{Name: "a", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
			{Name: "b", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
		SourceFile: "math.go",
	}
	lookup := func(string) *protocol.TargetContext { return &protocol.TargetContext{Analysis: analysis} }

	hints := planner.PerTargetHints{
		Defaults: map[string]planner.ParamValueHint{
			"a": {Literal: json.RawMessage(`100`), TypeHint: "int"},
		},
	}
	opts := planner.PlanRequirementsOptions{
		PerTargetHints: func(string) planner.PerTargetHints { return hints },
	}
	plans, unsat := planner.PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "math:Add"}},
		lookup,
		opts,
	)
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan")
	}
	// At least one plan should have a=100 (the hinted literal), since hint
	// candidates are emitted first per parameter and the composer's beam
	// search keeps the lowest-indexed enumerations.
	hitHint := false
	for _, plan := range plans {
		if len(plan.ArgumentPlans) >= 2 && string(plan.ArgumentPlans[0].Literal) == "100" {
			hitHint = true
			break
		}
	}
	if !hitHint {
		t.Errorf("no plan exercises the hinted a=100; plans = %+v", plans)
	}
}

// PlanRequirements with a generator hint on a parameter routes through the
// runtime-value registry instead of the primitive-family path.
func TestPlanRequirements_PerTargetHints_GeneratorApplied(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "Run",
		Params: []protocol.ParamInfo{
			{Name: "ctx", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := func(string) *protocol.TargetContext { return &protocol.TargetContext{Analysis: analysis} }
	hints := planner.PerTargetHints{
		Generators: map[string]string{"ctx": "context.Context"},
	}
	opts := planner.PlanRequirementsOptions{
		PerTargetHints: func(string) planner.PerTargetHints { return hints },
	}
	plans, unsat := planner.PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "pkg:Run"}},
		lookup,
		opts,
	)
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("expected 1 plan, got %d", len(plans))
	}
	arg := plans[0].ArgumentPlans[0]
	if arg.Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("arg.Kind = %q, want runtime_value", arg.Kind)
	}
	if arg.TypeHint != "context.Context" {
		t.Errorf("arg.TypeHint = %q, want context.Context", arg.TypeHint)
	}
}

func TestPlanRequirements_PerTargetHints_ConfiguredReceiverApplied(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name:       "(*Service).Run",
		SourceFile: "service.go",
	}
	target := &protocol.DiscoveredTarget{
		ID:   "pkg:(*Service).Run",
		Kind: protocol.TargetKindMethod,
		Receiver: &protocol.ReceiverShape{
			TypeName:  "Service",
			IsPointer: true,
		},
	}
	lookup := func(string) *protocol.TargetContext {
		return &protocol.TargetContext{
			Analysis:                     analysis,
			Target:                       target,
			ReceiverRequiresConstruction: true,
		}
	}
	hints := planner.PerTargetHints{
		Receiver: &config.ReceiverConfig{
			Label:      "seeded_service",
			Expression: "&Service{backend: fakeBackend{}}",
		},
	}
	opts := planner.PlanRequirementsOptions{
		PerTargetHints: func(string) planner.PerTargetHints { return hints },
	}
	plans, unsat := planner.PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "pkg:(*Service).Run"}},
		lookup,
		opts,
	)
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one configured receiver plan")
	}
	if plans[0].ReceiverKind != "configured:seeded_service" {
		t.Fatalf("plans[0].ReceiverKind = %q, want configured:seeded_service; plans=%+v", plans[0].ReceiverKind, plans)
	}
}

// AC2: the planner emits MockSpec entries for hint_config_v1 mocks.
// Ordering is deterministic (alphabetical by qualified function), which
// gives downstream codegen a stable wire.
func TestResolveMockSpecs_OrderingAndScope(t *testing.T) {
	t.Parallel()
	hints := planner.PerTargetHints{
		Mocks: map[string]string{
			"time.Now":    "func() time.Time { return time.Time{} }",
			"fmt.Println": "func(...any) (int, error) { return 0, nil }",
			"os.Getenv":   `func(string) string { return "" }`,
		},
	}
	specs := planner.ResolveMockSpecs("pkg:Func", hints)
	if len(specs) != 3 {
		t.Fatalf("len(specs) = %d, want 3", len(specs))
	}
	wantOrder := []string{"fmt.Println", "os.Getenv", "time.Now"}
	for i, want := range wantOrder {
		if specs[i].QualifiedFunction != want {
			t.Errorf("specs[%d].QualifiedFunction = %q, want %q", i, specs[i].QualifiedFunction, want)
		}
		if specs[i].TargetID != "pkg:Func" {
			t.Errorf("specs[%d].TargetID = %q, want pkg:Func", i, specs[i].TargetID)
		}
		if specs[i].Expression != hints.Mocks[want] {
			t.Errorf("specs[%d].Expression = %q, want %q", i, specs[i].Expression, hints.Mocks[want])
		}
	}
}

func TestResolveMockSpecs_EmptyMocks_ReturnsNil(t *testing.T) {
	t.Parallel()
	if got := planner.ResolveMockSpecs("pkg:Func", planner.PerTargetHints{}); got != nil {
		t.Errorf("expected nil for empty hints, got %+v", got)
	}
}

// Property: ResolveMockSpecs is a sorted, lossless flattening of the input
// map — no added or dropped entries, expressions preserved verbatim, output
// strictly increasing by QualifiedFunction.
func TestProperty_ResolveMockSpecs_SortedLossless(t *testing.T) {
	t.Parallel()
	rapid.Check(t, func(rt *rapid.T) {
		size := rapid.IntRange(0, 8).Draw(rt, "size")
		mocks := make(map[string]string, size)
		for i := 0; i < size; i++ {
			// Generate distinct keys by drawing an alphabetic name + index.
			key := rapid.StringMatching(`[a-z]{1,4}\.[A-Z][a-z]{0,4}`).Draw(rt, "qualified")
			expr := rapid.StringMatching(`[a-zA-Z]{1,8}`).Draw(rt, "expr")
			mocks[key] = expr
		}
		hints := planner.PerTargetHints{Mocks: mocks}
		specs := planner.ResolveMockSpecs("pkg:Func", hints)
		if len(specs) != len(mocks) {
			rt.Fatalf("len(specs) = %d, want %d", len(specs), len(mocks))
		}
		// Strictly sorted.
		for i := 1; i < len(specs); i++ {
			if !(specs[i-1].QualifiedFunction < specs[i].QualifiedFunction) {
				rt.Fatalf("not strictly sorted at %d: %q !< %q", i, specs[i-1].QualifiedFunction, specs[i].QualifiedFunction)
			}
		}
		// Lossless.
		for _, s := range specs {
			expr, ok := mocks[s.QualifiedFunction]
			if !ok {
				rt.Fatalf("spec %q not in input map", s.QualifiedFunction)
			}
			if s.Expression != expr {
				rt.Fatalf("spec %q expression = %q, want %q", s.QualifiedFunction, s.Expression, expr)
			}
			if s.TargetID != "pkg:Func" {
				rt.Fatalf("spec %q TargetID = %q, want pkg:Func", s.QualifiedFunction, s.TargetID)
			}
		}
	})
}

// Property: a generator hint on a parameter always produces ValuePlans that
// match the registry record for the named type — Kind=runtime_value, TypeHint
// equals the registered spelling, and Literal decodes to the registered Go
// expression. This pins AC3's contract across the registry's supported set.
func TestProperty_GeneratorPlans_MatchRegistry(t *testing.T) {
	t.Parallel()
	registered := planner.RegisteredRuntimeValueTypes()
	if len(registered) == 0 {
		t.Skip("no registered runtime-value types")
	}
	rapid.Check(t, func(rt *rapid.T) {
		idx := rapid.IntRange(0, len(registered)-1).Draw(rt, "typeIdx")
		typeName := registered[idx]
		// Use a primitive ParamInfo so we can verify the generator path
		// preempts family classification — the family would otherwise yield
		// a literal plan, not a runtime-value plan.
		p := protocol.ParamInfo{
			Name:     "x",
			Type:     protocol.TypeInfo{Kind: "int"},
			TypeName: strPtr("int"),
		}
		opts := planner.ParamPlanOptions{
			GeneratorsByName: map[string]string{"x": typeName},
		}
		plans, u := planner.PlanParam(testTargetID, 0, p, opts)
		if u != nil {
			rt.Fatalf("%s: unsatisfied %+v", typeName, u)
		}
		registry := planner.LookupRuntimeValue(typeName)
		// Sort registry expressions to make any future ordering shifts
		// visible as test failures rather than flakes.
		regExprs := make([]string, len(registry))
		for i, rv := range registry {
			regExprs[i] = rv.Expression
		}
		sort.Strings(regExprs)
		gotExprs := make([]string, len(plans))
		for i, plan := range plans {
			if plan.Kind != protocol.ValuePlanKindRuntimeValue {
				rt.Fatalf("%s plan[%d].Kind = %q, want runtime_value", typeName, i, plan.Kind)
			}
			if plan.TypeHint != typeName {
				rt.Fatalf("%s plan[%d].TypeHint = %q, want %q", typeName, i, plan.TypeHint, typeName)
			}
			var expr string
			if err := json.Unmarshal(plan.Literal, &expr); err != nil {
				rt.Fatalf("%s plan[%d] literal not JSON string: %v", typeName, i, err)
			}
			gotExprs[i] = expr
		}
		sort.Strings(gotExprs)
		if len(gotExprs) > len(regExprs) {
			rt.Fatalf("%s: produced %d plans, registry has %d", typeName, len(gotExprs), len(regExprs))
		}
		for i, expr := range gotExprs {
			if expr != regExprs[i] {
				rt.Fatalf("%s: expr[%d] = %q, want %q (registry: %v)", typeName, i, expr, regExprs[i], regExprs)
			}
		}
	})
}
