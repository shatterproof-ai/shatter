# Symbolic Triage (str-xmtw)

## Context

The concolic orchestrator generates candidate inputs from Z3 solutions and fuzzing, then executes every one via frontend subprocess IPC. Many candidates hit already-seen paths — redundant executions that cost 5-50ms each (IPC + JSON serialization + function execution + branch recording) but discover nothing new. For functions with many branches, the worklist grows fast (up to `solvable.len()` Z3 entries + fuzz variants per new-path execution), and the redundancy rate climbs.

Symbolic triage predicts whether a candidate input will hit a known path by evaluating accumulated branch constraints against the candidate's concrete values. If the predicted path is already covered, skip the execution. This is viable because constraint evaluation is pure in-process arithmetic (~5-100us) while frontend execution is subprocess IPC (~5-50ms) — a 100-1000x cost difference.

## Key Design Decisions

**Tristate verdict, not boolean filter.** Triage returns one of three outcomes:
- **Skip** — predicted to hit an already-covered path. Don't execute (with periodic sampling to catch errors).
- **Execute** — predicted to reach a branch direction never seen before. Definitely execute. Includes novelty metadata (count of novel branches, depth of first novel branch) for future priority integration.
- **Indeterminate** — too many Unknown constraints or no matching trace template. Execute normally, as if triage didn't exist.

**Branch trace templates.** A "branch trace" is the ordered sequence of `(branch_id, constraint)` pairs from one observed execution. Traces are deduplicated by branch-ID sequence. For a candidate input, triage walks each trace and evaluates every constraint to predict the taken/not-taken outcome at each branch point. The predicted `(branch_id, taken)` sequence is hashed and checked against `covered_paths`.

**Novelty detection via per-branch direction tracking.** Separately from path-hash prediction, triage tracks which directions `(branch_id, true/false)` have been observed. If any branch in a trace is predicted to take a direction never seen before, the verdict is Execute regardless of path-hash matching. This prioritizes unexplored branches, with `first_novel_depth` (shallowest novel branch) as a tiebreaker — shallower novelty means a more fundamentally different path.

**Adaptive self-disabling.** Two mechanisms:
1. *Skip-rate check*: After 20 verdicts, if fewer than 10% result in Skip, triage isn't helping — stop trying. Covers functions with mostly Unknown constraints or highly productive exploration where every candidate hits a new path.
2. *Misprediction check*: If sampled executions show >25% misprediction rate, the constraint model is unreliable — disable triage for this function.

**Never triage user-provided inputs.** `InputSource::UserProvided` entries always execute.

**Triage skips don't affect plateau_counter or total_executions.** No execution happened, so no execution-based counter should change. The worklist drains faster via triage, and `WorklistExhausted` is the natural termination when all remaining candidates are predicted redundant.

**Trace count cap at 64.** Safety belt for loop-heavy functions that produce many distinct branch-ID sequences. Beyond 64, stop recording new traces but continue evaluating against existing ones. Even at 64 traces x 30 branches x ~50ns per evaluation = ~100us per candidate — still cheap.

### Trace Explosion

Many distinct branch-ID sequences arise from: early returns at different depths, loops with varying iteration counts, and exception paths. This is not a triage-specific problem — the existing pipeline already pays heavily for these via O(branches) Z3 calls per new-path execution and worklist growth. Triage's trace-walking cost is negligible compared to Z3 and IPC costs. The real risk is low prediction accuracy (candidates don't match any trace template), which the adaptive shutdown handles by measuring skip rate.

### Cost Model

| Operation | Cost |
|---|---|
| Triage (10 traces x 10 branches, typical) | ~5us |
| Triage (64 traces x 30 branches, worst case) | ~100us |
| Frontend execution (IPC + function + recording) | 5-50ms |

Even a 10% skip rate saves meaningful wall-clock time over a long exploration run.

## Implementation

### New file: `shatter-core/src/triage.rs`

The concrete SymExpr evaluator, branch prediction, and triage state.

**Types:**

```rust
/// Outcome of evaluating a single branch constraint against concrete inputs.
pub enum BranchPrediction {
    Taken,
    NotTaken,
    /// Cannot determine — Unknown node or unsupported Call.
    Indeterminate,
}

/// Triage verdict for a candidate input.
pub enum TriageVerdict {
    /// Predicted to hit an already-covered path.
    Skip { predicted_path_hash: u64 },
    /// Predicted to reach at least one never-seen branch direction.
    Execute { novel_count: usize, first_novel_depth: usize },
    /// Cannot make a reliable prediction.
    Indeterminate,
}
```

**Triage state (accumulated during exploration):**

```rust
pub struct TriageState {
    /// Observed branch traces: ordered (branch_id, constraint) from executions
    /// that discovered new paths. Deduplicated by branch-ID sequence.
    traces: Vec<BranchTrace>,
    /// Per-branch observed directions.
    observed_directions: HashMap<u32, [bool; 2]>,  // [seen_taken, seen_not_taken]
    /// Counters.
    pub skipped: usize,
    pub sampled: usize,
    pub mispredictions: usize,
    verdicts_rendered: usize,
    skips_achieved: usize,
    /// Set true when triage is no longer worthwhile for this function.
    disabled: bool,
}
```

**Core functions:**

