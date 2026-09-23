---
slug: triage-policy-and-audit-epic-waves
kind: new
title: "Define P1, cap open P1s, and split and wave-order the stalled str-qwua7 audit epic"
priority: P2
type: task
labels: [agents, governance, audit, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Define P1, cap open P1s, and split and wave-order the stalled str-qwua7 audit epic

## Problem

Priority has lost its meaning, and audit follow-through stalls.

- About a quarter of open issues are P1, and the top of `bd ready` is all P1.
- Some higher-priority issues are blocked by lower-priority ones (priority
  inversions).
- The 2026-09-04 audit epic str-qwua7 has 12 of 62 direct children closed
  after 19 days, all of them small. No structural P1 child has a comment,
  branch or commit.
- Meanwhile a new feature epic (str-hjrnp) was created and finished on
  09-21/22.

AGENTS.md has no priority rubric, and nothing orders audit follow-through.

## Evidence

Re-counted 2026-09-23 from `bd list --all --json --limit 0` unless noted.

- Open issues by priority: P1 45, P2 112, P3 23, P4 1 (181 open;
  182 open or in_progress). `bd ready --json --limit 0` returns 157 issues.
  The audit counted 48 P1 on 2026-09-22.
- str-qwua7 has 62 direct children: 12 closed, 50 open (11 P1, 39 P2).
- Inversion: str-35vtk.10 (P1, open, "WS-H: Anti-regression closeout") is
  blocked by str-35vtk.29 (P2, open) and str-35vtk.31 (P2, open).
- The str-qwua7 children mix several kinds of work:
  - product bugs: .5 .10 .11 .13 .39
  - refactors: .6.x
  - agent and gate items: .2 .3 .22-.28 .51 .55
  - docs: .21 .44-.46
  - three nested epics

  The audit saw comment_count 0 on every open child.
- The `agents` label has 12 open issues, all P2 and none touched since filing
  (agent-repo-18).
- AGENTS.md has no priority rubric. `bd ready` sorts by priority and age.
- Findings: prior-14, prior-25, agent-repo-18. Source draft:
  `drafts/shatter-agent/11-triage-policy-and-audit-epic-waves.md`.

## Acceptance criteria

- [ ] AGENTS.md defines P0-P3. P1 means broken or misleading for users,
      blocks landing, or security; everything else is P2 or lower.
- [ ] Open P1s are re-triaged against the rubric. The result is at most 15
      open P1s, or a recorded maintainer override. Before and after counts are
      in the close reason.
- [ ] str-qwua7 is split into themed child epics (product correctness,
      refactor, agent/gates, docs), either wave-ordered with `bd swarm` or
      given explicit blocked-by edges. Obsolete children are closed by
      tracker-reconciliation-sweep.
- [ ] New drift-patrol checks, each with unit tests over canned bd data:
      - WARN when the open P1 count is over the cap.
      - FAIL on a blocked-by-lower-priority inversion.
      - WARN when an audit epic has more than half of its children untouched
        for 14 days.
- [ ] AGENTS.md WIP rule: new feature epics at P2 or lower wait while audit P1
      bugs are ready, unless the maintainer overrides.

## Suggested approach

Draft the rubric and the P1 re-triage list first, and get maintainer sign-off
on the demotions before running `bd update`. Apply the same waves to the
2026-09-22 epic when it is filed.

## Out of scope

- Implementing the child issues themselves.
- Closing obsolete issues (tracker-reconciliation-sweep).

## Priority / type / labels

P2, task. Labels: agents, governance, audit, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Related: tracker-reconciliation-sweep, str-qwua7, str-qwua7.62.
