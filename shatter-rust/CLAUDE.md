# shatter-rust

Rust language frontend for Shatter. Standalone binary implementing the JSON-over-stdio protocol.

## Architecture

- `src/main.rs` — Entry point: creates Handler, calls `run()`, prints fatal errors to stderr
- `src/protocol.rs` — Protocol types (Request/Response) matching the JSON wire format
- `src/handler.rs` — Protocol handler: read lines, parse JSON, dispatch, write response

Does **not** depend on `shatter-core` — defines its own protocol types that produce compatible JSON.

Commands: `handshake`, `analyze`, `instrument`, `execute`, `setup`, `teardown`, `generate`, `shutdown`

## Side Effect Parity Contract

Rust captures 3 of the 7 canonical side effect kinds. Capture is implemented in the generated harness code (standalone + dispatch modes). Crate-bridge harness captures thrown_error and global_state_change but not console_output (cannot inject libc dep into user crate).

| Kind | Captured? | Source | Notes |
|---|---|---|---|
| `console_output` | Yes | fd redirection (libc dup/dup2) in standalone/dispatch harness | stdout → level "log", stderr → level "error"; max 4096 chars/message; crate-bridge skips |
| `global_state_change` | Yes | mutable static variable snapshots (serde before/after) | Tracks `static mut` variables with Serialize derive |
| `thrown_error` | Yes | `catch_unwind` in harness | Captures error_type "runtime_error", message, stack: null |
| `global_mutation` | No | — | Not yet implemented |
| `file_write` | No | — | Not yet intercepted |
| `network_request` | No | — | Not yet intercepted |
| `environment_read` | No | — | Not yet intercepted |

See `protocol/parity-matrix.yaml` `allowed_divergences: rust-side-effects-not-captured` for tracking (status: resolved).

## Prepare Parity Contract

Rust implements the `prepare` command. It pre-builds the harness binary so subsequent execute calls skip compilation.

| Aspect | Detail |
|---|---|
| Handler | `handle_prepare()` in `src/handler.rs` |
| Advertised | Yes — `"prepare"` in capabilities list |
| prepare_id | SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars (`compute_prepare_id` in `executor.rs`) |
| Storage | `handler.prepared_harnesses: HashMap<String, PreparedHarnessInfo>` |
| Idempotent | Yes — returns existing prepare_id if already prepared |
| Prerequisite | Source file must exist; function must be analyzable |
| Cleanup | `prepared_harnesses.clear()` on function-level teardown + shutdown |

## Timeout Contract

Execution timeout: 5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `exec_timeout_from_env()` in `src/handler.rs`. Currently stored but not applied (execute is unimplemented).
