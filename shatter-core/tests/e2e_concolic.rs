//! End-to-end concolic exploration tests using the real TypeScript frontend.
//!
//! These tests validate the full pipeline: analyze -> instrument -> concolic explore
//! with Z3 solving -> verify that the solver discovers branches that random alone
//! cannot reliably find.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shatter_core::frontend::{Frontend, FrontendConfig, DEFAULT_REQUEST_TIMEOUT};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

/// Path to the TypeScript frontend entry point, resolved from the workspace root.
fn ts_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../shatter-ts/dist/main.js")
}

/// Path to the TypeScript example files, resolved from the workspace root.
fn examples_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../examples/typescript/src")
}

/// Spawn a real TypeScript frontend subprocess.
async fn spawn_ts_frontend() -> Frontend {
    let frontend_path = ts_frontend_path();
    assert!(
        frontend_path.exists(),
        "TypeScript frontend not built: {} does not exist. Run `cd shatter-ts && npm run build`.",
        frontend_path.display()
    );

    let mut config = FrontendConfig::new(PathBuf::from("node"));
    config.args = vec!["--no-warnings".to_string(), frontend_path.to_string_lossy().into_owned()];
    config.request_timeout = DEFAULT_REQUEST_TIMEOUT;

    Frontend::spawn(&config)
        .await
        .expect("failed to spawn TypeScript frontend")
}

/// Analyze a function from a TypeScript file and return the function analysis.
async fn analyze_function(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
) -> shatter_core::protocol::FunctionAnalysis {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: file.to_string(),
            function: Some(function_name.to_string()),
            project_root: None,
        })
        .await
        .expect("analyze command failed");

    match response.result {
        ResponseResult::Analyze { functions } => {
            functions
                .into_iter()
                .find(|f| f.name == function_name)
                .unwrap_or_else(|| {
                    panic!("function '{function_name}' not found in analysis results")
                })
        }
        other => panic!("expected Analyze response, got: {other:?}"),
    }
}

/// Instrument a function and assert success.
async fn instrument_function(frontend: &mut Frontend, file: &str, function_name: &str) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function_name.to_string(),
            mocks: vec![],
            project_root: None,
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

/// Collect the set of distinct return values from an exploration result.
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

