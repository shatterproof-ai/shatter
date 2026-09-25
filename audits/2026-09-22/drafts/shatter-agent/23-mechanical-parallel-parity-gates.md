# Replace prose-only parallel-path parity with gates: engine_parity E2E, per-BranchType known-answer fixtures, caller-named closures

- Priority: P2
- Type: task
- Labels: agents,parity,e2e,testing,quality-gates
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.29, str-qwua7.51; related bento-m4en)
- Source findings: core-22, frontend-ts-18
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
CLAUDE.md warns repeatedly about parallel code paths (random explorer vs
concolic orchestrator; buildSymExpr vs buildSymExprWithFlow), but enforcement is
"grep for the parallel path". This audit found at least seven random-vs-concolic
drifts that survived (setup ignored in concolic, mock variation regressed,
refine drops prepare_id, shrink after teardown, capture hard-coded, path
identity differs, float-probe path under-count), and TS switch/ternary/
value-position `&&`/`||` branches analyzed but never instrumented. Two issues
were closed on evidence from non-production paths (str-0s76.6 via a test that
calls `orchestrator::explore` directly; str-55ep fixed a dead shrinker copy);
str-w0d.1 claimed switch/ternary constraint emission that does not exist.

## Current Code Facts
- `shatter-core/tests/e2e_concolic.rs:1555` injects setup context directly into
  `orchestrator::explore`; production callers
  (`pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`,
  `observe.rs:186`) pass `None`.
- `e2e_concolic_go.rs` and `e2e_concolic_rust.rs` never call
  `pipeline_orchestrator`/`run_pipeline`.
- `shatter-ts/CLAUDE.md:285-287` records the "enum e2e reads raw_results because
  switch emits no branch_path" workaround as prose, not an issue.
- BranchTypes emitted by the TS analyzer (`shatter-ts/src/analyzer.ts:1364-1471`):
  if, else_if, switch, ternary, logical_and, logical_or, while, for, do.
- `_`-prefixed unused params hid the mock-variation regression
  (`orchestrator.rs:2147 _mock_params`, `:2643 _initial_mocks`).

## Acceptance Criteria
- New E2E suite `engine_parity` (shatter-core/tests): table of fixtures ×
  {random, concolic} run through `pipeline_orchestrator`/CLI entry, asserting
  path count, reached lines, setup side effect visible, mock-dependent branch
  reached, capture flag honoured. Initially marks known-divergent cases as
  expected-fail with issue IDs, so the suite runs green and flips when fixed.
- TS known-answer fixture per BranchType asserting analyze branch (id, line) ==
  instrument (id, line) and both outcomes discovered via `branch_path`.
- A lint script (wired into `check-static`) flags `_`-prefixed params/bindings
  in `shatter-core/src` without a `TODO(str-...)` comment on the same or
  previous line.
- CLAUDE.md Completion Checklist: close reasons for pipeline features name the
  production call site and the pipeline-level test that exercises it; test
  workarounds must be filed as issues, not only noted in CLAUDE.md.

## Suggested Approach
Split into child tasks when claimed (suite scaffold; TS fixtures; lint; docs).

## Out of Scope
Fixing the individual engine drifts (filed separately as product bugs).
