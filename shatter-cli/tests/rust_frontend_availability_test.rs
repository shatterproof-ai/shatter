//! str-bnsw: Rust frontend availability precheck.
//!
//! Verifies that when no `shatter-rust` binary is reachable on the host, the
//! CLI surfaces a clear "frontend unavailable" status with build instructions
//! during target discovery — not as a generic per-target spawn failure.

use std::process::Command;

/// Spawn `shatter explore <file.rs>` with PATH stripped of any `shatter-rust`
/// entry and cwd set to a tempdir (so the `./shatter-rust/target/debug` and
/// `./target/debug` candidates miss). The command must fail, and stderr must
/// name the missing frontend along with the install hint.
#[test]
fn explore_rust_target_reports_unavailable_with_install_hint() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    let rust_file = tmp.path().join("toy.rs");
    std::fs::write(&rust_file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .expect("write toy.rs");

    // Strip PATH entirely so `find_on_path("shatter-rust")` cannot succeed.
    // Setting cwd to the empty tempdir makes the `./shatter-rust/...` and
    // `./target/...` candidates miss as well.
    let output = Command::new(binary)
        .args(["explore", rust_file.to_str().expect("utf8 path")])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter explore");

    assert!(
        !output.status.success(),
        "explore should fail when the Rust frontend is unavailable; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");

    assert!(
        combined.contains("rust frontend unavailable")
            || combined.contains("shatter-rust frontend not found"),
        "expected frontend-unavailable status, got:\n{combined}"
    );
    assert!(
        combined.contains("cargo build --manifest-path shatter-rust/Cargo.toml"),
        "expected install hint with cargo build instructions, got:\n{combined}"
    );

    // The precheck must fire BEFORE any frontend session is announced —
    // i.e. no "Spawned N frontend session(s)" line should appear.
    assert!(
        !combined.contains("Spawned ") || !combined.contains("frontend session"),
        "precheck should run before frontend spawn; got:\n{combined}"
    );
}

/// Mixed-language scan: when Rust is unavailable but Go (or TS) is, scan
/// should skip the Rust files with a clear status and continue with the
/// available languages rather than aborting the whole run.
#[test]
fn scan_skips_rust_files_when_frontend_unavailable_and_other_languages_present() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    // One Go file (frontend embedded — always available) and one Rust file
    // (frontend external — unavailable in this isolated env).
    std::fs::write(
        tmp.path().join("toy.go"),
        "package toy\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write toy.go");
    std::fs::write(
        tmp.path().join("toy.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .expect("write toy.rs");

    let output = Command::new(binary)
        .args([
            "scan",
            tmp.path().to_str().expect("utf8 path"),
            "--dry-run",
            // Verbose so the warning about the skipped Rust file reaches stderr.
            "-v",
        ])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter scan");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");

    // The Rust file must be skipped with a clear status, NOT block the scan.
    assert!(
        combined.contains("skipping") && combined.to_lowercase().contains("rust"),
        "expected a skip-Rust status; got:\n{combined}"
    );
    assert!(
        combined.contains("cargo build --manifest-path shatter-rust/Cargo.toml"),
        "skip status should include the install hint:\n{combined}"
    );
}

// ---- str-jeen.13 regression: per-row classification + exit policy ----
//
// Kapow re-runs invoke `shatter explore <file>` per discovered file. When
// Rust is unavailable, the previous (str-bnsw) behavior returned a hard
// non-zero error indistinguishable from a real target failure. Wrappers
// classified the row as a hard `shatter-rust frontend not found` failure,
// inflating "failed targets" against a fundamentally environmental
// condition.
//
// Required behavior:
//   1. Mixed Go/TS/Rust explore run with Rust unavailable: skip Rust
//      targets with a structured `STATUS skipped_by_unavailable_frontend`
//      stderr line and continue with the available languages (exit 0).
//   2. Same run with `--require-rust` set: hard fail.
//   3. All-Rust run with Rust unavailable: hard fail (no work to do).

const STATUS_LINE_PREFIX: &str = "STATUS skipped_by_unavailable_frontend";

