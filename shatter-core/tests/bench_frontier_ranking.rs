//! Frontier-ranking benchmark runner (str-hjrnp.3). Ignored by default:
//!
//! ```text
//! BENCH_MODE=reference cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture
//! BENCH_ARMS=heuristic,random,cheating BENCH_OUT=target/bench-frontier \
//!   cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture
//! ```
//!
//! Env: `BENCH_MODE=reference|run` (default run), `BENCH_OUT` (default
//! `target/bench-frontier`), `BENCH_ARMS` and `BENCH_FIXTURES` (comma lists,
//! default all), `JEV_REPLAY_DIR` (default
//! `benchmarks/frontier-ranking/jev-replay`), `TYPESAFE_API_KEY`,
//! `ANTHROPIC_API_KEY`, `SHATTER_EXAMPLES_DIR`.
//!
//! Arms: heuristic (baseline), random (floor), cheating (ScriptedRanker
//! primed with the reference run's branch ids; ceiling), generative
//! (heuristic + existing anthropic seed oracle), jev (DecisionFrontierRanker
//! over a replaying Jev adapter).

use std::collections::{HashMap, HashSet};
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shatter_core::config::LlmConfig;
use shatter_core::frontend::{DEFAULT_REQUEST_TIMEOUT, Frontend, FrontendConfig};
use shatter_core::frontier::{FrontierRanker, HeuristicRanker, RandomRanker, ScriptedRanker};
use shatter_core::oracle::{OracleSlotMap, SeedOracle};
use shatter_core::orchestrator::{self, ExploreConfig, ExploreResult, OracleHandle};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_llm::jev::JevConfig;
use shatter_llm::{DecisionFrontierRanker, JevAdapter, MockSeedOracle, ReplayDecisionOracle};

#[derive(Deserialize)]
struct Manifest {
    seeds: Vec<u64>,
    /// Seeds used by the reference run; defaults to `seeds`.
    #[serde(default)]
    reference_seeds: Vec<u64>,
    regimes: HashMap<String, Regime>,
    #[serde(default = "default_reference_executions")]
    reference_max_executions: usize,
    fixtures: Vec<Fixture>,
}

fn default_reference_executions() -> usize {
    200
}

#[derive(Deserialize, Clone)]
struct Regime {
    /// Execution budget (`ExploreConfig::max_executions`). `None` means
    /// the wall-clock budget governs.
    max_executions: Option<usize>,
    timeout_explore_secs: Option<u64>,
}

#[derive(Deserialize, Clone)]
struct Fixture {
    id: String,
    file: String,
    function: String,
    stratum: String,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    expected_return_values: Vec<String>,
}

/// A branch side packed as `branch_id * 2 + taken`, so it survives JSON as
/// one integer and unpacks with `/ 2` and `% 2`.
fn side_id(branch_id: u32, taken: bool) -> u32 {
    branch_id * 2 + u32::from(taken)
}

