// Regression tests for str-jeen.52: every response written by the Go frontend
// must carry the same id as the request that produced it, even when the
// request triggers a frontend-side failure (parse error, file-not-found,
// invalid command, build/runtime failures surfaced through the analyzer).
// A response whose id does not align with the pending request shifts the
// JSON-over-stdio stream and surfaces on the core side as
//
//	frontend error: response id N does not match request id N+1
//
// which historically produced silent scan-attribution corruption (Zolem audit).
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
)

// TestResponseIDMatchesAcrossMixedSuccessAndFailure drives the handler
// with a sequence of sequential requests where roughly half trigger
// successful responses and half trigger error responses. The protocol
// invariant is that every response carries response.id == request.id, so
// that a downstream IdMismatch on the core can only arise from genuine
// transport corruption and never from frontend-side bookkeeping.
func TestResponseIDMatchesAcrossMixedSuccessAndFailure(t *testing.T) {
	tmpDir := t.TempDir()
	validFile := filepath.Join(tmpDir, "valid.go")
	source := "package main\n\nfunc Add(a, b int) int { return a + b }\n"
	if err := os.WriteFile(validFile, []byte(source), 0644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	missingFile := filepath.Join(tmpDir, "missing.go")

	// Pick a non-contiguous id sequence so the test catches an off-by-one
	// regression that happens to align with a small, sequential id space.
	requests := []struct {
		id      int
		request string
	}{
		{1001, reqJSON(1001, "handshake", `"capabilities":["analyze"]`)},
		{1002, reqJSON(1002, "analyze", fmt.Sprintf(`"file":"%s"`, validFile))},
		{1003, reqJSON(1003, "analyze", fmt.Sprintf(`"file":"%s"`, missingFile))},
		{1004, reqJSON(1004, "instrument", fmt.Sprintf(`"file":"%s"`, missingFile))},
		{1005, reqJSON(1005, "execute", `"function":"Missing"`)},
		{1006, reqJSON(1006, "analyze", fmt.Sprintf(`"file":"%s"`, validFile))},
		{1007, reqJSON(1007, "totally-bogus-command")},
		{1008, reqJSON(1008, "teardown", `"scope":"x","level":"function"`)},
		{1009, reqJSON(1009, "shutdown")},
	}

	lines := make([]string, len(requests))
	for i, r := range requests {
		lines[i] = r.request
	}

	responses := conversation(t, lines...)
	if len(responses) != len(requests) {
		t.Fatalf("got %d responses, want %d", len(responses), len(requests))
	}

	for i, resp := range responses {
		want := requests[i].id
		if resp.ID != want {
			t.Errorf("response[%d] id = %d, want %d (request: %s, status=%q, code=%q, message=%q)",
				i, resp.ID, want, requests[i].request, resp.Status, resp.Code, resp.Message)
		}
	}
}

// TestResponseIDRecoveredFromMalformedJSONLine exercises the malformed-JSON
// path: when json.Unmarshal of the incoming line fails outright, the handler
// must still attempt to recover the id from the raw bytes so the error
// response stays paired with the pending request. This is the defensive
// half of str-jeen.52 — any malformed line whose id is recoverable should
// not poison the stream for the next request.
func TestResponseIDRecoveredFromMalformedJSONLine(t *testing.T) {
	tests := []struct {
		name   string
		line   string
		wantID int
	}{
		{
			name:   "trailing garbage after valid id",
			line:   `{"id": 4242, "command": "analyze", garbage}`,
			wantID: 4242,
		},
		{
			name:   "id with no closing brace",
			line:   `{"id":7,"command":"analyze"`,
			wantID: 7,
		},
		{
			name:   "completely malformed line still falls back to 0",
			line:   `not json at all`,
			wantID: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			input := strings.NewReader(tt.line + "\n")
			var output bytes.Buffer
			handler := NewHandler(input, &output, io.Discard)
			if err := handler.Run(); err != nil {
				t.Fatalf("handler.Run: %v", err)
			}
			rawResponse := strings.TrimSpace(output.String())
			if rawResponse == "" {
				t.Fatal("handler did not emit a response")
			}
			var resp Response
			if err := json.Unmarshal([]byte(rawResponse), &resp); err != nil {
				t.Fatalf("unmarshal response: %v (raw: %s)", err, rawResponse)
			}
			if resp.ID != tt.wantID {
				t.Errorf("id = %d, want %d (raw: %s)", resp.ID, tt.wantID, rawResponse)
			}
			if resp.Status != "error" {
				t.Errorf("status = %q, want error", resp.Status)
			}
			if resp.Code != ErrInvalidRequest {
				t.Errorf("code = %q, want %q", resp.Code, ErrInvalidRequest)
			}
		})
	}
}

// TestResponseIDMatchesWithBuildFailureNeighbor pairs a frontend-side
// build failure (analyze of a target whose package has a type error) with
// a successful sibling analyze on the surrounding bookend requests. The
// fixture lives in testdata/response_id_match_target.go: WorkingAdd is
// buildable, BrokenSibling references an undeclared identifier. The
// handler's lenient analyze path may either return functions or surface
// an error — both outcomes are accepted here. The invariant under test is
// that every response.id matches its request.id regardless of which path
// fired.
func TestResponseIDMatchesWithBuildFailureNeighbor(t *testing.T) {
	target := filepath.Join("testdata", "response_id_match_target.go")
	if _, err := os.Stat(target); err != nil {
		t.Fatalf("fixture missing: %v", err)
	}
	absoluteTarget, err := filepath.Abs(target)
	if err != nil {
		t.Fatalf("abs: %v", err)
	}

	requests := []struct {
		id      int
		request string
	}{
		{2001, reqJSON(2001, "handshake", `"capabilities":["analyze"]`)},
		{2002, reqJSON(2002, "analyze", fmt.Sprintf(`"file":"%s","function":"WorkingAdd"`, absoluteTarget))},
		{2003, reqJSON(2003, "analyze", fmt.Sprintf(`"file":"%s","function":"BrokenSibling"`, absoluteTarget))},
		{2004, reqJSON(2004, "analyze", fmt.Sprintf(`"file":"%s","function":"WorkingAdd"`, absoluteTarget))},
		{2005, reqJSON(2005, "shutdown")},
	}

	lines := make([]string, len(requests))
	for i, r := range requests {
		lines[i] = r.request
	}

	responses := conversation(t, lines...)
	if len(responses) != len(requests) {
		t.Fatalf("got %d responses, want %d", len(responses), len(requests))
	}

	for i, resp := range responses {
		want := requests[i].id
		if resp.ID != want {
			t.Errorf("response[%d] id = %d, want %d (status=%q, code=%q, message=%q)",
				i, resp.ID, want, resp.Status, resp.Code, resp.Message)
		}
	}
}
