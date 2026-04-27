package planner_test

import (
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

const compositeTargetID = "example.com/pkg:Func"

func objectType(fields ...protocol.ObjectField) protocol.TypeInfo {
	return protocol.TypeInfo{Kind: "object", Fields: fields}
}

func field(name string, t protocol.TypeInfo) protocol.ObjectField {
	return protocol.ObjectField{Name: name, Type: t}
}

// AC1: type Req struct{Name string; N int} — working composite literal.
func TestPlanComposite_StringAndInt_EmitsLiteral(t *testing.T) {
	req := objectType(
		field("Name", protocol.TypeInfo{Kind: "str"}),
		field("N", protocol.TypeInfo{Kind: "int"}),
	)
	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.Req", "example.com/pkg", req, planner.CompositeOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if plan == nil {
		t.Fatal("expected plan, got nil")
	}
	wantExpr := `pkg.Req{Name: "", N: 0}`
	if plan.Expression != wantExpr {
		t.Errorf("Expression = %q, want %q", plan.Expression, wantExpr)
	}
	if plan.TypeHint != "pkg.Req" {
		t.Errorf("TypeHint = %q, want %q", plan.TypeHint, "pkg.Req")
	}
	if len(plan.Imports) != 1 || plan.Imports[0] != "example.com/pkg" {
		t.Errorf("Imports = %v, want [example.com/pkg]", plan.Imports)
	}
}

// AC2: type Req struct{DB *sql.DB} — unsatisfied with a detail mentioning the field.
// The analyzer flattens *sql.DB to {Kind:"opaque", Label:"sql.DB"}, so that's
// what the planner sees at the field site.
func TestPlanComposite_OpaquePointerField_Unsatisfied(t *testing.T) {
	req := objectType(
		field("DB", protocol.TypeInfo{Kind: "opaque", Label: "sql.DB"}),
	)
	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.Req", "example.com/pkg", req, planner.CompositeOptions{})
	if plan != nil {
		t.Errorf("expected nil plan, got %+v", plan)
	}
	if unsat == nil {
		t.Fatal("expected unsatisfied, got nil")
	}
	if unsat.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("Kind = %q, want %q", unsat.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if unsat.TargetID != compositeTargetID {
		t.Errorf("TargetID = %q, want %q", unsat.TargetID, compositeTargetID)
	}
	if !strings.Contains(unsat.Detail, "DB") || !strings.Contains(unsat.Detail, "sql.DB") {
		t.Errorf("Detail = %q, want it to mention both field %q and type %q", unsat.Detail, "DB", "sql.DB")
	}
}

// AC2 variant: nullable-wrapped opaque is also unsatisfied.
func TestPlanComposite_NullableOpaquePointerField_Unsatisfied(t *testing.T) {
	opaqueDB := protocol.TypeInfo{Kind: "opaque", Label: "sql.DB"}
	req := objectType(
		field("DB", protocol.TypeInfo{Kind: "nullable", Inner: &opaqueDB}),
	)
	_, unsat := planner.PlanComposite(compositeTargetID, "pkg.Req", "example.com/pkg", req, planner.CompositeOptions{})
	if unsat == nil {
		t.Fatal("expected unsatisfied, got nil")
	}
	if unsat.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("Kind = %q, want %q", unsat.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}
	if !strings.Contains(unsat.Detail, "sql.DB") {
		t.Errorf("Detail = %q, want it to mention %q", unsat.Detail, "sql.DB")
	}
}

// AC3: type Node struct{Next *Node} — terminates with Next: nil regardless
// of depth bound.
func TestPlanComposite_RecursivePointerField_TerminatesWithNil(t *testing.T) {
	// Node is self-referential. TypeInfo JSON cannot represent cycles, so
	// we model the pointee as an empty object {}; the synthesizer must not
	// descend into it, and must emit `nil`.
	innerNode := objectType() // placeholder pointee for *Node
	node := objectType(
		field("Next", protocol.TypeInfo{Kind: "nullable", Inner: &innerNode}),
	)
	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.Node", "example.com/pkg", node, planner.CompositeOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	wantExpr := `pkg.Node{Next: nil}`
	if plan.Expression != wantExpr {
		t.Errorf("Expression = %q, want %q", plan.Expression, wantExpr)
	}
}

// Depth bound: a non-pointer nested struct that exceeds the remaining depth
// produces an unsatisfied requirement; one more depth unit lets it through
// using Go's elided-type composite literal.
func TestPlanComposite_DepthBound_NestedStruct(t *testing.T) {
	leaf := objectType(field("X", protocol.TypeInfo{Kind: "int"}))
	outer := objectType(field("Inner", leaf))

	_, unsat := planner.PlanComposite(compositeTargetID, "pkg.Outer", "example.com/pkg", outer, planner.CompositeOptions{MaxDepth: 1})
	if unsat == nil {
		t.Fatal("expected unsatisfied at MaxDepth=1, got nil")
	}
	if unsat.Kind != protocol.UnsatisfiedRequirementKindComplexType {
		t.Errorf("Kind = %q, want %q", unsat.Kind, protocol.UnsatisfiedRequirementKindComplexType)
	}

	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.Outer", "example.com/pkg", outer, planner.CompositeOptions{MaxDepth: 2})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied at MaxDepth=2: %+v", unsat)
	}
	wantExpr := `pkg.Outer{Inner: {X: 0}}`
	if plan.Expression != wantExpr {
		t.Errorf("Expression = %q, want %q", plan.Expression, wantExpr)
	}
}

// A struct with no fields synthesizes as `pkg.Req{}`.
func TestPlanComposite_EmptyStruct(t *testing.T) {
	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.Empty", "example.com/pkg", objectType(), planner.CompositeOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if plan.Expression != "pkg.Empty{}" {
		t.Errorf("Expression = %q, want %q", plan.Expression, "pkg.Empty{}")
	}
}

