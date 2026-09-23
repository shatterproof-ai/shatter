# std DefaultHasher used for persisted/reproducible keys (core_sample --seed selection, Rust harness cache dirs)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | rust,cache,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | core-20, frontend-rust-13 |

<!-- body -->
## Problem

std documents DefaultHasher's algorithm as unspecified across releases, yet it keys on-disk harness caches and the `--seed` sampling that help promises is reproducible. There is no rust-toolchain.toml.

## Current code facts / evidence

- `shatter-core/src/core_sample.rs:389` `default_seed`, `:550` `stable_hash` use DefaultHasher; `shatter-cli/src/args.rs:971-977` --seed help promises reproducibility.
- `shatter-rust/src/executor.rs:764-767`, `:839-847`, `:3234-3236`, `:3749` (source_hash, stable_crate_harness_dir, stable_crate_bridge_dir) use DefaultHasher; `executor.rs:4122` already uses sha2 for prepare_id.

## Acceptance criteria

- Persisted or promised-stable hashes use a specified algorithm (SHA-256 truncated, FNV, or fixed-key siphasher) over an explicit byte encoding.
- HARNESS_CACHE_VERSION bumped once.
- rust-conventions skill gains a one-line rule: no DefaultHasher for persisted keys.

## Suggested approach

Mechanical replacement.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: core-20, frontend-rust-13 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
