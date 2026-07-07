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

/// Write a minimal multi-file library crate whose target function
/// (`src/logic.rs::classify_widget`) takes, BY VALUE, a struct (`Widget`)
/// defined in a SIBLING module (`src/domain.rs`), whose own field is a struct
/// (`Dimensions`) defined in yet another sibling module (`src/shapes.rs`).
///
/// This is the str-do53 cross-file synthesis regression fixture. It exercises,
/// end-to-end through the real frontend (analyze → instrument → execute → Z3):
/// - same-crate cross-FILE struct resolution (`Widget` lives in `domain.rs`,
///   the consumer in `logic.rs`) — without the crate-wide type registry
///   (`build_crate_type_registry` in shatter-rust `analyzer.rs`) `Widget` would
///   degrade to `Opaque` and the function would be skipped as unexecutable;
/// - RECURSIVE cross-file field synthesis (`Widget.dims` is `Dimensions` from a
///   third file `shapes.rs`), so a regression that stopped recursing into
///   cross-file field types would surface as a flat/opaque `dims` field.
///
/// The structs derive serde `Deserialize` because the frontend executes
/// crate-resident cross-file functions through the crate-bridge harness, which
/// materializes each argument via `serde_json::from_value::<T>` and therefore
/// requires `T: DeserializeOwned` (see `generate_crate_bridge_wrapper` in the
/// shatter-rust executor). This mirrors the cross-frontend contract: TS/Go pass
/// synthesized objects by value too; the by-value/owned-then-borrow shim is a
/// Rust harness detail, but the protocol-visible semantics (a cross-file struct
/// param becomes an executable `Object`) are identical across frontends.
///
/// Returns the path to `src/logic.rs`.
fn write_temp_cross_file_crate(dir: &Path) -> PathBuf {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"shatter_do53_fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n\n\
         [dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
    )
    .expect("write Cargo.toml");
    std::fs::write(
        src_dir.join("lib.rs"),
        "pub mod shapes;\npub mod domain;\npub mod logic;\n",
    )
    .expect("write lib.rs");
    std::fs::write(
        src_dir.join("shapes.rs"),
        "use serde::Deserialize;\n\n\
         #[derive(Deserialize)]\npub struct Dimensions {\n    pub length: i64,\n    pub height: i64,\n}\n",
    )
    .expect("write shapes.rs");
    std::fs::write(
        src_dir.join("domain.rs"),
        "use serde::Deserialize;\n\nuse crate::shapes::Dimensions;\n\n\
         #[derive(Deserialize)]\npub struct Widget {\n    pub size: i64,\n    pub dims: Dimensions,\n}\n",
    )
    .expect("write domain.rs");
    let logic = src_dir.join("logic.rs");
    std::fs::write(
        &logic,
        "use crate::domain::Widget;\n\n\
         /// Branches only on the cross-file struct's own field. The\n\
         /// `size == 4242` arm is the canonical Z3-only target: it is a\n\
         /// non-boundary value, and boundary-biased random integer generation\n\
         /// (see `generate_int` in shatter-core `input_gen.rs`, which favors 0,\n\
         /// -1, 1 and type extremes) essentially never lands on exactly 4242.\n\
         /// Reaching it therefore proves the solver produced a concrete input\n\
         /// for a symbolic FIELD of a synthesized cross-file struct and that the\n\
         /// solved `w.size` value was overlaid back into the object argument.\n\
         pub fn classify_widget(w: Widget) -> &'static str {\n\
         \x20   if w.size < 0 {\n        \"negative\"\n\
         \x20   } else if w.size == 4242 {\n        \"answer\"\n\
         \x20   } else if w.size <= 100 {\n        \"small\"\n\
         \x20   } else {\n        \"large\"\n    }\n}\n",
    )
    .expect("write logic.rs");
    logic
}

