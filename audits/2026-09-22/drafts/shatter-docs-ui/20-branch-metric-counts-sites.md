# Progress and report "N/N branches" counts branch sites, not sides; shows 2/2 while a side was never taken

- Priority: P2
- Type: bug
- Labels: report,progress,coverage,ux
- Tracker action: new issue (go-12 is partially covered by str-qwua7.39 for its init part, which is moved into the .39 note)
- Related: str-9q1z, str-4o07, str-cii2
- Source findings: audit 2026-09-22 core-18 (confirmed, dedupe related), frontend-go-12 (branch-metric part). The resume-ignores-explorer-mode part of go-12 is an L5 finding handled separately (goals-05/artifacts-01).

<!-- body -->
## Problem
The explore progress line and report say `2/2 branches` or `3/3 branches` even when one side of a branch was never taken. Users read that as full branch coverage. Examples:
- Go `Classify(x float64)` with nested `x>0.5` / `x<1`: progress `2/2 branches`, line coverage 80% (4/5). The `x<1` true side (`return "low"`) was never reached. Both engines behave the same way.
- Go `lit.go:Classify` concolic: `24 iters, 2 paths, 3/3 branches`, while 2 of 4 arms were never taken.

## Current code facts
- Concolic progress counter: `shatter-core/src/orchestrator.rs:2847`. Random explorer: `ExploreProgressSnapshot` in `shatter-core/src/explorer.rs`. Both count distinct branch IDs, not (id, taken) pairs.

## Acceptance criteria
- Progress and report show covered branch sides out of 2 × branch points (e.g. `3/4 branch sides`). Alternatively, rename the metric to "branch points reached" everywhere, including SPEC and the reports.
- Known-answer tests: a fixture with one untaken side shows less than the total.
- If any stopping logic (plateau or worklist exhaustion) keys on this metric, it uses sides.

## Scope
In: metric definition and labels. Out: line-coverage denominator issues (L5 goals-06).
