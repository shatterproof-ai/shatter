---
slug: automatic-version-bump
kind: new
title: "build-plugin: bump the patch version automatically from a hash of the full published payload, and add a check that fails on stale generated outputs, mismatched manifest versions, or an unrecorded payload change"
priority: P2
type: chore
labels: [release, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# build-plugin: bump the patch version automatically from a hash of the full published payload, and add a check that fails on stale generated outputs, mismatched manifest versions, or an unrecorded payload change

## Problem

storystore's plugin version has stayed at 0.1.1 since 2026-05-25 while
shipped content kept changing. The bento marketplace lists storystore with
no pinned version (github source), and Claude Code keys its plugin cache
directory on the manifest version (`~/.claude/plugins/cache/bento/storystore/<version>/`).
On the maintainer's Claude Code installation inspected by the audit, the
installed storystore is a 2026-05-10 build (commit ca16aef4), 128 commits
behind `main`, missing `shared/impact_trigger.py` and carrying older copies
of six shared scripts, including `inventory.py`. Whether other
installations are equally stale was not inspected; the unchanged version
string is the likely reason `/plugin update` does not refresh it, but that
causal link is inferred, not demonstrated.

A version bump exists but is manual: `scripts/build-plugin --bump`
(`scripts/build-plugin:231-235,259-261`, documented in README.md:53 and
INSTALL.md:118-121) increments the patch version when asked. Nothing bumps
it when the payload changes, and nothing fails when shipped content,
generated outputs, or manifest versions drift apart.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d)

- `cat plugin-version.json` -> `{"version": "0.1.1"}`.
  `git log --oneline -1 -- plugin-version.json` -> `147af27 Bump plugin version to 0.1.1`.
  `git log --oneline 147af27..HEAD | wc -l` -> 65.
- `~/.claude/plugins/installed_plugins.json`: `storystore@bento` version
  0.1.1, `gitCommitSha` ca16aef4 (committed 2026-05-10), `lastUpdated`
  2026-05-26, installPath `~/.claude/plugins/cache/bento/storystore/0.1.1`.
  `git rev-list --count ca16aef4..HEAD` -> 128.
- The cache's `shared/` has no `impact_trigger.py`; the audit's `diff -rq`
  found audit.py, coverage.py, drift_todo.py, impact_check.py, inventory.py
  and list_candidates.py differ from the repo.
- **The published payload is more than `shared/` and `skills/`.**
  `tests/test_published_bundle.py:36-53` makes the `git archive` contents
  the consumer contract and requires `spec.md`, `README.md`, `INSTALL.md`,
  `plugin-version.json`, both `plugin.json` manifests, and the `skills/`,
  `shared/`, `scripts/`, `.claude/skills/`, `examples/` trees; `docs/adr/`
  and `docs/contributing/` also ship today. Exclusions come from
  `.gitattributes` `export-ignore` (plan docs, `tests/`, `.beads/`).
- Generated outputs: `scripts/build-plugin` writes `.claude-plugin/plugin.json`,
  `.codex-plugin/plugin.json`, `.claude/skills/<name>.md`, and
  `.codex-plugin/skills/<name>/SKILL.md` plus materialized shared scripts
  under `.codex-plugin/skills/<name>/{scripts,references}/` (all tracked);
  `skills/*/scripts/` and `skills/*/references/` are gitignored build
  copies. `--shared-only` (`:250-256`) materializes shared scripts without
  touching manifests or version.
- **storystore has no `.github/workflows/`**; `python3 -m pytest tests/ -x -q`
  is the only automated gate.
- Precedent: shatter-agents `scripts/build-plugins:187-210`
  (`compute_plugin_hash`, `bump_version_if_changed`, hash stored in
  `catalog/plugin-versions.json`) plus `scripts/check-plugins-clean`, run by
  `.github/workflows/ci.yml` job `build-clean` (shatter-agents sa-8xg).
- Source finding: plugins-07. Old draft: `drafts/other-first-party/33-ss-version-bump.md`.

