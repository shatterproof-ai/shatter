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
