# Plan: str-ol3 — Stage 1 (Observe): Execute and trace collection

## Context

The `explorer.rs` module combines input generation with execution and trace collection. The issue asks for a clean, independent observe module that takes pre-generated inputs and produces execution traces — no Z3, no input generation logic. This gives users a composable building block for understanding execution coverage.

## Architecture

**New module: `observe.rs`** — pure execution + trace collection, separate from input generation.

The key insight: `observe_function()` is a **strict subset** of `explore_function()`. The explorer does input generation + observation. The observe module does observation only, with a clean `(inputs) → traces` interface.

Reuses existing `ObservationOutput` (already aliased as `ObserveResult`). No new output types needed.

## Implementation Steps

### Step 1: Make helpers `pub(crate)` in `explorer.rs`

Three private functions need `pub(crate)` visibility:
- `frontend_supports()` (line 464)
- `send_setup()` (line 469)
- `send_teardown()` (line 497)

Also need to make `ExploreError` variants accessible or create `ObserveError` with `From<ExploreError>`.

### Step 2: Create `observe.rs` with types

```rust
// Key types:
pub struct ObserveConfig {
    pub file: String,
    pub mocks: Vec<MockConfig>,
    pub setup_file: Option<String>,
    pub setup_mode: SetupMode,
    pub capabilities: FrontendCapabilities,
    pub project_root: Option<String>,
    pub loop_buckets: LoopBuckets,
    pub timeout: Option<Duration>,
    pub skip_instrument: bool,
}

pub enum ObserveError {
    Frontend(FrontendError),
    UnexpectedResponse(String),
    InstrumentationFailed(String),
}

pub struct BatchObservation {
    pub raw_results: Vec<(Vec<Value>, ExecuteResult)>,
    pub unique_path_hashes: HashSet<u64>,
    pub lines_covered: HashSet<u32>,
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
    pub new_path_executions: Vec<ExecutionSummary>,
}
```

### Step 3: Implement `observe_batch()`

Lower-level function — no instrumentation, no setup lifecycle. Pure execute loop:
- For each input: send Execute, compute path_hash, track coverage/discoveries
- Respects timeout
- Returns `BatchObservation`

### Step 4: Implement `observe_function()`

Full lifecycle wrapper:
1. Send `Instrument` (unless `skip_instrument`)
2. Handle setup lifecycle (per-function or per-execution)
3. Call the batch execution loop
4. Handle teardown
5. Return `ObservationOutput`

```rust
pub async fn observe_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    inputs: Vec<Vec<serde_json::Value>>,
    config: &ObserveConfig,
) -> Result<ObservationOutput, ObserveError>
```

### Step 5: Add `From<ExploreConfig>` for `ObserveConfig`

So callers can easily construct from existing configs.

### Step 6: Register in `lib.rs`

Add `pub mod observe;`

### Step 7: Tests

**Unit tests:**
- Empty inputs → zero-iteration output
- Path deduplication works correctly
- Discovery attribution (each branch_id at most once)
- Timeout enforcement stops execution early
- Error handling for instrument/execute failures

**Proptest:**
- Path dedup invariant: unique_paths == distinct path_hash count
- Coverage monotonicity: lines_covered grows monotonically
- Discovery uniqueness: no duplicate branch_ids in discoveries
- iterations == inputs.len() (or fewer if timeout hit)

Use shared generators from `test_arbitraries.rs`.

## Files to Modify

| File | Change |
|------|--------|
| `shatter-core/src/observe.rs` | **New** — ObserveConfig, ObserveError, BatchObservation, observe_function(), observe_batch() |
| `shatter-core/src/explorer.rs` | Make `frontend_supports`, `send_setup`, `send_teardown` `pub(crate)` |
| `shatter-core/src/lib.rs` | Add `pub mod observe;` |

## What NOT to do

- No input generation in observe.rs — that's the caller's job
- No new output type — reuse `ObservationOutput`
- No refactoring `explore_function` to delegate to observe (follow-up task)
- No Z3 or solver dependencies

## Verification

1. `cargo test -p shatter-core` — all tests pass including new observe tests
2. `cargo clippy -p shatter-core -- -D warnings` — clean
3. Existing explorer tests still pass (we only changed helper visibility)
