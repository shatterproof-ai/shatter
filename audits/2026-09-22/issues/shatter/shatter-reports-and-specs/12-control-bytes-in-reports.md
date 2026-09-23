---
slug: control-bytes-in-reports
kind: new
title: "Markdown and HTML reports embed raw NUL and ESC bytes from generated inputs"
priority: P3
type: bug
labels: [report, security, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Markdown and HTML reports embed raw NUL and ESC bytes from generated inputs

## Problem

A zolem scan's markdown report (`zolem-fixture-default.md`) contains 6 NUL bytes and 3 ESC (0x1b) bytes. `grep` treats the file as binary ("binary file matches"). The input values themselves are escaped correctly (`"\u0000"`), but the **error messages** thrown by the function under test echo the input back, and the report writes those messages raw. An input of `"\u001b[31m"` puts a real ANSI color escape into the report, so printing the report with `cat` can inject terminal escape sequences. Multi-line error messages also break the markdown list structure (continuation lines such as ` | ^` start at column 1). The verifier noted that the terminal-injection angle could justify P2; it is filed at P3 as the audit recommended.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `audits/2026-09-22/goals-runs/zolem-fixture-default.md` (on branch `audit-2026-09-22` until the audit reports land): a byte count gives 6 NUL and 3 ESC. Every one is inside a thrown-error message: lines 836-847 (`"\u0000"` and `"a\u0000b"` inputs, CEL error `token recognition error at: '<NUL>'` plus the echoed source line ` | <NUL>`) and lines 849-853 (`"\u001b[31m"` input, echoed as ` | <ESC>[31m`).
- Writer: `shatter-core/src/report.rs:2479` in the Interesting Inputs section: `writeln!(out, "- {inputs_str} -> **error:** {err}")`. `inputs_str` goes through `format_json_compact_list` (escaped); `err` (`thrown_error`) is written unescaped. Other places that print `thrown_error` or error messages into markdown, text or HTML likely do the same; list them at pickup (`/usr/bin/grep -rn thrown_error shatter-core/src shatter-cli/src`).
- The explore renderer's `value_short` (`shatter-cli/src/render.rs:277-284`) uses `serde_json::Value::to_string()`, which escapes controls, so values are not the problem there. But it truncates with `&s[..37]` (`render.rs:280`), a byte slice that panics when byte 37 falls inside a multi-byte UTF-8 character. The other `format_value_short` helpers (`compare.rs:265`, `explorer.rs:3259`, `export.rs:789`) should be checked for the same pattern.

Repro: scan zolem's `internal/fixture` package (or any function whose error message echoes a string input), then run `python3 -c 'import sys;b=open(sys.argv[1],"rb").read();print(b.count(b"\x00"), b.count(b"\x1b"))' report.md`.

## Acceptance criteria

- [ ] Error messages and any other free text from the target program (thrown errors, stdout/stderr captures, return strings shown unquoted) are escaped before they are written to markdown, text or HTML reports. The close comment lists every writer changed.
- [ ] All markdown, text and HTML renderers escape C0 controls other than `\n` and `\t`, and DEL, as `\u00XX` (HTML additionally entity-escapes as it does today). Multi-line error messages are indented or fenced so they stay inside their list item.
- [ ] A proptest asserts that rendered reports (markdown, text, HTML) for arbitrary string inputs are valid UTF-8 and contain no C0 control bytes other than newline and tab. It fails on current code; record both runs.
- [ ] Truncation helpers truncate on a char boundary; a test with a multi-byte string longer than the limit does not panic.

## Suggested approach

Add one shared `escape_display(&str) -> Cow<str>` in core and use it in every place that prints target-program text (error messages first, starting at `report.rs:2479`) or values into a report. Make the truncation helpers use `char_indices` or `floor_char_boundary`.

## Out of scope

JSON output (serde_json already escapes control characters).

## Related

Source finding: goals-17 (confirmed).
