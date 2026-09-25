# Add a "waiting for background work" rule and a no-op-poll guard hook

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P2
- labels: enhancement, documentation
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (shatter str-qwua7.26 covers only 'no foreground sleep')
- source findings: sessions-01

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

Agents busy-wait on background tasks with loops of `sleep 1; echo ok` and repeated `ReadNotifications` calls, which burns hundreds of turns and a very large number of cache-read tokens. Evidence from Shatter transcripts since 2026-09-04:

- 820 `sleep N[; echo x]` calls across 12 sessions, plus 600 ReadNotifications polls.
- Session `87606e10-4dce-47bb-a273-768abd7be0a2`: 1,575 tool calls, of which 609 were `sleep 1; echo ok` ("No-op wait for code review notification") and 598 were ReadNotifications. It produced 541 turns that start with "Still waiting" and about 934M cache-read tokens.
- Session `9f13ca23-...`: 200 `sleep 1` calls with description "yield".
- The agent writes "I'll wait for the background task notification instead of polling." and then keeps polling. The harness sleep block says "Do not chain shorter sleeps to work around this block."

No global instruction tells agents that ending the turn is the correct way to wait. Shatter `AGENTS.md:403-408` also tells team leads to "actively poll… never idle".

## Acceptance criteria

- [ ] `~/dotfiles/docs/agent-guidance.md` (or a leaf it inlines per the guidance-loading issue) contains a "Waiting for background work" rule. After launching `run_in_background`, the agent should end the turn, use a blocking TaskOutput / Monitor until-loop, or run one bounded background until-loop. Never `sleep <=5`/`echo` loops, and never more than 3 consecutive ReadNotifications with no other tool call.
- [ ] A PreToolUse hook in dotfiles `claude/settings.json` denies bare `sleep N` / `sleep N; echo X` Bash commands, and denies a 4th consecutive ReadNotifications with no intervening tool, with a message pointing at the rule.
- [ ] Hook has tests (fixture payloads for allowed and denied cases).
- [ ] A follow-up note is filed in shatter to reword the `AGENTS.md` "team-lead liveness … never idle" text so it means event-driven checks.

## Suggested approach

Implement the hook as a small Python script that reads the PreToolUse JSON and keeps a per-session counter in `$XDG_RUNTIME_DIR` for ReadNotifications.

## Related

The bento check-unpushed Stop hook blocks turn end during long landings (bento finding sessions-10), which pushes agents toward busy loops. That is tracked in bento. This issue covers only the global rule and guard.

## Source

Shatter audit 2026-09-22 finding sessions-01 (`audits/2026-09-22/areas/sessions.md`).
