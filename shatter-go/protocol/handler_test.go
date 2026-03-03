package protocol

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// sendRecv sends a single request and reads the response.
func sendRecv(t *testing.T, reqJSON string) Response {
	t.Helper()
	input := strings.NewReader(reqJSON + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	if len(lines) == 0 {
		t.Fatal("no response received")
	}
	var resp Response
	if err := json.Unmarshal([]byte(lines[0]), &resp); err != nil {
		t.Fatalf("unmarshal response: %v (raw: %s)", err, lines[0])
	}
	return resp
}

// conversation sends multiple requests (one per line) and returns all responses.
func conversation(t *testing.T, requests ...string) []Response {
	t.Helper()
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
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

func TestHandshakeReturnsGoLanguage(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}`)
	if resp.Status != "handshake" {
		t.Errorf("status = %q, want handshake", resp.Status)
	}
	if resp.Language != "go" {
		t.Errorf("language = %q, want go", resp.Language)
	}
	if resp.FrontendVersion != "0.1.0" {
		t.Errorf("frontend_version = %q, want 0.1.0", resp.FrontendVersion)
	}
}

func TestHandshakeReturnsAllCapabilities(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}`)
	caps := map[string]bool{}
	for _, c := range resp.Capabilities {
		caps[c] = true
	}
	for _, want := range []string{"analyze", "execute", "instrument"} {
		if !caps[want] {
			t.Errorf("missing capability %q in %v", want, resp.Capabilities)
		}
	}
}

func TestHandshakeEchoesRequestID(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":42,"command":"handshake","capabilities":[]}`)
	if resp.ID != 42 {
		t.Errorf("id = %d, want 42", resp.ID)
	}
}

func TestHandshakeIncludesProtocolVersion(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":[]}`)
	if resp.ProtocolVersion != "0.1.0" {
		t.Errorf("protocol_version = %q, want 0.1.0", resp.ProtocolVersion)
	}
}

func TestShutdownReturnsAckAndStops(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":5,"command":"shutdown"}`)
	if resp.Status != "shutdown_ack" {
		t.Errorf("status = %q, want shutdown_ack", resp.Status)
	}
	if resp.ID != 5 {
		t.Errorf("id = %d, want 5", resp.ID)
	}
}

func TestVersionMismatchReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"99.0.0","id":1,"command":"handshake","capabilities":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "version_mismatch" {
		t.Errorf("code = %q, want version_mismatch", resp.Code)
	}
}

func TestUnknownCommandReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":1,"command":"foobar"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestAnalyzeWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"nonexistent.go"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "file_not_found" {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestAnalyzeWithExistingFileReturnsEmptyFunctions(t *testing.T) {
	// Create a temp file to analyze
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":3,"command":"analyze","file":"` + tmp + `"}`
	resp := sendRecv(t, req)
	if resp.Status != "analyze" {
		t.Errorf("status = %q, want analyze", resp.Status)
	}
}

