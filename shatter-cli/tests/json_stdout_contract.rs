//! str-tzbr / str-nnty: regression tests for the CLI JSON-stdout contract.
//!
//! str-nnty extended `scan --dry-run` to emit a machine-readable plan
//! when JSON output is requested (either `--format json --stdout` or
//! `-o file.json`). The explore command still rejects `--format json`
//! on stdout because it has no equivalent plan shape.

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

/// Build a minimal Go target so `scan` has something to enumerate.
fn write_go_fixture(dir: &Path) {
    std::fs::write(
        dir.join("go.mod"),
        "module example.com/dryrun\n\ngo 1.21\n",
    )
    .expect("write go.mod");
    std::fs::write(
        dir.join("main.go"),
        "package main\n\nfunc Add(a, b int) int { return a + b }\n\nfunc main() {}\n",
    )
    .expect("write main.go");
}

/// str-nnty: `scan --dry-run --stdout --format json` emits a structured
/// plan to stdout. Replaces the prior str-tzbr rejection contract.
#[test]
fn scan_dry_run_json_stdout_emits_plan() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    write_go_fixture(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            ".",
            "--language",
            "go",
            "--dry-run",
            "--stdout",
            "--format",
            "json",
            "--no-cache",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected zero exit; status={:?} stdout={stdout:?} stderr={stderr:?}",
        output.status,
    );

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("stdout must be parseable JSON; err={e} stdout={stdout:?}");
    });

    assert_eq!(parsed["schema_version"], serde_json::json!(1));
    assert_eq!(parsed["kind"], serde_json::json!("scan_dry_run_plan"));
    assert!(parsed["summary"].is_object(), "summary missing");
    assert!(parsed["layers"].is_array(), "layers missing");
    assert!(parsed["skipped"].is_array(), "skipped missing");
    assert!(parsed["config"].is_object(), "config missing");

    // str-nnty acceptance: stdout JSON must not carry informational log
    // noise interleaved with the document under `--color never --render plain`.
    // A single trailing newline is acceptable; anything else would break
    // JSON parsing of the raw stream.
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "stdout must be a single JSON object with no interleaved noise; got: {stdout:?}",
    );
}

/// str-nnty: `scan --dry-run --format json` (no `--stdout`, no `-o`)
/// defaults to stdout and emits the plan as JSON.
#[test]
fn scan_dry_run_json_default_stdout_emits_plan() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    write_go_fixture(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            ".",
            "--language",
            "go",
            "--dry-run",
            "--format",
            "json",
            "--no-cache",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter");

    assert!(output.status.success(), "{:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout must be parseable JSON");
    assert_eq!(parsed["kind"], serde_json::json!("scan_dry_run_plan"));
}

/// str-nnty (was str-jeen.54): `scan --dry-run -o <file>.json` now
/// writes the JSON plan to the named file instead of skipping it with
/// a warning.
#[test]
fn scan_dry_run_with_json_output_file_succeeds() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    write_go_fixture(tmp.path());

    let json_out = tmp.path().join("scan-report.json");

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            ".",
            "--language",
            "go",
            "--dry-run",
            "--no-cache",
            "--no-seeds",
            "-o",
            json_out.to_str().unwrap(),
        ])
        .output()
        .expect("invoke shatter");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "expected --dry-run with -o <file>.json to succeed; \
         status={:?} stdout={stdout:?} stderr={stderr:?}",
        output.status,
    );
    assert!(
        json_out.exists(),
        "dry-run must write the JSON plan to {json_out:?}",
    );
    let plan_bytes = std::fs::read(&json_out).expect("read plan file");
    let parsed: serde_json::Value = serde_json::from_slice(&plan_bytes)
        .expect("plan file must be parseable JSON");
    assert_eq!(parsed["kind"], serde_json::json!("scan_dry_run_plan"));
    assert!(parsed["summary"]["included_functions"].is_number());
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
