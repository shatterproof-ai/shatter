# Escalation guidance when blocked on the user or on an auto-mode classifier denial

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P3
- labels: documentation
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: sessions-13

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

Sessions stall for many hours when an AskUserQuestion prompt is not seen or an auto-mode classifier denies a command.

- Session `455c2cd7`: after the classifier denied `git push --no-verify origin 7fbfa7ab:refs/heads/main` (2026-09-08T13:34), the session idled until the user wrote "I don't see a question--maybe it got pushed back" (2026-09-09T15:44).
- Session `5617a8ed`: a question at 2026-09-09T16:43 got an empty result, and the next action came at 2026-09-10T18:57 after the user typed "try again".

## Acceptance criteria

- [ ] Global guidance says: when blocked on the user, send a PushNotification with the one-line question, and also restate the question in plain text at the end of the turn, not only in AskUserQuestion.
- [ ] Guidance says a classifier denial means "use the sanctioned tool or path" (for example the land-work driver for pushes to main), not "wait".

## Out of scope

Harness UI display bugs.

## Source

Shatter audit 2026-09-22 finding sessions-13.
