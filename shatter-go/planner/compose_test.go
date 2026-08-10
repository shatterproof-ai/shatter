package planner_test

import (
	"encoding/json"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

const composeTargetID = "example.com/pkg:Target"

func valuePlan(paramIndex int, name string, kind protocol.ValuePlanKind, literal string, typeHint string) protocol.ValuePlan {
	return protocol.ValuePlan{
		ParamIndex: paramIndex,
		ParamName:  name,
		Kind:       kind,
		Literal:    json.RawMessage(literal),
		TypeHint:   typeHint,
	}
}

// AC: 2 receiver plans × 1 parameter with 3 value plans = 6 combinations, capped at 5.
func TestCompose_ReceiversTimesParams_CapsAtFive(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 0},
		{Kind: planner.ReceiverPlanKindUsefulZeroValue, ReceiverKind: "zero_value", Label: "zero_value_service", Priority: 1},
	}
	matrix := [][]protocol.ValuePlan{
		{
			valuePlan(0, "n", protocol.ValuePlanKindZero, "", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "1", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "-1", "int"),
		},
	}
	plans, unsat := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{})
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != planner.DefaultMaxComposedPlansPerTarget {
		t.Fatalf("len(plans)=%d, want %d", len(plans), planner.DefaultMaxComposedPlansPerTarget)
	}
	for i, p := range plans {
		if p.TargetID != composeTargetID {
			t.Errorf("plans[%d].TargetID=%q, want %q", i, p.TargetID, composeTargetID)
		}
		if p.Priority != i {
			t.Errorf("plans[%d].Priority=%d, want %d", i, p.Priority, i)
		}
		if len(p.ArgumentPlans) != 1 {
			t.Errorf("plans[%d].ArgumentPlans len=%d, want 1", i, len(p.ArgumentPlans))
		}
		if p.ReceiverKind == "" {
			t.Errorf("plans[%d].ReceiverKind must not be empty for method target", i)
		}
		if p.Label == "" {
			t.Errorf("plans[%d].Label must not be empty", i)
		}
	}
}

func TestCompose_RankingIsDeterministic(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 0},
		{Kind: planner.ReceiverPlanKindUsefulZeroValue, ReceiverKind: "zero_value", Label: "zero_value_service", Priority: 1},
	}
	matrix := [][]protocol.ValuePlan{
		{
			valuePlan(0, "n", protocol.ValuePlanKindZero, "", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "1", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "-1", "int"),
		},
	}
	first, _ := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{})
	second, _ := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{})
	if len(first) != len(second) {
		t.Fatalf("non-deterministic lengths: %d vs %d", len(first), len(second))
	}
	for i := range first {
		if first[i].Label != second[i].Label {
			t.Errorf("plans[%d] label differs: %q vs %q", i, first[i].Label, second[i].Label)
		}
		if first[i].ReceiverKind != second[i].ReceiverKind {
			t.Errorf("plans[%d] ReceiverKind differs: %q vs %q", i, first[i].ReceiverKind, second[i].ReceiverKind)
		}
		if len(first[i].ArgumentPlans) != len(second[i].ArgumentPlans) {
			t.Fatalf("plans[%d] arg count differs", i)
		}
		for j := range first[i].ArgumentPlans {
			if string(first[i].ArgumentPlans[j].Literal) != string(second[i].ArgumentPlans[j].Literal) {
				t.Errorf("plans[%d].ArgumentPlans[%d] Literal differs: %q vs %q",
					i, j, string(first[i].ArgumentPlans[j].Literal), string(second[i].ArgumentPlans[j].Literal))
			}
		}
	}
}

func TestCompose_FreeFunction_NoReceiver(t *testing.T) {
	matrix := [][]protocol.ValuePlan{
		{
			valuePlan(0, "s", protocol.ValuePlanKindZero, "", "string"),
			valuePlan(0, "s", protocol.ValuePlanKindLiteral, `"a"`, "string"),
		},
		{
			valuePlan(1, "n", protocol.ValuePlanKindZero, "", "int"),
			valuePlan(1, "n", protocol.ValuePlanKindLiteral, "1", "int"),
		},
	}
	plans, unsat := planner.Compose(composeTargetID, nil, matrix, nil, planner.ComposeOptions{})
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatal("expected at least one plan for free function")
	}
	for i, p := range plans {
		if p.ReceiverKind != "" {
			t.Errorf("plans[%d].ReceiverKind=%q, want empty for free function", i, p.ReceiverKind)
		}
		if len(p.ArgumentPlans) != 2 {
			t.Errorf("plans[%d].ArgumentPlans len=%d, want 2", i, len(p.ArgumentPlans))
		}
		if p.Priority != i {
			t.Errorf("plans[%d].Priority=%d, want %d", i, p.Priority, i)
		}
	}
}

func TestCompose_UnsatisfiedParam_PropagatesAndReturnsNoPlans(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 0},
	}
	matrix := [][]protocol.ValuePlan{
		{
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "1", "int"),
		},
		nil, // unsatisfied parameter at index 1
	}
	paramUnsat := []protocol.UnsatisfiedRequirement{
		{Kind: protocol.UnsatisfiedRequirementKindComplexType, TargetID: composeTargetID, Detail: `parameter "db" of type sql.DB is not a supported primitive`},
	}
	plans, unsat := planner.Compose(composeTargetID, recvs, matrix, paramUnsat, planner.ComposeOptions{})
	if len(plans) != 0 {
		t.Errorf("expected no plans, got %+v", plans)
	}
	if len(unsat) != 1 {
		t.Fatalf("len(unsat)=%d, want 1", len(unsat))
	}
	if unsat[0].Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("unsat[0].Kind=%q, want complex_type", unsat[0].Kind)
	}
}

