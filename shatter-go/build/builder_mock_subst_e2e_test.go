//go:build integration

package build_test

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// setupMockFixtureModule writes a two-package module:
//
//   - package dep with a "real" constructor NewThing(sentinel) that writes a
//     sentinel file (the expensive/unsafe side effect we must avoid) and
//     returns &Thing{N: 5}.
//   - package main whose Classify calls dep.NewThing and has a nil-guard plus
//     an input-driven branch behind the guard.
//
// The sentinel path is baked into target.go so the test can assert whether the
// real constructor body ran.
func setupMockFixtureModule(t *testing.T, sentinelPath string) (modDir string, ws *workspace.Workspace) {
	t.Helper()
	modDir = t.TempDir()

	if err := os.WriteFile(filepath.Join(modDir, "go.mod"),
		[]byte("module example.com/target\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	depDir := filepath.Join(modDir, "dep")
	if err := os.MkdirAll(depDir, 0o755); err != nil {
		t.Fatalf("mkdir dep: %v", err)
	}
	depSrc := `package dep

import "os"

type Thing struct{ N int }

// NewThing performs the "real" side effect (a filesystem write) that mock
// substitution must prevent, then returns a live *Thing.
func NewThing(sentinel string) *Thing {
	_ = os.WriteFile(sentinel, []byte("real-constructor-ran"), 0o644)
	return &Thing{N: 5}
}
`
	if err := os.WriteFile(filepath.Join(depDir, "dep.go"), []byte(depSrc), 0o644); err != nil {
		t.Fatalf("write dep.go: %v", err)
	}

	targetSrc := `package main

import "example.com/target/dep"

const sentinelPath = ` + "`" + sentinelPath + "`" + `

func Classify(n int) int {
	t := dep.NewThing(sentinelPath)
	if t == nil {
		return -1
	}
	if n > 0 {
		return t.N
	}
	return 0
}

func main() {}
`
	if err := os.WriteFile(filepath.Join(modDir, "target.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write target.go: %v", err)
	}

	ws = mustWorkspace(t)
	return modDir, ws
}

func mockFixtureRequest(modDir string, mocks []instrument.MockConfig) build.BuildRequest {
	return build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:           "example.com/target:Classify",
				SymbolName:   "Classify",
				Kind:         wrapper.TargetKindFunction,
				Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
				HasResult:    true,
				ResultGoType: "int",
			},
		},
		PackageName:            "main",
		TargetModulePath:       "example.com/target",
		TargetModuleDir:        modDir,
		TargetImportPath:       "example.com/target",
		TargetPackageDir:       modDir,
		InstrumentedSourceFile: filepath.Join(modDir, "target.go"),
		Mocks:                  mocks,
	}
}

func invokeClassify(t *testing.T, binaryPath string, n int) launcher.LauncherResponse {
	t.Helper()
	session, err := launcher.OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer func() {
		if closeErr := session.Close(); closeErr != nil {
			t.Fatalf("Close: %v", closeErr)
		}
	}()

	planJSON, _ := json.Marshal(map[string]any{
		"target_id":     "example.com/target:Classify",
		"receiver_kind": "",
	})
	inputJSON, _ := json.Marshal(n)
	resp, err := session.Invoke(launcher.LauncherRequest{
		Plan:    planJSON,
		Inputs:  []json.RawMessage{inputJSON},
		Capture: true,
	})
	if err != nil {
		t.Fatalf("Invoke(n=%d): %v", n, err)
	}
	if resp.Error != "" {
		t.Fatalf("launcher error (n=%d): %s", n, resp.Error)
	}
	return resp
}

