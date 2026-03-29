//! Input generation strategy traits and adaptive meta-strategy.
//!
//! # Strategy Tiers
//!
//! Strategies are organized into two tiers:
//!
//! - **Vector-level** ([`InputStrategy`]): operates on the full `Vec<Value>` input
//!   vector. All seeding strategies (user-provided, literals, boundary, pool),
//!   generation strategies (random), and solver strategies (Z3) live here.
//! - **Value-level** ([`ValueStrategy`]): operates on a single `Value` given its
//!   type. Atomic mutation building blocks (type-aware mutation, char mutation,
//!   havoc, fragment injection). See [`crate::value_strategy`].
//!
//! The [`FuzzerStrategy`] operates at **both** levels: crossover is vector-level,
//! while per-parameter mutation delegates to value-level logic via
//! [`mutate_value`](crate::input_gen::mutate_value).
//!
//! [`ValueToVectorAdapter`] bridges the two tiers, lifting any [`ValueStrategy`]
//! into an [`InputStrategy`] by applying it per-parameter.
//!
//! The [`MetaStrategy`] selects among registered vector-level strategies using
//! outcome-based adaptive scoring.
//!
//! [`ValueStrategy`]: crate::value_strategy::ValueStrategy
//! [`ValueToVectorAdapter`]: crate::value_strategy::ValueToVectorAdapter

use serde_json::Value;

// Re-export value-level strategy types for convenience.
pub use crate::value_strategy::{TypeAwareMutator, ValueStrategy, ValueToVectorAdapter};

use crate::boundary_dict::generate_boundary_inputs;
use crate::input_gen::{crossover_inputs, generate_random_inputs, literals_to_candidate_inputs, mutate_inputs};
use crate::execution_record::SymConstraint;
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{ExecuteResult, LiteralValue};
use crate::solver::{self, SolveResult};
use crate::sym_expr::SymExpr;
use crate::types::{ParamInfo, TypeInfo};

// ---------------------------------------------------------------------------
// Strategy context — shared read-only state available to all strategies
// ---------------------------------------------------------------------------

/// Read-only context passed to strategies on each `next()` call.
///
/// Contains the function's type signature, extracted literals, and frontend
/// capabilities so strategies can generate type-appropriate inputs without
/// holding references to the full analysis.
pub struct StrategyContext {
    /// Parameter names and types for the function under exploration.
    pub params: Vec<ParamInfo>,
    /// Literal constants extracted from the function body by static analysis.
    pub literals: Vec<LiteralValue>,
    /// Frontend capabilities (used to gate complex-type generation).
    pub capabilities: FrontendCapabilities,
}

impl StrategyContext {
    /// Convenience: extract just the `TypeInfo` for each parameter.
    pub fn param_types(&self) -> Vec<TypeInfo> {
        self.params.iter().map(|p| p.typ.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// StrategyTier — classification of strategy operational level
// ---------------------------------------------------------------------------

/// Classification of an input strategy by the level at which it operates.
///
/// Strategies fall into three tiers based on whether they produce/transform
/// complete input vectors, individual values, or both. This classification
/// enables the orchestrator and future GA population management (str-gx5)
/// to compose strategies at the correct level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyTier {
    /// Produces or transforms complete input vectors as atomic units.
    /// Examples: boundary seeds, pool seeding, parameter drilling, Z3 solver.
    Vector,
    /// Produces or transforms individual values within an input vector.
    /// Examples: char mutation, havoc mode, fragment injection.
    Value,
    /// Operates at both vector and value levels.
    /// Example: fuzzer with crossover (vector) + mutation (value).
    Hybrid,
}

// ---------------------------------------------------------------------------
// InputStrategy trait
// ---------------------------------------------------------------------------

/// A composable vector-level input generation strategy.
///
/// # Tier: Vector
///
/// Strategies produce candidate input vectors (`Vec<Value>`) and optionally
/// react to execution feedback. The [`MetaStrategy`] polls strategies by
/// adaptive priority and fans out feedback to all registered strategies
/// after each execution.
///
/// For value-level strategies that operate on a single `Value`, see
/// [`ValueStrategy`](crate::value_strategy::ValueStrategy). Use
/// [`ValueToVectorAdapter`](crate::value_strategy::ValueToVectorAdapter)
/// to lift a value strategy into this trait.
pub trait InputStrategy: Send {
    /// Produce the next candidate input vector, or `None` if exhausted.
    fn next(&mut self, ctx: &StrategyContext) -> Option<Vec<Value>>;

    /// Receive feedback from an execution result.
    ///
    /// Strategies that don't use feedback (literals, random, etc.) leave the
    /// default no-op implementation. Reactive strategies (Z3 solver, fuzzer)
    /// override this to record constraints or interesting values.
    fn feedback(&mut self, _inputs: &[Value], _result: &ExecuteResult, _was_new_path: bool) {}

    /// Human-readable strategy name, used for discovery attribution and config keys.
    fn name(&self) -> &str;

    /// Total number of candidates this strategy will ever produce, if known.
    ///
    /// Returns `Some(n)` for exhaustible strategies (boundary seeds, literals,
    /// pool, user-provided) and `None` for infinite strategies (random, fuzzer,
    /// Z3 solver).
    fn estimated_size(&self) -> Option<u64> {
        None
    }

    /// Whether this strategy is finite — permanently exhausted once `next()` returns `None`.
    ///
    /// Returns `true` for bounded strategies (user-provided, boundary seeds, literals, pool).
    /// Returns `false` for reactive strategies (Z3 solver, fuzzer) that produce outputs only
    /// after receiving feedback: they may return `None` now but produce inputs after a
    /// subsequent `feedback()` call. The [`MetaStrategy`] uses this to avoid permanently
    /// marking reactive strategies as exhausted when they temporarily have nothing queued.
    fn is_finite(&self) -> bool {
        true
    }

    /// The operational tier of this strategy.
    ///
    /// Used for classification, logging, and composability. Most strategies
    /// produce complete input vectors (`Vector`). Override for strategies
    /// that mutate individual values (`Value`) or do both (`Hybrid`).
    fn tier(&self) -> StrategyTier {
        StrategyTier::Vector
    }
}

// ---------------------------------------------------------------------------
// UserProvidedStrategy — yields a pre-built list of candidate inputs
// ---------------------------------------------------------------------------

/// Strategy that yields user-provided candidate inputs in order.
///
/// # Tier: Vector
///
/// Exhaustible: returns `None` once all candidates have been yielded.
/// Feedback is ignored — the input list is fixed at construction time.
pub struct UserProvidedStrategy {
    inputs: Vec<Vec<Value>>,
    index: usize,
}

impl UserProvidedStrategy {
    pub fn new(inputs: Vec<Vec<Value>>) -> Self {
        Self { inputs, index: 0 }
    }
}

impl InputStrategy for UserProvidedStrategy {
    fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
        if self.index < self.inputs.len() {
            let v = self.inputs[self.index].clone();
            self.index += 1;
            Some(v)
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "user_provided"
    }

    fn estimated_size(&self) -> Option<u64> {
        Some(self.inputs.len() as u64)
    }
}

// ---------------------------------------------------------------------------
// LiteralsStrategy — exhaustible strategy yielding literal-derived inputs
// ---------------------------------------------------------------------------

/// Yields inputs derived from literals extracted during static analysis.
///
/// # Tier: Vector
///
/// Pre-computes candidates via [`literals_to_candidate_inputs`] at construction
/// time, then yields them one at a time. Does not apply any budget cap — that
/// is the meta-strategy's responsibility.
pub struct LiteralsStrategy {
    candidates: Vec<Vec<Value>>,
    cursor: usize,
}

impl LiteralsStrategy {
    pub fn new(params: &[ParamInfo], literals: &[LiteralValue]) -> Self {
        Self {
            candidates: literals_to_candidate_inputs(params, literals),
            cursor: 0,
        }
    }
}

impl InputStrategy for LiteralsStrategy {
    fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
        if self.cursor < self.candidates.len() {
            let v = self.candidates[self.cursor].clone();
            self.cursor += 1;
            Some(v)
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "literals"
    }

    fn estimated_size(&self) -> Option<u64> {
        Some(self.candidates.len() as u64)
    }
}

// ---------------------------------------------------------------------------
// RandomStrategy — infinite strategy yielding random type-aware inputs
// ---------------------------------------------------------------------------

/// Infinite strategy that generates random inputs matching parameter types.
///
/// # Tier: Vector
///
/// Wraps [`generate_random_inputs`] with an owned RNG. Seeded for
/// reproducibility or from system entropy.
pub struct RandomStrategy {
    rng: StdRng,
}

impl RandomStrategy {
    /// Create a new random strategy. If `seed` is `Some`, the RNG is
    /// deterministic (useful for reproducible tests). If `None`, seeds
    /// from system entropy.
    pub fn new(seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_os_rng(),
        };
        Self { rng }
    }
}

impl InputStrategy for RandomStrategy {
    fn next(&mut self, ctx: &StrategyContext) -> Option<Vec<Value>> {
        Some(generate_random_inputs(
            &ctx.params,
            &mut self.rng,
            Some(&ctx.capabilities),
        ))
    }

