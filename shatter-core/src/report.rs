//! JSON report generation for scan results.
//!
//! Produces machine-readable JSON output after a scan completes. Contains
//! per-function data (branch coverage, discovered inputs, behavior clusters,
//! constraint stats) and codebase-level aggregates (total functions, overall
//! coverage, unreachable branches, dependency graph summary).

use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::batch_state::BatchState;
use crate::coverage_metrics::CoverageMetrics;
use crate::explorer::ObservationOutput;
use crate::protocol::OutcomeStatus;
use crate::scan_orchestrator::{FunctionResult, MockSource, ParallelScanResult, ScanResult};

/// Maximum number of behavior clusters displayed per function in the Markdown
/// report. Functions with more clusters show a "... and N more" summary line.
const MAX_DISPLAY_CLUSTERS: usize = 5;

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

/// Mock usage details for a single mocked dependency in the scan report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockUsageReport {
    /// Symbol name of the mocked dependency.
    pub name: String,
    /// How the mock was sourced: "behavior_map", "type_stub", or "stratum_excluded".
    pub source: String,
    /// Fraction of the callee's behaviors exercised by the caller (0.0-1.0).
    /// Present only for behavior-map-backed mocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_coverage_pct: Option<f64>,
    /// Number of concrete executions that informed the mock's behavior map.
    /// Present only for behavior-map-backed mocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_execution_count: Option<u64>,
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
    /// Functions mocked during exploration, with quality metrics.
    pub mocks_used: Vec<MockUsageReport>,
    /// Refactoring recommendations for hard-to-mock dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refactoring_recommendations: Vec<crate::mock_analysis::RefactoringRecommendation>,
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
    pub category: String,
}

// ---------------------------------------------------------------------------
// Top-level report
// ---------------------------------------------------------------------------

/// Cumulative progress across progressive batch runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CumulativeReport {
    /// Batch indices that have been completed.
    pub completed_batches: Vec<usize>,
    /// Total functions explored across all batches.
    pub total_functions_explored: usize,
    /// Total functions in the scan scope.
    pub total_scope_functions: usize,
    /// Cumulative coverage metrics merged across all batches.
    pub metrics: CoverageMetrics,
    /// Overall cumulative coverage percentage.
    pub cumulative_coverage_pct: f64,
}

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
    /// Cumulative stats across all batches (present only in batch mode).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cumulative: Option<CumulativeReport>,
}

// ---------------------------------------------------------------------------
// Report generation
// ---------------------------------------------------------------------------

/// Build a [`FunctionReport`] from a scan's [`FunctionResult`].
pub(crate) fn build_function_report(result: &FunctionResult, file_path: &str) -> FunctionReport {
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
            let thrown_error = b
                .thrown_error
                .as_ref()
                .map(|e| format!("{}: {}", e.error_type, e.message));
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
        .map(|(_, _mocks, r)| r.path_constraints.len())
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
        mocks_used: result
            .mocks_used
            .iter()
            .map(|m| {
                let (mock_coverage_pct, mock_execution_count) = match m.source {
                    MockSource::CachedBehaviorMap => {
                        // Look up BehaviorCoverage for this callee to get metrics.
                        let coverage = result
                            .behavior_coverage
                            .iter()
                            .find(|bc| bc.callee == m.name);
                        match coverage {
                            Some(bc) => {
                                let pct = if bc.total_behaviors > 0 {
                                    bc.exercised_behavior_ids.len() as f64
                                        / bc.total_behaviors as f64
                                } else {
                                    0.0
                                };
                                (Some(pct), Some(bc.total_behaviors as u64))
                            }
                            // CachedBehaviorMap but no coverage data (shouldn't happen,
                            // but degrade gracefully).
                            None => (Some(0.0), Some(0)),
                        }
                    }
                    MockSource::TypeAwareStub | MockSource::StratumExcluded => (None, None),
                };
                MockUsageReport {
                    name: m.name.clone(),
                    source: match m.source {
                        MockSource::CachedBehaviorMap => "behavior_map".to_string(),
                        MockSource::TypeAwareStub => "type_stub".to_string(),
                        MockSource::StratumExcluded => "stratum_excluded".to_string(),
                    },
                    mock_coverage_pct,
                    mock_execution_count,
                }
            })
            .collect(),
        refactoring_recommendations: result.refactoring_recommendations.clone(),
    }
}

/// Build dependency edges from the function results (caller -> mocked callee).
fn build_dependency_edges(function_results: &[FunctionResult]) -> Vec<DependencyEdge> {
    let mut edges = Vec::new();
    for result in function_results {
        for mock in &result.mocks_used {
            edges.push(DependencyEdge {
                caller: result.function_name.clone(),
                callee: mock.name.clone(),
            });
        }
    }
    edges
}

/// Build a [`CumulativeReport`] from batch state.
fn build_cumulative_report(state: &BatchState) -> CumulativeReport {
    CumulativeReport {
        completed_batches: state.completed_batches(),
        total_functions_explored: state.total_functions_explored(),
        total_scope_functions: state.total_scope_functions,
        metrics: state.cumulative_metrics.clone(),
        cumulative_coverage_pct: state.cumulative_coverage_pct(),
    }
}

/// Generate a [`ScanReport`] from a [`ParallelScanResult`].
///
/// The `file_map` maps function names to their source file paths.
/// When `batch_state` is provided, the report includes cumulative progress
/// across all completed batches.
pub fn generate_report(
    result: &ParallelScanResult,
    file_map: &std::collections::HashMap<String, String>,
    batch_state: Option<&BatchState>,
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
            category: match s.category {
                crate::scan_orchestrator::SkipCategory::Expected => "expected".into(),
                crate::scan_orchestrator::SkipCategory::Error => "error".into(),
            },
        })
        .collect();

    let dependency_graph = build_dependency_edges(&result.function_results);

    let cumulative = batch_state.map(build_cumulative_report);

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
        cumulative,
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
            category: match s.category {
                crate::scan_orchestrator::SkipCategory::Expected => "expected".into(),
                crate::scan_orchestrator::SkipCategory::Error => "error".into(),
            },
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
        cumulative: None,
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

