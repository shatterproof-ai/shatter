//! E2E tests for float probe classification.
//!
//! Validates that the float probe correctly classifies number parameters as
//! integer-treating or float-sensitive when running through the real pipeline.

use std::path::{Path, PathBuf};

use shatter_core::float_probe::FloatClassification;
use shatter_core::frontend::{DEFAULT_REQUEST_TIMEOUT, Frontend, FrontendConfig};
use shatter_core::orchestrator::{self, ExploreConfig};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

const FIXTURE_SOURCE: &str = r#"
export function integerBucket(n: number): string {
    const bucket = Math.floor(n);
    if (bucket < 0) return "negative";
    if (bucket === 0) return "zero";
    if (bucket <= 10) return "small";
    if (bucket <= 100) return "medium";
    return "large";
}

export function precisionCheck(x: number): string {
    if (x !== Math.floor(x)) return "fractional";
    if (x < 0) return "negative-integer";
    if (x === 0) return "zero";
    return "positive-integer";
}

export function greet(name: string): string {
    if (name.length === 0) return "Hello, stranger!";
    return "Hello, " + name + "!";
}
"#;

fn ts_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../shatter-ts/dist/main.js")
}

async fn spawn_ts_frontend() -> Frontend {
    let frontend_path = ts_frontend_path();
    assert!(
        frontend_path.exists(),
        "TypeScript frontend not built: {} does not exist. Run `cd shatter-ts && npm run build`.",
        frontend_path.display()
    );

    let mut config = FrontendConfig::new(PathBuf::from("node"));
    config.args = vec![
        "--no-warnings".to_string(),
        frontend_path.to_string_lossy().into_owned(),
    ];
    config.request_timeout = DEFAULT_REQUEST_TIMEOUT;

    Frontend::spawn(&config)
        .await
        .expect("failed to spawn TypeScript frontend")
}

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
            execution_profile: None,
        })
        .await
        .expect("analyze command failed");

    match response.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == function_name)
            .unwrap_or_else(|| panic!("function '{function_name}' not found")),
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

fn write_fixture(dir: &Path) -> PathBuf {
    let file = dir.join("float_probe_fixture.ts");
    std::fs::write(&file, FIXTURE_SOURCE).unwrap();
    file
}

#[tokio::test]
async fn float_probe_classifies_integer_treating() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_fixture(dir.path());
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "integerBucket").await;
    instrument_function(&mut frontend, &file_str, "integerBucket").await;

    let config = ExploreConfig {
        max_iterations: Some(50),
        max_executions: Some(100),
        ..Default::default()
    };

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "integerBucket",
        vec![vec![serde_json::json!(5)]],
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
    .expect("exploration failed");

    assert_eq!(
        result.float_probe_results.len(),
        1,
        "integerBucket has one Float param"
    );
    assert_eq!(
        result.float_probe_results[0].classification,
        FloatClassification::IntegerTreating,
        "integerBucket floors its input — should be IntegerTreating"
    );
    assert!(
        result.float_probe_results[0].divergent_values.is_empty(),
        "no divergent values for integer-treating param"
    );

    frontend.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn float_probe_classifies_float_sensitive() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_fixture(dir.path());
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "precisionCheck").await;
    instrument_function(&mut frontend, &file_str, "precisionCheck").await;

    let config = ExploreConfig {
        max_iterations: Some(50),
        max_executions: Some(100),
        ..Default::default()
    };

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "precisionCheck",
        vec![vec![serde_json::json!(5)]],
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
    .expect("exploration failed");

    assert_eq!(result.float_probe_results.len(), 1);
    assert_eq!(
        result.float_probe_results[0].classification,
        FloatClassification::FloatSensitive,
        "precisionCheck checks fractional part — should be FloatSensitive"
    );
    assert!(
        !result.float_probe_results[0].divergent_values.is_empty(),
        "should have divergent values for float-sensitive param"
    );

    frontend.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn float_probe_skips_non_float_params() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_fixture(dir.path());
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "greet").await;
    instrument_function(&mut frontend, &file_str, "greet").await;

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(50),
        ..Default::default()
    };

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "greet",
        vec![vec![serde_json::json!("Alice")]],
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
    .expect("exploration failed");

    assert!(
        result.float_probe_results.is_empty(),
        "greet has no Float params — should have no probe results"
    );

    frontend.shutdown().await.expect("shutdown failed");
}

