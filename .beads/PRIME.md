# Beads Context

Workflow rules (commands, issue types, priorities, decomposition, git merge,
session-close protocol) live in `AGENTS.md` and shared `beads.md` — not repeated here.

On session start or after compaction, recover dynamic state:

- `bd ready` — issues ready to work (no blockers)
- `bd list --status=in_progress` — your active work
- `bd memories` — persistent insights across sessions (add with `bd remember "…"`)
