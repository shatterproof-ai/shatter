---
name: swarm must use TeamCreate
description: The /swarm skill MUST use TeamCreate to spawn teammates — standalone background agents fail due to permission issues
type: feedback
---

When running the `/swarm` or `/swarm-project` skill, you MUST follow the
skill's Phase 2 protocol exactly:

1. Create a team via `TeamCreate` (e.g. `swarm-YYYY-MM-DD`)
2. Spawn teammates via `Agent` with `team_name: <team>` and `mode: "plan"`
3. Do NOT use `run_in_background: true` with standalone agents

**What went wrong (2026-03-11):** A session spawned 6 standalone background
agents with `mode: "bypassPermissions"` and `run_in_background: true` instead
of using TeamCreate. All 6 agents failed because background agents cannot get
interactive tool approval. The entire swarm had to be redone manually.

**Why TeamCreate matters:** Team-based agents inherit the user's allowed tool
permissions from `settings.json`. Standalone background agents do not — they
get auto-denied on any tool requiring confirmation.

**Rule:** Never shortcut the swarm skill's team setup. Read and follow the
skill file (`/home/ketan/dotfiles/claude/skills/swarm/SKILL.md`) Phase 2
exactly as written.
