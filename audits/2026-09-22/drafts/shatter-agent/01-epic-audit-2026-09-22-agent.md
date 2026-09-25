# Epic: Audit 2026-09-22 findings (agent system / process)

- Priority: P1
- Type: epic
- Labels: audit,agents
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new
- Source findings: all AGENT-level shatter findings
- Parent: none (this is the epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Purpose
Container for the agent-system and process issues from the 2026-09-22 full
project audit (report and per-area evidence under `audits/2026-09-22/` on branch
`audit-2026-09-22`; see `audits/2026-09-22/areas/*.md`). Product-level (L1-L6) findings
are filed under a sibling epic; this epic covers the instructions, gates,
hooks, memory, tracker hygiene and workflows that let defects appear and
linger.

## Themes
- Watchdogs that never ran (scheduled Drift Patrol red for 7 weeks, release
  workflow 0/200, no workflow-health signal).
- Tracker and snapshot drift (`bd sync` gone in bd 1.1.0, `.beads/issues.jsonl`
  frozen since 2026-09-07, landed-not-closed and obsolete issues).
- Poisoned repo state (fixture identity `Test <test@example.com>` in the
  primary `.git/config`).
- Stale or harmful agent memory and guidance (bypass advice, stale skills,
  rtk-managed block swallowing project rules).
- Gates that pass without proving they ran (dead gauntlet regex, unwired test
  modules, incomplete Task `sources:`, ungated linters).
- Parallel-path parity enforced only by prose.
- Audit follow-through: the 2026-09-04 report was never published and its
  epic (str-qwua7) has stalled.

## Acceptance
- All children closed with close reasons that cite a SHA on origin/main and
  the gate/evidence used.
- Before closing, re-run `python3 scripts/drift-patrol.py` and confirm the
  scheduled Drift Patrol workflow has at least one green scheduled run.

## Notes
Children that only append to existing issues are recorded in
`audits/2026-09-22/drafts/shatter-agent/INDEX.md`; they are not children of this epic.