/// Write a [`ScanReport`] as a markdown file to a directory.
///
/// Creates the output directory if it does not exist. Writes to
/// `<output_dir>/scan-report.md`.
pub fn write_markdown_report(
    report: &ScanReport,
    output_dir: &Path,
) -> Result<PathBuf, ReportError> {
    std::fs::create_dir_all(output_dir).map_err(|e| ReportError::Io {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let report_path = output_dir.join("scan-report.md");
    let markdown = format_markdown_report(report);
    std::fs::write(&report_path, markdown).map_err(|e| ReportError::Io {
        path: report_path.clone(),
        source: e,
    })?;

    Ok(report_path)
}

// ---------------------------------------------------------------------------
// HTML report generation
// ---------------------------------------------------------------------------

/// Render the HTML section for a single explored function.
///
/// `project_root` is used to resolve relative source file paths so the source
/// code block can be populated. Pass `None` to skip source code display.
///
/// Returns an HTML fragment (a `<details>` block) ready to embed in a full page.
#[must_use]
pub fn render_explore_fn_html(
    result: &ObservationOutput,
    location: &str,
    project_root: Option<&std::path::Path>,
) -> String {
    crate::html_templates::render_explore_fn(result, location, project_root)
}

/// Wrap exploration HTML fragments into a complete, self-contained HTML page.
///
/// `fragments` is a slice of `<details>` blocks produced by [`render_explore_fn_html`].
#[must_use]
pub fn wrap_explore_html(
    fragments: &[String],
    fn_count: usize,
    total_paths: usize,
    total_covered: usize,
    total_lines: u32,
) -> String {
    crate::html_templates::render_explore_page(
        fragments,
        fn_count,
        total_paths,
        total_covered,
        total_lines,
    )
}

/// Generate a self-contained HTML report for a [`ScanReport`].
///
/// `project_root` is used to resolve relative source file paths for the source
/// code display. Pass `None` to skip source code display.
#[must_use]
pub fn generate_html_scan_report(report: &ScanReport, project_root: Option<&Path>) -> String {
    crate::html_templates::render_scan_report(report, project_root)
}

/// Write a self-contained HTML scan report to a directory.
///
/// Creates the output directory if it does not exist. Writes to
/// `<output_dir>/scan-report.html`.
///
/// `project_root` is forwarded to [`generate_html_scan_report`] for source
/// code display. Pass `None` to skip source code display.
pub fn write_html_report(
    report: &ScanReport,
    output_dir: &Path,
    project_root: Option<&Path>,
) -> Result<PathBuf, ReportError> {
    std::fs::create_dir_all(output_dir).map_err(|e| ReportError::Io {
        path: output_dir.to_path_buf(),
        source: e,
    })?;

    let report_path = output_dir.join("scan-report.html");
    let html = generate_html_scan_report(report, project_root);
    std::fs::write(&report_path, html).map_err(|e| ReportError::Io {
        path: report_path.clone(),
        source: e,
    })?;

    Ok(report_path)
}

// ---------------------------------------------------------------------------
// Markdown report generation
// ---------------------------------------------------------------------------

/// Format a [`ScanReport`] as a human-readable markdown string.
#[must_use]
pub fn format_markdown_report(report: &ScanReport) -> String {
    let mut out = String::new();

    write_md_header(&mut out, report);
    write_md_cumulative(&mut out, &report.cumulative);
    write_md_summary_table(&mut out, report);
    write_md_function_details(&mut out, &report.functions);
    write_md_uncovered_branches(&mut out, &report.functions);
    write_md_interesting_inputs(&mut out, &report.functions);
    write_md_skipped_functions(&mut out, &report.codebase.skipped_functions);

    out
}

/// Format a [`ScanReport`] as plain text (markdown with formatting stripped).
#[must_use]
pub fn format_text_report(report: &ScanReport) -> String {
    let md = format_markdown_report(report);
    strip_markdown_text(&md)
}

/// Strip markdown formatting syntax, returning plain text.
///
/// Removes heading markers, bold/italic markers, inline code backticks,
/// table separator lines, and table cell delimiters.
pub fn strip_markdown_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        // Strip heading markers
        let line = line.trim_start_matches('#').trim_start();
        // Strip bold/italic markers
        let line = line.replace("**", "").replace('*', "");
        // Strip inline code backticks
        let line = line.replace('`', "");
        // Skip table separator lines (e.g. |---|---|)
        if line.chars().all(|c| matches!(c, '-' | '|' | ' ' | ':')) && line.contains('|') {
            continue;
        }
        // Strip table cell delimiters: | col | col | → col  col
        let line = if line.contains('|') {
            line.split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("  ")
        } else {
            line.to_string()
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn write_md_header(out: &mut String, report: &ScanReport) {
    let _ = writeln!(out, "# Shatter Scan Report\n");

    let total_covered: usize = report.functions.iter().map(|f| f.branches_covered).sum();
    let total_branches = report.codebase.total_branches;
    let coverage = report.codebase.overall_coverage;

    let _ = writeln!(
        out,
        "- **Functions explored:** {}",
        report.codebase.total_functions
    );
    let _ = writeln!(out, "- **Total branches:** {total_branches}");
    let _ = writeln!(out, "- **Branches covered:** {total_covered}");
    let _ = writeln!(out, "- **Overall coverage:** {coverage:.1}%");

    if !report.codebase.skipped_functions.is_empty() {
        let _ = writeln!(
            out,
            "- **Skipped functions:** {}",
            report.codebase.skipped_functions.len()
        );
    }

    out.push('\n');
}

fn write_md_cumulative(out: &mut String, cumulative: &Option<CumulativeReport>) {
    let Some(cum) = cumulative else {
        return;
    };

    let _ = writeln!(out, "## Cumulative Progress\n");
    let batches_str = cum
        .completed_batches
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "- **Batches completed:** {} ({})",
        cum.completed_batches.len(),
        batches_str,
    );
    let _ = writeln!(
        out,
        "- **Functions explored:** {}/{}",
        cum.total_functions_explored, cum.total_scope_functions,
    );
    let covered = cum.metrics.z3_solved + cum.metrics.random_found + cum.metrics.user_provided;
    let _ = writeln!(
        out,
        "- **Branches covered:** {}/{}",
        covered, cum.metrics.total_branches,
    );
    let _ = writeln!(
        out,
        "- **Cumulative coverage:** {:.1}%",
        cum.cumulative_coverage_pct,
    );
    out.push('\n');
}

fn write_md_summary_table(out: &mut String, report: &ScanReport) {
    if report.functions.is_empty() {
        let _ = writeln!(out, "*No functions were explored.*\n");
        return;
    }

    let _ = writeln!(out, "## Function Summary\n");
    let _ = writeln!(
        out,
        "| Status | Function | File | Coverage | Branches | Lines | Iterations |"
    );
    let _ = writeln!(
        out,
        "|--------|----------|------|----------|----------|-------|------------|"
    );

    for func in &report.functions {
        let status = if func.coverage_pct >= 100.0 {
            "PASS"
        } else if func.coverage_pct >= 50.0 {
            "WARN"
        } else {
            "FAIL"
        };

        let _ = writeln!(
            out,
            "| {status} | `{name}` | {file} | {cov:.1}% | {covered}/{total} | {lc}/{tl} | {iter} |",
            name = func.function_name,
            file = if func.file_path.is_empty() {
                "-"
            } else {
                &func.file_path
            },
            cov = func.coverage_pct,
            covered = func.branches_covered,
            total = func.branch_count,
            lc = func.lines_covered,
            tl = func.total_lines,
            iter = func.iterations,
        );
    }

    out.push('\n');
}

fn write_md_function_details(out: &mut String, functions: &[FunctionReport]) {
    if functions.is_empty() {
        return;
    }

    let _ = writeln!(out, "## Function Details\n");

    for func in functions {
        let _ = writeln!(out, "### `{}`\n", func.function_name);

        if !func.file_path.is_empty() {
            let _ = writeln!(out, "- **File:** {}", func.file_path);
        }
        let _ = writeln!(out, "- **Coverage:** {:.1}%", func.coverage_pct);
        let _ = writeln!(
            out,
            "- **Branches:** {}/{}",
            func.branches_covered, func.branch_count
        );
        let _ = writeln!(
            out,
            "- **Lines:** {}/{}",
            func.lines_covered, func.total_lines
        );
        let _ = writeln!(out, "- **Iterations:** {}", func.iterations);
        let _ = writeln!(
            out,
            "- **Constraints collected:** {}",
            func.constraint_stats.total_constraints
        );

        if !func.mocks_used.is_empty() {
            let mock_names: Vec<&str> = func.mocks_used.iter().map(|m| m.name.as_str()).collect();
            let _ = writeln!(out, "- **Mocks:** {}", mock_names.join(", "));
        }

        if !func.behavior_clusters.is_empty() {
            let _ = writeln!(out, "\n**Behaviors:**\n");
            let total = func.behavior_clusters.len();
            let display = total.min(MAX_DISPLAY_CLUSTERS);
            for cluster in &func.behavior_clusters[..display] {
                let outcome = if let Some(ref err) = cluster.thrown_error {
                    format!("throws {err}")
                } else if let Some(ref val) = cluster.return_value {
                    format!("returns {}", format_json_compact(val))
                } else {
                    "returns void".to_string()
                };
                let inputs = format_json_compact_list(&cluster.representative_inputs);
                let _ = writeln!(
                    out,
                    "- Cluster {}: {outcome} (inputs: {inputs})",
                    cluster.id
                );
            }
            if total > MAX_DISPLAY_CLUSTERS {
                let _ = writeln!(
                    out,
                    "- ... and {} more clusters",
                    total - MAX_DISPLAY_CLUSTERS
                );
            }
        }

        if !func.refactoring_recommendations.is_empty() {
            let _ = writeln!(out, "\n**Refactoring Recommendations:**\n");
            for rec in &func.refactoring_recommendations {
                let location = rec.line.map(|l| format!(" (line {l})")).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "- `{sym}`{loc}: {reason}. {suggestion}.",
                    sym = rec.symbol,
                    loc = location,
                    reason = rec.reason,
                    suggestion = rec.suggestion,
                );
            }
        }

        out.push('\n');
    }
}

fn write_md_uncovered_branches(out: &mut String, functions: &[FunctionReport]) {
    let low_coverage: Vec<&FunctionReport> = functions
        .iter()
        .filter(|f| f.coverage_pct < 100.0 && f.branch_count > 0)
        .collect();

    if low_coverage.is_empty() {
        return;
    }

    let _ = writeln!(out, "## Uncovered Branches\n");

    for func in &low_coverage {
        let uncovered = func.branch_count.saturating_sub(func.branches_covered);
        let _ = writeln!(
            out,
            "- `{}`: {uncovered} uncovered branch(es) ({:.1}% coverage)",
            func.function_name, func.coverage_pct
        );
    }

    out.push('\n');
}

fn write_md_interesting_inputs(out: &mut String, functions: &[FunctionReport]) {
    let has_interesting = functions.iter().any(|f| {
        f.discovered_inputs
            .iter()
            .any(|d| d.thrown_error.is_some() || is_boundary_value(&d.inputs))
    });

    if !has_interesting {
        return;
    }

    let _ = writeln!(out, "## Interesting Inputs\n");

    for func in functions {
        let interesting: Vec<&DiscoveredInput> = func
            .discovered_inputs
            .iter()
            .filter(|d| d.thrown_error.is_some() || is_boundary_value(&d.inputs))
            .collect();

        if interesting.is_empty() {
            continue;
        }

        let _ = writeln!(out, "### `{}`\n", func.function_name);

        for input in &interesting {
            let inputs_str = format_json_compact_list(&input.inputs);
            if let Some(ref err) = input.thrown_error {
                let _ = writeln!(out, "- {inputs_str} -> **error:** {err}");
            } else if let Some(ref val) = input.return_value {
                let _ = writeln!(out, "- {inputs_str} -> {}", format_json_compact(val));
            } else {
                let _ = writeln!(out, "- {inputs_str} -> void");
            }
        }

        out.push('\n');
    }
}

fn write_md_skipped_functions(out: &mut String, skipped: &[SkippedFunctionReport]) {
    if skipped.is_empty() {
        return;
    }

    let expected: Vec<_> = skipped
        .iter()
        .filter(|s| s.category == "expected")
        .collect();
    let errors: Vec<_> = skipped.iter().filter(|s| s.category == "error").collect();

    if !expected.is_empty() {
        let _ = writeln!(out, "## Skipped (Expected)\n");
        for s in &expected {
            let _ = writeln!(out, "- `{}`: {}", s.function_name, s.reason);
        }
        out.push('\n');
    }

    if !errors.is_empty() {
        let _ = writeln!(out, "## Errors\n");
        for s in &errors {
            let _ = writeln!(out, "- `{}`: {}", s.function_name, s.reason);
        }
        out.push('\n');
    }
}

fn is_boundary_value(inputs: &[serde_json::Value]) -> bool {
    inputs.iter().any(|v| match v {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                f == 0.0 || f == -1.0
            } else {
                false
            }
        }
        serde_json::Value::String(s) => s.is_empty(),
        serde_json::Value::Null => true,
        serde_json::Value::Array(a) => a.is_empty(),
        _ => false,
    })
}

