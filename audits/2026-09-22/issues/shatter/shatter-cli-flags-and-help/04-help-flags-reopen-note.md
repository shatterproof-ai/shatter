---
slug: help-flags-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.15: execution-only flags still shown by `help <cmd>` and 10 non-executing commands"
priority: P2
type: comment
labels: [cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.15
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on str-qwua7.15 (closed; do not reopen)

Target: `str-qwua7.15`. Action: add a comment only (`bd comments add str-qwua7.15 ...`). Leave the issue closed.

## Comment text

> Audit 2026-09-22 follow-up (findings cli-ux-05, prior-07, docs-22): this issue was closed on 2026-09-14 with the reason "Closed", but the fix is partial.
>
> - `shatter spec-diff --help` no longer shows `--allow-host-writes`, but `shatter help spec-diff` still does (line 50), along with `--set` (line 55): 78 lines against 48. `shatter help doctor` also shows it. The fix intercepts raw argv only for `-h/--help` (`shatter-cli/src/main.rs:70` `maybe_print_non_executing_help`), and clap's `help <cmd>` path bypasses it.
> - `NON_EXECUTING_COMMAND_PATHS` (`shatter-cli/src/args.rs:324`) lists five commands. analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace and build-frontend still show `--allow-host-writes` in `--help`, and the flags still parse on non-executing commands.
> - Verified 2026-09-23 with a `target/debug/shatter` built at HEAD `56c86168`.
>
> The structural fix is tracked in the new issue **help-hides-execution-flags** (<new id>): scope the execution-only options to executing commands (flattened `ExecOptions`, driven by `command_executes_targets()`), delete the argv intercept, and add a test over both `<cmd> --help` and `help <cmd>` for every subcommand. Tracker-ID removal from help text, which this issue left out of scope, is filed as **help-tracker-ids-lint** (<new id>).

The filer substitutes `<new id>` with the ids assigned to help-hides-execution-flags and help-tracker-ids-lint.
