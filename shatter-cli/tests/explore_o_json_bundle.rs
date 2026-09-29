//! str-49drv.11 regression: `shatter explore <file>:<fn> -o out.json` must
//! write the real spec bundle (SPEC §2.1: `-o` infers format from the
//! extension; a `.json` destination holds the §5 spec bundle), not the
//! empty `no_targets`/`unclassified` marker, whether or not `--spec` or
//! `--spec-out` is also given.
//!
//! Drives the real `shatter` binary against a small Go fixture (the Go
//! frontend is embedded, so always available). The `--from-artifacts`
//! finalize sink is covered with a synthesized no-target `summary.json`.

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

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

struct Fixture {
    _project: tempfile::TempDir,
    project_dir: PathBuf,
    target_arg: String,
}

fn write_fixture() -> Fixture {
    let project = tempfile::tempdir().expect("create project tempdir");
    let root = project.path().to_path_buf();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    let target_file = root.join("toy.go");
    std::fs::write(&target_file, GO_FIXTURE).expect("write toy.go");
    let target_arg = format!("{}:Add", target_file.to_str().expect("utf8 target"));
    // Skip implicit-init so its stdout chatter and side effects stay out.
    std::fs::create_dir_all(root.join(".shatter")).expect("pre-create .shatter/");
    Fixture {
        _project: project,
        project_dir: root,
        target_arg,
    }
}

fn run_explore(fixture: &Fixture, extra: &[&str]) -> Output {
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp.path())
        .args([
            "explore",
            &fixture.target_arg,
            "--project-dir",
            fixture.project_dir.to_str().expect("utf8 project dir"),
            "--max-iterations",
            "3",
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
            // No --no-cache/--no-seeds: with `-o` those select external-audit
            // mode (str-k9y5), whose cold scratch storage makes the embedded
            // Go frontend miss its request timeout on a fresh host.
            "--clean",
        ])
        .args(extra)
        .output()
        .expect("invoke shatter explore")
}

fn read_bundle(path: &Path, output: &Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "explore must succeed; stderr=\n{stderr}"
    );
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("bundle not written to {}: {e}\nstderr=\n{stderr}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("bundle is not JSON: {e}\n{raw}"))
}

fn assert_real_bundle(bundle: &serde_json::Value, output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(
        bundle["status"], "no_targets",
        "a successful run must not write the no-target marker; bundle={bundle}\nstderr=\n{stderr}"
    );
    let functions = bundle["functions"].as_array().expect("functions array");
    assert_eq!(functions.len(), 1, "one explored function; bundle={bundle}");
    assert_eq!(functions[0]["function_name"], "Add", "bundle={bundle}");
    assert!(
        functions[0]["classes"]
            .as_array()
            .is_some_and(|c| !c.is_empty()),
        "explored function must carry equivalence classes; bundle={bundle}"
    );
}

#[test]
fn explore_o_json_alone_writes_real_bundle() {
    let fixture = write_fixture();
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out = out_dir.path().join("out.json");
    let output = run_explore(&fixture, &["-o", out.to_str().unwrap()]);
    assert_real_bundle(&read_bundle(&out, &output), &output);
}

#[test]
fn explore_o_json_with_spec_flag_writes_real_bundle() {
    let fixture = write_fixture();
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out = out_dir.path().join("out.json");
    let output = run_explore(&fixture, &["--spec", "-o", out.to_str().unwrap()]);
    assert_real_bundle(&read_bundle(&out, &output), &output);
}

#[test]
fn explore_spec_out_alone_still_writes_real_bundle() {
    let fixture = write_fixture();
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out = out_dir.path().join("spec.json");
    let output = run_explore(&fixture, &["--spec-out", out.to_str().unwrap()]);
    assert_real_bundle(&read_bundle(&out, &output), &output);
}

#[test]
fn explore_o_json_and_spec_out_write_same_functions() {
    let fixture = write_fixture();
    let out_dir = tempfile::tempdir().expect("output tempdir");
    let out = out_dir.path().join("out.json");
    let spec = out_dir.path().join("spec.json");
    let output = run_explore(
        &fixture,
        &["-o", out.to_str().unwrap(), "--spec-out", spec.to_str().unwrap()],
    );
    let a = read_bundle(&out, &output);
    let b = read_bundle(&spec, &output);
    assert_real_bundle(&a, &output);
    assert_real_bundle(&b, &output);
    assert_eq!(a["functions"].as_array().unwrap().len(), b["functions"].as_array().unwrap().len());
}

fn write_no_target_summary(target_dir: &Path) {
    std::fs::create_dir_all(target_dir).expect("create target dir");
    let summary = serde_json::json!({
        "version": 2, "status": "completed", "file": "src/empty.go",
        "total_functions": 0, "completed": 0, "failed": 0, "skipped": 0,
        "elapsed_secs": 0.0, "build_failed": 0, "runtime_failed": 0,
        "timed_out": 0, "unsupported": 0, "skipped_by_policy": 0,
        "produced_coverage": 0, "no_target_reason": "unclassified",
        "functions": []
    });
    std::fs::write(
        target_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .expect("write summary.json");
}

/// `--from-artifacts -o x.json` with no specs used to write nothing at all;
/// it must write the same no-target marker as the `--spec-out` sink.
#[test]
fn from_artifacts_o_json_no_target_writes_marker() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_no_target_summary(&tmp.path().join("empty_go"));
    let out = tmp.path().join("out.json");
    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .args(["explore", "--from-artifacts"])
        .arg(tmp.path())
        .arg("-o")
        .arg(&out)
        .arg("placeholder.go")
        .output()
        .expect("invoke shatter explore");
    let bundle = read_bundle(&out, &output);
    assert_eq!(bundle["status"], "no_targets", "bundle={bundle}");
    assert_eq!(bundle["no_target_reason"], "unclassified", "bundle={bundle}");
}
