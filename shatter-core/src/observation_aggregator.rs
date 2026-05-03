//! Parallel-safe aggregation seam for exploration observations.
//!
//! `ObservationAggregator` owns the authoritative state previously inlined in
//! `explorer::explore_function` and `orchestrator::explore`'s per-execution
//! aggregation step:
//!
//! - `ObserveState` (path-hash set, branch-id set, line-coverage set).
//! - `discoveries` — per-branch first-discovery attribution.
//! - `new_path_executions` — execution summaries for each path-discovering execution.
//! - `raw_results` — every `(inputs, mocks, ExecuteResult)` tuple.
//! - `iterations` — executed-event counter feeding `ObservationOutput.iterations`.
//! - `last_discovery_iteration` — feeds `iters_since_new_discovery` on
//!   `ExploreProgressSnapshot`.
//!
//! See `docs/specs/concurrent-single-function-exploration.md` §6 for the
//! semantic contract. The aggregator is the single serial drain point: in
//! str-frc.3 the observer pool will route completed events from N observer
//! workers through one aggregator instance via a channel, so the same
//! single-threaded `aggregate()` entry point applies.
//!
//! # Out-of-order safety
//!
//! `aggregate()` accepts events in **arrival order** and is independent of any
//! producer-side sequence number. The set-valued fields (`seen_paths`,
//! `seen_branch_ids`, `all_lines`, and the set of discovered branch_ids) are
//! permutation-invariant — feeding the same multiset of events in any order
//! yields equal sets. Per spec §6.3, the per-branch `DiscoveryMethod` may
//! vary across runs because the first arrival wins attribution.
//!
//! # Idempotence
//!
//! Replaying an identical event preserves the set-valued fields (path/branch
//! discovery is set-based) but appends to the arrival-order vectors
//! (`raw_results`, `new_path_executions`, `discoveries`) — this matches the
//! existing single-worker behavior, where a fresh execution always pushes a
//! fresh `raw_results` entry.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::coverage_metrics::DiscoveryMethod;
use crate::explorer::{ExecutionSummary, LoopBuckets, ObservationOutput, classify_error_intent};
use crate::observe::ObserveState;
use crate::protocol::{ExecuteResult, MockConfig};
use crate::shrink::ShrinkStats;

/// One completed observation produced by an executor and ready to be folded
/// into the aggregator's authoritative state.
///
/// Producers (today: the sequential explorer loop; in str-frc.3: each pooled
/// observer worker) construct one event per `Execute` round-trip and hand it
/// to the aggregator. The aggregator is the single owner of the merged state.
#[derive(Debug, Clone)]
pub struct ObservationEvent {
    /// Inputs passed to `Execute`. Preserved verbatim into `raw_results`.
    pub inputs: Vec<JsonValue>,
    /// Mock values active for this execution. Preserved verbatim into `raw_results`.
    pub mocks: Vec<MockConfig>,
    /// Execution result returned by the frontend.
    pub result: ExecuteResult,
    /// How this execution was generated (random, Z3, fuzzed, …). Used for
    /// per-branch discovery attribution.
    pub discovery_method: DiscoveryMethod,
}

/// Outcome of a single `aggregate()` call, surfaced to the producer so it can
/// drive feedback loops (e.g. `MetaStrategy::record_outcome`, surplus-claim
/// recent-hits window) without re-deriving state already computed by the
/// aggregator.
#[derive(Debug, Clone)]
pub struct AggregateOutcome {
    /// `true` when the event's path hash had not been seen before.
    pub is_new_path: bool,
    /// Branch IDs first-seen by this event, in encounter order within the
    /// event's `branch_path`.
    pub new_branch_ids: Vec<u32>,
    /// The path hash computed for this event.
    pub path_hash: u64,
    /// 1-based index of this event in the aggregator's lifetime
    /// (= `iterations` after this aggregate call).
    pub iteration_index: u32,
}

