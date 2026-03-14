# str-cw3: Per-function result caching keyed by content hash

## Context

The codebase already has extensive caching infrastructure:
- `BehaviorMapCache` in `cache.rs` — caches `BehaviorMap` per function with fingerprint-based freshness
- `AnalysisCache` in `analysis_cache.rs` — caches `FunctionAnalysis` with content hash + protocol version
- `fingerprint.rs` — SHA-256 fingerprints (shallow + deep with callee propagation)
- `scan_orchestrator.rs` — integrated cache checks before exploration

**Gaps to close:**
1. `BehaviorMapCache` lacks protocol version — upgrades don't invalidate stale entries
2. `FunctionSpec` is not cached — rebuilt every time from exploration results
3. No proptest coverage for fingerprint/cache key invariants

## Plan

### Step 1: Add protocol version envelope to BehaviorMapCache

**File:** `shatter-core/src/cache.rs`

- Add private `BehaviorMapCacheEntry { protocol_version: String, behavior_map: BehaviorMap }` (Serialize + Deserialize)
- `store()`: wrap BehaviorMap in entry with `PROTOCOL_VERSION` before serializing
- `load()`: deserialize as entry, return `Ok(None)` on version mismatch or deserialization failure (old format gracefully becomes cache miss)
- `is_fresh()`: delegates to `load()` which already handles version check
- Update `load_old_cache_without_nondeterministic_fields` test (old bare JSON → None)
- Add tests: `protocol_version_mismatch_returns_none`, `old_bare_format_returns_none`

### Step 2: Extract shared path helper, add SpecCache

**File:** `shatter-core/src/cache.rs`

- Extract `cache_base_path(cache_dir, function_id) -> PathBuf` from `BehaviorMapCache::path_for`
- `BehaviorMapCache::path_for` → `cache_base_path(...).with_extension("json")` (no behavior change)
- Add `SpecCacheEntry { protocol_version, spec: FunctionSpec }` (private)
- Add `SpecCache` struct with: `new`, `load`, `store`, `is_fresh`, `default_dir`, `path_for`
- `path_for` → `cache_base_path(...).with_extension("spec.json")` (colocated with behavior map)
- Tests: roundtrip, missing returns None, protocol mismatch, freshness, hierarchical path

### Step 3: Integrate SpecCache in CLI explore command

**File:** `shatter-cli/src/main.rs`

- Instantiate `SpecCache` alongside existing `BehaviorMapCache`
- On cache hit (function in `fresh_set`): load cached spec, output it directly
- After building spec (line ~1342): store to `SpecCache`
- Use same `"file:funcName"` format for function_id consistency

### Step 4: Add proptest for fingerprint invariants

**File:** `shatter-core/src/fingerprint.rs`

Using `arb_function_analysis()` from `test_arbitraries.rs`:
- Determinism: same inputs → same fingerprint
- Length: always 64 hex characters
- Source sensitivity: different source → different fingerprint
- Deep fingerprint determinism and callee sensitivity

## Key Constraints

- No `unwrap()` in library code
- Reuse existing `arb_function_spec()`, `arb_behavior_map()`, `arb_function_analysis()` generators
- Old cache format gracefully degrades to cache miss (no migration needed)
- Store in `.shatter/cache/` using existing mirrored tree structure

## Verification

1. `cargo test -p shatter-core` — unit + proptest pass
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — clean
3. `cargo test --test e2e_concolic` — pipeline still works with versioned cache
4. Manual: delete `.shatter/cache/`, run explore twice, verify second run hits cache
