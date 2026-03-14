# str-ws3k: Go malformed JSON response

## Context

The Go frontend's `Handler.Run()` (handler.go:71-73) silently drops malformed JSON requests — it logs and `continue`s without emitting a response. This leaves the caller hanging until timeout. The TS frontend correctly returns an `invalid_request` error response with `id: 0` for the same case (handlers.ts:397-404).

## Fix

### 1. Add failing test in `shatter-go/protocol/handler_test.go`

Test `TestMalformedJSON`: send malformed JSON (e.g., `"not valid json"`) via `sendRecv()` and assert the response has `status: "error"`, `code: ErrInvalidRequest`, and a message containing the parse error. Use `id: 0` since we can't extract an ID from invalid JSON.

**Note:** `sendRecv` currently calls `t.Fatal("no response received")` when there's no output — this is how the test will fail before the fix.

### 2. Fix `Handler.Run()` in `shatter-go/protocol/handler.go`

Replace lines 71-73 (the `continue` on unmarshal error) with:
- Build an error `Response` with `ProtocolVersion`, `ID: 0`, `Status: "error"`, `Code: ErrInvalidRequest`, `Message` including the parse error
- Call `h.send(resp)` and return on send error
- Continue the loop (don't shutdown)

This matches the TS frontend behavior.

### 3. Verify

- `go test ./...` in `shatter-go/`
- `go vet ./...` in `shatter-go/`

## Files to modify

- `shatter-go/protocol/handler.go` — lines 71-73, change `continue` to emit error response
- `shatter-go/protocol/handler_test.go` — add `TestMalformedJSON` test
