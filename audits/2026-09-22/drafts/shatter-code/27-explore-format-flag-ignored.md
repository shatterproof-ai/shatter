# `explore --format text|html` has no effect on stdout (printer branches on --render); --render and --format overlap

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | cli,explore,report,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-zt4v, str-mpwp, str-9ee5 |
| source findings | cli-ux-02 |

<!-- body -->
## Problem

str-zt4v decided `--format` controls stdout. For explore, the streaming printer checks `output_format` (`--render md|plain`) instead, so `--format html` and `--format text` both print markdown. `strip_markdown_text` also strips every `*`, backtick and `|` including inside data values (e.g. Go `*T`).

## Current code facts / evidence

- `shatter-cli/src/commands/explore.rs:4780-4790`, `:3853-3865`, `:4029-4038` branch on output_format.
- `shatter-core/src/report.rs:1917-1945` `strip_markdown_text` character filter.
- Only explore --format test: `shatter-cli/tests/json_stdout_contract.rs:250` (clap rejects json).
- `--render plain` shows 'Branches: 3/3', '[random: 3 (100%)]', 'Symbolic: 3/3 constraints' that markdown omits (cli-ux-14, L6).

## Acceptance criteria

- `explore --format text` prints plain text and `--format html` prints HTML to stdout; goldens for both.
- `--render` is deprecated (warning) or removed after porting its extra lines into markdown.
- Text rendering does not corrupt `*`/`|` inside values (render from the view model, not by stripping).

## Suggested approach

Single stdout format selector driving the streaming printer.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: cli-ux-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-zt4v, str-mpwp, str-9ee5
