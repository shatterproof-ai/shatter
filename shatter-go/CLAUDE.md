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

Go produces `ite` SymExpr nodes from conditional variable reassignment across
if/else branches.  The mechanism:

1. `instrument/flow.go` — `flowMap`, `snapshot`, `mergeFlowMaps` (str-1hlk.17.1)
2. `instrument/flowwalk.go` — `walkStmtsForFlow`, `applyIfToFlow` (str-1hlk.17.2)
3. `protocol/analyzer.go` — `walkBodyForFlow` builds the body-level flow map;
   `extractBranches` threads it through `buildSymExprWithFlow` so branch
   conditions referencing conditionally-assigned local variables resolve to
   `ite{condition, then_expr, else_expr}` (str-1hlk.17.3).

Example: a variable `label` assigned 1 in the then-branch and -1 in the
else-branch produces `label = ite(x > 0, const(1), un_op(neg, const(1)))`.
A subsequent branch `label > 0` then surfaces as
`bin_op(gt, ite{...}, const(0))` in `FunctionAnalysis.Branches[i].Condition`.

Rust frontend analysis is implemented but does not yet produce `ite`.
See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

## Loop Snapshot Parity Contract

Go emits `loop_body_states` for analyzed canonical counted loops by combining cached `FunctionAnalysis.loops` metadata with runtime `scope_events`. Snapshots use the cross-frontend `loop_id` plus zero-based `iteration` contract. For the supported source-AST slice, Go also emits symbolic `locals` for the induction variable and simple identifier accumulators updated with `+=`, `-=`, `*=`, `/=`, `++`, or `--`.

## Side Effect Parity Contract

Go emits all 7 canonical side-effect kinds. Both `instrument.SideEffect` (in `executor.go`) and `protocol.SideEffect` (in `protocol/types.go`) carry fields for all 7.

Captured: `console_output` (overlay-instrumented calls to `fmt.Print*`, `log.Print*`, and package-level `slog.Debug/Info/Warn/Error` emit per-call entries with log/info/warn/error/debug levels; process-level stdout/stderr remains a fallback for uninstrumented output), `global_state_change` (pre/post diff of exported package-level variables), `thrown_error` (panic/error details in `side_effects` and top-level `thrown_error`), `global_mutation` (name-only entry emitted alongside detected global state changes), `file_write` (overlay-instrumented `os.WriteFile` with path/content), `network_request` (overlay-instrumented package-level `net/http.Get`, `Post`, and `PostForm` with method/url), `environment_read` (overlay-instrumented `os.Getenv` and `os.LookupEnv` with variable/value).

Limits: Go side-effect capture is source-overlay based, not syscall based. It does not capture `os.File.Write`/`WriteString`, syscall writes, `http.Client.Do`, custom clients/transports, raw `net.Dial` traffic, `os.Environ`, third-party APIs, or uninstrumented dependency code.

`convertSideEffects()` in `protocol/handler.go` maps all 7 fields.

Authoritative matrix: `protocol/parity-matrix.yaml` `side_effect_capabilities` and `allowed_divergences: go-side-effects-partial`.

## Prepare Parity Contract

Go implements `prepare` to pre-build a launcher-backed execution binary so subsequent execute calls skip rebuilds. Handler: `handlePrepare()` in `protocol/handler.go`, with launcher preparation in `protocol/prepared_launcher.go`. Advertised in `CommandCapabilities` (`protocol/constants.go`). `prepare_id` is SHA-256 of `file:function:mock-fingerprint:receiver_kind` (plus any generic type args), first 16 hex chars (`computePrepareID`); the mock fingerprint is `instrument.MockFingerprint` (symbol + expression + behavior + return_values, shared with `build.cacheKey`) so a mock body change can't reuse a stale harness (str-c8djq). When a Prepare request carries an `InvocationPlan`, `plan.receiver_kind` is included in the key so different receiver strategies for the same target produce different IDs and don't collide in the harness cache (str-oegu). Plan-less Prepare requests use an empty `receiver_kind` (equivalent to pre-str-oegu behavior). Storage: `handler.preparedHarnesses map[string]preparedExecution`. Idempotent. `handleTeardown` (level=function) + `handleShutdown` call `Cleanup()` on cached prepared executions.

## Invocation Model Parity Contract

Go has the adapter substrate (registry, dispatch, invocation hooks) and two concrete adapters: `go/http-handler` for net/http handler functions and `go/gin` for Gin handler functions. The substrate mirrors TS: `ChooseInvocationStrategy` dispatches to direct/adapter/unsupported; `ResolveRuntimeHooks` resolves an `ExecutionProfile` against registered `RuntimeHookFactory` instances; `ExecuteAdapterOwned` invokes an `InvocationHook` and returns an `instrument.ExecuteResult` with empty instrumentation fields.

