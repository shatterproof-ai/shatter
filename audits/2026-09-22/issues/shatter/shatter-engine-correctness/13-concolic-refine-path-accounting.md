---
slug: concolic-refine-path-accounting
kind: new
title: "Concolic refine-phase executions are discarded after boundary-witness updates; paths reached only there never reach unique_paths, raw_results or the report"
priority: P2
type: bug
labels: [concolic, orchestrator, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-refine-execute-builder]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine-phase executions are discarded after boundary-witness updates; paths reached only there never reach unique_paths, raw_results or the report

## Problem

`refine_boundaries_async` executes candidate inputs near each branch boundary, then uses each result only to update that branch's true/false witness. The execution results are never passed to observation or coverage accounting. A path or line reached only during refinement is missing from `covered_paths`, `unique_paths`, `raw_results`, the artifact and the report.

This changes a policy that open issue str-qwua7.5 currently assumes. str-qwua7.5's per-phase capture policy lists boundary refinement (its `:2314`, now `:2380`) as a **probe** site whose "outputs are compared for path/outcome equality and then discarded", so it keeps `capture: false`. If refine results become reported results, the refine site moves to the **reported** class, and under str-qwua7.5's rule it must honour the capture flag. This issue owns that policy change and must record it on str-qwua7.5.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2386-2408`: after each refine Execute, the result is read only to find `took_side` for the target `branch_id`, which sets `tw` or `fw`. `exec_result` is then dropped. `BoundaryResult` (`:2411-2416`) carries only the witnesses and `executions_used`.
- The concolic `Classify` artifact from the audit (Go, nested x>0.5 / x<1) had `boundary_results` true_witness `[0.5000037571385455]` (20 executions) while `unique_paths=2`, and the report said 4/5 lines. The verifier noted that this witness is on branch 0 (x>0.5), and the artifact does not show whether it took the x<1 'low' side. So this run does not prove that a path was lost; the regression test below must construct a case that does.
- str-qwua7.5 (`bd show str-qwua7.5`, checked 2026-09-23) lists the refine site as a probe with `capture: false`.

## Acceptance criteria

- [ ] Refine-phase executions go through the same observation call the main loop uses, so a path first reached during refinement is counted once in `unique_paths`, appears in `raw_results` and `new_path_executions`, and is rendered in the report. Refine executions that reach an already-known path do not change the counts.
- [ ] A regression test constructs a target where one path is reachable only by an input the refine phase produces (for example a branch on `x == boundary + epsilon` that the solver and fuzz phases are configured not to reach, or a recording frontend double that returns a new branch path only for refine-phase inputs). It asserts that the path appears in the artifact and the rendered report. At close, quote the test failing on current `main` and passing after the fix.
- [ ] Capture policy for the refine site is settled with str-qwua7.5:
  - A comment is posted on str-qwua7.5 stating that the boundary-refinement site is now a reported site and follows the capture flag, with the id of this issue.
  - If str-qwua7.5 has landed first, the refine site honours `capture_side_effects` in this change, with a test. If it has not, str-qwua7.5's implementer applies the reported-site rule to the refine site, and this issue's close note says so.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Return the refine executions from `refine_boundaries_async` (or pass it the aggregator), and feed each through the observation path used by the main loop, marking the discovery method as boundary refinement.

## Out of scope

- The request fields `prepare_id`/`execution_profile` (concolic-refine-execute-builder, which this is blocked by because both edit the same function).
- The shared Execute builder (execute-request-builder).
- The random explorer's float-probe accounting (float-probe-paths-uncounted), the same class of bug in the other engine.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: concolic-refine-execute-builder (same function; land the small field fix first).
- Related: str-qwua7.5 (open; capture policy for the refine site changes here), float-probe-paths-uncounted, execute-request-builder.

## References

Audit 2026-09-22 finding core-06 (verified, P2), second half. Split from concolic-refine-execute-builder after the Codex cross-check.
