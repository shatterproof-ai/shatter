# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Scope Limits

See [`docs/go-frontend-scope-limits.md`](../docs/go-frontend-scope-limits.md) for what the Go frontend does and does not analyze, and which limits are deferred vs. permanent.

### Vendor Resolution (str-nm5e)

`vendor/` directories are honored. The analyzer loads packages via `golang.org/x/tools/go/packages`, which delegates to `go list`; with `vendor/modules.txt` present and consistent with `go.mod`, the toolchain selects `-mod=vendor` automatically and vendored dependencies populate `pkg.TypesInfo`. No analyzer-side flag plumbing is required. Regression test: `shatter-go/protocol/vendor_test.go`.

### go.work Workspace Resolution (str-b66s)

`go.work` multi-module workspaces are honored. The same `go/packages` → `go list` path that handles vendor mode also picks up workspace mode: when a `go.work` file is present in the loader `Dir`'s ancestry (or `GOWORK` is set), cross-module imports between workspace `use` members resolve through `pkg.TypesInfo` without analyzer-side flag plumbing. Regression test: `shatter-go/protocol/goworkspace_test.go`.

### Build Tag Activation (str-jl9r)

Build tags configured via `GOFLAGS=-tags=tag1,tag2` (or via `GOOS` / `GOARCH`) are honored at both analyzer layers. The upfront `isBuildTagExcluded` guard (`protocol/build_tags.go`) builds its `go/build.Context` from `build.Default` extended with tags parsed from `GOFLAGS`, so a file gated on an active tag passes the guard instead of soft-skipping. The `go/packages` loader (`loader/loader.go`) forwards `os.Environ()` as `Env`, so `go list` reads `GOFLAGS=-tags=...` itself and the gated file appears in `pkg.Syntax`. Files gated on tags the user did **not** opt into still surface as `*BuildTagExcludedError` and `not_supported`, consumed by the Rust core's batch_analyze soft-skip path (str-8amu). No protocol field is added; the env-knob shape mirrors vendor and go.work precedent. Regression test: `shatter-go/protocol/build_tags_test.go`.

## Key Files

- `protocol/handler.go` — Protocol handler, uses `log/slog` for `[shatter-go]` prefixed stderr logging
- `protocol/log.go` — slog configuration: `LevelTrace` constant, `prefixHandler` for `[shatter-go]` format
- `protocol/prepared_launcher.go` — Direct execute/prepare path via launcher-backed cached programs
- `launcher/launcher.go` — Generated launcher module build/runtime bridge
- `instrument/executor.go` — Execution-side capture and legacy prepared harness support still exercised by tests

## Property-Based Testing (rapid + native fuzzing)

Two complementary approaches:

- **rapid** (`protocol/property_test.go`, `instrument/property_test.go`): semantic property tests — roundtrips, behavioral invariants, idempotency. Use for logical properties.
- **Native fuzzing** (`testing.F` in `*_fuzz_test.go`): byte-level mutation for crash/panic discovery at parsing boundaries. Use for any code that deserializes untrusted input.

When adding a protocol message type or parsing function, add both. Seed corpus from existing test fixtures.

## E2E Pipeline Gate (str-3op0)

`shatter-core/tests/e2e_concolic_go.rs` is the Go frontend's full-pipeline gate. It drives the real `shatter-go` subprocess through analyze → instrument → orchestrator-driven explore → Z3 solve against three known-answer Go targets covering distinct shapes:

