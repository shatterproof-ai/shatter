//! Baseline HTML snapshot tests for the hand-built report rendering functions.
//!
//! These tests capture the exact (whitespace-normalized) output of the three
//! public HTML renderers **before** any Askama templating refactor. They serve
//! as the regression baseline: after the refactor, the normalized output must
//! match these snapshots byte-for-byte.
//!
//! # First-run behaviour
//! If a snapshot file does not yet exist the test writes the current output to
//! disk and passes. On subsequent runs the written file is compared against the
//! freshly-rendered output.
//!
//! # Whitespace normalisation
//! `normalize_ws` collapses all runs of ASCII whitespace (including newlines) to
//! a single space and trims the result. This lets Askama templates produce
//! slightly different indentation while still catching structural / content
//! regressions.

use std::path::Path;

use shatter_core::behavior::BehaviorMap;
use shatter_core::explorer::{ExecutionSummary, ObservationOutput};
use shatter_core::report::{generate_html_scan_report, generate_report, wrap_explore_html};
use shatter_core::scan_orchestrator::{FunctionResult, MockSource, MockUsage, ParallelScanResult};
use shatter_core::shrink::ShrinkStats;

// ---------------------------------------------------------------------------
// Helper: whitespace normalisation
// ---------------------------------------------------------------------------

/// Collapse all runs of ASCII whitespace to a single space and trim.
///
/// This makes the snapshot comparison insensitive to insignificant whitespace
/// differences (indentation, trailing newlines, etc.) while still catching any
/// structural or content regression.
fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Helper: assert or write snapshot
// ---------------------------------------------------------------------------

