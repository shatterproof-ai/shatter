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
		"auth.GetAccount":          "auth.GetAccount",
		"module:Export":            "module.Export",
		"example.com/pkg:NewThing": "pkg.NewThing",
		"example.com/pkg.NewThing": "pkg.NewThing",
		"nested.pkg.path.Fn":       "path.Fn",
		"bare":                     "",
		"":                         "",
		"trailing.":                "",
		".leading":                 "",
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

// TestRewriteMockCallSites_SkipsShadowedLocal is the review-fix-1 regression:
// a config mock for package function auth.GetAccount must NOT rewrite a method
// call on a same-named local variable, while a genuine package call in a
// sibling function IS rewritten.
func TestRewriteMockCallSites_SkipsShadowedLocal(t *testing.T) {
	src := `package target

import "auth"

func Shadowed(id int) int {
	auth := newClient()
	return auth.GetAccount(id)
}

func Genuine(id int) int {
	return auth.GetAccount(id)
}

func newClient() *client { return nil }

type client struct{}

func (c *client) GetAccount(id int) int { return id }
`
	out, n := rewrite(t, src, []MockSubstitution{
		{QualifiedFunction: "auth.GetAccount", Expression: "42"},
	})
	if n != 1 {
		t.Fatalf("expected exactly 1 rewrite (the genuine package call), got %d\n%s", n, out)
	}
	// The shadowed local method call must survive.
	if !strings.Contains(out, "return auth.GetAccount(id)\n}\n\nfunc Genuine") &&
		!strings.Contains(out, "auth := newClient()") {
		t.Fatalf("shadowed local call was wrongly rewritten:\n%s", out)
	}
	// Genuine call replaced by the mock value.
	if strings.Count(out, "auth.GetAccount(id)") != 1 {
		t.Fatalf("expected 1 surviving auth.GetAccount call (the shadowed one), got:\n%s", out)
	}
}

// TestRewriteMockCallSites_RequiresImportedQualifier ensures the syntactic
// fallback does not rewrite a selector whose qualifier is not an imported
// package (e.g. a method call on a field or receiver).
func TestRewriteMockCallSites_RequiresImportedQualifier(t *testing.T) {
	src := `package target

type S struct{ auth authClient }

type authClient struct{}

func (a authClient) GetAccount(id int) int { return id }

func (s S) Do(id int) int {
	return s.auth.GetAccount(id) // s.auth is a field selector, not a package
}
`
	// s.auth.GetAccount is a call on selector s.auth; its Fun is a
	// SelectorExpr whose X is another SelectorExpr (not an *ast.Ident named
	// "auth"), so it never matches "auth.GetAccount".
	_, n := rewrite(t, src, []MockSubstitution{
		{QualifiedFunction: "auth.GetAccount", Expression: "0"},
	})
	if n != 0 {
		t.Fatalf("expected 0 rewrites for non-package qualifier, got %d", n)
	}
}

// TestRewriteMockCallSites_TypeResolvedAllowedFuncs exercises the type-resolved
// path: only call sites inside AllowedFuncs are rewritten (the imports
// heuristic does not apply; the shadow check still does — see
// TestRewriteMockCallSites_TypeResolvedMixedShadowSkipsFunction).
func TestRewriteMockCallSites_TypeResolvedAllowedFuncs(t *testing.T) {
	src := `package target

import "dep"

func A() int { return dep.Make() }
func B() int { return dep.Make() }
`
	out, n := rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowedFuncs:      map[string]bool{"A": true},
	}})
	if n != 1 {
		t.Fatalf("expected 1 rewrite (only A), got %d\n%s", n, out)
	}
	// Exactly one dep.Make() survives — the one inside B (not in AllowedFuncs).
	if c := strings.Count(out, "dep.Make()"); c != 1 {
		t.Fatalf("expected 1 surviving dep.Make() (in B), got %d:\n%s", c, out)
	}
}

