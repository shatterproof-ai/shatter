---
slug: ts-branchtype-known-answer-fixtures
kind: new
title: "TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path"
priority: P2
type: task
labels: [typescript, e2e, testing, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS known-answer E2E fixture per BranchType: analyze (id, line) == instrument (id, line) and both outcomes discovered via branch_path

## Problem

Branch-type capability claims for the TS frontend were closed without any per-type known-answer test. str-w0d.1 claimed constraint emission for `if, switch, ternary, &&, ||`, and str-wsg covered analyzer extraction only. The TS E2E suite (`shatter-core/tests/e2e_concolic.rs`) has no fixture per `BranchType` asserting that both arms are reached **through `branch_path`**. As a result, `switch`, `ternary` and value-position `&&`/`||` have been analyzed but never instrumented (ts-switch-ternary-instrumentation), and no gate noticed.

A known workaround was also written as prose instead of being filed. The enum E2E reads `raw_results` because switch emits no `branch_path`. This issue adds the missing gate, so that a branch type can only be declared supported with an end-to-end test behind it.

This was split from audit draft agent/23. The engine-level random-vs-concolic parity suite stays in engine-parity-e2e; the `_`-prefixed-param lint is underscore-binding-lint and the checklist/close-reason rule is pipeline-close-reason-rule (all shatter-concolic-and-engine-design bucket).

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- Generated `ALL_BRANCH_TYPES` (`shatter-ts/src/generated/protocol-enums.ts:78-88`): `else_if, for, if, logical_and, logical_or, select, switch, ternary, while`. `select` is Go-only.
- The TS analyzer emits them at `shatter-ts/src/analyzer.ts`:
  - `if`/`else_if` at :1340;
  - `switch` at :1374;
  - `ternary` at :1393;
  - `logical_and`/`logical_or` at :1410-1411;
  - `while` at :1436, and `do` also emits `while` (:1453);
  - `for` at :1477.
  - `for-of`/`for-in` emit **no** branch; they only walk the body (:1460-1464).
- The instrumentor probes only `if` and loop conditions. For `switch` it adds line records only (`instrumentor.ts:1393-1411`), and it has no `ConditionalExpression` handling.
- The prose workarounds are the only ones found by a grep of `shatter-ts/CLAUDE.md`, `shatter-ts/src/*.test.ts` and `e2e_concolic.rs` for workaround/limitation wording:
  - `shatter-ts/CLAUDE.md:285-287`;
  - `shatter-core/tests/e2e_concolic.rs:2947-2953`: "the TS instrumentor records switch-case *lines* but emits no `branch_path` decisions, so the orchestrator's path-based dedup collapses all switch executions into one empty-path entry".
- The E2E tests are `#[ignore = "subprocess E2E; run via task e2e-ts or core:test-ignored"]`. `task e2e-ts` is checksum-cached, so a Task "pass" can execute nothing; `task --force e2e-ts` re-runs it through the governed wrapper (`Taskfile.yml:591-609`).
- The instrument response carries no branch metadata: `InstrumentResponse` (`shatter-ts/src/protocol.ts:211-216`) has `instrumented`, `output_file` and `instrumentable_line_count`, and the internal `InstrumentResult` (`shatter-ts/src/instrumentor.ts:19-31`) exposes only `branchCount`. The branch ids and lines exist only as the first two arguments of the `__shatter_branch(<id>, <line>, ...)` calls in the instrumented source.
- Audit sources: finding frontend-ts-18 (with frontend-ts-02 and prior-17); `audits/2026-09-22/areas/frontend-ts.md` F18; `drafts/shatter-agent/23-mechanical-parallel-parity-gates.md`.

## Acceptance criteria

- [ ] A table-driven TS known-answer fixture set in `shatter-core/tests/e2e_concolic.rs` (or a new `e2e_concolic_ts_branch_types.rs` wired into `task e2e-ts`) has one small function per TS-emitted BranchType: `if`, `else_if`, `switch`, `ternary`, `logical_and`, `logical_or`, `while` (including a `do` variant) and `for`. Each function has a known triggering input for each outcome. `select` is excluded as Go-only, and the fixture file states this.
- [ ] Each fixture asserts **(a)** that the analyze branch `(id, line)` set equals the instrument branch `(id, line)` set for that function, where the instrument side is extracted by a test helper that parses the `__shatter_branch(<id>, <line>, ...)` calls in the file named by the instrument response's `output_file` (no protocol change is needed; if ts-switch-ternary-instrumentation adds branch metadata to the response, the helper may switch to it), and **(b)** that both outcomes of each branch are discovered through `branch_path` in `result.executions`, not `raw_results`, under both the default explorer and `--concolic`.
- [ ] Today, `switch`, `ternary`, `logical_and` and `logical_or` fail. Those cases are marked expected-fail with the ts-switch-ternary-instrumentation id, so the suite runs green, and each marker **flips to a failure when the case starts passing**, so it must be removed. No silent `#[ignore]`.
- [ ] A test asserts that every value in `ALL_BRANCH_TYPES` has either a fixture here or an explicit exclusion with a reason (for example `select`: Go-only). Adding a BranchType to the registry without a TS fixture then fails the test.
- [ ] **The known prose workaround is tied to its issue.** The `raw_results` workaround (`shatter-ts/CLAUDE.md:285-287`, `e2e_concolic.rs:2947-2953`) is edited to cite the ts-switch-ternary-instrumentation id, so it is removed when that issue lands. No wider sweep is part of this issue (see Out of scope).
- [ ] Close-time proof: `task --force e2e-ts` (the governed task; do not run the underlying `cargo test` bare, per AGENTS.md "Shared-Machine Resource Etiquette"). If the fixtures live in a new test file, wire it into `e2e-ts-governed` first. Paste the per-test lines showing every fixture executed, the expected-fail cases reported as expected-fail, and the `test result:` line. Record `task affected` `Gates selected`.

## Suggested approach

Model the fixtures on `examples/go/05-conditional-merge.go` and `shatter-core/tests/e2e_concolic_go.rs`. Keep each function tiny, so the triggering inputs are obvious. Get the analyze side from the frontend `analyze` response, and the instrument side by parsing the `__shatter_branch` probes in the instrumented `output_file`. This keeps the issue independent of ts-switch-ternary-instrumentation: the fixtures land first with expected-fail markers, and that issue flips them.

## Out of scope

- Fixing the instrumentor (ts-switch-ternary-instrumentation).
- Go/Rust per-BranchType fixtures. File follow-ups if wanted; per-frontend parity of emitted branch types is a candidate drift-patrol check.
- The engine_parity random-vs-concolic suite (engine-parity-e2e), the `_`-param lint (underscore-binding-lint), and the completion-checklist rule "test workarounds must be filed as issues" (pipeline-close-reason-rule).
- A repo-wide sweep of other frontends' docs and tests for further prose workarounds. It has no fixed end point; if wanted, it belongs with the pipeline-close-reason-rule checklist work or its own bounded issue.

## Related

- ts-switch-ternary-instrumentation (this bucket) turns the expected-fail cases green.
- engine-parity-e2e (shatter-concolic-and-engine-design bucket): the sibling split of agent/23.
- str-wsg, str-w0d.1, str-ts3n (closed without per-type E2E); str-u394l.4 (agent rules drift lint).

## Priority / type / size

P2 · task · size M
