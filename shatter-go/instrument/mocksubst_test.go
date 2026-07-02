package instrument

import (
	"go/parser"
	"go/printer"
	"go/token"
	"strings"
	"testing"

	"pgregory.net/rapid"
)

func mustFormat(t *testing.T, src string) string {
	t.Helper()
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse: %v\nsrc:\n%s", err, src)
	}
	var b strings.Builder
	if err := printer.Fprint(&b, fset, f); err != nil {
		t.Fatalf("print: %v", err)
	}
	return b.String()
}

func rewrite(t *testing.T, src string, subs []MockSubstitution) (string, int) {
	t.Helper()
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	n, err := RewriteMockCallSites(f, subs)
	if err != nil {
		t.Fatalf("rewrite err: %v", err)
	}
	var b strings.Builder
	if err := printer.Fprint(&b, fset, f); err != nil {
		t.Fatalf("print: %v", err)
	}
	out := b.String()
	// The rewritten output must always be valid Go.
	if _, err := parser.ParseFile(token.NewFileSet(), "y.go", out, 0); err != nil {
		t.Fatalf("rewritten source does not parse: %v\n%s", err, out)
	}
	return out, n
}

func TestRewriteMockCallSites_ReplacesPointerConstructor(t *testing.T) {
	src := `package target

import "other"

func FuncA(ctx int) string {
	thing := other.GetThing(ctx)
	if thing == nil {
		return "nil"
	}
	return "nonnil"
}
`
	out, n := rewrite(t, src, []MockSubstitution{
		{QualifiedFunction: "other.GetThing", Expression: "&other.Thing{N: 1}"},
	})
	if n != 1 {
		t.Fatalf("expected 1 rewrite, got %d\n%s", n, out)
	}
	if strings.Contains(out, "other.GetThing(ctx)") {
		t.Fatalf("original call still present:\n%s", out)
	}
	if !strings.Contains(out, "&other.Thing{N: 1}") {
		t.Fatalf("substitution expression missing:\n%s", out)
	}
}

func TestRewriteMockCallSites_MultipleCallSites(t *testing.T) {
	src := `package target

import "b"

func F() {
	_ = b.New()
	_ = b.New()
	_ = b.Other()
}
`
	out, n := rewrite(t, src, []MockSubstitution{
		{QualifiedFunction: "b.New", Expression: "fakeNew()"},
	})
	if n != 2 {
		t.Fatalf("expected 2 rewrites, got %d\n%s", n, out)
	}
	if strings.Count(out, "fakeNew()") != 2 {
		t.Fatalf("expected 2 fakeNew(), got:\n%s", out)
	}
	if !strings.Contains(out, "b.Other()") {
		t.Fatalf("non-mocked call must be preserved:\n%s", out)
	}
}

func TestRewriteMockCallSites_NoMatch(t *testing.T) {
	src := `package target

import "b"

func F() { _ = b.Keep() }
`
	_, n := rewrite(t, src, []MockSubstitution{
		{QualifiedFunction: "b.Gone", Expression: "nil"},
	})
	if n != 0 {
		t.Fatalf("expected 0 rewrites, got %d", n)
	}
}

func TestRewriteMockCallSites_InvalidExpressionSkipped(t *testing.T) {
	src := `package target

import "b"

func F() { _ = b.Real() }
`
	fset := token.NewFileSet()
	f, _ := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
	n, err := RewriteMockCallSites(f, []MockSubstitution{
		{QualifiedFunction: "b.Real", Expression: "this is not ) valid("},
	})
	if n != 0 {
		t.Fatalf("expected 0 rewrites for invalid expr, got %d", n)
	}
	if err == nil {
		t.Fatalf("expected error for invalid expression")
	}
}

func TestNormalizeMockSymbol(t *testing.T) {
	cases := map[string]string{
		"auth.GetAccount":            "auth.GetAccount",
		"module:Export":              "module.Export",
		"example.com/pkg:NewThing":   "pkg.NewThing",
		"example.com/pkg.NewThing":   "pkg.NewThing",
		"nested.pkg.path.Fn":         "path.Fn",
		"bare":                       "",
		"":                           "",
		"trailing.":                  "",
		".leading":                   "",
	}
	for in, want := range cases {
		if got := normalizeMockSymbol(in); got != want {
			t.Errorf("normalizeMockSymbol(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestMockSubstitutionsFromConfigs_IgnoresWireMocks(t *testing.T) {
	subs := MockSubstitutionsFromConfigs([]MockConfig{
		{Symbol: "wire.Fn", ReturnValues: []any{1, 2}}, // no expression
		{Symbol: "cfg.New", Expression: "fakeNew()"},
	})
	if len(subs) != 1 {
		t.Fatalf("expected 1 substitution, got %d: %+v", len(subs), subs)
	}
	if subs[0].QualifiedFunction != "cfg.New" || subs[0].Expression != "fakeNew()" {
		t.Fatalf("unexpected substitution: %+v", subs[0])
	}
}

// Property: rewriting is idempotent on the qualified-function target — after a
// rewrite, no call to the mocked symbol remains, so a second pass rewrites
// nothing. Also, the output always parses.
func TestProperty_RewriteMockCallSites_IdempotentAndValid(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		nCalls := rapid.IntRange(0, 5).Draw(t, "nCalls")
		var body strings.Builder
		for i := 0; i < nCalls; i++ {
			body.WriteString("\t_ = dep.Make()\n")
		}
		src := "package target\n\nimport \"dep\"\n\nfunc F() {\n" + body.String() + "}\n"

		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
		if err != nil {
			t.Fatalf("parse: %v", err)
		}
		subs := []MockSubstitution{{QualifiedFunction: "dep.Make", Expression: "sentinel()"}}
		n1, err := RewriteMockCallSites(f, subs)
		if err != nil {
			t.Fatalf("rewrite1: %v", err)
		}
		if n1 != nCalls {
			t.Fatalf("expected %d rewrites, got %d", nCalls, n1)
		}
		var b strings.Builder
		if err := printer.Fprint(&b, fset, f); err != nil {
			t.Fatalf("print: %v", err)
		}
		out := b.String()
		if _, err := parser.ParseFile(token.NewFileSet(), "y.go", out, 0); err != nil {
			t.Fatalf("output invalid: %v\n%s", err, out)
		}
		// Second pass over the produced AST rewrites nothing more.
		n2, err := RewriteMockCallSites(f, subs)
		if err != nil {
			t.Fatalf("rewrite2: %v", err)
		}
		if n2 != 0 {
			t.Fatalf("expected idempotent second pass, got %d", n2)
		}
	})
}

var _ = mustFormat
