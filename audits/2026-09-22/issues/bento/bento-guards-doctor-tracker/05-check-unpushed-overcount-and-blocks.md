---
slug: check-unpushed-overcount-and-blocks
kind: new
title: "check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running"
priority: P2
type: bug
labels: [audit, hooks, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running

Complements bento-neng (closed), which covered only the identical re-nag. Related: bento-neng, bento-k23u. Source findings: bento-11, sessions-10 (shatter audit 2026-09-22).

## Problem

The Stop hook `check-unpushed.py` fires at the end of every turn and blocks on "unpushed" commits. In shatter it:

1. overcounts after a rebase, because it counts `@{u}..HEAD` and the old upstream lacks the main commits pulled in by the rebase;
2. blocks on the shared primary `main`, where peer sessions leave commits;
3. blocks every turn while this session's `land.py` is still running in the background.

That removes the agent's legitimate way to wait (ending its turn) and feeds busy-wait `sleep 1; echo` loops. One shatter session made 609 such calls.

## Evidence (shatter transcripts since 2026-09-04)

- 77 blocks in 21 sessions. Session 9f13ca23 had 31 blocks out of 95 stop events while land.py ran in the background ("Still mid-landing ... that hook will clear once this branch merges").
- Rebase overcounts: "'str-6vl7p-redundant-canonicalize' has 71 unpushed commits" (10 times), and counts of 68, 69, 72 and 115.
- Primary: "Session end blocked: branch 'main' has 2 unpushed commits" (17 times), and "main has uncommitted changes and 1 unpushed commit" (13 times).

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/check-unpushed.py` lines 406-411: resolves `@{u}` and counts `git rev-list @{u}..HEAD --count`.
- Lines 476-510: the block messages ("Session end blocked: ... Note: Stop fires at the end of every turn ..." and "Session end still blocked ...").
- Line 581 onward: per-session throttle state (bento-neng).
- bento-k23u (closed) fixed blocking subagents under a lead hold.

## Acceptance criteria

- The count uses `git rev-list HEAD --not --remotes`. A rebased branch whose rebased-in commits are already on origin/main reports only its own commits. Test: a fixture with a rebase.
- In the primary checkout, on the primary branch, the hook reports without blocking unless the reflog shows this session created the commits. Test included.
- While a background `land.py` (or `git push`) started by this session is still running, detected via a marker/pid file that land.py writes, the hook does not block. Test included.
- The existing check-unpushed tests still pass.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Suggested approach

land.py writes `.land-work/in-progress.json` (pid, branch, session id, start time) in the git common dir and removes it in `finally`. The hook checks whether that pid is alive.

## Out of scope

- Moving the check to SessionEnd (worth noting as a follow-up option).

## Priority / Type / Labels

P2 / bug / audit, hooks, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
