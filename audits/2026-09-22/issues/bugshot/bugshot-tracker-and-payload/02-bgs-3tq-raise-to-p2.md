---
slug: bgs-3tq-raise-to-p2
kind: note-to-existing
title: "Note on bgs-3tq: raise to P2 because shatter str-qwua7.53 (P2) depends on it"
priority: P2
type: feature
labels: [audit-2026-09-22, cross-repo]
parent_epic: "(existing issue; not reparented)"
blocked_by: []
existing_id: bgs-3tq
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Note on bgs-3tq: raise to P2

Target: **bgs-3tq** (open, P3, feature, "Document and template a CLI/TUI
capture-command (`wire-bugshot --kind cli`)", created and last updated
2026-09-05).

Actions:

1. `bd update bgs-3tq --priority 2`
2. `bd comments add bgs-3tq` with the text below.

Do not change its scope or reparent it under the audit epic.

## Comment text

> Audit 2026-09-22 (Shatter audit finding plugins-08): raising this from P3 to
> P2 because of a cross-repo dependency. The two issues have no dependency
> edge between them. Direct issue-ID dependencies do not work across separate
> bd databases. `bd dep add` does support `external:<project>:<capability>`
> references, but `external_projects` is not configured here
> (`bd config get external_projects` returns "not set"). Even if such an edge
> existed, it would not change this issue's priority. So the priority is
> raised by hand.
>
> - Dependent: shatter **str-qwua7.53** "Wire bugshot for walkthrough output
>   once bugshot supports CLI capture (bgs-3tq)". It is P2 and OPEN, created
>   2026-09-07, and implements the 2026-09-06 maintainer decision to adopt
>   bugshot in shatter for walkthrough/gauntlet ANSI transcripts.
> - Until this issue lands, bento's agent-env doctor keeps reporting bugshot as
>   "dormant: capture-command missing" in every shatter session, and
>   str-qwua7.53 cannot start.
> - Verified 2026-09-23 (live bd): `bd show bgs-3tq` shows P3 OPEN;
>   `bd show str-qwua7.53` (in /home/ketan/project/shatter) shows P2 OPEN.
>
> When this closes, comment on str-qwua7.53 in the shatter tracker so the
> dependent can be unblocked. The related AGENTS.md/README gap for the viz
> skills is tracked separately in the audit issue
> "AGENTS.md covers only the bugshot skill ..." and leaves ANSI capture
> documentation here.

## Close proof

This is a note, and nothing closes it. It is complete when `bd show bgs-3tq`
reports P2 and the comment above is present.
