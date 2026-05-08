package instrument

import (
	"go/ast"
	"go/parser"
	"go/token"
	"testing"
)

func TestParamComparison(t *testing.T) {
	expr, _ := parser.ParseExpr("x > 10")
	params := map[string]bool{"x": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" {
		t.Fatalf("kind = %q, want bin_op", sym.Kind)
	}
	if sym.Op != "gt" {
		t.Errorf("op = %q, want gt", sym.Op)
	}
	if sym.Left.Kind != "param" || sym.Left.Name != "x" {
		t.Errorf("left = %+v, want param x", sym.Left)
	}
	if sym.Right.Kind != "const" || sym.Right.Type != "int" {
		t.Errorf("right = %+v, want const int", sym.Right)
	}
}

func TestStringEquality(t *testing.T) {
	expr, _ := parser.ParseExpr(`s == "hello"`)
	params := map[string]bool{"s": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" || sym.Op != "eq" {
		t.Errorf("got kind=%q op=%q, want bin_op eq", sym.Kind, sym.Op)
	}
	if sym.Left.Kind != "param" || sym.Left.Name != "s" {
		t.Errorf("left = %+v, want param s", sym.Left)
	}
	if sym.Right.Kind != "const" || sym.Right.Type != "str" {
		t.Errorf("right = %+v, want const str", sym.Right)
	}
}

func TestLogicalAnd(t *testing.T) {
	expr, _ := parser.ParseExpr("x > 0 && y < 10")
	params := map[string]bool{"x": true, "y": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" || sym.Op != "and" {
		t.Fatalf("got kind=%q op=%q, want bin_op and", sym.Kind, sym.Op)
	}
	if sym.Left.Op != "gt" {
		t.Errorf("left.op = %q, want gt", sym.Left.Op)
	}
	if sym.Right.Op != "lt" {
		t.Errorf("right.op = %q, want lt", sym.Right.Op)
	}
}

func TestLogicalOr(t *testing.T) {
	expr, _ := parser.ParseExpr("a == 1 || b == 2")
	params := map[string]bool{"a": true, "b": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" || sym.Op != "or" {
		t.Errorf("got kind=%q op=%q, want bin_op or", sym.Kind, sym.Op)
	}
}