fn format_json_compact(value: &serde_json::Value) -> String {
    let s = value.to_string();
    if s.len() > 60 {
        format!("{}...", &s[..57])
    } else {
        s
    }
}

fn format_json_compact_list(values: &[serde_json::Value]) -> String {
    let parts: Vec<String> = values.iter().map(format_json_compact).collect();
    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Progress reporting
// ---------------------------------------------------------------------------

/// A structured progress event for machine-readable output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Event type — always "progress".
    #[serde(rename = "type")]
    pub event_type: String,
    /// Optional progress status such as started, completed, skipped, or failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Name of the function currently being processed.
    pub function: String,
    /// 1-based index of the current function.
    pub current: usize,
    /// Total number of functions to process.
    pub total: usize,
    /// Milliseconds elapsed since the scan started.
    pub elapsed_ms: u64,
    /// Cumulative distinct branches covered for this function so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches_covered: Option<usize>,
    /// Total branches reported by static analysis for this function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches_total: Option<usize>,
    /// Total MC/DC conditions tracked, when MC/DC coverage is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcdc_total: Option<usize>,
    /// Independent MC/DC conditions satisfied so far, when MC/DC is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcdc_independent: Option<usize>,
    /// Iterations without a new branch discovery. Non-zero values signal the
    /// function is continuing to run without surfacing new coverage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iters_since_new_discovery: Option<u32>,
}

