---
slug: u394l-3-pending-turns-fail
kind: note-to-existing
title: "Note on str-u394l.3: drift-patrol PENDING slots unimplemented for 3+ months; a PENDING slot older than 60 days should turn FAIL"
priority: P2
type: task
labels: [quality-gates, drift-patrol, stories, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-u394l.3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-u394l.3 (open, P2: "Stories coverage gate")

Target: `str-u394l.3`. Post the text below as a comment. Do not change status.

## Comment text

> **Audit 2026-09-22 (finding prior-21): the PENDING placeholders never expire.**
>
> drift-patrol still reports two placeholder slots, both open for more than 3 months:
> - `docs-stories`: PENDING, tracked by **this issue** (open since 2026-06-17; `scripts/drift-patrol.py:453`ff., `check_docs_stories`).
> - `cli-surface-drift`: PENDING, tracked by str-wurp (open since 2026-06-12; `scripts/drift-patrol.py:437`ff.).
>
> `docs/stories/` is still absent (verified at 56c86168), and str-qwua7.52 ("Adopt storystore", decided 2026-09-06) is still unstarted.
>
> By design, PENDING does not fail the patrol (`scripts/drift-patrol.py:25-32`). A PENDING slot only becomes FAIL in two cases: under `--strict-pending`, or when its tracking issue is *closed* (`:290-307`). A slot whose issue simply stays open is therefore a reminder that never escalates. Combined with the scheduled patrol not running (audit finding prior-01), nobody sees even the reminder.
>
> **Proposal (auditor's, for the maintainer to accept or reject):** add an age limit. A PENDING slot whose tracking issue has been open more than **60 days** (by its `created_at`, or a per-slot `pending_since` date recorded in `drift-patrol.py`) reports FAIL, with a message naming the issue and its age. A maintainer can extend a slot deliberately by bumping `pending_since` in a commit, which leaves a visible record of the decision.
> - Acceptance: a unit test covers 59 days (PENDING) and 61 days (FAIL).
> - Acceptance: `docs/DRIFT-PATROL.md` documents the rule.
> - Result at close: under this rule, both slots above would fail today. That is the intended pressure, and it is resolved by landing this issue and str-wurp, or by explicitly re-dating them.
>
> The rule is patrol-wide, not stories-specific. If the maintainer prefers, move it to a small child issue of the drift-patrol work (str-u394l). It is posted here because this issue owns one of the two stale slots.
>
> Current blocker for this issue's coverage gate: storystore's inventory finds 0 clap CLI surfaces in shatter (see the note on str-qwua7.52 and the storystore issue clap-cobra-extractors).
