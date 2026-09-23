---
slug: close-reason-evidence
kind: new
title: 'Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch'
priority: P2
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch

Related: bento-1qry, bento-1bl, bento-v57, bento-m4en. Source finding: prior-13 (shatter audit 2026-09-22).

## Problem

Closures outside the land.py path often carry no verifiable evidence. In one case a "not reproducible" closure was checked against a commit that is not on main.

## Evidence (shatter)

- 17 of 49 closures since 2026-09-04 have the reason "Closed" or an empty reason, including str-qwua7.8, .9, .15, .27, .32, .41, .56 and str-2tyfk (empty).
- str-qwua7.14's reason is "Not reproducible against current main (e50fc399)". `git merge-base --is-ancestor e50fc399 origin/main` is false: e50fc399 is a stray fixture "init" commit.
- By contrast, land.py-era reasons (str-qwua7.4, str-vr7vq, str-gjsb2) carry the SHA, the gate and the review result.

## Current code facts (bento origin/main @ b1bb787)

- `catalog/skills/beads-issue-flow/SKILL.md`: the ancestry rule (`git merge-base --is-ancestor <merge-sha> <integration-branch>`, line 138) and the Closure Checklist (line 174) are guidance only (bento-1bl, bento-v57, closed). Nothing enforces them.
- bento-1qry (in_progress) requires gate evidence in the land.py close note only.
- `bd close` accepts any reason, including an empty one.

## Acceptance criteria

- beads-issue-flow's Closure Checklist requires one of these reason forms:
  - `<sha> landed on <primary> (gate: ...)`
  - `not reproducible at <sha> (<command>)`
  - `duplicate of <id>`
  - `wontfix: <reason>`
- A helper script (for example `catalog/skills/beads-issue-flow/scripts/close.py <id> --reason ...`) rejects reasons shorter than 20 characters or equal to "Closed". For any 7-40 hex SHA in the reason, it runs `git merge-base --is-ancestor <sha> <remote>/<primary>` and refuses on failure unless `--force` is given with a justification.
- land.py's close step uses the helper.
- Tests cover every rejection case and the accepted forms.
- Proof at close: the close note names the tests with failing-then-passing runs, and this issue's own close reason is produced through the new helper.

## Out of scope

- Retroactively fixing old close reasons.

## Priority / Type / Labels

P2 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None. Blocks followups-as-siblings (which extends the helper).