/// Aggregator for completed observations.
///
/// Single-threaded by construction: the caller (sequential loop today,
/// channel-draining task in str-frc.3) owns the only `&mut` reference and
/// calls `aggregate()` once per completed event.
pub struct ObservationAggregator {
    state: ObserveState,
    discoveries: Vec<(u32, DiscoveryMethod)>,
    new_path_executions: Vec<ExecutionSummary>,
    raw_results: Vec<(Vec<JsonValue>, Vec<MockConfig>, ExecuteResult)>,
    iterations: u32,
    last_discovery_iteration: u32,
    loop_buckets: LoopBuckets,
}

impl ObservationAggregator {
    /// Create an empty aggregator parameterised by the path-hash bucketing
    /// strategy used by all events it will see. The bucketing must match the
    /// one used elsewhere in the explore loop or `path_hash` values will not
    /// agree.
    pub fn new(loop_buckets: LoopBuckets) -> Self {
        Self {
            state: ObserveState::new(),
            discoveries: Vec::new(),
            new_path_executions: Vec::new(),
            raw_results: Vec::new(),
            iterations: 0,
            last_discovery_iteration: 0,
            loop_buckets,
        }
    }

    /// Borrow the underlying `ObserveState`. Intended for callers (e.g. float
    /// probe pre-pass) that bypass the event API and need to update path/line
    /// tracking directly. Use sparingly — direct mutation does not increment
    /// `iterations` or feed the discovery bookkeeping.
    pub fn observe_state_mut(&mut self) -> &mut ObserveState {
        &mut self.state
    }

    /// Number of events aggregated so far.
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Number of unique paths discovered.
    pub fn unique_paths_count(&self) -> usize {
        self.state.seen_paths.len()
    }

    /// Number of unique source lines covered.
    pub fn lines_covered_count(&self) -> usize {
        self.state.all_lines.len()
    }

    /// Number of discoveries (= branch IDs seen at least once).
    pub fn discoveries_count(&self) -> usize {
        self.discoveries.len()
    }

    /// Iteration index at which the most recent new path was aggregated (0
    /// when no new path has ever been observed).
    pub fn last_discovery_iteration(&self) -> u32 {
        self.last_discovery_iteration
    }

    /// Iterations elapsed since the last new path was aggregated.
    pub fn iters_since_new_discovery(&self) -> u32 {
        self.iterations.saturating_sub(self.last_discovery_iteration)
    }

    /// Read-only view of the aggregated raw results. Used by callers that
    /// need to drive shrinking, witness selection, or other post-pass
    /// analysis off the accumulated results without consuming the aggregator.
    pub fn raw_results(&self) -> &[(Vec<JsonValue>, Vec<MockConfig>, ExecuteResult)] {
        &self.raw_results
    }

    /// Fold one observation event into the aggregator. Returns the outcome
    /// the producer needs to drive feedback loops.
    ///
    /// Invariants (proved by the proptests at the bottom of this module):
    ///
    /// 1. `iterations` increments by exactly 1 per call.
    /// 2. `seen_paths`, `seen_branch_ids`, `all_lines` are set-valued and
    ///    permutation-invariant across event arrival order.
    /// 3. Each `branch_id` appears at most once in `discoveries`, regardless
    ///    of arrival order. Per spec §6.3, the attached `DiscoveryMethod` is
    ///    whichever event first surfaced that branch_id (arrival-order wins).
    /// 4. `lines_covered_count()` is monotonically non-decreasing across calls.
    /// 5. `last_discovery_iteration` advances only when `is_new_path` is
    ///    true; `iters_since_new_discovery()` resets to 0 in that case.
    pub fn aggregate(&mut self, event: ObservationEvent) -> AggregateOutcome {
        let ObservationEvent {
            inputs,
            mocks,
            result,
            discovery_method,
        } = event;

        for &line in &result.lines_executed {
            self.state.all_lines.insert(line);
        }

        let path_hash = crate::explorer::path_hash(&result, &self.loop_buckets);
        let is_new_path = self.state.seen_paths.insert(path_hash);

        let mut new_branch_ids = Vec::new();
        for decision in &result.branch_path {
            if self.state.seen_branch_ids.insert(decision.branch_id) {
                new_branch_ids.push(decision.branch_id);
                self.discoveries.push((decision.branch_id, discovery_method));
            }
        }

        if is_new_path {
            let summary = ExecutionSummary {
                inputs: inputs.clone(),
                return_value: result.return_value.clone(),
                thrown_error: result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: result.lines_executed.clone(),
                is_new_path: true,
                error_intent: classify_error_intent(&result),
            };
            self.new_path_executions.push(summary);
        }

        self.iterations = self.iterations.saturating_add(1);
        if is_new_path {
            self.last_discovery_iteration = self.iterations;
        }

        let outcome = AggregateOutcome {
            is_new_path,
            new_branch_ids,
            path_hash,
            iteration_index: self.iterations,
        };

        self.raw_results.push((inputs, mocks, result));

        outcome
    }

