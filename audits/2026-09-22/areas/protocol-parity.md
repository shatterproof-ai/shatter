# Area review: protocol and cross-frontend parity (2026-09-22)

Reviewer scope: `protocol/` (registry.yaml, schemas, fixtures, generated bindings, GOVERNANCE.md, PARITY.md, parity-matrix.yaml, conformance harness + golden tests), `scripts/validate-protocol-registry.py`, `scripts/validate-parity.py`, `scripts/protocol-codegen.py`, `Taskfile.yml` parity/conformance wiring, the frontend-parity skill, per-frontend protocol bindings (core `shatter-core/src/protocol.rs`, TS `shatter-ts/src/protocol.ts`, Go `shatter-go/protocol/types.go`, Rust FE `shatter-rust/src/protocol.rs`), and open tracker issues str-qwua7.7/.24/.34/.37, str-2fjn, str-qe9pp.

Worktree: `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22` @ `16794cef`. Observation only: nothing in the repo was edited. Mutation experiments ran on a **scratch copy** (`$SCRATCH/mut`) and a Go frontend binary was built into scratch (`go build -buildvcs=false -o $SCRATCH/pp/shatter-go .`).

Prior audit: `audit-2026-09-04/audits/2026-09-04/frontends-protocol.md` Part B. This review checks whether those items still hold and looks for what that audit missed, mainly **whether the parity machinery can detect drift at all**.

---

## 0. Headline

The protocol has a lot of governance machinery: registry, codegen, two validators, schema fixtures, a conformance harness, golden files, a parity matrix, a mirrored PARITY.md, a weekly expiry workflow and drift-patrol. Each piece is internally consistent and its own tests pass. Together they check much less than the docs claim:

* **Advertised vs dispatched commands are not checked.** In a scratch copy I removed the `prepare` dispatch from all three frontends and kept the handshake advertisement. `validate-protocol-registry.py` exited 0 and `validate-parity.py` printed "Parity check passed" (§2.1).
* **`known_drifts` in the conformance harness cannot match anything.** The patterns are regexes (`side_effects.*thrown_error`), but the harness tests them with a substring `in` check. No conformance case produces cross-frontend side-effect or condition output anyway (§2.2).
* **The published JSON Schemas reject real frontend output.** Go's analyze output for `if x<<2 > 8` fails `sym-expr.schema.json` because `shl` is not in the schema's op enum. The schemas are only ever checked against hand-written fixtures (§2.5).
* **Four of the eight parity-matrix sections are read by no script**: `side_effect_capabilities`, `feature_capabilities`, `adapter_capabilities` and `shared_wire_types` (§2.8).
* **`task parity` has no checksum sources for `parity-matrix.yaml`, `PARITY.md` or `validate-parity.py`.** When only those files change, the task is served from the go-task cache (§2.3).
* **The validator unit tests are not wired into any gate.** That includes the regression test that str-qwua7.7 just added (§2.4).

Prior-audit items that still hold with no commits: str-qwua7.24, .34, .35, .36, .37, str-2fjn. str-qwua7.7 is half done (Rust extraction fixed, TS still silently empty). str-qe9pp is stale-open: both of its symptoms were fixed months ago.

**Worth keeping:**
* The codegen `--check` pattern and the `codegen_parity.rs` / `generated_enums_test.go` mirror tests.
* The duplicate-key-rejecting YAML loader in validate-parity.
* The metadata and grace-window policy for divergences, backed by `parity-expiry.yml`.
* The structured `allowed_divergences` records, which carry owner, tracking issue and resolution condition.
* The handshake golden files.
* registry `field_model` for execute, which matches core `ExecuteResult` and `Command::Execute` exactly.

---

## 1. Validator runs (baseline)

```
$ python3 scripts/validate-protocol-registry.py
Registry: 10 commands, 11 statuses, 12 error codes
Warnings:
  shatter-rust/src/protocol.rs + handler.rs:
    commands: 'get_invocation_plan' in registry but not found in shatter-rust (may be unimplemented)
All checks passed (with informational warnings).        exit=0
$ python3 scripts/protocol-codegen.py --check            exit=0
$ python3 scripts/validate-parity.py --today 2026-09-22
Matrix: 10 commands, 32 complex_types, 9 allowed_divergences ... Parity check passed.   exit=0
$ python3 -m unittest scripts.test_validate_parity scripts.test_validate_protocol_registry
Ran 35 tests ... OK
```

