# str-qwua7.15 fix is an argv-scanning workaround: `shatter help <cmd>` and 9 non-executing commands still show execution-only globals

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | cli,usability,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.15, str-qwua7.20.1, str-9ee5 |
| source findings | cli-ux-05, prior-07 (L6) |

<!-- body -->
## Problem

Execution-only global flags (`--allow-host-writes`, `--set`, `--timing*`) are hidden only for `<cmd> --help` on five commands, via a second mutated clap tree chosen by guessing the subcommand from raw argv. `shatter help spec-diff` bypasses it, and analyze/solve/specify/stale/diff/compare/list-targets/nondeterminism/workspace (non-executing per SPEC §2.10) still show and accept the flags.

## Current code facts / evidence

- `shatter-cli/src/main.rs:70` `maybe_print_non_executing_help`; `shatter-cli/src/args.rs:280-420` `help_only_command`/`resolve_subcommand_path`; `NON_EXECUTING_COMMAND_PATHS` (args.rs:324) = spec-diff, init, doctor, cache, telemetry.
- `shatter help spec-diff` = 78 lines incl. --allow-host-writes and --set; `spec-diff --help` = 48.
- `shatter-cli/src/host_writes.rs:83-94` `command_executes_targets()` is an existing single source of truth.

## Acceptance criteria

- Execution-only options live in a flattened `ExecOptions` struct included only by executing subcommands (or hiding is driven by `command_executes_targets()`); the argv intercept is deleted.
- Test iterates every subcommand and asserts both `<cmd> --help` and `help <cmd>` show execution-only flags iff the command executes targets.

## Suggested approach

Prefer the flattened struct (aligns with str-qwua7.20.1).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: cli-ux-05, prior-07 (L6) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.15, str-qwua7.20.1, str-9ee5
