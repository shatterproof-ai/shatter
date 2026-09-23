---
slug: close-agents-mirror-issues
kind: new
title: "Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)"
priority: P2
type: chore
labels: [tracker-hygiene, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)

## Problem

The shatter-agents tracker holds 67 issues: 54 `sa-*` and 13 `agents-*`. Every `agents-*` issue has an `sa-*` twin with an identical title. This looks like a prefix rename or import that duplicated the database. Most pairs are closed on both sides, but two pairs are still live, so work appears twice in `bd ready`, `bd list` and in claims:

- `sa-xj6` / `agents-xj6` ("Add curl pipe install instruction for Codex"): both are IN_PROGRESS.
- `sa-ya6` / `agents-ya6` ("refute-from-plan skill"): both are DEFERRED.

Separately, `sa-d1b` ("Add shatter-advise and shatter-gaps skills") is still OPEN, although both skills ship in `catalog/skills/` and in the published payloads.

## Evidence

Re-verified on 2026-09-23 with `bd list --all --json` in `/home/ketan/project/shatter-agents`: 67 issues in total (54 `sa-`, 13 `agents-`). There are 13 title-identical cross-prefix pairs: 3lu, 8d6, 1fc, 08h, xj6, fqc, b33, smh, 16f, 4z3, 4tm, gre and ya6. Of these, xj6 is in_progress on both sides and ya6 is deferred on both sides; the rest are closed on both sides.

- `bd show sa-d1b` shows OPEN, P2. `catalog/skills/shatter-advise/` and `catalog/skills/shatter-gaps/` exist and are listed in `catalog/plugins.json`.

## Acceptance criteria

- [ ] Each non-closed `agents-*` issue that has a title-identical `sa-*` twin (currently agents-xj6 and agents-ya6) is closed as a duplicate, with a reason naming the twin. The closed/closed pairs may be left alone or annotated. The one-liner used for the check, run after the cleanup, prints no pair in which both sides are non-closed. Paste its output in the close comment.
- [ ] `sa-d1b` is closed with evidence (the skill paths and the landing commit), or narrowed to whatever part is still unshipped.
- [ ] `sa-xj6`'s claim state is reconciled: either it is in_progress with a live owner and a recent update, or it is released back to open.
- [ ] The tracker changes are persisted using the repo's current beads sync convention. Do not hand-edit `.beads/issues.jsonl`. Bento's beads-issue-flow is getting Dolt-remote sync guidance from this audit (D4), so follow whatever it says at the time of closing.

## Suggested approach

Run `bd list --all --json`, group by title, and close the `agents-*` side with `bd close <id> --reason "duplicate of sa-<x>"` (check `bd close --help` for the duplicate flag). Look at `sa-xj6` against README history to decide whether it has shipped.

## Out of scope

- A generic cross-prefix duplicate detector. That belongs in bento beads-issue-flow.
- The bugshot half of the same finding (bugshot-* and bgs-* mirrors), which is filed in the bugshot tracker.

## Dependencies

None.

## Priority / Type / Labels

P2 · chore · tracker-hygiene, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-16 (the shatter-agents half).