Extractor output (imported the script module and dumped the sets):

| source | commands | statuses | error_codes |
|---|---|---|---|
| core | 10 | 11 | 12 |
| **ts** | **0** | **0** | **0** |
| go | 10 | 11 | 12 |
| rust FE | 9 | 5 | 12 |

The TS extraction is still empty, because `extract_ts` regexes `type Command = ...;` in a file that only re-exports generated enums (`shatter-ts/src/protocol.ts:16-41`). `validate()` then silently skips the empty sets (`if not src_set: continue`, validate-protocol-registry.py:646-648). Rust statuses find 5 of 11, and nothing notices because missing statuses are never reported.

---

## 2. Findings

### 2.1 [P1 L4/L5] Advertised-but-not-dispatched commands pass every static parity gate
Mutation experiment on a scratch copy of `scripts/`, `protocol/` and the frontend protocol/handler sources:
1. TS: `case "prepare": {` → `case "prepare_DISABLED": {` in `shatter-ts/src/handlers.ts:556`.
2. Rust: the line `"prepare" => (self.handle_prepare(resp, req), false),` deleted from `shatter-rust/src/handler.rs:547`.
3. Go: `case "prepare":` at `shatter-go/protocol/handler.go:273` changed to an unreachable label.

Result: `validate-protocol-registry.py` reports only a *warning* ("'prepare' in registry but not found in shatter-rust (may be unimplemented)") and exits 0. `validate-parity.py` prints "Parity check passed." Why nothing fails:
* validate-parity reads only the handshake capability arrays: `SUPPORTED_CAPABILITIES`, `CommandCapabilities` and `handle_handshake` (validate-parity.py:294-386).
* The golden tests compare the same advertised list (`protocol/conformance/golden/handshake/*.json`).
* The only conformance `prepare` case is `prepare_supported_rust` with `frontends: [rust]` (conformance_cases.yaml:167-168). TS and Go prepare dispatch have no runtime parity check.

The matrix says `prepare: implemented` for all three. No check connects "matrix says implemented" to "the frontend dispatches it".

### 2.2 [P1 L1] Conformance `known_drifts` are inert: regex patterns matched as substrings, and no case can produce them
`protocol/conformance/conformance_harness.py:667-676`:
```python
known_drift_patterns = [kd["pattern"] for kd in config.get("known_drifts", [])]
for d in drifts:
    if any(pat in d for pat in known_drift_patterns):
```
The patterns are `"side_effects.*thrown_error"`, `"side_effects.*global_mutation"` and `"condition.*ite"` (conformance_cases.yaml:13-24). Demonstration:
```
d='execute_x -- side_effects[0].kind differs: thrown_error vs console_output'
[p in d for p in pats]          -> [False, False, False]
[bool(re.search(p,d)) ...]      -> [True, False, False]
```
The cross-frontend structural comparison needs at least two frontends to respond to the same case (`conformance_harness.py:641`). Every execute and success-path analyze case is single-frontend (`execute_outcome_shape_go/ts/rust`, `adapter_http_nethttp_go_analyze`, `analyze_runtime_value_go`, `planner_runtime_value_go`, `prepare_supported_rust`, `execute_with_invalid_prepare_id`). So cross-frontend comparison only covers handshake, error paths, setup, teardown, generate and shutdown. GOVERNANCE.md ("Known Drifts"), PARITY.md step 4 and the frontend-parity skill all tell agents to register drift in `known_drifts`. Doing so has no effect. The first entry also cites `side-effect-thrown-error-placement`, which is not a matrix ID.

