---
slug: fixture-corruption-incident-reverify
kind: new
title: "Record the 2026-09-07 fixture-corruption incident, review its recovery and contaminated branches, and re-verify str-qwua7.14 against an origin/main SHA"
priority: P2
type: task
labels: [agents, git, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the 2026-09-07 fixture-corruption incident, review its recovery and contaminated branches, and re-verify str-qwua7.14 against an origin/main SHA

## Problem

On 2026-09-07 the GIT_DIR fixture leak (fixed later by str-jttrf / str-y0rcz)
created a stray commit `e50fc399` ("init", author `Test <test@example.com>`).
The commit deletes 12,090 lines: it removes `shatter-vs/`, restores
`.claude/agents` and rewrites `.beads/issues.jsonl`. The recovery was done
ad hoc and never tracked:

- Two local `recovery/*` branches from 2026-09-12 have never been reviewed.
- Four remote `str-qwua7.*` feature branches contain the stray commit.
  str-qwua7.4's close reason says the duplicates were "both deleted as
  superseded", but they still exist on origin.
- **str-qwua7.14 (P1 bug) was closed as "Not reproducible against current main
  (e50fc399)".** `e50fc399` is not on main, so the diagnosis ran against a
  corrupted tree and the closure is unsound.

Nothing requires a diagnosis or close reason to cite a commit that is actually
on origin/main.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`
(origin/main = 70465921; remote-tracking refs as of the last fetch):

- `git merge-base --is-ancestor e50fc399 origin/main` -> exit 1 (NOT on main).
- `git show -s --format='%h %ad %an %s' --date=short e50fc399` ->
  `e50fc399 2026-09-07 Test init`. `git show --shortstat --format= e50fc399` ->
  `82 files changed, 620 insertions(+), 12090 deletions(-)`.
- `git branch --list 'recovery/*' -v` ->
  `recovery/shatter-index-20260912-11_4ty6m a6f4cbc0 recovery: preserve captured Shatter index`,
  `recovery/shatter-main-20260912-11_4ty6m e50fc399 init`.
- `git branch -r --contains e50fc399` ->
  `origin/str-qwua7.16-restore-bd-dolt`, `origin/str-qwua7.17-stale-claims-cleanup`,
  `origin/str-qwua7.4-testplan-http-body-fix`, `origin/str-qwua7.7-protocol-registry-validate`.
- `bd show str-qwua7.14` -> CLOSED, close reason begins
  "Not reproducible against current main (e50fc399)".
- `bd search recovery` finds no tracking issue.
- Audit sources: agent-repo-16, prior-06 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/agent-repo.md`, on branch `audit-2026-09-22`).

## Acceptance criteria

1. This issue (a comment or a linked `docs/` note) records the incident: a
   timeline (2026-09-07 stray commit, 2026-09-12 recovery branches, str-jttrf /
   str-y0rcz fixes), the cause (fixture `git` calls under a leaked `GIT_DIR`),
   and the blast radius (the 4 remote branches, the 2 recovery branches, the
   leaked repo-local identity handled in `mailmap-and-fixture-config-snapshot`).
2. Each recovery branch is reviewed with
   `git diff origin/main...<branch> --stat` plus a content check. The review
   lists any content not already on origin/main; wanted content is filed or
   landed. Branches are deleted **only after explicit operator confirmation**,
   recorded in the issue.
3. For each of the four contaminated remote branches, the issue records the
   origin/main SHA where that issue's real work landed (or states it did not).
   Remote deletion (`git push origin --delete <branch>`) happens **only after
   explicit operator confirmation**. Afterwards,
   `git ls-remote origin 'refs/heads/str-qwua7*'` no longer lists them.
4. str-qwua7.14 is reopened and re-diagnosed on a build from an origin/main
   SHA (`git merge-base --is-ancestor <sha> origin/main` exits 0). It is then
   closed or kept open based on that result, with a reason that cites the SHA
   and the command output.
5. AGENTS.md gains one rule: diagnoses and close reasons that name a commit
   must name one that is an ancestor of origin/main. Check it with
   `git merge-base --is-ancestor <sha> origin/main` before citing it.

## Suggested approach

- Do the review in a scratch linked worktree, never in the primary checkout.
- For str-qwua7.14, rebuild the CLI and the Rust frontend at the chosen SHA
  first (a stale binary produced a false audit finding before; see prior-09),
  then re-run the walkthrough Rust step it names.

## Out of scope

- Enforcing the ancestor-SHA rule in bento land-work (bento tracker).
- The general merged-branch sweep (`scripts/cleanup-merged-remote-branches.sh`;
  see the str-qwua7.23 note).
- Fixture-leak prevention (`mailmap-and-fixture-config-snapshot`, str-qwua7.1).

## Dependencies

None.
