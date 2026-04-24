//go:build integration

package build_test

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestBuilderTwoPlansSameTarget verifies acceptance criterion 1:
// Two plans for the same target trigger exactly one go build, not two.
func TestBuilderTwoPlansSameTarget(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir, ws := setupFixtureModule(t, singleTargetSrc, "example.com/targets")
	b := build.NewBuilder(ws)

	req := singleTargetRequest(modDir)

	ctx := context.Background()
	res1, err := b.Build(ctx, req)
	if err != nil {
		t.Fatalf("Build (plan 1): %v", err)
	}
	if res1.FromCache {
		t.Error("first Build should not be from cache")
	}

	res2, err := b.Build(ctx, req)
	if err != nil {
		t.Fatalf("Build (plan 2): %v", err)
	}
	if !res2.FromCache {
		t.Error("second Build with same discovery hash must be from cache (one build, not two)")
	}
	if res1.BinaryPath != res2.BinaryPath {
		t.Errorf("binary paths differ: %q vs %q", res1.BinaryPath, res2.BinaryPath)
	}
}

// TestBuilderTwoTargetsSamePackage verifies acceptance criterion 2:
// Two targets in the same package trigger two builds but share the GOCACHE.
func TestBuilderTwoTargetsSamePackage(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir, ws := setupFixtureModule(t, twoTargetSrc, "example.com/targets")
	b := build.NewBuilder(ws)

	reqA := twoTargetRequestA(modDir)
	reqB := twoTargetRequestB(modDir)

	ctx := context.Background()
	resA, err := b.Build(ctx, reqA)
	if err != nil {
		t.Fatalf("Build (target A): %v", err)
	}
	if resA.FromCache {
		t.Error("first Build should not be from cache")
	}

	resB, err := b.Build(ctx, reqB)
	if err != nil {
		t.Fatalf("Build (target B): %v", err)
	}
	if resB.FromCache {
		t.Error("second Build for a different target should not be from cache")
	}

	// The two binaries must be different (different discovery hashes).
	if resA.BinaryPath == resB.BinaryPath {
		t.Error("targets with different target sets should produce different binaries")
	}

	// Both binaries must exist.
	for _, path := range []string{resA.BinaryPath, resB.BinaryPath} {
		if _, err := os.Stat(path); err != nil {
			t.Errorf("binary missing: %v", err)
		}
	}
}

// TestBuilderBuildFailureDiagnostics verifies acceptance criterion 3:
// Build failures emit structured diagnostics.
func TestBuilderBuildFailureDiagnostics(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	// Use a non-existent module dir to force a build failure.
	ws := mustWorkspace(t)
	b := build.NewBuilder(ws)

	req := build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{ID: "bad.example/nope:Fn", SymbolName: "Fn", Kind: wrapper.TargetKindFunction},
		},
		PackageName:      "nope",
		TargetModulePath: "bad.example/nope",
		TargetModuleDir:  "/definitely/does/not/exist",
		TargetImportPath: "bad.example/nope",
		TargetPackageDir: "/definitely/does/not/exist",
	}

	res, err := b.Build(context.Background(), req)
	if err == nil {
		t.Fatal("expected Build to fail for non-existent module dir, got nil")
	}
	if len(res.Diagnostics) == 0 {
		t.Error("Build failure must emit at least one Diagnostic")
	}
	for _, d := range res.Diagnostics {
		if d.Kind != build.DiagnosticKindError {
			t.Errorf("diagnostic kind = %q, want %q", d.Kind, build.DiagnosticKindError)
		}
		if d.Message == "" {
			t.Error("diagnostic message must not be empty")
		}
	}
}

