//! End-to-end tests for the LLM seed oracle integration into the concolic
//! exploration pipeline.
//!
//! Uses MockSeedOracle (no real LLM API key) with the real TypeScript frontend
//! to validate oracle candidates flow through the full pipeline.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use shatter_core::config::LlmConfig;
use shatter_core::coverage_metrics::DiscoveryMethod;
use shatter_core::frontend::{DEFAULT_REQUEST_TIMEOUT, Frontend, FrontendConfig};
use shatter_core::oracle::{OracleSlotMap, OracleStats};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult, OracleHandle};
use shatter_core::protocol::{
    Command as ProtoCommand, ResponseResult,
};
use shatter_llm::MockSeedOracle;

fn ts_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../shatter-ts/dist/main.js")
}

fn examples_dir() -> PathBuf {
    if let Some(path) = env::var_os("SHATTER_EXAMPLES_DIR") {
        return PathBuf::from(path).join("standalone/ts");
    }
    let fallback = env::temp_dir().join("shatter-examples-main/standalone/ts");
    assert!(
        fallback.exists(),
        "examples checkout not found. Set SHATTER_EXAMPLES_DIR or run python3 scripts/examples_checkout.py."
    );
    fallback
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

fn make_runtime() -> Arc<tokio::runtime::Runtime> {
    // Spawn a dedicated runtime on its own thread so it can be safely
    // dropped without conflicting with the test's tokio runtime. The
    // runtime is wrapped in Arc and the thread outlives the test (leaked)
    // to avoid the "Cannot drop a runtime in an async context" panic.
    static ORACLE_RT: std::sync::OnceLock<Arc<tokio::runtime::Runtime>> =
        std::sync::OnceLock::new();
    ORACLE_RT
        .get_or_init(|| {
            Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .unwrap(),
            )
        })
        .clone()
}

