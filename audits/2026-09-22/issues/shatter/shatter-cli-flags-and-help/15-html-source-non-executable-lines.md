---
slug: html-source-non-executable-lines
kind: new
title: "HTML report source view marks non-executable lines (signature, braces, blanks) as uncovered"
priority: P3
type: bug
labels: [report, html, coverage, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# HTML report source view marks non-executable lines (signature, braces, blanks) as uncovered

## Problem

The HTML source view gives every line in a function's span either the `covered` or the `uncovered` class. Signature lines, closing braces, blank lines and comments therefore show as uncovered next to a header saying "7/7 lines".

The renderer cannot fix this on its own. `render_source_block` (`shatter-core/src/html_templates.rs:45-51`) receives only a file path, a start/end line span and a `HashSet<u32>` of covered lines. Coverage results carry an executable-line **count** (`total_lines`, for example `shatter-core/src/pipeline.rs:947`), not the **set** of executable lines. Guessing from text (braces, blank lines) is wrong: a signature line can hold default-argument code and a brace line can hold a statement.

## Evidence

- `shatter-core/src/html_templates.rs:45-51` signature; callers at `:258` and `:420`; test `render_source_block_marks_covered_and_uncovered_lines` at `:623`.
- `total_lines` is a count in `pipeline.rs:947` and the report structs.
- Finding artifacts-16 (P3; the HTML claim was not independently re-verified by the audit verifier).

## Acceptance criteria

- [ ] Diagnosis recorded first, in a comment before implementation: for TS, Go and Rust, where the executable-line set that produces `total_lines` is computed (frontend instrumentation or core), and whether it is available at report time. If it is not persisted, the fix persists it (and, if that crosses the protocol, follows `protocol/GOVERNANCE.md` and the parity checklist).
- [ ] `render_source_block` takes the executable-line set and emits three classes: covered, uncovered (executable, not hit), and neutral (not executable). No text-pattern heuristics decide executability.
- [ ] `render_source_block_marks_covered_and_uncovered_lines` is extended with a neutral line; a new test renders a real explored TS function and asserts that when the header says N/N lines, no line has the `uncovered` class. That test fails on current HEAD.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: none known.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 7.
