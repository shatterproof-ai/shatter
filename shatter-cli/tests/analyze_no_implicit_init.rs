//! str-vr7vq regression: `shatter analyze` must never run implicit init.
//!
//! `analyze` performs pure offline computation on an already-produced
//! Stage 1 observation JSON file -- "No frontend or solver required" (see
//! the `Analyze` doc comment in `args.rs`). It never reads project config,
//! a cache, or a seed pool. Before this fix it still called
//! `maybe_implicit_init` unconditionally (the only one of `scan`/`explore`/
//! `analyze` with no audit-mode guard at all), which resolved the project
//! root from the current working directory whenever `--project-dir` was
//! absent -- leaving a stray `.shatter/` + managed `.gitignore` block in
//! whatever directory the process happened to be launched from.
//!
//! Strategy: run the real `shatter` binary's `analyze` subcommand against a
//! (deliberately malformed, to keep the fixture minimal) observation file
//! with `.current_dir` pointed at a fresh, uninitialized tempdir and no
//! `--project-dir`, then assert neither `.shatter/` nor `.gitignore` was
//! created there, regardless of whether `analyze` itself succeeds.

use std::process::Command;

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn analyze_does_not_implicit_init_the_cwd() {
    let cwd = tempfile::tempdir().expect("create cwd tempdir");
    let observation = tempfile::tempdir().expect("create observation tempdir");
    let observation_path = observation.path().join("obs.json");
    // Content doesn't need to be well-formed for this test: implicit init
    // used to run unconditionally, before the observation file is even
    // parsed, so a malformed fixture still exercises the code path this
    // regression guards.
    std::fs::write(&observation_path, "{}").expect("write observation fixture");

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .current_dir(cwd.path())
        .arg("analyze")
        .arg(&observation_path)
        .output()
        .expect("invoke shatter analyze");

    assert!(
        !cwd.path().join(".shatter").exists(),
        "analyze must never run implicit init against the current working \
         directory -- it never reads project state; analyze exit status: {:?}, \
         stderr=\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !cwd.path().join(".gitignore").exists(),
        "analyze must never write a managed .gitignore block into the \
         current working directory"
    );
}