func TestAnalyzeEmptyFileJSONIncludesFunctionsField(t *testing.T) {
	// Regression test for str-xkb: doc-only files (no function definitions)
	// must still emit "functions":[] in JSON, not omit the field entirely.
	// The Rust core requires the field to be present for deserialization.
	tmp := filepath.Join(t.TempDir(), "doc.go")
	if err := os.WriteFile(tmp, []byte("// Package foo provides utilities.\npackage foo\n"), 0644); err != nil {
		t.Fatal(err)
	}

	// Send the analyze request and capture raw JSON output
	input := strings.NewReader(`{"protocol_version":"0.1.0","id":3,"command":"analyze","file":"` + tmp + `"}` + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	rawJSON := strings.TrimSpace(output.String())

	// The raw JSON must contain "functions":[] -- not omit the field
	if !strings.Contains(rawJSON, `"functions"`) {
		t.Fatalf("JSON response omits functions field entirely: %s", rawJSON)
	}
	if !strings.Contains(rawJSON, `"functions":[]`) {
		t.Fatalf("JSON response does not contain functions:[], got: %s", rawJSON)
	}

	// Also verify it deserializes correctly
	var resp Response
	if err := json.Unmarshal([]byte(rawJSON), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if resp.Status != "analyze" {
		t.Errorf("status = %q, want analyze", resp.Status)
	}
	if resp.Functions == nil {
		t.Error("functions should be non-nil empty slice, got nil")
	}
	if len(resp.Functions) != 0 {
		t.Errorf("functions len = %d, want 0", len(resp.Functions))
	}
}

func TestAnalyzeReturnsFunctionAnalysis(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "add.go")
	src := "package main\n\nfunc Add(a, b int) int { return a + b }\n"
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":3,"command":"analyze","file":"` + tmp + `"}`
	resp := sendRecv(t, req)
	if resp.Status != "analyze" {
		t.Fatalf("status = %q, want analyze", resp.Status)
	}
	if len(resp.Functions) != 1 {
		t.Fatalf("functions len = %d, want 1", len(resp.Functions))
	}
	fn := resp.Functions[0]
	if fn.Name != "Add" {
		t.Errorf("name = %q, want Add", fn.Name)
	}
	if len(fn.Params) != 2 {
		t.Errorf("params len = %d, want 2", len(fn.Params))
	}
}

func TestAnalyzeWithFunctionFilterReturnsOneFunction(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "multi.go")
	src := "package main\n\nfunc Foo() {}\nfunc Bar() {}\n"
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":3,"command":"analyze","file":"` + tmp + `","function":"Bar"}`
	resp := sendRecv(t, req)
	if resp.Status != "analyze" {
		t.Fatalf("status = %q, want analyze", resp.Status)
	}
	if len(resp.Functions) != 1 {
		t.Fatalf("functions len = %d, want 1", len(resp.Functions))
	}
	if resp.Functions[0].Name != "Bar" {
		t.Errorf("name = %q, want Bar", resp.Functions[0].Name)
	}
}

func TestAnalyzeWithMissingFunctionReturnsError(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "empty.go")
	if err := os.WriteFile(tmp, []byte("package main\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":3,"command":"analyze","file":"` + tmp + `","function":"Missing"}`
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Fatalf("status = %q, want error", resp.Status)
	}
	if resp.Code != "function_not_found" {
		t.Errorf("code = %q, want function_not_found", resp.Code)
	}
}

func TestAnalyzeWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":2,"command":"analyze"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestInstrumentWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"nonexistent.go"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "file_not_found" {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestInstrumentWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":3,"command":"instrument"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestInstrumentWithValidFileReturnsSuccess(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc F(x int) int { if x > 0 { return 1 } ; return 0 }\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"` + tmp + `"}`
	resp := sendRecv(t, req)
	if resp.Status != "instrument" {
		t.Errorf("status = %q, want instrument (message: %s)", resp.Status, resp.Message)
	}
	if resp.Instrumented == nil || !*resp.Instrumented {
		t.Error("instrumented should be true")
	}
	if resp.OutputFile == nil || *resp.OutputFile == "" {
		t.Error("output_file should be set")
	}
	// Cleanup
	if resp.OutputFile != nil {
		os.RemoveAll(*resp.OutputFile)
	}
}

func TestExecuteWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":4,"command":"execute","function":"F","inputs":[],"mocks":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestExecuteWithoutFunctionReturnsError(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc F(x int) int { return x }\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":4,"command":"execute","file":"` + tmp + `","inputs":[]}`
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestExecuteWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":4,"command":"execute","file":"/nonexistent.go","function":"F","inputs":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "file_not_found" {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestExecuteRunsFunctionAndReturnsBranchData(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":4,"command":"execute","file":"` + tmp + `","function":"classify","inputs":[5]}`
	resp := sendRecv(t, req)
	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}

	// Should have branch decisions
	if len(resp.BranchPath) == 0 {
		t.Error("expected branch_path to be populated")
	}

	// Should have lines executed
	if len(resp.LinesExecuted) == 0 {
		t.Error("expected lines_executed to be populated")
	}

	// Should have return value
	var retVal string
	if err := json.Unmarshal(resp.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "positive" {
		t.Errorf("expected %q, got %q", "positive", retVal)
	}

	// Should have path constraints
	if len(resp.PathConstraints) == 0 {
		t.Error("expected path_constraints to be populated")
	}

	// Should have performance metrics
	if resp.Performance == nil {
		t.Error("expected performance metrics")
	}
}

func TestExecuteReturnsPerformanceMetrics(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func identity(x int) int {
	return x
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":5,"command":"execute","file":"` + tmp + `","function":"identity","inputs":[42]}`
	resp := sendRecv(t, req)
	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}

	if resp.Performance == nil {
		t.Fatal("expected performance metrics")
	}
	if resp.Performance.WallTimeMs <= 0 {
		t.Errorf("expected positive wall_time_ms, got %f", resp.Performance.WallTimeMs)
	}
}

