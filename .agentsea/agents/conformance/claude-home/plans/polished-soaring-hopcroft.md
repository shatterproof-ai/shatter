# str-fpgb.10: Rust teardown_ack parity

## Context
The Rust frontend (`shatter-rust`) returns `"teardown"` as the response status for teardown commands, while the protocol spec and both other frontends (TS, Go) use `"teardown_ack"`. Tests encode the wrong behavior.

## Changes

### 1. Add failing test (handler.rs)
Add `teardown_returns_teardown_ack_status` test asserting `resp.status == "teardown_ack"`. Run to confirm it fails.

### 2. Fix handler (handler.rs:465)
Change `resp.status = "teardown".to_string()` → `resp.status = "teardown_ack".to_string()`

### 3. Update existing tests (handler.rs)
- Line 928: `assert_eq!(resp.status, "teardown")` → `"teardown_ack"`
- Line 992: `assert_eq!(responses[2].status, "teardown")` → `"teardown_ack"`

### Files
- `shatter-rust/src/handler.rs` — fix + tests

### Verification
1. `cargo test -p shatter-rust` — all pass
2. `cargo clippy -p shatter-rust -- -D warnings` — clean
