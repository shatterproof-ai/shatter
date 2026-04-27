package planner_test

import (
	"sort"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

// AC1: func[T any](t T) → 5 plans (string, int, bool, int64, float64).
func TestPlanGenericInstantiations_AnyConstraint_FiveDefaults(t *testing.T) {
	insts, u := planner.PlanGenericInstantiations(testTargetID, "T", "any")
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if got, want := len(insts), 5; got != want {
		t.Fatalf("len(insts) = %d, want %d; got=%+v", got, want, insts)
	}
	got := typeNamesOf(insts)
	want := []string{"bool", "float64", "int", "int64", "string"}
	if !equalStringSlices(got, want) {
		t.Errorf("type names = %v, want %v", got, want)
	}
	for _, inst := range insts {
		if inst.TypeParamName != "T" {
			t.Errorf("TypeParamName = %q, want %q", inst.TypeParamName, "T")
		}
	}
}

// "interface{}" and the empty constraint string are aliases for "any".
func TestPlanGenericInstantiations_InterfaceEmptyAliasAny(t *testing.T) {
	for _, c := range []string{"interface{}", "", "comparable"} {
		insts, u := planner.PlanGenericInstantiations(testTargetID, "T", c)
		if u != nil {
			t.Errorf("constraint %q: unexpected unsatisfied: %+v", c, u)
			continue
		}
		if len(insts) != 5 {
			t.Errorf("constraint %q: len(insts) = %d, want 5", c, len(insts))
		}
	}
}

// AC2: func[T comparable](t T) → 5 plans (all defaults are comparable).
func TestPlanGenericInstantiations_ComparableConstraint_FiveDefaults(t *testing.T) {
	insts, u := planner.PlanGenericInstantiations(testTargetID, "T", "comparable")
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	if got, want := len(insts), 5; got != want {
		t.Fatalf("len(insts) = %d, want %d", got, want)
	}
	for _, inst := range insts {
		if !isComparableDefault(inst.TypeName) {
			t.Errorf("type %q is not in the comparable default set", inst.TypeName)
		}
	}
}

// AC3: cmp.Ordered restricts to ordered types — bool is excluded.
func TestPlanGenericInstantiations_CmpOrdered_ExcludesBool(t *testing.T) {
	insts, u := planner.PlanGenericInstantiations(testTargetID, "T", "cmp.Ordered")
	if u != nil {
		t.Fatalf("unexpected unsatisfied: %+v", u)
	}
	got := typeNamesOf(insts)
	want := []string{"float64", "int", "int64", "string"}
	if !equalStringSlices(got, want) {
		t.Errorf("cmp.Ordered instantiations = %v, want %v", got, want)
	}
	for _, inst := range insts {
		if inst.TypeName == "bool" {
			t.Errorf("cmp.Ordered must not include bool; got %+v", inst)
		}
	}
}

// AC4: an unrecognized / unsatisfiable constraint produces an
// UnsatisfiedRequirement with a documented reason.
func TestPlanGenericInstantiations_UnsatisfiableConstraint(t *testing.T) {
	cases := []string{
		"io.Reader",                 // interface constraint we cannot synthesize
		"~int | ~string | ~float64", // type-set we don't yet decompose
		"NotARealConstraint",        // unknown identifier
	}
	for _, c := range cases {
		insts, u := planner.PlanGenericInstantiations(testTargetID, "T", c)
		if u == nil {
			t.Errorf("constraint %q: want unsatisfied, got insts=%+v", c, insts)
			continue
		}
		if u.Kind != protocol.UnsatisfiedRequirementKindGenericUnconstrained {
			t.Errorf("constraint %q: kind = %q, want %q", c, u.Kind, protocol.UnsatisfiedRequirementKindGenericUnconstrained)
		}
		if u.TargetID != testTargetID {
			t.Errorf("constraint %q: TargetID = %q, want %q", c, u.TargetID, testTargetID)
		}
		if u.Detail == "" || !strings.Contains(u.Detail, c) {
			t.Errorf("constraint %q: Detail = %q, expected to mention the constraint text", c, u.Detail)
		}
	}
}

// Whitespace and stdlib qualification variants for cmp.Ordered are recognized.
func TestPlanGenericInstantiations_CmpOrderedAliases(t *testing.T) {
	for _, c := range []string{"cmp.Ordered", "  cmp.Ordered  ", "constraints.Ordered"} {
		insts, u := planner.PlanGenericInstantiations(testTargetID, "T", c)
		if u != nil {
			t.Fatalf("constraint %q: unexpected unsatisfied: %+v", c, u)
		}
		if len(insts) != 4 {
			t.Errorf("constraint %q: len(insts) = %d, want 4", c, len(insts))
		}
	}
}

// Property: the returned set is a stable subset of the five defaults, with
// no duplicates and with a non-empty TypeName on every entry.
func TestPlanGenericInstantiations_Properties(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		constraint := rapid.SampledFrom([]string{
			"any", "interface{}", "", "comparable",
			"cmp.Ordered", "constraints.Ordered",
		}).Draw(rt, "constraint")
		paramName := rapid.SampledFrom([]string{"T", "U", "K", "V"}).Draw(rt, "paramName")
		insts, u := planner.PlanGenericInstantiations(testTargetID, paramName, constraint)
		if u != nil {
			rt.Fatalf("constraint %q: unexpected unsatisfied: %+v", constraint, u)
		}
		seen := map[string]struct{}{}
		for _, inst := range insts {
			if inst.TypeName == "" {
				rt.Errorf("empty TypeName: %+v", inst)
			}
			if !isAnyDefault(inst.TypeName) {
				rt.Errorf("type %q is not in the default set", inst.TypeName)
			}
			if inst.TypeParamName != paramName {
				rt.Errorf("TypeParamName = %q, want %q", inst.TypeParamName, paramName)
			}
			if _, dup := seen[inst.TypeName]; dup {
				rt.Errorf("duplicate type %q in instantiation set", inst.TypeName)
			}
			seen[inst.TypeName] = struct{}{}
		}
		if len(insts) == 0 {
			rt.Errorf("constraint %q: empty instantiation set", constraint)
		}
	})
}

