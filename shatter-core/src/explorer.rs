//! Exploration engine for discovering execution paths via random input generation.
//!
//! Drives the concolic execution loop: analyze a function's type signature,
//! generate random inputs, execute them via a language frontend, and track
//! unique execution paths. This module implements the random exploration phase
//! (no symbolic solving).

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::config::SetupMode;
use crate::frontend::{Frontend, FrontendError};
use crate::input_gen::{
    generate_inputs_with_custom, generate_random_inputs, prefetch_custom_values, PrefetchedValues,
    ValueSource,
};
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{
    Command as ProtoCommand, ExecuteResult, FunctionAnalysis, MockConfig, ResponseResult,
};

/// Configuration for an exploration run.
#[derive(Debug, Clone)]
pub struct ExploreConfig {
    /// Path to the source file being explored (needed for instrumentation).
    pub file: String,
    /// Maximum number of iterations (execute calls) per function.
    pub max_iterations: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Mock configurations to pass to Execute commands.
    pub mocks: Vec<MockConfig>,
    /// Path to the setup file, if configured.
    pub setup_file: Option<String>,
    /// When to run setup relative to executions.
    pub setup_mode: SetupMode,
    /// Where each parameter's value should come from.
    pub value_sources: Vec<ValueSource>,
    /// Frontend capabilities (used to gate setup/generate commands).
    pub capabilities: FrontendCapabilities,
}

/// Summary of a single function execution during exploration.
#[derive(Debug, Clone)]
pub struct ExecutionSummary {
    /// The input values sent to the function.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error message, if the function threw.
    pub thrown_error: Option<String>,
    /// Lines executed during this call.
    pub lines_executed: Vec<u32>,
    /// Whether this execution discovered a new unique path.
    pub is_new_path: bool,
}

/// Result of exploring a single function.
#[derive(Debug)]
pub struct ExplorationResult {
    /// Name of the explored function.
    pub function_name: String,
    /// Total iterations attempted.
    pub iterations: u32,
    /// Number of unique execution paths discovered.
    pub unique_paths: usize,
    /// Number of unique source lines covered across all executions.
    pub lines_covered: usize,
    /// Total source lines in the function (end_line - start_line + 1).
    pub total_lines: u32,
    /// Summary of each execution that discovered a new path.
    pub new_path_executions: Vec<ExecutionSummary>,
    /// Raw execution results paired with their inputs, for building BehaviorMaps.
    pub raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)>,
}

/// Errors that can occur during exploration.
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error("unexpected response from frontend: {0}")]
    UnexpectedResponse(String),
}

/// Compute a hash representing the "path signature" of an execution.
fn path_hash(result: &crate::protocol::ExecuteResult) -> u64 {
    let mut hasher = DefaultHasher::new();
    if !result.branch_path.is_empty() {
        for decision in &result.branch_path {
            decision.branch_id.hash(&mut hasher);
            decision.taken.hash(&mut hasher);
        }
    } else if !result.lines_executed.is_empty() {
        result.lines_executed.hash(&mut hasher);
    } else if let Some(ref err) = result.thrown_error {
            "error".hash(&mut hasher);
            err.error_type.hash(&mut hasher);
            err.message.hash(&mut hasher);
        } else {
            "ok".hash(&mut hasher);
            let ret_str = serde_json::to_string(&result.return_value).unwrap_or_default();
            ret_str.hash(&mut hasher);
        }
    hasher.finish()
}

/// Check whether the frontend declared support for a specific command.
fn frontend_supports(caps: &FrontendCapabilities, command: &str) -> bool {
    caps.commands.contains(command)
}

/// Send a Setup command to the frontend and return the setup_context.
async fn send_setup(
    frontend: &mut Frontend,
    setup_file: &str,
    function: &str,
    mode: SetupMode,
) -> Result<Option<serde_json::Value>, ExploreError> {
    let response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file.to_string(),
            function: function.to_string(),
            mode,
        })
        .await?;
    match response.result {
        ResponseResult::Setup { setup_context } => Ok(Some(setup_context)),
        ResponseResult::Error { message, .. } => {
            eprintln!("[shatter-core] setup error for {function}: {message}");
            Ok(None)
        }
        other => Err(ExploreError::UnexpectedResponse(format!(
            "expected Setup response, got {other:?}"
        ))),
    }
}