    /// Record an aggregation event whose `ObserveState` half (path-hash,
    /// seen-branch-ids, line-coverage sets) has *already been folded in* by a
    /// caller-owned helper such as [`crate::observe::observe_single`]. The
    /// caller passes the `is_new_path` / `new_branch_ids` it received from
    /// the helper so this method does not re-derive them.
    ///
    /// This is the integration point for today's sequential
    /// `explore_function` and `orchestrator::explore` loops, which share
    /// `ObserveState` with `observe_single`. Parallel callers (str-frc.3
    /// observer pool) use [`Self::aggregate`] instead, which folds the
    /// event in full from a single per-observer-event payload.
    ///
    /// Both code paths converge on the same `discoveries`, `raw_results`,
    /// `new_path_executions`, and counter state.
    pub fn record_post_observe(
        &mut self,
        inputs: Vec<JsonValue>,
        mocks: Vec<MockConfig>,
        result: ExecuteResult,
        discovery_method: DiscoveryMethod,
        is_new_path: bool,
        new_branch_ids: &[u32],
    ) -> AggregateOutcome {
        for &branch_id in new_branch_ids {
            self.discoveries.push((branch_id, discovery_method));
        }

        if is_new_path {
            let summary = ExecutionSummary {
                inputs: inputs.clone(),
                return_value: result.return_value.clone(),
                thrown_error: result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: result.lines_executed.clone(),
                is_new_path: true,
                error_intent: classify_error_intent(&result),
            };
            self.new_path_executions.push(summary);
        }

        self.iterations = self.iterations.saturating_add(1);
        if is_new_path {
            self.last_discovery_iteration = self.iterations;
        }

        let outcome = AggregateOutcome {
            is_new_path,
            new_branch_ids: new_branch_ids.to_vec(),
            path_hash: crate::explorer::path_hash(&result, &self.loop_buckets),
            iteration_index: self.iterations,
        };

        self.raw_results.push((inputs, mocks, result));

        outcome
    }

    /// Push a raw result into the aggregated `raw_results` vector without
    /// counting it as an iteration, recording a discovery, or touching
    /// `new_path_executions`. Intended only for ancillary passes (float
    /// probe pre-pass) that maintain their own per-pass counters and need
    /// the raw result to surface in `ObservationOutput.raw_results` for
    /// downstream consumers.
    pub fn push_raw_result(
        &mut self,
        inputs: Vec<JsonValue>,
        mocks: Vec<MockConfig>,
        result: ExecuteResult,
    ) {
        self.raw_results.push((inputs, mocks, result));
    }

