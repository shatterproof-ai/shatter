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

use crate::frontend::{Frontend, FrontendError};
use crate::input_gen::generate_random_inputs;
use crate::protocol::{Command as ProtoCommand, ExecuteResult, FunctionAnalysis, MockConfig, ResponseResult};

/// Configuration for an exploration run.
#[derive(Debug, Clone)]
pub struct ExploreConfig {
    /// Maximum number of iterations (execute calls) per function.
    pub max_iterations: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Mock configurations to pass to Execute commands.
    pub mocks: Vec<MockConfig>,
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
///
/// Uses the return value and error status to distinguish paths when the
/// frontend doesn't report branch_path data.
fn path_hash(result: &crate::protocol::ExecuteResult) -> u64 {
    let mut hasher = DefaultHasher::new();

    // If branch_path is available, use it for precise deduplication.
    if !result.branch_path.is_empty() {
        for decision in &result.branch_path {
            decision.branch_id.hash(&mut hasher);
            decision.taken.hash(&mut hasher);
        }
    } else {
        // Fall back to return value / error shape.
        if let Some(ref err) = result.thrown_error {
            "error".hash(&mut hasher);
            err.error_type.hash(&mut hasher);
            err.message.hash(&mut hasher);
        } else {
            "ok".hash(&mut hasher);
            let ret_str = serde_json::to_string(&result.return_value).unwrap_or_default();
            ret_str.hash(&mut hasher);
        }
    }

    hasher.finish()
}

/// Explore a single function by generating random inputs and executing them.
///
/// Returns an [`ExplorationResult`] summarizing the discovered paths and coverage.
pub async fn explore_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &ExploreConfig,
) -> Result<ExplorationResult, ExploreError> {
    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    let mut seen_paths: HashSet<u64> = HashSet::new();
    let mut all_lines: HashSet<u32> = HashSet::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();
    let mut iterations: u32 = 0;

    // Track return value → count for the summary
    let mut path_counts: HashMap<u64, u32> = HashMap::new();

    for _ in 0..config.max_iterations {
        iterations += 1;

        let inputs = generate_random_inputs(&analysis.params, &mut rng);

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: inputs.clone(),
                mocks: config.mocks.clone(),
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

        // Track coverage
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

/// Format an exploration result as a human-readable report.
pub fn format_exploration_report(result: &ExplorationResult) -> String {
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
                Some(v) if !v.is_null() => format!("→ {}", format_value_short(v)),
                _ => "→ (void)".to_string(),
            }
        };

        out.push_str(&format!("    {}: ({inputs_str}) {outcome}\n", i + 1));
    }

    out
}

/// Format a JSON value for display, truncating long values.
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
    use crate::protocol::ExecuteResult;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
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
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("positive-even")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn path_hash_same_return_value_produces_same_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("zero")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("zero")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        assert_eq!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn path_hash_distinguishes_error_from_success() {
        let ok = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        let err = ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "Error".into(),
                message: "boom".into(),
                stack: None,
            }),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
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
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1), path_hash(&r2));
    }

    #[test]
    fn format_value_short_truncates_long_values() {
        let short = serde_json::json!("hi");
        assert_eq!(format_value_short(&short), "\"hi\"");

        let long = serde_json::json!("a]very long string that exceeds forty characters easily");
        let formatted = format_value_short(&long);
        assert!(formatted.len() <= 43); // 37 + "..."
        assert!(formatted.ends_with("..."));
    }

    #[test]
    fn format_exploration_report_shows_paths() {
        let result = ExplorationResult {
            function_name: "classify".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![
                ExecutionSummary {
                    inputs: vec![serde_json::json!(5)],
                    return_value: Some(serde_json::json!("positive-odd")),
                    thrown_error: None,
                    lines_executed: vec![1, 2, 3],
                    is_new_path: true,
                },
                ExecutionSummary {
                    inputs: vec![serde_json::json!(-3)],
                    return_value: Some(serde_json::json!("negative")),
                    thrown_error: None,
                    lines_executed: vec![1, 4, 5],
                    is_new_path: true,
                },
            ],
            raw_results: vec![],
        };

        let report = format_exploration_report(&result);
        assert!(report.contains("10 iteration(s)"));
        assert!(report.contains("2 unique path(s)"));
        assert!(report.contains("50%"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("negative"));
    }

    #[test]
    fn format_exploration_report_shows_errors() {
        let result = ExplorationResult {
            function_name: "risky".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 0,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(null)],
                return_value: None,
                thrown_error: Some("TypeError: cannot read null".into()),
                lines_executed: vec![],
                is_new_path: true,
            }],
            raw_results: vec![],
        };

        let report = format_exploration_report(&result);
        assert!(report.contains("THROWS"));
        assert!(report.contains("TypeError"));
    }
}
