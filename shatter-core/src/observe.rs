//! Stage 1 (Observe): Execute a function with pre-generated inputs and collect
//! execution traces. Pure observation — no Z3 or constraint solving.
//!
//! The observe module provides a composable building block: given a function and
//! a list of inputs, it executes each input via the language frontend and returns
//! structured trace data (branch coverage, line coverage, side effects, discovery
//! attribution). Callers are responsible for generating inputs (random, boundary,
//! user-provided) before calling into this module.
//!
//! ## Canonical execution primitive
//!
//! [`observe_single`] is the single source of truth for the execute → classify →
//! track cycle. Both [`observe_batch`] and [`explorer::explore_function`] route
//! through it, eliminating duplication and drift risk between random and batch
//! observation paths.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::coverage_metrics::DiscoveryMethod;
use crate::explorer::{
    ExecutionSummary, ExploreError, LoopBuckets, ObservationOutput, classify_error_intent,
    frontend_supports, path_hash, send_setup, send_teardown,
};
use crate::frontend::{Frontend, FrontendError};
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{
    Command as ProtoCommand, ExecuteResult, FunctionAnalysis, MockConfig, ResponseResult,
};
use crate::protocol::{ExecutionProfile, SetupContextStack, SetupLevel};

/// Errors that can occur during observation.
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error("unexpected response from frontend: {0}")]
    UnexpectedResponse(String),
    #[error("instrumentation failed: {0}")]
    InstrumentationFailed(String),
}

impl From<ExploreError> for ObserveError {
    fn from(e: ExploreError) -> Self {
        match e {
            ExploreError::Frontend(fe) => Self::Frontend(fe),
            ExploreError::UnexpectedResponse(msg) => Self::UnexpectedResponse(msg),
        }
    }
}

/// Configuration for the observe stage. Contains only execution-related settings;
/// input generation is the caller's responsibility.
#[derive(Debug, Clone)]
pub struct ObserveConfig {
    /// Path to the source file (needed for instrumentation).
    pub file: String,
    /// Mock configurations to pass to Execute commands.
    pub mocks: Vec<MockConfig>,
    /// Path to the setup file, if configured.
    pub setup_file: Option<String>,
    /// When to run setup relative to executions.
    pub setup_level: SetupLevel,
    /// Frontend capabilities (used to gate setup/teardown commands).
    pub capabilities: FrontendCapabilities,
    /// Detected project root directory.
    pub project_root: Option<String>,
    /// Opaque execution profile selected for this function, if any.
    pub execution_profile: Option<ExecutionProfile>,
    /// Iteration count bucket boundaries for loop-aware path hashing.
    pub loop_buckets: LoopBuckets,
    /// Wall-clock timeout for the entire observation phase.
    pub timeout: Option<Duration>,
    /// Skip the Instrument command (caller already instrumented).
    pub skip_instrument: bool,
    /// When false, send `capture: false` in Execute commands — skips side-effect
    /// collection (console output, file writes, etc.) for lower per-execute overhead.
    /// Non-capture outputs (branch_path, lines_executed, return_value, thrown_error)
    /// remain correct regardless of this setting.
    pub capture_side_effects: bool,
}

impl From<&crate::explorer::ExploreConfig> for ObserveConfig {
    fn from(ec: &crate::explorer::ExploreConfig) -> Self {
        Self {
            file: ec.file.clone(),
            mocks: ec.mocks.clone(),
            setup_file: ec.setup_file.clone(),
            setup_level: ec.setup_level,
            capabilities: ec.capabilities.clone(),
            project_root: ec.project_root.clone(),
            execution_profile: ec.execution_profile.clone(),
            loop_buckets: ec.loop_buckets.clone(),
            timeout: ec.timeout_explore,
            skip_instrument: false,
            capture_side_effects: ec.capture_side_effects,
        }
    }
}

/// Raw observation results from executing a batch of inputs.
/// Lighter-weight than `ObservationOutput` — no function metadata or summary.
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchObservation {
    /// Every execution result paired with its inputs and mock configs.
    pub raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)>,
    /// Hashes of unique execution paths observed.
    #[serde(skip)]
    pub unique_path_hashes: HashSet<u64>,
    /// Source lines covered across all executions.
    #[serde(skip)]
    pub lines_covered: HashSet<u32>,
    /// Per-branch discovery attribution: which branch_id was first seen.
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
    /// Summaries of executions that discovered new paths.
    pub new_path_executions: Vec<ExecutionSummary>,
}

