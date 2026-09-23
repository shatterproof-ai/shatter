---
slug: git-hook-latency-visibility
kind: new
title: "launch-work and land-work: time git subprocesses and name slow hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers"
priority: P1
type: bug
labels: [audit, land-work, launch-work, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [beads-dolt-remote-guidance]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# launch-work and land-work: time git subprocesses and name slow hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers

Source findings: bento-03 (also sessions-05), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Related shatter issue: beads-retire-jsonl-import-dolt-remote (the shatter-side root-cause fix; mention in the body only, it is not a bento id).

## Problem

In shatter, git hooks installed by beads stall every worktree creation and every landing for minutes. Bento scripts that create worktrees pay that cost silently and in series (`launch-work-bootstrap --apply`, `land-work-create-preview.py`, merge/push in `land.py`). Agents see a hung command with no explanation and reach for hook bypasses. The bento git guard blocks the bypass, but its message sends them to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes", and that reference has no hook content.

The root cause was measured on 2026-09-23 (D4): bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s of CPU, so it is waiting, not computing). bd itself warns that the JSONL "is an export, not cross-machine sync or source of truth" and suggests a Dolt remote. The fix for that is the shatter issue beads-retire-jsonl-import-dolt-remote, plus the bento guidance in beads-dolt-remote-guidance. This issue makes the cost visible in bento's tools and points agents at the right fix. It deliberately adds no hook bypass and no timeout override.

## Evidence (shatter, 2026-09-19..23)

- `land.py` `create_preview` took 234.9-301.2 s in 11 successful landings (one 10.9 s outlier on 09-21).
- Every `launch-work-bootstrap --apply` exceeded the Bash tool timeout: more than 120 s on 09-19, more than 600 s on 09-21, more than 120 s for this audit's launch.
- Transcripts contain "beads: hook 'post-checkout' timed out after 300s" 32 times across 7 sessions.
- The installed hook `$(git rev-parse --git-common-dir)/hooks/post-checkout` in shatter carries `# --- BEGIN BEADS INTEGRATION v0.63.3 ---`, and `.beads/hooks/post-checkout` carries `# bd-hooks-version: 0.56.1`. `bd version` reports 1.1.0 (checked 2026-09-23).
- `time bd hooks run post-checkout` in a shatter linked worktree took 2m59.7s.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/launch-work/scripts/launch-work-bootstrap.py` lines 301-314: runs `git worktree add -b ...` with no timing or warning.
- `catalog/skills/land-work/scripts/land-work-create-preview.py` line 343: `git("worktree", "add", "--detach", ...)` for the preview, untimed.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` lines 229-236: the block message cites "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes".
- `catalog/skills/launch-work/references/dependency-bootstrap.md`: zero matches for "hook" or "slow".
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py`: has a hook-binaries check (`check_hook_binaries`, line 389) but no hook-latency or beads hook-version check.

## Acceptance criteria

- `launch-work-bootstrap` and `land-work-create-preview.py` (and the merge/push subprocesses in `land.py`) time each git subprocess. When one takes longer than 30 s, they print a one-line warning naming the command and the hook(s) that ran, if they can tell (for example, a beads-managed `post-checkout`), and pointing to the beads-issue-flow Dolt-remote section. Tests use a fake slow hook in a fixture repo and assert the warning; a fast hook produces no warning.
- The guard's hook-bypass block message cites a section that exists and discusses slow hooks: the beads-issue-flow "Snapshot and Dolt remote" guidance from beads-dolt-remote-guidance. A test asserts the cited file and heading exist in the catalog.
- The doctor reports when a repo's beads hook markers (`BEADS INTEGRATION vX` in the common-dir hooks, `bd-hooks-version: X` in `.beads/hooks`) are older than `bd version`, and names `bd hooks install` as the refresh step. Tested with a fixture hook file.
- Nothing added by this issue sets `BEADS_HOOK_TIMEOUT`, `core.hooksPath`, `--no-verify` or any other hook bypass or timeout override, and no guidance recommends one. Reviewers check this in the diff.
- Proof at close: the close note names the new tests with failing-then-passing runs, and includes the output of one real `launch-work-bootstrap --apply` in a repo with a slow hook showing the warning.

## Suggested approach

- Add a small `timed_git(...)` helper in the shared script utilities that records wall time and, over the threshold, lists hooks present for that git operation (post-checkout for `worktree add`, post-merge for `merge`, pre-push for `push`) from the effective hooks path.
- Warn only; do not change hook behaviour for previews or work worktrees.

## Out of scope

- Any hook bypass, timeout override, or hydration suppression (D4).
- Shatter's own hook and JSONL import changes (shatter beads-retire-jsonl-import-dolt-remote).
- Fixing bd hook latency upstream.
- The guard's bypass detection (git-guard-bypasses-and-false-positives).

## Priority / Type / Labels

P1 / bug / audit, land-work, launch-work, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by beads-dolt-remote-guidance: the guard pointer and the warning must cite the section that issue adds. The timing and doctor parts can start earlier.
