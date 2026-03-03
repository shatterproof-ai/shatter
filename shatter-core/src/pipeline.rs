//! Staged pipeline: composable Observe → Analyze → Solve → Specify stages.
//!
//! Each stage is a pure function with well-defined input/output types.
//! The Analyze stage groups raw exploration results into equivalence classes,
//! builds a behavior map, and computes coverage metrics.

use crate::behavior::BehaviorMap;
use crate::coverage_metrics::{CoverageMetrics, DiscoveryMethod};
use crate::equivalence::{self, EquivalenceClass};
use crate::execution_record::{ExecutionRecord, SymConstraint};
use crate::explorer::ExplorationResult;
use crate::protocol::{ExecuteResult, FunctionAnalysis};

use std::hash::{DefaultHasher, Hash, Hasher};

/// Output of the Analyze stage.
#[derive(Debug)]
pub struct AnalyzeOutput {
    /// Equivalence classes grouping executions by branch path.
    pub eq_classes: Vec<EquivalenceClass>,
    /// Behavior map built from execution records.
    pub behavior_map: BehaviorMap,
    /// Branch coverage metrics with per-method attribution.
    pub coverage_metrics: CoverageMetrics,
}

/// Run the Analyze stage on an observation result.
///
/// Takes the raw exploration output and the static function analysis,
/// then produces equivalence classes, a behavior map, and coverage metrics.
pub fn analyze(observe: &ExplorationResult, analysis: &FunctionAnalysis) -> AnalyzeOutput {
    // Build equivalence classes from raw results.
    let eq_classes = equivalence::group_into_classes(&observe.raw_results);

    // Build execution records for BehaviorMap construction.
    let records: Vec<ExecutionRecord> = observe
        .raw_results
        .iter()
        .map(|(inputs, result)| execution_record_from_result(&observe.function_name, inputs, result))
        .collect();

    let behavior_map = BehaviorMap::from_records(&observe.function_name, &records);

    // Collect all constraints observed across all executions for symexpr ratio.
    let all_constraints: Vec<SymConstraint> = observe
        .raw_results
        .iter()
        .flat_map(|(_, result)| {
            result
                .branch_path
                .iter()
                .map(|d| d.constraint.clone())
        })
        .collect();

    let coverage_metrics = CoverageMetrics::from_exploration(
        analysis.branches.len(),
        &observe.discoveries,
        &all_constraints,
    );

    AnalyzeOutput {
        eq_classes,
        behavior_map,
        coverage_metrics,
    }
}

/// Build an [`ExecutionRecord`] from an [`ExecuteResult`] and its inputs.
fn execution_record_from_result(
    function_id: &str,
    inputs: &[serde_json::Value],
    result: &ExecuteResult,
) -> ExecutionRecord {
    let mut hasher = DefaultHasher::new();
    let input_str = serde_json::to_string(inputs).unwrap_or_default();
    input_str.hash(&mut hasher);
    let input_hash = hasher.finish();

    ExecutionRecord {
        function_id: function_id.to_string(),
        input_hash,
        parameters: inputs.to_vec(),
        branch_path: result.branch_path.clone(),
        lines_executed: result.lines_executed.clone(),
        calls_to_external: result.calls_to_external.clone(),
        path_constraints: result.path_constraints.clone(),
        return_value: result.return_value.clone(),
        thrown_error: result.thrown_error.clone(),
        side_effects: result.side_effects.clone(),
        wall_time_ms: result.performance.wall_time_ms,
        cpu_time_us: result.performance.cpu_time_us,
        heap_used_bytes: result.performance.heap_used_bytes,
        heap_allocated_bytes: result.performance.heap_allocated_bytes,
        timestamp: String::new(),
        engine_version: String::new(),
    }
}

