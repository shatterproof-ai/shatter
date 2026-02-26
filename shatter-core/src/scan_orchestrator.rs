//! Scan orchestrator: multi-function exploration in dependency order.
//!
//! When function A calls function B, testing B first lets us record its
//! behavior map and use it as a high-fidelity mock when testing A.
//! The scan orchestrator builds a [`CallGraph`], computes a test order
//! (leaves first), and drives [`explore_function`] for each function
//! with appropriate mocks.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::behavior::{BehaviorCoverage, BehaviorMap, CallGraph, CallGraphError};
use crate::execution_record::ExecutionRecord;
use crate::explorer::{self, ExploreConfig, ExploreError, ExplorationResult};
use crate::frontend::Frontend;
use crate::protocol::{ExecuteResult, FunctionAnalysis, MockConfig};

/// Configuration for a scan run.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Maximum number of iterations (execute calls) per function.
    pub max_iterations_per_function: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
}

/// Result of exploring a single function during a scan.
#[derive(Debug)]
pub struct FunctionResult {
    /// Name of the explored function.
    pub function_name: String,
    /// The exploration result (paths, coverage, etc.).
    pub exploration: ExplorationResult,
    /// Behavior map built from execution results.
    pub behavior_map: BehaviorMap,
    /// Coverage of callee behaviors exercised by this function.
    pub behavior_coverage: Vec<BehaviorCoverage>,
    /// Names of functions that were mocked during exploration.
    pub mocks_used: Vec<String>,
}

/// Result of a full scan across multiple functions.
#[derive(Debug)]
pub struct ScanResult {
    /// Per-function results in test order.
    pub function_results: Vec<FunctionResult>,
    /// The order in which functions were tested.
    pub test_order: Vec<String>,
}

