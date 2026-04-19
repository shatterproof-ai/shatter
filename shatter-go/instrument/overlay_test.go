package instrument

import (
	"bytes"
	"encoding/json"
	"fmt"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"pgregory.net/rapid"

	"github.com/shatter-dev/shatter/shatter-go/overlay"
)

const fixtureDir = "testdata/overlaypkg"

func TestInstrumentPackageForOverlay_FileSelection(t *testing.T) {
	workspace := t.TempDir()
	files, err := InstrumentPackageForOverlay(fixtureDir, "abc123", filepath.Join(workspace, "generated"))
	if err != nil {
		t.Fatalf("InstrumentPackageForOverlay: %v", err)
	}

	if len(files) != 2 {
		t.Fatalf("expected 2 instrumented files, got %d: %+v", len(files), files)
	}

	wantBasenames := map[string]bool{"branching.go": false, "helper.go": false}
	for _, f := range files {
		base := filepath.Base(f.OriginalPath)
		if _, ok := wantBasenames[base]; !ok {
			t.Errorf("unexpected file in result: %s", base)
		}
		wantBasenames[base] = true

		info, err := os.Stat(f.InstrumentedPath)
		if err != nil {
			t.Errorf("instrumented file %q: stat: %v", f.InstrumentedPath, err)
			continue
		}
		if info.Size() == 0 {
			t.Errorf("instrumented file %q is empty", f.InstrumentedPath)
		}
		if f.PackageName != "overlaypkg" {
			t.Errorf("instrumented file %q: PackageName = %q, want overlaypkg", f.InstrumentedPath, f.PackageName)
		}
		expectedDir := filepath.Join(workspace, "generated", "abc123", "instrumented")
		gotDir := filepath.Dir(f.InstrumentedPath)
		absExpected, _ := filepath.Abs(expectedDir)
		if gotDir != absExpected {
			t.Errorf("instrumented file %q: dir = %q, want %q", f.InstrumentedPath, gotDir, absExpected)
		}
	}
	for base, seen := range wantBasenames {
		if !seen {
			t.Errorf("expected %s in results", base)
		}
	}

	for _, f := range files {
		if strings.HasSuffix(f.OriginalPath, "_test.go") {
			t.Errorf("_test.go file %q must be excluded", f.OriginalPath)
		}
	}
}

// TestInstrumentPackageForOverlay_ParityWithLegacyTempDir is the snapshot
// acceptance criterion: instrumented bytes produced by the overlay path
// must be identical (modulo the package line) to those produced by the
// legacy temp-dir path. Same transformFile invocation ⇒ same constraint
// emission at runtime.
func TestInstrumentPackageForOverlay_ParityWithLegacyTempDir(t *testing.T) {
	workspace := t.TempDir()
	overlayFiles, err := InstrumentPackageForOverlay(fixtureDir, "h", filepath.Join(workspace, "generated"))
	if err != nil {
		t.Fatalf("overlay path: %v", err)
	}

	branching := filepath.Join(fixtureDir, "branching.go")
	legacyDir, err := InstrumentFile(branching, nil, nil)
	if err != nil {
		t.Fatalf("legacy InstrumentFile: %v", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(legacyDir) })

	legacyBytes, err := os.ReadFile(filepath.Join(legacyDir, "branching.go"))
	if err != nil {
		t.Fatalf("read legacy output: %v", err)
	}

	var overlayBytes []byte
	for _, f := range overlayFiles {
		if filepath.Base(f.OriginalPath) == "branching.go" {
			overlayBytes, err = os.ReadFile(f.InstrumentedPath)
			if err != nil {
				t.Fatalf("read overlay output: %v", err)
			}
			break
		}
	}
	if overlayBytes == nil {
		t.Fatalf("overlay path did not produce branching.go")
	}

	if !bytes.Equal(stripPackageLine(legacyBytes), stripPackageLine(overlayBytes)) {
		t.Errorf("instrumented bytes diverge between legacy and overlay paths\n--- legacy ---\n%s\n--- overlay ---\n%s",
			legacyBytes, overlayBytes)
	}
}

func stripPackageLine(src []byte) []byte {
	_, rest, ok := bytes.Cut(src, []byte("\n"))
	if !ok {
		return src
	}
	return rest
}

func TestRegisterInstrumentedOverlay_WritesManifestEntries(t *testing.T) {
	workspace := t.TempDir()
	files, err := InstrumentPackageForOverlay(fixtureDir, "h", filepath.Join(workspace, "generated"))
	if err != nil {
		t.Fatalf("InstrumentPackageForOverlay: %v", err)
	}

	overlaysDir := filepath.Join(workspace, "overlays")
	b := overlay.NewBuilder(overlaysDir, "plan-1")
	if err := RegisterInstrumentedOverlay(b, files); err != nil {
		t.Fatalf("RegisterInstrumentedOverlay: %v", err)
	}
	manifestPath, err := b.Write()
	if err != nil {
		t.Fatalf("Builder.Write: %v", err)
	}

	data, err := os.ReadFile(manifestPath)
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	var manifest overlay.Manifest
	if err := json.Unmarshal(data, &manifest); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	if len(manifest.Replace) != len(files) {
		t.Fatalf("manifest has %d entries, want %d", len(manifest.Replace), len(files))
	}
	for _, f := range files {
		got, ok := manifest.Replace[f.OriginalPath]
		if !ok {
			t.Errorf("manifest missing entry for %q", f.OriginalPath)
			continue
		}
		if got != f.InstrumentedPath {
			t.Errorf("manifest[%q] = %q, want %q", f.OriginalPath, got, f.InstrumentedPath)
		}
	}
}

