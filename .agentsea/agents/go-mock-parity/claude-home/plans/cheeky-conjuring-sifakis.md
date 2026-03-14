# str-9ab: Restructure orchestrator::explore() into Observe → Solve/Generate phases

## Context

The current `explore()` function in `orchestrator.rs` (lines 329–684) has a tight inner loop where each worklist entry is executed, then immediately solved/fuzzed/drilled — observation and input generation are interleaved within a single `while let Some(entry) = worklist.pop()` loop. This makes it hard to plug in alternative generators or compose phases independently.

The goal is to restructure the loop into two distinct phases per round:
1. **Observe phase**: drain the worklist, execute all pending inputs, collect results
2. **Solve/Generate phase**: process all observations, run Z3/fuzz/drill, produce new worklist entries

This creates a clean seam for a future pluggable generator trait.

## Key files

- `shatter-core/src/orchestrator.rs` — main target (the `explore()` function)
- `shatter-core/src/strategy.rs` — `Z3SolverStrategy` (already extracted, not directly modified here)
- `shatter-core/src/pipeline.rs` — `From<ExploreResult> for ObservationOutput` (unchanged)

## Design

### New types

```rust
/// Output of a single observation (one execution with its classification).
pub struct Observation {
    pub inputs: Vec<serde_json::Value>,
    pub result: ExecuteResult,
    pub source: InputSource,
    pub path_id: u64,
    pub is_new_path: bool,
}

/// Output of the Solve/Generate phase — new candidate inputs to feed back.
pub struct SolveOutput {
    pub z3_candidates: Vec<WorklistEntry>,
    pub fuzz_candidates: Vec<WorklistEntry>,
    pub drill_candidates: Vec<WorklistEntry>,
    pub z3_count: usize,
    pub fuzz_count: usize,
    pub drill_count: usize,
}
```

### Restructured loop

The main `explore()` function becomes:

```
loop {
    // --- Observe phase ---
    let observations = observe_round(...);  // drain worklist, execute all, collect
    if observations.is_empty() { break; }   // worklist exhausted

    // Update coverage state from observations
    for obs in &observations { ... update covered_paths, triage, frontiers, discoveries ... }

    // Check termination conditions
    if should_terminate(...) { break; }

    // --- Solve/Generate phase ---
    let solve_output = solve_and_generate(...);  // Z3 + fuzz + drill from new-path observations

    // Feed candidates back to worklist
    for entry in solve_output.z3_candidates { worklist.push(entry); }
    for entry in solve_output.fuzz_candidates { worklist.push(entry); }
    for entry in solve_output.drill_candidates { worklist.push(entry); }

    z3_generated += solve_output.z3_count;
    fuzz_generated += solve_output.fuzz_count;
    drill_generated += solve_output.drill_count;
}
```

### Phase functions

#### `observe_round()`

Extracted helper that drains the worklist up to a batch limit (or all entries), executing each via the frontend. Returns `Vec<Observation>`. Handles:
- Termination budget checks (max_iterations, max_executions, timeout, plateau)
- Triage skip/sample logic
- Frontend error handling (skip on error, continue)

#### `solve_and_generate()`

Extracted helper that takes the new-path observations from this round plus shared state (frontier_set, param_infos, rng) and produces `SolveOutput`. Handles:
- `extract_sym_constraints()` → Z3 solving → `overlay_solved_values()`
- Fuzz generation for Unknown constraints
- Parameter drilling on stalled frontiers

### What stays in the main loop

- Coverage state updates (covered_paths, seen_branch_ids, seen_branch_sides, frontier_set, discoveries, triage_state)
- Termination reason tracking
- Accumulating executions and raw_results
- Building the final `ExploreResult`

### Batch size for observe round

Each observe round drains the **entire** worklist (all pending entries). This preserves the current behavior exactly — the only change is that solving happens after all pending inputs are executed, not interleaved. In practice, the first round has only seeds + user inputs, then subsequent rounds have Z3/fuzz/drill candidates from the previous round's observations.

**Important**: This changes the execution order slightly — currently Z3 results from early executions can be tried before later seeds finish. With batch observe, all seeds execute first, then all Z3 candidates from those seeds. This is a minor behavioral difference but should not affect correctness or coverage quality. The priority queue ordering within each batch is preserved.

## Implementation steps

1. Define `Observation` and `SolveOutput` types (after `ExploreResult`)
2. Extract `observe_round()` — takes worklist, frontend, config, triage state; returns observations + updated counters
3. Extract `solve_and_generate()` — takes observations, frontier_set, param_infos, config, rng; returns SolveOutput
4. Restructure the main `explore()` loop to call these in sequence
5. Update existing unit tests if they test internal loop behavior
6. Add unit tests for `SolveOutput` type and the new phase functions
7. Run `cargo test` and `cargo clippy -- -D warnings` in shatter-core
8. Run `cargo test --test e2e_concolic` to verify pipeline behavior preserved

## Verification

- `cargo test -p shatter-core` — unit tests pass
- `cargo clippy -p shatter-core -- -D warnings` — clean
- `cargo test --test e2e_concolic` — E2E pipeline discovers same branches
- Existing mock frontend tests (`explore_noop_frontend_exhausts_worklist`, `explore_concolic_frontend_discovers_paths_via_z3`) still pass

## Risks

- The batch-observe change means Z3 candidates from round N aren't available until round N+1. This could theoretically require one more round to converge, but won't affect final coverage.
- The `observe_round()` function needs access to mutable triage state and covered_paths for skip prediction — these are passed as mutable references.
