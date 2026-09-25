# Shatter frontends — code quality & protocol consistency review

Scope: shatter-ts, shatter-go (+shatter-go-tool), shatter-rust (+shatter-rust-runtime), protocol/ (registry, schemas, generated bindings, parity matrix), per-crate CLAUDE.md, PARITY.md, PROTOCOL.md, frontend-parity/ts-conventions/go-conventions skills. Read-only; no builds/tests run. `python3 scripts/protocol-codegen.py --check`, `scripts/validate-protocol-registry.py`, and `scripts/validate-parity.py --today 2026-09-03` were executed (pure scripts).

Severity: **P1** = gate/contract silently broken or major maintainability hazard; **P2** = real drift/bug risk; **P3** = hygiene.

---

## Part A — Frontend code quality

### A1. Size (non-test source lines; >500 flagged)

| Frontend | Total non-test | Test lines | Files >500 lines |
|---|---|---|---|
| shatter-ts/src | 16,725 | 17,765 | executor.ts 3,312 · analyzer.ts 2,584 · instrumentor.ts 2,363 · handlers.ts 1,262 · react-hook-invocation.ts 704 · protocol.ts 688 |
| shatter-go (+go-tool 394) | 29,997 | 43,132 | wrapper/wrapper.go 2,949 · protocol/analyzer.go 2,774 · protocol/handler.go 2,571 · build/instrumented_overlay.go 856 · planner/param.go 825 · launcher/launcher.go 798 · instrument/mocksubst.go 742 · protocol/prepared_launcher.go 737 · protocol/types.go 643 · instrument/visitor.go 634 · protocol/policy.go 561 · workspace/gc.go 538 · config/loader.go 524 |
| shatter-rust/src + runtime | 30,414 (tests inline) | — | **executor.rs 16,125** (594 KB; 7,607 non-test lines, 195 fns, then one 8,500-line `mod tests`) · analyzer.rs 4,475 · handler.rs 2,768 · protocol.rs 1,865 · adapters.rs 1,477 · instrument.rs 1,201 · shatter-rust-runtime/src/lib.rs 1,075 |

- **P1** `shatter-rust/src/executor.rs` is a single 16k-line file mixing harness codegen (string templates), three execution modes (standalone/crate-backed/crate-bridge), caches, axum adapter codegen, and its own test suite. It is 4× the next-largest file in any frontend and is the primary edit hotspot for the Rust frontend.
- **P2** Go's `protocol` package concentrates analyzer + handler + types + planner glue (2,774 + 2,571 + 643 lines) in one package; TS mirrors the pattern in executor/analyzer/instrumentor.

### A2. TypeScript

- tsconfig: `strict: true`, `noUncheckedIndexedAccess: true` (shatter-ts/tsconfig.json:9-10) — matches ts-conventions skill. ✓
- `: any` / `as any`: 11 + 1 occurrences, **all** in `src/__fixtures__/typed-input-shapes.ts` (excluded from tsconfig `include`); the one hit in analyzer.ts:2121 is prose in a comment. Effectively zero `any` in production code. ✓
- `@ts-ignore` / `@ts-expect-error`: 0. ✓
- **P2 No ESLint at all.** `shatter-ts/package.json` has no eslint dependency, no `lint` script, and no config file; yet ts-conventions/SKILL.md:10 lists ESLint rules (`no-explicit-any`, `ban-ts-comment`, `consistent-type-imports`, no default exports) as the "enforceable policy". Three `eslint-disable` comments in test files are dead annotations. The `any` cleanliness is currently maintained by convention, not tooling.
- fast-check: `property.test.ts` has 152 `fc.assert` calls; 6 other test files use it (browser-dom-adapter, entropy, opaque-stub-registry, param-shape-inference, react-hook-invocation, react-hook-recognizer). Coverage is real and includes the builder-parity property (property.test.ts:1174-1262). ✓

### A3. Go

