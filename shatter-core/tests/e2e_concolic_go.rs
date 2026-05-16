//! End-to-end concolic exploration tests using the real Go frontend (str-3op0).
//!
//! Counterpart of `e2e_concolic.rs` (TypeScript) and `e2e_concolic_rust.rs`
//! (Rust). Drives the real `shatter-go` subprocess through the full
//! analyze -> instrument -> orchestrator-driven explore -> Z3 solve pipeline
//! against three known-answer Go target programs covering distinct shapes:
//!
//! - **Free function with branches** -
//!   `<examples>/standalone/go/01-arithmetic.go::ClassifyNumber` (4 branches,
//!   the Go counterpart of the TS canonical case).
//! - **Method with same-package constructor** -
//!   `examples/go/service-method/svc.go::(*Service).Compute` (2 branches,
//!   exercises receiver-aware planning + plan-attached Execute).
//! - **Variadic helper** -
//!   `examples/go/variadic-sum/sum.go::SumThreshold` (2 branches,
//!   exercises the variadic-wrapper code path str-jeen.48 fixed).
//!
//! ## Why this exists
//!
//! Pre-str-3op0 the Go frontend had no Rust-driven E2E equivalent of
//! `e2e_concolic.rs`. Go pipeline coverage went through Go-side unit tests,
//! conformance/parity, the gauntlet, and the walkthrough -- none of which
//! exercise the same analyze -> instrument -> execute -> solve loop end-to-end
//! against the Go subprocess. str-jeen.48, .49, and .50 were Go-frontend
//! defects that survived Go-side unit tests because no Rust-orchestrated E2E
//! actually ran the generated wrappers through the full pipeline.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult};
use shatter_core::planner_consumer::fetch_planner_seeds;
use shatter_core::protocol::{Command as ProtoCommand, FunctionAnalysis, ResponseResult};
use shatter_core::sym_expr::SymExpr;

// ---------------------------------------------------------------------------
// Shared helpers (mirror the TS / Rust counterparts).
// ---------------------------------------------------------------------------

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Resolve the shared examples checkout used by integration tests.
fn examples_root() -> PathBuf {
    if let Some(path) = env::var_os("SHATTER_EXAMPLES_DIR") {
        return PathBuf::from(path);
    }
    let fallback = env::temp_dir().join("shatter-examples-main");
    assert!(
        fallback.exists(),
        "examples checkout not found. Set SHATTER_EXAMPLES_DIR or run python3 scripts/examples_checkout.py."
    );
    fallback
}

fn standalone_go_dir() -> PathBuf {
    examples_root().join("standalone/go")
}

fn repo_examples_go_dir() -> PathBuf {
    manifest_dir().join("..").join("examples").join("go")
}

/// Build the Go frontend on demand and return the binary path. Cached into
/// a per-process tmpdir so repeat invocations within one `cargo test` reuse
/// the binary. Mirrors `ensure_go_frontend_binary` in `e2e_concolic.rs`.
fn ensure_go_frontend_binary() -> PathBuf {
    let go_dir = manifest_dir().join("..").join("shatter-go");
    assert!(
        go_dir.join("main.go").exists(),
        "shatter-go/main.go not found at {} -- repo layout drift?",
        go_dir.display()
    );
    let tmpdir = env::temp_dir().join("shatter-3op0-go-frontend");
    std::fs::create_dir_all(&tmpdir).expect("create tmpdir for go binary");
    let binary_path = tmpdir.join("shatter-go");

    let status = std::process::Command::new("go")
        .args(["build", "-buildvcs=false", "-o"])
        .arg(&binary_path)
        .arg(".")
        .current_dir(&go_dir)
        .status()
        .expect("failed to run `go build` -- is Go installed?");
    assert!(
        status.success(),
        "go build failed (working_dir = {})",
        go_dir.display()
    );
    assert!(
        binary_path.exists(),
        "go binary missing after build: {}",
        binary_path.display()
    );
    binary_path
}