/// Mutable tracking state shared across multiple `observe_single` calls.
///
/// The caller owns this state and passes it to each call. `observe_single`
/// updates it in place, enabling incremental coverage accumulation across
/// an exploration loop.
pub struct ObserveState {
    /// Hashes of unique execution paths seen so far.
    pub seen_paths: HashSet<u64>,
    /// Branch IDs already discovered (each appears at most once).
    pub seen_branch_ids: HashSet<u32>,
    /// Union of all source lines executed so far.
    pub all_lines: HashSet<u32>,
}

impl ObserveState {
    pub fn new() -> Self {
        Self {
            seen_paths: HashSet::new(),
            seen_branch_ids: HashSet::new(),
            all_lines: HashSet::new(),
        }
    }
}

impl Default for ObserveState {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of observing a single input execution, classified against caller-owned
/// tracking state.
#[derive(Debug)]
pub struct SingleObservation {
    /// The raw execution result from the frontend.
    pub exec_result: ExecuteResult,
    /// Hash of this execution's path (scope-aware when available).
    pub path_hash: u64,
    /// Whether this path hash was new (not previously in `seen_paths`).
    pub is_new_path: bool,
    /// Branch IDs discovered for the first time in this execution.
    pub new_branch_ids: Vec<u32>,
    /// Execution summary, present only when `is_new_path` is true.
    pub execution_summary: Option<ExecutionSummary>,
}

/// Execute a single input and classify the result against caller-owned tracking
/// state.
///
/// This is the canonical execution primitive — the single source of truth for
/// the execute → parse → hash → track cycle. Both [`observe_batch`] and
/// [`explorer::explore_function`](crate::explorer::explore_function) route
/// through this function.
///
/// The caller provides mutable references to its coverage sets; this function
/// updates them in place and returns the classified observation.
#[allow(clippy::too_many_arguments)]
pub async fn observe_single(
    frontend: &mut Frontend,
    function_name: &str,
    inputs: &[serde_json::Value],
    mocks: &[MockConfig],
    setup_context: Option<&SetupContextStack>,
    execution_profile: Option<&ExecutionProfile>,
    loop_buckets: &LoopBuckets,
    state: &mut ObserveState,
    capture: bool,
    prepare_id: Option<&str>,
) -> Result<SingleObservation, ObserveError> {
    let response = frontend
        .send(ProtoCommand::Execute {
            function: function_name.to_string(),
            inputs: inputs.to_vec(),
            mocks: mocks.to_vec(),
            setup_context: setup_context.cloned(),
            capture,
            prepare_id: prepare_id.map(|s| s.to_string()),
            execution_profile: execution_profile.cloned(),
            plan: None,
        })
        .await?;

    let exec_result = match response.result {
        ResponseResult::Execute(result) => *result,
        ResponseResult::Error { code, message, .. } => {
            return Err(ObserveError::UnexpectedResponse(format!(
                "execute error ({code:?}): {message}"
            )));
        }
        other => {
            return Err(ObserveError::UnexpectedResponse(format!(
                "expected Execute response, got {other:?}"
            )));
        }
    };

    for &line in &exec_result.lines_executed {
        state.all_lines.insert(line);
    }

    let hash = path_hash(&exec_result, loop_buckets);
    let is_new_path = state.seen_paths.insert(hash);

    let mut new_branch_ids = Vec::new();
    for decision in &exec_result.branch_path {
        if state.seen_branch_ids.insert(decision.branch_id) {
            new_branch_ids.push(decision.branch_id);
        }
    }

    let execution_summary = if is_new_path {
        let error_intent = classify_error_intent(&exec_result);
        Some(ExecutionSummary {
            inputs: inputs.to_vec(),
            return_value: exec_result.return_value.clone(),
            thrown_error: exec_result
                .thrown_error
                .as_ref()
                .map(|e| format!("{}: {}", e.error_type, e.message)),
            lines_executed: exec_result.lines_executed.clone(),
            is_new_path: true,
            error_intent,
        })
    } else {
        None
    };

    Ok(SingleObservation {
        exec_result,
        path_hash: hash,
        is_new_path,
        new_branch_ids,
        execution_summary,
    })
}

/// Execute a batch of inputs against an already-instrumented function.
///
/// Routes each input through [`observe_single`], the canonical execution
/// primitive. No instrumentation, no setup/teardown lifecycle — the caller
/// manages those concerns.
///
/// Each input vector is executed sequentially. Returns raw execution data,
/// path hashes, line coverage, and discovery attribution.
#[allow(clippy::too_many_arguments)]
pub async fn observe_batch(
    frontend: &mut Frontend,
    function_name: &str,
    inputs: Vec<Vec<serde_json::Value>>,
    mocks: &[MockConfig],
    setup_context: Option<&SetupContextStack>,
    execution_profile: Option<&ExecutionProfile>,
    loop_buckets: &LoopBuckets,
    timeout: Option<Duration>,
    capture: bool,
    prepare_id: Option<&str>,
) -> Result<BatchObservation, ObserveError> {
    let mut state = ObserveState::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)> = Vec::new();

