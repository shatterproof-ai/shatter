package instrument

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
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

// ---------------------------------------------------------------------------
// Integration tests: MC/DC recording via ExecuteFunction
// ---------------------------------------------------------------------------

// writeTestSource is a local helper for mcdc_test.go.
func writeMcdcTestSource(t *testing.T, dir, filename, content string) string {
	t.Helper()
	path := dir + "/" + filename
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("writing test source: %v", err)
	}
	return path
}

func TestExecuteFunctionMcdcAndChain(t *testing.T) {
	// Enable MC/DC mode for this test.
	t.Setenv("SHATTER_MCDC", "1")

	srcDir := t.TempDir()
	src := writeMcdcTestSource(t, srcDir, "target.go", `package main

func compoundAnd(a int, b int) string {
	if a > 0 && b < 10 {
		return "both"
	}
	return "neither"
}
`)
	// Execute with a=5, b=5 → branch taken (both conditions true)
	result, err := ExecuteFunction(src, "compoundAnd", []json.RawMessage{
		json.RawMessage("5"),
		json.RawMessage("5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	if len(result.BranchPath) == 0 {
		t.Fatal("expected branch decisions to be recorded")
	}

	// Find the branch for the if condition.
	found := false
	for _, b := range result.BranchPath {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			// Both conditions should be non-masked and true.
			for i, c := range b.Conditions {
				if c.Masked {
					t.Errorf("condition %d should not be masked (both true)", i)
				}
				if c.Value == nil || !*c.Value {
					t.Errorf("condition %d should have value=true", i)
				}
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions, got: %+v", result.BranchPath)
	}
}

func TestExecuteFunctionMcdcAndChainShortCircuit(t *testing.T) {
	// Enable MC/DC mode for this test.
	t.Setenv("SHATTER_MCDC", "1")

	srcDir := t.TempDir()
	src := writeMcdcTestSource(t, srcDir, "target.go", `package main

func compoundAnd(a int, b int) string {
	if a > 0 && b < 10 {
		return "both"
	}
	return "neither"
}
`)
	// Execute with a=-1, b=5 → first condition false, second masked
	result, err := ExecuteFunction(src, "compoundAnd", []json.RawMessage{
		json.RawMessage("-1"),
		json.RawMessage("5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	found := false
	for _, b := range result.BranchPath {
		if !b.Taken && len(b.Conditions) == 2 {
			found = true
			// First condition should be false (not masked).
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if b.Conditions[0].Value == nil || *b.Conditions[0].Value {
				t.Error("condition 0 should have value=false")
			}
			// Second condition should be masked.
			if !b.Conditions[1].Masked {
				t.Error("condition 1 should be masked")
			}
			if b.Conditions[1].Value != nil {
				t.Error("masked condition 1 should have nil value")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a not-taken branch with 2 MC/DC conditions; got: %+v", result.BranchPath)
	}
}

func TestExecuteFunctionMcdcOrChain(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	srcDir := t.TempDir()
	src := writeMcdcTestSource(t, srcDir, "target.go", `package main

func compoundOr(x bool, y bool) string {
	if x || y {
		return "either"
	}
	return "none"
}
`)
	// Execute with x=false, y=true → first false, second true (not masked), decision=true
	result, err := ExecuteFunction(src, "compoundOr", []json.RawMessage{
		json.RawMessage("false"),
		json.RawMessage("true"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	found := false
	for _, b := range result.BranchPath {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			// First: false, not masked.
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if b.Conditions[0].Value == nil || *b.Conditions[0].Value {
				t.Error("condition 0 (x=false) should have value=false")
			}
			// Second: true, not masked.
			if b.Conditions[1].Masked {
				t.Error("condition 1 should not be masked")
			}
			if b.Conditions[1].Value == nil || !*b.Conditions[1].Value {
				t.Error("condition 1 (y=true) should have value=true")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions; got: %+v", result.BranchPath)
	}
}

func TestExecuteFunctionMcdcOrChainShortCircuit(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	srcDir := t.TempDir()
	src := writeMcdcTestSource(t, srcDir, "target.go", `package main

func compoundOr(x bool, y bool) string {
	if x || y {
		return "either"
	}
	return "none"
}
`)
	// Execute with x=true, y=true → first true, second masked
	result, err := ExecuteFunction(src, "compoundOr", []json.RawMessage{
		json.RawMessage("true"),
		json.RawMessage("true"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	found := false
	for _, b := range result.BranchPath {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if !b.Conditions[1].Masked {
				t.Error("condition 1 should be masked (short-circuit)")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions; got: %+v", result.BranchPath)
	}
}

func TestExecuteFunctionMcdcDisabled(t *testing.T) {
	// MC/DC disabled: conditions slice should be empty.
	t.Setenv("SHATTER_MCDC", "0")

	srcDir := t.TempDir()
	src := writeMcdcTestSource(t, srcDir, "target.go", `package main

func compoundAnd(a int, b int) string {
	if a > 0 && b < 10 {
		return "both"
	}
	return "neither"
}
`)
	result, err := ExecuteFunction(src, "compoundAnd", []json.RawMessage{
		json.RawMessage("5"),
		json.RawMessage("5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	for _, b := range result.BranchPath {
		if len(b.Conditions) > 0 {
			t.Errorf("MC/DC disabled: expected no conditions on branch, got %d", len(b.Conditions))
		}
	}
}
