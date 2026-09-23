---
slug: background-wait-rule-and-hook
kind: new
title: "Agents busy-wait on background work with sleep-1/echo and ReadNotifications loops: add a waiting rule and a PreToolUse guard"
priority: P2
type: enhancement
labels: [enhancement, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents busy-wait on background work with sleep-1/echo and ReadNotifications loops: add a waiting rule and a PreToolUse guard

Part of #<epic>. Priority: P2 (the verifier lowered it from P1: it wastes turns and tokens but does not produce incorrect repository state). Type: enhancement.

## Problem

After starting background work, agents busy-wait instead of ending the turn. They loop over `sleep 1; echo ok` and repeated `ReadNotifications` calls, burning hundreds of turns and a very large number of cache-read tokens. The harness blocks long foreground sleeps, and its block text says "Do not chain shorter sleeps to work around this block." Agents chain short sleeps anyway. No global instruction says that ending the turn is the correct way to wait.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, counted since 2026-09-04:

- 820 `sleep N[; echo x]` calls across 12 sessions, and 600 ReadNotifications polls.
- Session `87606e10-4dce-47bb-a273-768abd7be0a2` (claude-sonnet-5, 2026-09-19 to 09-21):
  - 1,575 tool calls. 609 were bare `sleep 1; echo ok` (description "No-op wait for code review notification") and 598 were ReadNotifications. The verifier recounted these.
  - 541 assistant turns begin "Still waiting".
  - 934,138,123 cache-read tokens.
- Session `9f13ca23-bf49-4efb-abd0-ed3519e0dd38` made 200 `sleep 1` calls with the description "yield".
- The agent writes "I'll wait for the background task notification instead of polling." and then keeps polling.
- Contributing text: shatter `AGENTS.md:405-409` ("Team-lead liveness") tells the lead to "actively poll teammate liveness … never idle until the user notices". Re-verified at shatter `56c86168`.
- There is no waiting rule anywhere in `~/dotfiles/docs/agent-guidance/` (dotfiles @ `81f35e1`).

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/waiting.md` states the rule. After a `run_in_background` launch, or while waiting on a teammate or a notification, do one of these:
  - end the turn;
  - use a blocking wait (TaskOutput with block, or a Monitor until-loop);
  - run one bounded background until-loop.

  Never use `sleep` ≤ 5 s or `sleep … ; echo …` loops. Never make more than 3 consecutive ReadNotifications calls with no other tool call in between.
- [ ] The rule's one-line summary is in the core-rules file from `global-guidance-actually-loads`. It is not added to `codex/AGENTS.md`, because dotfiles#23 caps that file's word count.
- [ ] A PreToolUse hook, registered in `claude/settings.json` with a `$HOME/dotfiles/...` path, denies two things, each with a message that names `waiting.md`:
  - a Bash command that is only `sleep N` or `sleep N; echo …` / `sleep N && echo …`;
  - the 4th consecutive ReadNotifications with no other tool call in between.
- [ ] Commands that merely contain `sleep`, such as `sleep 2 && curl …` inside a real script or a `timeout` wrapper, are allowed.
- [ ] Proof at close: `claude/tests/test_wait_guard.py`, run by `claude/tests/run.sh`, feeds fixture PreToolUse JSON payloads for the allowed and denied cases, including the ReadNotifications counter across four payloads with one session id. The closing comment includes the run output.

## Suggested approach

A small Python script (`claude/wait_guard.py`) that reads the PreToolUse JSON from stdin. It matches `tool_name == "Bash"` against an anchored regex. For ReadNotifications it keeps a per-session counter in `${XDG_RUNTIME_DIR:-/tmp}/claude-wait-guard/<session_id>`. Any other tool call resets the counter. Model it on `claude/rtk_prefilter.py` and `claude/tests/test_rtk_prefilter.py`.

## Out of scope

- Rewording shatter `AGENTS.md:405-409` to mean event-driven liveness checks. That belongs to the shatter repo, and whoever implements this issue should file it there once the rule lands.
- The bento check-unpushed Stop hook, which blocks turn end during long landings and pushes agents toward busy loops. That is tracked in bento as `check-unpushed-overcount-and-blocks` (source sessions-10).
- The shatter-local "no foreground sleep" line in str-qwua7.26.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` (the core summary) and `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding sessions-01, in `audits/2026-09-22/areas/sessions.md`.
