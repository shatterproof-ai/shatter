//! Benchmark runner: executes manifest targets with repeated exploration
//! and emits a structured JSON timing bundle.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use shatter_core::bench::{
    self, BenchConfig, BenchTarget, BenchmarkBundle, RunMeasurement, RunStatistics, ScenarioResult,
};
use shatter_core::explorer::{self, ExploreConfig, LoopBuckets};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::timing::TimingHandle;
use tracing::Instrument;

use crate::args::Language;
use crate::helpers::{apply_project_storage, frontend_config, resolve_project_root};

/// Run the benchmark harness.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_bench(
    manifest_path: &Path,
    tier: &str,
    repeats: u32,
    warmups: u32,
    max_iterations: u32,
    output: Option<&Path>,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    log_level: LogLevel,
    project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = bench::load_manifest(manifest_path)
        .map_err(|e| format!("failed to load manifest {}: {e}", manifest_path.display()))?;

    let targets = bench::resolve_targets(&manifest, tier)
        .map_err(|e| format!("failed to resolve targets for tier {tier:?}: {e}"))?;

    let config = BenchConfig {
        tier: tier.to_string(),
        repeats,
        warmups,
        max_iterations,
        request_timeout_secs: request_timeout,
        exec_timeout_secs: exec_timeout,
        build_timeout_secs: build_timeout,
    };

    eprintln!(
        "Benchmark: tier={}, targets={}, warmups={}, repeats={}, max_iterations={}",
        config.tier,
        targets.len(),
        config.warmups,
        config.repeats,
        config.max_iterations,
    );

    let started_at_unix_ms = shatter_core::timing::unix_timestamp_ms_now();

    // Determine project root from the first target.
    let first_file = Path::new(&targets[0].file);
    let storage_project_root = resolve_project_root(project_dir, first_file);

    // Spawn one frontend per language.
    let req_timeout = Duration::from_secs(request_timeout);
    let mut frontends: HashMap<Language, Frontend> = HashMap::new();
    let unique_langs: std::collections::HashSet<Language> = targets
        .iter()
        .filter_map(|t| language_from_manifest_key(&t.language))
        .collect();

    for lang in &unique_langs {
        let mut fc = frontend_config(
            *lang,
            req_timeout,
            log_level,
            exec_timeout,
            build_timeout,
            None,  // memory_limit
            None,  // shatter_dir
            false, // timing_enabled — we use scoped handles instead
            false, // release
        )?;
        apply_project_storage(&mut fc, storage_project_root.as_deref());
        let frontend = Frontend::spawn(&fc)
            .await
            .map_err(|e| format!("failed to spawn {} frontend: {e}", lang.label()))?;
        frontends.insert(*lang, frontend);
    }

    let total_runs_per_target = config.warmups + config.repeats;
    let mut scenarios: Vec<ScenarioResult> = Vec::new();

    for (target_idx, target) in targets.iter().enumerate() {
        let target_id = format!("{}:{}", target.file, target.function);
        eprintln!("[{}/{}] {}", target_idx + 1, targets.len(), target_id,);

        let lang = match language_from_manifest_key(&target.language) {
            Some(l) => l,
            None => {
                eprintln!("  skipping: unsupported language {:?}", target.language);
                continue;
            }
        };

        let frontend = match frontends.get_mut(&lang) {
            Some(f) => f,
            None => continue,
        };

        // Analyze the target to get function signatures.
        let project_root_str = resolve_project_root(project_dir, Path::new(&target.file));
        let analysis = match analyze_target(frontend, target, project_root_str.as_deref()).await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  analyze failed: {e}");
                scenarios.push(error_scenario(&target_id, &target.language, &e));
                continue;
            }
        };

        let mut runs: Vec<RunMeasurement> = Vec::new();
        for seq in 0..total_runs_per_target {
            let is_warmup = seq < config.warmups;
            let label = if is_warmup { "warmup" } else { "measure" };

            let measurement = run_single_exploration(
                frontend,
                &analysis,
                target,
                &config,
                project_root_str.as_deref(),
                seq,
                is_warmup,
            )
            .await;

            let dur = measurement.duration_ms;
            let paths = measurement.unique_paths;
            eprintln!("  {label} {seq}: {dur:.0}ms, {paths} paths");
            runs.push(measurement);

            // Teardown between runs to reset frontend state.
            let _ = frontend
                .send(ProtoCommand::Teardown {
                    scope: target.function.clone(),
                    level: shatter_core::protocol::SetupLevel::Function,
                })
                .await;
        }

        let stats = compute_run_statistics(&runs);
        scenarios.push(ScenarioResult {
            target: target_id,
            language: target.language.clone(),
            runs,
            stats,
        });
    }

    let finished_at_unix_ms = shatter_core::timing::unix_timestamp_ms_now();

    let bundle = BenchmarkBundle {
        schema_version: 1,
        bundle_id: uuid::Uuid::new_v4().to_string(),
        started_at_unix_ms,
        finished_at_unix_ms,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        tier: tier.to_string(),
        repeats,
        warmups,
        max_iterations,
        git_commit: bench::detect_git_commit(),
        scenarios,
    };

    let json = serde_json::to_string_pretty(&bundle)?;

    if let Some(out_path) = output {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out_path, &json)?;
        eprintln!("Wrote benchmark bundle to {}", out_path.display());
    } else {
        println!("{json}");
    }

    // Print summary to stderr.
    print_summary(&bundle);

    // Shutdown all frontends.
    for (_, mut frontend) in frontends {
        let _ = frontend.send(ProtoCommand::Shutdown).await;
    }

    Ok(())
}