### 2.3 [P2 AGENT] `task parity` checksum sources omit the files it validates
`Taskfile.yml:245-263` `parity.sources` lists registry.yaml, golden files, generated/manifest.json, validate-protocol-registry.py, protocol-codegen.py, and frontend sources. It does **not** list `protocol/parity-matrix.yaml`, `protocol/PARITY.md` or `scripts/validate-parity.py`. `parity-governed` (:265-276) runs `validate-parity.py`, which validates exactly those files. `scripts/affected-gates.py:136-140` correctly selects `parity` for any `protocol/` change, but in a worktree that already ran parity, go-task skips it as up to date. Result: a matrix-only or PARITY.md-only edit is "verified" without executing, and date-based expiry is never re-evaluated locally. This is consistent with the existing memory note that "task check/affected are checksum-cached (a pass may execute nothing)".

### 2.4 [P2 AGENT] Validator unit tests are wired to no gate
`scripts/test_validate_parity.py` and `scripts/test_validate_protocol_registry.py` (35 tests, currently green) appear in no Taskfile target and no workflow. `git grep` finds them only in `protocol/GOVERNANCE.md:120-121` as a manual instruction. The `meta` task (Taskfile.yml:440-470) runs 17 other `scripts.test_*` modules. So the regression test added by the str-qwua7.7 fix (commit 4cf2165f, `scripts/test_validate_protocol_registry.py +38`) never runs in CI. The same applies, outside this area, to `test_docs_smoke.py`, `test_gate_event_log.py`, `test_gate_pressure.py`, `test_perf_compare.py` and `test_build_cache_doctor.py` (handed to the tests/CI reviewer).

### 2.5 [P2 L2] Published schemas reject real frontend wire output; nothing validates live output against them
I built the Go frontend and ran analyze on a scratch fixture (`func Shift(x int) int { if x<<2 > 8 {...} }`, plus `strings.HasPrefix(s,"go_")` and a method call `u.IsAdmin()`). I then validated the output with `protocol/schemas`' own resolver:
```
analyze response-schema errors: 1
  Shift sym-expr errors: 1  ... "'shl' is not one of ['eq','ne','lt','le','gt','ge','add','sub','mul','div','mod','and','or','bitwise_and', ...]"
```
* Core `BinOpKind` has `Shl/Shr/BitClear` (`shatter-core/src/sym_expr.rs:107-110`), and Go emits them (`shatter-go/protocol/analyzer.go:2440-2444`, `instrument/symextract.go:215-219`).
* `sym-expr.schema.json`'s op enum and PROTOCOL.md "Binary Operators" (PROTOCOL.md:546-548) both omit them. Core `ConstValue::Complex` (sym_expr.rs:76-79) is also absent from the schema's const `type` enum.
* `protocol/schemas/test_schema_validation.py` validates only hand-written fixtures under `protocol/fixtures/`. The conformance harness uses its own `required_fields` shape checks and never loads a JSON Schema.

The same run shows the Go wire noise the prior audit noted: every node carries `"path": null, "args": []`.

### 2.6 [P2 L2/L3] protocol/PARITY.md hand-mirrors the matrix and has drifted; the validator only syncs divergence IDs
`validate-parity.py:419-445` (`parity_md_divergence_ids`) compares only `### \`<id>\`` headings. Content drift found:
* PARITY.md:76-78 says Rust has "No complex type capabilities are implemented yet". Registry, matrix, golden `handshake/rust.json` and `handler.rs` all have uuid, url, date and date_time.
* The Go complex-type table (PARITY.md:62-74) omits `go_byte`, which is in registry `frontends.go.complex_type_capabilities`. PARITY.md:45 cites `rune` as Go-only, but rune is not in Go's list.
* The command table (PARITY.md:29-39) omits `get_invocation_plan`.
* `adapter-owned-instrumentation-coverage-partial` has "Affected frontends: go, rust" (PARITY.md:283). The matrix has `[rust]` (parity-matrix.yaml:1157), and its text says Go closed the gap in str-1qd5i (closed 2026-07-10).
* PARITY.md:139 says "All 11 error codes". The registry has 12.
* A programmatic diff of all 9 divergence blocks: the adapter entry is the only affected_frontends mismatch. Two others differ only in tracking_issue prose ("none (intentional permanent divergence)").

