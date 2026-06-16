//! End-to-end concolic exploration tests using the real Rust frontend (str-o9rz).
//!
//! These tests are the Rust counterpart of `e2e_concolic.rs` (TypeScript). They
//! validate the full pipeline analyze → instrument → orchestrator-driven explore
//! against the real `shatter-rust` subprocess, using known-answer Rust target
//! programs from the external examples checkout.
//!
//! ## Why this exists
//!
//! Rust frontend regressions in solver wiring, instrumentor flow tracking, or
//! execute-response decoding can pass `task check` (which runs Rust frontend
//! unit tests, conformance, and parity) without anyone exercising the real
//! analyze → instrument → execute → solve loop end-to-end. The TS-only
//! `e2e_concolic.rs` does not catch Rust-side breakage.
//!
//! ## Why orchestrator::explore() works against the Rust frontend
//!
//! `Command::Execute` carries `function` but no `file` — TS and Go each retain
//! per-execute file context internally. The Rust frontend keeps a `last_file`
//! field set by Analyze and Instrument (`shatter-rust/src/handler.rs:552, 620`)
//! and falls back to it when Execute arrives without `file` (line 803). So the
//! orchestrator's stock Execute pattern works once Analyze + Instrument have
//! been called for the target file. (An older comment in
//! `rust_explore_integration.rs` claimed the orchestrator could not drive the
//! Rust frontend — that comment is now stale.)
//!
//! ## Known-answer coverage
//!
//! The acceptance criteria called for arithmetic, string, and nested-control
//! shapes. Pure string-branch shapes (`fn(s: &str) -> ...`) are not executable
//! through the current Rust harness because the generated launcher cannot
//! deserialize a borrowed `&str` from a JSON value (serde requires owned
//! types). Until a `String`-param example or a harness change lifts that
//! restriction, the three cases below cover:
//!
//! - **Arithmetic cascade** — `arithmetic::classify_number` (i64, 4 branches).
//! - **Nested control with float guard** — `self_hosting::classify_float`
//!   (division-by-zero guard then ratio threshold; the natural
//!   nested-control shape).
//! - **Enum + match guard** — `enums::classify_option`
//!   (`Option<i32>` with match guard; substitutes for the string-branch case).

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult};
use shatter_core::protocol::{
    Command as ProtoCommand, FunctionAnalysis, ResponseResult,
};

/// Path to the Rust frontend binary. Mirrors `support/rust_frontend_harness.rs`
/// so this test stays self-contained (it is run via `cargo test --test
/// e2e_concolic_rust` and lives outside the existing harness layout).
fn rust_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let standalone = manifest_dir.join("../shatter-rust/target/debug/shatter-rust");
    if standalone.exists() {
        return standalone;
    }
    let workspace = manifest_dir.join("../target/debug/shatter-rust");
    if workspace.exists() {
        return workspace;
    }
    panic!(
        "Rust frontend not built. Run `cargo build --manifest-path shatter-rust/Cargo.toml`.\n\
         Checked: {}\n         {}",
        standalone.display(),
        workspace.display()
    );
}

