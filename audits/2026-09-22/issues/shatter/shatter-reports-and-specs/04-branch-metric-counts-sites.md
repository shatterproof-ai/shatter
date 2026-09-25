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

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- Concolic: `shatter-core/src/orchestrator.rs:2847` sets `branches_covered: Some(discoveries.len())` in the `ExploreProgressSnapshot`.
- Random explorer: `shatter-core/src/explorer.rs:1430` and `:2537` set `branches_covered: Some(aggregator.discoveries_count())`.
- `shatter-core/src/explorer.rs:375-380`: the snapshot documents `total_branches` as "total branches reported by static analysis" and `branches_covered` as "distinct branches covered so far (unique branch IDs with recorded discoveries)". Both count branch IDs (sites), not `(id, taken)` pairs.
- Audit evidence: `audits/2026-09-22/areas/artifacts.md:127`, `areas/frontend-go.md` (go-15 row) and findings core-18 and frontend-go-12 in `audits/2026-09-22/findings.json` (on branch `audit-2026-09-22` until the audit reports land).

## Decision taken in this issue

The metric becomes **branch-side coverage**: the numerator counts distinct `(branch_id, taken)` pairs observed, and the denominator is 2 × the branch points reported by static analysis. The alternative of keeping the site count and renaming it ("branch points reached") is rejected: it would keep showing `2/2` for the Go `Classify` case above, which is the misleading output this issue exists to fix. Where static analysis gives no branch count (`total_branches` is `None`), the denominator is omitted and the line says `N branch sides`, not a fraction.

## Acceptance criteria

- [ ] Progress lines and reports (markdown, HTML, JSON) show covered branch sides out of 2 × branch points, labelled as sides (for example `3/4 branch sides`). The words "N/M branches" for the site count no longer appear in any output; a grep over `shatter-core/src` and `shatter-cli/src` for the old format string finds nothing.
- [ ] JSON: the scan and explore report schemas gain side-count fields (for example `branch_sides_covered`, `branch_sides_total`), and their schema versions are bumped under the existing bump policies. Existing fields keep their meaning, or are removed with a version bump; the close comment lists which.
- [ ] Known-answer tests run in both engines (random explorer, `explorer.rs`, and concolic orchestrator, `orchestrator.rs`; see CLAUDE.md "parallel parity") on a fixture with two branch points where exactly one side is never taken (the Go `Classify` nested `x > 0.5` / `x < 1` shape, or a TS equivalent with a fixed iteration budget). Each test asserts the exact progress value `3/4 branch sides`. Close-time proof: both tests failing on current code (showing `2/2`) and passing after the fix, pasted into the close comment.
- [ ] A second fixture with every side taken (TS `classifyNumber`) asserts `6/6 branch sides`, so the fix does not simply undercount.
- [ ] Any stopping or scheduling logic that reads `branches_covered` (plateau detection, worklist exhaustion, frontier ranking) is listed in the close comment with file:line, and each is either switched to sides or left on sites with a one-line reason.
- [ ] `task e2e` (TS, Go and Rust concolic E2E) passes with forced execution (not a cached "up to date"); the output is recorded in the close comment.

## Suggested approach

Track `(branch_id, taken)` pairs in the discovery aggregator and the orchestrator's discovery set, and carry a sides total alongside `total_branches`. Update the SPEC wording for the metric in the same change.

## Out of scope

- Line-coverage denominator issues (finding goals-06, tracked separately).
- The resume-ignores-explorer-mode part of frontend-go-12 (handled separately) and its init empty-path part (str-qwua7.39).

## Related

str-9q1z, str-4o07, str-cii2. Source findings: core-18 (confirmed), frontend-go-12 (branch-metric part only).
