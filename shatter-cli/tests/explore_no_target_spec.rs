//! Regression test for str-jeen.67: `shatter explore --spec-out` must emit a
//! machine-readable no-target marker bundle when a file has no executable
//! targets. Without this, batch tooling sees a missing spec file and
//! misclassifies the run as "partial" (the original Kapow bug — 624 of
//! 1,126 files were tagged "partial" even though many were legitimate
//! no-target files).
//!
//! Strategy: drive `shatter explore --from-artifacts --spec-out` with a
//! synthesized `summary.json` recording zero discovered functions and a
//! refined `no_target_reason`. This exercises the finalize path that owns
//! the spec write without depending on a particular language frontend.

use std::process::Command;

use serde_json::json;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn write_no_target_summary(target_dir: &std::path::Path, file: &str, reason: &str) {
    std::fs::create_dir_all(target_dir).expect("create target dir");
    let summary = json!({
        "version": 2,
        "status": "completed",
        "file": file,
        "total_functions": 0,
        "completed": 0,
        "failed": 0,
        "skipped": 0,
        "elapsed_secs": 0.0,
        "build_failed": 0,
        "runtime_failed": 0,
        "timed_out": 0,
        "unsupported": 0,
        "skipped_by_policy": 0,
        "produced_coverage": 0,
        "no_target_reason": reason,
        "functions": []
    });
    std::fs::write(
        target_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .expect("write summary.json");
}

/// Covers the TypeScript `.d.ts` declaration-only case from the issue
/// acceptance criteria.
#[test]
fn explore_spec_out_emits_no_target_marker_for_declaration_only_ts() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let artifact_root = tmp.path();
    write_no_target_summary(
        &artifact_root.join("types_d_ts"),
        "src/types.d.ts",
        "declaration_only",
    );
    let spec_out = tmp.path().join("spec.json");

    let output = Command::new(shatter_binary())
        .arg("explore")
        .arg("--from-artifacts")
        .arg(artifact_root)
        .arg("--spec-out")
        .arg(&spec_out)
        .arg("placeholder.ts")
        .output()
        .expect("invoke shatter explore");

    assert!(
        output.status.success(),
        "no-target run must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        spec_out.exists(),
        "spec file must be written even when no targets were discovered \
         (str-jeen.67) — stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );

    let raw = std::fs::read_to_string(&spec_out).expect("read spec output");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("spec is valid JSON");
    assert_eq!(
        parsed["status"], "no_targets",
        "spec bundle must carry status=no_targets so batch tooling can \
         classify the run; got:\n{raw}",
    );
    assert_eq!(
        parsed["no_target_reason"], "declaration_only",
        "spec bundle must surface the closed-taxonomy reason; got:\n{raw}",
    );
    assert!(
        parsed["functions"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "no-target bundle must carry an empty functions list; got:\n{raw}",
    );
}

/// Covers the Go-with-no-discoverable-functions case from the issue
/// acceptance criteria. We use `unclassified` to verify the default path
/// (no refined classifier matched) still produces a machine-readable
/// marker — that is what batch tooling needs to stop guessing.
#[test]
fn explore_spec_out_emits_no_target_marker_for_go_with_no_functions() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let artifact_root = tmp.path();
    write_no_target_summary(
        &artifact_root.join("empty_go"),
        "src/empty.go",
        "unclassified",
    );
    let spec_out = tmp.path().join("spec.json");

    let output = Command::new(shatter_binary())
        .arg("explore")
        .arg("--from-artifacts")
        .arg(artifact_root)
        .arg("--spec-out")
        .arg(&spec_out)
        .arg("placeholder.go")
        .output()
        .expect("invoke shatter explore");

    assert!(
        output.status.success(),
        "no-target run must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(spec_out.exists(), "spec file must be written");
    let raw = std::fs::read_to_string(&spec_out).expect("read spec output");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("spec is valid JSON");
    assert_eq!(parsed["status"], "no_targets");
    assert_eq!(parsed["no_target_reason"], "unclassified");
}