    fn name(&self) -> &str {
        "random"
    }
}

// ---------------------------------------------------------------------------
// BoundarySeeds — exhaustible strategy yielding type-aware boundary values
// ---------------------------------------------------------------------------

/// Yields pre-computed boundary-value input vectors using pairwise coverage.
///
/// # Tier: Vector
///
/// For each parameter position, every boundary value for that parameter's type
/// is paired with neutral defaults for all other parameters. This caps the
/// candidate count at `sum(boundaries_per_type_i)` instead of the cartesian
/// product. Delegates to [`generate_boundary_inputs`] for the actual generation.
pub struct BoundarySeeds {
    candidates: Vec<Vec<Value>>,
    cursor: usize,
}

impl BoundarySeeds {
    pub fn new(params: &[ParamInfo]) -> Self {
        Self {
            candidates: generate_boundary_inputs(params),
            cursor: 0,
        }
    }
}

impl InputStrategy for BoundarySeeds {
    fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
        if self.cursor < self.candidates.len() {
            let v = self.candidates[self.cursor].clone();
            self.cursor += 1;
            Some(v)
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "boundary"
    }

    fn estimated_size(&self) -> Option<u64> {
        Some(self.candidates.len() as u64)
    }
}

// ---------------------------------------------------------------------------
// MetaConfig — controls the adaptive selection algorithm
// ---------------------------------------------------------------------------

/// Configuration for the adaptive meta-strategy.
#[derive(Debug, Clone)]
pub struct MetaConfig {
    /// Sliding window size for outcome-based scoring.
    pub window_size: usize,
    /// Minimum candidates a strategy must supply before deprioritization.
    /// Clamped to `min(cold_start_threshold, estimated_size)` per strategy.
    pub cold_start_threshold: u64,
    /// Minimum allocation fraction for any non-exhausted strategy (range 0.01–0.05).
    pub floor: f64,
    /// When true, use adaptive scoring. When false (and no static_weights),
    /// use pure round-robin.
    pub adaptive: bool,
    /// Optional static weight distribution. Keys are strategy names, values
    /// are relative weights (normalized internally). Overrides adaptive scoring.
    pub static_weights: Option<Vec<(String, f64)>>,
}

/// Sensible defaults: adaptive scoring, 100-element window, cold start of 20,
/// 2% floor, no static weights.
impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            window_size: 100,
            cold_start_threshold: 20,
            floor: 0.02,
            adaptive: true,
            static_weights: None,
        }
    }
}

// ---------------------------------------------------------------------------
// MetaStrategy — adaptive selection over registered strategies
// ---------------------------------------------------------------------------

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;

/// Per-strategy scoring state.
struct StrategyState {
    /// The strategy implementation.
    strategy: Box<dyn InputStrategy>,
    /// Sliding window of recent outcomes (true = new path discovered).
    window: VecDeque<bool>,
    /// Total candidates supplied so far.
    total_supplied: u64,
    /// Whether the strategy has been exhausted (returned None from next()).
    exhausted: bool,
}

/// Adaptive meta-strategy that selects among registered [`InputStrategy`]
/// implementations using outcome-based scoring.
///
/// Selection algorithm:
/// 1. **Static weights**: if configured, use weighted random sampling among
///    non-exhausted strategies. On exhaustion, redistribute uniformly.
/// 2. **Adaptive**: score = hit rate over sliding window. Cold-start strategies
///    (fewer than threshold candidates) get the average score of graduated
///    strategies. Floor prevents any strategy from dropping below a minimum
///    allocation.
/// 3. **Round-robin**: if adaptive is disabled and no static weights.
pub struct MetaStrategy {
    states: Vec<StrategyState>,
    config: MetaConfig,
    /// Round-robin index (used when adaptive=false and no static_weights).
    rr_index: usize,
}

impl MetaStrategy {
    pub fn new(strategies: Vec<Box<dyn InputStrategy>>, config: MetaConfig) -> Self {
        let states = strategies
            .into_iter()
            .map(|s| StrategyState {
                strategy: s,
                window: VecDeque::with_capacity(config.window_size),
                total_supplied: 0,
                exhausted: false,
            })
            .collect();
        Self {
            states,
            config,
            rr_index: 0,
        }
    }

    /// Select and invoke the next strategy, returning the candidate inputs
    /// and the index of the strategy that produced them.
    ///
    /// Returns `None` when all strategies are exhausted.
    pub fn next(&mut self, ctx: &StrategyContext, rng: &mut impl Rng) -> Option<(Vec<Value>, usize)> {
        if self.all_exhausted() {
            return None;
        }

        if self.config.static_weights.is_some() {
            self.next_static(ctx, rng)
        } else if self.config.adaptive {
            self.next_adaptive(ctx, rng)
        } else {
            self.next_round_robin(ctx)
        }
    }

    /// Fan out execution feedback to all registered strategies.
    ///
    /// Feedback is sent to all non-exhausted strategies AND to any reactive
    /// (non-finite) strategies even if they were skipped earlier — they may
    /// use feedback to queue new candidates for future `next()` calls.
    pub fn feedback(&mut self, inputs: &[Value], result: &ExecuteResult, was_new_path: bool) {
        for state in &mut self.states {
            // Always deliver feedback to reactive strategies (Z3 solver, fuzzer):
            // they need it to queue solutions even if currently empty.
            // Skip only finite strategies that are truly exhausted.
            if !state.exhausted || !state.strategy.is_finite() {
                state.strategy.feedback(inputs, result, was_new_path);
            }
        }
    }

    /// Record an outcome for a specific strategy (called after execution).
    pub fn record_outcome(&mut self, strategy_idx: usize, was_new_path: bool) {
        if let Some(state) = self.states.get_mut(strategy_idx) {
            if state.window.len() >= self.config.window_size {
                state.window.pop_front();
            }
            state.window.push_back(was_new_path);
        }
    }

    /// The name of the strategy at the given index.
    pub fn strategy_name(&self, idx: usize) -> &str {
        self.states[idx].strategy.name()
    }

    /// The operational tier of the strategy at the given index.
    pub fn strategy_tier(&self, idx: usize) -> StrategyTier {
        self.states[idx].strategy.tier()
    }

