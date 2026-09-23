---
slug: delegate-discovery-to-engine
kind: new
title: "run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery"
priority: P2
type: task
labels: [skills, run-shatter, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery

## Problem

`catalog/skills/run-shatter/scripts/run_targets.py` walks the tree for `Cargo.toml`, `go.mod` and `package.json` itself, instead of asking the engine which targets it sees. Two open bugs come straight from this parallel discovery:

- sa-d8j: the generated `.shatter/cache/harness/Cargo.toml` is counted as a target.
- sa-c2q: Make wrappers are invisible to run-shatter.

`catalog/skills/shatter-doctor/SKILL.md` validates `.shatter/config.yaml` with `python3 -c "import sys, yaml; ..."`. That needs PyYAML, which is not in the Python standard library, so the skill falls back to a byte count when PyYAML is missing. The skill never calls the engine's own `shatter doctor` or `shatter list-targets`.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter audit HEAD.

- `catalog/skills/shatter-doctor/SKILL.md:71` has `python3 -c "import sys, yaml; yaml.safe_load(open(sys.argv[1]))" .shatter/config.yaml`. Lines 78-80 are the fallback "parse skipped: pyyaml not installed".
- `grep -n "shatter doctor\|list-targets"` over `catalog/skills/shatter-doctor/SKILL.md`, `catalog/skills/run-shatter/SKILL.md` and `run_targets.py` returns 0 matches.
- shatter `shatter-cli/src/args.rs:1767` defines `ListTargets(ListTargetsArgs)`, with `--format` (`ListTargetsFormat`, including json) at around line 1810. Line 1777 defines `Doctor`, which checks embed staleness and gitignore coverage. It does **not** validate config.yaml, so it complements the skill's config check rather than replacing it.
- sa-d8j (P2) is OPEN, and sa-c2q (P1) and sa-oio (P1) are OPEN.

## Acceptance criteria

- [ ] shatter-doctor runs `shatter doctor -d <root>` and `shatter list-targets --format json` (flags as shown by the current build's `--help`) and reports their output. It keeps only the config checks the engine does not cover, and those checks need no third-party Python module. "Uses only the standard library" is enforced by a test that runs the check under `python3 -S -I` or with PyYAML absent.
- [ ] `run_targets.py` gets target roots from `shatter list-targets --format json`. Its own file walking is removed, and it keeps only wrapper detection (package.json, Taskfile, Makefile) as the integration layer on top of engine targets.
- [ ] A regression test in `tests/test_run_targets.py` builds a fixture that contains `.shatter/cache/harness/Cargo.toml` and asserts that the file is not reported as a target. The test fails on the current code and passes after the change, which closes sa-d8j. If it cannot, sa-d8j is updated to say why.
- [ ] sa-c2q is updated to say whether engine-based discovery resolves it.
- [ ] `python -m pytest tests/` and `scripts/check-plugins-clean` pass (output pasted in the close comment).

## Suggested approach

Add a small helper in `run_targets.py` that shells out to `shatter list-targets --format json` and maps the returned roots to wrappers. Keep a clear error when the `shatter` binary is missing, because shatter-doctor already reports that. In tests, stub the binary with a fixture script that prints canned JSON, and put one real-binary case behind the cli-contract-test pin.

## Out of scope

- The wrapper subcommand fix (sa-oio; see the sa-oio-wrapper-convention note).
- Adding config.yaml validation to `shatter doctor` in the engine. If that is wanted, it is a shatter-repo issue.

## Dependencies

- None within this tracker.
- Related: sa-d8j, sa-c2q, sa-oio, cli-contract-test.

## Priority / Type / Labels

P2 · task · skills, run-shatter, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-10.
