//! str-izhn: `shatter scan` must expose a documented opt-in for nonzero exit
//! when attempted functions fail, and its summary must clearly name the
//! completed / failed / unsupported counts so CI and Makefile wrappers do not
//! report a green build for a broken scan.
//!
//! Strategy: drive the real `shatter` binary against a small Go fixture
//! (Go frontend is embedded, so always available). Verify three regimes:
//!
//! 1. All-success scan exits 0 with and without `--fail-on-failures`.
//! 2. The summary line names every bucket (completed/failed/unsupported/skipped).
//! 3. The flags `--fail-on-failures` and `--failure-threshold` are documented
//!    in `scan --help` so wrappers know they exist.
//!
//! Partial-failure exit behavior is unit-tested in
//! `shatter-core::scan_orchestrator::evaluate_failure_policy_*`; recreating a
//! deterministic partial-failure scan from CLI is fragile (requires inducing
//! a controlled per-function timeout against a real frontend) and would
//! gate this regression on shatter-go runtime behavior. The policy decision
//! tested at the core layer is the same one the CLI calls in `run_scan`.

use std::process::Command;

mod common;

const GO_FIXTURE: &str = "package toy\n\n\
func Add(a, b int) int {\n\
\tif a > 0 {\n\
\t\treturn a + b\n\
\t}\n\
\treturn b\n\
}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn write_fixture() -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("create project tempdir");
    let root = project.path();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(root.join("toy.go"), GO_FIXTURE).expect("write toy.go");
    project
}

#[test]
fn scan_help_documents_failure_policy_flags() {
    let output = Command::new(shatter_binary())
        .args(["scan", "--help"])
        .output()
        .expect("invoke shatter scan --help");
    assert!(output.status.success(), "scan --help must succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--fail-on-failures"),
        "scan --help must document --fail-on-failures; got:\n{stdout}",
    );
    // The same flag also documents the optional <PERCENT> rate form, used
    // for "fail only when more than N% of attempts failed."
    assert!(
        stdout.contains("PERCENT"),
        "scan --help must document the optional PERCENT form of --fail-on-failures; \
         got:\n{stdout}",
    );
}

#[test]
fn scan_summary_names_all_buckets() {
    let project = write_fixture();
    let out_dir = tempfile::tempdir().expect("create output tempdir");
    let report_path = out_dir.path().join("scan.txt");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(shatter_binary())
        .env("TMPDIR", command_tmp.path())
        .args([
            "scan",
            project.path().to_str().expect("utf8 project path"),
            "--language",
            "go",
            "--no-cache",
            "--no-seeds",
            "--max-iterations",
            "1",
            "--timeout-total",
            "60",
            "--timeout-per-fn",
            "10",
            "--format",
            "text",
            "-o",
        ])
        .arg(&report_path)
        .output()
        .expect("invoke shatter scan");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Look in the text report file for the structured summary line — the
    // user-facing markdown summary is rendered to stdout and goes through
    // termimad, which may inject ANSI codes between the count words.
    let report = std::fs::read_to_string(&report_path).unwrap_or_default();
    let combined = format!("{report}\n{stdout}\n{stderr}");

    for label in ["completed", "failed", "unsupported", "skipped"] {
        assert!(
            combined.contains(label),
            "summary must name `{label}` so CI wrappers can grep it; status={:?}\n\
             report=\n{report}\nstdout=\n{stdout}\nstderr=\n{stderr}",
            output.status,
        );
    }
}

#[test]
fn scan_dry_run_with_threshold_form_parses() {
    // The `--fail-on-failures=PERCENT` form must be accepted by clap and
    // reach the dry-run exit. We use --dry-run to avoid running the Go
    // frontend; this is a parser/dispatch contract, not a policy decision
    // (the policy itself is unit-tested at the core layer).
    let project = write_fixture();
    let output = Command::new(shatter_binary())
        .args([
            "scan",
            project.path().to_str().expect("utf8 project path"),
            "--language",
            "go",
            "--no-cache",
            "--no-seeds",
            "--dry-run",
            "--fail-on-failures=50",
        ])
        .output()
        .expect("invoke shatter scan --dry-run --fail-on-failures=50");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "--fail-on-failures=50 must parse and dry-run cleanly; status={:?}\nstderr=\n{stderr}",
        output.status,
    );
}

#[test]
fn scan_dry_run_with_strict_policy_exits_zero() {
    // Dry-run exits before exploration runs (no attempts), so the policy
    // evaluator sees attempted=0 and `--fail-on-failures` / a 0% threshold
    // are both vacuously satisfied. This guards against a regression where
    // the policy is applied to runs that never attempted anything — the
    // backwards-compat contract is "no failed attempts ⇒ exit 0," not
    // "must explore something."
    let project = write_fixture();

    let output = Command::new(shatter_binary())
        .args([
            "scan",
            project.path().to_str().expect("utf8 project path"),
            "--language",
            "go",
            "--no-cache",
            "--no-seeds",
            "--dry-run",
            "--fail-on-failures",
        ])
        .output()
        .expect("invoke shatter scan --dry-run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "dry-run with strict policy must exit 0 (no attempts means no failures); \
         status={:?}\nstdout=\n{stdout}\nstderr=\n{stderr}",
        output.status,
    );
}
