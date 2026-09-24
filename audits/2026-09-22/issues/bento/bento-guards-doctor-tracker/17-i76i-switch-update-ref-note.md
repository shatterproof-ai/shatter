---
slug: i76i-switch-update-ref-note
kind: note-to-existing
title: "Note on bento-i76i: also deny `git switch <primary>` and `git update-ref refs/heads/<primary>` in the primary checkout"
priority: P2
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-i76i
set_priority: P1
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-i76i

Target: bento-i76i (open, P2, "git guard: block commit, cherry-pick, pull, am, and revert on the primary branch ..."). Action: add a comment. This issue owns the primary-branch verb list, so the two verbs the audit found go here rather than into a separate issue. The draft also gives the filer the ordering edge git-guard-bypasses-and-false-positives blocked-by bento-i76i (same file, shared landing order).

## Comment text

Audit 2026-09-22 (shatter; finding bento-04). Two more primary-branch mutations pass the guard today (probed at bento 0b8d488 with cwd = the primary checkout; both exit 0):

- `git switch main` is the `switch` spelling of the already-denied `checkout <primary>`; deny it under the same conditions (and `switch -C`/`--force-create <primary>`).
- `git update-ref refs/heads/main <sha>` moves the primary branch directly, skipping every other rule; deny `update-ref` whose ref is `refs/heads/<primary>` (including `-d`) in the primary checkout.

Suggested tests: both commands exit 2 in the primary checkout, and `git switch feature` / `git update-ref refs/heads/feature <sha>` exit 0. The remaining bypass forms (`/usr/bin/git`, wrappers, `-C`, `cd`, `GIT_CONFIG_*` env) are tracked in <id of git-guard-bypasses-and-false-positives>, which lands after this issue.

Maintainer decision (2026-09-24): raised to P1, because the P1 follow-up <id of git-guard-bypasses-and-false-positives> is blocked on this issue.
