---
slug: stable-hash-persisted-keys
kind: new
title: "std DefaultHasher keys persisted or promised-reproducible values (scan --seed sampling, Rust harness caches, external-audit cache dir, run scope_hash, ExecutionRecord.input_hash); use a specified hash"
priority: P3
type: bug
labels: [audit-2026-09-22, rust, cache, reproducibility]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# std DefaultHasher keys persisted or promised-reproducible values (scan --seed sampling, Rust harness caches, external-audit cache dir, run scope_hash, ExecutionRecord.input_hash); use a specified hash

## Problem

The std docs say the `DefaultHasher` algorithm is unspecified and may change between Rust releases. Shatter uses it for values that are written to disk, used in cache paths, or promised reproducible. The repo has no `rust-toolchain.toml`, so a toolchain bump can silently change which functions `scan --seed` / `--batch next` select, and re-key or orphan on-disk caches.

Impact is bounded: `DefaultHasher::new()` uses fixed SipHash keys, so results are stable within one toolchain. The practical effects are spurious rebuilds, orphaned cache directories, run manifests whose `scope_hash` changes with no scope change, and non-reproducible sampling across toolchains. Wrong results would require a collision.

## Migration surface (full inventory, audit HEAD 56c86168)

Found with `grep -rn "DefaultHasher" --include=*.rs` over every workspace crate's `src/` (non-test code). This table is the whole surface. The implementer confirms each classification and records any change in the close note.

**Must migrate (persisted, cache path, or promised reproducible):**

| Site | Use | Why it counts |
|---|---|---|
| `shatter-core/src/core_sample.rs:389` `default_seed`, `:550` `stable_hash` | core-sample selection | `--seed` help promises reproducibility (`shatter-cli/src/args.rs:970-979`) |
| `shatter-rust/src/executor.rs` `native_replay_hash` (`:306-308`), `mocks_hash` (`:736-739`), `source_hash` (`:764-766`, folds in `HARNESS_CACHE_VERSION = 2` from `:758`), `stable_crate_harness_dir` (`:839-847`), `stable_crate_bridge_dir` (`:3234-3236`), `hash_native_replay_sources` (`:3734-3749`), `runtime_source_hash` (`:4139-4175`) | harness cache keys and directories | compiled binaries reused from disk |
| `shatter-cli/src/helpers.rs:711-721` `external_audit_cache_root` | `$TMP/shatter-audit-cache/<hash>/harness` | cache path |
| `shatter-cli/src/commands/run.rs:1041` `scope_hash` | `scope_hash` field of the run manifest (`run.rs:237`, `:274`) | written to disk; its doc comment only promises reproducibility "within a single Shatter version", which a toolchain bump breaks |

**Must classify (persisted status to be confirmed):**

| Site | Use |
|---|---|
| `shatter-core/src/pipeline.rs:794`, `invariants.rs:754`, `scan_orchestrator.rs:1287` | `ExecutionRecord.input_hash`. `ExecutionRecord` is `Serialize` (`execution_record.rs:161-165`), and `behavior.rs:264-271` dedups behaviour maps by it. If behaviour maps or records reach the on-disk behaviour-map cache, this migrates. |
| `shatter-core/src/scan_orchestrator.rs:1343` `detect_mock_misses` | `caller_execution_id` on `MockMiss`. Migrates if written to reports. |

**May keep `DefaultHasher` (in-memory only):** `orchestrator.rs:865` `hash_branch_path`; `explorer.rs:359` `path_feedback_fingerprint`, `:576` `legacy_path_hash`, `:757`/`:799` (loop collapse), `:2212` `candidate_fingerprint`; `float_probe.rs:157`; `recursive.rs:541` (module deleted by core-dead-code-removal). `shrunk_witnesses`, which is keyed by path hash, is not serialized into explore artifacts (checked on the audit's artifacts).

`shatter-rust/src/executor.rs:6208-6218` `compute_prepare_id` already uses `sha2::Sha256`.

## Acceptance criteria

- [ ] Every site in "Must migrate", and every "Must classify" site confirmed as persisted, uses a specified algorithm over an explicit byte encoding. Options: SHA-256 truncated to u64 (sha2 is already a dependency), FNV-1a, or fixed-key `siphasher`.
- [ ] The close note contains the final classification table (every `DefaultHasher` site in non-test code, marked migrated or kept with a reason). `grep -rn "DefaultHasher" --include=*.rs` over the workspace on the final branch matches that table exactly.
- [ ] `HARNESS_CACHE_VERSION` is bumped once. The run manifest records a scope-hash algorithm version (or a new field name) so old and new manifests are not compared as if equal.
- [ ] Golden-value unit tests pin the output for fixed inputs of each migrated function (at least `default_seed("x")`, `stable_hash(...)`, `source_hash("...")`, `scope_hash(<fixed scope>)`, `external_audit_cache_root` for a fixed path), so an algorithm change fails a test. Proof at close: test output.
- [ ] The rust-conventions skill (`.claude/skills/rust-conventions/`) gains a one-line rule: no `DefaultHasher` for persisted, cache-path or promised-stable keys.

## Suggested approach

A small `stable_hash_u64(&[u8]) -> u64` helper per crate, or one shared helper if a common crate fits the dependency direction (cli -> core; frontends -> protocol).

## Out of scope

- Adding a rust-toolchain pin (separate decision). Without a pin, the golden tests catch algorithm changes only on the toolchain CI uses; that is acceptable.
- Hashes that are never persisted.

## Metadata

- Priority: P3
- Type: bug
- Labels: audit-2026-09-22, rust, cache, reproducibility
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-0m0vn (closed; --seed reproducibility), seed-for-explore-and-run, core-dead-code-removal
- Source findings: core-20, frontend-rust-13 (draft shatter-code/25)