// TestMockSubstitution_ConstructorReplaced is the str-c8djq regression: a
// configured expression mock for a cross-package constructor replaces the
// real call site, so the real constructor body (a filesystem write) never
// runs and the mock's value drives execution. It also confirms the branch
// gated behind the nil-guard becomes reachable (both outcomes explored).
func TestMockSubstitution_ConstructorReplaced(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	sentinel := filepath.Join(t.TempDir(), "sentinel.txt")
	modDir, ws := setupMockFixtureModule(t, sentinel)

	mocks := []instrument.MockConfig{
		{Symbol: "dep.NewThing", Expression: "&dep.Thing{N: 99}"},
	}
	res, err := build.NewBuilder(ws).Build(context.Background(), mockFixtureRequest(modDir, mocks))
	if err != nil {
		t.Fatalf("Build: %v", err)
	}

	// n > 0 → returns the mocked Thing.N (99), NOT the real constructor's 5.
	respPos := invokeClassify(t, res.BinaryPath, 7)
	var got int
	if err := json.Unmarshal(respPos.ReturnValue, &got); err != nil {
		t.Fatalf("unmarshal return: %v", err)
	}
	if got != 99 {
		t.Fatalf("return(n=7) = %d, want 99 (mock value); real constructor returns 5", got)
	}

	// The real constructor body (os.WriteFile) must NOT have run.
	if _, err := os.Stat(sentinel); !os.IsNotExist(err) {
		t.Fatalf("sentinel file exists (%v): real constructor side effect was NOT suppressed", err)
	}

	// n <= 0 → the guarded region past the nil-check takes the other branch.
	respNonPos := invokeClassify(t, res.BinaryPath, -1)
	var got2 int
	if err := json.Unmarshal(respNonPos.ReturnValue, &got2); err != nil {
		t.Fatalf("unmarshal return: %v", err)
	}
	if got2 != 0 {
		t.Fatalf("return(n=-1) = %d, want 0", got2)
	}

	// With a non-nil mock, the input-driven branch behind the nil-guard is
	// reachable, so the two runs must differ in their branch path. (Before
	// substitution the real constructor could make only the nil branch
	// reachable.)
	if isEmptyBranchPath(respPos.BranchPath) || isEmptyBranchPath(respNonPos.BranchPath) {
		t.Fatalf("expected branch paths for both runs, got %s / %s", respPos.BranchPath, respNonPos.BranchPath)
	}
	if bytes.Equal(respPos.BranchPath, respNonPos.BranchPath) {
		t.Fatalf("branch paths identical (%s); the input-driven branch was not explored", respPos.BranchPath)
	}
}

func isEmptyBranchPath(raw json.RawMessage) bool {
	s := string(bytes.TrimSpace(raw))
	return s == "" || s == "null" || s == "[]"
}

// TestMockSubstitution_ControlRealConstructorRuns is the negative control:
// without a mock, the real constructor runs (sentinel written, value 5). This
// proves the mock — not the fixture — is what suppresses the side effect.
func TestMockSubstitution_ControlRealConstructorRuns(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	sentinel := filepath.Join(t.TempDir(), "sentinel.txt")
	modDir, ws := setupMockFixtureModule(t, sentinel)

	res, err := build.NewBuilder(ws).Build(context.Background(), mockFixtureRequest(modDir, nil))
	if err != nil {
		t.Fatalf("Build: %v", err)
	}

	resp := invokeClassify(t, res.BinaryPath, 7)
	var got int
	if err := json.Unmarshal(resp.ReturnValue, &got); err != nil {
		t.Fatalf("unmarshal return: %v", err)
	}
	if got != 5 {
		t.Fatalf("return(n=7) = %d, want 5 (real constructor)", got)
	}
	if _, err := os.Stat(sentinel); err != nil {
		t.Fatalf("sentinel file missing (%v): real constructor should have written it", err)
	}
}

