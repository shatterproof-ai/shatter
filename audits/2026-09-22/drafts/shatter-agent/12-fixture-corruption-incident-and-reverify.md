# Record the 2026-09-07 fixture-corruption incident, review recovery branches, and re-verify str-qwua7.14 on origin/main

- Priority: P2
- Type: task
- Labels: agents,git,governance
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-jttrf, str-y0rcz, str-qwua7.14)
- Source findings: agent-repo-16
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
On 2026-09-07 a fixture leak created a stray commit `e50fc399` ("init", author
Test) that deleted 12,090 lines (including `shatter-vs/` and restoring
`.claude/agents`). Recovery was ad hoc and untracked; two local recovery
branches remain unreviewed. str-qwua7.14 (P1) was closed as "Not reproducible
against current main (e50fc399)" — a commit that is not on main. The same
stray commit is in four unmerged remote `str-qwua7.*` branches.

## Current Code Facts
- `git merge-base --is-ancestor e50fc399 origin/main` -> false.
- Local branches: `recovery/shatter-main-20260912-11_4ty6m` (e50fc399),
  `recovery/shatter-index-20260912-11_4ty6m` (a6f4cbc0).
- Remote branches containing e50fc399 (102 commits ahead of main):
  `origin/str-qwua7.4-testplan-http-body-fix`,
  `origin/str-qwua7.7-protocol-registry-validate`,
  `origin/str-qwua7.16-restore-bd-dolt`,
  `origin/str-qwua7.17-stale-claims-cleanup`.
- str-qwua7.14 close reason cites e50fc399 as "current main".

## Acceptance Criteria
- This issue body (or a linked doc) records the incident timeline and cause
  (link str-jttrf/str-y0rcz).
- Recovery branches reviewed: any unique, wanted content filed/landed;
  branches deleted after operator confirmation.
- The four contaminated remote branches deleted after confirming each issue's
  work landed elsewhere (list the landing SHA per branch).
- str-qwua7.14 reopened and re-verified against an origin/main SHA; close or
  keep open based on that result, citing the SHA.
- AGENTS.md rule: close reasons and diagnoses must cite a SHA that is an
  ancestor of origin/main (`git merge-base --is-ancestor <sha> origin/main`).

## Out of Scope
Enforcing the SHA rule in bento land-work (bento tracker).
