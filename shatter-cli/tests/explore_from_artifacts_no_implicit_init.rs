//! str-vr7vq regression: `shatter explore --from-artifacts` must never run
//! implicit init.
//!
//! `--from-artifacts` takes the `finalize_explore` early-return path (see
//! `commands::explore::run_explore`), which only reads a previously-written
//! artifact directory and renders a report/spec bundle — it never analyzes,
//! caches, or seeds against a live project. Before this fix, `explore
//! --from-artifacts` still ran `maybe_implicit_init` unconditionally (unless
//! the unrelated external-audit-mode flags were also passed), which resolves
//! the project root from the current working directory whenever
//! `--project-dir` is not given. That left a stray `.shatter/` directory and
//! a managed `.gitignore` block in whatever directory the process happened to
//! be launched from — observed in practice as an untracked
//! `shatter-cli/.gitignore` left behind by `cargo test` integration tests
//! (`explore_no_target_spec.rs`, `explore_exit_status.rs`) that invoke the
//! compiled binary without pinning its working directory, since `cargo test`
//! runs integration tests with the crate's own manifest directory as cwd.
//!
//! Strategy: run the real `shatter` binary's `explore --from-artifacts`
//! subcommand with `.current_dir` pointed at a fresh, uninitialized tempdir
//! and no `--project-dir`, then assert neither `.shatter/` nor `.gitignore`
//! was created there.

use std::process::Command;

use serde_json::json;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn explore_from_artifacts_does_not_implicit_init_the_cwd() {
    let cwd = tempfile::tempdir().expect("create cwd tempdir");
    let artifact_root = tempfile::tempdir().expect("create artifact tempdir");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let target_dir = artifact_root.path().join("placeholder_ts");
    std::fs::create_dir_all(&target_dir).expect("create target dir");
    let summary = json!({
        "version": 2,
        "status": "completed",
        "file": "src/placeholder.ts",
        "total_functions": 0,
        "completed": 0,
        "failed": 0,
        "skipped": 0,
        "elapsed_secs": 0.0,
        "build_failed": 0,
        "runtime_failed": 0,
        "timed_out": 0,
        "unsupported": 0,
        "skipped_by_policy": 0,
        "produced_coverage": 0,
        "no_target_reason": "declaration_only",
        "functions": []
    });
    std::fs::write(
        target_dir.join("summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .expect("write summary.json");

    let spec_out = command_tmp.path().join("spec.json");

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .current_dir(cwd.path())
        .arg("explore")
        .arg("--from-artifacts")
        .arg(artifact_root.path())
        .arg("--spec-out")
        .arg(&spec_out)
        .arg("placeholder.ts")
        .output()
        .expect("invoke shatter explore");

    assert!(
        output.status.success(),
        "no-target from-artifacts run must exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        !cwd.path().join(".shatter").exists(),
        "explore --from-artifacts must never run implicit init against the \
         current working directory — it never analyzes a live project"
    );
    assert!(
        !cwd.path().join(".gitignore").exists(),
        "explore --from-artifacts must never write a managed .gitignore \
         block into the current working directory"
    );
}