const GO_FRONTEND_REQUEST_TIMEOUT_SECS: u64 = 180;

/// Spawn the Go frontend subprocess. Each invocation gets its own tempdir
/// pinned via `SHATTER_GO_WORKSPACE_ROOT` so concurrently-running e2e tests
/// do not race on the shared `<repo>/.shatter-cache/go-workspace/` tree.
async fn spawn_go_frontend(tag: &str) -> (Frontend, tempfile::TempDir) {
    let binary = ensure_go_frontend_binary();
    let workspace_dir = tempfile::Builder::new()
        .prefix(&format!("shatter-go-e2e-3op0-{tag}-"))
        .tempdir()
        .expect("create per-test Go workspace tempdir");
    let mut config = FrontendConfig::new(binary);
    config.request_timeout = Duration::from_secs(GO_FRONTEND_REQUEST_TIMEOUT_SECS);
    config.env_vars.push((
        "SHATTER_GO_WORKSPACE_ROOT".to_string(),
        workspace_dir.path().to_string_lossy().into_owned(),
    ));
    let frontend = Frontend::spawn(&config)
        .await
        .expect("failed to spawn Go frontend binary");
    (frontend, workspace_dir)
}

async fn analyze_function(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
) -> FunctionAnalysis {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: file.to_string(),
            function: Some(function_name.to_string()),
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("analyze command failed");

    match response.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            // Methods surface with receiver-decorated names like
            // "(*Service).Compute"; free functions use the bare name.
            // Match by bare suffix so the helper handles both.
            .find(|f| f.name == function_name || f.name.ends_with(&format!(".{function_name}")))
            .unwrap_or_else(|| panic!("function '{function_name}' not found in analysis results")),
        ResponseResult::Error { code, message, .. } => {
            panic!("analyze error ({code:?}): {message}")
        }
        other => panic!("expected Analyze response, got: {other:?}"),
    }
}

async fn instrument_function(frontend: &mut Frontend, file: &str, function_name: &str) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function_name.to_string(),
            mocks: vec![],
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("instrument command failed");

    match response.result {
        ResponseResult::Instrument { instrumented, .. } => {
            assert!(instrumented, "instrumentation returned false");
        }
        ResponseResult::Error { code, message, .. } => {
            panic!("instrument error ({code:?}): {message}");
        }
        other => panic!("expected Instrument response, got: {other:?}"),
    }
}