/// str-do53 regression gate: a function taking a struct defined in a SIBLING
/// file of the same crate — whose own field is a struct from a THIRD file —
/// must be analyzable AND executable end-to-end, reaching Z3-solved branch
/// coverage. Before same-crate cross-file synthesis
/// (`build_crate_type_registry`), such a parameter degraded to `Opaque` and the
/// consumer function was skipped as unexecutable; this is the single biggest
/// deep-coverage gap for real crates that split domain types from their
/// consumers (measured on pickpackit's `suggestions.rs`).
///
/// This test is the pipeline-level counterpart to the analyzer unit tests in
/// `shatter-rust/src/analyzer.rs` (`cross_file_struct_resolves_to_object` et
/// al.): a module can pass its own unit tests while being silently disconnected
/// from the analyze → instrument → execute → solve pipeline, so cross-file
/// synthesis is only "done" once it reaches Z3-solved coverage through the real
/// frontend subprocess.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_cross_file_struct_discovers_branches() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let logic = write_temp_cross_file_crate(tmp.path());
    let file_str = logic.to_string_lossy().to_string();

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classify_widget").await;
    assert_eq!(analysis.params.len(), 1, "classify_widget takes 1 param");
    // The cross-file struct must synthesize to an `Object`, and its nested
    // cross-file field must itself resolve RECURSIVELY to an `Object` (not stay
    // `Opaque`). This asserts both slices of str-do53 at the analyze layer.
    match &analysis.params[0].typ {
        shatter_core::types::TypeInfo::Object { fields } => {
            assert!(
                matches!(
                    fields.iter().find(|(n, _)| n == "size"),
                    Some((_, shatter_core::types::TypeInfo::Int { .. }))
                ),
                "cross-file Widget.size must resolve to Int; got fields {fields:?}"
            );
            match fields.iter().find(|(n, _)| n == "dims") {
                Some((_, shatter_core::types::TypeInfo::Object { fields: nested })) => {
                    assert!(
                        nested.iter().any(|(n, _)| n == "height"),
                        "recursively-synthesized cross-file Dimensions must expose its \
                         fields; got {nested:?}"
                    );
                }
                other => panic!(
                    "Widget.dims must recursively resolve to a cross-file Object; got {other:?}"
                ),
            }
        }
        other => panic!("cross-file Widget param must synthesize to Object, got {other:?}"),
    }

    instrument_function(&mut frontend, &file_str, "classify_widget").await;

    let config = ExploreConfig {
        max_iterations: Some(40),
        max_executions: Some(120),
        plateau_threshold: 25,
        ..Default::default()
    };

    // Seeds must be COMPLETE `Widget` JSON, including the nested `Dimensions`,
    // or the crate-bridge deserialize step rejects them (`missing field dims`).
    let seed_inputs = vec![
        vec![serde_json::json!({"size": 7, "dims": {"length": 1, "height": 2}})],
        vec![serde_json::json!({"size": -3, "dims": {"length": 1, "height": 2}})],
    ];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "classify_widget",
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
                eprintln!("skipping e2e_rust_cross_file_struct: {message}");
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    let return_values = return_value_set(&result);
    for expected in ["\"negative\"", "\"answer\"", "\"small\"", "\"large\""] {
        assert!(
            return_values.contains(expected),
            "should discover cross-file-struct branch returning {expected}; \
             found: {return_values:?}"
        );
    }
    assert!(
        result.unique_paths >= 4,
        "should have at least 4 unique paths; got {}",
        result.unique_paths
    );
    // The `size == 4242` ("answer") branch is a non-boundary exact-equality
    // target: it is only reachable by solving a constraint on a symbolic field
    // of the synthesized cross-file struct and overlaying the solved value back
    // into the object argument. Boundary-biased random generation essentially
    // never lands on 4242, so reaching "answer" AND a non-zero Z3 count together
    // prove the Z3 path (not luck) closed the branch.
    assert!(
        result.z3_generated > 0,
        "Z3 should have generated at least one input to hit the size==4242 branch on the \
         cross-file struct; got z3_generated={}",
        result.z3_generated
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// str-w17c regression gate: the crate-backed Axum harness must resolve the
/// inner extractor/state types it names by bare identifier (e.g. `AppStateLike`,
/// a custom `FromRequestParts` extractor, and `Uuid` from `use uuid::Uuid;`)
/// even when those names arrive via non-`crate::` imports (`use super::...`,
/// external crates, re-exports).
///
/// ## The bug
///
/// `generate_axum_crate_harness` emits the State inner type and custom-extractor
/// inner type by BARE name (`let __state_value_0: AppStateLike = ...`,
/// `let __extension_value_1: WhoAmI = ...`). Before the fix,
/// `crate_use_imports_for_harness` forwarded ONLY `use crate::...` statements
/// into the harness, dropping `use super::...`, external-crate, and glob
/// imports. So when a real handler reaches the State/extractor types via
/// `use super::{AppStateLike, WhoAmI};` (the common shape when the handler lives
/// in a submodule) those names were undefined in the harness crate → rustc
/// E0412/E0433 → `CompilationFailed` → the frontend returned `status:"error"`
/// → the orchestrator treated it as a frontend skip and recorded NOTHING →
/// the function was reported `dispatch_failed` ("no successful observations
/// recorded"). This is exactly the shape that made every pickpackit Axum
/// handler with crate-local State/extractors fail at COMPILE time before any
/// request ran.
///
/// ## Why this lives in e2e (not a unit test)
///
/// The in-process `execute_axum_handler` unit tests run through the STANDALONE
/// harness path (their tempdir fixtures have no `Cargo.toml`, so
/// `find_crate_root` returns `None`). Only a real multi-file crate routes
/// through `generate_axum_crate_harness`, and only the real frontend +
/// orchestrator reproduce the "CompilationFailed → frontend skip →
/// dispatch_failed" chain. Nothing exercised the crate-backed compile path
/// end-to-end before this test.
///
/// ## What this asserts
///
/// The function must NOT be dispatch_failed: at least one `ExecuteResult` is
/// recorded in `raw_results` (the handler may legitimately throw — the point is
/// that the harness COMPILES and produces an `ExecuteResult`, proving the
/// import-forwarding fix). State and the custom extractor are supplied via
/// native-replay generator seeds; `Path<Uuid>` uses the harness default.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess and compiles harnesses"]
async fn e2e_rust_crate_backed_axum_resolves_super_and_external_extractor_types() {
    // Crate name -> alias the driver references it under (dashes -> underscores).
    const CRATE_ALIAS: &str = "shatter_w17c_fixture";

    let tmp = tempfile::tempdir().expect("create tempdir");
    let crate_dir = tmp.path();
    let src_dir = crate_dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");

    // Cargo.toml: axum + uuid + serde + async-trait. The driver crate forwards
    // these deps, so `use uuid::Uuid;` resolves in the harness once forwarded.
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\n\
         name = \"shatter_w17c_fixture\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\n\
         [lib]\n\
         path = \"src/lib.rs\"\n\n\
         [dependencies]\n\
         axum = { version = \"0.8\", features = [\"json\"] }\n\
         serde = { version = \"1\", features = [\"derive\"] }\n\
         uuid = { version = \"1\", features = [\"serde\", \"v4\"] }\n\
         async-trait = \"0.1\"\n",
    )
    .expect("write Cargo.toml");

    // Crate root: State type + a custom FromRequestParts extractor read from
    // request extensions, both `pub` at the crate root, plus `pub mod handlers;`.
    std::fs::write(
        src_dir.join("lib.rs"),
        r#"
pub mod handlers;

#[derive(Clone)]
pub struct AppStateLike {
    pub prefix: String,
}

#[derive(Clone)]
pub struct WhoAmI {
    pub id: u64,
}

impl<S> axum::extract::FromRequestParts<S> for WhoAmI
where
    S: Send + Sync,
{
    type Rejection = &'static str;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<WhoAmI>()
                .cloned()
                .ok_or("missing WhoAmI"),
        )
    }
}
"#,
    )
    .expect("write lib.rs");

    // Submodule handler. The State + custom-extractor types arrive via
    // `use super::{...}` (NON-crate::, dropped before the fix) and `Uuid` via
    // `use uuid::Uuid;` (external, also dropped before the fix). This is the
    // failing pickpackit shape.
    let handler_path = src_dir.join("handlers.rs");
    std::fs::write(
        &handler_path,
        r#"
use axum::extract::{Path, State};
use super::{AppStateLike, WhoAmI};
use uuid::Uuid;

pub async fn h(
    State(state): State<AppStateLike>,
    who: WhoAmI,
    Path(id): Path<Uuid>,
) -> String {
    format!("{}:{}:{}", state.prefix, who.id, id)
}
"#,
    )
    .expect("write handlers.rs");
    let handler_str = handler_path.to_string_lossy().to_string();

    // Native-replay generator files. They compile as modules of the DRIVER
    // crate (via `#[path] mod ...`), so they reference the fixture crate's
    // types under the crate alias, NOT `crate::user_code::...` (which only
    // exists on the standalone path).
    let state_gen = crate_dir.join("state_gen.rs");
    std::fs::write(
        &state_gen,
        format!(
            "use {CRATE_ALIAS}::AppStateLike;\n\
             use shatter_rust::generators::GeneratorResult;\n\n\
             pub fn AppStateLikeGen(recipe: Option<serde_json::Value>) -> GeneratorResult {{\n\
             \x20   let prefix = recipe\n\
             \x20       .as_ref()\n\
             \x20       .and_then(|v| v.get(\"prefix\"))\n\
             \x20       .and_then(serde_json::Value::as_str)\n\
             \x20       .unwrap_or(\"state\")\n\
             \x20       .to_string();\n\
             \x20   GeneratorResult {{\n\
             \x20       id: \"app-state-like\".to_string(),\n\
             \x20       value: Box::new(AppStateLike {{ prefix }}),\n\
             \x20       recipe: recipe.unwrap_or(serde_json::Value::Null),\n\
             \x20   }}\n\
             }}\n"
        ),
    )
    .expect("write state generator");

    let who_gen = crate_dir.join("who_gen.rs");
    std::fs::write(
        &who_gen,
        format!(
            "use {CRATE_ALIAS}::WhoAmI;\n\
             use shatter_rust::generators::GeneratorResult;\n\n\
             pub fn WhoAmIGen(recipe: Option<serde_json::Value>) -> GeneratorResult {{\n\
             \x20   let id = recipe\n\
             \x20       .as_ref()\n\
             \x20       .and_then(|v| v.get(\"id\"))\n\
             \x20       .and_then(serde_json::Value::as_u64)\n\
             \x20       .unwrap_or(0);\n\
             \x20   GeneratorResult {{\n\
             \x20       id: \"who-am-i\".to_string(),\n\
             \x20       value: Box::new(WhoAmI {{ id }}),\n\
             \x20       recipe: recipe.unwrap_or(serde_json::Value::Null),\n\
             \x20   }}\n\
             }}\n"
        ),
    )
    .expect("write who generator");

    // Native-replay seed inputs: param 0 = State, param 1 = custom extractor,
    // param 2 = Path<Uuid> (defaulted by the harness via `null`).
    let state_input = serde_json::json!({
        "__shatter_native": true,
        "handle": "frontend-state",
        "__shatter_replay": {
            "language": "rust",
            "file": state_gen.to_string_lossy(),
            "name": "AppStateLikeGen",
            "recipe": {"prefix": "pack"}
        }
    });
    let who_input = serde_json::json!({
        "__shatter_native": true,
        "handle": "frontend-who",
        "__shatter_replay": {
            "language": "rust",
            "file": who_gen.to_string_lossy(),
            "name": "WhoAmIGen",
            "recipe": {"id": 7}
        }
    });

    let mut frontend = spawn_rust_frontend().await;

    let analysis = analyze_function(&mut frontend, &handler_str, "h").await;
    assert_eq!(analysis.params.len(), 3, "h takes 3 params");

    instrument_function(&mut frontend, &handler_str, "h").await;

    let config = ExploreConfig {
        max_iterations: Some(4),
        max_executions: Some(8),
        plateau_threshold: 3,
        ..Default::default()
    };

    // The native-replay seed is executed verbatim first (UserProvidedStrategy),
    // so at least one Execute carries valid generator inputs for State + the
    // custom extractor.
    let seed_inputs = vec![vec![
        state_input,
        who_input,
        serde_json::Value::Null,
    ]];

    let explore_outcome = orchestrator::explore(
        &mut frontend,
        "h",
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
                eprintln!(
                    "skipping e2e_rust_crate_backed_axum_resolves_super_and_external_extractor_types: {message}"
                );
                frontend.shutdown().await.expect("frontend shutdown failed");
                return;
            }
            panic!("orchestrator::explore failed: {message}");
        }
    };

    // The core regression assertion: the crate-backed Axum harness COMPILED and
    // produced at least one ExecuteResult. Before the str-w17c fix the harness
    // failed to compile (AppStateLike / WhoAmI / Uuid undefined), every Execute
    // was treated as a frontend skip, and `raw_results` was empty →
    // dispatch_failed. A throw is acceptable; a recorded observation is not.
    assert!(
        !result.raw_results.is_empty(),
        "crate-backed Axum handler must record at least one ExecuteResult (not \
         dispatch_failed); empty raw_results means the harness failed to compile. \
         executions={:?}",
        result.executions
    );

    // Sanity: the recorded observation must be a real ExecuteResult, whether it
    // returned a body or threw. (If the import fix regressed, we would never get
    // here because raw_results would be empty.)
    let saw_observation = result.raw_results.iter().any(|(_, _, exec)| {
        exec.return_value.is_some() || exec.thrown_error.is_some()
    });
    assert!(
        saw_observation,
        "recorded ExecuteResult should carry a return value or thrown error: {:?}",
        result.raw_results
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}
