//! str-03mfx.3: end-to-end contract of `defaults.exploration.budget_allocation`
//! through the real `shatter scan` binary on a three-function Go fixture.
//!
//! - `static` conserves the layer total (Σ budget_allocated == n × flat budget)
//!   and gives the loop-heavy parser more than the trivial getter;
//! - `flat` output is identical to a run with the knob absent (same seed) and
//!   carries no budget fields at all;
//! - under `static` with a tight per-function cap, surplus claiming happens.

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

fn make_fixture(dir: &Path) -> PathBuf {
    let target = dir.join("fixture");
    std::fs::create_dir_all(&target).expect("create fixture dir");
    std::fs::write(target.join("go.mod"), "module budgetfixture\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(
        target.join("shapes.go"),
        r##"package budgetfixture

import "strings"

// Trivial: one branch, one int parameter.
func IsPositive(n int) bool {
	if n > 0 {
		return true
	}
	return false
}

// Medium: a few branches over an int.
func Bucket(n int) string {
	if n < 0 {
		return "negative"
	}
	if n == 0 {
		return "zero"
	}
	if n < 10 {
		return "small"
	}
	if n < 100 {
		return "medium"
	}
	return "large"
}

// Heavy: string parameter, loops, several branches, an opaque call.
func CountWords(s string) int {
	if len(s) == 0 {
		return 0
	}
	parts := strings.Split(s, " ")
	count := 0
	for _, p := range parts {
		if p == "" {
			continue
		}
		if strings.HasPrefix(p, "#") {
			continue
		}
		if len(p) > 12 {
			count += 2
			continue
		}
		count++
	}
	if count > 100 {
		return 100
	}
	return count
}

// Productive at any cap: every negated branch is a fresh integer path for
// Z3, so this keeps discovering until its execution budget runs out. Named
// and placed last so the trivial functions run (and donate) before it.
func ZigzagLadder(n int) int {
	if n == 1 {
		return 3
	}
	if n == 2 {
		return 6
	}
	if n == 3 {
		return 2
	}
	if n == 4 {
		return 5
	}
	if n == 5 {
		return 1
	}
	if n == 6 {
		return 4
	}
	if n == 7 {
		return 0
	}
	if n == 8 {
		return 3
	}
	if n == 9 {
		return 6
	}
	if n == 10 {
		return 2
	}
	if n == 11 {
		return 5
	}
	if n == 12 {
		return 1
	}
	if n == 13 {
		return 4
	}
	if n == 14 {
		return 0
	}
	if n == 15 {
		return 3
	}
	if n == 16 {
		return 6
	}
	if n == 17 {
		return 2
	}
	if n == 18 {
		return 5
	}
	if n == 19 {
		return 1
	}
	if n == 20 {
		return 4
	}
	if n == 21 {
		return 0
	}
	if n == 22 {
		return 3
	}
	if n == 23 {
		return 6
	}
	if n == 24 {
		return 2
	}
	if n == 25 {
		return 5
	}
	if n == 26 {
		return 1
	}
	if n == 27 {
		return 4
	}
	if n == 28 {
		return 0
	}
	if n == 29 {
		return 3
	}
	if n == 30 {
		return 6
	}
	if n == 31 {
		return 2
	}
	if n == 32 {
		return 5
	}
	if n == 33 {
		return 1
	}
	if n == 34 {
		return 4
	}
	if n == 35 {
		return 0
	}
	if n == 36 {
		return 3
	}
	if n == 37 {
		return 6
	}
	if n == 38 {
		return 2
	}
	if n == 39 {
		return 5
	}
	if n == 40 {
		return 1
	}
	if n == 41 {
		return 4
	}
	if n == 42 {
		return 0
	}
	if n == 43 {
		return 3
	}
	if n == 44 {
		return 6
	}
	if n == 45 {
		return 2
	}
	if n == 46 {
		return 5
	}
	if n == 47 {
		return 1
	}
	if n == 48 {
		return 4
	}
	if n == 49 {
		return 0
	}
	if n == 50 {
		return 3
	}
	if n == 51 {
		return 6
	}
	if n == 52 {
		return 2
	}
	if n == 53 {
		return 5
	}
	if n == 54 {
		return 1
	}
	if n == 55 {
		return 4
	}
	if n == 56 {
		return 0
	}
	if n == 57 {
		return 3
	}
	if n == 58 {
		return 6
	}
	if n == 59 {
		return 2
	}
	if n == 60 {
		return 5
	}
	return -1
}
"##,
    )
    .expect("write shapes.go");
    target
}