/// First execution index (1-based, in observation order) at which each
/// branch *side* was seen. Branch-id granularity (`discoveries`) is too
/// coarse for a ranking benchmark: a seed input observes both ids of a
/// nested `if` at execution 1 while leaving half the sides unexplored.
fn side_discoveries(result: &ExploreResult) -> Vec<(u32, usize)> {
    let mut first: Vec<(u32, usize)> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for (idx, (_, _, exec)) in result.raw_results.iter().enumerate() {
        for d in &exec.branch_path {
            let key = side_id(d.branch_id, d.taken);
            if seen.insert(key) {
                first.push((key, idx + 1));
            }
        }
    }
    first
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Reference {
    /// Branch ids the heuristic reference discovered (union over seeds);
    /// feeds the cheating arm's `ScriptedRanker`.
    branch_ids: Vec<u32>,
    /// Branch sides (see `side_id`) the reference observed; the coverage
    /// target for executions-to-cover.
    #[serde(default)]
    side_ids: Vec<u32>,
    return_values: Vec<String>,
    /// Solve-stage hints recorded to sanity-check each fixture's stratum.
    #[serde(default)]
    abandoned_frontiers: usize,
    #[serde(default)]
    max_executions_seen: usize,
}

#[derive(Serialize)]
struct Row<'a> {
    fixture: &'a str,
    stratum: &'a str,
    seed: u64,
    arm: &'a str,
    regime: &'a str,
    budget: u64,
    total_executions: usize,
    /// Executions that produced an observation (`raw_results.len()`).
    /// `discoveries` indices count these; `total_executions` also counts
    /// frontend-skipped and unsupported iterations.
    observed_executions: usize,
    wall_ms: u128,
    /// `(side_id, observation_index)` first-seen pairs; see `side_id`.
    discoveries: Vec<(u32, usize)>,
    /// `(branch_id, total_executions)` from the orchestrator's own telemetry.
    branch_discoveries: Vec<(u32, usize)>,
    expected_return_values_hit: usize,
    expected_return_values_total: usize,
    methods: HashMap<String, usize>,
    rank_log: Vec<(usize, usize, u32, f64)>,
    decision_tokens: u32,
    oracle_tokens: u32,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn manifest_path() -> PathBuf {
    repo_root().join("benchmarks/frontier-ranking/manifest.json")
}

fn reference_path() -> PathBuf {
    repo_root().join("benchmarks/frontier-ranking/reference.json")
}

fn examples_dir() -> PathBuf {
    if let Some(p) = env::var_os("SHATTER_EXAMPLES_DIR") {
        return PathBuf::from(p).join("standalone/ts");
    }
    let fallback = env::temp_dir().join("shatter-examples-main/standalone/ts");
    assert!(
        fallback.exists(),
        "examples checkout not found: run python3 scripts/examples_checkout.py or set SHATTER_EXAMPLES_DIR"
    );
    fallback
}

async fn spawn_ts_frontend() -> Frontend {
    let fe = repo_root().join("shatter-ts/dist/main.js");
    assert!(
        fe.exists(),
        "TypeScript frontend not built: cd shatter-ts && npm run build"
    );
    let mut config = FrontendConfig::new(PathBuf::from("node"));
    config.args = vec!["--no-warnings".into(), fe.to_string_lossy().into_owned()];
    config.request_timeout = DEFAULT_REQUEST_TIMEOUT;
    Frontend::spawn(&config)
        .await
        .expect("failed to spawn TypeScript frontend")
}

async fn analyze(
    frontend: &mut Frontend,
    file: &str,
    function: &str,
) -> shatter_core::protocol::FunctionAnalysis {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: file.to_string(),
            function: Some(function.to_string()),
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("analyze command failed");
    match response.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == function)
            .unwrap_or_else(|| panic!("function '{function}' not found in {file}")),
        other => panic!("expected Analyze response, got: {other:?}"),
    }
}

async fn instrument(frontend: &mut Frontend, file: &str, function: &str) {
    let response = frontend
        .send(ProtoCommand::Instrument {
            file: file.to_string(),
            function: function.to_string(),
            mocks: vec![],
            project_root: None,
            execution_profile: None,
        })
        .await
        .expect("instrument command failed");
    match response.result {
        ResponseResult::Instrument { instrumented, .. } => {
            assert!(instrumented, "instrumentation returned false for {function}");
        }
        other => panic!("expected Instrument response, got: {other:?}"),
    }
}

/// Canonical, repeatable form of a return value. Scalars and errors are
/// kept verbatim; objects are reduced to their sorted key set plus a
/// `reason` string when present, because object bodies echo fuzz-generated
/// input literals that no other seed reproduces.
fn canonical_return(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let reason = map
                .get("reason")
                .and_then(|r| r.as_str())
                .map(|r| format!(",reason={r}"))
                .unwrap_or_default();
            format!("{{keys={:?}{reason}}}", keys)
        }
        serde_json::Value::Array(items) => format!("[len={}]", items.len()),
        other => other.to_string(),
    }
}

fn return_values(result: &ExploreResult) -> HashSet<String> {
    result
        .executions
        .iter()
        .map(|e| match (&e.thrown_error, &e.return_value) {
            (Some(err), _) => format!("ERROR:{}", err.message),
            (None, Some(v)) => canonical_return(v),
            (None, None) => "null".to_string(),
        })
        .collect()
}

fn oracle_runtime() -> Arc<tokio::runtime::Runtime> {
    static RT: std::sync::OnceLock<Arc<tokio::runtime::Runtime>> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("oracle runtime"),
        )
    })
    .clone()
}

struct ArmSpec {
    name: &'static str,
    ranker: Arc<dyn FrontierRanker>,
    seed_oracle: Option<Arc<dyn SeedOracle>>,
    decision: Option<Arc<DecisionFrontierRanker>>,
}

