package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// --- Test helpers ---

// stubHook is a minimal InvocationHook for testing.
type stubHook struct {
	id      string
	outcome *AdapterInvocationOutcome
	err     error
}

func (h *stubHook) ID() string { return h.id }
func (h *stubHook) Invoke(_ InvocationContext) (*AdapterInvocationOutcome, error) {
	return h.outcome, h.err
}

// stubFactory is a minimal RuntimeHookFactory for testing.
type stubFactory struct {
	id    string
	hooks []InvocationHook
}

func (f *stubFactory) ID() string { return f.id }
func (f *stubFactory) CreateRuntimeHooks(_ ExecutionAdapter, _ RuntimeHookContext) *RuntimeHooks {
	return &RuntimeHooks{InvocationHooks: f.hooks}
}

// conversationWithFactories creates a handler with hook factories pre-registered,
// sends multiple requests, and returns all responses.
func conversationWithFactories(t *testing.T, factories []RuntimeHookFactory, requests ...string) []Response {
	t.Helper()
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	for _, f := range factories {
		handler.RegisterHookFactory(f)
	}
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	var responses []Response
	for _, line := range lines {
		if line == "" {
			continue
		}
		var resp Response
		if err := json.Unmarshal([]byte(line), &resp); err != nil {
			t.Fatalf("unmarshal response: %v (raw: %s)", err, line)
		}
		responses = append(responses, resp)
	}
	return responses
}

// --- ChooseInvocationStrategy tests ---

func TestChooseInvocationStrategy_NilModel(t *testing.T) {
	s := ChooseInvocationStrategy(nil, nil)
	if s.Kind != "direct" {
		t.Fatalf("expected direct, got %s", s.Kind)
	}
}

func TestChooseInvocationStrategy_DirectModel(t *testing.T) {
	model := &InvocationModel{Kind: "direct"}
	s := ChooseInvocationStrategy(model, []InvocationHook{&stubHook{id: "x"}})
	if s.Kind != "direct" {
		t.Fatalf("expected direct, got %s", s.Kind)
	}
}

func TestChooseInvocationStrategy_AdapterMatched(t *testing.T) {
	hook := &stubHook{id: "my-adapter"}
	model := &InvocationModel{Kind: "adapter", AdapterID: "my-adapter"}
	s := ChooseInvocationStrategy(model, []InvocationHook{hook})
	if s.Kind != "adapter" {
		t.Fatalf("expected adapter, got %s", s.Kind)
	}
	if s.Hook.ID() != "my-adapter" {
		t.Fatalf("expected hook id my-adapter, got %s", s.Hook.ID())
	}
	if s.Model != model {
		t.Fatal("expected model to be passed through")
	}
}

func TestChooseInvocationStrategy_AdapterUnmatched(t *testing.T) {
	model := &InvocationModel{Kind: "adapter", AdapterID: "missing"}
	s := ChooseInvocationStrategy(model, []InvocationHook{&stubHook{id: "other"}})
	if s.Kind != "unsupported" {
		t.Fatalf("expected unsupported, got %s", s.Kind)
	}
	if s.AdapterID != "missing" {
		t.Fatalf("expected adapter id missing, got %s", s.AdapterID)
	}
}

func TestChooseInvocationStrategy_AdapterNoHooks(t *testing.T) {
	model := &InvocationModel{Kind: "adapter", AdapterID: "any"}
	s := ChooseInvocationStrategy(model, nil)
	if s.Kind != "unsupported" {
		t.Fatalf("expected unsupported, got %s", s.Kind)
	}
}

// --- ResolveRuntimeHooks tests ---

