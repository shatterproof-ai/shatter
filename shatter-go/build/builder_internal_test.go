package build

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

func mockSubstBuildRequest(t *testing.T, sourceFile string) BuildRequest {
	t.Helper()
	return BuildRequest{
		PackageName:      "target",
		TargetModulePath: "example.com/target",
		TargetModuleDir:  filepath.Dir(sourceFile),
		TargetImportPath: "example.com/target",
		TargetPackageDir: filepath.Dir(sourceFile),
		Targets: []wrapper.WrapperTarget{{
			ID:         "example.com/target:F",
			SymbolName: "F",
		}},
		InstrumentedSourceFile: sourceFile,
		Mocks: []instrument.MockConfig{{
			Symbol:     "dep.Make",
			Expression: "7",
		}},
	}
}

// The launcher cache key must change when the instrumented source CONTENT
// changes, not only when its path or the target signatures change. Mock
// substitution rewrites call sites based on source content; a body edit that
// adds, removes, or shadows a mocked call must not reuse a binary built from
// the old source (str-c8djq cross-review, finding 2).
func TestCacheKeyChangesWithSourceContent(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "target.go")
	if err := os.WriteFile(src, []byte("package target\n\nfunc F() int { return dep.Make() }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	before := cacheKey(mockSubstBuildRequest(t, src))

	if err := os.WriteFile(src, []byte("package target\n\nfunc F() int { dep := 1; _ = dep; return 0 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	after := cacheKey(mockSubstBuildRequest(t, src))

	if before == after {
		t.Fatalf("cacheKey unchanged after source content edit: %s", before)
	}
}

// Mock substitution rewrites every instrumented file in the package, so an
// edit to a SIBLING file (target file, signatures, and mock config unchanged)
// must also miss the cache (str-c8djq cross-file review).
func TestCacheKeyChangesWithSiblingSourceContent(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "target.go")
	sibling := filepath.Join(dir, "helper.go")
	if err := os.WriteFile(src, []byte("package target\n\nfunc F() int { return dep.Make() }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(sibling, []byte("package target\n\nfunc helper() int { return 1 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	before := cacheKey(mockSubstBuildRequest(t, src))

	if err := os.WriteFile(sibling, []byte("package target\n\nfunc helper() int { return dep.Make() }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	after := cacheKey(mockSubstBuildRequest(t, src))

	if before == after {
		t.Fatalf("cacheKey unchanged after sibling source edit: %s", before)
	}
}

// The binary bakes in which call sites were rewritten, which depends on the
// resolution OUTCOME, not just the mock config: identical Mocks with a
// different resolved substitution set (e.g. syntactic fallback after a
// transient type-load failure) must produce a different key.
func TestCacheKeyChangesWithSubstitutionResolution(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "target.go")
	if err := os.WriteFile(src, []byte("package target\n\nfunc F() int { return dep.Make() }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	req := mockSubstBuildRequest(t, src)
	req.MockSubstitutions = []instrument.MockSubstitution{{
		QualifiedFunction: "dep.Make",
		Expression:        "7",
		TypeResolved:      true,
		AllowedFuncs:      map[string]bool{"F": true},
	}}
	resolved := cacheKey(req)

	req.MockSubstitutions[0].TypeResolved = false
	req.MockSubstitutions[0].AllowedFuncs = nil
	fallback := cacheKey(req)

	if resolved == fallback {
		t.Fatalf("cacheKey identical for type-resolved and fallback substitution sets: %s", resolved)
	}
}

// Same content at the same path must produce a stable key (the cache must
// still hit across runs when nothing changed).
func TestCacheKeyStableForSameContent(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "target.go")
	if err := os.WriteFile(src, []byte("package target\n\nfunc F() int { return dep.Make() }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	k1 := cacheKey(mockSubstBuildRequest(t, src))
	k2 := cacheKey(mockSubstBuildRequest(t, src))
	if k1 != k2 {
		t.Fatalf("cacheKey not deterministic: %s vs %s", k1, k2)
	}
}
