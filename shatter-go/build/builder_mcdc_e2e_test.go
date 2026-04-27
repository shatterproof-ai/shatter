//go:build integration

package build_test

import (
	"context"
	"encoding/json"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

const compoundAndSrc = `package targets

func CompoundAnd(a int, b int) string {
	if a > 0 && b < 10 {
		return "both"
	}
	return "neither"
}
`

const compoundOrSrc = `package targets

func CompoundOr(x bool, y bool) string {
	if x || y {
		return "either"
	}
	return "none"
}
`

func compoundAndRequest(modDir string) build.BuildRequest {
	return build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:         "example.com/targets:CompoundAnd",
				SymbolName: "CompoundAnd",
				Kind:       wrapper.TargetKindFunction,
				Parameters: []wrapper.WrapperParam{
					{Name: "a", GoType: "int"},
					{Name: "b", GoType: "int"},
				},
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
}

func compoundOrRequest(modDir string) build.BuildRequest {
	return build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:         "example.com/targets:CompoundOr",
				SymbolName: "CompoundOr",
				Kind:       wrapper.TargetKindFunction,
				Parameters: []wrapper.WrapperParam{
					{Name: "x", GoType: "bool"},
					{Name: "y", GoType: "bool"},
				},
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
}

func invokeMcdc(t *testing.T, req build.BuildRequest, targetID string, inputs []json.RawMessage) []instrument.BranchDecision {
	t.Helper()
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	b := build.NewBuilder(mustWorkspace(t))

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
		"target_id":     targetID,
		"receiver_kind": "",
	})
	if err != nil {
		t.Fatalf("marshal plan: %v", err)
	}

	resp, err := session.Invoke(launcher.LauncherRequest{
		Plan:    planJSON,
		Inputs:  inputs,
		Capture: true,
	})
	if err != nil {
		t.Fatalf("Invoke: %v", err)
	}
	if resp.Error != "" {
		t.Fatalf("launcher error: %s", resp.Error)
	}
	if resp.ThrownError != nil {
		t.Fatalf("unexpected thrown error: %+v", resp.ThrownError)
	}

	var branches []instrument.BranchDecision
	if len(resp.BranchPath) == 0 {
		return branches
	}
	if err := json.Unmarshal(resp.BranchPath, &branches); err != nil {
		t.Fatalf("decode branch_path: %v", err)
	}
	return branches
}

func TestBuilderMcdcAndChain(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	modDir, _ := setupFixtureModule(t, compoundAndSrc, "example.com/targets")
	req := compoundAndRequest(modDir)

	branches := invokeMcdc(t, req, "example.com/targets:CompoundAnd",
		[]json.RawMessage{json.RawMessage("5"), json.RawMessage("5")})

	if len(branches) == 0 {
		t.Fatal("expected branch decisions to be recorded")
	}

	found := false
	for _, b := range branches {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			for i, c := range b.Conditions {
				if c.Masked {
					t.Errorf("condition %d should not be masked (both true)", i)
				}
				if c.Value == nil || !*c.Value {
					t.Errorf("condition %d should have value=true", i)
				}
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions, got: %+v", branches)
	}
}

func TestBuilderMcdcAndChainShortCircuit(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	modDir, _ := setupFixtureModule(t, compoundAndSrc, "example.com/targets")
	req := compoundAndRequest(modDir)

	branches := invokeMcdc(t, req, "example.com/targets:CompoundAnd",
		[]json.RawMessage{json.RawMessage("-1"), json.RawMessage("5")})

	found := false
	for _, b := range branches {
		if !b.Taken && len(b.Conditions) == 2 {
			found = true
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if b.Conditions[0].Value == nil || *b.Conditions[0].Value {
				t.Error("condition 0 should have value=false")
			}
			if !b.Conditions[1].Masked {
				t.Error("condition 1 should be masked")
			}
			if b.Conditions[1].Value != nil {
				t.Error("masked condition 1 should have nil value")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a not-taken branch with 2 MC/DC conditions; got: %+v", branches)
	}
}

func TestBuilderMcdcOrChain(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	modDir, _ := setupFixtureModule(t, compoundOrSrc, "example.com/targets")
	req := compoundOrRequest(modDir)

	branches := invokeMcdc(t, req, "example.com/targets:CompoundOr",
		[]json.RawMessage{json.RawMessage("false"), json.RawMessage("true")})

	found := false
	for _, b := range branches {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if b.Conditions[0].Value == nil || *b.Conditions[0].Value {
				t.Error("condition 0 (x=false) should have value=false")
			}
			if b.Conditions[1].Masked {
				t.Error("condition 1 should not be masked")
			}
			if b.Conditions[1].Value == nil || !*b.Conditions[1].Value {
				t.Error("condition 1 (y=true) should have value=true")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions; got: %+v", branches)
	}
}

func TestBuilderMcdcOrChainShortCircuit(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "1")

	modDir, _ := setupFixtureModule(t, compoundOrSrc, "example.com/targets")
	req := compoundOrRequest(modDir)

	branches := invokeMcdc(t, req, "example.com/targets:CompoundOr",
		[]json.RawMessage{json.RawMessage("true"), json.RawMessage("true")})

	found := false
	for _, b := range branches {
		if b.Taken && len(b.Conditions) == 2 {
			found = true
			if b.Conditions[0].Masked {
				t.Error("condition 0 should not be masked")
			}
			if !b.Conditions[1].Masked {
				t.Error("condition 1 should be masked (short-circuit)")
			}
			break
		}
	}
	if !found {
		t.Errorf("expected a taken branch with 2 MC/DC conditions; got: %+v", branches)
	}
}

func TestBuilderMcdcDisabled(t *testing.T) {
	t.Setenv("SHATTER_MCDC", "0")

	modDir, _ := setupFixtureModule(t, compoundAndSrc, "example.com/targets")
	req := compoundAndRequest(modDir)

	branches := invokeMcdc(t, req, "example.com/targets:CompoundAnd",
		[]json.RawMessage{json.RawMessage("5"), json.RawMessage("5")})

	for _, b := range branches {
		if len(b.Conditions) > 0 {
			t.Errorf("MC/DC disabled: expected no conditions on branch, got %d", len(b.Conditions))
		}
	}
}
