# shatter-rust

Rust language frontend. Standalone binary implementing the JSON-over-stdio protocol.

## Architecture

- `src/main.rs` — Entry point: creates Handler, calls `run()`, prints fatal errors to stderr
- `src/protocol.rs` — Protocol types (Request/Response) matching the JSON wire format
- `src/handler.rs` — Protocol handler: read lines, parse JSON, dispatch, write response

Does **not** depend on `shatter-core` — defines its own protocol types that produce compatible JSON.

Commands: `handshake`, `analyze`, `instrument`, `execute`, `setup`, `teardown`, `generate`, `shutdown`.

## Ite SymExpr Parity Contract

Can deserialize `ite` SymExpr nodes (`SymExpr::Ite` in `protocol.rs`) but does not produce them. The analyze handler is a stub and does not perform data flow tracking. See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

## Side Effect Parity Contract

Rust captures 3 of 7 canonical kinds. Capture lives in the generated harness code (standalone + dispatch modes). Crate-bridge harness captures `thrown_error` and `global_state_change` but not `console_output` (cannot inject libc dep into the user crate).

Captured: `console_output` (fd redirection via libc dup/dup2 in standalone/dispatch harness, stdout→"log" stderr→"error", max 4096 chars/message, crate-bridge skips), `global_state_change` (mutable static variable snapshots via serde, tracks `static mut` variables with `Serialize` derive), `thrown_error` (`catch_unwind` in harness, `error_type: "runtime_error"`, `stack: null`). Not captured: `global_mutation`, `file_write`, `network_request`, `environment_read`.

Authoritative matrix: `protocol/parity-matrix.yaml` `allowed_divergences: rust-side-effects-not-captured` (status: resolved).

## Prepare Parity Contract

Rust implements `prepare` to pre-build the harness binary so subsequent execute calls skip compilation. Handler: `handle_prepare()` in `src/handler.rs`. Advertised in capabilities list. `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars (`compute_prepare_id` in `executor.rs`). Storage: `handler.prepared_harnesses: HashMap<String, PreparedHarnessInfo>`. Idempotent. Source file must exist and function must be analyzable. `prepared_harnesses.clear()` on function-level teardown + shutdown.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `exec_timeout_from_env()` in `src/handler.rs`. Currently stored but not applied (execute is unimplemented).
