# NOTE TO APPEND to bento-eth: swarm lead should land from a lead-owned scratch worktree, not the teammate's

- Filing action: note to append to existing open issue bento-eth (make lead-lands the default). Do not file a new issue.
- Priority: (inherits bento-eth)
- Source findings: bento-12

---BODY---
## Addendum from shatter audit 2026-09-22

Serial-Mode Landing step 3 in `catalog/skills/swarm/SKILL.md` (about lines 337-342) says "Invoke bento:land-work from within the teammate's worktree". land.py requires the branch to be rebased there, so the lead runs checkout and rebase inside the teammate's worktree.

A shatter lesson from 2026-09-08 (memory feedback_lead_landing_prep_separate_worktree) records that this "silently moved the worktree's HEAD out from under" a teammate who was still making review fixes.

Proposed additions to bento-eth's acceptance criteria:
- The lead lands from a lead-owned scratch worktree created at the teammate branch tip, or only after confirming the teammate session has exited.
- Add `land.py --branch <name>` so it can land a named branch from any worktree without checking it out in the teammate's tree.
- Update swarm SKILL.md step 3 accordingly.