impl ProgressEvent {
    /// Create a new progress event.
    #[must_use]
    pub fn new(function: &str, current: usize, total: usize, elapsed_ms: u64) -> Self {
        Self {
            event_type: "progress".to_string(),
            status: None,
            function: function.to_string(),
            current,
            total,
            elapsed_ms,
            branches_covered: None,
            branches_total: None,
            mcdc_total: None,
            mcdc_independent: None,
            iters_since_new_discovery: None,
        }
    }

    /// Create a progress event with an explicit status.
    #[must_use]
    pub fn with_status(
        function: &str,
        current: usize,
        total: usize,
        elapsed_ms: u64,
        status: impl Into<String>,
    ) -> Self {
        Self {
            status: Some(status.into()),
            ..Self::new(function, current, total, elapsed_ms)
        }
    }

    /// Attach cumulative branch coverage counts to this event.
    #[must_use]
    pub fn with_branch_coverage(mut self, covered: usize, total: usize) -> Self {
        self.branches_covered = Some(covered);
        self.branches_total = Some(total);
        self
    }

    /// Attach an MC/DC summary `(total_conditions, independent_conditions)` to
    /// this event. Callers pass the pair from
    /// [`crate::mcdc::McdcTable::summary`].
    #[must_use]
    pub fn with_mcdc(mut self, mcdc_total: usize, mcdc_independent: usize) -> Self {
        self.mcdc_total = Some(mcdc_total);
        self.mcdc_independent = Some(mcdc_independent);
        self
    }

    /// Attach an "iterations without new discovery" counter.
    #[must_use]
    pub fn with_idle_iters(mut self, iters: u32) -> Self {
        self.iters_since_new_discovery = Some(iters);
        self
    }

    /// Serialize this event as a JSON string.
    ///
    /// Returns `None` if serialization fails (should not happen for valid data).
    #[must_use]
    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

/// Report format for scan output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// JSON only (default).
    Json,
    /// Markdown only.
    Markdown,
    /// Both JSON and Markdown.
    Both,
    /// Self-contained HTML report.
    Html,
}

