//go:build integration

package build_test

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestBuilderMixedPackageMainWithTestSiblings is the regression for str-x0sv:
// when the target package is `package main` and the directory contains
// sibling `_test.go` files (also `package main`), the renamed overlay
// (`shattertarget`) must coexist with those test files without triggering
// "found packages shattertarget (...) and main (..._test.go)" from
// `go build -overlay`.
func TestBuilderMixedPackageMainWithTestSiblings(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir := t.TempDir()
	const targetSrc = `package main

func Compute(n int) string {
	if n > 0 {
		return "positive"
	}
	return "nonpositive"
}

func main() {}
`
	const testSiblingSrc = `package main

import "testing"

func TestComputePositive(t *testing.T) {
	if Compute(1) != "positive" {
		t.Fatal("unexpected")
	}
}
`
	const externalTestSiblingSrc = `package main_test

import "testing"

func TestExternal(t *testing.T) {
	_ = t
}
`
	const goMod = "module example.com/admissions\n\ngo 1.23\n"

	must := func(name, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(modDir, name), []byte(content), 0o644); err != nil {
			t.Fatalf("write %s: %v", name, err)
		}
	}
	must("admissions.go", targetSrc)
	must("admissions_heuristic_test.go", testSiblingSrc)
	must("admissions_external_test.go", externalTestSiblingSrc)
	must("go.mod", goMod)

	ws := mustWorkspace(t)
	b := build.NewBuilder(ws)

	req := build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:           "example.com/admissions:Compute",
				SymbolName:   "Compute",
				Kind:         wrapper.TargetKindFunction,
				Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
				HasResult:    true,
				ResultGoType: "string",
			},
		},
		PackageName:            "main",
		TargetModulePath:       "example.com/admissions",
		TargetModuleDir:        modDir,
		TargetImportPath:       "example.com/admissions",
		TargetPackageDir:       modDir,
		InstrumentedSourceFile: filepath.Join(modDir, "admissions.go"),
	}

	res, err := b.Build(context.Background(), req)
	if err != nil {
		var diagMsgs []string
		for _, d := range res.Diagnostics {
			diagMsgs = append(diagMsgs, d.Message)
		}
		joined := strings.Join(diagMsgs, "\n")
		if strings.Contains(err.Error(), "found packages") || strings.Contains(joined, "found packages") {
			t.Fatalf("regression: mixed package error from _test.go siblings: %v\n%s", err, joined)
		}
		t.Fatalf("Build: %v\n%s", err, joined)
	}
	if res.BinaryPath == "" {
		t.Fatal("Build returned empty BinaryPath")
	}
	if _, statErr := os.Stat(res.BinaryPath); statErr != nil {
		t.Fatalf("binary not found: %v", statErr)
	}
}
