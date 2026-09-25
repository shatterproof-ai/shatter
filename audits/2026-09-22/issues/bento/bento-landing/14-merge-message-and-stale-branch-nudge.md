---
slug: merge-message-and-stale-branch-nudge
kind: new
title: "land.py: landing merge message should name the issue id as well as the branch"
priority: P3
type: feature
labels: [audit, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py: landing merge message should name the issue id as well as the branch

This draft originally bundled four deliverables (merge message, bare-SHA merge guard, stale-branch doctor nudge, one-session-per-branch rule). After the cross-check it covers only the merge message. The doctor nudge is `stale-pushed-branch-doctor-nudge` and the ownership rule is `one-session-per-branch-guidance`. The bare-SHA guard was dropped: see "Not included" below.

## Problem

land.py's merges are titled `Merge branch '<branch>'`. When the branch name carries an issue id this is readable, but the id is not a separate, parseable part of the subject, and branch names do not always start with it. History readers and tools (closure's tracker_mismatch, close-reason evidence) have to re-parse the branch name. A landing merge should state the issue it lands.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work and closure unchanged since 1c0c1e6):

- `catalog/skills/land-work/scripts/land.py:194` (merge-in-primary route) and `:221` (push-from-preview route): the message is `f"Merge branch '{feature_branch}'"` and does not include the issue id.
- `catalog/skills/closure/scripts/closure-scan.py:1436-1452`: closure already has an issue-id rule for branch names (`_BRANCH_ISSUE_ID_RE`, `resolve_branch_issue_id`), which this can reuse.
- In shatter, since land.py took over (2026-09-19), landing merges are uniform `Merge branch '<branch>'`, for example `16794cef Merge branch 'str-qwua7.4-landing2'`.

## Acceptance criteria

- [ ] land.py's merge message is `Merge branch '<branch>' (<issue-id>)` when the branch name yields an issue id under closure's rule, and `Merge branch '<branch>'` otherwise. Both routes (`:194`, `:221`) use one helper.
- [ ] The issue-id rule is shared (moved into a module both closure and land.py import, or imported from closure), not copied.
- [ ] Tests in `tests/land_work/test_land_driver.py` assert the subject for both routes, with and without an issue id in the branch name. They are committed failing first, then passing.
- [ ] Proof at close: test names plus the failing-then-passing output in the close reason. "Merged" is not sufficient.

## Not included (dropped after cross-check)

- **Refusing `git merge <bare-sha>` into the primary branch.** The six `Merge commit '<sha>' into HEAD` merges in shatter history (af6839f0, a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f) are all dated 2026-09-07/08. Since bento-rdtn.15 (closed; commit 5210e74, 2026-09-10), `require-worktree-git-guard.py` denies **every** `git merge` typed as a Bash command in the primary checkout, so this path is already closed, and land.py's own `git merge --ff-only <leased_sha>` (`land.py:179`) runs as a subprocess the guard never sees. A SHA-specific rule would add nothing; remaining guard gaps belong to bento-i76i and the audit's `git-guard-bypasses-and-false-positives`.

## Out of scope

- Rewriting existing history.
- Remote branch deletion (bento-73de; see `landing-deletes-remote-branches`).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9 and bento-rdtn.14 (closed), `stale-pushed-branch-doctor-nudge`, `one-session-per-branch-guidance`, `close-reason-evidence` (bucket bento-guards-doctor-tracker).

Priority: P3 · Type: feature · Labels: audit, land-work, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17
