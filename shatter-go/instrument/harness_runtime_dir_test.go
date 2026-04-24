package instrument

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestEnsureHarnessRuntimeDirPointsAtCheckedInModule(t *testing.T) {
	t.Parallel()

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
	if !strings.Contains(string(goModData), "module "+HarnessRuntimeModuleName) {
		t.Fatalf("%s does not declare module %q", goModPath, HarnessRuntimeModuleName)
	}

	runtimePath := filepath.Join(dir, "runtime.go")
	if _, err := os.Stat(runtimePath); err != nil {
		t.Fatalf("stat %s: %v", runtimePath, err)
	}
}
