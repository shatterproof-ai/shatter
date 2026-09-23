# CLAUDE.md points at AGENTS.md without importing it

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: bug
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-17

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`/home/ketan/project/shatter-agents/CLAUDE.md` (219 bytes) says "See [AGENTS.md](AGENTS.md) for the canonical agent guide … the content of interest is in AGENTS.md" with no `@AGENTS.md`. Claude sessions in this repo therefore do not load rules such as "never edit plugins/" and "run build-plugins". `bugshot/CLAUDE.md` is exactly `@AGENTS.md`.

## Acceptance criteria

- [ ] `CLAUDE.md` body is `@AGENTS.md`.
- [ ] A fresh Claude session in the repo shows AGENTS.md content loaded (for example, it can quote the build-plugins rule without reading the file).

## Related

A generic doctor check for "mentions AGENTS.md without importing it" belongs in bento (agent-env-doctor, bento-m4y5).

## Source

Shatter audit 2026-09-22 finding plugins-17.
