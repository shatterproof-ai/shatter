# check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running

- Filing action: new issue (complements bento-neng (now closed), which covered the identical re-nag only)
- Priority: P2
- Type: bug
- Labels: audit, hooks, land-work
- Parent: epic
- Links: related bento-neng, related bento-k23u
- Source findings: bento-11, sessions-10

---BODY---
## Problem

The Stop hook `check-unpushed.py` fires at the end of every turn and blocks on "unpushed" commits. In shatter it:

1. Overcounts after a rebase. It counts `@{u}..HEAD`, and the old upstream lacks the main commits pulled in by the rebase.
2. Blocks on the shared primary `main`, where peer sessions leave commits.
3. Blocks every turn while this session's `land.py` is still running in the background.

That removes the agent's legitimate way to wait (ending its turn) and feeds busy-wait `sleep 1; echo` loops. One shatter session had 609 such calls.

## Evidence (shatter transcripts since 09-04)

- 77 blocks in 21 sessions. Session 9f13ca23 had 31 blocks out of 95 stop events while land.py ran in the background ("Still mid-landing ... that hook will clear once this branch merges").
- Rebase overcounts: "'str-6vl7p-redundant-canonicalize' has 71 unpushed commits" (10 times), and counts of 68, 69, 72 and 115.
- Primary: "Session end blocked: branch 'main' has 2 unpushed commits" (17 times). Also "main has uncommitted changes and 1 unpushed commit" (13 times).

## Current code facts (bento @ 1c0c1e6)

- `catalog/hooks/bento/claude/scripts/check-unpushed.py` about lines 392-400: counts `git rev-list @{u}..HEAD`.
- About lines 445-462: the block message is "Session end blocked: ... Note: Stop fires at the end of every turn, not only when the session truly ends".
- About line 39: some advisory throttling already exists.
- bento-neng (closed) addressed throttling the identical re-nag every turn. bento-k23u (closed) fixed blocking subagents under a lead hold.

## Acceptance criteria

- The count uses `git rev-list HEAD --not --remotes`. A rebased branch whose rebased commits are already on origin/main reports only its own commits. Test: a fixture with a rebase.
- In the primary checkout, on the primary branch, the hook reports without blocking unless the reflog shows this session created the commits. Test included.
- When a background land.py (or `git push`) started by this session is still running, detected via a marker/pid file that land.py writes, the hook does not block. Test included.
- No regression in the existing check-unpushed tests.

## Suggested approach

land.py writes `.land-work/in-progress.json` (pid, branch, start time) to the git common dir and removes it in `finally`. The hook checks whether that pid is alive.

## Out of scope

- Moving the check to SessionEnd. Worth noting as a follow-up option, but not required here.