func TestCompose_MethodTargetWithNoReceiverPlans_Unsatisfied(t *testing.T) {
	matrix := [][]protocol.ValuePlan{
		{valuePlan(0, "n", protocol.ValuePlanKindZero, "", "int")},
	}
	plans, unsat := planner.Compose(composeTargetID, []planner.ReceiverPlan{}, matrix, nil, planner.ComposeOptions{
		IsMethod: true,
	})
	if len(plans) != 0 {
		t.Errorf("expected no plans, got %+v", plans)
	}
	if len(unsat) != 1 {
		t.Fatalf("len(unsat)=%d, want 1", len(unsat))
	}
	if unsat[0].Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Errorf("unsat[0].Kind=%q, want no_constructor", unsat[0].Kind)
	}
}

func TestCompose_HintReceiverRankedBeforeNonHint(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindHint, ReceiverKind: "configured:seeded_service", Label: "seeded_service", Priority: 0},
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 1},
	}
	matrix := [][]protocol.ValuePlan{
		{valuePlan(0, "n", protocol.ValuePlanKindZero, "", "int")},
	}
	plans, _ := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{})
	if len(plans) < 2 {
		t.Fatalf("expected >=2 plans, got %d", len(plans))
	}
	if plans[0].ReceiverKind != "configured:seeded_service" {
		t.Errorf("plans[0]=%+v; expected top plan to use configured receiver", plans[0])
	}
	if plans[0].Label != "seeded_service" {
		t.Errorf("plans[0].Label=%q, want seeded_service", plans[0].Label)
	}
}

func TestCompose_MaxPlansOverrideDefault(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 0},
	}
	matrix := [][]protocol.ValuePlan{
		{
			valuePlan(0, "n", protocol.ValuePlanKindZero, "", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "1", "int"),
			valuePlan(0, "n", protocol.ValuePlanKindLiteral, "-1", "int"),
		},
	}
	plans, _ := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{MaxPlans: 2})
	if len(plans) != 2 {
		t.Fatalf("len(plans)=%d, want 2", len(plans))
	}
}

func TestCompose_NoParams(t *testing.T) {
	recvs := []planner.ReceiverPlan{
		{Kind: planner.ReceiverPlanKindSamePackageConstructor, ReceiverKind: "constructor:New", Label: "new_service", Priority: 0},
	}
	plans, unsat := planner.Compose(composeTargetID, recvs, nil, nil, planner.ComposeOptions{})
	if len(unsat) != 0 {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans)=%d, want 1", len(plans))
	}
	if len(plans[0].ArgumentPlans) != 0 {
		t.Errorf("plans[0].ArgumentPlans len=%d, want 0", len(plans[0].ArgumentPlans))
	}
	if plans[0].ReceiverKind != "constructor:New" {
		t.Errorf("plans[0].ReceiverKind=%q", plans[0].ReceiverKind)
	}
}

// Rapid property: cap respected, priorities strictly 0..n-1, every composed
// plan has one argument_plan per parameter.
func TestCompose_Invariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		recvCount := rapid.IntRange(1, 3).Draw(rt, "recvCount")
		paramCount := rapid.IntRange(0, 3).Draw(rt, "paramCount")
		maxPlans := rapid.IntRange(1, 8).Draw(rt, "maxPlans")

		recvs := make([]planner.ReceiverPlan, recvCount)
		for i := range recvCount {
			kinds := []planner.ReceiverPlanKind{
				planner.ReceiverPlanKindSamePackageConstructor,
				planner.ReceiverPlanKindUsefulZeroValue,
				planner.ReceiverPlanKindHint,
			}
			kind := rapid.SampledFrom(kinds).Draw(rt, "kind")
			recvs[i] = planner.ReceiverPlan{
				Kind:         kind,
				ReceiverKind: "recv_" + string(rune('a'+i)),
				Label:        "r_" + string(rune('a'+i)),
				Priority:     i,
			}
		}
		matrix := make([][]protocol.ValuePlan, paramCount)
		for pi := range paramCount {
			vcount := rapid.IntRange(1, 3).Draw(rt, "vcount")
			matrix[pi] = make([]protocol.ValuePlan, vcount)
			for vi := range vcount {
				matrix[pi][vi] = valuePlan(pi, "p"+string(rune('a'+pi)), protocol.ValuePlanKindLiteral, "0", "int")
			}
		}
		plans, _ := planner.Compose(composeTargetID, recvs, matrix, nil, planner.ComposeOptions{MaxPlans: maxPlans})
		if len(plans) > maxPlans {
			t.Fatalf("len(plans)=%d exceeds MaxPlans=%d", len(plans), maxPlans)
		}
		for i, p := range plans {
			if p.Priority != i {
				t.Fatalf("plans[%d].Priority=%d, want %d", i, p.Priority, i)
			}
			if p.TargetID != composeTargetID {
				t.Fatalf("plans[%d].TargetID=%q", i, p.TargetID)
			}
			if len(p.ArgumentPlans) != paramCount {
				t.Fatalf("plans[%d].ArgumentPlans len=%d, want %d", i, len(p.ArgumentPlans), paramCount)
			}
			if p.Label == "" {
				t.Fatalf("plans[%d].Label empty", i)
			}
		}
	})
}
