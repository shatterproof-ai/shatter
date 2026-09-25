---
slug: tracker-migration-and-agents-md
kind: new
title: "Add AGENTS.md/CLAUDE.md (Dolt-remote tracker sync, no bd sync) and move the four root plan docs to docs/plans/ with export-ignore kept"
priority: P2
type: chore
labels: [agents, beads, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Maintainer has run the v32->v53 migration on ONE designated clone and pushed it. The filer must run `bd list` in /home/ketan/project/storystore first and STOP (non-zero exit, no partial filing) if its output contains 'refusing to auto-apply' or 'Writes are blocked', printing: 'storystore tracker is still on schema v32; run BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push on the designated clone (maintainer approval required), then re-run the filer.' This applies to every draft in this bucket, including the ss-yoa comment. The post-migration verification (other clones, beads.role) is tracked separately in tracker-migration-verification."
---

# Add AGENTS.md/CLAUDE.md (Dolt-remote tracker sync, no bd sync) and move the four root plan docs to docs/plans/ with export-ignore kept

> Slug note: this slug is kept from the pre-revision draft for stability.
> The migration record and clone verification that it used to carry now
> live in **tracker-migration-verification**. This issue is only the
> repository setup.

## Problem

1. **No agent conventions.** The repo root has `README.md`, `spec.md`,
   `INSTALL.md` and four top-level plan files
   (`2026-05-01-storystore-plan-1-foundation.md`, `-plan-2-fidelity.md`,
   `-plan-3-edits-and-impact.md`, `-target-design.md`). It has no `AGENTS.md`
   or `CLAUDE.md`, so an agent working here gets no build, version-bump,
   test or landing rules. bugshot's convention is a `CLAUDE.md` that contains
   only `@AGENTS.md`.

2. **Tracker sync guidance must follow shatter decision D4.** storystore
   commits `.beads/issues.jsonl` (54 lines, last committed a89e7e2 on
   2026-06-16, while `bd count` = 56, so it is already stale) and commits
   `.beads/hooks/{post-checkout,post-merge,pre-commit,pre-push,prepare-commit-msg}`.
   Those hooks are **not** installed in this clone: `core.hooksPath` is
   unset and `.git/hooks` has only samples. bd 1.1.0 describes the JSONL as
   "an export, not cross-machine sync or source of truth". The shatter audit
   decided (D4, 2026-09-23) that the Dolt remote is the sync channel and the
   JSONL import is retired. storystore's new AGENTS.md must not tell agents
   to `bd sync` or import the JSONL. The database is remote-backed
   (`bd dolt remote list` shows `origin git+ssh://git@github.com/ketang/storystore.git`).

3. **Moving the plan docs is not a plain `git mv`.** The four plan files are
   excluded from the published plugin archive by root-anchored
   `export-ignore` lines in `.gitattributes:12-15`, and
   `tests/test_published_bundle.py:27-34` (`EXCLUDED_PREFIXES`) asserts they
   are absent. `docs/` itself **is** shipped (the archive today contains
   `docs/adr/` and `docs/contributing/`), so after a move to `docs/plans/`
   the old anchored lines stop matching and the plans would leak into the
   consumer bundle unless `/docs/plans/` is export-ignored.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `ls /home/ketan/project/storystore`: no AGENTS.md or CLAUDE.md. The four
  `2026-05-01-storystore-*.md` files are at the root.
- `.gitattributes:12-15`: `/2026-05-01-storystore-*.md export-ignore`
  (four lines). `git archive --worktree-attributes HEAD | tar t` includes
  `docs/adr/*` and `docs/contributing/*`.
- `README.md` "Test" section: `python3 -m pytest tests/ -x -q`. Build:
  `scripts/build-plugin` (`--bump`, `--shared-only`; README.md:53-55,
  INSTALL.md:118-127).
- Source finding: plugins-09 (`audits/2026-09-22/findings.json` in the shatter
  audit worktree). Old draft: `drafts/other-first-party/31-ss-migration-agents-md.md`.

## Acceptance criteria

- [ ] `AGENTS.md` exists at the repo root and covers: building
      (`scripts/build-plugin`, `--shared-only`); the version rule (link to
      the automatic-version-bump issue until it lands; the rule text itself
      is added by that issue); the test command; branch/worktree and
      landing; and tracker use through bento:beads-issue-flow, with the Dolt
      remote as sync (`bd dolt push` / `bd dolt pull`).
- [ ] `AGENTS.md` states that `.beads/issues.jsonl` is an export, not a sync
      channel, and must not be imported or hand-edited. Close-time proof:
      `grep -nE 'bd sync|bd import|BEADS_HOOK_TIMEOUT|--no-verify' AGENTS.md CLAUDE.md`
      prints nothing (paste the empty result and exit status 1).
- [ ] `CLAUDE.md` exists and its only content is `@AGENTS.md`.
- [ ] The four plan files are in `docs/plans/`, moved with `git mv`
      (`git log --follow` on one of them shows the pre-move history).
- [ ] `.gitattributes` export-ignores `/docs/plans/` and no longer carries
      the four stale root-anchored lines. `EXCLUDED_PREFIXES` in
      `tests/test_published_bundle.py` lists `docs/plans/` instead of the
      four root names.
- [ ] Red->green proof that the leak check is live: with the
      `/docs/plans/ export-ignore` line temporarily removed,
      `python3 -m pytest tests/test_published_bundle.py -q` fails on
      `docs/plans/`; with it restored, it passes. Paste both summary lines.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.
- [ ] The close reason records the chosen handling of the committed
      `.beads/issues.jsonl` and the uninstalled `.beads/hooks/` (keep as an
      export, or remove), consistent with D4. Whatever is chosen, no hook
      that imports the JSONL on checkout/merge is installed.

## Suggested approach

On a feature branch/worktree: write AGENTS.md (use bugshot's AGENTS.md as a
model), add `CLAUDE.md` = `@AGENTS.md`, `git mv` the plan docs, replace the
four `.gitattributes` lines with `/docs/plans/ export-ignore`, update the
test's `EXCLUDED_PREFIXES`, and run the suite.

## Out of scope

- The schema migration itself, other-clone `bd bootstrap`, and `beads.role`
  (tracker-migration-verification).
- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).
- The clap extractor and the version bump (separate issues in this bucket).
- Adopting storystore in shatter (str-qwua7.52).

## Priority

P2: every agent session in the repo lacks conventions, but shipped behavior
is not broken.

## Type / Labels

chore; agents, beads, docs, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none. The migration is a filing precondition (already done
  when this exists), not an implementation dependency.
- Related: automatic-version-bump adds the version rule to this AGENTS.md
  if AGENTS.md exists when it lands.
