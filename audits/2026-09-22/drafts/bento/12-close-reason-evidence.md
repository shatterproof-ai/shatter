# Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, beads-issue-flow, land-work
- Parent: epic
- Links: related bento-1qry, related bento-1bl, related bento-v57, related bento-m4en
- Source findings: prior-13

---BODY---
## Problem

Closures outside the land.py path often carry no verifiable evidence. In one case a "not reproducible" closure was checked against a commit that is not on main.

## Evidence (shatter)

- 17 of 49 closures since 2026-09-04 have the reason "Closed" or an empty reason. That includes str-qwua7.8, .9, .15, .27, .32, .41 and .56, and str-2tyfk (empty).
- str-qwua7.14's reason is "Not reproducible against current main (e50fc399)". `git merge-base --is-ancestor e50fc399 origin/main` is false: e50fc399 is a stray fixture "init" commit.
- For contrast, land.py-era reasons (str-qwua7.4, str-vr7vq, str-gjsb2) carry the SHA, the gate and the review result.

## Current code facts

- bento-1bl and bento-v57 (closed) added an ancestry rule as guidance in beads-issue-flow. Nothing enforces it.
- bento-1qry (in_progress) requires gate evidence in the land.py close note only.
- `bd close` accepts any reason, including an empty one.

## Acceptance criteria

- beads-issue-flow's Closure Checklist requires one of these reason forms:
  - `<sha> landed on <primary> (gate: ...)`
  - `not reproducible at <sha> (<command>)`
  - `duplicate of <id>`
  - `wontfix: <reason>`
- A helper script (for example `beads-issue-flow/scripts/close.py <id> --reason ...`) rejects reasons shorter than 20 chars or equal to "Closed". For any 7-40 hex SHA in the reason, it runs `git merge-base --is-ancestor <sha> <remote>/<primary>` and refuses on failure unless `--force` is given with a justification.
- land.py's close step uses the helper.
- Tests cover every rejection case.

## Out of scope

- Retroactively fixing old close reasons.
