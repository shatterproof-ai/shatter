---
slug: engine-parity-e2e
kind: new
title: "Replace prose-only random-vs-concolic parity with gates: engine_parity E2E suite through the pipeline, `_`-param lint, close-reason call-site rule"
priority: P2
type: task
labels: [audit-2026-09-22, parity, e2e, testing, quality-gates]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Replace prose-only random-vs-concolic parity with gates: engine_parity E2E suite through the pipeline, `_`-param lint, close-reason call-site rule

## Problem

CLAUDE.md warns repeatedly about parallel code paths: the random explorer vs the concolic orchestrator, and the CLI wiring for `--concolic` vs the default. Enforcement, however, is only "grep for the parallel path". This audit found at least seven random-vs-concolic drifts that survived, each filed separately:

- `--setup` is ignored under concolic (concolic-setup-teardown);
- dynamic mock variation regressed (concolic-mock-variation-regression);
- the refine phase drops `prepare_id` and `execution_profile` (concolic-refine-execute-builder);
- shrinking runs after teardown;
- capture is hard-coded (str-qwua7.5);
- path identity differs (engine-path-identity-budget-config);
- float-probe paths are under-counted (float-probe-paths-uncounted).

Two issues were also closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly, and str-55ep, which fixed a dead shrinker copy.

The per-BranchType TS known-answer fixtures from the original draft are filed separately as ts-branchtype-known-answer-fixtures (bucket shatter-frontend-ts).

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/tests/e2e_concolic.rs:1553-1558`: `orchestrator_explore_with_setup_context` ("This is the parity test for the orchestrator path") injects setup context straight into `orchestrator::explore`. The production callers (`pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`, `observe.rs:186`, per core-03) pass `None`.
- `grep -c "pipeline_orchestrator\|run_pipeline"` gives `e2e_concolic.rs` 1, `e2e_concolic_go.rs` 0 and `e2e_concolic_rust.rs` 0. The Go and Rust E2E suites never exercise the pipeline wiring where drift happens.
- `_`-prefixed unused bindings hid the mock-variation regression:
  - `shatter-core/src/orchestrator.rs:2147`: `_mock_params: &[MockParam]`
  - `orchestrator.rs:2643`: `let _initial_mocks = ...`
- CLAUDE.md "Completion Checklist" (`CLAUDE.md:43`) accepts unit/API tests as proof of pipeline behaviour for anything except the named E2E commands. There is no requirement to name the production caller.

## Acceptance criteria

- [ ] New E2E suite `shatter-core/tests/engine_parity.rs`, wired into `task e2e`. It runs a table of fixtures × {random, concolic} through `pipeline_orchestrator`/`run_pipeline` or the CLI entry point, never `orchestrator::explore` or `explorer::explore_function` directly. It asserts:
  - path count;
  - reached lines;
  - that a `--setup` side effect is visible;
  - that a mock-dependent branch is reached;
  - that the capture flag is honoured.

  It covers at least one TS, one Go and one Rust fixture. Known-divergent cases are marked expected-fail with the tracking issue ID in the marker, so the suite runs green today and flips when each drift is fixed.
- [ ] A lint script, wired into `check-static`, flags `_`-prefixed parameters or `let` bindings in `shatter-core/src` and `shatter-cli/src` (non-test code) that lack a `TODO(str-...)` comment on the same or previous line. Existing hits are fixed or annotated.
- [ ] CLAUDE.md Completion Checklist gains a rule: a close reason for a pipeline feature or fix names the production call site exercised and the pipeline-level test that proves it. Test workarounds recorded only in CLAUDE.md prose (for example `shatter-ts/CLAUDE.md:283-287`, "the e2e reads `raw_results`" because switch emits no `branch_path`) must be filed as issues. Link bento close-reason-evidence (bento bucket) so the generic bento rule and this repo rule agree.
- [ ] Proof at close:
  - forced (uncached) `task e2e` output showing `engine_parity` executed, with its pass/expected-fail counts;
  - the lint script run on a planted `_unused` binding, showing it fails;
  - the CLAUDE.md diff.

## Suggested approach

Split into child tasks when claimed: (1) suite scaffold + TS rows, (2) Go/Rust rows, (3) lint, (4) CLAUDE.md rule. The path-count row depends on engine-path-identity-budget-config; start it as expected-fail.

## Out of scope

- Fixing the individual engine drifts (filed separately as product bugs).
- Per-BranchType TS fixtures (ts-branchtype-known-answer-fixtures).
- The module-reachability dead-code check (core-dead-code-removal).
- The module-graph cycle check (str-qwua7.29).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, parity, e2e, testing, quality-gates
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: ts-branchtype-known-answer-fixtures, engine-path-identity-budget-config, concolic-setup-teardown, concolic-mock-variation-regression, concolic-refine-execute-builder, float-probe-paths-uncounted, close-reason-evidence (bento); str-qwua7.29, str-qwua7.51, str-inct, str-qwua7.5, bento-m4en, bento-a0nz
- Source findings: core-22 (draft shatter-agent/23, split)
