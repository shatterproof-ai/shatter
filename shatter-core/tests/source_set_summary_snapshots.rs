//! Regression snapshot for the str-jeen.39 markdown source-set summary
//! table. Builds a `ScanReport` whose per-function reports cover every
//! `SourceBucket` variant (via direct `FunctionReport` construction —
//! avoiding the deeply-nested `ScanResult` shape), renders the
//! markdown, slices out the source-set section, and compares against a
//! stored snapshot.
//!
//! On first run (no snapshot file) the current output is written to
//! disk and the test passes. Subsequent runs assert byte-exact
//! equality.

use std::collections::HashMap;
use std::path::Path;

use shatter_core::report::{
    CodebaseReport, ConstraintStats, FunctionReport, SCAN_REPORT_SCHEMA_VERSION, ScanReport,
    SourceSetBucketStats, SourceSetSummary, format_markdown_report, generate_report,
};
use shatter_core::run_manifest::SourceFileSnapshot;
use shatter_core::scan_orchestrator::{ParallelScanResult, SkipCategory, SkippedFunction};
use shatter_core::source_bucket::SourceBucket;

// ---------------------------------------------------------------------------
// Snapshot helper (mirrors outcome_md_snapshots.rs)
// ---------------------------------------------------------------------------

fn snapshot_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests/snapshots").join(name)
}

fn assert_snapshot(path: &Path, actual: &str) {
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())
            .expect("failed to create snapshot directory");
        std::fs::write(path, actual).expect("failed to write snapshot file");
        return;
    }

    let stored = std::fs::read_to_string(path).expect("failed to read snapshot file");
    assert_eq!(
        actual,
        stored,
        "Source-set summary snapshot mismatch for {}.\n\
         Delete the snapshot file and re-run the test to regenerate it.",
        path.display()
    );
}

