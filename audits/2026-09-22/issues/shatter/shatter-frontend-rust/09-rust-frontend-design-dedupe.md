---
slug: rust-frontend-design-dedupe
kind: new
title: "shatter-rust rebuilds the crate type registry on every analyze (about N² file parses per crate scan)"
priority: P3
type: refactor
labels: [rust-frontend, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust rebuilds the crate type registry on every analyze (about N² file parses per crate scan)

## Problem

Every analyze call rebuilds the crate type registry. It walks the crate's `src/` and `syn`-parses every `.rs` file, with no cache on the `Handler`. Scanning N files of one crate costs about N×N file parses (inferred, not measured).

## Evidence

Re-verified against the audit worktree at commit 56c86168 (no change in `shatter-rust/` at main 16794cef):

- `shatter-rust/src/analyzer.rs:83` and `:201` both call `build_crate_type_registry(file_path)`, defined at `analyzer.rs:907`.

## Acceptance criteria

- [ ] Before changing code, measure: `shatter scan` wall time and the number of `build_crate_type_registry` file parses (temporary counter or `--timing` phase) on a multi-file Rust crate (name the crate and commit, e.g. an examples crate). Record in the close note.
- [ ] The crate type registry is cached on the `Handler`, keyed by crate root. Invalidation uses per-file change detection: the cache stores, for every `.rs` file it parsed, the path plus (size, mtime) or a content hash, and is rebuilt when any stored file changed, was removed or replaced, or when a new `.rs` file appears under the crate's source roots. An aggregate fingerprint such as max-mtime + file count is not acceptable: it misses an edit to an older file while a newer file keeps the max, and a replace that keeps the count.
- [ ] Tests: (1) a second analyze in the same crate does not re-parse (parse counter unchanged); (2) editing a file that is not the newest invalidates; (3) replacing a file with same-size different content invalidates (use a content hash or ensure mtime moves); (4) adding a file invalidates; (5) removing a file invalidates.
- [ ] The same scan re-measured after the change, with parse count and wall time in the close note.
- [ ] `task rust-fe:test` passes and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste the `test result:` line with 0 ignored. Protocol output is unchanged, so no parity-contract update is expected.

## Suggested approach

Keep a `HashMap<PathBuf, (Vec<(PathBuf, u64, SystemTime, u64 /*hash*/)>, Arc<CrateTypeRegistry>)>` on the handler and pass the `Arc` into the analyze functions. Re-stat the file list on each lookup (cheap compared with re-parsing).

## Out of scope

- Unifying the two Axum extractor classifiers (`rust-axum-extractor-classifier-dedupe`).
- Cross-crate / multi-file analysis beyond caching.

## Size

S

## References

- Finding frontend-rust-12 (audit 2026-09-22). Old draft: `drafts/shatter-code/64-rust-frontend-design-dedupe.md` (split: the extractor half is `rust-axum-extractor-classifier-dedupe`).
