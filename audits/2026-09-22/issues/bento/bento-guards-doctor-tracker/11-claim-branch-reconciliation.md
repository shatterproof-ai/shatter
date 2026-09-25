---
slug: claim-branch-reconciliation
kind: new
title: "closure: report stale in_progress claims (no branch/worktree, or branch already merged into the primary branch)"
priority: P2
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# closure: report stale in_progress claims (no branch/worktree, or branch already merged into the primary branch)

Related: bento-rdtn.8, bento-rdtn.9 (closed), bento-x4bm (open). Source findings: prior-10, bento-17 (shatter audit 2026-09-22).

**Scope boundary.** Making a landing close its issue is owned by bento-x4bm (land.py validates the closure note, lands, then closes the issue and records the landing durably; it fails preflight if the issue is already closed). The audit draft's first criterion ("land.py closes the issue in the same run, or verify-landing exits non-zero") duplicated x4bm and is dropped. This issue covers only the detection side: claims that no live work backs, whatever path created them.

## Problem

Stale `in_progress` claims keep recurring in shatter, in both directions:

1. **Landed but not closed.** str-mpgg1 was merged on 2026-09-02 (84941b37 is an ancestor of origin/main) and is still in_progress (re-checked with `bd show str-mpgg1` on 2026-09-23). Of the 13 stale claims cleared by shatter str-qwua7.17 on 09-08, 8 had also landed without being closed.
2. **Claimed with no live work.** str-8q1b4's branch was 105 commits behind main with its last commit on 2026-08-31 when drift-patrol flagged it for the third time. (It has since landed and was closed on 2026-09-23; the pattern, not this instance, is the point.)

bento-rdtn.8 only warns at verify-landing. bento-rdtn.9 reports the opposite case: branches whose issue is not in_progress.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/land-work/scripts/land-work-verify-landing.py` lines 77-130 (`_check_issue_status`): warns "`<id>` is not closed ..." and does not fail. `land.py` calls verify-landing without `--issue` (line 330).
- `catalog/skills/closure/scripts/closure-scan.py` lines 1539-1591: the rdtn.9 `tracker_mismatch` annotation, branch-to-issue direction only.

## Acceptance criteria

- `closure-scan.py` output gains `stale_claims`, listing in_progress issues in two classes:
  - `merged`: the issue's branch (matched by the existing branch-to-issue rule) is an ancestor of `<remote>/<primary>`;
  - `no_live_work`: claimed more than N days ago (default 7, `--stale-claim-days`), with no matching local branch, registered worktree or remote branch.
  Each entry has the issue id, claim age, class, and the evidence (branch name and merge-base result, or "no branch/worktree found"). Report only; no automatic release or close.
- The human-readable closure report prints the two classes with the exact `bd` command a person would run (`bd close <id> --reason ...` pointing at land-work's contract for `merged`, `bd update <id> --status open` for `no_live_work`), without running either.
- Tests use a fixture repo and a stubbed `bd` that returns in_progress issues covering: merged branch, unmerged live branch (not reported), no branch and older than N days (reported), no branch and newer than N days (not reported), and a `bd` failure (the section is skipped with a warning; the rest of the scan still succeeds).
- Proof at close: the close note names the tests with failing-then-passing runs, and includes the `stale_claims` section from a real run against shatter.

## Out of scope

- Automatically releasing or closing claims.
- Closing issues as part of landing (bento-x4bm).

## Priority / Type / Labels

P2 / feature / audit, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
