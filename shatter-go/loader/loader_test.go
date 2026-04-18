package loader

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

func TestLoadPackageReturnsTypedPackage(t *testing.T) {
	testLoader := newTestLoader(t)
	moduleRoot := makeModuleFixture(t, map[string]string{
		"go.mod": "module example.com/sample\n\ngo 1.23.0\n",
		"lib/types.go": `package lib

type Greeter struct {
	Name string
}
`,
		"lib/use.go": `package lib

func Format(g Greeter) string {
	return g.Name
}
`,
	})

	loadedPackage, err := testLoader.LoadPackage(filepath.Join(moduleRoot, "lib"))
	if err != nil {
		t.Fatalf("LoadPackage: %v", err)
	}

	if loadedPackage.Types == nil {
		t.Fatal("LoadPackage should populate Types")
	}
	if len(loadedPackage.Syntax) != 2 {
		t.Fatalf("LoadPackage Syntax length = %d, want 2", len(loadedPackage.Syntax))
	}
	if loadedPackage.TypesInfo == nil {
		t.Fatal("LoadPackage should populate TypesInfo")
	}
	if object := loadedPackage.Types.Scope().Lookup("Greeter"); object == nil {
		t.Fatal("LoadPackage should resolve Greeter in package scope")
	}
}

func TestLoadFileMaterializesStandaloneModule(t *testing.T) {
	testLoader := newTestLoader(t)
	standaloneRoot := t.TempDir()
	standaloneFilePath := filepath.Join(standaloneRoot, "standalone.go")
	standaloneSource := `package standalone

import "fmt"

type Value struct {
	ID int
}

func Describe(value Value) string {
	return fmt.Sprint(value.ID)
}
`
	if err := os.WriteFile(standaloneFilePath, []byte(standaloneSource), 0o644); err != nil {
		t.Fatalf("WriteFile(standalone): %v", err)
	}

	loadedPackage, err := testLoader.LoadFile(standaloneFilePath)
	if err != nil {
		t.Fatalf("LoadFile: %v", err)
	}

	if loadedPackage.Module == nil {
		t.Fatal("LoadFile should populate Module for synthetic module loads")
	}
	if loadedPackage.Types == nil {
		t.Fatal("LoadFile should populate Types")
	}
	if len(loadedPackage.Syntax) != 1 {
		t.Fatalf("LoadFile Syntax length = %d, want 1", len(loadedPackage.Syntax))
	}
	if loadedPackage.TypesInfo == nil {
		t.Fatal("LoadFile should populate TypesInfo")
	}
	if object := loadedPackage.Types.Scope().Lookup("Describe"); object == nil {
		t.Fatal("LoadFile should resolve Describe in package scope")
	}

	cacheKey := cacheKeyFor(cacheKindFile, standaloneFilePath)
	entry, found, err := testLoader.readCacheEntry(cacheKey)
	if err != nil {
		t.Fatalf("readCacheEntry: %v", err)
	}
	if !found {
		t.Fatal("LoadFile should write a cache entry")
	}
	if _, err := os.Stat(filepath.Join(entry.MaterializedRoot, "go.mod")); err != nil {
		t.Fatalf("stat synthetic go.mod: %v", err)
	}
	materializedSource, err := os.ReadFile(entry.MaterializedFile)
	if err != nil {
		t.Fatalf("ReadFile(materialized): %v", err)
	}
	if string(materializedSource) != standaloneSource {
		t.Fatal("materialized source should match the original standalone file")
	}
}

func TestLoadFileCacheRoundTripReusesMaterializedRoot(t *testing.T) {
	testWorkspace := newTestWorkspace(t)
	loaderA, err := New(testWorkspace)
	if err != nil {
		t.Fatalf("New(loaderA): %v", err)
	}

	standaloneRoot := t.TempDir()
	standaloneFilePath := filepath.Join(standaloneRoot, "cached.go")
	firstSource := `package cached

func First() int {
	return 1
}
`
	if err := os.WriteFile(standaloneFilePath, []byte(firstSource), 0o644); err != nil {
		t.Fatalf("WriteFile(firstSource): %v", err)
	}

	if _, err := loaderA.LoadFile(standaloneFilePath); err != nil {
		t.Fatalf("LoadFile(first): %v", err)
	}

	cacheKey := cacheKeyFor(cacheKindFile, standaloneFilePath)
	firstEntry, found, err := loaderA.readCacheEntry(cacheKey)
	if err != nil {
		t.Fatalf("readCacheEntry(first): %v", err)
	}
	if !found {
		t.Fatal("first LoadFile should write a cache entry")
	}

	secondSource := `package cached

func First() int {
	return 1
}

func Second() int {
	return 2
}
`
	if err := os.WriteFile(standaloneFilePath, []byte(secondSource), 0o644); err != nil {
		t.Fatalf("WriteFile(secondSource): %v", err)
	}

	loaderB, err := New(testWorkspace)
	if err != nil {
		t.Fatalf("New(loaderB): %v", err)
	}
	loadedPackage, err := loaderB.LoadFile(standaloneFilePath)
	if err != nil {
		t.Fatalf("LoadFile(second): %v", err)
	}

	secondEntry, found, err := loaderB.readCacheEntry(cacheKey)
	if err != nil {
		t.Fatalf("readCacheEntry(second): %v", err)
	}
	if !found {
		t.Fatal("second LoadFile should keep the cache entry")
	}
	if secondEntry.MaterializedRoot != firstEntry.MaterializedRoot {
		t.Fatalf("MaterializedRoot = %q, want %q", secondEntry.MaterializedRoot, firstEntry.MaterializedRoot)
	}
	if object := loadedPackage.Types.Scope().Lookup("Second"); object == nil {
		t.Fatal("LoadFile should refresh the materialized source when the original file changes")
	}
	materializedSource, err := os.ReadFile(secondEntry.MaterializedFile)
	if err != nil {
		t.Fatalf("ReadFile(materialized second): %v", err)
	}
	if !strings.Contains(string(materializedSource), "func Second() int") {
		t.Fatal("materialized source should be refreshed for repeated loads")
	}
}

func newTestLoader(t *testing.T) *Loader {
	t.Helper()

	testLoader, err := New(newTestWorkspace(t))
	if err != nil {
		t.Fatalf("New(loader): %v", err)
	}
	return testLoader
}

func newTestWorkspace(t *testing.T) *workspace.Workspace {
	t.Helper()

	testWorkspace, err := workspace.Open(filepath.Join(t.TempDir(), "workspace"))
	if err != nil {
		t.Fatalf("workspace.Open: %v", err)
	}
	if err := testWorkspace.Ensure(); err != nil {
		t.Fatalf("workspace.Ensure: %v", err)
	}
	return testWorkspace
}

func makeModuleFixture(t *testing.T, files map[string]string) string {
	t.Helper()

	root := t.TempDir()
	for relativePath, content := range files {
		absolutePath := filepath.Join(root, relativePath)
		if err := os.MkdirAll(filepath.Dir(absolutePath), 0o755); err != nil {
			t.Fatalf("MkdirAll(%q): %v", absolutePath, err)
		}
		if err := os.WriteFile(absolutePath, []byte(content), 0o644); err != nil {
			t.Fatalf("WriteFile(%q): %v", absolutePath, err)
		}
	}
	return root
}
