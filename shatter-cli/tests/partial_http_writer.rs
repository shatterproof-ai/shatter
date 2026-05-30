use std::path::Path;
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn prepare_partial_writer_project(dir: &Path) {
    let shatter_dir = dir.join(".shatter");
    std::fs::create_dir_all(&shatter_dir).expect("create .shatter dir");
    std::fs::write(shatter_dir.join("config.yaml"), "").expect("write config.yaml");
    std::fs::write(
        dir.join("go.mod"),
        "module example.com/partialwriter\n\ngo 1.21\n",
    )
    .expect("write go.mod");
    std::fs::write(
        dir.join("main.go"),
        r#"package main

import "net/http"

func WritePartial(w http.ResponseWriter) int {
	w.WriteHeader(204)
	return 1
}

func main() {}
"#,
    )
    .expect("write main.go");
}

#[test]
fn scan_dry_run_keeps_executable_partial_http_writer() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_partial_writer_project(tmp.path());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "scan",
            ".",
            "--project-dir",
            ".",
            "--language",
            "go",
            "--dry-run",
            "--stdout",
            "--format",
            "json",
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
    assert!(
        output.status.success(),
        "scan dry-run must succeed; status={:?}\nstdout=\n{stdout}\nstderr=\n{stderr}",
        output.status,
    );

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("stdout must be parseable JSON; err={e}\nstdout=\n{stdout}\nstderr=\n{stderr}");
    });
    let plan = parsed.to_string();
    assert!(
        plan.contains("WritePartial"),
        "dry-run plan must include the executable partial helper; plan={plan}",
    );
    assert!(
        !plan.contains("partial adapter signature"),
        "partial http writer helper must not be skipped as an adapter-only function; plan={plan}",
    );
}

#[test]
fn explore_keeps_executable_partial_http_writer() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_partial_writer_project(tmp.path());
    let target = format!("{}:WritePartial", tmp.path().join("main.go").display());

    let output = Command::new(shatter_binary())
        .current_dir(tmp.path())
        .args([
            "explore",
            &target,
            "--project-dir",
            ".",
            "--max-iterations",
            "1",
            "--timeout-explore",
            "10",
            "--color",
            "never",
            "--render",
            "plain",
        ])
        .output()
        .expect("invoke shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "explore must succeed; status={:?}\nstdout=\n{stdout}\nstderr=\n{stderr}",
        output.status,
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("WritePartial"),
        "explore output should name the attempted function; output=\n{combined}",
    );
    assert!(
        !combined.contains("unsupported: 1"),
        "partial http writer helper must not be pre-skipped as unsupported; output=\n{combined}",
    );
}
