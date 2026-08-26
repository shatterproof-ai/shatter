//! str-9fn2 review follow-up: the process-level `exit_code_conventions.rs`
//! suite only covers `spec-diff`; `run`'s coverage-budget gate and `scan`'s
//! `--fail-on-failures` gate are the other two named-in-issue paths that go
//! through the new `GateFailure` machinery (as opposed to spec-diff/compare/
//! stale's pre-existing `Ok(bool)` pattern), and were previously exercised
//! only indirectly via unit tests on `error_exit_code`'s `GateFailure`
//! downcast. This drives the real `shatter` binary against `run`'s
//! `--min-source-representation-percent` gate to prove exit code 1 actually
//! reaches the process boundary from that specific code path.
//!
//! `scan --fail-on-failures`'s own exit-1 path is deliberately NOT given an
//! equivalent CLI-level test here: `scan_failure_policy.rs` already documents
//! why a deterministic partial-failure scan is fragile to recreate from the
//! CLI (it requires inducing a controlled per-function failure/timeout
//! against a real frontend) and defers that regression to the core-layer
//! `evaluate_failure_policy` unit tests, which call the identical decision
//! function `run_scan` uses. `run`'s coverage-budget gate does not share that
//! fragility: an unreachably high `--min-source-representation-percent`
//! against a tiny fixture fails deterministically without needing a
//! frontend timeout.

use std::process::Command;

mod common;

/// A branch random exploration is astronomically unlikely to reach in a
/// handful of iterations, so source-representation coverage reliably stays
/// well under 100% within a tiny iteration budget — the gate must fire.
const GO_FIXTURE: &str = "package toy\n\n\
func Unlock(code int64) string {\n\
\tif code == 8675309 {\n\
\t\treturn \"granted\"\n\
\t}\n\
\treturn \"denied\"\n}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn run_coverage_budget_gate_exits_with_gate_code() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();
    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args([
            "run",
            project_root.to_str().expect("utf8 project path"),
            "--max-iterations",
            "3",
            "--timeout",
            "120",
            "--min-source-representation-percent",
            "100",
        ])
        .output()
        .expect("invoke shatter run");

    assert_eq!(
        output.status.code(),
        Some(1),
        "an unreachable coverage-budget gate must exit 1 (gate fired), not 0 \
         or the tool-error code 2; stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