- `_ =` sites in non-test code: ~40, almost all best-effort cleanup (`os.Remove`, `session.Kill`, `Close`). Two are unused-variable suppressions rather than error swallowing — **P3** `build/builder.go:277 _ = fresh`, `launcher/launcher.go:391 _ = anchorImport` (dead computation; delete or use). **P3** lock-file PID writes `_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())` at launcher/launcher.go:616 and build/builder.go:188 discard write errors on a file whose content is later read for stale-lock detection.
- `panic()` in non-test: `runtimeval/registry.go:38` (package-init wasm compile — acceptable), `timing/collector.go:77,84` (phase-stack underflow/mismatch). **P3** In a long-lived stdio frontend, a timing-instrumentation assertion panic kills the whole session; consider logging and resetting the stack instead.
- golangci-lint v2 config present (shatter-go/.golangci.yml): standard + gocritic + misspell, govet enable-all. **P2** `errcheck.exclude-functions` includes `encoding/json.Unmarshal` (line ~20): silently ignoring Unmarshal errors is exactly how malformed protocol input becomes a zero-value struct that "succeeds". Remove this exclusion and fix call sites. `shatter-go-tool` has no lint config (394 lines, low risk).
- rapid property tests: 27 test files in 7 of 19 packages (planner 12, protocol 6, instrument 5, build/overlay/workspace/wrapper 1 each). Packages with **no** property tests: launcher, sandbox, config, loader, harness, frontendsetup, generators, reconstruct, runtimeval, setup, timing. `reconstruct` and `config/loader` are round-trip-shaped and would benefit most.

### A4. Rust frontend

- Non-test `unwrap()`/`expect()` (excluding string-template harness code that is emitted, not executed in-process):
  - handler.rs:197,215,233,258,274,290,310,317,326 — nine `self.*cache.lock().unwrap()`; executor.rs:5702,5899,5954,6077,6350,6478,7417,7481,7538,7599 — same pattern. **P3** A poisoned mutex (any panic while holding it) permanently kills the frontend; use `lock().unwrap_or_else(PoisonError::into_inner)`.
  - executor.rs:4480 `dir.parent().unwrap()`, 7017 `kept.into_iter().next().unwrap()`, 7198 `public_module_path.unwrap()`, 5760/5877 `.unwrap()` — worth a look but likely invariant-backed.
  - `expect(...)`: generators.rs ×5, instrument.rs ×3, file_handle_generator.rs ×2, timing.rs ×1, wasm_generator.rs ×1, executor.rs 326/612/613/6750/6852/6868.
- Clippy `-D warnings` is enforced via `shatter-rust/Taskfile.yml:36` and root Taskfile. ✓ No `#![deny(missing_docs)]`.
- **P3** Module docs (`//!`) missing in 6/13 modules: `generators.rs`, `handler.rs`, `lib.rs`, `main.rs`, `timing.rs`, `wasm_generator.rs`. Present in adapters, analyzer, executor, file_handle_generator, instrument, protocol, setup, runtime lib.
- proptest used in 5 files (adapters, analyzer, file_handle_generator, protocol, timing). ✓

### A5. Parallel-path parity (SymExpr builders)

**TypeScript** — there are *three* builders, not two:

| Builder | File:line | Identifier resolution | Collapses all-unknown children? |
|---|---|---|---|
| `buildSymExprWithFlow` | instrumentor.ts:874 | `resolveName` callback only (no `paramNames`) | **Yes** — bin_op (920-923), un_op (931-933), typeof (950-953), call (962-971) return `unknown` when every child is unknown |
| `buildSymExpr` | instrumentor.ts:1862 | `paramNames` then `dataFlowMap` (1871-1879) | **No** — always emits the node (1917-1919, 1927-1928, 1934-1935, 1941-1946) |
| `buildSymExpr` (analyzer) | analyzer.ts:2280 | `paramNames` only | No |

