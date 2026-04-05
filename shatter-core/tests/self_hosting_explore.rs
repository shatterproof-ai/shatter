//! Self-hosting exploration tests: shatter explores its own algorithms.
//!
//! These tests run the full Rust frontend pipeline (analyze → instrument →
//! execute) against functions extracted from shatter-core. This validates that
//! shatter can meaningfully explore the kind of code it is built from —
//! the "shatter testing shatter" milestone.
//!
//! Each test documents expected branches and verifies that executing with
//! targeted inputs discovers distinct execution paths through real
//! shatter-core algorithms.

use std::collections::HashSet;
use std::path::PathBuf;

#[path = "support/rust_frontend_harness.rs"]
mod rust_frontend_harness;

use rust_frontend_harness::{
    analyze_function, collect_return_values, examples_root, execute_function_raw,
    instrument_function, spawn_rust_frontend, with_rust_frontend_test_lock,
};

fn self_hosting_file() -> PathBuf {
    examples_root().join("rust/src/self_hosting.rs")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// classify_float — 3 branches: total==0 → 0 (inconclusive),
/// ratio>=threshold → 1 (integer-treating), ratio<threshold → 2 (float-sensitive).
///
/// Extracted from shatter-core::float_probe::classify.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn self_hosting_classify_float() {
    with_rust_frontend_test_lock(async {
        let file = self_hosting_file();
        let file_str = file.to_string_lossy().to_string();
        let mut frontend = spawn_rust_frontend().await;

        // Analyze
        let analysis = analyze_function(&mut frontend, &file_str, "classify_float").await;
        assert_eq!(analysis.params.len(), 3, "classify_float takes 3 params");
        assert!(!analysis.branches.is_empty(), "should detect branches");

        // Instrument
        instrument_function(&mut frontend, &file_str, "classify_float").await;

        // Execute with inputs targeting each branch:
        // Branch 1: total==0 → inconclusive (0)
        // Branch 2: agreements=4, total=5, threshold=0.8 → ratio=0.8 >= 0.8 → integer-treating (1)
        // Branch 3: agreements=1, total=5, threshold=0.8 → ratio=0.2 < 0.8 → float-sensitive (2)
        let test_inputs: Vec<(Vec<serde_json::Value>, &str)> = vec![
            (
                vec![
                    serde_json::json!(0),
                    serde_json::json!(0),
                    serde_json::json!(0.5),
                ],
                "0",
            ),
            (
                vec![
                    serde_json::json!(4),
                    serde_json::json!(5),
                    serde_json::json!(0.8),
                ],
                "1",
            ),
            (
                vec![
                    serde_json::json!(1),
                    serde_json::json!(5),
                    serde_json::json!(0.8),
                ],
                "2",
            ),
        ];

        let mut results = Vec::new();
        for (inputs, expected) in &test_inputs {
            let result =
                execute_function_raw(&mut frontend, &file_str, "classify_float", inputs.clone())
                    .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "classify_float({inputs:?}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            3,
            "should discover all 3 branches; found: {return_values:?}"
        );

        // Verify instrumentation produced branch paths
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

/// coverage_percentages — 2 branches: total_branches==0 → all zeros,
/// total_branches>0 → computed percentages.
///
/// Extracted from shatter-core::coverage_metrics::CoverageMetrics::percentages.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn self_hosting_coverage_percentages() {
    with_rust_frontend_test_lock(async {
        let file = self_hosting_file();
        let file_str = file.to_string_lossy().to_string();
        let mut frontend = spawn_rust_frontend().await;

        let analysis = analyze_function(&mut frontend, &file_str, "coverage_percentages").await;
        assert_eq!(
            analysis.params.len(),
            5,
            "coverage_percentages takes 5 params"
        );
        assert!(!analysis.branches.is_empty(), "should detect branches");

        instrument_function(&mut frontend, &file_str, "coverage_percentages").await;

        // Branch 1: total_branches==0 → all zeros
        // Branch 2: total_branches=10, z3=5, random=3, user=1, uncovered=1 → percentages
        let test_inputs: Vec<(Vec<serde_json::Value>, &str)> = vec![
            (
                vec![
                    serde_json::json!(0),
                    serde_json::json!(0),
                    serde_json::json!(0),
                    serde_json::json!(0),
                    serde_json::json!(0),
                ],
                "[0.0,0.0,0.0,0.0]",
            ),
            (
                vec![
                    serde_json::json!(10),
                    serde_json::json!(5),
                    serde_json::json!(3),
                    serde_json::json!(1),
                    serde_json::json!(1),
                ],
                "[50.0,30.0,10.0,10.0]",
            ),
        ];

        let mut results = Vec::new();
        for (inputs, expected) in &test_inputs {
            let result = execute_function_raw(
                &mut frontend,
                &file_str,
                "coverage_percentages",
                inputs.clone(),
            )
            .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "coverage_percentages({inputs:?}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            2,
            "should discover both branches; found: {return_values:?}"
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// symexpr_ratio — 2 branches: total==0 → 0.0, otherwise → ratio.
///
/// Extracted from shatter-core::coverage_metrics::CoverageMetrics::symexpr_ratio.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn self_hosting_symexpr_ratio() {
    with_rust_frontend_test_lock(async {
        let file = self_hosting_file();
        let file_str = file.to_string_lossy().to_string();
        let mut frontend = spawn_rust_frontend().await;

        let analysis = analyze_function(&mut frontend, &file_str, "symexpr_ratio").await;
        assert_eq!(analysis.params.len(), 2, "symexpr_ratio takes 2 params");

        instrument_function(&mut frontend, &file_str, "symexpr_ratio").await;

        // Branch 1: both zero → 0.0
        // Branch 2: symexpr=3, unknown=1 → 0.75
        let test_inputs: Vec<(Vec<serde_json::Value>, &str)> = vec![
            (vec![serde_json::json!(0), serde_json::json!(0)], "0.0"),
            (vec![serde_json::json!(3), serde_json::json!(1)], "0.75"),
        ];

        let mut results = Vec::new();
        for (inputs, expected) in &test_inputs {
            let result =
                execute_function_raw(&mut frontend, &file_str, "symexpr_ratio", inputs.clone())
                    .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "symexpr_ratio({inputs:?}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        let return_values = collect_return_values(&results);
        assert_eq!(
            return_values.len(),
            2,
            "should discover both branches; found: {return_values:?}"
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}

/// executions_agree — 5 branches across path comparison and error/value matching:
/// path mismatch → false, both errors + same value → true,
/// both errors + different value → false, both ok + same value → true,
/// mixed error/ok → false.
///
/// Simplified from shatter-core::float_probe::executions_agree.
#[tokio::test]
#[ignore = "slow: spawns Rust frontend subprocess"]
async fn self_hosting_executions_agree() {
    with_rust_frontend_test_lock(async {
        let file = self_hosting_file();
        let file_str = file.to_string_lossy().to_string();
        let mut frontend = spawn_rust_frontend().await;

        let analysis = analyze_function(&mut frontend, &file_str, "executions_agree").await;
        assert_eq!(analysis.params.len(), 6, "executions_agree takes 6 params");
        assert!(!analysis.branches.is_empty(), "should detect branches");

        instrument_function(&mut frontend, &file_str, "executions_agree").await;

        // path_a, path_b, a_has_error, b_has_error, a_value, b_value
        let test_inputs: Vec<(Vec<serde_json::Value>, &str)> = vec![
            // Different paths → false
            (
                vec![
                    serde_json::json!(1),
                    serde_json::json!(2),
                    serde_json::json!(false),
                    serde_json::json!(false),
                    serde_json::json!(10),
                    serde_json::json!(10),
                ],
                "false",
            ),
            // Same path, both errors, same value → true
            (
                vec![
                    serde_json::json!(5),
                    serde_json::json!(5),
                    serde_json::json!(true),
                    serde_json::json!(true),
                    serde_json::json!(42),
                    serde_json::json!(42),
                ],
                "true",
            ),
            // Same path, both ok, same value → true
            (
                vec![
                    serde_json::json!(5),
                    serde_json::json!(5),
                    serde_json::json!(false),
                    serde_json::json!(false),
                    serde_json::json!(10),
                    serde_json::json!(10),
                ],
                "true",
            ),
            // Same path, mixed error/ok → false
            (
                vec![
                    serde_json::json!(5),
                    serde_json::json!(5),
                    serde_json::json!(true),
                    serde_json::json!(false),
                    serde_json::json!(10),
                    serde_json::json!(10),
                ],
                "false",
            ),
            // Same path, both ok, different value → false
            (
                vec![
                    serde_json::json!(5),
                    serde_json::json!(5),
                    serde_json::json!(false),
                    serde_json::json!(false),
                    serde_json::json!(10),
                    serde_json::json!(20),
                ],
                "false",
            ),
        ];

        let mut results = Vec::new();
        for (inputs, expected) in &test_inputs {
            let result =
                execute_function_raw(&mut frontend, &file_str, "executions_agree", inputs.clone())
                    .await;

            let ret_str = result
                .return_value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());

            assert_eq!(
                &ret_str, expected,
                "executions_agree({inputs:?}) should return {expected}, got {ret_str}"
            );

            results.push(result);
        }

        // We expect at least 3 distinct return value + path combinations
        // (true, false with different branch paths)
        let return_values = collect_return_values(&results);
        assert!(
            return_values.len() >= 2,
            "should discover at least true and false paths; found: {return_values:?}"
        );

        // Verify distinct branch paths exist (different paths through the match)
        let distinct_paths: HashSet<String> = results
            .iter()
            .map(|r| format!("{:?}", r.branch_path))
            .collect();
        assert!(
            distinct_paths.len() >= 3,
            "should have at least 3 distinct branch paths; found {}",
            distinct_paths.len()
        );

        frontend.shutdown().await.expect("frontend shutdown failed");
    })
    .await;
}
