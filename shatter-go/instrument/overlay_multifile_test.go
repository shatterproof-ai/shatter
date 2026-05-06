package instrument

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// TestInstrumentPackageForOverlay_MultiFilePreservesContents is the
// regression fixture for str-jeen.51. Zolem's broad scan saw the Go
// compiler reject overlay-materialized package files (discovery.go,
// loader.go, validator.go) with `expected package, found EOF` — a
// silent zero-byte materialization. This test asserts that a
// realistic multi-file Go package is materialized with non-empty
// instrumented sources whose package declarations match the input.
func TestInstrumentPackageForOverlay_MultiFilePreservesContents(t *testing.T) {
	const fixture = "testdata/multifilepkg"
	wantBasenames := []string{"discovery.go", "loader.go", "validator.go"}

	for _, base := range wantBasenames {
		info, err := os.Stat(filepath.Join(fixture, base))
		if err != nil {
			t.Fatalf("fixture %q: stat: %v", base, err)
		}
		if info.Size() == 0 {
			t.Fatalf("fixture %q is unexpectedly zero bytes", base)
		}
	}

	generatedDir := filepath.Join(t.TempDir(), "generated")
	files, err := InstrumentPackageForOverlay(fixture, "multihash", generatedDir)
	if err != nil {
		t.Fatalf("InstrumentPackageForOverlay: %v", err)
	}
	if len(files) != len(wantBasenames) {
		t.Fatalf("got %d instrumented files, want %d", len(files), len(wantBasenames))
	}

	seen := make(map[string]bool, len(wantBasenames))
	for _, f := range files {
		base := filepath.Base(f.OriginalPath)
		seen[base] = true

		if f.PackageName != "multifilepkg" {
			t.Errorf("instrumented %q: PackageName = %q, want multifilepkg", base, f.PackageName)
		}

		data, err := os.ReadFile(f.InstrumentedPath)
		if err != nil {
			t.Fatalf("read instrumented %q: %v", f.InstrumentedPath, err)
		}
		if len(data) == 0 {
			t.Fatalf("instrumented %q is zero bytes — workspace materialization regressed", f.InstrumentedPath)
		}
		if !strings.Contains(string(data), "package multifilepkg") {
			t.Errorf("instrumented %q missing `package multifilepkg` declaration:\n%s", base, data)
		}
	}
	for _, base := range wantBasenames {
		if !seen[base] {
			t.Errorf("expected instrumented file for %s", base)
		}
	}
}

// TestInstrumentPackageForOverlay_EmptySourceIsRejected guards the
// preflight contract: if a zero-byte materialized file ever sneaks past
// the writer, callers see a specific workspace-materialization error
// (wrapping workspace.ErrEmptyMaterializedFile) rather than an opaque
// `expected package, found EOF` from `go build`.
//
// We exercise the contract directly through the workspace verifier. An
// equivalent end-to-end provocation would require a mocking seam in
// InstrumentPackageForOverlay; the verifier is the authoritative
// detection point and is invoked from every materialization site.
func TestInstrumentPackageForOverlay_EmptySourceIsRejected(t *testing.T) {
	dir := t.TempDir()
	emptyPath := filepath.Join(dir, "discovery.go")
	if err := os.WriteFile(emptyPath, nil, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	err := workspace.VerifyMaterializedSource(emptyPath, true)
	if err == nil {
		t.Fatal("expected verifier to reject zero-byte source")
	}
	if !errors.Is(err, workspace.ErrEmptyMaterializedFile) {
		t.Fatalf("err %v does not wrap ErrEmptyMaterializedFile", err)
	}
}