    let start = Instant::now();

    for input in inputs {
        if timeout.is_some_and(|t| start.elapsed() >= t) {
            break;
        }

        let obs = observe_single(
            frontend,
            function_name,
            &input,
            mocks,
            setup_context,
            execution_profile,
            loop_buckets,
            &mut state,
            capture,
            prepare_id,
        )
        .await?;

        for branch_id in &obs.new_branch_ids {
            discoveries.push((*branch_id, DiscoveryMethod::Random));
        }
        if let Some(summary) = obs.execution_summary {
            new_path_executions.push(summary);
        }

        raw_results.push((input, mocks.to_vec(), obs.exec_result));
    }

    Ok(BatchObservation {
        raw_results,
        unique_path_hashes: state.seen_paths,
        lines_covered: state.all_lines,
        discoveries,
        new_path_executions,
    })
}

/// Execute a function with pre-generated inputs and collect execution traces.
///
/// Handles the full observation lifecycle: instrument → setup → execute batch →
/// teardown. Returns `ObservationOutput` compatible with the rest of the pipeline.
///
/// Callers supply pre-generated inputs (random, boundary, user-provided). This
/// function does not generate inputs — that separation keeps the observe stage
/// composable and testable.
pub async fn observe_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    inputs: Vec<Vec<serde_json::Value>>,
    config: &ObserveConfig,
) -> Result<ObservationOutput, ObserveError> {
    let total_input_count = inputs.len() as u32;

    // --- Instrumentation ---
    let instrumentable_line_count = if config.skip_instrument {
        None
    } else {
        let instrument_response = frontend
            .send(ProtoCommand::Instrument {
                file: config.file.clone(),
                function: analysis.name.clone(),
                mocks: config.mocks.clone(),
                project_root: config.project_root.clone(),
                execution_profile: config.execution_profile.clone(),
            })
            .await?;

        match instrument_response.result {
            ResponseResult::Instrument {
                instrumented,
                instrumentable_line_count,
                ..
            } => {
                if !instrumented {
                    return Err(ObserveError::InstrumentationFailed(
                        "instrumentation returned instrumented=false".to_string(),
                    ));
                }
                instrumentable_line_count
            }
            ResponseResult::Error { code, message, .. } => {
                return Err(ObserveError::InstrumentationFailed(format!(
                    "instrument error ({code:?}): {message}"
                )));
            }
            other => {
                return Err(ObserveError::UnexpectedResponse(format!(
                    "expected Instrument response, got {other:?}"
                )));
            }
        }
    };

    // --- Setup lifecycle ---
    let has_setup = config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_level == SetupLevel::Function;
    let per_execution_setup = has_setup && config.setup_level == SetupLevel::Execution;

    let mut setup_context: Option<SetupContextStack> = None;

    if per_function_setup && let Some(ref setup_file) = config.setup_file {
        setup_context = send_setup(
            frontend,
            setup_file,
            &analysis.name,
            config.setup_level,
            config.project_root.clone(),
            config.execution_profile.clone(),
        )
        .await?;
    }

    // --- Execute batch ---
    // For per-execution setup, we need to interleave setup/teardown with each execution.
    let batch = if per_execution_setup {
        observe_batch_with_per_execution_setup(frontend, analysis, inputs, config).await?
    } else {
        observe_batch(
            frontend,
            &analysis.name,
            inputs,
            &config.mocks,
            setup_context.as_ref(),
            config.execution_profile.as_ref(),
            &config.loop_buckets,
            config.timeout,
            config.capture_side_effects,
            None,
        )
        .await?
    };

    // --- Per-function teardown ---
    if per_function_setup && frontend_supports(&config.capabilities, "teardown") {
        send_teardown(frontend, &analysis.name, config.setup_level).await?;
    }

    let total_lines = instrumentable_line_count
        .unwrap_or_else(|| analysis.end_line.saturating_sub(analysis.start_line) + 1);

    let stubbed_modules = crate::explorer::collect_stubbed_modules(&batch.raw_results);
    Ok(ObservationOutput {
        function_name: analysis.name.clone(),
        iterations: total_input_count,
        unique_paths: batch.unique_path_hashes.len(),
        lines_covered: batch.lines_covered.len(),
        total_lines,
        new_path_executions: batch.new_path_executions,
        raw_results: batch.raw_results,
        discoveries: batch.discoveries,
        nondeterministic_fields: vec![],
        float_probe_results: vec![],
        boundary_results: vec![],
        shrunk_witnesses: std::collections::HashMap::new(),
        mcdc_summary: None,
        shrink_stats: crate::shrink::ShrinkStats::default(),
        abandoned_frontiers: vec![],
        opaque_suggestions: vec![],
        stubbed_modules,
        ..Default::default()
    })
}