func TestExecuteWithMissingFunctionReturnsFunctionNotFound(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func Foo() {}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := `{"protocol_version":"0.1.0","id":5,"command":"execute","file":"` + tmp + `","function":"NonExistent","inputs":[]}`
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Fatalf("status = %q, want error", resp.Status)
	}
	if resp.Code != "function_not_found" {
		t.Errorf("code = %q, want function_not_found", resp.Code)
	}
}

func TestExecuteHandshakeExecuteSequence(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	responses := conversation(t,
		`{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze","execute"]}`,
		`{"protocol_version":"0.1.0","id":2,"command":"execute","file":"`+tmp+`","function":"add","inputs":[3,4]}`,
		`{"protocol_version":"0.1.0","id":3,"command":"shutdown"}`,
	)
	if len(responses) != 3 {
		t.Fatalf("got %d responses, want 3", len(responses))
	}
	if responses[0].Status != "handshake" {
		t.Errorf("response[0].status = %q, want handshake", responses[0].Status)
	}
	if responses[1].Status != "execute" {
		t.Errorf("response[1].status = %q, want execute (message: %s)", responses[1].Status, responses[1].Message)
	}
	// Verify return value
	var retVal int
	if err := json.Unmarshal(responses[1].ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != 7 {
		t.Errorf("expected return value 7, got %d", retVal)
	}
	if responses[2].Status != "shutdown_ack" {
		t.Errorf("response[2].status = %q, want shutdown_ack", responses[2].Status)
	}
}

func TestMultipleCommandsInSequence(t *testing.T) {
	responses := conversation(t,
		`{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}`,
		`{"protocol_version":"0.1.0","id":2,"command":"shutdown"}`,
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}
	if responses[0].Status != "handshake" {
		t.Errorf("response[0].status = %q, want handshake", responses[0].Status)
	}
	if responses[1].Status != "shutdown_ack" {
		t.Errorf("response[1].status = %q, want shutdown_ack", responses[1].Status)
	}
}

func TestShutdownStopsProcessingFurtherCommands(t *testing.T) {
	responses := conversation(t,
		`{"protocol_version":"0.1.0","id":1,"command":"shutdown"}`,
		`{"protocol_version":"0.1.0","id":2,"command":"handshake","capabilities":[]}`,
	)
	// Should only get one response — handler stops after shutdown
	if len(responses) != 1 {
		t.Errorf("got %d responses, want 1 (shutdown should stop processing)", len(responses))
	}
}

func TestEmptyLinesAreSkipped(t *testing.T) {
	input := "\n\n" + `{"protocol_version":"0.1.0","id":1,"command":"shutdown"}` + "\n\n"
	var output bytes.Buffer
	handler := NewHandler(strings.NewReader(input), &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	if len(lines) != 1 {
		t.Errorf("got %d responses, want 1", len(lines))
	}
}

func TestResponseIsValidNDJSON(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":[]}` + "\n" +
		`{"protocol_version":"0.1.0","id":2,"command":"shutdown"}` + "\n"
	var output bytes.Buffer
	handler := NewHandler(strings.NewReader(input), &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	// Each line must be valid JSON
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	for i, line := range lines {
		if !json.Valid([]byte(line)) {
			t.Errorf("response line %d is not valid JSON: %s", i, line)
		}
	}
}

func TestDebugOutputGoesToLog(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":1,"command":"shutdown"}` + "\n"
	var output, logBuf bytes.Buffer
	handler := NewHandlerWithLogLevel(strings.NewReader(input), &output, &logBuf, "trace")
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	logStr := logBuf.String()
	if !strings.Contains(logStr, "[shatter-go]") {
		t.Errorf("log output missing prefix: %s", logStr)
	}
	if !strings.Contains(logStr, "Shutting down") {
		t.Errorf("log output missing shutdown message: %s", logStr)
	}
}

func TestLogLevelFilteringSuppressesTraceAtInfo(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":1,"command":"shutdown"}` + "\n"
	var output, logBuf bytes.Buffer
	handler := NewHandlerWithLogLevel(strings.NewReader(input), &output, &logBuf, "info")
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	logStr := logBuf.String()
	if strings.Contains(logStr, "Received:") {
		t.Errorf("trace messages should be suppressed at info level: %s", logStr)
	}
	if strings.Contains(logStr, "Sent:") {
		t.Errorf("trace messages should be suppressed at info level: %s", logStr)
	}
}

func TestLogLevelFilteringShowsDebugAtDebug(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":1,"command":"shutdown"}` + "\n"
	var output, logBuf bytes.Buffer
	handler := NewHandlerWithLogLevel(strings.NewReader(input), &output, &logBuf, "debug")
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	logStr := logBuf.String()
	if !strings.Contains(logStr, "Starting Go frontend") {
		t.Errorf("debug messages should appear at debug level: %s", logStr)
	}
	if !strings.Contains(logStr, "Shutting down") {
		t.Errorf("debug messages should appear at debug level: %s", logStr)
	}
	if strings.Contains(logStr, "Received:") {
		t.Errorf("trace messages should be suppressed at debug level: %s", logStr)
	}
}

func TestSetupReturnsNotImplementedError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":10,"command":"setup","file":"./setup.ts","function":"init","mode":"per_function"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "internal_error" {
		t.Errorf("code = %q, want internal_error", resp.Code)
	}
	if !strings.Contains(resp.Message, "not yet implemented") {
		t.Errorf("message = %q, want 'not yet implemented'", resp.Message)
	}
	if resp.ID != 10 {
		t.Errorf("id = %d, want 10", resp.ID)
	}
}

func TestTeardownReturnsNotImplementedError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":11,"command":"teardown","function":"init"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "internal_error" {
		t.Errorf("code = %q, want internal_error", resp.Code)
	}
	if !strings.Contains(resp.Message, "not yet implemented") {
		t.Errorf("message = %q, want 'not yet implemented'", resp.Message)
	}
	if resp.ID != 11 {
		t.Errorf("id = %d, want 11", resp.ID)
	}
}

