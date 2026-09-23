---
slug: beads-retire-jsonl-import-dolt-remote
kind: new
title: "Beads: stop importing .beads/issues.jsonl on checkout and use the Dolt remote for cross-machine sync (worktree add < 15 s)"
priority: P1
type: task
labels: [agents, beads, git-hooks, landing, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-jsonl-import-clobber-check]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: stop importing .beads/issues.jsonl on checkout and use the Dolt remote for cross-machine sync (worktree add < 15 s)

## Problem

The beads `post-checkout` hook re-imports the committed `.beads/issues.jsonl`
on every checkout and every `git worktree add`. The measured cost on
2026-09-23 was about 6 minutes of "importing JSONL from .beads/issues.jsonl"
for 1,773 issues, with only about 10 s of CPU. The hook's 300 s timeout
usually cuts it off. This puts 4-5 minutes of dead time into every landing
preview and every new worktree. The imported file is also a stale export
(frozen 2026-09-07), not a sync channel. bd 1.1.0 warns that it is "an export,
not cross-machine sync or source of truth" and suggests
`bd dolt remote add origin ... && bd dolt push`.

Maintainer decision D4 (2026-09-23): retire the JSONL import in shatter and
move tracker sync to a Dolt remote. Raising or lowering `BEADS_HOOK_TIMEOUT`
and adding a hook env block were rejected (see str-qwua7.28 and str-mpgg1,
closed as superseded by this issue).

## Evidence (re-verified 2026-09-23)

- `.git/hooks/post-checkout` (shared by all worktrees) contains the
  `BEADS INTEGRATION v0.63.3` block:
  `_bd_timeout=${BEADS_HOOK_TIMEOUT:-300}` followed by
  `timeout "$_bd_timeout" bd hooks run post-checkout "$@"`. `pre-push` has the
  same shape. `bd version` reports `1.1.0 (8e4e59d39)`, so the hook marker is
  older than the binary.
- `bd config show` lists `import.auto = true (default)`,
  `import.path = issues.jsonl`, `export.auto = false`, `backup.git-push = true`
  (from `.beads/config.yaml`), and `no-hooks = false`.
- A Dolt remote already exists in the primary checkout: `bd dolt remote list`
  shows `origin git+https://github.com/shatterproof-ai/shatter.git`. But
  `.beads/push-state.json` reports `"last_push": "2026-04-11T03:44:20Z"`, and
  `.beads/export-state.json` reports a last export on 2026-07-05. Landing
  previews and linked worktrees logged "post-checkout JSONL import warning:
  no Dolt remote configured" (sessions-05, transcript 19cbf3b5). So the remote
  is either not visible from linked worktrees or not in use.
- Landing cost: land.py `create_preview` took 234.9-301.2 s on 11 landings
  (09-19..09-22). Transcripts contain "hook 'post-checkout' timed out after
  300s" 32 times. `time bd hooks run post-checkout` in a linked worktree took
  2m59.7s (agent-repo-07).
- AGENTS.md:370-375 ("Leave the managed git hooks alone") says the hooks
  "hydrate the local DB from JSONL". It also allows a transient
  `core.hooksPath` bypass "for a known-hanging rebase/merge". That exception
  exists only because of this stall.
- Findings: sessions-05, agent-repo-07, prior-03 (`audits/2026-09-22/findings.json`).

## Acceptance criteria

- [ ] The JSONL import no longer runs on checkout in shatter. Use bd's own
      configuration (for example `bd config set import.auto false`, if bd
      1.1.0 documents it as controlling the hook import) or bd's own
      `bd hooks install` for 1.1.0. Do not hand-edit the managed hook blocks
      and do not set `BEADS_HOOK_TIMEOUT`. The exact change is recorded in the
      close reason. If a bd-managed hook reinstall is needed, the maintainer
      approves it first, because AGENTS.md reserves the hooks to beads.
- [ ] The Dolt remote works from the primary checkout, from a linked worktree
      and from a `/tmp/land-work-preview-*` worktree. Proof: `bd dolt push`
      then `bd dolt pull` succeed from each, and `bd dolt remote list` shows
      the same remote. If `origin` is the wrong target, the maintainer chooses
      the URL and it is recorded.
- [ ] Measured and recorded in the close reason:
      `time git worktree add <scratch path> -b <scratch branch> origin/main`
      in shatter finishes in **< 15 s**, and land.py's `create_preview` step
      reports **< 30 s** on one real landing (quote the land.py line).
- [ ] A round trip shows no state is lost: a `bd update` on machine or clone
      A, then `bd dolt push`, then `bd dolt pull` on B, shows the change on B.
      If there is no second machine, use a second clone. The commands and
      output are in the close reason.
- [ ] AGENTS.md states the sync procedure exactly once (pull at session
      start, push at landing, and which command runs where). The
      "Leave the managed git hooks alone" paragraph no longer says the hooks
      hydrate from JSONL, and it drops the transient `core.hooksPath` bypass
      exception. Other docs link to that one place; the consumer rewrite is
      done in beads-jsonl-consumers-drop-bd-sync.

## Suggested approach

1. Confirm from bd 1.1.0 docs or source which setting controls the
   post-checkout import (`import.auto`, `no-hooks`, or hook reinstall).
   Prefer the narrowest one that keeps the other hook duties, such as
   `prepare-commit-msg` trailers and pre-push chaining.
2. Find out why linked worktrees report "no Dolt remote configured" when the
   primary has `origin`. Dolt remote config may live per database directory.
   Fix it so every worktree resolves the same remote.
3. Decide with the maintainer whether `backup.git-push: true` stays.
4. Measure before and after with the same commands.

## Out of scope

- Checking and repairing past clobbers (beads-jsonl-import-clobber-check).
  This issue waits for it.
- Rewriting `bd sync` mentions, CI drift-patrol, and the cleanup script's
  JSONL reads (beads-jsonl-consumers-drop-bd-sync).
- Guidance for other repos (bento `beads-issue-flow`, filed in the bento
  bucket as beads-dolt-remote-guidance).
- Any hook-timeout env var, hook env block, or hook-bypass guidance (D4).

## Priority / type / labels

P1, task. Labels: agents, beads, git-hooks, landing, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: beads-jsonl-import-clobber-check.
- Blocks: beads-jsonl-consumers-drop-bd-sync.
- Related: bento beads-dolt-remote-guidance, bento git-hook-latency-visibility.
  Supersedes str-qwua7.28 (see qwua7-28-superseded).