Handled node kinds are the same set in all three (Parenthesized, Identifier, PropertyAccess, Numeric/String literal, true/false/null, Binary, PrefixUnary, TypeOf, Call). Divergences:
- **P2** Collapse semantics differ (above). The property test at property.test.ts:1174-1262 compares the two instrumentor builders, so this is presumably only observable on unknown leaves — but the documented contract (shatter-ts/CLAUDE.md:13-20) says only "same AST node types", not "same output", so the divergence is unspecified.
- **P2** Property-chain base resolution: `buildSymExpr` → `resolvePropertyChain` (1962) only accepts a *param* base and ignores `dataFlowMap`, so `const o = p; if (o.x)` is `unknown` on the branch path; `buildSymExprWithFlow` → `resolvePropertyChainWithFlow` (981) resolves the base through the flow map. Variables aliased from params become invisible to Z3 on one path but not the other — precisely the failure mode the CLAUDE.md warns about.
- **P3** Two identical operator tables: `binaryTokenToOp`/`unaryTokenToOp` (instrumentor.ts:1984/2051) vs `mapBinaryOp`/`mapUnaryOp` (analyzer.ts:2386/2431). Identical today; nothing keeps them so.
- **P3** Unhandled by all three: `ElementAccessExpression` (`arr[0]`, `obj["k"]`), `ConditionalExpression` (a natural `ite` source), template literals, the `undefined` identifier (protocol.ts:507 declares `{type:"undefined"}` but nothing produces it), `AsExpression`/`NonNullExpression`/`SatisfiesExpression` unwrapping, `void 0`, BigInt literals, postfix unary.
- **P3** shatter-ts/CLAUDE.md:17-18 cites "Lines ~278-352" and "~860-951"; actual are 874 and 1862.

