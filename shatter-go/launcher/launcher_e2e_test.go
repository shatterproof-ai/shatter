//go:build integration

package launcher_test

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
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
