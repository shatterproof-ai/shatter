//! JSON report generation for scan results.
//!
//! Produces machine-readable JSON output after a scan completes. Contains
//! per-function data (branch coverage, discovered inputs, behavior clusters,
//! constraint stats) and codebase-level aggregates (total functions, overall
//! coverage, unreachable branches, dependency graph summary).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scan_orchestrator::{FunctionResult, ParallelScanResult, ScanResult};

// ---------------------------------------------------------------------------
// Per-function report
// ---------------------------------------------------------------------------

/// A single discovered input and the path it triggered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredInput {
    /// The input values sent to the function.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error message, if the function threw.
    pub thrown_error: Option<String>,
    /// Lines executed during this call.
    pub lines_executed: Vec<u32>,
}

/// Constraint solving statistics for a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintStats {
    /// Total number of path constraints collected.
    pub total_constraints: usize,
    /// Number of solver-guided inputs generated (currently 0 for random-only).
    pub solver_guided_inputs: usize,
}

/// A behavior cluster summary for the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorClusterSummary {
    /// Cluster identifier.
    pub id: u32,
    /// Representative input args.
    pub representative_inputs: Vec<serde_json::Value>,
    /// Representative return value.
    pub return_value: Option<serde_json::Value>,
    /// Error, if this cluster represents a throwing path.
    pub thrown_error: Option<String>,
}

/// Report data for a single function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionReport {
    /// Name of the function.
    pub function_name: String,
    /// Source file path.
    pub file_path: String,
    /// Total branch points in the function.
    pub branch_count: usize,
    /// Number of branches covered (unique paths discovered).
    pub branches_covered: usize,
    /// Coverage percentage (0.0-100.0).
    pub coverage_pct: f64,
    /// Inputs that discovered new execution paths.
    pub discovered_inputs: Vec<DiscoveredInput>,
    /// Behavior cluster summaries.
    pub behavior_clusters: Vec<BehaviorClusterSummary>,
    /// Constraint solving statistics.
    pub constraint_stats: ConstraintStats,
    /// Total iterations attempted.
    pub iterations: u32,
    /// Number of unique source lines covered.
    pub lines_covered: usize,
    /// Total source lines in the function.
    pub total_lines: u32,
    /// Functions mocked during exploration.
    pub mocks_used: Vec<String>,
}

// ---------------------------------------------------------------------------
// Codebase-level report
// ---------------------------------------------------------------------------

/// A dependency edge in the codebase-level summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub caller: String,
    pub callee: String,
}

/// Codebase-level aggregate statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodebaseReport {
    /// Total functions explored.
    pub total_functions: usize,
    /// Total branch points across all functions.
    pub total_branches: usize,
    /// Overall branch coverage percentage (0.0-100.0).
    pub overall_coverage: f64,
    /// Functions that were skipped (timeout, error, etc.).
    pub skipped_functions: Vec<SkippedFunctionReport>,
    /// Dependency graph edges.
    pub dependency_graph: Vec<DependencyEdge>,
}

/// A function that was skipped during the scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedFunctionReport {
    pub function_name: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Top-level report
// ---------------------------------------------------------------------------

/// The complete JSON scan report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// Per-function reports.
    pub functions: Vec<FunctionReport>,
    /// Codebase-level aggregates.
    pub codebase: CodebaseReport,
    /// Test order used during the scan.
    pub test_order: Vec<String>,
}

// ---------------------------------------------------------------------------
// Report generation
// ---------------------------------------------------------------------------

