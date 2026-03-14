# str-0p2j: Observe/Explorer Path Alignment

## Context

`observe_function` (observe.rs) was built as the Stage-1 observe abstraction with a clean instrument → setup → batch execute → teardown lifecycle. However, `explore_function` (explorer.rs) duplicates this lifecycle inline because it needs per-iteration input generation, float probing, mock generation, and SetupManager integration.

The core execution classification logic — track lines, hash path, track branch discoveries, build ExecutionSummary, push raw_results — is copy-pasted in **three** places:
1. `observe_batch` (observe.rs:159-187)
2. `observe_batch_with_per_execution_setup` (observe.rs:383-411)
3. `explore_function` (explorer.rs:851-881)

This creates drift risk: a change to one copy (e.g., adding a field to ExecutionSummary) silently misses the others.

## Approach: Extract shared classification + add divergence tests

Routing `explore_function` through `observe_function` is impractical — explore generates inputs one-at-a-time and interleaves float probing, mock generation, and generator prefetch. Instead:

### Step 1: Extract `ExecutionTracker` helper struct (explorer.rs)

Create a struct that encapsulates the repeated state + logic:

```rust
pub(crate) struct ExecutionTracker {
    seen_paths: HashSet<u64>,
    all_lines: HashSet<u32>,
    seen_branch_ids: HashSet<u32>,
    discoveries: Vec<(u32, DiscoveryMethod)>,
    new_path_executions: Vec<ExecutionSummary>,
    raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)>,
    loop_buckets: LoopBuckets,
}

impl ExecutionTracker {
    fn new(loop_buckets: LoopBuckets) -> Self;

    /// Record one execution result. Returns true if this was a new path.
    fn record(&mut self, inputs: Vec<serde_json::Value>, mocks: Vec<MockConfig>, result: ExecuteResult) -> bool;

    /// Consume into final observation fields.
    fn finish(self) -> (HashSet<u64>, HashSet<u32>, Vec<(u32, DiscoveryMethod)>, Vec<ExecutionSummary>, Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)>);
}
```

### Step 2: Wire `ExecutionTracker` into all three sites

- `observe_batch`: replace inline tracking with `ExecutionTracker::record()`
- `observe_batch_with_per_execution_setup`: same
- `explore_function`: same

This eliminates the classification duplication. The lifecycle (instrument, setup, teardown) remains separate because the two functions genuinely differ there (observe has no SetupManager, no float probes, no generators).

### Step 3: Add divergence tests

Add tests in `observe.rs` that verify:
1. `ObserveConfig::from(&ExploreConfig)` preserves all fields (already exists, extend if needed)
2. `ExecutionTracker::record()` produces same results regardless of batch vs one-at-a-time calls (property test)
3. Both `observe_function` and `explore_function` handle the same set of `ResponseResult` variants for instrument and execute (static test via pattern match exhaustiveness)

## Files to modify

- `shatter-core/src/explorer.rs` — add `ExecutionTracker`, refactor `explore_function` to use it
- `shatter-core/src/observe.rs` — refactor `observe_batch` and `observe_batch_with_per_execution_setup` to use `ExecutionTracker`, add divergence tests

## Verification

1. `cargo test --lib -p shatter-core`
2. `cargo clippy -p shatter-core -- -D warnings`
3. `cargo test --test e2e_concolic` (touches explorer lifecycle)
