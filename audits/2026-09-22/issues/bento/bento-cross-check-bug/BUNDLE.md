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

A related gap: `--scope` is used only when rendering the saved review header
(`cross-check-run.py` lines 258 and 289). It is never included in the counterpart's prompt. So a
reviewer run from a neutral directory, which is how the problem was worked around on 2026-09-23, gets
no repository context except the absolute paths inside the artifact.

## Evidence

- In a 22-bucket run on 2026-09-23 (the shatter audit worktree had uncommitted draft files), 16 runs
  exited 4 on identity validation and 1 "passed" with a hook-reply body
  (`shatter/audits/2026-09-22/issues/crosscheck/shatter-frontend-rust.md`, branch `audit-2026-09-22`).
- The same bundle type, rerun with a clean worktree, produced a valid review. Codex stderr shows
  `hook: Stop` / `hook: Stop Completed` right after the identity block.
- `grep -rn CROSS_CHECK_ACTIVE plugins/*/bento/hooks` returns nothing (bento source 0b8d488).
- Environment of the failures: `codex-cli 0.155.1` with bento 2.3.73 installed in Codex
  (`~/.codex/plugins/cache/bento/bento/2.3.73/hooks/hooks.json` registers `Stop` ->
  `check-unpushed.py`); `cross-check-run.py` from Claude-side bento 2.3.82. The installed
  2.3.73 hooks and runner differ from current source, so the fix must be verified against a
  rebuilt, reinstalled plugin, not the source tree alone.
- `cross-check-run.py` `finally:` unlinks `last_file` on every path, including identity failure.

## Acceptance criteria

- **Narrow exemption.** Only Stop-lifecycle hooks that can hijack the final message
  (`check-unpushed.py` and any other bento Stop/SubagentStop hook) exit 0 silently when
  `CROSS_CHECK_ACTIVE=1`, in both the Claude and Codex plugin trees. PreToolUse guards (edit guards,
  the git worktree guard, destructive-git blocks) stay active. Tests show that each exempted hook is
  silent under the marker, and that the same hooks and the PreToolUse guards still block in an
  ordinary session.
- **Artifact-aware validation.** The runner rejects a counterpart reply that answers a hook rather
  than reviewing the artifact, without rejecting a legitimate clean review. Validation is per
  artifact type: `issue` and `plan` need a verdict line, `code` may report "no serious findings" as
  long as it gives a safe-to-land verdict. Regression tests cover the observed hook-reply text
  (rejected) and a clean no-findings review for each of code, plan and issue (accepted).
- **Full output preservation.** On any rejection (identity failure, validation failure, nonzero
  exit), the runner keeps the counterpart's stdout, stderr and final-message file, for example under
  `/tmp/cross-check-<slug>-<ts>.rejected/`. The fallback message prints their paths. `last_file` is
  no longer unconditionally unlinked.
- **Scope reaches the reviewer.** `--scope` text, plus any repository roots the caller names, is
  included in the counterpart prompt, not just the rendered header. A test asserts it appears in the
  composed prompt.
- **Reproducible end-to-end proof.** On a rebuilt and reinstalled plugin (record the bento SHA,
  plugin versions for both runtimes, and `codex --version`), run `cross-check-run.py
  --artifact-type issue` from a worktree with uncommitted changes. It must exit 0 with a real review
  that has findings or a verdict and no hook text. Record the command, fixture and output path in the
  close reason.

## Suggested approach

Add a shared `recursion_active()` check (reuse `cross_check_common.RECURSION_ENV`) at the top of the
Stop hook entry points only. In the runner, keep outputs on failure and move `--scope` into
`compose_prompt`.

## Out of scope

Changing the identity-block protocol itself. Blanket exemption of PreToolUse safety guards.

## Cross-check

Reviewed by Codex on 2026-09-23 (`crosscheck/bento-cross-check-bug.codex.md`). All 4 MAJOR findings
and the 1 MINOR are applied above: a narrower exemption, artifact-aware validation, full output
preservation, scope delivery, and a reproducible runtime.
