package protocol

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

func TestHTTPHandlerFactory_ID(t *testing.T) {
	f := createHTTPHandlerFactory()
	if f.ID() != HTTPHandlerAdapterID {
		t.Fatalf("expected %s, got %s", HTTPHandlerAdapterID, f.ID())
	}
}

func TestHTTPHandlerFactory_CreatesHook(t *testing.T) {
	f := createHTTPHandlerFactory()
	hooks := f.CreateRuntimeHooks(ExecutionAdapter{ID: HTTPHandlerAdapterID}, RuntimeHookContext{})
	if hooks == nil {
		t.Fatal("expected non-nil RuntimeHooks")
	}
	if len(hooks.InvocationHooks) != 1 {
		t.Fatalf("expected 1 hook, got %d", len(hooks.InvocationHooks))
	}
	if hooks.InvocationHooks[0].ID() != HTTPHandlerAdapterID {
		t.Fatalf("expected hook ID %s, got %s", HTTPHandlerAdapterID, hooks.InvocationHooks[0].ID())
	}
}

func TestHTTPHandlerSyntheticParams(t *testing.T) {
	params := httpHandlerSyntheticParams()
	if len(params) != 4 {
		t.Fatalf("expected 4 params, got %d", len(params))
	}
	expected := []string{"method", "path", "headers", "body"}
	for i, name := range expected {
		if params[i].Name != name {
			t.Errorf("param %d: expected %s, got %s", i, name, params[i].Name)
		}
	}
	wantKinds := map[string]string{
		"method":  "str",
		"path":    "str",
		"headers": "object",
		"body":    "str",
	}
	for _, p := range params {
		if got, want := p.Type.Kind, wantKinds[p.Name]; got != want {
			t.Errorf("param %q: kind = %q, want %q (Rust core rejects any other variant)", p.Name, got, want)
		}
	}
}

func TestAnalyze_HTTPHandler_SetsInvocationModel(t *testing.T) {
	file := testFilePath(t, "httphandler.go")
	functions, err := AnalyzeFile(file, "HelloHandler")
	if err != nil {
		t.Fatalf("analyze: %v", err)
	}
	if len(functions) != 1 {
		t.Fatalf("expected 1 function, got %d", len(functions))
	}
	fn := functions[0]
	if fn.InvocationModel == nil {
		t.Fatal("expected InvocationModel, got nil")
	}
	if fn.InvocationModel.Kind != "adapter" {
		t.Fatalf("expected kind adapter, got %s", fn.InvocationModel.Kind)
	}
	if fn.InvocationModel.AdapterID != HTTPHandlerAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", HTTPHandlerAdapterID, fn.InvocationModel.AdapterID)
	}
	if len(fn.InvocationModel.SyntheticParams) != 4 {
		t.Fatalf("expected 4 synthetic params, got %d", len(fn.InvocationModel.SyntheticParams))
	}
}

func TestAnalyze_HTTPHandlerPackageMainUnexported_NoInvocationModel(t *testing.T) {
	file := testFilePath(t, "http_main_project/handler.go")
	functions, err := AnalyzeFile(file, "unexportedMainHandler")
	if err != nil {
		t.Fatalf("analyze: %v", err)
	}
	if len(functions) != 1 {
		t.Fatalf("expected 1 function, got %d", len(functions))
	}
	if functions[0].InvocationModel != nil {
		t.Fatalf("expected nil InvocationModel for unexported package-main handler, got %+v", functions[0].InvocationModel)
	}
}

func TestAnalyze_NonHandler_NoInvocationModel(t *testing.T) {
	file := testFilePath(t, "httphandler.go")
	functions, err := AnalyzeFile(file, "NotAHandler")
	if err != nil {
		t.Fatalf("analyze: %v", err)
	}
	if len(functions) != 1 {
		t.Fatalf("expected 1 function, got %d", len(functions))
	}
	if functions[0].InvocationModel != nil {
		t.Fatalf("expected nil InvocationModel for non-handler, got %+v", functions[0].InvocationModel)
	}
}

