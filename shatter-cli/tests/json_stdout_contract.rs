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

/// str-mpwp: explore's `--format` no longer accepts `json`. Clap rejects
/// the value upfront so help text and runtime behavior agree — the help
/// listing for `--format` lists only markdown/html/text, and any attempt
/// to pass `json` fails with a clap value-validation error that points
/// at the supported values. JSON output is reachable only via
/// `-o <file>.json`, which writes a spec bundle.
#[test]
fn explore_json_stdout_is_rejected_by_clap() {
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
    // Clap's invalid-value diagnostic names the offending value and the
    // flag, and lists the valid alternatives. We assert all three pieces
    // so the failure message points the user at the fix.
    assert!(
        stderr.contains("'json'") && stderr.contains("--format"),
        "stderr should identify the rejected value and flag, got: {stderr}",
    );
    assert!(
        stderr.contains("markdown") && stderr.contains("html") && stderr.contains("text"),
        "stderr should list the supported --format values, got: {stderr}",
    );
    // Clap echoes the rejected value in the error message, so "json" will
    // appear there — but it must not appear in the [possible values] list.
    if let Some(pv_idx) = stderr.find("[possible values:") {
        let pv_block = &stderr[pv_idx..];
        assert!(
            !pv_block.contains("json"),
            "possible-values list must not include 'json', got: {pv_block}",
        );
    }
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty when --format json is rejected, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn explore_json_default_stdout_is_rejected_by_clap() {
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
        stderr.contains("'json'") && stderr.contains("--format"),
        "stderr should identify the rejected value and flag, got: {stderr}",
    );
}

#[test]
fn explore_from_artifacts_json_stdout_is_rejected_by_clap() {
    // The resumed-artifact case: clap rejects `--format json` before any
    // artifact reading happens.
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
        stderr.contains("'json'") && stderr.contains("--format"),
        "stderr should identify the rejected value and flag, got: {stderr}",
    );
    assert!(output.stdout.is_empty());
}

/// str-mpwp: `explore --help` must agree with runtime behavior. The
/// `--format` help line must NOT advertise `json` as a possible value,
/// and the help text must point at `-o <file>.json` as the JSON sink.
#[test]
fn explore_help_does_not_advertise_json_format() {
    let output = Command::new(shatter_binary())
        .args(["explore", "--help"])
        .output()
        .expect("invoke shatter explore --help");

    assert!(
        output.status.success(),
        "explore --help should succeed; stderr={:?}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Locate the `--format` block so we only inspect that flag's help text
    // (the broader help mentions `.json` in the -o description, which is
    // expected and points users at the supported JSON sink).
    let format_idx = stdout
        .find("--format")
        .unwrap_or_else(|| panic!("--format not in explore --help: {stdout}"));
    // Inspect a window large enough to cover the multi-line help text.
    let window_end = (format_idx + 600).min(stdout.len());
    let format_block = &stdout[format_idx..window_end];

    assert!(
        format_block.contains("markdown")
            && format_block.contains("html")
            && format_block.contains("text"),
        "explore --format help should list markdown/html/text, got: {format_block}",
    );
    assert!(
        !format_block.contains("json"),
        "explore --format help must not list 'json' as a value, got: {format_block}",
    );

    // The broader help must direct users to the supported JSON sink so
    // the help/runtime contract is self-documenting.
    assert!(
        stdout.contains(".json"),
        "explore --help should mention `.json` as the file-based JSON sink, got: {stdout}",
    );
}

#[test]
fn explore_json_to_file_is_accepted() {
    // `-o <file>.json` is the supported JSON sink for explore. We don't
    // run a real frontend here; we only confirm clap does not reject the
    // invocation and no JSON-stdout rejection message appears (the
    // command may still fail downstream for unrelated reasons).
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
        !stderr.contains("--format json"),
        "JSON-to-file path must not trigger any --format json rejection, got: {stderr}",
    );
}
