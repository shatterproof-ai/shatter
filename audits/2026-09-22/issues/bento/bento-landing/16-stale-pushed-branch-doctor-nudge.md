---
slug: stale-pushed-branch-doctor-nudge
kind: new
title: "SessionStart doctor: one collapsed, cached line for pushed branches older than 7 days whose issue is not in_progress"
priority: P3
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# SessionStart doctor: one collapsed, cached line for aging pushed branches with idle issues

Split from the audit draft `merge-message-and-stale-branch-nudge`, which bundled this with unrelated deliverables.

## Problem

Branches sit pushed for weeks and then need re-landing: `-landing2` branches, or diffs "ported/rewritten against current main". bento-rdtn.9 (closed) added closure's `tracker_mismatch` report, which finds exactly these branches (branch carries an issue id whose issue is not in_progress), but it runs only when someone invokes closure. Nothing surfaces the signal at session start, when an agent could act on it.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main 0b8d488:

- `16794cef Merge branch 'str-qwua7.4-landing2'` is a re-landing. Branches authored 2026-08-27 to 08-31 landed on 09-21/22.
- `catalog/skills/closure/scripts/closure-scan.py:1558` (`annotate_branches_with_tracker_mismatch`) and `:1539` (`suggest_tracker_mismatch_action`): the rdtn.9 logic, reachable only through closure.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py`: no branch-age or tracker-mismatch check.

## Acceptance criteria

- [ ] The SessionStart doctor (both runtimes) prints at most **one** line when remote branches whose last commit is older than 7 days carry an issue id whose issue is not `in_progress`, for example `3 pushed branches >7d with idle issues; run closure for details`. It reuses closure's tracker_mismatch logic by import, not by copy.
- [ ] The check is cached per repo (for example 6 hours) so SessionStart does not query the tracker on every session, and it has a hard time budget (for example 2 s) after which it is skipped silently. Branches whose issue lookup fails are not counted.
- [ ] Opt-out key in `.agent-mode.local` (added to `RECOGNIZED_AGENT_MODE_KEYS` in both doctors).
- [ ] Tests in `tests/test_agent_env_doctor.py` and `tests/test_agent_env_doctor_codex.py` with a fixture remote containing an old branch whose issue is closed (stubbed bd), a fresh branch, and an old branch whose issue is in_progress: exactly one line, counting 1. A second run within the cache window makes no tracker call. The opt-out suppresses the line.
- [ ] Proof at close: test names plus passing output in the close reason. "Merged" is not sufficient.

## Out of scope

- Deleting branches (bento-73de).
- The reverse direction, in_progress issues with no live branch (`claim-branch-reconciliation`, bucket bento-guards-doctor-tracker).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9 (closed), `claim-branch-reconciliation`, `landing-deletes-remote-branches` (note on bento-73de).

Priority: P3 · Type: feature · Labels: audit, closure, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17
