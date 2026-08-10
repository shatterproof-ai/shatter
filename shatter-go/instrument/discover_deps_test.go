package instrument

import (
	"os"
	"path/filepath"
	"testing"
)

func writeDepSource(t *testing.T, src string) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "target.go")
	if err := os.WriteFile(path, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func reportedModules(deps []DiscoveredDependency) map[string]bool {
	out := make(map[string]bool, len(deps))
	for _, d := range deps {
		out[d.SourceModule] = true
	}
	return out
}

// A config mock names a call site by source qualifier ("fetch.NewCacheOnly"),
// not by import path. The dependency report must treat the qualifier's import
// as mocked, whether the qualifier is the import's path base or a local alias
// (str-c8djq cross-review, finding 3).
func TestDiscoverDependenciesConfigMockQualifierSuppressesImport(t *testing.T) {
	path := writeDepSource(t, `package target

import (
	"github.com/acme/fetch"
	"github.com/acme/other"
)

func F() { fetch.NewCacheOnly(""); other.Do() }
`)
	deps := discoverDependencies(path, []MockConfig{{Symbol: "fetch.NewCacheOnly", Expression: "nil"}})
	mods := reportedModules(deps)
	if mods["github.com/acme/fetch"] {
		t.Fatalf("config-mocked qualifier fetch should suppress github.com/acme/fetch; got %v", mods)
	}
	if !mods["github.com/acme/other"] {
		t.Fatalf("unmocked github.com/acme/other should still be reported; got %v", mods)
	}
}

func TestDiscoverDependenciesConfigMockAliasSuppressesImport(t *testing.T) {
	path := writeDepSource(t, `package target

import f "github.com/acme/fetch"

func F() { f.NewCacheOnly("") }
`)
	deps := discoverDependencies(path, []MockConfig{{Symbol: "f.NewCacheOnly", Expression: "nil"}})
	if mods := reportedModules(deps); mods["github.com/acme/fetch"] {
		t.Fatalf("aliased config-mocked import should be suppressed; got %v", mods)
	}
}

// A path-qualified config spelling ("module/path.Func") suppresses exactly
// its own import — and, unlike the bare-qualifier form, does NOT suppress a
// same-base-name import from a different module.
func TestDiscoverDependenciesPathQualifiedSymbol(t *testing.T) {
	path := writeDepSource(t, `package target

import (
	"github.com/acme/fetch"
	thirdfetch "thirdparty.io/fetch"
)

func F() { fetch.NewCacheOnly(""); thirdfetch.NewCacheOnly("") }
`)
	deps := discoverDependencies(path, []MockConfig{{Symbol: "github.com/acme/fetch.NewCacheOnly", Expression: "nil"}})
	mods := reportedModules(deps)
	if mods["github.com/acme/fetch"] {
		t.Fatalf("path-qualified mock should suppress its exact import; got %v", mods)
	}
	if !mods["thirdparty.io/fetch"] {
		t.Fatalf("same-base-name import from a DIFFERENT module must stay reported; got %v", mods)
	}
}

// Module-path style symbols ("module" / "module:export") keep their existing
// exact-import-path suppression semantics.
func TestDiscoverDependenciesModuleSymbolStillSuppresses(t *testing.T) {
	path := writeDepSource(t, `package target

import "github.com/acme/fetch"

func F() { fetch.NewCacheOnly("") }
`)
	deps := discoverDependencies(path, []MockConfig{{Symbol: "github.com/acme/fetch:NewCacheOnly"}})
	if mods := reportedModules(deps); mods["github.com/acme/fetch"] {
		t.Fatalf("module:export symbol should suppress the import; got %v", mods)
	}
}
