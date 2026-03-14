# str-0p2j: Observe/Explorer Path Alignment

## Context

Three code paths duplicate the same execute-track lifecycle:
1. `observe.rs::observe_batch()` (lines 131-188) — well-tested but **unused by production code**
2. `explorer.rs::explore_function()` main loop (lines 822-882) — primary random exploration path
3. `orchestrator.rs::observe_one()` (line 431) — concolic path, uses different hashing (defer)

The duplication means bug fixes applied to one path silently miss the other. The observe module was designed as the canonical execution primitive but was never wired in. This plan routes explore through observe, making observe the single source of truth.

## Approach: Extract `observe_single()`, route both `observe_batch` and `explore_function` through it

### Step 1: Add `observe_single()` to observe.rs

New function — the atomic execute-track primitive. Both `observe_batch` and `explore_function` will call it.

```rust
pub struct SingleObservation {
    pub exec_result: ExecuteResult,
    pub path_hash: u64,
    pub is_new_path: bool,
    pub new_branch_ids: Vec<u32>,
    pub execution_summary: Option<ExecutionSummary>,  // Some if is_new_path
}

pub async fn observe_single(
    frontend: &mut Frontend,
    function_name: &str,
    inputs: &[serde_json::Value],
    mocks: &[MockConfig],
    setup_context: Option<&SetupContextStack>,
    loop_buckets: &LoopBuckets,
    seen_paths: &mut HashSet<u64>,
    seen_branch_ids: &mut HashSet<u32>,
    all_lines: &mut HashSet<u32>,
) -> Result<SingleObservation, ObserveError>
```

Body is the execute+track logic currently in `observe_batch` lines 136-185: send Execute, parse response, update line coverage, compute path_hash, check/insert seen_paths, track branch discoveries, build ExecutionSummary if new path.

**Files:** `shatter-core/src/observe.rs`

### Step 2: Rewrite `observe_batch()` to call `observe_single()`

Replace the inline loop (lines 131-188) with a loop that calls `observe_single()` and accumulates into `BatchObservation`. Same public contract, same behavior.

**Files:** `shatter-core/src/observe.rs`

### Step 3: Rewrite `observe_batch_with_per_execution_setup()` similarly

Replace its inline execute+track body with a call to `observe_single()`. Setup/teardown wrapping stays.

**Files:** `shatter-core/src/observe.rs`

### Step 4: Route `explore_function()` main loop through `observe_single()`

Replace explorer.rs lines 822-879 (execute command, parse response, track coverage, track discoveries, build ExecutionSummary) with a call to `observe::observe_single()`. Keep all surrounding logic intact:
- Input generation (lines 786-811) — stays in explorer
- Mock generation (lines 816-820) — stays in explorer
- Per-execution setup/teardown (lines 761-782, 843-849) — stays in explorer
- Timeout check (lines 752-756) — stays in explorer

Remove dead `path_counts` HashMap (line 634, 856) since it's only incremented, never read.

**Files:** `shatter-core/src/explorer.rs`

### Step 5: Move shared types/functions to observe.rs (with re-exports)

Move from explorer.rs to observe.rs:
- `path_hash()`, `legacy_path_hash()`, `scope_aware_hash()` — core to observation
- `classify_error_intent()`, `ErrorIntentLabel` — used by observe_single
- `ExecutionSummary` — returned by observe_single
- `LoopBuckets` — parameter to observe_single

Add re-exports in explorer.rs so no downstream code breaks:
```rust
pub use crate::observe::{path_hash, legacy_path_hash, scope_aware_hash,
    classify_error_intent, ErrorIntentLabel, ExecutionSummary, LoopBuckets};
```

**Files:** `shatter-core/src/observe.rs`, `shatter-core/src/explorer.rs`

### Step 6: Tests

1. **Unit test for `observe_single()`**: mock frontend, verify it updates seen_paths/seen_branch_ids/all_lines correctly, verify SingleObservation fields
2. **Proptest for `observe_single()` tracking invariants**: path is new iff hash was absent, discovery branch_ids are always new, lines_covered grows monotonically
3. **Existing tests must pass**: observe.rs tests, explorer.rs tests, E2E concolic tests

**Files:** `shatter-core/src/observe.rs` (test modules)

### Step 7: Document lifecycle source-of-truth

Add module-level doc comment to observe.rs documenting that `observe_single()` is the canonical execution primitive, and that all execution paths (random explore, observe_batch, future orchestrator) should route through it.

**Files:** `shatter-core/src/observe.rs`

## Out of Scope (Phase 2)

- Aligning orchestrator's `observe_one()` to use `observe_single()` — orchestrator uses `hash_branch_path` (different hashing), has triage/budget logic, and different error handling. Separate task.
- Moving `send_setup`/`send_teardown`/`frontend_supports` to observe.rs — these are lifecycle utilities, not execution primitives. Lower priority.

## Verification

1. `cargo test -p shatter-core` — unit + proptest
2. `cargo clippy -p shatter-core -- -D warnings` — lint clean
3. `cargo test --test e2e_concolic` — full pipeline (mandatory: touching explorer.rs)
4. `/pre-completion` skill

## Critical Files

| File | Changes |
|------|---------|
| `shatter-core/src/observe.rs` | Add `observe_single()`, `SingleObservation`; refactor `observe_batch` and `observe_batch_with_per_execution_setup`; receive moved types; add tests |
| `shatter-core/src/explorer.rs` | Route main loop through `observe_single()`; move types out with re-exports; remove dead `path_counts` |
| `shatter-core/src/orchestrator.rs` | No changes (Phase 2) |
| `shatter-cli/src/main.rs` | No changes |
