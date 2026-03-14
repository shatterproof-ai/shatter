# str-fpgb.11: Rust invalid JSON response

## Context
The Rust frontend silently drops malformed JSON requests — it logs the error but `continue`s without sending a protocol error response. The Go and TS frontends correctly emit an `invalid_request` error response. This causes the core engine to hang waiting for a response that never comes.

## Changes

**File:** `shatter-rust/src/handler.rs`

### 1. Add failing test (lines ~582+)
Add `malformed_json_returns_invalid_request` test using the `send_recv` helper with input like `"not valid json"`. Expect:
- `resp.status == "error"`
- `resp.code == Some("invalid_request")`
- `resp.id == 0` (can't parse an ID from garbage)
- `resp.message` contains "Invalid JSON"

Also add a test with multiple lines where malformed JSON is followed by a valid shutdown, to verify the handler continues processing after the error (doesn't abort).

### 2. Fix the `run()` method (lines 146-151)
Replace the `continue` in the JSON parse error branch with constructing and sending an error `Response`:

```rust
Err(e) => {
    self.logf(&format!("Failed to parse request: {e}"));
    let err_resp = Response {
        protocol_version: PROTOCOL_VERSION.to_string(),
        id: 0,
        status: "error".to_string(),
        code: Some("invalid_request".to_string()),
        message: Some(format!("Invalid JSON: {e}")),
        ..Response::default()
    };
    self.send(&err_resp)?;
    continue;
}
```

Key details:
- `id: 0` because we can't extract the request ID from unparseable JSON (matches Go frontend behavior)
- Uses `Response::base(0)` or default — need to check if `Response` has `Default`. If not, use `Response::base(0)` and set fields.
- Error message format: `"Invalid JSON: {e}"` matches Go's `fmt.Sprintf("Invalid JSON: %s", err.Error())`

### 3. Check Response construction
Need to verify `Response::base()` or use the existing pattern. From line 165: `Response::base(req.id)` exists, so use `Response::base(0)`.

## Verification
1. `cargo test -p shatter-rust` — all tests pass
2. `cargo clippy -p shatter-rust -- -D warnings` — no warnings
3. No E2E or walkthrough needed (this is a Rust frontend-only fix, doesn't affect the analyze→solve pipeline)
