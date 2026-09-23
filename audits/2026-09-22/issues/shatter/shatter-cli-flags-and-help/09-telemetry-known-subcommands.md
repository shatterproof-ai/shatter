---
slug: telemetry-known-subcommands
kind: new
title: "Telemetry KNOWN_SUBCOMMANDS is stale: 3 nonexistent commands listed, real ones redacted"
priority: P3
type: bug
labels: [telemetry, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Telemetry KNOWN_SUBCOMMANDS is stale: 3 nonexistent commands listed, real ones redacted

## Problem

A hand-maintained allow-list in core decides which subcommand names are reported in `command_run` telemetry events. Names not on the list are redacted. The list has drifted from the clap definition: it names three commands that do not exist and omits most real ones. Telemetry is on by default, so the data collected for spec-diff, doctor, list-targets, observe and others is useless.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`):

- `shatter-core/src/telemetry.rs:53-65` `KNOWN_SUBCOMMANDS` = explore, scan, **export**, **spec**, run, analyze, init, stale, telemetry, help, **version**. `export`, `spec` and `version` are not subcommands.
- Used at `telemetry.rs:263` (`let known_subcommands: HashSet<&str> = KNOWN_SUBCOMMANDS...`). Proptests at `:1120-1139` sample from the same list, so they cannot detect drift.
- Missing real subcommands include spec-diff, diff, doctor, list-targets, observe, bench, properties, revalidate, solve, specify, compare, nondeterminism, workspace, build-frontend, discover-deps, cache and test (compare with `shatter --help`).
- `shatter telemetry status` reports enabled by default.
- Finding cli-ux-18, verified at P3.

## Acceptance criteria

- [ ] Core no longer hardcodes the subcommand list. It is derived from `Cli::command().get_subcommands()` in shatter-cli and passed into the telemetry call, which keeps the cli → core dependency direction. Alternatively, if the list must stay in core, a shatter-cli test asserts it equals the clap subcommand set.
- [ ] A test fails if a clap subcommand is added without being reportable, for example by iterating the clap subcommands and asserting each is recorded unredacted. It is shown failing on current HEAD (for example for `spec-diff`).
- [ ] Unknown or free-form argv values are still redacted, and the existing redaction proptests still pass.
- [ ] When `diff` is removed by retire-snapshot-diff, the derived list follows with no manual edit.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Add a `known_subcommands: &[&str]` (or owned) parameter to the telemetry entry point, filled from clap in shatter-cli.

## Out of scope

- Other telemetry fields and the deferred telemetry epic (str-rlbq).

## Dependencies

- Blocked by: none.
- Related: str-rlbq (deferred telemetry epic), str-jhks (closed; added `command_run`), retire-snapshot-diff.

## Source

Audit 2026-09-22 finding cli-ux-18 (areas/cli-ux.md F18); old draft `drafts/shatter-code/33-telemetry-known-subcommands.md`.
