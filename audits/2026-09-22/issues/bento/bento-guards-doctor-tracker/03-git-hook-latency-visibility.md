---
slug: git-hook-latency-visibility
kind: new
title: "launch-work and land-work: warn while a git subprocess is slow and name its hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers"
priority: P1
type: bug
labels: [audit, land-work, launch-work, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [beads-dolt-remote-guidance, land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# launch-work and land-work: warn while a git subprocess is slow and name its hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers

Source findings: bento-03 (also sessions-05), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Related shatter issue: beads-retire-jsonl-import-dolt-remote (the shatter-side root-cause fix; mention in the body only, it is not a bento id). Related bento draft in this epic: land-py-invocation-progress-log (bucket bento-landing), which makes land.py run children with `Popen` and emit heartbeats; this issue relies on that plumbing so child warnings reach the user.

## Problem

In shatter, git hooks installed by beads stall every worktree creation and every landing for minutes. Bento scripts that create worktrees pay that cost silently and in series (`launch-work-bootstrap --apply`, `land-work-create-preview.py`, merge/push in `land.py`). Agents see a hung command with no explanation and reach for hook bypasses. The bento git guard blocks the bypass, but its message sends them to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes", and that reference has no hook content.

The root cause was measured on 2026-09-23 (D4): bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s of CPU, so it is waiting, not computing). The fix for that is the shatter issue beads-retire-jsonl-import-dolt-remote, plus the bento guidance in beads-dolt-remote-guidance. This issue makes the cost visible in bento's tools **while the wait is happening** and points agents at the right fix. It deliberately adds no hook bypass and no timeout override.

## Evidence (shatter, 2026-09-19..23)

- `land.py` `create_preview` took 234.9-301.2 s in 11 successful landings (one 10.9 s outlier on 09-21).
- Every `launch-work-bootstrap --apply` exceeded the Bash tool timeout: more than 120 s on 09-19, more than 600 s on 09-21, more than 120 s for this audit's launch.
- Transcripts contain "beads: hook 'post-checkout' timed out after 300s" 32 times across 7 sessions.
- The installed hook `$(git rev-parse --git-common-dir)/hooks/post-checkout` in shatter carries `# --- BEGIN BEADS INTEGRATION v0.63.3 ---`, and `.beads/hooks/post-checkout` carries `# bd-hooks-version: 0.56.1`. `bd version` reports 1.1.0 (checked 2026-09-23).
- `time bd hooks run post-checkout` in a shatter linked worktree took 2m59.7s.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/launch-work/scripts/launch-work-bootstrap.py` lines 301-314: runs `git worktree add -b ...` with no timing or warning.
- `catalog/skills/land-work/scripts/land-work-create-preview.py` line 343: `git("worktree", "add", "--detach", ...)` for the preview, untimed.
- `catalog/skills/land-work/scripts/land.py` `_run_script` (line 105): runs each step with `subprocess.run(..., capture_output=True)` and only surfaces the child's stderr inside a `StepFailure` message. On success the child's stderr is discarded, so a warning printed by `land-work-create-preview.py` today would never reach the user, and nothing is shown until the step returns.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` lines 229-236: the block message cites "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes".
- `catalog/skills/launch-work/references/dependency-bootstrap.md`: zero matches for "hook" or "slow".
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py`: has a hook-binaries check (`check_hook_binaries`, line 389) but no hook-latency or beads hook-version check.

## Acceptance criteria

- **Live warning, not a post-mortem.** `launch-work-bootstrap`, `land-work-create-preview.py` and the merge/push subprocesses in `land.py` run each hook-triggering git command through one helper. When the command is still running after the threshold (default 30 s), the helper prints, while it is still running, one line naming the command, the hook(s) that git will run for it (resolved from `git rev-parse --git-path hooks`, which honours `core.hooksPath`), whether a hook is beads-managed (contains a `BEADS INTEGRATION` or `bd-hooks-version` marker), and a pointer to the beads-issue-flow "Snapshot and Dolt remote" section. On completion over the threshold it prints the elapsed time.
- **Reaches the user through land.py.** A test runs `land.py` end to end against a fixture repo whose `post-checkout` hook sleeps past a test threshold, and asserts that the warning line appears on land.py's stderr (and in its progress log from land-py-invocation-progress-log) **before** the `create_preview` step finishes. A test that only calls `land-work-create-preview.py` directly does not satisfy this criterion.
- **Test threshold without env vars.** The threshold is a helper parameter and a CLI flag on the scripts (for example `--slow-git-warn-seconds`), so tests run in a few seconds. No environment variable is added (D4 forbids hook-timeout env vars; this keeps the surface unambiguous). A fast hook produces no warning.
- **Guard pointer.** The guard's hook-bypass block message cites a section that exists and discusses slow hooks: the beads-issue-flow "Snapshot and Dolt remote" guidance from beads-dolt-remote-guidance. A test asserts the cited file and heading exist in the catalog.
- **Stale beads hook markers.** The doctor reports when a repo's beads hook markers are older than the installed bd: it parses `BEADS INTEGRATION vX.Y.Z` in the effective hooks dir and `bd-hooks-version: X.Y.Z` in `.beads/hooks`, parses `bd version`, and warns when either marker's (major, minor) is lower than bd's. It names `bd hooks install` as the refresh step. `bd version` runs with a 5 s timeout; on timeout or absence the check is skipped silently. Fixture tests cover older, equal and newer markers, and a missing bd.
- Nothing added by this issue sets `BEADS_HOOK_TIMEOUT`, `core.hooksPath`, `--no-verify` or any other hook bypass or timeout override, and no guidance recommends one. The helper passes the caller's environment and git config through unchanged; a test asserts the child git process sees no added `GIT_CONFIG_*`, `-c` or `BEADS_*` settings. Reviewers confirm the prose.
- Proof at close: the close note names the new tests with failing-then-passing runs, and includes the stderr of one real `land.py` or `launch-work-bootstrap --apply` run in a repo with a slow hook, showing the warning line timestamped before the step completed.

## Suggested approach

- A small `timed_git(...)` helper in the shared script utilities using `Popen` plus `wait(timeout=threshold)`, printing the warning on the first timeout and continuing to wait.
- For `land.py`, reuse the `Popen`/heartbeat loop from land-py-invocation-progress-log and forward child stderr lines as they arrive, instead of discarding them.
- Warn only; do not change hook behaviour for previews or work worktrees.

## Out of scope

- Any hook bypass, timeout override, or hydration suppression (D4).
- Shatter's own hook and JSONL import changes (shatter beads-retire-jsonl-import-dolt-remote).
- Fixing bd hook latency upstream.
- The guard's bypass detection (git-guard-bypasses-and-false-positives, bento-l01v).
- land.py's progress log and heartbeat themselves (land-py-invocation-progress-log).

## Priority / Type / Labels

P1 / bug / audit, land-work, launch-work, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

- Blocked by beads-dolt-remote-guidance: the guard pointer and the warning must cite the section that issue adds.
- Blocked by land-py-invocation-progress-log (bucket bento-landing): the land.py part needs its `Popen`/heartbeat plumbing so child output is visible while a step runs.
- The launch-work timing and the doctor marker check can start earlier.
