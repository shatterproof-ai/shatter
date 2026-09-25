---
slug: doctor-state-reopen-note
kind: reopen-note
title: "Note on closed bento-rdtn.2: doctor seen/decided state is per checkout, so linked worktrees get full nudges"
priority: P2
type: note
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [doctor-state-per-worktree]
existing_id: bento-rdtn.2
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-rdtn.2

Target: bento-rdtn.2 (closed). Action: add a comment. Do not reopen; the work is tracked in the new issue. Post after doctor-state-per-worktree is filed, and replace the slug with its id.

## Comment text

Audit 2026-09-22 (shatter; finding bento-08): the collapsed-state goal of this issue is not met in linked worktrees. The seen/decided/remind_after keys live in `.agent-mode.local` at `git rev-parse --show-toplevel` (agent-env-doctor.py line 198, line 588), which is per checkout. Launch-work puts every working session in a linked worktree, so each one gets the full dormancy and superpowers text again, writes a fresh `.agent-mode.local` into its worktree root, and misses decisions recorded in the primary checkout's copy (for example `hook_bypass=allow`). Example: shatter primary has `agent_env_doctor_seen=bugshot,storystore`, while the `audit-2026-09-22` worktree's copy holds only what that session wrote. Follow-up: <id of doctor-state-per-worktree>.
