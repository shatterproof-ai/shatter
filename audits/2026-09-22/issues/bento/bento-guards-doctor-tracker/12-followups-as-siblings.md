---
slug: followups-as-siblings
kind: new
title: "Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; the non-landing close helper refuses parents with open children"
priority: P3
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [close-reason-evidence]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; the non-landing close helper refuses parents with open children

Source finding: prior-11 (shatter audit 2026-09-22). Related: bento-x4bm (land.py closes issues after landing; a companion comment below asks it to apply the same open-children check), bento-wzbt (closure contract).

## Problem

Code-review follow-ups are filed as children of the issue that is about to be closed, so they become permanent orphans. Shatter drift-patrol lists four:

- str-qwua7.56.1 (parent str-qwua7.56 closed)
- str-qwua7.9.1 (parent str-qwua7.9 closed)
- str-hy9b.J3 and str-hy9b.1 (epic str-hy9b closed 2026-04-27; open for 5 months)

Shatter's epic-lifecycle rule issue, str-5b9f, has been open since June.

## Current code facts (bento origin/main @ 0b8d488)

- `catalog/skills/land-work/SKILL.md` line 227 ("Create tracker follow-up items for Minor issues ...") and `catalog/skills/beads-issue-flow/SKILL.md` lines 185-187 ("file a follow-up issue ...") do not say where follow-ups go.
- bd supports `--deps discovered-from:<id>` and `--parent`.

## Where the check lives

Closes happen on two supported paths, and each gets the check:

- after a landing: `land.py` (bento-x4bm). Requested through the companion comment below; not implemented by this issue.
- every other close: `close.py` from close-reason-evidence. Implemented here.

A direct `bd close` bypasses both; the `close.py --audit` report from close-reason-evidence is the detective control. This issue does not claim enforcement beyond that.

## Acceptance criteria

- beads-issue-flow and land-work say: file follow-ups as siblings under the same parent epic, or as top-level issues, with `discovered-from:<closing-id>`, never as children of the issue being closed. A grep test asserts the sentence appears in both skills.
- `close.py` (close-reason-evidence) refuses to close an issue that has open children unless `--force --justification "<text>"` is given, and lists the open children in the refusal.
- Tests with a stubbed `bd`: refusal with one open child, success with only closed children, and the forced path.
- `close.py --audit` also lists closed issues that still have open children (the drift-patrol case above).
- Proof at close: the close note names the tests with failing-then-passing runs, and includes `close.py --audit` output against shatter showing the four orphans above (or whichever remain open).

## Out of scope

- Re-parenting shatter's existing orphans (shatter tracker work).
- Implementing the check inside land.py (bento-x4bm; see companion comment).

## Priority / Type / Labels

P3 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by close-reason-evidence (the close helper this extends). The doc change can land first.

## Comment for `bento-x4bm`

> Audit 2026-09-22 (shatter; finding prior-11): review follow-ups filed as children of the issue being closed become permanent orphans (shatter has four: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1). <id of followups-as-siblings> adds an open-children refusal to the non-landing close helper. Please give land.py's tracker step the same check: at `issue_preflight`, fail if the issue has open children (list them), so a landing never closes a parent over open children. Suggested test: a stubbed `bd` returning one open child makes preflight fail before `prepare`.
