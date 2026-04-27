package protocol

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"
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

	// Adapter-owned: empty branch path
	if len(execResp.BranchPath) != 0 {
		t.Fatalf("expected empty branch path for adapter-owned, got %d", len(execResp.BranchPath))
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
