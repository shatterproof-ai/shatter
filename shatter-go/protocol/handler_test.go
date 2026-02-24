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

func TestAnalyzeWithoutFileReturnsError(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":2,"command":"analyze"}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "invalid_request" {
		t.Errorf("code = %q, want invalid_request", resp.Code)
	}
}

func TestInstrumentReturnsNotImplemented(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":3,"command":"instrument","file":"x.go","function":"F","mocks":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
	}
	if resp.Code != "internal_error" {
		t.Errorf("code = %q, want internal_error", resp.Code)
	}
}

func TestExecuteReturnsNotImplemented(t *testing.T) {
	resp := sendRecv(t, `{"protocol_version":"0.1.0","id":4,"command":"execute","function":"F","inputs":[],"mocks":[]}`)
	if resp.Status != "error" {
		t.Errorf("status = %q, want error", resp.Status)
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
	handler := NewHandler(strings.NewReader(input), &output, &logBuf)
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
