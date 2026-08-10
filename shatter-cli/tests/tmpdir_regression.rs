//! str-jeen.64 regression: when TMPDIR is set, Shatter must not create any
//! shatter-* temporary directories under the host `/tmp` root.
//!
//! Kapow's broad-run wrapper sets a per-file TMPDIR under
//! `shatter-artifacts/tmp/run-*/explore-*/` before invoking Shatter, but
//! reported that Shatter still left `shatter-sandbox-*`, `shatter-instrument-*`,
//! and `shatter-go-e2e-*` entries directly under `/tmp`. That broke the
//! sandbox-change diagnostics. This test enforces that with TMPDIR pointed at
//! a sentinel directory, no `shatter-*` entries appear under `/tmp` for the
//! duration of an invocation.
//!
//! Strategy: drive the real `shatter` binary against a small Go fixture (the
//! Go frontend is embedded, so it is always available) with TMPDIR set to a
//! fresh sentinel. Snapshot `shatter-*` entries in `/tmp` before and after
//! the run; the diff must be empty.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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

/// Snapshot all entries directly under `/tmp` whose name starts with
/// `shatter-`. Returns an empty set if `/tmp` is not readable (e.g., in
/// an unusual sandbox).
fn snapshot_tmp_shatter_entries() -> HashSet<PathBuf> {
    let tmp = PathBuf::from("/tmp");
    let mut out = HashSet::new();
    let read = match std::fs::read_dir(&tmp) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in read.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with("shatter-")
        {
            out.insert(entry.path());
        }
    }
    out
}

fn shatter_entry_matches_pid(path: &std::path::Path, pid: u32) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let pid = pid.to_string();
    name.contains(&format!("-{pid}-"))
        || name.ends_with(&format!("-{pid}"))
        || name.contains(&format!("-{pid}."))
}

#[test]
fn explore_respects_tmpdir_no_host_tmp_pollution() {
    // Project fixture with a tiny Go function so scan has real work to do
    // without depending on any external Go module cache.
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();
    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");

    // Sentinel TMPDIR — Shatter and all its child processes must use this
    // for ephemeral state, not host `/tmp`.
    let sentinel = tempfile::tempdir().expect("create sentinel tempdir");
    let sentinel_path = sentinel.path().to_path_buf();

    // External output dir so the test doesn't depend on `shatter-artifacts/`
    // being created under the fixture project.
    let out_dir = tempfile::tempdir().expect("create output tempdir");
    let report_path = out_dir.path().join("scan.json");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let before = snapshot_tmp_shatter_entries();

    let child = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", &sentinel_path)
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("invoke shatter scan");
    let child_pid = child.id();
    let output = child.wait_with_output().expect("wait for shatter scan");

    let after = snapshot_tmp_shatter_entries();
    let new_entries: Vec<_> = after
        .difference(&before)
        .filter(|path| shatter_entry_matches_pid(path, child_pid))
        .collect();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        new_entries.is_empty(),
        "TMPDIR={} was set but Shatter created shatter-* entries in /tmp: {:?}\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        sentinel_path.display(),
        new_entries,
        output.status,
        stderr,
        stdout,
    );

    // Guard against a vacuous pass: if the sentinel TMPDIR is completely
    // untouched, the run did not exercise any temp-creation path and the
    // contract is not actually being tested. Require at least one entry
    // under the sentinel to prove the run did hit temp-creating code.
    let sentinel_used = std::fs::read_dir(&sentinel_path)
        .map(|r| r.flatten().next().is_some())
        .unwrap_or(false);
    assert!(
        sentinel_used,
        "sentinel TMPDIR {} was never written to — the scan invocation \
         did not exercise any temp-creation path, so this test cannot \
         detect /tmp pollution regressions.\n\
         shatter status: {:?}\nstderr=\n{}\nstdout=\n{}",
        sentinel_path.display(),
        output.status,
        stderr,
        stdout,
    );
}