#[test]
fn explore_mixed_rust_go_skips_rust_with_structured_status_and_exits_zero() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    let go_file = tmp.path().join("toy.go");
    std::fs::write(
        &go_file,
        "package toy\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write toy.go");
    let rust_file = tmp.path().join("toy.rs");
    std::fs::write(&rust_file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .expect("write toy.rs");

    let output = Command::new(binary)
        .args([
            "explore",
            "--analyze-only",
            go_file.to_str().expect("utf8 path"),
            rust_file.to_str().expect("utf8 path"),
        ])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter explore");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");

    assert!(
        output.status.success(),
        "mixed Go+Rust explore must exit 0 when Rust frontend is unavailable but \
         Go targets remain runnable; status={:?} stderr=\n{stderr}\nstdout=\n{stdout}",
        output.status,
    );

    let status_line = stderr
        .lines()
        .find(|l| l.starts_with(STATUS_LINE_PREFIX))
        .unwrap_or_else(|| {
            panic!(
                "expected one `{STATUS_LINE_PREFIX}` stderr line classifying the Rust \
             target; full stderr:\n{stderr}"
            )
        });
    assert!(
        status_line.contains("language=rust"),
        "status line should name the unavailable language: {status_line}"
    );
    assert!(
        status_line.contains(rust_file.to_str().unwrap()),
        "status line should name the skipped target file: {status_line}"
    );
    assert!(
        status_line.contains("hint=") && status_line.contains("cargo build"),
        "status line should include the install hint: {status_line}"
    );
    assert!(
        !combined.contains("Error: rust frontend unavailable")
            && !combined.contains("Error: no available frontends"),
        "mixed run must NOT emit a hard `Error:` line when other languages \
         remain runnable; got:\n{combined}"
    );
}

#[test]
fn explore_mixed_rust_go_with_require_rust_hard_fails() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    let go_file = tmp.path().join("toy.go");
    std::fs::write(
        &go_file,
        "package toy\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write toy.go");
    let rust_file = tmp.path().join("toy.rs");
    std::fs::write(&rust_file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .expect("write toy.rs");

    let output = Command::new(binary)
        .args([
            "explore",
            "--require-rust",
            "--analyze-only",
            go_file.to_str().expect("utf8 path"),
            rust_file.to_str().expect("utf8 path"),
        ])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter explore");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "--require-rust with Rust unavailable must hard-fail; stderr={stderr}",
    );
    assert!(
        stderr.contains("--require-rust"),
        "error should explain that --require-rust is the trigger: {stderr}"
    );
    // Even on hard fail, the per-target classification must still be emitted
    // so wrappers see consistent row-level status.
    assert!(
        stderr.lines().any(|l| l.starts_with(STATUS_LINE_PREFIX)),
        "per-target STATUS line should still appear before exit on \
         --require-rust failure: {stderr}"
    );
}

#[test]
fn explore_all_rust_targets_hard_fails_when_rust_unavailable() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");
    let rust_file = tmp.path().join("toy.rs");
    std::fs::write(&rust_file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .expect("write toy.rs");

    let output = Command::new(binary)
        .args(["explore", rust_file.to_str().expect("utf8 path")])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter explore");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "explore over only Rust targets must hard-fail when Rust is unavailable: {stderr}"
    );
    assert!(
        stderr.lines().any(|l| l.starts_with(STATUS_LINE_PREFIX)),
        "per-target STATUS line should appear so wrappers classify the row as \
         frontend-unavailable rather than as a generic spawn failure: {stderr}"
    );
}

#[test]
fn scan_mixed_emits_structured_status_per_skipped_rust_file() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    std::fs::write(
        tmp.path().join("a.go"),
        "package toy\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write a.go");
    let rust_one = tmp.path().join("one.rs");
    let rust_two = tmp.path().join("two.rs");
    std::fs::write(&rust_one, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .expect("write one.rs");
    std::fs::write(&rust_two, "pub fn sub(a: i32, b: i32) -> i32 { a - b }\n")
        .expect("write two.rs");

    let output = Command::new(binary)
        .args([
            "scan",
            tmp.path().to_str().expect("utf8 path"),
            "--dry-run",
            "-v",
        ])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter scan");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let status_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with(STATUS_LINE_PREFIX))
        .collect();
    assert_eq!(
        status_lines.len(),
        2,
        "scan must emit one `{STATUS_LINE_PREFIX}` stderr line per skipped Rust \
         file (expected 2 for one.rs + two.rs); got:\n{stderr}",
    );
    for line in &status_lines {
        assert!(
            line.contains("language=rust") && line.contains(".rs"),
            "every status line should name the language and file: {line}"
        );
    }
}

#[test]
fn scan_with_require_rust_hard_fails_when_rust_unavailable() {
    let binary = env!("CARGO_BIN_EXE_shatter");
    let tmp = tempfile::tempdir().expect("tempdir");

    std::fs::write(
        tmp.path().join("a.go"),
        "package toy\n\nfunc Add(a, b int) int { return a + b }\n",
    )
    .expect("write a.go");
    std::fs::write(
        tmp.path().join("toy.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .expect("write toy.rs");

    let output = Command::new(binary)
        .args([
            "scan",
            tmp.path().to_str().expect("utf8 path"),
            "--require-rust",
            "--dry-run",
            "-v",
        ])
        .env("PATH", "")
        .current_dir(tmp.path())
        .output()
        .expect("failed to run shatter scan");

    assert!(
        !output.status.success(),
        "scan with --require-rust must hard-fail when Rust is unavailable",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--require-rust"),
        "error should explain that --require-rust is the trigger: {stderr}"
    );
}
