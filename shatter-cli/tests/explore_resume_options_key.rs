//! str-49drv.10 regression: explore auto-resume must be keyed on the
//! result-affecting options (explorer mode, budgets, ...) and not only the
//! source fingerprint, and a resumed result keeps its original explorer label.
//!
//! Drives the real `shatter` binary against a small Go fixture (the Go
//! frontend is embedded). Every test gets a fresh project and artifact dir.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

const GO_FIXTURE: &str = "package toy\n\n\
func Add(a, b int) int {\n\
\tif a > 0 {\n\
\t\treturn a + b\n\
\t}\n\
\treturn b\n\
}\n";

const GO_FIXTURE_EDITED: &str = "package toy\n\n\
func Add(a, b int) int {\n\
\tif a > 100 {\n\
\t\treturn a * b\n\
\t}\n\
\tif a > 0 {\n\
\t\treturn a + b\n\
\t}\n\
\treturn b\n\
}\n";

struct Fixture {
    _project: tempfile::TempDir,
    _artifacts: tempfile::TempDir,
    project_dir: PathBuf,
    artifact_dir: PathBuf,
    source: PathBuf,
    target_arg: String,
}

fn write_fixture() -> Fixture {
    let project = tempfile::tempdir().expect("create project tempdir");
    let artifacts = tempfile::tempdir().expect("create artifact tempdir");
    let root = project.path().to_path_buf();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    let source = root.join("toy.go");
    std::fs::write(&source, GO_FIXTURE).expect("write toy.go");
    let target_arg = format!("{}:Add", source.to_str().expect("utf8 target"));
    std::fs::create_dir_all(root.join(".shatter")).expect("pre-create .shatter/");
    Fixture {
        artifact_dir: artifacts.path().to_path_buf(),
        _project: project,
        _artifacts: artifacts,
        project_dir: root,
        source,
        target_arg,
    }
}

fn run_explore(fixture: &Fixture, extra: &[&str]) -> Output {
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let output = Command::new(env!("CARGO_BIN_EXE_shatter"))
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("SHATTER_ARTIFACT_DIR", &fixture.artifact_dir)
        .env("TMPDIR", command_tmp.path())
        .args([
            "explore",
            &fixture.target_arg,
            "--project-dir",
            fixture.project_dir.to_str().expect("utf8 project dir"),
            // A cold embedded-Go build can exceed the 30s default request timeout.
            "--request-timeout",
            "120",
            "--timeout-explore",
            "120",
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
        ])
        .args(extra)
        .output()
        .expect("invoke shatter explore");
    assert!(
        output.status.success(),
        "explore {extra:?} must succeed; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Directory holding `summary.json` and the per-function artifacts.
fn target_artifact_dir(fixture: &Fixture) -> PathBuf {
    let results = fixture.artifact_dir.join("explore-results");
    std::fs::read_dir(&results)
        .unwrap_or_else(|e| panic!("read {}: {e}", results.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.join("summary.json").is_file())
        .unwrap_or_else(|| panic!("no summary.json under {}", results.display()))
}

fn summary_fingerprint(dir: &Path) -> String {
    let raw = std::fs::read_to_string(dir.join("summary.json")).expect("read summary");
    let summary: serde_json::Value = serde_json::from_str(&raw).expect("parse summary");
    summary["functions"][0]["deep_fingerprint"]
        .as_str()
        .expect("summary entry carries a deep fingerprint")
        .to_string()
}

fn write_sidecar(dir: &Path, sidecar: serde_json::Value) {
    let artifact = std::fs::read_dir(dir)
        .expect("read artifact dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_Add.json"))
        })
        .expect("Add artifact present");
    let name = artifact.file_name().unwrap().to_str().unwrap();
    let sidecar_path = dir.join(name.replace("_Add.json", "_Add.resume-state.json"));
    std::fs::write(sidecar_path, sidecar.to_string()).expect("write sidecar");
}

fn assert_not_resumed(output: &Output) {
    assert!(
        !stderr(output).contains("[resumed]"),
        "run must re-explore, not resume; stderr=\n{}",
        stderr(output)
    );
}

