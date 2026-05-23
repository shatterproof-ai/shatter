//! str-94cg: regression tests for the `--include` zero-match diagnostic.
//!
//! When a user passes `--include 'internal/runtime/*.go'` while scanning
//! `internal/runtime` directly, the pattern never matches because globs
//! are evaluated relative to the scan root. The diagnostic must name the
//! scan root and (where possible) suggest a corrected pattern.

use std::path::Path;
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn prepare_project(dir: &Path) {
    let shatter_dir = dir.join(".shatter");
    std::fs::create_dir_all(&shatter_dir).expect("create .shatter dir");
    std::fs::write(shatter_dir.join("config.yaml"), "").expect("write config.yaml");
}

/// Build a project layout that mirrors the str-94cg reproduction:
///
/// ```text
/// <tmp>/internal/runtime/api.go
/// <tmp>/go.mod
/// ```
fn write_runtime_fixture(dir: &Path) {
    std::fs::write(dir.join("go.mod"), "module example.com/dryrun\n\ngo 1.21\n")
        .expect("write go.mod");
    let runtime = dir.join("internal").join("runtime");
    std::fs::create_dir_all(&runtime).expect("create runtime dir");
    std::fs::write(
        runtime.join("api.go"),
        "package runtime\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write api.go");
}

#[test]
fn zero_match_include_pattern_emits_targeted_diagnostic() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    write_runtime_fixture(tmp.path());

    let runtime_dir = tmp.path().join("internal").join("runtime");

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            runtime_dir.to_str().unwrap(),
            "--language",
            "go",
            "--include",
            "internal/runtime/*.go",
            "--dry-run",
            "--stdout",
            "--no-cache",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter scan");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Diagnostic must name the supplied pattern.
    assert!(
        combined.contains("--include 'internal/runtime/*.go' matched 0 files"),
        "expected zero-match diagnostic naming the include pattern; got:\n{combined}",
    );
    // Diagnostic must name the scan root used as the pattern base.
    assert!(
        combined.contains("relative to scan root"),
        "expected diagnostic to explain pattern base; got:\n{combined}",
    );
    assert!(
        combined.contains(runtime_dir.to_str().unwrap()),
        "expected diagnostic to print the absolute scan root; got:\n{combined}",
    );
    // Diagnostic should suggest the corrected pattern.
    assert!(
        combined.contains("Try: --include '*.go'"),
        "expected diagnostic to suggest the stripped pattern '*.go'; got:\n{combined}",
    );
}

#[test]
fn no_include_pattern_keeps_existing_no_files_message() {
    // Without `--include`, the existing "No supported source files
    // found" message must remain (no new noise for the empty-dir case).
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());
    let empty_dir = tmp.path().join("empty");
    std::fs::create_dir_all(&empty_dir).expect("create empty dir");

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            empty_dir.to_str().unwrap(),
            "--language",
            "go",
            "--dry-run",
            "--stdout",
            "--no-cache",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter scan");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        combined.contains("No supported source files found"),
        "expected existing no-files message to remain; got:\n{combined}",
    );
    // The include-pattern diagnostic must NOT fire when no --include was passed.
    assert!(
        !combined.contains("matched 0 files"),
        "include-pattern diagnostic must not fire without --include; got:\n{combined}",
    );
}
