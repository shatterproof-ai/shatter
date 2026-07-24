//! str-yhsp regression: `shatter run --concolic` must route the whole-repo
//! one-shot pipeline through the concolic (Z3-backed) orchestrator, not just
//! accept the flag. `run` has its own exploration loop separate from `scan`'s
//! orchestrator (the project's two parallel explorer paths), so a CLI-level
//! test that drives the real binary is the only thing that proves the flag is
//! actually wired to the concolic path end-to-end.
//!
//! Strategy: drive the real `shatter` binary with `run --concolic` against a
//! small Go fixture (the Go frontend is embedded, so it is always available)
//! whose function has clearly separated branches, and assert the run succeeds
//! and reports the function as explored. The Go frontend supports `prepare`,
//! so this exercises the concolic instrument → prepare → orchestrator path.

use std::process::Command;

mod common;

const GO_FIXTURE: &str = "package toy\n\n\
func Classify(n int) string {\n\
\tif n > 100 {\n\
\t\treturn \"big\"\n\
\t}\n\
\tif n < 0 {\n\
\t\treturn \"neg\"\n\
\t}\n\
\treturn \"small\"\n}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn run_concolic_explores_go_fixture_end_to_end() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();

    // Minimal Go module so discovery treats this as a Go project.
    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");

    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args([
            "run",
            project_root.to_str().expect("utf8 project path"),
            "--concolic",
            "--max-iterations",
            "15",
            "--timeout",
            "120",
        ])
        .output()
        .expect("invoke shatter run --concolic");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "run --concolic must exit 0 on a supported Go fixture.\n\
         status: {:?}\nstderr=\n{}\nstdout=\n{}",
        output.status,
        stderr,
        stdout,
    );

    // The concolic path produces the same report shape as the random path, so
    // the exploration-results table must list the target function. If the
    // concolic branch were disconnected (silently unsupported / errored out),
    // the function would land in exploration failures instead of the results
    // table.
    assert!(
        stdout.contains("Classify"),
        "run --concolic report must list the explored function `Classify`.\n\
         stdout=\n{}\nstderr=\n{}",
        stdout,
        stderr,
    );
    assert!(
        stdout.contains("Exploration Results"),
        "run --concolic must emit the exploration-results section.\n\
         stdout=\n{}\nstderr=\n{}",
        stdout,
        stderr,
    );
}
