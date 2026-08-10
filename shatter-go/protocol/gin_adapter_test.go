package protocol

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

func TestGinHandlerFactory_ID(t *testing.T) {
	f := createGinHandlerFactory()
	if f.ID() != GinAdapterID {
		t.Fatalf("expected %s, got %s", GinAdapterID, f.ID())
	}
}

func TestGinHandlerFactory_CreatesHook(t *testing.T) {
	f := createGinHandlerFactory()
	hooks := f.CreateRuntimeHooks(ExecutionAdapter{ID: GinAdapterID}, RuntimeHookContext{})
	if hooks == nil {
		t.Fatal("expected non-nil RuntimeHooks")
	}
	if len(hooks.InvocationHooks) != 1 {
		t.Fatalf("expected 1 hook, got %d", len(hooks.InvocationHooks))
	}
	if hooks.InvocationHooks[0].ID() != GinAdapterID {
		t.Fatalf("expected hook ID %s, got %s", GinAdapterID, hooks.InvocationHooks[0].ID())
	}
}

func TestGinHandlerSyntheticParams(t *testing.T) {
	params := ginHandlerSyntheticParams()
	if len(params) != 5 {
		t.Fatalf("expected 5 params, got %d", len(params))
	}
	expected := []string{"method", "path", "headers", "body", "route_params"}
	for i, name := range expected {
		if params[i].Name != name {
			t.Errorf("param %d: expected %s, got %s", i, name, params[i].Name)
		}
	}
	wantKinds := map[string]string{
		"method":       "str",
		"path":         "str",
		"headers":      "object",
		"body":         "str",
		"route_params": "object",
	}
	for _, p := range params {
		want, ok := wantKinds[p.Name]
		if !ok {
			continue
		}
		if p.Type.Kind != want {
			t.Errorf("param %q: kind = %q, want %q (Rust core rejects any other variant)", p.Name, p.Type.Kind, want)
		}
	}
}

func TestAnalyze_GinHandler_SetsInvocationModel(t *testing.T) {
	file := testFilePath(t, "gin_project/handler.go")
	functions, err := AnalyzeFile(file, "ListUsers")
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
	if fn.InvocationModel.AdapterID != GinAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", GinAdapterID, fn.InvocationModel.AdapterID)
	}
	if len(fn.InvocationModel.SyntheticParams) != 5 {
		t.Fatalf("expected 5 synthetic params, got %d", len(fn.InvocationModel.SyntheticParams))
	}
}

func TestAnalyze_NonGinHandler_NoInvocationModel(t *testing.T) {
	file := testFilePath(t, "gin_project/handler.go")
	functions, err := AnalyzeFile(file, "NotAGinHandler")
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

func TestAnalyze_GinHandler_ProtocolResponse(t *testing.T) {
	file := testFilePath(t, "gin_project/handler.go")
	reqLine := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"ListUsers"`)
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
	if fn.InvocationModel.AdapterID != GinAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", GinAdapterID, fn.InvocationModel.AdapterID)
	}
}

func TestExecuteAdapterViaLauncher_GinHandler(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "gin_project/handler.go")
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/users")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")
	routeParamsJSON, _ := json.Marshal(map[string]string{})

	result, err := executeAdapterViaLauncher(GinAdapterID, InvocationContext{
		File:         file,
		FunctionName: "ListUsers",
		Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON, routeParamsJSON},
		Capture:      true,
	})
	if err != nil {
		t.Fatalf("execute adapter via launcher: %v", err)
	}

	var ginResp struct {
		Status  int                 `json:"status"`
		Headers map[string][]string `json:"headers"`
		Body    string              `json:"body"`
	}
	if err := json.Unmarshal(result.ReturnValue, &ginResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, result.ReturnValue)
	}
	if ginResp.Status != 200 {
		t.Fatalf("expected status 200, got %d", ginResp.Status)
	}
	if !strings.Contains(ginResp.Body, "alice") || !strings.Contains(ginResp.Body, "bob") {
		t.Fatalf("expected body containing alice and bob, got %q", ginResp.Body)
	}
}

// TestExecuteAdapterViaLauncher_GinHandlerReportsCoverage asserts the gin
// adapter launcher path threads real instrumentation coverage (str-1qd5i):
// AbortExample branches on the Authorization header, so the authorized and
// unauthorized requests report non-empty, distinct lines_executed.
func TestExecuteAdapterViaLauncher_GinHandlerReportsCoverage(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "gin_project/handler.go")
	pathJSON, _ := json.Marshal("/status")
	bodyJSON, _ := json.Marshal("")
	routeParamsJSON, _ := json.Marshal(map[string]string{})

	// Drive through the full adapter substrate (hook.Invoke -> outcome ->
	// ExecuteAdapterOwned) so the test also guards outcome propagation of
	// branch_path/lines_executed, matching the handleExecute path.
	invoke := func(headers map[string]string) *instrument.ExecuteResult {
		methodJSON, _ := json.Marshal("GET")
		headersJSON, _ := json.Marshal(headers)
		result, err := ExecuteAdapterOwned(&ginHandlerHook{}, InvocationContext{
			File:         file,
			FunctionName: "AbortExample",
			Inputs:       []json.RawMessage{methodJSON, pathJSON, headersJSON, bodyJSON, routeParamsJSON},
			Capture:      true,
		})
		if err != nil {
			t.Fatalf("execute gin adapter owned: %v", err)
		}
		if result.ThrownError != nil {
			t.Fatalf("unexpected thrown error: %+v", result.ThrownError)
		}
		return result
	}

	unauthorized := invoke(map[string]string{})
	authorized := invoke(map[string]string{"Authorization": "Bearer token"})

	if len(unauthorized.LinesExecuted) == 0 {
		t.Fatal("unauthorized: gin adapter-owned lines_executed must be non-empty (str-1qd5i)")
	}
	if len(authorized.LinesExecuted) == 0 {
		t.Fatal("authorized: gin adapter-owned lines_executed must be non-empty (str-1qd5i)")
	}

	if setsEqual(intSet(unauthorized.LinesExecuted), intSet(authorized.LinesExecuted)) {
		t.Fatalf(
			"header-driven branch should drive distinct line coverage; unauthorized=%v authorized=%v",
			intSet(unauthorized.LinesExecuted), intSet(authorized.LinesExecuted),
		)
	}
}

func TestGinHandler_Execute_Integration(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "gin_project/handler.go")

	// Step 1: Analyze to populate cached analysis
	analyzeReq := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"ListUsers"`)

	// Step 2: Execute with adapter
	methodJSON, _ := json.Marshal("GET")
	pathJSON, _ := json.Marshal("/users")
	headersJSON, _ := json.Marshal(map[string]string{})
	bodyJSON, _ := json.Marshal("")
	routeParamsJSON, _ := json.Marshal(map[string]string{})
	inputsJSON := fmt.Sprintf("[%s,%s,%s,%s,%s]", methodJSON, pathJSON, headersJSON, bodyJSON, routeParamsJSON)

	execProfile := fmt.Sprintf(`"execution_profile":{"adapters":[{"id":%q}]}`, GinAdapterID)
	executeReq := reqJSON(2, "execute",
		fmt.Sprintf(`"file":%q`, file),
		`"function":"ListUsers"`,
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

	// Parse the response from return_value
	var ginResp struct {
		Status  int                 `json:"status"`
		Headers map[string][]string `json:"headers"`
		Body    string              `json:"body"`
	}
	if err := json.Unmarshal(execResp.ReturnValue, &ginResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, execResp.ReturnValue)
	}

	if ginResp.Status != 200 {
		t.Fatalf("expected status 200, got %d", ginResp.Status)
	}
	// Gin JSON output: ["alice","bob"]
	if !strings.Contains(ginResp.Body, "alice") || !strings.Contains(ginResp.Body, "bob") {
		t.Fatalf("expected body containing alice and bob, got %q", ginResp.Body)
	}
	ct := ginResp.Headers["Content-Type"]
	if len(ct) == 0 || !strings.Contains(ct[0], "application/json") {
		t.Fatalf("expected Content-Type application/json, got %v", ct)
	}

	// Adapter-owned invocations now thread real instrumentation from the
	// launcher (str-1qd5i). ListUsers is branchless, so branch_path stays empty,
	// but its body executes source lines — lines_executed proves the
	// instrumented target ran and coverage reached the response.
	if len(execResp.BranchPath) != 0 {
		t.Fatalf("ListUsers is branchless; expected empty branch path, got %d", len(execResp.BranchPath))
	}
	if len(execResp.LinesExecuted) == 0 {
		t.Fatal("expected non-empty lines_executed for adapter-owned execute (str-1qd5i)")
	}
}