impl std::str::FromStr for ReportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "markdown" | "md" => Ok(Self::Markdown),
            "both" => Ok(Self::Both),
            "html" => Ok(Self::Html),
            _ => Err(format!(
                "unknown report format '{s}': expected 'json', 'markdown', 'both', or 'html'"
            )),
        }
    }
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
    use crate::explorer::{ExecutionSummary, ObservationOutput};
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
        use crate::scan_orchestrator::{MockSource, MockUsage};
        let mocks: Vec<MockUsage> = mocks
            .into_iter()
            .map(|name| MockUsage {
                name,
                source: MockSource::CachedBehaviorMap,
            })
            .collect();
        let new_path_executions: Vec<ExecutionSummary> = (0..unique_paths)
            .map(|i| ExecutionSummary {
                inputs: vec![serde_json::json!(i)],
                return_value: Some(serde_json::json!(i * 10)),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
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
                mock_values: vec![],
            })
            .collect();

        FunctionResult {
            function_name: name.to_string(),
            exploration: ObservationOutput {
                function_name: name.to_string(),
                iterations,
                unique_paths,
                lines_covered,
                total_lines,
                new_path_executions,
                raw_results: vec![],
                discoveries: vec![],
                nondeterministic_fields: vec![],
                float_probe_results: vec![],
                boundary_results: vec![],
                shrunk_witnesses: std::collections::HashMap::new(),
                mcdc_summary: None,
                shrink_stats: crate::shrink::ShrinkStats::default(),
                abandoned_frontiers: vec![],
                opaque_suggestions: vec![],
                stubbed_modules: vec![],
            },
            behavior_map: BehaviorMap {
                function_id: name.to_string(),
                behaviors,
                fingerprint: None,
                nondeterministic_fields: vec![],
            },
            behavior_coverage: vec![],
            mocks_used: mocks,
            mock_misses: vec![],
            coverage_metrics: Default::default(),
            refactoring_recommendations: vec![],
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
            workers_reaped: 0,
            sampling: None,
        };

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "src/math.ts".to_string());
        file_map.insert("caller".to_string(), "src/app.ts".to_string());

        let report = generate_report(&parallel_result, &file_map, None);

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
        assert_eq!(caller.mocks_used.len(), 1);
        assert_eq!(caller.mocks_used[0].name, "leaf");
        assert_eq!(caller.mocks_used[0].source, "behavior_map");
        // No behavior_coverage in make_function_result → fallback to 0.0/0
        assert_eq!(caller.mocks_used[0].mock_coverage_pct, Some(0.0));
        assert_eq!(caller.mocks_used[0].mock_execution_count, Some(0));

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
                category: crate::scan_orchestrator::SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

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
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

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
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

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
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

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
                category: crate::scan_orchestrator::SkipCategory::Error,
            }],
            workers_used: 2,
            workers_reaped: 0,
            sampling: None,
        };

        let mut file_map = HashMap::new();
        file_map.insert("f1".to_string(), "src/a.ts".to_string());
        file_map.insert("f2".to_string(), "src/b.ts".to_string());

        let report = generate_report(&parallel_result, &file_map, None);
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
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);
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
            cumulative: None,
        };

        let dir = std::env::temp_dir().join("shatter-report-test");
        // Clean up from previous runs
        let _ = std::fs::remove_dir_all(&dir);

        let path = write_report(&report, &dir).expect("write_report should succeed");
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "scan-report.json");

        // Read back and verify
        let contents = std::fs::read_to_string(&path).expect("read file");
        let deserialized: ScanReport = serde_json::from_str(&contents).expect("parse json");
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
                error_category: None,
            }),
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
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
                error_intent: None,
            });

        let parallel_result = ParallelScanResult {
            function_results: vec![func_result],
            test_order: vec!["risky".into()],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

        let func = &report.functions[0];
        assert_eq!(func.behavior_clusters.len(), 2);

        let error_cluster = &func.behavior_clusters[1];
        assert!(error_cluster.thrown_error.is_some());
        assert!(
            error_cluster
                .thrown_error
                .as_ref()
                .unwrap()
                .contains("TypeError")
        );

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
            sampling: None,
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
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);

        // branches = 2 + 3 = 5, covered = 2 + 3 = 5 => 100%
        assert!((report.codebase.overall_coverage - 100.0).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Markdown report tests
    // -----------------------------------------------------------------------

    fn make_report_with_functions() -> ScanReport {
        let parallel_result = ParallelScanResult {
            function_results: vec![
                make_function_result("leaf", 10, 2, 5, 10, vec![]),
                make_function_result("caller", 20, 3, 8, 10, vec!["leaf".to_string()]),
            ],
            test_order: vec!["leaf".into(), "caller".into()],
            skipped: vec![],
            workers_used: 2,
            workers_reaped: 0,
            sampling: None,
        };

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "src/math.ts".to_string());
        file_map.insert("caller".to_string(), "src/app.ts".to_string());

        generate_report(&parallel_result, &file_map, None)
    }

    #[test]
    fn markdown_report_contains_all_sections() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        assert!(md.contains("# Shatter Scan Report"), "missing heading");
        assert!(md.contains("## Function Summary"), "missing summary table");
        assert!(md.contains("## Function Details"), "missing details");
        assert!(md.contains("### `leaf`"), "missing leaf details");
        assert!(md.contains("### `caller`"), "missing caller details");
    }

    #[test]
    fn markdown_report_has_correct_statistics() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        assert!(
            md.contains("**Functions explored:** 2"),
            "bad function count: {md}"
        );
        assert!(
            md.contains("**Total branches:** 5"),
            "bad branch count: {md}"
        );
    }

    #[test]
    fn markdown_report_summary_table_has_headers() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        assert!(
            md.contains("| Status | Function | File | Coverage | Branches | Lines | Iterations |"),
            "missing table header"
        );
        assert!(
            md.contains("|--------|----------|------|----------|----------|-------|------------|"),
            "missing table separator"
        );
    }

    #[test]
    fn markdown_report_coverage_indicators() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        // leaf: 5/10 lines = 50% -> WARN, caller: 8/10 = 80% -> WARN
        assert!(
            md.contains("WARN"),
            "should contain WARN status for partial coverage"
        );
    }

    #[test]
    fn markdown_report_shows_mocks() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        assert!(md.contains("**Mocks:** leaf"), "missing mock info: {md}");
    }

    #[test]
    fn markdown_empty_report_produces_sensible_output() {
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
            cumulative: None,
        };

        let md = format_markdown_report(&report);

        assert!(md.contains("# Shatter Scan Report"), "missing heading");
        assert!(
            md.contains("**Functions explored:** 0"),
            "missing zero functions"
        );
        assert!(
            md.contains("*No functions were explored.*"),
            "missing empty message"
        );
        assert!(
            !md.contains("## Function Details"),
            "should not have details section"
        );
        assert!(
            !md.contains("## Uncovered Branches"),
            "should not have uncovered section"
        );
    }

    #[test]
    fn markdown_report_with_skipped_functions() {
        let report = ScanReport {
            version: 1,
            functions: vec![],
            codebase: CodebaseReport {
                total_functions: 0,
                total_branches: 0,
                overall_coverage: 0.0,
                skipped_functions: vec![SkippedFunctionReport {
                    function_name: "slow".to_string(),
                    reason: "timed out after 30s".to_string(),
                    category: "error".to_string(),
                }],
                dependency_graph: vec![],
            },
            test_order: vec![],
            cumulative: None,
        };

        let md = format_markdown_report(&report);

        assert!(md.contains("## Errors"), "missing errors section: {md}");
        assert!(
            md.contains("`slow`: timed out after 30s"),
            "missing skip detail: {md}"
        );
    }

    #[test]
    fn markdown_table_formatting_is_valid() {
        let report = make_report_with_functions();
        let md = format_markdown_report(&report);

        let in_table: Vec<&str> = md
            .lines()
            .skip_while(|l| !l.starts_with("| Status"))
            .take_while(|l| l.starts_with('|'))
            .collect();

        // header + separator + 2 data rows = 4 lines
        assert_eq!(
            in_table.len(),
            4,
            "table should have 4 rows, got: {in_table:?}"
        );

        for line in &in_table {
            assert!(line.starts_with('|'), "row should start with |: {line}");
            assert!(line.ends_with('|'), "row should end with |: {line}");
        }
    }

    #[test]
    fn write_markdown_report_creates_file() {
        let report = make_report_with_functions();
        let dir = std::env::temp_dir().join("shatter-md-report-test");
        let _ = std::fs::remove_dir_all(&dir);

        let path = write_markdown_report(&report, &dir).expect("write should succeed");
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "scan-report.md");

        let contents = std::fs::read_to_string(&path).expect("read file");
        assert!(contents.contains("# Shatter Scan Report"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn markdown_report_caps_behavior_clusters() {
        // Create a function with more clusters than MAX_DISPLAY_CLUSTERS
        let cluster_count = 12;
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result(
                "many_behaviors",
                100,
                cluster_count,
                50,
                100,
                vec![],
            )],
            test_order: vec!["many_behaviors".into()],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);
        let md = format_markdown_report(&report);

        // Should show exactly MAX_DISPLAY_CLUSTERS cluster lines
        let cluster_lines: Vec<&str> = md.lines().filter(|l| l.starts_with("- Cluster ")).collect();
        assert_eq!(
            cluster_lines.len(),
            MAX_DISPLAY_CLUSTERS,
            "should display exactly {MAX_DISPLAY_CLUSTERS} clusters, got {}: {cluster_lines:?}",
            cluster_lines.len()
        );

        // Should show the truncation summary
        let remaining = cluster_count - MAX_DISPLAY_CLUSTERS;
        let expected_summary = format!("... and {remaining} more clusters");
        assert!(
            md.contains(&expected_summary),
            "missing truncation summary: {expected_summary}"
        );
    }

    #[test]
    fn markdown_report_no_truncation_when_under_cap() {
        // Create a function with exactly MAX_DISPLAY_CLUSTERS clusters
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result(
                "few_behaviors",
                10,
                MAX_DISPLAY_CLUSTERS,
                5,
                10,
                vec![],
            )],
            test_order: vec!["few_behaviors".into()],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };

        let file_map = HashMap::new();
        let report = generate_report(&parallel_result, &file_map, None);
        let md = format_markdown_report(&report);

        let cluster_lines: Vec<&str> = md.lines().filter(|l| l.starts_with("- Cluster ")).collect();
        assert_eq!(
            cluster_lines.len(),
            MAX_DISPLAY_CLUSTERS,
            "should display all clusters when at cap"
        );
        assert!(
            !md.contains("more clusters"),
            "should not show truncation summary when at cap"
        );
    }

    // -----------------------------------------------------------------------
    // Progress event tests
    // -----------------------------------------------------------------------

    #[test]
    fn progress_event_has_correct_structure() {
        let event = ProgressEvent::new("classifyNumber", 1, 5, 1234);

        assert_eq!(event.event_type, "progress");
        assert_eq!(event.status, None);
        assert_eq!(event.function, "classifyNumber");
        assert_eq!(event.current, 1);
        assert_eq!(event.total, 5);
        assert_eq!(event.elapsed_ms, 1234);
    }

    #[test]
    fn progress_event_serializes_to_json() {
        let event = ProgressEvent::new("f", 2, 10, 500);
        let json = event.to_json().expect("should serialize");

        assert!(
            json.contains("\"type\":\"progress\""),
            "missing type: {json}"
        );
        assert!(
            json.contains("\"function\":\"f\""),
            "missing function: {json}"
        );
        assert!(json.contains("\"current\":2"), "missing current: {json}");
        assert!(json.contains("\"total\":10"), "missing total: {json}");
        assert!(
            json.contains("\"elapsed_ms\":500"),
            "missing elapsed: {json}"
        );
    }

    #[test]
    fn progress_event_round_trips() {
        let event = ProgressEvent::new("test", 3, 7, 999);
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, deserialized);
    }

    #[test]
    fn progress_event_with_status_round_trips() {
        let event = ProgressEvent::with_status("test", 3, 7, 999, "skipped");
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.status.as_deref(), Some("skipped"));
        assert_eq!(event, deserialized);
    }

    #[test]
    fn progress_event_new_omits_optional_fields_in_json() {
        let event = ProgressEvent::new("f", 1, 2, 100);
        let json = event.to_json().expect("serialize");
        assert!(
            !json.contains("branches_covered"),
            "bare progress event should not emit branches_covered: {json}"
        );
        assert!(
            !json.contains("mcdc_total"),
            "bare progress event should not emit mcdc_total: {json}"
        );
        assert!(
            !json.contains("iters_since_new_discovery"),
            "bare progress event should not emit iters_since_new_discovery: {json}"
        );
    }

    #[test]
    fn progress_event_with_coverage_round_trips() {
        let event = ProgressEvent::new("classifyNumber", 1, 5, 1234)
            .with_branch_coverage(8, 12)
            .with_mcdc(7, 3)
            .with_idle_iters(42);
        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, deserialized);
        assert_eq!(deserialized.branches_covered, Some(8));
        assert_eq!(deserialized.branches_total, Some(12));
        assert_eq!(deserialized.mcdc_total, Some(7));
        assert_eq!(deserialized.mcdc_independent, Some(3));
        assert_eq!(deserialized.iters_since_new_discovery, Some(42));
    }

    #[test]
    fn progress_event_legacy_json_deserializes_without_new_fields() {
        // Earlier producers/consumers did not know about the optional fields.
        // The struct must still accept their shape unchanged.
        let legacy = r#"{"type":"progress","function":"f","current":1,"total":3,"elapsed_ms":200}"#;
        let event: ProgressEvent = serde_json::from_str(legacy).expect("deserialize legacy");
        assert_eq!(event.function, "f");
        assert_eq!(event.branches_covered, None);
        assert_eq!(event.branches_total, None);
        assert_eq!(event.mcdc_total, None);
        assert_eq!(event.mcdc_independent, None);
        assert_eq!(event.iters_since_new_discovery, None);
    }

    #[test]
    fn report_format_from_str() {
        assert_eq!("json".parse::<ReportFormat>().unwrap(), ReportFormat::Json);
        assert_eq!(
            "markdown".parse::<ReportFormat>().unwrap(),
            ReportFormat::Markdown
        );
        assert_eq!(
            "md".parse::<ReportFormat>().unwrap(),
            ReportFormat::Markdown
        );
        assert_eq!("both".parse::<ReportFormat>().unwrap(), ReportFormat::Both);
        assert!("invalid".parse::<ReportFormat>().is_err());
    }

    /// Regression guard for str-u40f: report file_path values must be relative.
    /// The CLI is responsible for relativizing paths before passing them in file_map;
    /// generate_report passes them through verbatim.
    #[test]
    fn report_file_paths_are_relative_when_file_map_is_relative() {
        let parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("f1", 10, 2, 5, 10, vec![])],
            test_order: vec!["f1".into()],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };

        let mut file_map = HashMap::new();
        file_map.insert("f1".to_string(), "src/a.ts".to_string());

        let report = generate_report(&parallel_result, &file_map, None);

        for func in &report.functions {
            assert!(
                !func.file_path.starts_with('/'),
                "file_path should be relative, got: {}",
                func.file_path
            );
            assert_eq!(func.file_path, "src/a.ts");
        }
    }

    #[test]
    fn boundary_values_detected() {
        assert!(is_boundary_value(&[serde_json::json!(0)]));
        assert!(is_boundary_value(&[serde_json::json!(null)]));
        assert!(is_boundary_value(&[serde_json::json!("")]));
        assert!(is_boundary_value(&[serde_json::json!([])]));
        assert!(!is_boundary_value(&[serde_json::json!(42)]));
        assert!(!is_boundary_value(&[serde_json::json!("hello")]));
    }

    // -----------------------------------------------------------------------
    // HTML report tests
    // -----------------------------------------------------------------------

    #[test]
    fn html_scan_report_is_valid_structure() {
        let report = make_report_with_functions();
        let html = generate_html_scan_report(&report, None);
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "must start with doctype"
        );
        assert!(html.contains("<html"), "must have html tag");
        assert!(html.contains("</html>"), "must close html tag");
        assert!(html.contains("</body>"), "must close body tag");
    }

    #[test]
    fn html_scan_report_contains_function_names() {
        let report = make_report_with_functions();
        let html = generate_html_scan_report(&report, None);
        // make_report_with_functions produces functions named "leaf" and "caller"
        assert!(html.contains("leaf"), "must contain function leaf");
        assert!(html.contains("caller"), "must contain function caller");
    }

    #[test]
    fn html_scan_report_contains_coverage_metrics() {
        let report = make_report_with_functions();
        let html = generate_html_scan_report(&report, None);
        // Must show coverage bar (cov-bar class)
        assert!(html.contains("cov-bar"), "must contain coverage bar");
        // Must show some percentage
        assert!(html.contains('%'), "must show percentage");
    }

    #[test]
    fn html_scan_report_escapes_special_chars() {
        let mut parallel_result = ParallelScanResult {
            function_results: vec![make_function_result("fn<test>&\"", 5, 2, 4, 10, vec![])],
            test_order: vec![],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
        };
        // Add a skipped function with special chars in reason
        parallel_result.skipped.push(SkippedFunction {
            function_name: "fn<skip>".to_string(),
            reason: "param 'x' has opaque type net.Socket & <stuff>".to_string(),
            category: crate::scan_orchestrator::SkipCategory::Expected,
        });
        let mut file_map = HashMap::new();
        file_map.insert("fn<test>&\"".to_string(), "src/test.ts".to_string());
        let report = generate_report(&parallel_result, &file_map, None);
        let html = generate_html_scan_report(&report, None);

        // Raw special chars must not appear unescaped in HTML
        assert!(!html.contains("<test>"), "angle brackets must be escaped");
        assert!(html.contains("&lt;test&gt;"), "must contain escaped form");
        assert!(!html.contains("<skip>"), "skip reason must be escaped");
    }

    #[test]
    fn html_report_format_from_str() {
        assert_eq!("html".parse::<ReportFormat>().unwrap(), ReportFormat::Html);
    }

    #[test]
    fn render_explore_fn_html_contains_function_name() {
        use crate::explorer::{ExecutionSummary, ObservationOutput};

        let result = ObservationOutput {
            function_name: "myFunc".to_string(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 8,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(42)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
            }],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
        };
        let fragment = render_explore_fn_html(&result, "src/foo.ts:1-10", None);
        assert!(fragment.contains("myFunc"), "must contain function name");
        assert!(fragment.contains("cov-bar"), "must contain coverage bar");
        assert!(fragment.contains("<details>"), "must use details element");
        assert!(fragment.contains("42"), "must show input value");
    }

    #[test]
    fn wrap_explore_html_full_page() {
        let fragments = vec!["<details>foo</details>".to_string()];
        let html = wrap_explore_html(&fragments, 1, 3, 7, 10);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<details>foo</details>"));
        assert!(html.contains('%'), "must show coverage percentage");
    }

    #[test]
    fn mock_quality_metrics_with_behavior_coverage() {
        use crate::behavior::BehaviorCoverage;

        let mut func_result = make_function_result("caller", 10, 2, 5, 10, vec!["dep".to_string()]);
        // Add behavior coverage: 2 of 5 callee behaviors exercised
        func_result.behavior_coverage = vec![BehaviorCoverage {
            caller: "caller".to_string(),
            callee: "dep".to_string(),
            exercised_behavior_ids: vec![0, 3],
            total_behaviors: 5,
        }];

        let report = build_function_report(&func_result, "src/caller.ts");

        assert_eq!(report.mocks_used.len(), 1);
        let mock = &report.mocks_used[0];
        assert_eq!(mock.name, "dep");
        assert_eq!(mock.source, "behavior_map");
        assert!((mock.mock_coverage_pct.unwrap() - 0.4).abs() < f64::EPSILON);
        assert_eq!(mock.mock_execution_count, Some(5));
    }

    #[test]
    fn mock_quality_metrics_type_stub_has_none() {
        use crate::scan_orchestrator::{MockSource, MockUsage};

        let mut func_result = make_function_result("caller", 10, 2, 5, 10, vec![]);
        func_result.mocks_used = vec![MockUsage {
            name: "stub_dep".to_string(),
            source: MockSource::TypeAwareStub,
        }];

        let report = build_function_report(&func_result, "src/caller.ts");

        assert_eq!(report.mocks_used.len(), 1);
        let mock = &report.mocks_used[0];
        assert_eq!(mock.name, "stub_dep");
        assert_eq!(mock.source, "type_stub");
        assert!(mock.mock_coverage_pct.is_none());
        assert!(mock.mock_execution_count.is_none());
    }

    #[test]
    fn mock_quality_metrics_stratum_excluded_has_none() {
        use crate::scan_orchestrator::{MockSource, MockUsage};

        let mut func_result = make_function_result("caller", 10, 2, 5, 10, vec![]);
        func_result.mocks_used = vec![MockUsage {
            name: "excluded_dep".to_string(),
            source: MockSource::StratumExcluded,
        }];

        let report = build_function_report(&func_result, "src/caller.ts");

        assert_eq!(report.mocks_used.len(), 1);
        let mock = &report.mocks_used[0];
        assert_eq!(mock.source, "stratum_excluded");
        assert!(mock.mock_coverage_pct.is_none());
        assert!(mock.mock_execution_count.is_none());
    }

    #[test]
    fn mock_quality_metrics_mixed_sources() {
        use crate::behavior::BehaviorCoverage;
        use crate::scan_orchestrator::{MockSource, MockUsage};

        let mut func_result = make_function_result("caller", 10, 2, 5, 10, vec![]);
        func_result.mocks_used = vec![
            MockUsage {
                name: "cached".to_string(),
                source: MockSource::CachedBehaviorMap,
            },
            MockUsage {
                name: "stubbed".to_string(),
                source: MockSource::TypeAwareStub,
            },
            MockUsage {
                name: "excluded".to_string(),
                source: MockSource::StratumExcluded,
            },
        ];
        func_result.behavior_coverage = vec![BehaviorCoverage {
            caller: "caller".to_string(),
            callee: "cached".to_string(),
            exercised_behavior_ids: vec![0, 1, 2],
            total_behaviors: 3,
        }];

        let report = build_function_report(&func_result, "src/caller.ts");

        assert_eq!(report.mocks_used.len(), 3);
        // CachedBehaviorMap: has metrics
        assert_eq!(report.mocks_used[0].source, "behavior_map");
        assert!((report.mocks_used[0].mock_coverage_pct.unwrap() - 1.0).abs() < f64::EPSILON);
        assert_eq!(report.mocks_used[0].mock_execution_count, Some(3));
        // TypeAwareStub: no metrics
        assert!(report.mocks_used[1].mock_coverage_pct.is_none());
        assert!(report.mocks_used[1].mock_execution_count.is_none());
        // StratumExcluded: no metrics
        assert!(report.mocks_used[2].mock_coverage_pct.is_none());
        assert!(report.mocks_used[2].mock_execution_count.is_none());
    }

    #[test]
    fn mock_usage_report_serialization_roundtrip() {
        let report = MockUsageReport {
            name: "dep".to_string(),
            source: "behavior_map".to_string(),
            mock_coverage_pct: Some(0.75),
            mock_execution_count: Some(12),
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let deserialized: MockUsageReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, deserialized);
    }

    #[test]
    fn mock_usage_report_none_fields_omitted_in_json() {
        let report = MockUsageReport {
            name: "dep".to_string(),
            source: "type_stub".to_string(),
            mock_coverage_pct: None,
            mock_execution_count: None,
        };
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(!json.contains("mock_coverage_pct"));
        assert!(!json.contains("mock_execution_count"));
    }
}