func TestAnalyze_HTTPHandler_ProtocolResponse(t *testing.T) {
	file := testFilePath(t, "httphandler.go")
	// Use the conversationWithFactories helper to verify end-to-end analyze
	reqLine := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"HelloHandler"`)
	responses := conversationWithFactories(t, nil, reqLine)
	if len(responses) < 1 {
		t.Fatal("expected at least 1 response")
	}
	resp := responses[0]
	if resp.Status != "analyze" {
		t.Fatalf("expected analyze status, got %s (message: %s)", resp.Status, resp.Message)
	}
	if len(resp.Functions) != 1 {
		t.Fatalf("expected 1 function, got %d", len(resp.Functions))
	}
	fn := resp.Functions[0]
	if fn.InvocationModel == nil {
		t.Fatal("expected InvocationModel in protocol response, got nil")
	}
	if fn.InvocationModel.AdapterID != HTTPHandlerAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", HTTPHandlerAdapterID, fn.InvocationModel.AdapterID)
	}
}

func TestExecuteAdapterViaLauncher_HTTPHandler(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "httphandler.go")
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/hello")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")

	result, err := executeAdapterViaLauncher(HTTPHandlerAdapterID, InvocationContext{
		File:         file,
		FunctionName: "HelloHandler",
		Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON},
		Capture:      true,
	})
	if err != nil {
		t.Fatalf("execute adapter via launcher: %v", err)
	}

	var httpResp struct {
		Status  int                 `json:"status"`
		Headers map[string][]string `json:"headers"`
		Body    string              `json:"body"`
	}
	if err := json.Unmarshal(result.ReturnValue, &httpResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, result.ReturnValue)
	}
	if httpResp.Status != 200 {
		t.Fatalf("expected status 200, got %d", httpResp.Status)
	}
	if httpResp.Body != "hello" {
		t.Fatalf("expected body 'hello', got %q", httpResp.Body)
	}
}

func TestExecuteAdapterViaLauncher_HTTPHandlerPackageMain(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "http_main_project/handler.go")
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/hello")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")

	result, err := executeAdapterViaLauncher(HTTPHandlerAdapterID, InvocationContext{
		File:         file,
		FunctionName: "MainHelloHandler",
		Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON},
		Capture:      true,
	})
	if err != nil {
		t.Fatalf("execute package-main adapter via launcher: %v", err)
	}

	var httpResp struct {
		Status  int                 `json:"status"`
		Headers map[string][]string `json:"headers"`
		Body    string              `json:"body"`
	}
	if err := json.Unmarshal(result.ReturnValue, &httpResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, result.ReturnValue)
	}
	if httpResp.Status != http.StatusAccepted {
		t.Fatalf("expected status %d, got %d", http.StatusAccepted, httpResp.Status)
	}
	if httpResp.Body != "main hello" {
		t.Fatalf("expected body 'main hello', got %q", httpResp.Body)
	}
}

func TestExecuteAdapterViaLauncher_HTTPHandlerPackageMainUnexportedUnsupported(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "http_main_project/handler.go")
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/hello")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")

	_, err := executeAdapterViaLauncher(HTTPHandlerAdapterID, InvocationContext{
		File:         file,
		FunctionName: "unexportedMainHandler",
		Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON},
		Capture:      true,
	})
	if err == nil {
		t.Fatal("expected unexported package-main handler to be unsupported")
	}
	if !strings.Contains(err.Error(), "unexported package main HTTP handler") {
		t.Fatalf("expected unexported package-main unsupported error, got %v", err)
	}
}

// TestExecuteAdapterViaLauncher_HTTPHandlerReportsCoverage asserts that the
// adapter launcher path now threads real instrumentation coverage (str-1qd5i):
// a net/http handler whose executed lines are gated on the request method
// reports non-empty lines_executed, and GET vs POST drive distinct line sets
// (the method-driven branch), matching the direct wrapper path's behavior.
func TestExecuteAdapterViaLauncher_HTTPHandlerReportsCoverage(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "http_branch_project/handler.go")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")
	pathJSON, _ := json.Marshal("/items")

	// Drive through the full adapter substrate (hook.Invoke -> outcome ->
	// ExecuteAdapterOwned) that handleExecute uses, so the test also guards the
	// outcome propagation of branch_path/lines_executed, not just the launcher.
	invoke := func(method string) *instrument.ExecuteResult {
		methodJSON, _ := json.Marshal(method)
		result, err := ExecuteAdapterOwned(&httpHandlerHook{}, InvocationContext{
			File:         file,
			FunctionName: "MethodBranchHandler",
			Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON},
			Capture:      true,
		})
		if err != nil {
			t.Fatalf("execute adapter owned (%s): %v", method, err)
		}
		if result.ThrownError != nil {
			t.Fatalf("unexpected thrown error (%s): %+v", method, result.ThrownError)
		}
		return result
	}

	get := invoke("GET")
	post := invoke("POST")

	if len(get.LinesExecuted) == 0 {
		t.Fatal("GET: adapter-owned lines_executed must be non-empty (str-1qd5i)")
	}
	if len(post.LinesExecuted) == 0 {
		t.Fatal("POST: adapter-owned lines_executed must be non-empty (str-1qd5i)")
	}

	getLines := intSet(get.LinesExecuted)
	postLines := intSet(post.LinesExecuted)
	if setsEqual(getLines, postLines) {
		t.Fatalf("method-driven branch should drive distinct line coverage; GET=%v POST=%v", getLines, postLines)
	}
}

func intSet(lines []int) map[int]struct{} {
	set := make(map[int]struct{}, len(lines))
	for _, line := range lines {
		set[line] = struct{}{}
	}
	return set
}

func setsEqual(a, b map[int]struct{}) bool {
	if len(a) != len(b) {
		return false
	}
	for k := range a {
		if _, ok := b[k]; !ok {
			return false
		}
	}
	return true
}

func TestGenerateHTTPAdapterLauncherRejectsReceiverMethod(t *testing.T) {
	_, err := generateAdapterLauncherMain(HTTPHandlerAdapterID, "example.com/app", "(*Server).Handle")
	if err == nil {
		t.Fatal("expected receiver method adapter launcher generation to fail")
	}
	if !strings.Contains(err.Error(), "receiver method") {
		t.Fatalf("expected receiver method error, got %v", err)
	}
}

func TestHTTPHandler_Execute_Integration(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "httphandler.go")

	// Step 1: Analyze to populate cached analysis
	analyzeReq := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"HelloHandler"`)

	// Step 2: Execute with adapter
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/hello")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")
	inputsJSON := fmt.Sprintf("[%s,%s,%s,%s]", methodJSON, pathJSON, headersJSON, bodyJSON)

	execProfile := fmt.Sprintf(`"execution_profile":{"adapters":[{"id":%q}]}`, HTTPHandlerAdapterID)
	executeReq := reqJSON(2, "execute",
		fmt.Sprintf(`"file":%q`, file),
		`"function":"HelloHandler"`,
		fmt.Sprintf(`"inputs":%s`, inputsJSON),
		execProfile,
	)

	responses := conversationWithFactories(t, nil, analyzeReq, executeReq)
	if len(responses) < 2 {
		t.Fatalf("expected at least 2 responses, got %d", len(responses))
	}

	// Verify analyze response
	analyzeResp := responses[0]
	if analyzeResp.Status != "analyze" {
		t.Fatalf("expected analyze status, got %s (message: %s)", analyzeResp.Status, analyzeResp.Message)
	}

	// Verify execute response
	execResp := responses[1]
	if execResp.Status != "execute" {
		t.Fatalf("expected execute status, got %s (message: %s)", execResp.Status, execResp.Message)
	}

	// Parse the HTTP response from return_value
	var httpResp struct {
		Status  int                 `json:"status"`
		Headers map[string][]string `json:"headers"`
		Body    string              `json:"body"`
	}
	if err := json.Unmarshal(execResp.ReturnValue, &httpResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, execResp.ReturnValue)
	}

	if httpResp.Status != 200 {
		t.Fatalf("expected status 200, got %d", httpResp.Status)
	}
	if httpResp.Body != "hello" {
		t.Fatalf("expected body 'hello', got %q", httpResp.Body)
	}
	ct := httpResp.Headers["Content-Type"]
	if len(ct) == 0 || !strings.Contains(ct[0], "text/plain") {
		t.Fatalf("expected Content-Type text/plain, got %v", ct)
	}

	// Adapter-owned invocations now thread real instrumentation from the
	// launcher (str-1qd5i): HelloHandler branches on the request method, so the
	// GET path records a branch decision and executes source lines.
	if len(execResp.BranchPath) == 0 {
		t.Fatal("expected non-empty branch path for adapter-owned execute (str-1qd5i)")
	}
	if len(execResp.LinesExecuted) == 0 {
		t.Fatal("expected non-empty lines_executed for adapter-owned execute (str-1qd5i)")
	}
}

