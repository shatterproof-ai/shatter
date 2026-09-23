# Bundle: shatter-tracker-and-beads (repo: shatter)

Audit 2026-09-22, final issue drafts. Nothing has been filed.

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
- **D4 Beads:** the post-checkout hook spends about 6 minutes importing the
  stale `.beads/issues.jsonl` (1,773 issues, about 10 s of CPU). Retire the
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

## Contents

| NN | Slug | Kind | Target | P | Blocked by |
|---|---|---|---|---|---|
| 01 | beads-jsonl-import-clobber-check | new | - | P1 | - |
| 02 | beads-retire-jsonl-import-dolt-remote | new | - | P1 | 01 |
| 03 | beads-jsonl-consumers-drop-bd-sync | new | - | P1 | 02 |
| 04 | qwua7-28-superseded | note + close | str-qwua7.28 | P2 | - |
| 05 | ly5bz-superseded | note + close | str-ly5bz | P3 | - |
| 06 | mpgg1-close | note + close | str-mpgg1 | P2 | - |
| 07 | publish-audit-reports | new | - | P1 | - (precondition for the D6 filer) |
| 08 | tracker-reconciliation-sweep | new | - | P2 | qwua7-1-git-state-check (other bucket) |
| 09 | triage-policy-and-audit-epic-waves | new | - | P2 | - |
| 10 | downstream-coverage-goals-epic | new (epic) | - | P2 | - |
| 11 | qwua7-17-drift-patrol-hygiene | note | str-qwua7.17 (closed) | P3 | - |

## Reviewer attention (new facts found while re-verifying on 2026-09-23)

- **The `audit-2026-09-04` branch has been deleted.** Its 7 commits (tip
  `e067979d`) are unreachable in /home/ketan/project/shatter and will be lost
  at the next gc prune. The 2026-06-09 audit commits (`f9dad247` and others)
  are also unreachable. Draft 07 makes recovering a ref its first criterion.
  The maintainer should create the ref before anything is filed.
- The primary checkout already has a Dolt remote, `origin`
  (`git+https://github.com/shatterproof-ai/shatter.git`), but its last push
  was 2026-04-11, and preview worktrees report "no Dolt remote configured".
  `bd config show` gives `import.auto = true (default)`.
- JSONL vs live DB: 1,733 vs 1,778 issues, 20 status mismatches, 0 priority
  mismatches. In every mismatch the live DB is newer, so there is no surviving
  status clobber. Transient clobbers need a Dolt-history scan (draft 01).
- str-8q1b4 was closed on 2026-09-22. str-qwua7.17 has been closed since
  2026-09-08, so draft 11 is a comment on a closed issue.
  `/tmp/land-work-preview-a5l9ycto` is gone.
- Five merged remote branches (rmcrl, vr7vq, 0z1im, 6vl7p, 8q1b4) are still
  protected by cleanup-merged-remote-branches.sh, because the stale JSONL
  lists their issues as in_progress.


---

<!-- file: 01-beads-jsonl-import-clobber-check.md -->

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

---

<!-- file: 02-beads-retire-jsonl-import-dolt-remote.md -->

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

---

<!-- file: 03-beads-jsonl-consumers-drop-bd-sync.md -->

