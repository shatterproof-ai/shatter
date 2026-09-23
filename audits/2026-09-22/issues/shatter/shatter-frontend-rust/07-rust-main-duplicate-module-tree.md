---
slug: rust-main-duplicate-module-tree
kind: new
title: "shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice"
priority: P2
type: chore
labels: [rust-frontend, tests, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice

## Problem

The `shatter-rust` binary declares every module itself (`mod adapters; mod analyzer; ...`) under a crate-wide `#![allow(dead_code)]`, instead of depending on the `shatter_rust` library. So cargo builds two unit-test binaries, `unittests src/lib.rs` and `unittests src/main.rs`, with the same 587 inline tests, and runs both. `rust-fe:test` is the serialized tail of `check-unit`, so this doubles the slowest leaf of the landing gate. The crate-wide `allow(dead_code)` also hides real dead code, and `ENV_LOCK` exists twice. Tests that serialize on it in one binary do not serialize against the other.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/main.rs:3` `#![allow(dead_code)]`, then 12 `mod` declarations from `main.rs:5`. `lib.rs` declares the same modules.
- `ENV_LOCK` is duplicated: `shatter-rust/src/lib.rs:20` and `shatter-rust/src/main.rs:23`.
- `Taskfile.yml:556-562`: `rust-fe:test` runs serialized at the end of `check-unit`, per the comment about nested cargo builds.
- Audit measurement (finding frontend-rust-06): `cargo test --no-run` produced `unittests src/lib.rs` (587 tests via `--list`), `unittests src/main.rs` (587) and `codegen_parity` (4). A prior gate log showed `Starting 1180 tests across 3 binaries ... Summary [208.433s]`. The roughly 50% saving is an estimate.

## Acceptance criteria

- [ ] `main.rs` is a thin binary that uses `shatter_rust::handler::Handler` (or equivalent public entry) and has no `mod` declarations. If that is not possible, `[[bin]] test = false` in `shatter-rust/Cargo.toml`, with the reason recorded.
- [ ] The crate-wide `#![allow(dead_code)]` is removed. Real dead code it was hiding is deleted, or allowed item-by-item with a reason.
- [ ] One `ENV_LOCK`.
- [ ] Proof at close: `cargo nextest list -p shatter-rust` (or `cargo test -p shatter-rust -- --list`) before and after, showing the unit test count halved, and `task rust-fe:test` wall time before and after, forced to execute (not a checksum-cached no-op; see the project gate-cache note). Paste both into the close note.
- [ ] `task rust-fe:test` passes, and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing because every case is `#[ignore]`d). Paste the `test result:` line, which must show 0 ignored.

## Suggested approach

Make any items the binary needs `pub` (or `pub(crate)` re-exported through a small `pub mod` surface) and change `main.rs` to `fn main() { shatter_rust::run() }`-style. Check that `codegen_parity.rs` and other integration tests do not rely on binary-only items.

## Out of scope

- Splitting the executor module or reorganizing tests.
- Other crates' test layout.

## Size

S

## References

- Finding frontend-rust-06 (audit 2026-09-22). Old draft: `drafts/shatter-code/62-rust-main-duplicate-module-tree.md`.
