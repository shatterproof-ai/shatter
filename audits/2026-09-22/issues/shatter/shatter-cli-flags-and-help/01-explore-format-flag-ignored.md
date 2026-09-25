---
slug: explore-format-flag-ignored
kind: new
title: "explore --format text|html has no effect on stdout; --render and --format overlap"
priority: P2
type: bug
labels: [cli, explore, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore --format text|html has no effect on stdout; --render and --format overlap

## Problem

str-zt4v (closed) decided that `--format` controls what goes to stdout. For `explore`, the streaming printer branches on `output_format` instead. That field is the `--render md|plain` enum (`shatter-cli/src/args.rs:37-43`), so `explore --format html` and `explore --format text` both print markdown. On stdout, `--format` is honored only by the post-run replay that runs when `-o` and `--stdout` are both given. Explore has four format controls that interact silently: `--render {md,plain}`, `--format {markdown,html,text}`, `--color`, and inference from the `-o` file extension.

A second problem: `strip_markdown_text`, the text renderer, is a character filter. It deletes every `*` and backtick and splits every line containing `|`, including inside data values. Text mode therefore corrupts Go pointer types (`*T`) and values such as `a | b`. This already affects `-o FILE.txt` output: `StdoutFormat::Text` also drives the `-o` text-file writers (`explore.rs:3992-3994`, `:6631`), which call `strip_markdown_text`.

`--render` is a **global** flag (`args.rs:190-197`, `global = true`) that `main.rs` passes to both explore (`main.rs:448`) and scan (`main.rs:893`). This issue changes explore only. Retiring `--render` across commands belongs to the flag-vocabulary work in str-9ee5.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`).

- Streaming printer branches on `--render`: `shatter-cli/src/commands/explore.rs:3620`, `:3854`, `:3920`, `:4783` and `:6533` (`if output_format == crate::args::OutputFormat::Md`).
- `--format` on stdout is used only by the replays: `explore.rs:4028-4029` (in `finalize_explore`, reached via `--from-artifacts`) and `:6687-6688` (in `run_explore`, the live path), both `if !report_outputs.is_empty() && stdout`. It also selects the `-o` text writer (`:3992-3994`, `:6631`).
- `shatter-core/src/report.rs:1918-1946` `strip_markdown_text`: `.replace('*', "")`, `.replace('`', "")`, and `line.split('|')` on any line that contains `|`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `format-html.out` begins `# Shatter Explore` and contains `**4 path(s)**` and markdown tables; `format-text.out` (a `markup` fixture) begins `# Shatter Explore` and contains `**0 path(s)**`. `format-text-with-o.out` (`--format text -o r.md --stdout`) is also markdown with `**4 path(s)**`.
- The only test of `explore --format` is a clap rejection of `json` (`shatter-cli/tests/json_stdout_contract.rs:250`).
- `explore --help` and `scan --help` both list `--render <MODE>` and `--format <FORMAT>`.
- `--render plain` prints lines that markdown omits (`Branches: 3/3`, `[random: 3 (100%)]`, `Symbolic: 3/3 constraints`). That gap is tracked separately as markdown-drops-render-plain-info (bucket shatter-cli-runtime-output).
- Verifier (findings.json cli-ux-02) reproduced `--format html|text` printing markdown and lowered the priority from P1 to P2. The `strip_markdown_text` sub-claim was confirmed separately under artifacts-16 by reading the code.

## Acceptance criteria

- [ ] `shatter explore <file> --format text` prints plain text with no markdown syntax (`#` headings, `**`, table pipe rows) to stdout. `--format html` prints an HTML document (starts with `<!DOCTYPE html>` or `<html`) to stdout. `--format markdown` output is byte-identical to today's default.
- [ ] The same holds with `-o FILE --stdout`, on both the live path and the `--from-artifacts` path: the stdout format follows `--format`, not the file extension.
- [ ] Precedence is defined and tested for explore: when `--format` and `--render` are both given, `--format` wins; an explicit `--render` on `explore` prints a one-line deprecation warning on stderr naming `--format`. With neither given, explore output is unchanged.
- [ ] scan's behavior is unchanged: a test (or the existing scan output tests, named in the close comment) shows `scan --render plain` and `scan --render md` output identical before and after this change. Global retirement of `--render`, including scan's migration, is left to str-9ee5; the close comment adds a note there.
- [ ] Golden or snapshot tests in `shatter-cli/tests/` cover `explore --format {markdown,text,html}` without `-o`, and `--format text -o r.md --stdout`. The text and html tests fail on the current code and pass after the fix; the close comment records both runs (test names plus the failing assertion text).
- [ ] Text rendering preserves literal `*`, backtick and `|` inside data values. A test uses an outcome or type containing `*T` and `a | b` and asserts both appear verbatim in `--format text` stdout and in a `-o r.txt` file. This test fails on current code.
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Use one stdout-format selector for explore (`--format`, with `-o` extension inference only for files) that drives both the streaming printer and the replay paths. Render text from the report view model rather than stripping markdown after the fact. Replace or delete `strip_markdown_text`, and check its other callers (scan uses it too) before changing its behavior; if scan keeps calling it, fix the corruption there as well and cover it with the same `*T` / `a | b` test. This touches the same emitter code as explore-report-printed-twice, so do both in one branch, or one straight after the other.

## Out of scope

- Removing `--render` globally or changing scan's format flags (str-9ee5).
- Duplicate printing with `-o --stdout` and the header leak with `-q -o` (explore-report-printed-twice).
- Adding information that `--render plain` shows to the default markdown (markdown-drops-render-plain-info).
- JSON bundle contents written by `-o out.json` (explore-o-json-empty-bundle).

## Dependencies

- Blocked by: none.
- Related: explore-report-printed-twice (same code), markdown-drops-render-plain-info, str-zt4v, str-mpwp, str-9ee5.

## Source

Audit 2026-09-22 finding cli-ux-02 (areas/cli-ux.md F2); old draft `drafts/shatter-code/27-explore-format-flag-ignored.md`. Also 2026-09-04 usability-ui item 18, which was never filed.