/// Send a Teardown command to the frontend.
async fn send_teardown(frontend: &mut Frontend, function: &str) -> Result<(), ExploreError> {
    let response = frontend
        .send(ProtoCommand::Teardown {
            function: function.to_string(),
        })
        .await?;
    match response.result {
        ResponseResult::TeardownAck => Ok(()),
        ResponseResult::Error { message, .. } => {
            eprintln!("[shatter-core] teardown error for {function}: {message}");
            Ok(())
        }
        other => Err(ExploreError::UnexpectedResponse(format!(
            "expected TeardownAck response, got {other:?}"
        ))),
    }
}

/// Explore a single function by generating random inputs and executing them.
pub async fn explore_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &ExploreConfig,
) -> Result<ExplorationResult, ExploreError> {
    let instrument_response = frontend
        .send(ProtoCommand::Instrument {
            file: config.file.clone(),
            function: analysis.name.clone(),
            mocks: config.mocks.clone(),
        })
        .await?;

    match instrument_response.result {
        ResponseResult::Instrument { instrumented, .. } => {
            if !instrumented {
                return Err(ExploreError::UnexpectedResponse(
                    "instrumentation returned instrumented=false".to_string(),
                ));
            }
        }
        ResponseResult::Error { code, message, .. } => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "instrument error ({code:?}): {message}"
            )));
        }
        other => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "expected Instrument response, got {other:?}"
            )));
        }
    }

    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    // --- Setup lifecycle ---
    let has_setup =
        config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_mode == SetupMode::PerFunction;
    let per_execution_setup = has_setup && config.setup_mode == SetupMode::PerExecution;

    let mut setup_context: Option<serde_json::Value> = None;

    if per_function_setup
        && let Some(ref setup_file) = config.setup_file
    {
        setup_context =
            send_setup(frontend, setup_file, &analysis.name, config.setup_mode).await?;
    }

    // --- Generator prefetch ---
    let has_generators = config
        .value_sources
        .iter()
        .any(|s| matches!(s, ValueSource::CustomGenerator { .. }));
    let use_generators = has_generators && frontend_supports(&config.capabilities, "generate");

    let mut prefetched = if use_generators {
        prefetch_custom_values(&config.value_sources, frontend, config.max_iterations as usize)
            .await
            .unwrap_or_else(|e| {
                eprintln!("[shatter-core] prefetch failed, falling back to built-in: {e}");
                PrefetchedValues::new()
            })
    } else {
        PrefetchedValues::new()
    };

    let mut seen_paths: HashSet<u64> = HashSet::new();
    let mut all_lines: HashSet<u32> = HashSet::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();
    let mut iterations: u32 = 0;
    let mut path_counts: HashMap<u64, u32> = HashMap::new();

    for _ in 0..config.max_iterations {
        iterations += 1;

        // --- Per-execution setup ---
        if per_execution_setup
            && let Some(ref setup_file) = config.setup_file
        {
            setup_context =
                send_setup(frontend, setup_file, &analysis.name, config.setup_mode).await?;
        }

        // --- Input generation ---
        let inputs = if use_generators {
            generate_inputs_with_custom(
                &analysis.params,
                &config.value_sources,
                &mut prefetched,
                &mut rng,
                Some(&config.capabilities),
            )
        } else {
            generate_random_inputs(&analysis.params, &mut rng, None)
        };

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: inputs.clone(),
                mocks: config.mocks.clone(),
                setup_context: setup_context.clone(),
            })
            .await?;

        let exec_result = match response.result {
            ResponseResult::Execute(result) => result,
            ResponseResult::Error { code, message, .. } => {
                return Err(ExploreError::UnexpectedResponse(format!(
                    "execute error ({code:?}): {message}"
                )));
            }
            other => {
                return Err(ExploreError::UnexpectedResponse(format!("{other:?}")));
            }
        };

        // --- Per-execution teardown ---
        if per_execution_setup && frontend_supports(&config.capabilities, "teardown") {
            send_teardown(frontend, &analysis.name).await?;
        }

        for &line in &exec_result.lines_executed {
            all_lines.insert(line);
        }

        let hash = path_hash(&exec_result);
        *path_counts.entry(hash).or_insert(0) += 1;
        let is_new = seen_paths.insert(hash);

        if is_new {
            new_path_executions.push(ExecutionSummary {
                inputs: inputs.clone(),
                return_value: exec_result.return_value.clone(),
                thrown_error: exec_result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec_result.lines_executed.clone(),
                is_new_path: true,
            });
        }

        raw_results.push((inputs, exec_result));
    }

    // --- Per-function teardown ---
    if per_function_setup && frontend_supports(&config.capabilities, "teardown") {
        send_teardown(frontend, &analysis.name).await?;
    }

    let total_lines = analysis.end_line.saturating_sub(analysis.start_line) + 1;

    Ok(ExplorationResult {
        function_name: analysis.name.clone(),
        iterations,
        unique_paths: seen_paths.len(),
        lines_covered: all_lines.len(),
        total_lines,
        new_path_executions,
        raw_results,
    })
}

