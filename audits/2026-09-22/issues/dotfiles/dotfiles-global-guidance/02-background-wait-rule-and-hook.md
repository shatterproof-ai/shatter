---
slug: background-wait-rule-and-hook
kind: new
title: "Agents busy-wait on background work with alternating sleep-1 and ReadNotifications loops: add a waiting rule and a PreToolUse guard"
priority: P2
type: enhancement
labels: [enhancement, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents busy-wait on background work with alternating sleep-1 and ReadNotifications loops: add a waiting rule and a PreToolUse guard

Part of #<epic>. Priority: P2 (the verifier lowered it from P1: it wastes turns and tokens but does not produce incorrect repository state). Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

After starting background work, agents busy-wait instead of ending the turn. The dominant pattern alternates a bare `sleep 1; echo ok` Bash call with a `ReadNotifications` call, hundreds of times. The harness blocks long foreground sleeps, and its block text says "Do not chain shorter sleeps to work around this block." Agents chain short sleeps anyway. No global instruction says that ending the turn is the correct way to wait.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, counted since 2026-09-04:

- 820 `sleep N[; echo x]` calls across 12 sessions, and 600 ReadNotifications polls.
- Session `87606e10-4dce-47bb-a273-768abd7be0a2` (claude-sonnet-5, 2026-09-19 to 09-21):
  - 1,575 tool calls. 609 were bare `sleep 1; echo ok` (description "No-op wait for code review notification") and 598 were ReadNotifications.
  - Adjacent tool-call pairs, recounted on 2026-09-23: `sleep` then `ReadNotifications` 596 times, `ReadNotifications` then `sleep` 487 times. The loop alternates; there are almost no back-to-back ReadNotifications runs.
  - 541 assistant turns begin "Still waiting". 934,138,123 cache-read tokens.
- Session `9f13ca23-bf49-4efb-abd0-ed3519e0dd38` made 200 `sleep 1` calls with the description "yield".
- The agent writes "I'll wait for the background task notification instead of polling." and then keeps polling.
- Contributing text: shatter `AGENTS.md:405-409` ("Team-lead liveness") tells the lead to "actively poll teammate liveness … never idle until the user notices". Re-verified at shatter `56c86168`.
- There is no waiting rule anywhere in `~/dotfiles/docs/agent-guidance/` (dotfiles @ `81f35e1`).

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/waiting.md` states the rule. After a `run_in_background` launch, or while waiting on a teammate or a notification, do one of these: end the turn; use a blocking wait (TaskOutput with block, or a Monitor until-loop); or run one bounded background until-loop. Never poll with short sleeps or repeated notification checks.
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it. Nothing is added to `codex/AGENTS.md` (dotfiles#23 word cap).
- [ ] A PreToolUse hook `claude/wait_guard.py`, registered in `claude/settings.json` with a `$HOME/dotfiles/...` path, denies with a message that names `waiting.md`:
  - a Bash command that is only `sleep N`, or `sleep N` joined to `echo …`/`true`/`:` by `;`, `&&` or a newline;
  - a ReadNotifications call when the session's recent history is wait-only. Define "wait-class" calls as ReadNotifications and the bare-sleep Bash commands above (including denied ones). The guard keeps a per-session count of ReadNotifications calls since the last non-wait-class tool call. Only non-wait-class calls reset it. It denies the 4th.
- [ ] Commands that merely contain `sleep` (such as `sleep 2 && curl …`, or a `timeout` wrapper) are allowed and do not count as wait-class.
- [ ] Test `claude/tests/test_wait_guard.py`, run with `python3 -m pytest claude/tests/test_wait_guard.py -q` (`claude/tests/run.sh` discovers only `test_*.sh`, so it does not run Python tests). It feeds fixture PreToolUse JSON payloads under one session id and covers:
  - the motivating alternating sequence `sleep 1; echo ok` → ReadNotifications → `sleep 1; echo ok` → ReadNotifications → … : the sleeps are denied, and the 4th ReadNotifications is denied;
  - four adjacent ReadNotifications: the 4th is denied;
  - ReadNotifications ×3 → `Read` → ReadNotifications: allowed (a real tool call resets the count);
  - the allowed-`sleep` cases above;
  - two session ids do not share a counter.

## Proof at close

The closing comment includes the pytest output, and the guard's denial message as seen in one live session.

## Suggested approach

A small Python script that reads the PreToolUse JSON from stdin. It matches `tool_name == "Bash"` against an anchored regex. It keeps the counter in `${XDG_RUNTIME_DIR:-/tmp}/claude-wait-guard/<session_id>`. Model it on `claude/rtk_prefilter.py` and `claude/tests/test_rtk_prefilter.py`.

## Out of scope

- Rewording shatter `AGENTS.md:405-409` to mean event-driven liveness checks. That text lives in the shatter repo. The closing comment should point the maintainer at it; no issue for it exists in this audit.
- The bento check-unpushed Stop hook, which blocks turn end during long landings and pushes agents toward busy loops. That is drafted in bento as `check-unpushed-overcount-and-blocks` (source sessions-10).
- The shatter-local "no foreground sleep" line in str-qwua7.26.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` (core summary) and `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding sessions-01, in `audits/2026-09-22/areas/sessions.md`.
