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

## Timeout Contract

15s default, overridden by `SHATTER_EXEC_TIMEOUT` env var (seconds). See `getExecTimeoutMs()` in `src/executor.ts`.
