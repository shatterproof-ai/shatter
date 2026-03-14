# str-a7s: --core-sample flag

## Context

The issue asks to implement `--core-sample` flag for proportional sampling with stratified selection algorithm.

## Finding: Already Implemented

The `--core-sample` feature is **already fully implemented** on `main` (commit 49b9380):

### Implemented Components
1. **`shatter-core/src/core_sample.rs`** (38KB) — Full stratified proportional sampling algorithm
   - 4 classification axes: module/directory, complexity tier, dependency depth, function kind
   - `select_core_sample()` with largest-remainder budget allocation
   - `select_batch()` for progressive batching
   - `detect_next_batch()` for auto-detection
   - `default_seed()` for deterministic seeding
   - Dependency closure computation

2. **`shatter-cli/src/main.rs`** — CLI wiring
   - `--core-sample <SPEC>` (percentage or absolute count)
   - `--seed <SEED>` (deterministic seed)
   - `--batch <SPEC>` (progressive batch processing)
   - Integration with `--stratum` (pre-filtering)

3. **`shatter-core/src/scan_orchestrator.rs`** — `SamplingContext` for reporting

### Test Coverage
- 23 tests passing (unit + integration)
- Clippy clean

## Recommendation

No implementation work needed. The branch `str-a7s` can be closed as already complete, or we can verify with an E2E test and mark done.
