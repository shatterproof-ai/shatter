package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// TestExecuteEmitsCompletedOutcome verifies A2: a successful invocation
// produces an InvocationOutcome with status=completed.
func TestExecuteEmitsCompletedOutcome(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int { return a + b }
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"add","inputs":[3,4]`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}
	if resp.Outcome == nil {
		t.Fatal("resp.Outcome is nil; expected InvocationOutcome on successful execute")
	}
	if resp.Outcome.Status != OutcomeStatusCompleted {
		t.Errorf("outcome.Status = %q, want completed", resp.Outcome.Status)
	}
	if resp.Outcome.ShortReason != nil {
		t.Errorf("completed outcome should omit short_reason, got %q", *resp.Outcome.ShortReason)
	}
	if len(resp.Outcome.ReturnValue) == 0 {
		t.Error("completed outcome should carry return_value")
	}
}

// TestExecuteEmitsBuildFailedOutcome verifies A2: a target whose source fails
// to compile produces an InvocationOutcome with status=build_failed and a
// non-empty short_reason.
func TestExecuteEmitsBuildFailedOutcome(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	// Missing return statement on a non-void function is a compile error.
	src := `package main

func brokenAdd(a int, b int) int {
	_ = a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"brokenAdd","inputs":[1,2]`, tmp))
	resp := sendRecv(t, req)
	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusBuildFailed {
		t.Errorf("outcome.Status = %q, want build_failed (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ShortReason == nil || strings.TrimSpace(*resp.Outcome.ShortReason) == "" {
		t.Errorf("build_failed outcome must carry non-empty short_reason, got %v", resp.Outcome.ShortReason)
	}
	if resp.Outcome.ThrownError == nil {
		t.Error("build_failed outcome should carry thrown_error with compiler diagnostics")
	}
}

// TestExecuteEmitsTimedOutOutcome verifies A2: a target that exceeds
// SHATTER_EXEC_TIMEOUT produces an InvocationOutcome with status=timed_out.
func TestExecuteEmitsTimedOutOutcome(t *testing.T) {
	t.Setenv("SHATTER_EXEC_TIMEOUT", "1")
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func spin() int {
	x := 0
	for {
		x++
		if x < 0 { return x }
	}
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"spin","inputs":[]`, tmp))
	resp := sendRecv(t, req)
	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusTimedOut {
		t.Errorf("outcome.Status = %q, want timed_out (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ShortReason == nil || strings.TrimSpace(*resp.Outcome.ShortReason) == "" {
		t.Errorf("timed_out outcome must carry non-empty short_reason, got %v", resp.Outcome.ShortReason)
	}
}

// TestHandlerPinsGOCACHEToWorkspace verifies B2 end-to-end: when the handler
// is constructed with a workspace, subsequent `go build` invocations populate
// the workspace-backed build cache directory. Restores the previous provider
// after the test so other tests see the expected default.
func TestHandlerPinsGOCACHEToWorkspace(t *testing.T) {
	prev := instrument.WorkspaceGoEnv
	t.Cleanup(func() {
		// Restore nil provider so other tests fall back to legacy behavior.
		instrument.SetWorkspaceGoEnvProvider(nil)
		_ = prev
	})

	wsRoot := filepath.Join(t.TempDir(), "ws")
	ws, err := workspace.Open(wsRoot)
	if err != nil {
		t.Fatalf("workspace.Open: %v", err)
	}

	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int { return a + b }
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	reqLine := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"add","inputs":[3,4]`, tmp))
	input := strings.NewReader(reqLine + "\n")
	var output bytes.Buffer
	handler := NewHandlerWithWorkspace(input, &output, io.Discard, ws)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	var resp Response
	firstLine := strings.SplitN(strings.TrimSpace(output.String()), "\n", 2)[0]
	if err := json.Unmarshal([]byte(firstLine), &resp); err != nil {
		t.Fatalf("unmarshal: %v (raw: %s)", err, firstLine)
	}
	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (msg: %s)", resp.Status, resp.Message)
	}

	buildCacheDir := ws.BuildCacheDir()
	entries, err := os.ReadDir(buildCacheDir)
	if err != nil {
		t.Fatalf("ReadDir(%s): %v", buildCacheDir, err)
	}
	if len(entries) == 0 {
		t.Errorf("workspace build cache %s is empty; expected go build to have populated it", buildCacheDir)
	}
}

// TestExecuteEmitsUnsupportedOutcome verifies A2: requesting a function the
// source does not define produces an InvocationOutcome with status=unsupported.
func TestExecuteEmitsUnsupportedOutcome(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func present() {}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"absent","inputs":[]`, tmp))
	resp := sendRecv(t, req)
	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusUnsupported {
		t.Errorf("outcome.Status = %q, want unsupported (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ShortReason == nil || strings.TrimSpace(*resp.Outcome.ShortReason) == "" {
		t.Errorf("unsupported outcome must carry non-empty short_reason, got %v", resp.Outcome.ShortReason)
	}
}

// TestExecuteMethodTargetEmitsUnsupportedOutcome verifies C4: requesting a
// method target produces an InvocationOutcome with status=unsupported and a
// short_reason documenting that receiver planning is pending (Phase E), rather
// than a build failure.
func TestExecuteMethodTargetEmitsUnsupportedOutcome(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "service.go")
	src := `package main

type Service struct{ value int }

func (s *Service) Compute(n int) int { return s.value + n }
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"Compute","inputs":[1]`, tmp))
	resp := sendRecv(t, req)

	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusUnsupported {
		t.Errorf("outcome.Status = %q, want unsupported (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ShortReason == nil || !strings.Contains(*resp.Outcome.ShortReason, "receiver planning") {
		t.Errorf("outcome.ShortReason = %v, want message containing 'receiver planning'", resp.Outcome.ShortReason)
	}
	if resp.Outcome.ThrownError == nil || resp.Outcome.ThrownError.ErrorType != "method_not_supported" {
		t.Errorf("outcome.ThrownError.ErrorType = %v, want method_not_supported", resp.Outcome.ThrownError)
	}
}
