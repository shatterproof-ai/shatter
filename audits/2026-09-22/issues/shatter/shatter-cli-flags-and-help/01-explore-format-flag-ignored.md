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

str-zt4v (closed) decided that `--format` controls what goes to stdout. For `explore`, the streaming printer branches on `output_format` instead. That field is the `--render md|plain` enum (`shatter-cli/src/args.rs:37-43`), so `explore --format html` and `explore --format text` both print markdown. `--format` is honored only by the post-run replay that runs when `-o` and `--stdout` are both given. Explore has four format controls that interact silently: `--render {md,plain}`, `--format {markdown,html,text}`, `--color`, and inference from the `-o` file extension.

A second problem: `strip_markdown_text`, the text renderer, is a character filter. It deletes every `*` and backtick and splits every line containing `|`, including inside data values. Text mode would therefore corrupt Go pointer types (`*T`) and values such as `a | b`.

## Evidence

Re-verified against `audit-2026-09-22` (HEAD `56c86168`).

- Streaming printer branches on `--render`: `shatter-cli/src/commands/explore.rs:3620`, `:3854`, `:3920`, `:4783` and `:6533` (`if output_format == crate::args::OutputFormat::Md`).
- `--format` is used only by the replay: `explore.rs:4028-4029` and `:6687-6688` (`if !report_outputs.is_empty() && stdout`).
- `shatter-core/src/report.rs:1918-1946` `strip_markdown_text`: `.replace('*', "")`, `.replace('`', "")`, and `line.split('|')` on any line that contains `|`.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `format-html.out` and `format-text.out` both begin `# Shatter Explore` and contain `**4 path(s)**` and markdown tables. `format-text-with-o.out` (`--format text -o r.md --stdout`) is also markdown.
- The only test of `explore --format` is a clap rejection of `json` (`shatter-cli/tests/json_stdout_contract.rs:250`).
- `explore --help` lists both `--render <MODE>` and `--format <FORMAT>`.
- `--render plain` prints lines that markdown omits (`Branches: 3/3`, `[random: 3 (100%)]`, `Symbolic: 3/3 constraints`). That gap is tracked separately as markdown-drops-render-plain-info (bucket shatter-cli-runtime-output).
- Verifier (findings.json cli-ux-02) reproduced `--format html|text` printing markdown and lowered the priority from P1 to P2: the flag is accepted and ignored, but no data is lost. The verifier did not check the `strip_markdown_text` sub-claim. It was confirmed separately under artifacts-16 by reading the code.

## Acceptance criteria

- [ ] `shatter explore <file> --format text` prints plain text with no markdown syntax (`#`, `**`, table pipes) to stdout. `--format html` prints an HTML document to stdout. `--format markdown` is unchanged.
- [ ] The same holds with `-o FILE --stdout`: the stdout format follows `--format`, not the file extension.
- [ ] Golden or snapshot tests in `shatter-cli/tests/` cover `explore --format {markdown,text,html}`. Each test is shown failing on the current code and passing after the fix (record both runs in the close comment).
- [ ] Text rendering preserves literal `*`, backtick and `|` inside data values. A test uses an outcome or type containing `*T` and `a | b` and asserts both survive in `--format text`.
- [ ] `--render` is removed, or kept as a deprecated alias that prints a warning on stderr. Before removal, the extra `--render plain` lines are either ported into the default report or explicitly handed to markdown-drops-render-plain-info (link it in the close comment).
- [ ] `task affected` passes. The close comment records its `Gates selected` line.

## Suggested approach

Use one stdout-format selector (`--format`, with `-o` extension inference only for files) that drives both the streaming printer and the replay path. Render text from the report view model rather than stripping markdown after the fact. Replace or delete `strip_markdown_text`, and check its other callers (scan uses it too) before changing its behavior. This touches the same emitter code as explore-report-printed-twice, so doing both in one branch, or one straight after the other, avoids conflicts.

## Out of scope

- Duplicate printing with `-o --stdout` and the header leak with `-q -o` (explore-report-printed-twice).
- Adding information that `--render plain` shows to the default markdown (markdown-drops-render-plain-info), except as needed to retire `--render`.
- Format-flag vocabulary across commands and help grouping (str-9ee5).
- JSON bundle contents written by `-o out.json` (explore-o-json-empty-bundle).

## Dependencies

- Blocked by: none.
- Related: explore-report-printed-twice (same code), markdown-drops-render-plain-info, str-zt4v, str-mpwp, str-9ee5.

## Source

Audit 2026-09-22 finding cli-ux-02 (areas/cli-ux.md F2); old draft `drafts/shatter-code/27-explore-format-flag-ignored.md`. Also 2026-09-04 usability-ui item 18, which was never filed.
