---
slug: cli-contract-test
kind: new
title: "Add a CLI-syntax contract test between published skills and a pinned shatter build"
priority: P2
type: task
labels: [testing, cli-contract, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: [withdraw-shatter-diff-skill, recipes-marked-design-only]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Add a CLI-syntax contract test between published skills and a pinned shatter build

## Problem

Nothing checks that the `shatter` subcommands and flags cited in published shatter-agents skills exist in the shatter CLI. shatter-agents CI runs only `scripts/check-plugins-clean` and `python -m pytest tests/`, and no test invokes a shatter binary. The shatter repo does not reference shatter-agents at all, and its own CLI-surface drift check (str-wurp, still open) covers only shatter's SPEC.md and gauntlet. As a result, `shatter diff --staged` (withdraw-shatter-diff-skill) shipped undetected.

**What this test does and does not protect.** It checks CLI *syntax* only: every cited subcommand and long flag exists in the pinned build's `--help`. It cannot tell whether a skill's *described behaviour* happens (for example, that recipes are discovered or a config key is consumed; recipes-marked-design-only would **not** have been caught by it). Behavioural checks stay with the per-skill tests that exercise real behaviour: the wrapper run test proposed on sa-oio, the rendered-workflow test in wire-shatter-ci-standalone, and the hook-recovery test in withdraw-shatter-diff-skill.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter `70465921`.

- `.github/workflows/ci.yml` has two jobs: `build-clean` (`scripts/check-plugins-clean`) and `tests` (`python -m pytest tests/ -v`).
- `tests/` has 12 `test_*.py` files plus `fixtures/` (for example `test_skills_load.py` and `test_run_targets.py`). None runs `shatter` or parses `--help`.
- In the shatter repo, `grep -r "shatter-agents\|shatterproof-ai/agents"` (excluding `target/`, `audits/` and `.beads/`) finds nothing. The `cli-surface-drift` check in `docs/DRIFT-PATROL.md` is still pending on str-wurp.
- The shatter project has **no published GitHub release**: `gh release list -R shatterproof-ai/shatter` is empty. That is tracked in the shatter repo as <release-publish-and-install-smoke id> (audit slug `release-publish-and-install-smoke`, bucket shatter-ci-workflows; the filer substitutes the id). A pinned binary therefore has to be built from a pinned shatter commit for now.
- Building `shatter-cli` is not a bare `cargo build`: it links Z3 and embeds the Go, TypeScript and Rust frontends, so a build job needs Z3 headers/libs, a Go toolchain and Node, as shatter's own `.github/workflows/ci.yml` sets up.

## Acceptance criteria

- [ ] **Scope is the published payload.** A pytest test (for example `tests/test_cli_contract.py`) extracts every `shatter <subcommand> [--flag ...]` invocation from the built payload files, `plugins/claude/**` and `plugins/codex/**` (SKILL.md, `references/`, `scripts/`), including fenced code blocks and inline code spans. Unshipped catalog material (design proposals, withdrawn skills) is out of scope by construction.
- [ ] **Explicit negative-example exemption.** A line or fenced block may be exempted only with an inline marker, `<!-- cli-contract: ignore -- <reason> -->` on the preceding line; the test lists every exemption it honoured in its output, and a unit test shows an unmarked bogus flag fails while a marked one passes.
- [ ] The test validates each subcommand and long flag against `shatter <sub> --help` from the pinned build and fails on an unknown subcommand or flag. Placeholders (`<...>`) and positional arguments are ignored.
- [ ] The pinned shatter commit SHA (later, a `continuous-*` BUILD tag once releases publish) is named in one file in the repo. CI obtains that binary by one of: building shatter at the pinned SHA in a job that installs Z3, Go and Node (cached), or downloading a binary artifact from a shatter CI run at that SHA. The job name and mechanism are documented in AGENTS.md.
- [ ] Locally the test skips with a visible reason when no binary is found. In CI it is required: the CI job sets `SHATTER_CONTRACT_REQUIRED=1`, and under that variable a missing binary is a failure, not a skip (a unit test covers both branches).
- [ ] Proof at close: (1) a CI run URL where the contract test executed (not skipped) and passed; (2) a local transcript of the test failing when run against `119b807` (which still ships `shatter diff --staged`), and passing on the branch.

## Suggested approach

Parse each subcommand's `--help` for the `Commands:` and `Options:` sections and cache the inventory per run. Keep the extractor conservative: only lines or spans that begin with `shatter `. Once shatter retires the snapshot `shatter diff` (<retire-snapshot-diff id>), bumping the pin makes any stray `shatter diff` mention fail.

## Out of scope

- Behavioural verification of skills (see Problem).
- Skill `status` / `requires_shatter` metadata honoured by build-plugins. That is skill-status-metadata.
- The shatter-side drift-patrol check against this catalog (str-wurp).
- Fixing individual skills: withdraw-shatter-diff-skill, recipes-marked-design-only, sa-oio.

## Dependencies

- Blocked by withdraw-shatter-diff-skill and recipes-marked-design-only: the required CI test fails while `shatter diff --staged` still ships.
- Cross-repo context (not bd dependencies): shatter <release-publish-and-install-smoke id> (downloadable pinned builds), <retire-snapshot-diff id>, str-wurp.

## Priority / Type / Labels

P2 (the verifier corrected the finding's P1 to P2: this is a missing gate, and the P1 defects it let through are filed separately) · task · testing, cli-contract, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03. Revised after the Codex cross-check (findings 3, 4, 10); the metadata half was split out to skill-status-metadata.
