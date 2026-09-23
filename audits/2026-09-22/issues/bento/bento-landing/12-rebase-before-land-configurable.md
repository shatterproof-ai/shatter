---
slug: rebase-before-land-configurable
kind: new
title: "land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option; the preview already verifies the exact merge"
priority: P3
type: feature
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option

## Problem

land.py always calls prepare with `--require-up-to-date`. A branch that is behind the primary branch by even one commit fails `prepare`. The agent must then rebase and force-push. In shatter, each of those steps re-runs the slow git hooks (post-checkout beads import, pre-push `task affected`), and the Stop-hook "unpushed" counts balloon (see `check-unpushed-overcount-and-blocks`).

Rebase-first is documented, deliberate policy: SKILL.md step 5 says "Rebase onto the preferred primary-branch base", which keeps history linear. It is not a leftover to remove. But it is not needed for verification. create-preview already merges the feature onto the leased primary SHA, and the verifier checks that exact tree. Repos that do not need linear history should be able to turn it off.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6):

- `catalog/skills/land-work/scripts/land.py:274`: `prepare = self._run_script("prepare", PREPARE, ["--require-up-to-date"])`. The flag is unconditional.
- `land.py:291`: the preview is created with `--base-ref <leased_sha>`, so verification runs on the merge with current main.
- `catalog/skills/land-work/SKILL.md:239`: step 5, "Rebase onto the preferred primary-branch base reported by the helper".
- Shatter prepare failures: "current branch is behind the primary branch by 2 commit(s)" (session 87606e10, 2026-09-20 04:53); "by 4 commit(s)" (9f13ca23, 2026-09-21 15:19); "no commits ahead; behind by 1" (2026-09-20 23:23). Each forced a rebase and force-push cycle.

## Acceptance criteria

- [ ] verifier.json gains `require_up_to_date: true|false`, defaulting to `true`, so current behaviour is unchanged for every repo that does not set it. land.py passes `--require-up-to-date` only when the value is true.
- [ ] With `false`, a branch that is behind but merges cleanly lands through the preview merge. A merge conflict in the preview still fails `create_preview`, with an error that says a rebase is needed.
- [ ] "No commits ahead" still fails under both settings.
- [ ] SKILL.md step 5 and the land.py docs describe the option and name linear history as the reason for the default.
- [ ] Tests cover: default (behind fails), `false` + behind + clean merge (lands), `false` + conflict (fails with "rebase needed").
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Changing the default policy.
- Swarm-config landing blocks, unless verifier.json turns out to be the wrong home for the option; decide that during implementation and record the reason.

## Dependencies

- Blocked by: none.
- Related: `check-unpushed-overcount-and-blocks` (bucket bento-guards-doctor-tracker), `eth-swarm-lead-lands-from-teammate`.

Priority: P3 · Type: feature · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/19, bento-07 (verifier lowered it from P2 to P3 because the rebase is documented policy)