**Go** — dual paths exist *twice*, in two packages with a duplicated type:
- `protocol/analyzer.go`: `buildSymExpr` (2171) / `buildSymExprWithFlow` (2201).
- `instrument/symextract.go`: `exprToSymExpr` (40) / `exprToSymExprWithFlow` (48), over a private `symExpr` struct (symextract.go:13, "mirrors protocol.SymExpr but is defined locally to avoid import cycles").
- Drift between the two packages (**P2**):
  - `true`/`false`/`nil` identifiers → consts in instrument (symextract.go:294-302) but `unknown` in analyzer `identSymExpr` (analyzer.go:2297-2310).
  - String literals: analyzer `litSymExpr` uses `strings.Trim(lit.Value, "`\"'")` (analyzer.go:2336-2347; wrong for escapes) vs `strconv.Unquote` in instrument (symextract.go:372).
  - Selector chains: analyzer flattens nested `a.b.c` (analyzer.go:2310, `flattenSelector`); instrument handles one level only (symextract.go:338-346).
  - Call names: analyzer emits `exprString(call.Fun)` for any selector (`pkg.Fn`, `recv.Method`) (analyzer.go:2359-2388); instrument returns `unknown` unless base is an ident (symextract.go:393-400).
  - `buildBinOp` (analyzer.go:2263) does not guard `tokenToOp == ""`; instrument does (symextract.go:310).
  - `buildSymExprWithFlow`'s `default:` arm (analyzer.go:2258-2262) sends `SelectorExpr` to the flow-less builder, so `v.Field` where `v` is flow-tracked never resolves through `fm`.
  - Wire shape: `protocol.SymExpr.Path` has no `omitempty` (types.go:510) → every non-param node carries `"path":null`; `protocol.SymExpr.Args` is required (types.go:518) while `instrument.symExpr.Args` is `omitempty` (symextract.go:24). The `"args":null` regression test (analyzer_test.go:1983-2000, property_test.go:1309) covers only the analyzer.
- **P2 cross-frontend**: Go emits method calls as `{kind:"call", name:"recv.Method", receiver:null}`; TS (instrumentor.ts:1941-1943) and Rust (analyzer.rs:2015-2028) emit `{name:"Method", receiver:<expr>}`. Not recorded in parity-matrix.yaml or conformance `known_drifts`.

**Rust** — dual path is typed-vs-string-templated:
- `analyzer.rs::build_sym_expr` (1954) produces `protocol::SymExpr` (typed, serde-serialized); handles Path, Field (root-first chains), Binary (full `convert_bin_op` incl. arithmetic/bitwise/compound-assign, 2261+), Unary, Lit, MethodCall (with receiver), Call, Paren, Reference, `Let` patterns.
- `instrument.rs::constraint_for_expr` (519) hand-formats JSON strings with `escape_json_string` (704). It handles only `Eq/Ne/Lt/Le/Gt/Ge/And/Or` (523-530) and `Not/Neg`; arithmetic, bitwise, method/function calls (576, 583), and `Let` all become `{"kind":"unknown","hint":...}`.
- **P2** Consequence: runtime `path_constraints` from Rust are strictly narrower than the static `branches[].condition` the same frontend reported for the same source (e.g. `x + 1 > 5`), so the concolic loop sees `unknown` where analyze promised a solvable expression. The hand-rolled JSON encoder is also a second place to get escaping/shape wrong.

### A6. Cross-frontend duplication suggesting a missing shared spec

- **SymExpr construction semantics** — 7 hand-written builders + 6 operator tables across three languages implement one vocabulary (`core/sym_expr.rs` BinOpKind/UnOpKind/ConstValue) with no spec for: call `name`/`receiver` form, literal normalization, when to collapse to `unknown`, path direction (root-first — Rust had a bug here, analyzer.rs:1968-1974 comment), or which node kinds are mandatory. `protocol/schemas/sym-expr.schema.json` covers shape only.
- **`prepare_id` derivation** — TS: SHA-256(`file:function:sorted-mock-symbols`)[:16] (shatter-ts/CLAUDE.md:65); Go: SHA-256(`file:function:mock-fingerprint:receiver_kind`+generic args) (shatter-go/CLAUDE.md:90). Same field, different identity semantics; documented only in CLAUDE.md prose.
- **Outcome derivation** (`deriveOutcome` TS, `outcomeFromResult` Go, `derive_execute_outcome` Rust) — three tables with different timeout-detection heuristics, each documented in its own CLAUDE.md.
- **Exec timeout env parsing** — three copies; defaults 15s/5s/5s (executor.ts:76, executor.go:18, handler.rs:40).
- **Console capture limits** — TS 4096 B/msg, Go none, Rust 4096 chars.
- **Protocol message structs** — Request/Response mirrored by hand in core (`Command`/`ResponseResult` enums), TS (discriminated unions), Go (flat struct), Rust FE (flat struct). Only enums are generated (see B4).

### A7. TODO/FIXME/HACK/XXX

0 in every frontend (`grep -rnE '\b(TODO|FIXME|HACK|XXX)\b'` over *.ts/*.go/*.rs excluding testdata). The codebase instead embeds tracker IDs (`str-xxxx`) in comments as the deferred-work mechanism; several of those now point at stale facts (see B5/B6). Nothing to list as "most concerning" under the classic markers.

---

## Part B — Protocol consistency

### B1. Type inventory (core `shatter-core/src/protocol.rs` = 51 top-level defs; plus `sym_expr.rs`, `execution_record.rs`)

| Missing in… | Types |
|---|---|
| **TS** (`shatter-ts/src/protocol.ts`) | `InvocationPlan`, `InvocationRequirement`, `ValuePlan`/`ValuePlanKind`, `ValueRequirement`/`Kind`, `RuntimeRequirement`/`Kind`, `UnsatisfiedRequirement`/`Kind`, `ReceiverFieldPlan`; no `plan` field on `ExecuteRequest` (135-146) or `PrepareRequest` (126-133); no `GetInvocationPlanRequest` / `InvocationPlanResponse` variants in the `Request` (177-186) / `Response` (276-286) unions even though the generated `Command`/`ResponseStatus` enums include `get_invocation_plan` / `invocation_plan`. |
| **Go** (`shatter-go/protocol/types.go`) | `ConnectionFailure`, `RuntimeCryptoBoundary(+Kind)` — no `connection_failures` / `runtime_crypto_boundaries` on `Response` (233-245). Enums `BoundOp`, `DependencyKind`, `DepDetectionKind`, `BranchType`, `ErrorCategory` are bare strings. |
| **Rust FE** (`shatter-rust/src/protocol.rs`) | `MockConfig`/`MockBehavior` (`mocks: Vec<Value>`), `PerformanceMetrics` (`performance: Value`), `LoopBodyState`, `DiscoveredDependency`/`DepDetectionKind`, `ConnectionFailure`, `RuntimeCryptoBoundary`, entire InvocationPlan family, `ExecuteResult` (flattened), `Command`/`ResponseResult` (flat structs). Tracked for the execute fields (`rust-execute-response-fields-partial`) and `details` (`rust-error-details-not-emitted`). |

### B2. Field-level mismatches

- **P2** Rust FE `FunctionAnalysis.is_async` (protocol.rs, `#[serde(skip_serializing_if = "is_false")]`) exists in no other binding, not in core `FunctionAnalysis`, not in `registry.yaml`, not in `function-analysis.schema.json`. It is an undeclared wire field silently dropped by core.
- **P3** Rust FE `Request.harness_mode` (protocol.rs, ~line 455) is not in `registry.yaml` `execute.field_model`; the comment concedes "no producer currently sets it on the wire".
- **P3** `ErrorCategory`: TS declares `"validation" | "runtime" | "infrastructure" | "unknown"` (protocol.ts:631-635); `registry.yaml:69` enum `error_category` has only three values; core `ErrorInfo.error_category` is `Option<String>` (execution_record.rs:99), so nothing fails — the registry and TS disagree with no enforcement.
- **P3** `BinOpKind`: core (sym_expr.rs:107-110) has `shl`, `shr`, `bit_clear` (Go-specific); TS `BinOpKind` (protocol.ts:514-532) and Rust FE `BinOpKind` (protocol.rs) omit them. Harmless while TS/Rust never consume Go output; still a narrower mirror of the kind the matrix just resolved for Rust.
- **P3** Core `ConnectionFailure.error_kind: String` vs TS `ConnectionFailureKind` union (protocol.ts:572-577); no registry enum.
- **P3** Rust FE mirrors are weakly typed relative to core: `ConditionOutcome.constraint: Value`, `InvocationOutcome.thrown_error: Value`, `side_effects: Vec<Value>`, `Request.kind: Option<String>` (GeneratorKind), `CryptoBoundary.direction/output/confidence: String`.
- **P3** Numeric width drift (benign): `Request.id` u64 (core/Rust) vs `int` (Go) vs `number` (TS); `TruncationInfo.original_lines/bytes` u32/u64 in Go vs number in TS; `RuntimeCryptoBoundary.ciphertext_param_index` `Option<i32>` vs `number`.
- **P3** Go `DiscoveredDependency.Kind` comment (types.go:553) lists two kinds; registry has three (`stubbed_import`).
- **P3** `ts-rust-execute-plan-not-implemented` says TS/Rust "accept the field on the wire" — TS has no `plan` type at all; it is ignored only because the parser doesn't reject unknown keys.

