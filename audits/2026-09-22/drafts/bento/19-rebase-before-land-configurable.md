# land.py: make rebase-before-land (--require-up-to-date) a repo policy option, since the preview already verifies the exact merge

- Filing action: new issue
- Priority: P3
- Type: feature
- Labels: audit, land-work
- Parent: epic
- Source findings: bento-07

---BODY---
## Problem

land.py always calls prepare with `--require-up-to-date`. A branch that is behind the primary branch by even one commit fails prepare. The agent then has to rebase and force-push, and in shatter each of those steps runs the roughly 300 s beads hooks and pre-push `task affected` again. Stop-hook "unpushed" counts also balloon (see the check-unpushed issue).

create-preview already merges the feature into the leased primary SHA, and the verifier checks that exact tree, so the rebase adds nothing for verification. Rebase-first is documented policy (SKILL.md step 5, "Rebase onto the preferred primary-branch base") for linear history, which is a legitimate reason to keep it as an option.

## Evidence (shatter)

prepare failed with:
- "current branch is behind the primary branch by 2 commit(s)" (09-20 04:53)
- "by 4 commit(s)" (09-21 15:19)
- "no commits ahead; behind by 1" (09-20 23:23)

## Current code facts

- `catalog/skills/land-work/scripts/land.py:274`: `prepare = self._run_script("prepare", PREPARE, ["--require-up-to-date"])`.

## Acceptance criteria

- verifier.json (or swarm-config landing block) gains `require_up_to_date: true|false`. The default stays true, so current behaviour is unchanged.
- When it is false, a behind-but-mergeable branch lands via the preview merge, and a merge conflict in the preview still fails with "rebase needed".
- Tests cover both settings.

## Out of scope

- Changing the default policy.