func TestHTTPHandler_Execute_POST(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "httphandler.go")

	analyzeReq := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"HelloHandler"`)

	methodJSON, _ := json.Marshal("POST")
	pathJSON, _ := json.Marshal("/hello")
	headersJSON, _ := json.Marshal(map[string]string{"Content-Type": "application/json"})
	bodyJSON, _ := json.Marshal(`{"key":"value"}`)
	inputsJSON := fmt.Sprintf("[%s,%s,%s,%s]", methodJSON, pathJSON, headersJSON, bodyJSON)

	execProfile := fmt.Sprintf(`"execution_profile":{"adapters":[{"id":%q}]}`, HTTPHandlerAdapterID)
	executeReq := reqJSON(2, "execute",
		fmt.Sprintf(`"file":%q`, file),
		`"function":"HelloHandler"`,
		fmt.Sprintf(`"inputs":%s`, inputsJSON),
		execProfile,
	)

	responses := conversationWithFactories(t, nil, analyzeReq, executeReq)
	if len(responses) < 2 {
		t.Fatalf("expected at least 2 responses, got %d", len(responses))
	}

	execResp := responses[1]
	if execResp.Status != "execute" {
		t.Fatalf("expected execute status, got %s (message: %s)", execResp.Status, execResp.Message)
	}

	var httpResp struct {
		Status int    `json:"status"`
		Body   string `json:"body"`
	}
	if err := json.Unmarshal(execResp.ReturnValue, &httpResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, execResp.ReturnValue)
	}

	if httpResp.Status != 201 {
		t.Fatalf("expected status 201 for POST, got %d", httpResp.Status)
	}
	if httpResp.Body != "created" {
		t.Fatalf("expected body 'created', got %q", httpResp.Body)
	}
}
