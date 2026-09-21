//! Regression coverage for str-6vl7p: `main.rs` canonicalizes the scan
//! directory once (to search for `shatter.config.json`) and threads that
//! result into `commands::scan::run_scan` instead of letting `run_scan`
//! independently re-canonicalize the same path. This test locks in that
//! `scan` behaves identically regardless of whether the directory argument
//! is passed as a canonical absolute path, a relative path, or a symlinked
//! path — all three must resolve to the same project and report the same
//! function inventory.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Output};

const TS_FIXTURE: &str = "export function classify(value: number): boolean { return value > 0; }\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

/// Runs `shatter scan <directory> --dry-run --stdout --format json` from
/// `cwd`, so relative directory arguments resolve against `cwd`.
fn run_dry_scan(cwd: &Path, directory: &str, command_tmp: &Path) -> Output {
    Command::new(shatter_binary())
        .current_dir(cwd)
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp)
        .args([
            "scan",
            directory,
            "--language",
            "typescript",
            "--dry-run",
            "--stdout",
            "--format",
            "json",
            "--no-seeds",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter scan")
}

fn function_names(output: &Output) -> Vec<String> {
    assert!(
        output.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // `--stdout` also prints human-readable project-init messages (e.g. "No
    // .shatter/ found — initializing project") ahead of the JSON plan on a
    // fresh project, so locate the JSON object rather than parsing stdout
    // verbatim.
    let json_start = output
        .stdout
        .iter()
        .position(|&b| b == b'{')
        .expect("dry-run stdout contains a JSON object");
    let value: serde_json::Value = serde_json::from_slice(&output.stdout[json_start..])
        .expect("dry-run stdout tail is valid JSON");
    let layers = value["layers"]
        .as_array()
        .expect("dry-run JSON has a `layers` array");
    let mut names: Vec<String> = layers
        .iter()
        .flat_map(|layer| {
            layer["functions"]
                .as_array()
                .expect("layer has a `functions` array")
                .iter()
                .map(|f| {
                    f["name"]
                        .as_str()
                        .expect("function entry has a `name`")
                        .to_string()
                })
        })
        .collect();
    names.sort();
    names
}

/// A canonical absolute path, a relative path, and a symlinked path that all
/// point at the same project directory must produce identical scan results
/// (str-6vl7p: threading the caller's canonicalized path through `run_scan`
/// must not change resolution for any of these directory-argument forms).
#[test]
fn scan_resolves_directory_identically_across_path_forms() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let command_tmp = tempfile::tempdir().expect("create command tempdir");
    fs::write(project.path().join("lib.ts"), TS_FIXTURE).expect("write TypeScript fixture");

    let canonical_project = project
        .path()
        .canonicalize()
        .expect("canonicalize project dir");

    // 1. Canonical absolute path argument.
    let canonical_output = run_dry_scan(
        canonical_project.parent().unwrap(),
        canonical_project.to_str().unwrap(),
        command_tmp.path(),
    );
    let canonical_names = function_names(&canonical_output);
    assert_eq!(canonical_names, vec!["classify".to_string()]);

    // 2. Relative path argument (cwd = project dir itself, directory = ".").
    let relative_output = run_dry_scan(project.path(), ".", command_tmp.path());
    let relative_names = function_names(&relative_output);
    assert_eq!(relative_names, canonical_names);

    // 3. Symlinked directory argument.
    let symlink_parent = tempfile::tempdir().expect("create symlink parent tempdir");
    let symlink_path = symlink_parent.path().join("project-link");
    symlink(&canonical_project, &symlink_path).expect("create symlink to project dir");
    let symlink_output = run_dry_scan(
        symlink_parent.path(),
        symlink_path.to_str().unwrap(),
        command_tmp.path(),
    );
    let symlink_names = function_names(&symlink_output);
    assert_eq!(symlink_names, canonical_names);
}