/// Test that concolic exploration of classifyNumber discovers all 4 branches.
///
/// classifyNumber(n: number) has 4 paths:
///   1. n < 0        -> "negative"
///   2. n === 0      -> "zero"
///   3. n > 0, even  -> "positive-even"
///   4. n > 0, odd   -> "positive-odd"
///
/// The "zero" branch requires exactly n=0, which random number generation
/// is extremely unlikely to hit. Z3 solving should find it by negating the
/// n < 0 constraint to get n >= 0, then negating n === 0 to get n = 0.
#[tokio::test]
async fn concolic_classifynumber_discovers_all_branches() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Step 1: Analyze the function to get its type signature.
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    assert_eq!(analysis.params.len(), 1, "classifyNumber takes 1 param");

    // Step 2: Instrument the function for branch tracking.
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    // Step 3: Run concolic exploration with the orchestrator.
    let config = ExploreConfig {
        max_iterations: 20,
        max_executions: 100,
        plateau_threshold: 15,
        ..Default::default()
    };

    // Seed with a few diverse values to start exploration.
    let seed_inputs = vec![
        vec![serde_json::json!(5)],
        vec![serde_json::json!(-3)],
    ];

    let param_names: Vec<String> = analysis.params.iter().map(|p| p.name.clone()).collect();

    let result = orchestrator::explore(
        &mut frontend,
        "classifyNumber",
        seed_inputs,
        vec![], // no user-provided inputs
        &param_names,
        &config,
    )
    .await
    .expect("concolic exploration failed");

    // Step 4: Verify results.
    let return_values = return_value_set(&result);

    // The concolic pipeline should discover all 4 branches.
    assert!(
        return_values.contains("\"negative\""),
        "should discover 'negative' branch; found: {return_values:?}"
    );
    assert!(
        return_values.contains("\"zero\""),
        "should discover 'zero' branch (requires Z3 to solve n === 0); found: {return_values:?}"
    );
    assert!(
        return_values.contains("\"positive-even\""),
        "should discover 'positive-even' branch; found: {return_values:?}"
    );
    assert!(
        return_values.contains("\"positive-odd\""),
        "should discover 'positive-odd' branch; found: {return_values:?}"
    );

    // The orchestrator should have used Z3 to generate at least some inputs.
    assert!(
        result.z3_generated > 0,
        "Z3 should have generated at least one input; got z3_generated={}",
        result.z3_generated
    );

    assert!(
        result.unique_paths >= 4,
        "should have at least 4 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test concolic exploration on compareMagnitudes, which has compound arithmetic conditions.
///
/// compareMagnitudes(a, b) has 4 paths:
///   1. a+b > 100 AND a*b > 1000  -> "both-large"
///   2. a+b > 100 AND a*b <= 1000 -> "sum-large"
///   3. a+b <= 100 AND a*b > 1000 -> "product-large"
///   4. a+b <= 100 AND a*b <= 1000 -> "both-small"
///
/// The "product-large" branch (a+b <= 100 AND a*b > 1000) requires specific inputs
/// like a=50, b=21 — values that satisfy conflicting magnitude constraints. Z3 should
/// find such inputs by solving the compound conditions.
#[tokio::test]
async fn concolic_comparemagnitudes_discovers_compound_branches() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "compareMagnitudes").await;
    assert_eq!(analysis.params.len(), 2, "compareMagnitudes takes 2 params");

    instrument_function(&mut frontend, &file_str, "compareMagnitudes").await;

    let config = ExploreConfig {
        max_iterations: 30,
        max_executions: 200,
        plateau_threshold: 20,
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![serde_json::json!(10), serde_json::json!(5)],
        vec![serde_json::json!(200), serde_json::json!(200)],
    ];

    let param_names: Vec<String> = analysis.params.iter().map(|p| p.name.clone()).collect();

    let result = orchestrator::explore(
        &mut frontend,
        "compareMagnitudes",
        seed_inputs,
        vec![],
        &param_names,
        &config,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);

    // Should discover at least 3 of the 4 branches. The "product-large" case
    // (a+b <= 100 AND a*b > 1000) is the hardest for random to find but
    // Z3 should solve it.
    assert!(
        return_values.contains("\"both-small\""),
        "should discover 'both-small' branch; found: {return_values:?}"
    );
    assert!(
        return_values.contains("\"both-large\""),
        "should discover 'both-large' branch; found: {return_values:?}"
    );

    // At minimum, concolic should find more paths than random alone with 2 seeds.
    assert!(
        result.unique_paths >= 3,
        "should have at least 3 unique paths; got {}",
        result.unique_paths
    );

    assert!(
        result.z3_generated > 0,
        "Z3 should have generated inputs; got z3_generated={}",
        result.z3_generated
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test that concolic exploration discovers error-throwing paths in safeDivide.
///
/// safeDivide(numerator, denominator) has paths including:
///   1. denominator === 0 -> throws "division by zero"
///   2. !isFinite(numerator) -> throws "non-finite numerator"
///   3. Normal division paths
///
/// The denominator === 0 case requires exactly 0, which Z3 should solve.
#[tokio::test]
async fn concolic_safedivide_discovers_error_paths() {
    let file = examples_dir().join("04-errors.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "safeDivide").await;
    assert_eq!(analysis.params.len(), 2, "safeDivide takes 2 params");

    instrument_function(&mut frontend, &file_str, "safeDivide").await;

    let config = ExploreConfig {
        max_iterations: 20,
        max_executions: 100,
        plateau_threshold: 15,
        ..Default::default()
    };

    // Start with a normal case — Z3 should find the error paths.
    let seed_inputs = vec![
        vec![serde_json::json!(10), serde_json::json!(3)],
        vec![serde_json::json!(-7), serde_json::json!(2)],
    ];

    let param_names: Vec<String> = analysis.params.iter().map(|p| p.name.clone()).collect();

    let result = orchestrator::explore(
        &mut frontend,
        "safeDivide",
        seed_inputs,
        vec![],
        &param_names,
        &config,
    )
    .await
    .expect("concolic exploration failed");

    let return_values = return_value_set(&result);

    // Should discover the division-by-zero error path.
    let has_div_zero = return_values
        .iter()
        .any(|v| v.contains("division by zero"));
    assert!(
        has_div_zero,
        "should discover 'division by zero' error path; found: {return_values:?}"
    );

    // Should have multiple unique paths (normal + at least one error path).
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}
