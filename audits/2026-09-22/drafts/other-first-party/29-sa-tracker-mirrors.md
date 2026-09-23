# Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: chore
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-16

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

The shatter-agents tracker holds 51 `sa-*` and 13 `agents-*` issues. Several are title-identical twins, for example `sa-xj6`/`agents-xj6` (both in_progress) and `sa-ya6`/`agents-ya6` (both deferred). This looks like a prefix rename or import that duplicated the database. `sa-d1b` ("Add shatter-advise and shatter-gaps skills") is still open although both skills ship in `catalog/skills/`.

## Acceptance criteria

- [ ] Each `agents-*` issue whose title matches an `sa-*` issue is closed as a duplicate of its twin, with a reason naming the twin. `bd list --all` shows no title-identical open pairs across prefixes.
- [ ] `sa-d1b` is closed with evidence (the skill paths plus the landing commit), or narrowed to any remaining unshipped part.
- [ ] `sa-xj6` claim state is reconciled (in_progress with a live owner, or released).

## Out of scope

A generic cross-prefix duplicate detector, which belongs in bento beads-issue-flow.

## Source

Shatter audit 2026-09-22 finding plugins-16.