fn return_value_set(result: &ExploreResult) -> HashSet<String> {
    result
        .executions
        .iter()
        .map(|exec| {
            if let Some(ref err) = exec.thrown_error {
                format!("ERROR:{}", err.message)
            } else {
                match &exec.return_value {
                    Some(v) => v.to_string(),
                    None => "null".to_string(),
                }
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Test 1: free function with branches.
//
// ClassifyNumber(n int) string -- 4 paths:
//   1. n < 0        -> "negative"
//   2. n == 0       -> "zero"
//   3. n > 0, even  -> "positive-even"
//   4. n > 0, odd   -> "positive-odd"
//
// The "zero" branch requires exactly n=0; random integer generation almost
// never lands on it. Z3 should solve the negated `n < 0` and `n == 0`
// constraints to drive the explorer there.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_classify_number_discovers_all_branches() {
    let file = standalone_go_dir().join("01-arithmetic.go");
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("classify-number").await;

    let analysis = analyze_function(&mut frontend, &file_str, "ClassifyNumber").await;
    assert_eq!(analysis.params.len(), 1, "ClassifyNumber takes 1 param");
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in ClassifyNumber"
    );

    instrument_function(&mut frontend, &file_str, "ClassifyNumber").await;

    let config = ExploreConfig {
        max_iterations: Some(40),
        max_executions: Some(120),
        plateau_threshold: 25,
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(7)], vec![serde_json::json!(-3)]];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "ClassifyNumber",
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);
    for expected in [
        "\"negative\"",
        "\"zero\"",
        "\"positive-even\"",
        "\"positive-odd\"",
    ] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 4,
        "should have at least 4 unique paths; got {}",
        result.unique_paths
    );
    assert!(
        result.z3_generated > 0,
        "Z3 should have generated at least one input to hit the n==0 branch; got z3_generated={}",
        result.z3_generated
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Test 2: method with same-package constructor.
//
// (*Service).Compute(x int) int -- 2 paths:
//   1. x > 0   -> 1
//   2. x <= 0  -> -1
//
// Method targets require a planner-emitted InvocationPlan so the wrapper's
// receiver-kind switch can dispatch through the same-package `New`
// constructor. The orchestrator carries that plan on every Execute via
// `default_execute_plan`. This is the str-hy9b.H5 shape exercised through
// the orchestrator's full concolic loop rather than a single Execute.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_service_compute_discovers_branches() {
    let file = repo_examples_go_dir().join("service-method").join("svc.go");
    assert!(
        file.exists(),
        "fixture missing: {} -- was the worktree set up correctly?",
        file.display()
    );
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("service-compute").await;

    let analysis = analyze_function(&mut frontend, &file_str, "Compute").await;
    assert_eq!(
        analysis.params.len(),
        1,
        "(*Service).Compute takes 1 param"
    );
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in Compute"
    );

    instrument_function(&mut frontend, &file_str, "Compute").await;

    // Consult the planner: receiver-aware planning should emit at least one
    // plan with a non-empty receiver_kind for the same-package `New`
    // constructor. The orchestrator attaches that plan to every Execute.
    let target_id = format!(":{}", analysis.name);
    let bundle = fetch_planner_seeds(&mut frontend, &target_id, &analysis.params)
        .await
        .expect("PLANNER GAP: get_invocation_plan transport failed");
    let execute_plan = bundle
        .plans
        .into_iter()
        .find(|p| !p.receiver_kind.is_empty() && p.argument_plans.len() == analysis.params.len())
        .unwrap_or_else(|| {
            panic!(
                "PLANNER GAP: planner returned no plan with non-empty receiver_kind for {target_id}; \
                 unsatisfied={:?}",
                bundle.unsatisfied
            )
        });

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(60),
        plateau_threshold: 15,
        default_execute_plan: Some(execute_plan),
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(5)], vec![serde_json::json!(-1)]];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        &analysis.name,
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);
    for expected in ["1", "-1"] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Test 3: variadic helper.
//
// SumThreshold(threshold int, vals ...int) string -- 2 paths:
//   1. sum(vals) >= threshold  -> "above"
//   2. sum(vals) <  threshold  -> "below"
//
// The variadic parameter exercises the launcher's variadic-wrapper code
// path (str-jeen.48 was the regression that motivated this gate). The
// analyzer surfaces the variadic param as a slice type, so the orchestrator
// generates JSON-array inputs for it.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_variadic_sum_discovers_branches() {
    let file = repo_examples_go_dir().join("variadic-sum").join("sum.go");
    assert!(
        file.exists(),
        "fixture missing: {} -- was the worktree set up correctly?",
        file.display()
    );
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("variadic-sum").await;

    let analysis = analyze_function(&mut frontend, &file_str, "SumThreshold").await;
    assert_eq!(
        analysis.params.len(),
        2,
        "SumThreshold takes 2 params (threshold int, vals ...int)"
    );
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in SumThreshold"
    );

    instrument_function(&mut frontend, &file_str, "SumThreshold").await;

    let config = ExploreConfig {
        max_iterations: Some(30),
        max_executions: Some(80),
        plateau_threshold: 20,
        ..Default::default()
    };

    // Two seed shapes: empty vals slice (sum=0, "below" for threshold>0) and
    // a populated slice that exceeds the threshold.
    let seed_inputs = vec![
        vec![serde_json::json!(10), serde_json::json!([])],
        vec![serde_json::json!(10), serde_json::json!([7, 8])],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "SumThreshold",
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);
    for expected in ["\"above\"", "\"below\""] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Test 5: ite SymExpr in branch conditions (str-1hlk.17.3).