    /// Number of registered strategies.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Whether no strategies are registered.
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    fn all_exhausted(&self) -> bool {
        self.states.iter().all(|s| s.exhausted)
    }

    /// Try to get the next candidate from a specific strategy.
    ///
    /// Finite strategies (user-provided, boundary seeds, etc.) are marked exhausted
    /// when they return `None` — they will never produce more inputs. Reactive
    /// strategies (Z3 solver, fuzzer) are NOT marked exhausted; they may return
    /// `None` now but produce inputs after a subsequent `feedback()` call.
    fn try_next(&mut self, idx: usize, ctx: &StrategyContext) -> Option<Vec<Value>> {
        let state = &mut self.states[idx];
        if state.exhausted {
            return None;
        }
        match state.strategy.next(ctx) {
            Some(inputs) => {
                state.total_supplied += 1;
                Some(inputs)
            }
            None => {
                // Only permanently exhaust finite strategies. Reactive strategies
                // (Z3 solver, fuzzer) can produce inputs after future feedback calls.
                if state.strategy.is_finite() {
                    state.exhausted = true;
                }
                None
            }
        }
    }

    // --- Selection modes ---

    fn next_static(&mut self, ctx: &StrategyContext, rng: &mut impl Rng) -> Option<(Vec<Value>, usize)> {
        let weights = self.config.static_weights.clone()?;

        // Track strategies that returned None this call (temporarily empty reactive
        // strategies or exhausted finite ones) to avoid infinite loops.
        let mut skipped_this_call = std::collections::HashSet::new();
        loop {
            let mut candidates: Vec<(usize, f64)> = Vec::new();
            for (idx, state) in self.states.iter().enumerate() {
                if state.exhausted || skipped_this_call.contains(&idx) {
                    continue;
                }
                let weight = weights
                    .iter()
                    .find(|(name, _)| name == state.strategy.name())
                    .map(|(_, w)| *w)
                    .unwrap_or(1.0);
                candidates.push((idx, weight));
            }

            if candidates.is_empty() {
                return None;
            }

            let idx = weighted_select(&candidates, rng);
            if let Some(inputs) = self.try_next(idx, ctx) {
                return Some((inputs, idx));
            }
            // Strategy returned None; skip it for the rest of this call.
            skipped_this_call.insert(idx);
        }
    }

    fn next_adaptive(&mut self, ctx: &StrategyContext, rng: &mut impl Rng) -> Option<(Vec<Value>, usize)> {
        // Track strategies that returned None this call (temporarily empty reactive
        // strategies or exhausted finite ones) to avoid infinite loops.
        let mut skipped_this_call = std::collections::HashSet::new();
        loop {
            let scores = self.compute_scores();
            let candidates: Vec<(usize, f64)> = scores
                .into_iter()
                .filter(|(idx, _)| !self.states[*idx].exhausted && !skipped_this_call.contains(idx))
                .collect();

            if candidates.is_empty() {
                return None;
            }

            let idx = weighted_select(&candidates, rng);
            if let Some(inputs) = self.try_next(idx, ctx) {
                return Some((inputs, idx));
            }
            // Strategy returned None; skip it for the rest of this call.
            skipped_this_call.insert(idx);
        }
    }

    fn next_round_robin(&mut self, ctx: &StrategyContext) -> Option<(Vec<Value>, usize)> {
        let n = self.states.len();
        for _ in 0..n {
            let idx = self.rr_index % n;
            self.rr_index = self.rr_index.wrapping_add(1);
            if let Some(inputs) = self.try_next(idx, ctx) {
                return Some((inputs, idx));
            }
        }
        None
    }

    /// Compute adaptive scores for all non-exhausted strategies.
    fn compute_scores(&self) -> Vec<(usize, f64)> {
        // First pass: compute hit rates for graduated strategies to derive average.
        let mut graduated_scores: Vec<f64> = Vec::new();
        for (idx, state) in self.states.iter().enumerate() {
            if state.exhausted {
                continue;
            }
            let threshold = self.effective_cold_start(idx);
            if state.total_supplied >= threshold {
                graduated_scores.push(hit_rate(&state.window));
            }
        }
        let avg_score = if graduated_scores.is_empty() {
            1.0
        } else {
            graduated_scores.iter().sum::<f64>() / graduated_scores.len() as f64
        };

        // Second pass: assign scores.
        let mut result = Vec::new();
        for (idx, state) in self.states.iter().enumerate() {
            if state.exhausted {
                continue;
            }
            let threshold = self.effective_cold_start(idx);
            let score = if state.total_supplied < threshold {
                avg_score
            } else {
                hit_rate(&state.window)
            };
            result.push((idx, score.max(self.config.floor)));
        }
        result
    }

    /// Effective cold-start threshold for a strategy: min(configured, estimated_size).
    fn effective_cold_start(&self, idx: usize) -> u64 {
        let base = self.config.cold_start_threshold;
        match self.states[idx].strategy.estimated_size() {
            Some(size) => base.min(size),
            None => base,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hit rate: fraction of `true` values in the window. Returns 0.0 for empty windows.
fn hit_rate(window: &VecDeque<bool>) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    let hits = window.iter().filter(|&&b| b).count();
    hits as f64 / window.len() as f64
}

/// Weighted random selection. Returns the index from `candidates`.
/// Panics if `candidates` is empty.
fn weighted_select(candidates: &[(usize, f64)], rng: &mut impl Rng) -> usize {
    let total: f64 = candidates.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        // All weights zero — fall back to uniform.
        let pick = rng.random_range(0..candidates.len());
        return candidates[pick].0;
    }
    let mut roll: f64 = rng.random_range(0.0..total);
    for &(idx, weight) in candidates {
        roll -= weight;
        if roll <= 0.0 {
            return idx;
        }
    }
    // Floating-point rounding: return last candidate.
    candidates.last().unwrap().0
}

// ---------------------------------------------------------------------------
// PoolSeedsStrategy — yields cross-function pool seeds in order
// ---------------------------------------------------------------------------

/// Exhaustible strategy that yields pre-collected pool seed inputs from
/// cross-function seed sharing (`.shatter/seeds/pool.json`).
///
/// # Tier: Vector
pub struct PoolSeedsStrategy {
    seeds: Vec<Vec<Value>>,
    index: usize,
}

impl PoolSeedsStrategy {
    pub fn new(seeds: Vec<Vec<Value>>) -> Self {
        Self { seeds, index: 0 }
    }
}

impl InputStrategy for PoolSeedsStrategy {
    fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
        if self.index < self.seeds.len() {
            let v = self.seeds[self.index].clone();
            self.index += 1;
            Some(v)
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "pool"
    }

    fn estimated_size(&self) -> Option<u64> {
        Some(self.seeds.len() as u64)
    }
}

// ---------------------------------------------------------------------------
// FuzzerStrategy — infinite, feedback-driven mutation and crossover
// ---------------------------------------------------------------------------

/// Default per-parameter mutation probability for the fuzzer strategy.
const FUZZER_MUTATION_RATE: f64 = 0.3;

/// Lower mutation rate for diversity rounds (fewer parameters changed per candidate).
const FUZZER_GENTLE_RATE: f64 = 0.1;

/// Default crossover probability when two parents are available.
const FUZZER_CROSSOVER_RATE: f64 = 0.7;

/// Infinite strategy that mutates and crosses over inputs which hit Unknown
/// constraints or discovered new paths.
///
/// # Tier: Both (Vector + Value)
///
/// Crossover (`crossover_inputs`) is a vector-level operation — it recombines
/// entire input vectors. Per-parameter mutation (`mutate_inputs` →
/// `mutate_value`) is a value-level operation applied across the vector.
///
/// Feedback records interesting inputs (those reaching branches with no
/// symbolic constraint, or discovering new coverage). `next()` draws from
/// the interesting pool, generating mutations via `mutate_inputs()`
/// and `crossover_inputs()`.
pub struct FuzzerStrategy {
    rng: StdRng,
    /// Inputs that hit Unknown constraints or new paths — seeds for mutation.
    interesting: Vec<Vec<Value>>,
    /// Pending mutations generated from a feedback round, drained by `next()`.
    pending: VecDeque<Vec<Value>>,
}

impl FuzzerStrategy {
    pub fn new(seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_os_rng(),
        };
        Self {
            rng,
            interesting: Vec::new(),
            pending: VecDeque::new(),
        }
    }

