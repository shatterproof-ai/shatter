# Compiler-Inspired Loop Analysis Techniques for Shatter

## Context

Shatter's concolic engine struggles with loops containing conditionals due to path explosion: a loop with K branches and N iterations creates O(N*K) negation candidates. The solver treats loop branches identically to non-loop branches, leading to wasted budget on redundant iterations. Shatter already has scope-aware hashing with LoopBuckets for deduplication (passive), but lacks active loop-aware optimizations in the solver and worklist.

This plan defines 7 techniques drawn from compiler optimization, branch prediction, and profiling. Techniques 1-4 are low-effort incremental improvements to existing infrastructure. Techniques 5-7 are medium/high-effort architectural additions to be deferred.

---

## Technique 1: Coverage-Guided Diminishing Returns

**Summary**: Track per-loop branch coverage; suppress negation candidates for loops whose coverage has converged (no new branches discovered in recent observations).

**Files to modify**:
- `shatter-core/src/orchestrator.rs` — add `LoopCoverageTracker`, `extract_loop_context()`, wire into `explore()` and `solve_and_generate()`

**Implementation**:
1. Add `extract_loop_context(scope_events) -> HashMap<u32, Option<u32>>` — walks scope_events to map each branch_id to its innermost enclosing loop_id (same walk pattern as `scope_aware_hash()` but simpler)
2. Add `LoopCoverageTracker` struct:
   - `coverage: HashMap<u32, HashSet<(u32, bool)>>` — per-loop accumulated (branch_id, taken) pairs
   - `stale_count: HashMap<u32, u32>` — consecutive observations with no new coverage per loop
   - `converged: HashSet<u32>` — loop_ids that have converged
   - `window: u32` — configurable threshold (default 3)
   - Methods: `update()`, `is_converged()`, `converged_loop_branch_indices()`
3. Instantiate tracker before main loop in `explore()`. After each new-path observation, call `tracker.update()`
4. In `solve_and_generate()`, compute skip_indices from converged loops; skip those indices in the Z3 negation loop
5. Add `loop_convergence_window: u32` to config (default 3, 0 disables)
6. Graceful fallback: empty scope_events → no-op (Go frontend safe)

**Testing**: Unit tests for tracker convergence logic. Proptest: random TraceEvent sequences → `extract_loop_context` never panics. Integration: verify converged-loop suppression reduces executions without reducing final coverage.

---

## Technique 2: Loop-Invariant Branch Detection

**Summary**: Detect branches inside loops whose `taken` value is constant across all observed iterations; skip redundant negation for later occurrences.

**Approach**: Dynamic (observe executions in orchestrator) rather than static (instrumentor). Language-agnostic, no frontend/protocol changes needed.

**Files to modify**:
- `shatter-core/src/orchestrator.rs` — add `LoopInvariantDetector`, wire into `explore()` and `solve_and_generate()`

**Implementation**:
1. Add `LoopInvariantDetector` struct:
   - `observations: HashMap<(u32, u32), InvariantStats>` — per (loop_id, branch_id) tracks constant_count vs varied_count across multi-iteration executions
   - `invariant_branches: HashSet<u32>` — confirmed invariant branch_ids
   - `min_observations: u32` — require N concordant multi-iteration executions before classifying (default 2)
   - Methods: `observe()`, `is_invariant()`, `first_occurrence_indices()`
2. `observe()` walks scope_events to find loop scopes, collects per-iteration (branch_id, taken) for each loop. For loops with >1 iteration, checks if each branch's `taken` was constant. Updates stats.
3. Revocation: if a previously-invariant branch is observed to vary, remove from `invariant_branches`
4. In `solve_and_generate()`, for invariant branches inside loops, only keep the first occurrence index in the negation candidate set; skip subsequent occurrences from later iterations
5. Combine skip_indices with Technique 1's converged-loop set

**Testing**: Unit tests with synthetic traces (branch constant vs varying). Proptest: detector never marks a branch invariant if it has ever varied. Integration: function with loop-invariant condition → solver negates it once not N times.

---

## Technique 3: Loop Peeling (Prioritize Boundary Iterations)

**Summary**: Boost priority of negation candidates targeting loop boundary positions (iteration 0, 1, first exit) since off-by-one and empty-input bugs cluster at boundaries.

