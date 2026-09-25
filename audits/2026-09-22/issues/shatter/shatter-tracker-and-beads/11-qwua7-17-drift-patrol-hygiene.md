---
slug: qwua7-17-drift-patrol-hygiene
kind: reopen-note
title: "Note on str-qwua7.17: drift-patrol tracker-hygiene is red again (stale claims, 4 orphans)"
priority: P3
type: task
labels: [agents, drift, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.17
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.17: drift-patrol tracker-hygiene is red again (stale claims, 4 orphans)

**Target:** `str-qwua7.17` ("Release or finish the 13 stale in_progress claims
flagged by drift-patrol").

**Filer caution:** str-qwua7.17 has been **closed** since 2026-09-08
(re-checked 2026-09-23). The source finding's dedupe called it open, but it
is not. Post this as a comment only; **do not reopen it**. The action items
are carried by tracker-reconciliation-sweep (orphans), mpgg1-close, and
landed-not-closed-patrol-check (the recurrence check).

## Comment text

**Audit 2026-09-22 note: tracker-hygiene has regressed since this sweep**
(finding gates-09)

Drift-patrol tracker-hygiene failed on main on 2026-09-22
(`audits/2026-09-22/gates/drift-patrol.log:19,24-39`):
"2 stale in_progress issue(s), 4 orphaned child issue(s)".

- Stale claims: str-8q1b4 (22 d) has since been closed (2026-09-22).
  str-mpgg1 (21 d) is still in_progress although its merge 84941b37 is on
  main; it is being closed by <mpgg1-close>.
- Orphaned open children of closed parents: str-qwua7.56.1, str-qwua7.9.1,
  str-hy9b.J3, str-hy9b.1. <tracker-reconciliation-sweep> re-parents or
  closes them.
- Still PENDING: docs-stories (str-u394l.3) and cli-surface-drift (str-wurp).
  Consider running drift-patrol with `--strict-pending` once both land.

This is the second time stale claims have accumulated after a one-time sweep.
A one-time sweep does not fix it. The recurrence fix is a new drift-patrol
check that FAILs when an issue is still open or in_progress although a
landing merge for it is on origin/main: <landed-not-closed-patrol-check>.
The existing 14-day stale-claim FAIL stays as it is.
