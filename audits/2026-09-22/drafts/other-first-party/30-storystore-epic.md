# Epic: Audit 2026-09-22 findings (storystore)

## Filing metadata

- tracker/repo: storystore
- action: create new issue
- type: epic
- priority: P2
- labels: audit-2026-09-22
- parent: (none)
- dedupe relation: epic
- source findings: (epic)

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Summary

Shatter decided on 2026-09-06 to adopt storystore (shatter str-qwua7.52), but adoption is blocked by storystore itself. Its inventory cannot see Rust or Go CLIs, the installed plugin is 128 commits stale, and the storystore tracker is write-blocked. This epic tracks the storystore-side fixes.

## Done when

`stories-coverage` on the shatter repo reports its CLI subcommands, and consumers run a current plugin version.
