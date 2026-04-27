package instrument

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"testing"
)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// parseExprForTest parses a Go expression string for use in unit tests.
// Wraps the expression in a minimal function so the parser accepts it.
func parseExprForTest(t *testing.T, src string) (ast.Expr, *token.FileSet) {
	t.Helper()
	fset := token.NewFileSet()
	fullSrc := "package p\nfunc _f() bool { return " + src + " }"
	f, err := parser.ParseFile(fset, "test.go", fullSrc, 0)
	if err != nil {
		t.Fatalf("parse %q: %v", src, err)
	}
	fn := f.Decls[0].(*ast.FuncDecl)
	ret := fn.Body.List[0].(*ast.ReturnStmt)
	return ret.Results[0], fset
}

// ---------------------------------------------------------------------------
// flattenConditionsAST unit tests
// ---------------------------------------------------------------------------

func TestFlattenConditions_SimpleAnd(t *testing.T) {
	expr, fset := parseExprForTest(t, "a > 0 && b < 10")
	params := map[string]bool{"a": true, "b": true}

	result := flattenConditionsAST(expr, fset, params)
	if result == nil {
		t.Fatal("expected non-nil result for pure && chain")
	}
	if result.operator != mcdcAnd {
		t.Fatalf("operator=%q, want %q", result.operator, mcdcAnd)
	}
	if len(result.leaves) != 2 {
		t.Fatalf("got %d leaves, want 2", len(result.leaves))
	}
}

func TestFlattenConditions_SimpleOr(t *testing.T) {
	expr, fset := parseExprForTest(t, "x || y")
	params := map[string]bool{"x": true, "y": true}

	result := flattenConditionsAST(expr, fset, params)
	if result == nil {
		t.Fatal("expected non-nil result for pure || chain")
	}
	if result.operator != mcdcOr {
		t.Fatalf("operator=%q, want %q", result.operator, mcdcOr)
	}
	if len(result.leaves) != 2 {
		t.Fatalf("got %d leaves, want 2", len(result.leaves))
	}
}

func TestFlattenConditions_ThreeWayAnd(t *testing.T) {
	expr, fset := parseExprForTest(t, "a > 0 && b > 0 && c > 0")
	params := map[string]bool{"a": true, "b": true, "c": true}

	result := flattenConditionsAST(expr, fset, params)
	if result == nil {
		t.Fatal("expected non-nil result for 3-way && chain")
	}
	if len(result.leaves) != 3 {
		t.Fatalf("got %d leaves, want 3", len(result.leaves))
	}
}

func TestFlattenConditions_MixedOperatorsReturnsNil(t *testing.T) {
	// Mixed && and || should return nil (not supported in V1).
	// In Go, && has higher precedence than ||, so "a && b || c" parses as
	// "(a && b) || c". The top-level operator is || and the left subtree
	// has &&, so collectLeaves detects the operator mismatch and returns nil.
	expr, fset := parseExprForTest(t, "a || b && c")
	params := map[string]bool{"a": true, "b": true, "c": true}

	result := flattenConditionsAST(expr, fset, params)
	if result != nil {
		t.Fatalf("expected nil for mixed-operator expression, got %+v", result)
	}
}

func TestFlattenConditions_SingleConditionReturnsNil(t *testing.T) {
	// A single non-compound condition should return nil (no && or || at top level).
	expr, fset := parseExprForTest(t, "a > 0")
	params := map[string]bool{"a": true}

	result := flattenConditionsAST(expr, fset, params)
	if result != nil {
		t.Fatalf("expected nil for single condition, got %+v", result)
	}
}

func TestFlattenConditions_ParenthesizedChain(t *testing.T) {
	expr, fset := parseExprForTest(t, "(a > 0) && (b < 10)")
	params := map[string]bool{"a": true, "b": true}

	result := flattenConditionsAST(expr, fset, params)
	if result == nil {
		t.Fatal("expected non-nil result for parenthesized && chain")
	}
	if len(result.leaves) != 2 {
		t.Fatalf("got %d leaves, want 2", len(result.leaves))
	}
}

func TestFlattenConditions_CapAt16(t *testing.T) {
	// Build a chain of 17 conditions: a0 > 0 && a1 > 0 && ... && a16 > 0
	// This exceeds the cap and should return nil.
	src := "a0 > 0"
	params := map[string]bool{"a0": true}
	for i := 1; i <= 16; i++ {
		name := fmt.Sprintf("a%d", i)
		src += " && " + name + " > 0"
		params[name] = true
	}

	expr, fset := parseExprForTest(t, src)
	result := flattenConditionsAST(expr, fset, params)
	if result != nil {
		t.Fatalf("expected nil for chain exceeding cap (17 conditions), got %d leaves", len(result.leaves))
	}
}

func TestFlattenConditions_ExactlyAtCap(t *testing.T) {
	// Build a chain of exactly 16 conditions: a0 > 0 && ... && a15 > 0
	src := "a0 > 0"
	params := map[string]bool{"a0": true}
	for i := 1; i < 16; i++ {
		name := fmt.Sprintf("a%d", i)
		src += " && " + name + " > 0"
		params[name] = true
	}

	expr, fset := parseExprForTest(t, src)
	result := flattenConditionsAST(expr, fset, params)
	if result == nil {
		t.Fatal("expected non-nil result for chain of exactly 16 conditions")
	}
	if len(result.leaves) != 16 {
		t.Fatalf("got %d leaves, want 16", len(result.leaves))
	}
}
