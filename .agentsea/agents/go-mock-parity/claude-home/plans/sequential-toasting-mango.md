# str-bmh: Fingerprint Comparison and Merging

## Context

Issue str-bmh asks for fingerprint comparison against existing specs and result merging for incremental exploration. However, this functionality is **already fully implemented** in `shatter-core/src/spec.rs`:

- `compute_incremental_plan()` (lines 680-724) — compares current deep fingerprints against saved spec fingerprints, classifying functions as stale/fresh/removed
- `merge_file_spec_bundles()` (lines 732-754) — merges previous results with new exploration results
- `IncrementalPlan` struct (lines 660-667) — holds the comparison result

The CLI already uses both functions in `run_explore()` (main.rs:1040, 1398) and `run_stale()` (main.rs:2930).

**Unit tests exist** (lines 1702-1928): 6 tests for incremental plan + 3 tests for merge.

**What's missing**: proptest coverage for the comparison and merge logic. The existing proptest block (lines 2165-2244) only covers serialization roundtrips and invariant conversion — not the incremental plan or merge invariants.

## What to implement

Add proptest properties for `compute_incremental_plan` and `merge_file_spec_bundles` to validate their core invariants with generated inputs.

### Property tests to add (in `spec.rs` proptests module)

1. **Merge preserves count**: `|new_specs| + |carried_over_fresh| == |merged.functions|` (no duplicates, no drops beyond removed)

2. **Merge idempotent**: merging with empty new_specs and full current_names returns all existing specs

3. **Merge new_specs override existing**: if a function name appears in both new_specs and existing, the merged result uses the new_spec version

4. **Merge drops removed**: functions not in current_function_names are absent from the merged result

5. **Merge superset**: every function in the merged result has a name in current_function_names

### Files to modify

- `shatter-core/src/spec.rs` — add proptest properties in the existing `proptests` module

### Generators needed

- Use `arb_function_spec()` from `test_arbitraries.rs` (already imported)
- Generate `Vec<FunctionSpec>` for existing and new_specs
- Generate `HashSet<String>` for current_function_names (derived from the specs to ensure valid scenarios)

Note: `compute_incremental_plan` requires file I/O (reads source text to compute fingerprints), making it harder to proptest directly. Focus proptest coverage on `merge_file_spec_bundles` which is a pure function.

## Verification

```bash
cd shatter-core && cargo test -- proptests && cargo clippy -- -D warnings
```