fn build_arm(name: &str, seed: u64, reference: &Reference) -> Option<ArmSpec> {
    match name {
        "heuristic" => Some(ArmSpec {
            name: "heuristic",
            ranker: Arc::new(HeuristicRanker),
            seed_oracle: None,
            decision: None,
        }),
        "random" => Some(ArmSpec {
            name: "random",
            ranker: Arc::new(RandomRanker { seed }),
            seed_oracle: None,
            decision: None,
        }),
        "cheating" => Some(ArmSpec {
            name: "cheating",
            ranker: Arc::new(ScriptedRanker {
                priority: reference.branch_ids.iter().copied().collect(),
            }),
            seed_oracle: None,
            decision: None,
        }),
        "generative" => {
            if env::var("ANTHROPIC_API_KEY").map(|k| k.is_empty()).unwrap_or(true) {
                eprintln!("skip generative: ANTHROPIC_API_KEY unset");
                return None;
            }
            let cfg = LlmConfig {
                enabled: true,
                adapter: "anthropic".into(),
                ..LlmConfig::default()
            };
            let oracle = shatter_llm::build_oracle(&cfg).expect("anthropic adapter");
            Some(ArmSpec {
                name: "generative",
                ranker: Arc::new(HeuristicRanker),
                seed_oracle: Some(oracle),
                decision: None,
            })
        }
        "jev" => {
            let replay_dir = env::var("JEV_REPLAY_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| repo_root().join("benchmarks/frontier-ranking/jev-replay"));
            let live: Option<Arc<dyn shatter_core::decision::DecisionOracle>> =
                JevConfig::from_env().map(|c| {
                    Arc::new(JevAdapter::new(c).expect("jev adapter"))
                        as Arc<dyn shatter_core::decision::DecisionOracle>
                });
            if live.is_none() && !replay_dir.exists() {
                eprintln!(
                    "skip jev: TYPESAFE_API_KEY unset and no replay cache at {}",
                    replay_dir.display()
                );
                return None;
            }
            let decision = Arc::new(DecisionFrontierRanker::new(Arc::new(
                ReplayDecisionOracle::new(live, replay_dir),
            )));
            Some(ArmSpec {
                name: "jev",
                ranker: decision.clone(),
                seed_oracle: None,
                decision: Some(decision),
            })
        }
        other => panic!("unknown arm {other:?}"),
    }
}

struct RunOutcome {
    result: ExploreResult,
    wall_ms: u128,
    oracle_tokens: u32,
}

async fn run_one(fixture: &Fixture, seed: u64, regime: &Regime, arm: &ArmSpec, source: &str) -> RunOutcome {
    let file = examples_dir().join(&fixture.file);
    let file_str = file.to_string_lossy().to_string();
    let mut frontend = spawn_ts_frontend().await;
    let analysis = analyze(&mut frontend, &file_str, &fixture.function).await;
    instrument(&mut frontend, &file_str, &fixture.function).await;

    // `max_iterations` caps unique paths, not executions; the benchmark
    // budgets executions (or wall-clock), so leave the path cap unbounded.
    let config = ExploreConfig {
        max_iterations: None,
        max_executions: Some(regime.max_executions.unwrap_or(100_000)),
        plateau_threshold: 0,
        seed: Some(seed),
        timeout_explore: regime.timeout_explore_secs.map(Duration::from_secs),
        frontier_ranker: arm.ranker.clone(),
        ..Default::default()
    };

    // `function_source` reaches the ranker only through OracleHandle, so
    // arms without a seed oracle get an inert mock with a zero query budget.
    let seed_oracle: Arc<dyn SeedOracle> = arm
        .seed_oracle
        .clone()
        .unwrap_or_else(|| Arc::new(MockSeedOracle::always_empty()));
    let llm_cfg = LlmConfig {
        enabled: arm.seed_oracle.is_some(),
        max_queries_per_function: if arm.seed_oracle.is_some() { 10 } else { 0 },
        ..LlmConfig::default()
    };
    let mut slot_map = OracleSlotMap::new(seed_oracle, llm_cfg, oracle_runtime());

    let start = Instant::now();
    let (result, _) = orchestrator::explore_with_oracle(
        &mut frontend,
        &fixture.function,
        fixture.seed_inputs.clone(),
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
            function_source: source.to_string(),
        }),
    )
    .await
    .expect("exploration failed");
    RunOutcome {
        result,
        wall_ms: start.elapsed().as_millis(),
        oracle_tokens: slot_map.stats().tokens_used,
    }
}

