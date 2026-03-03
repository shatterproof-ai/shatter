package instrument

import (
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
