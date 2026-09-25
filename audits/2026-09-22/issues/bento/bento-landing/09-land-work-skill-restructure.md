---
slug: land-work-skill-restructure
kind: new
title: "land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule"
priority: P2
type: task
labels: [audit, land-work, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule

## Problem

`catalog/skills/land-work/SKILL.md` is the largest bento skill, and all of it loads on every landing: the manual 10-step flow, batch mode and the manifest rules, even though land.py now drives the serial path. The skill also contradicts itself:

- Its Command Rule forbids `$(...)` in commands, but its own command blocks use it.
- Its Tracker Handoff states a `.beads/issues.jsonl` policy ("may be intentionally untracked ... do not re-add or commit it") while beads-issue-flow's session-completion text allows committing the export. bento-49pg (open P2, owner decision Option C, 2026-09-22) resolves this: the export is untracked, and land-work and beads-issue-flow each carry **one** matching rule. Shatter's AGENTS.md currently requires committing the file; maintainer decision D4 retires shatter's JSONL import in favour of a Dolt remote, which agrees with Option C.

In shatter, bento:land-work was invoked for only about 9 of 35 first-parent merges to main since 2026-09-05. Several other landings were hand-scripted, including 4 raw `git push --no-verify ... :refs/heads/main` by one session.

bento-by8 (open, P3) trims about 1,500 words of repeated text across several skills. This issue goes further for land-work: a structural rewrite that makes land.py the serial path.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6):

- `wc -w catalog/skills/land-work/SKILL.md` gives 6,342 words (45,480 bytes). `swarm/SKILL.md` is 4,178 words, the next largest.
- Command Rule, `SKILL.md:101-106`: "... with `&&`, pipes, `$(...)`, or inline interpreters". Violations in commands the skill tells agents to run: `:143` (`--head-sha $(git rev-parse HEAD)`), `:181-182` (`BASE_SHA=$(git merge-base ...)`, `HEAD_SHA=$(git rev-parse HEAD)`), `:449` (`--merge-sha $(git rev-parse <primary-branch>)`). `:670` is deliberately escaped (`\$(...)`) and explained, so it is not a violation.
- The land.py invocation sits in step 7a (`:255-263`), behind the manual compare-and-set flow. The `## Batch Landing` section runs `:545-722` even though `references/batch-landing.md` already exists and SKILL.md already links it (`:85`, `:88`).
- Obligations of the current serial path that **land.py does not perform** (it runs prepare, fetch, create_preview, verify, lease_check, merge_push, cleanup and verify_landing only; `land.py:273-340`): `pre`/`post` lifecycle hook scripts and hook skills (steps 2a and 8a, `:129-162`, `:436-460`), gate-evidence discovery and the primary-branch baseline (step 2c, `:163-170`), the independent code review (step 4, `:171-235`), the tracker handoff and issue close, root hygiene (`land-work-root-hygiene.py`, `:80`), and feature-worktree/branch teardown (step 10, `:516-533`).
- Other skills also contain unescaped `$(` in command blocks (launch-work, closure, beads-issue-flow, github-issue-flow, issue-readiness-check). They have their own Command Rules or none; this issue does not touch them.
- Tracker Handoff, `:783-789`.

## Acceptance criteria

- [ ] The serial path in SKILL.md is an ordered checklist with land.py at its centre: (1) `pre` hook scripts and skills, (2) gate-evidence discovery and primary-branch baseline, (3) independent code review, (4) run land.py (using the "Running land.py" block from `land-py-invocation-progress-log`), read the final JSON and fix the step it names, (5) `post` hook scripts and skills, (6) tracker handoff and close, (7) root hygiene, (8) feature worktree and local branch teardown. Every obligation listed under Evidence as "not performed by land.py" appears in that checklist in SKILL.md itself (not only in a reference), each with its command or skill name. A review table in the close note maps each current step (1-10) to where it now lives.
- [ ] The manual compare-and-set flow and the manifest rules move to `catalog/skills/land-work/references/` (for example `references/manual-landing.md`), and the `## Batch Landing` section is folded into the existing `references/batch-landing.md`. SKILL.md links each by file name.
- [ ] `wc -w catalog/skills/land-work/SKILL.md` is at most 2,500.
- [ ] No unescaped `$(` appears in any fenced command block of land-work's SKILL.md or of `catalog/skills/land-work/references/*.md`. The computations move into scripts, or land.py derives the values itself.
- [ ] The Tracker Handoff carries exactly the single rule bento-49pg defines for land-work (the Beads JSONL export is untracked local state and is never committed during landing) and otherwise defers to `beads-issue-flow` or `github-issue-flow`. If bento-49pg has not landed yet, this issue leaves the Tracker Handoff wording to 49pg and only moves it.
- [ ] A lint test in the existing skill test suite, **scoped to `catalog/skills/land-work/`**, fails when SKILL.md or a `references/*.md` fenced `bash`/`sh` block contains an unescaped `$(`, when SKILL.md exceeds the word budget, or when any of the checklist items above is missing (by a fixed list of required step names). It is committed failing against the current SKILL.md, then passing.
- [ ] Proof at close: the `wc -w` output, the step-mapping table, and the lint test failing and then passing, in the close reason. "Merged" is not sufficient.

## Suggested approach

1. Land `land-py-invocation-progress-log` first so the invocation block exists.
2. Move sections out verbatim into references, then trim what remains in SKILL.md.
3. Add `land.py --print-shas` or similar only if the references still need computed SHAs. Otherwise let land.py compute them.
4. Coordinate with bento-by8 so the two do not trim the same paragraphs twice. Link the new issue as related to bento-by8.

## Out of scope

- Changes to land.py's behaviour, apart from any small helper needed to remove `$(...)`.
- `$(` in other skills' command blocks (launch-work, closure, beads-issue-flow, github-issue-flow, issue-readiness-check). A repo-wide lint would need its own issue with an inventory.
- The JSONL policy decision itself (bento-49pg).
- The beads-issue-flow content itself (`beads-dolt-remote-guidance`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log`.
- Related: bento-49pg (open; owns the Tracker Handoff rule, land it first if possible), bento-by8 (open; this extends it), bento-7kw, `beads-dolt-remote-guidance` (bucket bento-guards-doctor-tracker).

Priority: P2 · Type: task · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/16, bento-14 · Decision: D4 (Tracker Handoff wording only; consistent with bento-49pg Option C)
