//go:build integration

package instrument

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/overlay"
)

// TestOverlayBuildAndRun is the acceptance check: build a synthetic module
// with `go build -overlay <manifest>`, substituting the fixture's original
// sources with instrumented copies plus an in-package wrapper that
// contains the recorder. Then run the binary and verify branch records
// are emitted.
//
// The synthetic wrapper here is scaffolding for D5's own acceptance —
// D3's real wrapper will subsume it once it lands.
func TestOverlayBuildAndRun(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	fixtureAbs, err := filepath.Abs(fixtureDir)
	if err != nil {
		t.Fatalf("abs fixture: %v", err)
	}

	workspace := t.TempDir()
	generatedDir := filepath.Join(workspace, "generated")

	files, err := InstrumentPackageForOverlay(fixtureAbs, "hash1", generatedDir)
	if err != nil {
		t.Fatalf("InstrumentPackageForOverlay: %v", err)
	}

	// Module layout for the overlay build.
	modRoot := filepath.Join(workspace, "mod")
	pkgAnchor := filepath.Join(modRoot, "overlaypkg")
	if err := os.MkdirAll(pkgAnchor, 0o755); err != nil {
		t.Fatalf("mkdir pkgAnchor: %v", err)
	}
	goMod := "module synth/overlay\n\ngo 1.23\n"
	if err := os.WriteFile(filepath.Join(modRoot, "go.mod"), []byte(goMod), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	// A placeholder file so the overlaypkg directory is a real package
	// on disk (go build walks the tree before applying the overlay).
	placeholder := "package overlaypkg\n"
	for _, f := range files {
		target := filepath.Join(pkgAnchor, filepath.Base(f.OriginalPath))
		if err := os.WriteFile(target, []byte(placeholder), 0o644); err != nil {
			t.Fatalf("write placeholder %q: %v", target, err)
		}
	}

	// Entry point: package main in modRoot that calls the exported hook
	// in overlaypkg. The overlay splices an in-package wrapper that
	// defines the hook + the recorder.
	resultsPath := filepath.Join(workspace, "results.json")
	mainSource := `package main

import "synth/overlay/overlaypkg"

func main() {
	overlaypkg.ShatterDriveAndDump(` + strconv.Quote(resultsPath) + `)
}
`
	if err := os.WriteFile(filepath.Join(modRoot, "main.go"), []byte(mainSource), 0o644); err != nil {
		t.Fatalf("write main.go: %v", err)
	}

	// In-package wrapper: recorder + driver.
	realWrapper := filepath.Join(generatedDir, "shatter_wrapper.go")
	wrapperSource := generateRecorder("overlaypkg") + `
func ShatterDriveAndDump(path string) {
	Classify(1)
	__shatter_reset()
	Classify(-1)
	if err := __shatter_dump_results(path); err != nil {
		panic(err)
	}
}
`
	if err := os.WriteFile(realWrapper, []byte(wrapperSource), 0o644); err != nil {
		t.Fatalf("write wrapper: %v", err)
	}

	// Build the overlay manifest.
	overlaysDir := filepath.Join(workspace, "overlays")
	b := overlay.NewBuilder(overlaysDir, "plan-d5")
	for _, f := range files {
		anchored := filepath.Join(pkgAnchor, filepath.Base(f.OriginalPath))
		if err := b.Add(anchored, f.InstrumentedPath); err != nil {
			t.Fatalf("Builder.Add: %v", err)
		}
	}
	if err := b.AddGenerated(realWrapper, pkgAnchor, "shatter_wrapper.go"); err != nil {
		t.Fatalf("Builder.AddGenerated: %v", err)
	}
	manifest, err := b.Write()
	if err != nil {
		t.Fatalf("Builder.Write: %v", err)
	}

	binaryPath := filepath.Join(workspace, "harness")
	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifest, "-o", binaryPath, ".")
	cmd.Dir = modRoot
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go build -overlay failed: %v\n%s", err, out)
	}

	if out, err := exec.Command(binaryPath).CombinedOutput(); err != nil {
		t.Fatalf("harness run failed: %v\n%s", err, out)
	}

	raw, err := os.ReadFile(resultsPath)
	if err != nil {
		t.Fatalf("read results: %v", err)
	}
	var results struct {
		LinesExecuted []int `json:"lines_executed"`
		BranchPath    []struct {
			BranchID int  `json:"branch_id"`
			Line     int  `json:"line"`
			Taken    bool `json:"taken"`
		} `json:"branch_path"`
	}
	if err := json.Unmarshal(raw, &results); err != nil {
		t.Fatalf("decode results: %v\n%s", err, raw)
	}

	if len(results.BranchPath) == 0 {
		t.Fatalf("expected at least one branch record, got none.\nraw=%s", raw)
	}
	// After __shatter_reset before the second call, only the x=-1 branch
	// should remain — that's the not-taken path for `x > 0`.
	sawNotTaken := false
	for _, bp := range results.BranchPath {
		if !bp.Taken {
			sawNotTaken = true
		}
	}
	if !sawNotTaken {
		t.Errorf("expected a not-taken branch (Classify(-1) after reset); got %+v", results.BranchPath)
	}
}
