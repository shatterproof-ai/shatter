//! str-qwua7.11 regression: `shatter explore --spec-json <target>` writes
//! the human-readable markdown explore report AND the JSON spec to the
//! same stdout, so anything piping the output (`| jq`, `json.load`, CI)
//! fails to parse at line 1 because it sees markdown before the JSON.
//!
//! Only `--spec-out FILE` (or `-o file.json`) previously produced clean
//! JSON. This test drives the real `shatter` binary against a small Go
//! fixture (the Go frontend is embedded, so always available) and asserts:
//!
//! - `--spec-json` with no `--spec-out`: stdout is *exactly* one JSON
//!   document (nothing before or after it parses as JSON) — this is the
//!   bug fix, and fails against pre-fix code.
//! - `--spec-out FILE`: unaffected — stdout still carries the human report,
//!   and the file still carries valid JSON (regression coverage).
//! - `--spec` (markdown, no `--spec-json`): per the issue's acceptance
//!   criteria, markdown spec output follows the same stdout-purity rule by
//!   default — the report is redirected to stderr so stdout carries only
//!   the markdown spec.

use std::process::Command;

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

/// Shared explore invocation args, minus the spec-related flags under test.
fn base_args<'a>(target_arg: &'a str, project_dir: &'a str) -> Vec<&'a str> {
    vec![
        "explore",
        target_arg,
        "--project-dir",
        project_dir,
        "--max-iterations",
        "3",
        "--timeout-explore",
        "30",
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
        "--no-cache",
        "--no-seeds",
    ]
}

struct Fixture {
    _project: tempfile::TempDir,
    project_dir: std::path::PathBuf,
    target_arg: String,
}

fn write_fixture() -> Fixture {
    let project = tempfile::tempdir().expect("create project tempdir");
    let root = project.path().to_path_buf();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    let target_file = root.join("toy.go");
    std::fs::write(&target_file, GO_FIXTURE).expect("write toy.go");
    let target_arg = format!("{}:Add", target_file.to_str().expect("utf8 target"));
    // Pre-create an empty `.shatter/` so the implicit-init path (main.rs
    // `maybe_implicit_init`) is skipped — its own status messages
    // ("Created .shatter/", "Initialized Shatter project at ...") print to
    // stdout and are orthogonal to the stdout-purity contract under test
    // here (str-qwua7.11 is scoped to explore's own report/spec routing,
    // not implicit-init messaging).
    std::fs::create_dir_all(root.join(".shatter")).expect("pre-create .shatter/");
    Fixture {
        _project: project,
        project_dir: root,
        target_arg,
    }
}

#[test]
fn explore_spec_json_to_stdout_is_pure_json() {
    let fixture = write_fixture();
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args(base_args(
            &fixture.target_arg,
            fixture.project_dir.to_str().expect("utf8 project dir"),
        ))
        .arg("--spec-json")
        .output()
        .expect("invoke shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "explore --spec-json must succeed; stderr=\n{stderr}\nstdout=\n{stdout}",
    );

    // The core assertion: stdout must be nothing but valid JSON documents
    // (one per explored function — one here), with no markdown report text
    // interleaved. `Deserializer::from_str(..).into_iter()` walks every
    // top-level JSON value in the stream and errors if any byte in between
    // or after is not part of a JSON document.
    let trimmed = stdout.trim();
    assert!(
        !trimmed.is_empty(),
        "stdout must contain the spec JSON document; got empty output. stderr=\n{stderr}",
    );
    let stream = serde_json::Deserializer::from_str(trimmed).into_iter::<serde_json::Value>();
    let mut doc_count = 0;
    for (i, doc) in stream.enumerate() {
        let value = doc.unwrap_or_else(|e| {
            panic!(
                "stdout is not pure JSON (str-qwua7.11 regression) — parse error at \
                 document {i}: {e}\nfull stdout=\n{stdout}\nstderr=\n{stderr}"
            )
        });
        assert!(
            value.get("function_name").is_some(),
            "expected a spec document with a function_name field; got: {value}"
        );
        doc_count += 1;
    }
    assert_eq!(
        doc_count, 1,
        "expected exactly one spec JSON document on stdout for a single-function \
         target; stdout=\n{stdout}"
    );

    // The human-readable report must not have leaked onto stdout, and
    // should instead have gone to stderr (still visible to a human running
    // interactively).
    assert!(
        !stdout.contains("Shatter Explore"),
        "explore report header must not appear on stdout when --spec-json \
         targets stdout; stdout=\n{stdout}"
    );
    assert!(
        stderr.contains("Shatter Explore"),
        "explore report header should be redirected to stderr when \
         --spec-json targets stdout; stderr=\n{stderr}"
    );
}

#[test]
fn explore_spec_out_file_output_is_unaffected() {
    let fixture = write_fixture();
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let out_dir = tempfile::tempdir().expect("create output tempdir");
    let spec_out = out_dir.path().join("spec.json");
    let _host_tmp_lock = common::host_tmp_shatter_lock();

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp.path())
        .args(base_args(
            &fixture.target_arg,
            fixture.project_dir.to_str().expect("utf8 project dir"),
        ))
        .arg("--spec-out")
        .arg(&spec_out)
        .output()
        .expect("invoke shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "explore --spec-out must succeed; stderr=\n{stderr}\nstdout=\n{stdout}",
    );

    // --spec-out writes the spec to a file, not stdout, so the report is
    // still the correct thing to print to stdout (regression: unchanged).
    assert!(
        stdout.contains("Shatter Explore"),
        "explore report should still print to stdout when the spec goes \
         to --spec-out (regression); stdout=\n{stdout}"
    );

    assert!(
        spec_out.exists(),
        "--spec-out file must be written; stderr=\n{stderr}"
    );
    let raw = std::fs::read_to_string(&spec_out).expect("read spec-out file");
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("--spec-out file must contain valid JSON: {e}\nraw={raw}"));
    assert!(
        parsed.get("functions").is_some(),
        "--spec-out file must carry a spec bundle with a functions field; got: {parsed}"
    );
}

#[test]
fn explore_spec_markdown_to_stdout_is_pure_markdown_spec() {
    let fixture = write_fixture();
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let _host_tmp_lock = common::host_tmp_shatter_lock();

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp.path())
        .args(base_args(
            &fixture.target_arg,
            fixture.project_dir.to_str().expect("utf8 project dir"),
        ))
        .arg("--spec")
        .output()
        .expect("invoke shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "explore --spec must succeed; stderr=\n{stderr}\nstdout=\n{stdout}",
    );

    // Per the issue's acceptance criteria, `--spec` (markdown) follows the
    // same stdout-purity rule as `--spec-json` by default: the report is
    // redirected to stderr so stdout carries only the markdown spec.
    assert!(
        stdout.contains("# Specification:"),
        "stdout must contain the markdown spec document; stdout=\n{stdout}"
    );
    assert!(
        !stdout.contains("Shatter Explore"),
        "explore report header must not appear on stdout when --spec \
         targets stdout; stdout=\n{stdout}"
    );
    assert!(
        stderr.contains("Shatter Explore"),
        "explore report header should be redirected to stderr when \
         --spec targets stdout; stderr=\n{stderr}"
    );
}
