//! End-to-end exploration tests using the real Rust frontend (shatter-rust).
//!
//! These tests validate the full analyze -> instrument -> execute pipeline
//! against Rust example functions. Each test documents expected branches and
//! verifies that executing with targeted inputs produces the expected return
//! values and discovers distinct execution paths.
//!
//! NOTE: The orchestrator's `explore()` cannot drive the Rust frontend because
//! `Command::Execute` in the core protocol lacks a `file` field, which the Rust
//! frontend requires (it re-reads and compiles source per execution). These tests
//! drive the pipeline manually via individual protocol commands instead.

#[path = "support/rust_frontend_harness.rs"]
mod rust_frontend_harness;

use rust_frontend_harness::{
    analyze_function, collect_return_values, execute_function_raw, instrument_function,
    spawn_rust_frontend, with_rust_frontend_test_lock, workspace_path,
};

/// Path to the Rust example source files.
fn rust_examples_dir() -> std::path::PathBuf {
    workspace_path("../examples/rust/src")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// classify_number — 4 branches: n<0 -> "negative", n==0 -> "zero",
/// n<=100 -> "small positive", n>100 -> "large positive".
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn rust_explore_classify_number() {
    with_rust_frontend_test_lock(async {
        let file = rust_examples_dir().join("arithmetic.rs");
        let file_str = file.to_string_lossy().to_string();

        let mut frontend = spawn_rust_frontend().await;

        // Analyze: verify parameter signature.
        let analysis = analyze_function(&mut frontend, &file_str, "classify_number").await;
        assert_eq!(analysis.params.len(), 1, "classify_number takes 1 param");
        assert_eq!(analysis.params[0].name, "n");

        // Verify branches were detected.
        assert!(
            !analysis.branches.is_empty(),
            "should detect branches in classify_number"
        );

        // Instrument the function.
        instrument_function(&mut frontend, &file_str, "classify_number").await;

        // Execute with inputs targeting each branch.
        let test_inputs: Vec<(serde_json::Value, &str)> = vec![
            (serde_json::json!(-5), "\"negative\""),
            (serde_json::json!(0), "\"zero\""),
            (serde_json::json!(50), "\"small positive\""),
            (serde_json::json!(200), "\"large positive\""),
        ];

        let mut results = Vec::new();
        for (input, expected) in &test_inputs {
            let result = execute_function_raw(
                &mut frontend,
                &file_str,
                "classify_number",
                vec![input.clone()],
            )
            .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "classify_number({input}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        // Verify all 4 branches produce distinct execution paths.
        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            4,
            "should discover all 4 branches; found: {return_values:?}"
        );

        // Verify branch paths are populated (instrumentation working).
        for result in &results {
            assert!(
                !result.branch_path.is_empty(),
                "branch_path should be non-empty for instrumented execution"
            );
        }

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// classify_temperature — 4 branches: temp<0.0 -> "freezing", temp<20.0 -> "cold",
/// temp<35.0 -> "comfortable", temp>=35.0 -> "hot".
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn rust_explore_classify_temperature() {
    with_rust_frontend_test_lock(async {
        let file = rust_examples_dir().join("arithmetic.rs");
        let file_str = file.to_string_lossy().to_string();

        let mut frontend = spawn_rust_frontend().await;

        let analysis = analyze_function(&mut frontend, &file_str, "classify_temperature").await;
        assert_eq!(
            analysis.params.len(),
            1,
            "classify_temperature takes 1 param"
        );

        instrument_function(&mut frontend, &file_str, "classify_temperature").await;

        let test_inputs: Vec<(serde_json::Value, &str)> = vec![
            (serde_json::json!(-10.0), "\"freezing\""),
            (serde_json::json!(10.0), "\"cold\""),
            (serde_json::json!(25.0), "\"comfortable\""),
            (serde_json::json!(40.0), "\"hot\""),
        ];

        let mut results = Vec::new();
        for (input, expected) in &test_inputs {
            let result = execute_function_raw(
                &mut frontend,
                &file_str,
                "classify_temperature",
                vec![input.clone()],
            )
            .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "classify_temperature({input}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            4,
            "should discover all 4 branches; found: {return_values:?}"
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// classify_greeting — 5 branches: match arms for "hello" -> "english",
/// "hola" -> "spanish", "bonjour" -> "french", "ciao" -> "italian",
/// default -> "unknown".
///
/// Tests analyze + instrument only. Execute is not tested because the function
/// takes `&str` which the harness cannot deserialize from JSON (serde requires
/// owned types for deserialization). The harness generates
/// `let s: &str = serde_json::from_value(...)` which fails at compile time.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn rust_explore_classify_greeting_analyze_instrument() {
    with_rust_frontend_test_lock(async {
        let file = rust_examples_dir().join("strings.rs");
        let file_str = file.to_string_lossy().to_string();

        let mut frontend = spawn_rust_frontend().await;

        // Analyze: verify parameter signature and branches.
        let analysis = analyze_function(&mut frontend, &file_str, "classify_greeting").await;
        assert_eq!(analysis.params.len(), 1, "classify_greeting takes 1 param");
        assert_eq!(analysis.params[0].name, "s");
        assert!(
            !analysis.branches.is_empty(),
            "should detect branches in classify_greeting"
        );

        // Instrument the function.
        instrument_function(&mut frontend, &file_str, "classify_greeting").await;

        // Verify literals were extracted from the match arms.
        let literal_strings: Vec<&str> = analysis
            .literals
            .iter()
            .filter_map(|lit| match lit {
                shatter_core::protocol::LiteralValue::Str { value } => Some(value.as_str()),
                _ => None,
            })
            .collect();

        // The analyzer should extract at least some of the string literals.
        assert!(
            !literal_strings.is_empty(),
            "should extract string literals from match arms; got none"
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// classify_option — 3 branches: Some(n) where n>0 -> "positive: {n}",
/// Some(n) where n<=0 -> "non-positive: {n}", None -> "absent".
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn rust_explore_classify_option() {
    with_rust_frontend_test_lock(async {
        let file = rust_examples_dir().join("enums.rs");
        let file_str = file.to_string_lossy().to_string();

        let mut frontend = spawn_rust_frontend().await;

        let analysis = analyze_function(&mut frontend, &file_str, "classify_option").await;
        assert_eq!(analysis.params.len(), 1, "classify_option takes 1 param");

        instrument_function(&mut frontend, &file_str, "classify_option").await;

        // Option<i32> serializes as either null (None) or a number (Some).
        let test_inputs: Vec<(serde_json::Value, &str)> = vec![
            (serde_json::json!(42), "\"positive: 42\""),
            (serde_json::json!(-3), "\"non-positive: -3\""),
            (serde_json::json!(null), "\"absent\""),
        ];

        let mut results = Vec::new();
        for (input, expected) in &test_inputs {
            let result = execute_function_raw(
                &mut frontend,
                &file_str,
                "classify_option",
                vec![input.clone()],
            )
            .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "classify_option({input}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            3,
            "should discover all 3 branches; found: {return_values:?}"
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// parse_config_line — 4 branches: missing '=' -> Err, empty key -> Err,
/// empty value -> Err, valid key=value -> Ok.
///
/// Tests analyze + instrument only. Execute is not tested because the function
/// takes `&str` which the harness cannot deserialize from JSON (see
/// rust_explore_classify_greeting_analyze_instrument for details).
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn rust_explore_parse_config_line_analyze_instrument() {
    with_rust_frontend_test_lock(async {
        let file = rust_examples_dir().join("error_propagation.rs");
        let file_str = file.to_string_lossy().to_string();

        let mut frontend = spawn_rust_frontend().await;

        // Analyze: verify parameter signature and branches.
        let analysis = analyze_function(&mut frontend, &file_str, "parse_config_line").await;
        assert_eq!(analysis.params.len(), 1, "parse_config_line takes 1 param");
        assert_eq!(analysis.params[0].name, "line");
        assert!(
            !analysis.branches.is_empty(),
            "should detect branches in parse_config_line"
        );

        // Instrument the function.
        instrument_function(&mut frontend, &file_str, "parse_config_line").await;

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}
