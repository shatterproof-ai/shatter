---
slug: automatic-version-bump
kind: new
title: "Adopt automatic content-hash plugin version bumps with a staleness check; installed storystore cache is 128 commits behind"
priority: P2
type: chore
labels: [release, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [tracker-migration-and-agents-md]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# Adopt automatic content-hash plugin version bumps with a staleness check; installed storystore cache is 128 commits behind

## Problem

storystore's plugin version has stayed at 0.1.1 since 2026-05-25. The bento
marketplace lists storystore with no pinned version (github source), so
Claude Code keys its plugin cache on the manifest version. If the version
does not change, consumers keep the old cache. Every consumer is on a build
from 2026-05-10, 128 commits behind `main`. That build lacks
`shared/impact_trigger.py` and has older copies of six shared scripts,
including `inventory.py`, the file the clap/cobra extractor issue changes.
Nothing bumps the version automatically, and nothing fails when shipped
content changes without a bump.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d)

- `cat plugin-version.json` → `{"version": "0.1.1"}`.
  `git log --oneline -1 -- plugin-version.json` → `147af27 Bump plugin version to 0.1.1`.
  `git log --oneline 147af27..HEAD | wc -l` → 65.
- `~/.claude/plugins/installed_plugins.json`: `storystore@bento` version
  0.1.1, `gitCommitSha` ca16aef4d418..., installPath
  `~/.claude/plugins/cache/bento/storystore/0.1.1`.
  `git rev-list --count ca16aef4..HEAD` → 128.
- The cache's `shared/` has audit.py, coverage.py, drift_todo.py,
  edit_section.py, impact_check.py, inventory.py, list_candidates.py,
  lock_check.py, storystore_lib.py and write_story.py, but no
  `impact_trigger.py`. The audit's `diff -rq` found that audit.py,
  coverage.py, drift_todo.py, impact_check.py, inventory.py and
  list_candidates.py differ from the repo.
- `scripts/build-plugin` reads `VERSION_FILE = ROOT / "plugin-version.json"`
  and never changes it.
- **storystore has no `.github/workflows/`**, so there is no CI to fail. The
  test suite (`python3 -m pytest tests/ -x -q`, per README "Test") is the
  only automated gate.
- Precedent: shatter-agents `scripts/build-plugins:187-210`
  (`compute_plugin_hash` sha256 over the built plugin dir;
  `bump_version_if_changed` bumps patch when `content_hash` changes, stored
  in `catalog/plugin-versions.json`) plus `scripts/check-plugins-clean`, run
  by `.github/workflows/ci.yml` job `build-clean` (shatter-agents issue sa-8xg).
- Source finding: plugins-07. Old draft: `drafts/other-first-party/33-ss-version-bump.md`.

## Acceptance criteria

- [ ] `scripts/build-plugin` computes a content hash over the shipped
      payload (`shared/` and `skills/`, or the built plugin outputs), stores
      it next to the version (e.g. `plugin-version.json` gains
      `content_hash`), and bumps the patch version when the hash changes.
      Running it twice with no content change leaves the version unchanged.
      Tests in `tests/test_build_plugin.py` cover both cases.
- [ ] A staleness check (e.g. `scripts/check-plugin-clean`, plus a pytest
      test that runs it) fails when `shared/` or `skills/` differs from the
      recorded hash, i.e. content changed without running build-plugin. Show
      it failing on a deliberately edited shared file and passing after
      rebuild. If a GitHub Actions workflow is added, it runs this check and
      pytest on push to `main`, and the close reason includes a green run
      URL. If not, AGENTS.md names the check as a landing gate.
- [ ] The version is bumped now (0.1.2 or later). After the plugin is
      updated, a fresh install in `~/.claude/plugins/cache/bento/storystore/<new version>/shared/`
      contains `impact_trigger.py`. Paste the `ls`.
- [ ] The rule ("never hand-edit the version; build-plugin bumps it") is
      written in AGENTS.md (created by tracker-migration-and-agents-md).

## Suggested approach

Port `compute_plugin_hash` / `bump_version_if_changed` from shatter-agents
`scripts/build-plugins`, adapted to storystore's single-plugin
`plugin-version.json`. Hash in sorted path order, excluding `__pycache__`.
Write the new version into `.claude-plugin/plugin.json` and
`.codex-plugin/plugin.json` through the existing manifest writers.

## Out of scope

- A general installed-vs-source drift doctor for all plugins (finding
  plugins-04; tracked elsewhere).
- Changing the bento marketplace entry to pin versions.

## Priority

P2: consumers run a four-month-old build, but nothing is broken for them
beyond missing fixes.

## Type / Labels

chore; release, packaging, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: tracker-migration-and-agents-md (the tracker must accept
  writes; the version rule goes into its AGENTS.md).
- Related: clap-cobra-extractors. Its fix only reaches consumers once this
  issue's bump mechanism exists.