// setupScraperFixtureModule mimics a kapow scraper/browser importer shape:
// package browser exposes NewSession(headless bool) *Session whose real body
// launches an external process (a subprocess side effect standing in for a
// real Chromium/Rod launch). The target Explore constructs a session and
// branches on it — exactly the branch-heavy importer shape from the issue's
// low-coverage scan. The real constructor MUST NOT run under a mock.
func setupScraperFixtureModule(t *testing.T, sentinelPath string) (modDir string, ws *workspace.Workspace) {
	t.Helper()
	modDir = t.TempDir()

	if err := os.WriteFile(filepath.Join(modDir, "go.mod"),
		[]byte("module example.com/importer\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	browserDir := filepath.Join(modDir, "browser")
	if err := os.MkdirAll(browserDir, 0o755); err != nil {
		t.Fatalf("mkdir browser: %v", err)
	}
	// The real constructor spawns a subprocess (touch), standing in for a real
	// browser launch. Mock substitution must prevent this from ever running.
	browserSrc := `package browser

import "os/exec"

type Session struct {
	Ready bool
	Pages int
}

// NewSession is the "real" browser constructor: it launches an external
// process. Under Shatter validation this must be mocked, never executed.
func NewSession(headless bool, sentinel string) *Session {
	_ = exec.Command("touch", sentinel).Run()
	return &Session{Ready: true, Pages: 3}
}
`
	if err := os.WriteFile(filepath.Join(browserDir, "browser.go"), []byte(browserSrc), 0o644); err != nil {
		t.Fatalf("write browser.go: %v", err)
	}

	targetSrc := `package main

import "example.com/importer/browser"

const sentinelPath = ` + "`" + sentinelPath + "`" + `

// Explore is a branch-heavy importer entrypoint that constructs a browser
// session and cannot make progress past the nil-guard unless the session is
// live — the exact shape that scores 0 coverage when the real browser can't
// launch in the sandbox.
func Explore(n int) int {
	s := browser.NewSession(true, sentinelPath)
	if s == nil {
		return -1
	}
	if !s.Ready {
		return -2
	}
	if n > 0 {
		return s.Pages
	}
	return 0
}

func main() {}
`
	if err := os.WriteFile(filepath.Join(modDir, "importer.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write importer.go: %v", err)
	}

	ws = mustWorkspace(t)
	return modDir, ws
}

// TestMockSubstitution_ScraperBrowserShape is the str-c8djq kapow validation:
// a browser/scraper-shaped target explores its branches without launching the
// real browser subprocess, because the constructor call is replaced by a mock
// expression.
func TestMockSubstitution_ScraperBrowserShape(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	sentinel := filepath.Join(t.TempDir(), "browser-launched.txt")
	modDir, ws := setupScraperFixtureModule(t, sentinel)

	mocks := []instrument.MockConfig{
		{Symbol: "browser.NewSession", Expression: "&browser.Session{Ready: true, Pages: 42}"},
	}
	req := build.BuildRequest{
		Targets: []wrapper.WrapperTarget{
			{
				ID:           "example.com/importer:Explore",
				SymbolName:   "Explore",
				Kind:         wrapper.TargetKindFunction,
				Parameters:   []wrapper.WrapperParam{{Name: "n", GoType: "int"}},
				HasResult:    true,
				ResultGoType: "int",
			},
		},
		PackageName:            "main",
		TargetModulePath:       "example.com/importer",
		TargetModuleDir:        modDir,
		TargetImportPath:       "example.com/importer",
		TargetPackageDir:       modDir,
		InstrumentedSourceFile: filepath.Join(modDir, "importer.go"),
		Mocks:                  mocks,
	}

	res, err := build.NewBuilder(ws).Build(context.Background(), req)
	if err != nil {
		t.Fatalf("Build: %v", err)
	}

	session, err := launcher.OpenSession(res.BinaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	defer session.Close()

	planJSON, _ := json.Marshal(map[string]any{"target_id": "example.com/importer:Explore", "receiver_kind": ""})
	invoke := func(n int) int {
		inputJSON, _ := json.Marshal(n)
		resp, err := session.Invoke(launcher.LauncherRequest{Plan: planJSON, Inputs: []json.RawMessage{inputJSON}, Capture: true})
		if err != nil {
			t.Fatalf("Invoke(n=%d): %v", n, err)
		}
		if resp.Error != "" {
			t.Fatalf("launcher error (n=%d): %s", n, resp.Error)
		}
		var v int
		if err := json.Unmarshal(resp.ReturnValue, &v); err != nil {
			t.Fatalf("unmarshal (n=%d): %v", n, err)
		}
		return v
	}

	// The session-gated region is now reachable: n>0 returns the mocked
	// Pages (42), n<=0 returns 0. Neither -1 (nil) nor -2 (!Ready) is hit.
	if got := invoke(7); got != 42 {
		t.Fatalf("Explore(7) = %d, want 42 (mocked session.Pages)", got)
	}
	if got := invoke(-3); got != 0 {
		t.Fatalf("Explore(-3) = %d, want 0", got)
	}

	// The real browser subprocess must never have launched.
	if _, err := os.Stat(sentinel); !os.IsNotExist(err) {
		t.Fatalf("browser-launched sentinel exists (%v): real subprocess was NOT suppressed", err)
	}
}
