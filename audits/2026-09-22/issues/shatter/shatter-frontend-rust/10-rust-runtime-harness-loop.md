---
slug: rust-runtime-harness-loop
kind: new
title: "shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost"
priority: P3
type: bug
labels: [rust-frontend, runtime, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost

## Problem

1. **Malformed requests become empty inputs.** The runtime harness loop parses each request line with `serde_json::from_str(line).unwrap_or_default()`. An unparseable request turns into `Value::Null`, the target runs with empty inputs, and the result is a misleading `input 0 deserialization failed` instead of a protocol error.
2. **Unescaped fallback JSON.** `flush_results`' serialization-error fallback interpolates the error text into a JSON string with `format!` and does not escape it. An error message containing `"` or `\` produces invalid JSON.
3. **Thread-local branch tracking.** Branch/coverage state is `thread_local!`, so branches executed on threads spawned by user code (`std::thread::spawn`, rayon, `tokio::task::spawn_blocking`) are silently dropped. Closed str-dfnu2 / str-oc67 fixed only tokio cross-worker awaits (current-thread runtime). Neither `shatter-rust/CLAUDE.md` nor the parity matrix documents the remaining limitation.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust-runtime/src/lib.rs:558` and `:612`: `let req: Value = serde_json::from_str(line).unwrap_or_default();`, followed by `req["inputs"].as_array().cloned().unwrap_or_default()` (`:559`, `:614`). Similar `unwrap_or_default()` parsing at `:229`, `:256` (args) and `:358` (mocks).
- `lib.rs:340-345` (`flush_results`, defined at `:318`): `format!(r#"{{..."message":"{}"...}}"#, e)`, with no escaping.
- `lib.rs:167`: `thread_local! {` holds STATE.

## Acceptance criteria

- [ ] An unparseable request line (and a request missing `inputs`) produces an explicit protocol-error execute result that names the parse error. The target is not invoked. Unit test for both cases.
- [ ] The `flush_results` fallback is built with `serde_json::json!` (or equivalent). Unit test with an error message containing `"` and `\` asserts the output parses.
- [ ] The args/mocks `unwrap_or_default()` sites either get the same explicit error or carry a comment explaining why a default is correct.
- [ ] Thread limitation: either document it in `shatter-rust/CLAUDE.md` and `protocol/parity-matrix.yaml` (then run `task parity`), or implement a process-global recorder keyed by execution id, with a test where a branch on a `std::thread::spawn` thread is recorded.
- [ ] `cargo test -p shatter-rust-runtime` and `cargo test --test e2e_concolic_rust` pass.

## Suggested approach

Decode into a typed `struct Request { inputs: Vec<Value> }` with `serde_json::from_str::<Request>` and match the error into an `ExecuteResult` with `thrown_error.error_type = "protocol_error"`. Documenting the thread limitation is the cheap first step. A global recorder needs care with concurrent executions and should only be done if a real target needs it.

## Out of scope

- The crate-bridge stdout channel (`rust-crate-bridge-stdout`).
- Async runtime flavor changes.

## Size

S

## References

- Finding frontend-rust-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/65-rust-runtime-harness-loop.md`.
- Related: str-dfnu2, str-oc67 (closed; tokio cross-worker fix).
