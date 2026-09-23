---
slug: rust-axum-extractor-classifier-dedupe
kind: new
title: "shatter-rust has two independent Axum extractor classifiers (adapters.rs 12 kinds, executor.rs 5 kinds) that can disagree"
priority: P3
type: refactor
labels: [rust-frontend, axum, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust has two independent Axum extractor classifiers (adapters.rs 12 kinds, executor.rs 5 kinds) that can disagree

## Problem

`adapters.rs` classifies Axum extractors (12 kinds, keyed by `ParamInfo.type_name`) for the adapter path. `executor.rs` has a separate 5-kind classifier (Path/Query/Json/State/Multipart) that re-parses type strings with `syn` for the generic wrappers. They can disagree, and support added to one is missing from the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (no change in `shatter-rust/` at main 16794cef):

- `shatter-rust/src/adapters.rs:389` `pub enum AxumExtractorKind`.
- `shatter-rust/src/executor.rs:1796` `enum AxumExtractor`, used around `executor.rs:2167, 2195, 2222, 2452, 2776, 4989, 6571, 6693` (per the audit; re-check exact sites when implementing).

## Acceptance criteria

- [ ] Before refactoring, a table test runs every type string in `AXUM_EXTRACTOR_TYPES` (plus wrapped forms such as `Path<(u32, String)>`, `Json<Foo>`, `State<Arc<AppState>>`) through both classifiers and records where they disagree. Paste the disagreement list in the close note (it may be empty).
- [ ] One classifier in `adapters.rs` returns kind + inner type, and `executor.rs` consumes it; the `executor.rs` enum is deleted.
- [ ] The table test then asserts the single classifier's result for every entry.
- [ ] Existing Axum tests pass (`task rust-fe:test`), and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored`; paste the `test result:` line with 0 ignored. If execute output changes for any extractor, update `protocol/parity-matrix.yaml` and run `task parity` + `task conformance`.

## Out of scope

- Adding support for new extractor kinds (str-la75, str-62pj, str-38in cover individual extractors).
- Registry caching (`rust-frontend-design-dedupe`).

## Size

S

## References

- Finding frontend-rust-14 (audit 2026-09-22). Split from `rust-frontend-design-dedupe` after the Codex cross-check.