/// Adapter type for pipeline composability: wraps either random or concolic
/// exploration output into a common shape that `analyze()` can consume.
///
/// Use `From<ExplorationResult>` for random explorer output or
/// `From<orchestrator::ExploreResult>` for concolic output.
#[derive(Debug)]
pub struct ObservationOutput {
    /// Name of the explored function.
    pub function_name: String,
    /// Total iterations attempted.
    pub iterations: u32,
    /// Number of unique execution paths discovered.
    pub unique_paths: usize,
    /// Number of unique source lines covered across all executions.
    pub lines_covered: usize,
    /// Total source lines in the function.
    pub total_lines: u32,
    /// Summary of each execution that discovered a new path.
    pub new_path_executions: Vec<crate::explorer::ExecutionSummary>,
    /// Raw execution results paired with their inputs.
    pub raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)>,
    /// Per-branch discovery attribution.
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
}

impl From<ExplorationResult> for ObservationOutput {
    fn from(r: ExplorationResult) -> Self {
        Self {
            function_name: r.function_name,
            iterations: r.iterations,
            unique_paths: r.unique_paths,
            lines_covered: r.lines_covered,
            total_lines: r.total_lines,
            new_path_executions: r.new_path_executions,
            raw_results: r.raw_results,
            discoveries: r.discoveries,
        }
    }
}

impl From<crate::orchestrator::ExploreResult> for ObservationOutput {
    fn from(r: crate::orchestrator::ExploreResult) -> Self {
        // Build ExecutionSummary entries from unique-path executions.
        let new_path_executions: Vec<crate::explorer::ExecutionSummary> = r
            .executions
            .iter()
            .map(|exec| crate::explorer::ExecutionSummary {
                inputs: vec![], // Concolic explorer doesn't pair inputs with executions vec
                return_value: exec.return_value.clone(),
                thrown_error: exec
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec.lines_executed.clone(),
                is_new_path: true,
            })
            .collect();

        // Compute lines covered from raw_results.
        let mut all_lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for (_, result) in &r.raw_results {
            for &line in &result.lines_executed {
                all_lines.insert(line);
            }
        }

        Self {
            function_name: String::new(), // Must be set by caller
            iterations: r.total_executions as u32,
            unique_paths: r.unique_paths,
            lines_covered: all_lines.len(),
            total_lines: 0, // Must be set by caller
            new_path_executions,
            raw_results: r.raw_results,
            discoveries: r.discoveries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::explorer::ExplorationResult;
    use crate::protocol::PerformanceMetrics;
    use crate::types::{ParamInfo, TypeInfo};
    use serde_json::json;

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    fn stub_analysis(name: &str, branch_count: usize) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.into(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: (0..branch_count)
                .map(|i| crate::protocol::BranchInfo {
                    id: i as u32,
                    line: (i as u32 + 1) * 10,
                    condition_text: format!("x > {i}"),
                    condition: None,
                    branch_type: crate::protocol::BranchType::If,
                })
                .collect(),
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 10,
            literals: vec![],
        }
    }

    #[test]
    fn analyze_produces_all_outputs() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            },
        ];
        let exec_result = ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path,
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };

        let observe = ExplorationResult {
            function_name: "classify".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], exec_result)],
            discoveries: vec![(0, DiscoveryMethod::Random)],
        };

        let analysis = stub_analysis("classify", 2);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.eq_classes.len(), 1);
        assert_eq!(output.behavior_map.function_id, "classify");
        assert_eq!(output.coverage_metrics.total_branches, 2);
        assert_eq!(output.coverage_metrics.random_found, 1);
        assert_eq!(output.coverage_metrics.uncovered, 1);
        assert_eq!(output.coverage_metrics.unknown_count, 1);
    }

    #[test]
    fn analyze_empty_exploration() {
        let observe = ExplorationResult {
            function_name: "empty".into(),
            iterations: 0,
            unique_paths: 0,
            lines_covered: 0,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
        };

        let analysis = stub_analysis("empty", 3);
        let output = analyze(&observe, &analysis);

        assert!(output.eq_classes.is_empty());
        assert_eq!(output.behavior_map.behaviors.len(), 0);
        assert_eq!(output.coverage_metrics.total_branches, 3);
        assert_eq!(output.coverage_metrics.uncovered, 3);
    }

    #[test]
    fn observation_output_from_exploration_result() {
        let result = ExplorationResult {
            function_name: "test".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![(0, DiscoveryMethod::Random)],
        };

        let output: ObservationOutput = result.into();
        assert_eq!(output.function_name, "test");
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.discoveries.len(), 1);
    }
}
