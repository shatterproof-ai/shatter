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

// TestExecuteMethodTargetWithPlanCompletes verifies the H5 (str-hy9b.H5)
// happy path: an Execute request that carries a `plan` whose `receiver_kind`
// names a known constructor dispatches through the wrapper's receiver-kind
// switch, runs the method against the constructed receiver, and produces
// outcome=completed. This is the protocol-level mirror of the Rust e2e_concolic
// receiver-aware test and locks the planner→Execute→launcher contract on the
// Go side.
func TestExecuteMethodTargetWithPlanCompletes(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "service.go")
	src := `package main

type Service struct{}

func New() *Service { return &Service{} }

func (s *Service) Compute(n int) int {
	if n > 0 {
		return n + 1
	}
	return -1
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	planJSON := `{"target_id":"main:(*Service).Compute","receiver_kind":"constructor:New","argument_plans":[],"priority":0}`
	extra := fmt.Sprintf(`"file":"%s","function":"Compute","inputs":[5],"plan":%s`, tmp, planJSON)
	req := reqJSON(1, "execute", extra)
	resp := sendRecv(t, req)

	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusCompleted {
		var errDetail string
		if resp.Outcome.ThrownError != nil {
			errDetail = fmt.Sprintf(" thrown_error.message=%q error_type=%q", resp.Outcome.ThrownError.Message, resp.Outcome.ThrownError.ErrorType)
		}
		var reason string
		if resp.Outcome.ShortReason != nil {
			reason = *resp.Outcome.ShortReason
		}
		t.Errorf("outcome.Status = %q, want completed (H5: plan-aware method execute, short_reason=%q%s)", resp.Outcome.Status, reason, errDetail)
	}
	if resp.Outcome.ReturnValue == nil {
		t.Errorf("expected non-nil ReturnValue for completed outcome, got %+v", resp.Outcome)
	} else {
		got := strings.TrimSpace(string(resp.Outcome.ReturnValue))
		if got != "6" {
			t.Errorf("ReturnValue = %q, want 6 (= 5+1)", got)
		}
	}
}

// TestExecuteMethodTargetWithoutPlanSynthesizesPointerReceiverZeroValue
// verifies str-jeen.50: an Execute request that names a method but omits the
// `plan` field no longer falls through to the wrapper's `unknown receiver
// kind` default — the handler now synthesizes a default receiver_kind
// (here "zero_value" for a pointer receiver with no constructor) so the
// invocation completes. This reverses the pre-str-jeen.50 H5 behaviour that
// surfaced `runtime_failed` with "unknown receiver kind" for plan-less
// method targets; that misleading classification was being counted by the
// broad-scan as a successful completed exploration result.
func TestExecuteMethodTargetWithoutPlanSynthesizesPointerReceiverZeroValue(t *testing.T) {
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
	if resp.Outcome.Status != OutcomeStatusCompleted {
		var detail string
		if resp.Outcome.ShortReason != nil {
			detail = " short_reason=" + *resp.Outcome.ShortReason
		}
		if resp.Outcome.ThrownError != nil {
			detail += " thrown_error=" + resp.Outcome.ThrownError.Message
		}
		t.Errorf("outcome.Status = %q, want completed (str-jeen.50: plan-less method execute should synthesize zero_value receiver and complete;%s)", resp.Outcome.Status, detail)
	}
	if resp.Outcome.ThrownError != nil &&
		strings.Contains(resp.Outcome.ThrownError.Message, "unknown receiver kind") {
		t.Errorf("str-jeen.50: 'unknown receiver kind' must not leak through synthesis path, got %q", resp.Outcome.ThrownError.Message)
	}
}

// TestExecuteMethodTargetWithoutPlanSynthesizesValueReceiverZeroValue
// verifies the value-receiver companion to str-jeen.50 synthesis: a method
// declared on a non-pointer struct receiver is invoked against the zero
// value of the receiver type.
func TestExecuteMethodTargetWithoutPlanSynthesizesValueReceiverZeroValue(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "calc.go")
	src := `package main

type Calc struct{}

func (c Calc) Classify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "non_positive"
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"Classify","inputs":[5]`, tmp))
	resp := sendRecv(t, req)

	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusCompleted {
		t.Errorf("outcome.Status = %q, want completed (value receiver synthesis)", resp.Outcome.Status)
	}
	if resp.Outcome.ThrownError != nil &&
		strings.Contains(resp.Outcome.ThrownError.Message, "unknown receiver kind") {
		t.Errorf("'unknown receiver kind' must not leak through synthesis path, got %q", resp.Outcome.ThrownError.Message)
	}
}

// TestFailureOutcomeClassifiesUnknownReceiverKindAsUnsupported is a
// defense-in-depth check: even if the wrapper's `unknown receiver kind` error
// reaches failureOutcome (e.g. a caller deliberately passes an invalid
// receiver_kind that bypasses synthesis), the outcome must be classified as
// `unsupported` / `method_not_supported`, not `runtime_failed`. Counting
// these as "completed runtime failures" was the root cause of str-jeen.50.
func TestFailureOutcomeClassifiesUnknownReceiverKindAsUnsupported(t *testing.T) {
	outcome := failureOutcome(fmt.Errorf("shatter: unknown receiver kind for some-target: bogus"))
	if outcome.Status != OutcomeStatusUnsupported {
		t.Errorf("Status = %q, want unsupported", outcome.Status)
	}
	if outcome.ThrownError == nil {
		t.Fatal("ThrownError must be populated")
	}
	if outcome.ThrownError.ErrorType != "method_not_supported" {
		t.Errorf("ErrorType = %q, want method_not_supported", outcome.ThrownError.ErrorType)
	}
}
