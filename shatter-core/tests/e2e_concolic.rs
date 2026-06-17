//! End-to-end concolic exploration tests using the real TypeScript frontend.
//!
//! These tests validate the full pipeline: analyze -> instrument -> concolic explore
//! with Z3 solving -> verify that the solver discovers branches that random alone
//! cannot reliably find.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use shatter_core::config::GeneticConfig;
use shatter_core::coverage_metrics::{TargetBranch, TargetReason, extract_targets_concolic};
use shatter_core::frontend::{DEFAULT_REQUEST_TIMEOUT, Frontend, FrontendConfig};
use shatter_core::genetic_explorer;
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult, FrontendCapabilities};
use shatter_core::protocol::{
    Command as ProtoCommand, ExecutionAdapter, ExecutionAdapterApply, ExecutionProfile,
    ResponseResult, SetupContextEntry, SetupContextStack, SetupLevel,
};
use shatter_core::setup_manager::SetupManager;

/// Path to the TypeScript frontend entry point, resolved from the workspace root.
fn ts_frontend_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../shatter-ts/dist/main.js")
}

/// Path to standalone TypeScript example files, resolved from the workspace root.
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

/// Spawn a real TypeScript frontend subprocess.
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

/// Instrument a function and assert success.
async fn instrument_function(frontend: &mut Frontend, file: &str, function_name: &str) {
    instrument_function_with_mocks(frontend, file, function_name, vec![]).await;
}

