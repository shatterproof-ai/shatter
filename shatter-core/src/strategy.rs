//! Input generation strategy trait and adaptive meta-strategy.
//!
//! Each input source (literals, boundary seeds, pool, random, Z3 solver, fuzzer,
//! user-provided) implements [`InputStrategy`]. The [`MetaStrategy`] selects
//! among registered strategies using outcome-based adaptive scoring.

use serde_json::Value;

use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{ExecuteResult, LiteralValue};
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
// InputStrategy trait
// ---------------------------------------------------------------------------

/// A composable input generation strategy.
///
/// Strategies produce candidate input vectors and optionally react to execution
/// feedback. The [`MetaStrategy`] polls strategies by adaptive priority and
/// fans out feedback to all registered strategies after each execution.
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

use rand::Rng;
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
    pub fn feedback(&mut self, inputs: &[Value], result: &ExecuteResult, was_new_path: bool) {
        for state in &mut self.states {
            if !state.exhausted {
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

    /// Try to get the next candidate from a specific strategy, marking it
    /// exhausted if it returns None.
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
                state.exhausted = true;
                None
            }
        }
    }

    // --- Selection modes ---

    fn next_static(&mut self, ctx: &StrategyContext, rng: &mut impl Rng) -> Option<(Vec<Value>, usize)> {
        let weights = self.config.static_weights.clone()?;

        // Build weight vector for non-exhausted strategies.
        loop {
            let mut candidates: Vec<(usize, f64)> = Vec::new();
            for (idx, state) in self.states.iter().enumerate() {
                if state.exhausted {
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
            // Strategy exhausted on this call; loop to rebuild candidates.
        }
    }

    fn next_adaptive(&mut self, ctx: &StrategyContext, rng: &mut impl Rng) -> Option<(Vec<Value>, usize)> {
        loop {
            let scores = self.compute_scores();
            let candidates: Vec<(usize, f64)> = scores
                .into_iter()
                .filter(|(idx, _)| !self.states[*idx].exhausted)
                .collect();

            if candidates.is_empty() {
                return None;
            }

            let idx = weighted_select(&candidates, rng);
            if let Some(inputs) = self.try_next(idx, ctx) {
                return Some((inputs, idx));
            }
            // Strategy exhausted; loop to recompute.
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
}
