---
slug: triage-drift-patrol-checks
kind: new
title: "drift-patrol: FAIL on open-P1 count over the cap, on blocked-by-lower-priority inversions, and on stalled audit epics"
priority: P2
type: task
labels: [agents, governance, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [triage-policy-and-audit-epic-waves, beads-jsonl-consumers-drop-bd-sync]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# drift-patrol: FAIL on open-P1 count over the cap, on blocked-by-lower-priority inversions, and on stalled audit epics

## Problem

triage-policy-and-audit-epic-waves sets a priority rubric, a P1 cap, fixes
the known inversions and splits the str-qwua7 audit epic. Without a check,
the same drift comes back: this is already the second time stale tracker
state accumulated after a one-time sweep (str-qwua7.17, 2026-09-08). These
checks keep the policy enforced.

## Evidence (re-verified 2026-09-23)

- drift-patrol supports PASS, FAIL, PENDING and SKIP only
  (`scripts/drift-patrol.py:23-35`, `:53-56`). There is no WARN, and this
  issue does not add one: each new check is a normal FAIL-capable check that
  lands green because triage-policy-and-audit-epic-waves fixed the current
  violations first.
- Open issues on 2026-09-23: P1 45, P2 112, P3 23, P4 1. Known inversion:
  str-35vtk.10 (P1) blocked by str-35vtk.29 and .31 (P2).
- str-qwua7: 62 direct children, 12 closed after 19 days; the audit saw
  comment_count 0 on every open child.
- Findings: prior-14, prior-25, agent-repo-18.

## Acceptance criteria

- [ ] Three checks in `scripts/drift-patrol.py`, each reading live bd and
      returning SKIP with a reason when bd is absent (never reading the
      JSONL):
      - **p1-cap:** FAIL when the open P1 count exceeds the cap recorded in
        AGENTS.md by triage-policy-and-audit-epic-waves (the cap value is read
        from one place, not duplicated in the script and the doc).
      - **priority-inversion:** FAIL when an open issue is blocked by an open
        issue of strictly lower priority (higher P number). The failure lists
        each pair.
      - **audit-epic-stall:** FAIL when an open epic labelled `audit` has more
        than half of its open children with no update, comment or status
        change for 14 days. The threshold is a named constant documented in
        `docs/DRIFT-PATROL.md`.
- [ ] `scripts/test_drift_patrol.py` covers each check with canned bd JSON:
      a passing case, a failing case, and the bd-absent SKIP case. Each
      failing-case test is shown red with the check disabled or inverted.
- [ ] An integration run of `python3 scripts/drift-patrol.py --only
      p1-cap,priority-inversion,audit-epic-stall` on main against live bd is
      PASS with exit 0, and a run with a fixture over the cap exits 1. Both
      outputs are in the close reason.
- [ ] `docs/DRIFT-PATROL.md` lists the three checks, their thresholds and
      remediation text.
- [ ] Lands through launch-work/land-work with `task affected` green and its
      `Gates selected` line in the close reason.

## Out of scope

- Setting the rubric, cap, re-triage and epic split
  (triage-policy-and-audit-epic-waves).
- The landed-not-closed check (landed-not-closed-patrol-check).

## Priority / type / labels

P2, task. Labels: agents, governance, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: triage-policy-and-audit-epic-waves (so the checks land green),
  beads-jsonl-consumers-drop-bd-sync (live-bd reading and SKIP behavior).
- Related: landed-not-closed-patrol-check.