### 2.7 [P2 L3] GOVERNANCE.md, the "mandatory" process, omits most of the parity machinery
`grep -n "parity-matrix|validate-parity|protocol-codegen|generated|PARITY.md|allowed_divergences" protocol/GOVERNANCE.md` finds no matches.
* Step 1 says "Update the registry" but never says to regenerate bindings (`protocol-codegen.py --write`). An agent following it hits a `--check` failure.
* The step 5 file table (GOVERNANCE.md:62-68) points TS at `protocol.ts`, whose vocabulary is generated, and points Rust only at `protocol.rs`, although commands dispatch in `handler.rs`.
* "Run all five checks" omits codegen and validate-parity.
* "CI Integration" (:175-182) names `task schemas/conformance/golden-test`. The real gate is `task check` → `check-integration` → `conformance` + `parity`.
* The doc names two authorities. Line 7 says the registry is the "single source of truth". Step 4 (:57) says core `protocol.rs` "is the authoritative implementation — frontends must match it".
* Last touched 2026-05-05.

### 2.8 [P2 L4] Capability data is hand-replicated in six places, and half the matrix is validated by nothing
The same capability facts live in:
1. frontend source (handshake arrays)
2. `registry.yaml frontends.*.command_capabilities/complex_type_capabilities`
3. `parity-matrix.yaml commands/complex_type_capabilities`
4. `conformance/golden/handshake/*.json`
5. `protocol/PARITY.md` tables
6. crate CLAUDE.md files and the frontend-parity skill

Validators cover pairs (1↔3, 2↔3, 1↔4). Copies 5 and 6 are unchecked and stale (§2.6, §2.12). `git grep -l "side_effect_capabilities|feature_capabilities|adapter_capabilities|shared_wire_types"` finds them only in the matrix, CLAUDE.md, the skill, conformance comments and one prose mention in explore.rs. No script reads them, so the side-effect, feature and adapter capability claims are documentation that looks like a contract.

### 2.9 [P2 L2] PROTOCOL.md's execute section documents about half the wire
PROTOCOL.md:209-334 shows these request fields: `function, inputs, mocks, capture` (+ id/version). It shows these response fields: `return_value, thrown_error, branch_path, lines_executed, calls_to_external, path_constraints, side_effects, performance`.

Core `Command::Execute` (protocol.rs:412-441) and registry `field_model.execute` also carry `setup_context, prepare_id, execution_profile, plan`. The response also has `scope_events, loop_body_states, capture_truncation, discovered_dependencies, connection_failures, runtime_crypto_boundaries, outcome`. The registry field_model matches core exactly, so it could generate the doc table.

### 2.10 [P2 L2/L4] Core protocol doc comments misdescribe the fields; `runtime_crypto_boundaries` has no semantic consumer
* `shatter-core/src/protocol.rs:1169-1174` says of `runtime_crypto_boundaries`: "The core engine uses this to apply boundary splitting". The only reads are `tracing::debug!` in `explorer.rs:1606-1616` ("will be used for boundary splitting in a future solver integration pass") and `orchestrator.rs:3187-3196`.
* `protocol.rs:1177-1185` says of `outcome`: "TS / Rust frontends do not currently emit this field". But TS emits it (`executor.ts:3252 response.outcome = deriveOutcome(rawResult)`), Rust emits it (`handler.rs:1085`), and the matrix `feature_capabilities.outcome` is supported for all three.
* `protocol.rs:2027` says "Canonical error code list (11 codes)" on a `[_; 12]` array, and claims that adding a variant "causes a compiler error". A fixed array literal does not do that.

### 2.11 [P2 L2] Go never emits `connection_failures` / `runtime_crypto_boundaries`, and the matrix does not say so
The fields are absent from `shatter-go/protocol/types.go` Response (:227-245; `git grep` finds 0 non-test Go hits). `update_live_first_states` (explorer.rs:885-910, called from orchestrator.rs:3185 too) is driven only by `connection_failures`, so the LiveFirst → autonomous-mock fallback can never trigger for Go targets. The only matrix record is the Rust entry `rust-execute-response-fields-partial` and its vague "TypeScript and Go emit the subset each supports" (parity-matrix.yaml:1172-1190). The prior audit B1 listed the missing Go types. The behavioural consequence and the matrix gap are new.