/// Options for formatting an exploration report.
#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub location: Option<String>,
    pub show_perf: bool,
    pub wall_time: Option<std::time::Duration>,
    pub coverage_metrics: Option<crate::coverage_metrics::CoverageMetrics>,
}

pub fn format_exploration_report(result: &ExplorationResult, options: &ReportOptions) -> String {
    let mut out = String::new();
    let location = options.location.as_deref().unwrap_or("");
    if location.is_empty() {
        out.push_str(&format!("{}\n", result.function_name));
    } else {
        out.push_str(&format!("{} ({})\n", result.function_name, location));
    }
    out.push_str(&format!("  {} distinct path(s)\n", result.unique_paths));
    if result.total_lines > 0 && result.lines_covered > 0 {
        let pct = (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(
            "  Line coverage: {}/{} lines ({pct:.0}%)\n",
            result.lines_covered, result.total_lines
        ));
    }
    if !result.new_path_executions.is_empty() {
        out.push_str("\n  Path clusters:\n");
        for (i, exec) in result.new_path_executions.iter().enumerate() {
            let outcome_label = format_outcome_label(exec);
            out.push_str(&format!("    {}. {}\n", i + 1, outcome_label));
            let inputs_str = exec
                .inputs
                .iter()
                .map(format_value_short)
                .collect::<Vec<_>>()
                .join(", ");
            let outcome_short = format_outcome_short(exec);
            out.push_str(&format!(
                "       e.g. {}({inputs_str}) {outcome_short}\n",
                result.function_name
            ));
        }
    }
    if let Some(ref metrics) = options.coverage_metrics {
        out.push('\n');
        out.push_str(&crate::coverage_metrics::format_coverage_metrics(metrics));
    }
    if options.show_perf {
        if let Some(dur) = options.wall_time {
            out.push_str(&format!(
                "\n  Perf: {:.1}ms, {} iteration(s)\n",
                dur.as_secs_f64() * 1000.0,
                result.iterations
            ));
        } else {
            out.push_str(&format!("\n  Perf: {} iteration(s)\n", result.iterations));
        }
    }
    out
}

pub fn format_exploration_report_verbose(result: &ExplorationResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "  Exploration complete: {} iteration(s), {} unique path(s) discovered\n",
        result.iterations, result.unique_paths
    ));
    if result.total_lines > 0 && result.lines_covered > 0 {
        let pct = (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(
            "  Line coverage: {}/{} lines ({pct:.0}%)\n",
            result.lines_covered, result.total_lines
        ));
    }
    out.push_str("\n  Discovered paths:\n");
    for (i, exec) in result.new_path_executions.iter().enumerate() {
        let inputs_str = exec
            .inputs
            .iter()
            .map(format_value_short)
            .collect::<Vec<_>>()
            .join(", ");
        let outcome = if let Some(ref err) = exec.thrown_error {
            format!("THROWS {err}")
        } else {
            match &exec.return_value {
                Some(v) if !v.is_null() => format!("-> {}", format_value_short(v)),
                _ => "-> (void)".to_string(),
            }
        };
        out.push_str(&format!("    {}: ({inputs_str}) {outcome}\n", i + 1));
    }
    out
}

