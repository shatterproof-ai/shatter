# Error outcomes render inconsistently across languages; Rust shows truncated serde JSON

- Priority: P2
- Type: bug
- Labels: report,ux,parity,rust-frontend,go-frontend
- Tracker action: new issue
- Related: str-qwua7.56 (lifecycle export exclusion, closed)
- Source findings: audit 2026-09-22 cli-ux-11 (confirmed)

<!-- body -->
## Problem
The same error outcome from the three `04-errors` examples renders differently in explore reports:
- TS: `throws Error: division by zero`. Correct.
- Go: `throws function_error: division by zero`. Go functions return errors rather than throw, and `function_error` is an internal category name.
- Rust: `returns {"Err":"division by zero"}`, and successful structs render as `returns {"Ok":{"avg":2.0,"flag":null,"max":2....`, cut off mid-token. The Rust report also has a stray list item `- *Mocks: to_string*`, and `main` is explored as a target.

The walkthrough-review skill expects "Go's error string, Rust's enum variant".

## Current code facts
- Outcome text is produced by the report formatter in `shatter-core/src/explorer.rs` (`format_exploration_report`) and the explore renderer in `shatter-cli/src/render.rs` (`value_short`, around line 277), which truncate by character count.
- Examples: `ts/04-errors.ts`, `go/04-errors.go`, `rust/04_errors.rs` in the examples checkout (`SHATTER_EXAMPLES_DIR`).

## Acceptance criteria
- A per-language outcome formatter in core: Go and Rust error returns render as `errors: <msg>` (or `returns Err(<msg>)` for Rust); Rust Ok values render as `returns Ok(<value>)`; elision happens at token boundaries.
- A golden test renders the three `04-errors` examples and asserts equivalent outcome wording.
- Rust `main` is excluded from default targets, like TS lifecycle exports.
- The stray `Mocks:` bullet no longer appears when no mocks are in effect. If it is legitimate, give it a heading.

## Scope
In: rendering only. Out: changing protocol outcome categories.
