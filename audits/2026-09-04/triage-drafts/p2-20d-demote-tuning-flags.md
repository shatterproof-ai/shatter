---
repo: shatter
type: task
priority: 2
labels: cli, config
existing: none
---
# Triage: demote rarely-used tuning flags from the CLI to config-file/--set only

Triage: maintainer decision required — which flags are demoted and under what deprecation policy. Child of the p2-20 epic.

## Problem
`explore` exposes 79 flags and `scan` 69; a large share are strategy-tuning knobs with config-file equivalents that a user sets once per project, not per invocation. They inflate `--help`, the stack-budgeted `Cli` enum, and the SPEC tables, and every one must be kept in parity across explore/scan.

## Current code facts
- Candidates with config equivalents in `config.rs` (`ExplorationConfig` :778-799, `GeneticConfig` :669-692, `FuzzConfig` :728-744, `ProjectConfig` :527-534): `--score-window`, `--cold-start`, `--strategy-floor`, `--strategy-weights`, `--genetic-population`, `--genetic-generations`, `--genetic-timeout`, `--loop-buckets`, `--candidate-queue-capacity`, `--observer-pool`, `--refine-budget`, `--shrink-budget`, `--parallelism-min`, `--parallelism-max`.
- Global `--set KEY=VALUE` already overrides any config key per invocation (`help-top.txt`; precedence "above `.shatter/config.yaml`, below dedicated flags").
- `demo/gauntlet.sh` exercises some of these flags; SPEC §2.1 "Explorer strategy" table lists them.

## Options
1. **Demote with hidden aliases for one release (proposed default)**: keep each flag as `#[arg(hide = true)]` that prints a one-line deprecation warning pointing at `--set defaults.exploration.<key>=…`, remove from `--help`, SPEC and gauntlet in this release; delete the hidden flags in the next continuous-build cycle (record the removal date in SPEC §8).
2. **Keep all flags**, only regroup under `help_heading` "Advanced" (str-9ee5) — no surface reduction.
3. **Hard removal now** — rejected: breaks scripted invocations without warning.

## Acceptance checks
- Decision recorded; if option 1: list of demoted flags fixed in the issue; hidden aliases + warnings implemented; SPEC §2.1/§2.2 rows moved to a "config-only tuning keys" table in §3.6; gauntlet steps switched to `--set`; removal follow-up issue filed with a date.

## Scope
In: the listed flags. Out: user-facing budget/output flags.

## Size
medium.

## Provenance
Audit 2026-09-04, section 11, action item 20; evidence audits/2026-09-04/design-foundation.md §2.3; usability-ui.md §1.
