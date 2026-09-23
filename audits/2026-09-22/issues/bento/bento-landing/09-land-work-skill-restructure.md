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
- Its Tracker Handoff prescribes a `.beads/issues.jsonl` policy ("may be intentionally untracked ... do not re-add or commit it") instead of deferring to the tracker skill. Consumer repos document different policies. Shatter's AGENTS.md currently requires committing the file, though maintainer decision D4 retires shatter's JSONL import in favour of a Dolt remote.

In shatter, bento:land-work was invoked for only about 9 of 35 first-parent merges to main since 2026-09-05. Several other landings were hand-scripted, including 4 raw `git push --no-verify ... :refs/heads/main` by one session.

bento-by8 (open, P3) trims about 1,500 words of repeated text across several skills. This issue goes further for land-work: a structural rewrite that makes land.py the serial path.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6):

- `wc -w catalog/skills/land-work/SKILL.md` gives 6,342 words (45,480 bytes). `swarm/SKILL.md` is 4,178 words, the next largest.
- Command Rule, `SKILL.md:101-106`: "... with `&&`, pipes, `$(...)`, or inline interpreters". Violations in commands the skill tells agents to run: `:143` (`--head-sha $(git rev-parse HEAD)`), `:181-182` (`BASE_SHA=$(git merge-base ...)`, `HEAD_SHA=$(git rev-parse HEAD)`), `:449` (`--merge-sha $(git rev-parse <primary-branch>)`). `:670` is deliberately escaped (`\$(...)`) and explained, so it is not a violation.
- The land.py invocation sits in step 7a (`:255-263`), behind the manual compare-and-set flow. Batch Landing runs `:545-722`.
- Tracker Handoff, `:783-789`.

## Acceptance criteria

- [ ] The serial path in SKILL.md is: run land.py (using the "Running land.py" block from `land-py-invocation-progress-log`), read the final JSON, and fix the step it names. The manual compare-and-set flow, batch mode and the manifest rules move to `catalog/skills/land-work/references/`, and SKILL.md links to each by name.
- [ ] `wc -w catalog/skills/land-work/SKILL.md` is at most 2,500.
- [ ] No unescaped `$(` appears in any fenced command block of SKILL.md or of its references. The computations move into scripts, or land.py derives the values itself.
- [ ] The Tracker Handoff states no JSONL policy of its own. It defers to `beads-issue-flow` or `github-issue-flow` and to the repo's own documented policy. (`beads-dolt-remote-guidance` in this epic gives beads-issue-flow the D4 guidance: the JSONL is an export, and sync goes through a Dolt remote.)
- [ ] A skill lint test (in the existing skill test suite) fails when any `SKILL.md` or `references/*.md` fenced `bash`/`sh` block contains an unescaped `$(`, and when land-work's SKILL.md exceeds the word budget. It is committed failing against the current SKILL.md, then passing.
- [ ] Proof at close: the `wc -w` output, plus the lint test failing and then passing. "Merged" is not sufficient.

## Suggested approach

1. Land `land-py-invocation-progress-log` first so the invocation block exists.
2. Move sections out verbatim into references, then trim what remains in SKILL.md.
3. Add `land.py --print-shas` or similar only if the references still need computed SHAs. Otherwise let land.py compute them.
4. Coordinate with bento-by8 so the two do not trim the same paragraphs twice. Link the new issue as related to bento-by8.

## Out of scope

- Changes to land.py's behaviour, apart from any small helper needed to remove `$(...)`.
- The beads-issue-flow content itself (`beads-dolt-remote-guidance`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log`.
- Related: bento-by8 (open; this extends it), bento-7kw, `beads-dolt-remote-guidance` (bucket bento-guards-doctor-tracker).

Priority: P2 · Type: task · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/16, bento-14 · Decision: D4 (Tracker Handoff wording only)
