---
slug: per-language-outcome-rendering
kind: new
title: "Error outcomes render inconsistently across languages; outcome values are byte-truncated mid-token and can panic on non-ASCII"
priority: P2
type: bug
labels: [report, ux, parity, rust-frontend, go-frontend, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Error outcomes render inconsistently across languages; outcome values are byte-truncated mid-token and can panic on non-ASCII

## Problem

The same logical outcome, "division by zero", from the three `04-errors` examples renders three different ways in the default (markdown) explore report:

- **TS:** ``throws `Error: division by zero` ``. This is correct.
- **Go:** ``throws `function_error: division by zero` ``. Go functions return errors rather than throw, and `function_error` is an internal outcome category name.
- **Rust:** ``returns `{"Err":"division by zero"}` ``. Successful struct results render as ``returns `{"Ok":{"avg":2.0,"flag":null,"max":2....` ``, a raw serde JSON envelope cut off mid-token.
- Rust sections end with `- *Mocks: to_string*` as a stray list item directly after the table.

The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`, "3. What is the outcome of each behavior?" and criterion "D. Error paths describe the error type or value") asks for the error value in languages where errors are values.

Scope note: this draft was split during the cross-check. Excluding Rust `main` from default targets is rust-main-default-exclusion, and checking whether `to_string` really is mocked is rust-mocks-to-string-diagnosis. This issue covers rendering only.

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- The default markdown explore output is rendered by `shatter-cli/src/render.rs`: `shatter-cli/src/commands/explore.rs:3620-3638` dispatches `OutputFormat::Md` to `render::explore_fn_view` / `render::render_explore_fn`. `shatter-core/src/explorer.rs:2950` `format_exploration_report` is the legacy plain/ANSI path (`--render plain`, explore.rs:3639-3650), and `format_exploration_report_verbose` (explorer.rs:3183) is the trace-level path.
- `shatter-cli/src/render.rs:124-130`: the outcome is `throws `{thrown_error}`` if `thrown_error` is set, else `returns `{value_short(return_value)}``. There is no per-language formatting; Go's `function_error:` prefix arrives inside `thrown_error`.
- `shatter-cli/src/render.rs:277-284` `value_short()` does `let s = v.to_string(); if s.len() > 40 { format!("{}...", &s[..37]) }`, truncating by **byte** count. That causes the mid-token cut. From reading the code (not run), `&s[..37]` also panics when byte 37 is not a UTF-8 character boundary, for example a string outcome containing non-ASCII text. `shatter-core/src/explorer.rs:3262` has the same `&s[..37]` pattern.
- `shatter-cli/src/render.rs:139-142` pushes `Mocks: ...` into `extras`, which the template renders as a trailing list item.
- **Why a JSON-shape heuristic is unsafe for Rust `Result`:** the Rust harness serializes ordinary return values with serde, so `Result::Err("x")` and a `HashMap` containing only the key `"Err"` produce the same JSON. The analyzer maps `Result<T, E>` to a generic `TypeInfo::Union { variants, enum_values: [] }` (`shatter-rust/src/analyzer.rs:1325-1331`), which does not distinguish `Result` from other unions. The executor does know the declared return type is a `Result` (`is_result_return_shape`, `shatter-rust/src/executor.rs:2266-2275`), but nothing carries that to the CLI.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`: `ts-safedivide--concolic.out` (TS form), `go-explore.out` (``throws `function_error: division by zero` ``), `rust-explore3.out` lines 1-19 (the `{"Err":...}` and truncated `{"Ok":...` rows and the `- *Mocks: to_string*` line).
- The examples are `ts/04-errors.ts`, `go/04-errors.go` and `rust/04_errors.rs` in the examples checkout (`SHATTER_EXAMPLES_DIR`).
- Source findings: audit 2026-09-22 cli-ux-11 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F11.

## Acceptance criteria

- [ ] One outcome display function, used by `render.rs` (markdown) and by both core formatters (`format_exploration_report`, `format_exploration_report_verbose`):
  - TS exceptions: ``throws `Error: <msg>` `` (unchanged).
  - Go error returns: ``returns error `<msg>` ``. The internal `function_error` prefix never appears in rendered output.
  - Rust: ``returns `Err(<msg>)` `` and ``returns `Ok(<v>)` `` **only when trustworthy metadata says the declared return type is a `Result`**. That metadata comes from the Rust frontend (for example a result-variant field on the execute response, set from the executor's existing `is_result_return_shape` knowledge) — never from the JSON shape of the value. Without that metadata the value renders as plain JSON (elided correctly).
- [ ] If the Rust metadata is a new execute-response field, it is protocol-visible: update `protocol/` schema and regenerated bindings, `protocol/parity-matrix.yaml`, `shatter-rust/CLAUDE.md`, and run `task parity` and `task conformance`. If no protocol change is made, record why in the close reason.
- [ ] Negative tests: a Rust function returning a `HashMap<String, String>` with the single key `"Err"`, and a struct with a single field named `Ok`, render as JSON values, not as `Err(...)` / `Ok(...)`. A Go function whose return value is a string starting with `function_error:` (not an error) is not rewritten.
- [ ] One `elide(s, max_chars)` helper replaces both `&s[..37]` sites. A proptest over arbitrary strings, including multibyte ones, shows it never panics, always returns valid UTF-8, never exceeds the limit plus the ellipsis marker, and ends with the marker whenever it shortened the input. A unit test with a non-ASCII string whose byte 37 is mid-character panics on current `value_short` (red) and passes after the fix (green); record both in the close reason.
- [ ] Cross-language golden test renders the three `04-errors` examples through the markdown path and asserts the division-by-zero wording for each language and one success case for Rust (`Ok(...)` not cut mid-token).
- [ ] The `Mocks:` line, when present, renders as a labelled line under the section heading (before the table), not as a list item after it. Golden test covers it.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put the display function in core, keyed on `(language, outcome kind, value, frontend-supplied result-variant)`. For `elide`, cut on `char_indices` and prefer the last JSON token boundary before the limit.

## Out of scope

- Excluding Rust `main` from default targets (rust-main-default-exclusion).
- Whether `to_string` is really mocked for `safe_divide` (rust-mocks-to-string-diagnosis).
- Changing protocol outcome categories.
- The per-path constraint column (see markdown-drops-render-plain-info).

## Related

In this bucket: rust-main-default-exclusion, rust-mocks-to-string-diagnosis, markdown-drops-render-plain-info.

## Priority / Type

P2, bug.
