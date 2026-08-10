//! str-yhsp regression: `shatter run --concolic` must route the whole-repo
//! one-shot pipeline through the concolic (Z3-backed) orchestrator, not the
//! random explorer. `run` composes its own exploration loop, so a CLI-level
//! test that drives the real binary is the only thing that proves the flag is
//! actually wired to the concolic path end-to-end.
//!
//! The core guard is differential: a fixture guarded by an equality against a
//! large non-boundary constant (`code == 8675309`) has a branch that random
//! input generation is astronomically unlikely to reach within a handful of
//! iterations, while the concolic solver derives the exact value from the path
//! constraint. So `run --concolic` must cover strictly MORE lines than the
//! default random `run` on the same fixture with the same tiny iteration
//! budget. A test that only asserted exit-success + "function appears in
//! report" would pass even if the flag silently fell back to the random
//! explorer — this one cannot.
//!
//! The Go frontend is embedded (always available) and supports `prepare`, so
//! this exercises the concolic instrument → prepare → orchestrator path.

use std::process::Command;

mod common;

/// `Unlock` returns "granted" only for one exact code. Random int64 generation
/// will not hit `8675309` (not a boundary-dictionary value) in a few
/// iterations; only the concolic solver reaches the "granted" branch.
const GO_FIXTURE: &str = "package toy\n\n\
func Unlock(code int64) string {\n\
\tif code == 8675309 {\n\
\t\treturn \"granted\"\n\
\t}\n\
\treturn \"denied\"\n}\n";

fn shatter_binary() -> &'static str {
    env!("CARGO_BIN_EXE_shatter")
}

/// Run `shatter run` on `project_root`, optionally with `--concolic`, and
/// return (stdout, stderr, success).
fn run_shatter(project_root: &std::path::Path, concolic: bool) -> (String, String, bool) {
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");
    let mut args = vec!["run", project_root.to_str().expect("utf8 project path")];
    if concolic {
        args.push("--concolic");
    }
    args.extend(["--max-iterations", "5", "--timeout", "120"]);

    let output = Command::new(shatter_binary())
        .env("SHATTER_ALLOW_HOST_WRITES", "1") // str-gg9v: opt into unsandboxed host execution
        .env("TMPDIR", command_tmp.path())
        .args(&args)
        .output()
        .expect("invoke shatter run");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

/// Parse the covered-line numerator from the `**Total**` row of a `run` report
/// (`| **Total** | **2** | **4/6** | **67%** |`) — the "Lines Covered" cell is
/// the third column and looks like `**N/M**`. Returns the `N`.
fn total_lines_covered(stdout: &str) -> u32 {
    // Several tables have a `**Total**` row; the Exploration Results one is
    // the only Total row carrying a coverage percentage.
    let row = stdout
        .lines()
        .find(|l| l.contains("**Total**") && l.contains('%'))
        .unwrap_or_else(|| panic!("no coverage `**Total**` row in report:\n{stdout}"));
    let cells: Vec<&str> = row.split('|').map(str::trim).collect();
    // cells: ["", "**Total**", "**2**", "**4/6**", "**67%**", ""]
    let covered_cell = cells
        .get(3)
        .unwrap_or_else(|| panic!("no lines-covered cell in Total row: {row:?}"));
    let numerator = covered_cell
        .trim_matches('*')
        .split('/')
        .next()
        .unwrap_or_else(|| panic!("cannot parse covered cell {covered_cell:?}"));
    numerator
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("covered numerator not an integer: {numerator:?}"))
}

#[test]
fn run_concolic_reaches_narrow_branch_random_cannot() {
    let project = tempfile::tempdir().expect("create project tempdir");
    let project_root = project.path();

    // Minimal Go module so discovery treats this as a Go project.
    std::fs::write(project_root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(project_root.join("toy.go"), GO_FIXTURE).expect("write toy.go");

    let _host_tmp_lock = common::host_tmp_shatter_lock();

    let (concolic_out, concolic_err, concolic_ok) = run_shatter(project_root, true);
    let (random_out, random_err, random_ok) = run_shatter(project_root, false);

    // Both modes must complete the pipeline and emit the report.
    assert!(
        concolic_ok,
        "run --concolic must exit 0.\nstderr=\n{concolic_err}\nstdout=\n{concolic_out}"
    );
    assert!(
        random_ok,
        "default run must exit 0.\nstderr=\n{random_err}\nstdout=\n{random_out}"
    );
    for (label, out) in [("concolic", &concolic_out), ("random", &random_out)] {
        assert!(
            out.contains("Unlock") && out.contains("Exploration Results"),
            "{label} run report must list the explored function.\nstdout=\n{out}"
        );
    }

    let concolic_covered = total_lines_covered(&concolic_out);
    let random_covered = total_lines_covered(&random_out);

    // The decisive check: the concolic solver reaches the `code == 8675309`
    // branch that random exploration cannot within 5 iterations, so it must
    // cover strictly more lines. If `--concolic` silently fell back to the
    // random explorer (the bug this PR fixes), the two would be equal.
    assert!(
        concolic_covered > random_covered,
        "run --concolic must cover more lines than random by reaching the \
         equality-guarded branch: concolic covered {concolic_covered}, random \
         covered {random_covered} (equal ⇒ concolic path was NOT taken).\n\
         concolic stdout=\n{concolic_out}\nrandom stdout=\n{random_out}"
    );
}
