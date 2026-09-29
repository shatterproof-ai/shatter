//! str-49drv.13: a mixed-language scan runs one sub-scan per language, but
//! all of them share one scan id and one `scan-results/<id>/` directory. Each
//! sub-scan used to delete `functions/`, restart the artifact index at 00001
//! and overwrite `summary.json`, so only the last language's artifacts
//! survived while the stdout report still counted every language. The auto
//! resume checkpoint also lived in a different directory (16-hex id under
//! `<project>/shatter-artifacts`) than every other scan output.
//!
//! Contract locked in here, with and without `SHATTER_ARTIFACT_DIR`:
//!
//! * fresh scan: every reported function has an artifact and a summary entry,
//!   summary counts equal the report's, and no checkpoint is written;
//! * `--resume auto` (run twice): the checkpoint sits in the same directory as
//!   the summary, and the summary still covers both languages afterwards;
//! * `--resume PATH` reads and writes exactly PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

fn make_fixture(dir: &Path) -> PathBuf {
    let target = dir.join("mix");
    std::fs::create_dir_all(target.join("node_modules")).expect("create fixture dir");
    std::fs::write(target.join("go.mod"), "module mixfixture\n\ngo 1.21\n").expect("go.mod");
    std::fs::write(
        target.join("package.json"),
        r#"{"name":"mix","version":"1.0.0"}"#,
    )
    .expect("package.json");
    std::fs::write(
        target.join("classify.go"),
        r#"package mixfixture

func Classify(n int) string {
	if n < 0 {
		return "negative"
	}
	if n == 0 {
		return "zero"
	}
	return "positive"
}

func Clamp(n int) int {
	if n > 100 {
		return 100
	}
	return n
}
"#,
    )
    .expect("classify.go");
    std::fs::write(
        target.join("sign.ts"),
        r#"export function sign(n: number): string {
  if (n < 0) {
    return "neg";
  }
  if (n === 0) {
    return "zero";
  }
  return "pos";
}

export function isBig(n: number): boolean {
  if (n > 1000) {
    return true;
  }
  return false;
}
"#,
    )
    .expect("sign.ts");
    target
}

fn toolchains_available() -> bool {
    Command::new("go").arg("version").output().is_ok()
        && Command::new("node").arg("--version").output().is_ok()
}

/// Run a scan over the fixture and return the parsed stdout report.
fn run_scan(target: &Path, artifact_dir: Option<&Path>, resume: Option<&str>) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_shatter"));
    cmd.env("SHATTER_ALLOW_HOST_WRITES", "1")
        .arg("scan")
        .arg(target)
        .args(["--include", "*.go", "--include", "*.ts"])
        .args(["--no-cache", "--no-seeds", "--parallelism", "1"])
        .args(["--format", "json", "--color", "never"]);
    match artifact_dir {
        Some(dir) => cmd.env("SHATTER_ARTIFACT_DIR", dir),
        None => cmd.env_remove("SHATTER_ARTIFACT_DIR"),
    };
    if let Some(resume) = resume {
        cmd.args(["--resume", resume]);
    }
    let output = cmd.output().expect("run shatter scan");
    assert!(
        output.status.success(),
        "scan exited {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    // Implicit `init` prints a banner before the JSON report.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout
        .match_indices("{\n")
        .map(|(i, _)| i)
        .find(|&i| i == 0 || stdout.as_bytes()[i - 1] == b'\n')
        .unwrap_or_else(|| panic!("no JSON report on stdout:\n{stdout}"));
    serde_json::from_str(&stdout[json_start..])
        .unwrap_or_else(|e| panic!("parse report: {e}\nstdout:\n{stdout}"))
}