1. `evaluate_constraint(expr: &SymExpr, params: &HashMap<String, &Value>) -> Option<Value>`
   - Pure interpreter: walks SymExpr tree, substitutes Param nodes from the map (with field-path traversal for dotted access), evaluates BinOp/UnOp/Call against concrete values.
   - Returns `None` for `SymExpr::Unknown`, unsupported `Call` names, type mismatches.
   - Supported BinOps: Eq, Ne, Lt, Le, Gt, Ge (compare JSON values), Add, Sub, Mul, Div, Mod (numeric), And, Or (short-circuit boolean), Not, Neg.
   - Supported Calls: `contains`/`includes`, `startsWith`/`prefix`, `endsWith`/`suffix`, `indexOf`/`index_of`, `length`/`len` — aligned with the 8 canonical string ops from `string-ops.yaml`.

2. `predict_branch(constraint: &SymExpr, params: &HashMap<String, &Value>) -> BranchPrediction`
   - Calls `evaluate_constraint`, maps `Some(Value::Bool(true))` -> Taken, `Some(Value::Bool(false))` -> NotTaken, anything else -> Indeterminate.

3. `TriageState::update(&mut self, branch_path: &[BranchDecision])`
   - Called after each new-path execution. Updates `observed_directions` and adds trace if branch-ID sequence is new (up to MAX_BRANCH_TRACES).

4. `TriageState::triage_candidate(&mut self, inputs: &[Value], param_names: &[String], covered_paths: &HashSet<u64>) -> TriageVerdict`
   - Checks `disabled` and `is_worthwhile()` first.
   - Walks each trace, evaluates constraints, predicts path hash.
   - If any trace predicts a novel branch direction -> `Execute { novel_count, first_novel_depth }`.
   - If a trace predicts a path hash in `covered_paths` and no trace predicts novelty -> `Skip`.
   - If no trace fully evaluates -> `Indeterminate`.
   - Updates internal counters.

5. `TriageState::record_sample(&mut self, predicted_hash: u64, actual_hash: u64)`
   - Called on sampled Skip executions. If hashes differ, increments `mispredictions`. If misprediction rate > 25%, sets `disabled = true`.

**Constants:**
- `MAX_BRANCH_TRACES: usize = 64`
- `TRIAGE_SAMPLE_INTERVAL: usize = 20` (execute every 20th skip)
- `MIN_VERDICTS_FOR_EVAL: usize = 20` (before checking worthwhile)
- `MIN_SKIP_RATE: f64 = 0.10` (below this, disable)
- `MAX_MISPREDICTION_RATE: f64 = 0.25`

### Modify: `shatter-core/src/lib.rs`

Register `pub mod triage;` (alphabetically between `stratum` and `sym_expr`).

### Modify: `shatter-core/src/orchestrator.rs`

Wire triage into the `explore` function loop:

1. After worklist/counter initialization (~line 311), create `TriageState::new()`.

2. After new-path discovery (after `executions.push(exec_result)` at line 448), call `triage_state.update(&exec_result.branch_path)`.

3. Before the `frontend.send(Command::Execute ...)` call (~line 352), insert triage check:

```rust
// Skip triage for user-provided inputs.
if entry.source != InputSource::UserProvided {
    match triage_state.triage_candidate(&entry.inputs, &param_names, &covered_paths) {
        TriageVerdict::Skip { predicted_path_hash } => {
            // Periodic sampling: execute anyway to catch model errors
            if triage_state.skipped % TRIAGE_SAMPLE_INTERVAL != 0 {
                continue;
            }
            // Sampling — will compare predicted vs actual after execution
            sampling_predicted_hash = Some(predicted_path_hash);
            triage_state.sampled += 1;
        }
        TriageVerdict::Execute { .. } => {
            // Predicted novel — proceed to execution (novelty metadata
            // available for future fitness-based worklist ordering)
        }
        TriageVerdict::Indeterminate => {
            // Can't predict — execute normally
        }
    }
}
```

4. After computing `path_id` (~line 377), if `sampling_predicted_hash` is set, call `triage_state.record_sample(predicted, actual)` and clear.

5. Add `triage_skipped` and `triage_mispredictions` fields to `ExploreResult`, populate from `triage_state` at construction.

## Testing

### Unit tests in `triage.rs`

- `evaluate_constraint` with numeric comparisons: `x > 10` with x=15 -> Bool(true), x=5 -> Bool(false)
- `evaluate_constraint` with string ops: `includes("hello", "ell")` -> Bool(true)
- `evaluate_constraint` with field paths: `config.timeout > 0` with nested JSON
- `evaluate_constraint` with Unknown -> None
- `evaluate_constraint` with unsupported Call -> None
- `predict_branch` maps eval results to Taken/NotTaken/Indeterminate
- `TriageState::update` deduplicates traces by branch-ID sequence
- `TriageState::update` respects MAX_BRANCH_TRACES cap
- `TriageState::triage_candidate` returns Skip for predicted-redundant input
- `TriageState::triage_candidate` returns Execute with correct novel_count/first_novel_depth for predicted-novel input
- `TriageState::triage_candidate` returns Indeterminate when all constraints are Unknown
- `is_worthwhile` returns false when skip rate < 10%
- `record_sample` disables triage when misprediction rate > 25%

### Integration in orchestrator tests

- Existing orchestrator tests must still pass (triage is additive).
- New test: verify triage_skipped > 0 for a function with many redundant worklist entries.

### E2E verification

- `cargo test --test e2e_concolic` must still discover the same paths.
- Spot-check: `triage_skipped` > 0 in E2E results for functions with >5 unique paths.

### Walkthrough

No changes needed — triage is internal to the orchestrator, not visible in CLI output.