    /// Trailing fields supplied by the explorer at finalisation time. These
    /// are owned by the explorer (not the aggregator) because they originate
    /// from passes outside the per-event loop (float probe, shrink, MC/DC,
    /// frontier abandonment, opaque suggestions).
    #[allow(clippy::too_many_arguments)]
    pub fn into_observation_output(
        self,
        function_name: String,
        total_lines: u32,
        timed_out: bool,
        nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
        float_probe_results: Vec<crate::float_probe::FloatProbeResult>,
        boundary_results: Vec<crate::boundary_search::BoundaryResult>,
        shrunk_witnesses: HashMap<u64, Vec<JsonValue>>,
        mcdc_summary: Option<(usize, usize, usize)>,
        shrink_stats: ShrinkStats,
        abandoned_frontiers: Vec<(u32, u32)>,
        opaque_suggestions: Vec<crate::executability::OpaqueSuggestion>,
        stubbed_modules: Vec<String>,
    ) -> ObservationOutput {
        ObservationOutput {
            function_name,
            iterations: self.iterations,
            unique_paths: self.state.seen_paths.len(),
            lines_covered: self.state.all_lines.len(),
            total_lines,
            new_path_executions: self.new_path_executions,
            raw_results: self.raw_results,
            discoveries: self.discoveries,
            nondeterministic_fields,
            float_probe_results,
            boundary_results,
            shrunk_witnesses,
            mcdc_summary,
            shrink_stats,
            abandoned_frontiers,
            opaque_suggestions,
            stubbed_modules,
            timed_out,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};

    fn make_exec_result(branch_ids: &[(u32, bool)], lines: &[u32]) -> ExecuteResult {
        ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: branch_ids
                .iter()
                .map(|&(id, taken)| BranchDecision {
                    branch_id: id,
                    line: id * 10,
                    taken,
                    constraint: SymConstraint::Unknown {
                        hint: String::new(),
                    },
                    conditions: None,
                })
                .collect(),
            lines_executed: lines.to_vec(),
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 1000,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        }
    }

    fn event(
        branch_ids: &[(u32, bool)],
        lines: &[u32],
        method: DiscoveryMethod,
    ) -> ObservationEvent {
        ObservationEvent {
            inputs: vec![serde_json::json!(0)],
            mocks: vec![],
            result: make_exec_result(branch_ids, lines),
            discovery_method: method,
        }
    }

    #[test]
    fn aggregate_increments_iterations_per_event() {
        let mut agg = ObservationAggregator::new(LoopBuckets::none());
        for i in 0..5 {
            let outcome = agg.aggregate(event(&[(i, true)], &[i * 10], DiscoveryMethod::Random));
            assert_eq!(outcome.iteration_index, i + 1);
        }
        assert_eq!(agg.iterations(), 5);
    }

    #[test]
    fn aggregate_first_arrival_wins_attribution() {
        // Two events that both discover branch 7, with different methods.
        // The first one to be aggregated should own the attribution.
        let mut agg = ObservationAggregator::new(LoopBuckets::none());
        agg.aggregate(event(&[(7, true)], &[10], DiscoveryMethod::Random));
        agg.aggregate(event(&[(7, false)], &[20], DiscoveryMethod::Z3));

        let methods: Vec<DiscoveryMethod> = agg
            .discoveries
            .iter()
            .filter(|(id, _)| *id == 7)
            .map(|(_, m)| *m)
            .collect();
        assert_eq!(methods, vec![DiscoveryMethod::Random]);
    }

    #[test]
    fn aggregate_last_discovery_iteration_tracks_new_paths() {
        let mut agg = ObservationAggregator::new(LoopBuckets::none());
        // event 1: new path
        agg.aggregate(event(&[(1, true)], &[10], DiscoveryMethod::Random));
        assert_eq!(agg.last_discovery_iteration(), 1);
        // event 2: same path, not new
        agg.aggregate(event(&[(1, true)], &[10], DiscoveryMethod::Random));
        assert_eq!(agg.last_discovery_iteration(), 1);
        assert_eq!(agg.iters_since_new_discovery(), 1);
        // event 3: new path again
        agg.aggregate(event(&[(2, true)], &[20], DiscoveryMethod::Random));
        assert_eq!(agg.last_discovery_iteration(), 3);
        assert_eq!(agg.iters_since_new_discovery(), 0);
    }