// TestRewriteMockCallSites_TypeResolvedMixedShadowSkipsFunction: a function
// containing BOTH a genuine package call and a call on a same-named local
// (`dep := newClient(); dep.Make()`) is skipped entirely in the type-resolved
// path. The allow-list is function-granular while the rewriter is
// position-blind, so rewriting anything here would also rewrite the local's
// method call. Conservative under-mocking (the real dependency runs) is the
// contract (str-c8djq cross-review, finding 1).
func TestRewriteMockCallSites_TypeResolvedMixedShadowSkipsFunction(t *testing.T) {
	src := `package target

import "dep"

type client struct{}

func (c client) Make() int { return 99 }

func newClient() client { return client{} }

func Mixed() int {
	a := dep.Make() // genuine package call: resolve marked Mixed allowed
	dep := newClient()
	return a + dep.Make() // method call on the shadowing local
}

func Clean() int { return dep.Make() }
`
	out, n := rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowedFuncs:      map[string]bool{"Mixed": true, "Clean": true},
	}})
	if n != 1 {
		t.Fatalf("expected exactly 1 rewrite (Clean only; Mixed skipped as shadow-ambiguous), got %d\n%s", n, out)
	}
	if c := strings.Count(out, "dep.Make()"); c != 2 {
		t.Fatalf("expected both dep.Make() calls in Mixed to survive, got %d surviving:\n%s", c, out)
	}
}

// TestRewriteMockCallSites_PackageScopeFuncLitShadow: a package-level function
// literal that binds the qualifier locally makes ALL package-scope call sites
// ambiguous (they share the "" enclosing-function key), so none are rewritten
// in either matching mode (str-c8djq cross-review, finder A/C).
func TestRewriteMockCallSites_PackageScopeFuncLitShadow(t *testing.T) {
	src := `package target

import "dep"

type client struct{}

func (c client) Make() int { return 99 }

func newClient() client { return client{} }

var genuine = dep.Make()

var H = func() int {
	dep := newClient()
	return dep.Make()
}
`
	// Syntactic fallback: the "" bound-name set must block both sites.
	out, n := rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
	}})
	if n != 0 {
		t.Fatalf("syntactic: expected 0 rewrites with package-scope FuncLit shadow, got %d\n%s", n, out)
	}
	// Type-resolved with package scope allowed: same conservative skip.
	out, n = rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowPackageScope: true,
	}})
	if n != 0 {
		t.Fatalf("type-resolved: expected 0 rewrites with package-scope FuncLit shadow, got %d\n%s", n, out)
	}
}

// TestRewriteMockCallSites_TypeResolvedEmptyRewritesNothing: a type-resolved
// substitution whose allow-list is empty (symbol never a genuine package call)
// rewrites nowhere, even though the qualifier is imported.
func TestRewriteMockCallSites_TypeResolvedEmptyRewritesNothing(t *testing.T) {
	src := `package target

import "dep"

func A() int { return dep.Make() }
`
	_, n := rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowedFuncs:      map[string]bool{},
	}})
	if n != 0 {
		t.Fatalf("expected 0 rewrites for empty allow-list, got %d", n)
	}
}

// TestRewriteMockCallSites_MethodTypeResolved verifies funcKey correlation for
// a call inside a method body.
func TestRewriteMockCallSites_MethodTypeResolved(t *testing.T) {
	src := `package target

import "dep"

type Server struct{}

func (s *Server) Handle() int { return dep.Make() }
`
	out, n := rewrite(t, src, []MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowedFuncs:      map[string]bool{"(*Server).Handle": true},
	}})
	if n != 1 {
		t.Fatalf("expected 1 rewrite in the method, got %d\n%s", n, out)
	}
}

