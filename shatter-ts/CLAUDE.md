# shatter-ts

TypeScript language frontend. Node.js subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `src/main.ts` — Entry point, protocol handler
- `src/logger.ts` — pino logger configured for stderr with `[shatter-ts]` prefix, reads `SHATTER_LOG_LEVEL`

## Instrumentor Parity Contract

`instrumentor.ts` has two symbolic expression builders that **must handle the same AST node types**:

| Function | Purpose | Location |
|---|---|---|
| `buildSymExprWithFlow()` | Data flow analysis (tracks variables through assignments) | Lines ~278-352 |
| `buildSymExpr()` | Branch condition analysis (builds SymExpr for Z3) | Lines ~860-951 |

When adding support for a new AST node type (e.g. `CallExpression`, `ElementAccessExpression`), update **both** functions. If `buildSymExprWithFlow()` returns `{kind: "unknown"}` for a node type that `buildSymExpr()` handles, variables assigned from that expression become invisible to the solver, silently degrading concolic coverage.

## Property-Based Testing (fast-check)

Property tests live in `src/property.test.ts` using `fast-check`. Use `fc.letrec` for recursive types (SymExpr, TypeInfo). Priority targets:

- Protocol message roundtrips (table stakes)
- **SymExpr builder parity**: `buildSymExpr` and `buildSymExprWithFlow` must handle the same AST node types — test that neither returns `unknown` for nodes the other handles
- Instrumentor output structural validity
- Any function processing untrusted protocol input from the core engine

When adding a new AST node type or protocol message, add corresponding fast-check properties.

## BigInt Serialization Contract

TS serializes `bigint` values as `{"__complex_type": "big_int", "value": "<decimal string>"}` in all protocol responses. The `serializeReplacer` in `src/serialize.ts` is wired into `sendResponse()` in `main.ts` and into internal `JSON.stringify` calls in `executor.ts` (console capture, before/after state snapshots). The inverse operation (`reconstructValue` in `src/reconstruct.ts`) converts tagged objects back to native `BigInt` for function inputs. Go and Rust frontends do not produce `bigint` values natively; the Rust core accepts tagged objects as `serde_json::Value` and `export.rs` already formats them for test generation.

## Ite SymExpr Parity Contract

TS is the only frontend that produces `ite` SymExpr nodes — SSA phi-node merges from conditional variable reassignment (str-4kop). Go and Rust deserialize `ite` but do not produce it. See `protocol/parity-matrix.yaml` `ite-symexpr-production-partial`.

## Loop Snapshot Parity Contract

TS emits `loop_body_states` in execute responses for supported canonical counted loops (str-z0kp.2). The executor combines cached `FunctionAnalysis.loops` metadata with observed `scope_events` iteration counts and emits zero-based per-iteration symbolic local snapshots. Current support is intentionally narrow: counted `for` loops plus simple accumulator locals already tracked by the instrumentor's flow map.

Wire shape: `loop_body_states: [{ loop_id, iteration, locals }]` where `locals` is a map of identifier-local names to `SymExpr`.

Go and Rust also emit `loop_body_states` using the shared `loop_id` and zero-based `iteration` contract. Go emits symbolic locals for a narrow counted-loop accumulator slice; Rust currently emits empty `locals` maps because its runtime hook has no source-level symbolic environment. See `protocol/parity-matrix.yaml` `loop-body-states-typescript-only`.

## Side Effect Parity Contract

TS is the reference implementation. All 7 canonical kinds are defined in `src/protocol.ts`; `arbSideEffect` in `src/property.test.ts` generates all of them.

Captured: `console_output` (via `createCapturingConsole()` in `executor.ts`, max 4096 bytes/message, stdout→"log" stderr→"error"), `global_state_change` (pre/post diff of exported module-level variables), `thrown_error` (catch block, captures `error_type`/`message`/`stack`), `global_mutation` (name-only for exported module names). Not captured: `file_write`, `network_request`, `environment_read`. Console capture respects `capture: true` (default). Side effects list is truncated at 70 lines (head 50 + tail 20) — see `CAPTURE_HEAD_LINES`/`CAPTURE_TAIL_LINES` in `shatter-core/src/execution_record.rs`.

Authoritative matrix: `protocol/parity-matrix.yaml` `side_effect_capabilities`.

## Prepare Parity Contract

TS implements `prepare` to pre-warm the compiled script cache. Handler: `"prepare"` case in `src/handlers.ts`. Advertised in `SUPPORTED_CAPABILITIES`. `prepare_id` is SHA-256 of `file:function:sorted-mock-symbols`, first 16 hex chars. Cache backing: `compiledScriptCache` in `src/executor.ts`, pre-warmed via `warmCompiledScriptCache()`. `preparedKeys.clear()` runs on teardown, shutdown, and `clearInstrumentedSources`. `instrument` must be called first (source must be in `instrumentedSources`).

## Invocation Model Parity Contract

