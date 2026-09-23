---
slug: merge-message-and-stale-branch-nudge
kind: new
title: "land-work: merge message names branch and issue; refuse bare-SHA merges into the primary branch; SessionStart nudge for aging pushed branches; one session per branch"
priority: P3
type: feature
labels: [audit, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: merge message names branch and issue; refuse bare-SHA merges; nudge for aging branches; one session per branch

## Problem

Consumer history fills with landing noise that neither land.py nor the doctor prevents:

- Merges titled `Merge commit '<sha>' into HEAD`, which name neither a branch nor an issue. These come from manual merges of a bare SHA.
- Branches that sit pushed for weeks and then need re-landing (`-landing2` branches, or diffs "ported/rewritten against current main").
- Two concurrent sessions making opposite decisions on the same branch, which in shatter led to a revert.

Since land.py took over (2026-09-19), its merges are uniform (`Merge branch '<branch>'`), but they still omit the issue id. Manual merges are unconstrained. Stale branches are reported only when closure is run on demand.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main b1bb787:

- `git log origin/main --merges --since=2026-09-05 --format='%h %s' | grep -c "Merge commit '"` gives 6: af6839f0, a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f (2026-09-07/08).
- `16794cef Merge branch 'str-qwua7.4-landing2'` is a re-landing. Branches authored 2026-08-27 to 08-31 landed on 09-21/22.
- `catalog/skills/land-work/scripts/land.py:194` and `:221`: the message is `f"Merge branch '{feature_branch}'"` and does not include the issue id.
- bento-rdtn.9 (closed): closure's `tracker_mismatch` report (`catalog/skills/closure/scripts/*.py`, `annotate_branches_with_tracker_mismatch`) runs only when closure is invoked.

## Acceptance criteria

- [ ] land.py's merge message is `Merge branch '<branch>' (<issue-id>)` when the branch name contains an issue id (parsed with closure's existing rule), and `Merge branch '<branch>'` otherwise. Both routes use it (`:194`, `:221`), and a test asserts both.
- [ ] land-work guidance says never to `git merge <bare-sha>` into the primary branch. Where feasible, the bento git guard refuses `git merge <40-hex or short sha>` while the primary branch is checked out, and a guard test covers it. If the guard cannot do this reliably, record why in the issue and keep only the guidance.
- [ ] The SessionStart doctor prints at most one collapsed line when remote branches older than 7 days carry an issue id whose issue is not `in_progress`, for example `3 pushed branches >7d with idle issues; see closure report`. It reuses closure's tracker_mismatch logic and is cached so the tracker is not queried on every session.
- [ ] The swarm and land-work skills state: "One session owns a branch at a time; hand off explicitly (message + tracker note) before another session touches it."
- [ ] Tests cover the merge message (with and without an issue id), the doctor line (fixture with an old branch and an idle issue), and the guard refusal if implemented.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Rewriting existing history.
- Automatic branch deletion (`landing-deletes-remote-branches`).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9, bento-rdtn.14 (closed), `landing-deletes-remote-branches`, `claim-branch-reconciliation` (bucket bento-guards-doctor-tracker).

Priority: P3 · Type: feature · Labels: audit, land-work, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17