/// One discovered target's outcome, ready for markdown rendering.
///
/// Drives `render_explore_outcomes` so that every discovered target produces
/// a heading regardless of whether it completed, failed instrumentation,
/// failed at runtime, was skipped, or timed out. The detail block is reserved
/// for `Completed` / `CompletedWithFindings` — other statuses get only the
/// status line and reason.
#[derive(Debug, Clone)]
pub struct OutcomeRenderEntry<'a> {
    /// Fully qualified function name used as the markdown heading.
    pub qualified_name: &'a str,
    /// Machine-readable status from the outcome stream.
    pub status: OutcomeStatus,
    /// One-sentence human-readable reason. Always present, even for completed
    /// outcomes where it summarizes the result ("explored N paths" etc.).
    pub reason: &'a str,
    /// Per-function detail markdown (paths, branches, etc.). Caller passes
    /// `Some` only for `Completed` / `CompletedWithFindings`; the renderer
    /// ignores `Some` for other statuses to keep the output coherent.
    pub detail_md: Option<&'a str>,
}

/// Stable kebab-case label for an `OutcomeStatus` shown in markdown.
fn outcome_status_label(status: OutcomeStatus) -> &'static str {
    match status {
        OutcomeStatus::Completed => "completed",
        OutcomeStatus::CompletedWithFindings => "completed_with_findings",
        OutcomeStatus::Unsupported => "unsupported",
        OutcomeStatus::BuildFailed => "build_failed",
        OutcomeStatus::RuntimeFailed => "runtime_failed",
        OutcomeStatus::TimedOut => "timed_out",
        OutcomeStatus::SkippedByPolicy => "skipped_by_policy",
    }
}