//
// Categorize(x int) int -- 2 paths:
//   1. x > 0  -> label=1  -> label > 0 is true  -> returns 2
//   2. x <= 0 -> label=-1 -> label > 0 is false -> returns 0
//
// With the flow-map-aware branch extraction the second branch condition
// resolves `label` to ite{x>0, 1, -1}, so the static condition becomes
// bin_op{gt, ite{...}, 0}.  This test asserts:
//   a) the static analysis emits an ite SymExpr for branch 1's condition, AND
//   b) Z3 drives the full pipeline to both return values.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_categorize_ite_in_branch_condition() {
    let file = repo_examples_go_dir().join("05-conditional-merge.go");
    assert!(
        file.exists(),
        "fixture missing: {} -- was the worktree set up correctly?",
        file.display()
    );
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("categorize-ite").await;

    let analysis = analyze_function(&mut frontend, &file_str, "Categorize").await;
    assert_eq!(analysis.params.len(), 1, "Categorize takes 1 param (x int)");
    assert!(
        analysis.branches.len() >= 2,
        "Categorize should have >= 2 branches; got {}",
        analysis.branches.len()
    );

    // Assert that branch 1's condition contains an ite SymExpr as its left
    // operand — the key outcome of str-1hlk.17.3 (flow-map wired into
    // extractBranches so that `label` resolves to ite{x>0, 1, -1}).
    let br1 = &analysis.branches[1];
    let cond1 = br1.condition.as_ref().expect("branch 1 condition is None");
    let left1 = match cond1 {
        SymExpr::BinOp { left, .. } => left.as_ref(),
        other => panic!(
            "branch 1 condition should be BinOp (label > 0); got {:?}",
            other
        ),
    };
    match left1 {
        SymExpr::Ite { condition, .. } => {
            match condition.as_ref() {
                SymExpr::BinOp { .. } => {}
                other => panic!("ite.condition should be BinOp (x > 0); got {:?}", other),
            }
        }
        other => panic!(
            "branch 1 BinOp.left should be Ite (label resolved via flow map); got {:?}",
            other
        ),
    };

    instrument_function(&mut frontend, &file_str, "Categorize").await;

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(60),
        plateau_threshold: 15,
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![serde_json::json!(3)],
        vec![serde_json::json!(-2)],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "Categorize",
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);
    for expected in ["2", "0"] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Test 4: `package main` CLI entrypoints are filtered from analyze output.
