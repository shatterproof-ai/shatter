//! Regression coverage for str-gnagk: `scan` must key cached analysis by the
//! actual custom frontend selected for the source language.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

const RUST_FIXTURE: &str = "pub fn classify(value: i32) -> bool { value > 0 }\n";
const NOOP_FRONTEND: &str = include_str!("../../protocol/noop-frontend.sh");

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn run_dry_scan(project: &Path, cache_dir: &Path, command_tmp: &Path) -> Output {
    Command::new(shatter_binary())
        .current_dir(project)
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp)
        .args([
            "scan",
            ".",
            "--project-dir",
            ".",
            "--language",
            "rust",
            "--dry-run",
            "--stdout",
            "--format",
            "json",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
            "--cache-dir",
        ])
        .arg(cache_dir)
        .output()
        .expect("invoke shatter scan")
}

fn cached_analyzer_version(cache_dir: &Path) -> String {
    let analysis_dir = cache_dir.join("analysis");
    let entries: Vec<_> = fs::read_dir(&analysis_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", analysis_dir.display()))
        .map(|entry| entry.expect("read analysis cache entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    assert_eq!(entries.len(), 1, "expected one analysis cache entry");
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&entries[0]).expect("read analysis cache JSON"),
    )
    .expect("parse analysis cache JSON");
    value["analyzer_version"]
        .as_str()
        .expect("analysis cache entry has analyzer_version")
        .to_string()
}

#[test]
fn scan_replaces_cached_analysis_after_custom_frontend_changes() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let cache_dir = project.path().join("cache-under-test");
    let command_tmp = tempfile::tempdir().expect("create command tempdir");
    fs::write(project.path().join("lib.rs"), RUST_FIXTURE).expect("write Rust fixture");

    let custom_bin_dir = project.path().join(".shatter-cache/bin");
    fs::create_dir_all(&custom_bin_dir).expect("create custom frontend directory");
    let custom_frontend = custom_bin_dir.join("shatter-rust-custom");
    fs::write(&custom_frontend, NOOP_FRONTEND).expect("write custom frontend");
    let mut permissions = fs::metadata(&custom_frontend)
        .expect("stat custom frontend")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&custom_frontend, permissions).expect("make custom frontend executable");

    let first = run_dry_scan(project.path(), &cache_dir, command_tmp.path());
    assert!(
        first.status.success(),
        "first scan failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_version = cached_analyzer_version(&cache_dir);

    fs::write(
        &custom_frontend,
        format!("{NOOP_FRONTEND}\n# changed analyzer bytes\n"),
    )
    .expect("replace custom frontend");
    let second = run_dry_scan(project.path(), &cache_dir, command_tmp.path());
    assert!(
        second.status.success(),
        "second scan failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_version = cached_analyzer_version(&cache_dir);

    assert_ne!(first_version, second_version);
}
