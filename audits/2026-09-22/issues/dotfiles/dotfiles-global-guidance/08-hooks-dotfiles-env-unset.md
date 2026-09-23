---
slug: hooks-dotfiles-env-unset
kind: new
title: "Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running"
priority: P3
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running

Part of #<epic>. Priority: P3. Type: bug.

## Problem

Several hook commands in `~/dotfiles/claude/settings.json` start with `$DOTFILES/...`. `DOTFILES` is exported only by `bashrc:101`, which runs after the interactive-shell guard at `bashrc:3-4`. The settings `env` block does not set it. In teammate, remote and spawned sessions the variable is empty, and the hooks fail with `/bin/sh: 1: /claude/tmux-state.sh: not found`. Because those errors are non-blocking, neither the user nor the agent sees them.

## Evidence

Re-verified at dotfiles @ `81f35e1`, `claude/settings.json`:

- `:285` PreToolUse (Bash): `python3 $DOTFILES/claude/rtk_prefilter.py`. The source finding missed this one. When `DOTFILES` is unset the prefilter fails, so the #11 byte-identity protection and the rtk rewrite both silently do not run.
- `:334` SessionStart: `cat | $DOTFILES/claude/tmux-state.sh session-start`.
- `:362` Stop: `$DOTFILES/claude/tmux-state.sh stop`, `$DOTFILES/claude/generate_summary.py` and `$DOTFILES/claude/generate_session_name.py`. Session summaries and names silently do nothing.
- `:373` PermissionRequest and `:384` UserPromptSubmit: `$DOTFILES/claude/tmux-state.sh`.
- `:33-35` `env` sets only `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`.
- In Shatter transcripts there are 94 `hook_non_blocking_error` attachments across 47 sessions (36 across 18 sessions since 2026-09-04). 23 transcript files contain `tmux-state.sh: not found`.

## Acceptance criteria

- [ ] No hook command in `claude/settings.json` or in any host overlay under `claude/hosts/` depends on `$DOTFILES`. Either the commands use `$HOME/dotfiles/...`, or `DOTFILES` is set in the settings `env` block. The first is preferred because it does not depend on `env` expansion order. `grep -n '\$DOTFILES' claude/settings.json claude/hosts/*/settings.json` returns nothing, or matches only the `env` definition.
- [ ] A test under `claude/tests/` runs every hook command from the rendered settings with `env -i HOME=$HOME PATH=/usr/bin:/bin sh -c '<command>' </dev/null` and asserts that none exits 127 or prints `not found`. It must fail on the current tree.
- [ ] Proof at close: the closing comment includes the test's before and after output, plus one fresh non-interactive session (for example a spawned teammate) whose transcript has no `hook_non_blocking_error` for these hooks.
- [ ] Optional: a SessionStart self-check warns once when any hook script path does not resolve.

## Out of scope

Bento's agent-env doctor (bento-m4y5) checking global hook paths. It could be extended later, but the root fix belongs here.

## Dependencies

None. New hooks from `background-wait-rule-and-hook` and `global-guidance-actually-loads` should use `$HOME/dotfiles` paths from the start.

## Source

Shatter audit 2026-09-22 finding sessions-14.
