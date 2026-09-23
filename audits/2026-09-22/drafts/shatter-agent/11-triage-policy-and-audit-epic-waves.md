# Define P1, cap open P1s, and split/wave-order the stalled str-qwua7 audit epic

- Priority: P2
- Type: task
- Labels: agents,governance,audit,drift
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-qwua7, str-qwua7.62)
- Source findings: prior-14, prior-25, agent-repo-18
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Priority has lost meaning and audit follow-through stalls. 48 open P1s (27% of
open issues), 154 ready issues whose top 25 are all P1, and priority
inversions. The 2026-09-04 audit epic str-qwua7 has 12 of 62 children closed
after 17 days, all small; no structural P1 child has a comment, branch or
commit, while a new P2 feature epic (str-hjrnp) was created and completed on
09-21/22. All 12 `agents`-label items are untouched P2s.

## Current Code Facts
- `bd list --status open`: P1 48, P2 111, P3 23, P4 4.
- Inversion: str-35vtk.10 (P1) blocked by str-35vtk.29 (P2) and .31 (P2).
- str-qwua7 children mix product bugs (.5 .10 .11 .13 .39), refactors (.6.x),
  agent/gate items (.2 .3 .22-.28 .51 .55) and docs (.21 .44-.46), plus 3
  nested epics; comment_count 0 on all open children.
- AGENTS.md has no priority rubric; `bd ready` sorts by priority and age.

## Acceptance Criteria
- AGENTS.md defines P0-P3 (P1 = broken or misleading for users, blocks
  landing, or security; everything else P2+).
- Open P1s re-triaged against the rubric; the result is ≤ 15 open P1s or a
  recorded maintainer override.
- str-qwua7 split into themed child epics (product-correctness, refactor,
  agent/gates, docs) with `bd swarm` wave ordering or explicit blocked-by
  edges; obsolete children closed (draft 10).
- New drift-patrol checks: WARN when open P1 count exceeds the cap; FAIL on
  blocked-by-lower-priority inversions; WARN when an audit epic has more than
  half its children untouched for 14 days.
- AGENTS.md WIP rule: new feature epics at P2+ wait while audit P1 bugs are
  ready, unless the maintainer overrides.

## Out of Scope
Implementing the child issues themselves.