    /// Refill `self.pending` by mutating a random interesting input.
    fn refill(&mut self, ctx: &StrategyContext) {
        if self.interesting.is_empty() {
            return;
        }

        let idx = self.rng.random_range(0..self.interesting.len());
        let base = self.interesting[idx].clone();

        let params: Vec<ParamInfo> = ctx.params.clone();

        // 1. Gentle type-aware mutation for diversity.
        let gentle_mutated = mutate_inputs(&base, &params, FUZZER_GENTLE_RATE, &[], &mut self.rng);
        self.pending.push_back(gentle_mutated);

        // 2. Aggressive type-aware mutation via input_gen.
        let mutated = mutate_inputs(&base, &params, FUZZER_MUTATION_RATE, &[], &mut self.rng);
        self.pending.push_back(mutated);

        // 3. Crossover when at least two interesting inputs exist.
        if self.interesting.len() >= 2 {
            let other_idx = loop {
                let candidate = self.rng.random_range(0..self.interesting.len());
                if candidate != idx || self.interesting.len() == 1 {
                    break candidate;
                }
            };
            let (child_a, _child_b) = crossover_inputs(
                &base,
                &self.interesting[other_idx],
                &params,
                FUZZER_CROSSOVER_RATE,
                &mut self.rng,
            );
            self.pending.push_back(child_a);
        }
    }
}

impl InputStrategy for FuzzerStrategy {
    fn next(&mut self, ctx: &StrategyContext) -> Option<Vec<Value>> {
        if let Some(candidate) = self.pending.pop_front() {
            return Some(candidate);
        }

        // Try to refill from interesting pool.
        self.refill(ctx);
        self.pending.pop_front()
    }

    fn feedback(&mut self, inputs: &[Value], result: &ExecuteResult, was_new_path: bool) {
        // Record inputs that discovered new coverage.
        if was_new_path {
            self.interesting.push(inputs.to_vec());
            return;
        }

        // Record inputs that hit Unknown constraints (branches the solver
        // cannot handle, so fuzzing is the only way to explore them).
        let has_unknown = result.branch_path.iter().any(|decision| {
            matches!(decision.constraint, SymConstraint::Unknown { .. })
        });
        if has_unknown {
            self.interesting.push(inputs.to_vec());
        }
    }

    fn name(&self) -> &str {
        "fuzzer"
    }

    fn is_finite(&self) -> bool {
        false
    }

    fn tier(&self) -> StrategyTier {
        StrategyTier::Hybrid
    }
}

// ---------------------------------------------------------------------------
// Z3SolverStrategy — infinite, feedback-driven constraint solving
// ---------------------------------------------------------------------------

/// Strategy name constant for Z3 solver, used for discovery attribution.
const Z3_SOLVER_STRATEGY_NAME: &str = "z3_solver";

/// Reactive strategy that uses Z3 constraint solving to generate inputs
/// targeting unexplored branches.
///
/// # Tier: Vector
///
/// `feedback()` extracts symbolic constraints from execution results, negates
/// each solvable constraint, solves with Z3, and overlays solutions onto the
/// base inputs. Solved inputs queue in `pending` and drain via `next()`.
///
/// Infinite: produces work while unsolved constraints exist, but the queue
/// may be empty between feedback cycles.
pub struct Z3SolverStrategy {
    solver_timeout_ms: Option<u64>,
    param_infos: Vec<ParamInfo>,
    pending: VecDeque<Vec<Value>>,
    /// Canonical loop metadata from static analysis, used to collapse backedge constraints.
    loops: Vec<crate::protocol::LoopInfo>,
}

impl Z3SolverStrategy {
    pub fn new(
        solver_timeout_ms: Option<u64>,
        param_infos: Vec<ParamInfo>,
        loops: Vec<crate::protocol::LoopInfo>,
    ) -> Self {
        Self {
            solver_timeout_ms,
            param_infos,
            pending: VecDeque::new(),
            loops,
        }
    }
}