### go/http-handler adapter

Recognizes package-level functions with signature `func(http.ResponseWriter, *http.Request)` (including unnamed params). Detection uses Go's type checker, not string matching. When recognized, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/http-handler"}` with 4 synthetic params (method, path, headers, body). At execute time, the adapter builds a specialized launcher entrypoint that runs the handler against `httptest.NewRequest`/`httptest.NewRecorder` and returns the HTTP response (status, headers, body) as `return_value`. Instrumentation fields (branch_path, lines_executed) are empty for adapter-owned calls.

Receiver methods with handler-shaped signatures still receive `AdapterHints`,
but they do not auto-promote to `InvocationModel` until the adapter launcher
has a receiver-aware construction path. They execute through the normal method
wrapper path instead; the adapter launcher defensively rejects receiver-shaped
function names such as `(*Server).ServeHTTP` rather than emitting invalid
`target.(*Server).ServeHTTP` source.

Key files: `protocol/nethttp_recognizer.go` (detection), `protocol/nethttp_adapter.go` (factory/hook), `protocol/adapter_launcher.go` (launcher generation/execution).

### Adapter hint recognizers

`protocol/recognizer.go` adds hint-based recognizers that run as post-processing in `AnalyzeFileWithTiming` and emit `AdapterHint` values on `FunctionAnalysis.AdapterHints`. These complement the per-function `recognizeHTTPHandler` (which sets `InvocationModel` directly for exact matches) by also detecting partial matches and Gin handlers:

- **net/http**: Detects `ResponseWriter`+`*Request` params (high confidence) and partial matches like `ResponseWriter`-only or `ServeHTTP` methods (medium/high). Uses `go/http-handler` adapter ID. Receiver-method hints are discovery signals only; they are not auto-promoted to adapter invocation models.
- **Gin**: Detects `*gin.Context` params (high confidence) and characteristic API calls (`c.JSON`, `c.Param`, etc.) via AST fallback since the type checker cannot resolve third-party imports. Uses `go/gin` adapter ID.

High-confidence hints auto-promote to `InvocationModel` (with `SyntheticParams` resolved via `syntheticParamsForAdapter()` in `analyzer.go`) when not already set by the per-function recognizer, except for receiver-method targets, which stay on the receiver-aware wrapper path.

### go/gin adapter

