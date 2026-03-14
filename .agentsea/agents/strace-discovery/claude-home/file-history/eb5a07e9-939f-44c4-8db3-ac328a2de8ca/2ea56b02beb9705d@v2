---
name: Agent teams workflow preference
description: User wants TeamCreate with worktree isolation for parallel agent work — visible in tmux panes, not background subagents
type: feedback
---

## Agent Teams Workflow

- Use `TeamCreate` + `Agent` with `team_name` and `isolation: "worktree"` for parallel work
- Do NOT use background subagents (`run_in_background: true`) — user wants to see agent activity in tmux panes
- Teammates must be spawned in foreground so they appear as visible tmux panes
- `mode: "bypassPermissions"` works with TeamCreate teammates
- Edit and Write are in the global allowlist (`~/.claude/settings.json`) — required for agents to work

## Permission Setup

- `~/.claude/settings.json` must include `"Edit"` and `"Write"` in `permissions.allow` for agents to edit files
- Permissions are snapshotted at session start — changes require a session restart
- The `Agent` tool with `isolation: "worktree"` (subagents) cannot escalate permissions beyond the lead's allowlist