/// Instrument a function with mock configurations and assert success.
async fn instrument_function_with_mocks(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
    mocks: Vec<shatter_core::protocol::MockConfig>,
) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function_name.to_string(),
            mocks,
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
        max_iterations: Some(20),
        max_executions: Some(100),
        plateau_threshold: 15,
        ..Default::default()
    };

    // Seed the sign and positive-parity classes so this regression does not
    // depend on random or solver ordering to observe the even branch.
    let seed_inputs = vec![
        vec![serde_json::json!(5)],
        vec![serde_json::json!(-3)],
        vec![serde_json::json!(2)],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "classifyNumber",
        seed_inputs,
        vec![], // no user-provided inputs
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
        max_iterations: Some(30),
        max_executions: Some(200),
        plateau_threshold: 20,
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![serde_json::json!(10), serde_json::json!(5)],
        vec![serde_json::json!(200), serde_json::json!(200)],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "compareMagnitudes",
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
        max_iterations: Some(20),
        max_executions: Some(100),
        plateau_threshold: 15,
        ..Default::default()
    };

    // Start with a normal case — Z3 should find the error paths.
    let seed_inputs = vec![
        vec![serde_json::json!(10), serde_json::json!(3)],
        vec![serde_json::json!(-7), serde_json::json!(2)],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "safeDivide",
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

    // Should discover the division-by-zero error path.
    let has_div_zero = return_values.iter().any(|v| v.contains("division by zero"));
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

/// Test concolic exploration on validateEmail, a string-heavy function with 20 branches.
///
/// validateEmail(email: string) validates email addresses against RFC 5321/5322 rules.
/// Key paths include:
///   - empty string -> { valid: false, reason: "empty" }
///   - no '@' -> { valid: false, reason: "missing @" }
///   - multiple '@' -> { valid: false, reason: "multiple @" }
///   - empty local/domain parts
///   - dot placement rules (starts/ends with dot, consecutive dots)
///   - plus-addressing -> { valid: true, tag: "..." }
///   - quoted local part -> { valid: true, quoted: true }
///   - standard valid -> { valid: true }
///
/// String-heavy functions are much harder for concolic exploration — most branches
/// require precise character placement. We seed with structurally diverse emails
/// and set a modest coverage bar (>=6 paths), with TODOs to raise it as string
/// solver capabilities improve.
#[tokio::test]
async fn concolic_validateemail_discovers_string_paths() {
    let file = examples_dir().join("15-email-validator.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Step 1: Analyze the function to get its type signature.
    let analysis = analyze_function(&mut frontend, &file_str, "validateEmail").await;
    assert_eq!(analysis.params.len(), 1, "validateEmail takes 1 param");

    // Step 2: Instrument the function for branch tracking.
    instrument_function(&mut frontend, &file_str, "validateEmail").await;

    // Step 3: Run concolic exploration with generous limits for string-heavy function.
    let config = ExploreConfig {
        max_iterations: Some(50),
        max_executions: Some(300),
        plateau_threshold: 30,
        ..Default::default()
    };

    // Seed with structurally diverse emails that exercise different validation paths.
    let seed_inputs = vec![
        vec![serde_json::json!("")],                 // empty string
        vec![serde_json::json!("no-at-sign")],       // missing @
        vec![serde_json::json!("a@@b.com")],         // multiple @
        vec![serde_json::json!("@domain.com")],      // empty local part
        vec![serde_json::json!("user@")],            // empty domain
        vec![serde_json::json!(".dot@x.com")],       // local starts with dot
        vec![serde_json::json!("user+tag@x.com")],   // plus-addressing
        vec![serde_json::json!("test@example.com")], // standard valid
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "validateEmail",
        seed_inputs,
        vec![], // no user-provided inputs
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

    // Step 4: Verify results.
    let return_values = return_value_set(&result);

    // With 8 diverse seeds, we should hit at least the paths those seeds trigger directly.
    // TODO(str-q26c): Raise this bar as string solver improves — the function has 20 branches,
    // and better string constraint solving should discover more of them automatically.
    assert!(
        result.unique_paths >= 6,
        "should have at least 6 unique paths from string-heavy function; got {}",
        result.unique_paths
    );

    // Verify some specific paths are discovered — these are directly triggered by seeds.
    let has_empty = return_values.iter().any(|v| v.contains("empty"));
    assert!(
        has_empty,
        "should discover 'empty' path (from empty string seed); found: {return_values:?}"
    );

    let has_missing_at = return_values.iter().any(|v| v.contains("missing @"));
    assert!(
        has_missing_at,
        "should discover 'missing @' path; found: {return_values:?}"
    );

    let has_valid = return_values
        .iter()
        .any(|v| v.contains("\"valid\":true") || v.contains("\"valid\": true"));
    assert!(
        has_valid,
        "should discover at least one valid email path; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Regression test for str-omrx: concolic explorer gets past the '@' guard
/// using boundary seeds + literal-derived seeds, matching CLI behavior.
///
/// The CLI concolic path includes `generate_boundary_inputs()` AND
/// `literals_to_candidate_inputs()`. The TS analyzer extracts "@", ".", "+"
/// etc. from the function body; these literal seeds provide structurally
/// relevant inputs that reach past guard clauses, enabling Z3 to solve
/// for deeper branches.
#[tokio::test]
async fn concolic_validateemail_with_literal_seeds() {
    let file = examples_dir().join("15-email-validator.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "validateEmail").await;
    assert_eq!(analysis.params.len(), 1, "validateEmail takes 1 param");

    instrument_function(&mut frontend, &file_str, "validateEmail").await;

    let config = ExploreConfig {
        max_iterations: Some(50),
        max_executions: Some(300),
        plateau_threshold: 30,
        ..Default::default()
    };

    // Match CLI concolic seeding: boundary seeds + literal-derived seeds.
    let mut seed_inputs = shatter_core::boundary_dict::generate_boundary_inputs(&analysis.params);
    let literal_candidates =
        shatter_core::input_gen::literals_to_candidate_inputs(&analysis.params, &analysis.literals);
    seed_inputs.extend(literal_candidates);

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "validateEmail",
        seed_inputs,
        vec![], // no user-provided inputs
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

    eprintln!("  [str-omrx] unique_paths: {}", result.unique_paths);
    eprintln!("  [str-omrx] z3_generated: {}", result.z3_generated);
    eprintln!("  [str-omrx] fuzz_generated: {}", result.fuzz_generated);
    eprintln!("  [str-omrx] total_executions: {}", result.total_executions);
    eprintln!("  [str-omrx] termination: {:?}", result.termination_reason);
    eprintln!("  [str-omrx] return_values: {return_values:?}");

    // Must get past the '@' guard — not stuck on just "empty" and "missing @".
    // "empty local part" / "empty domain" count as past the guard: the '@' was
    // found in the input, so indexOf('@') succeeded. The substring "empty" appears
    // in several guard-passing paths, so we match only the exact empty-input path
    // and the missing-@ path to distinguish "stuck" from "past the guard".
    let stuck_before_at_guard = return_values
        .iter()
        .all(|v| v.contains("missing @") || v == "{\"reason\":\"empty\",\"valid\":false}");

    assert!(
        !stuck_before_at_guard,
        "str-omrx: concolic explorer stuck before '@' guard — only found: {return_values:?}. \
         Literal seeds should provide '@' to get past indexOf('@') guard."
    );

    // With literal seeds, the literal '@' seed reliably finds 3 distinct paths
    // (empty-string, missing-@, and at least one '@'-containing path such as
    // "empty local part"). Z3 string-indexOf constraints are not fully solvable,
    // so 3 is the reliable minimum; higher counts are a bonus.
    assert!(
        result.unique_paths >= 3,
        "str-omrx: expected >=3 unique paths with boundary + literal seeds; got {}. \
         return_values: {return_values:?}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Setup/Teardown lifecycle E2E tests (str-0s76.12)
// ---------------------------------------------------------------------------

/// Path to setup fixture files, resolved from the workspace root.
fn setup_fixtures_dir() -> PathBuf {
    examples_dir()
}

/// Build FrontendCapabilities that include "setup" and "teardown".
fn capabilities_with_setup() -> FrontendCapabilities {
    FrontendCapabilities::from_raw(&[
        "analyze".into(),
        "execute".into(),
        "instrument".into(),
        "setup".into(),
        "teardown".into(),
    ])
}

/// Session-level setup returns a context that can be passed to Execute commands.
///
/// Validates the protocol round-trip: Setup -> context returned -> Teardown -> ack.
/// This tests the core mechanism that all higher-level setup flows depend on.
#[tokio::test]
async fn setup_session_context_flows_to_execute() {
    let setup_file = setup_fixtures_dir().join("setup-session.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Send session-level Setup command.
    let response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "test-session".to_string(),
            level: SetupLevel::Session,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("setup command failed");

    // Verify setup returned a context with expected fields.
    let setup_ctx = match response.result {
        ResponseResult::Setup { setup_context } => {
            assert!(
                setup_context.get("sessionId").is_some(),
                "setup context should contain sessionId; got: {setup_context:?}"
            );
            assert_eq!(
                setup_context.get("scope").and_then(|v| v.as_str()),
                Some("test-session"),
                "setup context should echo the scope"
            );
            setup_context
        }
        ResponseResult::Error { message, .. } => {
            panic!("setup returned error: {message}");
        }
        other => panic!("expected Setup response, got: {other:?}"),
    };

    // Execute a function with the setup context to verify it flows through.
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();

    let _analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    let exec_response = frontend
        .send(ProtoCommand::Execute {
            function: "classifyNumber".to_string(),
            inputs: vec![serde_json::json!(5)],
            mocks: vec![],
            setup_context: Some(SetupContextStack {
                contexts: vec![SetupContextEntry {
                    level: SetupLevel::Session,
                    context: setup_ctx.clone(),
                }],
            }),
            capture: true,
            prepare_id: None,
            execution_profile: None,
            plan: None,
        })
        .await
        .expect("execute with setup context failed");

    match &exec_response.result {
        ResponseResult::Execute(result) => {
            assert!(
                result.return_value.is_some() || result.thrown_error.is_some(),
                "execution should produce a result"
            );
        }
        ResponseResult::Error { message, .. } => {
            panic!("execute returned error: {message}");
        }
        other => panic!("expected Execute response, got: {other:?}"),
    }

    // Teardown the session.
    let teardown_response = frontend
        .send(ProtoCommand::Teardown {
            scope: "test-session".to_string(),
            level: SetupLevel::Session,
        })
        .await
        .expect("teardown command failed");

    match teardown_response.result {
        ResponseResult::TeardownAck => {}
        ResponseResult::Error { message, .. } => {
            panic!("teardown returned error: {message}");
        }
        other => panic!("expected TeardownAck response, got: {other:?}"),
    }

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// File-level setup is scoped per source file -- each file gets its own context.
///
/// Sends two file-level Setup commands with different scopes, verifying that
/// each returns a context reflecting its own scope. Then tears down both.
#[tokio::test]
async fn setup_file_level_scoped_per_file() {
    let setup_file = setup_fixtures_dir().join("setup-file-level.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Setup for file "auth.ts"
    let response_auth = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "auth.ts".to_string(),
            level: SetupLevel::File,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("setup for auth.ts failed");

    let ctx_auth = match response_auth.result {
        ResponseResult::Setup { setup_context } => {
            assert_eq!(
                setup_context.get("fileScope").and_then(|v| v.as_str()),
                Some("auth.ts"),
                "auth.ts setup should return its own scope"
            );
            assert_eq!(
                setup_context.get("initialized").and_then(|v| v.as_bool()),
                Some(true),
            );
            setup_context
        }
        other => panic!("expected Setup response for auth.ts, got: {other:?}"),
    };

    // Setup for file "data.ts" -- should get a different scope.
    let response_data = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "data.ts".to_string(),
            level: SetupLevel::File,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("setup for data.ts failed");

    let ctx_data = match response_data.result {
        ResponseResult::Setup { setup_context } => {
            assert_eq!(
                setup_context.get("fileScope").and_then(|v| v.as_str()),
                Some("data.ts"),
                "data.ts setup should return its own scope"
            );
            setup_context
        }
        other => panic!("expected Setup response for data.ts, got: {other:?}"),
    };

    // Verify contexts are distinct.
    assert_ne!(
        ctx_auth.get("fileScope"),
        ctx_data.get("fileScope"),
        "file-level contexts should be scoped independently"
    );

    // Teardown both files.
    let td_auth = frontend
        .send(ProtoCommand::Teardown {
            scope: "auth.ts".to_string(),
            level: SetupLevel::File,
        })
        .await
        .expect("teardown auth.ts failed");
    assert!(
        matches!(td_auth.result, ResponseResult::TeardownAck),
        "expected TeardownAck for auth.ts"
    );

    let td_data = frontend
        .send(ProtoCommand::Teardown {
            scope: "data.ts".to_string(),
            level: SetupLevel::File,
        })
        .await
        .expect("teardown data.ts failed");
    assert!(
        matches!(td_data.result, ResponseResult::TeardownAck),
        "expected TeardownAck for data.ts"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// When setup fails, the SetupManager records the failure and skips dependent
/// levels -- inner setup attempts return Skipped errors instead of running.
///
/// This tests both the TS frontend (returning an error response on setup failure)
/// and the Rust SetupManager (tracking failures and blocking dependents).
#[tokio::test]
async fn setup_failure_skips_dependents() {
    let setup_file = setup_fixtures_dir().join("setup-failing.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Send session-level setup with the failing fixture.
    let response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "test-session".to_string(),
            level: SetupLevel::Session,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("setup command should complete (even if it fails)");

    // The frontend should return an error (the setup() function throws).
    match &response.result {
        ResponseResult::Error { message, .. } => {
            assert!(
                message.contains("Intentional setup failure"),
                "error should contain the fixture's message; got: {message}"
            );
        }
        ResponseResult::Setup { .. } => {
            panic!("expected error from failing setup, got success");
        }
        other => panic!("expected Error response, got: {other:?}"),
    }

    // Record the failure in a SetupManager and verify skip behavior.
    let mut mgr = SetupManager::from_env();
    mgr.record_failure(SetupLevel::Session, "Intentional setup failure".into())
        .expect("record_failure with fail_on_error=false should succeed");

    // Session failure should block all inner levels.
    assert!(
        mgr.should_skip(SetupLevel::Session),
        "session level itself should be marked as skip"
    );
    assert!(
        mgr.should_skip(SetupLevel::File),
        "file level should be skipped when session failed"
    );
    assert!(
        mgr.should_skip(SetupLevel::Function),
        "function level should be skipped when session failed"
    );
    assert!(
        mgr.should_skip(SetupLevel::Execution),
        "execution level should be skipped when session failed"
    );

    // Attempting setup at File level through the manager should fail with Skipped.
    let result = mgr.setup(SetupLevel::File, "some-file.ts", serde_json::json!({}));
    assert!(
        result.is_err(),
        "file setup should be blocked by session failure"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Teardown runs in reverse order: inner levels torn down before outer levels.
///
/// Sets up Session -> File -> Function levels, then tears down in reverse order
/// (Function -> File -> Session). Each teardown validates its context, confirming
/// the protocol supports the full lifecycle.
#[tokio::test]
async fn setup_teardown_runs_in_reverse_order() {
    let setup_file = setup_fixtures_dir().join("setup-teardown-order.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Setup Session level.
    let resp_session = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "test-session".to_string(),
            level: SetupLevel::Session,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("session setup failed");
    assert!(
        matches!(&resp_session.result, ResponseResult::Setup { .. }),
        "session setup should succeed"
    );

    // Setup File level.
    let resp_file = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "auth.ts".to_string(),
            level: SetupLevel::File,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("file setup failed");
    assert!(
        matches!(&resp_file.result, ResponseResult::Setup { .. }),
        "file setup should succeed"
    );

    // Setup Function level.
    let resp_func = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str.clone(),
            scope: "validateToken".to_string(),
            level: SetupLevel::Function,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("function setup failed");
    assert!(
        matches!(&resp_func.result, ResponseResult::Setup { .. }),
        "function setup should succeed"
    );

    // Teardown in reverse order: Function -> File -> Session.
    // Each teardown validates scope matching in the fixture, so wrong-scope
    // teardown would fail.
    let td_func = frontend
        .send(ProtoCommand::Teardown {
            scope: "validateToken".to_string(),
            level: SetupLevel::Function,
        })
        .await
        .expect("function teardown failed");
    assert!(
        matches!(td_func.result, ResponseResult::TeardownAck),
        "function teardown should succeed"
    );

    let td_file = frontend
        .send(ProtoCommand::Teardown {
            scope: "auth.ts".to_string(),
            level: SetupLevel::File,
        })
        .await
        .expect("file teardown failed");
    assert!(
        matches!(td_file.result, ResponseResult::TeardownAck),
        "file teardown should succeed"
    );

    let td_session = frontend
        .send(ProtoCommand::Teardown {
            scope: "test-session".to_string(),
            level: SetupLevel::Session,
        })
        .await
        .expect("session teardown failed");
    assert!(
        matches!(td_session.result, ResponseResult::TeardownAck),
        "session teardown should succeed"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Explorer path: explore_function with setup_file runs setup/teardown lifecycle.
///
/// Uses the explorer's explore_function with a setup_file configured, verifying
/// that exploration succeeds when setup is active. This validates the explorer
/// path handles setup correctly (parity requirement with orchestrator).
#[tokio::test]
async fn explorer_explore_function_with_setup() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();
    let setup_file = setup_fixtures_dir().join("setup-session.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;

    let config = shatter_core::explorer::ExploreConfig {
        file: file_str.clone(),
        execution_profile: None,
        max_iterations: Some(10),
        observer_pool: 1,
        observer_frontend_config: None,
        candidate_queue_capacity: None,
        seed: Some(42),
        mocks: vec![],
        mock_params: vec![],
        setup_file: Some(setup_file_str),
        setup_level: SetupLevel::Function,
        value_sources: vec![],
        capabilities: capabilities_with_setup(),
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
    };

    let mut mgr = SetupManager::from_env();
    let result = shatter_core::explorer::explore_function(
        &mut frontend,
        &analysis,
        &config,
        Some(&mut mgr),
        None,
    )
    .await
    .expect("explore_function with setup should succeed");

    assert!(
        result.unique_paths >= 1,
        "explorer with setup should discover at least 1 path; got {}",
        result.unique_paths
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Orchestrator path: explore with setup_context passes context to executions.
///
/// Runs orchestrator::explore with a pre-established setup context, verifying
/// that the orchestrator correctly threads setup context through to Execute
/// commands. This is the parity test for the orchestrator path.
#[tokio::test]
async fn orchestrator_explore_with_setup_context() {
    let file = examples_dir().join("01-arithmetic.ts");
    let file_str = file.to_string_lossy().to_string();
    let setup_file = setup_fixtures_dir().join("setup-session.ts");
    let setup_file_str = setup_file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // First, establish a setup context via the protocol.
    let setup_response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file_str,
            scope: "classifyNumber".to_string(),
            level: SetupLevel::Function,
            project_root: None,
            parent_context: None,
            execution_profile: None,
        })
        .await
        .expect("setup command failed");

    let setup_ctx = match setup_response.result {
        ResponseResult::Setup { setup_context } => SetupContextStack {
            contexts: vec![SetupContextEntry {
                level: SetupLevel::Function,
                context: setup_context,
            }],
        },
        other => panic!("expected Setup response, got: {other:?}"),
    };

    // Analyze and instrument.
    let analysis = analyze_function(&mut frontend, &file_str, "classifyNumber").await;
    instrument_function(&mut frontend, &file_str, "classifyNumber").await;

    // Run orchestrator with the setup context.
    let config = ExploreConfig {
        max_iterations: Some(10),
        max_executions: Some(50),
        plateau_threshold: 8,
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(5)], vec![serde_json::json!(-3)]];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "classifyNumber",
        seed_inputs,
        vec![],
        &analysis.params,
        &config,
        Some(setup_ctx),
        None,
        vec![],
        None,
        None,
    )
    .await
    .expect("orchestrator explore with setup context failed");

    assert!(
        result.unique_paths >= 2,
        "orchestrator with setup context should discover at least 2 paths; got {}",
        result.unique_paths
    );

    // Teardown.
    let td = frontend
        .send(ProtoCommand::Teardown {
            scope: "classifyNumber".to_string(),
            level: SetupLevel::Function,
        })
        .await
        .expect("teardown failed");
    assert!(
        matches!(td.result, ResponseResult::TeardownAck),
        "teardown should succeed"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Dynamic mock branch discovery E2E tests (str-3ky9.8)
// ---------------------------------------------------------------------------

use shatter_core::auto_mock::{IoCategory, MockParam, ValueSource};
use shatter_core::protocol::{MockBehavior, MockConfig};
use shatter_core::types::TypeInfo;

/// Build a MockConfig for fs.readFileSync with varied return values
/// that cycle through the provided strings.
fn readfilesync_mock(values: &[&str]) -> MockConfig {
    MockConfig {
        symbol: "fs:readFileSync".into(),
        return_values: values.iter().map(|v| serde_json::json!(v)).collect(),
        should_track_calls: false,
        default_behavior: MockBehavior::RepeatLast,
    }
}

/// Build a MockConfig for fs.existsSync with varied boolean return values.
fn existssync_mock(values: &[bool]) -> MockConfig {
    MockConfig {
        symbol: "fs:existsSync".into(),
        return_values: values.iter().map(|v| serde_json::json!(v)).collect(),
        should_track_calls: false,
        default_behavior: MockBehavior::RepeatLast,
    }
}

/// Build a MockParam for dynamic per-iteration mock generation.
fn fs_mock_param(symbol: &str, return_type: TypeInfo, call_count: u32) -> MockParam {
    MockParam {
        symbol: symbol.into(),
        return_type,
        category: IoCategory::FileSystem,
        call_count_estimate: call_count,
        value_source: ValueSource::AutoGenerated,
    }
}

/// Helper to extract distinct return values from explorer's ObservationOutput.
fn explorer_return_value_set(
    result: &shatter_core::explorer::ObservationOutput,
) -> HashSet<String> {
    result
        .raw_results
        .iter()
        .map(|(_, _, exec)| {
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

/// Test that dynamic mock discovery finds branches gated by mock return values
/// in classifyStatus (string-length branching).
///
/// classifyStatus(configPath) reads a file via readFileSync and branches on
/// the length of the content:
///   1. length === 0  → "empty"
///   2. length < 5    → "short"
///   3. length < 15   → "medium"
///   4. length >= 15  → "long"
///
/// Uses the random explorer (not orchestrator) because it regenerates mock
/// values per iteration via mock_params, providing the variety needed to
/// discover multiple mock-gated branches.
#[tokio::test]
async fn concolic_mock_status_branches_discovered() {
    let file = examples_dir().join("17-mock-branches.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classifyStatus").await;

    // Construct mocks explicitly — the analyzer doesn't detect per-function
    // dependencies for standalone files, so we wire them manually.
    let mocks = vec![readfilesync_mock(&[
        "ab",
        "hello world",
        "",
        "a very long string here",
    ])];
    let mock_params = vec![fs_mock_param("fs:readFileSync", TypeInfo::Str, 1)];

    let config = shatter_core::explorer::ExploreConfig {
        file: file_str.clone(),
        execution_profile: None,
        max_iterations: Some(30),
        observer_pool: 1,
        observer_frontend_config: None,
        candidate_queue_capacity: None,
        seed: None,
        mocks,
        mock_params,
        setup_file: None,
        setup_level: SetupLevel::Function,
        value_sources: vec![],
        capabilities: FrontendCapabilities::default(),
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
    };

    let result =
        shatter_core::explorer::explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("explore_function failed");

    let return_values = explorer_return_value_set(&result);

    eprintln!(
        "  [str-3ky9.8/status] unique_paths: {}",
        result.unique_paths
    );
    eprintln!("  [str-3ky9.8/status] return_values: {return_values:?}");

    // Dynamic mocking with random strings of varied lengths should discover
    // at least 2 of the 4 branches.
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths from mock-gated branches; got {}. \
         return_values: {return_values:?}",
        result.unique_paths
    );

    // Verify at least one length-based branch is discovered.
    let has_branch = return_values.iter().any(|v| {
        v.contains("empty") || v.contains("short") || v.contains("medium") || v.contains("long")
    });
    assert!(
        has_branch,
        "should discover at least one status branch; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test that dynamic mock discovery explores both success and failure paths
/// in loadOrDefault (Result-like branching).
///
/// loadOrDefault(filePath) branches on:
///   1. existsSync returns falsy    → "missing"
///   2. file exists, content truthy → "loaded"
///   3. file exists, content falsy  → "empty-config"
///
/// Uses the random explorer which regenerates mock boolean values per
/// iteration via mock_params, naturally alternating between true/false.
#[tokio::test]
async fn concolic_mock_result_branches_discovered() {
    let file = examples_dir().join("17-mock-branches.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "loadOrDefault").await;

    let mocks = vec![
        existssync_mock(&[true, false, true]),
        readfilesync_mock(&["hello world", "", "content"]),
    ];
    let mock_params = vec![
        fs_mock_param("fs:existsSync", TypeInfo::Bool, 1),
        fs_mock_param("fs:readFileSync", TypeInfo::Str, 1),
    ];

    let config = shatter_core::explorer::ExploreConfig {
        file: file_str.clone(),
        execution_profile: None,
        max_iterations: Some(30),
        observer_pool: 1,
        observer_frontend_config: None,
        candidate_queue_capacity: None,
        seed: None,
        mocks,
        mock_params,
        setup_file: None,
        setup_level: SetupLevel::Function,
        value_sources: vec![],
        capabilities: FrontendCapabilities::default(),
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
    };

    let result =
        shatter_core::explorer::explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("explore_function failed");

    let return_values = explorer_return_value_set(&result);

    eprintln!(
        "  [str-3ky9.8/result] unique_paths: {}",
        result.unique_paths
    );
    eprintln!("  [str-3ky9.8/result] return_values: {return_values:?}");

    // Should discover at least 2 of 3 branches from varied mock boolean/string values.
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths from mock-gated Ok/Err branches; got {}. \
         return_values: {return_values:?}",
        result.unique_paths
    );

    // Verify at least one mock-dependent branch is discovered.
    let has_mock_branch = return_values
        .iter()
        .any(|v| v.contains("missing") || v.contains("loaded") || v.contains("empty-config"));
    assert!(
        has_mock_branch,
        "should discover at least one mock-gated branch; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test that dynamic mock discovery handles loop-based mock calls
/// in classifyConfigs (per-iteration mock return values).
///
/// classifyConfigs(paths) loops over paths, calling readFileSync per element:
///   1. all files start with "#"    → "all-comments"
///   2. no files start with "#"     → "no-comments"
///   3. mixed                       → "mixed"
///
/// Dynamic mocking cycles through return_values, so varied values per call
/// produce different branch outcomes. Empty paths array → "no-comments".
/// Uses the random explorer for per-iteration mock regeneration.
#[tokio::test]
async fn concolic_mock_loop_branches_discovered() {
    let file = examples_dir().join("17-mock-branches.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "classifyConfigs").await;

    // Cycle through "#comment" and "no-comment" to trigger the mixed branch.
    let mocks = vec![readfilesync_mock(&[
        "#comment",
        "no-comment",
        "#also-comment",
    ])];
    let mock_params = vec![fs_mock_param(
        "fs:readFileSync",
        TypeInfo::Str,
        3, // called once per loop iteration
    )];

    let config = shatter_core::explorer::ExploreConfig {
        file: file_str.clone(),
        execution_profile: None,
        max_iterations: Some(30),
        observer_pool: 1,
        observer_frontend_config: None,
        candidate_queue_capacity: None,
        seed: None,
        mocks,
        mock_params,
        setup_file: None,
        setup_level: SetupLevel::Function,
        value_sources: vec![],
        capabilities: FrontendCapabilities::default(),
        user_seeds: vec![
            vec![serde_json::json!(["a.txt", "b.txt"])],
            vec![serde_json::json!(["x.txt", "y.txt", "z.txt"])],
            vec![serde_json::json!([])],
        ],
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
    };

    let result =
        shatter_core::explorer::explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("explore_function failed");

    let return_values = explorer_return_value_set(&result);

    eprintln!("  [str-3ky9.8/loop] unique_paths: {}", result.unique_paths);
    eprintln!("  [str-3ky9.8/loop] return_values: {return_values:?}");

    // With varied mock returns per iteration, should discover at least 2 of 3 branches.
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths from loop mock cycling; got {}. \
         return_values: {return_values:?}",
        result.unique_paths
    );

    // Verify at least one comment-related branch is discovered.
    let has_comment_branch = return_values
        .iter()
        .any(|v| v.contains("comments") || v.contains("mixed"));
    assert!(
        has_comment_branch,
        "should discover at least one comment-related branch; found: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// MC/DC coverage E2E tests (str-6wmm.9)
// ---------------------------------------------------------------------------

/// Test that concolic exploration with MC/DC enabled discovers all branches
/// of compoundAnd and reports an mcdc_summary.
///
/// compoundAnd(a: number, b: number) has:
///   1. a > 0 && b < 10  → "both"
///   2. otherwise         → "neither"
///
/// With mcdc: true, the orchestrator should also find condition-independence
/// witnesses:
///   - a > 0 witness: inputs where only a > 0 flips and the decision flips
///   - b < 10 witness: inputs where only b < 10 flips and the decision flips
///
/// The exploration must discover both return values and the mcdc_summary
/// must be present (indicating MC/DC tracking was active).
#[tokio::test]
async fn mcdc_compound_and_discovers_all_branches_and_reports_summary() {
    let file = examples_dir().join("13-mcdc-compound.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Step 1: Analyze the function to get its type signature.
    let analysis = analyze_function(&mut frontend, &file_str, "compoundAnd").await;
    assert_eq!(analysis.params.len(), 2, "compoundAnd takes 2 params");

    // Step 2: Instrument the function for branch tracking.
    instrument_function(&mut frontend, &file_str, "compoundAnd").await;

    // Step 3: Run concolic exploration with MC/DC enabled.
    // Use moderate budgets — compoundAnd has only 2 conditions, so convergence
    // should be fast.
    let config = ExploreConfig {
        max_iterations: Some(30),
        max_executions: Some(150),
        plateau_threshold: 20,
        mcdc: true,
        ..Default::default()
    };

    // Seed with values that trigger each branch directly.
    let seed_inputs = vec![
        vec![serde_json::json!(1), serde_json::json!(5)], // a > 0, b < 10 -> "both"
        vec![serde_json::json!(-1), serde_json::json!(5)], // a <= 0 -> "neither"
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "compoundAnd",
        seed_inputs,
        vec![], // no user-provided inputs
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

    eprintln!(
        "  [str-6wmm.9/compoundAnd] unique_paths: {}",
        result.unique_paths
    );
    eprintln!(
        "  [str-6wmm.9/compoundAnd] z3_generated: {}",
        result.z3_generated
    );
    eprintln!(
        "  [str-6wmm.9/compoundAnd] mcdc_summary: {:?}",
        result.mcdc_summary
    );

    // Step 4: Verify both branches are discovered.
    let return_values = return_value_set(&result);

    assert!(
        return_values.contains("\"both\""),
        "should discover 'both' branch (a > 0 && b < 10); found: {return_values:?}"
    );
    assert!(
        return_values.contains("\"neither\""),
        "should discover 'neither' branch; found: {return_values:?}"
    );

    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    // Step 5: Verify MC/DC summary is present when mcdc: true.
    // The summary is (total_conditions, independent_conditions, opaque_conditions).
    // We don't assert specific counts here since MC/DC implementation may be
    // partially complete, but the field must be populated when mcdc is enabled.
    assert!(
        result.mcdc_summary.is_some(),
        "mcdc_summary must be present when ExploreConfig::mcdc is true; got None"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// Test that concolic exploration with MC/DC enabled on compoundOr discovers
/// both branches.
///
/// compoundOr(x: boolean, y: boolean) has:
///   1. x || y  → "either"
///   2. !x && !y → "none"
#[tokio::test]
async fn mcdc_compound_or_discovers_all_branches() {
    let file = examples_dir().join("13-mcdc-compound.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    let analysis = analyze_function(&mut frontend, &file_str, "compoundOr").await;
    assert_eq!(analysis.params.len(), 2, "compoundOr takes 2 params");

    instrument_function(&mut frontend, &file_str, "compoundOr").await;

    let config = ExploreConfig {
        max_iterations: Some(20),
        max_executions: Some(100),
        plateau_threshold: 15,
        mcdc: true,
        ..Default::default()
    };

    // Seed with one case — Z3 should find the other.
    let seed_inputs = vec![
        vec![serde_json::json!(false), serde_json::json!(false)], // "none"
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "compoundOr",
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

    eprintln!(
        "  [str-6wmm.9/compoundOr] unique_paths: {}",
        result.unique_paths
    );
    eprintln!(
        "  [str-6wmm.9/compoundOr] mcdc_summary: {:?}",
        result.mcdc_summary
    );

    let return_values = return_value_set(&result);

    assert!(
        return_values.contains("\"none\""),
        "should discover 'none' branch; found: {return_values:?}"
    );

    // Z3 should be able to find the 'either' branch from the seed.
    assert!(
        result.unique_paths >= 2,
        "should have at least 2 unique paths; got {}",
        result.unique_paths
    );

    // MC/DC summary must be present.
    assert!(
        result.mcdc_summary.is_some(),
        "mcdc_summary must be present when ExploreConfig::mcdc is true; got None"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Go method-receiver planner-driven E2E (str-hy9b.H5)
// ---------------------------------------------------------------------------

/// Build the Go frontend on demand (mirroring shatter-cli/build.rs but
/// inside the integration test) and return the binary path. Caches the
/// build into a per-process tmpdir keyed on a stable name so repeat
/// invocations within one `cargo test` invocation reuse the binary.
fn ensure_go_frontend_binary() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let go_dir = manifest_dir.join("..").join("shatter-go");
    assert!(
        go_dir.join("main.go").exists(),
        "shatter-go/main.go not found at {} — repo layout drift?",
        go_dir.display()
    );
    let tmpdir = env::temp_dir().join("shatter-h5-go-frontend");
    std::fs::create_dir_all(&tmpdir).expect("create tmpdir for go binary");
    let binary_path = tmpdir.join("shatter-go");

    let status = std::process::Command::new("go")
        .args(["build", "-buildvcs=false", "-o"])
        .arg(&binary_path)
        .arg(".")
        .current_dir(&go_dir)
        .status()
        .expect("failed to run `go build` — is Go installed?");
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

/// Spawn the Go frontend subprocess from the freshly-built binary.
///
/// Each invocation creates its own `tempfile::TempDir` and pins
/// `SHATTER_GO_WORKSPACE_ROOT` to it so concurrently-running e2e tests do not
/// race on the shared `<repo>/.shatter-cache/go-workspace/` tree. The returned
/// `TempDir` must outlive the `Frontend` — drop the tuple together at end of
/// test. See str-8bsv: default parallel `cargo test` previously raced two Go
/// method-planner E2Es over the same `generated/` overlay-manifest path.
///
/// `tag` should be a short, distinct, test-name-derived label (it is only
/// used as a directory prefix to make stale tempdirs easier to identify).
async fn spawn_go_frontend(tag: &str) -> (Frontend, tempfile::TempDir) {
    let binary = ensure_go_frontend_binary();
    let workspace_dir = tempfile::Builder::new()
        .prefix(&format!("shatter-go-e2e-{tag}-"))
        .tempdir()
        .expect("create per-test Go workspace tempdir");
    let mut config = FrontendConfig::new(binary);
    config.request_timeout = std::time::Duration::from_secs(60);
    config.env_vars.push((
        "SHATTER_GO_WORKSPACE_ROOT".to_string(),
        workspace_dir.path().to_string_lossy().into_owned(),
    ));
    let frontend = Frontend::spawn(&config)
        .await
        .expect("failed to spawn Go frontend binary");
    (frontend, workspace_dir)
}

/// Path to the H5 method-receiver fixture. The fixture is a minimal
/// `example.com/service-method` module with a `*Service.Compute(x int) int`
/// method and a same-package `func New() *Service` constructor. It
/// deliberately does NOT live under `internal/...` because the launcher
/// synthesizes its module outside the target's tree and Go's internal-
/// visibility rule would block the import — orthogonal to H5's AC scope.
fn h5_method_receiver_fixture() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("..")
        .join("examples")
        .join("go")
        .join("service-method")
        .join("svc.go")
}

/// str-hy9b.H5 — Planner-driven E2E for a method target with a same-package
/// constructor. Exercises analyze → `get_invocation_plan` → Execute against
/// the Go frontend, validating that:
///
///   1. The planner emits a non-trivial `InvocationPlan` (non-empty
///      `receiver_kind` AND ≥1 parameter `ValuePlan`).
///   2. Execute consumes the plan and produces an outcome of `completed`
///      or `runtime_failed` (NOT `unsupported` / `build_failed`).
///   3. Failure modes are distinguishable per AC #4: assertion text marks
///      the gap as PLANNER GAP (planner returned nothing / wrong shape),
///      EXECUTOR GAP (Execute transport-errored), or EXECUTOR GAP (outcome
///      was the wrong status).
///
/// The fixture is `examples/go/internal-method/internal/svc/svc.go` —
/// `*Service.DoIt(x int) int` with a same-package `func New() *Service`
/// constructor and a branch on `x`. Targeting a method-with-receiver
/// fixture closes the H2 gap (free-function-only planning) by exercising
/// `planner.PlanReceivers` + `Compose(IsMethod=true)` end-to-end and
/// dispatching through the wrapper's receiver-kind switch.
#[tokio::test]
async fn go_method_planner_driven_e2e() {
    use shatter_core::planner_consumer::fetch_planner_seeds;
    use shatter_core::protocol::{Command, OutcomeStatus, ResponseResult};

    let (mut frontend, _workspace_dir) = spawn_go_frontend("method-planner-driven").await;
    let fixture = h5_method_receiver_fixture();
    assert!(
        fixture.exists(),
        "fixture missing: {} (was the worktree set up correctly?)",
        fixture.display()
    );
    let file_str = fixture.to_string_lossy().into_owned();

    // Step 1 — analyze the method. The Go frontend emits one
    // FunctionAnalysis per declared func; the analyzed name is the
    // qualified-method form `(*Service).DoIt`.
    let analyze_resp = frontend
        .send(Command::Analyze {
            file: file_str.clone(),
            function: Some("Compute".into()),
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("analyze command transport failed");

    // str-fuhw.1.1: the Go analyzer now emits receiver-decorated names for
    // methods (e.g. "(*Service).Compute") so two methods sharing a bare
    // name in the same file produce distinct FunctionAnalysis entries.
    // Free functions still surface as the bare name. Match by bare suffix
    // so the test stays receiver-agnostic and fails only on a real frontend
    // gap.
    let analysis = match analyze_resp.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == "Compute" || f.name.ends_with(".Compute"))
            .expect("FRONTEND GAP: analyze did not return a `Compute` entry"),
        other => panic!("FRONTEND GAP: expected Analyze response, got {other:?}"),
    };
    assert_eq!(
        analysis.params.len(),
        1,
        "FRONTEND GAP: (*Service).Compute should have 1 param, got {}",
        analysis.params.len()
    );

    // Step 2 — consult the planner. The Go planner resolves the cached
    // analysis, scans same-package constructors (finding `New`), and
    // composes receiver plans with parameter plans. AC #4 distinguishability
    // is anchored in this assertion's message.
    // Use the colon-prefix form the existing CLI planner-seeding path uses
    // (see shatter-cli/src/commands/explore.rs::fetch_planner_extra_seeds);
    // the Go handler's `bareSymbolFromTargetID` strips everything before
    // the final colon, so any package prefix (or empty) followed by the
    // analyzed bare name resolves the cached analysis.
    let target_id = format!(":{}", analysis.name);
    let bundle = fetch_planner_seeds(&mut frontend, &target_id, &analysis.params)
        .await
        .expect("PLANNER GAP: get_invocation_plan transport failed");

    assert!(
        !bundle.plans.is_empty(),
        "PLANNER GAP: planner returned 0 plans for method target {target_id}; \
         unsatisfied={:?}. Receiver-aware planning (str-hy9b.H5) should emit \
         ≥1 plan when a same-package constructor is in scope.",
        bundle.unsatisfied
    );

    // AC #2: at least one plan must have a non-empty receiver_kind AND ≥1
    // parameter ValuePlan. Method plans should always populate both — the
    // `len == argument_plans` invariant is the property test in plan_test.go.
    let plan = bundle
        .plans
        .iter()
        .find(|p| !p.receiver_kind.is_empty() && !p.argument_plans.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "PLANNER GAP: no plan satisfies AC #2 (non-empty receiver_kind \
                 AND ≥1 parameter ValuePlan). Got plans: {:#?}",
                bundle.plans
            )
        });
    assert!(
        plan.receiver_kind.starts_with("constructor:") || plan.receiver_kind == "zero_value",
        "PLANNER GAP: plan.receiver_kind = {:?}, expected `constructor:<Name>` \
         or `zero_value` (the strategies PlanReceivers emits for a same-package \
         non-interface receiver).",
        plan.receiver_kind
    );

    // Step 3 — execute the chosen plan. The Go frontend threads
    // plan.receiver_kind through `InvokeWithReceiverKind` so the wrapper's
    // switch dispatches against the right constructor and the method runs
    // against a constructed receiver.
    let exec_resp = frontend
        .send(Command::Execute {
            function: analysis.name.clone(),
            inputs: vec![serde_json::json!(7)],
            mocks: vec![],
            setup_context: None,
            capture: true,
            prepare_id: None,
            execution_profile: None,
            plan: Some(plan.clone()),
        })
        .await
        .expect("EXECUTOR GAP (transport): Execute command transport failed");

    let exec = match exec_resp.result {
        ResponseResult::Execute(er) => *er,
        ResponseResult::Error {
            code,
            message,
            details,
        } => panic!(
            "EXECUTOR GAP (transport): Execute returned protocol error \
             code={code:?} message={message} details={details:?}"
        ),
        other => panic!("EXECUTOR GAP (shape): expected Execute response, got {other:?}"),
    };

    let outcome = exec
        .outcome
        .expect("EXECUTOR GAP: Go frontend should always emit `outcome` on Execute responses");
    assert!(
        matches!(
            outcome.status,
            OutcomeStatus::Completed | OutcomeStatus::RuntimeFailed
        ),
        "EXECUTOR GAP: outcome.status = {:?} for plan {{receiver_kind={:?}, label={:?}}}, \
         expected `completed` or `runtime_failed` (NOT `unsupported`/`build_failed`). \
         short_reason = {:?}, thrown_error = {:?}.",
        outcome.status,
        plan.receiver_kind,
        plan.label,
        outcome.short_reason,
        outcome.thrown_error,
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

/// str-yi9y AC3 — Orchestrator path threads `default_execute_plan` into Execute.
///
/// Validates that `orchestrator::ExploreConfig.default_execute_plan` propagates
/// to every `Command::Execute` issued by the concolic orchestrator, so method
/// targets dispatch into a real constructor rather than falling through to
/// "unknown receiver kind".
///
/// Uses the same `service-method` fixture as `go_method_planner_driven_e2e`:
/// `(*Service).Compute(x int) int` with a `New() *Service` constructor.
/// The test obtains a plan via `fetch_planner_seeds`, sets it as the
/// `default_execute_plan`, runs the concolic orchestrator for a small budget,
/// and asserts that at least one execution reached `completed` (i.e., the plan
/// was threaded into Execute and the Go launcher dispatched via the constructor).
#[tokio::test]
async fn go_method_planner_driven_via_orchestrator() {
    use shatter_core::orchestrator::{ExploreConfig, explore};
    use shatter_core::planner_consumer::fetch_planner_seeds;
    use shatter_core::protocol::{Command, OutcomeStatus, ResponseResult};

    let (mut frontend, _workspace_dir) = spawn_go_frontend("method-planner-via-orchestrator").await;
    let fixture = h5_method_receiver_fixture();
    assert!(
        fixture.exists(),
        "fixture missing: {} (was the worktree set up correctly?)",
        fixture.display()
    );
    let file_str = fixture.to_string_lossy().into_owned();

    // Analyze: the Go frontend emits a FunctionAnalysis whose name is the
    // receiver-decorated qualified form for methods (str-fuhw.1.1), e.g.
    // "(*Service).Compute". Match by bare suffix.
    let analyze_resp = frontend
        .send(Command::Analyze {
            file: file_str.clone(),
            function: Some("Compute".into()),
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("analyze command transport failed");

    let analysis = match analyze_resp.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == "Compute" || f.name.ends_with(".Compute"))
            .expect("FRONTEND GAP: analyze did not return a `Compute` entry"),
        other => panic!("FRONTEND GAP: expected Analyze response, got {other:?}"),
    };

    // Consult the planner to get an InvocationPlan with a non-empty receiver_kind.
    let target_id = format!(":{}", analysis.name);
    let bundle = fetch_planner_seeds(&mut frontend, &target_id, &analysis.params)
        .await
        .expect("PLANNER GAP: get_invocation_plan transport failed");

    let plan = bundle
        .plans
        .into_iter()
        .find(|p| !p.receiver_kind.is_empty())
        .expect(
            "PLANNER GAP: no plan with non-empty receiver_kind; receiver-aware planning \
             (str-hy9b.H5) must emit ≥1 plan when a same-package constructor is in scope.",
        );

    // Build orchestrator config with the plan as the default Execute plan.
    let config = ExploreConfig {
        max_iterations: Some(5),
        max_executions: Some(20),
        plateau_threshold: 10,
        default_execute_plan: Some(plan),
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(0)]];

    let (result, _) = explore(
        &mut frontend,
        "Compute",
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
    .expect("ORCHESTRATOR GAP: orchestrator::explore returned an error");

    // At least one execution must have produced `completed`; if all executions
    // returned `runtime_failed: "unknown receiver kind"`, the plan was not
    // threaded into Execute.
    let completed_count = result
        .executions
        .iter()
        .filter(|e| {
            e.outcome
                .as_ref()
                .map(|o| o.status == OutcomeStatus::Completed)
                .unwrap_or(false)
        })
        .count();

    assert!(
        completed_count > 0,
        "ORCHESTRATOR GAP: no execution reached `completed`; \
         default_execute_plan was not propagated into Command::Execute. \
         total_executions={}, outcomes={:?}",
        result.executions.len(),
        result
            .executions
            .iter()
            .map(|e| e
                .outcome
                .as_ref()
                .map(|o| (&o.status, o.short_reason.as_deref())))
            .collect::<Vec<_>>(),
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// Genetic algorithm integration tests
// ---------------------------------------------------------------------------

/// Test that the genetic algorithm pipeline runs end-to-end on a function with
/// an opaque predicate.
///
/// `classifyWithChecksum(s)` has 4 branches including one guarded by a checksum
/// comparison. This test:
/// 1. Runs concolic exploration to collect seed inputs and discover branches.
/// 2. Extracts unsolved targets; if concolic solves everything, constructs
///    synthetic targets from the analysis to ensure the GA has work to do.
/// 3. Runs genetic exploration and asserts it produced a valid `GeneticResult`.
#[tokio::test]
async fn genetic_opaque_predicate_runs_and_produces_result() {
    let file = examples_dir().join("22-opaque-predicate.ts");
    let file_str = file.to_string_lossy().to_string();

    let mut frontend = spawn_ts_frontend().await;

    // Step 1: Analyze
    let analysis = analyze_function(&mut frontend, &file_str, "classifyWithChecksum").await;
    assert!(
        !analysis.params.is_empty(),
        "classifyWithChecksum should have at least 1 param"
    );
    assert!(
        analysis.branches.len() >= 3,
        "classifyWithChecksum should have at least 3 branches; got {}",
        analysis.branches.len()
    );

    // Step 2: Instrument
    instrument_function(&mut frontend, &file_str, "classifyWithChecksum").await;

    // Step 3: Concolic exploration (collect seed inputs for GA population)
    let config = ExploreConfig {
        max_iterations: Some(10),
        max_executions: Some(30),
        plateau_threshold: 8,
        ..Default::default()
    };

    let seed_inputs = vec![
        vec![serde_json::json!("hello")],
        vec![serde_json::json!("")],
    ];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "classifyWithChecksum",
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
    eprintln!("concolic return values: {return_values:?}");
    eprintln!("concolic unique_paths: {}", result.unique_paths);

    // Step 4: Extract targets from concolic results. If concolic happened to
    // solve everything (the hash is polynomial arithmetic, which Z3 can
    // sometimes handle), construct synthetic targets from the analysis so the
    // GA pipeline is still exercised.
    let mut targets = extract_targets_concolic(&analysis, &result);
    if targets.is_empty() {
        eprintln!("concolic solved all branches — using synthetic targets for GA exercise");
        targets = analysis
            .branches
            .iter()
            .take(1)
            .map(|b| TargetBranch {
                branch_id: b.id,
                line: b.line,
                reason: TargetReason::OpaqueConstraint,
                constraint_hint: Some("synthetic target for GA integration test".to_string()),
            })
            .collect();
    }
    eprintln!("GA targets: {targets:?}");

    // Collect seed inputs from concolic results for the GA population
    let ga_seed_inputs: Vec<Vec<serde_json::Value>> = result
        .raw_results
        .iter()
        .map(|(inputs, _mocks, _exec)| inputs.clone())
        .collect();

    // Step 5: Run genetic exploration
    let ga_config = GeneticConfig {
        enabled: true,
        population_size: 20,
        max_generations: 10,
        mutation_rate: 0.3,
        crossover_rate: 0.7,
        timeout_secs: 15,
    };

    let ga_result = genetic_explorer::genetic_explore(
        &mut frontend,
        "classifyWithChecksum",
        ga_seed_inputs,
        targets,
        &analysis.params,
        &[],
        &ga_config,
    )
    .await
    .expect("genetic exploration failed");

    // Step 6: Assert the GA ran and produced valid output
    eprintln!(
        "GA result: generations={}, executions={}, targets_solved={}, discoveries={}",
        ga_result.generations_run,
        ga_result.total_executions,
        ga_result.targets_solved,
        ga_result.discoveries.len()
    );

    assert!(
        ga_result.generations_run > 0,
        "GA should have run at least 1 generation"
    );
    assert!(
        ga_result.total_executions > 0,
        "GA should have executed at least 1 candidate"
    );

    // If the GA found a target branch, verify the discovery is well-formed
    if ga_result.targets_solved > 0 {
        assert!(
            !ga_result.discoveries.is_empty(),
            "targets_solved > 0 but discoveries is empty"
        );
        for discovery in &ga_result.discoveries {
            assert!(
                !discovery.input_args.is_empty(),
                "discovery should have input arguments"
            );
            eprintln!(
                "GA discovery: inputs={:?}, return={:?}",
                discovery.input_args, discovery.return_value
            );
        }
    }

    frontend.shutdown().await.expect("frontend shutdown failed");
}

// ---------------------------------------------------------------------------
// str-jeen.28: TS path alias resolution via the tsconfig-paths adapter.
//
// The TS frontend already implements `ts/module-resolution/tsconfig-paths` as
// a runtime hook factory that rewrites bare aliases through
// `ts.resolveModuleName`. The resolver is gated on the request's
// `execution_profile`: every prior e2e_concolic case here passed
// `execution_profile: None`, so the alias path was never proven end-to-end.
// This case closes that gap with a fixture using the literal `@/` alias.
// ---------------------------------------------------------------------------

/// Adapter id for the TS tsconfig-paths runtime hook (mirrors
/// `ADAPTER_ID_TSCONFIG_PATHS` in shatter-ts/src/runtime-hints.ts).
const TSCONFIG_PATHS_ADAPTER_ID: &str = "ts/module-resolution/tsconfig-paths";

/// Build an ExecutionProfile that requires the tsconfig-paths adapter. Used by
/// future profile-driven E2E cases (str-jeen.29 JSX, .30 browser globals) by
/// extending `adapters` with the additional adapter id.
fn tsconfig_paths_profile() -> ExecutionProfile {
    ExecutionProfile {
        adapters: vec![ExecutionAdapter {
            id: TSCONFIG_PATHS_ADAPTER_ID.to_string(),
            apply: Some(ExecutionAdapterApply::Required),
            options: None,
        }],
    }
}

/// Path to the TS workspace root, used to resolve in-tree fixtures from the
/// shatter-core tests crate.
fn ts_workspace_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../shatter-ts")
}

/// Analyze a function with an explicit ExecutionProfile and project_root.
///
/// Wrapper around the bare-bones `analyze_function` so existing tests that pass
/// `execution_profile: None` are untouched. Future adapter-aware E2E cases
/// (str-jeen.29 / .30) can reuse this same shape.
async fn analyze_function_with_profile(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
    project_root: Option<&str>,
    execution_profile: Option<ExecutionProfile>,
) -> shatter_core::protocol::FunctionAnalysis {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: file.to_string(),
            function: Some(function_name.to_string()),
            project_root: project_root.map(|s| s.to_string()),
            execution_profile,
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

/// Instrument a function with an explicit ExecutionProfile and project_root.
///
/// See `analyze_function_with_profile` rationale.
async fn instrument_function_with_profile(
    frontend: &mut Frontend,
    file: &str,
    function_name: &str,
    project_root: Option<&str>,
    execution_profile: Option<ExecutionProfile>,
) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function_name.to_string(),
            mocks: vec![],
            project_root: project_root.map(|s| s.to_string()),
            execution_profile,
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

/// E2E proof for str-jeen.28: a fixture importing through the literal `@/`
/// tsconfig path alias executes through the full analyze → instrument →
/// concolic-explore → Z3 pipeline without producing the
/// unresolvable-module-stub fallback that signals an unresolved alias.
///
/// `describeNumber(value: number)` in the fixture has three branch outcomes
/// driven by the value sign. The aliased helper `classifySign` is interpolated
/// into the return string; when the alias is unresolved, the stub returns ""
/// and the asserted strings ("pos:positive", "neg:negative", "pos:zero")
/// cannot appear. Asserting their presence proves the alias resolved on every
/// concrete execution that produced them.
#[tokio::test]
async fn concolic_tsconfig_at_alias_executes() {
    let fixture_dir = ts_workspace_dir().join("src/__fixtures__/adapter-tsconfig-at-alias");
    let entry_file = fixture_dir.join("src/entry.ts");
    let entry_str = entry_file.to_string_lossy().to_string();

    assert!(
        entry_file.exists(),
        "fixture entry not found: {}",
        entry_file.display()
    );

    let mut frontend = spawn_ts_frontend().await;

    let profile = tsconfig_paths_profile();

    // Intentionally omit project_root: the fixture is a checked-in sample
    // without `node_modules/` (gitignored), so passing project_root would
    // trigger the handler's missing-node-modules preflight and short-circuit
    // every command. The tsconfig-paths runtime hook falls back to walking up
    // from the entry file's directory to discover tsconfig.json
    // (`findTsconfigPath` in shatter-ts/src/runtime-hooks.ts), which is the
    // path exercised here.
    let analysis = analyze_function_with_profile(
        &mut frontend,
        &entry_str,
        "describeNumber",
        None,
        Some(profile.clone()),
    )
    .await;
    assert_eq!(analysis.params.len(), 1, "describeNumber takes 1 param");

    instrument_function_with_profile(
        &mut frontend,
        &entry_str,
        "describeNumber",
        None,
        Some(profile.clone()),
    )
    .await;

    let config = ExploreConfig {
        max_iterations: Some(15),
        max_executions: Some(60),
        plateau_threshold: 10,
        execution_profile: Some(profile),
        ..Default::default()
    };

    let seed_inputs = vec![vec![serde_json::json!(7)], vec![serde_json::json!(-3)]];

    let (result, _) = orchestrator::explore(
        &mut frontend,
        "describeNumber",
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

    // No execution should have surfaced a MODULE_NOT_FOUND for the alias —
    // that would indicate the tsconfig-paths adapter failed to resolve `@/`.
    for execution in &result.executions {
        if let Some(err) = &execution.thrown_error {
            assert!(
                !err.message.contains("@/lib/sign"),
                "tsconfig-paths adapter failed to resolve `@/lib/sign`; \
                 thrown_error: {} (full set: {return_values:?})",
                err.message
            );
        }
    }

    // The aliased helper must have been the real `classifySign`, not the
    // unresolvable-module stub. The stub returns "" for primitive coercion,
    // which would yield "pos:" / "neg:" return strings; asserting the suffixed
    // strings appear proves the import resolved at execute-time.
    assert!(
        return_values.contains("\"pos:positive\"")
            || return_values.contains("\"neg:negative\"")
            || return_values.contains("\"pos:zero\""),
        "expected at least one alias-resolved return value among the executions; \
         got: {return_values:?}"
    );

    frontend.shutdown().await.expect("frontend shutdown failed");
}