func TestGenerateReturnsNotImplementedError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":12,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "internal_error" {
		t.Errorf("code = %q, want internal_error", resp.Code)
	}
	if !strings.Contains(resp.Message, "not yet implemented") {
		t.Errorf("message = %q, want 'not yet implemented'", resp.Message)
	}
	if resp.ID != 12 {
		t.Errorf("id = %d, want 12", resp.ID)
	}
}

func TestNewCommandStubsTableDriven(t *testing.T) {
	tests := []struct {
		name    string
		request string
		wantID  int
	}{
		{
			name:    "setup per_function",
			request: `{"protocol_version":"0.1.0","id":20,"command":"setup","file":"./setup.ts","function":"fn1","mode":"per_function"}`,
			wantID:  20,
		},
		{
			name:    "setup per_execution",
			request: `{"protocol_version":"0.1.0","id":21,"command":"setup","file":"./setup.ts","function":"fn1","mode":"per_execution"}`,
			wantID:  21,
		},
		{
			name:    "teardown",
			request: `{"protocol_version":"0.1.0","id":22,"command":"teardown","function":"fn1"}`,
			wantID:  22,
		},
		{
			name:    "generate type_name",
			request: `{"protocol_version":"0.1.0","id":23,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}`,
			wantID:  23,
		},
		{
			name:    "generate param_name",
			request: `{"protocol_version":"0.1.0","id":24,"command":"generate","file":"./gen.ts","name":"authToken","kind":"param_name"}`,
			wantID:  24,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			resp := sendRecv(t, tt.request)
			if resp.Status != "error" {
				t.Errorf("status = %q, want error", resp.Status)
			}
			if resp.Code != "internal_error" {
				t.Errorf("code = %q, want internal_error", resp.Code)
			}
			if !strings.Contains(resp.Message, "not yet implemented") {
				t.Errorf("message = %q, should contain 'not yet implemented'", resp.Message)
			}
			if resp.ID != tt.wantID {
				t.Errorf("id = %d, want %d", resp.ID, tt.wantID)
			}
			if resp.ProtocolVersion != "0.1.0" {
				t.Errorf("protocol_version = %q, want 0.1.0", resp.ProtocolVersion)
			}
		})
	}
}