//
// str-jeen.55: Zolem broad-run scans were dispatching `func main()` targets
// through the launcher subprocess; bodies that called `os.Exit` /
// `log.Fatal` killed the harness before it wrote a response, and the Go
// session reader surfaced "launcher: subprocess exited unexpectedly" —
// which the scan classifier reported as a launcher infrastructure failure.
//
// The fix filters `func main()` at the Go analyzer (mirroring the existing
// `init` skip), so scan never reaches the launcher with a CLI entrypoint.
// This E2E asserts the contract end-to-end against the real Go frontend
// subprocess for all three fixture shapes (printer, os.Exit, log.Fatal):
//
//   - the analyze response lists no `main` entry, AND
//   - the named non-`main` helper IS surfaced (so `package main` stays
//     useful for explorable helpers), AND
//   - no analyze response message mentions "subprocess exited unexpectedly"
//     (the original Zolem-visible misclassification string).
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess"]
async fn e2e_go_filters_main_entrypoints_from_scan() {
    let cases: &[(&str, &str, &str)] = &[
        ("main-entrypoint", "main.go", "Helper"),
        ("main-os-exit", "main.go", "Compute"),
        ("main-log-fatal", "main.go", "Classify"),
    ];

    for (dir, file_name, helper_name) in cases {
        let file = repo_examples_go_dir().join(dir).join(file_name);
        assert!(
            file.exists(),
            "fixture missing: {} -- was the worktree set up correctly?",
            file.display()
        );
        let file_str = file.to_string_lossy().into_owned();

        let (mut frontend, _workspace_dir) =
            spawn_go_frontend(&format!("filter-main-{dir}")).await;

        let response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.clone(),
                function: None,
                project_root: None,
                execution_profile: None,
            })
            .await
            .expect("analyze command failed");

        match response.result {
            ResponseResult::Analyze { functions } => {
                let names: Vec<String> = functions.iter().map(|f| f.name.clone()).collect();
                assert!(
                    !names.iter().any(|n| n == "main"),
                    "{dir}: analyze surfaced `main` as a target (names={names:?}); \
                     str-jeen.55 requires the Go analyzer to filter `func main()` \
                     in `package main`"
                );
                assert!(
                    names.iter().any(|n| n == helper_name),
                    "{dir}: analyze dropped non-main helper {helper_name} \
                     (names={names:?}); only `main` should be filtered"
                );
            }
            ResponseResult::Error { code, message, .. } => {
                assert!(
                    !message.contains("subprocess exited unexpectedly"),
                    "{dir}: analyze surfaced launcher misclassification \
                     ({code:?}): {message}"
                );
                panic!("{dir}: analyze error ({code:?}): {message}");
            }
            other => panic!("{dir}: expected Analyze response, got: {other:?}"),
        }

        frontend.shutdown().await.expect("frontend shutdown failed");
    }
}

// ---------------------------------------------------------------------------
// Test 5: pointer-receiver method on a struct with no constructor.
//
// (*Counter).Classify(n int) string -- 2 paths:
//   1. n > 0   -> "positive"
//   2. n <= 0  -> "non-positive"
//
// str-jeen.50 regression: Zolem's broad scan dispatched method targets like
// these without consulting the planner. The launcher wrapper's switch on
// `d.ReceiverKind` then fell into its default arm and returned
// `"shatter: unknown receiver kind"`. With a planner-attached plan the
// receiver-aware fallback (`fallback_zero_value`, str-qo1.9) emits
// `receiver_kind = "zero_value"`, which dispatches as `&Counter{}` through
// the wrapper's zero-value case and exercises the method body cleanly.
//
// The negative assertion is the regression guard: no recorded execution
// outcome may carry the "unknown receiver kind" sentinel.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_pointer_zero_receiver_no_unknown_receiver_kind() {
    let file = repo_examples_go_dir()
        .join("pointer-zero-receiver")
        .join("counter.go");
    assert!(
        file.exists(),
        "fixture missing: {} -- was the worktree set up correctly?",
        file.display()
    );
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("pointer-zero-receiver").await;

    let analysis = analyze_function(&mut frontend, &file_str, "Classify").await;
    assert_eq!(
        analysis.params.len(),
        1,
        "(*Counter).Classify takes 1 param"
    );

    instrument_function(&mut frontend, &file_str, "Classify").await;

    let target_id = format!(":{}", analysis.name);
    let bundle = fetch_planner_seeds(&mut frontend, &target_id, &analysis.params)
        .await
        .expect("PLANNER GAP: get_invocation_plan transport failed");
    let execute_plan = bundle
        .plans
        .into_iter()
        .find(|p| !p.receiver_kind.is_empty() && p.argument_plans.len() == analysis.params.len())
        .unwrap_or_else(|| {
            panic!(
                "PLANNER GAP: planner returned no plan with non-empty receiver_kind for {target_id}; \
                 unsatisfied={:?}",
                bundle.unsatisfied
            )
        });
    assert_eq!(
        execute_plan.receiver_kind, "zero_value",
        "pointer-receiver, no-constructor target must take the fallback_zero_value path; \
         got receiver_kind={:?}",
        execute_plan.receiver_kind
    );

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(60),
        plateau_threshold: 15,
        default_execute_plan: Some(execute_plan),
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(5)], vec![serde_json::json!(-1)]];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        &analysis.name,
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    assert_no_unknown_receiver_kind(&result, "(*Counter).Classify");

    let return_values = return_value_set(&result);
    for expected in ["\"positive\"", "\"non-positive\""] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Test 6: value-receiver method on a constructor-less struct.
