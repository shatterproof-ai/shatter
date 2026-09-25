# Global hooks reference `$DOTFILES`, which is unset in many sessions (94 hook failures, skipped summaries)

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: bug
- priority: P3
- labels: bug
- parent: repo epic (see INDEX)
- dedupe relation: related (bento-m4y5 doctor); treated as new
- source findings: sessions-14

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`/home/ketan/dotfiles/claude/settings.json` hook commands use `$DOTFILES/claude/tmux-state.sh` (SessionStart, UserPromptSubmit) and `$DOTFILES/claude/generate_summary.py` (Stop), at roughly lines 334/362/373/384. The settings `env` block sets only `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`. In teammate, remote and spawned sessions `$DOTFILES` is unset, so the hooks fail with `/bin/sh: 1: /claude/tmux-state.sh: not found`. Across Shatter transcripts there were 94 `hook_non_blocking_error` attachments in 47 sessions (23 files contain `tmux-state.sh: not found`). The Stop-hook summary/session naming silently does nothing in those sessions.

## Acceptance criteria

- [ ] Hook commands resolve without depending on an interactive shell: either `DOTFILES` is set in settings.json `env`, or the commands use `$HOME/dotfiles/...`.
- [ ] A fresh non-interactive session (e.g. a spawned teammate) runs SessionStart/Stop hooks with no `not found` error.
- [ ] Optionally, a SessionStart self-check warns once if any hook script path is unresolvable.

## Source

Shatter audit 2026-09-22 finding sessions-14.