**Files to modify**:
- `shatter-core/src/orchestrator.rs` — add `classify_iteration_positions()`, apply fitness boost in `solve_and_generate()`

**Implementation**:
1. Add `IterationPosition` enum: `First`, `Second`, `FirstExit`, `Interior`, `NonLoop`
2. Add `classify_iteration_positions(scope_events) -> Vec<IterationPosition>` — walks scope_events, maintains per-loop-id iteration counter, classifies each branch event
3. In `solve_and_generate()`, when creating WorklistEntry for Z3Solved candidates:
   - `First` or `FirstExit` → set `fitness = Some(1.0)` (max priority)
   - `Second` → set `fitness = Some(0.9)`
   - `Interior` / `NonLoop` → leave at `None` (normal priority)
4. Uses existing fitness-based priority in `WorklistEntry::cmp()` (fitness-scored entries already outrank non-scored)
5. Graceful fallback: empty scope_events → all `NonLoop`, no boosting

**Testing**: Unit tests for `classify_iteration_positions` with single/nested/empty loops. Verify boundary candidates sort above interior candidates in worklist. Integration: off-by-one bug at iteration 0 found before interior iterations.

---

## Technique 4: Profile-Guided Prioritization

**Summary**: Collect branch frequency data from the random exploration phase; bias concolic effort toward rare/uncovered branches.

**Files to modify**:
- `shatter-core/src/explorer.rs` — add `collect_branch_profile()`
- `shatter-core/src/orchestrator.rs` — accept `Option<BranchProfile>`, use in fitness scoring
- `shatter-core/src/genetic_fitness.rs` — add `rarity` weight component
- `shatter-core/src/frontier.rs` — add rarity to frontier priority (gated on profile presence)
- `shatter-cli/src/main.rs` — wire profile from random phase to concolic phase

**Implementation**:
1. Add `BranchProfile` struct: `frequencies: HashMap<(u32, bool), f64>`, `total_executions: usize`, method `rarity(branch_id, taken) -> f64` (1.0 - frequency, 0.0 for always-seen, 1.0 for never-seen)
2. Add `collect_branch_profile(output: &ObservationOutput) -> BranchProfile` in `explorer.rs` — counts per-execution (branch_id, taken) occurrences across raw_results, divides by total
3. Add `rarity: f64` weight to `FitnessWeights` (new default: coverage 0.25, proximity 0.35, unknown_bonus 0.10, novelty 0.15, rarity 0.15)
4. In `genetic_fitness::score()`, compute rarity_score as average `profile.rarity()` across branches in the execution. When no profile, rarity defaults to 0.5
5. Add `rarity_boost: Option<f64>` to `Frontier`, factor into `frontier_priority()` (higher rarity preferred, gated on profile existence)
6. CLI wiring: in scan mode, profile is naturally available from random phase. In standalone concolic mode, profile is `None` (feature disabled)

**Testing**: Unit tests for `collect_branch_profile` and `rarity()`. Proptest: all frequencies in [0.0, 1.0]. Integration: random phase covers default path, concolic with profile finds rare branch faster.

---

## Technique 5: Induction Variable + Trip Count Analysis (DEFERRED — MEDIUM EFFORT)

**Summary**: Recognize canonical counted loops (`for (i = 0; i < n; i++)`), extract induction variable metadata, encode `i == k` directly in Z3 instead of building k prefix backedge constraints.

**Prerequisites**: None (independent of 1-4)

**Files to modify/create**:
- `shatter-core/src/protocol.rs` — add `LoopInfo`, `InductionVar` types; add `loops: Vec<LoopInfo>` to `FunctionAnalysis`
- `shatter-core/src/loop_analysis.rs` (NEW) — `rewrite_loop_constraints()` function
- `shatter-core/src/orchestrator.rs` — call `rewrite_loop_constraints()` before `solve_for_new_path()`
- `shatter-ts/src/instrumentor.ts` — add `analyzeForLoopInductionVar()` for canonical ForStatement recognition
- `shatter-go/instrument/visitor.go` — add `analyzeForStmtInductionVar()`
- `shatter-rust/src/protocol.rs` — add types for deserialization