### B3. Naming

- Wire keys are uniformly snake_case in all four bindings (no camelCase `json:"…"` tags; no camelCase serde renames; no camelCase keys in protocol.ts). ✓
- **P3** Discriminator key is inconsistent *in the core contract itself*: `kind` for SymExpr/TypeInfo/SideEffect/ScopeEvent/SymConstraint/InvocationModel (protocol.rs:748, sym_expr.rs:21, execution_record.rs:69) vs `type` for TraceEvent/LiteralValue/ConstValue (protocol.rs:681, sym_expr.rs:66, execution_record.rs:82). Every frontend faithfully mirrors the inconsistency; `TraceEvent{type}` wrapping `ScopeEvent{kind}` is the most confusing case.
- **P3** Core Rust field names differ from wire names for the plan command (`requirements`→`invocation_requirements`, `plans`→`invocation_plans`, protocol.rs `GetInvocationPlan`/`InvocationPlan` variants) — fine, but a grep for the wire name finds only Go.

### B4. Generated bindings — are they generated?

- `scripts/protocol-codegen.py --check` → exit 0; `protocol/generated/manifest.json`, `shatter-ts/src/generated/protocol-enums.ts`, `shatter-go/protocol/protocol_enums_gen.go`, `shatter-rust/src/generated/protocol_enums.rs` all carry DO-NOT-EDIT headers and match the registry. Wired in `Taskfile.yml:296-310` (`protocol-codegen-check`) and the drift-patrol task. ✓
- **Only enums/consts are generated.** All message *shapes* are hand-maintained in four places. `registry.yaml` has a `field_model` per command, but `validate-protocol-registry.py` only checks it against the registry's own flat `fields:` list — never against source. That is how TS can lack `plan`, `get_invocation_plan`, and `invocation_plan` (B1) with every gate green.
- Go keeps hand-written duplicates of generated data: `types.go:88-112` (`SetupLevel` consts + `ValidSetupLevels`) and `constants.go:4-21` (`Err*`), reconciled by `generated_enums_test.go` (TestErrorCodeConstantsMatchGenerated, TestSetupLevelConstantsMatchGenerated, TestCommandCapabilitiesAreSubsetOfGenerated). Acceptable, but it is generate-then-mirror.
- **P1 The registry validator's source-parity layer is a silent no-op for TS and produces false warnings for Rust.** `extract_ts` (validate-protocol-registry.py:545-560) regex-matches `type Command = …;` in `shatter-ts/src/protocol.ts`; since str-1hlk.7 that file only re-exports from `./generated/protocol-enums.js`, so all three sets come back empty (verified: no match for Command/ResponseStatus/ErrorCode) and the script still prints "All checks passed". The Rust extractor greps `shatter-rust/src/protocol.rs` for command literals that live in `handler.rs`, yielding bogus warnings that `instrument`, `prepare`, `shutdown`, `get_invocation_plan` are "not found in shatter-rust" (three of the four are implemented and advertised at handler.rs:567-575). GOVERNANCE.md:260-281 presents this layer as a required gate.

