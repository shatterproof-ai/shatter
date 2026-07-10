//! str-6c6p: regression tests for `--quiet` report emission.
//!
//! `--quiet` must lower info/progress logging to warnings-only but MUST
//! still emit the requested command result (the final report) to stdout
//! or the requested output path. Before the fix, the explore code paths
//! gated header / per-function report / footer printing on
//! `log::log_enabled!(Level::Info)`, which suppressed the report itself
//! when the user only asked to suppress logs. The bare-`eprintln!`
//! `[progress]` stream in `emit_explore_progress` ignored the log level
//! entirely; that path is now also gated.
//!
//! Coverage matrix:
//!
//! | Command | Format | Behavior under `--quiet --stdout`              |
//! |---------|--------|------------------------------------------------|
//! | explore | text   | report present on stdout, no info logs         |
//! | explore | json   | rejected (separate str-tzbr contract)          |
//! | scan    | text   | report present on stdout, no info logs         |
//! | scan    | json   | report present on stdout, no info logs         |
//!
//! `explore --stdout --format json` is rejected unconditionally by the
//! str-tzbr contract (no stable JSON-on-stdout shape for explore). The
//! `--quiet` variant locks that the rejection reason is unaffected by the
//! quiet flag — quiet must not bypass nor weaken the contract.

use std::path::{Path, PathBuf};
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

/// Path to the small Go fixture used by the live-run quiet tests. The
/// fixture has 4 functions across 3 files and finishes in well under a
/// second with `--max-iterations 5`, keeping these tests fast enough to
/// run by default rather than gating behind an E2E feature flag.
fn go_internal_method_fixture() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("examples")
        .join("go")
        .join("internal-method")
}

/// Pre-create `.shatter/` in `dir` so the CLI's implicit-init path does
/// not write `Created  .shatter/` lines to stdout — those would
/// contaminate the "report on stdout" assertions. Init messages are
/// command output, not info logging, so they are unaffected by `--quiet`.
fn prepare_project(dir: &Path) {
    let shatter_dir = dir.join(".shatter");
    if !shatter_dir.exists() {
        std::fs::create_dir_all(&shatter_dir).expect("create .shatter dir");
        std::fs::write(shatter_dir.join("config.yaml"), "").expect("write config.yaml");
    }
}

/// Recursively copy a directory tree. Used to materialize the read-only
/// Go fixture into a per-test tempdir so concurrent tests do not race on
/// `.shatter-cache/` writes and the source tree stays untouched.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in std::fs::read_dir(src).expect("read_dir src") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            copy_dir_recursive(&path, &dst_path);
        } else if file_type.is_file() {
            std::fs::copy(&path, &dst_path).expect("copy file");
        }
        // Symlinks etc. are not present in the fixture — skip silently.
    }
}

/// Stage the Go fixture into a fresh tempdir so each test has an isolated
/// project root. Returns the tempdir guard plus the staged fixture root.
fn stage_go_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let staged = tmp.path().join("internal-method");
    copy_dir_recursive(&go_internal_method_fixture(), &staged);
    prepare_project(&staged);
    (tmp, staged)
}

#[test]
fn explore_quiet_stdout_text_emits_report() {
    let (_tmp, fixture) = stage_go_fixture();
    let target_file = fixture
        .join("api")
        .join("internal")
        .join("handler")
        .join("handler.go");
    let target = format!("{}:Classify", target_file.display());

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(&fixture)
        .args([
            "--quiet",
            "explore",
            &target,
            "--stdout",
            "--format",
            "text",
            "--render",
            "plain",
            "--no-cache",
        ])
        .output()
        .expect("invoke shatter explore");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "explore --quiet --stdout --format text must succeed; status={:?}\nstdout=\n{stdout}\nstderr=\n{stderr}",
        output.status,
    );

    // The text-format report frame: header, per-function section, footer.
    assert!(
        stdout.contains("Shatter Explore"),
        "expected explore header on stdout under --quiet; got:\n{stdout}",
    );
    assert!(
        stdout.contains("Classify"),
        "expected per-function report for Classify on stdout under --quiet; got:\n{stdout}",
    );
    assert!(
        stdout.contains("Summary"),
        "expected summary footer on stdout under --quiet; got:\n{stdout}",
    );

    // `--quiet` lowers log level to Warn — info/progress lines must NOT
    // leak to stderr. We assert on the bracketed info/progress markers
    // the CLI uses; warnings remain allowed.
    assert!(
        !stderr.contains("[info]") && !stderr.contains("[progress]"),
        "--quiet must suppress info/progress logging on stderr; got:\n{stderr}",
    );
}

