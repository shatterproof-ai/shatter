---
slug: fixture-corruption-incident-reverify
kind: new
title: "Record the 2026-09-07 fixture-corruption incident and review its two recovery branches and four contaminated remote branches"
priority: P2
type: task
labels: [agents, git, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the 2026-09-07 fixture-corruption incident and review its two recovery branches and four contaminated remote branches

## Problem

On 2026-09-07 the GIT_DIR fixture leak (fixed later by str-jttrf / str-y0rcz)
created a stray commit `e50fc399` ("init", author `Test <test@example.com>`).
The commit deletes 12,090 lines: among other things it removes `shatter-vs/`,
deletes all three `.claude/agents/*/AGENT.md` files and rewrites
`.beads/issues.jsonl`. The recovery was done ad hoc and never tracked:

- Two local `recovery/*` branches from 2026-09-12 have never been reviewed.
- Four remote `str-qwua7.*` feature branches contain the stray commit.
  str-qwua7.4's close reason says the duplicates were "both deleted as
  superseded", but they still exist on origin.
- str-qwua7.14 (P1 bug) was closed as "Not reproducible against current main
  (e50fc399)". `e50fc399` is not on main, so that diagnosis may have run
  against a corrupted tree. Its re-verification is split out as
  `qwua7-14-reverify-on-main`; the close-reason SHA-labelling rule is added
  to str-qwua7.51 (note `qwua7-51-identity-root-cause`).

This issue is the incident record plus a review of the six branches. It is
complete when every branch has a recorded disposition. **Deleting a branch is
optional and needs explicit operator approval; a declined deletion is a valid
outcome, not a blocker.**

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`
(origin/main = 70465921; remote-tracking refs as of the last fetch):

- `git merge-base --is-ancestor e50fc399 origin/main` -> exit 1 (NOT on main).
- `git show -s --format='%h %ad %an %s' --date=short e50fc399` ->
  `e50fc399 2026-09-07 Test init`. `git show --shortstat --format= e50fc399` ->
  `82 files changed, 620 insertions(+), 12090 deletions(-)`.
- `git show --name-status --format= e50fc399 -- .claude/agents` ->
  `D .claude/agents/go-dev/AGENT.md`, `D .claude/agents/rust-dev/AGENT.md`,
  `D .claude/agents/ts-dev/AGENT.md` (deletions, not restorations).
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
   leaked repo-local identity handled in `mailmap-and-fixture-config-snapshot`,
   and str-qwua7.14 handled in `qwua7-14-reverify-on-main`).
2. Each of the six branches gets a disposition table row, recorded in this
   issue, with: the command output of `git diff origin/main...<branch> --stat`;
   a list of content not already on origin/main (or "none"); for the four
   `str-qwua7.*` branches, the origin/main SHA where that issue's real work
   landed (verified with `git merge-base --is-ancestor <sha> origin/main`) or
   "did not land"; and a disposition: `salvage` (wanted content filed as a new
   issue or landed, with its id/SHA), `delete` or `retain`.
3. For each `delete` disposition, the operator's explicit approval is quoted
   in the issue before deletion. After deletion,
   `git ls-remote origin 'refs/heads/str-qwua7*'` (remote) or
   `git branch --list 'recovery/*'` (local) no longer lists that branch.
4. For each `retain` disposition (including a declined deletion), the issue
   records the reason and marks the branch "known-contaminated: contains
   e50fc399, do not merge". The issue can close with retained branches.
5. The close reason links the incident record and the disposition table.

## Suggested approach

- Do the review in a scratch linked worktree, never in the primary checkout.
- A content check beyond `--stat`: `git log --oneline origin/main..<branch>`
  and `git diff origin/main...<branch> -- ':!.beads'` for anything that is not
  part of the stray deletion.

## Out of scope

- Re-verifying str-qwua7.14 (`qwua7-14-reverify-on-main`).
- The close-reason SHA rule (str-qwua7.51, via `qwua7-51-identity-root-cause`).
- Enforcing an ancestor-SHA rule in bento land-work (bento tracker).
- The general merged-branch sweep (`scripts/cleanup-merged-remote-branches.sh`;
  see the str-qwua7.23 note).
- Fixture-leak prevention (`mailmap-and-fixture-config-snapshot`, str-qwua7.1).

## Dependencies

None.
