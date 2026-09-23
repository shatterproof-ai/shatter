---
slug: cross-check-stop-hook-hijack
kind: new
title: "cross-check: bento Stop hook hijacks the read-only Codex reviewer's final message; identity check then fails, or passes on an empty review"
priority: P1
type: bug
labels: [audit, hooks, cross-check]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# cross-check: bento Stop hook hijacks the read-only Codex reviewer's final message; identity check then fails, or passes on an empty review

Found while cross-checking the shatter 2026-09-22 audit issue drafts (2026-09-23).

## Problem

`cross-check-run.py` runs the counterpart as `codex exec --sandbox read-only ... -o <last-message-file> -`
with `cwd` set to the caller's working directory, and exports `CROSS_CHECK_ACTIVE=1`. Codex has bento
installed (`~/.codex/plugins/cache/bento/bento/2.3.73`, `[features] hooks = true`), so bento's
`check-unpushed.py` Stop hook fires inside the reviewer. None of the bento hook scripts read
`CROSS_CHECK_ACTIVE` (the constant exists only in `skills/cross-check/scripts/cross_check_common.py`).

When the reviewed repo's worktree has uncommitted changes, the hook blocks the reviewer's Stop. Codex
then answers the hook, and that answer becomes the final message the `-o` file captures. It is not
the review. Two failure modes follow:

1. **False fallback.** The hook reply has no identity block, so `validate_identity` fails ("reviewer
   omitted the required identity block"), the script exits 4, and the caller falls back to a
   same-runtime DEGRADED review. The full Codex review in the discarded output is lost: on failure the
   runner deletes `last_file` and prints `output=<none written>`.
2. **False success.** When Codex copies the identity block into its hook reply, validation passes and
   a "cross" review is written whose whole body is the hook reply. Observed text: "The review made no
   edits. The stop hook is reporting existing branch changes; committing those would exceed the
   read-only review scope…".

## Evidence

- In a 22-bucket run on 2026-09-23 (the shatter audit worktree had uncommitted draft files), 16 runs
  exited 4 on identity validation and 1 "passed" with a hook-reply body
  (`shatter/audits/2026-09-22/issues/crosscheck/shatter-frontend-rust.md`, branch `audit-2026-09-22`).
- The same bundle type, rerun with a clean worktree, produced a valid review. Codex stderr shows
  `hook: Stop` / `hook: Stop Completed` right after the identity block.
- `grep -rn CROSS_CHECK_ACTIVE plugins/*/bento/hooks` returns nothing.
- `cross-check-run.py` `finally:` unlinks `last_file` on every path, including identity failure.

## Acceptance criteria

- Every bento hook that can block (at least Stop/`check-unpushed.py`, and any PreToolUse deny) exits 0
  without output when `CROSS_CHECK_ACTIVE=1`, in both the Claude and Codex plugin trees. A test covers
  each hook.
- `validate_identity` (or the runner) rejects a review whose body is a hook reply or has no findings
  structure. At minimum, a body with no severity-tagged finding and no "ready to file" verdict counts
  as a failure. There is a regression test using the observed hook-reply text.
- On identity failure, the runner keeps the raw counterpart output next to the fallback message
  (e.g. `/tmp/cross-check-<slug>-<ts>.rejected.md`) so an expensive review is never silently lost.
- End-to-end proof: run `cross-check-run.py --artifact-type issue` against a dirty worktree with Codex
  installed. It exits 0 with a real review. Record the output in the close reason.

## Suggested approach

Add an early `if recursion_active(os.environ): sys.exit(0)` to the hook entry points (a shared helper
next to `bento_telemetry.py`). Consider also running the counterpart with `cwd` set to a neutral temp
directory and passing repo paths in the scope, which is the workaround used on 2026-09-23.

## Out of scope

Changing the identity-block protocol itself.
