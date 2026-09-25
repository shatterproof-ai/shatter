# Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children

- Filing action: new issue
- Priority: P3
- Type: feature
- Labels: audit, beads-issue-flow, land-work
- Parent: epic
- Source findings: prior-11

---BODY---
## Problem

Code-review follow-ups are filed as children of the issue that is about to be closed, so they become permanent orphans. Shatter drift-patrol lists 4 of them:

- str-qwua7.56.1 (parent str-qwua7.56 closed)
- str-qwua7.9.1 (parent str-qwua7.9 closed)
- str-hy9b.J3 and str-hy9b.1 (epic str-hy9b closed 2026-04-27, open for 5 months)

Shatter's epic-lifecycle rule issue, str-5b9f, has been open since June.

## Current code facts

- The follow-up filing guidance in `catalog/skills/land-work/SKILL.md` and `catalog/skills/beads-issue-flow/SKILL.md` does not say where follow-ups go.
- bd supports `--deps discovered-from:<id>` and `--parent`.

## Acceptance criteria

- beads-issue-flow and land-work say: file follow-ups as siblings under the same parent epic, or as top-level issues, with `discovered-from:<closing-id>`, never as children of the issue being closed.
- The close helper (see the close-reason-evidence issue) refuses to close an issue that has open children unless `--force` is given with a reason.
- A test covers the refusal.

## Out of scope

- Re-parenting shatter's existing orphans. That is shatter tracker work.
