# Bundle: shatter-tracker-and-beads (repo: shatter)

Audit 2026-09-22, final issue drafts, revised 2026-09-23 after the Codex
cross-check (see REVISION.md). Nothing has been filed.

**Theme:** Tracker truth. Retire the JSONL import and move to a Dolt remote
(D4), reconcile stale issues, set a triage policy, publish audit reports, and
give the downstream coverage goals an owner.

**Tracker:** bd in /home/ketan/project/shatter (prefix str).
**Parent epic:** Epic: Audit 2026-09-22 findings.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and
  aarch64-unknown-linux-gnu in the release matrix, and fix them. Release work
  closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot `shatter diff` command and the
  unused Snapshot writer. spec-diff is the regression tool. The `diff` name
  becomes free; str-81xiw decides whether to take it.
- **D3 Concolic:** measure first. P1 benchmark comparing default and concolic
  modes, plus a P1 fix for early termination. A follow-up decision issue
  re-decides the positioning. No doc softening now.
- **D4 Beads:** a direct `bd -v hooks run post-checkout` spent about 6 minutes
  importing the stale `.beads/issues.jsonl` (the file has 1,733 records; the
  live DB had 1,773 issues at audit time) with about 10 s of CPU. Retire the
  JSONL import in shatter and sync through a Dolt remote:
  - First verify whether the import clobbered newer DB state.
  - AGENTS.md drops `bd sync`.
  - str-qwua7.28 is superseded.
  - bento `beads-issue-flow` gets matching guidance.
  - No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed.
  Add a `.mailmap`, a git-state check (via str-qwua7.1) and a fixture config
  snapshot.
- **D6 Filing:** after reconciliation and the Codex cross-check, the
  maintainer runs one filer script. Agents file nothing.

## Filing bootstrap (maintainer; resolves the filing-order cycle)

1. Now, no issue needed: in /home/ketan/project/shatter run
   `git branch audit-2026-09-04-recovered e067979d` (and, if wanted,
   `git branch audit-2026-06-09-recovered f9dad247`) so `git gc` cannot drop
   the deleted audit branches' commits.
2. File only the shatter epic and `publish-audit-reports` (filer with
   `ONLY=shatter` and `HOLD` = every other shatter slug, or by hand plus a
   ledger row).
3. Work `publish-audit-reports` to completion: the 2026-09-22 report and
   evidence land on origin/main. It needs no other audit issue (no Dolt
   procedure, no `bd sync`).
4. Run the bulk filer.

## Contents

| NN | Slug | Kind | Target | P | Blocked by |
|---|---|---|---|---|---|
| 01 | beads-jsonl-import-clobber-check | new | - | P1 | - |
| 02 | beads-retire-jsonl-import-dolt-remote | new | - | P1 | 01, 12 |
| 03 | beads-jsonl-consumers-drop-bd-sync | new | - | P1 | 02 |
| 04 | qwua7-28-superseded | note-to-existing | str-qwua7.28 | P2 | - |
| 05 | ly5bz-superseded | note-to-existing | str-ly5bz | P3 | - |
| 06 | mpgg1-close | note-to-existing | str-mpgg1 | P2 | - |
| 07 | publish-audit-reports | new | - | P1 | - |
| 08 | tracker-reconciliation-sweep | new | - | P2 | - |
| 09 | triage-policy-and-audit-epic-waves | new | - | P2 | - |
| 10 | downstream-coverage-goals-epic | new | - | P2 | - |
| 11 | qwua7-17-drift-patrol-hygiene | note-to-existing | str-qwua7.17 | P3 | - |
| 12 | beads-hook-stall-diagnosis | new | - | P1 | - |
| 13 | audit-2026-09-04-report-recovery | new | - | P1 | - |
| 14 | qwua7-22-audit-land-before-file-note | note-to-existing | str-qwua7.22 | P2 | - |
| 15 | audit-branch-deletion-cause | new | - | P2 | - |
| 16 | qwua7-12-rescope-note | note-to-existing | str-qwua7.12 | P1 | - |
| 17 | landed-not-closed-patrol-check | new | - | P2 | 08, 03 |
| 18 | triage-drift-patrol-checks | new | - | P2 | 09, 03 |

Ordering notes without dep edges: post `qwua7-1-git-state-check` (bucket
shatter-agent-guidance-and-repo-hygiene) before working 08; `mpgg1-close` (06)
and 08 must not both close str-mpgg1.

## Reviewer attention (re-verified 2026-09-23)

- **The `audit-2026-09-04` branch has been deleted.** Its 7 commits (tip
  `e067979d`, 97 files under `audits/2026-09-04/` plus the report) are
  unreachable and will be lost at the next gc prune. The 2026-06-09 audit
  commits (`f9dad247` and others) are also unreachable. Bootstrap step 1
  preserves them.
- **str-qwua7.12 must not be closed** (changed from the first draft): its
  multi-target partial-failure acceptance check (exit 1) contradicts
  `decide_exit_status_ok_partial_success_with_some_failed_targets`
  (`shatter-cli/src/commands/explore.rs:7075`). Draft 16 re-scopes it. Bucket
  shatter-cli-flags-and-help's `cli-minor-output-and-help-polish` item 11
  still proposes closing it and should be reconciled.
- shatter has beads-managed `post-checkout` **and** `post-merge` hooks; draft
  02 now requires every import entry point to be off, and draft 12 lists them
  first.
- The primary checkout already has a Dolt remote, `origin`
  (`git+https://github.com/shatterproof-ai/shatter.git`), but its last push
  was 2026-04-11, and preview worktrees report "no Dolt remote configured".
- JSONL vs live DB: 1,733 vs 1,778 issues, 20 status mismatches, 0 priority
  mismatches; in every mismatch the live DB is newer. Transient clobbers need
  the Dolt-history scan (draft 01), which now separates candidates from
  confirmed damage.
- str-8q1b4 was closed on 2026-09-22. str-qwua7.17 has been closed since
  2026-09-08, so draft 11 is a comment on a closed issue.
- Five merged remote branches (rmcrl, vr7vq, 0z1im, 6vl7p, 8q1b4) are still
  protected by cleanup-merged-remote-branches.sh, because the stale JSONL
  lists their issues as in_progress.


---

<!-- file: 01-beads-jsonl-import-clobber-check.md -->

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

---

<!-- file: 02-beads-retire-jsonl-import-dolt-remote.md -->

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

---

<!-- file: 03-beads-jsonl-consumers-drop-bd-sync.md -->