func TestNegation(t *testing.T) {
	expr, _ := parser.ParseExpr("!ok")
	params := map[string]bool{"ok": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "un_op" || sym.Op != "not" {
		t.Fatalf("got kind=%q op=%q, want un_op not", sym.Kind, sym.Op)
	}
	if sym.Operand.Kind != "param" || sym.Operand.Name != "ok" {
		t.Errorf("operand = %+v, want param ok", sym.Operand)
	}
}

func TestNilComparison(t *testing.T) {
	expr, _ := parser.ParseExpr("p == nil")
	params := map[string]bool{"p": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" || sym.Op != "eq" {
		t.Fatalf("got kind=%q op=%q, want bin_op eq", sym.Kind, sym.Op)
	}
	if sym.Right.Kind != "const" || sym.Right.Type != "null" {
		t.Errorf("right = %+v, want const null", sym.Right)
	}
}

func TestLenCall(t *testing.T) {
	expr, _ := parser.ParseExpr("len(items)")
	params := map[string]bool{"items": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "call" || sym.Name != "len" {
		t.Fatalf("got kind=%q name=%q, want call len", sym.Kind, sym.Name)
	}
	if len(sym.Args) != 1 || sym.Args[0].Kind != "param" {
		t.Errorf("args = %+v, want [param items]", sym.Args)
	}
}

func TestSelectorExpr(t *testing.T) {
	expr, _ := parser.ParseExpr("user.Age")
	params := map[string]bool{"user": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "param" || sym.Name != "user" {
		t.Fatalf("got kind=%q name=%q, want param user", sym.Kind, sym.Name)
	}
	if len(sym.Path) != 1 || sym.Path[0] != "Age" {
		t.Errorf("path = %v, want [Age]", sym.Path)
	}
}

func TestUnknownFallback(t *testing.T) {
	// A composite literal is not representable
	expr, _ := parser.ParseExpr("[]int{1, 2, 3}")
	params := map[string]bool{}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "unknown" {
		t.Errorf("kind = %q, want unknown", sym.Kind)
	}
}

func TestParenExprUnwraps(t *testing.T) {
	expr, _ := parser.ParseExpr("(x > 5)")
	params := map[string]bool{"x": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "bin_op" || sym.Op != "gt" {
		t.Errorf("got kind=%q op=%q, want bin_op gt", sym.Kind, sym.Op)
	}
}

func TestExtractConstraintExpr(t *testing.T) {
	fset := token.NewFileSet()
	expr, _ := parser.ParseExpr("x > 10")
	params := map[string]bool{"x": true}
	c := extractConstraint(fset, expr, params)

	if c.Kind != "expr" {
		t.Fatalf("kind = %q, want expr", c.Kind)
	}
	if c.Expr == nil || c.Expr.Kind != "bin_op" {
		t.Errorf("expr = %+v, want bin_op", c.Expr)
	}
}

func TestExtractConstraintUnknown(t *testing.T) {
	fset := token.NewFileSet()
	expr, _ := parser.ParseExpr("[]int{1}")
	params := map[string]bool{}
	c := extractConstraint(fset, expr, params)

	if c.Kind != "unknown" {
		t.Fatalf("kind = %q, want unknown", c.Kind)
	}
	if c.Hint == "" {
		t.Error("hint should not be empty for unknown constraint")
	}
}

func TestTokenToOpBitwise(t *testing.T) {
	tests := []struct {
		expr string
		op   string
	}{
		{"x & y", "bitwise_and"},
		{"x | y", "bitwise_or"},
		{"x ^ y", "bitwise_xor"},
		{"x << y", "shl"},
		{"x >> y", "shr"},
		{"x &^ y", "bit_clear"},
	}
	params := map[string]bool{"x": true, "y": true}
	for _, tc := range tests {
		expr, err := parser.ParseExpr(tc.expr)
		if err != nil {
			t.Fatalf("parse %q: %v", tc.expr, err)
		}
		sym := exprToSymExpr(expr, params)
		if sym.Kind != "bin_op" {
			t.Errorf("%s: kind = %q, want bin_op", tc.expr, sym.Kind)
			continue
		}
		if sym.Op != tc.op {
			t.Errorf("%s: op = %q, want %q", tc.expr, sym.Op, tc.op)
		}
	}
}

func TestUnaryMinusAndBitwiseNot(t *testing.T) {
	params := map[string]bool{"x": true}

	// Unary minus
	expr, _ := parser.ParseExpr("-x")
	sym := exprToSymExpr(expr, params)
	if sym.Kind != "un_op" || sym.Op != "neg" {
		t.Errorf("-x: got kind=%q op=%q, want un_op neg", sym.Kind, sym.Op)
	}

	// Bitwise NOT (^x in Go)
	expr, _ = parser.ParseExpr("^x")
	sym = exprToSymExpr(expr, params)
	if sym.Kind != "un_op" || sym.Op != "bitwise_not" {
		t.Errorf("^x: got kind=%q op=%q, want un_op bitwise_not", sym.Kind, sym.Op)
	}
}

func TestBoolLiterals(t *testing.T) {
	for _, tc := range []struct {
		input string
		value bool
	}{
		{"true", true},
		{"false", false},
	} {
		expr, _ := parser.ParseExpr(tc.input)
		sym := exprToSymExpr(expr, map[string]bool{})
		if sym.Kind != "const" || sym.Type != "bool" {
			t.Errorf("%s: got kind=%q type=%q, want const bool", tc.input, sym.Kind, sym.Type)
		}
		if sym.Value != tc.value {
			t.Errorf("%s: got value=%v, want %v", tc.input, sym.Value, tc.value)
		}
	}
}

// ── exprToSymExprWithFlow ─────────────────────────────────────────────────────

func TestExprToSymExprWithFlow_ResolvesFlowMapEntry(t *testing.T) {
	// A variable tracked in the flow map resolves to its symbolic value.
	fm := flowMap{"label": constInt(42)}
	expr, _ := parser.ParseExpr("label")
	sym := exprToSymExprWithFlow(expr, map[string]bool{}, fm)
	if sym.Kind != "const" || sym.Value.(int64) != 42 {
		t.Errorf("got %+v, want const int 42", sym)
	}
}

func TestExprToSymExprWithFlow_ParamFallback(t *testing.T) {
	// A name not in the flow map falls back to param resolution.
	fm := flowMap{"other": constInt(1)}
	expr, _ := parser.ParseExpr("x > 0")
	params := map[string]bool{"x": true}
	sym := exprToSymExprWithFlow(expr, params, fm)
	if sym.Kind != "bin_op" || sym.Op != "gt" {
		t.Fatalf("got kind=%q op=%q, want bin_op gt", sym.Kind, sym.Op)
	}
	if sym.Left.Kind != "param" || sym.Left.Name != "x" {
		t.Errorf("left = %+v, want param x", sym.Left)
	}
}

func TestExprToSymExprWithFlow_NilFmMatchesExprToSymExpr(t *testing.T) {
	expr, _ := parser.ParseExpr("x > 10")
	params := map[string]bool{"x": true}
	without := exprToSymExpr(expr, params)
	withNil := exprToSymExprWithFlow(expr, params, nil)
	if without.Kind != withNil.Kind || without.Op != withNil.Op {
		t.Errorf("nil fm: got %+v, want %+v", withNil, without)
	}
}

func TestExprToSymExprWithFlow_SubexprResolution(t *testing.T) {
	// label is tracked as const(5); the expression "label + 1" should resolve
	// the label operand through the flow map → bin_op(add, const(5), const(1)).
	fm := flowMap{"label": constInt(5)}
	expr, _ := parser.ParseExpr("label + 1")
	sym := exprToSymExprWithFlow(expr, map[string]bool{}, fm)
	if sym.Kind != "bin_op" || sym.Op != "add" {
		t.Fatalf("kind=%q op=%q, want bin_op add", sym.Kind, sym.Op)
	}
	if sym.Left.Kind != "const" || sym.Left.Value.(int64) != 5 {
		t.Errorf("left = %+v, want const int 5 from flow map", sym.Left)
	}
}

// ── walkStmtsForFlow ──────────────────────────────────────────────────────────

func TestWalkStmtsForFlow_SimpleAssign(t *testing.T) {
	// label := x + 1 seeds the flow map with a bin_op.
	src := `package p
func f(x int) {
	label := x + 1
	_ = label
}`
	fm := parseAndWalkFirstFunc(t, src)
	got, ok := fm["label"]
	if !ok {
		t.Fatal("label not in flow map")
	}
	if got.Kind != "bin_op" || got.Op != "add" {
		t.Errorf("kind=%q op=%q, want bin_op add", got.Kind, got.Op)
	}
	if got.Left.Kind != "param" || got.Left.Name != "x" {
		t.Errorf("left = %+v, want param x", got.Left)
	}
}

func TestWalkStmtsForFlow_ConditionalReassignment(t *testing.T) {
	// var label int
	// if x > 0 { label = 1 } else { label = 2 }
	// → label becomes ite(x>0, const(1), const(2)).
	// Note: using 2 instead of -1 to keep both branches as BasicLit (positive integers).
	src := `package p
func f(x int) int {
	var label int
	if x > 0 {
		label = 1
	} else {
		label = 2
	}
	return label
}`
	fm := parseAndWalkFirstFunc(t, src)
	got, ok := fm["label"]
	if !ok {
		t.Fatal("label not in flow map")
	}
	if got.Kind != "ite" {
		t.Fatalf("kind = %q, want ite", got.Kind)
	}
	if got.Condition == nil || got.Condition.Kind != "bin_op" || got.Condition.Op != "gt" {
		t.Errorf("condition = %+v, want bin_op gt", got.Condition)
	}
	if got.ThenExpr == nil || got.ThenExpr.Kind != "const" {
		t.Errorf("then_expr = %+v, want const int 1", got.ThenExpr)
	}
	if got.ElseExpr == nil || got.ElseExpr.Kind != "const" {
		t.Errorf("else_expr = %+v, want const int 2", got.ElseExpr)
	}
}

func TestWalkStmtsForFlow_IfOnly_NoElse(t *testing.T) {
	// label := 0
	// if x > 0 { label = 1 }
	// → ite(x>0, const(1), const(0)).
	src := `package p
func f(x int) int {
	label := 0
	if x > 0 {
		label = 1
	}
	return label
}`
	fm := parseAndWalkFirstFunc(t, src)
	got, ok := fm["label"]
	if !ok {
		t.Fatal("label not in flow map")
	}
	if got.Kind != "ite" {
		t.Fatalf("kind = %q, want ite", got.Kind)
	}
	if got.ThenExpr == nil || got.ThenExpr.Kind != "const" {
		t.Errorf("then_expr = %+v, want const", got.ThenExpr)
	}
	if got.ElseExpr == nil || got.ElseExpr.Kind != "const" {
		t.Errorf("else_expr = %+v, want const (pre-if value)", got.ElseExpr)
	}
}

func TestWalkStmtsForFlow_NoReassignment_NoIte(t *testing.T) {
	// label := 42; if x > 0 { /* no label reassignment */ }
	// → label stays const(42), no ite emitted.
	src := `package p
func f(x int) int {
	label := 42
	if x > 0 {
		_ = x
	}
	return label
}`
	fm := parseAndWalkFirstFunc(t, src)
	got, ok := fm["label"]
	if !ok {
		t.Fatal("label not in flow map")
	}
	if got.Kind == "ite" {
		t.Errorf("expected no ite when label is not reassigned in branches")
	}
	if got.Kind != "const" {
		t.Errorf("kind = %q, want const", got.Kind)
	}
}

// parseAndWalkFirstFunc parses src, extracts the first function's body and
// params, seeds the flow map from params, and returns the result of
// walkStmtsForFlow over the body statements.
func parseAndWalkFirstFunc(t *testing.T, src string) flowMap {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "t.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(file.Decls) == 0 {
		t.Fatal("no declarations in source")
	}
	fn, ok := file.Decls[0].(*ast.FuncDecl)
	if !ok || fn.Body == nil {
		t.Fatal("first declaration is not a function with a body")
	}
	params := make(map[string]bool)
	if fn.Type.Params != nil {
		for _, field := range fn.Type.Params.List {
			for _, name := range field.Names {
				params[name.Name] = true
			}
		}
	}
	fm := make(flowMap, len(params))
	for name := range params {
		fm[name] = &symExpr{Kind: "param", Name: name, Path: []string{}}
	}
	return walkStmtsForFlow(fn.Body.List, params, fm)
}
