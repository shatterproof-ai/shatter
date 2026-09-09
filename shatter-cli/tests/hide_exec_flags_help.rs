//! str-qwua7.15: `--help` on non-executing commands must not show
//! execution-only global flags (`--allow-host-writes`, `--set`, and the
//! four `timing*` flags), while executing commands keep showing them.
//!
//! Snapshot fixtures live in `tests/fixtures/help/`. Regenerate them (after
//! confirming the change is intentional) with:
//!   cargo run -p shatter-cli --bin shatter -- <command> --help > shatter-cli/tests/fixtures/help/<name>.txt

use std::path::Path;
use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn run_help(args: &[&str]) -> String {
    let output = Command::new(shatter_binary())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run shatter {args:?}: {e}"));
    assert!(
        output.status.success(),
        "shatter {args:?} --help exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("help output is valid UTF-8")
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/help/{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"))
}

const EXECUTION_ONLY_FLAGS: &[&str] = &[
    "--allow-host-writes",
    "--set ",
    "--timing ",
    "--timing-format",
    "--timing-output ",
    "--timing-output-dir",
];

/// `shatter spec-diff --help` must match the checked-in snapshot exactly:
/// no execution-only global flags, and the display globals (`--color`,
/// `--render`, `--log-level`, `-v/-q`, `--project-dir`) preserved.
#[test]
fn spec_diff_help_snapshot() {
    let actual = run_help(&["spec-diff", "--help"]);
    let expected = fixture("spec-diff");
    assert_eq!(
        actual, expected,
        "shatter spec-diff --help output drifted from the checked-in snapshot.\n\
         If this is an intentional flag change, regenerate the fixture (see file header).\n\
         --- actual ---\n{actual}\n--- expected ---\n{expected}"
    );
}

/// `shatter doctor --help` must match the checked-in snapshot exactly.
#[test]
fn doctor_help_snapshot() {
    let actual = run_help(&["doctor", "--help"]);
    let expected = fixture("doctor");
    assert_eq!(
        actual, expected,
        "shatter doctor --help output drifted from the checked-in snapshot.\n\
         If this is an intentional flag change, regenerate the fixture (see file header).\n\
         --- actual ---\n{actual}\n--- expected ---\n{expected}"
    );
}

/// `spec-diff --help` has only its own 1 flag plus the 6 allowed display
/// globals — 7 total, meeting the issue's "≤7 flags" target — plus `-h`.
///
/// Each flag entry starts a new line with two leading spaces then a `-`
/// (e.g. `      --json` or `  -v, --verbose...`); its wrapped help text on
/// following lines is indented further and does not match this prefix.
#[test]
fn spec_diff_help_flag_count_is_within_target() {
    let actual = run_help(&["spec-diff", "--help"]);
    let flag_count = actual
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            indent <= 6 && trimmed.starts_with('-')
        })
        .count();
    assert!(
        flag_count <= 8, // 7 real flags + -h/--help
        "expected shatter spec-diff --help to have <=8 flags (7 flags + help), got {flag_count}:\n{actual}"
    );
}

/// Every non-executing command named in the issue hides all six
/// execution-only global flags, including one level into `cache`/`telemetry`
/// sub-actions for consistency.
#[test]
fn non_executing_commands_hide_execution_only_globals() {
    let cases: &[&[&str]] = &[
        &["spec-diff", "--help"],
        &["init", "--help"],
        &["doctor", "--help"],
        &["cache", "--help"],
        &["telemetry", "--help"],
        &["cache", "clear", "--help"],
        &["telemetry", "status", "--help"],
    ];
    for args in cases {
        let output = run_help(args);
        for flag in EXECUTION_ONLY_FLAGS {
            assert!(
                !output.contains(flag),
                "shatter {args:?} --help unexpectedly shows execution-only flag {flag:?}:\n{output}"
            );
        }
        // Display/global flags must still be present.
        for kept in ["--color", "--render", "--log-level", "--project-dir", "-v,"] {
            assert!(
                output.contains(kept),
                "shatter {args:?} --help is missing display/global flag {kept:?}:\n{output}"
            );
        }
    }
}

/// `explore` (an executing command) must keep showing every execution-only
/// global flag — this issue hides visibility on non-executing commands, it
/// must not remove or hide these flags anywhere they're actually needed.
#[test]
fn executing_command_still_shows_execution_only_globals() {
    let output = run_help(&["explore", "--help"]);
    for flag in EXECUTION_ONLY_FLAGS {
        assert!(
            output.contains(flag),
            "shatter explore --help unexpectedly lost execution-only flag {flag:?}:\n{output}"
        );
    }
}

/// Hiding `--allow-host-writes` from `spec-diff --help` must not remove the
/// flag: it should still parse and behave exactly as before (str-qwua7.15
/// hides visibility only, never behavior, on non-executing commands).
#[test]
fn hidden_flag_still_functions_on_non_executing_command() {
    let output = Command::new(shatter_binary())
        .args(["spec-diff", "--allow-host-writes", "/nonexistent-old.json", "/nonexistent-new.json"])
        .output()
        .expect("failed to run shatter spec-diff --allow-host-writes");
    // The flag must be accepted (no "unexpected argument" clap error); the
    // command then fails for the expected reason (missing input files).
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("unrecognized"),
        "shatter spec-diff --allow-host-writes was rejected as an unknown flag:\n{stderr}"
    );
}

/// `-h` (short help) must also honor the hidden set, not just `--help`.
#[test]
fn short_help_flag_also_hides_execution_only_globals() {
    let output = run_help(&["spec-diff", "-h"]);
    for flag in EXECUTION_ONLY_FLAGS {
        assert!(
            !output.contains(flag),
            "shatter spec-diff -h unexpectedly shows execution-only flag {flag:?}:\n{output}"
        );
    }
}

/// Top-level `shatter --help` is unaffected by this change (it isn't one of
/// the non-executing leaf commands the issue scopes).
#[test]
fn top_level_help_unaffected() {
    let output = run_help(&["--help"]);
    assert!(
        output.contains("--allow-host-writes"),
        "shatter --help should still document --allow-host-writes at the top level:\n{output}"
    );
}