### B5. parity-matrix.yaml divergence hygiene

- 10 `allowed_divergences`; required metadata present on all. Only `status: resolved` entries carry a date. `rust-protocol-enum-vocabulary-narrower` was `resolved_at: '2026-08-12'`; the 30-day grace (`DRIFT_RESOLUTION_GRACE_DAYS`, validate-parity.py:94) ends **2026-09-11** — validator currently warns "schedule removal within 8 day(s)" and will fail CI after that. **P2** remove the entry now.
- **P3** `tracked`/`accepted` entries (7 tracked-ish, 3 accepted) have no review/expiry date and no mechanism to force revisiting; e.g. `ts-rust-execute-plan-not-implemented` is deferred "until a real planner exists" with no owner action.
- **P2 Dangling references to divergence IDs that no longer exist in the matrix:**
  - `error-code-preflight-failed-typescript-only` — shatter-ts/CLAUDE.md:208, shatter-go/CLAUDE.md:206, shatter-rust/CLAUDE.md:171, shatter-go/protocol/constants.go:19, shatter-go/protocol/types.go:590, shatter-rust/src/protocol.rs:531 and :563. Worse, the surrounding claim ("Go/Rust declare but do not emit `preflight_failed`") is **false**: Go emits it at protocol/handler.go:394 and Rust at handler.rs:650 (`run_preflight`, sticky per-root preflight).
  - `loop-body-states-typescript-only` — shatter-ts/CLAUDE.md:52.
  - `rust-side-effects-not-captured` — shatter-rust/CLAUDE.md:25 ("status: resolved" — the entry has since been deleted).
- `conformance_cases.yaml` `known_drifts` (13-25) has 3 patterns; the first cites `side-effect-thrown-error-placement`, which is also not a matrix ID.

### B6. Capability declarations: code vs matrix vs CLAUDE.md

