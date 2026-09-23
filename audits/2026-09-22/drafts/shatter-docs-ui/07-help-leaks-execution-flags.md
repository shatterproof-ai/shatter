# Execution-only global flags still appear in help for non-executing commands (`help <cmd>` path and 9 commands missed by str-qwua7.15)

- Priority: P2
- Type: bug
- Labels: cli,ux,usability
- Tracker action: new issue (follow-up to str-qwua7.15, which was closed with reason "Closed" and only partly fixed)
- Related: str-qwua7.15, str-qwua7.20.1, str-9ee5
- Source findings: audit 2026-09-22 prior-07, docs-22 (confirmed). The design half is L4 finding cli-ux-05 (replace the argv-scanning workaround with a flattened ExecOptions); fold it in if filed.

<!-- body -->
## Problem
str-qwua7.15 was meant to hide execution-only global flags (`--allow-host-writes`, `--set`, `--timing*`) on commands that never execute targets. The fix scans raw argv and only intercepts `<cmd> --help`, and only for a hard-coded list of five commands.

## Evidence (target/debug/shatter at HEAD)
- `shatter spec-diff --help` has no `--allow-host-writes`. `shatter help spec-diff` (78 lines, against 48) shows `--allow-host-writes` at line 50 and `--set` at line 55. `help doctor` also leaks.
- `--help` for diff, compare, stale, analyze, solve, specify, list-targets, nondeterminism and build-frontend each still lists `--allow-host-writes`. SPEC §2.10 (`SPEC.md:591-596`) says these commands never execute targets.

## Current code facts
- `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help` intercepts argv before clap parses it.
- `shatter-cli/src/args.rs:324` `NON_EXECUTING_COMMAND_PATHS` lists spec-diff, init, doctor, cache and telemetry.
- `shatter-cli/src/host_writes.rs:83-94` `command_executes_targets()` already exists as the single source of truth.

## Acceptance criteria
- For every subcommand where `command_executes_targets()` is false, neither `shatter <cmd> --help` nor `shatter help <cmd>` shows execution-only flags.
- A test iterates over all subcommands and asserts help contents against `command_executes_targets()`, in both help forms.
- The argv-scanning helpers (`help_only_command`, `resolve_subcommand_path`) are removed, or justified if kept.

## Suggested approach
Preferred: move the execution-only globals into a flattened `ExecOptions` struct included only by executing commands (the str-qwua7.20.1 direction). Minimal fallback: drive the hiding from `command_executes_targets()` and hook clap's `help` subcommand path as well.

## Scope
In: help surface and flag scoping. Out: help_heading grouping (str-9ee5), and tracker IDs in help text (separate issue).
