//! str-bni0: clean-audit scans must not leave `.shatter-launchers/` or
//! other untracked artefacts in the target project tree.
//!
//! The Go frontend builds per-target launcher binaries from a transient
//! `.shatter-launchers/<hash>-<pid>/` source directory inside the target
//! module (required for Go's `internal/` package visibility). Earlier
//! versions cleaned the directory via a `defer` that ran on normal returns
//! but was skipped on signal-induced exit, leaving the dir behind. They also
//! did not sweep orphans left by previous interrupted runs.
//!
//! This test exercises the user-visible contract: after a `--no-cache
//! --no-seeds -o /tmp/...` scan against a Go fixture, the fixture directory
//! must not contain a `.shatter-launchers/` entry, and a stale orphan
//! `.shatter-launchers/<hash>-<dead-pid>/` placed before the scan must be
//! gone afterward.

use std::path::Path;
use std::process::Command;

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst");
    for entry in std::fs::read_dir(src).expect("read src") {
        let entry = entry.expect("entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type().expect("file_type");
        if ft.is_dir() {
            copy_dir_recursive(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy file");
        }
    }
}

#[test]
fn scan_with_no_cache_no_seeds_does_not_dirty_target_project() {
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("skipping: `go` not available on PATH");
        return;
    }

    let binary = env!("CARGO_BIN_EXE_shatter");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = Path::new(manifest_dir)
        .parent()
        .expect("workspace root parent");
    let fixture_src = repo_root.join("examples").join("go").join("variadic-sum");
    if !fixture_src.is_dir() {
        eprintln!(
            "skipping: fixture {} not present in this checkout",
            fixture_src.display()
        );
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let fixture = tmp.path().join("variadic-sum");
    copy_dir_recursive(&fixture_src, &fixture);

    // Seed an orphan launcher directory whose pid suffix is unlikely to be
    // alive — the scan should sweep it.
    let orphan_parent = fixture.join(".shatter-launchers");
    let orphan_dir = orphan_parent.join("orphanhash-99999998");
    std::fs::create_dir_all(&orphan_dir).expect("mkdir orphan");
    std::fs::write(orphan_dir.join("main.go"), b"package main\n").expect("seed orphan main.go");

    let report_path = tmp.path().join("report.json");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let output = Command::new(binary)
        .env("TMPDIR", command_tmp.path())
        .current_dir(&fixture)
        .args([
            "scan",
            ".",
            "--language",
            "go",
            "--no-cache",
            "--no-seeds",
            "--fail-on-failures=0",
            "-o",
        ])
        .arg(&report_path)
        .args(["--format", "json", "--color", "never"])
        .output()
        .expect("run shatter scan");

    assert!(
        output.status.success(),
        "scan must succeed; status={:?}\nstderr={}\nstdout={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );

    let launchers_dir = fixture.join(".shatter-launchers");
    assert!(
        !launchers_dir.exists(),
        "clean-audit scan left `.shatter-launchers/` in the target project: {}",
        launchers_dir.display(),
    );

    // The fixture should still contain exactly the two source files it
    // shipped with — no stray Shatter artefacts elsewhere in the tree.
    let mut remaining: Vec<String> = std::fs::read_dir(&fixture)
        .expect("read fixture")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    remaining.sort();
    assert_eq!(
        remaining,
        vec!["go.mod".to_string(), "sum.go".to_string()],
        "clean-audit scan left unexpected artefacts in target project: {remaining:?}",
    );
}
