---
slug: rust-frontend-design-dedupe
kind: new
title: "shatter-rust: crate type registry rebuilt on every analyze (O(N^2) parses per crate scan), and two independent Axum extractor classifiers"
priority: P3
type: refactor
labels: [rust-frontend, performance, axum, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust: crate type registry rebuilt on every analyze (O(N^2) parses per crate scan), and two independent Axum extractor classifiers

## Problem

Two design inefficiencies in shatter-rust:

1. **Registry rebuilt per analyze.** Every analyze call rebuilds the crate type registry. It walks the crate's `src/` and `syn`-parses every `.rs` file, with no cache on the `Handler`. Scanning N files of one crate costs about N×N file parses (inferred, not measured).
2. **Two Axum extractor classifiers.** `adapters.rs` classifies Axum extractors (12 kinds, keyed by `ParamInfo.type_name`) for the adapter path. `executor.rs` has a separate 5-kind classifier (Path/Query/Json/State/Multipart) that re-parses type strings with `syn` for the generic wrappers. They can disagree, and support added to one is missing from the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/analyzer.rs:83` and `:201` both call `build_crate_type_registry(file_path)`, defined at `analyzer.rs:907`.
- `shatter-rust/src/adapters.rs:389` `pub enum AxumExtractorKind`. `shatter-rust/src/executor.rs:1796` `enum AxumExtractor`, used around `executor.rs:2167, 2195, 2222, 2452, 2776, 4989, 6571, 6693` (per the audit; re-check exact sites when implementing).

## Acceptance criteria

- [ ] The crate type registry is cached on the `Handler`, keyed by crate root plus a cheap fingerprint (e.g. max mtime + file count of `src/**/*.rs`), and invalidated on teardown/shutdown or fingerprint change. A test shows a second analyze in the same crate does not re-parse, and that editing a file invalidates the cache.
- [ ] Record `shatter scan` wall time on a multi-file Rust crate (e.g. the pickpackit or an examples crate) before and after in the close note.
- [ ] One extractor classifier in `adapters.rs` returns kind + inner type, and `executor.rs` consumes it (the `executor.rs` enum is deleted). A test asserts classification for every type in `AXUM_EXTRACTOR_TYPES`.
- [ ] Existing Axum tests and `cargo test --test e2e_concolic_rust` pass. The protocol output is unchanged, so no parity-contract update is needed; if output does change, update `protocol/parity-matrix.yaml` and run `task parity` + `task conformance`.

## Suggested approach

Keep the registry cache in a `HashMap<PathBuf, (Fingerprint, Arc<CrateTypeRegistry>)>` on the handler and pass the `Arc` into the analyze functions. For the classifier, extend `AxumExtractorKind` with a method that yields the inner type from a `syn::Type`, and delete the executor copy.

## Out of scope

- Adding support for new extractor kinds (str-la75, str-62pj, str-38in cover individual extractors).
- Cross-crate / multi-file analysis beyond caching (see project memory on single-file analysis).

## Size

M

## References

- Findings frontend-rust-12, frontend-rust-14 (audit 2026-09-22). Old draft: `drafts/shatter-code/64-rust-frontend-design-dedupe.md`.