- **Free function with branches** — `<examples>/standalone/go/01-arithmetic.go::ClassifyNumber` (4 branches).
- **Method with same-package constructor** — `examples/go/service-method/svc.go::(*Service).Compute` (planner-emitted plan attached via `ExploreConfig::default_execute_plan`).
- **Variadic helper** — `examples/go/variadic-sum/sum.go::SumThreshold` (exercises the launcher's variadic-wrapper path that str-jeen.48 fixed).

Each case asserts both expected branches discovered AND at least one triggering input per branch (modeled on the TS counterpart `shatter-core/tests/e2e_concolic.rs`). The tests are `#[ignore]`d so plain `cargo test` stays fast; `task check` (Full tier) runs them via `cargo test -p shatter-core -- --include-ignored`.

Run after any change to: the Go analyzer, instrumentor, launcher, wrapper generator, planner, prepared-launcher path, or any execute-response field the orchestrator consumes. Adding a new launcher / wrapper code path that the existing three shapes don't cover requires adding a fourth test case here before closing.

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

Go implements `prepare` to pre-build a launcher-backed execution binary so subsequent execute calls skip rebuilds. Handler: `handlePrepare()` in `protocol/handler.go`, with launcher preparation in `protocol/prepared_launcher.go`. Advertised in `CommandCapabilities` (`protocol/constants.go`). `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols:receiver_kind`, first 16 hex chars (`computePrepareID`). When a Prepare request carries an `InvocationPlan`, `plan.receiver_kind` is included in the key so different receiver strategies for the same target produce different IDs and don't collide in the harness cache (str-oegu). Plan-less Prepare requests use an empty `receiver_kind` (equivalent to pre-str-oegu behavior). Storage: `handler.preparedHarnesses map[string]preparedExecution`. Idempotent. `handleTeardown` (level=function) + `handleShutdown` call `Cleanup()` on cached prepared executions.

## Invocation Model Parity Contract

Go has the adapter substrate (registry, dispatch, invocation hooks) and two concrete adapters: `go/http-handler` for net/http handler functions and `go/gin` for Gin handler functions. The substrate mirrors TS: `ChooseInvocationStrategy` dispatches to direct/adapter/unsupported; `ResolveRuntimeHooks` resolves an `ExecutionProfile` against registered `RuntimeHookFactory` instances; `ExecuteAdapterOwned` invokes an `InvocationHook` and returns an `instrument.ExecuteResult` with empty instrumentation fields.

### go/http-handler adapter

Recognizes functions with signature `func(http.ResponseWriter, *http.Request)` (including method receivers and unnamed params). Detection uses Go's type checker, not string matching. When recognized, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/http-handler"}` with 4 synthetic params (method, path, headers, body). At execute time, the adapter builds a specialized launcher entrypoint that runs the handler against `httptest.NewRequest`/`httptest.NewRecorder` and returns the HTTP response (status, headers, body) as `return_value`. Instrumentation fields (branch_path, lines_executed) are empty for adapter-owned calls.

Key files: `protocol/nethttp_recognizer.go` (detection), `protocol/nethttp_adapter.go` (factory/hook), `protocol/adapter_launcher.go` (launcher generation/execution).

### Adapter hint recognizers

`protocol/recognizer.go` adds hint-based recognizers that run as post-processing in `AnalyzeFileWithTiming` and emit `AdapterHint` values on `FunctionAnalysis.AdapterHints`. These complement the per-function `recognizeHTTPHandler` (which sets `InvocationModel` directly for exact matches) by also detecting partial matches and Gin handlers:

- **net/http**: Detects `ResponseWriter`+`*Request` params (high confidence) and partial matches like `ResponseWriter`-only or `ServeHTTP` methods (medium/high). Uses `go/http-handler` adapter ID.
- **Gin**: Detects `*gin.Context` params (high confidence) and characteristic API calls (`c.JSON`, `c.Param`, etc.) via AST fallback since the type checker cannot resolve third-party imports. Uses `go/gin` adapter ID.

High-confidence hints auto-promote to `InvocationModel` (with `SyntheticParams` resolved via `syntheticParamsForAdapter()` in `analyzer.go`) when not already set by the per-function recognizer.

### go/gin adapter

Recognizes functions with `*gin.Context` parameter via hint-based AST detection (type checker cannot resolve third-party imports). When recognized with high confidence, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/gin"}` with 5 synthetic params (method, path, headers, body, route_params). At execute time, the adapter builds a specialized launcher entrypoint that runs the handler against `gin.CreateTestContext(httptest.NewRecorder())` with `gin.SetMode(gin.TestMode)`, sets `c.Request` and `c.Params` from inputs, and returns the HTTP response (status, headers, body) as `return_value`. Route parameters are injected directly via `c.Params` (bypassing Gin's router). Instrumentation fields are empty for adapter-owned calls.

Key files: `protocol/recognizer.go` (detection), `protocol/gin_adapter.go` (factory/hook), `protocol/adapter_launcher.go` (launcher generation/execution).

The handler caches analyses from `handleAnalyze` and reads `invocation_model` in `handleExecute` to dispatch. Cache is cleared on function-level teardown and shutdown.

Key files: `protocol/adapter.go` (types, pure functions), `protocol/handler.go` (integration).

## Feature Capability Parity

Go declares support for all four redesign feature capabilities in
`protocol/parity-matrix.yaml` `feature_capabilities`:

- `outcome` — standardized invocation-outcome wire shape (str-hy9b.A1).
- `invocation_plan` — planner artifact schema (str-hy9b.E1) and the receiver-aware planner pathway (str-hy9b.H5). Go-only at this stage. `planner.PlanRequirements` returns method-target plans with non-empty `receiver_kind` (e.g. `"constructor:NewService"`) and `argument_plans` per parameter; `Command::Execute.plan` (an optional InvocationPlan on Execute requests) is the bridge from the Rust core's `planner_consumer` into the Go launcher's wrapper-aware dispatch path. TS/Rust frontends accept `Execute.plan` on the wire (additive, omitempty) but ignore it — see `ts-rust-execute-plan-not-implemented` in `protocol/parity-matrix.yaml`. The `argument_plans[].kind` enum includes `runtime_value` (str-1hlk.4): Go emits it for parameters resolved from the runtime-value registry (e.g. `context.Context` → `context.Background()`), with `literal` carrying the JSON-encoded source expression and `type_hint` naming the registered Go type. The Rust core deserializes the kind but does not materialize a seed for it — the Go launcher resolves the value at execute time via `planner.LookupRuntimeValue`.
- `adapter_http_nethttp` — net/http handler adapter, ID `go/http-handler` (str-hy9b.G1). See the Invocation Model Parity Contract above. Go-specific.
- `hint_config_v1` — `.shatter/config.yaml` hint schema (str-hy9b.G3). Go-only.

TS and Rust currently declare `outcome` only; conformance tests
(`task conformance`) enforce that the Go-only capabilities return a
clean "capability not supported" response from TS/Rust rather than
crashing or returning malformed data.

## No-Target-Reason Classifier Contract

The Go per-language no-target-reason classifier (str-jeen.23) refines
zero-target Go files into one of `test_file`, `generated`, or
`receiver_method_gap`. Files that don't match any Go-specific signal
fall through to `unclassified` (the str-jeen.21 default).

**The classifier lives CLI-side**, not in this crate. It is hosted in
`shatter-cli/src/commands/explore.rs` (`go_classify_no_target_reason`
and helpers) following the str-jeen.25 frontend-agnostic pre-classifier
pattern and the str-jeen.22 / str-jeen.24 TS / Rust precedents. The
frontend Analyze response wire shape is unchanged — the protocol does
not yet carry `no_target_reason` from frontend → CLI, so emitting
per-language classifications would require a protocol surface change.
When that field is added, the classifier can move into this frontend
without behavioral change for callers.

Order of checks (first match wins):

1. `test_file` (path) — basename ends in `_test.go`. Go's testing
   convention is unambiguous regardless of file body.
2. `generated` (content) — file's pre-`package`-clause prologue
   contains a line matching the canonical Go marker
   `// Code generated ... DO NOT EDIT.` (per
   <https://pkg.go.dev/cmd/go#hdr-Generate_Go_files>). Markers buried
   inside function bodies or after the package clause are ignored.
3. `receiver_method_gap` (content) — file declares one or more
   methods (`func (recv Recv) Name(...)`) and zero free top-level
   functions (`func Name(...)`). Conservative: any free function
   present at depth zero rejects this classification — the analyzer
   should have produced a target — and the caller falls through to
   `unclassified`.

Authoritative matrix entry: `protocol/parity-matrix.yaml`
`shared_wire_types.no_target_reason.frontends.go:
implemented_via_cli_classifier`.

## Timeout Contract

5s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `execTimeout()` in `instrument/executor.go`.

## Environment Preflight Contract (str-jeen.40)

The `preflight_failed` error code (`ErrPreflightFailed`) and the matching `OutcomeStatusPreflightFailed` value are declared in `protocol/constants.go` / `protocol/types.go` for wire compatibility with the TS frontend's env-preflight short-circuit. **The Go frontend does not currently emit either** — Go has no equivalent missing-toolchain or missing-module-cache preflight pass yet. See `parity-matrix.yaml` allowed_divergence `error-code-preflight-failed-typescript-only`. Adding a Go preflight emitter (e.g. for missing `go.sum` / module cache) requires no core-side change because `batch_analyze` already treats `preflight_failed` the same as `not_supported`.

## Invocation Outcome Contract (str-hy9b.A2)

Every execute response carries an `InvocationOutcome` under `response.outcome`. The status is one of `completed`, `build_failed`, `runtime_failed`, `timed_out`, or `unsupported` (the last for a function not present in the source). Non-completed statuses always carry a non-empty one-sentence `short_reason`; `build_failed` and `runtime_failed` also carry a `thrown_error` with compiler diagnostics or a panic trace. Classification lives in `failureOutcome()` (host-level errors) and `outcomeFromResult()` (harness-captured runtime state) in `protocol/handler.go`. Legacy `response.code` / `response.message` fields are preserved on error paths for backwards compatibility.

**Method targets require a plan (str-hy9b.H5).** Method execution is dispatched through the launcher wrapper's receiver-kind switch, driven by `Command::Execute.plan.receiver_kind`. An Execute request that targets a method but does not carry a `plan` field falls into the wrapper's default case and surfaces a `runtime_failed` outcome whose `short_reason` (and `thrown_error.message`) contains `"unknown receiver kind"`. This is intentionally **not** the pre-H5 hard rejection: the C4 `unsupported` / `method_not_supported` outcome was retired so that pipeline behavior stays uniform regardless of plan presence — plan-aware callers (`planner_consumer` against the Go planner) get clean dispatch into a real constructor; plan-less callers get a uniform runtime failure rather than a special-cased capability rejection. Free-function targets are unaffected and still take the empty-receiver path. The `unsupported` / `method_not_supported` classification in `failureOutcome` is now reserved for host-level "receiver planning" errors that surface before the launcher runs (see the `receiver planning` arm), not for the launcher's own dispatch outcome.

## Safety Policy Contract (str-hy9b.G4)

Every direct `execute` is gated by a default safety policy before any harness runs. The gate classifies the target via its cached `FunctionAnalysis` (parameter types + declared external dependencies) into `SideEffectClass` values defined in `protocol/policy.go`: `pure`, `local_fs`, `network`, `subprocess`, `database`, `process_global`, `unknown_high`.

Default allow set: `pure`, `local_fs`. Any classification outside the allow set produces an `InvocationOutcome` with `status = skipped_by_policy` and a `short_reason` naming the offending class and component. No build, no harness, no side effects.

Per-target overrides live in `.shatter/config.yaml` parsed by `shatter-go/config/loader.go`:

```yaml
functions:
  "path/to/file.go:FuncName":
    policy:
      allow: [database]
```

The loader walks upward from the target file looking for `.shatter/config.yaml`. Match entries use `path.Match` semantics; the most specific match wins. Unknown allow strings are dropped with a warn-level log.

Adapter-owned targets (those with `InvocationModel.Kind == "adapter"`) bypass the gate because the adapter's curated httptest harness provides its own safety envelope.

The `skipped_by_policy` status value is already part of the shared `outcome` capability in `protocol/parity-matrix.yaml`; no parity-contract change is required to emit it. The `.shatter/config.yaml` loader is the first implementation under the Go-only `hint_config_v1` capability; broader hint-schema support is described in the Hint Config v1 Contract below.

### OS sandbox runner (str-jeen.56)

Go launcher and setup subprocesses can be routed through an OS-level sandbox by setting `SHATTER_SANDBOX_BACKEND`:

- `none` or unset — compatibility mode; runs the launcher directly.
- `bwrap` — uses bubblewrap with private `/tmp`, private home, no network namespace, read-only host system binds, and a disposable writable scratch copy mounted at the target module's original absolute path.
- `docker` — uses `docker run` with no network, read-only root filesystem, dropped capabilities, no-new-privileges, private tmpfs mounts, and bind mounts for the scratch project and staged launcher.

Docker options:

- `SHATTER_SANDBOX_DOCKER_IMAGE` defaults to `debian:bookworm-slim`.
- `SHATTER_SANDBOX_DOCKER_RUNTIME` optionally selects a runtime such as `runsc` for gVisor.

The sandbox runner contains local filesystem writes, but it does not yet emit `file_write` side effects and it is not yet required by the `local_fs` policy gate. The next enforcement step is to make `local_fs` execution require an active sandbox unless an explicit unsafe compatibility flag is set.

## Hint Config v1 Contract (str-hy9b.G3)

`shatter-go/config/loader.go` parses `policy`, `defaults`, `mocks`, and `generators` sections of each `functions.<glob>` entry in `.shatter/config.yaml`. Unknown top-level and per-function keys are surfaced via `File.Warnings` rather than failing the parse; most-specific-match-wins (handled by `MatchTarget`) extends to the new sections unchanged.

Wired end-to-end today:
- `defaults`: per-parameter literal overrides flow into `planner.ParamPlanOptions.HintsByName` and become top-priority `ValuePlan`s, taking precedence over `classifyParamFamily` defaults.
- `generators`: per-parameter runtime-value registry name flows into `planner.ParamPlanOptions.GeneratorsByName`; `PlanParam` consults the named registry entry before falling back to primitive families. An unknown generator name yields `UnsatisfiedRequirementKindComplexType` so config typos surface.
- `policy.allow`: unchanged from the G4 contract above.

Mocks are partially wired:
- The loader parses the `mocks` map and the planner emits sorted, target-scoped `planner.MockSpec` entries via `planner.ResolveMockSpecs` for use by code generators.
- **Not yet wired:** execute-time substitution (build-time symbol swap or launcher-level shim) is **not implemented**. Anything relying on `mocks` to take effect at runtime today is unsupported. Tracked under **str-8v66** (blocked by str-ruw0).

Resolution flow: `protocol/handler.go` populates `FunctionAnalysis.SourceFile` during `analyze`; `main.go`'s planner closure (`hintConfigResolver` + `translateHintConfig`) loads `.shatter/config.yaml` per target and threads the matched entry into `planner.PlanRequirementsOptions.PerTargetHints`.

`hint_config_v1` is declared as Go-only with no wire probe in `protocol/parity-matrix.yaml`; nothing here flows over the protocol boundary, so adding mock substitution in str-8v66 will not require a parity-matrix change.

## Workspace GOCACHE Binding (str-hy9b.B2)

Every `go build` invoked from shatter-go pins `GOCACHE` to `<workspace>/cache/build` via `Workspace.GoEnv()`. Wiring lives in `instrument.applyGoBuildEnv` (for `instrument/` build sites) and `instrument.WorkspaceGoEnv()` (consumed by `setup/loader.go`). The handler installs the provider from its workspace handle in `newHandler()`; tests that construct a handler without a workspace fall back to the legacy `SHATTER_HARNESS_CACHE`-based cache hierarchy.
