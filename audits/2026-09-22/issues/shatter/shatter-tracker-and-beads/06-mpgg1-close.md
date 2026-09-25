---
slug: mpgg1-close
kind: note-to-existing
title: "Close str-mpgg1: revert landed in 84941b37; hook-env edits stay reverted, D4 replaces the timeout debate"
priority: P2
type: task
labels: [agents, beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-mpgg1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-mpgg1: revert landed in 84941b37; hook-env edits stay reverted, D4 replaces the timeout debate

**Target:** `str-mpgg1` (in_progress, P1, "Revert unauthorized beads-hook
edits from str-35vtk.4"; last updated 2026-09-01)

**Action:** post the comment below, then run
`bd close str-mpgg1 --reason "Landed: 84941b37 (merge of str-mpgg1-revert-hook-edits) is an ancestor of origin/main; hook-env edits stay reverted per audit 2026-09-22 D4."`
If tracker-reconciliation-sweep is filed in the same run, that sweep can do
this close. Do not do it twice.

## Comment text

**Audit 2026-09-22 note: closing as landed**

`git merge-base --is-ancestor 84941b37 origin/main` succeeds (re-checked
2026-09-23), so the revert has been on main since 2026-09-02. The issue has
stayed `in_progress` for 21 days, and drift-patrol tracker-hygiene flags it
as a stale claim (`audits/2026-09-22/gates/drift-patrol.log:32-34`).

Maintainer decision D4 (2026-09-23): the hook-env edits stay reverted. No
`BEADS_HOOK_TIMEOUT` env line or managed env block will be added to the hooks.
The timeout debate between this issue and str-qwua7.28 is replaced by fixing
the root cause, which is the post-checkout JSONL import. See
<beads-retire-jsonl-import-dolt-remote>; str-qwua7.28 is being closed as
superseded.

Evidence: agent-repo-07, prior-10 (`audits/2026-09-22/findings.json`).
