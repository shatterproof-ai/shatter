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
