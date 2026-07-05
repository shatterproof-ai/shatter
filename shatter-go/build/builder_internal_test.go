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
