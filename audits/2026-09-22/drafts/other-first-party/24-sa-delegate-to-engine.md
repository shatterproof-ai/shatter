# run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: task
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (symptoms sa-d8j, sa-c2q, sa-oio open) -> new root-cause issue
- source findings: plugins-10

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`catalog/skills/run-shatter/scripts/run_targets.py` walks Cargo.toml/go.mod/package.json on its own. The open bugs sa-d8j (`.shatter/cache/harness` Cargo.toml counted as a target) and sa-c2q (Make wrappers invisible) are artifacts of this parallel discovery. `catalog/skills/shatter-doctor/SKILL.md` (around line 71) parses config with `python3 -c "import yaml ..."`, which uses PyYAML and is not stdlib, with a fallback at around line 78. It never calls the engine's own commands.

## Current code facts

- shatter `shatter-cli/src/args.rs`: `ListTargets` (around line 1767, supports `--format json`) and `Doctor` (around line 1777: embed staleness plus gitignore coverage; it does **not** validate config.yaml).

## Acceptance criteria

- [ ] shatter-doctor runs `shatter doctor -d <root>` and `shatter list-targets --format json` and reports their output. It keeps only the config checks the engine does not cover, with no PyYAML dependency.
- [ ] run_targets.py gets target roots from `shatter list-targets --format json`. Wrapper detection remains as the integration layer only.
- [ ] A regression test shows a `.shatter/cache/harness/Cargo.toml` is not a target (closes sa-d8j), or sa-d8j is updated with why not.

## Out of scope

The sa-oio wrapper-subcommand fix (tracked separately).

## Source

Shatter audit 2026-09-22 finding plugins-10.