### 2.12 [P2 AGENT] Divergence entries marked `tracked` point at closed issues
* `go-symbolic-http-request-body`: tracked, tracking_issue `str-e41w`, **closed 2026-07-05**. It is also named `go-` but its `affected_frontends` are `[typescript, rust]`.
* `ite-symexpr-production-partial`: tracked, `str-1hlk.17`, **closed 2026-05-12**. Rust ite production has no open issue.
* `ts-rust-execute-plan-not-implemented` → `str-1hlk.16` is `deferred`.

`validate_divergence_metadata` (validate-parity.py:445-600) checks only that `tracking_issue` is a non-empty string and that `none` is used only when accepted. It never checks that the issue exists or is open.

### 2.13 [P2 AGENT] str-qwua7.34 (dangling divergence IDs / false preflight claim) is untouched, and the stale prose has spread
`git grep` over non-audit, non-beads files still finds:
* `error-code-preflight-failed-typescript-only` in shatter-go/CLAUDE.md:206, constants.go:19, types.go:590, shatter-rust/CLAUDE.md:171, shatter-rust/src/protocol.rs:531 and :563, and shatter-ts/CLAUDE.md:208
* `loop-body-states-typescript-only` in shatter-ts/CLAUDE.md:52
* `rust-side-effects-not-captured` in shatter-rust/CLAUDE.md:25
* `side-effect-thrown-error-placement` in conformance_cases.yaml:17

The false claim is still there: shatter-go/CLAUDE.md:206 says "The Go frontend does not currently emit either". Go emits it at `handler.go:394` and Rust at `handler.rs:650`. Separately, `shatter-go/CLAUDE.md:138` ("TS and Rust currently declare `outcome` only") is a stale-capability location that str-qwua7.24's list does not name. `git log --all --grep` shows 0 commits for str-qwua7.24/.34/.35/.36/.37 and str-2fjn.

### 2.14 [P2 AGENT] str-qwua7.37 rests on a wrong premise; implementing it literally would break Go string solving
str-qwua7.37 "Step 0" says Go's `name:"recv.Method", receiver:null` form "cannot be solved — it is broken for the solver" and requires Go to switch to `name`=bare and `receiver`=expr. That is wrong for the case that matters to the solver:
* `shatter-core/data/string-ops.yaml:33-34` declares `{ language: go, method: "strings.HasPrefix", style: free }`.
* `solver.rs:918-924` `receiver_and_first_arg` has an explicit "Go-style: no receiver, two positional args" branch, with tests at solver.rs:2420-2633.
* The live Go output for `strings.HasPrefix(s,"go_")` is `{"kind":"call","name":"strings.HasPrefix","args":[param s, const "go_"]}` (scratch run), and that is solvable.

If an agent applied the issue as written, `strings` would become a receiver and working Go string constraints would regress. The divergence that does exist is **method calls on values**: Go emits `u.IsAdmin()` as `{"name":"u.IsAdmin","args":[]}`, which drops the receiver, so `collect_param_names` (sym_expr.rs:138-145) loses param `u` for branch-parameter attribution. The issue also cites `data/string-ops.yaml`; the real path is `shatter-core/data/string-ops.yaml`.

### 2.15 [P2 AGENT] str-qwua7.7 is half-landed and its direction conflicts with GOVERNANCE
Commit 4cf2165f (2026-09-21) fixed only the Rust command extraction. Its message says the vocabulary cross-check "is no longer this script's job". The TS extractor still returns empty sets and passes. GOVERNANCE.md:89-91 still describes a source-name parity layer for "core and every frontend". The issue's acceptance criterion ("empty source extraction must fail") is unmet, and the issue stays open. The Rust `get_invocation_plan` "may be unimplemented" warning prints on every run, although the matrix marks it `not_implemented` on purpose. That permanent warning noise teaches agents to ignore the warning channel.

### 2.16 [P3 AGENT] str-qe9pp is stale-open
The issue (opened 2026-06-17) reports a Rust handshake golden mismatch and a `prepare_supported_rust` 30 s timeout. The golden was updated in `750b7ff8` (2026-06-18, "parity: update Rust handshake golden for complex_type capabilities"). The timeout was fixed by str-uoclg ("Rust prepare conformance timeout", closed 2026-08-13). The issue is still `open` with no notes.

