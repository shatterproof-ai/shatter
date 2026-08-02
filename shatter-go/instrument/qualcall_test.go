package instrument

import (
	"go/ast"
	"go/parser"
	"go/token"
	"sort"
	"strings"
	"testing"
)

func parseTestFile(t *testing.T, src string) *ast.File {
	t.Helper()
	file, err := parser.ParseFile(token.NewFileSet(), "target.go", src, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse fixture: %v", err)
	}
	return file
}

// collectQualifiedCalls renders every site the shared walker reports as
// "<enclosingFuncKey>|<qualifier>.<Func>" so tests can compare whole sets.
func collectQualifiedCalls(file *ast.File) []string {
	var got []string
	WalkQualifiedCalls(file, func(site QualifiedCallSite) ast.Expr {
		got = append(got, site.EnclosingFunc+"|"+site.QualifiedName())
		return nil
	})
	sort.Strings(got)
	return got
}

func TestWalkQualifiedCalls_SiteShapes(t *testing.T) {
	// Each declaration below pins one call-site shape the mock resolver and
	// rewriter must agree on: plain selector, single/multi type-argument
	// generic instantiation, method value on a local (still reported — the
	// package-identity guard is the caller's job), nested literals, and
	// package-scope literals (empty enclosing key).
	const src = `package target

import "example.com/dep"

type client struct{}

func (client) Fetch() int { return 0 }

func newClient() client { return client{} }

func Plain() int { return dep.Fetch() }

func Generic() int { return dep.Map[int](1) }

func GenericMulti() int { return dep.Pair[int, string](1, "x") }

func Local() int {
	dep := newClient()
	return dep.Fetch()
}

func (s *Svc) Method() int { return dep.Fetch() }

func Nested() func() int {
	return func() int {
		return func() int { return dep.Fetch() }()
	}
}

var PackageScope = func() int { return dep.Fetch() }

func Chained() int { return outer.inner.Fetch() }
`
	want := []string{
		"(*Svc).Method|dep.Fetch",
		"GenericMulti|dep.Pair",
		"Generic|dep.Map",
		"Local|dep.Fetch",
		"Nested|dep.Fetch",
		"Plain|dep.Fetch",
		"|dep.Fetch", // package-scope function literal
	}
	got := collectQualifiedCalls(parseTestFile(t, src))
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("qualified call sites mismatch\n got: %v\nwant: %v", got, want)
	}
	// `outer.inner.Fetch()` is a selector on a selector, not on an identifier,
	// so it is deliberately not reported by either pass.
	for _, site := range got {
		if strings.Contains(site, "inner.Fetch") {
			t.Fatalf("chained selector should not be reported as a qualified call: %v", got)
		}
	}
}

func TestWalkQualifiedCalls_ReportsGenericInstantiation(t *testing.T) {
	const src = `package target

import "example.com/dep"

func One() int { return dep.Map[int](1) }

func Two() int { return dep.Pair[int, string](1, "x") }

func Plain() int { return dep.Fetch() }
`
	byName := map[string]QualifiedCallSite{}
	WalkQualifiedCalls(parseTestFile(t, src), func(site QualifiedCallSite) ast.Expr {
		byName[site.QualifiedName()] = site
		return nil
	})
	for _, name := range []string{"dep.Map", "dep.Pair"} {
		site, ok := byName[name]
		if !ok {
			t.Fatalf("generic instantiation %q not reported", name)
		}
		if !site.Instantiated {
			t.Errorf("%q should be flagged as an instantiation", name)
		}
		if site.Selector == nil || site.QualifierIdent == nil {
			t.Errorf("%q missing selector/qualifier nodes", name)
		}
	}
	if site := byName["dep.Fetch"]; site.Instantiated {
		t.Error("plain selector call should not be flagged as an instantiation")
	}
}

func TestWalkQualifiedCalls_ReplacesReturnedExpression(t *testing.T) {
	const src = `package target

import "example.com/dep"

func One() int { return dep.Map[int](1) }
`
	file := parseTestFile(t, src)
	WalkQualifiedCalls(file, func(site QualifiedCallSite) ast.Expr {
		if site.QualifiedName() != "dep.Map" {
			return nil
		}
		return ast.NewIdent("42")
	})
	if got := collectQualifiedCalls(file); len(got) != 0 {
		t.Fatalf("replaced call still reported: %v", got)
	}
}

func TestRewriteMockCallSites_GenericInstantiation(t *testing.T) {
	const src = `package target

import "example.com/dep"

func One() int { return dep.Map[int](1) }

func Two() int { return dep.Pair[int, string](1, "x") }
`
	file := parseTestFile(t, src)
	subs := []MockSubstitution{
		{QualifiedFunction: "dep.Map", Expression: "7", TypeResolved: true, AllowedFuncs: map[string]bool{"One": true}},
		{QualifiedFunction: "dep.Pair", Expression: "8", TypeResolved: true, AllowedFuncs: map[string]bool{"Two": true}},
	}
	count, err := RewriteMockCallSites(file, subs)
	if err != nil {
		t.Fatalf("rewrite: %v", err)
	}
	if count != 2 {
		t.Fatalf("expected both generic instantiations rewritten, got %d", count)
	}
}