func TestResolveRuntimeHooks_NilProfile(t *testing.T) {
	hooks, err := ResolveRuntimeHooks(nil, RuntimeHookContext{}, nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(hooks.InvocationHooks) != 0 {
		t.Fatalf("expected empty hooks, got %d", len(hooks.InvocationHooks))
	}
}

func TestResolveRuntimeHooks_EmptyAdapters(t *testing.T) {
	profile := &ExecutionProfile{Adapters: []ExecutionAdapter{}}
	hooks, err := ResolveRuntimeHooks(profile, RuntimeHookContext{}, nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(hooks.InvocationHooks) != 0 {
		t.Fatalf("expected empty hooks, got %d", len(hooks.InvocationHooks))
	}
}

func TestResolveRuntimeHooks_DisabledSkipped(t *testing.T) {
	disabled := ExecutionAdapterApplyDisabled
	profile := &ExecutionProfile{
		Adapters: []ExecutionAdapter{
			{ID: "test", Apply: &disabled},
		},
	}
	hooks, err := ResolveRuntimeHooks(profile, RuntimeHookContext{}, nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(hooks.InvocationHooks) != 0 {
		t.Fatalf("expected empty hooks, got %d", len(hooks.InvocationHooks))
	}
}

func TestResolveRuntimeHooks_UnknownFactory(t *testing.T) {
	profile := &ExecutionProfile{
		Adapters: []ExecutionAdapter{
			{ID: "unknown-adapter"},
		},
	}
	_, err := ResolveRuntimeHooks(profile, RuntimeHookContext{}, nil)
	if err == nil {
		t.Fatal("expected error for unknown adapter")
	}
	if !strings.Contains(err.Error(), "unknown-adapter") {
		t.Fatalf("expected error to mention adapter id, got: %s", err.Error())
	}
}

func TestResolveRuntimeHooks_MergesHooks(t *testing.T) {
	hook1 := &stubHook{id: "adapter-a"}
	hook2 := &stubHook{id: "adapter-b"}
	factories := []RuntimeHookFactory{
		&stubFactory{id: "adapter-a", hooks: []InvocationHook{hook1}},
		&stubFactory{id: "adapter-b", hooks: []InvocationHook{hook2}},
	}
	profile := &ExecutionProfile{
		Adapters: []ExecutionAdapter{
			{ID: "adapter-a"},
			{ID: "adapter-b"},
		},
	}
	hooks, err := ResolveRuntimeHooks(profile, RuntimeHookContext{}, factories)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(hooks.InvocationHooks) != 2 {
		t.Fatalf("expected 2 hooks, got %d", len(hooks.InvocationHooks))
	}
	if hooks.InvocationHooks[0].ID() != "adapter-a" || hooks.InvocationHooks[1].ID() != "adapter-b" {
		t.Fatal("hooks not in expected order")
	}
}

// --- ExecuteAdapterOwned tests ---

func TestExecuteAdapterOwned_Success(t *testing.T) {
	retVal := json.RawMessage(`42`)
	hook := &stubHook{
		id: "test",
		outcome: &AdapterInvocationOutcome{
			Status:      OutcomeStatusCompleted,
			ReturnValue: retVal,
		},
	}
	result, err := ExecuteAdapterOwned(hook, InvocationContext{
		File:         "/tmp/test.go",
		FunctionName: "Foo",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if string(result.ReturnValue) != "42" {
		t.Fatalf("expected return value 42, got %s", result.ReturnValue)
	}
	if result.ThrownError != nil {
		t.Fatal("expected no thrown error")
	}
	// Adapter-owned should have empty instrumentation fields
	if len(result.BranchPath) != 0 {
		t.Fatalf("expected empty branch path, got %d", len(result.BranchPath))
	}
	if len(result.LinesExecuted) != 0 {
		t.Fatalf("expected empty lines executed, got %d", len(result.LinesExecuted))
	}
	if len(result.ExternalCalls) != 0 {
		t.Fatalf("expected empty external calls, got %d", len(result.ExternalCalls))
	}
}

func TestExecuteAdapterOwned_WithError(t *testing.T) {
	hook := &stubHook{
		id: "test",
		outcome: &AdapterInvocationOutcome{
			Status: OutcomeStatusRuntimeFailed,
			ThrownError: &instrument.ErrorInfo{
				ErrorType: "panic",
				Message:   "something went wrong",
			},
		},
	}
	result, err := ExecuteAdapterOwned(hook, InvocationContext{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.ThrownError == nil {
		t.Fatal("expected thrown error")
	}
	if result.ThrownError.ErrorType != "panic" {
		t.Fatalf("expected error type panic, got %s", result.ThrownError.ErrorType)
	}
}

func TestExecuteAdapterOwned_WithSideEffects(t *testing.T) {
	hook := &stubHook{
		id: "test",
		outcome: &AdapterInvocationOutcome{
			Status:      OutcomeStatusCompletedWithFindings,
			ReturnValue: json.RawMessage(`null`),
			SideEffects: []instrument.SideEffect{
				{Kind: "console_output", Level: "log", Message: "hello"},
			},
		},
	}
	result, err := ExecuteAdapterOwned(hook, InvocationContext{})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result.SideEffects) != 1 {
		t.Fatalf("expected 1 side effect, got %d", len(result.SideEffects))
	}
	if result.SideEffects[0].Kind != "console_output" {
		t.Fatalf("expected console_output, got %s", result.SideEffects[0].Kind)
	}
}

func TestExecuteAdapterOwned_HookError(t *testing.T) {
	hook := &stubHook{
		id:  "failing",
		err: fmt.Errorf("hook crashed"),
	}
	_, err := ExecuteAdapterOwned(hook, InvocationContext{})
	if err == nil {
		t.Fatal("expected error from hook")
	}
	if !strings.Contains(err.Error(), "failing") {
		t.Fatalf("expected error to mention adapter id, got: %s", err.Error())
	}
}

// --- Handler integration: cached analyses + adapter dispatch ---

// testFilePath returns the absolute path to a testdata fixture file.
func testFilePath(t *testing.T, name string) string {
	t.Helper()
	abs, err := filepath.Abs(filepath.Join("testdata", name))
	if err != nil {
		t.Fatalf("filepath.Abs: %v", err)
	}
	return abs
}

func TestHandleExecute_UnsupportedAdapter(t *testing.T) {
	file := testFilePath(t, "types.go")
	// Manually inject a cached analysis with an adapter invocation model,
	// but don't register a matching factory → should get "unsupported" error.
	input := strings.NewReader(
		reqJSON(1, "execute", fmt.Sprintf(`"file":%q`, file), `"function":"Foo"`, `"inputs":[]`) + "\n",
	)
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)

	// Inject cached analysis with adapter invocation model
	handler.cachedAnalyses[file+"\x00Foo"] = &FunctionAnalysis{
		Name: "Foo",
		InvocationModel: &InvocationModel{
			Kind:      "adapter",
			AdapterID: "go/http-handler",
		},
	}
	// Register a factory for a *different* adapter to trigger resolution
	handler.RegisterHookFactory(&stubFactory{id: "go/other", hooks: nil})

	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	var resp Response
	if err := json.Unmarshal([]byte(lines[0]), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if resp.Status != "error" {
		t.Fatalf("expected error status, got %s", resp.Status)
	}
	if !strings.Contains(resp.Message, "go/http-handler") {
		t.Fatalf("expected error to mention adapter id, got: %s", resp.Message)
	}
}

func TestHandleExecute_AdapterDispatch(t *testing.T) {
	file := testFilePath(t, "types.go")
	retVal := json.RawMessage(`{"status":200}`)
	hook := &stubHook{
		id: "go/test-adapter",
		outcome: &AdapterInvocationOutcome{
			Status:      OutcomeStatusCompleted,
			ReturnValue: retVal,
		},
	}
	factory := &stubFactory{id: "go/test-adapter", hooks: []InvocationHook{hook}}

	// Build an execute request with execution_profile
	execProfile := `"execution_profile":{"adapters":[{"id":"go/test-adapter"}]}`
	input := strings.NewReader(
		reqJSON(1, "execute", fmt.Sprintf(`"file":%q`, file), `"function":"Foo"`, `"inputs":[]`, execProfile) + "\n",
	)
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	handler.RegisterHookFactory(factory)

	// Inject cached analysis with adapter invocation model
	handler.cachedAnalyses[file+"\x00Foo"] = &FunctionAnalysis{
		Name: "Foo",
		InvocationModel: &InvocationModel{
			Kind:      "adapter",
			AdapterID: "go/test-adapter",
		},
	}

	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	var resp Response
	if err := json.Unmarshal([]byte(lines[0]), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if resp.Status != "execute" {
		t.Fatalf("expected execute status, got %s (message: %s)", resp.Status, resp.Message)
	}
	if string(resp.ReturnValue) != `{"status":200}` {
		t.Fatalf("expected return value {\"status\":200}, got %s", resp.ReturnValue)
	}
	// Adapter-owned: empty branch path (omitempty serializes empty as absent → nil on deserialize)
	if len(resp.BranchPath) != 0 {
		t.Fatalf("expected empty branch path, got %d", len(resp.BranchPath))
	}
}

func TestShouldForceDirectReceiverExecution(t *testing.T) {
	analysis := &FunctionAnalysis{
		Name: "(*Server).Handle",
		InvocationModel: &InvocationModel{
			Kind:      "adapter",
			AdapterID: HTTPHandlerAdapterID,
		},
	}

	if !shouldForceDirectReceiverExecution("(*Server).Handle", analysis) {
		t.Fatal("expected receiver-qualified adapter model to force direct execution")
	}
	if shouldForceDirectReceiverExecution("Handle", analysis) {
		t.Fatal("package-level adapter model should remain adapter-owned")
	}
	analysis.InvocationModel.Kind = "direct"
	if shouldForceDirectReceiverExecution("(*Server).Handle", analysis) {
		t.Fatal("direct invocation model should not be forced again")
	}
}

func TestCachedAnalyses_ClearedOnShutdown(t *testing.T) {
	input := strings.NewReader(
		reqJSON(1, "shutdown") + "\n",
	)
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	handler.cachedAnalyses["test\x00Foo"] = &FunctionAnalysis{Name: "Foo"}

	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	if len(handler.cachedAnalyses) != 0 {
		t.Fatalf("expected cached analyses to be cleared, got %d entries", len(handler.cachedAnalyses))
	}
}