// --- Random-explorer path accounting (str-49drv.91) ---
//
// The float-probe pre-pass used to insert its path hashes straight into
// `seen_paths`, so the main loop treated those paths as already seen and they
// never reached `new_path_executions` (the basis of the explore report's
// "N path(s)" and rows). The invariant: every path counted in `unique_paths`
// has a matching `new_path_executions` row.

const PATH_COUNT_FIXTURE: &str = r#"
export function g(x: number): number {
    if (x > 1) return 1;
    return 0;
}

export function fmt2(n: number): string {
    if (n > 10) return "big";
    if (n < 0) return "neg";
    throw new Error("unformattable");
}
"#;

fn random_explorer_config(file: &str) -> shatter_core::explorer::ExploreConfig {
    use shatter_core::protocol::SetupLevel;
    shatter_core::explorer::ExploreConfig {
        file: file.to_string(),
        execution_profile: None,
        max_iterations: Some(100),
        max_executions_override: None,
        observer_pool: 1,
        observer_frontend_config: None,
        candidate_queue_capacity: None,
        seed: Some(42),
        mocks: vec![],
        mock_params: vec![],
        setup_file: None,
        setup_level: SetupLevel::Function,
        value_sources: vec![],
        capabilities: Default::default(),
        user_seeds: vec![],
        candidate_inputs: vec![],
        pool_seeds: vec![],
        project_root: None,
        loop_buckets: Default::default(),
        timeout_explore: None,
        meta_config: shatter_core::strategy::MetaConfig::default(),
        shrink_budget: 0,
        isolation: shatter_core::explorer::IsolationMode::None,
        capture_side_effects: false,
        budget_surplus: None,
        claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
        planner: None,
        default_execute_plan: None,
        prepare_id_override: None,
    }
}

/// Distinct (branch_id, taken) sequences over the raw results: the ground
/// truth for how many paths the run actually executed.
fn distinct_branch_paths(
    result: &shatter_core::explorer::ObservationOutput,
) -> std::collections::HashSet<Vec<(u32, bool)>> {
    result
        .raw_results
        .iter()
        .map(|(_, _, r)| r.branch_path.iter().map(|d| (d.branch_id, d.taken)).collect())
        .collect()
}

async fn assert_random_mode_path_accounting(function: &str, expected_paths: usize) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("path_count_fixture.ts");
    std::fs::write(&file, PATH_COUNT_FIXTURE).unwrap();
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze_function(&mut frontend, &file_str, function).await;

    let result = shatter_core::explorer::explore_function(
        &mut frontend,
        &analysis,
        &random_explorer_config(&file_str),
        None,
        None,
    )
    .await
    .expect("exploration failed");

    assert!(
        !result.float_probe_results.is_empty(),
        "{function}: float probe must run for this fixture to exercise the bug"
    );
    assert_eq!(
        distinct_branch_paths(&result).len(),
        expected_paths,
        "{function}: executed distinct paths"
    );
    assert_eq!(
        result.unique_paths, expected_paths,
        "{function}: unique_paths"
    );
    assert_eq!(
        result.new_path_executions.len(),
        result.unique_paths,
        "{function}: report rows (new_path_executions) must match unique_paths"
    );

    frontend.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn random_mode_float_probe_paths_are_counted_two_branch() {
    assert_random_mode_path_accounting("g", 2).await;
}

#[tokio::test]
async fn random_mode_float_probe_paths_are_counted_fall_through_throw() {
    assert_random_mode_path_accounting("fmt2", 3).await;
}