/// Render the explore-mode markdown report from an outcome stream.
///
/// Behavior:
/// - Empty `entries` → emits a `## No targets discovered` body. The caller
///   passes the explanatory reason via `empty_reason` (typically the upstream
///   discovery diagnostic).
/// - Otherwise: one `## {qualified_name}` heading per entry, then a
///   `**Status:** {label}` line, then the reason on its own line, then the
///   detail block if the status warrants one. Sections are joined with a
///   horizontal rule.
pub fn render_explore_outcomes(entries: &[OutcomeRenderEntry<'_>], empty_reason: &str) -> String {
    if entries.is_empty() {
        return format!("## No targets discovered\n\n{empty_reason}\n");
    }

    let mut sections: Vec<String> = Vec::with_capacity(entries.len());
    for entry in entries {
        let status_label = outcome_status_label(entry.status);
        let mut section = format!(
            "## {name}\n\n**Status:** `{status_label}`\n\n{reason}\n",
            name = entry.qualified_name,
            reason = entry.reason,
        );
        let show_detail = matches!(
            entry.status,
            OutcomeStatus::Completed | OutcomeStatus::CompletedWithFindings,
        );
        if show_detail
            && let Some(detail) = entry.detail_md
            && !detail.trim().is_empty()
        {
            section.push('\n');
            section.push_str(detail.trim_end());
            section.push('\n');
        }
        sections.push(section);
    }
    sections.join("\n---\n\n")
}

#[cfg(test)]
mod outcome_render_tests {
    use super::*;

    #[test]
    fn empty_entries_emits_no_targets_section() {
        let md = render_explore_outcomes(&[], "discovery returned an empty function list");
        assert!(md.contains("## No targets discovered"));
        assert!(md.contains("discovery returned an empty function list"));
        assert!(!md.is_empty());
    }

    #[test]
    fn failed_entry_gets_heading_and_status() {
        let entries = vec![OutcomeRenderEntry {
            qualified_name: "pkg/foo.Bar",
            status: OutcomeStatus::BuildFailed,
            reason: "go build returned exit code 1",
            detail_md: None,
        }];
        let md = render_explore_outcomes(&entries, "");
        assert!(md.contains("## pkg/foo.Bar"));
        assert!(md.contains("**Status:** `build_failed`"));
        assert!(md.contains("go build returned exit code 1"));
    }

    #[test]
    fn completed_entry_includes_detail_block() {
        let entries = vec![OutcomeRenderEntry {
            qualified_name: "pkg/foo.Quux",
            status: OutcomeStatus::Completed,
            reason: "explored 3 paths",
            detail_md: Some("### Paths\n- input=42 → return=true"),
        }];
        let md = render_explore_outcomes(&entries, "");
        assert!(md.contains("## pkg/foo.Quux"));
        assert!(md.contains("**Status:** `completed`"));
        assert!(md.contains("### Paths"));
        assert!(md.contains("input=42"));
    }

    #[test]
    fn non_completed_entry_drops_detail_block_even_when_supplied() {
        let entries = vec![OutcomeRenderEntry {
            qualified_name: "pkg/foo.Stale",
            status: OutcomeStatus::Unsupported,
            reason: "parameter type contains an interface",
            detail_md: Some("### Paths\n- should not appear"),
        }];
        let md = render_explore_outcomes(&entries, "");
        assert!(!md.contains("should not appear"));
        assert!(md.contains("**Status:** `unsupported`"));
    }

    #[test]
    fn mixed_statuses_each_get_their_own_section() {
        let entries = vec![
            OutcomeRenderEntry {
                qualified_name: "pkg.A",
                status: OutcomeStatus::Completed,
                reason: "ok",
                detail_md: Some("detail-A"),
            },
            OutcomeRenderEntry {
                qualified_name: "pkg.B",
                status: OutcomeStatus::TimedOut,
                reason: "exceeded 30s budget",
                detail_md: None,
            },
            OutcomeRenderEntry {
                qualified_name: "pkg.C",
                status: OutcomeStatus::RuntimeFailed,
                reason: "panic: nil pointer",
                detail_md: None,
            },
        ];
        let md = render_explore_outcomes(&entries, "");
        assert!(md.contains("## pkg.A"));
        assert!(md.contains("## pkg.B"));
        assert!(md.contains("## pkg.C"));
        assert!(md.contains("`completed`"));
        assert!(md.contains("`timed_out`"));
        assert!(md.contains("`runtime_failed`"));
        assert!(md.contains("detail-A"));
        // Section separator between three entries → exactly two `\n---\n\n`.
        assert_eq!(md.matches("\n---\n\n").count(), 2);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_mock_source() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("behavior_map".to_string()),
            Just("type_stub".to_string()),
            Just("stratum_excluded".to_string()),
        ]
    }

    fn arb_mock_usage_report() -> impl Strategy<Value = MockUsageReport> {
        ("[a-z_]{1,20}", arb_mock_source())
            .prop_flat_map(|(name, source)| {
                let has_metrics = source == "behavior_map";
                let coverage = if has_metrics {
                    (0.0..=1.0f64).prop_map(Some).boxed()
                } else {
                    Just(None).boxed()
                };
                let exec_count = if has_metrics {
                    (0..=1000u64).prop_map(Some).boxed()
                } else {
                    Just(None).boxed()
                };
                (Just(name), Just(source), coverage, exec_count)
            })
            .prop_map(
                |(name, source, mock_coverage_pct, mock_execution_count)| MockUsageReport {
                    name,
                    source,
                    mock_coverage_pct,
                    mock_execution_count,
                },
            )
    }

    proptest! {
        #[test]
        fn mock_usage_report_roundtrip(report in arb_mock_usage_report()) {
            let json = serde_json::to_string(&report).expect("serialize");
            let deserialized: MockUsageReport = serde_json::from_str(&json).expect("deserialize");
            // Coverage pct needs approximate comparison for floats
            prop_assert_eq!(&deserialized.name, &report.name);
            prop_assert_eq!(&deserialized.source, &report.source);
            prop_assert_eq!(deserialized.mock_execution_count, report.mock_execution_count);
            match (deserialized.mock_coverage_pct, report.mock_coverage_pct) {
                (Some(a), Some(b)) => prop_assert!((a - b).abs() < 1e-10),
                (None, None) => {},
                _ => prop_assert!(false, "coverage_pct presence mismatch"),
            }
        }

        #[test]
        fn behavior_map_mocks_always_have_metrics(
            coverage_pct in 0.0..=1.0f64,
            exec_count in 0..=1000u64,
        ) {
            let report = MockUsageReport {
                name: "dep".to_string(),
                source: "behavior_map".to_string(),
                mock_coverage_pct: Some(coverage_pct),
                mock_execution_count: Some(exec_count),
            };
            prop_assert!(report.mock_coverage_pct.is_some());
            prop_assert!(report.mock_execution_count.is_some());
            let pct = report.mock_coverage_pct.unwrap();
            prop_assert!((0.0..=1.0).contains(&pct), "coverage must be in [0.0, 1.0]");
        }

        #[test]
        fn non_behavior_map_mocks_never_have_metrics(
            source in prop_oneof![Just("type_stub".to_string()), Just("stratum_excluded".to_string())],
        ) {
            let report = MockUsageReport {
                name: "dep".to_string(),
                source,
                mock_coverage_pct: None,
                mock_execution_count: None,
            };
            prop_assert!(report.mock_coverage_pct.is_none());
            prop_assert!(report.mock_execution_count.is_none());
        }
    }
}