    #[test]
    fn into_observation_output_preserves_aggregated_state() {
        let mut agg = ObservationAggregator::new(LoopBuckets::none());
        agg.aggregate(event(&[(1, true)], &[10, 20], DiscoveryMethod::Random));
        agg.aggregate(event(&[(2, false)], &[30], DiscoveryMethod::Z3));

        let output = agg.into_observation_output(
            "fn".into(),
            42,
            false,
            vec![],
            vec![],
            vec![],
            HashMap::new(),
            None,
            ShrinkStats::default(),
            vec![],
            vec![],
            vec![],
        );

        assert_eq!(output.iterations, 2);
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.lines_covered, 3);
        assert_eq!(output.total_lines, 42);
        assert_eq!(output.discoveries.len(), 2);
        assert_eq!(output.raw_results.len(), 2);
        assert_eq!(output.new_path_executions.len(), 2);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};
    use proptest::prelude::*;
    use std::collections::HashSet;

    /// Maximum branch_id used by the proptest generators. Kept small so that
    /// generated event sequences exercise plenty of overlap (multiple events
    /// hitting the same branch_id) — the order-independence and discovery-
    /// uniqueness invariants only mean something under overlap.
    const MAX_BRANCH_ID: u32 = 8;

    /// Maximum line number used by the proptest generators. Same overlap
    /// motivation as `MAX_BRANCH_ID`.
    const MAX_LINE: u32 = 32;

    fn arb_branch_decision() -> impl Strategy<Value = BranchDecision> {
        (0..MAX_BRANCH_ID, any::<bool>()).prop_map(|(branch_id, taken)| BranchDecision {
            branch_id,
            line: branch_id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: String::new(),
            },
            conditions: None,
        })
    }

    fn arb_exec_result() -> impl Strategy<Value = ExecuteResult> {
        let branches = proptest::collection::vec(arb_branch_decision(), 0..5);
        let lines = proptest::collection::vec(1..MAX_LINE, 0..6);
        (branches, lines).prop_map(|(branch_path, lines_executed)| ExecuteResult {
            return_value: Some(serde_json::json!(0)),
            thrown_error: None,
            branch_path,
            lines_executed,
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 1000,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        })
    }

    fn arb_discovery_method() -> impl Strategy<Value = DiscoveryMethod> {
        prop_oneof![
            Just(DiscoveryMethod::Random),
            Just(DiscoveryMethod::Z3),
            Just(DiscoveryMethod::UserProvided),
            Just(DiscoveryMethod::Drilled),
            Just(DiscoveryMethod::Fuzzed),
        ]
    }

    fn arb_event() -> impl Strategy<Value = ObservationEvent> {
        (arb_exec_result(), arb_discovery_method()).prop_map(|(result, method)| ObservationEvent {
            inputs: vec![serde_json::json!(0)],
            mocks: vec![],
            result,
            discovery_method: method,
        })
    }

    fn fold_events(events: &[ObservationEvent]) -> ObservationAggregator {
        let mut agg = ObservationAggregator::new(LoopBuckets::none());
        for ev in events {
            agg.aggregate(ev.clone());
        }
        agg
    }

    fn discovered_branch_ids(agg: &ObservationAggregator) -> HashSet<u32> {
        agg.discoveries.iter().map(|(id, _)| *id).collect()
    }

    proptest! {
        /// `iterations` after N aggregate calls equals N.
        #[test]
        fn iterations_counts_events(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let agg = fold_events(&events);
            prop_assert_eq!(agg.iterations() as usize, events.len());
        }

        /// Order independence: permuting the event sequence preserves the
        /// set-valued fields (paths, branches, lines, discovered branch_ids).
        ///
        /// Per spec §6.3, the per-branch `DiscoveryMethod` may differ across
        /// permutations because first arrival wins — that's why we compare
        /// the *set* of discovered branch_ids, not the (id, method) pairs.
        #[test]
        fn aggregate_order_independent(
            events in proptest::collection::vec(arb_event(), 0..20),
            shuffle_seed in any::<u64>(),
        ) {
            use rand::seq::SliceRandom;
            use rand::SeedableRng;

            let agg_a = fold_events(&events);

            let mut shuffled = events.clone();
            let mut rng = rand::rngs::StdRng::seed_from_u64(shuffle_seed);
            shuffled.shuffle(&mut rng);
            let agg_b = fold_events(&shuffled);

            prop_assert_eq!(&agg_a.state.seen_paths, &agg_b.state.seen_paths);
            prop_assert_eq!(&agg_a.state.seen_branch_ids, &agg_b.state.seen_branch_ids);
            prop_assert_eq!(&agg_a.state.all_lines, &agg_b.state.all_lines);
            prop_assert_eq!(discovered_branch_ids(&agg_a), discovered_branch_ids(&agg_b));
            prop_assert_eq!(agg_a.iterations(), agg_b.iterations());
            prop_assert_eq!(agg_a.unique_paths_count(), agg_b.unique_paths_count());
            prop_assert_eq!(agg_a.lines_covered_count(), agg_b.lines_covered_count());
        }

        /// Replaying an identical event preserves the set-valued fields:
        /// path hashes are set-typed, so a second arrival of the same path
        /// hash never grows `unique_paths`.
        #[test]
        fn aggregate_idempotent_on_replay_for_set_fields(
            events in proptest::collection::vec(arb_event(), 1..15)
        ) {
            let single = fold_events(&events);

            let mut doubled_events = events.clone();
            doubled_events.extend(events.iter().cloned());
            let doubled = fold_events(&doubled_events);

            prop_assert_eq!(&single.state.seen_paths, &doubled.state.seen_paths);
            prop_assert_eq!(&single.state.seen_branch_ids, &doubled.state.seen_branch_ids);
            prop_assert_eq!(&single.state.all_lines, &doubled.state.all_lines);
            prop_assert_eq!(
                discovered_branch_ids(&single),
                discovered_branch_ids(&doubled)
            );
            // Iterations doubles because each event was a real execution.
            prop_assert_eq!(doubled.iterations(), single.iterations() * 2);
        }

        /// No branch_id appears more than once in `discoveries`, regardless
        /// of how many events surfaced it or in what order.
        #[test]
        fn discovery_branch_ids_unique(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let agg = fold_events(&events);
            let ids: Vec<u32> = agg.discoveries.iter().map(|(id, _)| *id).collect();
            let unique: HashSet<u32> = ids.iter().copied().collect();
            prop_assert_eq!(ids.len(), unique.len());
        }

        /// `lines_covered_count` is monotonically non-decreasing.
        #[test]
        fn coverage_monotonic(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let mut agg = ObservationAggregator::new(LoopBuckets::none());
            let mut prev = 0usize;
            for ev in events {
                agg.aggregate(ev);
                let now = agg.lines_covered_count();
                prop_assert!(now >= prev);
                prev = now;
            }
        }

        /// `unique_paths_count` is monotonically non-decreasing and equals
        /// the number of distinct path hashes across the aggregated events.
        #[test]
        fn unique_paths_no_double_counting(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let mut agg = ObservationAggregator::new(LoopBuckets::none());
            let mut prev = 0usize;
            let mut expected_hashes: HashSet<u64> = HashSet::new();
            for ev in &events {
                let h = crate::explorer::path_hash(&ev.result, &LoopBuckets::none());
                expected_hashes.insert(h);
                agg.aggregate(ev.clone());
                prop_assert!(agg.unique_paths_count() >= prev);
                prev = agg.unique_paths_count();
            }
            prop_assert_eq!(agg.unique_paths_count(), expected_hashes.len());
        }

        /// `last_discovery_iteration` advances strictly when (and only when)
        /// the aggregated event introduced a new path hash.
        #[test]
        fn last_discovery_iteration_tracks_new_paths(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let mut agg = ObservationAggregator::new(LoopBuckets::none());
            let mut last = 0u32;
            let mut iter = 0u32;
            for ev in events {
                let outcome = agg.aggregate(ev);
                iter += 1;
                if outcome.is_new_path {
                    prop_assert_eq!(agg.last_discovery_iteration(), iter);
                    last = iter;
                } else {
                    prop_assert_eq!(agg.last_discovery_iteration(), last);
                }
                prop_assert_eq!(
                    agg.iters_since_new_discovery(),
                    iter.saturating_sub(last)
                );
            }
        }

        /// `new_path_executions` length equals the number of events that
        /// introduced a new path hash — never more, never less.
        #[test]
        fn new_path_executions_count_matches_new_paths(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let mut agg = ObservationAggregator::new(LoopBuckets::none());
            let mut new_path_count = 0usize;
            for ev in events {
                let outcome = agg.aggregate(ev);
                if outcome.is_new_path {
                    new_path_count += 1;
                }
            }
            prop_assert_eq!(agg.new_path_executions.len(), new_path_count);
        }

        /// `raw_results` length equals `iterations` — every event produces
        /// exactly one raw_results entry (no dedup, no skip).
        #[test]
        fn raw_results_length_equals_iterations(
            events in proptest::collection::vec(arb_event(), 0..20)
        ) {
            let agg = fold_events(&events);
            prop_assert_eq!(agg.raw_results.len() as u32, agg.iterations());
        }

        /// `aggregate()` (full event path, used by the parallel observer
        /// pool) and `record_post_observe()` (used by the sequential loop
        /// that shares `ObserveState` with `observe_single`) converge on
        /// equivalent set-valued state and counter values for the same
        /// event sequence.
        #[test]
        fn aggregate_and_record_post_observe_converge(
            events in proptest::collection::vec(arb_event(), 0..15)
        ) {
            // Path A: full-event aggregate, mirroring str-frc.3 observers.
            let agg_a = fold_events(&events);

            // Path B: simulate observe_single's ObserveState mutation
            // before each record_post_observe call, mirroring today's
            // sequential explore_function loop.
            let mut agg_b = ObservationAggregator::new(LoopBuckets::none());
            for ev in &events {
                let state = agg_b.observe_state_mut();

                for &line in &ev.result.lines_executed {
                    state.all_lines.insert(line);
                }

                let h = crate::explorer::path_hash(&ev.result, &LoopBuckets::none());
                let is_new_path = state.seen_paths.insert(h);

                let mut new_branch_ids = Vec::new();
                for decision in &ev.result.branch_path {
                    if state.seen_branch_ids.insert(decision.branch_id) {
                        new_branch_ids.push(decision.branch_id);
                    }
                }

                agg_b.record_post_observe(
                    ev.inputs.clone(),
                    ev.mocks.clone(),
                    ev.result.clone(),
                    ev.discovery_method,
                    is_new_path,
                    &new_branch_ids,
                );
            }

            prop_assert_eq!(&agg_a.state.seen_paths, &agg_b.state.seen_paths);
            prop_assert_eq!(&agg_a.state.seen_branch_ids, &agg_b.state.seen_branch_ids);
            prop_assert_eq!(&agg_a.state.all_lines, &agg_b.state.all_lines);
            prop_assert_eq!(agg_a.iterations(), agg_b.iterations());
            prop_assert_eq!(agg_a.unique_paths_count(), agg_b.unique_paths_count());
            prop_assert_eq!(agg_a.lines_covered_count(), agg_b.lines_covered_count());
            prop_assert_eq!(&agg_a.discoveries, &agg_b.discoveries);
            prop_assert_eq!(agg_a.new_path_executions.len(), agg_b.new_path_executions.len());
            prop_assert_eq!(agg_a.raw_results.len(), agg_b.raw_results.len());
            prop_assert_eq!(agg_a.last_discovery_iteration(), agg_b.last_discovery_iteration());
        }
    }
}
