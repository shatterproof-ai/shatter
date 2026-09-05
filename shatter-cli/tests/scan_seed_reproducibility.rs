//! str-0m0vn: `--seed` must make a scan reproducible.
//!
//! `ScanConfig.seed` was always wired through core -- `orchestrator.rs` builds
//! its RNG from it -- but no CLI path ever set it: `--seed` was plumbed only as
//! the core-sample selection seed and both `ScanConfig` construction sites in
//! `scan.rs` passed `seed: None`. Exploration therefore drew from entropy, and
//! two scans of unchanged source could not be compared.
//!
//! The user-visible contract this locks in:
//!
//! * same seed, unchanged source  -> identical per-function coverage
//! * omitted seed                 -> unchanged from previous behaviour, i.e.
//!                                   the flag is opt-in and nothing existing
//!                                   silently becomes deterministic
//!
//! Seeding does not make a scan fully deterministic on its own: parallel
//! scheduling and wall-clock timeouts still leak nondeterminism, which is why
//! this test asserts on per-function coverage rather than on byte-identical
//! reports.
//!
//! KNOWN VERIFICATION GAP, stated rather than glossed: this test passes with
//! and without the `seed: core_sample_seed` wiring it accompanies. It was run
//! against a reverted build to check, and it did not fail. Exploration on
//! small fixtures is already deterministic because the input generator mines
//! literals out of the source, so both an entropy draw and a seeded draw
//! reach the same branches. Attempts to build a fixture whose coverage varies
//! between unseeded runs -- sparse equality guards over a wide integer space,
//! a capped iteration budget -- produced identical coverage across runs too.
//!
//! What this test therefore locks in is the user-visible contract (same seed,
//! unchanged source, same coverage) and a guard against a future regression
//! that makes seeded scans non-reproducible. It is NOT evidence that the seed
//! reaches the RNG; the effective seed is not observable in any output. A test
//! that discriminates needs either a corpus large enough for exploration to be
//! genuinely partial, or a diagnostic that reports the effective seed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn shatter_bin() -> PathBuf {
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("shatter")
}

/// A minimal, self-contained Go module with a few branches to explore.
///
/// Built here rather than pointed at `examples/`, because that tree has no
/// `go.mod` at its root (every file fails preflight with `missing_go_mod`) and
/// because a fixture the test owns cannot be invalidated by the examples tree
/// being reorganised.
fn make_fixture(dir: &Path) -> PathBuf {
    let target = dir.join("fixture");
    std::fs::create_dir_all(&target).expect("create fixture dir");
    std::fs::write(target.join("go.mod"), "module seedfixture\n\ngo 1.21\n")
        .expect("write go.mod");
    std::fs::write(
        target.join("classify.go"),
        r#"package seedfixture

func Classify(n int) string {
	if n < 0 {
		return "negative"
	}
	if n == 0 {
		return "zero"
	}
	if n < 10 {
		return "small"
	}
	return "large"
}
"#,
    )
    .expect("write classify.go");
    target
}

/// Per-function `(qualified_id, lines_covered, branches_covered)`, sorted.
/// Comparing this rather than the whole report keeps the assertion on the
/// exploration result and off timing fields that legitimately vary.
fn coverage_fingerprint(report: &Path) -> Vec<(String, u64, u64)> {
    let text = std::fs::read_to_string(report)
        .unwrap_or_else(|e| panic!("read {}: {e}", report.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse report: {e}"));
    let mut rows: Vec<(String, u64, u64)> = value["functions"]
        .as_array()
        .expect("functions array")
        .iter()
        .map(|f| {
            (
                f["qualified_id"].as_str().unwrap_or_default().to_string(),
                f["lines_covered"].as_u64().unwrap_or_default(),
                f["branches_covered"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn scan_into(target: &Path, out: &Path, seed: Option<&str>) {
    let mut command = Command::new(shatter_bin());
    command
        // str-gg9v: opt into unsandboxed host execution, as the other CLI
        // scan tests do. Targets still run in a throwaway working directory.
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .arg("scan")
        .arg(target)
        .arg("--include")
        .arg("*.go")
        .arg("--parallelism")
        .arg("1")
        .arg("--timeout-total")
        .arg("600")
        .arg("--no-cache")
        .arg("--no-seeds")
        .arg("-o")
        .arg(out);
    if let Some(seed) = seed {
        command.arg("--seed").arg(seed);
    }
    let output = command.output().expect("run shatter scan");
    assert!(
        out.exists(),
        "scan wrote no report (status {:?})\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn same_seed_reproduces_the_same_coverage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(dir.path());
    let first = dir.path().join("first.json");
    let second = dir.path().join("second.json");

    scan_into(&target, &first, Some("4242"));
    scan_into(&target, &second, Some("4242"));

    assert_eq!(
        coverage_fingerprint(&first),
        coverage_fingerprint(&second),
        "two scans at --seed 4242 over unchanged source produced different \
         coverage; exploration is not reproducible"
    );
}

#[test]
fn omitting_the_seed_still_produces_a_usable_report() {
    // The flag is opt-in: without it, exploration keeps drawing from entropy.
    // This asserts only that the unseeded path still works, since asserting a
    // *difference* would be a flaky test -- two entropy draws may coincide on
    // a fixture this small.
    let dir = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(dir.path());
    let report = dir.path().join("unseeded.json");
    scan_into(&target, &report, None);
    assert!(
        !coverage_fingerprint(&report).is_empty(),
        "unseeded scan discovered no functions"
    );
}