- **Handshake lists match the matrix exactly** (validate-parity passes): TS `SUPPORTED_CAPABILITIES` (handlers.ts:56-69) = 7 commands + complex_type {date, date_time, duration, reg_exp, url, big_int, buffer, error, symbol, closure}; Go (constants.go:30 + handler.go:303-306) = 8 commands incl. `get_invocation_plan` + {date, duration, url, reg_exp, ip_address, big_int, rational, big_decimal, error, go_byte}; Rust (handler.rs:567-584) = 7 commands + {uuid, url, date, date_time}. ✓
- CLAUDE.md prose vs matrix — stale in several places (**P2**, because the frontend-parity skill tells agents to read these as contracts):
  - shatter-ts/CLAUDE.md:147-150 and shatter-rust/CLAUDE.md:117-120: "declares support for `outcome` only" — matrix `feature_capabilities` has TS supported for `execute_capture`, `host_write_isolation`, `instrumentable_line_count`; Rust for `execute_capture`, `host_write_isolation`.
  - shatter-ts/CLAUDE.md:43-45: "TS is the **only** frontend that produces `ite`… Go and Rust… do not produce it" — contradicted by shatter-go/CLAUDE.md:52-70, the matrix (`affected_frontends: [rust]`), and analyzer.go:2201.
  - shatter-rust/CLAUDE.md:15: "Commands: handshake, analyze, instrument, execute, setup, teardown, generate, shutdown" — omits `prepare` (implemented, advertised).
  - `.claude/skills/frontend-parity/SKILL.md` is the most stale artifact: side-effect table (107-115) shows Go `thrown_error` ✗ and Go file_write/network/env ✗ (matrix: all `captured`); "Rust's analyze handler is a stub" (133; analyzer.rs is 4,475 lines); "Go lacks data flow tracking" (133); Rust timeout "stored, not yet applied — execute unimpl" (143). The skill says the matrix wins on conflict, but it is the document agents load first.
  - Root PARITY.md ("Last updated 2026-05-13") is consistent with the matrix on side effects and timeouts; it predates `get_invocation_plan`, `instrumentable_line_count`, and adapter capabilities and lists none of them.

---

## Recommendations (issue-ready)

1. **[P1] Fix `validate-protocol-registry.py` source extraction for TS and Rust (the source-parity gate is a silent no-op).** `extract_ts` regexes `type Command = …;` in `shatter-ts/src/protocol.ts`, which now only re-exports generated enums, so it returns empty sets and the script still reports success; the Rust extractor reads `protocol.rs` while command literals live in `handler.rs`, producing false "not found" warnings. Point both extractors at the generated enum files plus the handler dispatch sites (or delete the layer and rely on codegen `--check` + golden tests), and make an empty extraction a hard failure.

2. **[P1] Split `shatter-rust/src/executor.rs` (16,125 lines / 594 KB).** Move harness codegen (standalone, crate-backed, crate-bridge, axum adapter templates), the three harness caches, and the 8,500-line inline test module into separate modules/files under `src/executor/`. Keep the public `execute_function*` entry points stable so handler.rs is untouched.

3. **[P2] Generate or verify message shapes from `registry.yaml` `field_model`, not just enums.** TS `Request`/`Response` unions lack `plan`, `get_invocation_plan`, and `invocation_plan`; Rust FE carries undeclared `is_async` and `harness_mode`; Go lacks `connection_failures`/`runtime_crypto_boundaries`. Extend `protocol-codegen.py` to emit request/response field interfaces (TS/Go/Rust FE) from `field_model`, or add a validator step that diffs each binding's field names against `field_model` and fails on undeclared/missing fields.

