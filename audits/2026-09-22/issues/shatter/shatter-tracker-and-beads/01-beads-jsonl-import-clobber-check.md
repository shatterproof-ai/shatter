---
slug: beads-jsonl-import-clobber-check
kind: new
title: "Beads: check whether the post-checkout JSONL import has overwritten newer tracker state since 2026-09-07, and repair it"
priority: P1
type: bug
labels: [agents, beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: check whether the post-checkout JSONL import has overwritten newer tracker state since 2026-09-07, and repair it

## Problem

Every `git checkout` and `git worktree add` in shatter runs the beads
`post-checkout` hook, which imports `.beads/issues.jsonl` into the live Dolt
database. The committed JSONL has not been re-exported since 2026-09-07
(134dd616). bd 1.1.0 itself calls that file "an export, not cross-machine sync
or source of truth". So for more than two weeks, every checkout has imported a
stale snapshot into a database that is newer than it.

Maintainer decision D4 (2026-09-23) retires the JSONL import in shatter and
moves cross-machine sync to a Dolt remote. Before retiring the import (the
next issue), we must know whether it has already overwritten newer DB state.
Examples would be a closed issue reverted to open, a new priority reverted, or
an edited body reverted. Any damaged issues must be repaired.

Measured root cause (2026-09-23): the hook spends about 6 minutes on
"importing JSONL from .beads/issues.jsonl" (1,773 issues). It uses only about
10 s of CPU, so the time goes to waiting, not computing. This is the same
landing and launch stall reported in sessions-05 and agent-repo-07.

## Evidence (re-verified 2026-09-23 unless noted)

- `git log -1 --format='%h %ci' -- .beads/issues.jsonl` gives
  `134dd616 2026-09-07 21:52:47 -0500`. The file has 1,733 records.
- Live DB: `bd list --all --json --limit 0` returns 1,778 issues. That is 45
  issues missing from the JSONL, all created after 09-07.
- Comparing the live DB with the committed JSONL on the same ids gives 20
  **status** mismatches. In every one, the live DB holds the newer state
  (closed), so the current state shows no surviving clobber:
  str-jttrf, str-qwua7.4, .7, .8, .9, .14, .15, .16, .17, .56, str-8q1b4,
  str-0m0vn, str-leozr, str-rmcrl, str-vr7vq, str-0z1im, str-6vl7p, str-duens,
  str-na9db, str-gjsb2.
- 0 priority mismatches. 12 issues differ in title, description, notes,
  acceptance_criteria or design: str-jttrf, str-qwua7.56, .15, .8, .9,
  str-35vtk.24, str-qwua7.45, .38, .22, str-leozr, str-joyqu, str-hy9b.J3.
- `bd history str-qwua7.8 --limit 8` shows 8 Dolt commits by author `beads`
  that rewrite the same unchanged closed row. Five of them fall within
  2026-09-23 13:06-13:09. This suggests the import writes rows on every
  checkout even when nothing changed. Whether it would also write *older*
  values over newer ones has not been checked; that is this issue's job.
- `bd config show` gives `import.auto = true (default)` and
  `import.path = issues.jsonl (default)`.
- The hook text is `.git/hooks/post-checkout:2-6`, marker
  `BEADS INTEGRATION v0.63.3`, `timeout "${BEADS_HOOK_TIMEOUT:-300}" bd hooks run post-checkout`.
  The installed bd is 1.1.0, and two bd binaries are on PATH
  (`/usr/local/bin/bd`, `~/.local/bin/bd`).
- The prior audit draft `drafts/shatter-agent/05-bd-sync-removed-jsonl-stale.md`
  counted "19 status mismatches"; there are now 20.
  `drafts/shatter-agent/08-beads-hook-timeout-decision.md` has the landing-cost
  data: land.py `create_preview` took 234.9-301.2 s on 11 landings between
  09-19 and 09-22, and the transcripts contain 32 instances of
  "hook 'post-checkout' timed out after 300s".
- Findings: prior-03, sessions-05, agent-repo-07, docs-16 (see
  `audits/2026-09-22/findings.json`).

## Acceptance criteria

- [ ] Documented from bd 1.1.0 docs or source: the merge rule bd's JSONL
      import uses (always overwrite, newer `updated_at` wins, or insert-only),
      and whether it can write older field values over newer ones. Cite the
      source.
- [ ] A script (kept in the issue notes or under `scripts/`) walks the Dolt
      history from 2026-09-07T21:52 to now and lists every issue where status,
      priority, title, description, notes, acceptance_criteria, assignee or
      dependencies went back to the 134dd616 JSONL value after a newer value
      had been written. Its output is attached to the close reason: either the
      list, or "0 regressions found" with the commit range scanned.
- [ ] Every clobbered issue is restored to its last intended state. The
      repaired ids and the restored fields are recorded in the close reason.
- [ ] The 20 status mismatches and 12 text diffs listed above are each
      classified as "live newer (expected)" or "clobbered (repaired)".
- [ ] A backup is taken before any repair (`bd export -o <scratch path>` or a
      Dolt branch/tag), and the close reason names it.

## Suggested approach

1. Stop new damage while investigating: do not create new worktrees or
   checkouts in shatter until beads-retire-jsonl-import-dolt-remote lands. If
   that is impractical, rely on the history scan to catch damage done during
   the investigation.
2. Read bd 1.1.0's import code path (`bd hooks run post-checkout` into the
   import) to learn the merge rule.
3. Use `bd history <id>` for the 1,733 JSONL ids, or query the Dolt database
   under `.beads/dolt` directly with `dolt log` / `dolt diff`. For each
   issue, compare the sequence of row values against the JSONL value.
4. Repair with `bd update` (status, priority, fields) using the last intended
   value from history.

## Out of scope

- Disabling the import and configuring the Dolt remote
  (beads-retire-jsonl-import-dolt-remote).
- Rewriting AGENTS.md, skills and JSONL consumers
  (beads-jsonl-consumers-drop-bd-sync).
- Any `BEADS_HOOK_TIMEOUT` change, hook env block, or hook-bypass guidance.
  D4 rejected these; do not add them.

## Priority / type / labels

P1, bug. Labels: agents, beads, tracker, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. This is the first D4 step.
- Blocks: beads-retire-jsonl-import-dolt-remote.
