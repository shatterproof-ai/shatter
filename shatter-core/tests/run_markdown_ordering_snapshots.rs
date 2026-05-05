//! Regression snapshot for str-jeen.19: the run-report markdown must
//! lead with the whole-source (production-ish) denominator, then the
//! attempted-function span, and only then surface the
//! completed-function branch coverage line — which must be explicitly
//! labeled as a subset metric.
//!
//! The acceptance criteria are:
//!
//! 1. The Source Set Summary section appears before the Coverage
//!    section, and includes the **Production-ish source lines** bullet
//!    (whole-source denominator).
//! 2. The Coverage section's first bullet describes the
//!    attempted-function span.
//! 3. The Coverage section's coverage-percentage bullet is labeled
//!    `(completed-functions subset)` so a reader cannot mistake the
//!    completed-function denominator for codebase coverage.
//!
//! On first run (no snapshot file) the current rendered section is
//! written to disk and the test passes. Subsequent runs assert
//! byte-exact equality.

use std::path::Path;

use shatter_core::report::{
    CodebaseReport, SCAN_REPORT_SCHEMA_VERSION, ScanReport, SourceSetBucketStats,
    SourceSetSummary, format_markdown_report,
};

// ---------------------------------------------------------------------------
// Snapshot helper (mirrors source_set_summary_snapshots.rs)
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
        "Run-report markdown ordering snapshot mismatch for {}.\n\
         Delete the snapshot file and re-run the test to regenerate it.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Build a `ScanReport` with non-trivial values for every denominator
/// the run-report ordering depends on. No `FunctionReport`s are needed —
/// the bullets we assert against come from `CodebaseReport` totals.
fn build_fixture_report() -> ScanReport {
    // Pick distinct, visible numbers so the snapshot is unambiguous.
    let production_ish_lines: u64 = 1234;
    let total_discovered: usize = 50;
    let attempted: usize = 30;
    let completed: usize = 20;
    let total_branches: usize = 60;
    let overall_coverage: f64 = 42.5;

    let source_set = SourceSetSummary {
        production_ish: SourceSetBucketStats {
            file_count: 5,
            line_count: production_ish_lines,
        },
        test_spec: SourceSetBucketStats {
            file_count: 2,
            line_count: 200,
        },
        generated: SourceSetBucketStats {
            file_count: 1,
            line_count: 300,
        },
        declaration_only: SourceSetBucketStats {
            file_count: 1,
            line_count: 50,
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

    ScanReport {
        version: SCAN_REPORT_SCHEMA_VERSION,
        functions: vec![],
        codebase: CodebaseReport {
            attempted_functions: attempted,
            completed_functions: completed,
            total_discovered_functions: total_discovered,
            total_branches,
            overall_coverage,
            productionish_source_lines: production_ish_lines,
            source_set,
            ..Default::default()
        },
        test_order: vec![],
        test_order_display_names: vec![],
        cumulative: None,
    }
}

// ---------------------------------------------------------------------------
// Section extraction
// ---------------------------------------------------------------------------

/// Slice the lines from `## Source Set Summary` through the end of the
/// `## Coverage` section (i.e. up to but not including the next `## `
/// heading, or end-of-string). This is the ordering-sensitive window
/// for str-jeen.19.
fn extract_subordinates_section(md: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut started = false;
    let mut seen_coverage = false;
    for line in md.lines() {
        if line.starts_with("## Source Set Summary") {
            started = true;
            lines.push(line);
            continue;
        }
        if started {
            if line.starts_with("## Coverage") {
                seen_coverage = true;
                lines.push(line);
                continue;
            }
            if seen_coverage && line.starts_with("## ") {
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
// Tests
// ---------------------------------------------------------------------------

const PRODUCTIONISH_BULLET_PREFIX: &str = "- **Production-ish source lines:**";
const ATTEMPTED_SPAN_BULLET_PREFIX: &str = "- **Attempted-function span:**";
const COMPLETED_SUBSET_LABEL: &str = "(completed-functions subset)";
const OVERALL_COVERAGE_BULLET_PREFIX: &str = "- **Overall coverage";

fn line_index(md: &str, needle: &str) -> usize {
    md.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("expected to find a line containing `{needle}` in:\n{md}"))
}

#[test]
fn run_markdown_orders_whole_source_then_attempted_then_completed_subset() {
    let report = build_fixture_report();
    let md = format_markdown_report(&report);

    let production_ish_idx = line_index(&md, PRODUCTIONISH_BULLET_PREFIX);
    let attempted_span_idx = line_index(&md, ATTEMPTED_SPAN_BULLET_PREFIX);
    let coverage_idx = line_index(&md, COMPLETED_SUBSET_LABEL);

    assert!(
        production_ish_idx < attempted_span_idx,
        "whole-source bullet (line {production_ish_idx}) must precede \
         attempted-span bullet (line {attempted_span_idx}):\n{md}",
    );
    assert!(
        attempted_span_idx < coverage_idx,
        "attempted-span bullet (line {attempted_span_idx}) must precede \
         completed-functions coverage line (line {coverage_idx}):\n{md}",
    );

    // The subset label must live on the overall-coverage line, not just
    // appear elsewhere in the document.
    let coverage_line = md
        .lines()
        .find(|l| l.contains(OVERALL_COVERAGE_BULLET_PREFIX))
        .unwrap_or_else(|| panic!("missing overall-coverage bullet:\n{md}"));
    assert!(
        coverage_line.contains(COMPLETED_SUBSET_LABEL),
        "overall-coverage bullet must carry the `{COMPLETED_SUBSET_LABEL}` \
         subset label, got: {coverage_line}",
    );
}

#[test]
fn run_markdown_ordering_section_snapshot() {
    let report = build_fixture_report();
    let md = format_markdown_report(&report);
    let section = extract_subordinates_section(&md);

    // Sanity: the extracted window must contain both subordinate
    // sections in the right order.
    assert!(
        section.contains("## Source Set Summary"),
        "extracted section missing Source Set Summary heading:\n{section}",
    );
    assert!(
        section.contains("## Coverage"),
        "extracted section missing Coverage heading:\n{section}",
    );

    assert_snapshot(
        &snapshot_path("run_markdown_ordering.md"),
        &section,
    );
}
