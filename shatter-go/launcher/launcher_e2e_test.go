//go:build integration

package launcher_test

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestLauncherLoopHarness is the acceptance-criteria test for D4.
//
// It verifies that for one target with 5 plans × 10 inputs (50 invocations):
//   - The binary is compiled exactly once (second BuildLauncher call returns fresh=false)
//   - The binary subprocess is started exactly once (one OpenSession call)
//   - Exactly 50 request lines are dispatched and InvocationsDispatched==50
func TestLauncherLoopHarness(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	// --- Synthetic target module with a single Add function ---
	modDir := t.TempDir()
	const targetSrc = `package targets

func Add(a, b int) int { return a + b }
`
	if err := os.WriteFile(filepath.Join(modDir, "targets.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write targets.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/targets\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	// --- Generate the wrapper (D3) ---
	targets := []wrapper.WrapperTarget{
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
	}

	hash := wrapper.DiscoveryHash(targets, nil)
	wrapperDir := t.TempDir()
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "targets", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	wrapperInTree := filepath.Join(modDir, wrapper.WrapperFilename(hash))

	// --- Build the launcher binary (D4) ---
	workDir := t.TempDir()
	generatedDir := filepath.Join(workDir, "generated")
	binariesDir := filepath.Join(workDir, "binaries")

	opts := launcher.BuildOptions{
		TargetModulePath:  "example.com/targets",
		TargetModuleDir:   modDir,
		TargetImportPath:  "example.com/targets",
		DiscoveryHash:     hash,
		WrapperRealPath:   wrapperPath,
		WrapperInTreePath: wrapperInTree,
		GeneratedDir:      generatedDir,
		BinariesDir:       binariesDir,
		GoEnv:             append(os.Environ(), "GOFLAGS="),
	}

	binaryPath, fresh1, err := launcher.BuildLauncher(opts)
	if err != nil {
		t.Fatalf("BuildLauncher (first): %v", err)
	}
	if !fresh1 {
		t.Error("first BuildLauncher call must return fresh=true")
	}

	// --- Second call must reuse the cached binary ---
	_, fresh2, err := launcher.BuildLauncher(opts)
	if err != nil {
		t.Fatalf("BuildLauncher (second): %v", err)
	}
	if fresh2 {
		t.Error("second BuildLauncher call must return fresh=false (cache hit)")
	}

	// --- Open a single session (binary invoked exactly once) ---
	session, err := launcher.OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer session.Close()

	// --- 5 plan descriptors × 10 inputs = 50 invocations ---
	const numPlans = 5
	const numInputsPerPlan = 10

	targetID := "example.com/targets:Add"
	for planIdx := 0; planIdx < numPlans; planIdx++ {
		planJSON, _ := json.Marshal(map[string]string{
			"target_id":     targetID,
			"receiver_kind": "",
		})
		for inputIdx := 0; inputIdx < numInputsPerPlan; inputIdx++ {
			aRaw, _ := json.Marshal(inputIdx)
			bRaw, _ := json.Marshal(planIdx)
			req := launcher.LauncherRequest{
				Plan:   planJSON,
				Inputs: []json.RawMessage{aRaw, bRaw},
			}
			resp, invokeErr := session.Invoke(req)
			if invokeErr != nil {
				t.Fatalf("Invoke[plan=%d,input=%d]: %v", planIdx, inputIdx, invokeErr)
			}
			if resp.Error != "" {
				t.Errorf("Invoke[plan=%d,input=%d] error: %s", planIdx, inputIdx, resp.Error)
			}
		}
	}

	const wantInvocations = numPlans * numInputsPerPlan
	if session.InvocationsDispatched != wantInvocations {
		t.Errorf("InvocationsDispatched = %d, want %d",
			session.InvocationsDispatched, wantInvocations)
	}
}

func TestLauncherBuildsForInternalTargetPackage(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir := t.TempDir()
	targetDir := filepath.Join(modDir, "api", "internal", "handler")
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		t.Fatalf("mkdir target package: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/target\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	const targetSrc = `package handler

func Double(n int) int { return n * 2 }
`
	if err := os.WriteFile(filepath.Join(targetDir, "handler.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write handler.go: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:           "example.com/target/api/internal/handler:Double",
			SymbolName:   "Double",
			Kind:         wrapper.TargetKindFunction,
			Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
			HasResult:    true,
			ResultGoType: "int",
		},
	}

	hash := wrapper.DiscoveryHash(targets, nil)
	wrapperDir := t.TempDir()
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "handler", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	workDir := t.TempDir()
	binaryPath, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath:  "example.com/target",
		TargetModuleDir:   modDir,
		TargetImportPath:  "example.com/target/api/internal/handler",
		DiscoveryHash:     hash,
		WrapperRealPath:   wrapperPath,
		WrapperInTreePath: filepath.Join(targetDir, wrapper.WrapperFilename(hash)),
		GeneratedDir:      filepath.Join(workDir, "generated"),
		BinariesDir:       filepath.Join(workDir, "binaries"),
		GoEnv:             append(os.Environ(), "GOFLAGS="),
	})
	if err != nil {
		t.Fatalf("BuildLauncher: %v", err)
	}

	session, err := launcher.OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer session.Close()

	planJSON, err := json.Marshal(map[string]string{
		"target_id":     "example.com/target/api/internal/handler:Double",
		"receiver_kind": "",
	})
	if err != nil {
		t.Fatalf("marshal plan: %v", err)
	}
	inputJSON, err := json.Marshal(21)
	if err != nil {
		t.Fatalf("marshal input: %v", err)
	}
	resp, err := session.Invoke(launcher.LauncherRequest{
		Plan:   planJSON,
		Inputs: []json.RawMessage{inputJSON},
	})
	if err != nil {
		t.Fatalf("Invoke: %v", err)
	}
	if resp.Error != "" {
		t.Fatalf("launcher error: %s", resp.Error)
	}

	var got int
	if err := json.Unmarshal(resp.ReturnValue, &got); err != nil {
		t.Fatalf("unmarshal return value: %v", err)
	}
	if got != 42 {
		t.Fatalf("return value = %d, want 42", got)
	}
}

// TestLauncherBuildsForInternalFixture is the str-jeen.32 regression test.
// It exercises the launcher build pipeline against the checked-in
// `examples/go/internal-method/` fixture, targeting the deeply-nested
// internal package `example.com/spike/api/internal/handler:Classify`.
//
// Cross-ref: str-b7zh. The contract under regression is the launcher
// module-name anchor logic in
// `shatter-go/launcher/launcher.go::computeLauncherModuleName`: when a
// target import path contains an `internal/` segment that is NOT directly
// under the module root, the anchor must be the path immediately above the
// deepest `internal/` segment. Without the anchor, the synthesised launcher
// module path is rooted at the target module path and falls outside Go's
// internal-visibility tree (parent of the deepest `internal/`), so
// `go build` fails with `use of internal package ... not allowed`.
//
// The fixture's module-internal shape (`module example.com/spike` with the
// target at `api/internal/handler`) is deliberate: the analyzer-side
// fixture (`api.go` -> `internal/svc`) at the module root cannot exercise
// the anchor because `internal/` directly under the module root collapses
// the anchor into a no-op (segments[:deepestInternalIndex] equals
// targetModulePath). The deeper nesting is the load-bearing shape for the
// regression.
//
// Empirical fail-without-fix verification (str-jeen.32 / option (a),
// 2026-05-01): running this test green, then neutralising the
// `if deepestInternalIndex > 0 { anchorPath = strings.Join(...) }` branch
// in computeLauncherModuleName (lines 198-200 of launcher.go), reproduces
// a `go build` failure with the exact diagnostic
//
//	main.go:10:2: use of internal package
//	example.com/spike/api/internal/handler not allowed
//
// because the launcher module path collapses to
// `example.com/spike/shatter_launcher_<hash>`, which is outside the
// `example.com/spike/api/` subtree that Go requires for an importer of
// `example.com/spike/api/internal/handler`. Restoring the anchor branch
// makes the test pass. The single load-bearing line is the
// `anchorPath = strings.Join(segments[:deepestInternalIndex], "/")`
// assignment.
//
// This test is read-only on launcher source and only edits the fixture.
func TestLauncherBuildsForInternalFixture(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	_, thisFile, _, _ := runtime.Caller(0)
	// thisFile is shatter-go/launcher/launcher_e2e_test.go; three levels up is repo root.
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	fixtureDir := filepath.Join(repoRoot, "examples", "go", "internal-method")
	fixtureGoMod := filepath.Join(fixtureDir, "go.mod")
	if _, err := os.Stat(fixtureGoMod); err != nil {
		t.Skipf("internal-method fixture not present: %v", err)
	}
	handlerSrc := filepath.Join(fixtureDir, "api", "internal", "handler", "handler.go")
	if _, err := os.Stat(handlerSrc); err != nil {
		t.Skipf("internal-method/api/internal/handler/handler.go fixture not present: %v", err)
	}

	const (
		targetModulePath = "example.com/spike"
		targetImportPath = "example.com/spike/api/internal/handler"
		targetID         = "example.com/spike/api/internal/handler:Classify"
	)

	targets := []wrapper.WrapperTarget{
		{
			ID:           targetID,
			SymbolName:   "Classify",
			Kind:         wrapper.TargetKindFunction,
			Parameters:   []wrapper.WrapperParam{{Name: "x", GoType: "int"}},
			HasResult:    true,
			ResultGoType: "int",
		},
	}

	hash := wrapper.DiscoveryHash(targets, nil)
	wrapperDir := t.TempDir()
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "handler", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	// The wrapper is overlaid into the target package directory (not written
	// in-tree) via the `-overlay` flag inside BuildLauncher, so the fixture
	// stays read-only at run time.
	wrapperInTree := filepath.Join(fixtureDir, "api", "internal", "handler", wrapper.WrapperFilename(hash))

	workDir := t.TempDir()
	binaryPath, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath:  targetModulePath,
		TargetModuleDir:   fixtureDir,
		TargetImportPath:  targetImportPath,
		DiscoveryHash:     hash,
		WrapperRealPath:   wrapperPath,
		WrapperInTreePath: wrapperInTree,
		GeneratedDir:      filepath.Join(workDir, "generated"),
		BinariesDir:       filepath.Join(workDir, "binaries"),
		GoEnv:             append(os.Environ(), "GOFLAGS="),
	})
	if err != nil {
		t.Fatalf("BuildLauncher (str-b7zh regression): %v", err)
	}

	session, err := launcher.OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer session.Close()

	planJSON, err := json.Marshal(map[string]string{
		"target_id":     targetID,
		"receiver_kind": "",
	})
	if err != nil {
		t.Fatalf("marshal plan: %v", err)
	}
	const positiveInput = 7
	const expectedPositive = 1
	inputJSON, err := json.Marshal(positiveInput)
	if err != nil {
		t.Fatalf("marshal input: %v", err)
	}
	resp, err := session.Invoke(launcher.LauncherRequest{
		Plan:   planJSON,
		Inputs: []json.RawMessage{inputJSON},
	})
	if err != nil {
		t.Fatalf("Invoke: %v", err)
	}
	if resp.Error != "" {
		t.Fatalf("launcher error: %s", resp.Error)
	}
	var got int
	if err := json.Unmarshal(resp.ReturnValue, &got); err != nil {
		t.Fatalf("unmarshal return value: %v", err)
	}
	if got != expectedPositive {
		t.Fatalf("Classify(%d) = %d, want %d", positiveInput, got, expectedPositive)
	}
}

// TestLauncherBuildsForMultiImportWrapper is the str-jeen.33 acceptance test.
//
// It exercises the wrapper+launcher build pipeline against the checked-in
// `examples/go/multi-import-wrapper/` fixture, whose single target `Handle`
// has a parameter list that pulls in ten distinct packages — five stdlib
// (context, log/slog, os, io, go/ast) and five non-stdlib local stubs that
// mimic third-party / application packages (pgx, gqlerror, model, search,
// config).
//
// Contract under regression: shatter-go/wrapper.GenerateWrapper must emit
// imports for every package referenced by a target's parameter or result
// type. Without that, the generated wrapper file declares
// `var ctx context.Context`, `var conn *pgx.Conn`, etc. while only importing
// encoding/json + fmt, and `go build` fails with `undefined: context`,
// `undefined: pgx`, and so on. With imports emitted, the launcher binary
// builds cleanly.
//
// Cross-ref: str-jeen.33. The deliberate non-stdlib stubs (pgx, gqlerror,
// model, search, config) are local subpackages of the fixture module
// (`example.com/multiimport/{pgx,gqlerror,...}`). Each WrapperParam below
// uses the package's *short* name in `GoType` (matching what
// wrapperGoType would produce in real wrapper-gen) while also declaring
// the resolved import path via the new `Imports` channel on WrapperTarget.
func TestLauncherBuildsForMultiImportWrapper(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	_, thisFile, _, _ := runtime.Caller(0)
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	fixtureDir := filepath.Join(repoRoot, "examples", "go", "multi-import-wrapper")
	fixtureGoMod := filepath.Join(fixtureDir, "go.mod")
	if _, err := os.Stat(fixtureGoMod); err != nil {
		t.Skipf("multi-import-wrapper fixture not present: %v", err)
	}

	const (
		targetModulePath = "example.com/multiimport"
		targetImportPath = "example.com/multiimport"
		targetID         = "example.com/multiimport:Handle"
	)

	targets := []wrapper.WrapperTarget{
		{
			ID:         targetID,
			SymbolName: "Handle",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "ctx", GoType: "context.Context"},
				{Name: "logger", GoType: "*slog.Logger"},
				{Name: "user", GoType: "model.User"},
				{Name: "query", GoType: "search.Query"},
				{Name: "file", GoType: "*os.File"},
				{Name: "conn", GoType: "*pgx.Conn"},
				{Name: "reader", GoType: "io.Reader"},
				{Name: "ident", GoType: "*ast.Ident"},
				{Name: "gqlErr", GoType: "*gqlerror.Error"},
				{Name: "cfg", GoType: "config.Config"},
			},
			Imports: []string{
				"context",
				"log/slog",
				"example.com/multiimport/model",
				"example.com/multiimport/search",
				"os",
				"example.com/multiimport/pgx",
				"io",
				"go/ast",
				"example.com/multiimport/gqlerror",
				"example.com/multiimport/config",
			},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
	}

	hash := wrapper.DiscoveryHash(targets, nil)
	wrapperDir := t.TempDir()
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "targets", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	// Overlay the wrapper into the fixture's target package directory so the
	// fixture stays read-only at run time (same pattern as str-jeen.32).
	wrapperInTree := filepath.Join(fixtureDir, wrapper.WrapperFilename(hash))

	workDir := t.TempDir()
	binaryPath, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath:  targetModulePath,
		TargetModuleDir:   fixtureDir,
		TargetImportPath:  targetImportPath,
		DiscoveryHash:     hash,
		WrapperRealPath:   wrapperPath,
		WrapperInTreePath: wrapperInTree,
		GeneratedDir:      filepath.Join(workDir, "generated"),
		BinariesDir:       filepath.Join(workDir, "binaries"),
		GoEnv:             append(os.Environ(), "GOFLAGS="),
	})
	if err != nil {
		t.Fatalf("BuildLauncher (str-jeen.33 multi-import): %v", err)
	}
	if _, statErr := os.Stat(binaryPath); statErr != nil {
		t.Fatalf("launcher binary missing at %s: %v", binaryPath, statErr)
	}
}
