// Package httpbody is the str-e41w E2E fixture: a handler-shaped function
// whose branches are gated on the decoded JSON request body. Reaching the
// deeper return codes requires the symbolic-body synthesis — the pre-e41w
// fixed empty-body request could only ever produce the parse-guard result.
//
// Known-answer branches:
//
//	-1 -> unreachable via the wrapper (stub auth headers are always set)
//	 0 -> body is not valid JSON (e.g. "not json")
//	 1 -> valid JSON, missing/empty "model" (e.g. "{}")
//	 2 -> model set, stream false      (e.g. `{"model":"m"}`)
//	 3 -> model set, stream true       (e.g. `{"model":"m","stream":true}`)
package httpbody

import (
	"encoding/json"
	"io"
	"net/http"
)

// ClassifyRequest mirrors the decode-guard shape of real LLM-provider
// handlers: auth presence check, body read, JSON parse, field validation,
// then a behavior branch on a body field.
func ClassifyRequest(r *http.Request) int {
	if r.Header.Get("x-api-key") == "" && r.Header.Get("Authorization") == "" {
		return -1
	}
	body, err := io.ReadAll(r.Body)
	if err != nil {
		return -2
	}
	var req struct {
		Model  string `json:"model"`
		Stream bool   `json:"stream"`
	}
	if err := json.Unmarshal(body, &req); err != nil {
		return 0
	}
	if req.Model == "" {
		return 1
	}
	if req.Stream {
		return 3
	}
	return 2
}
