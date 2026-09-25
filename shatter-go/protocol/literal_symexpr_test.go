package protocol

import (
	"go/ast"
	"go/parser"
	"path/filepath"
	"runtime"
	"strconv"
	"testing"

	"pgregory.net/rapid"
)

func parseLit(t interface {
	Helper()
	Fatalf(string, ...any)
}, src string) *ast.BasicLit {
	t.Helper()
	expr, err := parser.ParseExpr(src)
	if err != nil {
		t.Fatalf("ParseExpr(%q): %v", src, err)
	}
	lit, ok := expr.(*ast.BasicLit)
	if !ok {
		t.Fatalf("%q is not a BasicLit", src)
	}
	return lit
}

// TestLitSymExpr_StringRoundTrip: the literal SymExpr of strconv.Quote(s) has Value == s.
func TestLitSymExpr_StringRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		s := rapid.String().Draw(t, "s")
		got := litSymExpr(parseLit(t, strconv.Quote(s)))
		if got.Kind != "const" || got.Type != "str" || got.Value != s {
			t.Fatalf("litSymExpr(%s) = %+v, want const str %q", strconv.Quote(s), got, s)
		}
	})
}

// TestLitSymExpr_RuneIsInt: a rune literal is an int codepoint constant.
func TestLitSymExpr_RuneIsInt(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		r := rapid.Rune().Draw(t, "r")
		got := litSymExpr(parseLit(t, strconv.QuoteRune(r)))
		if got.Kind != "const" || got.Type != "int" || got.Value != int64(r) {
			t.Fatalf("litSymExpr(%s) = %+v, want const int %d", strconv.QuoteRune(r), got, r)
		}
	})
}

// TestAnalyze_LiteralProbe is the str-49drv.102 audit probe (finding frontend-go-03).
func TestAnalyze_LiteralProbe(t *testing.T) {
	_, file, _, _ := runtime.Caller(0)
	path := filepath.Join(filepath.Dir(file), "..", "..", "examples", "go", "rune-literals", "lit.go")
	results, err := AnalyzeFile(path, "Classify")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) < 3 {
		t.Fatalf("branches = %d, want >= 3", len(fn.Branches))
	}
	rhs := func(i int) *SymExpr {
		c := fn.Branches[i].Condition
		if c == nil || c.Right == nil {
			t.Fatalf("branch %d has no right operand: %+v", i, c)
		}
		return c.Right
	}
	if r := rhs(0); r.Type != "str" || r.Value != "a\tb" {
		t.Errorf("branch 0 rhs = %+v, want str \"a\\tb\" with a real tab", r)
	}
	if r := rhs(1); r.Type != "str" || r.Value != "'q'" {
		t.Errorf("branch 1 rhs = %+v, want str \"'q'\"", r)
	}
	if r := rhs(2); r.Type != "int" || r.Value != int64(120) {
		t.Errorf("branch 2 rhs = %+v, want int 120", r)
	}
}

// TestAnalyze_RuneLiteralsSeedStringAndInt: rune literals are harvested as both
// an int codepoint and a one-character string, so a string param ranged over
// with `c == 'x'` still gets "x" from the pool.
func TestAnalyze_RuneLiteralsSeedStringAndInt(t *testing.T) {
	_, file, _, _ := runtime.Caller(0)
	path := filepath.Join(filepath.Dir(file), "..", "..", "examples", "go", "rune-literals", "lit.go")
	results, err := AnalyzeFile(path, "CountRune")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	has := func(typ string, val any) bool {
		for _, l := range results[0].Literals {
			if l.Type == typ && l.Value == val {
				return true
			}
		}
		return false
	}
	for _, c := range []struct {
		r rune
	}{{'x'}, {'y'}} {
		if !has("int", int64(c.r)) {
			t.Errorf("literals missing int %d: %+v", c.r, results[0].Literals)
		}
		if !has("str", string(c.r)) {
			t.Errorf("literals missing str %q: %+v", string(c.r), results[0].Literals)
		}
	}
}
