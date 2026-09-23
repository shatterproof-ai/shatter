---
slug: cli-contract-test
kind: new
title: "Add a contract test between catalog skills and a pinned shatter CLI, plus status/requires metadata that build-plugins honours"
priority: P2
type: task
labels: [testing, cli-contract, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Add a contract test between catalog skills and a pinned shatter CLI, plus status/requires metadata that build-plugins honours

## Problem

Nothing checks that the `shatter` subcommands and flags cited in shatter-agents skills exist in the shatter CLI. shatter-agents CI runs only `scripts/check-plugins-clean` and `python -m pytest tests/`, and no test invokes a shatter binary. The shatter repo does not reference shatter-agents at all, and its own CLI-surface drift check (str-wurp, still open) covers only shatter's SPEC.md and gauntlet. As a result, skills that cite nonexistent commands shipped undetected: `shatter diff --staged` (withdraw-shatter-diff-skill) and per-recipe runs (recipes-marked-design-only).

There is also no way to mark a skill as documenting a future command. `catalog/skills/*/metadata.json` carries only `recommended_model` and `audience`, and `scripts/build-plugins` has no status or requires handling. So every catalog skill listed in `catalog/plugins.json` ships.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `.github/workflows/ci.yml` has two jobs: `build-clean` (`scripts/check-plugins-clean`) and `tests` (`python -m pytest tests/ -v`).
- `tests/` has 13 test files (for example `test_skills_load.py` and `test_run_targets.py`). None runs `shatter` or parses `--help`.
- `grep -n "status\|requires" scripts/build-plugins` returns 0 matches. `_is_ignored` (around line 83) skips only `__pycache__` and `.pyc`.
- In the shatter repo, `grep -r "shatter-agents\|shatterproof-ai/agents"` (excluding `target/`, `audits/` and `.beads/`) finds nothing. The `cli-surface-drift` check in `docs/DRIFT-PATROL.md` is still pending on str-wurp.
- The shatter project has **no published GitHub release**: `gh release list -R shatterproof-ai/shatter` is empty. It is tracked in the shatter repo as `release-publish-and-install-smoke`. For now, a pinned binary therefore has to be built from a pinned shatter commit, not downloaded.

## Acceptance criteria

- [ ] A pytest test (for example `tests/test_cli_contract.py`) extracts every `shatter <subcommand> [--flag ...]` invocation from `catalog/**/*.md` and `catalog/**/scripts/*`, including fenced code blocks and inline code spans. It then validates each subcommand and long flag against `shatter <sub> --help` from a pinned shatter build, and fails on an unknown subcommand or flag. Placeholders (`<...>`) and positional arguments are ignored.
- [ ] The pinned build is named in one place in the repo (a shatter commit SHA, or a `continuous-*` BUILD tag once releases publish). CI obtains that binary, either by building shatter at the pinned SHA with a cache or by downloading the pinned release. The test is skipped with a visible reason when no binary is available locally, but it is **required** in CI.
- [ ] Skill `metadata.json` accepts `status` (`released` | `experimental` | `unreleased`) and `requires_shatter` (a minimum build or an upstream issue id). `scripts/build-plugins` excludes non-released skills from both the Claude and Codex payloads. A test in `tests/test_build_plugins.py` proves the exclusion.
- [ ] Proof at close: link a CI run in which the contract test executed (not skipped) and passed. Also include a local transcript showing the test failing against a checkout that still ships the `shatter diff --staged` text (before withdraw-shatter-diff-skill), or failing on a deliberately injected bogus flag.

## Suggested approach

Parse each subcommand's `--help` output for the `Commands:` and `Options:` sections, and cache the resulting inventory per run. Keep the extractor conservative: only lines or spans that begin with `shatter `. Start the CI job by building `shatter-cli` at the pinned SHA with `cargo build -p shatter-cli --release` and a cargo cache, and switch to `install.sh` with `BUILD=` once shatter publishes releases.

Also, once shatter retires the snapshot `shatter diff` (shatter `retire-snapshot-diff`), bumping the pin should make any stray `shatter diff` mention fail this test.

## Out of scope

- The shatter-side drift-patrol check against this catalog. That is an optional advisory in the shatter repo (str-wurp).
- Fixing the individual skills. Those are withdraw-shatter-diff-skill, recipes-marked-design-only and sa-oio.

## Dependencies

- None within this tracker. The status/requires mechanism can land first, and withdraw-shatter-diff-skill may reuse it.
- Cross-repo context: shatter `release-publish-and-install-smoke` (downloadable pinned builds) and str-wurp.

## Priority / Type / Labels

P2 (the verifier corrected the finding's P1 to P2: this is a missing gate, and the P1 defects it let through are filed separately) · task · testing, cli-contract, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03.