// TestBuilderInstrumentedLauncherEmitsRecorderData verifies that the J2
// retirement path builds a launcher binary that returns recorder-backed
// branch and line data rather than the old direct-call harness output.
func TestBuilderInstrumentedLauncherEmitsRecorderData(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir, ws := setupFixtureModule(t, branchingTargetSrc, "example.com/targets")
	b := build.NewBuilder(ws)

	req := build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:           "example.com/targets:Classify",
				SymbolName:   "Classify",
				Kind:         wrapper.TargetKindFunction,
				Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
				HasResult:    true,
				ResultGoType: "string",
			},
		},
		PackageName:            "targets",
		TargetModulePath:       "example.com/targets",
		TargetModuleDir:        modDir,
		TargetImportPath:       "example.com/targets",
		TargetPackageDir:       modDir,
		InstrumentedSourceFile: filepath.Join(modDir, "targets.go"),
	}

	res, err := b.Build(context.Background(), req)
	if err != nil {
		t.Fatalf("Build: %v", err)
	}

	session, err := launcher.OpenSession(res.BinaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer func() {
		if closeErr := session.Close(); closeErr != nil {
			t.Fatalf("Close: %v", closeErr)
		}
	}()

	planJSON, err := json.Marshal(map[string]any{
		"target_id":     "example.com/targets:Classify",
		"receiver_kind": "",
	})
	if err != nil {
		t.Fatalf("marshal plan: %v", err)
	}
	inputJSON, err := json.Marshal(7)
	if err != nil {
		t.Fatalf("marshal input: %v", err)
	}

	resp, err := session.Invoke(launcher.LauncherRequest{
		Plan:    planJSON,
		Inputs:  []json.RawMessage{inputJSON},
		Capture: true,
	})
	if err != nil {
		t.Fatalf("Invoke: %v", err)
	}
	if resp.Error != "" {
		t.Fatalf("launcher error: %s", resp.Error)
	}
	if len(resp.BranchPath) == 0 {
		t.Fatal("expected branch_path to be populated")
	}
	if len(resp.LinesExecuted) == 0 {
		t.Fatal("expected lines_executed to be populated")
	}

	var ret string
	if err := json.Unmarshal(resp.ReturnValue, &ret); err != nil {
		t.Fatalf("unmarshal return value: %v", err)
	}
	if ret != "positive" {
		t.Fatalf("return value = %q, want positive", ret)
	}
}

// ---- helpers ----

const singleTargetSrc = `package targets

func Add(a, b int) int { return a + b }
`

const twoTargetSrc = `package targets

func Add(a, b int) int  { return a + b }
func Sub(a, b int) int  { return a - b }
`

const branchingTargetSrc = `package targets

func Classify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "nonpositive"
}
`

func setupFixtureModule(t *testing.T, src, modulePath string) (modDir string, ws *workspace.Workspace) {
	t.Helper()
	modDir = t.TempDir()
	if err := os.WriteFile(filepath.Join(modDir, "targets.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write targets.go: %v", err)
	}
	goMod := "module " + modulePath + "\n\ngo 1.23\n"
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte(goMod), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	ws = mustWorkspace(t)
	return modDir, ws
}

func mustWorkspace(t *testing.T) *workspace.Workspace {
	t.Helper()
	ws, err := workspace.Open(t.TempDir())
	if err != nil {
		t.Fatalf("open workspace: %v", err)
	}
	if err := ws.Ensure(); err != nil {
		t.Fatalf("workspace.Ensure: %v", err)
	}
	return ws
}

func singleTargetRequest(modDir string) build.BuildRequest {
	return build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:         "example.com/targets:Add",
				SymbolName: "Add",
				Kind:       wrapper.TargetKindFunction,
				Parameters: []wrapper.WrapperParam{
					{Name: "a", GoType: "int"},
					{Name: "b", GoType: "int"},
				},
				HasResult:    true,
				ResultGoType: "int",
			},
		},
		PackageName:      "targets",
		TargetModulePath: "example.com/targets",
		TargetModuleDir:  modDir,
		TargetImportPath: "example.com/targets",
		TargetPackageDir: modDir,
	}
}

func twoTargetRequestA(modDir string) build.BuildRequest {
	req := singleTargetRequest(modDir)
	req.Targets = req.Targets[:1] // just Add
	return req
}

func twoTargetRequestB(modDir string) build.BuildRequest {
	return build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:         "example.com/targets:Sub",
				SymbolName: "Sub",
				Kind:       wrapper.TargetKindFunction,
				Parameters: []wrapper.WrapperParam{
					{Name: "a", GoType: "int"},
					{Name: "b", GoType: "int"},
				},
				HasResult:    true,
				ResultGoType: "int",
			},
		},
		PackageName:      "targets",
		TargetModulePath: "example.com/targets",
		TargetModuleDir:  modDir,
		TargetImportPath: "example.com/targets",
		TargetPackageDir: modDir,
	}
}

func init() {
	// Suppress unused import error for strings — used in other tests in the package.
	_ = strings.Contains
}