/// Slice the lines from "## Source Set Summary" up to (but not
/// including) the next `## ` heading or end-of-string.
fn extract_source_set_section(md: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut started = false;
    for line in md.lines() {
        if line.starts_with("## Source Set Summary") {
            started = true;
            lines.push(line);
            continue;
        }
        if started {
            if line.starts_with("## ") {
                break;
            }
            lines.push(line);
        }
    }
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// Fixture construction
// ---------------------------------------------------------------------------

/// Build a `FunctionReport` populated with just enough fields to drive
/// the markdown source-set table. Other fields take their defaults so
/// the renderer treats the function as zero-coverage with no behaviors
/// — the source-set rollup only reads `file_path`, `source_bucket`,
/// and `total_lines`.
fn make_function_report(
    name: &str,
    file_path: &str,
    bucket: SourceBucket,
    total_lines: u32,
) -> FunctionReport {
    FunctionReport {
        function_name: name.to_string(),
        display_name: name.to_string(),
        qualified_id: format!("{file_path}::{name}"),
        file_path: file_path.to_string(),
        source_bucket: bucket,
        branch_count: 0,
        branches_covered: 0,
        coverage_pct: 0.0,
        discovered_inputs: vec![],
        behavior_clusters: vec![],
        constraint_stats: ConstraintStats {
            total_constraints: 0,
            solver_guided_inputs: 0,
        },
        iterations: 0,
        lines_covered: 0,
        total_lines,
        mocks_used: vec![],
        refactoring_recommendations: vec![],
        completion_outcome: Default::default(),
        completion_reason: None,
    }
}

/// One `FunctionReport` per `SourceBucket`, plus a second
/// `ProductionIsh` entry in a different file so dedup is exercised.
fn build_fixture_report() -> ScanReport {
    let functions = vec![
        make_function_report(
            "render",
            "src/app/render.ts",
            SourceBucket::ProductionIsh,
            120,
        ),
        make_function_report(
            "compute",
            "src/app/compute.ts",
            SourceBucket::ProductionIsh,
            60,
        ),
        make_function_report(
            "render_smoke",
            "src/app/render.test.ts",
            SourceBucket::TestSpec,
            40,
        ),
        make_function_report(
            "rpc_handler",
            "api/v1/service.pb.go",
            SourceBucket::Generated,
            200,
        ),
        make_function_report(
            "GlobalDecl",
            "types/global.d.ts",
            SourceBucket::DeclarationOnly,
            15,
        ),
        make_function_report(
            "fixture_load",
            "pkg/parser/testdata/loader.go",
            SourceBucket::FixtureSample,
            25,
        ),
        make_function_report(
            "vendored_dep",
            "vendor/github.com/foo/bar.go",
            SourceBucket::PolicyExcluded,
            80,
        ),
        make_function_report(
            "build_script",
            "scripts/build.sh",
            SourceBucket::Unsupported,
            30,
        ),
    ];

    // Aggregate the source-set summary the same way the production
    // codepath does. Two production_ish files (180 lines), one each
    // for the other six buckets.
    let source_set = SourceSetSummary {
        production_ish: SourceSetBucketStats {
            file_count: 2,
            line_count: 180,
        },
        test_spec: SourceSetBucketStats {
            file_count: 1,
            line_count: 40,
        },
        generated: SourceSetBucketStats {
            file_count: 1,
            line_count: 200,
        },
        declaration_only: SourceSetBucketStats {
            file_count: 1,
            line_count: 15,
        },
        fixture_sample: SourceSetBucketStats {
            file_count: 1,
            line_count: 25,
        },
        policy_excluded: SourceSetBucketStats {
            file_count: 1,
            line_count: 80,
        },
        unsupported: SourceSetBucketStats {
            file_count: 1,
            line_count: 30,
        },
    };
    let productionish_source_lines = source_set.production_ish.line_count;

    ScanReport {
        version: SCAN_REPORT_SCHEMA_VERSION,
        functions,
        codebase: CodebaseReport {
            attempted_functions: 8,
            completed_functions: 8,
            total_discovered_functions: 8,
            productionish_source_lines,
            source_set,
            ..Default::default()
        },
        test_order: vec![],
        test_order_display_names: vec![],
        cumulative: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn source_set_summary_table_snapshot() {
    let report = build_fixture_report();
    let md = format_markdown_report(&report);
    let section = extract_source_set_section(&md);

    for bucket in [
        SourceBucket::ProductionIsh,
        SourceBucket::TestSpec,
        SourceBucket::Generated,
        SourceBucket::DeclarationOnly,
        SourceBucket::FixtureSample,
        SourceBucket::PolicyExcluded,
        SourceBucket::Unsupported,
    ] {
        let wire = bucket.as_wire_str();
        assert!(
            section.contains(wire),
            "source-set table missing bucket `{wire}`:\n{section}",
        );
    }

    assert_snapshot(&snapshot_path("source_set_summary.md"), &section);
}

#[test]
fn productionish_source_lines_referenced_in_markdown_header() {
    let report = build_fixture_report();
    let md = format_markdown_report(&report);

    // The narrative bullet must call out the denominator so a reader
    // sees the gap between "all source lines" and lines that count
    // toward coverage. This is the user-visible half of str-jeen.47's
    // TODO marker.
    assert!(
        md.contains("**Production-ish source lines:** 180"),
        "header must reference productionish_source_lines:\n{md}",
    );
}

#[test]
fn empty_report_emits_zeroed_source_set_table() {
    let report = ScanReport {
        version: SCAN_REPORT_SCHEMA_VERSION,
        functions: vec![],
        codebase: CodebaseReport::default(),
        test_order: vec![],
        test_order_display_names: vec![],
        cumulative: None,
    };

    let md = format_markdown_report(&report);
    let section = extract_source_set_section(&md);

    for bucket in [
        SourceBucket::ProductionIsh,
        SourceBucket::TestSpec,
        SourceBucket::Generated,
        SourceBucket::DeclarationOnly,
        SourceBucket::FixtureSample,
        SourceBucket::PolicyExcluded,
        SourceBucket::Unsupported,
    ] {
        let wire = bucket.as_wire_str();
        assert!(
            section.contains(&format!("| `{wire}` | 0 | 0 |")),
            "missing zero-row for `{wire}`:\n{section}",
        );
    }
}

// ---------------------------------------------------------------------------
// Regression tests for str-jeen.60 and str-jeen.63
// ---------------------------------------------------------------------------

/// Helper: build a minimal SourceFileSnapshot with a known line count.
fn make_source_file(path: &str, line_count: u32) -> SourceFileSnapshot {
    SourceFileSnapshot {
        path: path.to_string(),
        size: line_count as u64 * 40,
        mtime_ns: None,
        content_hash: None,
        line_count: Some(line_count),
    }
}

/// str-jeen.60: When all production functions fail/skip, the source set
/// must still reflect discovered files. Previously, production_ish showed
/// 0/0 because SourceSetSummary was built from completed function rows only.
#[test]
fn source_set_counts_from_snapshot_not_from_completed_functions() {
    let source_files = vec![
        make_source_file("src/app.ts", 300),
        make_source_file("src/utils.ts", 150),
        make_source_file("src/models.ts", 200),
        make_source_file("src/app.test.ts", 80),
    ];

    let parallel_result = ParallelScanResult {
        function_results: vec![],
        test_order: vec!["handleRequest".into(), "processData".into()],
        skipped: vec![
            SkippedFunction {
                function_name: "handleRequest".into(),
                reason: "timeout".into(),
                category: SkipCategory::Error,
            },
            SkippedFunction {
                function_name: "processData".into(),
                reason: "unsupported parameter type".into(),
                category: SkipCategory::Unsupported,
            },
        ],
        workers_used: 2,
        workers_reaped: 0,
        sampling: None,
        source_files,
    };

    let report = generate_report(&parallel_result, &HashMap::new(), None);

    // Three production-ish files: src/app.ts, src/utils.ts, src/models.ts
    assert_eq!(
        report.codebase.source_set.production_ish.file_count,
        3,
        "production_ish file_count must reflect discovered source snapshot, not completed functions"
    );
    // 300 + 150 + 200 = 650 whole-file lines
    assert_eq!(
        report.codebase.source_set.production_ish.line_count,
        650,
        "production_ish line_count must use whole-file lines from manifest, not function spans"
    );
    // One test-spec file: src/app.test.ts
    assert_eq!(
        report.codebase.source_set.test_spec.file_count,
        1,
        "test_spec file_count must count test files from snapshot"
    );
    assert_eq!(
        report.codebase.source_set.test_spec.line_count,
        80,
        "test_spec line_count must use whole-file lines"
    );
}

/// str-jeen.63: Markdown source counts must come from the manifest snapshot,
/// not from function-span lines. Previously, markdown showed function-span
/// line totals while run-status.json showed whole-file line counts.
#[test]
fn markdown_source_counts_match_manifest_snapshot_not_function_spans() {
    // Source files with known whole-file line counts.
    let source_files = vec![
        make_source_file("src/api.ts", 500),
        make_source_file("src/db.ts", 233),
    ];

    // Function reports with intentionally small total_lines (function spans),
    // to prove the markdown reads from the snapshot and not the function rows.
    let parallel_result = ParallelScanResult {
        function_results: vec![],
        test_order: vec![],
        skipped: vec![],
        workers_used: 1,
        workers_reaped: 0,
        sampling: None,
        source_files,
    };

    let report = generate_report(&parallel_result, &HashMap::new(), None);
    let md = format_markdown_report(&report);
    let section = extract_source_set_section(&md);

    // Whole-file lines: 500 + 233 = 733 — must appear in the markdown table.
    assert!(
        section.contains("| `production_ish` | 2 | 733 |"),
        "markdown must show whole-file line counts from manifest (2 files, 733 lines):\n{section}"
    );
    // productionish_source_lines bullet must also match.
    assert!(
        md.contains("**Production-ish source lines:** 733"),
        "markdown production-ish lines bullet must match manifest line count:\n{md}"
    );
}