## Acceptance criteria

- [ ] **Hash scope = the published payload.** `scripts/build-plugin`
      computes a content hash over every file the published archive ships
      (tracked files not marked `export-ignore`, i.e. the same set
      `tests/test_published_bundle.py` checks), in sorted path order, over
      path + bytes, excluding only the three version-carrying files
      (`plugin-version.json`, `.claude-plugin/plugin.json`,
      `.codex-plugin/plugin.json`). A parametrized test in
      `tests/test_build_plugin.py` shows the hash changes when a file under
      each of `shared/`, `skills/`, `scripts/`, `examples/`, `docs/` and a
      root doc (`README.md`) changes, and when a new shipped file is added;
      and does **not** change when a file under `tests/` or `.beads/`
      changes.
- [ ] **Automatic bump.** A full `scripts/build-plugin` run records the
      hash next to the version (e.g. `plugin-version.json` gains
      `content_hash`) and bumps the patch version when the hash differs from
      the recorded one. Tests: a second run with no content change leaves
      version and hash unchanged; a content change bumps exactly one patch
      level.
- [ ] **Flags defined.** `--bump` either is removed or becomes an explicit
      force-bump that still records the hash; `--shared-only` never changes
      the version and leaves the check below failing until a full build
      runs. Both behaviours have tests, and README.md ("Build") and
      INSTALL.md (:118-127) describe the new behaviour; no doc still says
      the version is bumped only by `--bump`.
- [ ] **Staleness check that cannot pass on broken outputs.** A
      `scripts/check-plugin-clean` (run by a pytest test) regenerates all
      build outputs into a temporary copy of the tracked tree and fails if
      any of these hold: a generated file listed above is missing or
      differs from the committed one; the version in either `plugin.json`
      differs from `plugin-version.json`; the recorded `content_hash`
      differs from the recomputed hash. Tests use a temp fixture repo and
      show each of the three failure modes red (one test per mode, e.g. a
      hand-edited `shared/inventory.py` with no rebuild, a deleted
      `.codex-plugin/skills/*/SKILL.md`, a hand-edited manifest version),
      and green after a full rebuild. Paste the red and green runs.
- [ ] **Gate wiring.** Either a GitHub Actions workflow runs the check and
      pytest on push/PR to `main` (close reason includes a green run URL),
      or AGENTS.md (if present; tracker-migration-and-agents-md creates it)
      and README.md name the check as a landing gate.
- [ ] **Bump now.** The version is bumped (0.1.2 or later) by the new
      mechanism and pushed. After `/plugin update` (or reinstall) on the
      maintainer's Claude Code installation,
      `~/.claude/plugins/cache/bento/storystore/<new version>/shared/`
      contains `impact_trigger.py`. Paste the `ls` and the new
      `installed_plugins.json` entry.
- [ ] The rule "never hand-edit the version; build-plugin bumps it" is
      written in AGENTS.md if it exists when this lands, and in README.md
      either way.

## Suggested approach

Port `compute_plugin_hash` / `bump_version_if_changed` from shatter-agents
`scripts/build-plugins`, adapted to storystore's single-plugin
`plugin-version.json` and enumerating files with `git ls-files` filtered by
`git check-attr export-ignore`. Write the new version into both manifests
through the existing `claude_manifest` / `codex_manifest` writers. For the
check, copy the tracked tree to a temp dir, run the build there, and diff.

## Out of scope

- A general installed-vs-source drift doctor for all plugins (finding
  plugins-04; tracked elsewhere).
- Changing the bento marketplace entry to pin versions.

## Priority

P2: at least one consumer runs a four-month-old build, but nothing is broken
beyond missing fixes.

## Type / Labels

chore; release, packaging, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none (the migration is a filing precondition; the AGENTS.md
  line is conditional on AGENTS.md existing).
- Related: tracker-migration-and-agents-md (AGENTS.md links here for the
  version rule). clap-cobra-extractors and its follow-ups reach consumers
  only through a bump this issue automates.
