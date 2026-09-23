---
slug: blocked-escalation
kind: new
title: "Sessions stall for hours on unseen questions and classifier denials: add escalation guidance"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Sessions stall for hours on unseen questions and classifier denials: add escalation guidance

Part of #<epic>. Priority: P3. The verifier lowered this from P2 because part of the root cause is harness UI, outside first-party control. Type: enhancement.

## Problem

Two situations stall sessions for many hours: a question the user never sees, and an auto-mode classifier denial. In both, the agent waits silently. `~/dotfiles/docs/agent-guidance/questions.md` (7 lines) covers how often to ask and how to number questions. It says nothing about how to make a blocking question visible, or what a classifier denial means.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`:

- `455c2cd7-3b18-4803-a9ec-2e1e543c18c1`: the auto-mode classifier denied `git push --no-verify origin 7fbfa7ab:refs/heads/main` at 2026-09-08T13:34. The session then idled until the user wrote "I don't see a question--maybe it got pushed back" at 2026-09-09T15:44. The verifier confirmed this quote.
- `5617a8ed`: an AskUserQuestion at 2026-09-09T16:43 got an empty result. The next action came at 2026-09-10T18:57, after the user typed "try again". The verifier did not re-check these timestamps.
- Since 2026-09-04 there have been 14 classifier denials across 9 sessions. The verifier did not recount this.
- Earlier, the user said: "don't wait 21 hours.  wait 15 minutes." (`5f2377ef`, 2026-08-29).
- Re-verified at dotfiles @ `81f35e1`: `grep -in "PushNotification\|classifier\|blocked" docs/agent-guidance/questions.md` finds nothing.

## Acceptance criteria

- [ ] `docs/agent-guidance/questions.md` says what to do when the work is blocked on the user. Send a PushNotification (where available) with the one-line question, and restate the question in plain text at the end of the turn, not only inside AskUserQuestion.
- [ ] It says that an auto-mode classifier denial means "use the sanctioned tool or path", for example the bento land-work driver for pushes to main. It does not mean "wait", and it does not mean "retry with a bypass" (consistent with `never-recommend-bypass`).
- [ ] The summary line is in the core-rules file from `global-guidance-actually-loads`.
- [ ] Proof at close: the closing comment shows the output of `grep -n "PushNotification\|classifier" ~/dotfiles/docs/agent-guidance/questions.md` and `codex/agents-sync.sh status` reporting `render vs snapshot: ok`.

## Out of scope

Harness UI bugs where a question is not displayed.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` and `never-recommend-bypass`.

## Source

Shatter audit 2026-09-22 finding sessions-13 (verifier: partially confirmed, lowered to P3).
