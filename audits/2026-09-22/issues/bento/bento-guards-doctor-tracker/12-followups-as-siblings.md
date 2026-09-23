---
slug: followups-as-siblings
kind: new
title: "Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children"
priority: P3
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [close-reason-evidence]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children

Source finding: prior-11 (shatter audit 2026-09-22).

## Problem

Code-review follow-ups are filed as children of the issue that is about to be closed, so they become permanent orphans. Shatter drift-patrol lists four:

- str-qwua7.56.1 (parent str-qwua7.56 closed)
- str-qwua7.9.1 (parent str-qwua7.9 closed)
- str-hy9b.J3 and str-hy9b.1 (epic str-hy9b closed 2026-04-27; open for 5 months)

Shatter's epic-lifecycle rule issue, str-5b9f, has been open since June.

## Current code facts (bento origin/main @ b1bb787)

- `catalog/skills/land-work/SKILL.md` line 227 ("Create tracker follow-up items for Minor issues ...") and `catalog/skills/beads-issue-flow/SKILL.md` lines 185-187 ("file a follow-up issue ...") do not say where follow-ups go.
- bd supports `--deps discovered-from:<id>` and `--parent`.

## Acceptance criteria

- beads-issue-flow and land-work say: file follow-ups as siblings under the same parent epic, or as top-level issues, with `discovered-from:<closing-id>`, never as children of the issue being closed.
- The close helper from close-reason-evidence refuses to close an issue that has open children unless `--force` is given with a reason.
- A test covers the refusal and the forced path.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Re-parenting shatter's existing orphans (shatter tracker work).

## Priority / Type / Labels

P3 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by close-reason-evidence (the close helper this extends). The doc change can land first.
