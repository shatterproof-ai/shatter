---
slug: beads-hook-stall-diagnosis
kind: new
title: "Beads: find what the post-checkout hook waits on, list every JSONL-import entry point, and record one reproducible latency baseline"
priority: P1
type: task
labels: [agents, beads, git-hooks, landing, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: find what the post-checkout hook waits on, list every JSONL-import entry point, and record one reproducible latency baseline

## Problem

Maintainer decision D4 (2026-09-23) retires the beads JSONL import in shatter,
because the beads `post-checkout` hook stalls landings and new worktrees for
minutes. The decision is taken; this issue does not reopen it. But the
implementation issue (beads-retire-jsonl-import-dolt-remote) needs three facts
that the audit did not establish:

1. **What the hook is waiting on.** The measured runs use about 10 s of CPU
   over minutes of wall time, so the time is spent waiting. Low CPU does not
   say *what* is waited on: the JSONL import itself, a Dolt server start, a
   lock held by a sibling worktree's bd process, or the network (for example
   a Dolt remote fetch). Turning the import off only fixes the stall if the
   wait is inside the import path.
2. **Every place the import can run.** shatter has beads-managed
   `post-checkout` **and** `post-merge` hooks, and bd may also auto-import on
   ordinary commands when `import.auto` is true. A fix verified only on
   checkout could leave the other paths importing.
3. **One baseline.** The audit's numbers come from different runs and are not
   comparable (see Evidence). The implementation's latency targets need a
   baseline taken with a recorded command, binary and directory.

## Evidence (re-verified 2026-09-23)

The existing measurements, with their sources. They are separate runs, not
one contradictory run:

| Number | Command | Where | Source |
|---|---|---|---|
| about 6 min, about 10 s CPU | `bd -v hooks run post-checkout` (direct, no `timeout` wrapper) | not recorded | D4 text in `audits/2026-09-22.md` ("Maintainer decisions") |
| 2m59.7s | `time bd hooks run post-checkout` (direct) | a linked worktree | finding agent-repo-07 |
| 234.9-301.2 s | land.py `create_preview` (runs `git worktree add`, so the hook runs under `timeout 300`) | `/tmp/land-work-preview-*` | 11 landings 09-19..09-22, `drafts/shatter-agent/08-beads-hook-timeout-decision.md` |
| "timed out after 300s", 32 times | hook-wrapped runs | various | session transcripts (sessions-05) |

The hook-wrapped runs cannot exceed 300 s because of
`timeout "${BEADS_HOOK_TIMEOUT:-300}"`; the 6-minute figure is from a direct,
unwrapped run.

- Hooks present in `/home/ketan/project/shatter/.git/hooks/` (shared by all
  worktrees): `post-checkout`, `post-merge`, `pre-commit`,
  `prepare-commit-msg`, `pre-push`. `post-checkout` and `post-merge` both
  carry the `BEADS INTEGRATION v0.63.3` block that runs
  `timeout "$_bd_timeout" bd hooks run <hook> "$@"`.
- `bd version` → `1.1.0 (8e4e59d39)`; two binaries on PATH
  (`/usr/local/bin/bd`, `~/.local/bin/bd`).
- `bd config show`: `import.auto = true (default)`,
  `import.path = issues.jsonl (default)`, `export.auto = false`,
  `backup.git-push = true`, `no-hooks = false`.
- Preview and linked worktrees logged "post-checkout JSONL import warning: no
  Dolt remote configured" (sessions-05, transcript 19cbf3b5) even though the
  primary checkout lists a Dolt remote `origin`.
- Findings: sessions-05, agent-repo-07, prior-03.

## Acceptance criteria

- [ ] **Baseline table.** For each of (a) `bd hooks run post-checkout` direct,
      (b) `bd hooks run post-merge` direct, (c) `git worktree add <scratch>
      -b <scratch-branch> origin/main` (hook-wrapped), run in both the primary
      checkout and a linked worktree: the exact command, `command -v bd` and
      `bd version`, the working directory, the wall and CPU time (`/usr/bin/time
      -v` or `time`), and the JSONL record count. Attached to the close reason.
- [ ] **Wait attributed.** One traced run (for example `strace -f -tt -e
      trace=network,file,process,futex` or bd's debug logging) identifies where
      the wall time goes, as a breakdown that accounts for at least 80% of it
      (for example: "import loop N s, Dolt server start M s, lock wait K s").
      The trace excerpt is attached.
- [ ] **Import entry points listed.** From bd 1.1.0 source (file:line cited),
      every code path that reads `issues.jsonl` into the DB: which hooks, and
      whether ordinary `bd` commands auto-import when `import.auto` is true.
      For each, the setting that turns it off, and whether that setting is
      stored in a tracked file (`.beads/config.yaml`) or only in the local DB
      (so it would not reach a fresh clone).
- [ ] **Remote visibility explained.** Why linked and preview worktrees report
      "no Dolt remote configured" while the primary lists `origin` (for
      example: remote config stored per database directory), with the command
      output that shows it.
- [ ] **Conclusion stated.** One paragraph: does disabling the import remove
      the wait? If part of the wait is outside the import (for example Dolt
      server start), name it and its measured share, so the implementation
      issue can set its targets from data.
- [ ] No hook file is edited, no `BEADS_HOOK_TIMEOUT` is set, and no hook
      bypass is used for this work (D4).

## Suggested approach

Run in a scratch linked worktree so the primary checkout is not disturbed.
Take the trace on the direct `bd hooks run` form, which has no 300 s cap.

## Out of scope

- Changing any configuration or hook (beads-retire-jsonl-import-dolt-remote).
- The clobber scan (beads-jsonl-import-clobber-check).
- Any hook-timeout env var or hook-bypass guidance (D4).

## Priority / type / labels

P1, task. Labels: agents, beads, git-hooks, landing, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Blocks: beads-retire-jsonl-import-dolt-remote.
