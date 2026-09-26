//! str-49drv.74: `shatter explore` artifacts must carry the engine-computed
//! `stop_reason` and `solver_guided_inputs`. `ExploreResultAccumulator` used to
//! rebuild the final `ObservationOutput` with `..Default::default()`, so every
//! artifact reported `worklist_exhausted` / `0` regardless of what ended the
//! run. Drives the real binary (embedded Go frontend) and reads the written
//! per-function artifact JSON.

use std::path::Path;
use std::process::Command;

mod common;

/// `Unlock` is only granted for one exact, non-boundary code: random input
/// generation will not hit it in a few iterations, the solver derives it.
const GO_FIXTURE: &str = "package toy\n\n\
func Unlock(code int64) string {\n\
\tif code == 8675309 {\n\
\t\treturn \"granted\"\n\
\t}\n\
\treturn \"denied\"\n}\n";

const MAX_ITERATIONS: u64 = 3;

fn find_observation(dir: &Path) -> Option<serde_json::Value> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_observation(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "json")
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && v["function_name"] == "Unlock"
            && v["observation"].is_object()
        {
            return Some(v["observation"].clone());
        }
    }
    None
}

/// Run `shatter explore` on a fresh copy of the fixture and return the
/// artifact's `observation` object.
fn explore_observation(concolic: bool) -> serde_json::Value {
    let project = tempfile::tempdir().expect("create project tempdir");
    let root = project.path();
    std::fs::write(root.join("go.mod"), "module toy\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(root.join("toy.go"), GO_FIXTURE).expect("write toy.go");
    let command_tmp = tempfile::tempdir().expect("create command tmpdir");

    let mut args = vec!["explore", "toy.go:Unlock"];
    if concolic {
        args.push("--concolic");
    }
    let max = MAX_ITERATIONS.to_string();
    args.extend(["--max-iterations", &max]);

    let output = Command::new(env!("CARGO_BIN_EXE_shatter"))
        .current_dir(root)
        .env("SHATTER_ALLOW_HOST_WRITES", "1")
        .env("TMPDIR", command_tmp.path())
        .args(&args)
        .output()
        .expect("invoke shatter explore");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "explore (concolic={concolic}) must exit 0.\nstderr=\n{stderr}\nstdout=\n{stdout}"
    );
    find_observation(&root.join("shatter-artifacts")).unwrap_or_else(|| {
        panic!("no Unlock artifact with an observation.\nstderr=\n{stderr}\nstdout=\n{stdout}")
    })
}

#[test]
fn concolic_explore_artifact_reports_solver_inputs_and_real_stop_reason() {
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let obs = explore_observation(true);
    let solver_inputs = obs["solver_guided_inputs"].as_u64().unwrap_or(0);
    assert!(
        solver_inputs > 0,
        "concolic artifact must report solver_guided_inputs > 0; observation={obs}"
    );
    // The budget (max_executions) was the limit, not an exhausted worklist.
    assert_ne!(
        obs["stop_reason"], "worklist_exhausted",
        "stop_reason must reflect what ended the run; observation={obs}"
    );
}

#[test]
fn default_explore_artifact_stop_reason_matches_explorer_classification() {
    let _host_tmp_lock = common::host_tmp_shatter_lock();
    let obs = explore_observation(false);
    let iterations = obs["iterations"].as_u64().expect("iterations");
    // Mirrors explorer::classify_stop_reason (no timeout in this run).
    let expected = if iterations >= MAX_ITERATIONS {
        "max_iterations"
    } else {
        "worklist_exhausted"
    };
    assert_eq!(obs["stop_reason"], expected, "observation={obs}");
}