#[test]
fn explore_quiet_stdout_json_remains_rejected() {
    // str-tzbr contract: explore --format json on stdout is rejected.
    // --quiet must NOT bypass or weaken this contract.
    let tmp = tempfile::tempdir().expect("create tempdir");
    prepare_project(tmp.path());

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(tmp.path())
        .args([
            "--quiet",
            "explore",
            "nonexistent.go:Func",
            "--stdout",
            "--format",
            "json",
        ])
        .output()
        .expect("invoke shatter explore");

    assert!(
        !output.status.success(),
        "explore --stdout --format json must be rejected even under --quiet",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The str-tzbr contract is enforced at the clap level — `json` is not a
    // valid `--format` variant for explore. The rejection message comes from
    // clap's own validation rather than a custom error handler.
    assert!(
        stderr.contains("invalid value") && stderr.contains("json"),
        "expected clap rejection for json format on stderr; got:\n{stderr}",
    );
    assert!(
        output.stdout.is_empty(),
        "rejected combo must not write to stdout, got: {:?}",
        String::from_utf8_lossy(&output.stdout),
    );
}

#[test]
fn scan_quiet_stdout_text_emits_report() {
    let (_tmp, fixture) = stage_go_fixture();

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(&fixture)
        .args([
            "--quiet",
            "scan",
            ".",
            "--stdout",
            "--format",
            "text",
            "--render",
            "plain",
            "--max-iterations",
            "5",
            "--no-cache",
        ])
        .output()
        .expect("invoke shatter scan");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "scan --quiet --stdout --format text must succeed; status={:?}\nstdout=\n{stdout}\nstderr=\n{stderr}",
        output.status,
    );

    // The text scan report carries a coverage summary plus per-function
    // entries by name. Assert on stable strings the report always emits.
    assert!(
        stdout.contains("Classify") || stdout.contains("Process"),
        "expected per-function entries on stdout under --quiet; got:\n{stdout}",
    );
    assert!(
        stdout.contains("coverage") || stdout.contains("Coverage"),
        "expected coverage section on stdout under --quiet; got:\n{stdout}",
    );

    assert!(
        !stderr.contains("[info]") && !stderr.contains("[progress]"),
        "--quiet must suppress info/progress logging on stderr; got:\n{stderr}",
    );
}

#[test]
fn scan_quiet_stdout_json_emits_report() {
    let (_tmp, fixture) = stage_go_fixture();

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(&fixture)
        .args([
            "--quiet",
            "scan",
            ".",
            "--stdout",
            "--format",
            "json",
            "--max-iterations",
            "5",
            "--no-cache",
        ])
        .output()
        .expect("invoke shatter scan");

    let stdout_bytes = output.stdout.clone();
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "scan --quiet --stdout --format json must succeed; status={:?}\nstderr=\n{stderr}",
        output.status,
    );

    // The JSON scan_report shape is documented and stable; parse it to
    // prove the full report (not a truncation) reached stdout under
    // --quiet. A non-empty `functions` array is the load-bearing claim.
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("scan json output must parse as JSON: {e}\nstdout=\n{stdout}"));
    let functions = parsed
        .get("functions")
        .and_then(|v| v.as_array())
        .expect("scan json report must have a `functions` array");
    assert!(
        !functions.is_empty(),
        "scan json report must list at least one function under --quiet; got:\n{stdout}",
    );

    assert!(
        !stderr.contains("[info]") && !stderr.contains("[progress]"),
        "--quiet must suppress info/progress logging on stderr; got:\n{stderr}",
    );
}
