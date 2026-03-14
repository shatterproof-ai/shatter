# str-au5: --stratum flag implementation

## Context

The `--stratum` flag is **already substantially implemented**. Both dependencies (str-bat: range parsing, str-mgi: function classification) are closed. The CLI flag exists on `scan`, stratum parsing/resolution/filtering works, and cross-stratum auto-mocking functions mechanically. However, the implementation has gaps in **mock source attribution** and **test coverage** for cross-stratum dependency mocking.

## Gaps to Close

### 1. MockSource doesn't distinguish stratum-excluded from external auto-mocks

**File:** `shatter-core/src/scan_orchestrator.rs:95-102`

Currently `MockSource::TypeAwareStub` is used for ALL auto-mocks, whether the function was excluded by stratum filtering or is truly external. Add a `StratumExcluded` variant so downstream consumers (reports, exports) can distinguish.

```rust
pub enum MockSource {
    CachedBehaviorMap,
    TypeAwareStub,
    StratumExcluded,  // NEW: auto-mock for function excluded by --stratum filter
}
```

### 2. Tag stratum-excluded mocks in parallel_scan and serial scan

**File:** `shatter-core/src/scan_orchestrator.rs`

In both `scan()` (~line 390-420) and `parallel_scan()` (~line 766-778), when generating auto-mocks, check if the dependency exists in the full scan analysis set but was excluded by stratum filtering. If so, use `MockSource::StratumExcluded` instead of `TypeAwareStub`.

**Approach:** Pass the set of stratum-excluded function names into the mock-building section. For each auto-mock, check if the symbol is in the excluded set → `StratumExcluded`, otherwise → `TypeAwareStub`.

### 3. Add integration test for cross-stratum dependency mocking

**File:** `shatter-core/src/scan_orchestrator.rs` (test module)

Add a test that:
- Creates a 3-layer call chain: `root` (layer 2) → `mid` (layer 1) → `leaf` (layer 0)
- Applies stratum "1" (only mid layer)
- Verifies `mid` is explored
- Verifies `leaf` receives `MockSource::StratumExcluded` (not `TypeAwareStub`)
- Verifies `root` is not explored

### 4. Surface stratum-excluded mocks in scan report output

**File:** `shatter-core/src/scan_orchestrator.rs` (report formatting)

When printing mock usage in scan results, label `StratumExcluded` distinctly from `TypeAwareStub` (e.g., "stratum-excluded" vs "auto-mock").

## Files to Modify

| File | Change |
|------|--------|
| `shatter-core/src/scan_orchestrator.rs` | Add `StratumExcluded` to `MockSource`, tag excluded mocks, add test, update report formatting |

## Verification

1. `cargo test -p shatter-core` — unit tests pass including new cross-stratum test
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — no warnings
3. Existing stratum tests still pass (stratum_then_core_sample, stratum_only_filters)
