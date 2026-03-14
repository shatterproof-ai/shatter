# str-1fr: Cache invalidation when function dependencies change

## Context

The `scan_orchestrator` already handles cross-file dependency-aware cache invalidation correctly — it accumulates `deep_fingerprints` across all functions from all files, processes leaves-first, and uses the cross-file-aware deep fingerprint for `is_fresh()` checks.

However, the **single-file commands** (`explore` and `stale`) use `compute_deep_fingerprints()` which only considers intra-file callees. If function A calls function B in another file and B changes, A is incorrectly considered "fresh" and skipped.

## Plan

### 1. Extend `compute_deep_fingerprints()` — `shatter-core/src/fingerprint.rs`

Add parameter: `external_fingerprints: &HashMap<String, String>` (callee name → deep fingerprint from cache).

Changes:
- Seed `deep` map from `external_fingerprints` (so cross-file callee FPs are available during computation)
- Keep `topo_callees_map` filtered to `name_set` (for Kahn's algorithm ordering — unchanged)
- Use **unfiltered** callee sets when calling `compute_deep_fingerprint()` (so cross-file deps are incorporated)
- Filter return map to only include functions from `analyses` (don't leak external entries)
- Update all existing test call sites to pass `&HashMap::new()`

### 2. Extend `compute_incremental_plan()` — `shatter-core/src/spec.rs`

Add parameter: `external_fingerprints: &HashMap<String, String>`. Pass through to `compute_deep_fingerprints()`. Update test call sites to pass `&HashMap::new()`.

### 3. Add helper + wire up CLI — `shatter-cli/src/main.rs`

Add `load_external_fingerprints(analyses, cache) -> HashMap<String, String>`:
- For each function's `dependencies`, if the dep symbol is NOT in the current file's function set, load its `BehaviorMap` from cache and extract the stored fingerprint
- Cache keys are bare function names (matching `ExternalDependency.symbol`)

Wire into:
- **`explore` command** (~line 1036): Compute external FPs before incremental plan. Pass to both `compute_incremental_plan()` and `compute_deep_fingerprints()`
- **`stale` command** (~line 2950): Add `--cache-dir` flag, compute external FPs, pass to `compute_incremental_plan()`

### 4. Tests

**`fingerprint.rs`:**
- Unit: cross-file callee FP change propagates to caller's deep FP
- Unit: external entries not leaked into return map
- Unit: empty external map preserves existing behavior
- Proptest: changing external callee FP changes caller deep FP

**`spec.rs`:**
- Unit: cross-file dep change marks caller stale in incremental plan

## Verification

1. `cargo test -p shatter-core` — unit + proptest
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings`
3. `cargo test --test e2e_concolic` — E2E pipeline
