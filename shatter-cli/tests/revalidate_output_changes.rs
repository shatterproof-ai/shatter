//! str-49drv.14 regression: `shatter revalidate` must compare return/thrown
//! values, not just branch paths and severity. A function whose output changed
//! on a replayed input (same branch path, same severity) previously reported
//! `confirmed` and exited 0.
//!
//! Known-answer flow against the embedded Go frontend: explore a fixture to
//! populate the behavior-map cache, replay it unchanged (control: exit 0), then
//! change one returned string and replay again (must exit 1 and name the
//! function and the input).

use std::path::Path;
use std::process::{Command, Output};

mod common;

const GO_FIXTURE_TEMPLATE: &str = "package toy\n\n\
func Classify(n int64) string {\n\
\tif n == 0 {\n\
\t\treturn \"@ZERO@\"\n\
\t}\n\
\treturn \"nonzero\"\n}\n";

/// Same outputs as the template, but an extra never-returning-different branch
/// changes the branch path for every nonzero input: expected drift only.
const GO_DRIFT_FIXTURE: &str = "package toy\n\n\
func Classify(n int64) string {\n\
\tif n == 0 {\n\
\t\treturn \"zero\"\n\
\t}\n\
\tif n == 1000003 {\n\
\t\treturn \"nonzero\"\n\
\t}\n\
\treturn \"nonzero\"\n}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

fn run_shatter(project: &Path, cache: &Path, args: &[&str]) -> Output {
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    Command::new(shatter_binary())
        .current_dir(project)
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .env("SHATTER_CACHE_DIR", cache)
        .args(args)
        .output()
        .expect("invoke shatter")
}

fn text(o: &Output) -> String {
    format!(
        "stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn revalidate_fails_when_return_value_changes() {
    let project = tempfile::tempdir().expect("project tempdir");
    let cache = tempfile::tempdir().expect("cache tempdir");
    let root = project.path();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    let src = root.join("toy.go");
    std::fs::write(&src, GO_FIXTURE_TEMPLATE.replace("@ZERO@", "zero")).expect("write toy.go");
    let src_str = src.to_str().expect("utf8 path");

    let _host_tmp_lock = common::host_tmp_shatter_lock();

    let explore = run_shatter(
        root,
        cache.path(),
        &[
            "explore",
            src_str,
            "--max-iterations",
            "20",
            "--timeout-explore",
            "120",
        ],
    );
    assert!(
        explore.status.success(),
        "explore must succeed.\n{}",
        text(&explore)
    );

    // Control: unchanged source revalidates cleanly.
    let control = run_shatter(root, cache.path(), &["revalidate", src_str]);
    assert!(
        control.status.success(),
        "unchanged source must revalidate with exit 0.\n{}",
        text(&control)
    );

    // Mutate exactly one return value; branch path and severity are unchanged.
    std::fs::write(&src, GO_FIXTURE_TEMPLATE.replace("@ZERO@", "nil")).expect("mutate toy.go");

    let changed = run_shatter(root, cache.path(), &["revalidate", src_str]);
    let out = String::from_utf8_lossy(&changed.stdout);
    assert_eq!(
        changed.status.code(),
        Some(1),
        "a changed return value must fail revalidation.\n{}",
        text(&changed)
    );
    assert!(
        out.contains("Classify(0)") && out.contains("output changed"),
        "output must name the function and input.\n{}",
        text(&changed)
    );
    assert!(
        !out.contains("behaviors confirmed"),
        "must not report the old N/M confirmed summary.\n{}",
        text(&changed)
    );
}

/// Explore the unchanged fixture, then rewrite the source with `mutated` and
/// return the fixture handles for further `revalidate` runs.
fn explored_then_mutated(mutated: &str) -> (tempfile::TempDir, tempfile::TempDir, String) {
    let project = tempfile::tempdir().expect("project tempdir");
    let cache = tempfile::tempdir().expect("cache tempdir");
    let root = project.path();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    let src = root.join("toy.go");
    std::fs::write(&src, GO_FIXTURE_TEMPLATE.replace("@ZERO@", "zero")).expect("write toy.go");
    let src_str = src.to_str().expect("utf8 path").to_string();
    let explore = run_shatter(
        root,
        cache.path(),
        &[
            "explore",
            &src_str,
            "--max-iterations",
            "20",
            "--timeout-explore",
            "120",
        ],
    );
    assert!(
        explore.status.success(),
        "explore must succeed.\n{}",
        text(&explore)
    );
    std::fs::write(&src, mutated).expect("mutate toy.go");
    (project, cache, src_str)
}

#[test]
fn revalidate_drift_fails_by_default_and_allow_drift_restores_exit_zero() {
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let (project, cache, src) = explored_then_mutated(GO_DRIFT_FIXTURE);

    let default = run_shatter(project.path(), cache.path(), &["revalidate", &src]);
    assert_eq!(
        default.status.code(),
        Some(1),
        "drift-only must exit 1 by default.\n{}",
        text(&default)
    );
    assert!(
        String::from_utf8_lossy(&default.stdout).contains("expected drift"),
        "drift must be reported separately.\n{}",
        text(&default)
    );

    let allowed = run_shatter(
        project.path(),
        cache.path(),
        &["revalidate", &src, "--allow-drift"],
    );
    assert!(
        allowed.status.success(),
        "--allow-drift must restore exit 0 for drift-only results.\n{}",
        text(&allowed)
    );
}

#[test]
fn revalidate_output_change_fails_even_with_allow_drift() {
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let (project, cache, src) =
        explored_then_mutated(&GO_FIXTURE_TEMPLATE.replace("@ZERO@", "nil"));

    let out = run_shatter(
        project.path(),
        cache.path(),
        &["revalidate", &src, "--allow-drift"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "an output change must fail even with --allow-drift.\n{}",
        text(&out)
    );
}