// Property: across randomized call-site positions (argument, if-condition,
// range, nested selector chains) the rewriter replaces exactly the genuine
// package calls, output always parses, and a second pass is idempotent.
func TestProperty_RewriteMockCallSites_PositionsAndIdempotent(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		// Build a function body embedding dep.Make() in various positions.
		positions := []string{
			"\t_ = dep.Make()\n",
			"\tif dep.Make() > 0 {\n\t\t_ = 1\n\t}\n",
			"\t_ = []int{dep.Make(), dep.Make()}\n",
			"\tfor range []int{dep.Make()} {\n\t}\n",
		}
		n := rapid.IntRange(0, len(positions)).Draw(t, "n")
		var body strings.Builder
		want := 0
		for i := 0; i < n; i++ {
			body.WriteString(positions[i])
			want += strings.Count(positions[i], "dep.Make()")
		}
		src := "package target\n\nimport \"dep\"\n\nfunc F() {\n" + body.String() + "}\n"

		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, "x.go", src, parser.ParseComments)
		if err != nil {
			t.Fatalf("parse: %v\n%s", err, src)
		}
		subs := []MockSubstitution{{QualifiedFunction: "dep.Make", Expression: "sentinel()"}}
		got, err := RewriteMockCallSites(f, subs)
		if err != nil {
			t.Fatalf("rewrite: %v", err)
		}
		if got != want {
			t.Fatalf("rewrote %d, want %d\nsrc:\n%s", got, want, src)
		}
		var b strings.Builder
		if err := printer.Fprint(&b, fset, f); err != nil {
			t.Fatalf("print: %v", err)
		}
		if _, err := parser.ParseFile(token.NewFileSet(), "y.go", b.String(), 0); err != nil {
			t.Fatalf("output invalid: %v\n%s", err, b.String())
		}
		if second, _ := RewriteMockCallSites(f, subs); second != 0 {
			t.Fatalf("second pass should be idempotent, rewrote %d", second)
		}
	})
}

