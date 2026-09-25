---
slug: landed-not-closed-patrol-check
kind: new
title: "drift-patrol: FAIL on open or in_progress issues whose work has affirmatively landed on origin/main"
priority: P2
type: task
labels: [agents, beads, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [tracker-reconciliation-sweep, beads-jsonl-consumers-drop-bd-sync]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# drift-patrol: FAIL on open or in_progress issues whose work has affirmatively landed on origin/main

## Problem

Landed work regularly stays open or `in_progress` (str-mpgg1: merged
2026-09-02, still in_progress 21 days later; str-qwua7.17 swept 13 stale
claims on 2026-09-08 and they came back). drift-patrol's existing
tracker-hygiene check (`scripts/drift-patrol.py:523 check_tracker_hygiene`)
only FAILs on `in_progress` issues untouched for more than
`DEFAULT_STALE_DAYS = 14` (`:67`) and on orphaned children. Nothing detects
"this issue's work is on main, but the issue is not closed".

Branch ancestry alone is not evidence of landing: a branch just created at
`origin/main` is already an ancestor of it before its first commit, and the
normal land-work cleanup deletes the feature branch after merging. So the
check must look for **affirmative landing evidence on main**, not for
branches.

## Evidence (re-verified 2026-09-23)

- drift-patrol statuses are PASS, FAIL, PENDING and SKIP
  (`scripts/drift-patrol.py:23-35`, `:53-56`); there is no WARN. This issue
  adds none.
- Landing commits on main name the issue id in a stable form, for example
  `Merge branch 'str-6nul9-bench-timeout-override'` (70465921) and
  `str-6nul9: exclude bench_frontier_ranking ...` (20692b08).
- Findings: agent-repo-05, sessions-09, gates-09.

## Acceptance criteria

- [ ] A new check (next to `check_tracker_hygiene`) reports **FAIL** for an
      issue with status `open` or `in_progress` when origin/main's history
      contains a landing commit for it: a merge commit whose subject is
      `Merge branch '<id>-...'` or `Merge branch '<id>'`, whose second parent
      has at least one commit not reachable from the merge's first parent.
      Branch existence is not required.
- [ ] **Reopened issues:** if the issue's reopen or last status change is
      later than its newest landing commit, it is not flagged (a reopened
      issue legitimately has landed commits). The rule is documented in
      `docs/DRIFT-PATROL.md`.
- [ ] The check reads live bd; without bd it returns SKIP with a reason
      (consistent with beads-jsonl-consumers-drop-bd-sync; never the JSONL).
- [ ] The existing 14-day stale-claim FAIL is unchanged; no new status
      (WARN) is introduced.
- [ ] Unit tests in `scripts/test_drift_patrol.py` use a scratch git repo
      fixture and canned bd output for: (a) branch created at origin/main
      with no commits → not flagged; (b) branch with commits but unmerged,
      and uncommitted worktree changes → not flagged; (c) merged and branch
      deleted → FAIL; (d) merged, then issue reopened later → not flagged;
      (e) merged and issue closed → PASS. Each test is shown red before the
      check exists or with the check's condition inverted (paste the run).
- [ ] An integration run of `python3 scripts/drift-patrol.py --only
      <new-check-id>` against live bd on main is PASS (after
      tracker-reconciliation-sweep), and its exit code is 0; a forced run
      against a fixture with case (c) exits 1. Both outputs are in the close
      reason.
- [ ] Lands through launch-work/land-work with `task affected` green and its
      `Gates selected` line in the close reason.

## Out of scope

- The one-time closes (tracker-reconciliation-sweep).
- P1-cap, inversion and audit-epic checks (triage-policy-and-audit-epic-waves).

## Priority / type / labels

P2, task. Labels: agents, beads, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: tracker-reconciliation-sweep (so the check lands green),
  beads-jsonl-consumers-drop-bd-sync (live-bd reading and SKIP behavior in
  drift-patrol).
- Related: qwua7-17-drift-patrol-hygiene, mpgg1-close.
