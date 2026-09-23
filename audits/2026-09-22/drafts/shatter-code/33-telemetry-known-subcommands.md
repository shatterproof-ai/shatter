# Telemetry KNOWN_SUBCOMMANDS is stale (3 nonexistent, ~15 real commands missing and redacted)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | telemetry,cli,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | cli-ux-18 |

<!-- body -->
## Problem

A hand-maintained subcommand list decides which command names are reported in command_run events; it has drifted.

## Current code facts / evidence

- `shatter-core/src/telemetry.rs:53-65`: explore, scan, export, spec, run, analyze, init, stale, telemetry, help, version. export/spec/version do not exist; spec-diff, diff, doctor, list-targets, observe, … are missing. Used at :209.
- `shatter telemetry status` → enabled (default).

## Acceptance criteria

- The list is derived from `Cli::command().get_subcommands()` in the CLI and passed to core, or a test binds the two lists.

## Suggested approach

Pass the clap-derived list in.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: cli-ux-18 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