---
slug: beads-jsonl-consumers-drop-bd-sync
kind: new
title: "Beads: remove every `bd sync` instruction from docs and skills, and stop CI drift-patrol and branch cleanup from trusting the stale issues.jsonl"
priority: P1
type: task
labels: [agents, beads, docs, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: remove every `bd sync` instruction from docs and skills, and stop CI drift-patrol and branch cleanup from trusting the stale issues.jsonl

## Problem

AGENTS.md and the repo skills require `bd sync`, but that command does not
exist in the installed bd 1.1.0. Two automated consumers still treat the
committed `.beads/issues.jsonl` as the truth, although it is a stale export
frozen at 2026-09-07:

- CI drift-patrol tracker-hygiene;
- `scripts/cleanup-merged-remote-branches.sh`, which reads its "in-progress,
  do not delete" set from it.

Maintainer decision D4 (2026-09-23) retires the JSONL import and makes a Dolt
remote the sync channel (beads-retire-jsonl-import-dolt-remote). This issue
brings the docs, skills and scripts in line with that decision.

**Ownership.** This issue owns the removal of **every** line-level `bd sync`
instruction, including the ones inside the AGENTS.md landing prose (`:125`)
and the "Beads Sync Cadence" section (`:345-369`). The broader rewrite of the
landing sections into a pointer to `bento:land-work` stays with str-qwua7.23
(see the qwua7-23-agents-md-rtk-and-landing note, which already assigns the
line-level removal here). No ordering between the two is needed: whichever
lands second rebases over the other.

## Evidence (re-verified 2026-09-23 in the audit-2026-09-22 worktree)

- `bd version` reports 1.1.0. `bd sync --help` fails with
  `Error: unknown command "sync" for "bd"`. bd warns that two binaries are on
  PATH (`/usr/local/bin/bd`, `~/.local/bin/bd`).
- `bd sync` instructions:
  - AGENTS.md:125 (landing step 5: "`bd sync` once here")
  - AGENTS.md:293 ("`bd sync` intentionally commits `.beads/*.jsonl`")
  - AGENTS.md:340 (commit-message convention `bd sync: close ...`)
  - AGENTS.md:345-369 (the whole "Beads Sync Cadence" section)
  - `.claude/skills/audit/SKILL.md:385` ("handled by `bd sync`")
- AGENTS.md:7 and :19 deliberately name nonexistent commands (`bd claim`,
  `bd assign --self`, `bd start`) to forbid them. Any command check must not
  flag these negative examples.
- `.beads/PRIME.md:8` gives a different procedure ("`bd dolt pull` →
  `git commit` → `git push`").
- `scripts/drift-patrol.py:188-221` falls back to the committed
  `.beads/issues.jsonl` when bd is not on PATH, which is the case in CI.
  `docs/DRIFT-PATROL.md:53-55` documents this.
- `scripts/cleanup-merged-remote-branches.sh`:
  - `:105-133` reads the in-progress set from the tracked JSONL, then
    (`:135-150`) optionally supplements it with a bulk
    `bd list --status in_progress`;
  - `:157-183` and `:244-273`: before each delete under `--execute`, a fresh
    per-branch `bd show <id>` re-check protects a branch whose issue is
    in_progress or assigned (fail closed); if bd is not on PATH this layer is
    skipped with a WARNING (`:249`);
  - `--skip-bd` (`:13-17`, `:52`) skips both the in-progress set and the
    per-branch re-check.
  - Tests: `scripts/test_cleanup_merged_remote_branches.sh`.
  AGENTS.md:274-277 documents the JSONL source. Concrete effect today: the
  JSONL lists str-rmcrl, str-vr7vq, str-0z1im, str-6vl7p and str-8q1b4 as
  `in_progress`, but all are closed in the live DB, so their merged remote
  branches (`origin/str-rmcrl-clippy-const-assert`,
  `origin/str-vr7vq-shatter-init-gitignore`,
  `origin/str-0z1im-negzero-roundtrip`,
  `origin/str-6vl7p-redundant-canonicalize`,
  `origin/str-8q1b4-resume-report-parity`) are still protected.
- JSONL vs live DB: 1,733 vs 1,778 issues and 20 status mismatches (details
  in beads-jsonl-import-clobber-check).
- `.beads/issues.recovered.jsonl` is tracked (1,201 lines) with no
  explanation; last touched by 29295bf7 (2026-05-24, "bd sync: hide jsonl
  export from auto import").
- Findings: prior-03 (P1), agent-repo-06 (the verifier set P2; D4 folds it
  into the P1 retirement), docs-16. Source draft:
  `drafts/shatter-agent/05-bd-sync-removed-jsonl-stale.md`; its "keep or
  untrack" decision is replaced by D4.

## Acceptance criteria

- [ ] `grep -rn "bd sync" AGENTS.md CLAUDE.md .beads/PRIME.md .claude/ docs/ scripts/`
      returns only (a) historical audit reports under `audits/` or
      `docs/audits/`, (b) changelog entries, and (c) mentions that explicitly
      say `bd sync` does not exist. The command and its output are in the
      close reason. Every former instruction links to the one sync procedure
      that beads-retire-jsonl-import-dolt-remote wrote into AGENTS.md;
      PRIME.md's session-close line matches it.
- [ ] The `bd sync: close <id>` commit convention and the "one `bd sync`
      commit per landing" cadence text are removed or replaced by the
      Dolt-remote equivalent.
- [ ] **Cleanup script** (`scripts/cleanup-merged-remote-branches.sh`):
      - The in-progress set comes from live bd after `bd dolt pull`; the
        tracked-JSONL read is removed.
      - `bd dolt pull` fails but local bd answers: dry-run lists candidates
        marked "unverified (pull failed)"; `--execute` deletes nothing and
        exits non-zero with the reason.
      - bd not on PATH or not answering: dry-run lists candidates as
        unverified; `--execute` deletes nothing and exits non-zero. It never
        falls back to the JSONL.
      - `--skip-bd` is kept only for dry-run; `--skip-bd --execute` is
        rejected with a usage error (maintainer may override this in the
        issue before work starts; record the choice).
      - The per-branch live `bd show` re-check before each delete is kept.
      - `scripts/test_cleanup_merged_remote_branches.sh` gains cases with a
        stubbed `bd` for: pull fails; bd missing; `--skip-bd --execute`
        rejected; and **a branch whose issue is claimed after the bulk list
        but before its delete is not deleted**. Each new case is shown
        failing on the pre-change script (paste the red run) and passing
        after.
- [ ] **Drift-patrol** (`scripts/drift-patrol.py`): tracker checks read live
      bd (after `bd dolt pull`) where bd is available. Where bd is absent (CI)
      they return the existing `SKIP` status with a reason, never a PASS/FAIL
      computed from the JSONL. `docs/DRIFT-PATROL.md:53-55` is updated.
      `scripts/test_drift_patrol.py` has a case with bd absent asserting SKIP.
      `python3 scripts/drift-patrol.py --only tracker-hygiene` output from a
      bd-less environment (for example `PATH` without bd) is in the close
      reason.
- [ ] **Recorded maintainer decision:** `.beads/issues.jsonl` stays tracked as
      a plain export (who refreshes it; documented as "not read by anything"),
      or it is untracked. The chosen state is applied.
- [ ] `.beads/issues.recovered.jsonl` is deleted, or AGENTS.md explains why it
      is kept.
- [ ] **bd command lint.** A check in `docs-smoke` (or a meta test) extracts
      `bd <subcommand>` invocations **only from fenced `bash`/`sh` code blocks
      and inline code spans that begin a command line** in AGENTS.md,
      PRIME.md and `.claude/skills/**`, and compares the subcommand against a
      checked-in list generated from `bd help` at bd 1.1.x (so CI needs no bd
      binary). Lines that name a command to forbid it are exempt via an
      explicit marker (for example `<!-- bd-lint: forbidden-example -->`) or
      because they are prose, not code. Tests: a fixture with `bd sync` in a
      code block fails; the AGENTS.md:7/:19 negative examples pass; a
      fixture using a real subcommand passes.
- [ ] AGENTS.md "bd Quick Reference" states the expected bd version (1.1.x)
      and which binary on PATH is canonical.
- [ ] `task affected` is green, with the `Gates selected` line in the close
      reason.

## Suggested approach

Doc rewrite in one commit; the cleanup script, drift-patrol change and bd
lint, each with its tests, in separate commits. Coordinate with
drift-patrol-workflow-go-mod (bucket shatter-ci-workflows), which fixes the
drift-patrol workflow that is the CI consumer.

## Out of scope

- Disabling the import and configuring the remote
  (beads-retire-jsonl-import-dolt-remote).
- bento's generic `beads-issue-flow` guidance (bento beads-dolt-remote-guidance).
- Replacing the AGENTS.md landing sections with a land-work pointer
  (str-qwua7.23). This issue removes only the `bd sync` lines in them.
- The audit skill's landing-order rewrite (str-qwua7.22; see
  qwua7-22-audit-land-before-file-note). This issue only removes the
  `bd sync` sentence at SKILL.md:385.
- Any hook-timeout env var or hook-bypass guidance (D4).

## Priority / type / labels

P1, task. Labels: agents, beads, docs, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: beads-retire-jsonl-import-dolt-remote.
- Related: drift-patrol-workflow-go-mod, qwua7-23-agents-md-rtk-and-landing,
  qwua7-22-audit-land-before-file-note. Supersedes str-ly5bz (see
  ly5bz-superseded).

---

<!-- file: 04-qwua7-28-superseded.md -->

---
slug: qwua7-28-superseded
kind: note-to-existing
title: "Close str-qwua7.28 as superseded: JSONL import retired (D4), no BEADS_HOOK_TIMEOUT change"
priority: P2
type: task
labels: [agents, beads, git-hooks, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.28
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-qwua7.28 as superseded: JSONL import retired (D4), no BEADS_HOOK_TIMEOUT change

**Target:** `str-qwua7.28` (open, P2, "Install the BEADS_HOOK_TIMEOUT env
section setup-hooks.sh already defines (raise to 60 s), ...")

**Action:** post the comment below, then run
`bd close str-qwua7.28 --reason "Superseded by <beads-retire-jsonl-import-dolt-remote id> (audit 2026-09-22, maintainer decision D4): the post-checkout stall is the JSONL import, not the timeout value."`
The filer substitutes the real id for the slug placeholder.

## Comment text

**Audit 2026-09-22 note: superseded (maintainer decision D4, 2026-09-23)**

This issue will not be implemented. The maintainer has decided not to change
`BEADS_HOOK_TIMEOUT` and not to add any hook env block.

Measured 2026-09-23: a direct, unwrapped `bd -v hooks run post-checkout`
spent about 6 minutes in "importing JSONL from .beads/issues.jsonl" (the file
has 1,733 records) while using about 10 s of CPU. Hook-wrapped runs are cut
off by the 300 s timeout (land.py `create_preview` 234.9-301.2 s on 11
landings). What the import waits on is being pinned down by
<beads-hook-stall-diagnosis>. The imported file is a stale
export, last committed at 134dd616 on 2026-09-07, and bd 1.1.0 itself calls
it "an export, not cross-machine sync or source of truth". Tuning the timeout
only changes how much of that wait is cut off. It does not stop a stale
snapshot from being imported into a newer database.

Replacement work:
- <beads-jsonl-import-clobber-check>: checks whether the import has
  overwritten newer DB state, and repairs any damage.
- <beads-hook-stall-diagnosis>: finds what the hook waits on, lists every
  import entry point, and records one latency baseline.
- <beads-retire-jsonl-import-dolt-remote>: stops the import on every entry
  point through bd configuration, makes the Dolt remote the sync channel, and
  measures `git worktree add` and land.py `create_preview` against the
  baseline. This issue's post-checkout smoke-test idea lives there.
- <beads-jsonl-consumers-drop-bd-sync>: docs, skills, CI and the cleanup
  script.

Stale fact in this issue's body: it says `scripts/setup-hooks.sh:41` already
defines the BEADS_HOOK_TIMEOUT section. That has been false since b5cd25ec
(str-mpgg1, merged in 84941b37 on 2026-09-02).
`grep -rn BEADS_HOOK_TIMEOUT scripts/ .beads/hooks Taskfile.yml` finds
nothing. The only occurrence is the beads-managed default
`${BEADS_HOOK_TIMEOUT:-300}` in `.git/hooks/post-checkout` and `pre-push`.

Evidence: audit findings sessions-05 and agent-repo-07
(`audits/2026-09-22/findings.json`).

---

<!-- file: 05-ly5bz-superseded.md -->

---
slug: ly5bz-superseded
kind: note-to-existing
title: "Close str-ly5bz as superseded: bd sync and the JSONL import are retired (D4)"
priority: P3
type: task
labels: [agents, beads, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-ly5bz
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-ly5bz as superseded: bd sync and the JSONL import are retired (D4)

**Target:** `str-ly5bz` (open, P3, "AGENTS.md: bd sync-once-at-landing
cadence narrows the merged-branch-cleanup JSONL safety window")

**Action:** post the comment below, then run
`bd close str-ly5bz --reason "Superseded by <beads-jsonl-consumers-drop-bd-sync id> (audit 2026-09-22, D4): bd sync no longer exists and the cleanup script stops reading the JSONL."`

## Comment text

**Audit 2026-09-22 note: superseded (maintainer decision D4, 2026-09-23)**

The cadence question here is moot, for two reasons:

- `bd sync` does not exist in the installed bd 1.1.0: `bd sync --help`
  reports `unknown command "sync"`.
- Under D4, shatter stops importing `.beads/issues.jsonl` and syncs through a
  Dolt remote instead.

The committed JSONL has been frozen since 134dd616 (2026-09-07). So the "JSONL
safety window" this issue worried about has grown to more than two weeks.
`scripts/cleanup-merged-remote-branches.sh` is currently protecting five
merged branches whose issues are closed (str-rmcrl, str-vr7vq, str-0z1im,
str-6vl7p, str-8q1b4), because the stale JSONL still lists them as
`in_progress`.

Replacement: <beads-jsonl-consumers-drop-bd-sync>. It removes every `bd sync`
mention and makes the cleanup script read live bd after `bd dolt pull`,
refusing to delete when bd is unreachable. It also makes CI drift-patrol SKIP
instead of reading the JSONL.

Evidence: audit findings prior-03 and agent-repo-06
(`audits/2026-09-22/findings.json`).

---

<!-- file: 06-mpgg1-close.md -->

---
slug: mpgg1-close
kind: note-to-existing
title: "Close str-mpgg1: revert landed in 84941b37; hook-env edits stay reverted, D4 replaces the timeout debate"
priority: P2
type: task
labels: [agents, beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-mpgg1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Close str-mpgg1: revert landed in 84941b37; hook-env edits stay reverted, D4 replaces the timeout debate

**Target:** `str-mpgg1` (in_progress, P1, "Revert unauthorized beads-hook
edits from str-35vtk.4"; last updated 2026-09-01)

**Action:** post the comment below, then run
`bd close str-mpgg1 --reason "Landed: 84941b37 (merge of str-mpgg1-revert-hook-edits) is an ancestor of origin/main; hook-env edits stay reverted per audit 2026-09-22 D4."`
If tracker-reconciliation-sweep is filed in the same run, that sweep can do
this close. Do not do it twice.

## Comment text

**Audit 2026-09-22 note: closing as landed**

`git merge-base --is-ancestor 84941b37 origin/main` succeeds (re-checked
2026-09-23), so the revert has been on main since 2026-09-02. The issue has
stayed `in_progress` for 21 days, and drift-patrol tracker-hygiene flags it
as a stale claim (`audits/2026-09-22/gates/drift-patrol.log:32-34`).

Maintainer decision D4 (2026-09-23): the hook-env edits stay reverted. No
`BEADS_HOOK_TIMEOUT` env line or managed env block will be added to the hooks.
The timeout debate between this issue and str-qwua7.28 is replaced by fixing
the root cause, which is the post-checkout JSONL import. See
<beads-retire-jsonl-import-dolt-remote>; str-qwua7.28 is being closed as
superseded.

Evidence: agent-repo-07, prior-10 (`audits/2026-09-22/findings.json`).

---

<!-- file: 07-publish-audit-reports.md -->

---
slug: publish-audit-reports
kind: new
title: "Land the 2026-09-22 audit report and evidence on main before the bulk issue filer runs (filing bootstrap)"
priority: P1
type: task
labels: [agents, audit, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Land the 2026-09-22 audit report and evidence on main before the bulk issue filer runs (filing bootstrap)

## Problem

Every issue drafted from the 2026-09-22 audit cites evidence under
`audits/2026-09-22/` (`findings.json`, `areas/`, `gates/`, `sessions/`). That
evidence exists only on the local, unpushed branch `audit-2026-09-22`. If the
issues are filed first, a fresh agent that picks one up cannot read its
evidence. Past audits were lost this way: the 2026-07-10 report was never
committed (str-sff87), and the 2026-09-04 and 2026-06-09 audit branches have
been deleted (their recovery is audit-2026-09-04-report-recovery).

This issue is the **bootstrap** step of filing. It must not depend on any
other unfiled audit issue. In particular it needs no Dolt-remote procedure:
landing a report uses land-work exactly as it works today, and makes no
tracker-sync step.

## Filing bootstrap (for the maintainer; D6: agents file nothing)

1. **Now, before any filing, no issue needed:** preserve the unreachable audit
   commits in `/home/ketan/project/shatter` so `git gc` cannot drop them:
   `git branch audit-2026-09-04-recovered e067979d` and, if wanted,
   `git branch audit-2026-06-09-recovered f9dad247`.
2. **File only this issue** (and the shatter audit epic): run the filer with
   `ONLY=shatter` and `HOLD` set to every other shatter slug, so the ledger
   records `epic:shatter` and `publish-audit-reports`. (Or create it by hand
   and add its ledger row, so the bulk run skips it.)
3. **Work this issue** to completion (below).
4. **Run the bulk filer** for everything else. Its evidence paths now resolve
   on origin/main.

## Evidence (re-verified 2026-09-23)

- `git ls-remote --heads origin 'audit*'` returns only
  `audit-kapow-2026-05-24`; `audit-2026-09-22` is unpushed. On 2026-09-23 it
  had 5 commits over origin/main (first 56c86168) and 850 tracked files under
  `audits/2026-09-22/`, of which 16 are session-transcript files under
  `audits/2026-09-22/sessions/` and 343 are issue drafts under
  `audits/2026-09-22/issues/`. The untracked `goals-runs/` is about 255 MB.
- `git ls-tree origin/main -- audits/` lists only `2026-02-28.md`,
  `2026-05-21.md` and `kapow-2026-05-21`.
- str-qwua7.44 (open, P2, docs IA) decides that root `audits/` merges into
  `docs/audits/` **after** the in-flight audit branch lands at `audits/`, and
  that `docs/INDEX.md` gains an "Audits" section. So the landing path is
  `audits/2026-09-22.md` + `audits/2026-09-22/`, and the INDEX section stays
  with .44.
- Findings: agent-repo-04 (verified P1), docs-04. Source draft:
  `drafts/shatter-agent/06-publish-audit-reports-and-audit-skill-landing.md`.

## Acceptance criteria

- [ ] **Privacy review done first.** The maintainer reviews the 16
      `audits/2026-09-22/sessions/` files (and any other transcript excerpt)
      for content that should not be on a public main; each file is kept,
      redacted or dropped, and the list with the decision is in the close
      reason.
- [ ] `audits/2026-09-22.md` and the tracked `audits/2026-09-22/` evidence
      land on origin/main through launch-work/land-work. `goals-runs/` stays
      out. Whether `audits/2026-09-22/issues/` (drafts, filer, ledger) lands
      too is the maintainer's call, recorded in the close reason.
- [ ] **Evidence paths resolve.** A script extracts every
      `audits/2026-09-22/...` path cited in `audits/2026-09-22/issues/**/*.md`
      and runs `git cat-file -e origin/main:<path>` on each (fragments like
      `:NN` stripped). The output shows 0 missing paths and is in the close
      reason. The script is kept (for example under `scripts/`) so the audit
      skill can reuse it.
- [ ] The landing does not run or document `bd sync` and does not commit
      `.beads/*.jsonl`.
- [ ] This issue is closed **before** the bulk filer run; the close reason
      gives the origin/main SHA that contains the report.

## Out of scope

- Recovering and landing the 2026-09-04 report
  (audit-2026-09-04-report-recovery).
- Why the audit branches were deleted (audit-branch-deletion-cause).
- The `/audit` skill's post-audit order (str-qwua7.22; see
  qwua7-22-audit-land-before-file-note).
- The `docs/INDEX.md` "Audits" section and any move to `docs/audits/`
  (str-qwua7.44).
- Filing the 2026-09-22 issues (D6: the maintainer's filer).

## Priority / type / labels

P1, task. Labels: agents, audit, docs, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. It is filed and closed before the bulk filer run.
- Related: str-qwua7.44, str-qwua7.22, str-sff87 (closed),
  audit-2026-09-04-report-recovery.

---

<!-- file: 08-tracker-reconciliation-sweep.md -->

---
slug: tracker-reconciliation-sweep
kind: new
title: "Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, drop stale blocking edges, and re-home orphans"
priority: P2
type: chore
labels: [agents, beads, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, drop stale blocking edges, and re-home orphans

## Problem

The tracker lags reality:

- Issues whose premise no longer holds stay open, and their stale blocking
  edges keep other issues out of `bd ready`.
- Duplicates stay open.
- Open children sit under closed parents.

This issue is **tracker-only** work: closes, edge removals and re-parents,
each with evidence. It needs no branch. The recurring detector (a
landed-not-closed drift-patrol check) is a separate code change:
landed-not-closed-patrol-check.

## Ordering (no dependency edge)

Post the qwua7-1-git-state-check note (bucket
shatter-agent-guidance-and-repo-hygiene) on str-qwua7.1 before working this
issue, so that .1 is treated as re-scoped rather than closed. This is an
ordering note only; there is deliberately **no** blocked-by edge, because the
filer would resolve that note slug to str-qwua7.1 itself and block this sweep
on the still-open git-state-check work.

## Evidence (re-verified 2026-09-23 with `bd show`; verify each again before acting)

| Issue | State now | Evidence | Action |
|---|---|---|---|
| str-qwua7.1 (P1, "Repair primary checkout (core.bare=true) and add a git-state hygiene check") | open | `git -C /home/ketan/project/shatter config core.bare` → `false` | **Do not close.** Only remove its stale `blocks` edges to .18 and .19. |
| str-qwua7.18 (P1, "Land the six parked worktree branches in order; close str-duens and remove its worktree") | open; `bd dep list` shows `str-qwua7.1 ... via blocks` | str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq, str-8q1b4 and str-duens are closed in the live DB; `git worktree list` shows no duens or 8q1b4 worktree | Close. The five merged remote branches (`origin/str-{0z1im,6vl7p,8q1b4,rmcrl,vr7vq}-*`) are left to cleanup-merged-remote-branches.sh once beads-jsonl-consumers-drop-bd-sync fixes its in-progress source. |
| str-qwua7.19 (P1, "Push local main ... and prune the land-work preview worktrees") | open, blocked by str-qwua7.1 | main == origin/main; `git worktree list \| grep -c land-work-preview` → 0 | Close. |
| str-mpgg1 | in_progress since 2026-09-01 | 84941b37 is an ancestor of origin/main | Closed by mpgg1-close; do not close twice. |
| str-qe9pp (P2, "Rust frontend parity/conformance drift: golden + prepare timeout") | open, last updated 2026-07-06 | golden updated in 750b7ff8 (2026-06-18); prepare timeout fixed by str-uoclg (closed 2026-08-13) | Re-run the check the issue names (Rust conformance/parity gate) on current main; close with that output plus both citations if green, otherwise comment with the failure. |
| str-uj3y (P2, "build-frontend rust should vendor shatter-rust-runtime beside the custom binary ...") | open | Duplicates str-da35 (closed, landed 4182e51e; `build_frontend.rs` vendors the runtime) | Close as a duplicate of str-da35 after confirming the vendoring code is on origin/main (`git grep` output in the reason). |
| Orphans: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1 | open under closed parents | `audits/2026-09-22/gates/drift-patrol.log:35-39` | Re-parent each under a live epic, or close it, with a reason. |

**Not in this sweep:**

- **str-qwua7.12** (SPEC §2.11 exit codes) is **not** closed. Single-target
  error classes now exit 2, but its multi-target acceptance check (exit 1 on
  partial failure) contradicts current code and its test
  `decide_exit_status_ok_partial_success_with_some_failed_targets`
  (`shatter-cli/src/commands/explore.rs:7075`). It gets a re-scoping note
  instead: qwua7-12-rescope-note.
- **str-1fik / str-wfd2, str-hrg2 / str-0wxw, str-2zsy, str-cl53,
  str-u394l.4:** these are str-qwua7.62's approved 2026-09-06 decisions and
  stay with str-qwua7.62. The audit reviewer (frontend-rust-10) notes that
  wfd2 is the cross-crate item and same-crate synthesis was delivered by
  str-do53; whoever executes .62 should give the survivor a body saying
  "same-crate resolved (str-do53); cross-crate still Opaque".
- str-8q1b4 was closed on 2026-09-22 and needs nothing.

Findings: agent-repo-05, sessions-09, prior-09, protocol-parity-16,
frontend-rust-10 (tracker part). Source draft:
`drafts/shatter-agent/10-tracker-reconciliation-sweep.md`.

## Acceptance criteria

- [ ] Every row in the table ends closed, re-scoped (edges removed) or
      re-parented, or carries a comment explaining why not. Each close reason
      cites its evidence (command plus output, or a SHA). No bare "Closed"
      reasons.
- [ ] `bd dep list str-qwua7.18` and `bd dep list str-qwua7.19` no longer
      show `str-qwua7.1`, or both issues are closed; the output is in the
      close reason.
- [ ] `bd show str-qwua7.12` is still open after this sweep.
- [ ] `python3 scripts/drift-patrol.py --only tracker-hygiene`, run against
      live bd after the sweep, shows no orphan from the table and no stale
      claim except ones with a documented reason; output in the close reason.

## Out of scope

- The landed-not-closed patrol check (landed-not-closed-patrol-check).
- The priority rubric, P1 cap and epic waves (triage-policy-and-audit-epic-waves).
- Executing str-qwua7.62's decisions.
- Deleting remote branches (cleanup script, after beads-jsonl-consumers-drop-bd-sync).
- Re-verifying str-qwua7.14 (fixture-corruption-incident-reverify).

## Priority / type / labels

P2, chore. Labels: agents, beads, governance, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none (ordering note above: post qwua7-1-git-state-check first).
- Related: mpgg1-close, qwua7-12-rescope-note, qwua7-17-drift-patrol-hygiene,
  landed-not-closed-patrol-check, str-qwua7.62,
  beads-jsonl-consumers-drop-bd-sync.

---

<!-- file: 09-triage-policy-and-audit-epic-waves.md -->

---
slug: triage-policy-and-audit-epic-waves
kind: new
title: "Define P1, cap open P1s, fix priority inversions, and split and wave-order the stalled str-qwua7 audit epic"
priority: P2
type: task
labels: [agents, governance, audit, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Define P1, cap open P1s, fix priority inversions, and split and wave-order the stalled str-qwua7 audit epic

## Problem

Priority has lost its meaning, and audit follow-through stalls.

- About a quarter of open issues are P1, and the top of `bd ready` is all P1.
- Some higher-priority issues are blocked by lower-priority ones (priority
  inversions).
- The 2026-09-04 audit epic str-qwua7 has 12 of 62 direct children closed
  after 19 days, all of them small. No structural P1 child has a comment,
  branch or commit.
- Meanwhile a new feature epic (str-hjrnp) was created and finished on
  09-21/22.

AGENTS.md has no priority rubric, and nothing orders audit follow-through.

## Evidence

Re-counted 2026-09-23 from `bd list --all --json --limit 0` unless noted.

- Open issues by priority: P1 45, P2 112, P3 23, P4 1 (181 open;
  182 open or in_progress). `bd ready --json --limit 0` returns 157 issues.
  The audit counted 48 P1 on 2026-09-22.
- str-qwua7 has 62 direct children: 12 closed, 50 open (11 P1, 39 P2).
- Inversion: str-35vtk.10 (P1, open, "WS-H: Anti-regression closeout") is
  blocked by str-35vtk.29 (P2, open) and str-35vtk.31 (P2, open).
- The str-qwua7 children mix several kinds of work:
  - product bugs: .5 .10 .11 .13 .39
  - refactors: .6.x
  - agent and gate items: .2 .3 .22-.28 .51 .55
  - docs: .21 .44-.46
  - three nested epics

  The audit saw comment_count 0 on every open child.
- The `agents` label has 12 open issues, all P2 and none touched since filing
  (agent-repo-18).
- AGENTS.md has no priority rubric. `bd ready` sorts by priority and age.
- Findings: prior-14, prior-25, agent-repo-18. Source draft:
  `drafts/shatter-agent/11-triage-policy-and-audit-epic-waves.md`.

## Acceptance criteria

- [ ] AGENTS.md defines P0-P3. P1 means broken or misleading for users,
      blocks landing, or security; everything else is P2 or lower.
- [ ] Open P1s are re-triaged against the rubric. The result is at most 15
      open P1s, or a recorded maintainer override. Before and after counts are
      in the close reason.
- [ ] str-qwua7 is split into themed child epics (product correctness,
      refactor, agent/gates, docs), either wave-ordered with `bd swarm` or
      given explicit blocked-by edges. Obsolete children are closed by
      tracker-reconciliation-sweep.
- [ ] Known priority inversions are resolved: str-35vtk.10 (P1) blocked by
      str-35vtk.29/.31 (P2) is fixed by raising the blockers, lowering .10, or
      removing a stale edge, and any other inversion found by a one-off
      query (command and output in the close reason) is fixed the same way.
      This lets triage-drift-patrol-checks land green.
- [ ] AGENTS.md WIP rule: new feature epics at P2 or lower wait while audit P1
      bugs are ready, unless the maintainer overrides.

## Suggested approach

Draft the rubric and the P1 re-triage list first, and get maintainer sign-off
on the demotions before running `bd update`. Apply the same waves to the
2026-09-22 epic when it is filed.

## Out of scope

- Implementing the child issues themselves.
- Closing obsolete issues (tracker-reconciliation-sweep).
- The drift-patrol checks that keep this policy enforced
  (triage-drift-patrol-checks).

## Priority / type / labels

P2, task. Labels: agents, governance, audit, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Blocks: triage-drift-patrol-checks.
- Related: tracker-reconciliation-sweep, str-qwua7, str-qwua7.62.

---

<!-- file: 10-downstream-coverage-goals-epic.md -->

---
slug: downstream-coverage-goals-epic
kind: new
title: "Track the downstream coverage goals (kapow and pickpackit ≥90% feature / ≥80% UI, zolem ≥90%) in the tracker with an owner; stalled at 18-28% since 2026-07-07"
priority: P2
type: epic
labels: [agents, coverage, downstream, kapow, zolem, pickpackit, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Track the downstream coverage goals (kapow and pickpackit ≥90% feature / ≥80% UI, zolem ≥90%) in the tracker with an owner; stalled at 18-28% since 2026-07-07

## Problem

In early July, Shatter set real-world success goals on three downstream
projects. They exist only in per-user agent memory files, which a fresh agent
on another machine cannot read. They were last measured on 2026-07-05..07, at
18-28%, and nothing prompts anyone to re-measure or act. No tracker epic and
no person owns them.

## The goals as set (quoted from the memory files, 2026-09-23)

| Project | Goal | Excluded | Eligible set | Metric | Last measurement |
|---|---|---|---|---|---|
| kapow (set 2026-07-01) | ≥90% line coverage of true feature code; **UI code may be 80%** | tests, generated code | `scripts/shatter-source-set.sh list-eligible` in kapow (1,015 files: 713 Go, 302 TS/TSX; 687 after the kapow-yrxv scope exclusions) | `shatter explore` summary.json line fields: `covered_completed_lines` vs `discovered_function_span_lines`, aggregated over the eligible set | 18.3% exec lines (Go 17.3%, TS 34.6%), 2026-07-06; resolver dir 18.8% |
| zolem (set 2026-07-03) | ≥90% line coverage of true feature code (no UI layer, so flat 90%) | tests, generated code | all non-test `*.go` under `cmd/` + `internal/` (64 files, about 9,780 lines) | whole-repo scan line coverage | 28.0% lines (1754/6265), branch 63.5%, 2026-07-05 |
| pickpackit (set 2026-07-02) | ≥90% line coverage of true feature code; **UI may be 80%** | tests, generated code, `*.d.ts`, `api/src/bin`, `test_support.rs`, `images/tests.rs` | 115 `web/src` TS/TSX + 60 `api/src` `.rs` files (2026-07-03; the file lists were kept only in a session scratchpad and must be regenerated) | scan/explore line coverage (covered vs instrumentable/function-span lines) over the eligible set, UI vs logic split | combined 26.3% lines (1957/7428): web 39.2%, api 21.4%, 2026-07-07 (durable summaries: pickpackit `docs/shatter/baselines/20260707-{web,api}-all-summary.json`) |

Sources: `~/.claude/projects/-home-ketan-project-shatter/memory/`
`project_kapow_shatter_advise_log.md`, `project_zolem_shatter_advise_log.md`,
`project_pickpackit_coverage_goal.md`; `~/project/zolem/docs/shatter/CHANGELOG-for-advise.md`
(newest entry 2026-07-05) and `~/project/pickpackit/docs/shatter/CHANGELOG-for-advise.md`
(2026-07-07). kapow has no `docs/shatter/`.

## Evidence

- Open levers named in memory: str-j49xg (P1 epic, open), str-la75,
  str-jyxr, str-4yc9w, str-wfd2, str-bh9wu (all open, 2026-09-23).
- This audit found the coverage metric itself inconsistent (Go scan inflates
  to 100%, Rust deflates to about 54%) and that explore resume ignores
  explorer mode. Re-measurement must wait for go-scan-coverage-clamp,
  rust-instrumentable-line-count and explore-resume-options-key (bucket
  shatter-artifacts-correctness).
- Finding: goals-09 (the verifier corrected P1 to P2: stalled tracking and
  ownership, not broken behavior). Source draft:
  `drafts/shatter-agent/27-downstream-coverage-goals-epic.md`.

## Acceptance criteria

- [ ] **Owner.** Before any child is started, the maintainer names the epic's
      owner (`bd update <epic> --assignee <name>`). An unowned epic is not
      "done" with any other criterion.
- [ ] **Durable goal doc.** `docs/goals/downstream-coverage.md` (landed via
      launch-work/land-work) records, per project: the goal with its UI
      threshold (80% for kapow and pickpackit UI code, flat 90% for zolem),
      how "UI code" is classified, the exclusions, the command that produces
      the eligible set (committed in the downstream repo or in this doc, so it
      does not depend on a scratchpad), the metric formula, the exact run
      recipe (shatter binary path, cache-clearing prerequisites such as
      kapow's `~/.config/shatter/go-workspace/analysis` note, timeouts), and
      the dated measurements above.
- [ ] **One child per project**, each with its own completion condition: the
      child closes when a measurement recorded in the doc meets that project's
      goal (feature ≥90% and UI ≥80% where applicable), or when the maintainer
      changes the goal in the doc. The open levers are linked to the relevant
      child as dependencies.
- [ ] **Re-measurement child**, blocked by go-scan-coverage-clamp,
      rust-instrumentable-line-count and explore-resume-options-key. It
      re-runs the recipe for all three projects and records dated results in
      the doc (with the shatter SHA used).
- [ ] The three memory files are reduced to pointers to the doc and the epic.

## Suggested approach

Start from the pickpackit memory file, which is the most complete. The
concolic benchmark (concolic-vs-default-benchmark) uses one downstream
project; share its run recipe where possible.

## Out of scope

- Engine improvements themselves.

## Priority / type / labels

P2, epic. Labels: agents, coverage, downstream, kapow, zolem, pickpackit,
audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. Only the re-measurement child is blocked, as listed
  above.
- Related: concolic-vs-default-benchmark, str-j49xg.

---

<!-- file: 11-qwua7-17-drift-patrol-hygiene.md -->

---
slug: qwua7-17-drift-patrol-hygiene
kind: note-to-existing
title: "Note on str-qwua7.17: drift-patrol tracker-hygiene is red again (stale claims, 4 orphans)"
priority: P3
type: task
labels: [agents, drift, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.17
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.17: drift-patrol tracker-hygiene is red again (stale claims, 4 orphans)

**Target:** `str-qwua7.17` ("Release or finish the 13 stale in_progress claims
flagged by drift-patrol").

**Filer caution:** str-qwua7.17 has been **closed** since 2026-09-08
(re-checked 2026-09-23). The source finding's dedupe called it open, but it
is not. Post this as a comment only; **do not reopen it**. The action items
are carried by tracker-reconciliation-sweep (orphans), mpgg1-close, and
landed-not-closed-patrol-check (the recurrence check).

## Comment text

**Audit 2026-09-22 note: tracker-hygiene has regressed since this sweep**
(finding gates-09)

Drift-patrol tracker-hygiene failed on main on 2026-09-22
(`audits/2026-09-22/gates/drift-patrol.log:19,24-39`):
"2 stale in_progress issue(s), 4 orphaned child issue(s)".

- Stale claims: str-8q1b4 (22 d) has since been closed (2026-09-22).
  str-mpgg1 (21 d) is still in_progress although its merge 84941b37 is on
  main; it is being closed by <mpgg1-close>.
- Orphaned open children of closed parents: str-qwua7.56.1, str-qwua7.9.1,
  str-hy9b.J3, str-hy9b.1. <tracker-reconciliation-sweep> re-parents or
  closes them.
- Still PENDING: docs-stories (str-u394l.3) and cli-surface-drift (str-wurp).
  Consider running drift-patrol with `--strict-pending` once both land.

This is the second time stale claims have accumulated after a one-time sweep.
A one-time sweep does not fix it. The recurrence fix is a new drift-patrol
check that FAILs when an issue is still open or in_progress although a
landing merge for it is on origin/main: <landed-not-closed-patrol-check>.
The existing 14-day stale-claim FAIL stays as it is.

---

<!-- file: 12-beads-hook-stall-diagnosis.md -->

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

---

<!-- file: 13-audit-2026-09-04-report-recovery.md -->

---
slug: audit-2026-09-04-report-recovery
kind: new
title: "Recover the deleted 2026-09-04 audit report (unreachable commit e067979d) and land it so the 50 open str-qwua7 children can read their evidence"
priority: P1
type: task
labels: [agents, audit, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Recover the deleted 2026-09-04 audit report (unreachable commit e067979d) and land it so the 50 open str-qwua7 children can read their evidence

## Problem

The 2026-09-04 audit report and its evidence lived only on the local branch
`audit-2026-09-04`. That branch has since been deleted, and its commits are
unreachable. The str-qwua7 epic (62 children, 50 open) and other open issues
cite `audits/2026-09-04...` paths that do not exist on main. A fresh agent
cannot read the evidence for the audit's own follow-up work, and the commits
will be lost at the next `git gc` prune unless a ref holds them.

## Evidence (re-verified 2026-09-23 in /home/ketan/project/shatter)

- `git for-each-ref | grep -i audit` lists only `refs/heads/audit-2026-09-22`
  and `refs/remotes/origin/audit-kapow-2026-05-24`. `audit-2026-09-04` is gone
  (the 2026-09-22 audit findings still saw it: "102 behind / 7 ahead").
- `git cat-file -t e067979d` → `commit`, and no reflog entry holds it. The
  chain: `e067979d` (tip, 2026-09-07 "audit: 2026-09-04 record ids of decided
  issues"), `032c1319`, `a21108b2`, `ce8c1f04`, `e577d697`, `6eb87f9d`,
  `42c112cd` (2026-09-04 "audit: 2026-09-04"); parent 84941b37.
- `git ls-tree -r --name-only e067979d -- audits/2026-09-04 | wc -l` → **97**
  files (gates, triage-drafts, ui), plus `audits/2026-09-04.md`.
- Unreachable 2026-06-09 audit commits: `f9dad247` (latest, 2026-06-12),
  `a7a27ea3`, `f307ac3a`, `8b8bcc14`.
- str-qwua7.44 (open) says root `audits/` moves to `docs/audits/` only after
  the 09-04 branch lands at `audits/`.
- Findings: agent-repo-04, docs-04.

## Acceptance criteria

- [ ] A ref holds the 09-04 tip (`audit-2026-09-04-recovered` at `e067979d`),
      created by the maintainer during the filing bootstrap (see
      publish-audit-reports). If it is missing when work starts and
      `git cat-file -e e067979d` fails, the issue is closed as "unrecoverable"
      with that output, and each open str-qwua7 child citing 09-04 evidence
      gets a comment saying so.
- [ ] `audits/2026-09-04.md` and `audits/2026-09-04/` (97 files) land on
      origin/main at those paths (not `docs/audits/`; str-qwua7.44 moves them
      later) through launch-work/land-work, taken from the recovered ref
      (cherry-pick or checkout of the paths; no rewrite of their content
      beyond privacy redactions the maintainer asks for).
- [ ] **Evidence paths resolve.** For every open issue whose body cites
      `audits/2026-09-04`, each cited path is checked with
      `git cat-file -e origin/main:<path>`; the output shows 0 missing, or
      lists each missing path with a comment posted on the citing issue.
      The command and output are in the close reason.
- [ ] Recorded maintainer decision on the 2026-06-09 commits: land its report
      the same way, or leave it only on the recovery ref.

## Out of scope

- Why the branch was deleted (audit-branch-deletion-cause).
- The 2026-09-22 report (publish-audit-reports).
- Moving `audits/` to `docs/audits/` and the INDEX section (str-qwua7.44).

## Priority / type / labels

P1, task. Labels: agents, audit, docs, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none (the maintainer's recovery ref is a bootstrap step, not an
  issue).
- Related: publish-audit-reports, audit-branch-deletion-cause, str-qwua7.44.

---

<!-- file: 14-qwua7-22-audit-land-before-file-note.md -->

---
slug: qwua7-22-audit-land-before-file-note
kind: note-to-existing
title: "Note on str-qwua7.22: amend the /audit Phase 10 order to land the report before filing, and drop the bd sync sentence"
priority: P2
type: task
labels: [agents, audit, skills, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.22
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.22: land the audit report before filing

**Target:** `str-qwua7.22` (open, P2, "Rewrite /audit Phase 10 to be
worktree- and tracker-safe; add a patrol check for stale audit issues").

**Action:** post the comment below with `bd comments add str-qwua7.22`. Do not
close it and do not change its priority. str-qwua7.22 stays the single owner
of the `/audit` skill rewrite; no 2026-09-22 issue duplicates it.

## Comment text

**Audit 2026-09-22 note: amended acceptance order (supersedes the first
acceptance check's order)**

The 2026-09-22 audit found that filing before landing leaves filed issues
citing evidence that exists only on a local branch, and that branch can be
deleted: `audit-2026-09-04` was deleted after its issues were filed, so the
50 open str-qwua7 children cite `audits/2026-09-04...` paths that are not on
main (being recovered by <audit-2026-09-04-report-recovery>). The 2026-07-10
report was lost the same way (str-sff87).

Replace this issue's first acceptance check with this order:

1. `bd create` the audit-labelled tracking issue.
2. `bento:launch-work` on it; write `audits/<date>.md` and the evidence in that
   worktree.
3. Findings are **drafted** in the worktree (not filed).
4. `bento:land-work` lands the report and evidence on main.
5. A check proves every evidence path cited by a draft exists on origin/main
   (`git cat-file -e origin/main:<path>`; reuse the script added by
   <publish-audit-reports>).
6. Only then are issues filed, with `--parent <audit-id>`; the report's final
   table lists them in a follow-up commit or a comment on the audit issue.
7. The audit issue is closed with reason `report: audits/<date>.md`.

Also: the skill must not mention `bd sync` (it does not exist in bd 1.1.0 and
is retired by maintainer decision D4). The line-level removal of the
`SKILL.md:385` sentence is done by <beads-jsonl-consumers-drop-bd-sync>;
this issue must not reintroduce it. Tracker sync, if any, follows the
AGENTS.md Dolt-remote procedure from <beads-retire-jsonl-import-dolt-remote>.

The patrol check and the str-sff87 close in this issue are unchanged.
Evidence: `audits/2026-09-22/findings.json` agent-repo-04, docs-04.

---

<!-- file: 15-audit-branch-deletion-cause.md -->

---
slug: audit-branch-deletion-cause
kind: new
title: "Find out (time-boxed) what deleted the unmerged, unpushed audit-2026-09-04 branch, and file a bug on the tool if one did it"
priority: P2
type: task
labels: [agents, audit, git, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Find out (time-boxed) what deleted the unmerged, unpushed audit-2026-09-04 branch, and file a bug on the tool if one did it

## Problem

The local branch `audit-2026-09-04` (tip `e067979d`, 7 commits, never pushed,
never merged) was deleted between the 2026-09-22 audit run, which still saw
it ("102 behind / 7 ahead"), and 2026-09-23. The 2026-06-09 audit commits
(`f9dad247` and others) are unreachable too. If an automated tool (bento
closure, a cleanup script, land-work teardown) deletes unmerged, unpushed
branches, it will destroy more work. If it was a manual delete, the fix is
guidance instead.

Missing refs and surviving objects show that the work is recoverable. They do
not show who deleted the branch or why, and the trail may be gone. So this
investigation is bounded, and "cause undetermined" is an acceptable outcome.

## Evidence (re-verified 2026-09-23)

- `git for-each-ref | grep -i audit` in /home/ketan/project/shatter shows no
  `audit-2026-09-04`; `git cat-file -t e067979d` → `commit`; no reflog entry
  holds it.
- Candidate actors: bento `closure` (garbage-collects other agents' branches),
  bento `land-work` teardown, `scripts/cleanup-merged-remote-branches.sh`
  (remote only), `scripts/cleanup.sh`, manual `git branch -D`.
- Finding: agent-repo-04.

## Acceptance criteria

- [ ] At most one working session (about 2 hours) is spent. The sources
      searched are listed in the close reason: at least session transcripts
      under `~/.claude/projects/-home-ketan-project-shatter/` from 2026-09-22
      to 2026-09-23 (grep for `audit-2026-09-04`, `branch -D`, `closure`),
      bento closure/land-work logs if any exist, and `git reflog` of
      `HEAD` in each worktree.
- [ ] The outcome is one of: (a) cause identified, with the transcript line or
      log entry quoted; (b) cause undetermined, with the searches that came
      up empty.
- [ ] If (a) and a tool deleted an unmerged, unpushed branch, a bug is filed
      against that tool's tracker with a reproduction (for example a scratch
      repo where the tool deletes an unmerged local branch), and its id is in
      the close reason. If a manual delete, a one-line guidance change is
      proposed to the maintainer instead.
- [ ] This issue does not block recovering or landing any report.

## Out of scope

- Recovering the commits (the maintainer's bootstrap ref) and landing the
  report (audit-2026-09-04-report-recovery).

## Priority / type / labels

P2, task. Labels: agents, audit, git, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Related: audit-2026-09-04-report-recovery, publish-audit-reports.

---

<!-- file: 16-qwua7-12-rescope-note.md -->

---
slug: qwua7-12-rescope-note
kind: note-to-existing
title: "Note on str-qwua7.12: single-target exit classes are done; multi-target partial-failure exit code needs a maintainer decision before close"
priority: P1
type: task
labels: [agents, cli, spec, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.12
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.12: re-scope, do not close

**Target:** `str-qwua7.12` (open, P1, "Implement SPEC §2.11 exit codes: 2 for
tool/usage errors, 1 only for fired gates").

**Action:** post the comment below with `bd comments add str-qwua7.12`. Do
**not** close it. (An earlier audit draft proposed closing it; that was wrong,
because one acceptance check is unmet and contradicted by current code.)

## Comment text

**Audit 2026-09-22 note: partly done; one acceptance check conflicts with
current code**

Done at origin/main (audit-time runs, `audits/2026-09-22/areas/cli-ux.md`
section 0 and `areas/prior-audit-regress.md`, finding prior-09): missing file,
unsupported extension, unknown function, malformed spec in `spec-diff`, and
the host-write refusal all exit **2**. The exit class is typed, not
substring-matched: `shatter-cli/src/main.rs:1475 error_exit_code` returns 1
only for a `GateFailure` and 2 otherwise. (`categorize_error` at
`main.rs:1521` still matches substrings, but it only labels telemetry.) The
issue's original facts came from a stale binary.

Still open against this issue's acceptance checks:

1. **Multi-target partial failure.** This issue requires exit **1** when at
   least one target succeeds and at least one fails. Current code does the
   opposite on purpose: `decide_explore_exit_status`
   (`shatter-cli/src/commands/explore.rs:744`) returns Ok for partial
   success, and the test
   `decide_exit_status_ok_partial_success_with_some_failed_targets`
   (`explore.rs:7075`) asserts exit 0 ("Partial-success policy"). The
   maintainer must choose one policy. Then either change the code and that
   test (with a red-then-green CLI test for `explore`, `scan` and `run`), or
   amend this acceptance check and SPEC §2.11 to say partial failure exits 0.
2. **Integration tests.** One CLI integration test per error class plus the
   two multi-target cases in `shatter-cli/tests/`, each asserting the exit
   code. Check which already exist before writing new ones; list them in the
   close reason.
3. **SPEC.** SPEC.md:634 still cites the nonexistent `--failure-threshold`
   (the flag is `--fail-on-failures=PERCENT`). Fix it together with the
   multi-target wording; the SPEC tracker-id cleanup in help-tracker-ids-lint
   (bucket shatter-cli-flags-and-help) is related.

Close only when all three are resolved, with the command outputs in the
close reason.

---

<!-- file: 17-landed-not-closed-patrol-check.md -->

---
slug: landed-not-closed-patrol-check
kind: new
title: "drift-patrol: FAIL on open or in_progress issues whose work has affirmatively landed on origin/main"
priority: P2
type: task
labels: [agents, beads, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [tracker-reconciliation-sweep, beads-jsonl-consumers-drop-bd-sync]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# drift-patrol: FAIL on open or in_progress issues whose work has affirmatively landed on origin/main

## Problem

Landed work regularly stays open or `in_progress` (str-mpgg1: merged
2026-09-02, still in_progress 21 days later; str-qwua7.17 swept 13 stale
claims on 2026-09-08 and they came back). drift-patrol's existing
tracker-hygiene check (`scripts/drift-patrol.py:523 check_tracker_hygiene`)
only FAILs on `in_progress` issues untouched for more than
`DEFAULT_STALE_DAYS = 14` (`:67`) and on orphaned children. Nothing detects
"this issue's work is on main, but the issue is not closed".

Branch ancestry alone is not evidence of landing: a branch just created at
`origin/main` is already an ancestor of it before its first commit, and the
normal land-work cleanup deletes the feature branch after merging. So the
check must look for **affirmative landing evidence on main**, not for
branches.

## Evidence (re-verified 2026-09-23)

- drift-patrol statuses are PASS, FAIL, PENDING and SKIP
  (`scripts/drift-patrol.py:23-35`, `:53-56`); there is no WARN. This issue
  adds none.
- Landing commits on main name the issue id in a stable form, for example
  `Merge branch 'str-6nul9-bench-timeout-override'` (70465921) and
  `str-6nul9: exclude bench_frontier_ranking ...` (20692b08).
- Findings: agent-repo-05, sessions-09, gates-09.

## Acceptance criteria

- [ ] A new check (next to `check_tracker_hygiene`) reports **FAIL** for an
      issue with status `open` or `in_progress` when origin/main's history
      contains a landing commit for it: a merge commit whose subject is
      `Merge branch '<id>-...'` or `Merge branch '<id>'`, whose second parent
      has at least one commit not reachable from the merge's first parent.
      Branch existence is not required.
- [ ] **Reopened issues:** if the issue's reopen or last status change is
      later than its newest landing commit, it is not flagged (a reopened
      issue legitimately has landed commits). The rule is documented in
      `docs/DRIFT-PATROL.md`.
- [ ] The check reads live bd; without bd it returns SKIP with a reason
      (consistent with beads-jsonl-consumers-drop-bd-sync; never the JSONL).
- [ ] The existing 14-day stale-claim FAIL is unchanged; no new status
      (WARN) is introduced.
- [ ] Unit tests in `scripts/test_drift_patrol.py` use a scratch git repo
      fixture and canned bd output for: (a) branch created at origin/main
      with no commits → not flagged; (b) branch with commits but unmerged,
      and uncommitted worktree changes → not flagged; (c) merged and branch
      deleted → FAIL; (d) merged, then issue reopened later → not flagged;
      (e) merged and issue closed → PASS. Each test is shown red before the
      check exists or with the check's condition inverted (paste the run).
- [ ] An integration run of `python3 scripts/drift-patrol.py --only
      <new-check-id>` against live bd on main is PASS (after
      tracker-reconciliation-sweep), and its exit code is 0; a forced run
      against a fixture with case (c) exits 1. Both outputs are in the close
      reason.
- [ ] Lands through launch-work/land-work with `task affected` green and its
      `Gates selected` line in the close reason.

## Out of scope

- The one-time closes (tracker-reconciliation-sweep).
- P1-cap, inversion and audit-epic checks (triage-policy-and-audit-epic-waves).

## Priority / type / labels

P2, task. Labels: agents, beads, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: tracker-reconciliation-sweep (so the check lands green),
  beads-jsonl-consumers-drop-bd-sync (live-bd reading and SKIP behavior in
  drift-patrol).
- Related: qwua7-17-drift-patrol-hygiene, mpgg1-close.

---

<!-- file: 18-triage-drift-patrol-checks.md -->

---
slug: triage-drift-patrol-checks
kind: new
title: "drift-patrol: FAIL on open-P1 count over the cap, on blocked-by-lower-priority inversions, and on stalled audit epics"
priority: P2
type: task
labels: [agents, governance, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [triage-policy-and-audit-epic-waves, beads-jsonl-consumers-drop-bd-sync]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# drift-patrol: FAIL on open-P1 count over the cap, on blocked-by-lower-priority inversions, and on stalled audit epics

## Problem

triage-policy-and-audit-epic-waves sets a priority rubric, a P1 cap, fixes
the known inversions and splits the str-qwua7 audit epic. Without a check,
the same drift comes back: this is already the second time stale tracker
state accumulated after a one-time sweep (str-qwua7.17, 2026-09-08). These
checks keep the policy enforced.

## Evidence (re-verified 2026-09-23)

- drift-patrol supports PASS, FAIL, PENDING and SKIP only
  (`scripts/drift-patrol.py:23-35`, `:53-56`). There is no WARN, and this
  issue does not add one: each new check is a normal FAIL-capable check that
  lands green because triage-policy-and-audit-epic-waves fixed the current
  violations first.
- Open issues on 2026-09-23: P1 45, P2 112, P3 23, P4 1. Known inversion:
  str-35vtk.10 (P1) blocked by str-35vtk.29 and .31 (P2).
- str-qwua7: 62 direct children, 12 closed after 19 days; the audit saw
  comment_count 0 on every open child.
- Findings: prior-14, prior-25, agent-repo-18.

## Acceptance criteria

- [ ] Three checks in `scripts/drift-patrol.py`, each reading live bd and
      returning SKIP with a reason when bd is absent (never reading the
      JSONL):
      - **p1-cap:** FAIL when the open P1 count exceeds the cap recorded in
        AGENTS.md by triage-policy-and-audit-epic-waves (the cap value is read
        from one place, not duplicated in the script and the doc).
      - **priority-inversion:** FAIL when an open issue is blocked by an open
        issue of strictly lower priority (higher P number). The failure lists
        each pair.
      - **audit-epic-stall:** FAIL when an open epic labelled `audit` has more
        than half of its open children with no update, comment or status
        change for 14 days. The threshold is a named constant documented in
        `docs/DRIFT-PATROL.md`.
- [ ] `scripts/test_drift_patrol.py` covers each check with canned bd JSON:
      a passing case, a failing case, and the bd-absent SKIP case. Each
      failing-case test is shown red with the check disabled or inverted.
- [ ] An integration run of `python3 scripts/drift-patrol.py --only
      p1-cap,priority-inversion,audit-epic-stall` on main against live bd is
      PASS with exit 0, and a run with a fixture over the cap exits 1. Both
      outputs are in the close reason.
- [ ] `docs/DRIFT-PATROL.md` lists the three checks, their thresholds and
      remediation text.
- [ ] Lands through launch-work/land-work with `task affected` green and its
      `Gates selected` line in the close reason.

## Out of scope

- Setting the rubric, cap, re-triage and epic split
  (triage-policy-and-audit-epic-waves).
- The landed-not-closed check (landed-not-closed-patrol-check).

## Priority / type / labels

P2, task. Labels: agents, governance, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: triage-policy-and-audit-epic-waves (so the checks land green),
  beads-jsonl-consumers-drop-bd-sync (live-bd reading and SKIP behavior).
- Related: landed-not-closed-patrol-check.
