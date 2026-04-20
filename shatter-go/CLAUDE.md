# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Scope Limits

See [`docs/go-frontend-scope-limits.md`](../docs/go-frontend-scope-limits.md) for what the Go frontend does and does not analyze, and which limits are deferred vs. permanent.

## Key Files

- `protocol/handler.go` — Protocol handler, uses `log/slog` for `[shatter-go]` prefixed stderr logging
- `protocol/log.go` — slog configuration: `LevelTrace` constant, `prefixHandler` for `[shatter-go]` format
- `instrument/executor.go` — Function execution and instrumentation

## Property-Based Testing (rapid + native fuzzing)

Two complementary approaches:

- **rapid** (`protocol/property_test.go`, `instrument/property_test.go`): semantic property tests — roundtrips, behavioral invariants, idempotency. Use for logical properties.
- **Native fuzzing** (`testing.F` in `*_fuzz_test.go`): byte-level mutation for crash/panic discovery at parsing boundaries. Use for any code that deserializes untrusted input.

When adding a protocol message type or parsing function, add both. Seed corpus from existing test fixtures.

## Ite SymExpr Parity Contract

Go can deserialize `ite` SymExpr nodes but does not produce them — Go lacks data flow tracking; `exprToSymExpr` only resolves function parameters. Adding SSA phi-node merging is a separate effort. See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

## Loop Snapshot Parity Contract

Go includes `loop_body_states` in protocol structs for execute-response round-tripping, but does not emit the field yet. TypeScript is currently the only frontend that produces loop snapshots for supported counted loops. This drift is tracked in `protocol/parity-matrix.yaml` as `loop-body-states-typescript-only`.

## Side Effect Parity Contract

Go captures 2 of 7 canonical kinds. Both `instrument.SideEffect` (in `executor.go`) and `protocol.SideEffect` (in `protocol/types.go`) carry fields for all 7; only capture logic is missing.

Captured: `console_output` (stdout/stderr buffers in `executor.go`, stdout→"log" stderr→"error", no per-message truncation), `global_state_change` (pre/post diff of exported package-level variables). Not captured: `thrown_error` (panics handled internally — see `go-side-effects-partial`), `global_mutation`, `file_write`, `network_request`, `environment_read`.

`convertSideEffects()` in `protocol/handler.go` maps all 7 fields, so adding a new capture kind only requires populating the struct in `executor.go`.

Authoritative matrix: `protocol/parity-matrix.yaml` `side_effect_capabilities` and `allowed_divergences: go-side-effects-partial`.

## Prepare Parity Contract

Go implements `prepare` to pre-build the instrumented harness binary so subsequent execute calls skip `go build`. Handler: `handlePrepare()` in `protocol/handler.go`. Advertised in `CommandCapabilities` (`protocol/constants.go`). `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars (`computePrepareID`). Storage: `handler.preparedHarnesses map[string]*instrument.PreparedHarness`. Idempotent. `generateHarnessTemplate` generates code that reads `shatter_inputs.json` at runtime. `handleTeardown` (level=function) + `handleShutdown` call `Cleanup()` on all harnesses.

## Invocation Model Parity Contract

Go has the adapter substrate (registry, dispatch, invocation hooks) and two concrete adapters: `go/http-handler` for net/http handler functions and `go/gin` for Gin handler functions. The substrate mirrors TS: `ChooseInvocationStrategy` dispatches to direct/adapter/unsupported; `ResolveRuntimeHooks` resolves an `ExecutionProfile` against registered `RuntimeHookFactory` instances; `ExecuteAdapterOwned` invokes an `InvocationHook` and returns an `instrument.ExecuteResult` with empty instrumentation fields.

### go/http-handler adapter

Recognizes functions with signature `func(http.ResponseWriter, *http.Request)` (including method receivers and unnamed params). Detection uses Go's type checker, not string matching. When recognized, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/http-handler"}` with 4 synthetic params (method, path, headers, body). At execute time, the adapter compiles a harness that runs the handler against `httptest.NewRequest`/`httptest.NewRecorder` and returns the HTTP response (status, headers, body) as `return_value`. Instrumentation fields (branch_path, lines_executed) are empty for adapter-owned calls.

