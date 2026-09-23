---
slug: concolic-early-termination-fix
kind: new
title: "Fix the concolic early-termination defect identified by concolic-early-termination, with a known-answer E2E test under a bounded budget"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-early-termination]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fix the concolic early-termination defect identified by concolic-early-termination, with a known-answer E2E test under a bounded budget

## Problem

Maintainer decision D3 makes the concolic early-termination fix P1, so the default-vs-concolic benchmark measures a working engine. The diagnosis issue (concolic-early-termination) establishes what ends `--concolic` runs after 21-35 executions on functions such as computeArea, matchRoute and negotiateLanguage, and whether it is a defect. This issue implements the fix that diagnosis names.

If the diagnosis verdict is **expected** (every loss is accounted for by other issues, such as the TS instrumentation or Z3 sort-split bugs), close this issue with a link to that verdict. Do not invent a fix.

## Evidence

See concolic-early-termination for the per-function table from the audit run and the code sites. Scope, fixture and code sites for this issue come from that issue's close note.

## Acceptance criteria

- [ ] Known-answer E2E test in `shatter-core/tests/e2e_concolic.rs` (TS), driven through `pipeline_orchestrator` or the CLI entry point, not `orchestrator::explore` directly. The fixture is the one the diagnosis named. It has a branch that the pre-fix engine provably misses and that is reachable only by a solver-generated input. The test:
  - runs with a fixed seed and a bounded budget stated in the test (for example `max_iterations = 40`);
  - asserts that the target branch outcome is covered;
  - asserts that the covering input's discovery method is `z3` (or the solver provenance the diagnosis names), not `user_provided` or `fuzzed`;
  - asserts the artifact's `stop_reason` equals the expected reason for that budget, and `solver_guided_inputs >= 1`.

  It does not assert a minimum execution count: a correct solver may reach the target sooner.
- [ ] Proof: the commit SHA where the test fails on the pre-fix code, with the failing assertion output, and the passing output after the fix.
- [ ] If the diagnosis found the same defect reachable from Go or Rust, a matching case is added to `e2e_concolic_go.rs` or `e2e_concolic_rust.rs`, with the same red/green proof.
- [ ] A re-run of the diagnosis's 9-file subset, same command and examples SHA, attached to the close note as a before/after per-function table (`iterations`, `stop_reason`, `solver_guided_inputs`, branches). Every function whose branch coverage went down is explained.
- [ ] Forced (uncached) `task e2e` output at close.

## Out of scope

- Losses the diagnosis attributes to z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation or known-answer-ratchet-and-ts-discriminants. Those are fixed in their own issues.
- The benchmark and its post-fix run (concolic-vs-default-benchmark, concolic-benchmark-postfix-run).

## Metadata

- Priority: P1 (D3)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, solver
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: concolic-early-termination
- Blocks: concolic-benchmark-postfix-run
- Related: explore-stop-reason-accounting, z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation
- Source findings: goals-08
- Decision refs: D3
