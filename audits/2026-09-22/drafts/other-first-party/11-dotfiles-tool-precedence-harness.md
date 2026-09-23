# Reconcile the Read/Grep-first rule with the harness bypass-mode guidance

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P3
- labels: documentation
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (dotfiles#10, #11 closed; shatter str-qwua7.26)
- source findings: sessions-16

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`codex/AGENTS.md` "Tool-Specific Notes" says to prefer Read/Grep/Glob over Bash cat/grep/find. The Claude Code bypass-permissions harness text explicitly allows cat/grep/sed through Bash. Agents follow the harness. Since 2026-09-04, Shatter sessions made 655 Bash calls led by grep/cat/sed/find (grep 372, cat 151, sed 70, find 58), against Read 379, Grep 128 and Glob 11. `find` failed 10% of the time, including `rtk find does not support compound predicates or actions (e.g. -not, -exec)`. Before refiling that last part, check whether it predates the #11 prefilter landing (88e9cb5, 2026-09-08).

## Acceptance criteria

- [ ] Global guidance acknowledges the harness mode and states the rule that matters: Read for files being edited, Grep for multi-file search, and plain shell only for one-off reads.
- [ ] If rtk still rewrites `find` with `-not`/`-exec` after 88e9cb5, the prefilter passes those forms through, with a test.

## Source

Shatter audit 2026-09-22 finding sessions-16.