fn format_outcome_label(exec: &ExecutionSummary) -> String {
    if let Some(ref err) = exec.thrown_error {
        format!("throws {err}")
    } else {
        match &exec.return_value {
            Some(v) if !v.is_null() => format!("returns {}", format_value_short(v)),
            _ => "returns (void)".to_string(),
        }
    }
}

fn format_outcome_short(exec: &ExecutionSummary) -> String {
    if exec.thrown_error.is_some() {
        "-> Error".to_string()
    } else {
        match &exec.return_value {
            Some(v) if !v.is_null() => format!("-> {}", format_value_short(v)),
            _ => "-> (void)".to_string(),
        }
    }
}

fn format_value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SetupMode;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
    use crate::input_gen::ValueSource;
    use crate::orchestrator::FrontendCapabilities;
    use crate::protocol::ExecuteResult;
    use crate::protocol::PerformanceMetrics;

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    #[test]
    fn path_hash_distinguishes_different_return_values() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("negative")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("positive-even")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn path_hash_same_lines_executed_produces_same_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!(3.5)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!(99.0)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        assert_eq!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn path_hash_different_lines_executed_produces_different_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 4],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn path_hash_distinguishes_error_from_success() {
        let ok = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        let err = ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo { error_type: "Error".into(), message: "boom".into(), stack: None }),
            branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            performance: empty_perf(),
        };
        assert_ne!(path_hash(&ok), path_hash(&err));
    }

    #[test]
    fn path_hash_uses_branch_path_when_available() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0, line: 10, taken: true,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            }],
            lines_executed: vec![], calls_to_external: vec![], path_constraints: vec![],
            side_effects: vec![], performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0, line: 10, taken: false,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            }],
            lines_executed: vec![], calls_to_external: vec![], path_constraints: vec![],
            side_effects: vec![], performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn format_value_short_truncates_long_values() {
        let short = serde_json::json!("hi");
        assert_eq!(format_value_short(&short), "\"hi\"");
        let long = serde_json::json!("a]very long string that exceeds forty characters easily");
        let formatted = format_value_short(&long);
        assert!(formatted.len() <= 43);
        assert!(formatted.ends_with("..."));
    }

    #[test]
    fn format_exploration_report_shows_clustered_paths() {
        let result = ExplorationResult {
            function_name: "classify".into(), iterations: 10, unique_paths: 2,
            lines_covered: 5, total_lines: 10,
            new_path_executions: vec![
                ExecutionSummary {
                    inputs: vec![serde_json::json!(5)],
                    return_value: Some(serde_json::json!("positive-odd")),
                    thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true,
                },
                ExecutionSummary {
                    inputs: vec![serde_json::json!(-3)],
                    return_value: Some(serde_json::json!("negative")),
                    thrown_error: None, lines_executed: vec![1, 4, 5], is_new_path: true,
                },
            ],
            raw_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(report.contains("classify"));
        assert!(report.contains("2 distinct path(s)"));
        assert!(report.contains("50%"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("negative"));
        assert!(report.contains("Path clusters:"));
        assert!(report.contains("e.g."));
    }

    #[test]
    fn format_exploration_report_with_location() {
        let result = ExplorationResult {
            function_name: "safeDivide".into(), iterations: 5, unique_paths: 1,
            lines_covered: 3, total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(10)],
                return_value: Some(serde_json::json!(5)),
                thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true,
            }],
            raw_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions {
            location: Some("src/math.ts:10".into()), ..Default::default()
        });
        assert!(report.contains("safeDivide (src/math.ts:10)"));
    }

    #[test]
    fn format_exploration_report_shows_errors() {
        let result = ExplorationResult {
            function_name: "risky".into(), iterations: 5, unique_paths: 1,
            lines_covered: 0, total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(null)],
                return_value: None,
                thrown_error: Some("TypeError: cannot read null".into()),
                lines_executed: vec![], is_new_path: true,
            }],
            raw_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(report.contains("throws"));
        assert!(report.contains("TypeError"));
    }

    #[test]
    fn format_exploration_report_with_perf() {
        let result = ExplorationResult {
            function_name: "fast".into(), iterations: 10, unique_paths: 1,
            lines_covered: 0, total_lines: 0, new_path_executions: vec![], raw_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions {
            show_perf: true, wall_time: Some(std::time::Duration::from_millis(42)),
            ..Default::default()
        });
        assert!(report.contains("Perf:"));
        assert!(report.contains("42.0ms"));
        assert!(report.contains("10 iteration(s)"));
    }

    #[test]
    fn format_exploration_report_includes_coverage_metrics() {
        let result = ExplorationResult {
            function_name: "analyze".into(), iterations: 20, unique_paths: 3,
            lines_covered: 8, total_lines: 10, new_path_executions: vec![], raw_results: vec![],
        };
        let metrics = crate::coverage_metrics::CoverageMetrics {
            total_branches: 4, z3_solved: 2, random_found: 1, user_provided: 0,
            uncovered: 1, symexpr_count: 3, unknown_count: 1,
        };
        let report = format_exploration_report(&result, &ReportOptions {
            coverage_metrics: Some(metrics), ..Default::default()
        });
        assert!(report.contains("Coverage metrics:"));
        assert!(report.contains("Z3 solved"));
        assert!(report.contains("Uncovered"));
        assert!(report.contains("Symbolic expr"));
    }

    #[test]
    fn format_exploration_report_verbose_shows_legacy_format() {
        let result = ExplorationResult {
            function_name: "classify".into(), iterations: 10, unique_paths: 2,
            lines_covered: 5, total_lines: 10,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(5)],
                return_value: Some(serde_json::json!("positive-odd")),
                thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true,
            }],
            raw_results: vec![],
        };
        let report = format_exploration_report_verbose(&result);
        assert!(report.contains("10 iteration(s)"));
        assert!(report.contains("2 unique path(s)"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("Discovered paths:"));
    }

    async fn spawn_noop_frontend() -> Frontend {
        use crate::frontend::FrontendConfig;
        use std::path::{Path, PathBuf};
        use std::time::Duration;
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![noop_path.to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        Frontend::spawn(&config).await.expect("spawn noop frontend")
    }

    fn stub_analysis() -> FunctionAnalysis {
        use crate::types::{ParamInfo, TypeInfo};
        FunctionAnalysis {
            name: "stub".into(), exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![], dependencies: vec![],
            return_type: TypeInfo::Unknown, start_line: 1, end_line: 5,
        }
    }

    #[tokio::test]
    async fn explore_function_instruments_before_executing() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 3, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: FrontendCapabilities::default(),
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("should succeed with noop frontend");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 3);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn per_function_setup_teardown_lifecycle() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(), "execute".into(), "instrument".into(),
            "setup".into(), "teardown".into(),
        ]);
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("per_function setup should succeed");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn per_execution_setup_teardown_lifecycle() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(), "execute".into(), "instrument".into(),
            "setup".into(), "teardown".into(),
        ]);
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerExecution,
            value_sources: vec![], capabilities: caps,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("per_execution setup should succeed");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 2);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn setup_skipped_when_frontend_lacks_capability() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(), "execute".into(), "instrument".into(),
        ]);
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("should succeed without setup capability");
        assert_eq!(result.iterations, 2);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn generator_integration_uses_custom_values() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(), "execute".into(), "instrument".into(), "generate".into(),
        ]);
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![ValueSource::CustomGenerator {
                generator_name: "x".into(), param_name: Some("x".into()),
                generator_file: "gen.ts".into(),
                kind: crate::protocol::GeneratorKind::ParamName,
            }],
            capabilities: caps,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("generators should succeed");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn fallback_to_builtin_when_no_generators_configured() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(), "execute".into(), "instrument".into(), "generate".into(),
        ]);
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 3, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("no generators should succeed");
        assert_eq!(result.iterations, 3);
        frontend.shutdown().await.expect("shutdown failed");
    }
}
