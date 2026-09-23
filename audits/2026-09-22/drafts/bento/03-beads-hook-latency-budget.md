# Budget and surface beads git-hook latency in launch-work and land-work; the guard's slow-hook pointer cites a reference with no hooks content

- Filing action: new issue
- Priority: P1
- Type: bug
- Labels: audit, land-work, launch-work, hooks
- Parent: epic
- Links: related str-qwua7.28 (shatter-side hook timeout; not a bento id — mention in body only)
- Source findings: bento-03 (also sessions-05)

---BODY---
## Problem

In shatter, the beads post-checkout, post-merge and pre-push hooks each run up to their 300 s timeout. Bento scripts that create worktrees pay this cost silently and in series:

- `launch-work-bootstrap --apply`
- `land-work-create-preview.py`
- merge/push in `land.py`

This adds roughly 4-5 minutes to every landing and every worktree launch. Agents then try `--no-verify` or `core.hooksPath=/dev/null`. The bento git guard now blocks that, and its message sends them to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes". That reference contains nothing about hooks.

## Evidence (shatter, 2026-09-19..22)

- `land.py` `create_preview` took 234.9-301.2 s in 11 successful landings. One 10.9 s outlier was seen on 09-21.
- Every `launch-work-bootstrap --apply` exceeded the Bash tool timeout: more than 120 s on 09-19, more than 600 s on 09-21, and more than 120 s for this audit's launch.
- Transcripts contain "beads: hook 'post-checkout' timed out after 300s" 32 times across 7 sessions.
- The shatter hooks carry the marker `BEADS INTEGRATION v0.63.3` with `_bd_timeout=${BEADS_HOOK_TIMEOUT:-300}`. Installed `bd version` is 1.1.0.
- `time bd hooks run post-checkout` in a shatter linked worktree took 2m59.7s.

## Current code facts (bento @ 1c0c1e6)

- `catalog/skills/launch-work/scripts/launch-work-bootstrap.py` about lines 302-310: runs `git worktree add` with no timing or warning.
- `catalog/skills/land-work/scripts/land-work-create-preview.py` about line 343: `git worktree add` for the preview, untimed.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` about lines 229-234: the block message cites "dependency-bootstrap guidance for slow-hook fixes".
- `catalog/skills/launch-work/references/dependency-bootstrap.md` has zero matches for "hook" or "slow". It covers only lockfile installers and ext4 disk notes.
- `agent-env-doctor.py` has no hook-latency or hook-version check.

## Acceptance criteria

- Throwaway land-work preview worktrees are created with beads hydration suppressed for that subprocess, for example `BEADS_HOOK_TIMEOUT=5` in the child env or an equivalent documented mechanism. A test asserts the env is set.
- Bootstrap and create-preview time each git subprocess and print a one-line warning that names the hook when it exceeds 30 s.
- dependency-bootstrap.md (or the reference the guard cites) gets a "Slow git hooks" section covering: diagnosis (`time bd hooks run post-checkout`), the sanctioned remedies (env timeout in the session environment, upgrading hooks via `bd hooks install`), and what not to do (no `--no-verify`).
- The doctor reports when installed beads hook markers are older than `bd version`.

## Suggested approach

Suppress hydration only for previews, because they are deleted after verification. Do not suppress it for real work worktrees; there, warn only.

## Decision left to implementer

Choose the exact suppression mechanism once you confirm it with beads 1.1 docs. `BEADS_HOOK_TIMEOUT` is honoured by the v0.63.3 hook script.

## Out of scope

- Shatter's own hook env block (shatter str-qwua7.28).
- Fixing bd hook latency upstream.
