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

	if sym.Kind != "binop" {
		t.Fatalf("kind = %q, want binop", sym.Kind)
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

	if sym.Kind != "binop" || sym.Op != "eq" {
		t.Errorf("got kind=%q op=%q, want binop eq", sym.Kind, sym.Op)
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

	if sym.Kind != "binop" || sym.Op != "and" {
		t.Fatalf("got kind=%q op=%q, want binop and", sym.Kind, sym.Op)
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

	if sym.Kind != "binop" || sym.Op != "or" {
		t.Errorf("got kind=%q op=%q, want binop or", sym.Kind, sym.Op)
	}
}

func TestNegation(t *testing.T) {
	expr, _ := parser.ParseExpr("!ok")
	params := map[string]bool{"ok": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "unop" || sym.Op != "not" {
		t.Fatalf("got kind=%q op=%q, want unop not", sym.Kind, sym.Op)
	}
	if sym.Operand.Kind != "param" || sym.Operand.Name != "ok" {
		t.Errorf("operand = %+v, want param ok", sym.Operand)
	}
}

func TestNilComparison(t *testing.T) {
	expr, _ := parser.ParseExpr("p == nil")
	params := map[string]bool{"p": true}
	sym := exprToSymExpr(expr, params)

	if sym.Kind != "binop" || sym.Op != "eq" {
		t.Fatalf("got kind=%q op=%q, want binop eq", sym.Kind, sym.Op)
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

	if sym.Kind != "binop" || sym.Op != "gt" {
		t.Errorf("got kind=%q op=%q, want binop gt", sym.Kind, sym.Op)
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
	if c.Expr == nil || c.Expr.Kind != "binop" {
		t.Errorf("expr = %+v, want binop", c.Expr)
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

func TestBoolLiterals(t *testing.T) {
	for _, tc := range []struct {
		input string
		value string
	}{
		{"true", "true"},
		{"false", "false"},
	} {
		expr, _ := parser.ParseExpr(tc.input)
		sym := exprToSymExpr(expr, map[string]bool{})
		if sym.Kind != "const" || sym.Type != "bool" {
			t.Errorf("%s: got kind=%q type=%q, want const bool", tc.input, sym.Kind, sym.Type)
		}
	}
}
