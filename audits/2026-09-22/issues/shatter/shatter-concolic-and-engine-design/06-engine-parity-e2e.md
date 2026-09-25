---
slug: engine-parity-e2e
kind: new
title: "Add an engine_parity E2E suite that runs fixtures under random and concolic through the production pipeline, with strict expected-divergence markers"
priority: P2
type: task
labels: [audit-2026-09-22, parity, e2e, testing]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add an engine_parity E2E suite that runs fixtures under random and concolic through the production pipeline, with strict expected-divergence markers

Scope note: the slug is kept from the earlier combined draft. The `_`-binding lint moved to underscore-binding-lint. The CLAUDE.md close-reason rule moved to pipeline-close-reason-rule. The per-BranchType TS fixtures are ts-branchtype-known-answer-fixtures (bucket shatter-frontend-ts).

## Problem

CLAUDE.md warns repeatedly about parallel code paths (the random explorer vs the concolic orchestrator, and the CLI wiring for `--concolic` vs the default), but the only enforcement is "grep for the parallel path". This audit found at least seven random-vs-concolic drifts that survived, each filed separately:

- `--setup` is ignored under concolic (concolic-setup-teardown);
- dynamic mock variation regressed (concolic-mock-variation-regression);
- the refine phase drops `prepare_id` and `execution_profile` (concolic-refine-execute-builder);
- shrinking runs after teardown;
- capture is hard-coded (str-qwua7.5);
- path identity differs (engine-path-identity-budget-config);
- float-probe paths are under-counted (float-probe-paths-uncounted).

Two issues were also closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly, and str-55ep, which fixed a dead shrinker copy.

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/tests/e2e_concolic.rs:1553-1558`: `orchestrator_explore_with_setup_context` ("This is the parity test for the orchestrator path") injects setup context straight into `orchestrator::explore`. The production callers (`pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`, `observe.rs:186`, per core-03) pass `None`.
- `grep -c "pipeline_orchestrator\|run_pipeline"` gives `e2e_concolic.rs` 1, `e2e_concolic_go.rs` 0 and `e2e_concolic_rust.rs` 0. The Go and Rust E2E suites never exercise the pipeline wiring where drift happens.

## Acceptance criteria

- [ ] New E2E suite `shatter-core/tests/engine_parity.rs`, wired into `task e2e`. It runs a table of fixtures × {random, concolic} through `pipeline_orchestrator`/`run_pipeline` or the CLI entry point. It never calls `orchestrator::explore` or `explorer::explore_function` directly; a grep in the close note shows this. It covers at least one TS, one Go and one Rust fixture.
- [ ] Each row asserts, for both engines, under a stated budget and fixed seed:
  - every expected return behaviour or branch outcome of the fixture is reached (coverage expectation, not equal path counts);
  - a `--setup` side effect is visible to the function under test;
  - a mock-dependent branch is reached;
  - the capture setting is honoured.
- [ ] **Expected-divergence semantics.** A known drift is recorded as a marker on one row and one named assertion: `expect_divergence(assertion = "setup_visible", engine = Concolic, issue = "str-...")`. The rules:
  - every assertion in the row still executes; there is no `#[ignore]` and no blanket allowance;
  - the marked assertion must **fail** in the marked engine. If it passes, the test fails with "unexpected pass: remove the marker and close <issue>";
  - every unmarked assertion in the row must pass, so an unrelated failure is never hidden by the marker;
  - the suite prints, at the end, a count of passing, expected-divergent and unexpected-pass assertions.
- [ ] A self-test of the harness shows all three cases: a row with a marker on an assertion that passes fails the suite; a marked row with an unrelated failing assertion fails the suite; a correctly marked row passes.
- [ ] Every drift in the Problem list that is reachable from these fixtures has a row, either passing or marked with its issue ID.
- [ ] Proof at close: forced (uncached) `task e2e` output showing `engine_parity` executed, with its pass / expected-divergent / unexpected-pass counts, and the harness self-test output.

## Suggested approach

Build the scaffold and the TS rows first, then add Go and Rust rows. The Loopy row from engine-path-identity-budget-config checks that both return behaviours are reached, not path counts.

## Out of scope

- Fixing the individual engine drifts (filed separately).
- The `_`-binding lint (underscore-binding-lint) and the close-reason rule (pipeline-close-reason-rule).
- Per-BranchType TS fixtures (ts-branchtype-known-answer-fixtures).
- The module-reachability check (core-reachability-gate) and the module-graph cycle check (str-qwua7.29).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, parity, e2e, testing
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: ts-branchtype-known-answer-fixtures, engine-path-identity-budget-config, concolic-setup-teardown, concolic-mock-variation-regression, concolic-refine-execute-builder, float-probe-paths-uncounted, underscore-binding-lint, pipeline-close-reason-rule; str-qwua7.29, str-qwua7.51, str-inct, str-qwua7.5
- Source findings: core-22 (draft shatter-agent/23, split)
