package workspace

import (
	"path/filepath"
	"strings"
	"testing"
)

// TestGoEnvPinsGOCACHEToBuildCacheDir verifies B2: GoEnv() returns an
// environment slice containing GOCACHE pointing at <root>/cache/build.
func TestGoEnvPinsGOCACHEToBuildCacheDir(t *testing.T) {
	root := filepath.Join(t.TempDir(), "ws")
	ws, err := Open(root)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	env := ws.GoEnv()
	wantPrefix := "GOCACHE=" + ws.BuildCacheDir()
	var found string
	for _, entry := range env {
		if strings.HasPrefix(entry, "GOCACHE=") {
			found = entry
		}
	}
	if found == "" {
		t.Fatalf("GoEnv() did not include a GOCACHE= entry; got %d entries", len(env))
	}
	if found != wantPrefix {
		t.Errorf("GOCACHE entry = %q, want %q", found, wantPrefix)
	}
	if !filepath.IsAbs(strings.TrimPrefix(found, "GOCACHE=")) {
		t.Errorf("GOCACHE value must be absolute: %q", found)
	}
}

// TestGoEnvOverridesPreexistingGOCACHE verifies that GoEnv() replaces (not
// duplicates) any GOCACHE entry inherited from the parent process. A
// duplicate would cause `go` to use the last-wins value, but keeping the
// slice clean avoids surprising downstream consumers that scan env.
func TestGoEnvOverridesPreexistingGOCACHE(t *testing.T) {
	t.Setenv("GOCACHE", "/tmp/should-be-replaced")
	root := filepath.Join(t.TempDir(), "ws")
	ws, err := Open(root)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	env := ws.GoEnv()
	hits := 0
	for _, entry := range env {
		if strings.HasPrefix(entry, "GOCACHE=") {
			hits++
			if entry != "GOCACHE="+ws.BuildCacheDir() {
				t.Errorf("GOCACHE entry = %q, want GOCACHE=%s", entry, ws.BuildCacheDir())
			}
		}
	}
	if hits != 1 {
		t.Errorf("expected exactly one GOCACHE entry in env, got %d", hits)
	}
}