/// Build a [`FunctionReport`] from a scan's [`FunctionResult`].
fn build_function_report(result: &FunctionResult, file_path: &str) -> FunctionReport {
    let exploration = &result.exploration;

    let discovered_inputs: Vec<DiscoveredInput> = exploration
        .new_path_executions
        .iter()
        .map(|exec| DiscoveredInput {
            inputs: exec.inputs.clone(),
            return_value: exec.return_value.clone(),
            thrown_error: exec.thrown_error.clone(),
            lines_executed: exec.lines_executed.clone(),
        })
        .collect();

    let behavior_clusters: Vec<BehaviorClusterSummary> = result
        .behavior_map
        .behaviors
        .iter()
        .map(|b| {
            let thrown_error = b.thrown_error.as_ref().map(|e| {
                format!("{}: {}", e.error_type, e.message)
            });
            BehaviorClusterSummary {
                id: b.id,
                representative_inputs: b.input_args.clone(),
                return_value: b.return_value.clone(),
                thrown_error,
            }
        })
        .collect();

    let total_constraints: usize = exploration
        .raw_results
        .iter()
        .map(|(_, r)| r.path_constraints.len())
        .sum();

    let coverage_pct = if exploration.total_lines > 0 {
        (exploration.lines_covered as f64 / exploration.total_lines as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    FunctionReport {
        function_name: result.function_name.clone(),
        file_path: file_path.to_string(),
        branch_count: exploration.unique_paths,
        branches_covered: exploration.unique_paths,
        coverage_pct,
        discovered_inputs,
        behavior_clusters,
        constraint_stats: ConstraintStats {
            total_constraints,
            solver_guided_inputs: 0,
        },
        iterations: exploration.iterations,
        lines_covered: exploration.lines_covered,
        total_lines: exploration.total_lines,
        mocks_used: result.mocks_used.clone(),
    }
}

/// Build dependency edges from the function results (caller -> mocked callee).
fn build_dependency_edges(function_results: &[FunctionResult]) -> Vec<DependencyEdge> {
    let mut edges = Vec::new();
    for result in function_results {
        for mock in &result.mocks_used {
            edges.push(DependencyEdge {
                caller: result.function_name.clone(),
                callee: mock.clone(),
            });
        }
    }
    edges
}

/// Generate a [`ScanReport`] from a [`ParallelScanResult`].
///
/// The `file_map` maps function names to their source file paths.
pub fn generate_report(
    result: &ParallelScanResult,
    file_map: &std::collections::HashMap<String, String>,
) -> ScanReport {
    let functions: Vec<FunctionReport> = result
        .function_results
        .iter()
        .map(|fr| {
            let file_path = file_map
                .get(&fr.function_name)
                .map(|s| s.as_str())
                .unwrap_or("");
            build_function_report(fr, file_path)
        })
        .collect();

    let total_branches: usize = functions.iter().map(|f| f.branch_count).sum();
    let total_covered: usize = functions.iter().map(|f| f.branches_covered).sum();
    let overall_coverage = if total_branches > 0 {
        total_covered as f64 / total_branches as f64 * 100.0
    } else {
        0.0
    };

    let skipped_functions: Vec<SkippedFunctionReport> = result
        .skipped
        .iter()
        .map(|s| SkippedFunctionReport {
            function_name: s.function_name.clone(),
            reason: s.reason.clone(),
        })
        .collect();

    let dependency_graph = build_dependency_edges(&result.function_results);

    ScanReport {
        version: 1,
        functions,
        codebase: CodebaseReport {
            total_functions: result.function_results.len(),
            total_branches,
            overall_coverage,
            skipped_functions,
            dependency_graph,
        },
        test_order: result.test_order.clone(),
    }
}

/// Generate a [`ScanReport`] from a sequential [`ScanResult`].
///
/// The `file_map` maps function names to their source file paths.
pub fn generate_report_from_scan(
    result: &ScanResult,
    file_map: &std::collections::HashMap<String, String>,
) -> ScanReport {
    let functions: Vec<FunctionReport> = result
        .function_results
        .iter()
        .map(|fr| {
            let file_path = file_map
                .get(&fr.function_name)
                .map(|s| s.as_str())
                .unwrap_or("");
            build_function_report(fr, file_path)
        })
        .collect();

    let total_branches: usize = functions.iter().map(|f| f.branch_count).sum();
    let total_covered: usize = functions.iter().map(|f| f.branches_covered).sum();
    let overall_coverage = if total_branches > 0 {
        total_covered as f64 / total_branches as f64 * 100.0
    } else {
        0.0
    };

    let dependency_graph = build_dependency_edges(&result.function_results);

    let skipped_functions: Vec<SkippedFunctionReport> = result
        .skipped_functions
        .iter()
        .map(|s| SkippedFunctionReport {
            function_name: s.function_name.clone(),
            reason: s.reason.clone(),
        })
        .collect();

    ScanReport {
        version: 1,
        functions,
        codebase: CodebaseReport {
            total_functions: result.function_results.len(),
            total_branches,
            overall_coverage,
            skipped_functions,
            dependency_graph,
        },
        test_order: result.test_order.clone(),
    }
}

/// Write a [`ScanReport`] as pretty-printed JSON to a directory.
///
/// Creates the output directory if it does not exist. Writes to
/// `<output_dir>/scan-report.json`.
pub fn write_report(report: &ScanReport, output_dir: &Path) -> Result<PathBuf, ReportError> {
    std::fs::create_dir_all(output_dir).map_err(|e| ReportError::Io {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let report_path = output_dir.join("scan-report.json");
    let json = serde_json::to_string_pretty(report).map_err(ReportError::Serialize)?;
    std::fs::write(&report_path, json).map_err(|e| ReportError::Io {
        path: report_path.clone(),
        source: e,
    })?;

    Ok(report_path)
}

/// Errors that can occur during report generation or writing.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("failed to write to {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("JSON serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::{Behavior, BehaviorMap};
    use crate::execution_record::ErrorInfo;
    use crate::explorer::{ExecutionSummary, ExplorationResult};
    use crate::scan_orchestrator::{FunctionResult, ParallelScanResult, SkippedFunction};
    use std::collections::HashMap;

    fn make_function_result(
        name: &str,
        iterations: u32,
        unique_paths: usize,
        lines_covered: usize,
        total_lines: u32,
        mocks: Vec<String>,
    ) -> FunctionResult {
        let new_path_executions: Vec<ExecutionSummary> = (0..unique_paths)
            .map(|i| ExecutionSummary {
                inputs: vec![serde_json::json!(i)],
                return_value: Some(serde_json::json!(i * 10)),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
            })
            .collect();

        let behaviors: Vec<Behavior> = (0..unique_paths)
            .map(|i| Behavior {
                id: i as u32,
                input_args: vec![serde_json::json!(i)],
                return_value: Some(serde_json::json!(i * 10)),
                thrown_error: None,
                branch_path: vec![],
                side_effects: vec![],
                dependency_trace: None,
            })
            .collect();

        FunctionResult {
            function_name: name.to_string(),
            exploration: ExplorationResult {
                function_name: name.to_string(),
                iterations,
                unique_paths,
                lines_covered,
                total_lines,
                new_path_executions,
                raw_results: vec![],
            },
            behavior_map: BehaviorMap {
                function_id: name.to_string(),
                behaviors,
            },
            behavior_coverage: vec![],
            mocks_used: mocks,
        }
    }

    #[test]
    fn generate_report_from_parallel_scan() {
        let parallel_result = ParallelScanResult {
            function_results: vec![
                make_function_result("leaf", 10, 2, 5, 10, vec![]),
                make_function_result("caller", 20, 3, 8, 10, vec!["leaf".to_string()]),
            ],
            test_order: vec!["leaf".into(), "caller".into()],
            skipped: vec![],
            workers_used: 2,
        };

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "src/math.ts".to_string());
        file_map.insert("caller".to_string(), "src/app.ts".to_string());

        let report = generate_report(&parallel_result, &file_map);

        assert_eq!(report.version, 1);
        assert_eq!(report.functions.len(), 2);
        assert_eq!(report.test_order, vec!["leaf", "caller"]);

        // Check leaf function report
        let leaf = &report.functions[0];
        assert_eq!(leaf.function_name, "leaf");
        assert_eq!(leaf.file_path, "src/math.ts");
        assert_eq!(leaf.branches_covered, 2);
        assert_eq!(leaf.iterations, 10);
        assert_eq!(leaf.lines_covered, 5);
        assert_eq!(leaf.total_lines, 10);
        assert_eq!(leaf.discovered_inputs.len(), 2);
        assert_eq!(leaf.behavior_clusters.len(), 2);
        assert!(leaf.mocks_used.is_empty());

        // Check caller function report
        let caller = &report.functions[1];
        assert_eq!(caller.function_name, "caller");
        assert_eq!(caller.file_path, "src/app.ts");
        assert_eq!(caller.mocks_used, vec!["leaf"]);

        // Check codebase report
        assert_eq!(report.codebase.total_functions, 2);
        assert_eq!(report.codebase.total_branches, 5); // 2 + 3
        assert!(report.codebase.skipped_functions.is_empty());

        // Check dependency graph
        assert_eq!(report.codebase.dependency_graph.len(), 1);
        assert_eq!(report.codebase.dependency_graph[0].caller, "caller");
        assert_eq!(report.codebase.dependency_graph[0].callee, "leaf");
    }

    #[test]
    fn generate_report_with_skipped_functions() {
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("good", 5, 1, 3, 5, vec![])],
            test_order: vec!["good".into(), "slow".into()],
            skipped: vec![SkippedFunction {
                function_name: "slow".to_string(),
                reason: "timed out after 30s".to_string(),
            }],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        assert_eq!(report.codebase.skipped_functions.len(), 1);
        assert_eq!(report.codebase.skipped_functions[0].function_name, "slow");
        assert_eq!(
            report.codebase.skipped_functions[0].reason,
            "timed out after 30s"
        );
    }

    #[test]
    fn empty_scan_produces_valid_report() {
        let parallel_result = ParallelScanResult {
            function_results: vec![],
            test_order: vec![],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        assert_eq!(report.version, 1);
        assert!(report.functions.is_empty());
        assert_eq!(report.codebase.total_functions, 0);
        assert_eq!(report.codebase.total_branches, 0);
        assert_eq!(report.codebase.overall_coverage, 0.0);
        assert!(report.codebase.skipped_functions.is_empty());
        assert!(report.codebase.dependency_graph.is_empty());
        assert!(report.test_order.is_empty());
    }

    #[test]
    fn coverage_percentage_calculation() {
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("f", 10, 2, 7, 10, vec![])],
            test_order: vec!["f".into()],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        let func = &report.functions[0];
        assert!((func.coverage_pct - 70.0).abs() < 0.01);
    }

    #[test]
    fn coverage_percentage_zero_total_lines() {
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("f", 10, 1, 0, 0, vec![])],
            test_order: vec!["f".into()],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        assert_eq!(report.functions[0].coverage_pct, 0.0);
    }

    #[test]
    fn json_serialization_round_trip() {
        let parallel_result = ParallelScanResult {
            function_results: vec![
                make_function_result("f1", 10, 2, 5, 10, vec![]),
                make_function_result("f2", 5, 1, 3, 5, vec!["f1".to_string()]),
            ],
            test_order: vec!["f1".into(), "f2".into()],
            skipped: vec![SkippedFunction {
                function_name: "f3".to_string(),
                reason: "error: boom".to_string(),
            }],
            workers_used: 2,
        };

        let mut file_map = HashMap::new();
        file_map.insert("f1".to_string(), "src/a.ts".to_string());
        file_map.insert("f2".to_string(), "src/b.ts".to_string());

        let report = generate_report(&parallel_result, &file_map);
        let json = serde_json::to_string_pretty(&report).expect("serialize");
        let deserialized: ScanReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, deserialized);
    }

    #[test]
    fn report_contains_all_required_fields() {
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("f", 10, 2, 5, 10, vec![])],
            test_order: vec!["f".into()],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);
        let json = serde_json::to_string(&report).expect("serialize");

        // Top-level fields
        assert!(json.contains("\"version\""));
        assert!(json.contains("\"functions\""));
        assert!(json.contains("\"codebase\""));
        assert!(json.contains("\"test_order\""));

        // Function-level fields
        assert!(json.contains("\"function_name\""));
        assert!(json.contains("\"file_path\""));
        assert!(json.contains("\"branch_count\""));
        assert!(json.contains("\"branches_covered\""));
        assert!(json.contains("\"coverage_pct\""));
        assert!(json.contains("\"discovered_inputs\""));
        assert!(json.contains("\"behavior_clusters\""));
        assert!(json.contains("\"constraint_stats\""));
        assert!(json.contains("\"iterations\""));
        assert!(json.contains("\"lines_covered\""));
        assert!(json.contains("\"total_lines\""));
        assert!(json.contains("\"mocks_used\""));

        // Codebase-level fields
        assert!(json.contains("\"total_functions\""));
        assert!(json.contains("\"total_branches\""));
        assert!(json.contains("\"overall_coverage\""));
        assert!(json.contains("\"skipped_functions\""));
        assert!(json.contains("\"dependency_graph\""));
    }

    #[test]
    fn write_report_creates_directory_and_file() {
        let report = ScanReport {
            version: 1,
            functions: vec![],
            codebase: CodebaseReport {
                total_functions: 0,
                total_branches: 0,
                overall_coverage: 0.0,
                skipped_functions: vec![],
                dependency_graph: vec![],
            },
            test_order: vec![],
        };

        let dir = std::env::temp_dir().join("shatter-report-test");
        // Clean up from previous runs
        let _ = std::fs::remove_dir_all(&dir);

        let path = write_report(&report, &dir).expect("write_report should succeed");
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "scan-report.json");

        // Read back and verify
        let contents = std::fs::read_to_string(&path).expect("read file");
        let deserialized: ScanReport =
            serde_json::from_str(&contents).expect("parse json");
        assert_eq!(deserialized.version, 1);

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn function_report_with_errors() {
        let mut func_result = make_function_result("risky", 5, 1, 3, 5, vec![]);
        // Add an error behavior
        func_result.behavior_map.behaviors.push(Behavior {
            id: 1,
            input_args: vec![serde_json::json!(null)],
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "TypeError".to_string(),
                message: "cannot read null".to_string(),
                stack: None,
            }),
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
        });
        func_result
            .exploration
            .new_path_executions
            .push(ExecutionSummary {
                inputs: vec![serde_json::json!(null)],
                return_value: None,
                thrown_error: Some("TypeError: cannot read null".to_string()),
                lines_executed: vec![1],
                is_new_path: true,
            });

        let parallel_result = ParallelScanResult {
            function_results: vec![func_result],
            test_order: vec!["risky".into()],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        let func = &report.functions[0];
        assert_eq!(func.behavior_clusters.len(), 2);

        let error_cluster = &func.behavior_clusters[1];
        assert!(error_cluster.thrown_error.is_some());
        assert!(error_cluster
            .thrown_error
            .as_ref()
            .unwrap()
            .contains("TypeError"));

        let error_input = func
            .discovered_inputs
            .iter()
            .find(|d| d.thrown_error.is_some());
        assert!(error_input.is_some());
    }

    #[test]
    fn generate_report_from_sequential_scan() {
        let scan_result = ScanResult {
            function_results: vec![
                make_function_result("a", 5, 1, 3, 5, vec![]),
                make_function_result("b", 10, 2, 7, 10, vec!["a".to_string()]),
            ],
            test_order: vec!["a".into(), "b".into()],
            skipped_functions: vec![],
        };

        let mut file_map = HashMap::new();
        file_map.insert("a".to_string(), "src/a.ts".to_string());

        let report = generate_report_from_scan(&scan_result, &file_map);

        assert_eq!(report.version, 1);
        assert_eq!(report.functions.len(), 2);
        assert_eq!(report.functions[0].file_path, "src/a.ts");
        assert_eq!(report.functions[1].file_path, ""); // not in file_map
        assert!(report.codebase.skipped_functions.is_empty());
        assert_eq!(report.codebase.dependency_graph.len(), 1);
    }

    #[test]
    fn overall_coverage_computed_correctly() {
        // Two functions: one with 2 branches, one with 3 branches = 5 total
        // Both fully covered => 100%
        let parallel_result = ParallelScanResult {
            function_results: vec![
                make_function_result("a", 10, 2, 5, 10, vec![]),
                make_function_result("b", 10, 3, 8, 10, vec![]),
            ],
            test_order: vec!["a".into(), "b".into()],
            skipped: vec![],
            workers_used: 1,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map);

        // branches = 2 + 3 = 5, covered = 2 + 3 = 5 => 100%
        assert!((report.codebase.overall_coverage - 100.0).abs() < 0.01);
    }
}
