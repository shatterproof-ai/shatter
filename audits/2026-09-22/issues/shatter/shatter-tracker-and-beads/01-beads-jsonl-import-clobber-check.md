---
slug: beads-jsonl-import-clobber-check
kind: new
title: "Beads: find and adjudicate tracker rows the post-checkout JSONL import may have reverted since 2026-09-07, and repair confirmed damage"
priority: P1
type: bug
labels: [agents, beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: find and adjudicate tracker rows the post-checkout JSONL import may have reverted since 2026-09-07, and repair confirmed damage

## Problem

Every `git checkout` and `git worktree add` in shatter runs the beads-managed
`post-checkout` hook, which imports `.beads/issues.jsonl` into the live Dolt
database. A beads-managed `post-merge` hook also runs `bd hooks run
post-merge` on every merge; whether it imports too is not yet confirmed
(beads-hook-stall-diagnosis finds out). The committed JSONL has not been re-exported since 2026-09-07
(134dd616). bd 1.1.0 itself calls that file "an export, not cross-machine sync
or source of truth". So for more than two weeks, checkouts have imported a
stale snapshot into a database that is newer than it.

Maintainer decision D4 (2026-09-23) retires the JSONL import in shatter and
moves cross-machine sync to a Dolt remote. Before the import is retired
(beads-retire-jsonl-import-dolt-remote), we need to know whether it has
already written older values over newer DB state (a closed issue reverted to
open, a priority reverted, an edited body reverted), and repair any damage
that is **confirmed**.

A row that returns to its JSONL value is only a *candidate*. A person or agent
may have set that value on purpose. History alone does not prove the "last
intended state", so this issue separates detection from confirmation and
allows an "unresolved" outcome.

## Evidence (re-verified 2026-09-23 unless noted)

- `git log -1 --format='%h %ci' -- .beads/issues.jsonl` gives
  `134dd616 2026-09-07 21:52:47 -0500`. The file has **1,733** records
  (`wc -l .beads/issues.jsonl`).
- Live DB: `bd list --all --json --limit 0` returned **1,778** issues on
  2026-09-23 (45 more than the JSONL, all created after 09-07). The
  audit-time count on 2026-09-22 was 1,773 (`bd stats`,
  `areas/prior-audit-regress.md:82`). Earlier drafts wrongly described 1,773
  as the number of issues the hook imports; the import reads the 1,733-record
  JSONL.
- Comparing the live DB with the committed JSONL on the same ids gives 20
  **status** mismatches. In every one, the live DB holds the newer state
  (closed), so the *current* state shows no surviving status clobber:
  str-jttrf, str-qwua7.4, .7, .8, .9, .14, .15, .16, .17, .56, str-8q1b4,
  str-0m0vn, str-leozr, str-rmcrl, str-vr7vq, str-0z1im, str-6vl7p, str-duens,
  str-na9db, str-gjsb2.
- 0 priority mismatches. 12 issues differ in title, description, notes,
  acceptance_criteria or design: str-jttrf, str-qwua7.56, .15, .8, .9,
  str-35vtk.24, str-qwua7.45, .38, .22, str-leozr, str-joyqu, str-hy9b.J3.
- `bd history str-qwua7.8 --limit 8` shows 8 Dolt commits by author `beads`
  that rewrite the same unchanged closed row; five fall within
  2026-09-23 13:06-13:09. So the import writes rows on checkout even when
  nothing changed. Whether it writes *older* values over newer ones is not
  yet known.
- The hook runs under `timeout "${BEADS_HOOK_TIMEOUT:-300}"`
  (`.git/hooks/post-checkout`, `.git/hooks/post-merge`, marker
  `BEADS INTEGRATION v0.63.3`). The transcripts contain 32 instances of
  "hook 'post-checkout' timed out after 300s", so many imports were killed
  mid-run. A killed import may have left a **partial** write; the scan must
  look for that case too.
- `bd config show` gives `import.auto = true (default)` and
  `import.path = issues.jsonl (default)`. Installed bd is 1.1.0
  (`bd version` → `1.1.0 (8e4e59d39)`); two bd binaries are on PATH
  (`/usr/local/bin/bd`, `~/.local/bin/bd`).
- Findings: prior-03, sessions-05, agent-repo-07, docs-16
  (`audits/2026-09-22/findings.json`). Source drafts:
  `drafts/shatter-agent/05-bd-sync-removed-jsonl-stale.md` (it counted 19
  status mismatches; there are now 20) and
  `drafts/shatter-agent/08-beads-hook-timeout-decision.md`.

## Acceptance criteria

- [ ] **Merge rule documented.** From bd 1.1.0 source or docs (cite file and
      line, or the doc URL), state the rule the JSONL import applies per row
      (always overwrite, newer `updated_at` wins, or insert-only) and whether
      it can write an older field value over a newer one. If the rule cannot
      be determined, say so and treat every candidate below as needing
      adjudication.
- [ ] **Backup first.** Before any repair, a backup exists
      (`bd export -o <scratch path>` or a Dolt branch/tag) and its location is
      in the close reason.
- [ ] **Candidate scan, with a reproducible script.** A script (committed under
      `scripts/` through launch-work/land-work, or attached verbatim to the
      issue notes) walks the Dolt history from 2026-09-07T21:52 to the scan
      time. It lists every issue where status, priority, title, description,
      notes, acceptance_criteria, assignee or dependencies changed **to** the
      134dd616 JSONL value after a different, later value had been written.
      For each candidate it records: issue id, field, the Dolt commit that
      reverted it, that commit's author (`beads` import vs a person/agent),
      its timestamp, and the value before and after. It also lists commits by
      author `beads` that touched only part of the JSONL id set within one
      import window (possible partial imports cut off by the 300 s timeout).
      The script's output and the exact commit range scanned are attached.
- [ ] **Self-test of the scan.** The script is run once against a scratch
      Dolt clone where a row was deliberately updated and then re-imported
      from an older JSONL. It reports that row. The output of that run is
      attached, so "0 candidates" cannot come from a broken scan.
- [ ] **Adjudication.** Every candidate is classified as exactly one of:
      - `intended` — evidence (a comment, close reason, commit message or
        maintainer statement) shows the older value was set on purpose;
      - `import damage` — the reverting commit is an import commit (author
        `beads`, inside a checkout/merge hook window) and no evidence shows
        intent;
      - `unresolved` — attribution is unclear; the maintainer is asked, and
        the item is listed as unresolved in the close reason if no answer
        comes.
      The 20 status mismatches and 12 text diffs listed above are included in
      this classification (as "live newer, no candidate" where the scan finds
      nothing).
- [ ] **Repair only confirmed damage.** Each `import damage` row is restored
      with `bd update` to the value immediately before the reverting import
      commit. The maintainer approves the repair list before it is applied.
      Repaired ids and fields are in the close reason. `unresolved` rows are
      not changed.
- [ ] The close reason records the scan's upper time bound. Damage can keep
      happening until the import is disabled, so
      beads-retire-jsonl-import-dolt-remote re-runs this script from that
      bound to the moment the import is off.

## Suggested approach

1. Read bd 1.1.0's `bd hooks run post-checkout` / `post-merge` code path into
   the import to learn the merge rule and the commit author it uses.
2. Query the Dolt database under `.beads/dolt` with `dolt log` / `dolt diff`
   (or `bd history <id>` for the 1,733 JSONL ids) and compare each row's value
   sequence with the JSONL value.
3. Group candidates by reverting commit; an import commit that reverts many
   rows at once is strong evidence of damage.
4. Present the classification table to the maintainer before repairing.

Damage continues during the investigation, because every landing creates a
preview worktree. Do not try to stop all checkouts; rely on the re-run in
beads-retire-jsonl-import-dolt-remote.

## Out of scope

- Disabling the import and configuring the Dolt remote
  (beads-retire-jsonl-import-dolt-remote).
- Finding what the hook waits on (beads-hook-stall-diagnosis).
- Rewriting AGENTS.md, skills and JSONL consumers
  (beads-jsonl-consumers-drop-bd-sync).
- Any `BEADS_HOOK_TIMEOUT` change, hook env block, or hook-bypass guidance.
  D4 rejected these; do not add them.

## Priority / type / labels

P1, bug. Labels: agents, beads, tracker, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. This is the first D4 step, alongside
  beads-hook-stall-diagnosis.
- Blocks: beads-retire-jsonl-import-dolt-remote.
