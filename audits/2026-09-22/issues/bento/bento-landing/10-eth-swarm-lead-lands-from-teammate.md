---
slug: eth-swarm-lead-lands-from-teammate
kind: note-to-existing
title: "Note on bento-eth: the swarm lead should land from a lead-owned scratch worktree, not the teammate's; add land.py --branch"
priority: P2
type: note
labels: [audit, swarm, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-eth
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-eth: land from a lead-owned scratch worktree

Target: **bento-eth** (open, P3, "Bake recurring teammate-prompt lessons into the swarm template; make lead-lands the default ..."). Post as a comment. Do not file a new issue. The audit rates this addendum P2. The maintainer can decide whether to raise bento-eth.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (finding bento-12)**
>
> bento-eth makes lead-lands the default, and bento-qiw settles who lands. Neither says *where* the lead lands from. Today the swarm skill has the lead land from inside the teammate's worktree:
>
> - `catalog/skills/swarm/SKILL.md:337-342` (Serial-Mode Landing step 3, re-verified 2026-09-23 at bento origin/main b1bb787): "Invoke `bento:land-work` from within the teammate's worktree". land.py derives the branch from its cwd and needs the branch up to date (`--require-up-to-date`, `land.py:274`), so the lead ends up running checkout and rebase inside the teammate's tree.
> - A shatter lesson from 2026-09-08 (memory `feedback_lead_landing_prep_separate_worktree`) records that this "silently moved the worktree's HEAD out from under" a teammate who was still making review fixes.
>
> **Proposed additions to acceptance criteria:**
> 1. The lead lands from a lead-owned scratch worktree created at the teammate's branch tip, or from the teammate's worktree only after confirming that the teammate session has exited.
> 2. `land.py --branch <name>` lands a named branch from any worktree without checking it out in the teammate's tree. The preview is built from the ref, and local cleanup skips any worktree it does not own.
> 3. Swarm SKILL.md step 3 is updated to match, and the teammate prompt template says "the lead will not touch your worktree while you are live".
> 4. Test: `land.py --branch` run from a different worktree leaves the teammate worktree's HEAD, index and working tree unchanged.
>
> Proof at close: the test above passing, and the SKILL.md diff. Related: bento-qiw, `rebase-before-land-configurable` (drops the rebase need when a repo opts out of it).
