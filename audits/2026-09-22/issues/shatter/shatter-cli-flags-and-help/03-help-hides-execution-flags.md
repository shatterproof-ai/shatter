---
slug: help-hides-execution-flags
kind: new
title: "Scope execution-only flags to executing commands: `help <cmd>` and 10 non-executing commands still show --allow-host-writes/--set (replace the str-qwua7.15 argv intercept)"
priority: P2
type: bug
labels: [cli, usability, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scope execution-only flags to executing commands: `help <cmd>` and 10 non-executing commands still show --allow-host-writes/--set (replace the str-qwua7.15 argv intercept)

## Problem

str-qwua7.15 ("Hide execution-only global flags on non-executing commands") was closed on 2026-09-14 with the bare reason "Closed" and only a partial fix. The execution-only flags (`--allow-host-writes`, `--set`, `--timing*`) are still `global = true` on `Cli`. The fix hides them only when raw argv looks like `<cmd> --help` for one of five hard-coded commands. It does this by guessing the subcommand from argv and rendering help from a second, mutated clap `Command`. The commit notes that the natural fix (`mut_arg(... hide)`) broke clap parsing, so the workaround was chosen instead.

Three gaps follow:

1. `shatter help <cmd>` goes through clap's own `help` subcommand and bypasses the intercept, so `help spec-diff` and `help doctor` still show the flags.
2. SPEC §2.10 names more non-executing commands than the five in the list. `--help` still shows `--allow-host-writes` for analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend.
3. The flags still *parse* on non-executing commands: `spec-diff --allow-host-writes a b` is accepted.

The codebase already has a single source of truth, `command_executes_targets()`, but the help logic does not use it.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built in the `audit-2026-09-22` worktree (HEAD `56c86168`):

- `shatter help spec-diff | wc -l` → 78; line 50 is `--allow-host-writes` and line 55 is `--set <KEY=VALUE>`. `shatter spec-diff --help | wc -l` → 48, with neither flag.
- `shatter doctor --help | grep -c allow-host-writes` → 0, but `shatter help doctor | grep -c allow-host-writes` → 1.
- `<cmd> --help | grep -c allow-host-writes` → 1 for each of analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend. `help <cmd>` gives the same result.
- Code:
  - `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help(raw_args)`, called at `main.rs:95` before clap parses.
  - `shatter-cli/src/args.rs:324` `NON_EXECUTING_COMMAND_PATHS` (spec-diff, init, doctor, cache, telemetry); `args.rs:344` `help_only_command()`; `args.rs:402` `resolve_subcommand_path()`.
  - `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()`: true only for Explore, Scan, Run, Observe, Bench, Properties and Revalidate.
- SPEC §2.10 (`SPEC.md:591-596`) lists analyze, solve, specify, stale, diff, spec-diff, compare, list-targets and doctor as never executing targets.
- Transcripts: `audits/2026-09-22/cli-ux-transcripts/help/help-subcommand-spec-diff.txt` compared with `help/spec-diff.txt`.
- Findings cli-ux-05 and prior-07 (both verified at P2) and docs-22 (P3, covered by this issue).

## Acceptance criteria

- [ ] The execution-only options (`--allow-host-writes`, `--set`, `--timing`, `--timing-*`) are defined only on commands for which `command_executes_targets()` is true, for example through a flattened `ExecOptions` struct. They are no longer `global = true` on `Cli`.
- [ ] `maybe_print_non_executing_help`, `help_only_command`, `resolve_subcommand_path` and `NON_EXECUTING_COMMAND_PATHS` are deleted.
- [ ] A test walks every subcommand of `Cli::command()`, including nested ones, and for each asserts that both `shatter <cmd> --help` and `shatter help <cmd>` contain `--allow-host-writes` and `--set` if and only if the command executes targets. The same predicate drives the test and the code, or the test cross-checks `command_executes_targets()` against a table that lists every subcommand. The test fails on current HEAD (at least for `help spec-diff` and `analyze --help`) and passes after the fix; record both runs in the close comment.
- [ ] `shatter spec-diff --allow-host-writes a.json b.json` is rejected as a usage error (exit 2), and a test covers it.
- [ ] The `command_executes_targets()` list is reviewed against SPEC §2.10 and the actual behavior of every command not in either list (for example `test`, `discover-deps`, `build-frontend`, `workspace`). SPEC §2.10 is updated so the two agree.
- [ ] `SHATTER_ALLOW_HOST_WRITES` and config-file behavior are unchanged for executing commands; the existing host-write tests still pass.
- [ ] `task affected` passes, plus `task gauntlet`, since this changes flag parsing on many commands. The close comment records the gates selected.

## Suggested approach

Move the execution-only globals into a `#[command(flatten)] exec: ExecOptions` field on each executing subcommand. That is the direction of str-qwua7.20.1 (CLI argument-surface epic). Update `host_writes.rs` and the `--set` and timing plumbing to read from the subcommand, not from `Cli`. The code comment in `main.rs` about why `mut_arg(hide)` failed is no longer relevant once the flags are not global. If flattening is not practical, the fallback is to hide the args with `mut_subcommand` driven by `command_executes_targets()`. That fallback must also cover the `help` subcommand path, and it still does not fix gap 3, so say so in the close comment if it is chosen.

Note that the snapshot `shatter diff` command is being retired (retire-snapshot-diff, maintainer decision D2). If that lands first, drop `diff` from the checks. Otherwise the table covers it like any other non-executing command.

## Out of scope

- Grouping help output with `help_heading` (str-9ee5).
- Tracker IDs in help text (help-tracker-ids-lint).
- Adding `--seed` to more commands (seed-for-explore-and-run). If `ExecOptions` lands first, that issue can reuse it.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15 (closed, partial; see help-flags-reopen-note), str-qwua7.20.1 (shared options struct), str-9ee5, retire-snapshot-diff.

## Source

Audit 2026-09-22 findings cli-ux-05 (areas/cli-ux.md F5), prior-07 (areas/prior-audit-regress.md) and docs-22. Merges the old drafts `drafts/shatter-code/28-help-only-argv-intercept.md` (the design fix) and `drafts/shatter-docs-ui/07-help-leaks-execution-flags.md` (the symptom).
