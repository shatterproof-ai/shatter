---
slug: land-py-branch-flag
kind: new
title: "land.py --branch <name>: land a named branch from any worktree without checking it out or rebasing in the owner's worktree"
priority: P2
type: feature
labels: [audit, land-work, swarm, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py --branch <name>: land a named branch from any worktree without touching the owner's worktree

Split from the audit's bento-eth addendum (`eth-swarm-lead-lands-from-teammate`): bento-eth covers only swarm template text and excludes land-work mechanics, so the driver capability is its own issue.

## Problem

land.py can land only the branch checked out in its cwd. A swarm lead, who lands by default (operator decision on bento-qiw/bento-eth, 2026-06-12), therefore has to run land.py from inside the teammate's worktree, and when the branch is behind, rebase there. In shatter this moved a live teammate's HEAD out from under it (memory `feedback_lead_landing_prep_separate_worktree`, 2026-09-08). The lead needs to land a named branch from a worktree it owns, without changing anything in the teammate's worktree.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6). Files under `catalog/skills/land-work/scripts/`.

- `land.py:74-78` (`parse_args`): only `--timeout` and `--runtime`.
- `land.py:274-277`: the branch comes from `land-work-prepare.py`'s view of the cwd (`prepare["branch"]`), and prepare runs with `--require-up-to-date`.
- `land-work-create-preview.py` already takes a `--feature-ref` argument (`:36`; `feature_ref = args.feature_ref or branch` at `:250`), so the preview can be built from a ref other than the cwd's branch.
- `land.py:193-195`: `_merge_in_primary` merges `feature_branch` by name in the primary checkout, which works for any local branch ref.
- `catalog/skills/swarm/SKILL.md:340-341`: "Invoke `bento:land-work` from within the teammate's worktree".

## Acceptance criteria

- [ ] `land.py --branch <name>` lands the named local branch from any worktree of the repo (including a lead-owned scratch worktree or a linked worktree on another branch). prepare, create-preview (via `--feature-ref`), verify and merge all use `<name>`; nothing is checked out, rebased or reset in the worktree that has `<name>` checked out.
- [ ] If `<name>` is behind the primary branch and `--require-up-to-date` applies, land.py fails at prepare with a message naming the branch and the owning worktree; it never rebases. (A repo that sets `require_up_to_date: false` via `rebase-before-land-configurable` lands through the preview merge instead.)
- [ ] Local cleanup after a `--branch` landing does not remove or modify any worktree land.py did not create; the final JSON reports `owner_worktree` and that it was left untouched.
- [ ] Without `--branch`, behaviour is unchanged.
- [ ] Tests in `tests/land_work/test_land_driver.py`: (a) with a "teammate" worktree on `feat` that has an uncommitted file and a staged change, `land.py --branch feat` run from a different worktree lands `feat`, and the teammate worktree's HEAD, index (`git diff --cached`) and working tree (`git status --porcelain`) are byte-identical before and after; (b) `--branch` on a behind branch fails at prepare and leaves the teammate worktree unchanged; (c) no-flag regression. Test (a) is committed failing (unknown flag) first, then passing.
- [ ] Proof at close: the test names and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver` output in the close reason. "Merged" is not sufficient.

## Out of scope

- Swarm SKILL.md text (bento-eth, with the audit's addendum).
- Preview ownership between concurrent landers (bento-e583).

## Dependencies

- Blocked by: none.
- Blocks: `eth-swarm-lead-lands-from-teammate` (the bento-eth comment cites this issue's id).
- Related: bento-qiw, bento-eth, `rebase-before-land-configurable`, `land-py-merge-abort-ownership`.

Priority: P2 · Type: feature · Labels: audit, land-work, swarm, concurrency · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento-12
