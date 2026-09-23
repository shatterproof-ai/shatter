# Markdown and HTML reports embed raw NUL and ESC bytes from generated inputs

- Priority: P3
- Type: bug
- Labels: report,security,ux
- Tracker action: new issue
- Source findings: audit 2026-09-22 goals-17 (confirmed; ESC means cat-ing a report can emit terminal escape sequences)

<!-- body -->
## Problem
A zolem scan's markdown report contains 6 NUL bytes and 3 ESC (0x1b) bytes. `grep` treats the file as binary ("binary file matches"), and an input of `"\u0000"` renders as raw control characters. Printing the report to a terminal can inject escape sequences.

## Current code facts
- `shatter-cli/src/render.rs` `value_short` (around line 277) does no control-character escaping. The shared report builders in `shatter-core/src/report.rs` and `reporter.rs` likewise.

## Acceptance criteria
- All markdown, text and HTML renderers escape C0 controls other than `\n` and `\t`, and DEL, as `\u00XX`.
- A property test (proptest) asserts that rendered reports for arbitrary string inputs are valid UTF-8 with no C0 controls other than newline and tab.
