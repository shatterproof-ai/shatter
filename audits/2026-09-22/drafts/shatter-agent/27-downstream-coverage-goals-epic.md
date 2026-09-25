# Track downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07

- Priority: P2
- Type: epic
- Labels: agents,coverage,downstream,kapow,zolem,pickpackit
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new
- Source findings: goals-09
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Shatter's real-world success goals — ≥90% coverage on kapow, zolem and
pickpackit, set early July — exist only in per-user agent memory files. They
were last measured 2026-07-05..07 at 18-28% and nothing prompts anyone to
re-measure or act. No tracker epic owns them.

## Current Code Facts
- Memory: `project_pickpackit_coverage_goal.md` (metric, eligible set,
  baseline 26.3% combined, 2026-07-07), `project_kapow_shatter_advise_log.md`
  (18.3%, resolver dir 18.8%), `project_zolem_shatter_advise_log.md` (28.0%
  lines, 2026-07-05).
- Newest dated entries: `~/project/zolem/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-05, `~/project/pickpackit/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-07; kapow has no committed changelog.
- Open levers named in memory: str-j49xg (P1 epic), str-la75, str-jyxr,
  str-4yc9w, str-wfd2, str-bh9wu.
- This audit found the coverage metric itself inconsistent (Go scan inflates
  to 100%, Rust deflates to ~50%) and explore resume ignoring explorer mode, so
  any re-measurement must wait for those fixes.

## Acceptance Criteria
- An in-repo doc (e.g. `docs/goals/downstream-coverage.md`) records per project:
  metric definition, eligible set, run recipe, dated measurements.
- This epic has one child per downstream project with the last measurement and
  the open levers linked as dependencies.
- Re-measurement scheduled as a child blocked by the metric-correctness and
  resume-key product issues; the result is recorded with a date.
- The memory files are reduced to pointers to the doc/epic.

## Out of Scope
Engine improvements themselves.
