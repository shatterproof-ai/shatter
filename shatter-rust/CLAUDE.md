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

## Loop Snapshot Parity Contract

Rust includes `loop_body_states` in protocol structs for execute-response wire compatibility, but does not emit loop snapshots yet. TypeScript currently owns snapshot production for supported counted loops; tracked drift lives in `protocol/parity-matrix.yaml` as `loop-body-states-typescript-only`.

## Prepare Parity Contract

Rust implements `prepare` to pre-build the harness binary so subsequent execute calls skip compilation. Handler: `handle_prepare()` in `src/handler.rs`. Advertised in capabilities list. `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars (`compute_prepare_id` in `executor.rs`). Storage: `handler.prepared_harnesses: HashMap<String, PreparedHarnessInfo>`. Idempotent. Source file must exist and function must be analyzable. `prepared_harnesses.clear()` on function-level teardown + shutdown.

## Adapter Parity Contract

Rust implements the adapter substrate (str-t4uo.6.1) with recognizers (str-t4uo.6.2) and Tokio runtime adapter (str-t4uo.6.3).

- **Substrate infrastructure**: `AdapterRecognizer` trait, `AdapterRegistry` (pre-populated with builtins via `with_builtins()`), `InvocationStrategy` enum, `choose_invocation_strategy()`, `derive_invocation_model()`.
- **Adapter constants**: `rust/async-runtime`, `rust/async-tokio`, and `rust/framework/axum-handler` IDs defined. All three are in `SUPPORTED_ADAPTERS` and fully functional.
- **Recognizers**: `AsyncRuntimeRecognizer` (Medium, any async fn), `TokioRecognizer` (High, async + tokio evidence), `AxumHandlerRecognizer` (High, async + axum extractors).
- **Invocation model**: `InvocationModel::Direct` (default) or `InvocationModel::Adapter { adapter_id, synthetic_params, scenario_schema }`. Serializes to `{"kind":"direct"}` / `{"kind":"adapter",...}`.
- **Tokio runtime adapter**: `execute_adapter_owned()` for `rust/async-tokio` and `rust/async-runtime` delegates to `execute_function()`. The harness generators auto-detect `async fn` and wrap calls in `tokio::runtime::Runtime::new().unwrap().block_on(...)`. The harness Cargo.toml includes `tokio = { version = "1", features = ["full"] }` when any target function is async. Sync functions are unaffected.
- **Axum handler adapter**: `execute_adapter_owned()` for `rust/framework/axum-handler` classifies extractor parameters via `classify_axum_extractors()` and generates an Axum-specific harness via `generate_axum_harness()` in `executor.rs`. The harness builds a minimal `axum::Router`, mounts the handler, sends a synthetic `http::Request` via `tower::ServiceExt::oneshot`, and normalizes the HTTP response (status, headers, body) into `ExecuteResult`. Input format: `inputs[0]` is a JSON object with keys `method`, `path`, `query`, `body`, `headers`, `state`. Supported extractors: Json, Path, Query, State, Form, Extension, RawBody, RawQuery, Host, OriginalUri. Unsupported extractors (Multipart, TypedHeader, ConnectInfo, MatchedPath, NestedPath) cause `NonExecutable` error. The `execute_adapter_owned()` signature includes `analysis: Option<&FunctionAnalysis>` to pass extractor type info from cached analysis.
- **Execution boundary**: Timers, spawned tasks, and channels within the Tokio runtime are supported. The runtime is created per-invocation and dropped after `block_on` returns.
- **Wire compatibility**: adapter types (`ExecutionProfile`, `AdapterHint`, `InvocationModel`, etc.) serialize to JSON matching shatter-core equivalents.
- **Handler wiring**: `adapter_registry` + `cached_analyses` fields on Handler. Recognize runs in `handle_analyze`, strategy dispatch in `handle_execute`. Cache cleared on function-level teardown and shutdown.

Authoritative matrix: `protocol/parity-matrix.yaml`.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `exec_timeout_from_env()` in `src/handler.rs`. Currently stored but not applied (execute is unimplemented).