func TestGinHandler_Execute_WithRouteParams(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}

	file := testFilePath(t, "gin_project/handler.go")

	analyzeReq := reqJSON(1, "analyze", fmt.Sprintf(`"file":%q`, file), `"function":"CreateUser"`)

	methodJSON, _ := json.Marshal("POST")
	pathJSON, _ := json.Marshal("/users/alice")
	headersJSON, _ := json.Marshal(map[string]string{"Content-Type": "application/json"})
	bodyJSON, _ := json.Marshal("")
	routeParamsJSON, _ := json.Marshal(map[string]string{"name": "alice"})
	inputsJSON := fmt.Sprintf("[%s,%s,%s,%s,%s]", methodJSON, pathJSON, headersJSON, bodyJSON, routeParamsJSON)

	execProfile := fmt.Sprintf(`"execution_profile":{"adapters":[{"id":%q}]}`, GinAdapterID)
	executeReq := reqJSON(2, "execute",
		fmt.Sprintf(`"file":%q`, file),
		`"function":"CreateUser"`,
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

	var ginResp struct {
		Status int    `json:"status"`
		Body   string `json:"body"`
	}
	if err := json.Unmarshal(execResp.ReturnValue, &ginResp); err != nil {
		t.Fatalf("unmarshal return value: %v (raw: %s)", err, execResp.ReturnValue)
	}

	if ginResp.Status != 201 {
		t.Fatalf("expected status 201 for CreateUser, got %d", ginResp.Status)
	}
	if !strings.Contains(ginResp.Body, "alice") {
		t.Fatalf("expected body containing 'alice', got %q", ginResp.Body)
	}
}

func TestSyntheticParamsForAdapter(t *testing.T) {
	tests := []struct {
		adapterID string
		wantLen   int
		wantNil   bool
	}{
		{HTTPHandlerAdapterID, 4, false},
		{GinAdapterID, 5, false},
		{"unknown/adapter", 0, true},
	}
	for _, tt := range tests {
		t.Run(tt.adapterID, func(t *testing.T) {
			params := syntheticParamsForAdapter(tt.adapterID)
			if tt.wantNil {
				if params != nil {
					t.Fatalf("expected nil for %s, got %v", tt.adapterID, params)
				}
				return
			}
			if params == nil {
				t.Fatalf("expected non-nil for %s", tt.adapterID)
			}
			if len(params) != tt.wantLen {
				t.Fatalf("expected %d params for %s, got %d", tt.wantLen, tt.adapterID, len(params))
			}
		})
	}
}
