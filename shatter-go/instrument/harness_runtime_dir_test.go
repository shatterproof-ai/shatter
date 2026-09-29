package instrument

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEnsureHarnessRuntimeDirMaterializesEmbeddedModule(t *testing.T) {
	root := t.TempDir()
	SetHarnessRuntimeRootProvider(func() string { return root })
	t.Cleanup(func() { SetHarnessRuntimeRootProvider(nil) })

	dir, err := EnsureHarnessRuntimeDir()
	if err != nil {
		t.Fatalf("EnsureHarnessRuntimeDir: %v", err)
	}

	if !filepath.IsAbs(dir) {
		t.Fatalf("EnsureHarnessRuntimeDir returned non-absolute path: %q", dir)
	}

	goModPath := filepath.Join(dir, "go.mod")
	goModData, err := os.ReadFile(goModPath)
	if err != nil {
		t.Fatalf("read %s: %v", goModPath, err)
	}
	if !strings.HasPrefix(dir, root) {
		t.Fatalf("dir %q is not under the provided root %q", dir, root)
	}
	if !strings.Contains(string(goModData), "module "+HarnessRuntimeModuleName) {
		t.Fatalf("%s does not declare module %q", goModPath, HarnessRuntimeModuleName)
	}

	runtimePath := filepath.Join(dir, "runtime.go")
	if _, err := os.Stat(runtimePath); err != nil {
		t.Fatalf("stat %s: %v", runtimePath, err)
	}
}

func TestEnsureHarnessRuntimeDirReportsUnavailableRuntime(t *testing.T) {
	blocker := filepath.Join(t.TempDir(), "file")
	if err := os.WriteFile(blocker, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	SetHarnessRuntimeRootProvider(func() string { return blocker })
	t.Cleanup(func() { SetHarnessRuntimeRootProvider(nil) })

	_, err := EnsureHarnessRuntimeDir()
	if err == nil || !strings.Contains(err.Error(), HarnessRuntimeUnavailableMarker) {
		t.Fatalf("err = %v, want it to contain %q", err, HarnessRuntimeUnavailableMarker)
	}
}