impl InputStrategy for Z3SolverStrategy {
    fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
        self.pending.pop_front()
    }

    fn feedback(&mut self, inputs: &[Value], result: &ExecuteResult, _was_new_path: bool) {
        let sym_constraints = crate::orchestrator::extract_sym_constraints(result);

        // Technique 5: collapse O(k) backedge constraints into O(1) for canonical counted loops.
        let rewritten =
            crate::loop_analysis::rewrite_loop_constraints(&sym_constraints, &self.loops, result);

        // Technique 6: merge remaining per-iteration constraints into ITE chains.
        let rewritten =
            crate::loop_analysis::merge_loop_states(&rewritten, &self.loops, result);

        let solvable: Vec<SymExpr> = rewritten
            .iter()
            .filter_map(|c| c.clone())
            .collect();

        if solvable.is_empty() {
            return;
        }

        let param_names: Vec<String> = self.param_infos.iter().map(|p| p.name.clone()).collect();

        for solve_idx in 0..solvable.len() {
            // solve_for_new_path may fail (unsupported expressions, type mismatches,
            // or constraint/param misalignment). Treat all failures as "no solution".
            let solve_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                solver::solve_for_new_path(
                    &solvable,
                    solve_idx,
                    self.solver_timeout_ms,
                    &self.param_infos,
                )
            }));

            match solve_result {
                Ok(Ok(SolveResult::Sat(values))) => {
                    let new_inputs = crate::orchestrator::overlay_solved_values(
                        inputs,
                        &values,
                        &param_names,
                    );
                    self.pending.push_back(new_inputs);
                }
                _ => {
                    // Unsat, solver error, or panic (debug_assert on param mismatch).
                    // Stall tracking is the orchestrator's responsibility.
                }
            }
        }
    }

    fn name(&self) -> &str {
        Z3_SOLVER_STRATEGY_NAME
    }

    fn is_finite(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeInfo;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Minimal strategy that yields a fixed sequence of inputs.
    struct FixedStrategy {
        name: &'static str,
        items: Vec<Vec<Value>>,
        idx: usize,
    }

    impl FixedStrategy {
        fn new(name: &'static str, items: Vec<Vec<Value>>) -> Self {
            Self { name, items, idx: 0 }
        }
    }

    impl InputStrategy for FixedStrategy {
        fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
            if self.idx < self.items.len() {
                let v = self.items[self.idx].clone();
                self.idx += 1;
                Some(v)
            } else {
                None
            }
        }

        fn name(&self) -> &str {
            self.name
        }

        fn estimated_size(&self) -> Option<u64> {
            Some(self.items.len() as u64)
        }
    }

    /// Infinite strategy that always yields the same input.
    struct InfiniteStrategy {
        value: Vec<Value>,
    }

    impl InputStrategy for InfiniteStrategy {
        fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
            Some(self.value.clone())
        }

        fn name(&self) -> &str {
            "infinite"
        }
    }

    fn empty_ctx() -> StrategyContext {
        StrategyContext {
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        }
    }

    fn make_exec_result() -> ExecuteResult {
        serde_json::from_str(
            r#"{"return_value": 0, "branch_path": [], "lines_executed": [], "path_constraints": [], "performance": {"wall_time_ms": 1.0, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0}}"#,
        )
        .unwrap()
    }

    #[test]
    fn exhaustible_strategy_returns_none_when_done() {
        let mut s = FixedStrategy::new("test", vec![vec![Value::from(1)]]);
        let ctx = empty_ctx();
        assert!(s.next(&ctx).is_some());
        assert!(s.next(&ctx).is_none());
    }

    #[test]
    fn meta_round_robin_interleaves_strategies() {
        let a = FixedStrategy::new("a", vec![vec![Value::from(1)], vec![Value::from(2)]]);
        let b = FixedStrategy::new("b", vec![vec![Value::from(10)], vec![Value::from(20)]]);
        let config = MetaConfig {
            adaptive: false,
            static_weights: None,
            ..MetaConfig::default()
        };
        let mut meta = MetaStrategy::new(
            vec![Box::new(a), Box::new(b)],
            config,
        );
        let ctx = empty_ctx();
        let mut rng = StdRng::seed_from_u64(42);

        let (v1, idx1) = meta.next(&ctx, &mut rng).unwrap();
        let (v2, idx2) = meta.next(&ctx, &mut rng).unwrap();
        // Round-robin: alternates between strategies.
        assert_ne!(idx1, idx2);
        assert_ne!(v1, v2);
    }

    #[test]
    fn meta_skips_exhausted_strategies() {
        let short = FixedStrategy::new("short", vec![vec![Value::from(1)]]);
        let long = FixedStrategy::new("long", vec![
            vec![Value::from(10)],
            vec![Value::from(20)],
            vec![Value::from(30)],
        ]);
        let config = MetaConfig {
            adaptive: false,
            static_weights: None,
            ..MetaConfig::default()
        };
        let mut meta = MetaStrategy::new(
            vec![Box::new(short), Box::new(long)],
            config,
        );
        let ctx = empty_ctx();
        let mut rng = StdRng::seed_from_u64(42);

        let mut results = Vec::new();
        while let Some((v, idx)) = meta.next(&ctx, &mut rng) {
            results.push((v, idx));
        }
        // 1 from short + 3 from long = 4 total.
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn meta_returns_none_when_all_exhausted() {
        let s = FixedStrategy::new("only", vec![vec![Value::from(1)]]);
        let config = MetaConfig {
            adaptive: false,
            static_weights: None,
            ..MetaConfig::default()
        };
        let mut meta = MetaStrategy::new(vec![Box::new(s)], config);
        let ctx = empty_ctx();
        let mut rng = StdRng::seed_from_u64(42);

        assert!(meta.next(&ctx, &mut rng).is_some());
        assert!(meta.next(&ctx, &mut rng).is_none());
    }

    #[test]
    fn adaptive_cold_start_gives_fair_share() {
        // Two strategies: one graduated (10 hits out of 20), one in cold start.
        let infinite = InfiniteStrategy {
            value: vec![Value::from(0)],
        };
        let fresh = InfiniteStrategy {
            value: vec![Value::from(1)],
        };
        let config = MetaConfig::default();
        let mut meta = MetaStrategy::new(
            vec![Box::new(infinite), Box::new(fresh)],
            config,
        );

        // Simulate graduated state for strategy 0.
        for i in 0..20 {
            meta.states[0].total_supplied += 1;
            meta.record_outcome(0, i < 10); // 50% hit rate
        }

        let scores = meta.compute_scores();
        // Strategy 0 graduated with 50% hit rate.
        assert!((scores[0].1 - 0.5).abs() < 0.01);
        // Strategy 1 in cold start gets average of graduated = 0.5.
        assert!((scores[1].1 - 0.5).abs() < 0.01);
    }

    #[test]
    fn floor_prevents_zero_allocation() {
        let config = MetaConfig {
            floor: 0.05,
            ..MetaConfig::default()
        };
        let a = InfiniteStrategy {
            value: vec![Value::from(0)],
        };
        let mut meta = MetaStrategy::new(vec![Box::new(a)], config);

        // Graduate with 0% hit rate.
        for _ in 0..25 {
            meta.states[0].total_supplied += 1;
            meta.record_outcome(0, false);
        }

        let scores = meta.compute_scores();
        // Score should be clamped to floor (0.05), not 0.0.
        assert!((scores[0].1 - 0.05).abs() < 0.001);
    }

    #[test]
    fn effective_cold_start_clamped_to_estimated_size() {
        let small = FixedStrategy::new("small", vec![vec![Value::from(1)], vec![Value::from(2)]]);
        let config = MetaConfig {
            cold_start_threshold: 20,
            ..MetaConfig::default()
        };
        let meta = MetaStrategy::new(vec![Box::new(small)], config);
        // estimated_size = 2, threshold = 20, effective = min(20, 2) = 2.
        assert_eq!(meta.effective_cold_start(0), 2);
    }

    #[test]
    fn feedback_fans_out_to_all_strategies() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        struct CountingStrategy {
            count: Arc<AtomicU32>,
        }

        impl InputStrategy for CountingStrategy {
            fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<Value>> {
                Some(vec![Value::from(0)])
            }

            fn feedback(&mut self, _inputs: &[Value], _result: &ExecuteResult, _was_new_path: bool) {
                self.count.fetch_add(1, Ordering::Relaxed);
            }

            fn name(&self) -> &str {
                "counting"
            }
        }

        let c1 = Arc::new(AtomicU32::new(0));
        let c2 = Arc::new(AtomicU32::new(0));

        let s1 = CountingStrategy { count: c1.clone() };
        let s2 = CountingStrategy { count: c2.clone() };

        let mut meta = MetaStrategy::new(
            vec![Box::new(s1), Box::new(s2)],
            MetaConfig::default(),
        );

        let result = make_exec_result();
        meta.feedback(&[Value::from(0)], &result, true);

        assert_eq!(c1.load(Ordering::Relaxed), 1);
        assert_eq!(c2.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn static_weights_selection() {
        let a = FixedStrategy::new("a", vec![
            vec![Value::from(1)], vec![Value::from(2)], vec![Value::from(3)],
            vec![Value::from(4)], vec![Value::from(5)], vec![Value::from(6)],
        ]);
        let b = FixedStrategy::new("b", vec![
            vec![Value::from(10)], vec![Value::from(20)], vec![Value::from(30)],
            vec![Value::from(40)], vec![Value::from(50)], vec![Value::from(60)],
        ]);
        let config = MetaConfig {
            static_weights: Some(vec![("a".into(), 1.0), ("b".into(), 1.0)]),
            ..MetaConfig::default()
        };
        let mut meta = MetaStrategy::new(
            vec![Box::new(a), Box::new(b)],
            config,
        );
        let ctx = empty_ctx();
        let mut rng = StdRng::seed_from_u64(42);

        // With equal weights and enough samples, both strategies should be used.
        let mut a_count = 0;
        let mut b_count = 0;
        while let Some((_, idx)) = meta.next(&ctx, &mut rng) {
            if idx == 0 {
                a_count += 1;
            } else {
                b_count += 1;
            }
        }
        assert!(a_count > 0, "strategy a should have been selected");
        assert!(b_count > 0, "strategy b should have been selected");
        assert_eq!(a_count + b_count, 12);
    }

    #[test]
    fn weighted_select_handles_zero_weights() {
        let mut rng = StdRng::seed_from_u64(1);
        let candidates = vec![(0, 0.0), (1, 0.0)];
        // Should not panic — falls back to uniform.
        let _ = weighted_select(&candidates, &mut rng);
    }

    #[test]
    fn hit_rate_empty_window() {
        let window = VecDeque::new();
        assert_eq!(hit_rate(&window), 0.0);
    }

    #[test]
    fn hit_rate_all_hits() {
        let window: VecDeque<bool> = vec![true, true, true].into();
        assert!((hit_rate(&window) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hit_rate_mixed() {
        let window: VecDeque<bool> = vec![true, false, true, false].into();
        assert!((hit_rate(&window) - 0.5).abs() < f64::EPSILON);
    }

    // --- BoundarySeeds tests ---

    use crate::boundary_dict::get_boundary_values;

    fn make_params(types: &[TypeInfo]) -> Vec<ParamInfo> {
        types
            .iter()
            .enumerate()
            .map(|(i, t)| ParamInfo {
                name: format!("p{i}"),
                typ: t.clone(),
                type_name: None,
            })
            .collect()
    }

    #[test]
    fn boundary_seeds_single_int_param() {
        let params = make_params(&[TypeInfo::Int]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        let expected = get_boundary_values(&TypeInfo::Int).len();
        assert_eq!(bs.estimated_size(), Some(expected as u64));

        let mut count = 0;
        while bs.next(&ctx).is_some() {
            count += 1;
        }
        assert_eq!(count, expected);
        // Exhausted — next returns None.
        assert!(bs.next(&ctx).is_none());
    }

    #[test]
    fn boundary_seeds_single_string_param() {
        let params = make_params(&[TypeInfo::Str]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        let expected = get_boundary_values(&TypeInfo::Str).len();
        assert_eq!(bs.estimated_size(), Some(expected as u64));

        let mut count = 0;
        while bs.next(&ctx).is_some() {
            count += 1;
        }
        assert_eq!(count, expected);
    }

    #[test]
    fn boundary_seeds_multi_param_pairwise() {
        let params = make_params(&[TypeInfo::Int, TypeInfo::Str]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        let expected =
            get_boundary_values(&TypeInfo::Int).len() + get_boundary_values(&TypeInfo::Str).len();
        assert_eq!(bs.estimated_size(), Some(expected as u64));

        let mut count = 0;
        while let Some(v) = bs.next(&ctx) {
            assert_eq!(v.len(), 2, "each candidate should have 2 elements");
            count += 1;
        }
        assert_eq!(count, expected);
    }

    #[test]
    fn boundary_seeds_bool_exhausts_after_two() {
        let params = make_params(&[TypeInfo::Bool]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        assert_eq!(bs.estimated_size(), Some(2));
        assert!(bs.next(&ctx).is_some());
        assert!(bs.next(&ctx).is_some());
        assert!(bs.next(&ctx).is_none());
    }

    #[test]
    fn boundary_seeds_empty_params() {
        let params: Vec<ParamInfo> = vec![];
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        assert_eq!(bs.estimated_size(), Some(0));
        assert!(bs.next(&ctx).is_none());
    }

    #[test]
    fn boundary_seeds_unknown_type_yields_nothing() {
        let params = make_params(&[TypeInfo::Unknown]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        assert_eq!(bs.estimated_size(), Some(0));
        assert!(bs.next(&ctx).is_none());
    }

    #[test]
    fn boundary_seeds_name() {
        let bs = BoundarySeeds::new(&[]);
        assert_eq!(bs.name(), "boundary");
    }

    #[test]
    fn boundary_seeds_estimated_size_matches_drain_count() {
        let params = make_params(&[TypeInfo::Float, TypeInfo::Bool]);
        let mut bs = BoundarySeeds::new(&params);
        let ctx = empty_ctx();
        let est = bs.estimated_size().unwrap();
        let mut count = 0u64;
        while bs.next(&ctx).is_some() {
            count += 1;
        }
        assert_eq!(count, est);
    }

    // --- UserProvidedStrategy tests ---

    #[test]
    fn user_provided_preserves_ordering() {
        let inputs = vec![
            vec![Value::from(1)],
            vec![Value::from(2)],
            vec![Value::from(3)],
        ];
        let mut s = UserProvidedStrategy::new(inputs.clone());
        let ctx = empty_ctx();

        for expected in &inputs {
            assert_eq!(s.next(&ctx).as_ref(), Some(expected));
        }
    }

    #[test]
    fn user_provided_exhausts_then_stays_none() {
        let mut s = UserProvidedStrategy::new(vec![vec![Value::from(42)]]);
        let ctx = empty_ctx();

        assert!(s.next(&ctx).is_some());
        assert!(s.next(&ctx).is_none());
        assert!(s.next(&ctx).is_none()); // stays None
    }

    #[test]
    fn user_provided_empty_returns_none_immediately() {
        let mut s = UserProvidedStrategy::new(vec![]);
        let ctx = empty_ctx();
        assert!(s.next(&ctx).is_none());
    }

    #[test]
    fn user_provided_estimated_size() {
        let s = UserProvidedStrategy::new(vec![vec![Value::from(1)], vec![Value::from(2)]]);
        assert_eq!(s.estimated_size(), Some(2));
        assert_eq!(s.name(), "user_provided");
    }

    // --- PoolSeedsStrategy tests ---

    #[test]
    fn pool_seeds_yields_in_order() {
        let seeds = vec![
            vec![Value::from(10), Value::from("a")],
            vec![Value::from(20), Value::from("b")],
            vec![Value::from(30), Value::from("c")],
        ];
        let mut strategy = PoolSeedsStrategy::new(seeds.clone());
        let ctx = empty_ctx();

        for expected in &seeds {
            let got = strategy.next(&ctx).expect("should yield a value");
            assert_eq!(&got, expected);
        }
    }

    #[test]
    fn pool_seeds_exhausts() {
        let seeds = vec![vec![Value::from(1)], vec![Value::from(2)]];
        let mut strategy = PoolSeedsStrategy::new(seeds);
        let ctx = empty_ctx();

        assert_eq!(strategy.estimated_size(), Some(2));
        assert_eq!(strategy.name(), "pool");

        assert!(strategy.next(&ctx).is_some());
        assert!(strategy.next(&ctx).is_some());
        assert!(strategy.next(&ctx).is_none());
        // Stays exhausted on subsequent calls.
        assert!(strategy.next(&ctx).is_none());
    }

    #[test]
    fn pool_seeds_empty() {
        let mut strategy = PoolSeedsStrategy::new(vec![]);
        let ctx = empty_ctx();

        assert_eq!(strategy.estimated_size(), Some(0));
        assert!(strategy.next(&ctx).is_none());
    }

    // --- LiteralsStrategy tests ---

    use crate::input_gen::literals_to_candidate_inputs;
    use crate::protocol::LiteralValue;

    #[test]
    fn literals_strategy_yields_expected_candidates() {
        let params = make_params(&[TypeInfo::Str]);
        let literals = vec![
            LiteralValue::Str { value: "hello".into() },
            LiteralValue::Str { value: "world".into() },
        ];
        let expected = literals_to_candidate_inputs(&params, &literals);
        let mut strat = LiteralsStrategy::new(&params, &literals);
        let ctx = empty_ctx();

        let mut got = Vec::new();
        while let Some(v) = strat.next(&ctx) {
            got.push(v);
        }
        assert_eq!(got, expected);
    }

    #[test]
    fn literals_strategy_exhausts_then_none() {
        let params = make_params(&[TypeInfo::Int]);
        let literals = vec![LiteralValue::Int { value: 42 }];
        let mut strat = LiteralsStrategy::new(&params, &literals);
        let ctx = empty_ctx();

        assert!(strat.next(&ctx).is_some());
        assert!(strat.next(&ctx).is_none());
        assert!(strat.next(&ctx).is_none());
    }

    #[test]
    fn literals_strategy_empty_literals() {
        let params = make_params(&[TypeInfo::Int]);
        let mut strat = LiteralsStrategy::new(&params, &[]);
        let ctx = empty_ctx();
        assert_eq!(strat.estimated_size(), Some(0));
        assert!(strat.next(&ctx).is_none());
    }

    #[test]
    fn literals_strategy_empty_params() {
        let literals = vec![LiteralValue::Str { value: "test".into() }];
        let mut strat = LiteralsStrategy::new(&[], &literals);
        let ctx = empty_ctx();
        assert_eq!(strat.estimated_size(), Some(0));
        assert!(strat.next(&ctx).is_none());
    }

    #[test]
    fn literals_strategy_name() {
        let strat = LiteralsStrategy::new(&[], &[]);
        assert_eq!(strat.name(), "literals");
    }

    #[test]
    fn literals_strategy_estimated_size_matches_drain() {
        let params = make_params(&[TypeInfo::Int, TypeInfo::Str]);
        let literals = vec![
            LiteralValue::Int { value: 1 },
            LiteralValue::Int { value: 2 },
            LiteralValue::Str { value: "abc".into() },
        ];
        let mut strat = LiteralsStrategy::new(&params, &literals);
        let ctx = empty_ctx();
        let est = strat.estimated_size().unwrap();
        let mut count = 0u64;
        while strat.next(&ctx).is_some() {
            count += 1;
        }
        assert_eq!(count, est);
    }

    // --- RandomStrategy tests ---

    #[test]
    fn random_strategy_never_exhausts() {
        let mut s = RandomStrategy::new(Some(42));
        let ctx = empty_ctx();
        for _ in 0..100 {
            assert!(s.next(&ctx).is_some(), "random strategy should never return None");
        }
    }

    #[test]
    fn random_strategy_produces_type_appropriate_values() {
        let mut s = RandomStrategy::new(Some(99));

        let int_ctx = StrategyContext {
            params: vec![ParamInfo {
                name: "n".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        };
        for _ in 0..20 {
            let vals = s.next(&int_ctx).unwrap();
            assert_eq!(vals.len(), 1);
            assert!(vals[0].is_number(), "Int param should produce a number, got {:?}", vals[0]);
        }

        let str_ctx = StrategyContext {
            params: vec![ParamInfo {
                name: "s".into(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        };
        for _ in 0..20 {
            let vals = s.next(&str_ctx).unwrap();
            assert_eq!(vals.len(), 1);
            assert!(vals[0].is_string(), "Str param should produce a string, got {:?}", vals[0]);
        }

        let bool_ctx = StrategyContext {
            params: vec![ParamInfo {
                name: "b".into(),
                typ: TypeInfo::Bool,
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        };
        for _ in 0..20 {
            let vals = s.next(&bool_ctx).unwrap();
            assert_eq!(vals.len(), 1);
            assert!(vals[0].is_boolean(), "Bool param should produce a boolean, got {:?}", vals[0]);
        }
    }

    #[test]
    fn random_strategy_name() {
        let s = RandomStrategy::new(Some(0));
        assert_eq!(s.name(), "random");
    }

    #[test]
    fn random_strategy_estimated_size_is_none() {
        let s = RandomStrategy::new(Some(0));
        assert_eq!(s.estimated_size(), None);
    }

    #[test]
    fn random_strategy_deterministic_with_seed() {
        let ctx = empty_ctx();
        let mut s1 = RandomStrategy::new(Some(123));
        let mut s2 = RandomStrategy::new(Some(123));
        for _ in 0..50 {
            assert_eq!(s1.next(&ctx), s2.next(&ctx));
        }
    }

    // -----------------------------------------------------------------------
    // FuzzerStrategy tests
    // -----------------------------------------------------------------------

    fn make_exec_result_with_unknown() -> ExecuteResult {
        serde_json::from_value(serde_json::json!({
            "return_value": 0,
            "branch_path": [{
                "branch_id": 1,
                "line": 10,
                "taken": true,
                "constraint": { "kind": "unknown", "hint": "opaque call" }
            }],
            "lines_executed": [10],
            "path_constraints": [],
            "performance": {
                "wall_time_ms": 1.0,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0
            }
        }))
        .expect("valid ExecuteResult JSON")
    }

    #[test]
    fn fuzzer_returns_none_without_feedback() {
        let mut fuzzer = FuzzerStrategy::new(Some(42));
        let ctx = empty_ctx();
        assert!(fuzzer.next(&ctx).is_none());
        assert!(fuzzer.interesting.is_empty());
    }

    #[test]
    fn fuzzer_produces_mutations_after_unknown_feedback() {
        let mut fuzzer = FuzzerStrategy::new(Some(42));
        let ctx = empty_ctx();
        let inputs = vec![Value::from(5)];
        let result = make_exec_result_with_unknown();

        fuzzer.feedback(&inputs, &result, false);
        assert_eq!(fuzzer.interesting.len(), 1);

        // Should now produce mutations from the interesting input.
        let first = fuzzer.next(&ctx);
        assert!(first.is_some());
        // Should keep producing (infinite strategy).
        let second = fuzzer.next(&ctx);
        assert!(second.is_some());
    }

    #[test]
    fn fuzzer_records_new_path_inputs() {
        let mut fuzzer = FuzzerStrategy::new(Some(42));
        let inputs = vec![Value::from(10)];
        let result = make_exec_result();

        // was_new_path = true should record even without Unknown constraints.
        fuzzer.feedback(&inputs, &result, true);
        assert_eq!(fuzzer.interesting.len(), 1);
        assert_eq!(fuzzer.interesting[0], inputs);
    }

    #[test]
    fn fuzzer_estimated_size_is_none() {
        let fuzzer = FuzzerStrategy::new(Some(42));
        assert!(fuzzer.estimated_size().is_none());
    }

    #[test]
    fn fuzzer_name_is_fuzzer() {
        let fuzzer = FuzzerStrategy::new(Some(42));
        assert_eq!(fuzzer.name(), "fuzzer");
    }

    #[test]
    fn fuzzer_crossover_with_two_seeds() {
        let mut fuzzer = FuzzerStrategy::new(Some(42));
        let ctx = empty_ctx();
        let result_unknown = make_exec_result_with_unknown();

        // Feed two different inputs.
        fuzzer.feedback(&[Value::from(1)], &result_unknown, false);
        fuzzer.feedback(&[Value::from(100)], &result_unknown, false);
        assert_eq!(fuzzer.interesting.len(), 2);

        // Drain enough to verify crossover candidates are generated.
        let mut seen = Vec::new();
        for _ in 0..20 {
            if let Some(v) = fuzzer.next(&ctx) {
                seen.push(v);
            }
        }
        // Should have generated multiple distinct candidates.
        assert!(seen.len() >= 2);
    }

    // -----------------------------------------------------------------------
    // Z3SolverStrategy tests
    // -----------------------------------------------------------------------

    fn make_z3_solver(params: Vec<ParamInfo>) -> Z3SolverStrategy {
        Z3SolverStrategy::new(Some(1000), params, vec![])
    }

    fn int_param(name: &str) -> ParamInfo {
        ParamInfo {
            name: name.into(),
            typ: TypeInfo::Int,
            type_name: None,
        }
    }

    #[test]
    fn z3_solver_next_returns_none_without_feedback() {
        let mut s = make_z3_solver(vec![int_param("x")]);
        assert!(s.next(&empty_ctx()).is_none());
    }

    #[test]
    fn z3_solver_name() {
        let s = make_z3_solver(vec![]);
        assert_eq!(s.name(), Z3_SOLVER_STRATEGY_NAME);
    }

    #[test]
    fn z3_solver_is_infinite() {
        let s = make_z3_solver(vec![]);
        assert!(s.estimated_size().is_none());
    }

    #[test]
    fn z3_solver_empty_branch_path_yields_nothing() {
        let mut s = make_z3_solver(vec![int_param("x")]);
        let result = make_exec_result(); // empty branch_path
        s.feedback(&[Value::from(0)], &result, false);
        assert!(s.next(&empty_ctx()).is_none());
    }

    #[test]
    fn z3_solver_unknown_constraints_yield_nothing() {
        let mut s = make_z3_solver(vec![int_param("x")]);
        let result = make_exec_result_with_unknown();
        s.feedback(&[Value::from(0)], &result, false);
        assert!(s.next(&empty_ctx()).is_none());
    }

    #[test]
    fn z3_solver_solvable_constraint_queues_input() {
        use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

        let mut s = make_z3_solver(vec![int_param("x")]);

        // Constraint: x == 5 (taken=true). Solver negates to x != 5 → SAT.
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
        };
        let result: ExecuteResult = serde_json::from_value(serde_json::json!({
            "return_value": 0,
            "branch_path": [{
                "branch_id": 1,
                "line": 10,
                "taken": true,
                "constraint": { "kind": "expr", "expr": constraint }
            }],
            "lines_executed": [10],
            "path_constraints": [],
            "performance": {
                "wall_time_ms": 1.0,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0
            }
        }))
        .expect("valid ExecuteResult JSON");

        s.feedback(&[Value::from(5)], &result, false);

        let solved = s.next(&empty_ctx());
        assert!(solved.is_some(), "Z3 should produce a solved input for x != 5");
        let solved = solved.unwrap();
        assert_eq!(solved.len(), 1, "output must preserve input vector length");
        // The solved value should differ from 5.
        assert_ne!(solved[0], Value::from(5));
    }

    #[test]
    fn z3_solver_output_preserves_input_length() {
        use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

        let params = vec![int_param("a"), int_param("b")];
        let mut s = make_z3_solver(params);

        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "a".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };
        let result: ExecuteResult = serde_json::from_value(serde_json::json!({
            "return_value": 0,
            "branch_path": [{
                "branch_id": 1,
                "line": 5,
                "taken": true,
                "constraint": { "kind": "expr", "expr": constraint }
            }],
            "lines_executed": [5],
            "path_constraints": [],
            "performance": {
                "wall_time_ms": 1.0,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0
            }
        }))
        .expect("valid ExecuteResult JSON");

        s.feedback(&[Value::from(10), Value::from(20)], &result, false);

        while let Some(output) = s.next(&empty_ctx()) {
            assert_eq!(output.len(), 2, "output must preserve input vector length");
        }
    }

    #[test]
    fn z3_solver_multiple_feedbacks_accumulate() {
        use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

        let mut s = make_z3_solver(vec![int_param("x")]);

        let make_result = |val: i64| -> ExecuteResult {
            let constraint = SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(val))),
            };
            serde_json::from_value(serde_json::json!({
                "return_value": 0,
                "branch_path": [{
                    "branch_id": 1,
                    "line": 10,
                    "taken": true,
                    "constraint": { "kind": "expr", "expr": constraint }
                }],
                "lines_executed": [10],
                "path_constraints": [],
                "performance": {
                    "wall_time_ms": 1.0,
                    "cpu_time_us": 0,
                    "heap_used_bytes": 0,
                    "heap_allocated_bytes": 0
                }
            }))
            .expect("valid ExecuteResult JSON")
        };

        s.feedback(&[Value::from(5)], &make_result(5), false);
        s.feedback(&[Value::from(10)], &make_result(10), true);

        // Should have accumulated solved inputs from both feedback calls.
        let mut count = 0;
        while s.next(&empty_ctx()).is_some() {
            count += 1;
        }
        assert!(count >= 2, "expected at least 2 solved inputs, got {count}");
    }

    mod z3_solver_proptests {
        use super::*;
        use crate::test_arbitraries::arb_execute_result;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(30))]

            /// Feeding arbitrary ExecuteResults to Z3SolverStrategy must never panic.
            #[test]
            fn feedback_never_panics(
                er in arb_execute_result(),
                len in 1..4usize,
            ) {
                let params: Vec<ParamInfo> = (0..len)
                    .map(|i| ParamInfo {
                        name: format!("p{i}"),
                        typ: TypeInfo::Int,
                        type_name: None,
                    })
                    .collect();
                let inputs: Vec<Value> = (0..len).map(|i| Value::from(i as i64)).collect();
                let mut s = Z3SolverStrategy::new(Some(500), params, vec![]);
                // Must not panic.
                s.feedback(&inputs, &er, false);
            }

            /// Any solved inputs produced must have the same length as the input vector.
            #[test]
            fn output_preserves_length(
                er in arb_execute_result(),
                len in 1..4usize,
            ) {
                let params: Vec<ParamInfo> = (0..len)
                    .map(|i| ParamInfo {
                        name: format!("p{i}"),
                        typ: TypeInfo::Int,
                        type_name: None,
                    })
                    .collect();
                let inputs: Vec<Value> = (0..len).map(|i| Value::from(i as i64)).collect();
                let ctx = StrategyContext {
                    params: params.clone(),
                    literals: vec![],
                    capabilities: FrontendCapabilities::from_raw(&[]),
                };
                let mut s = Z3SolverStrategy::new(Some(500), params, vec![]);
                s.feedback(&inputs, &er, false);
                while let Some(output) = s.next(&ctx) {
                    prop_assert_eq!(output.len(), len);
                }
            }
        }

    // --- StrategyTier classification tests ---

    #[test]
    fn strategy_tier_classification() {
        let user = UserProvidedStrategy::new(vec![]);
        assert_eq!(user.tier(), StrategyTier::Vector);

        let literals = LiteralsStrategy::new(&[], &[]);
        assert_eq!(literals.tier(), StrategyTier::Vector);

        let random = RandomStrategy::new(Some(42));
        assert_eq!(random.tier(), StrategyTier::Vector);

        let boundary = BoundarySeeds::new(&[]);
        assert_eq!(boundary.tier(), StrategyTier::Vector);

        let pool = PoolSeedsStrategy::new(vec![]);
        assert_eq!(pool.tier(), StrategyTier::Vector);

        let fuzzer = FuzzerStrategy::new(Some(42));
        assert_eq!(fuzzer.tier(), StrategyTier::Hybrid);

        let z3 = Z3SolverStrategy::new(None, vec![], vec![]);
        assert_eq!(z3.tier(), StrategyTier::Vector);
    }

    #[test]
    fn default_tier_is_vector() {
        let s = FixedStrategy::new("test", vec![]);
        assert_eq!(s.tier(), StrategyTier::Vector);
    }

    #[test]
    fn meta_strategy_tier_query() {
        let user = UserProvidedStrategy::new(vec![vec![Value::from(1)]]);
        let fuzzer = FuzzerStrategy::new(Some(42));
        let meta = MetaStrategy::new(
            vec![Box::new(user), Box::new(fuzzer)],
            MetaConfig::default(),
        );
        assert_eq!(meta.strategy_tier(0), StrategyTier::Vector);
        assert_eq!(meta.strategy_tier(1), StrategyTier::Hybrid);
    }

    #[test]
    fn hybrid_strategies_are_infinite() {
        let fuzzer = FuzzerStrategy::new(Some(0));
        assert!(!fuzzer.is_finite());
        assert_eq!(fuzzer.tier(), StrategyTier::Hybrid);
    }
    }
}