TS is the reference implementation for invocation model dispatch (Go/Rust will reach parity later). The executor inspects each function's `FunctionAnalysis.invocation_model` (cached at analyze time) and routes:

- Absent or `{ kind: "direct" }` → `executeInstrumented` / `executeFunction` (default path)
- `{ kind: "adapter", adapter_id, ... }` → `executeAdapterOwned` → `InvocationHook` resolved from `RuntimeHooks.invocation_hooks` by `adapter_id` (supplied via `RuntimeHookFactory` whose `id` matches an `ExecutionProfile.adapters` entry)
- `{ kind: "adapter", ... }` with no matching hook → `not_supported` error: `"execution adapter not supported by TypeScript frontend: <id>"`

Synthetic parameters and structured outcomes ride through existing wire fields (`inputs`, `return_value`, `thrown_error`, `side_effects`). **No new protocol fields.** Adapter-owned calls return empty `branch_path` / `lines_executed` / `path_constraints` / `calls_to_external` — instrumented adapter execution is a follow-up.

Implementation: `chooseInvocationStrategy` in `src/runtime-hooks.ts`, `executeAdapterOwned` in `src/executor.ts`, dispatch site in `src/handlers.ts` execute case. Analyses cached in `cachedAnalyses` keyed by `${resolvedFile}:${functionName}`, cleared on shutdown / function-level teardown / `clearInstrumentedSources`.

## Feature Capability Parity

TS declares support for `outcome` only in
`protocol/parity-matrix.yaml` `feature_capabilities` — the standardized
invocation-outcome wire shape reached cross-frontend parity in str-hy9b.A5.

The planner-surface capabilities (`invocation_plan`, `adapter_http_nethttp`,
`hint_config_v1`) are declared Go-only at this stage. TS does not yet
implement them; conformance tests (`task conformance`) expect TS to
return a clean "capability not supported" response rather than crashing
or returning malformed data when these are probed.

The Execute command's optional `plan` field (an `InvocationPlan` from
`get_invocation_plan`, added in str-hy9b.H5) is accepted on the wire but
ignored by the TS executor. This is a tracked divergence — see the
`ts-rust-execute-plan-not-implemented` entry in
`protocol/parity-matrix.yaml`. TS callers that pass `plan` should expect
identical behavior to a request without `plan`; the field exists so that
plan-aware callers can speak a single wire shape across frontends without
branching on language. Implementation is deferred until TS grows a
planner.

## Outcome Emission Contract

Every `execute` response (status `execute`) carries an `outcome:
InvocationOutcome` field. `deriveOutcome` in `src/executor.ts` classifies
the result:

| Condition on `rawResult.thrown_error` | `outcome.status` |
|---|---|
| `null` | `completed` (carries `return_value`) |
| `error_type == "ERR_SCRIPT_EXECUTION_TIMEOUT"`, or `error_category == "infrastructure"` whose message matches `TIMEOUT_PATTERNS` | `timed_out` |
| any other thrown error | `runtime_failed` |

Status values `build_failed`, `unsupported`, `completed_with_findings`, and
`skipped_by_policy` are not produced by this path. `completed_with_findings`
is reserved for upstream consumers; the TS executor does not surface
build/unsupported through `execute` — those arrive on `error` responses
from earlier pipeline stages.

The wire shape is identical to Rust and Go (str-hy9b.A5,
parity-matrix.yaml `feature_capabilities.outcome`). Conformance lock:
`protocol/conformance/conformance_cases.yaml` `execute_outcome_shape_ts`.

## Environment Preflight Contract (str-jeen.26 → str-jeen.40)

The TS frontend runs a one-shot environment preflight on the first
request that carries a `project_root`: if `node_modules/` is absent the
failure is cached and every subsequent `analyze` / `instrument` /
`prepare` / `execute` / `setup` short-circuits with the cached error.
The wire-level error code is **`preflight_failed`** (a first-class
variant added by str-jeen.40, replacing the str-jeen.26 stopgap that
reused `not_supported`); the message keeps the structured prefix
`preflight_failed: <reason>: <detail>` so log scrapers written against
the stopgap remain compatible. The Rust core's `batch_analyze` skips
both `not_supported` and `preflight_failed` files so the run still
terminates cleanly without per-target `runtime_failed` noise.

Go and Rust frontends declare the wire code and the
`OutcomeStatus::PreflightFailed` variant for compatibility but do not
emit them yet — see `parity-matrix.yaml` allowed_divergence
`error-code-preflight-failed-typescript-only`.

## No-Target-Reason Classifier Contract

The TS per-language no-target-reason classifier (str-jeen.22) refines
zero-target TS files into one of `declaration_only`, `jsx_component_only`,
or `test_or_spec`. Files that don't match any TS-specific signal fall
through to `unclassified` (the str-jeen.21 default).

