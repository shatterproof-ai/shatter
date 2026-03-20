# shatter-rust

Rust language frontend for Shatter. Standalone binary implementing the JSON-over-stdio protocol.

## Architecture

- `src/main.rs` — Entry point: creates Handler, calls `run()`, prints fatal errors to stderr
- `src/protocol.rs` — Protocol types (Request/Response) matching the JSON wire format
- `src/handler.rs` — Protocol handler: read lines, parse JSON, dispatch, write response

Does **not** depend on `shatter-core` — defines its own protocol types that produce compatible JSON.

Commands: `handshake`, `analyze`, `instrument`, `execute`, `setup`, `teardown`, `generate`, `shutdown`

## Side Effect Parity Contract

The Rust frontend's execute handler is partial (see `rust-execute-partial` in `protocol/parity-matrix.yaml`). Side effects are not yet captured:

| Kind | Captured? | Notes |
|---|---|---|
| `console_output` | No | Execute partial; returns `not_supported` for many inputs |
| All other kinds | No | Not yet implemented |

When execute is fully implemented, `side_effects` in responses must use the canonical 7-kind wire format defined in `shatter-core/src/execution_record.rs`. The Rust frontend stores `side_effects: Option<Vec<serde_json::Value>>` in its response struct — this is intentionally untyped until execute is real.

See `protocol/parity-matrix.yaml` `allowed_divergences: rust-side-effects-not-captured` for tracking.

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `exec_timeout_from_env()` in `src/handler.rs`. Currently stored but not applied (execute is unimplemented).
