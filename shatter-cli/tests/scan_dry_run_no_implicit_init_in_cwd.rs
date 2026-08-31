//! str-vr7vq regression: implicit init must resolve the project root from
//! the directory actually being scanned, not from the process's ambient
//! working directory.
//!
//! Before this fix, `maybe_implicit_init` (in `main.rs`) resolved its target
//! directory from `--project-dir` only, falling back to
//! `std::env::current_dir()` whenever that flag was absent -- even though
//! `run_scan` itself resolves its own `project_root_str` from the scan
//! target directory a few lines later. Since `--project-dir` is rarely
//! passed, any `scan <dir>` invocation (dry-run included, since implicit
//! init runs before the dry-run short-circuit) launched from an unrelated
//! working directory wrote a stray `.shatter/` + managed `.gitignore` block
//! into that ambient cwd instead of into the directory being scanned. This
//! was reproducible via `cargo test`/`cargo nextest run`, whose default
//! working directory for an integration test binary is the crate's own
//! manifest directory: `scan_failure_policy.rs`'s
//! `scan_dry_run_with_threshold_form_parses` and
//! `scan_dry_run_with_strict_policy_exits_zero` tests pass neither
//! `--project-dir` nor `-o`/`--no-cache`/`--no-seeds` audit-mode flags (the
//! `-o` output flag, specifically), nor pin `.current_dir`, so they left a
//! `.shatter/` and `.gitignore` behind in `shatter-cli/` on every run --
//! the exact symptom this issue reports.
//!
//! Strategy: pin `.current_dir` to a fresh, uninitialized "ambient" tempdir
//! that is NOT the scan target, pass the scan target as an unrelated second
//! tempdir, and assert implicit init lands in the target, never the ambient
//! cwd.

use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

const GO_FIXTURE: &str = "package toy\n\n\
func Add(a, b int) int {\n\
\tif a > 0 {\n\
\t\treturn a + b\n\
\t}\n\
\treturn b\n\
}\n";

#[test]
fn scan_dry_run_implicit_init_targets_the_scanned_directory_not_the_cwd() {
    let ambient_cwd = tempfile::tempdir().expect("create ambient cwd tempdir");
    let project = tempfile::tempdir().expect("create scan target tempdir");
    std::fs::write(project.path().join("go.mod"), "module toy\n\ngo 1.21\n")
        .expect("write go.mod");
    std::fs::write(project.path().join("toy.go"), GO_FIXTURE).expect("write toy.go");

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(ambient_cwd.path())
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
        .expect("invoke shatter scan --dry-run");

    assert!(
        output.status.success(),
        "dry-run with a threshold policy must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        !ambient_cwd.path().join(".shatter").exists(),
        "implicit init must never target the ambient cwd -- it must resolve \
         the project root from the directory being scanned"
    );
    assert!(
        !ambient_cwd.path().join(".gitignore").exists(),
        "implicit init must never write a managed .gitignore block into the \
         ambient cwd"
    );
    assert!(
        project.path().join(".shatter").exists(),
        "implicit init must still initialize the actual scan target \
         (first-run ergonomics preserved)"
    );
}