Recognizes functions with `*gin.Context` parameter via hint-based AST detection (type checker cannot resolve third-party imports). When recognized with high confidence, `FunctionAnalysis.InvocationModel` is set to `{kind: "adapter", adapter_id: "go/gin"}` with 5 synthetic params (method, path, headers, body, route_params). At execute time, the adapter builds a specialized launcher entrypoint that runs the handler against `gin.CreateTestContext(httptest.NewRecorder())` with `gin.SetMode(gin.TestMode)`, sets `c.Request` and `c.Params` from inputs, and returns the HTTP response (status, headers, body) as `return_value`. Route parameters are injected directly via `c.Params` (bypassing Gin's router). Instrumentation fields are empty for adapter-owned calls.

Key files: `protocol/recognizer.go` (detection), `protocol/gin_adapter.go` (factory/hook), `protocol/adapter_launcher.go` (launcher generation/execution).

The handler caches analyses from `handleAnalyze` and reads `invocation_model` in `handleExecute` to dispatch. Cache is cleared on function-level teardown and shutdown.

Key files: `protocol/adapter.go` (types, pure functions), `protocol/handler.go` (integration).

## Feature Capability Parity

Go declares support for all four redesign feature capabilities in
`protocol/parity-matrix.yaml` `feature_capabilities`:

- `outcome` — standardized invocation-outcome wire shape (str-hy9b.A1).
- `invocation_plan` — planner artifact schema (str-hy9b.E1) and the receiver-aware planner pathway (str-hy9b.H5). Go-only at this stage. `planner.PlanRequirements` returns method-target plans with non-empty `receiver_kind` (e.g. `"constructor:NewService"`) and `argument_plans` per parameter; parameterized constructors also emit `constructor_arg_plans`, which the Rust core materializes as an input-vector prefix before method arguments and the Go wrapper consumes before method arg deserialization. `Command::Execute.plan` (an optional InvocationPlan on Execute requests) is the bridge from the Rust core's `planner_consumer` into the Go launcher's wrapper-aware dispatch path. TS/Rust frontends accept `Execute.plan` on the wire (additive, omitempty) but ignore it — see `ts-rust-execute-plan-not-implemented` in `protocol/parity-matrix.yaml`. The `argument_plans[].kind` enum includes `runtime_value` (str-1hlk.4): Go emits it for parameters resolved from the runtime-value registry (e.g. `context.Context` → `context.Background()`), with `literal` carrying the JSON-encoded source expression and `type_hint` naming the registered Go type. The Rust core deserializes the kind but does not materialize a seed for it. str-gxjs.1: the resolution moved from execute time to wrapper-build time — `shatter-go/wrapper` consults `shatter-go/runtimeval.Lookup` for each parameter's Go-source type and bakes the registered expression into the generated wrapper as a direct assignment (`var ctx context.Context = context.Background()`), with the required imports threaded through the wrapper's import block. Functions taking `context.Context`, `http.ResponseWriter`, `io.Writer`, or `io.ReadCloser` therefore compile, link, and execute without a JSON input slot per param. Exception (str-e41w): a *direct* `*http.Request` parameter is NOT bound to the fixed runtime value — the analyzer reports it as `{kind: "str", label: "*http.Request"}`, the planner seeds schema-agnostic JSON bodies (project config hints take precedence), and the wrapper consumes one string input slot as the request body via `httptest.NewRequest("POST", "/", strings.NewReader(body))` with stub `x-api-key`, `Authorization: Bearer`, `x-goog-api-key`, and `Content-Type: application/json` headers, so the solver/explorer can drive payloads into HTTP handlers. Nested `*http.Request` (struct fields, slice elements, constructor args) still uses the fixed runtime-value expression. Go-only: see `go-symbolic-http-request-body` in `protocol/parity-matrix.yaml`.
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

**Method targets require a plan (str-hy9b.H5), refined by str-jeen.50.** Method execution is dispatched through the launcher wrapper's receiver-kind switch, driven by `Command::Execute.plan.receiver_kind`. When an Execute request targets a method but does not carry a `plan` field (or carries one with an empty `receiver_kind`), `handleExecute` synthesizes a default receiver_kind before invoking the launcher via `synthesizeExecuteReceiverKind`:

- Interface receivers and generic-unconstrained receivers short-circuit to `OutcomeStatusUnsupported` with `error_type = "method_not_supported"` and a `short_reason` naming the unconstructible-receiver reason — the launcher never runs.
- Other method targets get either `"constructor:<FuncName>"` (when a parameterless same-package constructor exists) or the wrapper's always-emitted `"zero_value"` fallback, mirroring the receiver planner's priority order.

`failureOutcome` carries a defense-in-depth arm: any error whose message contains `"unknown receiver kind"` (e.g. a caller that bypasses synthesis with a hand-crafted plan and an invalid receiver_kind) is reclassified as `OutcomeStatusUnsupported` / `method_not_supported`. The pre-str-jeen.50 default `runtime_failed` classification caused these structural failures to be misclassified as completed runtime exploration outcomes; the new classification reflects that the target body was never actually executed.

Free-function targets are unaffected and still take the empty-receiver path. The `unsupported` / `method_not_supported` classification in `failureOutcome` now covers both the host-level "receiver planning" errors that surface before the launcher runs and the wrapper-level "unknown receiver kind" arm; both encode that the method target was never dispatched, just discovered to be unconstructible at different layers.

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

`shatter-go/config/loader.go` parses `policy`, `defaults`, `mocks`, `generators`, and `receiver` sections of each `functions.<glob>` entry in `.shatter/config.yaml`. Unknown top-level and per-function keys are surfaced via `File.Warnings` rather than failing the parse; most-specific-match-wins (handled by `MatchTarget`) extends to the new sections unchanged.

Wired end-to-end today:
- `defaults`: per-parameter literal overrides flow into `planner.ParamPlanOptions.HintsByName` and become top-priority `ValuePlan`s, taking precedence over `classifyParamFamily` defaults.
- `generators`: per-parameter runtime-value registry name flows into `planner.ParamPlanOptions.GeneratorsByName`; `PlanParam` consults the named registry entry before falling back to primitive families. An unknown generator name yields `UnsatisfiedRequirementKindComplexType` so config typos surface.
- `policy.allow`: unchanged from the G4 contract above.
- `receiver`: per-method receiver recipes flow into `planner.PerTargetHints.Receiver`; `PlanReceivers` emits a top-priority configured receiver plan with `receiver_kind` `configured:<label>` (or `configured` when the label is empty), ahead of auto-discovered receiver strategies so it is not capped out by `MaxReceiverPlans`. `shatter-go/wrapper` reads the same config from the source file to emit a matching receiver-kind switch case using the configured Go expression and imports. Missing or whitespace-only receiver expressions are warned by the config loader and ignored by planner/wrapper wiring.

Mocks are wired end-to-end via execute-time call-site substitution (str-c8djq):
- The loader parses the `mocks` map (`config.FunctionConfig.Mocks`, `map[qualifiedFunc]goExpression`) and the planner still emits sorted, target-scoped `planner.MockSpec` entries via `planner.ResolveMockSpecs` for planning/reporting.
- **Execute-time substitution is implemented as AST call-site replacement.** At execute/prepare time, `protocol/handler.go`'s `(*Handler).configMockConfigs(file, function)` loads the matched `.shatter/config.yaml` `mocks` entries (memoized by resolved-path + mtime; malformed configs are logged at WARN, never silently swallowed) and appends them to `execMocks` as expression-bearing `instrument.MockConfig` values. `instrument.DedupeMocks` then collapses any wire mock and config mock for the same symbol, letting the config **Expression win** (otherwise `sanitizeMockName` would emit two identical `ShatterMock_<name>` funcs and break the build). `buildDirectExecutionRequest` resolves these into `build.BuildRequest.MockSubstitutions` using the loaded package's `TypesInfo` (`protocol/mock_resolve.go`), and the overlay build (`build/instrumented_overlay.go`) calls `instrument.RewriteMockCallSitesInFile` to replace each genuine call site with the parsed `Expression` before `go build -overlay` compiles it. The real callee body — and its side effects (filesystem, network, subprocess/browser launch) — never runs. Because the config load happens frontend-side at execute time, config mocks apply regardless of whether the explorer or orchestrator Rust driver issued the Execute request.
- **Type-aware matching (not blind syntactic).** A config mock for `auth.GetAccount` must not rewrite a method call on a same-named local (`auth := newClient(); auth.GetAccount(id)`). The preferred path is type-resolved: `resolveMockSubstitutionScopes` walks `pkg.Syntax` and records, per enclosing function (`funcKey`), where the qualifier resolves to a `*types.PkgName`; the rewriter only substitutes inside those functions. When type info is unavailable the rewriter falls back to scope-aware syntactic matching — the qualifier must be an imported package in the file and must not be shadowed by a local binding in the enclosing function — and logs the fallback.
- Contract: each mock value is a **Go source expression** pasted in place of the whole call (including its arguments). The expression may reference only packages already imported by the target file — call-site substitution does not add imports. Cache correctness: both `build/builder.go` `cacheKey` and `protocol/handler.go` `computePrepareID` feed the single `instrument.MockFingerprint` (symbol + expression + behavior + **return values**) into their hashes, so any mock change — including a different `return_values` table on the prepare fast path — invalidates both the launcher-binary and prepared-harness caches.
- **Scoping semantics:** substitution is applied across the **whole target package** (every instrumented file), so a mocked symbol called from a sibling function in the same package is also substituted. It does **not** reach other packages — a helper in `internal/validate` that calls the mocked symbol still runs the real one — so mixed real/mock state within a single exploration is possible. Adapter-owned targets (`InvocationModel.Kind=="adapter"`, net/http & gin) run through a curated httptest harness, not the overlay build, so config mocks do **not** apply there; the handler logs a WARN when expression mocks are configured for such a target.
- Rewriter core: `instrument/mocksubst.go` (`RewriteMockCallSites`, `RewriteMockCallSitesInFile`, `MockSubstitutionsFromConfigs`, `DedupeMocks`, `normalizeMockSymbol`, `FuncKeyForDecl`) + `instrument/mockfingerprint.go` + `protocol/mock_resolve.go`. Regression coverage: `instrument/mocksubst_test.go` (unit + rapid properties incl. shadowed-local skip, type-resolved allow-lists, varied call-site positions, normalize dot/colon equivalence), `protocol/config_mock_test.go` (config→execMocks bridge, mtime caching, wire/config dedupe), `protocol/execute_mock_pipeline_test.go` (full handler seam: `.shatter/config.yaml` → execute response carries the substituted value, real side effect absent), `build/builder_mock_subst_e2e_test.go` (integration: constructor value substituted, real filesystem/subprocess side effect absent, nil-guarded branch reachable, kapow browser/scraper shape explored without launching a real process).
- **Deferrals:** same-package unqualified calls (`newQSScraperBrowser(...)`), argument-spread call sites (`f(a, xs...)`), and the wire `MockConfig.ReturnValues` typed-decode path (auto-discovered shim values without a user-supplied expression) are not yet substituted at the call site. Expression-only mocks no longer generate dead `ShatterMock_*` shims (only wire ReturnValues mocks do). `str-8v66` (blocked by str-ruw0) still tracks the ReturnValues-based automatic path.

Resolution flow: `protocol/handler.go` populates `FunctionAnalysis.SourceFile` during `analyze`; `main.go`'s planner closure (`hintConfigResolver` + `translateHintConfig`) loads `.shatter/config.yaml` per target and threads the matched entry into `planner.PlanRequirementsOptions.PerTargetHints`.

`hint_config_v1` is declared as Go-only with no wire probe in `protocol/parity-matrix.yaml`; nothing here flows over the protocol boundary, so adding mock substitution in str-8v66 will not require a parity-matrix change.

## Duration Parameter Wire Format (str-is5g)

`time.Duration` is an int64 alias in nanoseconds. The canonical wire format for a `time.Duration` parameter is therefore an integer-nanosecond JSON literal — that is what the parameter's default `UnmarshalJSON` consumes. The Go planner's `classifyParamFamily` (`shatter-go/planner/param.go`) emits `durationFamily()` candidates of that shape (zero, 1ms in ns, 1s in ns, -1s in ns) when `ParamInfo.TypeName == "time.Duration"`.

The Rust core's random input generator (`shatter-core/src/input_gen.rs::generate_duration`) emits the legacy shape `{"__complex_type":"duration","ms":N}` shared with the TypeScript frontend. To keep the random-explorer path working without crossing the crate boundary, the wrapper generator special-cases `time.Duration` parameters in `wrapper.writeDurationParamDeserialization`: it tries an integer-nanosecond decode first; on `UnmarshalTypeError` it retries against `{ "__complex_type": "duration", "ms": <int64> }` and converts milliseconds to nanoseconds (`time.Duration(ms) * time.Millisecond`). Any other object shape preserves the original integer-decode error so the failure message stays specific.

`shatter-go/reconstruct/reconstruct.go` carries the same ms→ns conversion math at the `interface{}` level (historical, no current callers); the wrapper helper is the live path.

Regression coverage: `shatter-go/wrapper/wrapper_duration_test.go` (static-source guards + compile + run with both wire shapes), `shatter-go/planner/param_test.go::TestPlanParam_Duration_IntegerNanosecondCandidates`, and `shatter-core/tests/e2e_concolic_go.rs::e2e_go_duration_param_categorize` (full pipeline against `examples/go/duration-param/duration.go`).

## Error Parameter Wire Format (str-jn9r0)

A bare builtin `error` parameter cannot be `json.Unmarshal`ed directly (the analyzer maps builtin `error` to `ComplexKind:"error"` in `complexKindFromNamed`; the param's `GoType` reaching the wrapper is the literal string `"error"`). The Rust core's random generator (`shatter-core/src/input_gen.rs::generate_error`) emits the cross-frontend shape `{"__complex_type":"error","class":...,"message":m}`. The wrapper generator special-cases `GoType == "error"` in `wrapper.writeErrorParamDeserialization` (dispatched beside the `time.Duration` case): it tries a plain decode first — JSON `null` decodes into the interface as a nil error, giving the caller the nil branch for free — and on any decode error falls back to the tagged object, reconstructing `errors.New(message)`. The `class` field is intentionally ignored (no typed-error reconstruction; sentinel `errors.Is`/`As` satisfaction is str-kvzh7). Any other shape preserves the original plain-decode error. The `errors` import is threaded through the wrapper import block via `wrapperNeedsErrorImport` (same mechanism as `time` for Duration params). Scope: bare builtin `error` only — named error-implementing types are out of scope.

Regression coverage: `shatter-go/wrapper/wrapper_error_test.go` (static-source guards + compile + run with null and the object wire shape) and `shatter-core/tests/e2e_concolic_go.rs::e2e_go_error_param_classify` (full pipeline against `examples/go/error-param/classify.go`, both nil/non-nil branches).

## Workspace GOCACHE Binding (str-hy9b.B2)

Every `go build` invoked from shatter-go pins `GOCACHE` to `<workspace>/cache/build` via `Workspace.GoEnv()`. Wiring lives in `instrument.applyGoBuildEnv` (for `instrument/` build sites) and `instrument.WorkspaceGoEnv()` (consumed by `setup/loader.go`). The handler installs the provider from its workspace handle in `newHandler()`; tests that construct a handler without a workspace fall back to the legacy `SHATTER_HARNESS_CACHE`-based cache hierarchy.
