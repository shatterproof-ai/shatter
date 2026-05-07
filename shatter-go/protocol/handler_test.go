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
	"github.com/shatter-dev/shatter/shatter-go/launcher"
)

// reqJSON builds a JSON request string using ProtocolVersion instead of a
// hard-coded version literal.
func reqJSON(id int, command string, extra ...string) string {
	base := fmt.Sprintf(`{"protocol_version":%q,"id":%d,"command":%q`, ProtocolVersion, id, command)
	for _, e := range extra {
		base += "," + e
	}
	return base + "}"
}

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

func timingPhaseNames(resp Response) map[string]bool {
	phases := map[string]bool{}
	if resp.Timing == nil {
		return phases
	}
	for _, phase := range resp.Timing.Phases {
		phases[phase.PhasePath] = true
	}
	return phases
}

type fakePreparedExecution struct {
	ArtifactDir  string
	BinaryPath   string
	InvokeResult *instrument.ExecuteResult
	InvokeErr    error
	// LastReceiverKind records the receiver_kind the most recent
	// InvokeWithReceiverKind call received. Tests assert on this to
	// verify the receiver-aware Execute path threads the plan through
	// (str-hy9b.H5).
	LastReceiverKind string
}

func (f *fakePreparedExecution) IsValid() bool {
	if f.ArtifactDir != "" {
		if _, err := os.Stat(f.ArtifactDir); err != nil {
			return false
		}
	}
	if f.BinaryPath != "" {
		if _, err := os.Stat(f.BinaryPath); err != nil {
			return false
		}
	}
	return true
}

func (f *fakePreparedExecution) Cleanup() {
	if f.ArtifactDir != "" {
		_ = os.RemoveAll(f.ArtifactDir)
	}
}

func (f *fakePreparedExecution) KillProc() {}

func (f *fakePreparedExecution) Invoke(_ []json.RawMessage, _ bool) (*instrument.ExecuteResult, error) {
	return f.InvokeWithReceiverKind("", nil, false)
}

func (f *fakePreparedExecution) InvokeWithReceiverKind(receiverKind string, _ []json.RawMessage, _ bool) (*instrument.ExecuteResult, error) {
	f.LastReceiverKind = receiverKind
	if f.InvokeErr != nil {
		return nil, f.InvokeErr
	}
	if f.InvokeResult != nil {
		return f.InvokeResult, nil
	}
	return &instrument.ExecuteResult{}, nil
}

func TestHandshakeResponse(t *testing.T) {
	resp := sendRecv(t, reqJSON(42, "handshake", `"capabilities":["analyze"]`))
	if resp.Status != "handshake" {
		t.Errorf("status = %q, want handshake", resp.Status)
	}
	if resp.Language != "go" {
		t.Errorf("language = %q, want go", resp.Language)
	}
	if resp.FrontendVersion != ProtocolVersion {
		t.Errorf("frontend_version = %q, want %s", resp.FrontendVersion, ProtocolVersion)
	}
	if resp.ProtocolVersion != ProtocolVersion {
		t.Errorf("protocol_version = %q, want %s", resp.ProtocolVersion, ProtocolVersion)
	}
	if resp.ID != 42 {
		t.Errorf("id = %d, want 42", resp.ID)
	}
	caps := map[string]bool{}
	for _, c := range resp.Capabilities {
		caps[c] = true
	}
	for _, want := range CommandCapabilities {
		if !caps[want] {
			t.Errorf("missing capability %q in %v", want, resp.Capabilities)
		}
	}
}

func TestHandshakeWithTimingCapabilityDoesNotEmitTiming(t *testing.T) {
	resp := sendRecv(t, reqJSON(42, "handshake", `"capabilities":["analyze","timing"]`))
	if resp.Timing != nil {
		t.Fatalf("handshake timing = %+v, want nil", resp.Timing)
	}
}

func TestShutdownReturnsAckAndStops(t *testing.T) {
	resp := sendRecv(t, reqJSON(5, "shutdown"))
	if resp.Status != "shutdown_ack" {
		t.Errorf("status = %q, want shutdown_ack", resp.Status)
	}
	if resp.ID != 5 {
		t.Errorf("id = %d, want 5", resp.ID)
	}
}

// TestShutdownCleansUpPreparedHarnesses verifies that handleShutdown calls
// Cleanup() on all cached prepared executions, removing their artifact
// directories and clearing the preparedHarnesses map.
func TestShutdownCleansUpPreparedHarnesses(t *testing.T) {
	artifactDir := t.TempDir()

	// Build a handler and inject a prepared execution with a known artifact dir.
	// No subprocess is needed — we test the dir-removal path here.
	var output bytes.Buffer
	h := NewHandler(strings.NewReader(reqJSON(1, "shutdown")+"\n"), &output, io.Discard)
	h.preparedHarnesses["test-prepare-id"] = &fakePreparedExecution{ArtifactDir: artifactDir}

	if err := h.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	var resp Response
	if err := json.Unmarshal([]byte(strings.TrimSpace(output.String())), &resp); err != nil {
		t.Fatalf("unmarshal response: %v", err)
	}
	if resp.Status != "shutdown_ack" {
		t.Errorf("status = %q, want shutdown_ack", resp.Status)
	}

	// Cleanup() should have removed the artifact dir.
	if _, err := os.Stat(artifactDir); !os.IsNotExist(err) {
		t.Errorf("artifact dir should be removed on shutdown, os.Stat error = %v", err)
	}

	// preparedHarnesses map should be empty after shutdown.
	if len(h.preparedHarnesses) != 0 {
		t.Errorf("preparedHarnesses should be empty after shutdown, len = %d", len(h.preparedHarnesses))
	}
}

func TestVersionMismatchReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"99.0.0","id":1,"command":"handshake","capabilities":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrVersionMismatch {
		t.Errorf("code = %q, want version_mismatch", resp.Code)
	}
}

func TestVersionCompatibleWithPatchDifference(t *testing.T) {
	// A request with a different patch version but same major.minor should succeed.
	resp := sendRecv(t, `{"protocol_version":"0.1.999","id":1,"command":"handshake","capabilities":[]}`)
	if resp.Status != "handshake" {
		t.Errorf("status = %q, want handshake (patch difference should be compatible)", resp.Status)
	}
}

func TestParseMajorMinor(t *testing.T) {
	tests := []struct {
		input     string
		wantMajor int
		wantMinor int
		wantOK    bool
	}{
		{"0.1.0", 0, 1, true},
		{"1.2.3", 1, 2, true},
		{"0.1.999", 0, 1, true},
		{"1.0", 1, 0, true},
		{"bad", 0, 0, false},
		{"", 0, 0, false},
		{"a.b.c", 0, 0, false},
	}
	for _, tt := range tests {
		major, minor, ok := parseMajorMinor(tt.input)
		if ok != tt.wantOK || major != tt.wantMajor || minor != tt.wantMinor {
			t.Errorf("parseMajorMinor(%q) = (%d, %d, %v), want (%d, %d, %v)",
				tt.input, major, minor, ok, tt.wantMajor, tt.wantMinor, tt.wantOK)
		}
	}
}

func TestIsVersionCompatible(t *testing.T) {
	tests := []struct {
		version string
		want    bool
	}{
		{ProtocolVersion, true}, // exact match
		{"0.1.999", true},       // patch difference
		{"0.1", true},           // no patch
		{"0.2.0", false},        // minor mismatch
		{"1.1.0", false},        // major mismatch
		{"99.0.0", false},       // completely different
		{"bad", false},          // malformed
	}
	for _, tt := range tests {
		got := isVersionCompatible(tt.version)
		if got != tt.want {
			t.Errorf("isVersionCompatible(%q) = %v, want %v", tt.version, got, tt.want)
		}
	}
}

func TestMalformedJSONReturnsInvalidRequest(t *testing.T) {
	resp := sendRecv(t, "this is not valid json{{{")
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
	if resp.ID != 0 {
		t.Errorf("id = %d, want 0", resp.ID)
	}
}

func TestUnknownCommandReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(1, "foobar"))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestAnalyzeWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(2, "analyze", `"file":"nonexistent.go"`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFileNotFound {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestAnalyzeWithExistingFileReturnsEmptyFunctions(t *testing.T) {
	// Create a temp file to analyze
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(3, "analyze", fmt.Sprintf(`"file":"%s"`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "analyze" {
		t.Errorf("status = %q, want analyze", resp.Status)
	}
}

func TestAnalyzeEmitsTimingWhenRequested(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc Add(a int, b int) int { return a + b }\n"), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze","timing"]`),
		reqJSON(2, "analyze", fmt.Sprintf(`"file":"%s","function":"Add"`, tmp)),
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}

	phases := timingPhaseNames(responses[1])
	for _, want := range []string{
		"analyze.total",
		"analyze.parse",
		"analyze.typecheck",
		"analyze.walk",
		"serialize.response",
	} {
		if !phases[want] {
			t.Errorf("missing timing phase %q in %+v", want, responses[1].Timing)
		}
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
	input := strings.NewReader(reqJSON(3, "analyze", fmt.Sprintf(`"file":"%s"`, tmp)) + "\n")
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
	req := reqJSON(3, "analyze", fmt.Sprintf(`"file":"%s"`, tmp))
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
	req := reqJSON(3, "analyze", fmt.Sprintf(`"file":"%s","function":"Bar"`, tmp))
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
	req := reqJSON(3, "analyze", fmt.Sprintf(`"file":"%s","function":"Missing"`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Fatalf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFunctionNotFound {
		t.Errorf("code = %q, want function_not_found", resp.Code)
	}
}

func TestAnalyzeWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(2, "analyze"))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestInstrumentWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(3, "instrument", `"file":"nonexistent.go"`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFileNotFound {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestInstrumentWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(3, "instrument"))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestInstrumentWithValidFileReturnsSuccess(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc F(x int) int { if x > 0 { return 1 } ; return 0 }\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(3, "instrument", fmt.Sprintf(`"file":"%s"`, tmp))
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
	if resp.OutputFile != nil {
		if _, err := os.Stat(filepath.Join(*resp.OutputFile, filepath.Base(tmp))); err != nil {
			t.Fatalf("instrumented source missing from output dir: %v", err)
		}
	}
	// Cleanup
	if resp.OutputFile != nil {
		os.RemoveAll(*resp.OutputFile)
	}
}

func TestPrepareWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(3, "prepare", `"file":"/nonexistent.go","function":"F","mocks":[]`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFileNotFound {
		t.Errorf("code = %q, want file_not_found", resp.Code)
	}
}

func TestPrepareWithoutFunctionReturnsError(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc F(x int) int { return x }\n"), 0644); err != nil {
		t.Fatal(err)
	}
	resp := sendRecv(t, reqJSON(3, "prepare", fmt.Sprintf(`"file":"%s","mocks":[]`, tmp)))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestPrepareWithValidFileReturnsSuccess(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	resp := sendRecv(t, reqJSON(3, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp)))
	if resp.Status != "prepare" {
		t.Fatalf("status = %q, want prepare (message: %s)", resp.Status, resp.Message)
	}
	if resp.PrepareID == "" {
		t.Error("prepare_id should be non-empty")
	}
	if len(resp.PrepareID) != 16 {
		t.Errorf("prepare_id length = %d, want 16", len(resp.PrepareID))
	}
}

func TestPrepareIsIdempotent(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	prepReq := reqJSON(3, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	r1 := sendRecv(t, prepReq)
	r2 := sendRecv(t, prepReq)
	if r1.Status != "prepare" || r2.Status != "prepare" {
		t.Fatalf("expected both prepare responses, got %q and %q", r1.Status, r2.Status)
	}
	if r1.PrepareID != r2.PrepareID {
		t.Errorf("prepare_id should be deterministic: %q != %q", r1.PrepareID, r2.PrepareID)
	}
}

func TestPrepareAndExecuteSucceeds(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	prepReq := reqJSON(3, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	execReq := reqJSON(4, "execute", fmt.Sprintf(`"file":"%s","function":"add","inputs":[3,4],"mocks":[]`, tmp))

	responses := conversation(t, prepReq, execReq)
	if len(responses) != 2 {
		t.Fatalf("expected 2 responses, got %d", len(responses))
	}

	prepResp := responses[0]
	if prepResp.Status != "prepare" {
		t.Fatalf("prepare status = %q, want prepare (message: %s)", prepResp.Status, prepResp.Message)
	}
	prepareID := prepResp.PrepareID

	// Re-run using conversation with prepare_id in the execute request.
	execWithIDReq := reqJSON(5, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[3,4],"mocks":[],"prepare_id":%q`,
		tmp, prepareID,
	))
	responses2 := conversation(t, prepReq, execWithIDReq)
	if len(responses2) != 2 {
		t.Fatalf("expected 2 responses (with prepare_id), got %d", len(responses2))
	}
	execResp := responses2[1]
	if execResp.Status != "execute" {
		t.Fatalf("execute status = %q, want execute (message: %s)", execResp.Status, execResp.Message)
	}
	_ = execReq // suppress unused warning
}

func TestExecuteWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(4, "execute", `"function":"F","inputs":[],"mocks":[]`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestExecuteWithoutFunctionReturnsError(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "test.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc F(x int) int { return x }\n"), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(4, "execute", fmt.Sprintf(`"file":"%s","inputs":[]`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestExecuteWithMissingFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(4, "execute", `"file":"/nonexistent.go","function":"F","inputs":[]`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFileNotFound {
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
	req := reqJSON(4, "execute", fmt.Sprintf(`"file":"%s","function":"classify","inputs":[5]`, tmp))
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
	req := reqJSON(5, "execute", fmt.Sprintf(`"file":"%s","function":"identity","inputs":[42]`, tmp))
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
	req := reqJSON(5, "execute", fmt.Sprintf(`"file":"%s","function":"NonExistent","inputs":[]`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Fatalf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrFunctionNotFound {
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
		reqJSON(1, "handshake", `"capabilities":["analyze","execute"]`),
		reqJSON(2, "execute", fmt.Sprintf(`"file":"%s","function":"add","inputs":[3,4]`, tmp)),
		reqJSON(3, "shutdown"),
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

func TestExecuteEmitsTimingWhenRequested(t *testing.T) {
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
		reqJSON(1, "handshake", `"capabilities":["instrument","execute","timing"]`),
		reqJSON(2, "execute", fmt.Sprintf(`"file":"%s","function":"add","inputs":[3,4]`, tmp)),
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}
	if responses[1].Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", responses[1].Status, responses[1].Message)
	}

	phases := timingPhaseNames(responses[1])
	// The persistent-subprocess path emits these phases on the first (cold) call.
	// File-parsing phases (execute.parse_perf, execute.parse_results, etc.) no longer
	// exist; results stream back over stdout rather than being written to temp files.
	for _, want := range []string{
		"execute.total",
		"execute.analyze",
		"execute.instrument",
		"execute.build",
		"execute.run",
		"serialize.response",
	} {
		if !phases[want] {
			t.Errorf("missing timing phase %q in %+v", want, responses[1].Timing)
		}
	}
}

func TestInstrumentEmitsTimingWhenRequested(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func Add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	fn := "Add"
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","timing"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"%s"`, tmp, fn)),
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}
	if responses[1].Status != "instrument" {
		t.Fatalf("status = %q, want instrument (message: %s)", responses[1].Status, responses[1].Message)
	}

	phases := timingPhaseNames(responses[1])
	for _, want := range []string{
		"instrument.total",
		"serialize.response",
	} {
		if !phases[want] {
			t.Errorf("missing timing phase %q in %+v", want, responses[1].Timing)
		}
	}
}

func TestPrepareEmitsTimingWhenRequested(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func Add(a int, b int) int {
	return a + b
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	fn := "Add"
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["prepare","timing"]`),
		reqJSON(2, "prepare", fmt.Sprintf(`"file":"%s","function":"%s"`, tmp, fn)),
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}
	if responses[1].Status != "prepare" {
		t.Fatalf("status = %q, want prepare (message: %s)", responses[1].Status, responses[1].Message)
	}

	phases := timingPhaseNames(responses[1])
	for _, want := range []string{
		"prepare.total",
		"serialize.response",
	} {
		if !phases[want] {
			t.Errorf("missing timing phase %q in %+v", want, responses[1].Timing)
		}
	}
}

func TestTimingNotEmittedWithoutCapability(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte("package main\n\nfunc Add(a int, b int) int { return a + b }\n"), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze"]`),
		reqJSON(2, "analyze", fmt.Sprintf(`"file":"%s","function":"Add"`, tmp)),
	)
	if len(responses) != 2 {
		t.Fatalf("got %d responses, want 2", len(responses))
	}
	if responses[1].Timing != nil {
		t.Errorf("timing should be nil when capability not requested, got %+v", responses[1].Timing)
	}
}

func TestMultipleCommandsInSequence(t *testing.T) {
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze"]`),
		reqJSON(2, "shutdown"),
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
		reqJSON(1, "shutdown"),
		reqJSON(2, "handshake", `"capabilities":[]`),
	)
	// Should only get one response — handler stops after shutdown
	if len(responses) != 1 {
		t.Errorf("got %d responses, want 1 (shutdown should stop processing)", len(responses))
	}
}

func TestEmptyLinesAreSkipped(t *testing.T) {
	input := "\n\n" + reqJSON(1, "shutdown") + "\n\n"
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
	input := reqJSON(1, "handshake", `"capabilities":[]`) + "\n" +
		reqJSON(2, "shutdown") + "\n"
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
	input := reqJSON(1, "shutdown") + "\n"
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
	input := reqJSON(1, "shutdown") + "\n"
	var output, logBuf bytes.Buffer
	handler := NewHandlerWithLogLevel(strings.NewReader(input), &output, &logBuf, "info")
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	logStr := logBuf.String()
	if strings.Contains(logStr, "Received") {
		t.Errorf("trace messages should be suppressed at info level: %s", logStr)
	}
	if strings.Contains(logStr, "Sent") {
		t.Errorf("trace messages should be suppressed at info level: %s", logStr)
	}
}

func TestLogLevelFilteringShowsDebugAtDebug(t *testing.T) {
	input := reqJSON(1, "shutdown") + "\n"
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
	if strings.Contains(logStr, "Received") {
		t.Errorf("trace messages should be suppressed at debug level: %s", logStr)
	}
}

func TestGenerateWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(13, "generate", `"name":"User","kind":"type_name"`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestGenerateWithoutNameReturnsError(t *testing.T) {
	resp := sendRecv(t, reqJSON(14, "generate", `"file":"./gen.wasm","kind":"type_name"`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInvalidRequest {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestSetupValidationTableDriven(t *testing.T) {
	tests := []struct {
		name     string
		request  string
		wantID   int
		wantCode string
	}{
		{
			name:     "setup missing file",
			request:  reqJSON(20, "setup", `"scope":"fn1","level":"function"`),
			wantID:   20,
			wantCode: ErrInvalidRequest,
		},
		{
			name:     "setup missing scope",
			request:  reqJSON(21, "setup", `"file":"./setup.go","level":"function"`),
			wantID:   21,
			wantCode: ErrInvalidRequest,
		},
		{
			name:     "setup invalid level",
			request:  reqJSON(22, "setup", `"file":"./setup.go","scope":"fn1","level":"bogus"`),
			wantID:   22,
			wantCode: ErrInvalidRequest,
		},
		{
			name:     "setup file not found",
			request:  reqJSON(23, "setup", `"file":"./nonexistent.go","scope":"fn1","level":"function"`),
			wantID:   23,
			wantCode: ErrFileNotFound,
		},
		{
			name:     "teardown missing scope",
			request:  reqJSON(24, "teardown", `"level":"function"`),
			wantID:   24,
			wantCode: ErrInvalidRequest,
		},
		{
			name:     "teardown invalid level",
			request:  reqJSON(25, "teardown", `"scope":"fn1","level":"bogus"`),
			wantID:   25,
			wantCode: ErrInvalidRequest,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			resp := sendRecv(t, tt.request)
			if resp.Status != "error" {
				t.Errorf("status = %q, want error", resp.Status)
			}
			if resp.Code != tt.wantCode {
				t.Errorf("code = %q, want %q", resp.Code, tt.wantCode)
			}
			if resp.ID != tt.wantID {
				t.Errorf("id = %d, want %d", resp.ID, tt.wantID)
			}
			if resp.ProtocolVersion != ProtocolVersion {
				t.Errorf("protocol_version = %q, want %s", resp.ProtocolVersion, ProtocolVersion)
			}
		})
	}
}

func TestSetupWithValidFileReturnsContext(t *testing.T) {
	// Create a temp Go file that prints setup context JSON to stdout.
	tmp := filepath.Join(t.TempDir(), "setup_fixture.go")
	src := "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"{\\\"db\\\":\\\"test_db\\\",\\\"ready\\\":true}\")\n}\n"
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	req := reqJSON(30, "setup", fmt.Sprintf(`"file":"%s","scope":"myFunc","level":"function"`, tmp))
	resp := sendRecv(t, req)
	if resp.Status != "setup" {
		t.Fatalf("status = %q, want setup (message: %s)", resp.Status, resp.Message)
	}
	if resp.SetupContext == nil {
		t.Fatal("setup_context must not be nil")
	}
	// Verify the context contains the expected JSON
	var ctx map[string]interface{}
	if err := json.Unmarshal(*resp.SetupContext, &ctx); err != nil {
		t.Fatalf("unmarshal setup_context: %v", err)
	}
	if ctx["db"] != "test_db" {
		t.Errorf("setup_context.db = %v, want test_db", ctx["db"])
	}
	if ctx["ready"] != true {
		t.Errorf("setup_context.ready = %v, want true", ctx["ready"])
	}
}

func TestTeardownWithoutSetupReturnsError(t *testing.T) {
	// Teardown without prior setup should error — matches TS frontend behavior.
	resp := sendRecv(t, reqJSON(31, "teardown", `"scope":"myFunc","level":"function"`))
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrInternalError {
		t.Errorf("code = %q, want %q", resp.Code, ErrInternalError)
	}
	if !strings.Contains(resp.Message, "No setup context") {
		t.Errorf("message = %q, should contain 'No setup context'", resp.Message)
	}
	if resp.ID != 31 {
		t.Errorf("id = %d, want 31", resp.ID)
	}
}

func TestTeardownAfterSetupReturnsAck(t *testing.T) {
	// Setup then teardown should succeed.
	tmp := filepath.Join(t.TempDir(), "setup_fixture.go")
	src := "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"{\\\"ok\\\":true}\")\n}\n"
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze"]`),
		reqJSON(2, "setup", fmt.Sprintf(`"file":"%s","scope":"fn1","level":"function"`, tmp)),
		reqJSON(3, "teardown", `"scope":"fn1","level":"function"`),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[1].Status != "setup" {
		t.Fatalf("setup status = %q, want setup", responses[1].Status)
	}
	if responses[2].Status != "teardown_ack" {
		t.Errorf("teardown status = %q, want teardown_ack", responses[2].Status)
	}
}

func TestTeardownClearsSessionState(t *testing.T) {
	// After teardown, lastAnalyzedFile should be cleared. An execute without
	// an explicit file should fail with "missing file" rather than falling
	// back to a stale analyzed file.
	tmp := filepath.Join(t.TempDir(), "setup_fixture.go")
	src := "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"{\\\"ok\\\":true}\")\n}\n"
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	goFile := filepath.Join(t.TempDir(), "target.go")
	goSrc := "package main\n\nfunc Add(a, b int) int { return a + b }\n"
	if err := os.WriteFile(goFile, []byte(goSrc), 0644); err != nil {
		t.Fatal(err)
	}
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze"]`),
		// analyze sets lastAnalyzedFile
		reqJSON(2, "analyze", fmt.Sprintf(`"file":"%s"`, goFile)),
		// setup + teardown should clear it
		reqJSON(3, "setup", fmt.Sprintf(`"file":"%s","scope":"fn1","level":"function"`, tmp)),
		reqJSON(4, "teardown", `"scope":"fn1","level":"function"`),
		// execute without file — should fail because lastAnalyzedFile was cleared
		reqJSON(5, "execute", `"function":"Add","args":[1,2]`),
		reqJSON(6, "shutdown"),
	)
	if len(responses) != 6 {
		t.Fatalf("got %d responses, want 6", len(responses))
	}
	if responses[3].Status != "teardown_ack" {
		t.Errorf("teardown status = %q, want teardown_ack", responses[3].Status)
	}
	// Execute without file after teardown should error with "requires a file path"
	// because lastAnalyzedFile was cleared — not a downstream execution error.
	if responses[4].Status != "error" {
		t.Errorf("execute status = %q, want error (stale file should be cleared)", responses[4].Status)
	}
	if responses[4].Code != ErrInvalidRequest {
		t.Errorf("execute code = %q, want %q (should be missing-file, not execution error)", responses[4].Code, ErrInvalidRequest)
	}
	if !strings.Contains(responses[4].Message, "requires a file path") {
		t.Errorf("execute message = %q, should contain 'requires a file path'", responses[4].Message)
	}
}

func TestGenerateUnsupportedExtensionTableDriven(t *testing.T) {
	tests := []struct {
		name    string
		request string
		wantID  int
	}{
		{
			name:    "generate type_name with .ts file",
			request: reqJSON(23, "generate", `"file":"./gen.ts","name":"User","kind":"type_name"`),
			wantID:  23,
		},
		{
			name:    "generate param_name with .ts file",
			request: reqJSON(24, "generate", `"file":"./gen.ts","name":"authToken","kind":"param_name"`),
			wantID:  24,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			resp := sendRecv(t, tt.request)
			if resp.Status != "error" {
				t.Errorf("status = %q, want error", resp.Status)
			}
			if resp.Code != ErrInternalError {
				t.Errorf("code = %q, want internal_error", resp.Code)
			}
			if !strings.Contains(resp.Message, "unsupported generator type") {
				t.Errorf("message = %q, should contain 'unsupported generator type'", resp.Message)
			}
			if resp.ID != tt.wantID {
				t.Errorf("id = %d, want %d", resp.ID, tt.wantID)
			}
			if resp.ProtocolVersion != ProtocolVersion {
				t.Errorf("protocol_version = %q, want 0.1.0", resp.ProtocolVersion)
			}
		})
	}
}

func TestNewCommandsInConversationSequence(t *testing.T) {
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["analyze"]`),
		reqJSON(2, "setup", `"file":"./nonexistent.go","scope":"fn1","level":"function"`),
		reqJSON(3, "teardown", `"scope":"fn1","level":"function"`),
		reqJSON(4, "generate", `"file":"./gen.ts","name":"User","kind":"type_name"`),
		reqJSON(5, "shutdown"),
	)
	if len(responses) != 5 {
		t.Fatalf("got %d responses, want 5", len(responses))
	}
	if responses[0].Status != "handshake" {
		t.Errorf("response[0].status = %q, want handshake", responses[0].Status)
	}
	if responses[1].Status != "error" || responses[1].Code != ErrFileNotFound {
		t.Errorf("response[1] = status:%q code:%q, want error/file_not_found", responses[1].Status, responses[1].Code)
	}
	// Teardown after failed setup should error — no context was stored.
	if responses[2].Status != "error" || responses[2].Code != ErrInternalError {
		t.Errorf("response[2] = status:%q code:%q, want error/internal_error", responses[2].Status, responses[2].Code)
	}
	if responses[3].Status != "error" || responses[3].Code != ErrInternalError {
		t.Errorf("response[3] = status:%q code:%q, want error/internal_error", responses[3].Status, responses[3].Code)
	}
	if responses[4].Status != "shutdown_ack" {
		t.Errorf("response[4].status = %q, want shutdown_ack", responses[4].Status)
	}
}

func TestNewCommandRequestDeserialization(t *testing.T) {
	tests := []struct {
		name      string
		json      string
		wantCmd   string
		wantFile  string
		wantScope string
		wantLevel SetupLevel
	}{
		{
			name:      "setup request",
			json:      reqJSON(1, "setup", `"file":"./setup.go","scope":"fn1","level":"function"`),
			wantCmd:   "setup",
			wantFile:  "./setup.go",
			wantScope: "fn1",
			wantLevel: SetupLevelFunction,
		},
		{
			name:      "teardown request",
			json:      reqJSON(2, "teardown", `"scope":"fn1","level":"function"`),
			wantCmd:   "teardown",
			wantScope: "fn1",
			wantLevel: SetupLevelFunction,
		},
		{
			name:     "generate request",
			json:     reqJSON(3, "generate", `"file":"./gen.ts","name":"User","kind":"type_name"`),
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
			if tt.wantScope != "" && req.Scope != tt.wantScope {
				t.Errorf("scope = %q, want %q", req.Scope, tt.wantScope)
			}
			if tt.wantLevel != "" && req.Level != tt.wantLevel {
				t.Errorf("level = %q, want %q", req.Level, tt.wantLevel)
			}
		})
	}
}

func TestSetupRequestDeserializesLevel(t *testing.T) {
	tests := []struct {
		name      string
		json      string
		wantLevel SetupLevel
		wantScope string
	}{
		{
			name:      "session",
			json:      reqJSON(1, "setup", `"file":"./s.go","scope":"proj","level":"session"`),
			wantLevel: SetupLevelSession,
			wantScope: "proj",
		},
		{
			name:      "file",
			json:      reqJSON(1, "setup", `"file":"./s.go","scope":"mod","level":"file"`),
			wantLevel: SetupLevelFile,
			wantScope: "mod",
		},
		{
			name:      "function",
			json:      reqJSON(1, "setup", `"file":"./s.go","scope":"fn1","level":"function"`),
			wantLevel: SetupLevelFunction,
			wantScope: "fn1",
		},
		{
			name:      "execution",
			json:      reqJSON(1, "setup", `"file":"./s.go","scope":"fn1","level":"execution"`),
			wantLevel: SetupLevelExecution,
			wantScope: "fn1",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var req Request
			if err := json.Unmarshal([]byte(tt.json), &req); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if req.Level != tt.wantLevel {
				t.Errorf("level = %q, want %q", req.Level, tt.wantLevel)
			}
			if req.Scope != tt.wantScope {
				t.Errorf("scope = %q, want %q", req.Scope, tt.wantScope)
			}
		})
	}
}

func TestSetupRequestWithParentContextDeserializes(t *testing.T) {
	reqStr := reqJSON(1, "setup", `"file":"./s.go","scope":"fn1","level":"function","parent_context":{"contexts":[{"level":"session","context":{"id":"s1"}},{"level":"file","context":{"path":"f.go"}}]}`)
	var req Request
	if err := json.Unmarshal([]byte(reqStr), &req); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if req.ParentContext == nil {
		t.Fatal("parent_context must not be nil")
	}
	if len(req.ParentContext.Contexts) != 2 {
		t.Fatalf("parent_context.contexts len = %d, want 2", len(req.ParentContext.Contexts))
	}
	if req.ParentContext.Contexts[0].Level != SetupLevelSession {
		t.Errorf("contexts[0].level = %q, want session", req.ParentContext.Contexts[0].Level)
	}
	if req.ParentContext.Contexts[1].Level != SetupLevelFile {
		t.Errorf("contexts[1].level = %q, want file", req.ParentContext.Contexts[1].Level)
	}
	// Verify the nested context values are preserved
	var ctx0 map[string]interface{}
	if err := json.Unmarshal(*req.ParentContext.Contexts[0].Context, &ctx0); err != nil {
		t.Fatalf("unmarshal contexts[0].context: %v", err)
	}
	if ctx0["id"] != "s1" {
		t.Errorf("contexts[0].context.id = %v, want s1", ctx0["id"])
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
			json:     reqJSON(1, "generate", `"file":"./g.ts","name":"User","kind":"type_name"`),
			wantName: "User",
			wantKind: "type_name",
		},
		{
			name:     "param_name",
			json:     reqJSON(1, "generate", `"file":"./g.ts","name":"authToken","kind":"param_name"`),
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

// TestConvertSideEffects verifies that instrument.SideEffect values are
// correctly mapped to protocol SideEffect values with the right JSON field
// names (kind, not type) and snake_case values (console_output, not ConsoleOutput).
// TestExecuteAfterInstrumentWithoutAnalyze verifies the scan flow:
// Instrument (with file) → Execute (without file) succeeds because
// handleInstrument sets lastAnalyzedFile.
func TestExecuteAfterInstrumentWithoutAnalyze(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func double(x int) int {
	return x * 2
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}
	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","execute"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"double"`, tmp)),
		reqJSON(3, "execute", `"function":"double","inputs":[5],"mocks":[]`),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[1].Status != "instrument" {
		t.Fatalf("instrument: status = %q, want instrument (message: %s)", responses[1].Status, responses[1].Message)
	}
	if responses[2].Status != "execute" {
		t.Fatalf("execute: status = %q, want execute (message: %s)", responses[2].Status, responses[2].Message)
	}
	var retVal int
	if err := json.Unmarshal(responses[2].ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != 10 {
		t.Errorf("expected return value 10, got %d", retVal)
	}
	// Cleanup instrumented output
	if responses[1].OutputFile != nil {
		os.RemoveAll(*responses[1].OutputFile)
	}
}

func TestExecutePanicEmitsThrownErrorSideEffect(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

func explode() int {
	panic("boom")
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","execute"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"explode"`, tmp)),
		reqJSON(3, "execute", `"function":"explode","inputs":[],"mocks":[]`),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[2].Status != "execute" {
		t.Fatalf("execute: status = %q, want execute (message: %s)", responses[2].Status, responses[2].Message)
	}
	if responses[2].ThrownError == nil {
		t.Fatal("execute response missing top-level thrown_error")
	}

	var thrownEffects []SideEffect
	for _, effect := range responses[2].SideEffects {
		if effect.Kind == "thrown_error" {
			thrownEffects = append(thrownEffects, effect)
		}
	}
	if len(thrownEffects) != 1 {
		t.Fatalf("thrown_error side effects = %d, want 1; all side effects: %+v", len(thrownEffects), responses[2].SideEffects)
	}
	if thrownEffects[0].ErrorType != "panic" {
		t.Fatalf("thrown_error error_type = %q, want panic", thrownEffects[0].ErrorType)
	}
	if thrownEffects[0].Message != "boom" {
		t.Fatalf("thrown_error message = %q, want boom", thrownEffects[0].Message)
	}
	if thrownEffects[0].Stack == nil || !strings.Contains(*thrownEffects[0].Stack, "explode") {
		t.Fatalf("thrown_error stack missing explode frame: %v", thrownEffects[0].Stack)
	}
}

func TestExecuteGlobalStateChangeEmitsGlobalMutationSideEffect(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

var Counter int

func bump() int {
	Counter++
	return Counter
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","execute"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"bump"`, tmp)),
		reqJSON(3, "execute", `"function":"bump","inputs":[],"mocks":[]`),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[2].Status != "execute" {
		t.Fatalf("execute: status = %q, want execute (message: %s)", responses[2].Status, responses[2].Message)
	}

	var sawStateChange, sawMutation bool
	for _, effect := range responses[2].SideEffects {
		switch effect.Kind {
		case "global_state_change":
			if effect.Variable == "Counter" {
				sawStateChange = true
			}
		case "global_mutation":
			if effect.Name == "Counter" {
				sawMutation = true
			}
		}
	}
	if !sawStateChange {
		t.Fatalf("missing global_state_change for Counter; side effects: %+v", responses[2].SideEffects)
	}
	if !sawMutation {
		t.Fatalf("missing global_mutation for Counter; side effects: %+v", responses[2].SideEffects)
	}
}

func TestExecuteConsoleOutputEmitsPerCallLevels(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	src := `package main

import (
	"fmt"
	"log/slog"
	"os"
)

func chatter() int {
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelDebug})))
	fmt.Print("plain")
	fmt.Println("line")
	fmt.Printf("formatted %d", 7)
	slog.Info("info message")
	slog.Warn("warn message")
	slog.Error("error message")
	slog.Debug("debug message")
	return 1
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","execute"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"chatter"`, tmp)),
		reqJSON(3, "execute", `"function":"chatter","inputs":[],"mocks":[]`),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[2].Status != "execute" {
		t.Fatalf("execute: status = %q, want execute (message: %s)", responses[2].Status, responses[2].Message)
	}

	got := map[string]int{}
	for _, effect := range responses[2].SideEffects {
		if effect.Kind == "console_output" {
			got[effect.Level+"|"+effect.Message]++
		}
	}
	for _, want := range []string{
		"log|plain",
		"log|line",
		"log|formatted 7",
		"info|info message",
		"warn|warn message",
		"error|error message",
		"debug|debug message",
	} {
		if got[want] != 1 {
			t.Fatalf("console effect %q count = %d, want 1; all side effects: %+v", want, got[want], responses[2].SideEffects)
		}
	}
}

func TestExecuteCapturesGoOSLevelSideEffects(t *testing.T) {
	dir := t.TempDir()
	tmp := filepath.Join(dir, "target.go")
	outPath := filepath.Join(dir, "out.txt")
	src := `package main

import (
	"net/http"
	"net/http/httptest"
	"os"
)

func osEffects(path string) int {
	_ = os.WriteFile(path, []byte("payload"), 0644)
	os.Setenv("SHATTER_SIDE_EFFECT_TEST", "seen")
	_ = os.Getenv("SHATTER_SIDE_EFFECT_TEST")
	_, _ = os.LookupEnv("SHATTER_SIDE_EFFECT_TEST")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()
	resp, err := http.Get(server.URL + "/ping")
	if err == nil {
		defer resp.Body.Close()
	}
	return 1
}
`
	if err := os.WriteFile(tmp, []byte(src), 0644); err != nil {
		t.Fatal(err)
	}

	responses := conversation(t,
		reqJSON(1, "handshake", `"capabilities":["instrument","execute"]`),
		reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s","function":"osEffects"`, tmp)),
		reqJSON(3, "execute", fmt.Sprintf(`"function":"osEffects","inputs":[%q],"mocks":[]`, outPath)),
		reqJSON(4, "shutdown"),
	)
	if len(responses) != 4 {
		t.Fatalf("got %d responses, want 4", len(responses))
	}
	if responses[2].Status != "execute" {
		t.Fatalf("execute: status = %q, want execute (message: %s)", responses[2].Status, responses[2].Message)
	}

	var sawFileWrite, sawEnvRead, sawNetworkRequest bool
	for _, effect := range responses[2].SideEffects {
		switch effect.Kind {
		case "file_write":
			if effect.Path == outPath && effect.Content == "payload" {
				sawFileWrite = true
			}
		case "environment_read":
			if effect.Variable == "SHATTER_SIDE_EFFECT_TEST" && effect.Value != nil && *effect.Value == "seen" {
				sawEnvRead = true
			}
		case "network_request":
			if effect.Method == "GET" && strings.Contains(effect.URL, "/ping") {
				sawNetworkRequest = true
			}
		}
	}
	if !sawFileWrite {
		t.Fatalf("missing file_write side effect for %s; side effects: %+v", outPath, responses[2].SideEffects)
	}
	if !sawEnvRead {
		t.Fatalf("missing environment_read side effect for SHATTER_SIDE_EFFECT_TEST; side effects: %+v", responses[2].SideEffects)
	}
	if !sawNetworkRequest {
		t.Fatalf("missing network_request side effect for GET /ping; side effects: %+v", responses[2].SideEffects)
	}
}

func TestConvertSideEffects(t *testing.T) {
	input := []instrument.SideEffect{
		{Kind: "console_output", Level: "log", Message: "hello stdout"},
		{Kind: "console_output", Level: "error", Message: "oops stderr"},
	}
	result := convertSideEffects(input)
	if len(result) != 2 {
		t.Fatalf("expected 2 side effects, got %d", len(result))
	}

	// Verify field mapping
	if result[0].Kind != "console_output" {
		t.Errorf("result[0].Kind = %q, want %q", result[0].Kind, "console_output")
	}
	if result[0].Level != "log" {
		t.Errorf("result[0].Level = %q, want %q", result[0].Level, "log")
	}
	if result[0].Message != "hello stdout" {
		t.Errorf("result[0].Message = %q, want %q", result[0].Message, "hello stdout")
	}
	if result[1].Kind != "console_output" {
		t.Errorf("result[1].Kind = %q, want %q", result[1].Kind, "console_output")
	}
	if result[1].Level != "error" {
		t.Errorf("result[1].Level = %q, want %q", result[1].Level, "error")
	}

	// Verify JSON serialization uses correct field names
	data, err := json.Marshal(result[0])
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	jsonStr := string(data)
	if !strings.Contains(jsonStr, `"kind":"console_output"`) {
		t.Errorf("JSON should contain \"kind\":\"console_output\", got: %s", jsonStr)
	}
	if strings.Contains(jsonStr, `"type"`) {
		t.Errorf("JSON should not contain \"type\" field, got: %s", jsonStr)
	}
}

func TestConvertSideEffectsEmpty(t *testing.T) {
	result := convertSideEffects(nil)
	if len(result) != 0 {
		t.Fatalf("expected 0 side effects, got %d", len(result))
	}
}

func TestConvertSideEffectsAllKinds(t *testing.T) {
	stack := "at foo:1"
	value := "secret"
	body := json.RawMessage(`{"x":1}`)
	before := json.RawMessage(`0`)
	after := json.RawMessage(`1`)

	input := []instrument.SideEffect{
		{Kind: "console_output", Level: "warn", Message: "watch out"},
		{Kind: "file_write", Path: "/tmp/out.txt", Content: "data"},
		{Kind: "network_request", Method: "POST", URL: "https://example.com", Body: &body},
		{Kind: "environment_read", Variable: "HOME", Value: &value},
		{Kind: "global_mutation", Name: "GlobalCounter"},
		{Kind: "thrown_error", ErrorType: "TypeError", Message: "bad type", Stack: &stack},
		{Kind: "global_state_change", Variable: "Counter", Before: &before, After: &after},
	}
	result := convertSideEffects(input)
	if len(result) != 7 {
		t.Fatalf("expected 7 side effects, got %d", len(result))
	}

	cases := []struct {
		idx  int
		kind string
	}{
		{0, "console_output"},
		{1, "file_write"},
		{2, "network_request"},
		{3, "environment_read"},
		{4, "global_mutation"},
		{5, "thrown_error"},
		{6, "global_state_change"},
	}
	for _, c := range cases {
		if result[c.idx].Kind != c.kind {
			t.Errorf("result[%d].Kind = %q, want %q", c.idx, result[c.idx].Kind, c.kind)
		}
		// Each entry must round-trip through JSON with correct "kind" field
		data, err := json.Marshal(result[c.idx])
		if err != nil {
			t.Fatalf("marshal result[%d]: %v", c.idx, err)
		}
		var m map[string]interface{}
		if err := json.Unmarshal(data, &m); err != nil {
			t.Fatalf("unmarshal result[%d]: %v", c.idx, err)
		}
		if m["kind"] != c.kind {
			t.Errorf("result[%d] JSON kind = %q, want %q", c.idx, m["kind"], c.kind)
		}
	}

	// Spot-check individual field mappings
	if result[1].Path != "/tmp/out.txt" {
		t.Errorf("file_write Path = %q, want %q", result[1].Path, "/tmp/out.txt")
	}
	if result[1].Content != "data" {
		t.Errorf("file_write Content = %q, want %q", result[1].Content, "data")
	}
	if result[2].Method != "POST" {
		t.Errorf("network_request Method = %q, want %q", result[2].Method, "POST")
	}
	if result[4].Name != "GlobalCounter" {
		t.Errorf("global_mutation Name = %q, want %q", result[4].Name, "GlobalCounter")
	}
	if result[5].ErrorType != "TypeError" {
		t.Errorf("thrown_error ErrorType = %q, want %q", result[5].ErrorType, "TypeError")
	}
	if result[5].Stack == nil || *result[5].Stack != "at foo:1" {
		t.Errorf("thrown_error Stack = %v, want %q", result[5].Stack, "at foo:1")
	}
	if result[6].Variable != "Counter" {
		t.Errorf("global_state_change Variable = %q, want %q", result[6].Variable, "Counter")
	}
}

func TestConvertLauncherSideEffectsAllKinds(t *testing.T) {
	stack := "at foo:1"
	value := "secret"
	body := json.RawMessage(`{"x":1}`)
	before := json.RawMessage(`0`)
	after := json.RawMessage(`1`)

	input := []launcher.LauncherSideEffect{
		{Kind: "console_output", Level: "warn", Message: "watch out"},
		{Kind: "file_write", Path: "/tmp/out.txt", Content: "data"},
		{Kind: "network_request", Method: "POST", URL: "https://example.com", Body: &body},
		{Kind: "environment_read", Variable: "HOME", Value: &value},
		{Kind: "global_mutation", Name: "GlobalCounter"},
		{Kind: "thrown_error", ErrorType: "TypeError", Message: "bad type", Stack: &stack},
		{Kind: "global_state_change", Variable: "Counter", Before: before, After: after},
	}
	result := convertLauncherSideEffects(input)
	if len(result) != 7 {
		t.Fatalf("expected 7 side effects, got %d", len(result))
	}

	if result[1].Path != "/tmp/out.txt" || result[1].Content != "data" {
		t.Fatalf("file_write fields lost: %+v", result[1])
	}
	if result[2].Method != "POST" || result[2].URL != "https://example.com" || result[2].Body == nil {
		t.Fatalf("network_request fields lost: %+v", result[2])
	}
	if result[3].Variable != "HOME" || result[3].Value == nil || *result[3].Value != "secret" {
		t.Fatalf("environment_read fields lost: %+v", result[3])
	}
	if result[4].Name != "GlobalCounter" {
		t.Fatalf("global_mutation fields lost: %+v", result[4])
	}
	if result[5].ErrorType != "TypeError" || result[5].Stack == nil || *result[5].Stack != "at foo:1" {
		t.Fatalf("thrown_error fields lost: %+v", result[5])
	}
	if result[6].Before == nil || result[6].After == nil {
		t.Fatalf("global_state_change before/after fields lost: %+v", result[6])
	}
}

// convertBranchPath must always emit a non-nil constraint, even when
// the Go instrumentor provides no symbolic constraint (ConstraintJSON == "").
// Without this, Rust's serde rejects the response due to the missing field.
func TestConvertBranchPathEmitsConstraintWhenEmpty(t *testing.T) {
	branches := []instrument.BranchDecision{
		{BranchID: 1, Line: 10, Taken: true, ConstraintJSON: ""},
	}
	result := convertBranchPath(branches)
	if len(result) != 1 {
		t.Fatalf("expected 1 decision, got %d", len(result))
	}
	bd := result[0]
	if bd.Constraint == nil {
		t.Fatal("constraint must not be nil when ConstraintJSON is empty")
	}
	if bd.Constraint.Kind != "unknown" {
		t.Errorf("constraint.Kind = %q, want %q", bd.Constraint.Kind, "unknown")
	}

	// Verify the JSON contains the constraint field (not omitted)
	data, err := json.Marshal(bd)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	jsonStr := string(data)
	if !strings.Contains(jsonStr, `"constraint"`) {
		t.Errorf("JSON must contain constraint field, got: %s", jsonStr)
	}
	if !strings.Contains(jsonStr, `"kind":"unknown"`) {
		t.Errorf("JSON constraint must have kind=unknown, got: %s", jsonStr)
	}
}

func TestConvertBranchPathPreservesExplicitConstraint(t *testing.T) {
	constraintJSON := `{"kind":"expr","expr":{"kind":"binop","op":"==","left":{"kind":"param","name":"x","path":[]},"right":{"kind":"const","value":5}}}`
	branches := []instrument.BranchDecision{
		{BranchID: 2, Line: 20, Taken: false, ConstraintJSON: constraintJSON},
	}
	result := convertBranchPath(branches)
	if result[0].Constraint == nil {
		t.Fatal("constraint must not be nil for explicit constraint")
	}
	if result[0].Constraint.Kind != "expr" {
		t.Errorf("constraint.Kind = %q, want %q", result[0].Constraint.Kind, "expr")
	}
}

// simpleGoSource returns a minimal Go source file with an add function.
func simpleGoSource() string {
	return `package main

func add(a int, b int) int {
	return a + b
}
`
}

func TestExecuteReusesPreparedSubprocess(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	prepReq := reqJSON(1, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	exec1 := reqJSON(2, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[1,2],"mocks":[]`, tmp))
	exec2 := reqJSON(3, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[3,4],"mocks":[]`, tmp))

	// Use a single handler session so subprocess state persists.
	input := strings.NewReader(strings.Join([]string{prepReq, exec1, exec2}, "\n") + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	if len(lines) != 3 {
		t.Fatalf("expected 3 responses, got %d", len(lines))
	}

	var prepResp, exec1Resp, exec2Resp Response
	json.Unmarshal([]byte(lines[0]), &prepResp)
	json.Unmarshal([]byte(lines[1]), &exec1Resp)
	json.Unmarshal([]byte(lines[2]), &exec2Resp)

	if prepResp.Status != "prepare" {
		t.Fatalf("prepare status = %q (message: %s)", prepResp.Status, prepResp.Message)
	}
	if exec1Resp.Status != "execute" {
		t.Fatalf("first execute status = %q (message: %s)", exec1Resp.Status, exec1Resp.Message)
	}
	if exec2Resp.Status != "execute" {
		t.Fatalf("second execute status = %q (message: %s)", exec2Resp.Status, exec2Resp.Message)
	}
}

func TestExecuteAutoLookupPreparedHarness(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	// Prepare first, then execute WITHOUT prepare_id — should auto-lookup.
	prepReq := reqJSON(1, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	execReq := reqJSON(2, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[5,6],"mocks":[]`, tmp))

	responses := conversation(t, prepReq, execReq)
	if len(responses) != 2 {
		t.Fatalf("expected 2 responses, got %d", len(responses))
	}
	if responses[0].Status != "prepare" {
		t.Fatalf("prepare status = %q (message: %s)", responses[0].Status, responses[0].Message)
	}
	if responses[1].Status != "execute" {
		t.Fatalf("execute status = %q (message: %s)", responses[1].Status, responses[1].Message)
	}
}

func TestPreparedHarnessStaleKeyForceRebuild(t *testing.T) {
	// Verify that different mock configurations produce different prepare_ids,
	// which means the handler caches them separately (stale key = different key).
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	mocksA := []instrument.MockConfig{}
	mocksB := []instrument.MockConfig{{Symbol: "someFunc"}}

	idA := computePrepareID(tmp, "add", mocksA, "")
	idB := computePrepareID(tmp, "add", mocksB, "")

	if idA == idB {
		t.Errorf("different mock configs must produce different prepare_ids: %s == %s", idA, idB)
	}

	// Also verify same inputs produce same id (idempotent).
	idA2 := computePrepareID(tmp, "add", mocksA, "")
	if idA != idA2 {
		t.Errorf("same inputs must produce same prepare_id: %s != %s", idA, idA2)
	}
}

// TestComputePrepareIDReceiverKindSensitive verifies that two computePrepareID
// calls with the same (file, function, mocks) but different receiver_kind values
// produce different IDs (str-oegu).
func TestComputePrepareIDReceiverKindSensitive(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	mocks := []instrument.MockConfig{}
	idFreeFunc := computePrepareID(tmp, "NewService", mocks, "")
	idConstructor := computePrepareID(tmp, "NewService", mocks, "constructor:NewService")

	if idFreeFunc == idConstructor {
		t.Errorf("different receiver_kind must produce different prepare_ids: both=%s", idFreeFunc)
	}

	// Idempotency: same receiver_kind must reproduce the same ID.
	idConstructor2 := computePrepareID(tmp, "NewService", mocks, "constructor:NewService")
	if idConstructor != idConstructor2 {
		t.Errorf("same receiver_kind must be deterministic: first=%s second=%s", idConstructor, idConstructor2)
	}
}

// TestHandlePrepareWithPlanKeysOnReceiverKind verifies that a Prepare request
// carrying a plan with a non-empty receiver_kind produces a prepare_id that
// differs from a plan-less Prepare for the same target (str-oegu).
func TestHandlePrepareWithPlanKeysOnReceiverKind(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	// ID for a plan-less prepare (receiverKind = "").
	idNoReceiver := computePrepareID(tmp, "add", nil, "")
	// ID for a plan with a concrete receiver_kind.
	idWithReceiver := computePrepareID(tmp, "add", nil, "constructor:NewService")

	if idNoReceiver == idWithReceiver {
		t.Errorf("plan-less prepare and plan-with-receiver must have different prepare_ids: both=%s", idNoReceiver)
	}
}

func TestPreparedHarnessDeadProcessRecovery(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	// Use a long-lived handler to test subprocess recovery.
	prepReq := reqJSON(1, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	exec1 := reqJSON(2, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[1,2],"mocks":[],"prepare_id":"PLACEHOLDER"`, tmp))
	shutdownReq := reqJSON(99, "shutdown")

	// First: prepare and get the prepare_id.
	input1 := strings.NewReader(prepReq + "\n")
	var output1 bytes.Buffer
	h := NewHandler(input1, &output1, io.Discard)
	if err := h.Run(); err != nil {
		t.Fatalf("handler.Run (prepare): %v", err)
	}
	var prepResp Response
	json.Unmarshal([]byte(strings.TrimSpace(output1.String())), &prepResp)
	if prepResp.Status != "prepare" {
		t.Fatalf("prepare status = %q (message: %s)", prepResp.Status, prepResp.Message)
	}

	// Execute once to spawn the subprocess.
	execWithID := reqJSON(2, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[1,2],"mocks":[],"prepare_id":%q`, tmp, prepResp.PrepareID))
	input2 := strings.NewReader(execWithID + "\n")
	var output2 bytes.Buffer
	h2 := NewHandlerWithLogLevel(input2, &output2, io.Discard, "error")
	h2.preparedHarnesses = h.preparedHarnesses // share the cache
	if err := h2.Run(); err != nil {
		t.Fatalf("handler.Run (exec1): %v", err)
	}
	var exec1Resp Response
	json.Unmarshal([]byte(strings.TrimSpace(output2.String())), &exec1Resp)
	if exec1Resp.Status != "execute" {
		t.Fatalf("exec1 status = %q (message: %s)", exec1Resp.Status, exec1Resp.Message)
	}

	// Kill the subprocess to simulate a dead process.
	harness := h2.preparedHarnesses[prepResp.PrepareID]
	harness.KillProc()

	// Execute again — should detect dead process and respawn.
	execAgain := reqJSON(3, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[10,20],"mocks":[],"prepare_id":%q`, tmp, prepResp.PrepareID))
	input3 := strings.NewReader(execAgain + "\n" + shutdownReq + "\n")
	var output3 bytes.Buffer
	h3 := NewHandlerWithLogLevel(input3, &output3, io.Discard, "error")
	h3.preparedHarnesses = h2.preparedHarnesses
	if err := h3.Run(); err != nil {
		t.Fatalf("handler.Run (exec2): %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output3.String()), "\n")
	var exec2Resp Response
	json.Unmarshal([]byte(lines[0]), &exec2Resp)
	if exec2Resp.Status != "execute" {
		t.Fatalf("exec2 (after recovery) status = %q (message: %s)", exec2Resp.Status, exec2Resp.Message)
	}

	_ = exec1
	_ = shutdownReq
}

func TestPrepareStaleMocksInvalidatesOldHarness(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	// Prepare with no mocks → prepare_id_1.
	prep1 := reqJSON(1, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	resp1 := sendRecv(t, prep1)
	if resp1.Status != "prepare" {
		t.Fatalf("prepare 1 status = %q (message: %s)", resp1.Status, resp1.Message)
	}
	id1 := resp1.PrepareID

	// Prepare with a mock → prepare_id_2 (different).
	prep2 := reqJSON(2, "prepare", fmt.Sprintf(
		`"file":"%s","function":"add","mocks":[{"symbol":"someFunc"}]`, tmp))

	// Run both prepares in a conversation so the handler sees the stale target.
	responses := conversation(t, prep1, prep2)
	if len(responses) != 2 {
		t.Fatalf("expected 2 responses, got %d", len(responses))
	}
	if responses[0].Status != "prepare" || responses[1].Status != "prepare" {
		t.Fatalf("expected both prepare, got %q and %q", responses[0].Status, responses[1].Status)
	}
	id2 := responses[1].PrepareID

	if id1 == id2 {
		t.Errorf("different mock configs must produce different prepare_ids: %s == %s", id1, id2)
	}

	// Execute with the new prepare_id succeeds.
	exec2 := reqJSON(3, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[1,2],"mocks":[{"symbol":"someFunc"}],"prepare_id":%q`, tmp, id2))
	responses2 := conversation(t, prep1, prep2, exec2)
	if len(responses2) != 3 {
		t.Fatalf("expected 3 responses, got %d", len(responses2))
	}
	if responses2[2].Status != "execute" {
		t.Fatalf("execute with new prepare_id: status = %q (message: %s)", responses2[2].Status, responses2[2].Message)
	}
}

func TestExecuteWithStalePrepareIdFallsThrough(t *testing.T) {
	tmp := filepath.Join(t.TempDir(), "target.go")
	if err := os.WriteFile(tmp, []byte(simpleGoSource()), 0644); err != nil {
		t.Fatal(err)
	}

	// Prepare, then execute with a bogus prepare_id — should fall through to one-shot.
	prep := reqJSON(1, "prepare", fmt.Sprintf(`"file":"%s","function":"add","mocks":[]`, tmp))
	exec := reqJSON(2, "execute", fmt.Sprintf(
		`"file":"%s","function":"add","inputs":[3,4],"mocks":[],"prepare_id":"deadbeefcafe0000"`, tmp))

	responses := conversation(t, prep, exec)
	if len(responses) != 2 {
		t.Fatalf("expected 2 responses, got %d", len(responses))
	}
	if responses[0].Status != "prepare" {
		t.Fatalf("prepare status = %q (message: %s)", responses[0].Status, responses[0].Message)
	}
	// Stale prepare_id should NOT error — should fall through to one-shot execution.
	if responses[1].Status != "execute" {
		t.Fatalf("execute with stale prepare_id: status = %q, want execute (message: %s)", responses[1].Status, responses[1].Message)
	}
}

func TestPruneOrphansRemovesStaleEntries(t *testing.T) {
	// Create a handler with a harness registered for a non-existent source file.
	h := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	artifactDir := t.TempDir()
	fakeFile := filepath.Join(t.TempDir(), "deleted.go")
	prepareID := "orphan-id"
	targetKey := fakeFile + "\x00" + "MyFunc"

	h.preparedHarnesses[prepareID] = &fakePreparedExecution{ArtifactDir: artifactDir}
	h.preparedTargets[targetKey] = prepareID

	pruned := h.pruneOrphans()
	if pruned != 1 {
		t.Errorf("pruneOrphans returned %d, want 1", pruned)
	}
	if len(h.preparedHarnesses) != 0 {
		t.Errorf("preparedHarnesses should be empty after pruning, len = %d", len(h.preparedHarnesses))
	}
	if len(h.preparedTargets) != 0 {
		t.Errorf("preparedTargets should be empty after pruning, len = %d", len(h.preparedTargets))
	}
	// Artifact dir should be cleaned up.
	if _, err := os.Stat(artifactDir); !os.IsNotExist(err) {
		t.Errorf("artifact dir should be removed on prune, os.Stat error = %v", err)
	}
}

func TestPruneOrphansKeepsValidEntries(t *testing.T) {
	// Create a handler with a harness registered for an existing source file.
	h := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	artifactDir := t.TempDir()
	// Use a real existing file so it won't be pruned.
	realFile := filepath.Join(t.TempDir(), "exists.go")
	if err := os.WriteFile(realFile, []byte("package main"), 0o644); err != nil {
		t.Fatal(err)
	}
	prepareID := "valid-id"
	targetKey := realFile + "\x00" + "MyFunc"

	h.preparedHarnesses[prepareID] = &fakePreparedExecution{ArtifactDir: artifactDir}
	h.preparedTargets[targetKey] = prepareID

	pruned := h.pruneOrphans()
	if pruned != 0 {
		t.Errorf("pruneOrphans returned %d, want 0", pruned)
	}
	if len(h.preparedHarnesses) != 1 {
		t.Errorf("preparedHarnesses should still have 1 entry, len = %d", len(h.preparedHarnesses))
	}
}

func TestPruneOrphansIsIdempotent(t *testing.T) {
	h := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	fakeFile := filepath.Join(t.TempDir(), "gone.go")
	prepareID := "orphan-id"
	targetKey := fakeFile + "\x00" + "Foo"

	h.preparedHarnesses[prepareID] = &fakePreparedExecution{ArtifactDir: t.TempDir()}
	h.preparedTargets[targetKey] = prepareID

	first := h.pruneOrphans()
	second := h.pruneOrphans()
	if first != 1 {
		t.Errorf("first prune returned %d, want 1", first)
	}
	if second != 0 {
		t.Errorf("second prune returned %d, want 0 (idempotent)", second)
	}
}

func TestShutdownPrunesOrphansBeforeCleanup(t *testing.T) {
	// Register a harness for a deleted source file; shutdown should not panic.
	artifactDir := t.TempDir()
	fakeFile := filepath.Join(t.TempDir(), "deleted.go")
	targetKey := fakeFile + "\x00" + "Foo"

	var output bytes.Buffer
	h := NewHandler(strings.NewReader(reqJSON(1, "shutdown")+"\n"), &output, io.Discard)
	h.preparedHarnesses["orphan-id"] = &fakePreparedExecution{ArtifactDir: artifactDir}
	h.preparedTargets[targetKey] = "orphan-id"

	if err := h.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}

	var resp Response
	if err := json.Unmarshal([]byte(strings.TrimSpace(output.String())), &resp); err != nil {
		t.Fatalf("unmarshal response: %v", err)
	}
	if resp.Status != "shutdown_ack" {
		t.Errorf("status = %q, want shutdown_ack", resp.Status)
	}
	if len(h.preparedHarnesses) != 0 {
		t.Errorf("preparedHarnesses should be empty, len = %d", len(h.preparedHarnesses))
	}
}

func TestLookupPreparedHarnessPrunesInvalid(t *testing.T) {
	// A harness with a deleted artifact dir should be pruned on lookup.
	h := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	artifactDir := t.TempDir()
	realFile := filepath.Join(t.TempDir(), "source.go")
	if err := os.WriteFile(realFile, []byte("package main"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Compute the real prepare_id so lookupPreparedHarness finds the entry.
	prepareID := computePrepareID(realFile, "Foo", nil, "")

	// Register and then delete the artifact dir.
	h.preparedHarnesses[prepareID] = &fakePreparedExecution{
		ArtifactDir: artifactDir,
		BinaryPath:  filepath.Join(artifactDir, "binary"),
	}
	h.preparedTargets[realFile+"\x00"+"Foo"+"\x00"+""] = prepareID
	os.RemoveAll(artifactDir)

	result := h.lookupPreparedHarness(realFile, "Foo", nil, "")
	if result != nil {
		t.Error("lookupPreparedHarness should return nil for invalid harness")
	}
	if len(h.preparedHarnesses) != 0 {
		t.Errorf("invalid harness should be pruned from map, len = %d", len(h.preparedHarnesses))
	}
}

// TestConvertExternalCallsNilArgs verifies that convertExternalCalls emits
// "args":[] (not null) when the executor's Args field is nil. Rust's
// Vec<serde_json::Value> cannot deserialize null, so nil slices cause
// "missing field `args`" deserialization failures (str-iqnk).
func TestConvertExternalCallsNilArgs(t *testing.T) {
	calls := []instrument.ExternalCall{
		{Symbol: "fmt.Println", Args: nil, ReturnValue: nil},
	}
	result := convertExternalCalls(calls)
	if len(result) != 1 {
		t.Fatalf("expected 1 call, got %d", len(result))
	}

	data, err := json.Marshal(result[0])
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	jsonStr := string(data)

	// args must be [] not null
	if strings.Contains(jsonStr, `"args":null`) {
		t.Errorf("args must not be null in JSON, got: %s", jsonStr)
	}
	if !strings.Contains(jsonStr, `"args":[]`) {
		t.Errorf("args must be empty array [], got: %s", jsonStr)
	}
}

// --- Regression: _test.go files must be rejected, not crash the frontend ---

func TestAnalyzeTestFileReturnsNotSupported(t *testing.T) {
	file := testdataPath("sample_test.go")
	req := reqJSON(2, "analyze", fmt.Sprintf(`"file":"%s"`, file))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrNotSupported {
		t.Errorf("code = %q, want %s", resp.Code, ErrNotSupported)
	}
	if !strings.Contains(resp.Message, "_test.go") {
		t.Errorf("message should mention _test.go, got %q", resp.Message)
	}
}

func TestInstrumentTestFileReturnsNotSupported(t *testing.T) {
	file := testdataPath("sample_test.go")
	req := reqJSON(2, "instrument", fmt.Sprintf(`"file":"%s"`, file))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrNotSupported {
		t.Errorf("code = %q, want %s", resp.Code, ErrNotSupported)
	}
}

func TestExecuteTestFileReturnsNotSupported(t *testing.T) {
	file := testdataPath("sample_test.go")
	fn := "TestAdd"
	req := reqJSON(2, "execute", fmt.Sprintf(`"file":"%s","function":"%s","inputs":[]`, file, fn))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrNotSupported {
		t.Errorf("code = %q, want %s", resp.Code, ErrNotSupported)
	}
}

func TestPrepareTestFileReturnsNotSupported(t *testing.T) {
	file := testdataPath("sample_test.go")
	fn := "TestAdd"
	req := reqJSON(2, "prepare", fmt.Sprintf(`"file":"%s","function":"%s"`, file, fn))
	resp := sendRecv(t, req)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != ErrNotSupported {
		t.Errorf("code = %q, want %s", resp.Code, ErrNotSupported)
	}
}
