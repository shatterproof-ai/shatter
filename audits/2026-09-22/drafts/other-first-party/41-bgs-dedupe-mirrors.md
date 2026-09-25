# Close the 55 bugshot-* mirror duplicates of bgs-* issues

## Filing metadata

- tracker/repo: bugshot
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

`bd list --all --json` shows 115 issues: 60 `bgs-*` and 55 `bugshot-*`, with 54 titles duplicated across the two prefixes. This looks like a prefix rename or import that duplicated the database. Open twin pairs include bgs/bugshot-47p, -hx3, -7g0 and -wyf; 6zc is deferred. `bd ready` and search results are doubled, and claims can land on either twin.

## Acceptance criteria

- [ ] Every `bugshot-*` issue whose title matches a `bgs-*` issue is closed as a duplicate, with a reason naming the twin. Any state held only on the mirror (comments, status) is copied to the `bgs-*` twin first.
- [ ] `bd list --all` shows no title-identical open pairs across prefixes.

## Source

Shatter audit 2026-09-22 finding plugins-16.