fn read_source(fixture: &Fixture) -> String {
    std::fs::read_to_string(examples_dir().join(&fixture.file)).expect("read fixture source")
}

fn selected(list_env: &str, all: Vec<String>) -> Vec<String> {
    match env::var(list_env) {
        Ok(s) if !s.trim().is_empty() => s.split(',').map(|x| x.trim().to_string()).collect(),
        _ => all,
    }
}

#[tokio::test]
#[ignore = "benchmark; run with `cargo test -p shatter-core --test bench_frontier_ranking -- --ignored --nocapture`"]
async fn bench_frontier_ranking() {
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(manifest_path()).expect("manifest"))
            .expect("manifest parses");
    let mode = env::var("BENCH_MODE").unwrap_or_else(|_| "run".into());
    let ids = selected(
        "BENCH_FIXTURES",
        manifest.fixtures.iter().map(|f| f.id.clone()).collect(),
    );
    let fixtures: Vec<Fixture> = manifest
        .fixtures
        .iter()
        .filter(|f| ids.contains(&f.id))
        .cloned()
        .collect();
    assert!(!fixtures.is_empty(), "no fixtures selected");

    if mode == "reference" {
        let mut reference: HashMap<String, Reference> = if reference_path().exists() {
            serde_json::from_str(&std::fs::read_to_string(reference_path()).unwrap()).unwrap()
        } else {
            HashMap::new()
        };
        let arm = build_arm("heuristic", 0, &Reference::default()).unwrap();
        let regime = Regime {
            max_executions: Some(manifest.reference_max_executions),
            timeout_explore_secs: None,
        };
        for fx in &fixtures {
            let source = read_source(fx);
            let mut ids = HashSet::new();
            let mut sides = HashSet::new();
            let mut rvs = HashSet::new();
            let mut abandoned = 0usize;
            let mut max_exec = 0usize;
            let ref_seeds = if manifest.reference_seeds.is_empty() {
                &manifest.seeds
            } else {
                &manifest.reference_seeds
            };
            for seed in ref_seeds {
                let out = run_one(fx, *seed, &regime, &arm, &source).await;
                eprintln!(
                    "  reference {} seed={}: {} exec, {} ms, {} branches",
                    fx.id,
                    seed,
                    out.result.total_executions,
                    out.wall_ms,
                    out.result.discoveries.len()
                );
                ids.extend(out.result.discoveries.iter().map(|(id, _)| *id));
                sides.extend(side_discoveries(&out.result).into_iter().map(|(k, _)| k));
                rvs.extend(return_values(&out.result));
                abandoned = abandoned.max(out.result.abandoned_frontiers.len());
                max_exec = max_exec.max(out.result.total_executions);
            }
            let mut branch_ids: Vec<u32> = ids.into_iter().collect();
            branch_ids.sort_unstable();
            let mut side_ids: Vec<u32> = sides.into_iter().collect();
            side_ids.sort_unstable();
            let mut return_values: Vec<String> = rvs.into_iter().collect();
            return_values.sort();
            eprintln!(
                "{}: {} branches, {} sides, {} return values, {} abandoned frontiers, max {} executions",
                fx.id,
                branch_ids.len(),
                side_ids.len(),
                return_values.len(),
                abandoned,
                max_exec
            );
            reference.insert(
                fx.id.clone(),
                Reference {
                    branch_ids,
                    side_ids,
                    return_values,
                    abandoned_frontiers: abandoned,
                    max_executions_seen: max_exec,
                },
            );
        }
        let mut ordered: Vec<(&String, &Reference)> = reference.iter().collect();
        ordered.sort_by(|a, b| a.0.cmp(b.0));
        let ordered: serde_json::Map<String, serde_json::Value> = ordered
            .into_iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
            .collect();
        std::fs::write(
            reference_path(),
            serde_json::to_string_pretty(&serde_json::Value::Object(ordered)).unwrap() + "\n",
        )
        .unwrap();
        return;
    }

    let reference: HashMap<String, Reference> = serde_json::from_str(
        &std::fs::read_to_string(reference_path()).expect("run BENCH_MODE=reference first"),
    )
    .expect("reference parses");
    let out_dir = env::var("BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target/bench-frontier"));
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut out = std::fs::File::create(out_dir.join("rows.jsonl")).unwrap();
    let arms = selected(
        "BENCH_ARMS",
        ["heuristic", "random", "cheating", "generative", "jev"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );

    let mut regimes: Vec<(&String, &Regime)> = manifest.regimes.iter().collect();
    regimes.sort_by(|a, b| a.0.cmp(b.0));
    // Runs where a non-heuristic ranker never produced a score: the
    // orchestrator degrades to the heuristic on ranker errors (e.g. a
    // replay-only jev cache miss), which would silently turn those rows
    // into heuristic rows.
    let mut silent_ranker_runs: HashMap<&'static str, usize> = HashMap::new();

    for fx in &fixtures {
        let source = read_source(fx);
        let reference = reference.get(&fx.id).cloned().unwrap_or_else(|| {
            panic!(
                "no reference entry for {}; run BENCH_MODE=reference first",
                fx.id
            )
        });
        assert!(
            !reference.side_ids.is_empty(),
            "reference for {} has no side ids; regenerate it",
            fx.id
        );
        let expected: Vec<String> = if fx.expected_return_values.is_empty() {
            reference.return_values.clone()
        } else {
            fx.expected_return_values.clone()
        };
        for (regime_name, regime) in &regimes {
            for seed in &manifest.seeds {
                for arm_name in &arms {
                    let Some(arm) = build_arm(arm_name, *seed, &reference) else {
                        continue;
                    };
                    let outcome = run_one(fx, *seed, regime, &arm, &source).await;
                    let r = &outcome.result;
                    let rvs = return_values(r);
                    let mut methods: HashMap<String, usize> = HashMap::new();
                    for (_, m) in &r.discoveries {
                        *methods.entry(format!("{m:?}")).or_default() += 1;
                    }
                    if arm.name != "heuristic" && r.rank_log.is_empty() {
                        *silent_ranker_runs.entry(arm.name).or_default() += 1;
                        eprintln!(
                            "WARNING: {} seed={} {} {}: ranker produced no scores (ranked by heuristic)",
                            fx.id, seed, regime_name, arm.name
                        );
                    }
                    let row = Row {
                        fixture: &fx.id,
                        stratum: &fx.stratum,
                        seed: *seed,
                        arm: arm.name,
                        regime: regime_name,
                        budget: regime
                            .max_executions
                            .map(|n| n as u64)
                            .or(regime.timeout_explore_secs)
                            .unwrap_or(0),
                        total_executions: r.total_executions,
                        observed_executions: r.raw_results.len(),
                        wall_ms: outcome.wall_ms,
                        discoveries: side_discoveries(r),
                        branch_discoveries: r.discovery_iterations.clone(),
                        expected_return_values_hit: expected.iter().filter(|v| rvs.contains(*v)).count(),
                        expected_return_values_total: expected.len(),
                        methods,
                        rank_log: r
                            .rank_log
                            .iter()
                            .map(|d| (d.round, d.executions, d.branch_id, d.score))
                            .collect(),
                        decision_tokens: arm.decision.as_ref().map(|d| d.tokens_used()).unwrap_or(0),
                        oracle_tokens: outcome.oracle_tokens,
                    };
                    writeln!(out, "{}", serde_json::to_string(&row).unwrap()).unwrap();
                    let sides_hit = row
                        .discoveries
                        .iter()
                        .filter(|(k, _)| reference.side_ids.contains(k))
                        .count();
                    eprintln!(
                        "{} seed={} {} {}: {} exec, {} ms, {}/{} expected, {}/{} sides",
                        fx.id,
                        seed,
                        regime_name,
                        arm.name,
                        r.total_executions,
                        outcome.wall_ms,
                        row.expected_return_values_hit,
                        row.expected_return_values_total,
                        sides_hit,
                        reference.side_ids.len()
                    );
                }
            }
        }
    }
    if let Some(n) = silent_ranker_runs.get("jev") {
        panic!(
            "jev arm produced no scores in {n} run(s): a replay-only cache miss or API failure; \
             set TYPESAFE_API_KEY or regenerate the replay cache"
        );
    }
    for (arm, n) in &silent_ranker_runs {
        eprintln!("NOTE: {arm} arm had {n} run(s) with an empty rank_log");
    }
}
