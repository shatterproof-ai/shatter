//! str-tzbr: regression tests for the CLI JSON-stdout contract.
//!
//! Choice documented in `commands/scan.rs` and `commands/explore.rs`:
//! we **reject** the unsupported `--stdout --format json` combos (and
//! the equivalent default-stdout cases) before any work, instead of
//! silently shipping Markdown to a JSON-tagged stream. Both commands
//! retain their existing JSON-to-file path via `-o <file>.json`, and
//! `scan --stdout --format json` (without `--dry-run`) continues to
//! emit the documented `scan_report` shape.

use std::path::Path;
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

/// Pre-create `.shatter/` so the CLI's implicit init does not write
/// status lines to stdout (which would otherwise contaminate the
/// "stdout must be empty on rejection" assertions). Init messages are
/// command-output, not log/progress, so the JSON contract assertions
/// only need to reason about post-init stdout.
fn prepare_project(dir: &Path) {
    let shatter_dir = dir.join(".shatter");
    std::fs::create_dir_all(&shatter_dir).expect("create .shatter dir");
    std::fs::write(shatter_dir.join("config.yaml"), "").expect("write config.yaml");
}

#[test]
fn scan_dry_run_json_stdout_is_rejected() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            ".",
            "--dry-run",
            "--stdout",
            "--format",
            "json",
        ])
        .output()
        .expect("invoke shatter");

    assert!(
        !output.status.success(),
        "expected non-zero exit; status={:?} stdout={:?} stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--dry-run") && stderr.contains("--format json"),
        "stderr should explain the rejected combo, got: {stderr}",
    );
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty when JSON combo is rejected, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn scan_dry_run_json_default_stdout_is_rejected() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args(["scan", ".", "--dry-run", "--format", "json"])
        .output()
        .expect("invoke shatter");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn explore_json_stdout_is_rejected() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "explore",
            "nonexistent.go:Func",
            "--stdout",
            "--format",
            "json",
        ])
        .output()
        .expect("invoke shatter");

    assert!(
        !output.status.success(),
        "expected non-zero exit; stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("explore --format json") && stderr.contains("stdout"),
        "stderr should explain the rejected combo, got: {stderr}",
    );
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty when JSON combo is rejected, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn explore_json_default_stdout_is_rejected() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args(["explore", "nonexistent.go:Func", "--format", "json"])
        .output()
        .expect("invoke shatter");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("explore --format json"),
        "stderr should explain the rejected combo, got: {stderr}",
    );
}

#[test]
fn explore_from_artifacts_json_stdout_is_rejected() {
    // The resumed-artifact case: even when a prior artifact dir exists,
    // --stdout --format json must be rejected before reading artifacts.
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    let artifacts = tmp.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "explore",
            "nonexistent.go:Func",
            "--from-artifacts",
            artifacts.to_str().unwrap(),
            "--stdout",
            "--format",
            "json",
        ])
        .output()
        .expect("invoke shatter");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("explore --format json"),
        "stderr should explain the rejected combo, got: {stderr}",
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn explore_json_to_file_does_not_trigger_stdout_rejection() {
    // `-o <file>.json` is the supported JSON sink for explore. We don't
    // run a real frontend here; we just confirm the rejection does NOT
    // fire when JSON targets a file (the command then proceeds and may
    // fail downstream for unrelated reasons — we only assert the stderr
    // does not contain the JSON-stdout rejection message).
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    let out_path = tmp.path().join("out.json");

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "explore",
            "nonexistent.go:Func",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .output()
        .expect("invoke shatter");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("explore --format json is not supported on stdout"),
        "JSON-to-file path must not trigger the JSON-stdout rejection, got: {stderr}",
    );
}