---
slug: beads-jsonl-consumers-drop-bd-sync
kind: new
title: "Beads: remove `bd sync` from agent docs and skills, and stop CI drift-patrol and branch cleanup from trusting the stale issues.jsonl"
priority: P1
type: task
labels: [agents, beads, docs, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Beads: remove `bd sync` from agent docs and skills, and stop CI drift-patrol and branch cleanup from trusting the stale issues.jsonl

## Problem

AGENTS.md and the repo skills require `bd sync`, but that command does not
exist in the installed bd 1.1.0. Two automated consumers still treat the
committed `.beads/issues.jsonl` as the truth, although it is a stale export
frozen at 2026-09-07:

- CI drift-patrol tracker-hygiene
- `scripts/cleanup-merged-remote-branches.sh`, which reads its "in-progress,
  do not delete" set from it

Maintainer decision D4 (2026-09-23) retires the JSONL import and makes a Dolt
remote the sync channel (beads-retire-jsonl-import-dolt-remote). This issue
brings the docs, skills and scripts in line with that decision.

## Evidence (re-verified 2026-09-23 in the audit-2026-09-22 worktree)

- `bd version` reports 1.1.0. `bd sync --help` fails with
  `Error: unknown command "sync" for "bd"`. bd also warns that two binaries
  are on PATH (`/usr/local/bin/bd`, `~/.local/bin/bd`).
- `bd sync` is required at:
  - AGENTS.md:125 (landing step 5: "`bd sync` once here")
  - AGENTS.md:293 ("`bd sync` intentionally commits `.beads/*.jsonl`")
  - AGENTS.md:340 (commit-message convention `bd sync: close ...`)
  - AGENTS.md:345-369 (the whole "Beads Sync Cadence" section)
  - `.claude/skills/audit/SKILL.md:385` ("handled by `bd sync`")
- `.beads/PRIME.md:8` gives a different procedure ("`bd dolt pull` →
  `git commit` → `git push`").
- `scripts/drift-patrol.py:188-221` falls back to the committed
  `.beads/issues.jsonl` when bd is not on PATH, which is the case in CI.
  `docs/DRIFT-PATROL.md:53-55` documents this.
- `scripts/cleanup-merged-remote-branches.sh:15,27,105-110` read the
  in-progress set from the tracked JSONL; AGENTS.md:274-277 documents this.
  Concrete effect today: the JSONL still lists str-rmcrl, str-vr7vq,
  str-0z1im, str-6vl7p and str-8q1b4 as `in_progress`, but all are closed in
  the live DB. Their merged remote branches
  (`origin/str-rmcrl-clippy-const-assert`, `origin/str-vr7vq-shatter-init-gitignore`,
  `origin/str-0z1im-negzero-roundtrip`, `origin/str-6vl7p-redundant-canonicalize`,
  `origin/str-8q1b4-resume-report-parity`) still exist, so the script is
  protecting them from cleanup.
- JSONL vs live DB: 1,733 vs 1,778 issues and 20 status mismatches (details
  in beads-jsonl-import-clobber-check).
- `.beads/issues.recovered.jsonl` is tracked (1,201 lines) with no
  explanation. It was last touched by 29295bf7 (2026-05-24, "bd sync: hide
  jsonl export from auto import").
- Findings: prior-03 (P1), agent-repo-06 (the verifier set P2; D4 folds it
  into the P1 retirement), docs-16. Source draft:
  `drafts/shatter-agent/05-bd-sync-removed-jsonl-stale.md`. Its
  "keep or untrack" decision is replaced by D4.

## Acceptance criteria

- [ ] `grep -rn "bd sync" AGENTS.md CLAUDE.md .beads/PRIME.md .claude/ docs/ scripts/`
      returns nothing, except historical audit reports and changelog entries.
      Every former mention links to the one sync procedure that
      beads-retire-jsonl-import-dolt-remote wrote into AGENTS.md. PRIME.md's
      session-close line matches it.
- [ ] The `bd sync: close <id>` commit convention and the "one `bd sync`
      commit per landing" cadence text are removed or replaced by the
      Dolt-remote equivalent.
- [ ] `scripts/cleanup-merged-remote-branches.sh` gets its in-progress set from
      live bd after a `bd dolt pull`. If bd is unreachable it refuses to delete
      and says why; it must not fall back to the stale JSONL. A test with
      canned inputs covers both paths.
- [ ] `scripts/drift-patrol.py` tracker checks read live bd (after pull) where
      bd is available. In CI without bd they report **SKIP** with a reason,
      not a PASS/FAIL computed from the JSONL. `docs/DRIFT-PATROL.md:53-55` is
      updated. `python3 scripts/drift-patrol.py --only tracker-hygiene` output
      from a bd-less environment is quoted in the close reason and shows SKIP.
- [ ] Recorded decision (maintainer): `.beads/issues.jsonl` stays tracked as
      a plain export (who refreshes it, and documented as "not read by
      anything"), or it is untracked. The chosen state is applied.
- [ ] `.beads/issues.recovered.jsonl` is deleted, or AGENTS.md explains why it
      is kept.
- [ ] A docs check (in `docs-smoke` or a meta test) runs
      `bd <subcommand> --help` for every bd subcommand named in AGENTS.md,
      PRIME.md and `.claude/skills/**`, and fails on an unknown one. The check
      fails when `bd sync` is reintroduced (shown by a test fixture).
- [ ] AGENTS.md "bd Quick Reference" states the expected bd version (1.1.x)
      and says which binary on PATH is canonical.
- [ ] `task affected` is green, with the `Gates selected` line in the close
      reason.

## Suggested approach

Do the doc rewrite in one commit and the two script changes, each with its own
tests, in separate commits. Coordinate with drift-patrol-workflow-go-mod
(bucket shatter-ci-workflows): it fixes the drift-patrol workflow, which is the
CI consumer.

## Out of scope

- Disabling the import and configuring the remote
  (beads-retire-jsonl-import-dolt-remote).
- bento's generic `beads-issue-flow` guidance (bento beads-dolt-remote-guidance).
- The `bd sync` mentions in the landing prose tracked by str-qwua7.23. The
  note qwua7-23-agents-md-rtk-and-landing already points them at the D4
  procedure.
- The audit skill's own landing rewrite (publish-audit-reports). This issue
  only removes the `bd sync` sentence at SKILL.md:385.
- Any hook-timeout env var or hook-bypass guidance (D4).

## Priority / type / labels

P1, task. Labels: agents, beads, docs, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: beads-retire-jsonl-import-dolt-remote.
- Related: drift-patrol-workflow-go-mod, qwua7-23-agents-md-rtk-and-landing.
  Supersedes str-ly5bz (see ly5bz-superseded).

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

Root cause, measured 2026-09-23: the beads `post-checkout` hook spends about
6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about
10 s of CPU, so it is waiting, not computing). The imported file is a stale
export, last committed at 134dd616 on 2026-09-07, and bd 1.1.0 itself calls
it "an export, not cross-machine sync or source of truth". Tuning the timeout
only changes how much of that wait is cut off. It does not stop a stale
snapshot from being imported into a newer database.

Replacement work:
- <beads-jsonl-import-clobber-check>: checks whether the import has
  overwritten newer DB state, and repairs any damage.
- <beads-retire-jsonl-import-dolt-remote>: stops the import through bd
  configuration, makes the Dolt remote the sync channel, and measures
  `git worktree add` < 15 s and land.py `create_preview` < 30 s. This issue's
  post-checkout smoke-test idea lives there.
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
title: "Recover and land the 2026-09-04 audit report (branch deleted, commits unreachable), land the 2026-09-22 report, and make /audit land before it files"
priority: P1
type: task
labels: [agents, audit, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Recover and land the 2026-09-04 audit report (branch deleted, commits unreachable), land the 2026-09-22 report, and make /audit land before it files

## Problem

Audit reports are never published to main, so the issues filed from them cite
evidence that fresh agents cannot read.

- The 2026-09-04 report and evidence (62 str-qwua7 children, 68 open issues
  cite `audits/2026-09-04...`) lived only on the local branch
  `audit-2026-09-04`. **Since the audit, that branch has been deleted.** Its
  commits are now unreachable and will be lost at the next `git gc` prune.
- The 2026-07-10 audit was lost the same way (str-sff87: "no 2026-07-10
  report was committed").
- The 2026-06-09 audit commits are also unreachable.
- This 2026-09-22 report exists only on the local, unpushed branch
  `audit-2026-09-22`.

The repo `/audit` skill ends with "commit report" on main, which the
require-worktree hook blocks. It relies on `bd sync`, which does not exist in
bd 1.1.0 and is retired by D4. It has no landing step.

## Evidence (re-verified 2026-09-23)

- In `/home/ketan/project/shatter`:
  - `git for-each-ref | grep -i audit` lists only `refs/heads/audit-2026-09-22`
    and `refs/remotes/origin/audit-kapow-2026-05-24`. **`audit-2026-09-04` is
    gone.** (The 2026-09-22 findings still saw it: "102 behind / 7 ahead".)
  - `git fsck --unreachable` lists the 09-04 chain as unreachable commits:
    `e067979d` (tip, 2026-09-07 "audit: 2026-09-04 record ids of decided
    issues"), `032c1319`, `a21108b2`, `ce8c1f04`, `e577d697`, `6eb87f9d`,
    `42c112cd` (2026-09-04 "audit: 2026-09-04"). The parent is 84941b37.
    `git ls-tree -r e067979d -- audits` has 98 files under
    `audits/2026-09-04/` (gates, triage-drafts, ui) plus the report.
  - Unreachable 2026-06-09 audit commits: `f9dad247` (latest, 2026-06-12),
    `a7a27ea3`, `f307ac3a`, `8b8bcc14`.
- `git ls-remote --heads origin 'audit*'` returns only
  `audit-kapow-2026-05-24`. `audit-2026-09-22` is unpushed; it has one commit
  (56c86168), 507 tracked files, and 16 of them are session-transcript files
  under `audits/2026-09-22/sessions/`.
- `git ls-tree origin/main -- audits/` lists only `2026-02-28.md`,
  `2026-05-21.md` and `kapow-2026-05-21`. `docs/audits/` now exists on main
  and holds `2026-04-19-go-planner-parity.md`.
- `.claude/skills/audit/SKILL.md:380-387`: step 4 says to write
  `audits/YYYY-MM-DD.md`; step 6 says "Commit report ... Do NOT commit beads
  issue changes — those are handled by `bd sync`". There is no launch-work or
  land-work step.
- `docs/INDEX.md` has no "Audits" section.
- Related open issues: str-qwua7.22 (P2, "Rewrite /audit Phase 10 to be
  worktree- and tracker-safe; add a patrol check for stale audit ...") and
  str-qwua7.44 (P2, docs IA; it decided to move `audits/` to `docs/audits/`).
  Neither recovers or lands the 09-04 report.
- Findings: agent-repo-04 (verified P1), docs-04. Source draft:
  `drafts/shatter-agent/06-publish-audit-reports-and-audit-skill-landing.md`.

## Acceptance criteria

- [ ] **First, before anything else:** a ref preserves the 09-04 tip
      (for example `git branch audit-2026-09-04-recovered e067979d`, run by
      the maintainer or with their approval), and the 06-09 tip if it is
      wanted. The commands used are in the close reason.
- [ ] The cause of the deletion is identified (bento closure, a
      cleanup script, or a manual delete) and recorded. If a tool deleted an
      unmerged, unpushed branch, a bug is filed against that tool.
- [ ] The 2026-09-04 report and its evidence are on origin/main at the path
      str-qwua7.44 chose (`docs/audits/2026-09-04/`), or at
      `audits/2026-09-04/` if .44 has not landed. They land through
      launch-work/land-work. A script checks every evidence path cited by an
      open str-qwua7 child with `git cat-file -e origin/main:<path>` (after
      any path rewrite), and its zero-failure output is in the close reason.
- [ ] The 2026-09-22 report (`audits/2026-09-22.md` plus the tracked
      evidence) lands on origin/main the same way **before** the maintainer's
      D6 filer script files issues that cite it. The maintainer first reviews
      the `sessions/` transcript files for content that should not be on main.
      Untracked `goals-runs/` (about 255 MB) stays out.
- [ ] `.claude/skills/audit/SKILL.md` post-audit steps are, in order: create
      the audit issue; launch-work a worktree; write the report there; land
      the report; then file issues. The skill states the check command that
      proves issue evidence paths exist on origin/main. No step mentions
      `bd sync` (D4); tracker sync follows the AGENTS.md Dolt-remote procedure.
- [ ] `docs/INDEX.md` has an "Audits" section that links every landed report.

## Suggested approach

Recover the refs first; that is time-critical. Then coordinate with
str-qwua7.22. Either land this issue first and mark .22's Phase-10 part done,
or post a note on .22 that this issue carries the skill rewrite. Link both
str-qwua7.22 and str-qwua7.44 when filing.

## Out of scope

- The drift-patrol "audit follow-through" burn-down check
  (triage-policy-and-audit-epic-waves).
- Filing the 2026-09-22 issues themselves (D6: the maintainer's filer script).
- Moving older reports (02-28, 05-21) unless str-qwua7.44 does it.

## Priority / type / labels

P1, task. Labels: agents, audit, skills, docs, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. This must run before the D6 filer script.
- Related: str-qwua7.22, str-qwua7.44, str-sff87 (closed), and
  beads-jsonl-consumers-drop-bd-sync (SKILL.md:385 `bd sync` line).

---

<!-- file: 08-tracker-reconciliation-sweep.md -->

---
slug: tracker-reconciliation-sweep
kind: new
title: "Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, and add a landed-not-closed drift-patrol check"
priority: P2
type: chore
labels: [agents, beads, drift, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [qwua7-1-git-state-check]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, and add a landed-not-closed drift-patrol check

## Problem

The tracker lags reality in several ways:

- Issues whose premise no longer holds stay open, and their stale blocking
  edges keep other issues in `bd ready`.
- Landed work stays `in_progress`.
- Duplicates stay open.
- Work done under a different issue id never touches the umbrella issue.

Nothing detects "branch merged but issue still open or in progress", so these
cases pile up until an audit finds them.

## Evidence (re-verified 2026-09-23; verify each again before acting)

| Issue | State now | Evidence | Action |
|---|---|---|---|
| str-qwua7.1 (P1, "Repair primary checkout (core.bare=true) and add a git-state hygiene check") | open | `git -C /home/ketan/project/shatter config core.bare` gives `false` | **Do not close.** qwua7-1-git-state-check re-scopes it to the git-state check (D5). Only remove its stale `blocks` edges to .18 and .19. |
| str-qwua7.12 (P1, SPEC §2.11 exit codes) | open | At HEAD, `explore nope.ts:foo`, an unknown function, the sandbox refusal and a bad-file `spec-diff` all exit 2. `error_exit_code` dates from 464e3c9e, an ancestor of the 09-04 audit base, so the finding came from a stale binary (prior-09). | Close it with that evidence. Move the remaining sub-item (SPEC.md:634 names a nonexistent `--failure-threshold`) to a new small issue, or to the SPEC-drift work under str-wurp. |
| str-qwua7.18 (P1, "Land the six parked worktree branches ...; close str-duens ...") | open, `blocked_by` str-qwua7.1 (`bd dep list`) | str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq, str-8q1b4 and str-duens are all closed in the live DB. `git worktree list` shows no duens or 8q1b4 worktree. | Close. Five merged remote branches remain (`origin/str-{0z1im,6vl7p,8q1b4,rmcrl,vr7vq}-*`); leave them to cleanup-merged-remote-branches.sh once beads-jsonl-consumers-drop-bd-sync fixes its in-progress source. |
| str-qwua7.19 (P1, "Push local main ... and prune the land-work preview worktrees") | open, `blocked_by` str-qwua7.1 | main == origin/main. `git worktree list \| grep -c land-work-preview` gives 0 (the `/tmp/land-work-preview-a5l9ycto` seen on 09-22 is gone). | Close. |
| str-mpgg1 | in_progress since 2026-09-01 | 84941b37 is an ancestor of origin/main | Close via mpgg1-close (same run; do not close twice). |
| str-qe9pp (P2, "Rust frontend parity/conformance drift: golden + prepare timeout") | open, last updated 2026-07-06 | golden updated in 750b7ff8 (2026-06-18); prepare timeout fixed by str-uoclg (closed 2026-08-13) | Close, citing both. |
| str-uj3y (P2) | open | Duplicates str-da35 (closed, landed 4182e51e; `build_frontend.rs` vendors the runtime) | Close as a duplicate of str-da35. |
| str-1fik (P1, empty body) / str-wfd2 (P2) | both open | Both ask for cross-file/cross-crate Rust type synthesis. str-qwua7.62 (approved 2026-09-06) decided to merge wfd2 into 1fik. The audit reviewer (frontend-rust-10) suggests the reverse: wfd2 is cross-crate synthesis, and same-crate was delivered by str-do53. | Follow the recorded .62 decision unless the maintainer overrides it. Either way the survivor gets a body that says "same-crate resolved (str-do53); cross-crate still Opaque". |
| Orphans: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1 | open under closed parents | `audits/2026-09-22/gates/drift-patrol.log:35-39` | Re-parent (for example under the 2026-09-22 epic or a live epic) or close each, with a reason. |

str-8q1b4 (flagged in the report) was closed on 2026-09-22 and needs no
action here.

Findings: agent-repo-05, sessions-09, prior-09, protocol-parity-16,
frontend-rust-10 (tracker part only; the memory note was corrected on
2026-09-23). The source draft is
`drafts/shatter-agent/10-tracker-reconciliation-sweep.md`. Related:
prior-12, which records that str-qwua7.62's approved decisions are still
unexecuted after 16 days (str-hrg2 → str-0wxw duplicate close, wfd2/1fik
merge, str-2zsy and str-cl53 bodies, str-u394l.4 → P1).

## Acceptance criteria

- [ ] Every row in the table is closed, re-scoped or re-parented. Each close
      reason cites the evidence (command plus output, or a SHA). No bare
      "Closed" reasons.
- [ ] `bd dep list str-qwua7.18` and `bd dep list str-qwua7.19` no longer
      show `str-qwua7.1`, or both issues are closed.
- [ ] str-qwua7.62's approved decisions are executed in the same session, or
      a note on .62 explains why each remaining one is deferred.
- [ ] A note appended to str-qwua7.62 points to this issue for the items
      above.
- [ ] `python3 scripts/drift-patrol.py --only tracker-hygiene`, run against
      live bd, passes or lists only items with a documented reason. The output
      is in the close reason.
- [ ] New drift-patrol check, added in `scripts/drift-patrol.py` next to
      `check_tracker_hygiene` (:523):
      - **FAIL** for an `in_progress` or `open` issue whose branch (`<id>-*`
        or `<id>`, local or `origin/`) is an ancestor of origin/main
        (landed but not closed).
      - **WARN** for an `in_progress` issue older than 14 days with no
        matching local or remote branch or worktree.
      - Unit tests with canned bd and git data cover FAIL, WARN and clean
        cases.
- [ ] The patrol check lands through launch-work/land-work, with `task
      affected` green and its `Gates selected` line in the close reason.

## Suggested approach

The closes are tracker-only work and need no branch. The patrol check needs
a branch. Run the closes after qwua7-1-git-state-check has been posted, so
.1 is re-scoped rather than closed.

## Out of scope

- The priority rubric, P1 cap and epic waves (triage-policy-and-audit-epic-waves).
- Implementing Rust cross-crate type synthesis.
- Deleting remote branches (cleanup script, after beads-jsonl-consumers-drop-bd-sync).
- Re-verifying str-qwua7.14 (fixture-corruption-incident-reverify).
- Memory files (already corrected 2026-09-23).

## Priority / type / labels

P2, chore. Labels: agents, beads, drift, governance, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: qwua7-1-git-state-check (the note re-scoping str-qwua7.1, in
  bucket shatter-agent-guidance-and-repo-hygiene).
- Related: mpgg1-close, qwua7-17-drift-patrol-hygiene, str-qwua7.62,
  beads-jsonl-consumers-drop-bd-sync.

---

<!-- file: 09-triage-policy-and-audit-epic-waves.md -->

---
slug: triage-policy-and-audit-epic-waves
kind: new
title: "Define P1, cap open P1s, and split and wave-order the stalled str-qwua7 audit epic"
priority: P2
type: task
labels: [agents, governance, audit, drift, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Define P1, cap open P1s, and split and wave-order the stalled str-qwua7 audit epic

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
- [ ] New drift-patrol checks, each with unit tests over canned bd data:
      - WARN when the open P1 count is over the cap.
      - FAIL on a blocked-by-lower-priority inversion.
      - WARN when an audit epic has more than half of its children untouched
        for 14 days.
- [ ] AGENTS.md WIP rule: new feature epics at P2 or lower wait while audit P1
      bugs are ready, unless the maintainer overrides.

## Suggested approach

Draft the rubric and the P1 re-triage list first, and get maintainer sign-off
on the demotions before running `bd update`. Apply the same waves to the
2026-09-22 epic when it is filed.

## Out of scope

- Implementing the child issues themselves.
- Closing obsolete issues (tracker-reconciliation-sweep).

## Priority / type / labels

P2, task. Labels: agents, governance, audit, drift, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Related: tracker-reconciliation-sweep, str-qwua7, str-qwua7.62.

---

<!-- file: 10-downstream-coverage-goals-epic.md -->

---
slug: downstream-coverage-goals-epic
kind: new
title: "Track the downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07"
priority: P2
type: epic
labels: [agents, coverage, downstream, kapow, zolem, pickpackit, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Track the downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07

## Problem

In early July, Shatter set real-world success goals: at least 90% coverage on
kapow, zolem and pickpackit. These goals exist only in per-user agent memory
files. They were last measured on 2026-07-05..07, at 18-28%, and nothing
prompts anyone to re-measure or act. No tracker epic owns them.

## Evidence

- Memory files under `~/.claude/projects/-home-ketan-project-shatter/memory/`:
  - `project_pickpackit_coverage_goal.md`: metric, eligible set, baseline
    26.3% combined, 2026-07-07.
  - `project_kapow_shatter_advise_log.md`: 18.3%; the resolver dir is 18.8%
    (07-06).
  - `project_zolem_shatter_advise_log.md`: 28.0% lines, 2026-07-05.
- Newest dated entries: `~/project/zolem/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-05 and `~/project/pickpackit/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-07. `ls` re-checked 2026-09-23: pickpackit's file is dated Jul 7.
  kapow has no `docs/shatter/`.
- Open levers named in memory: str-j49xg (P1 epic, open), str-la75, str-jyxr,
  str-4yc9w, str-wfd2, str-bh9wu.
- This audit found the coverage metric itself inconsistent (Go scan inflates
  to 100%, Rust deflates to about 54%). It also found that explore resume
  ignores explorer mode. Any re-measurement must wait for those fixes:
  go-scan-coverage-clamp, rust-instrumentable-line-count and
  explore-resume-options-key (bucket shatter-artifacts-correctness).
- Finding: goals-09 (the verifier corrected P1 to P2: stalled tracking and
  ownership, not broken behavior). Source draft:
  `drafts/shatter-agent/27-downstream-coverage-goals-epic.md`.

## Acceptance criteria

- [ ] An in-repo doc (for example `docs/goals/downstream-coverage.md`)
      records, for each project, the metric definition, the eligible set, the
      run recipe and the dated measurements.
- [ ] This epic has one child per downstream project. Each child holds the
      last measurement, and the open levers are linked as dependencies.
- [ ] A re-measurement child is blocked by go-scan-coverage-clamp,
      rust-instrumentable-line-count and explore-resume-options-key. Its
      result is recorded in the doc with a date.
- [ ] The three memory files are reduced to pointers to the doc and the epic.

## Suggested approach

Start by copying the metric and recipe from the pickpackit memory file, which
is the most complete, into the doc. Then create the per-project children.
The concolic benchmark (concolic-vs-default-benchmark) uses one downstream
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
are carried by tracker-reconciliation-sweep (orphans, landed-not-closed
check) and mpgg1-close.

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
The recurrence fix is the landed-not-closed and stale-claim patrol check in
<tracker-reconciliation-sweep>. A one-time sweep does not fix it.
