---
slug: eth-swarm-lead-lands-from-teammate
kind: note-to-existing
title: "Note on bento-eth: the swarm template should have the lead land from a lead-owned scratch worktree, not the teammate's (driver support tracked separately)"
priority: P2
type: note
labels: [audit, swarm, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-branch-flag]
existing_id: bento-eth
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-eth: land from a lead-owned scratch worktree

Target: **bento-eth** (open, P3, "Bake recurring teammate-prompt lessons into the swarm template; make lead-lands the default ..."). Post as a comment. Do not file a new issue. bento-eth's scope explicitly excludes "land-work's own mechanics", so the driver change this needs (`land.py --branch`) is a separate new issue, `land-py-branch-flag`, and this comment covers only swarm template text. bento-eth is sequenced after bento-qiw (open), which rewrites the Phase 2/4 landing text; the maintainer can decide whether this text belongs in qiw's rewrite instead. The audit rates this addendum P2; the maintainer can decide whether to raise bento-eth.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (finding bento-12)**
>
> The operator decision recorded on bento-qiw and bento-eth (2026-06-12) makes lead-lands the default. Neither issue says *where* the lead lands from. Today the swarm skill has the lead land from inside the teammate's worktree:
>
> - `catalog/skills/swarm/SKILL.md:337-342` (Serial-Mode Landing step 3, re-verified 2026-09-23 at bento origin/main 0b8d488): "Invoke `bento:land-work` from within the teammate's worktree". land.py derives the branch from its cwd and needs the branch up to date (`--require-up-to-date`, `land.py:274`), so the lead ends up running checkout and rebase inside the teammate's tree.
> - A shatter lesson from 2026-09-08 (memory `feedback_lead_landing_prep_separate_worktree`) records that this "silently moved the worktree's HEAD out from under" a teammate who was still making review fixes.
>
> **Proposed additions to this issue's template text** (swarm SKILL.md only):
> 1. Serial-Mode Landing step 3: the lead lands from a lead-owned scratch worktree using `land.py --branch <teammate-branch>` (added by `<id of land-py-branch-flag>`), or from the teammate's worktree only after confirming that the teammate session has exited.
> 2. The teammate prompt template says "the lead will not check out, rebase or reset in your worktree while you are live".
>
> Acceptance for these additions: `grep -n "from within the teammate's worktree" catalog/skills/swarm/SKILL.md` finds nothing; step 3 names `land.py --branch` and the exited-session exception; the teammate prompt template contains the sentence above. Proof at close: the SKILL.md diff. Related: bento-qiw, `<id of land-py-branch-flag>`, `rebase-before-land-configurable` (drops the rebase need when a repo opts out of it).
