---
slug: branch-metric-counts-sites
kind: new
title: "Progress and report 'N/N branches' counts branch sites, not sides; shows 2/2 while a side was never taken"
priority: P2
type: bug
labels: [report, progress, coverage, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Progress and report "N/N branches" counts branch sites, not sides; shows 2/2 while a side was never taken

## Problem

The explore progress line and the reports say `2/2 branches` or `3/3 branches` even when one side of a branch was never taken. Users read that as full branch coverage. Observed examples:

- Go `Classify(x float64)` with nested `x > 0.5` / `x < 1`: progress `2/2 branches`, line coverage 80% (4/5). The true side of `x < 1` (`return "low"`) was never reached. Default and concolic engines behave the same.
- Go `lit.go:Classify`, concolic: `24 iters, 2 paths, 3/3 branches`, while 2 of 4 arms were never taken.
- TS `classifyNumber`: `30 iters, 4 paths, 3/3 branches` (`areas/artifacts.md:127`); here the count happens to be right, which hides the problem.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- Concolic: `shatter-core/src/orchestrator.rs:2847` sets `branches_covered: Some(discoveries.len())` in the `ExploreProgressSnapshot`.
- Random explorer: `shatter-core/src/explorer.rs:1430` and `:2537` set `branches_covered: Some(aggregator.discoveries_count())`.
- `shatter-core/src/explorer.rs:375-380`: the snapshot documents `total_branches` as "total branches reported by static analysis" and `branches_covered` as "distinct branches covered so far (unique branch IDs with recorded discoveries)". Both count branch IDs (sites), not `(id, taken)` pairs.
- Audit evidence: `audits/2026-09-22/areas/artifacts.md:127`, `areas/frontend-go.md` (go-15 row) and findings core-18 and frontend-go-12 in `audits/2026-09-22/findings.json` (on branch `audit-2026-09-22` until the audit reports land).

## Acceptance criteria

- [ ] Progress lines and reports (markdown, HTML, JSON) show covered branch sides out of 2 × branch points, for example `3/4 branch sides`. Alternatively, the metric is renamed "branch points reached" everywhere, including SPEC and all reports. The close comment says which option was chosen.
- [ ] Known-answer tests in both engines (random explorer and concolic orchestrator; see CLAUDE.md "parallel parity") use a fixture with one untaken side and assert the shown count is below the total. They fail on current code; record both runs.
- [ ] Any stopping logic (plateau detection, worklist exhaustion) that keys on this metric is identified; if it exists, it uses sides. The close comment lists what was checked.
- [ ] `task e2e` (TS, Go and Rust concolic E2E) passes with forced execution (not a cached "up to date"); record the output.

## Suggested approach

Track `(branch_id, taken)` pairs in the discovery aggregator and the orchestrator's discovery set, and carry a sides total alongside `total_branches`. Update the SPEC wording for the metric in the same change.

## Out of scope

- Line-coverage denominator issues (finding goals-06, tracked separately).
- The resume-ignores-explorer-mode part of frontend-go-12 (handled separately) and its init empty-path part (str-qwua7.39).

## Related

str-9q1z, str-4o07, str-cii2. Source findings: core-18 (confirmed), frontend-go-12 (branch-metric part only).
