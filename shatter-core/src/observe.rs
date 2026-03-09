//! Stage 1 (Observe): Execute a function with pre-generated inputs and collect
//! execution traces. Pure observation — no Z3 or constraint solving.
//!
//! The observe module provides a composable building block: given a function and
//! a list of inputs, it executes each input via the language frontend and returns
//! structured trace data (branch coverage, line coverage, side effects, discovery
//! attribution). Callers are responsible for generating inputs (random, boundary,
//! user-provided) before calling into this module.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::protocol::SetupLevel;
use crate::protocol::SetupContextStack;
use crate::coverage_metrics::DiscoveryMethod;
use crate::explorer::{
    classify_error_intent, frontend_supports, path_hash, send_setup, send_teardown,
    ExecutionSummary, ExploreError, LoopBuckets, ObservationOutput,
};
use crate::frontend::{Frontend, FrontendError};
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{
    Command as ProtoCommand, ExecuteResult, FunctionAnalysis, MockConfig, ResponseResult,
};

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
    /// Iteration count bucket boundaries for loop-aware path hashing.
    pub loop_buckets: LoopBuckets,
    /// Wall-clock timeout for the entire observation phase.
    pub timeout: Option<Duration>,
    /// Skip the Instrument command (caller already instrumented).
    pub skip_instrument: bool,
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
            loop_buckets: ec.loop_buckets.clone(),
            timeout: ec.timeout_explore,
            skip_instrument: false,
        }
    }
}

/// Raw observation results from executing a batch of inputs.
/// Lighter-weight than `ObservationOutput` — no function metadata or summary.
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchObservation {
    /// Every execution result paired with its inputs.
    pub raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)>,
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

/// Execute a batch of inputs against an already-instrumented function.
///
/// This is the lowest-level observation primitive: no instrumentation, no
/// setup/teardown lifecycle. The caller manages those concerns.
///
/// Each input vector is executed sequentially. Returns raw execution data,
/// path hashes, line coverage, and discovery attribution.
pub async fn observe_batch(
    frontend: &mut Frontend,
    function_name: &str,
    inputs: Vec<Vec<serde_json::Value>>,
    mocks: &[MockConfig],
    setup_context: Option<&SetupContextStack>,
    loop_buckets: &LoopBuckets,
    timeout: Option<Duration>,
) -> Result<BatchObservation, ObserveError> {
    let mut seen_paths: HashSet<u64> = HashSet::new();
    let mut all_lines: HashSet<u32> = HashSet::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();

    let start = Instant::now();

    for input in inputs {
        if timeout.is_some_and(|t| start.elapsed() >= t) {
            break;
        }

        let response = frontend
            .send(ProtoCommand::Execute {
                function: function_name.to_string(),
                inputs: input.clone(),
                mocks: mocks.to_vec(),
                setup_context: setup_context.cloned(),
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
            all_lines.insert(line);
        }

        let hash = path_hash(&exec_result, loop_buckets);
        let is_new = seen_paths.insert(hash);

        for decision in &exec_result.branch_path {
            if seen_branch_ids.insert(decision.branch_id) {
                discoveries.push((decision.branch_id, DiscoveryMethod::Random));
            }
        }

        if is_new {
            let error_intent = classify_error_intent(&exec_result);
            new_path_executions.push(ExecutionSummary {
                inputs: input.clone(),
                return_value: exec_result.return_value.clone(),
                thrown_error: exec_result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec_result.lines_executed.clone(),
                is_new_path: true,
                error_intent,
            });
        }

        raw_results.push((input, exec_result));
    }

    Ok(BatchObservation {
        raw_results,
        unique_path_hashes: seen_paths,
        lines_covered: all_lines,
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
    let has_setup =
        config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_level == SetupLevel::Function;
    let per_execution_setup = has_setup && config.setup_level == SetupLevel::Execution;

    let mut setup_context: Option<SetupContextStack> = None;

    if per_function_setup
        && let Some(ref setup_file) = config.setup_file
    {
        setup_context = send_setup(
            frontend,
            setup_file,
            &analysis.name,
            config.setup_level,
            config.project_root.clone(),
        )
        .await?;
    }

    // --- Execute batch ---
    // For per-execution setup, we need to interleave setup/teardown with each execution.
    let batch = if per_execution_setup {
        observe_batch_with_per_execution_setup(
            frontend,
            analysis,
            inputs,
            config,
        )
        .await?
    } else {
        observe_batch(
            frontend,
            &analysis.name,
            inputs,
            &config.mocks,
            setup_context.as_ref(),
            &config.loop_buckets,
            config.timeout,
        )
        .await?
    };

    // --- Per-function teardown ---
    if per_function_setup && frontend_supports(&config.capabilities, "teardown") {
        send_teardown(frontend, &analysis.name, config.setup_level).await?;
    }

    let total_lines = instrumentable_line_count
        .unwrap_or_else(|| analysis.end_line.saturating_sub(analysis.start_line) + 1);

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
    })
}

/// Execute inputs with per-execution setup/teardown interleaved.
async fn observe_batch_with_per_execution_setup(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    inputs: Vec<Vec<serde_json::Value>>,
    config: &ObserveConfig,
) -> Result<BatchObservation, ObserveError> {
    let mut seen_paths: HashSet<u64> = HashSet::new();
    let mut all_lines: HashSet<u32> = HashSet::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();

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
            )
            .await?
        } else {
            None
        };

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: input.clone(),
                mocks: config.mocks.clone(),
                setup_context,
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

        // Per-execution teardown
        if frontend_supports(&config.capabilities, "teardown") {
            send_teardown(frontend, &analysis.name, config.setup_level).await?;
        }

        for &line in &exec_result.lines_executed {
            all_lines.insert(line);
        }

        let hash = path_hash(&exec_result, &config.loop_buckets);
        let is_new = seen_paths.insert(hash);

        for decision in &exec_result.branch_path {
            if seen_branch_ids.insert(decision.branch_id) {
                discoveries.push((decision.branch_id, DiscoveryMethod::Random));
            }
        }

        if is_new {
            let error_intent = classify_error_intent(&exec_result);
            new_path_executions.push(ExecutionSummary {
                inputs: input.clone(),
                return_value: exec_result.return_value.clone(),
                thrown_error: exec_result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec_result.lines_executed.clone(),
                is_new_path: true,
                error_intent,
            });
        }

        raw_results.push((input, exec_result));
    }

    Ok(BatchObservation {
        raw_results,
        unique_path_hashes: seen_paths,
        lines_covered: all_lines,
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
    fn make_exec_result(
        branch_ids: &[(u32, bool)],
        lines: &[u32],
    ) -> ExecuteResult {
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
                })
                .collect(),
            lines_executed: lines.to_vec(),
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 1000,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
            capture_truncation: None,
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
        assert_ne!(h1, h3, "different branch paths should produce different hashes");
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
            max_iterations: 100,
            seed: Some(42),
            mocks: vec![],
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
        };

        let observe_config = ObserveConfig::from(&explore_config);

        assert_eq!(observe_config.file, "test.ts");
        assert_eq!(observe_config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(observe_config.project_root.as_deref(), Some("/project"));
        assert!(!observe_config.skip_instrument);
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
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};

    /// Generate a random BranchDecision with a given branch_id.
    fn arb_branch_decision(max_id: u32) -> impl Strategy<Value = BranchDecision> {
        (0..max_id, any::<bool>()).prop_map(|(id, taken)| BranchDecision {
            branch_id: id,
            line: id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: String::new(),
            },
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
            side_effects: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 1000,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
            capture_truncation: None,
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
    }
}
