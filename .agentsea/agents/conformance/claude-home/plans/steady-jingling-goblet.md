# str-mgi: Function Classification into Strata — Already Implemented

## Context

Issue str-mgi requests implementing a function classification system that groups functions into strata along 4 axes: module/directory, complexity tier, dependency depth, and function kind. This is described as "a foundational component used by both --core-sample and --stratum flags."

## Finding: Already Fully Implemented

All requested functionality already exists in the codebase:

### `shatter-core/src/core_sample.rs` (1144 lines)
- **`ComplexityTier`** enum (lines 77-103): Buckets by branch count — Trivial(0-1), Simple(2-5), Moderate(6-15), Complex(16+)
- **`FunctionKind`** enum (lines 106-158): Classifies as Pure, Io, Constructor, Handler based on name patterns and dependencies
- **`StratumKey`** struct (lines 161-179): Composite key with module, complexity, depth, kind — exactly the 4 axes requested
- **`CoreSampleConfig`** / **`CoreSampleResult`** / **`StratumInfo`**: Full sampling infrastructure with budget, seed, per-stratum breakdown
- Stratified proportional sampling, stable hashing, dependency closure, batch selection
- Comprehensive proptest coverage

### `shatter-core/src/stratum.rs` (432 lines)
- **`StratumSpec`** parsing: single layers, ranges, open-ended, negative indices
- **`filter_layers()`**: Filters call graph topological layers by stratum range
- Extensive property-based tests

### CLI wiring (`shatter-cli/src/main.rs`)
- `--core-sample <BUDGET>`, `--seed`, `--batch`, `--stratum` flags — all defined and wired
- Integration with `ScanConfig` in scan_orchestrator

## Recommendation

Close str-mgi as already implemented. No code changes needed.
