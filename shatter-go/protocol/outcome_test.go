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

func TestExecutePropagatesTargetReturnedError(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

import (
	"fmt"
	"strconv"
)

func Validate(s string) error {
	if s == "bad" {
		return fmt.Errorf("invalid value: %s", s)
	}
	return nil
}

func Parse(s string) (int, error) {
	return strconv.Atoi(s)
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	validateReq := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"Validate","inputs":["bad"]`, tmp))
	validateResp := sendRecv(t, validateReq)
	assertFunctionErrorOutcome(t, validateResp, "invalid value: bad")

	parseBadReq := reqJSON(2, "execute", fmt.Sprintf(`"file":"%s","function":"Parse","inputs":["bad"]`, tmp))
	parseBadResp := sendRecv(t, parseBadReq)
	assertFunctionErrorOutcome(t, parseBadResp, "invalid syntax")

	parseGoodReq := reqJSON(3, "execute", fmt.Sprintf(`"file":"%s","function":"Parse","inputs":["42"]`, tmp))
	parseGoodResp := sendRecv(t, parseGoodReq)
	if parseGoodResp.Status != "execute" {
		t.Fatalf("Parse good status = %q, want execute (message: %s)", parseGoodResp.Status, parseGoodResp.Message)
	}
	if parseGoodResp.Outcome == nil || parseGoodResp.Outcome.Status != OutcomeStatusCompleted {
		t.Fatalf("Parse good outcome = %+v, want completed", parseGoodResp.Outcome)
	}
	var got int
	if err := json.Unmarshal(parseGoodResp.ReturnValue, &got); err != nil {
		t.Fatalf("Parse good return value: %v", err)
	}
	if got != 42 {
		t.Fatalf("Parse good return value = %d, want 42", got)
	}
}

func assertFunctionErrorOutcome(t *testing.T, resp Response, wantMessage string) {
	t.Helper()
	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}
	if resp.Outcome == nil {
		t.Fatalf("response missing outcome: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusRuntimeFailed {
		t.Fatalf("outcome.Status = %q, want runtime_failed (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ThrownError == nil {
		t.Fatalf("response missing thrown_error: %+v", resp)
	}
	if resp.Outcome.ThrownError.ErrorType != "function_error" {
		t.Fatalf("thrown_error type = %q, want function_error", resp.Outcome.ThrownError.ErrorType)
	}
	if !strings.Contains(resp.Outcome.ThrownError.Message, wantMessage) {
		t.Fatalf("thrown_error message = %q, want substring %q", resp.Outcome.ThrownError.Message, wantMessage)
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
	// str-adfp: Performance must be present so the core can deserialize the response.
	if resp.Performance == nil {
		t.Fatalf("performance field must be present on unsupported responses")
	}
}

func TestExecuteUnsupportedAdapterErrorIncludesOutcome(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

import "net/http"

func Serve(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"Serve","inputs":[]`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Fatalf("status = %q, want error (response: %+v)", resp.Status, resp)
	}
	if resp.Code != ErrNotSupported {
		t.Fatalf("code = %q, want %s (response: %+v)", resp.Code, ErrNotSupported, resp)
	}
	if resp.Outcome == nil {
		t.Fatalf("unsupported adapter error must include outcome; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusUnsupported {
		t.Errorf("outcome.Status = %q, want unsupported (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Performance == nil {
		t.Fatalf("performance field must be present on unsupported adapter responses")
	}
}

// TestNonCompletedOutcomeWireIncludesPerformance is the str-adfp wire-level
// regression: serialized execute responses for unsupported/skipped_by_policy
// MUST include a "performance" key, because the Rust core's ExecuteResponse
// deserializer requires it (no serde(default) on that field).
func TestNonCompletedOutcomeWireIncludesPerformance(t *testing.T) {
	// Use a function absent from the file to trigger the unsupported path.
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func present() {}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"absent","inputs":[]`, tmp))

	input := strings.NewReader(req + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	raw := strings.TrimSpace(output.String())
	lines := strings.Split(raw, "\n")
	if len(lines) == 0 {
		t.Fatal("no response")
	}
	if !strings.Contains(lines[0], `"performance"`) {
		t.Fatalf("str-adfp regression: wire response missing \"performance\" key\nraw: %s", lines[0])
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

func TestExecuteMethodTargetWithParameterizedConstructorUsesInputPrefix(t *testing.T) {
	tmpDir := t.TempDir()
	tmp := filepath.Join(tmpDir, "loader.go")
	fixtureDir := filepath.Join(tmpDir, "fixtures")
	if err := os.Mkdir(fixtureDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(fixtureDir, "one.txt"), []byte("ok"), 0644); err != nil {
		t.Fatal(err)
	}
	src := `package main

import "os"

type Loader struct{ dir string }

func NewLoader(dir string) *Loader { return &Loader{dir: dir} }

func (l *Loader) Load() (int, error) {
	entries, err := os.ReadDir(l.dir)
	if err != nil {
		return 0, err
	}
	return len(entries), nil
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	fixtureDirJSON, err := json.Marshal(fixtureDir)
	if err != nil {
		t.Fatal(err)
	}
	planJSON := `{"target_id":"main:(*Loader).Load","receiver_kind":"constructor:NewLoader","argument_plans":[],"constructor_arg_plans":[{"param_index":0,"param_name":"dir","kind":"zero","type_hint":"string"}],"priority":0}`
	extra := fmt.Sprintf(`"file":"%s","function":"Load","inputs":[%s],"plan":%s`, tmp, fixtureDirJSON, planJSON)
	resp := sendRecv(t, reqJSON(1, "execute", extra))

	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusCompleted {
		var detail string
		if resp.Outcome.ThrownError != nil {
			detail = " thrown_error=" + resp.Outcome.ThrownError.Message
		}
		t.Fatalf("outcome.Status = %q, want completed;%s", resp.Outcome.Status, detail)
	}
	got := strings.TrimSpace(string(resp.Outcome.ReturnValue))
	if got != "1" {
		t.Fatalf("ReturnValue = %q, want 1", got)
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

// TestFailureOutcomeClassifiesSubprocessExitAsRuntimeCrash is the
// classifier half of the str-jeen.80 regression. When the launcher
// subprocess dies before producing a response, session.go now returns
// an error containing the binary path, exit status, and captured
// stderr. The outcome must be `runtime_failed` with the structured
// short reason and `subprocess_crashed` error type so the broad-run
// report shows the actionable cause instead of an opaque
// runtime_failed bucket.
func TestFailureOutcomeClassifiesSubprocessExitAsRuntimeCrash(t *testing.T) {
	err := fmt.Errorf("launcher: subprocess exited unexpectedly: /tmp/launcher-bin: exit status 7\nstderr: fatal: bad config 12345")
	outcome := failureOutcome(err)
	if outcome.Status != OutcomeStatusRuntimeFailed {
		t.Errorf("Status = %q, want runtime_failed", outcome.Status)
	}
	if outcome.ShortReason == nil || *outcome.ShortReason != "launcher subprocess exited before responding" {
		got := "<nil>"
		if outcome.ShortReason != nil {
			got = *outcome.ShortReason
		}
		t.Errorf("ShortReason = %q, want \"launcher subprocess exited before responding\"", got)
	}
	if outcome.ThrownError == nil {
		t.Fatal("ThrownError must be populated")
	}
	if outcome.ThrownError.ErrorType != "subprocess_crashed" {
		t.Errorf("ErrorType = %q, want subprocess_crashed", outcome.ThrownError.ErrorType)
	}
	if !strings.Contains(outcome.ThrownError.Message, "fatal: bad config 12345") {
		t.Errorf("Message = %q, must preserve captured stderr", outcome.ThrownError.Message)
	}
	if !strings.Contains(outcome.ThrownError.Message, "exit status 7") {
		t.Errorf("Message = %q, must preserve exit status", outcome.ThrownError.Message)
	}
}

// TestExecuteAsyncGoroutinePanicEmitsRuntimeFailure verifies str-1y6q: a target
// that spawns a goroutine which panics (before or shortly after the target
// returns) must not be reported as a successful invocation. The instrumented
// overlay wraps every `go` statement so a panic in the spawned goroutine is
// captured as the invocation's thrown_error rather than crashing the harness
// subprocess after the response has already been written as "completed".
func TestExecuteAsyncGoroutinePanicEmitsRuntimeFailure(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "watcher.go")
	src := `package main

import "time"

func startWatcher() string {
	go func() {
		time.Sleep(30 * time.Millisecond)
		var p *int
		_ = *p
	}()
	return "started"
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"startWatcher","inputs":[]`, tmp))
	resp := sendRecv(t, req)

	if resp.Outcome == nil {
		t.Fatalf("resp.Outcome is nil; response: %+v", resp)
	}
	if resp.Outcome.Status == OutcomeStatusCompleted {
		t.Fatalf("outcome.Status = completed, but a spawned goroutine panicked: this is the str-1y6q bug — async panics must surface as a failure outcome (response: %+v)", resp)
	}
	if resp.Outcome.Status != OutcomeStatusRuntimeFailed {
		t.Errorf("outcome.Status = %q, want runtime_failed (response: %+v)", resp.Outcome.Status, resp)
	}
	if resp.Outcome.ThrownError == nil {
		t.Fatalf("expected ThrownError populated for runtime_failed outcome (response: %+v)", resp)
	}
	msg := resp.Outcome.ThrownError.Message
	if !strings.Contains(msg, "nil pointer") && !strings.Contains(msg, "invalid memory") {
		t.Errorf("ThrownError.Message should reference the goroutine panic, got %q", msg)
	}
}