//
// (Calc).Sign(n int) string -- 3 paths:
//   1. n > 0  -> "pos"
//   2. n < 0  -> "neg"
//   3. n == 0 -> "zero"
//
// Companion to test 5: value receivers take the wrapper's `var _recv T`
// path (not `&T{}`). Same str-jeen.50 negative assertion applies — no
// recorded outcome may surface "unknown receiver kind".
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "slow: spawns Go frontend subprocess and compiles per-execute harnesses"]
async fn e2e_go_value_zero_receiver_no_unknown_receiver_kind() {
    let file = repo_examples_go_dir()
        .join("value-zero-receiver")
        .join("calc.go");
    assert!(
        file.exists(),
        "fixture missing: {} -- was the worktree set up correctly?",
        file.display()
    );
    let file_str = file.to_string_lossy().into_owned();

    let (mut frontend, _workspace_dir) = spawn_go_frontend("value-zero-receiver").await;

    let analysis = analyze_function(&mut frontend, &file_str, "Sign").await;
    assert_eq!(analysis.params.len(), 1, "(Calc).Sign takes 1 param");

    instrument_function(&mut frontend, &file_str, "Sign").await;

    let target_id = format!(":{}", analysis.name);
    let bundle = fetch_planner_seeds(&mut frontend, &target_id, &analysis.params)
        .await
        .expect("PLANNER GAP: get_invocation_plan transport failed");
    let execute_plan = bundle
        .plans
        .into_iter()
        .find(|p| !p.receiver_kind.is_empty() && p.argument_plans.len() == analysis.params.len())
        .unwrap_or_else(|| {
            panic!(
                "PLANNER GAP: planner returned no plan with non-empty receiver_kind for {target_id}; \
                 unsatisfied={:?}",
                bundle.unsatisfied
            )
        });
    assert_eq!(
        execute_plan.receiver_kind, "zero_value",
        "value-receiver, no-constructor target must take a zero-value plan; \
         got receiver_kind={:?}",
        execute_plan.receiver_kind
    );

    let config = ExploreConfig {
        max_iterations: Some(25),
        max_executions: Some(80),
        plateau_threshold: 18,
        default_execute_plan: Some(execute_plan),
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![serde_json::json!(3)],
        vec![serde_json::json!(-2)],
        vec![serde_json::json!(0)],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        &analysis.name,
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("concolic exploration failed");

    assert_no_unknown_receiver_kind(&result, "(Calc).Sign");

    let return_values = return_value_set(&result);
    for expected in ["\"pos\"", "\"neg\"", "\"zero\""] {
        assert!(
            return_values.contains(expected),
            "should discover branch returning {expected}; found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 3,
        "should have at least 3 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Negative regression assertion for str-jeen.50: no recorded execution
/// outcome from `target` may carry the launcher wrapper's
/// "unknown receiver kind" sentinel. Surfaces a precise diagnostic with
/// the offending message verbatim so failures point at the dispatch path
/// rather than the (unrelated) branch-discovery assertions below.
fn assert_no_unknown_receiver_kind(result: &ExploreResult, target: &str) {
    for exec in &result.executions {
        if let Some(ref err) = exec.thrown_error
            && err.message.contains("unknown receiver kind")
        {
            panic!(
                "str-jeen.50 regression: {target} dispatch surfaced \
                 \"unknown receiver kind\" — receiver plan was not threaded \
                 into the wrapper. thrown_error.message={:?}",
                err.message
            );
        }
    }
}
