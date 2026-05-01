//go:build integration

package build_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestBuilderMixedPackageFixture is the str-jeen.35 acceptance test.
//
// It exercises the shatter-go build pipeline against the checked-in
// `examples/go/mixed-package/` fixture, whose target source file declares
// `package main` and lives next to two `_test.go` siblings — one internal
// (`package main`) and one external (`package main_test`).
//
// Contract under regression: when the builder's instrumented-overlay path
// rewrites the target package to `shattertarget`, it must also stage
// rewritten copies of every `_test.go` sibling so `go build -overlay` sees
// a consistent directory. The original str-x0sv failure surfaced as
// `found packages shattertarget (admissions.go) and main (admissions_heuristic_test.go)`
// from `go list`/`go build`. With the fix, the launcher binary builds
// cleanly.
//
// This complements `TestBuilderMixedPackageMainWithTestSiblings`
// (instrumented_overlay_mixedpkg_test.go), which uses a synthesised
// temp-dir fixture. The checked-in fixture under examples/go/ is the
// jeen.35 acceptance artifact and lets the same regression be reproduced
// from the CLI gauntlet or by hand without recreating files.
//
// Cross-ref: str-x0sv (original bug + harness), str-jeen.35 (this fixture).
func TestBuilderMixedPackageFixture(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	_, thisFile, _, _ := runtime.Caller(0)
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	fixtureDir := filepath.Join(repoRoot, "examples", "go", "mixed-package")
	fixtureGoMod := filepath.Join(fixtureDir, "go.mod")
	if _, err := os.Stat(fixtureGoMod); err != nil {
		t.Skipf("mixed-package fixture not present: %v", err)
	}

	const (
		targetModulePath = "example.com/mixedpkg"
		targetImportPath = "example.com/mixedpkg"
		targetID         = "example.com/mixedpkg:Compute"
	)

	req := build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:           targetID,
				SymbolName:   "Compute",
				Kind:         wrapper.TargetKindFunction,
				Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
				HasResult:    true,
				ResultGoType: "string",
			},
		},
		PackageName:            "main",
		TargetModulePath:       targetModulePath,
		TargetModuleDir:        fixtureDir,
		TargetImportPath:       targetImportPath,
		TargetPackageDir:       fixtureDir,
		InstrumentedSourceFile: filepath.Join(fixtureDir, "admissions.go"),
	}

	ws := mustWorkspace(t)
	b := build.NewBuilder(ws)

	res, err := b.Build(context.Background(), req)
	if err != nil {
		var diagMsgs []string
		for _, d := range res.Diagnostics {
			diagMsgs = append(diagMsgs, d.Message)
		}
		joined := strings.Join(diagMsgs, "\n")
		if strings.Contains(err.Error(), "found packages") || strings.Contains(joined, "found packages") {
			t.Fatalf("regression (str-x0sv / str-jeen.35): mixed-package build failed: %v\n%s", err, joined)
		}
		t.Fatalf("Build: %v\n%s", err, joined)
	}
	if res.BinaryPath == "" {
		t.Fatal("Build returned empty BinaryPath")
	}
	if _, statErr := os.Stat(res.BinaryPath); statErr != nil {
		t.Fatalf("launcher binary missing at %s: %v", res.BinaryPath, statErr)
	}
}
