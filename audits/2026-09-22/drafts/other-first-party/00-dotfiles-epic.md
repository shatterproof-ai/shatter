# Epic: Audit 2026-09-22 findings (global agent guidance and hooks)

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: epic
- priority: P1
- labels: documentation, enhancement
- parent: (none)
- dedupe relation: epic
- source findings: (epic)

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Summary

The 2026-09-22 Shatter audit found agent-behaviour defects whose root cause is in the global agent configuration in `~/dotfiles`. This epic tracks those items. Each child issue body says "Part of #<this epic>".

## Children

The children are filed by `audits/2026-09-22/drafts/other-first-party/file.sh` in the shatter audit branch. Their titles are:

- Make the global Required-Loads guidance actually load (inline core rules; absolute paths)
- Add a "waiting for background work" rule and a no-op-poll guard hook
- Never mark a hook or gate bypass as the recommended option
- Enable plugin autoUpdate for first-party marketplaces and detect stale installed plugins
- rtk still shows summarized content for `head -N` inside compound commands
- Add a memory lifecycle rule: tooling bugs go to the tracker, and memory is a pointer that retires when the issue closes
- fail-closed guidance: validators must fail on empty extraction and carry a canary test
- Global hooks reference `$DOTFILES`, which is unset in many sessions
- Planning guidance: an experiment or benchmark plan must start with a falsification probe
- Escalation guidance when blocked on the user or on an auto-mode classifier denial
- Reconcile the Read/Grep-first rule with the harness bypass-mode guidance

## Done when

Every child is closed or explicitly deferred with a reason.