/// Compare `actual` (whitespace-normalised) against the snapshot at `path`.
///
/// If the snapshot file does not exist yet the raw (un-normalised) HTML is
/// written to `path` and the test passes — this is the first-run baseline
/// capture. On subsequent runs the normalised actual output is compared
/// against the normalised snapshot content.
fn assert_snapshot(path: &Path, actual: &str) {
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())
            .expect("failed to create snapshot directory");
        std::fs::write(path, actual).expect("failed to write snapshot file");
        return;
    }

    let stored = std::fs::read_to_string(path).expect("failed to read snapshot file");
    let norm_actual = normalize_ws(actual);
    let norm_stored = normalize_ws(&stored);

    assert_eq!(
        norm_actual,
        norm_stored,
        "HTML snapshot mismatch for {}.\n\
         Run the tests once with the snapshot file deleted to regenerate it.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Deterministic test data builders
// ---------------------------------------------------------------------------

fn make_observation_output() -> ObservationOutput {
    ObservationOutput {
        function_name: "myFunc".to_string(),
        iterations: 10,
        unique_paths: 2,
        lines_covered: 5,
        total_lines: 8,
        new_path_executions: vec![
            ExecutionSummary {
                inputs: vec![serde_json::json!(42)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
            },
            ExecutionSummary {
                inputs: vec![serde_json::json!(-1)],
                return_value: None,
                thrown_error: Some("RangeError: negative".to_string()),
                lines_executed: vec![1, 4],
                is_new_path: true,
                error_intent: None,
            },
        ],
        raw_results: vec![],
        discoveries: vec![],
        nondeterministic_fields: vec![],
        float_probe_results: vec![],
        boundary_results: vec![],
        shrunk_witnesses: std::collections::HashMap::new(),
        mcdc_summary: None,
        shrink_stats: ShrinkStats::default(),
        abandoned_frontiers: vec![],
        opaque_suggestions: vec![],
        stubbed_modules: vec![],
    ..Default::default()
    }
}

fn make_scan_report() -> shatter_core::report::ScanReport {
    use shatter_core::behavior::Behavior;

    let make_fn = |name: &str,
                   iterations: u32,
                   unique_paths: usize,
                   lines_covered: usize,
                   total_lines: u32,
                   mocks: Vec<&str>|
     -> FunctionResult {
        let mocks_used: Vec<MockUsage> = mocks
            .into_iter()
            .map(|n| MockUsage {
                name: n.to_string(),
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
                shrink_stats: ShrinkStats::default(),
                abandoned_frontiers: vec![],
                opaque_suggestions: vec![],
                stubbed_modules: vec![],
                            ..Default::default()
            },
            behavior_map: BehaviorMap {
                function_id: name.to_string(),
                behaviors,
                fingerprint: None,
                nondeterministic_fields: vec![],
            },
            behavior_coverage: vec![],
            mocks_used,
            mock_misses: vec![],
            // str-9q1z: branch_count/branches_covered now derive from
            // coverage_metrics rather than exploration.unique_paths.
            // Mirror the legacy semantics in this fixture so the snapshot
            // stays meaningful: pretend every discovered path corresponds
            // to an analyzer branch covered by Z3.
            coverage_metrics: shatter_core::coverage_metrics::CoverageMetrics {
                total_branches: unique_paths,
                z3_solved: unique_paths,
                random_found: 0,
                user_provided: 0,
                fuzz_found: 0,
                uncovered: 0,
                symexpr_count: 0,
                unknown_count: 0,
                mcdc_metrics: None,
            },
            refactoring_recommendations: vec![],
        }
    };

    let parallel_result = ParallelScanResult {
        function_results: vec![
            make_fn("leaf", 10, 2, 5, 10, vec![]),
            make_fn("caller", 20, 3, 8, 10, vec!["leaf"]),
        ],
        test_order: vec!["leaf".into(), "caller".into()],
        skipped: vec![],
        workers_used: 2,
        workers_reaped: 0,
        sampling: None,
        source_files: vec![],
    };

    let mut file_map = std::collections::HashMap::new();
    file_map.insert("leaf".to_string(), "src/math.ts".to_string());
    file_map.insert("caller".to_string(), "src/app.ts".to_string());

    generate_report(&parallel_result, &file_map, None)
}

// ---------------------------------------------------------------------------
// Snapshot directory (relative to the crate manifest)
// ---------------------------------------------------------------------------

fn snapshot_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests/snapshots").join(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Snapshot: `render_explore_fn_html` fragment for a single function.
#[test]
fn snapshot_explore_fn_html() {
    use shatter_core::report::render_explore_fn_html;

    let result = make_observation_output();
    // No project_root: source block is skipped gracefully.
    let html = render_explore_fn_html(&result, "src/foo.ts:1-10", None);

    assert!(
        !html.is_empty(),
        "render_explore_fn_html must not return empty string"
    );
    assert_snapshot(&snapshot_path("explore_fn.html"), &html);
}

/// Snapshot: `wrap_explore_html` full page wrapping a single fragment.
#[test]
fn snapshot_explore_page_html() {
    let result = make_observation_output();
    let fragment = {
        use shatter_core::report::render_explore_fn_html;
        render_explore_fn_html(&result, "src/foo.ts:1-10", None)
    };

    let html = wrap_explore_html(&[fragment], 1, 2, 5, 8);

    assert!(
        !html.is_empty(),
        "wrap_explore_html must not return empty string"
    );
    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "must be a full HTML page"
    );
    assert_snapshot(&snapshot_path("explore_page.html"), &html);
}

/// Snapshot: `generate_html_scan_report` for a two-function scan result.
#[test]
fn snapshot_scan_report_html() {
    let report = make_scan_report();
    // No project_root: source block is skipped gracefully.
    let html = generate_html_scan_report(&report, None);

    assert!(
        !html.is_empty(),
        "generate_html_scan_report must not return empty string"
    );
    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "must be a full HTML page"
    );
    assert_snapshot(&snapshot_path("scan_report.html"), &html);
}

/// Source code block appears when a readable source file and valid location are provided.
#[test]
fn source_block_rendered_when_project_root_provided() {
    use shatter_core::report::render_explore_fn_html;

    // Write a temporary source file with 5 lines.
    let dir = std::env::temp_dir().join("shatter_html_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("target.ts"),
        "function add(a, b) {\n  if (a < 0) {\n    return 0;\n  }\n  return a + b;\n}\n",
    )
    .unwrap();

    let mut result = make_observation_output();
    // Use line numbers matching the 5-line file (1-based).
    result.new_path_executions[0].lines_executed = vec![1, 2, 5];
    result.new_path_executions[1].lines_executed = vec![1, 2, 3];

    let location = "src/target.ts:1-5";
    let html = render_explore_fn_html(&result, location, Some(&dir));

    assert!(html.contains("src-block"), "source block div must appear");
    assert!(
        html.contains("src-line covered"),
        "covered lines must appear"
    );
    assert!(
        html.contains("src-line uncovered"),
        "uncovered lines must appear"
    );
    assert!(html.contains("return a + b"), "source text must appear");
    assert!(html.contains("&lt;"), "source text must be HTML-escaped");
}
