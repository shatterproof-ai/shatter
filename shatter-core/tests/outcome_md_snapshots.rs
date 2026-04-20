//! Regression snapshot tests for the outcome-driven markdown renderer.
//!
//! These tests capture the exact output of `render_explore_outcomes` for a
//! fixture that contains a deliberately failing function. They ensure that a
//! discovered target with `build_failed` or `unsupported` status appears in
//! the markdown — not as a missing entry — and that an empty discovery
//! produces an explicit "no targets discovered" section rather than an empty
//! file.
//!
//! # First-run behavior
//! If a snapshot file does not yet exist the current output is written to
//! disk and the test passes. On subsequent runs the normalised actual output
//! is compared against the stored snapshot.

use std::path::Path;

use shatter_core::protocol::OutcomeStatus;
use shatter_core::report::{OutcomeRenderEntry, render_explore_outcomes};

// ---------------------------------------------------------------------------
// Whitespace normalisation (same helper as html_snapshots.rs)
// ---------------------------------------------------------------------------

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
// Snapshot helper
// ---------------------------------------------------------------------------

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
        "Markdown snapshot mismatch for {}.\n\
         Delete the snapshot file and re-run the test to regenerate it.",
        path.display()
    );
}

fn snapshot_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests/snapshots").join(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Snapshot: a fixture with one `build_failed` function and one `completed`
/// function. The `build_failed` entry must appear in the markdown with the
/// correct status label — it must not be absent or silently dropped.
#[test]
fn snapshot_outcome_md_with_build_failed_function() {
    let entries = vec![
        OutcomeRenderEntry {
            qualified_name: "examples/go/failing-build.go:FailingFunc",
            status: OutcomeStatus::BuildFailed,
            reason: "go build returned exit code 1: undefined: unknownPackage",
            detail_md: None,
        },
        OutcomeRenderEntry {
            qualified_name: "examples/go/failing-build.go:PassingFunc",
            status: OutcomeStatus::Completed,
            reason: "explored 3 paths",
            detail_md: Some("### Paths\n- input=0 → return=0\n- input=1 → return=1\n- input=-1 → return=-1\n"),
        },
    ];

    let md = render_explore_outcomes(&entries, "");

    // Non-empty output — no zero-byte report.
    assert!(!md.is_empty(), "render_explore_outcomes must not return empty string");

    // build_failed function is present with correct status.
    assert!(md.contains("examples/go/failing-build.go:FailingFunc"), "failing function must appear");
    assert!(md.contains("`build_failed`"), "build_failed status must appear");
    assert!(md.contains("go build returned exit code 1"), "build error reason must appear");

    // Completed function also appears.
    assert!(md.contains("examples/go/failing-build.go:PassingFunc"), "passing function must appear");
    assert!(md.contains("`completed`"), "completed status must appear");
    assert!(md.contains("### Paths"), "detail block must appear for completed entry");

    assert_snapshot(&snapshot_path("outcome_md_with_build_failed.md"), &md);
}

/// Snapshot: a fixture with one `unsupported` function. Verifies that
/// `unsupported` targets appear in the markdown (not silently dropped).
#[test]
fn snapshot_outcome_md_with_unsupported_function() {
    let entries = vec![OutcomeRenderEntry {
        qualified_name: "examples/go/unsupported.go:UnsupportedFunc",
        status: OutcomeStatus::Unsupported,
        reason: "method invocation requires receiver planning (Phase E)",
        detail_md: None,
    }];

    let md = render_explore_outcomes(&entries, "");

    assert!(!md.is_empty(), "render_explore_outcomes must not return empty string");
    assert!(md.contains("examples/go/unsupported.go:UnsupportedFunc"), "unsupported function must appear");
    assert!(md.contains("`unsupported`"), "unsupported status must appear");
    assert!(md.contains("receiver planning"), "reason must appear");
    // Detail block must NOT appear for unsupported outcomes.
    assert!(!md.contains("### Paths"), "detail block must not appear for unsupported entry");

    assert_snapshot(&snapshot_path("outcome_md_with_unsupported.md"), &md);
}

/// Snapshot: empty outcome list → explicit "no targets discovered" section,
/// never an empty file.
#[test]
fn snapshot_outcome_md_empty_discovery() {
    let md = render_explore_outcomes(&[], "the analyzer returned no exportable functions");

    assert!(!md.is_empty(), "empty discovery must not produce empty output");
    assert!(md.contains("No targets discovered"), "must emit no-targets heading");
    assert!(md.contains("no exportable functions"), "must include caller-supplied reason");

    assert_snapshot(&snapshot_path("outcome_md_empty_discovery.md"), &md);
}