/// Analyze a target and return the first matching FunctionAnalysis.
async fn analyze_target(
    frontend: &mut Frontend,
    target: &BenchTarget,
    project_root: Option<&str>,
) -> Result<shatter_core::protocol::FunctionAnalysis, String> {
    let response = frontend
        .send(ProtoCommand::Analyze {
            file: target.file.clone(),
            function: Some(target.function.clone()),
            project_root: project_root.map(String::from),
            execution_profile: None,
        })
        .await
        .map_err(|e| format!("{e}"))?;

    match response.result {
        ResponseResult::Analyze { functions } => functions
            .into_iter()
            .find(|f| f.name == target.function)
            .ok_or_else(|| {
                format!(
                    "function {:?} not found in analyze response",
                    target.function
                )
            }),
        ResponseResult::Error { code, message, .. } => {
            Err(format!("analyze error ({code:?}): {message}"))
        }
        other => Err(format!("unexpected analyze response: {other:?}")),
    }
}

/// Run a single exploration and capture timing.
async fn run_single_exploration(
    frontend: &mut Frontend,
    analysis: &shatter_core::protocol::FunctionAnalysis,
    target: &BenchTarget,
    config: &BenchConfig,
    project_root: Option<&str>,
    sequence: u32,
    is_warmup: bool,
) -> RunMeasurement {
    let explore_config = ExploreConfig {
        file: target.file.clone(),
        execution_profile: None,
        max_iterations: Some(config.max_iterations),
        seed: None,
        mocks: vec![],
        mock_params: vec![],
        setup_file: None,
        setup_level: shatter_core::protocol::SetupLevel::Function,
        value_sources: shatter_core::input_gen::resolve_value_sources(
            &analysis.params,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        ),
        capabilities: shatter_core::orchestrator::FrontendCapabilities::from_raw(
            frontend.capabilities(),
        ),
        user_seeds: vec![],
        candidate_inputs: vec![],
        pool_seeds: vec![],
        project_root: project_root.map(String::from),
        loop_buckets: LoopBuckets::default(),
        timeout_explore: None,
        meta_config: shatter_core::strategy::MetaConfig::default(),
        shrink_budget: 0, // No shrinking during benchmarks.
        isolation: explorer::IsolationMode::None,
        capture_side_effects: false,
        budget_surplus: None,
        claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
        planner: None,
    };

    // Use a scoped timing handle for per-run phase capture.
    let timing_handle = TimingHandle::default();
    let dispatch = timing_handle.dispatch();

    let wall_start = Instant::now();
    let result = tracing::dispatcher::with_default(&dispatch, || async {
        explorer::explore_function(frontend, analysis, &explore_config, None, None)
            .instrument(tracing::info_span!("bench.explore"))
            .await
    })
    .await;
    let duration_ms = wall_start.elapsed().as_secs_f64() * 1000.0;

    let phases = timing_handle.snapshot();

    match result {
        Ok(output) => RunMeasurement {
            sequence,
            is_warmup,
            duration_ms,
            iterations: output.iterations,
            unique_paths: output.unique_paths,
            exit_ok: true,
            error: None,
            phases,
        },
        Err(e) => RunMeasurement {
            sequence,
            is_warmup,
            duration_ms,
            iterations: 0,
            unique_paths: 0,
            exit_ok: false,
            error: Some(e.to_string()),
            phases,
        },
    }
}

