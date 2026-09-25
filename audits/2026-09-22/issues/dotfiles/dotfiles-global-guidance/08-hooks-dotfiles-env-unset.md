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

Part of #<epic>. Priority: P3. Type: bug. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

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

- [ ] No hook command in `claude/settings.json` or in any host overlay under `claude/hosts/` depends on `$DOTFILES`. The commands use `$HOME/dotfiles/...` (preferred, because it does not depend on `env` expansion order), or `DOTFILES` is set in the settings `env` block. `grep -n '\$DOTFILES' claude/settings.json claude/hosts/*/settings.json` returns nothing, or matches only the `env` definition.
- [ ] Test `claude/tests/test_hook_paths.py`, run with `python3 -m pytest claude/tests/test_hook_paths.py -q` (`claude/tests/run.sh` runs only `test_*.sh`). It is isolated and exercises only the affected hooks:
  - It builds a temp `HOME` containing `dotfiles/claude/` with **stub** versions of `tmux-state.sh`, `rtk_prefilter.py`, `generate_summary.py` and `generate_session_name.py`. Each stub appends its own name and arguments to a log file and exits 0. No real hook, `tmux`, `bd`, `rtk` or ollama call runs.
  - For each affected hook command (PreToolUse Bash, SessionStart `tmux-state.sh`, Stop, PermissionRequest, UserPromptSubmit), taken from the rendered settings, it runs `env -i HOME=<tmp> PATH=/usr/bin:/bin sh -c '<command>'` with a representative event payload on stdin. The Stop payload includes a `transcript_path` pointing at a temp file, so the summary and session-name branches run.
  - It asserts that every stub the command should reach appears in the log. Exit codes alone are not enough: the Stop command ends in `; true`, and its Python scripts run in the background.
  - It waits for background stubs, with a bounded timeout, before reading the log.
  - Unrelated SessionStart commands (`bd prime`, `ensure-dolt.sh`, `tmc/agent-track`) are not run.
- [ ] The test fails on the current tree (the stubs are never reached, because `$DOTFILES` expands to empty) and passes after the change.
- [ ] Optional: a SessionStart self-check warns once when any hook script path does not resolve.

## Proof at close

The closing comment includes the test's red and green output, and one fresh non-interactive session (for example a spawned teammate) whose transcript has no `hook_non_blocking_error` for these hooks.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Bento's agent-env doctor (bento-m4y5) checking global hook paths. It could be extended later, but the root fix belongs here.

## Dependencies

None. New hooks from `background-wait-rule-and-hook`, `global-guidance-actually-loads` and `first-party-plugin-staleness-check` should use `$HOME/dotfiles` paths from the start.

## Source

Shatter audit 2026-09-22 finding sessions-14.