### 2.17 [P3 AGENT] The frontend-parity skill is still stale and points at the inert mechanism
`.claude/skills/frontend-parity/SKILL.md` was last changed 2026-05-05.
* Lines 37-41 show Go `thrown_error` ✗ and `file_write/network_request/environment_read` ✗ for all frontends. The matrix and PARITY.md show Go captured.
* Line 59 says "TS is the only frontend that produces ite ... Rust's analyze handler is a stub".
* Line 69 says the Rust timeout is "stored, not yet applied — execute unimpl".
* It tells agents to "document the drift in `known_drifts`" (inert, §2.2). It never mentions the PARITY.md mirror rule or `validate-parity.py`.

This confirms str-qwua7.24. The new points are the known_drifts advice and the missing validate-parity step.

### 2.18 [P3 AGENT] frontend-issue-template.md parity checklist is TS-only
`protocol/frontend-issue-template.md:13-19` lists only the TS `buildSymExpr` / `buildSymExprWithFlow` pair and explorer vs orchestrator. It omits Go's `protocol/analyzer.go` vs `instrument/symextract.go` builders, Rust `analyzer.rs build_sym_expr` vs `instrument.rs constraint_for_expr`, and any `parity-matrix.yaml` / `allowed_divergences` / PARITY.md step. These are exactly the parallel paths where str-qwua7.35/.36 found drift.

### 2.19 [P3 L3] Test-double frontends clutter `protocol/`, and one is dead
There are 15 `*-frontend.sh` scripts at `protocol/` root, next to the normative docs. `delayed-execute-frontend.sh` has 0 references (`git grep -c` excluding audits/beads; last touched 145e94a7, 2026-05-24).

### 2.20 [P3 L4] Codegen covers 6 of the registry's enums; the 13 `enums:` entries are unenforced
The generated TS, Go and Rust files export only PROTOCOL_VERSION, commands, statuses, error codes, setup levels, generator kinds and branch types. Registry `enums:` also defines `outcome_status, value_plan_kind, value_requirement_kind, runtime_requirement_kind, apply_policy, error_category, unsatisfied_requirement_kind, discovered_dependency_kind, trace_event_type, crypto_boundary_kind`. None are generated or checked against source, which is why TS `ErrorCategory` still has `"unknown"` (protocol.ts:631-635) while the registry enum has three values (prior B2, still true).

---

## 3. Prior-audit (2026-09-04) Part B status

| Prior item | Status 2026-09-22 |
|---|---|
| B4 validator TS no-op / Rust false warnings (str-qwua7.7) | Rust fixed (4cf2165f); TS still empty; issue open (§2.15) |
| B5 dangling IDs (str-qwua7.34) | Unchanged; all citations present (§2.13) |
| B5 expired `rust-protocol-enum-vocabulary-narrower` | Fixed (5e3aa49c) |
| B6 CLAUDE.md/skill capability prose (str-qwua7.24) | Unchanged + extra location (§2.13, §2.17) |
| B1 TS lacks InvocationPlan/plan; Go lacks connection_failures etc. | Unchanged (git grep 0 hits) |
| B2 Rust FE `is_async` undeclared field | Unchanged (shatter-rust/src/protocol.rs:494-496) |
| B2 error_category "unknown" | Unchanged (§2.20) |
| A5 Go call-shape divergence (str-qwua7.37) | Divergence real for methods; issue premise wrong for free functions (§2.14) |
| str-2fjn shape cross-validation | No commits; still the key missing control |

## 4. Grades

* L1 (script and harness code quality): C+. The scripts are readable and unit-tested, but the harness has a matching bug and the validators have silent empty-set skips.
* L2 (docs ↔ code): D. PARITY.md, PROTOCOL.md, GOVERNANCE, core doc comments, CLAUDE.md files and the skill all drift.
* L3 (docs organisation): C. There are too many overlapping authorities, and the test doubles sit in the normative directory.
* L4 (design): C. The one-registry design is good in principle, but only enums are generated and capabilities are replicated six times.
* L5 (does the parity machinery achieve its goal of detecting drift?): D. It detects handshake-list drift well and little else.
* AGENT: D. Stale-open and wrong-premise issues, unwired validator tests, a missing checksum source, and agent-facing docs that point at an inert mechanism.
