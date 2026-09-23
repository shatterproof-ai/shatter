---
slug: stable-hash-persisted-keys
kind: new
title: "std DefaultHasher keys persisted/reproducible values (core_sample --seed selection, Rust harness cache dirs); use a specified hash"
priority: P3
type: bug
labels: [audit-2026-09-22, rust, cache, reproducibility]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# std DefaultHasher keys persisted/reproducible values (core_sample --seed selection, Rust harness cache dirs); use a specified hash

## Problem

The std docs say the `DefaultHasher` algorithm is unspecified and may change between Rust releases. Shatter uses it for:

- the `scan --seed` sampling, which the help text promises is reproducible;
- on-disk Rust harness cache directories, whose compiled binaries are reused.

The repo has no `rust-toolchain.toml`. A toolchain bump can therefore silently change which functions `--seed`/`--batch next` selects, and invalidate or mis-key harness caches.

Impact is bounded: `DefaultHasher::new()` uses fixed SipHash keys, so results are stable within one toolchain. The practical effect is spurious rebuilds and non-reproducible sampling across toolchains. Wrong results would require a collision.

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/src/core_sample.rs:16` imports `DefaultHasher`. `default_seed` (`:388-389`) and `stable_hash` (`:549-550`) use it.
- `shatter-cli/src/args.rs:970-979`: the `--seed` help says "the same seed over unchanged source yields the same exploration".
- `shatter-rust/src/executor.rs` uses `DefaultHasher` in:
  - `native_replay_hash` (`:306-308`)
  - `mocks_hash` (`:736-739`)
  - `source_hash` (`:764-766`, which folds in `HARNESS_CACHE_VERSION = 2` from `:758`)
  - `stable_crate_harness_dir` (`:839-847`)
  - `stable_crate_bridge_dir` (`:3234-3236`)
  - `hash_native_replay_sources` (`:3734-3749`)
  - `runtime_source_hash` (`:4139-4175`)
- `executor.rs:6208-6218` `compute_prepare_id` already uses `sha2::Sha256`.
- `ls rust-toolchain*` finds no file.

## Acceptance criteria

- [ ] Every hash that is persisted to disk, used in a cache path, or promised reproducible uses a specified algorithm over an explicit byte encoding. Options: SHA-256 truncated to u64 (sha2 is already a dependency), FNV-1a, or fixed-key `siphasher`. In-memory-only uses (for example `orchestrator::hash_branch_path`) may keep `DefaultHasher`.
- [ ] `HARNESS_CACHE_VERSION` bumped once.
- [ ] Golden-value unit tests pin the hash output for fixed inputs (`default_seed("x")`, `stable_hash(...)`, `source_hash("...")`), so an algorithm change fails a test. Proof at close: test output.
- [ ] The rust-conventions skill (`.claude/skills/rust-conventions/`) gains a one-line rule: no `DefaultHasher` for persisted or promised-stable keys.

## Suggested approach

Mechanical replacement with a small `stable_hash_u64(&[u8]) -> u64` helper in each crate, or a shared one if a common crate fits the dependency direction (cli -> core; frontends -> protocol).

## Out of scope

- Adding a rust-toolchain pin (separate decision).
- Hashes that are never persisted.

## Metadata

- Priority: P3
- Type: bug
- Labels: audit-2026-09-22, rust, cache, reproducibility
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-0m0vn (--seed reproducibility), seed-for-explore-and-run
- Source findings: core-20, frontend-rust-13 (draft shatter-code/25)
