//! str-k9y5 regression: when `shatter explore` is invoked with explicit
//! external `--output` AND both caches and seeds disabled, no project-local
//! artifact or cache directories must be created inside the target module.
//! Mirrors the str-1wcl contract for `scan` (see scan_no_side_effects.rs).
//!
//! Strategy: drive the real `shatter` binary against a small Go module nested
//! inside an outer project tempdir (Go frontend is embedded, so always
//! available), pass `--output <other-tempdir>/report.md --no-cache --no-seeds`
//! and `--project-dir` pointing at the inner module, and assert that none of
//! `.shatter/`, `.shatter-cache/`, or `shatter-artifacts/` appears under the
//! inner module after the run.
//!
//! Picks tight budgets so the explore finishes quickly; the side-effect
//! contract is what is being asserted, not coverage.

use std::process::Command;

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
fn explore_with_external_output_and_no_cache_no_seeds_writes_no_project_local_dirs() {
    // Nested layout: outer project tempdir contains an inner `api/` module
    // with its own go.mod. We point `--project-dir` at the inner module to
    // match the reproduction in str-k9y5.
    let project = tempfile::tempdir().expect("create project tempdir");
    let outer_root = project.path();
    let inner_root = outer_root.join("api");
    std::fs::create_dir_all(&inner_root).expect("create inner module dir");

    // Outer module so the layout looks like a typical multi-module repo.
    std::fs::write(outer_root.join("go.mod"), "module outer\n\ngo 1.21\n")
        .expect("write outer go.mod");
    // Inner nested Go module with the target.
    std::fs::write(inner_root.join("go.mod"), "module outer/api\n\ngo 1.21\n")
        .expect("write inner go.mod");
    let target_file = inner_root.join("toy.go");
    std::fs::write(&target_file, GO_FIXTURE).expect("write toy.go");

    // Output goes to a separate tempdir — explicit external path.
    let out_dir = tempfile::tempdir().expect("create output tempdir");
    let report_path = out_dir.path().join("report.md");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let target_arg = format!("{}:Add", target_file.to_str().expect("utf8 target"));

    let output = Command::new(shatter_binary())
        .env("TMPDIR", command_tmp.path())
        .args([
            "explore",
            &target_arg,
            "--project-dir",
            inner_root.to_str().expect("utf8 inner root"),
            "--max-iterations",
            "1",
            "--timeout-explore",
            "30",
            "--exec-timeout",
            "10",
            "--build-timeout",
            "60",
            "--workers",
            "1",
            "--parallelism-min",
            "1",
            "--parallelism-max",
            "1",
            "--no-cache",
            "--no-seeds",
            "--output",
        ])
        .arg(&report_path)
        .output()
        .expect("invoke shatter explore");

    // The explore itself does not have to succeed for this contract — the
    // side-effect rule must hold even on partial failure. Surface the exit
    // status in assertion messages to make debugging tractable.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let artifacts_dir = inner_root.join("shatter-artifacts");
    assert!(
        !artifacts_dir.exists(),
        "explore with `--output <external> --no-cache --no-seeds` must not \
         create <project>/shatter-artifacts/. Found: {}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        artifacts_dir.display(),
        output.status,
        stderr,
        stdout,
    );

    let cache_dir = inner_root.join(".shatter-cache");
    assert!(
        !cache_dir.exists(),
        "explore with `--output <external> --no-cache --no-seeds` must not \
         create <project>/.shatter-cache/. Found: {}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        cache_dir.display(),
        output.status,
        stderr,
        stdout,
    );

    let shatter_dir = inner_root.join(".shatter");
    assert!(
        !shatter_dir.exists(),
        "explore with `--output <external> --no-cache --no-seeds` must not \
         create <project>/.shatter/. Found: {}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        shatter_dir.display(),
        output.status,
        stderr,
        stdout,
    );
}
