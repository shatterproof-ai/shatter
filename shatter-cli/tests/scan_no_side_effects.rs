//! str-1wcl regression: when `shatter scan` is invoked with explicit external
//! output paths AND both caches and seeds disabled, no project-local artifact
//! or cache directories must be created. Shatter must behave as a clean
//! external audit tool under that flag combination.
//!
//! Strategy: drive the real `shatter` binary against a small Go fixture in a
//! tempdir (Go frontend is embedded, so it is always available), pass `-o
//! <other-tempdir>/scan.json --no-cache --no-seeds`, and assert that neither
//! `<project>/shatter-artifacts/` nor `<project>/.shatter-cache/` exists after
//! the run.
//!
//! Picks `--max-iterations 1` and a small `--timeout-total` so the scan
//! finishes quickly; the side-effect contract is what is being asserted, not
//! coverage.

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

#[test]
fn scan_with_external_output_and_no_cache_no_seeds_writes_no_project_local_dirs() {
    // The scanned project lives in its own tempdir so we can assert nothing
    // appears under it after the run.
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();

    // Minimal Go module so discovery treats this as a Go project.
    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");

    // Output goes to a separate tempdir — explicit external path.
    let out_dir = tempfile::tempdir().expect("create output tempdir");
    let report_path = out_dir.path().join("scan.json");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args([
            "scan",
            project_root.to_str().expect("utf8 project path"),
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
            "-o",
        ])
        .arg(&report_path)
        .output()
        .expect("invoke shatter scan");

    // The scan itself does not have to succeed for this contract — the side-
    // effect rule must hold even on partial failure. Surface the exit status
    // in assertion messages to make debugging tractable.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let artifacts_dir = project_root.join("shatter-artifacts");
    assert!(
        !artifacts_dir.exists(),
        "scan with `-o <external> --no-cache --no-seeds` must not create \
         <project>/shatter-artifacts/. Found: {}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        artifacts_dir.display(),
        output.status,
        stderr,
        stdout,
    );

    let cache_dir = project_root.join(".shatter-cache");
    assert!(
        !cache_dir.exists(),
        "scan with `-o <external> --no-cache --no-seeds` must not create \
         <project>/.shatter-cache/. Found: {}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        cache_dir.display(),
        output.status,
        stderr,
        stdout,
    );

    // Same rule for the seed pool: `--no-seeds` must already prevent any
    // .shatter/ writes in the project root from this command.
    let seeds_dir = project_root.join(".shatter");
    assert!(
        !seeds_dir.exists(),
        "scan with `--no-seeds` must not create <project>/.shatter/. Found: {}",
        seeds_dir.display(),
    );
}
