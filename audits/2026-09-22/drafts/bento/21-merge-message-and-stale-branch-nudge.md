# land.py: enforce a merge-message template; scheduled nudge for aging pushed branches; one-session-per-branch guidance

- Filing action: new issue
- Priority: P3
- Type: feature
- Labels: audit, land-work, hygiene
- Parent: epic
- Links: related bento-rdtn.9, related bento-rdtn.14
- Source findings: agent-repo-17

---BODY---
## Problem

Shatter's main history has six merges titled `Merge commit '<sha>' into HEAD` (2026-09-07/08: af6839f0, a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f), which name neither a branch nor an issue. Branches authored 08-27 to 08-31 sat for 3-4 weeks before landing on 09-21/22, and some needed re-landing via "-landing2" branches. Two concurrent sessions made opposite decisions on one branch, which led to a revert. land.py merges since 09-19 are uniform.

## Current code facts

- land.py builds its own merge message. Manual merges are unconstrained.
- bento-rdtn.9 (closure tracker_mismatch) reports stale branches only when closure is run on demand.

## Acceptance criteria

- land.py's merge message is `Merge branch '<branch>' (<issue-id>)`, and a test asserts it.
- The land-work guidance, and the git guard where feasible, refuses `git merge <bare-sha>` into the primary branch.
- The doctor (SessionStart) prints a single collapsed line when pushed branches older than 7 days have an issue that is not in_progress, pointing to the closure report.
- The swarm and land-work skills state: "one session owns a branch at a time; hand off explicitly".

## Out of scope

- Rewriting existing history.
