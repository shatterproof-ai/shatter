---
slug: claim-branch-reconciliation
kind: new
title: "Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree"
priority: P2
type: feature
labels: [audit, closure, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree

Related: bento-rdtn.8, bento-rdtn.9. Source findings: prior-10, bento-17 (shatter audit 2026-09-22).

## Problem

Stale `in_progress` claims keep recurring in shatter, in both directions:

1. **Landed but not closed.** str-mpgg1 was merged on 2026-09-02 (84941b37 is an ancestor of origin/main) and is still in_progress (re-checked with `bd show str-mpgg1` on 2026-09-23). Of the 13 stale claims cleared by shatter str-qwua7.17 on 09-08, 8 had also landed without being closed.
2. **Claimed with no live work.** str-8q1b4's branch was 105 commits behind main with its last commit on 2026-08-31 when drift-patrol flagged it for the third time. (It has since landed and was closed on 2026-09-23; the pattern, not this instance, is the point.)

bento-rdtn.8 only warns at verify-landing. bento-rdtn.9 reports the opposite case: branches whose issue is not in_progress.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/land-work/scripts/land-work-verify-landing.py` lines 77-129 (`_check_issue_status`): appends a warning "`<id>` is not closed (status: ...); run: bd close ..." and does not fail.
- `catalog/skills/closure/scripts/closure-scan.py` lines 1539-1591: the rdtn.9 `tracker_mismatch` annotation only.

## Acceptance criteria

- land.py closes the tracker issue in the same run that pushes the primary branch, or verify-landing exits non-zero with an actionable message when the landed branch's issue is still open or in_progress.
- A closure report lists in_progress issues older than N days (default 7) with no matching local branch, worktree or remote branch, and in_progress issues whose named branch is already merged into the primary branch. Report only; no automatic release.
- Tests use a beads fixture or a stubbed bd.
- Proof at close: the close note names the tests with failing-then-passing runs and includes the report's output against shatter.

## Out of scope

- Automatically releasing claims.

## Priority / Type / Labels

P2 / feature / audit, closure, land-work, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
