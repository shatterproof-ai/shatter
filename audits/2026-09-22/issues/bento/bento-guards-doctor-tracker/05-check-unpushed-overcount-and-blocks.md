---
slug: check-unpushed-overcount-and-blocks
kind: new
title: "check-unpushed Stop hook: count against all remotes (not @{u}), report without blocking on the primary branch in the primary checkout, and do not block a branch that a live land.py is landing"
priority: P2
type: bug
labels: [audit, hooks, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# check-unpushed Stop hook: count against all remotes (not @{u}), report without blocking on the primary branch in the primary checkout, and do not block a branch that a live land.py is landing

Complements bento-neng (closed), which covered only the identical re-nag. Related: bento-neng, bento-k23u (closed; subagents under a lead hold), bento-2p2p (closed 2026-09-23 as 3731a6f; decoupled the Beads exemptions in the same file), bento-x4bm (open; adds a durable landing record under `<git-common-dir>/bento/landing/<issue-id>/`), bento-i76i (open; blocks commit verbs on the primary branch). Source findings: bento-11, sessions-10 (shatter audit 2026-09-22).

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

## Current code facts (bento origin/main @ 0b8d488, after 3731a6f)

- `catalog/hooks/bento/claude/scripts/check-unpushed.py` lines 515-520: resolves `@{u}` and counts `git rev-list @{u}..HEAD --count`; line 540 lists `@{u}..HEAD`.
- Lines 591-625: the block messages ("Session end blocked: ..." and "Session end still blocked ...").
- Lines 232-257: the cooperative, session-scoped turn-hold marker (`_turn_hold_path`, `consume_turn_hold`), keyed on the payload `session_id`; line 695 onward: per-session throttle state (bento-neng).
- Git reflogs carry no agent session id, and `land.py` does not know the session id of the agent that started it. So "this session's commits" and "this session's land.py" cannot be attributed by session; the design below uses the branch and a live process instead.

## Design contract

- **Ownership is by branch, not session.** land.py writes one marker per branch it is landing: `<git-common-dir>/bento/landing-in-progress/<branch-slug>.json` with `{branch, worktree, pid, pid_start_time, host, started_at}`. `pid_start_time` is field 22 of `/proc/<pid>/stat` (or `ps -o lstart=` where /proc is absent). The marker is written atomically (temp file + `os.replace`) after `prepare` succeeds and removed in `finally`.
- **Liveness.** The hook treats a marker as live only if `host` matches, the pid exists, and its start time equals `pid_start_time`. A reused pid or dead process makes the marker stale; the hook ignores it for blocking, removes it, and mentions the stale marker once.
- **Scope of the suppression.** The hook suppresses the unpushed-commit block only when the Stop payload's worktree is on the marker's `branch` (or is the marker's `worktree`). Uncommitted changes still block as today. Concurrent landers of different branches have separate markers. The marker is created exclusively (`O_CREAT|O_EXCL` on the temp name, then a check that no live marker exists for the branch), so a second land.py for the same branch while the first is live fails fast with a clear message; a stale marker is replaced.
- **A bare background `git push`** started without land.py is not covered: the hook still blocks. The block message says so.
- If bento-x4bm's landing-record directory lands first, the marker may live beside it; the contract above still applies.

## Acceptance criteria

- The count uses `git rev-list HEAD --not --remotes`. Test: a fixture where a feature branch with 2 own commits is rebased onto an origin/main that gained 50 commits reports 2, not 52.
- In the primary checkout, on the primary branch, the hook prints the unpushed/uncommitted summary to stderr and exits 0 (never 2). Test included. The message notes that commits on the primary branch are blocked at commit time by the git guard (bento-i76i), so the Stop hook no longer duplicates that.
- Marker tests, each asserting the hook's exit code: live marker for the current branch (exit 0, no block); live marker for a different branch (still blocks); marker with a dead pid (blocks, marker removed); marker whose pid is alive but whose start time differs (blocks, marker removed); marker from another host (ignored); two concurrent landers of different branches (each only unblocks its own branch).
- land.py tests: the marker exists while a stubbed verifier sleeps, and is gone after both success and an injected failure; a second land.py for the same branch while the first is live fails with the documented message.
- The existing check-unpushed tests still pass, including the bento-2p2p exemption tests.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Out of scope

- Moving the check to SessionEnd (worth noting as a follow-up option).
- Attributing commits to agent sessions.

## Priority / Type / Labels

P2 / bug / audit, hooks, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None hard. Coordinate the marker location with bento-x4bm if it is in flight.
