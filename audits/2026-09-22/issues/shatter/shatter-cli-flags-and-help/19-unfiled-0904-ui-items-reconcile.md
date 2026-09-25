---
slug: unfiled-0904-ui-items-reconcile
kind: new
title: "Reconcile the four unfiled 2026-09-04 usability items (12, 16, 17, 20) against the tracker and hand drafts to the maintainer"
priority: P3
type: task
labels: [audit, tracker, usability]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reconcile the four unfiled 2026-09-04 usability items (12, 16, 17, 20) against the tracker and hand drafts to the maintainer

## Problem

The 2026-09-04 audit's filing step (str-qwua7 epic) filed only the P1 and selected P2 items. Its usability/UI section had items 9, 12, 16, 17, 18 and 20 with no tracker issue. This audit covers two of them directly: item 9 (tracker IDs in help) is help-tracker-ids-lint, and item 18 (`--format text` not stripped) is explore-format-flag-ignored. Four remain.

This issue is bounded to those four items. It does not file issues itself: under the audit filing rule (maintainer decision D6 of 2026-09-23), the output is a reconciliation table plus issue drafts that the maintainer files.

## Items

As described in `audits/2026-09-22/areas/cli-ux.md` F17 (lines ~235-247):

| 09-04 item | Description available now |
|---|---|
| 12 | Surface the termination reason (why exploration of a function stopped) in the report |
| 16 | scan prints a double report and absolute paths |
| 17 | `Wrote ... artifact -> <abs path>` is printed at info level |
| 20 | Not described in any file in the `audit-2026-09-22` tree |

## Source location (known gap)

The 2026-09-04 report is referenced by the str-qwua7 epic as "audits/2026-09-04.md on branch audit-2026-09-04, with per-area reviewer reports ... under audits/2026-09-04/". On 2026-09-23 no local branch, tag or `origin` ref named `audit-2026-09-04` exists (`git for-each-ref`, `git ls-remote origin 'audit-2026-09-04*'`), and `git log --all -- 'audits/2026-09-04/usability-ui.md'` finds nothing. Item 20's text may therefore be unrecoverable.

## Acceptance criteria

- [ ] The source is searched in this order, and the close comment records each result: `git log --all` for `audits/2026-09-04*` in shatter; `git fsck --lost-found` / unreachable commits containing `usability-ui.md`; other worktrees under `~/.local/share/worktrees/shatter/`; asking the maintainer. If found, cite the exact commit and path of `usability-ui.md`.
- [ ] For each of items 12, 16, 17 and 20, the close comment records exactly one outcome: **covered by <existing id>** (with the `bd show` title and why it covers the item), **covered by a 2026-09-22 draft slug**, **new draft** (path of a draft file in the audit drafts area, written in the repo's issue-draft format, for the maintainer to file), or **unrecoverable** (item 20 only, if the source is not found).
- [ ] Each item is checked against at least str-9ee5, the open children of str-qwua7, and this audit's drafts in the shatter-cli-flags-and-help and shatter-cli-runtime-output buckets, using targeted `bd search` terms listed in the close comment.
- [ ] No tracker issue is created by the implementing agent; drafts are handed to the maintainer.

## Out of scope

- Items 9 and 18 (help-tracker-ids-lint, explore-format-flag-ignored).
- Changing the audit filing process (str-qwua7.22).

## Dependencies

- Blocked by: none.
- Related: str-qwua7, str-qwua7.22, str-9ee5, help-tracker-ids-lint, explore-format-flag-ignored.

## Source

Audit 2026-09-22 finding cli-ux-17 (areas/cli-ux.md F17). Split out of help-tracker-ids-lint.
