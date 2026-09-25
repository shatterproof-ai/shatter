# Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, closure, land-work, hygiene
- Parent: epic
- Links: related bento-rdtn.8, related bento-rdtn.9
- Source findings: prior-10, bento-17

---BODY---
## Problem

Stale `in_progress` claims keep recurring in shatter, in both directions:

1. **Landed but not closed.** str-mpgg1 was merged on 2026-09-02 (84941b37 is an ancestor of origin/main) and has been in_progress for 20 days. Of the 13 stale claims cleared by shatter str-qwua7.17 on 09-08, 8 had also landed without being closed.
2. **Claimed with no live work.** str-8q1b4's branch is 105 commits behind main, with its last commit on 2026-08-31. Drift-patrol has flagged it three times. Claims also show owner "Test" (a separate shatter identity bug).

bento-rdtn.8 only warns at verify-landing. bento-rdtn.9 reports the opposite case: branches whose issue is not in_progress.

## Current code facts

- `catalog/skills/land-work/scripts/land-work-verify-landing.py`: rdtn.8 warning when the landed branch's issue is still open. It does not fail.
- `catalog/skills/closure/scripts/closure-scan.py` about line 1565: the rdtn.9 tracker_mismatch report only.

## Acceptance criteria

- land.py closes the tracker issue in the same run that pushes the primary branch, or verify-landing exits non-zero with an actionable message when the landed branch's issue id is still open or in_progress.
- A closure report lists in_progress issues older than N days (default 7) with no matching local branch, worktree or remote branch, and in_progress issues whose named branch is already merged into the primary branch. Report only, with no automatic release.
- Tests use a beads fixture, or a stubbed bd.

## Out of scope

- Automatically releasing claims.
