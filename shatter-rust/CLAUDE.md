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

## Feature Capability Parity

Rust declares support for `outcome` only in
`protocol/parity-matrix.yaml` `feature_capabilities` — the standardized
invocation-outcome wire shape reached cross-frontend parity in str-hy9b.A5.

The planner-surface capabilities (`invocation_plan`, `adapter_http_nethttp`,
`hint_config_v1`) are declared Go-only at this stage. Rust does not yet
implement them; conformance tests (`task conformance`) expect Rust to
return a clean "capability not supported" response rather than crashing
or returning malformed data when these are probed.

The Execute command's optional `plan` field (an `InvocationPlan` from
`get_invocation_plan`, added in str-hy9b.H5) is accepted on the wire but
ignored by the Rust frontend. This is a tracked divergence — see the
`ts-rust-execute-plan-not-implemented` entry in
`protocol/parity-matrix.yaml`. Rust callers that pass `plan` should
expect identical behavior to a request without `plan`; the field exists
so plan-aware callers can speak a single wire shape across frontends
without branching on language. Implementation is deferred until the
Rust frontend grows method-target invocation support.

## Outcome Emission Contract

Every `execute` response carries an `outcome: InvocationOutcome` field
(str-hy9b.A1/A5). The Rust frontend emits outcomes on both success and
error responses so cross-frontend consumers see a uniform invocation
envelope. Emission lives in `derive_execute_outcome` and `error_outcome`
in `src/handler.rs`, plumbed from `handle_execute`.

| Source path | `outcome.status` |
|---|---|
| `Ok(result)` with `result.thrown_error == None` | `completed` (carries `return_value`) |
| `Ok(result)` with `thrown_error.error_type == "timeout"` (set by the executor's `RecvTimeoutError::Timeout` arm) | `timed_out` |
| `Ok(result)` with any other thrown error | `runtime_failed` |
| `Err(CompilationFailed(_))` | `build_failed` |
| `Err(NonExecutable(_))` | `unsupported` |
| `Err(FileError(_))` and any other `Err(_)` | `runtime_failed` |

`completed_with_findings` and `skipped_by_policy` are reserved for
upstream consumers and are not produced here. The wire shape matches the
TS and Go frontends — see `protocol/parity-matrix.yaml`
`feature_capabilities.outcome` and the conformance lock in
`protocol/conformance/conformance_cases.yaml`
(`execute_outcome_shape_rust`).

### Environment Preflight Contract (str-jeen.40)

The `preflight_failed` error code (`ERR_PREFLIGHT_FAILED`) and the
matching `OutcomeStatus::PreflightFailed` variant are declared in
`src/protocol.rs` for wire compatibility with the TS frontend's
env-preflight short-circuit. **The Rust frontend does not currently emit
either** — Rust has no equivalent missing-toolchain / missing-target-dir
preflight pass yet. See `parity-matrix.yaml` allowed_divergence
`error-code-preflight-failed-typescript-only`. Adding a Rust preflight
emitter requires no core-side change because `batch_analyze` already
treats `preflight_failed` the same as `not_supported`.

## No-Target-Reason Classifier Contract

The Rust per-language no-target-reason classifier (str-jeen.24) refines
zero-target Rust files into one of `build_script`, `test_module`, or
`declaration_only`. Files that don't match any Rust-specific signal fall
through to `unclassified`.

**The classifier lives CLI-side**, not in this crate. It is hosted in
`shatter-cli/src/commands/explore.rs` (`rust_classify_no_target_reason`
and helpers) following the str-jeen.25 frontend-agnostic pre-classifier
pattern. The frontend Analyze response wire shape is unchanged — the
protocol does not yet carry `no_target_reason` from frontend → CLI, so
emitting per-language classifications would require a protocol surface
change. When that protocol field is added, the classifier can move into
this crate without behavioral change for callers.

Order of checks (first match wins):

1. `build_script` — basename is exactly `build.rs` AND a sibling
   `Cargo.toml` exists. A `build.rs` deep in a fixtures tree without a
   sibling manifest does NOT classify.
2. `test_module` (path) — file under any `tests/` directory segment, or
   basename ending in `_test.rs` / `_tests.rs`.
3. `declaration_only` — content scan finds only `mod` / `use` /
   `pub use` / `pub mod` / `extern crate` declarations plus attributes
   and comments. Conservative: macro-heavy files (`include!`, inline
   `mod x { ... }`, `macro_rules!`) return `None` and the caller emits
   `unclassified` rather than risk mislabeling.
4. `test_module` (content fallback) — every non-attribute item sits
   under `#[cfg(test)]` or carries `#[test]`.

Authoritative matrix entry: `protocol/parity-matrix.yaml`
`shared_wire_types.no_target_reason.frontends.rust:
implemented_via_cli_classifier`.

## Rust E2E Gate

`shatter-core/tests/e2e_concolic_rust.rs` (str-o9rz) is the Rust counterpart
of `shatter-core/tests/e2e_concolic.rs`. It drives the real `shatter-rust`
subprocess through analyze → instrument → orchestrator-driven explore (with
Z3 input generation) against three known-answer Rust target programs:

- `arithmetic::classify_number` — i64 cascade, 4 branches; the n==0 branch
  exercises Z3 input generation.
- `self_hosting::classify_float` — division-by-zero guard followed by a
  ratio threshold; the canonical nested-control shape.
- `enums::classify_option` — `Option<i32>` with match guard; substitutes
  for the string-branch case while a borrowed-`&str` harness limitation
  blocks executable string-branch examples.

The test runs under `task check` (Full tier) via `core:test-ignored`. Run
it directly with:

```
SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py)" \
  cargo test --test e2e_concolic_rust -- --include-ignored
```

`orchestrator::explore()` works against the Rust frontend because Analyze
and Instrument set `last_file` (`src/handler.rs:552, 620`) and Execute
falls back to it when no `file` is provided (line 803). No protocol
extension is needed.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `exec_timeout_from_env()` in `src/handler.rs`. Currently stored but not applied (execute is unimplemented).