4. **[P2] Purge dangling divergence references and the false "does not emit preflight_failed" claims.** `error-code-preflight-failed-typescript-only` is cited in 7 files (3 CLAUDE.md, constants.go:19, types.go:590, protocol.rs:531/563) but no longer exists, and Go (handler.go:394) and Rust (handler.rs:650) do emit the code. Also remove `loop-body-states-typescript-only` (ts CLAUDE.md:52), `rust-side-effects-not-captured` (rust CLAUDE.md:25), `side-effect-thrown-error-placement` (conformance known_drifts). Add a drift-patrol check that every divergence ID mentioned in *.md/*.go/*.rs/*.ts/*.yaml resolves to a matrix entry.

5. **[P2] Refresh `frontend-parity` skill and per-crate CLAUDE.md capability prose from the matrix.** Regenerate the skill's side-effect/ite/timeout tables from `parity-matrix.yaml` (or replace them with a pointer), fix "TS is the only frontend that produces ite", "outcome only" (TS/Rust), the Rust command list missing `prepare`, and the stale line numbers in shatter-ts/CLAUDE.md:17-18. Consider a `task parity` sub-check that greps CLAUDE.md capability tables against the matrix.

6. **[P2] Unify the SymExpr builders per frontend and write a one-page construction spec.** TS: collapse the three builders into one parameterized by an identifier resolver, delete the duplicate op tables, and fix `resolvePropertyChain` to consult the flow map. Go: move `SymExpr`/`SymConstraint` to a leaf package so `instrument` stops mirroring them, and reconcile true/false/nil, `strconv.Unquote`, nested selectors, and the `omitempty` differences. Rust: have `instrument.rs` build `protocol::SymExpr` and `serde_json::to_string` it instead of hand-formatting JSON, and cover arithmetic/bitwise/call forms so runtime constraints match static conditions. Spec should define call name/receiver form, literal normalization, path direction, and unknown-collapse rules.

7. **[P2] Record (or fix) the cross-frontend `call` SymExpr shape divergence.** Go emits `{name:"recv.Method", receiver:null}`; TS and Rust emit `{name:"Method", receiver:<expr>}`. Decide the canonical form in the spec from item 6, then either change Go's `callSymExpr`/`callExprToSymExpr` or add an `allowed_divergences` entry and a conformance `known_drifts` pattern.

8. **[P2] Remove `rust-protocol-enum-vocabulary-narrower` from `allowed_divergences` before 2026-09-11** and add an optional `review_by` date to `tracked` entries that the validator warns on, so non-resolved divergences cannot linger indefinitely.

9. **[P2] Stop excluding `encoding/json.Unmarshal` from errcheck in `shatter-go/.golangci.yml`.** Audit the call sites the exclusion currently hides (protocol decoding, config loading, recorder drain) and handle or propagate the errors; malformed input must not decode to zero values silently.

10. **[P2] Add ESLint (typescript-eslint) to shatter-ts** with the rules the ts-conventions skill already claims are enforced (`no-explicit-any`, `ban-ts-comment`, `consistent-type-imports`, `consistent-type-exports`, no default exports), a `lint` script, and a Taskfile hook in `ts:test-fast`. Remove the dead `eslint-disable` comments or keep them once the tool exists.

11. **[P3] Rust FE: replace `Mutex::lock().unwrap()` with poison-tolerant locking** in handler.rs (9 sites) and executor.rs (10 sites), and add `//!` module docs to generators.rs, handler.rs, lib.rs, main.rs, timing.rs, wasm_generator.rs (optionally `#![warn(missing_docs)]`).

12. **[P3] Align the `error_category` vocabulary.** TS emits/declares `"unknown"`; `registry.yaml` enum has three values; core stores a free string. Either add `unknown` to the registry enum and schemas, or drop it from TS, and make core's `error_category` an enum with `#[serde(other)]` fallback.

13. **[P3] Tidy Go dead-assignment and lock-file writes.** Delete `_ = fresh` (build/builder.go:277) and `_ = anchorImport` (launcher/launcher.go:391); check the `fmt.Fprintf(lockFile, pid)` results at launcher.go:616 and builder.go:188 since stale-lock detection reads that content back.

14. **[P3] Extend rapid property coverage to `reconstruct`, `config`, and `launcher` packages** (currently none), prioritising round-trip and malformed-input properties per the go-conventions skill.