**Key types**:
```rust
struct InductionVar { name: String, init_expr: SymExpr, step_expr: SymExpr, bound_expr: SymExpr, bound_op: BinOpKind }
struct LoopInfo { loop_id: u32, line: u32, induction_var: Option<InductionVar> }
```

**Algorithm**: Walk scope_events to associate constraint indices with (loop_id, iteration). For constraints in loops with InductionVar, replace backedge constraint with `induction_var == init + step * iteration_number`. Rewritten constraints pass through standard `solve_for_new_path()`.

**Risks**: Non-canonical loops (i modified in body) misclassified → be conservative, only match when induction variable is unmodified in body. Float induction variables deferred.

---

## Technique 6: State Merging at Backedges (DEFERRED — MEDIUM EFFORT)

**Summary**: Merge per-iteration symbolic states using ITE chains. Instead of N negation candidates per loop, produce one merged formula where iteration count is a Z3 free variable.

**Prerequisites**: Technique 5 (for loop_id-to-constraint mapping)

**Files to modify/create**:
- `shatter-core/src/protocol.rs` (or sym_expr location) — add `SymExpr::Ite { condition, then_expr, else_expr }` variant
- `shatter-core/src/solver.rs` — add Z3 ITE conversion (Z3's native `ite()`)
- `shatter-core/src/loop_analysis.rs` — add `merge_loop_states()`, `MergedLoopState` type
- `shatter-core/src/orchestrator.rs` — call `merge_loop_states()` after constraint extraction
- `shatter-core/src/test_arbitraries.rs` — add Ite to proptest generators

**Key idea**: For branch B inside loop L with constraints c0, c1, c2 at iterations 0, 1, 2: `merged = ITE(__loop_L_iter == 0, c0, ITE(__loop_L_iter == 1, c1, c2))`. Solver picks iteration via free variable `__loop_L_iter` bounded by `0 <= __loop_L_iter < N`.

**Risks**: Deep ITE chains may slow Z3 → cap merge depth at 10. Adding `Ite` variant to `SymExpr` is pervasive (every match arm). Sort compatibility required between then/else branches.

---

## Technique 7: Bounded Symbolic Unrolling (DEFERRED — HIGH EFFORT)

**Summary**: Build a parameterized constraint template from observed concrete traces, then symbolically unroll K times and use Z3 to find inputs (including iteration counts) that reach specific branches.

**Prerequisites**: Technique 5 (InductionVar) + Technique 6 (ITE expressions)

**Files to modify/create**:
- `shatter-core/src/symbolic_unroll.rs` (NEW) — template building, unrolling, template-to-constraint conversion
- `shatter-core/src/loop_analysis.rs` — extend with `LoopTemplate`, `extract_iteration_template()`
- `shatter-core/src/orchestrator.rs` — add as alternative solve strategy (fallback when standard solving stalls on loop branches)
- `shatter-core/src/protocol.rs` — add `LoopBodyState` for per-iteration symbolic state snapshots
- `shatter-ts/src/instrumentor.ts` — enhance `buildDataFlowMap` to track variable mutations inside loops
- `shatter-ts/src/executor.ts` — emit per-iteration symbolic state snapshots

**Key types**:
```rust
struct IterationTemplate { loop_id: u32, body_constraints: Vec<SymExpr>, state_updates: Vec<StateUpdate>, exit_condition: Option<SymExpr> }
struct StateUpdate { var_name: String, update_expr: SymExpr }
struct UnrolledFormula { loop_id: u32, unroll_depth: u32, constraints: Vec<SymExpr> }
```

**Hybrid approach**: Use concrete executions as templates (observe 2+ iterations), generalize by replacing concrete iteration values with symbolic `__k`, verify pattern holds, unroll to depth K. Triggers as fallback when standard solving stalls on loop branches.

**Risks**: Template generalization is fragile for data-dependent control flow. Frontend data flow enhancement (tracking loop body mutations) is the most invasive change. Cap unroll depth based on solver timeout.

---

## Cross-Technique Dependencies

```
Independent:  1, 2, 3, 4  (can be implemented in any order, in parallel)
Sequential:   5 → 6 → 7  (each builds on the prior)
```

## Issue Plan

- **Implement now (4 issues)**: Techniques 1, 2, 3, 4
- **Defer (3 issues)**: Techniques 5, 6, 7