Key files: `protocol/nethttp_recognizer.go` (detection), `protocol/nethttp_adapter.go` (factory/hook), `instrument/http_harness.go` (harness generation/execution).

### Adapter hint recognizers

`protocol/recognizer.go` adds hint-based recognizers that run as post-processing in `AnalyzeFileWithTiming` and emit `AdapterHint` values on `FunctionAnalysis.AdapterHints`. These complement the per-function `recognizeHTTPHandler` (which sets `InvocationModel` directly for exact matches) by also detecting partial matches and Gin handlers:

- **net/http**: Detects `ResponseWriter`+`*Request` params (high confidence) and partial matches like `ResponseWriter`-only or `ServeHTTP` methods (medium/high). Uses `go/http-handler` adapter ID.
- **Gin**: Detects `*gin.Context` params (high confidence) and characteristic API calls (`c.JSON`, `c.Param`, etc.) via AST fallback since the type checker cannot resolve third-party imports. Uses `go/gin` adapter ID.

High-confidence hints auto-promote to `InvocationModel` (with `SyntheticParams` resolved via `syntheticParamsForAdapter()` in `analyzer.go`) when not already set by the per-function recognizer.

### go/gin adapter

Recognizes functions with `*gin.Context` parameter via hint-based AST detection (type checker cannot resolve third-party imports). When recognized with high confidence, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/gin"}` with 5 synthetic params (method, path, headers, body, route_params). At execute time, the adapter compiles a harness that runs the handler against `gin.CreateTestContext(httptest.NewRecorder())` with `gin.SetMode(gin.TestMode)`, sets `c.Request` and `c.Params` from inputs, and returns the HTTP response (status, headers, body) as `return_value`. Route parameters are injected directly via `c.Params` (bypassing Gin's router). Instrumentation fields are empty for adapter-owned calls.

Key files: `protocol/recognizer.go` (detection), `protocol/gin_adapter.go` (factory/hook), `instrument/gin_harness.go` (harness generation/execution).

The handler caches analyses from `handleAnalyze` and reads `invocation_model` in `handleExecute` to dispatch. Cache is cleared on function-level teardown and shutdown.

Key files: `protocol/adapter.go` (types, pure functions), `protocol/handler.go` (integration).

## Feature Capability Parity

Go declares support for all four redesign feature capabilities in
`protocol/parity-matrix.yaml` `feature_capabilities`:

- `outcome` — standardized invocation-outcome wire shape (str-hy9b.A1).
- `invocation_plan` — planner artifact schema (str-hy9b.E1). Go-only at this stage.
- `adapter_http_nethttp` — net/http handler adapter, ID `go/http-handler` (str-hy9b.G1). See the Invocation Model Parity Contract above. Go-specific.
- `hint_config_v1` — `.shatter/config.yaml` hint schema (str-hy9b.G3). Go-only.

TS and Rust currently declare `outcome` only; conformance tests
(`npx task conformance`) enforce that the Go-only capabilities return a
clean "capability not supported" response from TS/Rust rather than
crashing or returning malformed data.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.

## Invocation Outcome Contract (str-hy9b.A2)

Every execute response carries an `InvocationOutcome` under `response.outcome`. The status is one of `completed`, `build_failed`, `runtime_failed`, `timed_out`, or `unsupported` (the last for a function not present in the source). Non-completed statuses always carry a non-empty one-sentence `short_reason`; `build_failed` and `runtime_failed` also carry a `thrown_error` with compiler diagnostics or a panic trace. Classification lives in `failureOutcome()` (host-level errors) and `outcomeFromResult()` (harness-captured runtime state) in `protocol/handler.go`. Legacy `response.code` / `response.message` fields are preserved on error paths for backwards compatibility.

## Workspace GOCACHE Binding (str-hy9b.B2)

Every `go build` invoked from shatter-go pins `GOCACHE` to `<workspace>/cache/build` via `Workspace.GoEnv()`. Wiring lives in `instrument.applyGoBuildEnv` (for `instrument/` build sites) and `instrument.WorkspaceGoEnv()` (consumed by `setup/loader.go`). The handler installs the provider from its workspace handle in `newHandler()`; tests that construct a handler without a workspace fall back to the legacy `SHATTER_HARNESS_CACHE`-based cache hierarchy.
