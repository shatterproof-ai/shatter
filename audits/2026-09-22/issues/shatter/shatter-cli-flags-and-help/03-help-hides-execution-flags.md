---
slug: help-hides-execution-flags
kind: new
title: "Scope --allow-host-writes and --set to the commands that use them: `help <cmd>` and 10 non-executing commands still show them (replace the str-qwua7.15 argv intercept)"
priority: P2
type: bug
labels: [cli, usability, help, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scope --allow-host-writes and --set to the commands that use them: `help <cmd>` and 10 non-executing commands still show them (replace the str-qwua7.15 argv intercept)

## Problem

str-qwua7.15 ("Hide execution-only global flags on non-executing commands") was closed on 2026-09-14 with the bare reason "Closed" and only a partial fix. `--allow-host-writes` and `--set` are still `global = true` on `Cli`. The fix hides them only when raw argv looks like `<cmd> --help` for one of five hard-coded commands, by guessing the subcommand from argv and rendering help from a second, mutated clap `Command`. The commit notes that the natural fix (`mut_arg(... hide)`) broke clap parsing, so the workaround was chosen instead.

Three gaps follow:

1. `shatter help <cmd>` goes through clap's own `help` subcommand and bypasses the intercept, so `help spec-diff` and `help doctor` still show the flags.
2. SPEC §2.10 names more non-executing commands than the five in the list. `--help` still shows `--allow-host-writes` for analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend.
3. The flags still *parse* on non-executing commands: `spec-diff --allow-host-writes a b` is accepted and silently ignored.

The two flags have different real scopes, and this issue treats them separately:

- `--allow-host-writes` matters exactly where `command_executes_targets()` (`shatter-cli/src/host_writes.rs:84-95`) is true: Explore, Scan, Run, Observe, Bench, Properties, Revalidate.
- `--set` is consumed **only by explore** today: `cli.set_overrides` is read once in `main.rs` (`:437`, the explore dispatch) and flows into `resolve_function_config_with_inputs` (`explore.rs:4832-4838`) and `resolve_llm_config` (`explore.rs:4314`). Scan, run and the other executing commands accept it and ignore it. Moving `--set` onto every executing command would advertise a flag they do not honor.

The `--timing`, `--timing-format`, `--timing-output` and `--timing-output-dir` flags are **not** execution-only and stay global: the timing collector is installed for every command and `persist_timing_run` (defined at `main.rs:1483`, called from the common finalization at `:1456-1463`) writes a timing artifact for any subcommand, executing or not. The `// ... the four timing* flags above are ...` comment at `args.rs:285` and the `timing*` entries in the hide list (`args.rs:312-315`) are wrong and are corrected by this issue.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built in the `audit-2026-09-22` worktree (source at `56c86168`):

- `shatter help spec-diff | wc -l` → 78; line 50 is `--allow-host-writes` and line 55 is `--set <KEY=VALUE>`. `shatter spec-diff --help | wc -l` → 48, with neither flag.
- `shatter doctor --help | grep -c allow-host-writes` → 0, but `shatter help doctor | grep -c allow-host-writes` → 1.
- `<cmd> --help | grep -c allow-host-writes` → 1 for each of analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend. `help <cmd>` gives the same result.
- Code:
  - `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help(raw_args)`, called at `main.rs:95` before clap parses.
  - `shatter-cli/src/args.rs:324` `NON_EXECUTING_COMMAND_PATHS` (spec-diff, init, doctor, cache, telemetry); `args.rs:344` `help_only_command()`; `args.rs:402` `resolve_subcommand_path()`; hidden-arg list `args.rs:310-315`.
  - `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()`.
  - `set_overrides` consumers: `main.rs:437` only (grep `set_overrides` across `shatter-cli/src`; `run.rs:3371` is a test name, not a consumer).
  - Timing: `main.rs:1456-1463` calls `persist_timing_run` for every subcommand.
- SPEC §2.10 (`SPEC.md:591-596`) lists analyze, solve, specify, stale, diff, spec-diff, compare, list-targets and doctor as never executing targets.
- Transcripts: `audits/2026-09-22/cli-ux-transcripts/help/help-subcommand-spec-diff.txt` compared with `help/spec-diff.txt`.
- Findings cli-ux-05 and prior-07 (both verified at P2) and docs-22 (P3, covered by this issue).

## Acceptance criteria

- [ ] `--allow-host-writes` is no longer `global = true` on `Cli`. It is defined only on the subcommands for which `command_executes_targets()` is true (for example through a flattened `ExecOptions` struct).
- [ ] `--set` is no longer global. It is defined only on subcommands that apply it. With today's code that is `explore` only. Wiring `--set` into scan/run is a separate decision and out of scope; if the implementer wires it into another command in this change, that command gets a test proving an override changes its effective config.
- [ ] `--timing*` flags remain global and keep working on every command; a test runs a non-executing command (for example `shatter list-targets <dir> --timing summary --timing-output <file>`) and asserts the timing file is written, both before and after the change. The `args.rs:285` comment and the hide-list entries for `timing*` are removed.
- [ ] `maybe_print_non_executing_help`, `help_only_command`, `resolve_subcommand_path` and `NON_EXECUTING_COMMAND_PATHS` are deleted. The `mut_subcommand`/hide-only approach is not an acceptable resolution, because it neither removes the definitions nor rejects the flags (gap 3).
- [ ] A test walks every subcommand of `Cli::command()`, including nested ones, and for each asserts: `shatter <cmd> --help` and `shatter help <cmd>` contain `--allow-host-writes` iff `command_executes_targets()` is true for it, and contain `--set` iff it is in an explicit `SET_CONSUMERS` table (initially `[explore]`). The subcommand list is enumerated from clap, so a new subcommand without a table entry fails the test. The test fails on current HEAD (at least for `help spec-diff` and `analyze --help`) and passes after the fix; the close comment records both runs.
- [ ] `shatter spec-diff --allow-host-writes a.json b.json` and `shatter scan <dir> --set defaults.max_iterations=5` are rejected as usage errors (exit 2, clap `unexpected argument`), each covered by a test.
- [ ] Behavior change is documented: the SPEC §8 changelog (`SPEC.md:1173`; the repo has no CHANGELOG file) notes that `--set` on commands other than explore, and `--allow-host-writes` on non-executing commands, are now rejected instead of silently ignored, and that the pre-subcommand position (`shatter --allow-host-writes explore ...`) is no longer accepted if that is the result. The walkthrough, gauntlet, `demo/` scripts and docs are grepped for such uses and fixed.
- [ ] The `command_executes_targets()` list is reviewed against SPEC §2.10 and the actual behavior of every command not in either list (for example `test`, `discover-deps`, `build-frontend`, `workspace`). SPEC §2.10 is updated so the two agree; the close comment lists each command and its classification.
- [ ] `SHATTER_ALLOW_HOST_WRITES` and config-file behavior are unchanged for executing commands; the existing host-write tests still pass.
- [ ] `task affected` passes, plus `task gauntlet` and `task walkthrough`, since this changes flag parsing on many commands. The close comment records the gates selected.

## Suggested approach

Move `--allow-host-writes` into a `#[command(flatten)] exec: ExecOptions` field on each executing subcommand (the direction of str-qwua7.20.1), and move `--set` onto `explore`'s args. Update `host_writes.rs` and the `--set` plumbing to read from the subcommand, not from `Cli`. The code comment in `main.rs` about why `mut_arg(hide)` failed is no longer relevant once the flags are not global.

The snapshot `shatter diff` command is being retired (retire-snapshot-diff, maintainer decision D2). If that lands first, `diff` drops out of the table automatically because the test enumerates clap.

## Out of scope

- Grouping help output with `help_heading` (str-9ee5).
- Tracker IDs in help text (help-tracker-ids-lint).
- Adding `--seed` to more commands (seed-for-explore-and-run). If `ExecOptions` lands first, that issue can reuse it.
- Making scan/run honor `--set` (a feature decision, not filed here).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.15 (closed, partial; see help-flags-reopen-note), str-qwua7.20.1 (shared options struct), str-9ee5, retire-snapshot-diff, unknown-config-keys-warn (also touches `--set`).

## Source

Audit 2026-09-22 findings cli-ux-05 (areas/cli-ux.md F5), prior-07 (areas/prior-audit-regress.md) and docs-22. Merges the old drafts `drafts/shatter-code/28-help-only-argv-intercept.md` (the design fix) and `drafts/shatter-docs-ui/07-help-leaks-execution-flags.md` (the symptom).