/// Compute aggregate statistics from the measured (non-warmup) runs.
fn compute_run_statistics(runs: &[RunMeasurement]) -> RunStatistics {
    let measured: Vec<&RunMeasurement> = runs.iter().filter(|r| !r.is_warmup).collect();
    if measured.is_empty() {
        return RunStatistics {
            measured_count: 0,
            duration_ms: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
            iterations: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
            unique_paths: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
        };
    }

    let durations: Vec<f64> = measured.iter().map(|r| r.duration_ms).collect();
    let iterations: Vec<f64> = measured.iter().map(|r| r.iterations as f64).collect();
    let paths: Vec<f64> = measured.iter().map(|r| r.unique_paths as f64).collect();

    RunStatistics {
        measured_count: measured.len() as u32,
        duration_ms: bench::compute_statistics(&durations),
        iterations: bench::compute_statistics(&iterations),
        unique_paths: bench::compute_statistics(&paths),
    }
}

/// Build a scenario result for a target that failed during analysis.
fn error_scenario(target_id: &str, language: &str, error: &str) -> ScenarioResult {
    ScenarioResult {
        target: target_id.to_string(),
        language: language.to_string(),
        runs: vec![RunMeasurement {
            sequence: 0,
            is_warmup: false,
            duration_ms: 0.0,
            iterations: 0,
            unique_paths: 0,
            exit_ok: false,
            error: Some(error.to_string()),
            phases: vec![],
        }],
        stats: RunStatistics {
            measured_count: 0,
            duration_ms: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
            iterations: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
            unique_paths: bench::StatSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
            },
        },
    }
}

/// Print a human-readable summary to stderr.
fn print_summary(bundle: &BenchmarkBundle) {
    eprintln!("\n--- Benchmark Summary ---");
    eprintln!(
        "Tier: {}, Targets: {}, Warmups: {}, Repeats: {}",
        bundle.tier,
        bundle.scenarios.len(),
        bundle.warmups,
        bundle.repeats,
    );
    let total_ms = bundle
        .finished_at_unix_ms
        .saturating_sub(bundle.started_at_unix_ms);
    eprintln!("Total wall time: {:.1}s", total_ms as f64 / 1000.0);
    eprintln!();
    for scenario in &bundle.scenarios {
        let median = scenario.stats.duration_ms.median;
        let paths = scenario.stats.unique_paths.median;
        eprintln!(
            "  {:<60} median={:.0}ms  paths={:.0}",
            scenario.target, median, paths,
        );
    }
}

/// Map manifest language keys to CLI Language enum.
fn language_from_manifest_key(key: &str) -> Option<Language> {
    match key {
        "typescript" => Some(Language::TypeScript),
        "go" => Some(Language::Go),
        "rust" => Some(Language::Rust),
        _ => None,
    }
}
