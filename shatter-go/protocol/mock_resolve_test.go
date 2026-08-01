package protocol

import (
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// The mock resolver (this package) and the rewriter (instrument) must recognize
// exactly the same call sites: the resolver records per-enclosing-function
// allow-lists that the rewriter consumes, so a site one pass sees and the other
// does not means either a mock silently stops applying (allow-list entry the
// rewriter never reaches) or an unvetted site gets rewritten. Since str-n0rtz
// both passes drive instrument.WalkQualifiedCalls; these tests pin that
// agreement against sources built to break a hand-rolled parallel walker
// (shadowing, package-scope literals, generic instantiations, nested literals).

// mockAgreementCase is one corpus entry. wantRewritten and wantResolved are
// "<enclosingFuncKey>|<qualifier>.<Func>" site keys; they differ only where a
// pass-specific guard (not the traversal) deliberately drops a site.
type mockAgreementCase struct {
	name          string
	src           string
	symbols       []string
	wantResolved  []string
	wantRewritten []string
}

const mockAgreementDepPkg = `package dep

// Fetch, Map and Pair stand in for mockable dependency entry points; Map and
// Pair are generic so call sites parse as IndexExpr / IndexListExpr.
func Fetch() int { return 1 }

func Map[T any](v T) int { return 2 }

func Pair[A any, B any](a A, b B) int { return 3 }
`

func mockAgreementCases() []mockAgreementCase {
	return []mockAgreementCase{
		{
			name: "shadowing_methods_and_generics",
			src: `package target

import "example.com/agree/dep"

type client struct{}

func (client) Fetch() int { return 9 }

func newClient() client { return client{} }

type Svc struct{}

// Genuine is a plain package call: resolved and rewritten.
func Genuine() int { return dep.Fetch() }

// Shadowed calls a method on a local named like the package: neither pass may
// treat it as a package call.
func Shadowed() int {
	dep := newClient()
	return dep.Fetch()
}

// Generic / GenericPair are instantiations; before str-n0rtz neither pass saw
// them, so mocks silently never applied.
func Generic() int { return dep.Map[int](1) }

func GenericPair() int { return dep.Pair[int, string](1, "x") }

// Method exercises the "(recv).Name" enclosing-function key form.
func (s *Svc) Method() int { return dep.Fetch() }

// Nested pins that a call inside nested function literals inherits the
// enclosing named function's key on both sides.
func Nested() func() int {
	return func() int {
		return func() int { return dep.Fetch() }()
	}
}
`,
			symbols:       []string{"dep.Fetch", "dep.Map", "dep.Pair"},
			wantResolved:  []string{"(*Svc).Method|dep.Fetch", "GenericPair|dep.Pair", "Generic|dep.Map", "Genuine|dep.Fetch", "Nested|dep.Fetch"},
			wantRewritten: []string{"(*Svc).Method|dep.Fetch", "GenericPair|dep.Pair", "Generic|dep.Map", "Genuine|dep.Fetch", "Nested|dep.Fetch"},
		},
		{
			name: "package_scope_literals",
			src: `package target

import "example.com/agree/dep"

// Both initializers call the package at package scope: enclosing key "".
var Handler = func() int { return dep.Fetch() }

var Generic = func() int { return dep.Map[int](1) }
`,
			symbols:       []string{"dep.Fetch", "dep.Map"},
			wantResolved:  []string{"|dep.Fetch", "|dep.Map"},
			wantRewritten: []string{"|dep.Fetch", "|dep.Map"},
		},
		{
			name: "package_scope_literal_shadow_guard",
			src: `package target

import "example.com/agree/dep"

type client struct{}

func (client) Fetch() int { return 9 }

func newClient() client { return client{} }

// A package-scope literal binding "dep" makes every ""-scope site ambiguous
// for the position-blind rewriter, which skips them all even though the
// resolver proved Other's site is a real package call.
var Shadow = func() int {
	dep := newClient()
	return dep.Fetch()
}

var Other = func() int { return dep.Fetch() }
`,
			symbols:       []string{"dep.Fetch"},
			wantResolved:  []string{"|dep.Fetch"},
			wantRewritten: nil,
		},
	}
}

// TestMockResolveAndRewriteAgreeOnCallSites is the str-n0rtz pinning test.
func TestMockResolveAndRewriteAgreeOnCallSites(t *testing.T) {
	for _, tc := range mockAgreementCases() {
		t.Run(tc.name, func(t *testing.T) {
			dir := writeMockAgreementModule(t, tc.src)
			pkg := loadMockAgreementPackage(t, dir)

			subs := make([]instrument.MockSubstitution, 0, len(tc.symbols))
			for _, sym := range tc.symbols {
				subs = append(subs, instrument.MockSubstitution{
					QualifiedFunction: sym,
					// A qualified call so the rewritten AST can be re-walked
					// with the same shared walker to recover each rewrite's
					// enclosing-function key.
					Expression: "mocked.Value()",
				})
			}
			resolved := resolveMockSubstitutionScopes(pkg, subs, nil)

			gotResolved := resolvedSiteKeys(resolved)
			if !sameSites(gotResolved, tc.wantResolved) {
				t.Errorf("resolver sites\n got: %v\nwant: %v", gotResolved, tc.wantResolved)
			}

			gotRewritten := rewrittenSiteKeys(t, filepath.Join(dir, "target.go"), resolved)
			if !sameSites(gotRewritten, tc.wantRewritten) {
				t.Errorf("rewriter sites\n got: %v\nwant: %v", gotRewritten, tc.wantRewritten)
			}

			// Every site the rewriter touched must have been vetted by the
			// resolver; a rewrite outside the allow-list means the passes
			// desynced in the unsafe direction.
			resolvedSet := make(map[string]bool, len(gotResolved))
			for _, s := range gotResolved {
				resolvedSet[s] = true
			}
			for _, s := range gotRewritten {
				if !resolvedSet[s] {
					t.Errorf("rewritten site %q was never recorded by the resolver", s)
				}
			}
		})
	}
}

// TestMockResolveAndRewriteEnumerateIdenticalCandidates asserts the raw
// candidate sets — before either pass applies its own guards — are identical
// between the type-checked AST the resolver walks and the freshly parsed file
// the rewriter walks.
func TestMockResolveAndRewriteEnumerateIdenticalCandidates(t *testing.T) {
	for _, tc := range mockAgreementCases() {
		t.Run(tc.name, func(t *testing.T) {
			dir := writeMockAgreementModule(t, tc.src)
			pkg := loadMockAgreementPackage(t, dir)

			var resolverSide []string
			for _, file := range pkg.Syntax {
				instrument.WalkQualifiedCalls(file, func(site instrument.QualifiedCallSite) ast.Expr {
					resolverSide = append(resolverSide, site.EnclosingFunc+"|"+site.QualifiedName())
					return nil
				})
			}

			var rewriterSide []string
			instrument.WalkQualifiedCalls(parseMockAgreementFile(t, filepath.Join(dir, "target.go")),
				func(site instrument.QualifiedCallSite) ast.Expr {
					rewriterSide = append(rewriterSide, site.EnclosingFunc+"|"+site.QualifiedName())
					return nil
				})

			if !sameSites(resolverSide, rewriterSide) {
				t.Fatalf("candidate call sites diverged\nresolver: %v\nrewriter: %v",
					sortedSiteKeys(resolverSide), sortedSiteKeys(rewriterSide))
			}
		})
	}
}

func writeMockAgreementModule(t *testing.T, src string) string {
	t.Helper()
	dir := t.TempDir()
	depDir := filepath.Join(dir, "dep")
	if err := os.MkdirAll(depDir, 0o755); err != nil {
		t.Fatal(err)
	}
	files := map[string]string{
		filepath.Join(dir, "go.mod"):    "module example.com/agree\n\ngo 1.21\n",
		filepath.Join(dir, "target.go"): src,
		filepath.Join(depDir, "dep.go"): mockAgreementDepPkg,
	}
	for path, content := range files {
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func loadMockAgreementPackage(t *testing.T, dir string) *packages.Package {
	t.Helper()
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedDeps | packages.NeedTypes |
			packages.NeedSyntax | packages.NeedTypesInfo,
		Dir: dir,
		Env: os.Environ(),
	}
	loaded, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("load package: %v", err)
	}
	if len(loaded) != 1 {
		t.Fatalf("expected 1 package, got %d", len(loaded))
	}
	pkg := loaded[0]
	for _, e := range pkg.Errors {
		t.Fatalf("package error: %v", e)
	}
	if pkg.TypesInfo == nil {
		t.Fatal("package loaded without type info")
	}
	return pkg
}

func parseMockAgreementFile(t *testing.T, path string) *ast.File {
	t.Helper()
	file, err := parser.ParseFile(token.NewFileSet(), path, nil, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse %q: %v", path, err)
	}
	return file
}

// resolvedSiteKeys flattens the allow-lists a resolve pass recorded.
func resolvedSiteKeys(subs []instrument.MockSubstitution) []string {
	var keys []string
	for _, s := range subs {
		if s.AllowPackageScope {
			keys = append(keys, "|"+s.QualifiedFunction)
		}
		for fn := range s.AllowedFuncs {
			keys = append(keys, fn+"|"+s.QualifiedFunction)
		}
	}
	return keys
}

// rewrittenSiteKeys rewrites a fresh parse of path with subs, then re-walks the
// result to report where the substitution expression landed. The mock
// expression is itself a qualified call, so the shared walker recovers the
// enclosing-function key of every rewrite.
func rewrittenSiteKeys(t *testing.T, path string, subs []instrument.MockSubstitution) []string {
	t.Helper()
	file := parseMockAgreementFile(t, path)
	count, err := instrument.RewriteMockCallSites(file, subs)
	if err != nil {
		t.Fatalf("rewrite: %v", err)
	}
	var keys []string
	instrument.WalkQualifiedCalls(file, func(site instrument.QualifiedCallSite) ast.Expr {
		if site.QualifiedName() != "mocked.Value" {
			return nil
		}
		keys = append(keys, site.EnclosingFunc+"|")
		return nil
	})
	if len(keys) != count {
		t.Fatalf("rewrite reported %d sites but %d substitutions are visible in the output", count, len(keys))
	}
	// Recover which symbol each rewrite replaced by diffing against the
	// candidate sites present before the rewrite.
	before := map[string]int{}
	instrument.WalkQualifiedCalls(parseMockAgreementFile(t, path), func(site instrument.QualifiedCallSite) ast.Expr {
		before[site.EnclosingFunc+"|"+site.QualifiedName()]++
		return nil
	})
	after := map[string]int{}
	instrument.WalkQualifiedCalls(file, func(site instrument.QualifiedCallSite) ast.Expr {
		after[site.EnclosingFunc+"|"+site.QualifiedName()]++
		return nil
	})
	var replaced []string
	for key, n := range before {
		for i := 0; i < n-after[key]; i++ {
			replaced = append(replaced, key)
		}
	}
	return replaced
}

func sortedSiteKeys(in []string) []string {
	out := append([]string(nil), in...)
	sort.Strings(out)
	return out
}

func sameSites(got, want []string) bool {
	return strings.Join(sortedSiteKeys(got), ",") == strings.Join(sortedSiteKeys(want), ",")
}
