---
slug: per-language-outcome-rendering
kind: new
title: "Error outcomes render inconsistently across languages; Rust shows truncated serde JSON"
priority: P2
type: bug
labels: [report, ux, parity, rust-frontend, go-frontend, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Error outcomes render inconsistently across languages; Rust shows truncated serde JSON

## Problem

The same logical outcome, "division by zero", from the three `04-errors` examples renders three different ways in the explore report:

- **TS:** ``throws `Error: division by zero` ``. This is correct.
- **Go:** ``throws `function_error: division by zero` ``. Go functions return errors rather than throw, and `function_error` is an internal outcome category name.
- **Rust:** ``returns `{"Err":"division by zero"}` ``. Successful struct results render as ``returns `{"Ok":{"avg":2.0,"flag":null,"max":2....` ``, which is a raw serde JSON envelope cut off mid-token.
- Rust sections also end with a stray list item, `- *Mocks: to_string*`, placed directly after the table with no heading. `main` is explored as a target.

The walkthrough-review rubric (`.claude/skills/walkthrough-review/SKILL.md`) requires "the error value in languages where errors are scalars (e.g., Go's `error` string, Rust's enum variant)".

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/render.rs:277-284` `value_short()` does `let s = v.to_string(); if s.len() > 40 { format!("{}...", &s[..37]) }`. It truncates the JSON by **byte** count. That causes the mid-token cut. From reading the code (not run), `&s[..37]` also panics when byte 37 is not a UTF-8 character boundary, for example when a string outcome contains non-ASCII text.
- The "Mocks:" line comes from `shatter-cli/src/render.rs:141` (`extras.push(format!("Mocks: {}", ...))`), which is rendered as a trailing list item.
- The markdown report formatter in core is `format_exploration_report` at `shatter-core/src/explorer.rs:2950`.
- TS lifecycle-export exclusion lives at `shatter-core/src/discovery.rs:605-617` (`LIFECYCLE_EXPORT_NAMES` and `is_lifecycle_export_name`, from str-qwua7.56). There is no Rust `main` equivalent.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `ts-safedivide--concolic.out`: the TS form.
  - `go-explore.out`: ``throws `function_error: division by zero` ``.
  - `rust-explore3.out` lines 9-19: `{"Err":...}`, `{"Ok":{"avg":2.0,"flag":null,"max":2....`, `- *Mocks: to_string*`, and a `main` section.
- The examples are `ts/04-errors.ts`, `go/04-errors.go` and `rust/04_errors.rs` in the examples checkout (`SHATTER_EXAMPLES_DIR`).
- Source findings: audit 2026-09-22 cli-ux-11 (confirmed, P2; the Mocks line and `main` were not re-verified by the verifier). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F11.

## Acceptance criteria

- [ ] One per-language outcome formatter in `shatter-core`, used by both the core markdown formatter and `shatter-cli/src/render.rs`:
  - TS exceptions: `throws Error: <msg>` (unchanged).
  - Go error returns: `errors: <msg>`. The internal `function_error` never appears in rendered output.
  - Rust `Err(e)`: `returns Err(<msg>)`. Rust `Ok(v)`: `returns Ok(<v pretty>)`.
- [ ] Elision happens at a token or character boundary and never splits a UTF-8 character. A unit or property test (proptest over arbitrary strings, including multibyte ones) shows the truncation helper never panics and always yields valid UTF-8 with a closing ellipsis marker. This test must fail (panic) on current `value_short` for a non-ASCII case.
- [ ] A cross-language golden test renders the three `04-errors` examples and asserts equivalent outcome wording for the division-by-zero case and for one `Ok` or success case.
- [ ] Rust `fn main` is excluded from default explore/scan targets, the way TS lifecycle exports are, and can still be explored when named explicitly.
- [ ] The `Mocks:` line renders only when mocks are in effect, as a labelled line under the section heading. It no longer renders as a stray bullet after the table. Confirm whether `to_string` really is being mocked for `safe_divide`. If it is not, fix the mock-recording source.
- [ ] If rendered output is protocol-visible per `protocol/parity-matrix.yaml`, update the matrix and the affected frontend CLAUDE.md and run `task parity`. Otherwise record in the close reason that no parity change is needed.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put an `OutcomeDisplay` function in core that takes `(language, outcome category, value)` and returns a short display string. Rust `Result` envelopes are recognised by their single `Ok` or `Err` key. Replace both truncation sites with one `elide(s, max_chars)` helper that cuts on `char_indices` and prefers the last JSON token boundary before the limit.

## Out of scope

- Changing protocol outcome categories or the wire format.
- The per-path constraint column (see markdown-drops-render-plain-info).

## Related

str-qwua7.56 (closed; TS lifecycle-export exclusion, the model for excluding Rust `main`).

## Priority / Type

P2, bug.
