# land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, land-work, cleanup
- Parent: epic
- Links: related bento-rdtn.14, related bento-gd2, related bento-7n7
- Source findings: prior-06, bento-15

---BODY---
## Problem

Landing cleans up only the local branch and worktree. Consumer repos then accumulate merged remote branches, and branches that a re-landing superseded stay on the remote. Shatter's AGENTS.md calls remote deletion "mandatory", but land.py never does it and never checks it. Close reasons still claim the deletions happened.

## Evidence (shatter, 2026-09-22)

- `git branch -r --merged origin/main` finds 38-39 branches, out of 66-67 remote branches. Examples: origin/str-hjrnp.1..4, origin/str-2tyfk-lint-errcheck.
- Unmerged remote branches `str-qwua7.4-testplan-http-body-fix`, `str-qwua7.7-protocol-registry-validate`, `str-qwua7.16-restore-bd-dolt` and `str-qwua7.17-stale-claims-cleanup` are each 102 commits ahead of main and contain a stray fixture commit, e50fc399 "init". str-qwua7.4's close reason says its duplicate branches were deleted; one still exists.
- A `/tmp/land-work-preview-a5l9ycto` worktree (detached at 16794cef) was still registered after landing.

## Current code facts (bento @ 1c0c1e6)

- `catalog/skills/land-work/SKILL.md` about lines 516-533 (step 10) deletes the local branch and worktree only.
- `land.py` pushes only the primary ref. There is no `git push origin --delete`.

## Acceptance criteria

- After verify-landing succeeds, land.py deletes `origin/<feature>` if it is an ancestor of the landed SHA. Repos can opt out with verifier.json `delete_remote_branch: false`.
- It then verifies with `git ls-remote` and reports `remote_branch_deleted: true|false|skipped` in the JSON.
- Optional `--superseded <branch>...`: deletes those remote branches too, after confirming their issue id matches the landed branch's issue id.
- land.py removes its own preview worktree in `finally` and verifies that `git worktree list` no longer contains it.
- Closure gains a report (not auto-apply) of remote branches already merged into the primary branch.
- Tests with a bare-remote fixture.

## Out of scope

- One-off cleanup of shatter's existing branches. That is a shatter task.