// Property: normalizeMockSymbol treats the dot and colon separators as
// equivalent and is idempotent on already-normalized input.
func TestProperty_NormalizeMockSymbol_DotColonEquivalenceAndIdempotent(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		identRune := func(t *rapid.T, label string) string {
			s := rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9]*`).Draw(t, label)
			return s
		}
		pkg := identRune(t, "pkg")
		fn := identRune(t, "fn")
		dot := normalizeMockSymbol(pkg + "." + fn)
		colon := normalizeMockSymbol(pkg + ":" + fn)
		if dot != colon {
			t.Fatalf("dot %q != colon %q", dot, colon)
		}
		if dot != pkg+"."+fn {
			t.Fatalf("normalize(%q.%q) = %q, want %q", pkg, fn, dot, pkg+"."+fn)
		}
		// Idempotent.
		if again := normalizeMockSymbol(dot); again != dot {
			t.Fatalf("not idempotent: %q -> %q", dot, again)
		}
	})
}

var _ = mustFormat

// TestParseMockSymbol covers the package-identity decomposition (str-djcv2):
// bare vs path-qualified vs wire-colon spellings.
func TestParseMockSymbol(t *testing.T) {
	cases := []struct {
		in         string
		ok         bool
		importPath string
		base       string
		fn         string
	}{
		{"auth.GetAccount", true, "", "auth", "GetAccount"},
		{"example.com/auth.GetAccount", true, "example.com/auth", "auth", "GetAccount"},
		{"example.com/auth:GetAccount", true, "example.com/auth", "auth", "GetAccount"},
		{"auth:GetAccount", true, "", "auth", "GetAccount"},
		{"a/util.Do", true, "a/util", "util", "Do"},
		{"nested.pkg.path.Fn", true, "", "path", "Fn"},
		{"bare", false, "", "", ""},
		{"", false, "", "", ""},
		{"trailing.", false, "", "", ""},
		{".leading", false, "", "", ""},
	}
	for _, c := range cases {
		got, ok := parseMockSymbol(c.in)
		if ok != c.ok {
			t.Errorf("parseMockSymbol(%q) ok = %v, want %v", c.in, ok, c.ok)
			continue
		}
		if !ok {
			continue
		}
		if got.ImportPath != c.importPath || got.Base != c.base || got.Func != c.fn {
			t.Errorf("parseMockSymbol(%q) = %+v, want {ImportPath:%q Base:%q Func:%q}",
				c.in, got, c.importPath, c.base, c.fn)
		}
	}
}

// TestDedupeMocks_SameBaseNamePackagesDoNotCollide is the str-djcv2 regression
// for consequence 1: two path-qualified mocks whose packages share a base name
// ("util") must NOT collapse — keying by source spelling used to drop one.
func TestDedupeMocks_SameBaseNamePackagesDoNotCollide(t *testing.T) {
	deduped := DedupeMocks([]MockConfig{
		{Symbol: "example.com/a/util.Do", Expression: "fakeA()"},
		{Symbol: "example.com/b/util.Do", Expression: "fakeB()"},
	})
	if len(deduped) != 2 {
		t.Fatalf("expected both same-base-name mocks preserved, got %d: %+v", len(deduped), deduped)
	}
	byExpr := map[string]bool{}
	for _, m := range deduped {
		byExpr[m.Expression] = true
	}
	if !byExpr["fakeA()"] || !byExpr["fakeB()"] {
		t.Fatalf("both expressions must survive, got %+v", deduped)
	}
}

// TestDedupeMocks_PreservesWireFieldsOnUpgrade is the str-djcv2 regression for
// consequence 3: merging a wire mock (ReturnValues/ShouldTrackCalls/
// DefaultBehavior) with a config mock (Expression) must keep BOTH sides'
// contributions, not replace the wire entry wholesale.
func TestDedupeMocks_PreservesWireFieldsOnUpgrade(t *testing.T) {
	deduped := DedupeMocks([]MockConfig{
		{Symbol: "auth:GetAccount", ReturnValues: []any{1, 2}, ShouldTrackCalls: true, DefaultBehavior: "repeat_last"},
		{Symbol: "auth.GetAccount", Expression: "&auth.Account{}"},
	})
	if len(deduped) != 1 {
		t.Fatalf("expected 1 merged mock, got %d: %+v", len(deduped), deduped)
	}
	m := deduped[0]
	if m.Expression != "&auth.Account{}" {
		t.Errorf("config Expression lost: %+v", m)
	}
	if len(m.ReturnValues) != 2 {
		t.Errorf("wire ReturnValues discarded: %+v", m)
	}
	if !m.ShouldTrackCalls {
		t.Errorf("wire ShouldTrackCalls discarded: %+v", m)
	}
	if m.DefaultBehavior != "repeat_last" {
		t.Errorf("wire DefaultBehavior discarded: %+v", m)
	}
}

// TestDedupeMocks_MergeOrderIndependent verifies the merge preserves both
// halves regardless of which entry (wire or config) is seen first.
func TestDedupeMocks_MergeOrderIndependent(t *testing.T) {
	deduped := DedupeMocks([]MockConfig{
		{Symbol: "auth.GetAccount", Expression: "&auth.Account{}"},
		{Symbol: "auth:GetAccount", ReturnValues: []any{1}, ShouldTrackCalls: true},
	})
	if len(deduped) != 1 {
		t.Fatalf("expected 1 merged mock, got %d: %+v", len(deduped), deduped)
	}
	m := deduped[0]
	if m.Expression != "&auth.Account{}" || len(m.ReturnValues) != 1 || !m.ShouldTrackCalls {
		t.Fatalf("merge dropped a field: %+v", m)
	}
}

// TestRewriteMockCallSites_MultipleCandidatesPerSpelling exercises the
// position-blind rewriter's per-function candidate selection (str-djcv2): two
// substitutions sharing the local spelling "util.Do" but with disjoint
// AllowedFuncs each rewrite only their own function.
func TestRewriteMockCallSites_MultipleCandidatesPerSpelling(t *testing.T) {
	src := "package target\n\n" +
		"import \"util\"\n\n" +
		"func A() { _ = util.Do() }\n" +
		"func B() { _ = util.Do() }\n"
	subs := []MockSubstitution{
		{QualifiedFunction: "util.Do", Expression: "\"fromA\"", TypeResolved: true, AllowedFuncs: map[string]bool{"A": true}},
		{QualifiedFunction: "util.Do", Expression: "\"fromB\"", TypeResolved: true, AllowedFuncs: map[string]bool{"B": true}},
	}
	out, n := rewrite(t, src, subs)
	if n != 2 {
		t.Fatalf("expected 2 rewrites, got %d\n%s", n, out)
	}
	// Both distinct expressions must appear (each candidate applied to its own
	// function) and no original call site may remain.
	if !strings.Contains(out, `"fromA"`) || !strings.Contains(out, `"fromB"`) {
		t.Errorf("expected both fromA and fromB expressions in output:\n%s", out)
	}
	if strings.Contains(out, "util.Do") {
		t.Errorf("original call site not fully rewritten:\n%s", out)
	}
	// Association check: fromA precedes fromB (func A declared before func B).
	if strings.Index(out, `"fromA"`) > strings.Index(out, `"fromB"`) {
		t.Errorf("candidate selection mismatched function order:\n%s", out)
	}
}