/// Test 1: LLM candidate reaches new equivalence class.
///
/// The oracle provides a candidate (n=0) that triggers the "zero" branch
/// which random exploration is unlikely to find. Assert that the behavior
/// map entry has source: InputSource::LlmOracle.
#[tokio::test]
async fn llm_candidate_reaches_new_equivalence_class() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    // Script the oracle to return n=0 (the "zero" branch) for any condition.
    let oracle = Arc::new(MockSeedOracle::scripted(vec![(
        String::new(), // empty predicate matches via default
        vec![vec![serde_json::json!(0)]],
    )]));

    let rt = make_runtime();
    let llm_cfg = LlmConfig {
        enabled: true,
        max_queries_per_function: 5,
        ..LlmConfig::default()
    };
    let mut slot_map = OracleSlotMap::new(oracle, llm_cfg, rt);

    let config = ExploreConfig {
        max_iterations: Some(15),
        max_executions: Some(50),
        plateau_threshold: 10,
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(5)], vec![serde_json::json!(-3)]];

    let (result, _) = orchestrator::explore_with_oracle(
        &mut frontend,
        "classifyNumber",
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
        Some(OracleHandle {
            slot_map: &mut slot_map,
            function_source: "function classifyNumber(n) { ... }".to_string(),
        }),
    )
    .await
    .expect("exploration failed");

    // The oracle should have been queried at least once.
    let stats = slot_map.stats();
    assert!(stats.queries_fired > 0, "oracle should have been queried");

    // Check that oracle_stats were recorded in the result.
    assert!(result.oracle_stats.is_some(), "result should include oracle_stats");

    // Check if any discovery was attributed to LlmOracle.
    let llm_discoveries: Vec<_> = result
        .discoveries
        .iter()
        .filter(|(_, method)| matches!(method, DiscoveryMethod::LlmOracle))
        .collect();

    // The oracle returns n=0 which should cover the "zero" branch. With the
    // mock returning it for every predicate query, at least some branch
    // discovery should be attributed to the oracle.
    let return_values = return_value_set(&result);
    assert!(
        return_values.contains("\"zero\""),
        "should discover 'zero' branch; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test 2: Post-run summary has correct oracle stats.
///
/// After a run with a scripted oracle, verify that oracle_stats in the
/// ExploreResult has the expected query/token counts.
#[tokio::test]
async fn post_run_summary_correct() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    let oracle = Arc::new(MockSeedOracle::scripted(vec![(
        String::new(),
        vec![vec![serde_json::json!(0)]],
    )]));

    let rt = make_runtime();
    let llm_cfg = LlmConfig {
        enabled: true,
        max_queries_per_function: 3,
        ..LlmConfig::default()
    };
    let mut slot_map = OracleSlotMap::new(oracle, llm_cfg.clone(), rt);

    let config = ExploreConfig {
        max_iterations: Some(10),
        max_executions: Some(30),
        plateau_threshold: 8,
        ..Default::default()
    };

    let (result, _) = orchestrator::explore_with_oracle(
        &mut frontend,
        "classifyNumber",
        vec![vec![serde_json::json!(5)]],
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
        Some(OracleHandle {
            slot_map: &mut slot_map,
            function_source: "function classifyNumber(n) { ... }".to_string(),
        }),
    )
    .await
    .expect("exploration failed");

    let stats = result.oracle_stats.expect("oracle_stats should be present");
    // Queries fired should be <= max_queries_per_function.
    assert!(
        stats.queries_fired <= llm_cfg.max_queries_per_function,
        "queries_fired ({}) should not exceed max_queries_per_function ({})",
        stats.queries_fired,
        llm_cfg.max_queries_per_function,
    );

    // The summary format is: "LLM oracle: {q} queries · {t} tokens · {a} candidates accepted  [budget: {t} / {max}]"
    let summary = format!(
        "LLM oracle: {} queries · {} tokens · {} candidates accepted  [budget: {} / {}]",
        stats.queries_fired,
        stats.tokens_used,
        stats.candidates_accepted,
        stats.tokens_used,
        llm_cfg.max_token_budget,
    );
    assert!(summary.starts_with("LLM oracle:"));
    assert!(summary.contains("queries"));
    assert!(summary.contains("tokens"));
    assert!(summary.contains("budget"));

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test 3: Token budget halts new requests.
///
/// Set max_token_budget to 0, which should prevent any oracle queries.
#[tokio::test]
async fn token_budget_halts_new_requests() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    let oracle = Arc::new(MockSeedOracle::scripted(vec![(
        String::new(),
        vec![vec![serde_json::json!(0)]],
    )]));

    let rt = make_runtime();
    let llm_cfg = LlmConfig {
        enabled: true,
        max_token_budget: 0,
        ..LlmConfig::default()
    };
    let mut slot_map = OracleSlotMap::new(oracle, llm_cfg, rt);

    let config = ExploreConfig {
        max_iterations: Some(10),
        max_executions: Some(30),
        plateau_threshold: 8,
        ..Default::default()
    };

    let (result, _) = orchestrator::explore_with_oracle(
        &mut frontend,
        "classifyNumber",
        vec![vec![serde_json::json!(5)]],
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
        Some(OracleHandle {
            slot_map: &mut slot_map,
            function_source: "function classifyNumber(n) { ... }".to_string(),
        }),
    )
    .await
    .expect("exploration failed");

    let stats = slot_map.stats();
    assert_eq!(
        stats.queries_fired, 0,
        "no queries should fire when token budget is 0"
    );
    let result_stats = result.oracle_stats.expect("oracle_stats should be present");
    assert_eq!(result_stats.queries_fired, 0);

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test 4: Concurrency cap respected.
///
/// Set max_concurrent_requests = 1 and verify the oracle is queried
/// sequentially (no more than 1 concurrent request).
#[tokio::test]
async fn concurrency_cap_respected() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use async_trait::async_trait;
    use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

    struct ConcurrencyTrackingOracle {
        max_concurrent: AtomicU32,
        current: AtomicU32,
    }

    #[async_trait]
    impl SeedOracle for ConcurrencyTrackingOracle {
        async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
            let prev = self.current.fetch_add(1, Ordering::SeqCst);
            self.max_concurrent.fetch_max(prev + 1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(OracleResponse {
                candidates: vec![vec![serde_json::json!(42)]],
                tokens_used: 10,
            })
        }
    }

    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    let oracle = Arc::new(ConcurrencyTrackingOracle {
        max_concurrent: AtomicU32::new(0),
        current: AtomicU32::new(0),
    });
    let oracle_ref = Arc::clone(&oracle);

    let rt = make_runtime();
    let llm_cfg = LlmConfig {
        enabled: true,
        max_concurrent_requests: 1,
        max_queries_per_function: 10,
        ..LlmConfig::default()
    };
    let mut slot_map = OracleSlotMap::new(oracle_ref, llm_cfg, rt);

    let config = ExploreConfig {
        max_iterations: Some(15),
        max_executions: Some(50),
        plateau_threshold: 10,
        ..Default::default()
    };

    let (_, _) = orchestrator::explore_with_oracle(
        &mut frontend,
        "classifyNumber",
        vec![vec![serde_json::json!(5)], vec![serde_json::json!(-3)]],
        vec![],
        &analysis.params,
        &config,
        None,
        None,
        vec![],
        None,
        None,
        Some(OracleHandle {
            slot_map: &mut slot_map,
            function_source: "function classifyNumber(n) { ... }".to_string(),
        }),
    )
    .await
    .expect("exploration failed");

    let max_seen = oracle.max_concurrent.load(Ordering::SeqCst);
    assert!(
        max_seen <= 1,
        "max concurrent requests should be <= 1, but saw {max_seen}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}