/// Errors that can occur during a scan.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("exploration error: {0}")]
    Explore(#[from] ExploreError),
    #[error("call graph cycle detected: {0}")]
    Cycle(#[from] CallGraphError),
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

/// Run a multi-function scan in dependency order.
///
/// Builds a call graph from the analyses, determines test order (leaves first),
/// then explores each function. Callees that have already been tested provide
/// mock configurations derived from their behavior maps.
pub async fn scan(
    frontend: &mut Frontend,
    analyses: &[FunctionAnalysis],
    config: &ScanConfig,
) -> Result<ScanResult, ScanError> {
    let call_graph = CallGraph::from_analyses(analyses);
    let test_order = call_graph.test_order()?;

    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    let mut behavior_maps: HashMap<String, BehaviorMap> = HashMap::new();
    let mut function_results: Vec<FunctionResult> = Vec::new();

    for func_name in &test_order {
        let analysis = match analysis_map.get(func_name.as_str()) {
            Some(a) => *a,
            None => continue,
        };

        // Build mocks from callees that have already been tested.
        let callees = call_graph.callees(func_name);
        let mut mocks: Vec<MockConfig> = Vec::new();
        let mut mocks_used: Vec<String> = Vec::new();

        for callee in &callees {
            if let Some(bmap) = behavior_maps.get(callee) {
                mocks.push(bmap.to_mock_config());
                mocks_used.push(callee.clone());
            }
        }
        mocks_used.sort();

        let explore_config = ExploreConfig {
            max_iterations: config.max_iterations_per_function,
            seed: config.seed,
            mocks,
        };

        let exploration = explorer::explore_function(frontend, analysis, &explore_config).await?;

        // Build ExecutionRecords from raw results for BehaviorMap construction.
        let records: Vec<ExecutionRecord> = exploration
            .raw_results
            .iter()
            .map(|(inputs, result)| execution_record_from_result(func_name, inputs, result))
            .collect();

        let behavior_map = BehaviorMap::from_records(func_name, &records);

        // Compute behavior coverage for each callee.
        let mut behavior_coverage: Vec<BehaviorCoverage> = Vec::new();
        for callee in &callees {
            if let Some(callee_map) = behavior_maps.get(callee) {
                let coverage = BehaviorCoverage::compute(func_name, &records, callee_map);
                behavior_coverage.push(coverage);
            }
        }

        behavior_maps.insert(func_name.clone(), behavior_map.clone());

        function_results.push(FunctionResult {
            function_name: func_name.clone(),
            exploration,
            behavior_map,
            behavior_coverage,
            mocks_used,
        });
    }

    Ok(ScanResult {
        function_results,
        test_order,
    })
}

/// Format a scan result as a human-readable report.
pub fn format_scan_report(result: &ScanResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested\n",
        result.function_results.len()
    ));

    out.push_str("\nTest order: ");
    out.push_str(&result.test_order.join(" → "));
    out.push('\n');

    for func_result in &result.function_results {
        out.push_str(&format!("\n── {} ──\n", func_result.function_name));

        out.push_str(&explorer::format_exploration_report(&func_result.exploration));

        if !func_result.mocks_used.is_empty() {
            out.push_str(&format!(
                "  Mocks used: {}\n",
                func_result.mocks_used.join(", ")
            ));
        }

        for cov in &func_result.behavior_coverage {
            let exercised = cov.exercised_behavior_ids.len();
            let total = cov.total_behaviors;
            let pct = if total > 0 {
                (exercised as f64 / total as f64 * 100.0).round()
            } else {
                0.0
            };
            out.push_str(&format!(
                "  Behavior coverage of {}: {}/{} ({pct:.0}%)\n",
                cov.callee, exercised, total
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        DependencyKind, ExecuteResult, ExternalDependency, PerformanceMetrics,
    };
    use crate::types::TypeInfo;

    fn make_analysis(name: &str, deps: Vec<&str>) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            params: vec![],
            branches: vec![],
            dependencies: deps
                .into_iter()
                .map(|d| ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: d.to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                })
                .collect(),
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
        }
    }

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    #[test]
    fn execution_record_from_result_builds_correctly() {
        let exec_result = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };

        let inputs = vec![serde_json::json!(10)];
        let record = execution_record_from_result("myFunc", &inputs, &exec_result);

        assert_eq!(record.function_id, "myFunc");
        assert_eq!(record.parameters, inputs);
        assert_eq!(record.return_value, Some(serde_json::json!(42)));
        assert_eq!(record.lines_executed, vec![1, 2, 3]);
    }

    #[test]
    fn execution_record_from_result_hashes_inputs_consistently() {
        let exec_result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };

        let inputs = vec![serde_json::json!(1), serde_json::json!("hello")];
        let r1 = execution_record_from_result("f", &inputs, &exec_result);
        let r2 = execution_record_from_result("f", &inputs, &exec_result);
        assert_eq!(r1.input_hash, r2.input_hash);

        let different_inputs = vec![serde_json::json!(2)];
        let r3 = execution_record_from_result("f", &different_inputs, &exec_result);
        assert_ne!(r1.input_hash, r3.input_hash);
    }

    #[test]
    fn scan_cycle_returns_error() {
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec!["a"]),
        ];
        let call_graph = CallGraph::from_analyses(&analyses);
        let result = call_graph.test_order();
        assert!(result.is_err());
    }

    #[test]
    fn format_scan_report_shows_test_order() {
        let result = ScanResult {
            test_order: vec!["leaf".into(), "caller".into()],
            function_results: vec![
                FunctionResult {
                    function_name: "leaf".into(),
                    exploration: ExplorationResult {
                        function_name: "leaf".into(),
                        iterations: 5,
                        unique_paths: 2,
                        lines_covered: 3,
                        total_lines: 5,
                        new_path_executions: vec![],
                        raw_results: vec![],
                    },
                    behavior_map: BehaviorMap {
                        function_id: "leaf".into(),
                        behaviors: vec![],
                    },
                    behavior_coverage: vec![],
                    mocks_used: vec![],
                },
                FunctionResult {
                    function_name: "caller".into(),
                    exploration: ExplorationResult {
                        function_name: "caller".into(),
                        iterations: 10,
                        unique_paths: 3,
                        lines_covered: 8,
                        total_lines: 10,
                        new_path_executions: vec![],
                        raw_results: vec![],
                    },
                    behavior_map: BehaviorMap {
                        function_id: "caller".into(),
                        behaviors: vec![],
                    },
                    behavior_coverage: vec![BehaviorCoverage {
                        caller: "caller".into(),
                        callee: "leaf".into(),
                        exercised_behavior_ids: vec![0, 1],
                        total_behaviors: 3,
                    }],
                    mocks_used: vec!["leaf".into()],
                },
            ],
        };

        let report = format_scan_report(&result);
        assert!(report.contains("2 function(s) tested"));
        assert!(report.contains("leaf → caller"));
        assert!(report.contains("Mocks used: leaf"));
        assert!(report.contains("Behavior coverage of leaf: 2/3"));
    }

    #[test]
    fn format_scan_report_single_function_no_deps() {
        let result = ScanResult {
            test_order: vec!["standalone".into()],
            function_results: vec![FunctionResult {
                function_name: "standalone".into(),
                exploration: ExplorationResult {
                    function_name: "standalone".into(),
                    iterations: 10,
                    unique_paths: 1,
                    lines_covered: 5,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![],
                },
                behavior_map: BehaviorMap {
                    function_id: "standalone".into(),
                    behaviors: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
            }],
        };

        let report = format_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(!report.contains("Mocks used"));
        assert!(!report.contains("Behavior coverage"));
    }
}
