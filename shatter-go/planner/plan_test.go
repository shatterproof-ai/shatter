package planner

import (
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

func strPtr(s string) *string { return &s }

func TestPlanRequirements_FreeFunctionProducesPlans(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "Add",
		Params: []protocol.ParamInfo{
			{Name: "a", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
			{Name: "b", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := func(id string) *protocol.FunctionAnalysis {
		if id == "example.com/pkg:Add" {
			return analysis
		}
		return nil
	}

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

func TestPlanRequirements_TargetNotAnalyzed(t *testing.T) {
	t.Parallel()
	lookup := func(string) *protocol.FunctionAnalysis { return nil }

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

func TestPlanRequirements_MethodReturnsNoConstructor(t *testing.T) {
	t.Parallel()
	analysis := &protocol.FunctionAnalysis{
		Name: "(*Service).Run",
		Params: []protocol.ParamInfo{
			{Name: "x", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := func(string) *protocol.FunctionAnalysis { return analysis }

	plans, unsat := PlanRequirements(
		[]protocol.InvocationRequirement{{TargetID: "example.com/pkg:(*Service).Run"}},
		lookup,
		PlanRequirementsOptions{},
	)
	if len(plans) != 0 {
		t.Fatalf("method target should yield no plans, got %d", len(plans))
	}
	if len(unsat) != 1 || unsat[0].Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Fatalf("expected NoConstructor unsatisfied, got %+v", unsat)
	}
}

func TestPlanRequirements_AggregatesAcrossRequirements(t *testing.T) {
	t.Parallel()
	add := &protocol.FunctionAnalysis{
		Name: "Add",
		Params: []protocol.ParamInfo{
			{Name: "a", Type: protocol.TypeInfo{Kind: "int"}, TypeName: strPtr("int")},
		},
	}
	lookup := func(id string) *protocol.FunctionAnalysis {
		if id == "example.com/pkg:Add" {
			return add
		}
		return nil
	}

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
