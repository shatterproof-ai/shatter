---
slug: tracker-migration-and-agents-md
kind: new
title: "Record the v32->v53 bd schema migration, set beads.role, and add AGENTS.md/CLAUDE.md with plan docs moved to docs/plans/"
priority: P2
type: chore
labels: [agents, beads, tracker, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Maintainer has run the v32->v53 migration on ONE designated clone and pushed it. The filer must run `bd list` in /home/ketan/project/storystore first and STOP (non-zero exit, no partial filing) if its output contains 'refusing to auto-apply' or 'Writes are blocked', printing: 'storystore tracker is still on schema v32; run BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push on the designated clone (maintainer approval required), then re-run the filer.' This applies to every draft in this bucket, including the ss-yoa comment."
---

# Record the v32->v53 bd schema migration, set beads.role, and add AGENTS.md/CLAUDE.md with plan docs moved to docs/plans/

## Problem

1. **Tracker was write-blocked.** On 2026-09-23, `bd list` in
   `/home/ketan/project/storystore` (bd 1.1.0) printed:

   ```
   Warning: refusing to auto-apply 21 pending schema migrations to a remote-backed database (v32 -> v53): migrating clones independently forks the schema (#4259)
     Read-only command: continuing on schema v32 without migrating.
     Writes are blocked until the schema is reconciled.
     ...
       • designated migrator (only ONE machine): BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push
       • every other clone (another already migrated): bd bootstrap
   ```

   `bd config get beads.role` gives the same refusal. The audit also saw
   "beads.role not configured". The database is remote-backed
   (`bd dolt remote list` shows `origin git+ssh://git@github.com/ketang/storystore.git`),
   so migrating more than one clone would fork the schema. That is why the
   migration needs the maintainer's approval and exactly one migrator.
   **This issue can only be filed after that migration.** The filer script
   checks for it (see `filer_precondition`). The migration itself happens
   before filing. This issue records it, verifies the other clones, and
   covers the remaining setup.

2. **No agent conventions.** The repo root has `README.md`, `spec.md`,
   `INSTALL.md` and four top-level plan files
   (`2026-05-01-storystore-plan-1-foundation.md`, `-plan-2-fidelity.md`,
   `-plan-3-edits-and-impact.md`, `-target-design.md`). It has no `AGENTS.md`
   or `CLAUDE.md`, so an agent working here gets no build, version-bump,
   test or landing rules. bugshot's convention is a `CLAUDE.md` that contains
   only `@AGENTS.md`.

3. **Tracker sync guidance must follow shatter decision D4.** storystore
   commits `.beads/issues.jsonl` (54 lines, last committed a89e7e2 on
   2026-06-16, while `bd count` = 56, so it is already stale) and commits
   `.beads/hooks/{post-checkout,post-merge,pre-commit,pre-push,prepare-commit-msg}`.
   Those hooks are **not** installed in this clone: `core.hooksPath` is
   unset and `.git/hooks` has only samples. bd 1.1.0 describes the JSONL as
   "an export, not cross-machine sync or source of truth". The shatter audit
   decided (D4, 2026-09-23) that the Dolt remote is the sync channel and the
   JSONL import is retired. storystore's new AGENTS.md must not tell agents
   to `bd sync` or import the JSONL.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `bd list` / `bd config get beads.role`: the migration refusal quoted above.
- `ls /home/ketan/project/storystore`: no AGENTS.md or CLAUDE.md. The four
  `2026-05-01-storystore-*.md` files are at the root.
- The plan file names are referenced in `tests/test_published_bundle.py:28-31`
  and `.gitattributes:12-15` (`export-ignore` entries with root-anchored
  paths). Moving the files requires updating both.
- `README.md` "Test" section: `python3 -m pytest tests/ -x -q`. Build:
  `scripts/build-plugin` (and `--shared-only`).
- Source finding: plugins-09 (`audits/2026-09-22/findings.json` in the shatter
  audit worktree). Old draft: `drafts/other-first-party/31-ss-migration-agents-md.md`.

## Acceptance criteria

- [ ] The close reason names the designated migrator clone and the time
      `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` was run. It
      includes `bd list` output from that clone with no "refusing to
      auto-apply" line.
- [ ] Every other storystore clone the maintainer uses has run `bd bootstrap`
      (not `bd migrate`). The close reason lists those clones, or says "no
      other clones".
- [ ] `bd config get beads.role` prints a value, and `bd create --title test
      ... && bd delete <id>` (or an equivalent write) succeeds. Paste the output.
- [ ] `AGENTS.md` exists at the repo root and covers: building
      (`scripts/build-plugin`, `--shared-only`); the version-bump rule (link
      to the automatic-version-bump issue, or the rule it lands); the test
      command; branch/worktree and landing; and tracker use through
      bento:beads-issue-flow, with the Dolt remote as sync (`bd dolt push` /
      `bd dolt pull`). It contains no `bd sync` and no instruction to import
      or hand-edit `.beads/issues.jsonl`.
- [ ] `CLAUDE.md` exists and contains `@AGENTS.md`.
- [ ] The four plan files are in `docs/plans/`, moved with `git mv`.
      `.gitattributes` export-ignore entries and
      `tests/test_published_bundle.py` are updated. `python3 -m pytest tests/ -x -q`
      passes (paste the summary line).

## Suggested approach

1. (Before filing; maintainer) Choose one clone, run
   `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push`, then run
   `bd bootstrap` in any other clone. Set `beads.role` as bd suggests.
2. File this bucket.
3. On a feature branch/worktree: write AGENTS.md (use bugshot's AGENTS.md
   as a model), add `CLAUDE.md` = `@AGENTS.md`, `git mv` the plan docs, fix
   the `.gitattributes` and test references, and run the test suite.
4. Whether to stop committing `.beads/issues.jsonl` (and remove the
   uninstalled `.beads/hooks/`) follows the D4 outcome in shatter and the
   matching bento beads-issue-flow guidance. Note which option was chosen in
   AGENTS.md. Do not install the JSONL-importing post-checkout hook.

## Out of scope

- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).
- The clap/cobra extractors and the version bump (separate issues in this
  bucket).
- Adopting storystore in shatter (str-qwua7.52).

## Priority

P2: this blocks every other storystore write and every agent session in the
repo, but it does not break shipped behavior.

## Type / Labels

chore; agents, beads, tracker, docs, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none. The migration is a filing precondition, not a tracker
  dependency.
- Blocks, in practice: every other draft in this bucket, since none can be
  filed until the migration is done.
