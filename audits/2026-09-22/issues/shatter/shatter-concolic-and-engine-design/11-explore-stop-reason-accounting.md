---
slug: explore-stop-reason-accounting
kind: new
title: "explore artifacts always report stop_reason worklist_exhausted and solver_guided_inputs 0: ExploreResultAccumulator drops both fields"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, cli, artifacts, explore]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore artifacts always report stop_reason worklist_exhausted and solver_guided_inputs 0: ExploreResultAccumulator drops both fields

## Problem

`shatter explore` merges per-batch `ObservationOutput`s through `ExploreResultAccumulator` (`shatter-cli/src/commands/explore.rs:190-330`). The accumulator has no field for `stop_reason` or `solver_guided_inputs`. `into_result` builds the final `ObservationOutput` with `..Default::default()`. So every explore artifact and report shows:

- `stop_reason: worklist_exhausted`, the `#[default]` variant of `StopReason` (`shatter-core/src/explorer.rs:435-447`);
- `solver_guided_inputs: 0`.

This happens on both engines and whatever actually ended the run. The engine computes both values correctly: `pipeline.rs:951` sets `solver_guided_inputs = z3_generated + boundary_generated + drill_generated`, and the orchestrator returns the real `TerminationReason` (`orchestrator.rs:1621-1647`, `:3179`). The CLI then discards them.

Impact: the audit misread these fields as "the Z3 loop contributes nothing and every run drains its worklist". The same artifacts show `z3` discoveries on 6 of 21 functions (see concolic-early-termination). Any benchmark or diagnosis that reads these fields from explore artifacts is measuring a constant.

## Evidence

- Audit run `ts-sub-concolic` (21 functions, `--concolic`): all 21 artifacts report `worklist_exhausted` and `solver_guided_inputs: 0`, including functions that ran exactly 100 iterations against a 100 budget and functions with 9 `z3` discoveries (classifyHttpResponse).
- `explore.rs:5238-5249` (resume path) and `:5824` (normal batch path) both route every observation through `accumulators[work_index].merge(...)`.
- No field in the struct at `explore.rs:190-211` carries either value.
- Tracker search (`bd search stop_reason`, `bd search solver_guided_inputs`, `bd search ExploreResultAccumulator`, 2026-09-23): no existing issue.

## Acceptance criteria

- [ ] The accumulator carries both fields with a documented merge policy in its doc comment:
  - `solver_guided_inputs` is summed across batches;
  - `stop_reason` is taken from the last successful batch (or a documented precedence), and a resumed-only function keeps its prior artifact's value.
- [ ] Unit tests on `ExploreResultAccumulator` merge two synthetic `ObservationOutput`s with non-default values (for example `MaxExecutions` and `solver_guided_inputs: 3` and `4`), and assert the merged `stop_reason` and `7`. Proof: the test fails on current main (paste the assertion output) and passes after the fix.
- [ ] A CLI-level test runs `shatter explore --concolic` on a fixture whose branch is only reachable with a solver-generated input (for example an equality against a constant), with a small `--max-iterations`. It asserts, from the written artifact JSON, that `solver_guided_inputs > 0` and that `stop_reason` is not the default when the budget was the limit. The same test with the default explorer asserts `stop_reason` equals the explorer's `classify_stop_reason` result.
- [ ] A grep over `shatter-cli/src` for `..Default::default()` in constructions of `ObservationOutput` finds no other construction that silently drops a field the engine sets. Any that remain are listed in the close note with the reason they are safe.
- [ ] Proof at close: the red and green test output, and the forced (uncached) `task e2e` output.

## Suggested approach

Add the two fields to the accumulator and its `merge`, and drop `..Default::default()` in `into_result` so the compiler flags any future `ObservationOutput` field the accumulator forgets. If a field is intentionally defaulted, name it explicitly.

## Out of scope

- Why concolic runs stop early (concolic-early-termination).
- The scan path's own artifact writer, unless the same defaulting is found there. If it is, list it in the close note and fix it here.

## Metadata

- Priority: P1 (blocks the D3 diagnosis and benchmark)
- Type: bug
- Labels: audit-2026-09-22, concolic, cli, artifacts, explore
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-early-termination, concolic-vs-default-benchmark
- Related: explore-resume-options-key
- Source findings: goals-08 (from Codex cross-check of draft concolic-early-termination)
- Decision refs: D3
