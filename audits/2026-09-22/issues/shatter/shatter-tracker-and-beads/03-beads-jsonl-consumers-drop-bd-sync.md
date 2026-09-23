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
