---
repo: shatter
type: chore
priority: 2
labels: governance, beads
existing: none
---
# Tracker content sweep: dedupe, merge, fill bodies, fix placeholder refs, priorities
## Decision (2026-09-06)
All proposed defaults approved: close str-hrg2 as duplicate of str-0wxw; merge str-wfd2 into str-1fik (P1) and close str-wfd2; str-2zsy author fills the body or it closes as insufficient detail; defer str-cl53; replace the str-35vtk.* `issue<N>` placeholders with real ids from docs/perf/efficiency-plan.md; str-j49xg body becomes "not split; work tracked here"; promote str-u394l.4 to P1 (str-u394l.3 is now being adopted, see storystore decision); str-rmcrl to P2 and land it.

## Problem
A handful of open issues are unstartable or misleading for a fresh agent: two are byte-for-byte duplicates, two describe one feature at two priorities (one with an empty body), the `str-35vtk.*` batch cross-references plan-local ids that resolve to nothing, and an epic promises children that were never filed. Together they are why `bd ready` hands out the wrong work.

## Current code facts
- `str-0wxw` ≡ `str-hrg2`: both P2 bugs "axum handler generators re-seed FK chain per iteration", created 2026-06-16, near-identical bodies; `str-0wxw` carries the refinement notes (single- vs multi-param path handlers).
- `str-1fik` (P1 feature, **empty body**) vs `str-wfd2` (P2, full body: `analyzer.rs convert_type_path()` single-file scope, pickpackit impact, desired change) — same work.
- `str-2zsy` (P2 bug) empty body; `str-cl53` (oldest open, 2026-03-08) body is one line pointing at `docs/plans/str-zwgc-test-impact-analysis.md`.
- `str-35vtk.29` body: "blocked by issues17,18", "issue3 lease", "issue17 candidates"; similar placeholders in other `str-35vtk.*` (batch-filed from the plan under `docs/perf/efficiency-plan.md`).
- `str-j49xg` (P1 epic) body says "Child issues (filed alongside)" — no dotted children exist.
- Priority hygiene: `str-u394l` P1 epic with only P2 children; `str-rmcrl` P3 for a red `clippy -D warnings` on main; 23 of 96 open are P1.

## Options considered
- Duplicate: close `str-hrg2` as duplicate of `str-0wxw` (keeps the notes) — mechanical.
- Merge: keep `str-1fik` (older id, P1) with `str-wfd2`'s body pasted in; close `str-wfd2` as duplicate; priority P1 if pickpackit coverage is still a goal, else P2.
- `str-2zsy`: author (str-5lfr follow-up) fills the body or it is closed "insufficient detail"; `str-cl53`: defer (plan doc exists, no activity in 178 d).
- `str-35vtk.*`: replace each `issue<N>` with the `str-35vtk.<N>` id from `docs/perf/efficiency-plan.md`'s numbering — mechanical once the mapping is confirmed by the epic owner.
- `str-j49xg`: the epic author decides file vs retract; default: edit the body to "not split; work tracked here".
- Priorities: promote `str-u394l.3/.4` to P1 or demote the epic (a2-40a may close `.3`); `str-rmcrl` → P2 and land (red main).

## Acceptance checks
- Each decision applied; `bd show` of every touched id reflects it; `python3 scripts/drift-patrol.py --only tracker-hygiene` unchanged or better; no open body < 300 chars without a reason.

## Scope
In: tracker edits only. Out: process rules (p2-36b, p2-36c), the stale-claim/worktree sweep (p1-17).

## Size
small.

## Provenance
Audit 2026-09-04, section 11, action item 36; evidence audits/2026-09-04/issues-hygiene.md B3, B5, B6, B7, R7, R8.