func TestRegisterInstrumentedOverlay_NilBuilder(t *testing.T) {
	if err := RegisterInstrumentedOverlay(nil, nil); err == nil {
		t.Fatal("expected error for nil builder")
	}
}

func TestInstrumentPackageForOverlay_EmptyDir(t *testing.T) {
	empty := t.TempDir()
	if _, err := InstrumentPackageForOverlay(empty, "h", t.TempDir()); err == nil {
		t.Fatal("expected error for empty package dir")
	}
}

func TestInstrumentPackageForOverlay_MissingDir(t *testing.T) {
	missing := filepath.Join(t.TempDir(), "does-not-exist")
	if _, err := InstrumentPackageForOverlay(missing, "h", t.TempDir()); err == nil {
		t.Fatal("expected error for missing package dir")
	}
}

func TestInstrumentPackageForOverlay_RequiredArgs(t *testing.T) {
	cases := []struct {
		name                              string
		pkgDir, hash, generatedDir string
	}{
		{"empty pkgDir", "", "h", "g"},
		{"empty hash", fixtureDir, "", "g"},
		{"empty generatedDir", fixtureDir, "h", ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := InstrumentPackageForOverlay(tc.pkgDir, tc.hash, tc.generatedDir); err == nil {
				t.Fatal("expected error")
			}
		})
	}
}

// TestInstrumentPackageForOverlay_PropertyValidGoWithBranchRecorders asserts:
// for every synthetic single-branch package generator emits, every output
// file parses as valid Go and every output references __shatter_record_*
// at least once. This is the behavioral invariant on which the
// "branch coverage surfaces in result_summary" acceptance criterion relies.
func TestInstrumentPackageForOverlay_PropertyValidGoWithBranchRecorders(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		nFuncs := rapid.IntRange(1, 5).Draw(t, "nFuncs")
		nFiles := rapid.IntRange(1, 3).Draw(t, "nFiles")

		pkgDir, _ := newSyntheticPackage(t, nFiles, nFuncs)
		generatedDir := newRapidTempDir(t)

		files, err := InstrumentPackageForOverlay(pkgDir, "synth", generatedDir)
		if err != nil {
			t.Fatalf("InstrumentPackageForOverlay: %v", err)
		}
		if len(files) != nFiles {
			t.Fatalf("got %d instrumented files, want %d", len(files), nFiles)
		}

		fset := token.NewFileSet()
		for _, f := range files {
			data, err := os.ReadFile(f.InstrumentedPath)
			if err != nil {
				t.Fatalf("read %q: %v", f.InstrumentedPath, err)
			}
			if _, err := parser.ParseFile(fset, f.InstrumentedPath, data, parser.ParseComments); err != nil {
				t.Fatalf("instrumented file %q does not parse as Go: %v\n%s", f.InstrumentedPath, err, data)
			}
			if !bytes.Contains(data, []byte("__shatter_record_branch")) {
				t.Errorf("instrumented file %q lacks __shatter_record_branch call:\n%s", f.InstrumentedPath, data)
			}
		}
	})
}

// newSyntheticPackage writes nFiles Go source files, each declaring nFuncs
// single-branch functions, into a fresh temp directory. Returns the dir
// and the list of file paths.
func newSyntheticPackage(t rapid.TB, nFiles, nFuncs int) (string, []string) {
	t.Helper()
	dir := newRapidTempDir(t)
	paths := make([]string, 0, nFiles)
	for fi := range nFiles {
		var src bytes.Buffer
		fmt.Fprintln(&src, "package synth")
		for fn := range nFuncs {
			fmt.Fprintf(&src, "func F%d_%d(x int) int {\n\tif x > 0 {\n\t\treturn 1\n\t}\n\treturn 0\n}\n", fi, fn)
		}
		path := filepath.Join(dir, fmt.Sprintf("file_%d.go", fi))
		if err := os.WriteFile(path, src.Bytes(), 0o644); err != nil {
			t.Fatalf("write synthetic source: %v", err)
		}
		paths = append(paths, path)
	}
	return dir, paths
}

func newRapidTempDir(t rapid.TB) string {
	t.Helper()
	dir, err := os.MkdirTemp("", "shatter-overlay-prop-*")
	if err != nil {
		t.Fatalf("MkdirTemp: %v", err)
	}
	if cleaner, ok := t.(interface{ Cleanup(func()) }); ok {
		cleaner.Cleanup(func() { _ = os.RemoveAll(dir) })
	}
	return dir
}