fn scan_into(target: &Path, out: &Path, extra: &[&str]) {
    let mut command = Command::new(shatter_bin());
    command
        .arg("scan")
        .arg(target)
        .arg("--include")
        .arg("*.go")
        .arg("--concolic")
        .arg("--parallelism")
        .arg("1")
        .arg("--seed")
        .arg("7")
        .arg("--timeout-total")
        .arg("900")
        // The 60-branch ladder needs more than the 30 s default per function
        // when the gate runs this suite in parallel with the rest of cli:test.
        .arg("--timeout-per-fn")
        .arg("300")
        .arg("--build-timeout")
        .arg("300")
        .arg("--no-cache")
        .arg("--no-seeds")
        .arg("-o")
        .arg(out);
    for a in extra {
        command.arg(a);
    }
    let output = command.output().expect("run shatter scan");
    assert!(
        output.status.success(),
        "scan failed: status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.exists(), "scan wrote no report at {}", out.display());
}

/// (function_name, iterations, branches_covered, lines_covered, budget_allocated, budget_claimed)
fn rows(report: &Path) -> Vec<(String, u64, u64, u64, u64, u64)> {
    let text = std::fs::read_to_string(report).expect("read report");
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse report: {e}"));
    let mut out: Vec<_> = value["functions"]
        .as_array()
        .expect("functions array")
        .iter()
        .map(|f| {
            (
                f["function_name"].as_str().unwrap_or("").to_string(),
                f["iterations"].as_u64().unwrap_or(0),
                f["branches_covered"].as_u64().unwrap_or(0),
                f["lines_covered"].as_u64().unwrap_or(0),
                f["budget_allocated"].as_u64().unwrap_or(0),
                f["budget_claimed"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    out.sort();
    out
}

fn has_budget_keys(report: &Path) -> bool {
    let text = std::fs::read_to_string(report).expect("read report");
    text.contains("\"budget_allocated\"") || text.contains("\"budget_claimed\"")
}

#[test]
fn static_allocation_conserves_total_and_favors_the_parser() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(dir.path());
    let out = dir.path().join("static.json");
    scan_into(
        &target,
        &out,
        &["--max-iterations", "20", "--set", "defaults.exploration.budget_allocation=static"],
    );
    let rows = rows(&out);
    assert_eq!(rows.len(), 4, "expected four functions: {rows:?}");
    let total: u64 = rows.iter().map(|r| r.4).sum();
    assert_eq!(total, 4 * 20 * 5, "layer total is conserved (n × flat executions): {rows:?}");
    let alloc = |name: &str| rows.iter().find(|r| r.0.ends_with(name)).map(|r| r.4).unwrap_or_else(|| panic!("{name} missing in {rows:?}"));
    assert!(
        alloc("IsPositive") < alloc("CountWords"),
        "the loop-heavy parser must receive more than the trivial getter: {rows:?}"
    );
    assert!(rows.iter().all(|r| r.4 >= 20), "every function keeps at least the floor: {rows:?}");
}

#[test]
fn flat_is_identical_to_knob_absent_and_carries_no_budget_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(dir.path());
    let absent = dir.path().join("absent.json");
    let flat = dir.path().join("flat.json");
    scan_into(&target, &absent, &["--max-iterations", "20"]);
    scan_into(
        &target,
        &flat,
        &["--max-iterations", "20", "--set", "defaults.exploration.budget_allocation=flat"],
    );
    assert!(!has_budget_keys(&absent), "knob-absent output must not mention budget fields");
    assert!(!has_budget_keys(&flat), "flat output must not mention budget fields");
    let a = rows(&absent);
    let f = rows(&flat);
    assert!(!a.is_empty());
    assert_eq!(a, f, "flat must match a knob-absent run with the same seed");
}

#[test]
fn static_allocation_lets_a_capped_function_claim_surplus() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = make_fixture(dir.path());
    let out = dir.path().join("static-tight.json");
    // 10 paths → 50 executions per function under flat. A low floor lets the
    // trivial functions plateau (20 duplicate executions) and donate, and a
    // ceiling factor of 1.0 keeps the parser at the flat cap so it must claim
    // from the surplus to keep going.
    scan_into(
        &target,
        &out,
        &[
            "--max-iterations",
            "10",
            "--set",
            "defaults.exploration.budget_allocation=static",
            "--set",
            "defaults.exploration.budget_floor=5",
            "--set",
            "defaults.exploration.budget_ceiling_factor=1.0",
        ],
    );
    let rows = rows(&out);
    let claimed: u64 = rows.iter().map(|r| r.5).sum();
    let allocated: u64 = rows.iter().map(|r| r.4).sum();
    assert_eq!(allocated, 4 * 10 * 5, "conserved even with a tight cap: {rows:?}");
    assert!(
        claimed > 0,
        "expected at least one surplus claim under a tight cap: {rows:?}"
    );
    let ladder = rows.iter().find(|r| r.0.ends_with("ZigzagLadder")).expect("ladder row");
    assert!(ladder.5 > 0, "the productive ladder is the claimant: {rows:?}");
    assert!(ladder.1 > 50, "its executions exceed the flat cap thanks to the claim: {rows:?}");
}