// All primitive families are covered by the value catalog.
func TestPlanComposite_AllPrimitiveFamilies(t *testing.T) {
	byteSlice := protocol.TypeInfo{Kind: "array", Element: &protocol.TypeInfo{Kind: "int"}}
	req := objectType(
		field("S", protocol.TypeInfo{Kind: "str"}),
		field("I", protocol.TypeInfo{Kind: "int"}),
		field("F", protocol.TypeInfo{Kind: "float"}),
		field("B", protocol.TypeInfo{Kind: "bool"}),
		field("Bs", byteSlice),
	)
	plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.All", "example.com/pkg", req, planner.CompositeOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	wantExpr := `pkg.All{S: "", I: 0, F: 0, B: false, Bs: nil}`
	if plan.Expression != wantExpr {
		t.Errorf("Expression = %q, want %q", plan.Expression, wantExpr)
	}
}

// Empty pkgImport means Imports is empty (useful for package-local synthesis).
func TestPlanComposite_NoPkgImport_NoImports(t *testing.T) {
	plan, unsat := planner.PlanComposite(compositeTargetID, "Req", "", objectType(field("N", protocol.TypeInfo{Kind: "int"})), planner.CompositeOptions{})
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plan.Imports) != 0 {
		t.Errorf("Imports = %v, want empty", plan.Imports)
	}
}

// Top-level input that isn't an object kind is rejected.
func TestPlanComposite_NonObjectTopLevel_Unsatisfied(t *testing.T) {
	_, unsat := planner.PlanComposite(compositeTargetID, "pkg.NotStruct", "example.com/pkg", protocol.TypeInfo{Kind: "int"}, planner.CompositeOptions{})
	if unsat == nil {
		t.Fatal("expected unsatisfied for non-object input")
	}
}

// Property: PlanComposite terminates (returns) for any depth-bounded input
// TypeInfo tree within a small number of recursive calls, and never panics.
// The combinator generates objects nested to an arbitrary depth with a mix
// of primitive, pointer, and nested-object fields.
func TestPlanComposite_Termination_Invariant(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		inputDepth := rapid.IntRange(0, 8).Draw(rt, "inputDepth")
		maxDepth := rapid.IntRange(1, 5).Draw(rt, "maxDepth")
		top := genTypeInfo(rt, inputDepth, "top")
		if top.Kind != "object" {
			// Ensure the top-level is always a struct so the call exercises
			// the struct-literal path.
			top = objectType(field("Inner", top))
		}
		plan, unsat := planner.PlanComposite(compositeTargetID, "pkg.T", "example.com/pkg", top, planner.CompositeOptions{MaxDepth: maxDepth})
		if plan == nil && unsat == nil {
			rt.Fatalf("PlanComposite returned (nil, nil)")
		}
		if plan != nil && unsat != nil {
			rt.Fatalf("PlanComposite returned both plan and unsatisfied")
		}
		if plan != nil {
			if plan.TypeHint != "pkg.T" {
				rt.Fatalf("TypeHint = %q, want %q", plan.TypeHint, "pkg.T")
			}
			if !strings.HasPrefix(plan.Expression, "pkg.T{") || !strings.HasSuffix(plan.Expression, "}") {
				rt.Fatalf("Expression = %q, want pkg.T{...}", plan.Expression)
			}
		}
		if unsat != nil {
			if unsat.Kind != protocol.UnsatisfiedRequirementKindComplexType {
				rt.Fatalf("unsat.Kind = %q, want %q", unsat.Kind, protocol.UnsatisfiedRequirementKindComplexType)
			}
			if unsat.TargetID != compositeTargetID {
				rt.Fatalf("unsat.TargetID = %q, want %q", unsat.TargetID, compositeTargetID)
			}
		}
	})
}

// genTypeInfo produces an arbitrary TypeInfo tree up to the given depth. A
// deterministic field-name suffix makes generated struct tags reproducible
// under rapid's shrinker.
func genTypeInfo(rt *rapid.T, depth int, tag string) protocol.TypeInfo {
	if depth <= 0 {
		kind := rapid.SampledFrom([]string{"str", "int", "float", "bool", "opaque"}).Draw(rt, tag+":leaf-kind")
		if kind == "opaque" {
			return protocol.TypeInfo{Kind: "opaque", Label: "pkg.Opaque"}
		}
		return protocol.TypeInfo{Kind: kind}
	}
	switch rapid.SampledFrom([]string{"str", "int", "float", "bool", "opaque", "nullable", "object"}).Draw(rt, tag+":kind") {
	case "str":
		return protocol.TypeInfo{Kind: "str"}
	case "int":
		return protocol.TypeInfo{Kind: "int"}
	case "float":
		return protocol.TypeInfo{Kind: "float"}
	case "bool":
		return protocol.TypeInfo{Kind: "bool"}
	case "opaque":
		return protocol.TypeInfo{Kind: "opaque", Label: "pkg.Opaque"}
	case "nullable":
		inner := genTypeInfo(rt, depth-1, tag+":inner")
		return protocol.TypeInfo{Kind: "nullable", Inner: &inner}
	case "object":
		nFields := rapid.IntRange(0, 3).Draw(rt, tag+":nFields")
		fields := make([]protocol.ObjectField, nFields)
		for i := range nFields {
			fields[i] = protocol.ObjectField{
				Name: "F" + string(rune('A'+i)),
				Type: genTypeInfo(rt, depth-1, tag+":f"+string(rune('A'+i))),
			}
		}
		return protocol.TypeInfo{Kind: "object", Fields: fields}
	}
	return protocol.TypeInfo{Kind: "int"}
}
