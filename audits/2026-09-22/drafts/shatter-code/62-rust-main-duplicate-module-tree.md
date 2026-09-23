# shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P2 |
| labels | rust-frontend,tests,performance,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-rust-06 |

<!-- body -->
## Problem

The binary declares every module itself instead of using the library, doubling unit-test compile and run time in the slowest serialized gate leaf.

## Current code facts / evidence

- `shatter-rust/src/main.rs:1-24` `#![allow(dead_code)]` + `mod adapters; ... mod wasm_generator;`; `lib.rs:1-20` declares the same; ENV_LOCK duplicated (lib.rs:19, main.rs:22).
- `cargo test --no-run` produces `unittests src/lib.rs` (587 tests) and `unittests src/main.rs` (587 tests); a gate log shows 'Starting 1180 tests across 3 binaries ... 208.433s'.
- rust-fe:test is the serialized tail of check-unit (Taskfile.yml:556-562).

## Acceptance criteria

- main.rs is a thin binary using `shatter_rust::handler::Handler` with no mod declarations (or `[[bin]] test = false`).
- Crate-wide allow(dead_code) and duplicate ENV_LOCK removed.
- rust-fe:test test count halves; wall time recorded before/after.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-rust-06 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