/// Resolve a workspace-relative path from the shatter-core crate.
fn workspace_path(relative: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join(relative)
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

fn rust_examples_dir() -> PathBuf {
    examples_root().join("rust/src")
}

const RUST_FRONTEND_BUILD_TIMEOUT_SECS: &str = "180";
const RUST_FRONTEND_EXEC_TIMEOUT_SECS: &str = "60";
const RUST_FRONTEND_REQUEST_TIMEOUT_SECS: u64 = 240;

async fn spawn_rust_frontend() -> Frontend {
    let frontend_path = rust_frontend_path();
    let runtime_path = workspace_path("../shatter-rust-runtime");
    assert!(
        runtime_path.join("Cargo.toml").exists(),
        "shatter-rust-runtime not found at {}",
        runtime_path.display()
    );

    let mut config = FrontendConfig::new(frontend_path);
    config.request_timeout = Duration::from_secs(RUST_FRONTEND_REQUEST_TIMEOUT_SECS);
    config.env_vars.push((
        "SHATTER_RUNTIME_PATH".to_string(),
        runtime_path.to_string_lossy().into_owned(),
    ));
    config.env_vars.push((
        "SHATTER_BUILD_TIMEOUT".to_string(),
        RUST_FRONTEND_BUILD_TIMEOUT_SECS.to_string(),
    ));
    config.env_vars.push((
        "SHATTER_EXEC_TIMEOUT".to_string(),
        RUST_FRONTEND_EXEC_TIMEOUT_SECS.to_string(),
    ));

    Frontend::spawn(&config)
        .await
        .expect("failed to spawn Rust frontend")
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
            .find(|f| f.name == function_name)
            .unwrap_or_else(|| panic!("function '{function_name}' not found in analysis results")),
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

fn is_offline_compile_error(message: &str) -> bool {
    message.contains("spurious network error")
        || message.contains("download of config.json failed")
        || message.contains("Could not resolve host")
        || message.contains("Could not resolve hostname")
}

/// classify_number — 4 branches: n<0 → "negative", n==0 → "zero",
/// n<=100 → "small positive", n>100 → "large positive".
///
/// The "zero" branch requires exactly n=0, the canonical Z3-only target
/// (random integer generation almost never lands on 0).
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_classify_number_discovers_all_branches() {
    let file = rust_examples_dir().join("arithmetic.rs");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classify_number").await;
    assert_eq!(analysis.params.len(), 1, "classify_number takes 1 param");
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in classify_number"
    );

    instrument_function(&mut frontend, &file_str, "classify_number").await;

    let config = ExploreConfig {
        max_iterations: Some(40),
        max_executions: Some(120),
        plateau_threshold: 25,
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(7)], vec![serde_json::json!(-3)]];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "classify_number",
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
    .await;

    let (result, _) = match explore_outcome {
        Ok(pair) => pair,
        Err(err) => {
            let message = format!("{err:?}");
            if is_offline_compile_error(&message) {
                eprintln!("skipping e2e_rust_classify_number: {message}");
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    let return_values = return_value_set(&result);
    for expected in [
        "\"negative\"",
        "\"zero\"",
        "\"small positive\"",
        "\"large positive\"",
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

/// classify_float — 3 branches with a nested division-by-zero guard.
///   1. total == 0           → 0  (early return)
///   2. ratio >= threshold   → 1  (after the guard)
///   3. ratio <  threshold   → 2  (after the guard)
///
/// This is the genuine nested-control shape: an outer guard followed by a
/// derived comparison. Triggering branch 1 requires `total = 0`, which random
/// generation rarely picks; reaching it via the orchestrator confirms branch
/// frontier exploration on the Rust pipeline.
///
/// NOTE: The orchestrator currently logs `solver: constraint references
/// unknown param "ratio"` panics during this test because the Rust analyzer
/// emits a branch condition over the local binding `ratio` rather than the
/// params. The orchestrator catches the solver panic and falls back to other
/// strategies, so the test still proves all three branches are reached. The
/// underlying analyzer/solver gap is filed separately and is not in scope
/// for str-o9rz.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_classify_float_discovers_nested_branches() {
    let file = rust_examples_dir().join("self_hosting.rs");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classify_float").await;
    assert_eq!(analysis.params.len(), 3, "classify_float takes 3 params");
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in classify_float"
    );

    instrument_function(&mut frontend, &file_str, "classify_float").await;

    let config = ExploreConfig {
        max_iterations: Some(30),
        max_executions: Some(80),
        plateau_threshold: 20,
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![
            serde_json::json!(7usize),
            serde_json::json!(10usize),
            serde_json::json!(0.5f64),
        ],
        vec![
            serde_json::json!(2usize),
            serde_json::json!(10usize),
            serde_json::json!(0.5f64),
        ],
    ];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "classify_float",
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
    .await;

    let (result, _) = match explore_outcome {
        Ok(pair) => pair,
        Err(err) => {
            let message = format!("{err:?}");
            if is_offline_compile_error(&message) {
                eprintln!("skipping e2e_rust_classify_float: {message}");
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    let return_values = return_value_set(&result);
    for expected in ["0", "1", "2"] {
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

/// classify_option — 3 branches over `Option<i32>`:
///   1. `Some(n) if n > 0` → "positive: {n}"
///   2. `Some(n)`          → "non-positive: {n}"
///   3. `None`              → "absent"
///
/// Stands in for the string-branch case from the original acceptance criteria
/// (see file header). Exercises enum dispatch + match guard, a shape neither
/// of the arithmetic tests touches.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_classify_option_discovers_enum_branches() {
    let file = rust_examples_dir().join("enums.rs");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classify_option").await;
    assert_eq!(analysis.params.len(), 1, "classify_option takes 1 param");
    assert!(
        !analysis.branches.is_empty(),
        "analyze should detect branches in classify_option"
    );

    instrument_function(&mut frontend, &file_str, "classify_option").await;

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(50),
        plateau_threshold: 15,
        ..Default::default()
    };

    // Option<i32> serializes as either a number (Some) or null (None).
    let seed_inputs = vec![
        vec![serde_json::json!(7)],
        vec![serde_json::json!(-1)],
        vec![serde_json::json!(null)],
    ];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "classify_option",
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
    .await;

    let (result, _) = match explore_outcome {
        Ok(pair) => pair,
        Err(err) => {
            let message = format!("{err:?}");
            if is_offline_compile_error(&message) {
                eprintln!("skipping e2e_rust_classify_option: {message}");
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    let return_values = return_value_set(&result);
    let saw_positive = return_values.iter().any(|v| v.contains("positive: "));
    let saw_non_positive = return_values.iter().any(|v| v.contains("non-positive: "));
    let saw_absent = return_values.iter().any(|v| v.contains("absent"));
    assert!(
        saw_positive,
        "should discover 'positive: N' branch; found: {return_values:?}"
    );
    assert!(
        saw_non_positive,
        "should discover 'non-positive: N' branch; found: {return_values:?}"
    );
    assert!(
        saw_absent,
        "should discover 'absent' branch; found: {return_values:?}"
    );
    assert!(
        result.unique_paths >= 3,
        "should have at least 3 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Write a minimal standalone crate exposing `func_src` and return the path to
/// its `src/lib.rs`. The crate has no external dependencies so the Rust
/// frontend harness builds it without network access.
fn write_temp_crate(dir: &Path, func_src: &str) -> PathBuf {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"shatter_ddxe_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .expect("write Cargo.toml");
    let lib = src_dir.join("lib.rs");
    std::fs::write(&lib, func_src).expect("write lib.rs");
    lib
}

/// str-ddxe regression gate: a function taking a `u8` must be executable
/// end-to-end. Before the fix, the core input generator produced full-i64-range
/// integers (e.g. 926, negatives, i64::MAX) for the bare `Int` type the Rust
/// analyzer emitted for `u8`; those failed `serde_json::from_value` into `u8`
/// ("invalid value: integer `926`, expected u8"), yielding only error_only
/// outcomes and explorer timeouts. With sized `Int{width,signed}` carried
/// through generation and solving, generated/solved `u8` inputs stay in [0,255]
/// and the function's real return branches are reached.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_u8_param_stays_in_range_and_executes() {
    let func_src = r#"
/// Classify a byte. All branches are reachable only with in-range u8 values.
pub fn classify_byte(b: u8) -> &'static str {
    if b < 10 {
        "low"
    } else if b == 200 {
        "match-200"
    } else if b > 250 {
        "high"
    } else {
        "mid"
    }
}
"#;

    let tmp = tempfile::tempdir().expect("create tempdir");
    let lib = write_temp_crate(tmp.path(), func_src);
    let file_str = lib.to_string_lossy().to_string();

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classify_byte").await;
    assert_eq!(analysis.params.len(), 1, "classify_byte takes 1 param");
    // The analyzer must report the u8 param as a sized unsigned 8-bit int.
    assert_eq!(
        analysis.params[0].typ,
        shatter_core::types::TypeInfo::Int {
            int_width: Some(8),
            int_signed: Some(false),
        },
        "u8 param must carry width=8, signed=false"
    );

    instrument_function(&mut frontend, &file_str, "classify_byte").await;

    let config = ExploreConfig {
        max_iterations: Some(40),
        max_executions: Some(120),
        plateau_threshold: 25,
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(5)], vec![serde_json::json!(100)]];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "classify_byte",
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
    .await;

    let (result, _) = match explore_outcome {
        Ok(pair) => pair,
        Err(err) => {
            let message = format!("{err:?}");
            if is_offline_compile_error(&message) {
                eprintln!("skipping e2e_rust_u8_param: {message}");
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    // Every integer input ever executed must be a valid u8 (str-ddxe). An
    // out-of-range value would prove the generator/solver escaped the range.
    for (inputs, _mocks, _exec) in &result.raw_results {
        for v in inputs {
            if let Some(n) = v.as_i64() {
                assert!(
                    (0..=255).contains(&n),
                    "u8 param received out-of-range value {n}: {inputs:?}"
                );
            }
        }
    }

    // The outcome must NOT be error-only: real string branches must be reached,
    // which only happens if the u8 inputs deserialized successfully.
    let return_values = return_value_set(&result);
    let real_returns: Vec<&String> = return_values
        .iter()
        .filter(|v| !v.starts_with("ERROR:"))
        .collect();
    assert!(
        !real_returns.is_empty(),
        "u8 function must produce non-error return values; got: {return_values:?}"
    );
    // The exact-equality branch b==200 is the Z3-only target (random generation
    // rarely lands on 200), so reaching it confirms in-range solving works.
    assert!(
        return_values.iter().any(|v| v.contains("match-200")),
        "should reach the b==200 branch via in-range Z3 solving; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}