func TestNewCommandsInConversationSequence(t *testing.T) {
	responses := conversation(t,
		`{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}`,
		`{"protocol_version":"0.1.0","id":2,"command":"setup","file":"./setup.ts","function":"fn1","mode":"per_function"}`,
		`{"protocol_version":"0.1.0","id":3,"command":"teardown","function":"fn1"}`,
		`{"protocol_version":"0.1.0","id":4,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}`,
		`{"protocol_version":"0.1.0","id":5,"command":"shutdown"}`,
	)
	if len(responses) != 5 {
		t.Fatalf("got %d responses, want 5", len(responses))
	}
	if responses[0].Status != "handshake" {
		t.Errorf("response[0].status = %q, want handshake", responses[0].Status)
	}
	if responses[1].Status != "error" || responses[1].Code != "internal_error" {
		t.Errorf("response[1] = status:%q code:%q, want error/internal_error", responses[1].Status, responses[1].Code)
	}
	if responses[2].Status != "error" || responses[2].Code != "internal_error" {
		t.Errorf("response[2] = status:%q code:%q, want error/internal_error", responses[2].Status, responses[2].Code)
	}
	if responses[3].Status != "error" || responses[3].Code != "internal_error" {
		t.Errorf("response[3] = status:%q code:%q, want error/internal_error", responses[3].Status, responses[3].Code)
	}
	if responses[4].Status != "shutdown_ack" {
		t.Errorf("response[4].status = %q, want shutdown_ack", responses[4].Status)
	}
}

func TestNewCommandRequestDeserialization(t *testing.T) {
	tests := []struct {
		name     string
		json     string
		wantCmd  string
		wantFile string
		wantFunc string
	}{
		{
			name:     "setup request",
			json:     `{"protocol_version":"0.1.0","id":1,"command":"setup","file":"./setup.ts","function":"fn1","mode":"per_function"}`,
			wantCmd:  "setup",
			wantFile: "./setup.ts",
			wantFunc: "fn1",
		},
		{
			name:    "teardown request",
			json:    `{"protocol_version":"0.1.0","id":2,"command":"teardown","function":"fn1"}`,
			wantCmd: "teardown",
		},
		{
			name:     "generate request",
			json:     `{"protocol_version":"0.1.0","id":3,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}`,
			wantCmd:  "generate",
			wantFile: "./gen.ts",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var req Request
			if err := json.Unmarshal([]byte(tt.json), &req); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if req.Command != tt.wantCmd {
				t.Errorf("command = %q, want %q", req.Command, tt.wantCmd)
			}
			if tt.wantFile != "" && req.File != tt.wantFile {
				t.Errorf("file = %q, want %q", req.File, tt.wantFile)
			}
			if tt.wantFunc != "" {
				if req.Function == nil || *req.Function != tt.wantFunc {
					got := "<nil>"
					if req.Function != nil {
						got = *req.Function
					}
					t.Errorf("function = %q, want %q", got, tt.wantFunc)
				}
			}
		})
	}
}

func TestSetupRequestDeserializesMode(t *testing.T) {
	tests := []struct {
		name     string
		json     string
		wantMode string
	}{
		{
			name:     "per_function",
			json:     `{"protocol_version":"0.1.0","id":1,"command":"setup","file":"./s.ts","function":"f","mode":"per_function"}`,
			wantMode: "per_function",
		},
		{
			name:     "per_execution",
			json:     `{"protocol_version":"0.1.0","id":1,"command":"setup","file":"./s.ts","function":"f","mode":"per_execution"}`,
			wantMode: "per_execution",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var req Request
			if err := json.Unmarshal([]byte(tt.json), &req); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if req.Mode != tt.wantMode {
				t.Errorf("mode = %q, want %q", req.Mode, tt.wantMode)
			}
		})
	}
}

func TestGenerateRequestDeserializesKind(t *testing.T) {
	tests := []struct {
		name     string
		json     string
		wantName string
		wantKind string
	}{
		{
			name:     "type_name",
			json:     `{"protocol_version":"0.1.0","id":1,"command":"generate","file":"./g.ts","name":"User","kind":"type_name"}`,
			wantName: "User",
			wantKind: "type_name",
		},
		{
			name:     "param_name",
			json:     `{"protocol_version":"0.1.0","id":1,"command":"generate","file":"./g.ts","name":"authToken","kind":"param_name"}`,
			wantName: "authToken",
			wantKind: "param_name",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var req Request
			if err := json.Unmarshal([]byte(tt.json), &req); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if req.Name != tt.wantName {
				t.Errorf("name = %q, want %q", req.Name, tt.wantName)
			}
			if req.Kind != tt.wantKind {
				t.Errorf("kind = %q, want %q", req.Kind, tt.wantKind)
			}
		})
	}
}