#[test]
fn random_then_concolic_reexplores_and_is_labelled_concolic() {
    let fixture = write_fixture();
    let first = run_explore(&fixture, &["--max-iterations", "5"]);
    assert!(!stdout(&first).contains("Explorer: concolic"));
    let second = run_explore(&fixture, &["--max-iterations", "5", "--concolic"]);
    assert_not_resumed(&second);
    assert!(
        stdout(&second).contains("Explorer: concolic"),
        "concolic run must be labelled concolic; stdout=\n{}",
        stdout(&second)
    );
}

#[test]
fn same_options_twice_resumes() {
    let fixture = write_fixture();
    run_explore(&fixture, &["--max-iterations", "5"]);
    let second = run_explore(&fixture, &["--max-iterations", "5"]);
    assert!(
        stderr(&second).contains("[resumed]"),
        "identical options must resume; stderr=\n{}",
        stderr(&second)
    );
    assert!(
        stdout(&second).contains("resumed from prior run; --clean to re-run"),
        "resumed report must say so; stdout=\n{}",
        stdout(&second)
    );
}

#[test]
fn concolic_then_random_keeps_concolic_label_only_when_reexplored() {
    let fixture = write_fixture();
    run_explore(&fixture, &["--max-iterations", "5", "--concolic"]);
    let second = run_explore(&fixture, &["--max-iterations", "5"]);
    assert_not_resumed(&second);
    assert!(
        !stdout(&second).contains("Explorer: concolic"),
        "random re-run must not carry the concolic label; stdout=\n{}",
        stdout(&second)
    );
}

#[test]
fn max_iterations_change_reexplores() {
    let fixture = write_fixture();
    run_explore(&fixture, &["--max-iterations", "5"]);
    let second = run_explore(&fixture, &["--max-iterations", "7"]);
    assert_not_resumed(&second);
    assert!(
        stderr(&second).contains("max_iterations"),
        "info line must name the differing option; stderr=\n{}",
        stderr(&second)
    );
}

#[test]
fn partial_resume_after_source_edit_rejects_sidecar() {
    let fixture = write_fixture();
    run_explore(&fixture, &["--max-iterations", "5", "--concolic"]);
    let dir = target_artifact_dir(&fixture);
    let stale_fp = summary_fingerprint(&dir);
    write_sidecar(
        &dir,
        serde_json::json!({
            "covered_paths": [1, 2],
            "discovery_inputs": [[1, 2]],
            "deep_fingerprint": stale_fp,
            "resume_key": {
                "explorer": "concolic",
                "options_hash": "irrelevant",
                "options": {},
            },
        }),
    );
    std::fs::write(&fixture.source, GO_FIXTURE_EDITED).expect("edit source");
    let second = run_explore(&fixture, &["--max-iterations", "5", "--concolic"]);
    let err = stderr(&second);
    assert!(
        !err.contains("Loaded partial resume state"),
        "stale sidecar must not load; stderr=\n{err}"
    );
    assert!(
        err.contains("partial") && err.contains("fingerprint"),
        "info line must give the fingerprint reason; stderr=\n{err}"
    );
    assert_not_resumed(&second);
}

#[test]
fn partial_resume_after_mode_change_rejects_sidecar() {
    let fixture = write_fixture();
    run_explore(&fixture, &["--max-iterations", "5"]);
    let dir = target_artifact_dir(&fixture);
    write_sidecar(
        &dir,
        serde_json::json!({
            "covered_paths": [1, 2],
            "discovery_inputs": [[1, 2]],
            "deep_fingerprint": summary_fingerprint(&dir),
            "resume_key": {
                "explorer": "concolic",
                "options_hash": "irrelevant",
                "options": {},
            },
        }),
    );
    // A budget change keeps full resume out of the picture so the sidecar
    // loader is the only resume source consulted.
    let second = run_explore(&fixture, &["--max-iterations", "7"]);
    let err = stderr(&second);
    assert!(
        !err.contains("Loaded partial resume state"),
        "concolic sidecar must not load into a random run; stderr=\n{err}"
    );
    assert!(
        err.contains("partial") && err.contains("explorer"),
        "info line must name the explorer mismatch; stderr=\n{err}"
    );
}
