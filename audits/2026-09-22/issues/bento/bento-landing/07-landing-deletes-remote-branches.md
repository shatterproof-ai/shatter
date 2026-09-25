---
slug: landing-deletes-remote-branches
kind: note-to-existing
title: "Note on bento-73de: shatter evidence; call the lease-protected delete helper from land.py; superseded same-issue branches are report-only; closure report of merged remote heads"
priority: P2
type: note
labels: [audit, land-work, cleanup]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-73de
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-73de: shatter evidence and remaining deltas

Target: **bento-73de** (open, P2, "land-work cleanup: delete the landed feature branch on the remote, not only locally"). Post as a comment. Do not file a new issue.

This draft was originally a new issue. bento-73de (filed 2026-09-23) already specifies the core fix, and specifies it more safely than the draft did: a helper `land-work-delete-remote-branch.py` that fetches the exact `refs/heads/<branch>` into a private ref, checks ancestry of that exact SHA, and deletes with `--force-with-lease=refs/heads/<branch>:<remote_sha>`, so a push that lands after the check is never deleted (its `test_delete_remote_branch_lease_rejected`). The draft's `git ls-remote --heads` confirmation and unconditional ancestry-then-delete are dropped in favour of 73de's design. Only the deltas below are posted.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (findings bento/11, bento-15, prior-06)**
>
> **Evidence from shatter** (audit worktree 56c86168, 2026-09-23):
> - `git branch -r | wc -l` gives 66, and `git branch -r --merged origin/main | wc -l` gives 38 (for example `origin/str-hjrnp.1` through `.4`, `origin/str-2tyfk-lint-errcheck`).
> - `git ls-remote origin 'refs/heads/str-qwua7*'` still lists `str-qwua7.4-testplan-http-body-fix`, `str-qwua7.7-protocol-registry-validate`, `str-qwua7.16-restore-bd-dolt` and `str-qwua7.17-stale-claims-cleanup`: unmerged, each about 102 commits ahead of main, each carrying a stray fixture commit (e50fc399 "init"). The close reason of str-qwua7.4 says its duplicate branches were deleted; one is still there. Shatter's AGENTS.md calls remote deletion "mandatory" and keeps a hand-run `scripts/cleanup-merged-remote-branches.sh` to compensate.
>
> **Proposed deltas to this issue's scope** (each optional; the maintainer decides which to take here and which to split):
> 1. **land.py calls the helper.** 73de says "if land.py later gains teardown, it calls the same helper". Since land.py is the default serial path, add a `delete_remote` step after `verify_landing` that invokes `land-work-delete-remote-branch.py --branch <feature>` and copies its JSON into the final result as `remote_branch: {deleted, reason, remote_sha}`. A non-`deleted` outcome is a warning, not a failed landing. Test: a land.py run against the bare-origin fixture ends with `remote_branch.reason == "deleted"`, and with the opt-out key set ends with `"opted-out"`.
> 2. **Superseded same-issue branches are reported, never auto-deleted.** A matching issue id proves neither that a branch's commits are safe to discard nor that no other session is using it (the shatter str-qwua7.* branches above have about 102 unique commits each). So land.py only lists other remote branches that carry the landed branch's issue id, with `git cherry` unique-commit counts and whether any local worktree has them checked out, under `superseded_candidates` in the final JSON. Deletion stays an explicit operator action through the same helper, which already refuses anything that is not an ancestor of the primary branch.
> 3. **Closure report of merged remote heads.** 73de lists "closure remote reporting" as a possible follow-up. Shatter shows it is needed for the backlog that already exists: a report-only closure section listing remote heads that are ancestors of the primary branch, each with the helper command to delete it. No automatic deletion.
>
> Related: bento-rdtn.14 (closed; land.py), bento-rdtn.9 (closed; closure tracker_mismatch), `close-reason-evidence` (audit bucket bento-guards-doctor-tracker), shatter str-qwua7.19.
