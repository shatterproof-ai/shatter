---
slug: beads-retire-jsonl-import-dolt-remote
kind: new
title: "Beads: stop importing .beads/issues.jsonl on every entry point (checkout, merge, auto-import), keep it off for fresh clones, and use the Dolt remote for cross-machine sync"
priority: P1
type: task
labels: [agents, beads, git-hooks, landing, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-jsonl-import-clobber-check, beads-hook-stall-diagnosis]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: stop importing .beads/issues.jsonl on every entry point (checkout, merge, auto-import), keep it off for fresh clones, and use the Dolt remote for cross-machine sync

## Problem

The beads hooks re-import the committed `.beads/issues.jsonl` into the live
Dolt DB on checkout and `git worktree add` (and possibly on merge and on
ordinary bd commands; beads-hook-stall-diagnosis lists every entry point).
That import stalls every landing preview and every new worktree for minutes,
and the file it imports is a stale export (frozen 2026-09-07), not a sync
channel. bd 1.1.0 warns that it is "an export, not cross-machine sync or
source of truth" and suggests `bd dolt remote add origin ... && bd dolt push`.

Maintainer decision D4 (2026-09-23): retire the JSONL import in shatter and
move tracker sync to a Dolt remote. Raising or lowering `BEADS_HOOK_TIMEOUT`,
adding a hook env block and hook-bypass guidance were rejected (str-qwua7.28
and str-mpgg1 are closed as superseded/landed).

## Evidence (re-verified 2026-09-23)

- `/home/ketan/project/shatter/.git/hooks/` (shared by all worktrees) holds
  beads-managed `post-checkout`, `post-merge` and `pre-push` hooks with the
  `BEADS INTEGRATION v0.63.3` block:
  `_bd_timeout=${BEADS_HOOK_TIMEOUT:-300}` then
  `timeout "$_bd_timeout" bd hooks run <hook> "$@"`. `bd version` reports
  `1.1.0 (8e4e59d39)`, so the hook marker is older than the binary.
- `bd config show`: `import.auto = true (default)`,
  `import.path = issues.jsonl`, `export.auto = false`,
  `backup.git-push = true` (from `.beads/config.yaml`), `no-hooks = false`.
- `bd dolt remote list` in the primary checkout shows
  `origin git+https://github.com/shatterproof-ai/shatter.git`, but
  `.beads/push-state.json` reports `"last_push": "2026-04-11T03:44:20Z"` and
  `.beads/export-state.json` a last export on 2026-07-05. Linked and preview
  worktrees logged "post-checkout JSONL import warning: no Dolt remote
  configured" (sessions-05, transcript 19cbf3b5).
- Latency: see the measurement table in beads-hook-stall-diagnosis (direct
  `bd hooks run post-checkout` runs of about 6 min and 2m59.7s; land.py
  `create_preview` 234.9-301.2 s on 11 landings, capped by the 300 s hook
  timeout; 32 "timed out after 300s" messages in transcripts).
- AGENTS.md:370-375 ("Leave the managed git hooks alone") says the hooks
  "hydrate the local DB from JSONL", and allows a transient `core.hooksPath`
  bypass "for a known-hanging rebase/merge". That exception exists only
  because of this stall.
- Findings: sessions-05, agent-repo-07, prior-03
  (`audits/2026-09-22/findings.json`).

## Acceptance criteria

- [ ] **Every entry point off.** For each import entry point listed by
      beads-hook-stall-diagnosis (at least `post-checkout`, `post-merge`, and
      bd command-time auto-import if it exists), the import no longer runs.
      Proof per entry point: run it with bd's verbose/debug output and show
      no "importing JSONL" line, **and** show no new Dolt commit authored by
      the import (`dolt log -n 3` or `bd history` on a row before and after).
      The change is made through bd's own configuration or bd's own
      `bd hooks install` for 1.1.0 (maintainer approves a reinstall first,
      because AGENTS.md reserves the hooks to beads). No hand edit of the
      managed hook blocks; no `BEADS_HOOK_TIMEOUT`.
- [ ] **Survives a fresh clone.** The setting lives in a tracked file (for
      example `.beads/config.yaml`), or, if bd stores it only in the local DB,
      AGENTS.md documents a one-time bootstrap command that a fresh clone must
      run, and `bd doctor` (or a repo script run by `task`) fails when it has
      not been run. Proof: `git clone` into a scratch dir, run the documented
      bootstrap, then `git checkout -b x` and show no import (as above).
- [ ] **Dolt remote works everywhere.** From the primary checkout, a linked
      worktree and a `/tmp/land-work-preview-*`-style worktree:
      `bd dolt remote list` shows the same remote, and `bd dolt pull` and
      `bd dolt push` succeed. If `origin` is the wrong target, the maintainer
      chooses the URL and it is recorded.
- [ ] **Round trip.** A `bd update` in clone A, `bd dolt push`, then
      `bd dolt pull` in clone B shows the change in B (a second clone is fine).
      Commands and output in the close reason.
- [ ] **Latency measured against the diagnosis baseline.** Re-run the same
      commands from beads-hook-stall-diagnosis's baseline table and attach
      before/after. Targets: `git worktree add` in shatter **< 15 s** and one
      real landing's land.py `create_preview` **< 30 s** (quote the land.py
      line), **unless** the diagnosis attributed part of the wait to something
      outside the import; in that case the target is "import share removed",
      and the remaining cause is filed as a new issue linked here.
- [ ] **Clobber window closed.** beads-jsonl-import-clobber-check's scan
      script is re-run from that issue's recorded upper bound to the moment
      the import was disabled; output attached; any new candidates are
      adjudicated the same way.
- [ ] **AGENTS.md** states the sync procedure exactly once (pull at session
      start, push at landing, which command runs where, and the fresh-clone
      bootstrap if one is needed). The "Leave the managed git hooks alone"
      paragraph no longer says the hooks hydrate from JSONL, and drops the
      transient `core.hooksPath` bypass exception. Line-level `bd sync`
      removal elsewhere is done in beads-jsonl-consumers-drop-bd-sync.
- [ ] The docs change lands through launch-work/land-work with `task affected`
      green and its `Gates selected` line in the close reason.

## Suggested approach

1. Use the entry-point list and settings from beads-hook-stall-diagnosis.
   Prefer the narrowest setting that keeps other hook duties
   (`prepare-commit-msg` trailers, pre-push chaining).
2. Fix remote visibility so every worktree resolves the same remote.
3. Decide with the maintainer whether `backup.git-push: true` stays.

## Out of scope

- Checking and repairing past clobbers (beads-jsonl-import-clobber-check).
- Rewriting `bd sync` mentions, CI drift-patrol, and the cleanup script's
  JSONL reads (beads-jsonl-consumers-drop-bd-sync).
- Guidance for other repos (bento beads-dolt-remote-guidance).
- Any hook-timeout env var, hook env block, or hook-bypass guidance (D4).

## Priority / type / labels

P1, task. Labels: agents, beads, git-hooks, landing, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: beads-jsonl-import-clobber-check, beads-hook-stall-diagnosis.
- Blocks: beads-jsonl-consumers-drop-bd-sync.
- Related: bento beads-dolt-remote-guidance, bento git-hook-latency-visibility.
  Supersedes str-qwua7.28 (see qwua7-28-superseded).
