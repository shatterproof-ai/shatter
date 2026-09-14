//! str-qwua7.56 review follow-up: `scan`'s exported-target selection
//! (`rebuild_analyses_from_registry` in `scan.rs`) excludes lifecycle
//! exports (`setup`/`teardown`/`beforeAll`/`afterAll`/`beforeEach`/
//! `afterEach`) from a file that also exports a setup-shaped API, but `run`
//! builds its call graph directly from the raw registry and iterates every
//! layer without that filter (CLAUDE.md parallel-path rule: "never add a
//! capability to one explorer path without checking the other"). This drives
//! the real `shatter` binary against a fixture that exports both `setup` and
//! `teardown` alongside a real function, and proves `run` explores the real
//! function but skips the lifecycle exports — matching `scan`'s behavior.

use std::process::Command;

mod common;

const TS_FIXTURE: &str = "\
export function setup(): void {}\n\
export function teardown(): void {}\n\
export function doWork(n: number): string {\n\
  if (n > 0) {\n\
    return \"positive\";\n\
  }\n\
  return \"non-positive\";\n\
}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

#[test]
fn run_skips_lifecycle_exports_from_a_setup_shaped_file() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();
    std::fs::write(project_root.join("package.json"), "{}\n").expect("write package.json");
    std::fs::write(project_root.join("toy.ts"), TS_FIXTURE).expect("write toy.ts");
    // The TS frontend's preflight check refuses to analyze without
    // node_modules present; an empty directory satisfies it.
    std::fs::create_dir_all(project_root.join("node_modules")).expect("create node_modules");

    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args([
            "run",
            project_root.to_str().expect("utf8 project path"),
            "--max-iterations",
            "3",
            "--timeout",
            "60",
            "-v",
        ])
        .output()
        .expect("invoke shatter run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("Exploring setup") && !stderr.contains("Exploring teardown"),
        "run must skip lifecycle exports (setup/teardown) from a setup-shaped file, \
         matching scan's rebuild_analyses_from_registry exclusion; stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("Exploring doWork"),
        "run must still explore the file's real, non-lifecycle export; stderr=\n{stderr}"
    );
}