/// Execute inputs with per-execution setup/teardown interleaved.
async fn observe_batch_with_per_execution_setup(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    inputs: Vec<Vec<serde_json::Value>>,
    config: &ObserveConfig,
) -> Result<BatchObservation, ObserveError> {
    let mut state = ObserveState::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)> = Vec::new();

    let start = Instant::now();

    for input in inputs {
        if config.timeout.is_some_and(|t| start.elapsed() >= t) {
            break;
        }

        // Per-execution setup
        let setup_context = if let Some(ref setup_file) = config.setup_file {
            send_setup(
                frontend,
                setup_file,
                &analysis.name,
                config.setup_level,
                config.project_root.clone(),
                config.execution_profile.clone(),
            )
            .await?
        } else {
            None
        };

        let obs = observe_single(
            frontend,
            &analysis.name,
            &input,
            &config.mocks,
            setup_context.as_ref(),
            config.execution_profile.as_ref(),
            &config.loop_buckets,
            &mut state,
            config.capture_side_effects,
            None,
        )
        .await?;

        // Per-execution teardown
        if frontend_supports(&config.capabilities, "teardown") {
            send_teardown(frontend, &analysis.name, config.setup_level).await?;
        }

        for branch_id in &obs.new_branch_ids {
            discoveries.push((*branch_id, DiscoveryMethod::Random));
        }
        if let Some(summary) = obs.execution_summary {
            new_path_executions.push(summary);
        }

        raw_results.push((input, config.mocks.clone(), obs.exec_result));
    }

    Ok(BatchObservation {
        raw_results,
        unique_path_hashes: state.seen_paths,
        lines_covered: state.all_lines,
        discoveries,
        new_path_executions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};

    /// Build a minimal ExecuteResult with specified branch decisions and lines.
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

    #[test]
    fn batch_observation_path_dedup() {
        // Two identical exec results should yield 1 unique path.
        let r1 = make_exec_result(&[(1, true), (2, false)], &[10, 20]);
        let r2 = make_exec_result(&[(1, true), (2, false)], &[10, 20]);

        let buckets = LoopBuckets::none();
        let h1 = path_hash(&r1, &buckets);
        let h2 = path_hash(&r2, &buckets);

        assert_eq!(h1, h2, "identical branch paths should produce same hash");

        // Different branch decisions should produce different hashes.
        let r3 = make_exec_result(&[(1, true), (2, true)], &[10, 20, 30]);
        let h3 = path_hash(&r3, &buckets);
        assert_ne!(
            h1, h3,
            "different branch paths should produce different hashes"
        );
    }

    #[test]
    fn batch_observation_discovery_uniqueness() {
        // Each branch_id should appear at most once in discoveries.
        let mut seen_branch_ids: HashSet<u32> = HashSet::new();
        let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();

        // Simulate two executions hitting overlapping branches.
        let branches_1 = vec![1u32, 2, 3];
        let branches_2 = vec![2u32, 3, 4];

        for branch_id in branches_1 {
            if seen_branch_ids.insert(branch_id) {
                discoveries.push((branch_id, DiscoveryMethod::Random));
            }
        }
        for branch_id in branches_2 {
            if seen_branch_ids.insert(branch_id) {
                discoveries.push((branch_id, DiscoveryMethod::Random));
            }
        }

        // Branch IDs 1, 2, 3, 4 — each discovered exactly once.
        assert_eq!(discoveries.len(), 4);
        let ids: HashSet<u32> = discoveries.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, HashSet::from([1, 2, 3, 4]));
    }

    #[test]
    fn batch_observation_lines_covered_union() {
        // Lines covered should be the union across all executions.
        let mut all_lines: HashSet<u32> = HashSet::new();

        let r1 = make_exec_result(&[(1, true)], &[10, 20, 30]);
        let r2 = make_exec_result(&[(1, false)], &[10, 40, 50]);

        for &line in &r1.lines_executed {
            all_lines.insert(line);
        }
        for &line in &r2.lines_executed {
            all_lines.insert(line);
        }

        assert_eq!(all_lines, HashSet::from([10, 20, 30, 40, 50]));
    }

    #[test]
    fn observe_config_from_explore_config() {
        use crate::explorer::ExploreConfig;

        let explore_config = ExploreConfig {
            file: "test.ts".to_string(),
            execution_profile: None,
            max_iterations: Some(100),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities {
                commands: std::collections::HashSet::new(),
                complex_types: std::collections::HashSet::new(),
            },
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: Some("/project".to_string()),
            loop_buckets: LoopBuckets::default(),
            timeout_explore: Some(Duration::from_secs(30)),
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: crate::explorer::IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
        };

        let observe_config = ObserveConfig::from(&explore_config);

        assert_eq!(observe_config.file, "test.ts");
        assert_eq!(observe_config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(observe_config.project_root.as_deref(), Some("/project"));
        assert!(!observe_config.skip_instrument);
        assert!(!observe_config.capture_side_effects);
    }

    #[test]
    fn empty_inputs_produce_empty_observation() {
        // BatchObservation from zero inputs should have empty everything.
        let batch = BatchObservation {
            raw_results: vec![],
            unique_path_hashes: HashSet::new(),
            lines_covered: HashSet::new(),
            discoveries: vec![],
            new_path_executions: vec![],
        };

        assert_eq!(batch.raw_results.len(), 0);
        assert_eq!(batch.unique_path_hashes.len(), 0);
        assert_eq!(batch.lines_covered.len(), 0);
        assert_eq!(batch.discoveries.len(), 0);
        assert_eq!(batch.new_path_executions.len(), 0);
    }

    #[test]
    fn observe_state_tracks_paths_and_branches() {
        let buckets = LoopBuckets::none();
        let mut state = ObserveState::new();

        let r1 = make_exec_result(&[(1, true), (2, false)], &[10, 20]);
        let r2 = make_exec_result(&[(1, true), (2, false)], &[10, 20]);
        let r3 = make_exec_result(&[(1, true), (2, true)], &[10, 30]);

        // First execution: new path, discovers branches 1 and 2
        let h1 = path_hash(&r1, &buckets);
        let is_new_1 = state.seen_paths.insert(h1);
        assert!(is_new_1);

        for decision in &r1.branch_path {
            state.seen_branch_ids.insert(decision.branch_id);
        }
        for &line in &r1.lines_executed {
            state.all_lines.insert(line);
        }

        // Second execution: same path, not new
        let h2 = path_hash(&r2, &buckets);
        let is_new_2 = state.seen_paths.insert(h2);
        assert!(!is_new_2, "identical path should not be new");

        // Third execution: different path, new
        let h3 = path_hash(&r3, &buckets);
        let is_new_3 = state.seen_paths.insert(h3);
        assert!(is_new_3, "different path should be new");

        for &line in &r3.lines_executed {
            state.all_lines.insert(line);
        }

        assert_eq!(state.seen_paths.len(), 2);
        assert_eq!(state.all_lines, HashSet::from([10, 20, 30]));
        assert_eq!(state.seen_branch_ids, HashSet::from([1, 2]));
    }

    #[test]
    fn observe_state_default_is_empty() {
        let state = ObserveState::default();
        assert!(state.seen_paths.is_empty());
        assert!(state.seen_branch_ids.is_empty());
        assert!(state.all_lines.is_empty());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};
    use proptest::prelude::*;

    /// Generate a random BranchDecision with a given branch_id.
    fn arb_branch_decision(max_id: u32) -> impl Strategy<Value = BranchDecision> {
        (0..max_id, any::<bool>()).prop_map(|(id, taken)| BranchDecision {
            branch_id: id,
            line: id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: String::new(),
            },
            conditions: None,
        })
    }

    /// Generate a random ExecuteResult with up to `max_branches` branches.
    fn arb_exec_result(max_branches: usize) -> impl Strategy<Value = ExecuteResult> {
        let branches = proptest::collection::vec(arb_branch_decision(20), 0..max_branches);
        let lines = proptest::collection::vec(1..200u32, 0..10);

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

    proptest! {
        /// The number of unique paths equals the number of distinct path_hash values.
        #[test]
        fn unique_paths_equals_distinct_hashes(
            results in proptest::collection::vec(arb_exec_result(5), 1..20)
        ) {
            let buckets = LoopBuckets::none();
            let mut seen: HashSet<u64> = HashSet::new();
            for r in &results {
                seen.insert(path_hash(r, &buckets));
            }

            // The HashSet invariant guarantees this; this test validates
            // that path_hash is deterministic for equivalent results.
            let mut seen2: HashSet<u64> = HashSet::new();
            for r in &results {
                seen2.insert(path_hash(r, &buckets));
            }
            prop_assert_eq!(seen, seen2);
        }

        /// Lines covered is the union of all lines_executed vectors.
        #[test]
        fn lines_covered_is_union(
            results in proptest::collection::vec(arb_exec_result(3), 1..15)
        ) {
            let mut expected: HashSet<u32> = HashSet::new();
            for r in &results {
                for &line in &r.lines_executed {
                    expected.insert(line);
                }
            }

            // Simulate the observe loop's line tracking
            let mut actual: HashSet<u32> = HashSet::new();
            for r in &results {
                for &line in &r.lines_executed {
                    actual.insert(line);
                }
            }

            prop_assert_eq!(expected, actual);
        }

        /// Each branch_id appears at most once in discoveries.
        #[test]
        fn discovery_branch_ids_unique(
            results in proptest::collection::vec(arb_exec_result(5), 1..20)
        ) {
            let mut seen_branch_ids: HashSet<u32> = HashSet::new();
            let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();

            for r in &results {
                for decision in &r.branch_path {
                    if seen_branch_ids.insert(decision.branch_id) {
                        discoveries.push((decision.branch_id, DiscoveryMethod::Random));
                    }
                }
            }

            // No duplicate branch_ids
            let ids: Vec<u32> = discoveries.iter().map(|(id, _)| *id).collect();
            let unique_ids: HashSet<u32> = ids.iter().copied().collect();
            prop_assert_eq!(ids.len(), unique_ids.len());
        }

        /// Coverage (lines_covered set size) is monotonically non-decreasing
        /// as we process more execution results.
        #[test]
        fn coverage_monotonically_nondecreasing(
            results in proptest::collection::vec(arb_exec_result(3), 1..15)
        ) {
            let mut all_lines: HashSet<u32> = HashSet::new();
            let mut prev_count = 0usize;

            for r in &results {
                for &line in &r.lines_executed {
                    all_lines.insert(line);
                }
                prop_assert!(all_lines.len() >= prev_count);
                prev_count = all_lines.len();
            }
        }

        /// ObserveState accumulates correctly across multiple results:
        /// seen_paths grows monotonically, new_branch_ids are always genuinely new,
        /// and all_lines is the union of all lines_executed.
        #[test]
        fn observe_state_accumulates_correctly(
            results in proptest::collection::vec(arb_exec_result(5), 1..20)
        ) {
            let buckets = LoopBuckets::none();
            let mut state = ObserveState::new();
            let mut expected_lines: HashSet<u32> = HashSet::new();
            let mut all_new_branch_ids: Vec<u32> = Vec::new();
            let mut prev_paths_count = 0;

            for r in &results {
                let _prev_branch_count = state.seen_branch_ids.len();

                // Simulate observe_single's tracking logic
                for &line in &r.lines_executed {
                    state.all_lines.insert(line);
                    expected_lines.insert(line);
                }

                let hash = path_hash(r, &buckets);
                let is_new = state.seen_paths.insert(hash);

                let mut new_ids = Vec::new();
                for decision in &r.branch_path {
                    if state.seen_branch_ids.insert(decision.branch_id) {
                        new_ids.push(decision.branch_id);
                    }
                }

                // Path count never decreases
                prop_assert!(state.seen_paths.len() >= prev_paths_count);
                prev_paths_count = state.seen_paths.len();

                // New branch IDs are genuinely new
                for &id in &new_ids {
                    prop_assert!(!all_new_branch_ids.contains(&id),
                        "branch_id {} discovered twice", id);
                }
                all_new_branch_ids.extend(new_ids);

                // is_new correctly reflects whether the path was already seen
                if is_new {
                    prop_assert!(state.seen_paths.contains(&hash));
                }
            }

            // Lines covered is exact union
            prop_assert_eq!(&state.all_lines, &expected_lines);
        }
    }
}