**The classifier lives CLI-side**, not in this crate. It is hosted in
`shatter-cli/src/commands/explore.rs` (`ts_classify_no_target_reason` and
helpers) following the str-jeen.25 frontend-agnostic pre-classifier pattern
and the str-jeen.24 Rust precedent. The frontend Analyze response wire
shape is unchanged — the protocol does not yet carry `no_target_reason`
from frontend → CLI, so emitting per-language classifications would
require a protocol surface change. When that field is added, the
classifier can move into this frontend without behavioral change for
callers.

Order of checks (first match wins):

1. `declaration_only` (path) — basename ends in `.d.ts`, `.d.cts`, or
   `.d.mts`. Ambient-declaration files are unambiguous regardless of
   contents.
2. `test_or_spec` (path) — file under any `__tests__`, `__mocks__`, or
   `tests` directory segment, OR basename contains a `.test.` /
   `.spec.` / `.stories.` infix (Jest / Vitest / Storybook
   conventions).
3. `jsx_component_only` (content) — file extension is `.tsx` / `.jsx`
   AND the source contains a JSX closing-tag form (`</`). TS generics
   use `<T>` but never `</T>`, so closing-tag form is a reliable JSX
   signal that doesn't mistake `Array<number>` for a component.
4. `declaration_only` (content) — every depth-zero, non-blank,
   non-comment line is a pure type-level declaration: `import …`,
   `export type …`, `export interface …`, `export *`, `export { … }`,
   `type X = …`, `interface X { … }`, `declare …`, or
   `export declare …`. Conservative: any value-level definition
   (`function`, `const`, `let`, `var`, `class`, `enum`, or their
   `export` variants) returns `false` and the caller falls through to
   `unclassified`.

Authoritative matrix entry: `protocol/parity-matrix.yaml`
`shared_wire_types.no_target_reason.frontends.typescript:
implemented_via_cli_classifier`.

## Timeout Contract

15s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `getExecTimeoutMs()` in `src/executor.ts`.

## Browser-Global Missing-Adapter Classification Contract (str-jeen.30)

When a TS execute target references a browser global (`window`, `document`,
`navigator`, `location`, `history`, `localStorage`, `sessionStorage`,
`matchMedia`, `XMLHttpRequest`, the observer constructors, animation-frame
helpers, or browser dialog functions) and no `browser-globals` adapter is
applied, the VM throws a `ReferenceError`. The execute handler converts
that into a first-class `error` response rather than letting it bucket as a
generic `runtime_failed` outcome:

| Field | Value |
|---|---|
| `code` | `not_supported` |
| `message` | `unsupported_missing_global: <name> (<category>) - enable the 'browser-globals' adapter to execute targets that depend on browser globals` |

The wire-level error code stays `not_supported` (no new variant — the same
stopgap str-jeen.26 used for missing `node_modules` preflight). The
structured `unsupported_missing_global:` prefix lets log scrapers and the
run report distinguish missing-browser-global outcomes from other
`not_supported` errors. str-jeen.40 will refine bucketing once a dedicated
status / code is introduced.

Implementation: `classifyMissingBrowserGlobal` and
`formatMissingBrowserGlobalMessage` in `src/browser-globals-recognizer.js`,
called from the execute case in `src/handlers.js` after `rawResult` is
obtained and before the standard execute response is built. The
classification is conservative: only ReferenceErrors whose message matches
`<identifier> is not defined` for an identifier listed in `BROWSER_GLOBALS`
are converted. Ambiguous globals (e.g. `fetch`, which Node 18+ provides)
are intentionally NOT classified, because their absence signals a
different misconfiguration.

The fixture `src/__fixtures__/adapter-browser-globals.ts` exercises this
path; regression tests live in the `"missing browser global classification
(str-jeen.30)"` describe block in `src/handlers.test.ts`.

## TS Transform Failure Classification (str-jeen.11)

TS-to-JS transformation failures during `execute` and `prepare` surface as
`error` responses with `code: "compilation_error"` rather than the generic
`internal_error`/`runtime_failed` they used to take. The executor throws
`TranspileError` (in `src/executor.ts`) carrying `category`:

| `category` | Triggered when |
|---|---|
| `transpile_failed` | `ts.transpileModule` throws or emits fatal `ts.DiagnosticCategory.Error` diagnostics |
| `compile_failed` | `new vm.Script(...)` rejects the emitted JS (typically because TS type syntax — interface refs, type-only identifiers in value position, JSX in a misclassified file — survived transpile) |

Handlers (`src/handlers.ts` `case "execute"` and `case "prepare"`) translate
`TranspileError` into a `compilation_error` response with the category and
diagnostic text in the message. This avoids opaque `runtime_failed` outcomes
on TS/TSX runtime failures observed in Kapow validation. Regression
fixtures live in `src/__fixtures__/type-syntax-coverage.tsx` and the
`"TS type syntax pipeline coverage (str-jeen.11)"` describe block in
`src/executor.test.ts`.
