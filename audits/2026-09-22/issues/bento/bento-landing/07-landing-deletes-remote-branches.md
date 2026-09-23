---
slug: landing-deletes-remote-branches
kind: new
title: "land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step"
priority: P2
type: feature
labels: [audit, land-work, cleanup]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step

## Problem

Landing cleans up only the local branch and its worktree. Consumer repos therefore pile up merged remote branches, and branches that a re-landing superseded stay on the remote. Shatter's AGENTS.md calls remote deletion "mandatory" and keeps a hand-run `scripts/cleanup-merged-remote-branches.sh` to make up for it, but land.py never deletes a remote branch and never checks for one. Close reasons still claim the deletions happened.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main b1bb787:

- `git branch -r | wc -l` gives 66, and `git branch -r --merged origin/main | wc -l` gives 38. Examples: `origin/str-hjrnp.1` through `.4`, `origin/str-2tyfk-lint-errcheck`.
- `git ls-remote origin 'refs/heads/str-qwua7*'` still lists `str-qwua7.4-testplan-http-body-fix`, `str-qwua7.7-protocol-registry-validate`, `str-qwua7.16-restore-bd-dolt` and `str-qwua7.17-stale-claims-cleanup`. They are unmerged, each about 102 commits ahead of main, and each contains a stray fixture commit, e50fc399 "init". The close reason for str-qwua7.4 says its duplicate branches were deleted; one of them is still there.
- At audit time a `/tmp/land-work-preview-a5l9ycto` worktree (detached at 16794cef) was still registered after its landing. It has since been removed (`git worktree list` now shows no preview), but nothing in land.py checks for its own preview after cleanup.
- `catalog/skills/land-work/SKILL.md:516-533` (step 10) deletes only the local branch and worktree.
- `catalog/skills/land-work/scripts/land.py`: the only pushes are to the primary ref (`:211`, `:232-233`). There is no `git push origin --delete`.

## Acceptance criteria

- [ ] After `verify_landing` succeeds, land.py deletes `origin/<feature>` when it is an ancestor of the landed SHA. Repos can opt out with verifier.json `delete_remote_branch: false`.
- [ ] land.py then confirms the deletion with `git ls-remote --heads origin <feature>` and reports `remote_branch_deleted: true|false|skipped` (plus a reason) in the final JSON. A failed deletion is a warning, not a failed landing.
- [ ] Optional `--superseded <branch>...`: land.py deletes those remote branches too, but only after checking that each one's issue id (parsed with the same issue-id rule closure uses) matches the landed branch's issue id. It confirms each deletion with `ls-remote` and reports it.
- [ ] land.py removes its own scratch preview in a `finally` block and confirms that `git worktree list --porcelain` no longer contains it. The result is reported in the JSON.
- [ ] Closure gains a report-only section listing remote branches already merged into the primary branch, with the `git push origin --delete` command for each. It does not delete anything automatically.
- [ ] Tests with a bare-remote fixture cover: merged branch deleted; opt-out honoured; unmerged feature ref not deleted; `--superseded` with a mismatched issue id refused; preview absent after both success and failure.
- [ ] Proof at close: test names plus passing output, and one real landing's JSON showing `remote_branch_deleted: true`. "Merged" is not sufficient.

## Suggested approach

Add a `delete_remote` step after `verify_landing`, recorded with `_record()` like the other steps. Use `git merge-base --is-ancestor origin/<feature> <merge_sha>` as the safety check.

## Out of scope

- One-off cleanup of shatter's existing 38 merged and 4 contaminated remote branches. That is a shatter task.
- The general preview-leak and scoping work (`stale-previews-leak-and-scoping`, bento-e583).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.14 (closed), bento-gd2 and bento-7n7 (closed; preview leaks), bento-rdtn.9 (closed; closure tracker_mismatch), `close-reason-evidence` (bucket bento-guards-doctor-tracker), shatter str-qwua7.19.

Priority: P2 · Type: feature · Labels: audit, land-work, cleanup · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/11, bento-15, prior-06
