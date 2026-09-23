---
slug: 49pg-dolt-remote-section-note
kind: note-to-existing
title: "Note on bento-49pg: a follow-up adds the beads-issue-flow \"Snapshot and Dolt remote\" section your one-sentence rule can point to"
priority: P2
type: note
labels: [audit, beads-issue-flow, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-49pg
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-49pg

Target: bento-49pg (open, P2, "Untrack the Beads JSONL export ..."). Action: add a comment. Scope unchanged. This draft gives the filer the edge beads-dolt-remote-guidance blocked-by bento-49pg.

## Comment text

Audit 2026-09-22 (shatter; finding bento-16; maintainer decision D4). Shatter's bd post-checkout hook spends about 6 minutes importing `.beads/issues.jsonl` on every checkout, worktree creation and landing preview, and its AGENTS.md still requires `bd sync`, which bd 1.1.0 does not have. <id of beads-dolt-remote-guidance> adds a `## Snapshot and Dolt remote` section to beads-issue-flow (no JSONL import on checkout; linked worktrees share one database per `bd worktree --help`; Dolt remote only for separate clones or machines; no `bd sync`) and two doctor checks (import-on-checkout, `bd sync` in repo docs). It is blocked by this issue and keeps this issue's one-sentence rule and `check_tracked_beads_export` as the single source for untracking; it only adds the longer section and links to it.