/// The single `scan-results/<id>/` directory under `artifact_root`.
fn only_scan_dir(artifact_root: &Path) -> PathBuf {
    let results = artifact_root.join("scan-results");
    let dirs: Vec<PathBuf> = std::fs::read_dir(&results)
        .unwrap_or_else(|e| panic!("read {}: {e}", results.display()))
        .map(|e| e.expect("entry").path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(dirs.len(), 1, "expected one scan dir in {results:?}: {dirs:?}");
    dirs.into_iter().next().unwrap()
}

fn report_function_count(report: &serde_json::Value) -> usize {
    report["functions"].as_array().expect("functions").len()
}

fn artifact_files(scan_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(scan_dir.join("functions"))
        .unwrap_or_else(|e| panic!("read functions dir: {e}"))
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    names
}

fn read_summary(scan_dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(scan_dir.join("summary.json")).expect("summary.json");
    serde_json::from_str(&text).expect("parse summary")
}

fn assert_summary_covers_report(scan_dir: &Path, report: &serde_json::Value) {
    let n = report_function_count(report);
    assert_eq!(n, 4, "fixture should discover 4 functions across 2 languages");

    let files = artifact_files(scan_dir);
    assert_eq!(files.len(), n, "one artifact per reported function: {files:?}");
    let prefixes: Vec<&str> = files.iter().map(|f| &f[..5]).collect();
    let mut unique = prefixes.clone();
    unique.dedup();
    assert_eq!(prefixes, unique, "artifact indexes must be unique: {files:?}");

    let summary = read_summary(scan_dir);
    assert_eq!(summary["total_functions"].as_u64(), Some(n as u64));
    let entries = summary["functions"].as_array().expect("summary functions");
    assert_eq!(entries.len(), n, "summary entry per function: {summary}");
    let counted = ["completed", "failed", "skipped"]
        .iter()
        .map(|k| summary[*k].as_u64().unwrap_or(0))
        .sum::<u64>();
    assert_eq!(counted, n as u64, "summary counts must add up: {summary}");
    for entry in entries {
        let rel = entry["artifact"].as_str().expect("artifact path");
        assert!(scan_dir.join(rel).exists(), "summary points at missing {rel}");
    }
    assert!(scan_dir.join("manifest.json").exists());
    assert!(scan_dir.join("run-status.json").exists());
    let status = std::fs::read_to_string(scan_dir.join("run-status.json")).expect("run-status");
    let status: serde_json::Value = serde_json::from_str(&status).expect("parse run-status");
    let targets = status["targets"].as_array().map_or(0, Vec::len);
    assert_eq!(targets, n, "run-status must cover every language: {status}");
}

fn all_files_named(root: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if entry.file_name() != "node_modules" {
                all_files_named(&p, name, out);
            }
        } else if entry.file_name() == name {
            out.push(p);
        }
    }
}

fn fresh_scan_case(with_artifact_dir: bool) {
    if !toolchains_available() {
        eprintln!("skipping: go/node not available");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(tmp.path());
    let art = tmp.path().join("art");
    let root = if with_artifact_dir {
        art.clone()
    } else {
        target.join("shatter-artifacts")
    };

    let report = run_scan(&target, with_artifact_dir.then_some(art.as_path()), None);
    let scan_dir = only_scan_dir(&root);
    assert_summary_covers_report(&scan_dir, &report);

    let mut checkpoints = Vec::new();
    all_files_named(tmp.path(), "checkpoint.json", &mut checkpoints);
    assert!(checkpoints.is_empty(), "no --resume, no checkpoint: {checkpoints:?}");
}

#[test]
fn fresh_mixed_scan_keeps_every_language_artifacts_with_artifact_dir() {
    fresh_scan_case(true);
}

#[test]
fn fresh_mixed_scan_keeps_every_language_artifacts_default_root() {
    fresh_scan_case(false);
}

fn resume_auto_case(with_artifact_dir: bool) {
    if !toolchains_available() {
        eprintln!("skipping: go/node not available");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(tmp.path());
    let art = tmp.path().join("art");
    let root = if with_artifact_dir {
        art.clone()
    } else {
        target.join("shatter-artifacts")
    };
    let dir_arg = with_artifact_dir.then_some(art.as_path());

    let first = run_scan(&target, dir_arg, Some("auto"));
    let scan_dir = only_scan_dir(&root);
    assert!(
        scan_dir.join("checkpoint.json").exists(),
        "auto checkpoint must live beside summary.json in {scan_dir:?}"
    );
    let mut checkpoints = Vec::new();
    all_files_named(tmp.path(), "checkpoint.json", &mut checkpoints);
    assert_eq!(checkpoints.len(), 1, "exactly one checkpoint: {checkpoints:?}");
    assert_summary_covers_report(&scan_dir, &first);

    let second = run_scan(&target, dir_arg, Some("auto"));
    assert_eq!(only_scan_dir(&root), scan_dir);
    assert_eq!(report_function_count(&second), 4);
    assert_summary_covers_report(&scan_dir, &second);
    let mut checkpoints = Vec::new();
    all_files_named(tmp.path(), "checkpoint.json", &mut checkpoints);
    assert_eq!(checkpoints.len(), 1, "still one checkpoint: {checkpoints:?}");
}

#[test]
fn resume_auto_mixed_scan_uses_scan_root_with_artifact_dir() {
    resume_auto_case(true);
}

#[test]
fn resume_auto_mixed_scan_uses_scan_root_default_root() {
    resume_auto_case(false);
}

#[test]
fn resume_off_and_explicit_path_behave_as_before() {
    if !toolchains_available() {
        eprintln!("skipping: go/node not available");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(tmp.path());
    let art = tmp.path().join("art");

    run_scan(&target, Some(&art), Some("off"));
    let mut checkpoints = Vec::new();
    all_files_named(tmp.path(), "checkpoint.json", &mut checkpoints);
    assert!(checkpoints.is_empty(), "--resume off writes none: {checkpoints:?}");

    let explicit = tmp.path().join("explicit-ckpt.json");
    run_scan(&target, Some(&art), Some(explicit.to_str().unwrap()));
    assert!(explicit.exists(), "--resume PATH writes exactly PATH");
    let mut checkpoints = Vec::new();
    all_files_named(tmp.path(), "checkpoint.json", &mut checkpoints);
    assert!(checkpoints.is_empty(), "nothing under scan_root: {checkpoints:?}");
}