// Property: cmp.Ordered's instantiation set is always a strict subset of
// the comparable / any sets.
func TestPlanGenericInstantiations_OrderedSubsetOfComparable(t *testing.T) {
	ordered, _ := planner.PlanGenericInstantiations(testTargetID, "T", "cmp.Ordered")
	comparable, _ := planner.PlanGenericInstantiations(testTargetID, "T", "comparable")
	cmpSet := typeNameSet(comparable)
	for _, inst := range ordered {
		if _, ok := cmpSet[inst.TypeName]; !ok {
			t.Errorf("ordered type %q not in comparable set", inst.TypeName)
		}
	}
	if len(ordered) >= len(comparable) {
		t.Errorf("expected ordered to be a strict subset; len(ordered)=%d len(comparable)=%d", len(ordered), len(comparable))
	}
}

// ----- helpers -----

func typeNamesOf(insts []planner.GenericInstantiation) []string {
	names := make([]string, 0, len(insts))
	for _, inst := range insts {
		names = append(names, inst.TypeName)
	}
	sort.Strings(names)
	return names
}

func typeNameSet(insts []planner.GenericInstantiation) map[string]struct{} {
	out := map[string]struct{}{}
	for _, inst := range insts {
		out[inst.TypeName] = struct{}{}
	}
	return out
}

func equalStringSlices(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func isAnyDefault(name string) bool {
	switch name {
	case "string", "int", "int64", "float64", "bool":
		return true
	}
	return false
}

func isComparableDefault(name string) bool {
	// All five defaults are comparable in Go.
	return isAnyDefault(name)
}
