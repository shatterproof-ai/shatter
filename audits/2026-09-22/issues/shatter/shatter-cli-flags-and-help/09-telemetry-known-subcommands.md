---
slug: telemetry-known-subcommands
kind: new
title: "Telemetry KNOWN_SUBCOMMANDS is stale: sanitized_args redacts most real subcommand tokens and lists 3 nonexistent ones"
priority: P3
type: bug
labels: [telemetry, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Telemetry KNOWN_SUBCOMMANDS is stale: sanitized_args redacts most real subcommand tokens and lists 3 nonexistent ones

## Problem

`telemetry::sanitize_args` uses a hand-maintained allow-list in core, `KNOWN_SUBCOMMANDS`, to decide which bare argv tokens are kept verbatim; any other non-flag token is redacted. The list has drifted from the clap definition: it names three commands that do not exist and omits most real ones. So for `spec-diff`, `doctor`, `list-targets`, `observe` and others, the subcommand token (and nested subcommand tokens such as `cache clear`) is redacted in the `sanitized_args` field of `command_run` events and in `bad_cli_args` events.

The command's identity itself is **not** lost in `command_run`: its `subcommand` field is filled independently from the parsed clap variant (`subcommand_label`, passed straight into `EventPayload::CommandRun` in `queue_command_run_event`, `shatter-cli/src/main.rs:1543-1555`). The defect is limited to argv redaction: `sanitized_args` for those commands, and `bad_cli_args` events (`main.rs:96-110`), where no parsed subcommand exists and `sanitized_args` is the only record of what was typed.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-core/src/telemetry.rs:53-65` `KNOWN_SUBCOMMANDS` = explore, scan, **export**, **spec**, run, analyze, init, stale, telemetry, help, **version**. `export`, `spec` and `version` are not subcommands.
- Used by `sanitize_args` at `telemetry.rs:263` (`let known_subcommands: HashSet<&str> = KNOWN_SUBCOMMANDS...`; "Rule 2: Preserve known subcommands"). Proptests at `:1120-1139` sample from the same list, so they cannot detect drift.
- Callers: `queue_command_run_event` (`main.rs:1543`, `sanitized_args` field) and the clap-error path in `main.rs:96-110` (`bad_cli_args`).
- Missing real subcommands include spec-diff, diff, doctor, list-targets, observe, bench, properties, revalidate, solve, specify, compare, nondeterminism, workspace, build-frontend, discover-deps, cache and test (compare with `shatter --help`).
- `shatter telemetry status` reports enabled by default.
- Finding cli-ux-18, verified at P3.

## Acceptance criteria

- [ ] Core no longer hardcodes the subcommand list. It is derived from `Cli::command()` (all subcommand names, including nested ones such as `cache clear`) in shatter-cli and passed into `sanitize_args`, keeping the cli → core dependency direction. Alternatively, if the list must stay in core, a shatter-cli test asserts it equals the clap subcommand name set.
- [ ] A shatter-cli test iterates every clap subcommand name and asserts `sanitize_args(&[name, "/some/path"])` keeps `name` verbatim in the returned `sanitized_args` and still redacts the path. It fails on current HEAD for `spec-diff`; the close comment records the failing and passing runs.
- [ ] A test asserts that a misspelled subcommand in a `bad_cli_args` event (for example `spec-dif`) is redacted, so free-form user text is never kept just because it resembles a command.
- [ ] Unknown or free-form argv values are still redacted, and the existing redaction proptests still pass.
- [ ] `export`, `spec` and `version` are no longer preserved unless they become real subcommands.
- [ ] When `diff` is removed by retire-snapshot-diff, the derived list follows with no manual edit.
- [ ] `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Add a `known_subcommands: &[&str]` (or owned) parameter to `sanitize_args`, filled from clap in shatter-cli.

## Out of scope

- The `command_run.subcommand` field, which is already correct.
- Other telemetry fields and the deferred telemetry epic (str-rlbq).

## Dependencies

- Blocked by: none.
- Related: str-rlbq (deferred telemetry epic), str-jhks (closed; added `command_run`), retire-snapshot-diff.

## Source

Audit 2026-09-22 finding cli-ux-18 (areas/cli-ux.md F18); old draft `drafts/shatter-code/33-telemetry-known-subcommands.md`.
