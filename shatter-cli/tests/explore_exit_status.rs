//! Regression test for str-960w: `shatter explore` must exit nonzero when
//! every attempted target fails (e.g. all `build_failed`), even though
//! reports and summary artifacts are still written to disk.
//!
//! Strategy: drive the real `shatter` binary through the `--from-artifacts`
//! finalize path with a synthesized `summary.json` that records a single
//! attempted target whose only outcome was `build_failed`. This exercises
//! the same exit-code decision the live exploration path uses without
//! depending on a particular language frontend producing a build failure.

use std::process::Command;

use serde_json::json;

/// Locate the compiled `shatter` binary that `cargo test` produced.
fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn explore_from_artifacts_exits_nonzero_when_every_target_failed() {
    let tmp = tempfile::tempdir().expect("create tempdir for artifact root");
    let artifact_root = tmp.path();

    // `load_explore_summaries` searches recursively for files named
    // `summary.json`, so place ours under a sanitized file-component
    // subdirectory matching what `write_explore_summary` produces.
    let target_dir = artifact_root.join("broken_ts");
    std::fs::create_dir_all(&target_dir).expect("create target dir");
    let summary_path = target_dir.join("summary.json");
    let summary = json!({
        "version": 2,
        "status": "failed",
        "file": "src/broken.ts",
        "total_functions": 1,
        "completed": 0,
        "failed": 1,
        "skipped": 0,
        "elapsed_secs": 0.0,
        "build_failed": 1,
        "runtime_failed": 0,
        "timed_out": 0,
        "unsupported": 0,
        "skipped_by_policy": 0,
        "produced_coverage": 0,
        "functions": [
            {
                "function_name": "broken",
                "status": "build_failed",
                "reason": "instrumentation_failed: synthetic"
            }
        ]
    });
    std::fs::write(&summary_path, serde_json::to_string_pretty(&summary).unwrap())
        .expect("write summary.json");

    // The `targets` argument is required by clap even though the
    // `--from-artifacts` short-circuit skips analysis. Pass an arbitrary
    // placeholder so argument parsing succeeds.
    let output = Command::new(shatter_binary())
        .arg("explore")
        .arg("--from-artifacts")
        .arg(artifact_root)
        .arg("placeholder.ts")
        .output()
        .expect("invoke shatter explore");

    assert!(
        !output.status.success(),
        "explore must exit nonzero when every attempted target failed; \
         status={:?}\nstdout=\n{}\nstderr=\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Artifacts must survive the failure exit so downstream tooling can
    // still inspect per-target status.
    assert!(
        summary_path.exists(),
        "summary.json must remain after failure exit",
    );
    let surviving =
        std::fs::read_to_string(&summary_path).expect("re-read summary.json after explore");
    let parsed: serde_json::Value =
        serde_json::from_str(&surviving).expect("summary.json is valid JSON");
    assert_eq!(parsed["build_failed"], 1);
    assert_eq!(parsed["completed"], 0);

    // The error message must name the failure mode in a stable, greppable
    // form for CI and agent consumers.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("attempted target") && stderr.contains("build_failed=1"),
        "expected machine-readable failure reason on stderr; got:\n{stderr}",
    );
}

#[test]
fn explore_from_artifacts_exits_zero_when_some_target_succeeded() {
    let tmp = tempfile::tempdir().expect("create tempdir for artifact root");
    let artifact_root = tmp.path();

    // Failed target.
    let bad_dir = artifact_root.join("bad_ts");
    std::fs::create_dir_all(&bad_dir).expect("create bad dir");
    std::fs::write(
        bad_dir.join("summary.json"),
        serde_json::to_string_pretty(&json!({
            "version": 2,
            "status": "failed",
            "file": "src/bad.ts",
            "total_functions": 1,
            "completed": 0,
            "failed": 1,
            "skipped": 0,
            "elapsed_secs": 0.0,
            "build_failed": 1,
            "runtime_failed": 0,
            "timed_out": 0,
            "unsupported": 0,
            "skipped_by_policy": 0,
            "produced_coverage": 0,
            "functions": []
        }))
        .unwrap(),
    )
    .expect("write bad summary");

    // Successful target — at least one completed function flips the run
    // back into the partial-success regime.
    let good_dir = artifact_root.join("good_ts");
    std::fs::create_dir_all(&good_dir).expect("create good dir");
    std::fs::write(
        good_dir.join("summary.json"),
        serde_json::to_string_pretty(&json!({
            "version": 2,
            "status": "completed",
            "file": "src/good.ts",
            "total_functions": 1,
            "completed": 1,
            "failed": 0,
            "skipped": 0,
            "elapsed_secs": 0.0,
            "build_failed": 0,
            "runtime_failed": 0,
            "timed_out": 0,
            "unsupported": 0,
            "skipped_by_policy": 0,
            "produced_coverage": 1,
            "functions": []
        }))
        .unwrap(),
    )
    .expect("write good summary");

    let output = Command::new(shatter_binary())
        .arg("explore")
        .arg("--from-artifacts")
        .arg(artifact_root)
        .arg("placeholder.ts")
        .output()
        .expect("invoke shatter explore");

    assert!(
        output.status.success(),
        "partial-success run must exit 0; status={:?}\nstderr=\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
}
