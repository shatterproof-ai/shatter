---
slug: one-session-per-branch-guidance
kind: new
title: "land-work and swarm: state that one session owns a branch at a time and that handoff is explicit"
priority: P3
type: task
labels: [audit, land-work, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work and swarm: one session owns a branch at a time; handoff is explicit

Split from the audit draft `merge-message-and-stale-branch-nudge`, which bundled this with unrelated deliverables.

## Problem

Neither land-work nor swarm says which session owns a branch. In shatter two concurrent sessions worked the same branch and made opposite decisions on it, which ended in a revert (audit finding agent-repo-17). Swarm's lead-lands default (bento-qiw operator decision) makes the handoff between teammate and lead routine, so the ownership rule needs to be written down where agents read it.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488:

- `grep -n -i "owns\|one session" catalog/skills/land-work/SKILL.md catalog/skills/swarm/SKILL.md` finds only ownership of landing and cleanup (land-work `:775` "The landing agent owns direct post-merge cleanup"; swarm `:310` "The lead owns the single serialized landing path"). Nothing says who may commit to or rebase a branch while it is in flight.
- `catalog/skills/swarm/SKILL.md:340-341` has the lead land "from within the teammate's worktree" (see `eth-swarm-lead-lands-from-teammate`).

## Acceptance criteria

- [ ] land-work SKILL.md and swarm SKILL.md each contain one short rule: "One session owns a branch at a time. Before another session touches it, the owner hands off explicitly: a message to the receiving session and a tracker note naming the new owner." Swarm's rule names the teammate-to-lead handoff at completion as the standard case.
- [ ] The swarm teammate prompt template includes the teammate side of the rule ("after you report completion, do not push further commits to the branch unless the lead hands it back").
- [ ] A skill lint check (in the existing skill test suite) asserts the rule text is present in both files, so a later trim cannot drop it silently.
- [ ] Proof at close: the SKILL.md diffs and the passing lint output in the close reason.

## Out of scope

- Mechanical enforcement (branch locks, claim checks). `claim-branch-reconciliation` (bucket bento-guards-doctor-tracker) covers claim/branch reconciliation.
- Where the lead lands from (`eth-swarm-lead-lands-from-teammate`, `land-py-branch-flag`).

## Dependencies

- Blocked by: none.
- Related: bento-qiw, bento-eth, `land-work-skill-restructure` (must keep the rule when trimming SKILL.md), `claim-branch-reconciliation`.

Priority: P3 · Type: task · Labels: audit, land-work, swarm, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: agent-repo-17
