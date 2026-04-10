use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use serde::Serialize;
use shatter_core::adapter_selection;
use shatter_core::behavior::BehaviorMap;
use shatter_core::cache::BehaviorMapCache;
use shatter_core::config::{self as shatter_config, GeneticConfig, ShatterConfig};
use shatter_core::executability;
use shatter_core::explorer::{self, ExploreConfig, GeneticStats, ReportOptions};
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::report::ProgressEvent;
use shatter_core::scope::{ScopeConfig, ScopeMatcher};
use shatter_core::spec::FileSpecBundle;
use tracing::Instrument;

use crate::args::*;
use crate::helpers::*;

/// Result of exploring a single function, collected after parallel execution.
struct FuncExploreOutcome {
    work_index: usize,
    func: shatter_core::protocol::FunctionAnalysis,
    mock_symbols: Vec<String>,
    result: Result<shatter_core::explorer::ObservationOutput, String>,
    wall_time: Duration,
    genetic_config: GeneticConfig,
}

#[derive(Serialize)]
struct ExploreFunctionArtifact<'a> {
    version: u32,
    status: &'a str,
    file: &'a str,
    function_name: &'a str,
    start_line: u32,
    end_line: u32,
    wall_time_ms: u64,
    mock_symbols: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<&'a shatter_core::explorer::ObservationOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

fn explore_artifact_root(project_root: Option<&str>) -> PathBuf {
    project_root
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shatter-artifacts")
        .join("explore-results")
}

fn sanitize_artifact_component(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn explore_artifact_path(
    root: &Path,
    file: &str,
    func: &shatter_core::protocol::FunctionAnalysis,
) -> PathBuf {
    let file_component = sanitize_artifact_component(file);
    let fn_component = sanitize_artifact_component(&func.name);
    root.join(file_component)
        .join(format!("{:05}_{}.json", func.start_line, fn_component))
}

fn write_explore_artifact(
    root: &Path,
    file: &str,
    outcome: &FuncExploreOutcome,
) -> Result<PathBuf, String> {
    let status = if outcome.result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    let artifact = ExploreFunctionArtifact {
        version: 1,
        status,
        file,
        function_name: &outcome.func.name,
        start_line: outcome.func.start_line,
        end_line: outcome.func.end_line,
        wall_time_ms: outcome.wall_time.as_millis() as u64,
        mock_symbols: &outcome.mock_symbols,
        observation: outcome.result.as_ref().ok(),
        error: outcome.result.as_ref().err().map(String::as_str),
    };
    let path = explore_artifact_path(root, file, &outcome.func);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create explore artifact dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&artifact)
        .map_err(|e| format!("failed to serialize explore artifact: {e}"))?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json)
        .map_err(|e| format!("failed to write explore artifact temp file: {e}"))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("failed to finalize explore artifact: {e}"))?;
    Ok(path)
}

fn emit_explore_progress(
    function: &str,
    current: usize,
    total: usize,
    elapsed: Duration,
    status: &str,
) {
    let line = match status {
        "started" => format!("[progress] starting {current}/{total}: {function}"),
        "completed" => format!(
            "[progress] completed {current}/{total}: {function} ({:.1}s)",
            elapsed.as_secs_f64()
        ),
        "failed" => format!(
            "[progress] failed {current}/{total}: {function} ({:.1}s)",
            elapsed.as_secs_f64()
        ),
        other => format!("[progress] {other} {current}/{total}: {function}"),
    };
    eprintln!("{line}");

    if let Some(json) =
        ProgressEvent::with_status(function, current, total, elapsed.as_millis() as u64, status)
            .to_json()
    {
        eprintln!("{json}");
    }
}

/// Run the explore command.
// Each argument corresponds to a CLI flag; grouping into a struct would add indirection
// without improving clarity since this is only called from one callsite.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_explore(
    targets: &[String],
    max_iterations: u32,
    timeout: u64,
    timeout_explore: Option<f64>,
    scope_path: Option<&Path>,
    analyze_only: bool,
    _show_clusters: bool,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    timing_enabled: bool,
    inputs_path: Option<&Path>,
    config_path: Option<&Path>,
    output_path: Option<&Path>,
    log_level: LogLevel,
    show_perf: bool,
    colors: &Colors,
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
    use_concolic: bool,
    solver_timeout: Option<u64>,
    memory_limit: Option<u64>,
    clean: bool,
    dry_run: bool,
    project_dir: Option<&Path>,
    loop_buckets_str: &str,
    use_color: bool,
    seeds_dir: &Path,
    no_seeds: bool,
    record: bool,
    set_overrides: &[String],
    meta_config: &shatter_core::strategy::MetaConfig,
    observe_output: Option<&Path>,
    replay_recorded: bool,
    no_replay: bool,
    refine_budget: usize,
    shrink_budget: usize,
    mcdc: bool,
    isolation: shatter_core::explorer::IsolationMode,
    capture_side_effects: bool,
    output_format: crate::args::OutputFormat,
    report_outputs: &[std::path::PathBuf],
    stdout: bool,
    format: crate::args::StdoutFormat,
    jobs: usize,
    cli_genetic: bool,
    cli_genetic_population: Option<u32>,
    cli_genetic_generations: Option<u32>,
    cli_genetic_timeout: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _explore_span = tracing::info_span!("core.explore_command").entered();
    let pool_path = if no_seeds {
        None
    } else {
        Some(seeds_dir.join("pool.json"))
    };
    let loop_buckets = parse_loop_buckets(loop_buckets_str)?;
    let scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            log::info!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let _scope_matcher =
        ScopeMatcher::new(&scope_config).map_err(|e| format!("invalid scope config: {e}"))?;

    let cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(p) => p.to_path_buf(),
            None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
        };
        Some(BehaviorMapCache::new(dir).map_err(|e| format!("failed to initialize cache: {e}"))?)
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;
    validate_targets(&parsed)?;

    let req_timeout = Duration::from_secs(request_timeout);

    let mut file_spec_bundles: Vec<FileSpecBundle> = Vec::new();

    let report_style = if use_color {
        shatter_core::report_style::ReportStyle::ansi()
    } else {
        shatter_core::report_style::ReportStyle::default()
    };

    // Count total functions across all targets for header/footer.
    let mut total_function_count: usize = 0;
    let mut total_paths: usize = 0;
    let mut total_covered: usize = 0;
    let mut total_lines: u32 = 0;
    let mut header_printed = false;

    // Resolve effective parallelism: 0 means auto-detect (CPU count).
    let effective_jobs = if jobs == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        jobs
    };

    // Resolve project root once for harness storage env propagation.
    // Explicit --project-dir wins; otherwise auto-detect from the first target.
    let storage_project_root = resolve_project_root(project_dir, &parsed[0].file);

    // Build per-language FrontendConfig templates for spawning per-function explore
    // frontends.  Also spawn one shared frontend per language for the analysis phase
    // (analysis is fast and doesn't benefit from parallelism).
    let mut frontends: HashMap<crate::args::Language, Frontend> = HashMap::new();
    let mut fe_configs: HashMap<crate::args::Language, FrontendConfig> = HashMap::new();
    let unique_langs: HashSet<crate::args::Language> = parsed.iter().map(|t| t.language).collect();
    for lang in unique_langs {
        let mut config = frontend_config(
            lang,
            req_timeout,
            log_level,
            exec_timeout,
            build_timeout,
            memory_limit,
            None,
            timing_enabled,
            release,
        )?;
        apply_project_storage(&mut config, storage_project_root.as_deref());
        if mcdc {
            config
                .env_vars
                .push(("SHATTER_MCDC".to_string(), "1".to_string()));
        }
        fe_configs.insert(lang, config.clone());
        let frontend = Frontend::spawn(&config)
            .await
            .map_err(|e| format!("failed to spawn {} frontend: {e}", lang.label()))?;
        log::debug!(
            "Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );
        frontends.insert(lang, frontend);
    }
    log::info!(
        "Spawned {} frontend session(s) for {} target(s) ({} parallel job(s))",
        frontends.len(),
        parsed.len(),
        effective_jobs,
    );

    // Accumulate HTML and markdown fragments for -o report files.
    let mut html_fragments: Vec<String> = Vec::new();
    let mut md_fragments: Vec<String> = Vec::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target.function.as_deref().unwrap_or("(all)");

        let project_root_str = resolve_project_root(project_dir, &target.file);

        if let Some(ref root) = project_root_str {
            log::debug!("Project root: {root}");
        }
        log::debug!(
            "Exploring {file_str}:{func_display} [language={}, max_iterations={max_iterations}]",
            target.language.label()
        );

        let frontend = frontends
            .get_mut(&target.language)
            .expect("frontend must exist for target language — spawned above");

        // Analyze phase
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
                project_root: project_root_str.clone(),
                execution_profile: None,
            })
            .instrument(tracing::info_span!("frontend.analyze"))
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match &analyze_response.result {
            ResponseResult::Analyze { functions } => {
                log::debug!("Found {} function(s):", functions.len());
                for func in functions {
                    log::debug!(
                        "  - {} ({} params, {} branches)",
                        func.name,
                        func.params.len(),
                        func.branches.len(),
                    );
                }
            }
            ResponseResult::Error { code, message, .. } => {
                log::error!("Analyze error ({code:?}): {message}");
                continue;
            }
            other => {
                log::error!("Unexpected analyze response: {other:?}");
                continue;
            }
        }

        let functions = match &analyze_response.result {
            ResponseResult::Analyze { functions } => functions.clone(),
            _ => unreachable!("already matched above"),
        };

        if analyze_only {
            if log::log_enabled!(log::Level::Info) {
                for func in &functions {
                    println!(
                        "{}{}{}  ({file_str}:{})",
                        colors.bold, func.name, colors.reset, func.start_line
                    );
                    println!(
                        "  {}params: {}, branches: {}{}",
                        colors.dim,
                        func.params.len(),
                        func.branches.len(),
                        colors.reset
                    );

                    // Show adapter selection results.
                    if let Ok(selection) =
                        adapter_selection::select_adapters(None, &func.adapter_hints)
                    {
                        for active in &selection.active {
                            println!(
                                "  {}adapter [active]: {} ({}){}",
                                colors.bold,
                                active.adapter.id,
                                active.provenance,
                                colors.reset,
                            );
                        }
                        for suggested in &selection.suggested {
                            println!(
                                "  {}adapter [suggested]: {} [{:?}]{}",
                                colors.dim,
                                suggested.adapter.id,
                                suggested.confidence,
                                colors.reset,
                            );
                        }
                    }
                }
            }
            continue;
        }

        // Load cached fingerprints for cross-file dependencies.
        let external_fingerprints = {
            let _cache_load_span =
                tracing::info_span!("cache.load_external_fingerprints").entered();
            load_external_fingerprints(&functions, cache.as_ref())
        };

        // Incremental plan: compare fingerprints against existing spec when --output is set
        let incremental_plan = if let Some(out) = output_path
            && !clean
            && out.exists()
        {
            match shatter_core::spec::read_file_spec_bundle(out) {
                Ok(existing) => {
                    match shatter_core::spec::compute_incremental_plan(
                        &target.file,
                        &functions,
                        &existing,
                        &external_fingerprints,
                    ) {
                        Ok(plan) => Some((plan, existing)),
                        Err(e) => {
                            log::debug!("Failed to compute incremental plan: {e}");
                            None
                        }
                    }
                }
                Err(e) => {
                    log::debug!("Failed to read existing spec: {e}");
                    None
                }
            }
        } else {
            None
        };

        let fresh_set: HashSet<String> = incremental_plan
            .as_ref()
            .map(|(plan, _)| plan.fresh.iter().cloned().collect())
            .unwrap_or_default();

        // Dry-run mode: print incremental plan and exit
        if dry_run {
            if let Some((ref plan, _)) = incremental_plan {
                if !plan.stale.is_empty() {
                    println!("Stale ({}):", plan.stale.len());
                    for name in &plan.stale {
                        println!("  {name}");
                    }
                }
                if !plan.fresh.is_empty() {
                    println!("Fresh ({}):", plan.fresh.len());
                    for name in &plan.fresh {
                        println!("  {name}");
                    }
                }
                if !plan.removed.is_empty() {
                    println!("Removed ({}):", plan.removed.len());
                    for name in &plan.removed {
                        println!("  {name}");
                    }
                }
            } else {
                println!(
                    "No existing spec to compare against — all {} function(s) are stale.",
                    functions.len()
                );
                for func in &functions {
                    println!("  {}", func.name);
                }
            }
            continue;
        }

        if !fresh_set.is_empty() && log::log_enabled!(log::Level::Info) {
            log::info!("Skipping {} fresh function(s):", fresh_set.len());
            for name in &fresh_set {
                log::info!("  {name}");
            }
        }

        // Load .shatter/ config for this target
        let shatter_configs: Vec<ShatterConfig> = if let Some(cp) = config_path {
            // Explicit config bypasses discovery
            let cfg = shatter_config::parse_config(cp)
                .map_err(|e| format!("failed to load config: {e}"))?;
            log::debug!("Loaded config from {}", cp.display());
            vec![cfg]
        } else {
            // Hierarchical discovery from target file's directory
            let target_dir = target.file.parent().unwrap_or(Path::new("."));
            shatter_config::discover_configs(target_dir)
                .map_err(|e| format!("config discovery error: {e}"))?
        };

        // Compute deep fingerprints (call-graph-aware) for spec output.
        let deep_fingerprints: std::collections::HashMap<String, String> =
            shatter_core::fingerprint::compute_deep_fingerprints(
                &target.file,
                &functions,
                &external_fingerprints,
            )
            .unwrap_or_default();

        // Track function count for header/footer.
        total_function_count += functions.len();

        // Print header on first non-analyze-only target.
        if !analyze_only && !header_printed && log::log_enabled!(log::Level::Info) {
            if output_format == crate::args::OutputFormat::Md {
                print_markdown("# Shatter Explore\n\n", use_color);
            } else {
                print!(
                    "\n{bold}\u{2550}\u{2550}\u{2550} Shatter Explore \u{2550}\u{2550}\u{2550}{reset}\n\n",
                    bold = report_style.bold,
                    reset = report_style.reset,
                );
            }
            header_printed = true;
        }

        // Exploration phase: generate random inputs and execute.
        //
        // Three phases:
        //   1. Collect work items (sequential — config resolution, mock generation)
        //   2. Parallel exploration (tokio::spawn per function, each with its own frontend)
        //   3. Process results (sequential — stats, reports, specs)
        let mut skipped_unexecutable: Vec<(String, Vec<executability::SkipReason>)> = Vec::new();
        let mut file_specs: Vec<shatter_core::spec::FunctionSpec> = Vec::new();

        // Capture capabilities from the shared analysis frontend for ExploreConfig construction.
        let frontend_caps =
            shatter_core::orchestrator::FrontendCapabilities::from_raw(frontend.capabilities());

        // --- Phase 1: Collect work items (fast, sequential) ---
        struct FuncWorkItem {
            func: shatter_core::protocol::FunctionAnalysis,
            explore_config: ExploreConfig,
            mock_symbols: Vec<String>,
            concolic_config: Option<shatter_core::orchestrator::ExploreConfig>,
            seed_inputs: Vec<Vec<serde_json::Value>>,
            user_inputs: Vec<Vec<serde_json::Value>>,
            genetic_config: GeneticConfig,
        }

        let mut work_items: Vec<FuncWorkItem> = Vec::new();
        for func in &functions {
            // Skip fresh functions in incremental mode
            if fresh_set.contains(&func.name) {
                continue;
            }

            let function_id = format!("{}:{}", file_str, func.name);

            // Resolve per-function config
            let resolved = shatter_config::resolve_function_config_with_inputs(
                &function_id,
                target.file.parent().unwrap_or(Path::new(".")),
                inputs_path,
                max_iterations,
                timeout,
                set_overrides,
            )
            .map_err(|e| format!("config resolution error for {}: {e}", func.name))?;

            // Run adapter selection policy: merge config profile with frontend hints.
            let adapter_selection_result = adapter_selection::select_adapters(
                resolved.execution_profile.as_ref(),
                &func.adapter_hints,
            )
            .map_err(|e| format!("adapter selection error for {}: {e}", func.name))?;

            let resolved_execution_profile = adapter_selection_result.to_execution_profile();

            for active in &adapter_selection_result.active {
                log::info!(
                    "  {} adapter [active]: {} ({})",
                    func.name,
                    active.adapter.id,
                    active.provenance,
                );
            }
            for suggested in &adapter_selection_result.suggested {
                log::info!(
                    "  {} adapter [suggested]: {} [{:?}]",
                    func.name,
                    suggested.adapter.id,
                    suggested.confidence,
                );
            }
            for rejected in &adapter_selection_result.rejected {
                log::warn!(
                    "  {} adapter [rejected]: {} — {}",
                    func.name,
                    rejected.adapter_id,
                    rejected.reason,
                );
            }

            // Merge CLI --genetic flags with config.yaml resolved genetic config.
            // CLI --genetic explicitly enables; when absent, config.yaml provides defaults.
            let effective_genetic = if cli_genetic {
                GeneticConfig {
                    enabled: true,
                    population_size: cli_genetic_population
                        .unwrap_or(resolved.genetic.population_size),
                    max_generations: cli_genetic_generations
                        .unwrap_or(resolved.genetic.max_generations),
                    timeout_secs: cli_genetic_timeout.unwrap_or(resolved.genetic.timeout_secs),
                    ..resolved.genetic
                }
            } else {
                resolved.genetic.clone()
            };

            if resolved.skip {
                log::debug!("Skipping {} (skip=true in config)", func.name);
                continue;
            }

            // Check for unexecutable parameter types (opaque types like net.Socket).
            let skip_reasons = executability::check_executability(&func.params, &[]);
            if !skip_reasons.is_empty() {
                log::debug!("Skipping {} (unexecutable parameter types)", func.name);
                skipped_unexecutable.push((func.name.clone(), skip_reasons));
                continue;
            }

            // Generate mocks: passthrough in record mode, auto-mocks otherwise.
            let (auto_mocks, mock_params) = if record {
                let passthrough =
                    shatter_core::recorded_mocks::build_passthrough_mocks(&func.dependencies);
                (passthrough, vec![])
            } else {
                // Check for recorded mock fixtures to seed from prior --record runs.
                let recorded_configs = if !no_replay {
                    let artifacts_dir = std::path::Path::new("shatter-artifacts");
                    let legacy_dir = std::path::Path::new(".shatter");
                    let should_replay = replay_recorded
                        || artifacts_dir
                            .join(shatter_core::recorded_mocks::RECORDED_MOCKS_DIR)
                            .is_dir()
                        || legacy_dir
                            .join(shatter_core::recorded_mocks::RECORDED_MOCKS_DIR)
                            .is_dir();
                    if should_replay {
                        // Check new location first, then fall back to legacy .shatter/
                        if let Some(mock_path) = shatter_core::recorded_mocks::find_recorded_mocks(
                            artifacts_dir,
                            &file_str,
                            &func.name,
                        )
                        .or_else(|| {
                            shatter_core::recorded_mocks::find_recorded_mocks(
                                legacy_dir, &file_str, &func.name,
                            )
                        }) {
                            match shatter_core::recorded_mocks::load_recorded_mocks(&mock_path) {
                                Ok(mock_file) => {
                                    let configs = shatter_core::recorded_mocks::recorded_mocks_to_mock_configs(&mock_file);
                                    log::info!(
                                        "Loaded {} recorded mock(s) for {} from {}",
                                        configs.len(),
                                        func.name,
                                        mock_path.display(),
                                    );
                                    configs
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to load recorded mocks for {} from {}: {e}",
                                        func.name,
                                        mock_path.display(),
                                    );
                                    vec![]
                                }
                            }
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                let auto_generated = shatter_core::auto_mock::generate_auto_mocks(
                    &func.dependencies,
                    None,
                    &resolved.mock_overrides,
                    &recorded_configs,
                );
                // Recorded configs first (higher priority), then auto-generated for remaining deps.
                let mut mocks = recorded_configs;
                mocks.extend(auto_generated);
                let params = shatter_core::auto_mock::build_mock_params(&func.dependencies, &mocks);
                (mocks, params)
            };
            let mock_symbols: Vec<String> = auto_mocks.iter().map(|m| m.symbol.clone()).collect();

            // Build candidate inputs from config, then extend with cached seeds
            // from prior exploration runs so discovery compounds across runs.
            let mut candidate_inputs: Vec<Vec<serde_json::Value>> = resolved
                .candidate_inputs
                .iter()
                .map(|input| input.args.clone())
                .collect();
            if let Some(ref cache) = cache
                && let Ok(Some(cached_map)) = cache.load(&function_id)
            {
                let cached_seeds = cached_map.extract_seed_inputs();
                if !cached_seeds.is_empty() {
                    log::debug!(
                        "Loaded {} cached seed(s) for {}",
                        cached_seeds.len(),
                        func.name,
                    );
                    candidate_inputs.extend(cached_seeds);
                }
            }

            let explore_config = ExploreConfig {
                file: file_str.to_string(),
                max_iterations: resolved.max_iterations,
                seed: None,
                mocks: auto_mocks,
                mock_params,
                setup_file: resolved.setup.as_ref().map(|p| p.display().to_string()),
                setup_level: resolved.setup_level,
                value_sources: shatter_core::input_gen::resolve_value_sources(
                    &func.params,
                    &resolved.param_generators,
                    &resolved.generators,
                ),
                capabilities: frontend_caps.clone(),
                user_seeds: vec![],
                candidate_inputs,
                pool_seeds: match &pool_path {
                    Some(pp) => match shatter_core::interesting_pool::load_pool(pp) {
                        Ok(Some(pool)) => {
                            shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool)
                        }
                        _ => vec![],
                    },
                    None => vec![],
                },
                project_root: project_root_str.clone(),
                execution_profile: resolved_execution_profile.clone(),
                loop_buckets: loop_buckets.clone(),
                timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                meta_config: meta_config.clone(),
                shrink_budget,
                isolation,
                capture_side_effects,
                budget_surplus: None,
                claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
            };

            // Build concolic-specific config if needed.
            let (concolic_config, seed_inputs, user_inputs) = if use_concolic {
                let mut seeds = shatter_core::boundary_dict::generate_boundary_inputs(&func.params);
                let users: Vec<Vec<serde_json::Value>> = resolved
                    .candidate_inputs
                    .iter()
                    .map(|input| input.args.clone())
                    .collect();

                // Add pool-derived seeds for concolic mode
                if let Some(ref pp) = pool_path
                    && let Ok(Some(pool)) = shatter_core::interesting_pool::load_pool(pp)
                {
                    let pool_candidates =
                        shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool);
                    seeds.extend(pool_candidates);
                }

                // Literal-derived seeds: string/number constants from static analysis
                let literal_candidates = shatter_core::input_gen::literals_to_candidate_inputs(
                    &func.params,
                    &func.literals,
                );
                seeds.extend(literal_candidates);

                // Add cached seeds from prior exploration runs.
                if let Some(ref cache) = cache
                    && let Ok(Some(cached_map)) = cache.load(&function_id)
                {
                    let cached_seeds = cached_map.extract_seed_inputs();
                    if !cached_seeds.is_empty() {
                        log::debug!(
                            "Loaded {} cached seed(s) for concolic on {}",
                            cached_seeds.len(),
                            func.name,
                        );
                        seeds.extend(cached_seeds);
                    }
                }

                let cc = shatter_core::orchestrator::ExploreConfig {
                    max_iterations: explore_config.max_iterations as usize,
                    max_executions: (explore_config.max_iterations as usize) * 5,
                    plateau_threshold: if mcdc { 60 } else { 20 },
                    mocks: explore_config.mocks.clone(),
                    mock_params: explore_config.mock_params.clone(),
                    solver_timeout_ms: solver_timeout.map(|s| s * 1000),
                    timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                    branch_profile: None, // standalone concolic has no prior random phase
                    meta_config: meta_config.clone(),
                    execution_profile: explore_config.execution_profile.clone(),
                    loop_convergence_window: 3,
                    refine_budget: if refine_budget > 0 {
                        Some(refine_budget)
                    } else {
                        None
                    },
                    shrink_budget,
                    mcdc,
                };
                (Some(cc), seeds, users)
            } else {
                (None, vec![], vec![])
            };

            if !resolved.candidate_inputs.is_empty() {
                log::debug!(
                    "Exploring {} ({} candidate input(s) from config)...",
                    func.name,
                    resolved.candidate_inputs.len()
                );
            } else {
                log::debug!("Exploring {}...", func.name);
            }

            let _ = &shatter_configs; // suppress unused warning

            work_items.push(FuncWorkItem {
                func: func.clone(),
                explore_config,
                mock_symbols,
                concolic_config,
                seed_inputs,
                user_inputs,
                genetic_config: effective_genetic,
            });
        }

        // --- Phase 2: Parallel exploration ---
        let semaphore = Arc::new(tokio::sync::Semaphore::new(effective_jobs));
        let completed_functions = Arc::new(AtomicUsize::new(0));
        let mut join_set = tokio::task::JoinSet::new();
        let total_work_items = work_items.len();
        let artifact_root = explore_artifact_root(project_root_str.as_deref());

        let fe_config_for_lang = fe_configs
            .get(&target.language)
            .expect("fe_config must exist for target language")
            .clone();

        for (work_index, item) in work_items.into_iter().enumerate() {
            let sem = Arc::clone(&semaphore);
            let completed_functions = Arc::clone(&completed_functions);
            let fe_config = fe_config_for_lang.clone();
            let file_str_owned = file_str.to_string();
            let project_root_owned = project_root_str.clone();
            let progress_index = work_index + 1;
            let progress_total = total_work_items;

            join_set.spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore is never closed");
                let func_start = Instant::now();
                emit_explore_progress(
                    &item.func.name,
                    progress_index,
                    progress_total,
                    Duration::ZERO,
                    "started",
                );

                let mut task_frontend = match Frontend::spawn(&fe_config).await {
                    Ok(fe) => fe,
                    Err(e) => {
                        let completed = completed_functions.fetch_add(1, Ordering::Relaxed) + 1;
                        emit_explore_progress(
                            &item.func.name,
                            completed,
                            progress_total,
                            func_start.elapsed(),
                            "failed",
                        );
                        return FuncExploreOutcome {
                            work_index,
                            func: item.func,
                            mock_symbols: item.mock_symbols,
                            result: Err(format!("failed to spawn frontend: {e}")),
                            wall_time: func_start.elapsed(),
                            genetic_config: item.genetic_config,
                        };
                    }
                };

                let explore_result = if let Some(ref concolic_config) = item.concolic_config {
                    // Concolic path: instrument → prepare → orchestrator::explore
                    if let Err(e) = task_frontend
                        .send(ProtoCommand::Instrument {
                            file: file_str_owned.clone(),
                            function: item.func.name.clone(),
                            mocks: concolic_config.mocks.clone(),
                            project_root: project_root_owned.clone(),
                            execution_profile: None,
                        })
                        .await
                    {
                        log::debug!("instrument failed for concolic path: {e}");
                    }

                    let caps = shatter_core::orchestrator::FrontendCapabilities::from_raw(
                        task_frontend.capabilities(),
                    );
                    let prepare_id: Option<String> = if caps.commands.contains("prepare") {
                        match task_frontend
                            .send(ProtoCommand::Prepare {
                                file: file_str_owned.clone(),
                                function: item.func.name.clone(),
                                mocks: concolic_config.mocks.clone(),
                                project_root: project_root_owned.clone(),
                                execution_profile: None,
                            })
                            .await
                        {
                            Ok(resp) => match resp.result {
                                ResponseResult::Prepare { prepare_id } => {
                                    log::debug!("concolic prepare succeeded: {prepare_id}");
                                    Some(prepare_id)
                                }
                                other => {
                                    log::debug!("concolic prepare unexpected response: {other:?}");
                                    None
                                }
                            },
                            Err(e) => {
                                log::debug!("concolic prepare failed, falling back: {e}");
                                None
                            }
                        }
                    } else {
                        None
                    };

                    match shatter_core::orchestrator::explore(
                        &mut task_frontend,
                        &item.func.name,
                        item.seed_inputs,
                        item.user_inputs,
                        &item.func.params,
                        concolic_config,
                        None,
                        prepare_id,
                        item.func.loops.clone(),
                    )
                    .await
                    {
                        Ok(mut concolic_result) => {
                            concolic_result.total_lines =
                                item.func.end_line.saturating_sub(item.func.start_line) + 1;
                            let obs: shatter_core::explorer::ObservationOutput =
                                concolic_result.into();
                            Ok(obs)
                        }
                        Err(shatter_core::orchestrator::ExploreError::Frontend(fe)) => {
                            Err(shatter_core::explorer::ExploreError::Frontend(fe))
                        }
                    }
                } else {
                    // Random path: explore_function handles instrument + prepare internally.
                    explorer::explore_function(
                        &mut task_frontend,
                        &item.func,
                        &item.explore_config,
                        None,
                    )
                    .instrument(tracing::info_span!("explore.function"))
                    .await
                };

                let result = explore_result.map_err(|e| e.to_string());
                let completed = completed_functions.fetch_add(1, Ordering::Relaxed) + 1;
                emit_explore_progress(
                    &item.func.name,
                    completed,
                    progress_total,
                    func_start.elapsed(),
                    if result.is_ok() {
                        "completed"
                    } else {
                        "failed"
                    },
                );

                let _ = task_frontend.shutdown().await;

                FuncExploreOutcome {
                    work_index,
                    func: item.func,
                    mock_symbols: item.mock_symbols,
                    result,
                    wall_time: func_start.elapsed(),
                    genetic_config: item.genetic_config,
                }
            });
        }

        // --- Phase 3: Collect results and process (sequential, in order) ---
        let mut outcomes = Vec::new();
        while let Some(joined) = join_set.join_next().await {
            let outcome = match joined {
                Ok(o) => o,
                Err(e) => {
                    log::error!("Task join error: {e}");
                    continue;
                }
            };
            match write_explore_artifact(&artifact_root, &file_str, &outcome) {
                Ok(path) => log::info!(
                    "Wrote explore artifact for {} -> {}",
                    outcome.func.name,
                    path.display()
                ),
                Err(e) => log::warn!(
                    "Failed to write explore artifact for {}: {e}",
                    outcome.func.name
                ),
            }
            outcomes.push(outcome);
        }
        outcomes.sort_by_key(|outcome| outcome.work_index);

        for outcome in outcomes {
            let func = &outcome.func;

            match outcome.result {
                Ok(result) => {
                    let wall_time = outcome.wall_time;
                    let mock_symbols = &outcome.mock_symbols;

                    // Harvest interesting inputs into the cross-function pool.
                    if let Some(ref pp) = pool_path {
                        let mut pool = shatter_core::interesting_pool::load_pool(pp)
                            .unwrap_or_else(|e| {
                                log::warn!("failed to load interesting pool: {e}");
                                None
                            })
                            .unwrap_or_default();
                        let harvested = shatter_core::interesting_pool::harvest_from_exploration(
                            &mut pool,
                            &result.raw_results,
                            &func.params,
                            &func.name,
                        );
                        if harvested > 0
                            && let Err(e) = shatter_core::interesting_pool::save_pool(&pool, pp)
                        {
                            log::warn!("failed to save interesting pool: {e}");
                        }
                    }

                    // Record mode: persist external dependency observations.
                    if record {
                        let behaviors = shatter_core::recorded_mocks::aggregate_recordings(
                            &result.raw_results,
                            &func.dependencies,
                        );
                        if !behaviors.is_empty() {
                            let mock_file = shatter_core::recorded_mocks::build_recorded_mock_file(
                                &func.name, &file_str, behaviors,
                            );
                            let artifacts_dir = std::path::Path::new("shatter-artifacts");
                            match shatter_core::recorded_mocks::save_recorded_mocks(
                                &mock_file,
                                artifacts_dir,
                            ) {
                                Ok(path) => log::info!(
                                    "Recorded {} dep(s) for {} -> {}",
                                    mock_file.dependencies.len(),
                                    func.name,
                                    path.display(),
                                ),
                                Err(e) => log::error!(
                                    "Failed to save recorded mocks for {}: {e}",
                                    func.name,
                                ),
                            }
                        }
                    }

                    // Accumulate stats for footer.
                    total_paths += result.unique_paths;
                    total_covered += result.lines_covered;
                    total_lines += result.total_lines;

                    // Accumulate HTML/markdown fragments for -o report files.
                    {
                        let location = format!("{file_str}:{}-{}", func.start_line, func.end_line);
                        html_fragments.push(shatter_core::report::render_explore_fn_html(
                            &result,
                            &location,
                            project_root_str.as_deref().map(std::path::Path::new),
                        ));
                    }

                    // Run the Analyze stage to get coverage metrics and eq classes.
                    let analyze_output = {
                        let _pipeline_analyze_span =
                            tracing::info_span!("pipeline.analyze").entered();
                        shatter_core::pipeline::analyze(&result, func)
                    };

                    // Save raw observation data for offline analysis if requested.
                    if let Some(obs_dir) = observe_output {
                        let safe_name = func
                            .name
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
                        let obs_path = obs_dir.join(format!("{safe_name}.observe.json"));
                        let stage_json = serde_json::json!({
                            "observation": &result,
                            "analysis": func,
                            "file": file_str,
                        });
                        if let Some(parent) = obs_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match serde_json::to_string_pretty(&stage_json) {
                            Ok(json) => {
                                if let Err(e) = std::fs::write(&obs_path, json) {
                                    log::error!(
                                        "Failed to write observe output for {}: {e}",
                                        func.name
                                    );
                                } else {
                                    log::info!("Wrote observe output: {}", obs_path.display());
                                }
                            }
                            Err(e) => log::error!(
                                "Failed to serialize observe output for {}: {e}",
                                func.name
                            ),
                        }
                    }

                    // --- Genetic algorithm follow-up phase ---
                    let mut ga_stored_cache = false;
                    let ga_stats: Option<GeneticStats> = if outcome.genetic_config.enabled {
                        let targets =
                            shatter_core::coverage_metrics::extract_targets(func, &result);
                        if targets.is_empty() {
                            log::debug!("No unsolved targets for GA on {}", func.name);
                            None
                        } else {
                            let targets_attempted = targets.len();
                            log::info!(
                                "Starting GA for {} ({} unsolved target(s))",
                                func.name,
                                targets_attempted,
                            );
                            let mut seed_inputs: Vec<Vec<serde_json::Value>> = result
                                .raw_results
                                .iter()
                                .map(|(inputs, _, _)| inputs.clone())
                                .collect();
                            // Extend GA seeds with cached inputs from prior runs.
                            if let Some(ref cache) = cache {
                                let ga_function_id = format!("{}:{}", file_str, func.name);
                                if let Ok(Some(cached_map)) = cache.load(&ga_function_id) {
                                    seed_inputs.extend(cached_map.extract_seed_inputs());
                                }
                            }
                            let ga_fe_config = fe_configs
                                .get(&target.language)
                                .expect("fe_config must exist for target language")
                                .clone();
                            match Frontend::spawn(&ga_fe_config).await {
                                Ok(mut ga_frontend) => {
                                    // Instrument before running GA so execute calls work.
                                    let mock_symbols_for_ga: Vec<shatter_core::protocol::MockConfig> =
                                        outcome.mock_symbols.iter().map(|s| {
                                            shatter_core::protocol::MockConfig {
                                                symbol: s.clone(),
                                                return_values: vec![],
                                                should_track_calls: false,
                                                default_behavior: shatter_core::protocol::MockBehavior::ReturnGenerated,
                                            }
                                        }).collect();
                                    let _ = ga_frontend
                                        .send(ProtoCommand::Instrument {
                                            file: file_str.to_string(),
                                            function: func.name.clone(),
                                            mocks: mock_symbols_for_ga,
                                            project_root: project_root_str.clone(),
                                            execution_profile: None,
                                        })
                                        .await;
                                    match shatter_core::genetic_explorer::genetic_explore(
                                        &mut ga_frontend,
                                        &func.name,
                                        seed_inputs,
                                        targets,
                                        &func.params,
                                        &outcome.genetic_config,
                                    )
                                    .await
                                    {
                                        Ok(ga_result) => {
                                            let stats = GeneticStats {
                                                targets_attempted,
                                                targets_solved: ga_result.targets_solved,
                                                generations_run: ga_result.generations_run,
                                                total_executions: ga_result.total_executions,
                                            };
                                            if !ga_result.discoveries.is_empty() {
                                                log::info!(
                                                    "GA found {} new behavior(s) for {}",
                                                    ga_result.discoveries.len(),
                                                    func.name,
                                                );
                                                // Persist GA discoveries to cache.
                                                let mut bmap = BehaviorMap::from_exploration_result(
                                                    &func.name, &result,
                                                );
                                                let added = bmap
                                                    .merge_ga_discoveries(&ga_result.discoveries);
                                                if added > 0
                                                    && let Some(ref cache) = cache
                                                {
                                                    if let Err(e) = cache.store(&bmap) {
                                                        log::warn!(
                                                            "failed to cache GA-augmented behavior map for {}: {e}",
                                                            func.name
                                                        );
                                                    } else {
                                                        ga_stored_cache = true;
                                                    }
                                                }
                                            }
                                            let _ = ga_frontend.shutdown().await;
                                            Some(stats)
                                        }
                                        Err(e) => {
                                            log::error!("GA error for {}: {e}", func.name);
                                            let _ = ga_frontend.shutdown().await;
                                            None
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!(
                                        "Failed to spawn GA frontend for {}: {e}",
                                        func.name
                                    );
                                    None
                                }
                            }
                        }
                    } else {
                        None
                    };

                    if log::log_enabled!(log::Level::Info) {
                        if log::log_enabled!(log::Level::Trace) {
                            let report = {
                                let _report_span = tracing::info_span!("report.render").entered();
                                explorer::format_exploration_report_verbose(&result)
                            };
                            if report_outputs.is_empty() || stdout {
                                print!("{report}");
                            }
                        } else if output_format == crate::args::OutputFormat::Md {
                            let location =
                                format!("{file_str}:{}-{}", func.start_line, func.end_line);
                            let view = crate::render::explore_fn_view(
                                &result,
                                crate::render::ExploreRenderOpts {
                                    location: Some(&location),
                                    mocks_used: mock_symbols,
                                    is_concolic: use_concolic,
                                },
                            );
                            let md = {
                                let _report_span = tracing::info_span!("report.render").entered();
                                crate::render::render_explore_fn(&view)
                            };
                            md_fragments.push(md.clone());
                            if report_outputs.is_empty() || stdout {
                                print_markdown(&md, use_color);
                            }
                        } else {
                            let report_opts = ReportOptions {
                                location: Some(format!(
                                    "{file_str}:{}-{}",
                                    func.start_line, func.end_line
                                )),
                                show_perf,
                                wall_time: Some(wall_time),
                                coverage_metrics: Some(analyze_output.coverage_metrics.clone()),
                                style: report_style.clone(),
                                genetic_stats: ga_stats,
                            };
                            let report = {
                                let _report_span = tracing::info_span!("report.render").entered();
                                explorer::format_exploration_report(&result, &report_opts)
                            };
                            md_fragments.push(report.clone());
                            if report_outputs.is_empty() || stdout {
                                print!("{report}");
                                if !mock_symbols.is_empty() {
                                    println!("  Mocks used: {}", mock_symbols.join(", "));
                                }
                                if use_concolic {
                                    println!("  Explorer: concolic (Z3-backed)");
                                }
                            }
                        }
                        if report_outputs.is_empty() || stdout {
                            println!();
                        }
                    }

                    // Spec output: use eq classes from analyze stage
                    if show_spec || detect_invariants {
                        let eq_classes = &analyze_output.eq_classes;
                        let location =
                            Some(format!("{file_str}:{}-{}", func.start_line, func.end_line));

                        // Use deep fingerprint (call-graph-aware) for spec output.
                        let fingerprint = deep_fingerprints.get(&func.name).cloned();

                        let spec = {
                            let _spec_span = tracing::info_span!("spec.build").entered();
                            if detect_invariants {
                                shatter_core::spec::build_spec_with_invariants(
                                    &result,
                                    eq_classes,
                                    location,
                                    fingerprint,
                                )
                            } else {
                                shatter_core::spec::build_spec(
                                    &result,
                                    eq_classes,
                                    location,
                                    fingerprint,
                                )
                            }
                        };
                        if output_path.is_some() {
                            // Collect for file-level bundle output
                            file_specs.push(spec);
                        } else if spec_as_json {
                            match shatter_core::spec::format_spec_json(&spec) {
                                Ok(json) => println!("{json}"),
                                Err(e) => log::error!("Error serializing spec: {e}"),
                            }
                        } else {
                            print_markdown(
                                &shatter_core::spec::format_spec_markdown(&spec),
                                use_color,
                            );
                        }
                    }

                    if !ga_stored_cache {
                        let behavior_map =
                            BehaviorMap::from_exploration_result(&func.name, &result);
                        if let Some(ref cache) = cache {
                            let cache_result = {
                                let _cache_store_span =
                                    tracing::info_span!("cache.store").entered();
                                cache.store(&behavior_map)
                            };
                            if let Err(e) = cache_result {
                                log::warn!("failed to cache behavior map for {}: {e}", func.name);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Exploration error for {}: {e}", func.name);
                }
            }
        }

        // Print summary of skipped unexecutable functions.
        if !skipped_unexecutable.is_empty() && log::log_enabled!(log::Level::Info) {
            log::info!(
                "Skipped {} function(s) (unexecutable parameter types):",
                skipped_unexecutable.len()
            );
            for (name, reasons) in &skipped_unexecutable {
                for reason in reasons {
                    log::info!("  {name}: {}", reason.format_human());
                }
            }
        }

        // Collect file-level spec bundle when --output is set.
        if output_path.is_some() {
            let current_function_names: HashSet<String> =
                functions.iter().map(|f| f.name.clone()).collect();

            let bundle = if let Some((_, ref existing)) = incremental_plan {
                // Merge newly explored specs with fresh specs carried over from existing
                shatter_core::spec::merge_file_spec_bundles(
                    existing,
                    &file_specs,
                    &current_function_names,
                )
            } else {
                FileSpecBundle {
                    file: file_str.to_string(),
                    functions: file_specs,
                }
            };

            if !bundle.functions.is_empty() {
                file_spec_bundles.push(bundle);
            }
        }
    }

    // Shut down all frontend sessions now that all targets are complete.
    for (_, frontend) in frontends {
        if let Err(e) = frontend.shutdown().await {
            log::warn!("frontend shutdown error: {e}");
        }
    }

    // Print summary footer (only when streaming to stdout).
    if header_printed
        && log::log_enabled!(log::Level::Info)
        && (report_outputs.is_empty() || stdout)
    {
        if output_format == crate::args::OutputFormat::Md {
            let coverage_suffix = if total_lines > 0 {
                let pct = ((total_covered as f64 / total_lines as f64) * 100.0)
                    .min(100.0)
                    .round() as u32;
                format!(" · **{pct}%** coverage ({total_covered}/{total_lines} lines)")
            } else {
                String::new()
            };
            print_markdown(
                &format!(
                    "\n---\n\n**Summary:** {total_paths} path(s) across \
                     {total_function_count} function(s){coverage_suffix}\n"
                ),
                use_color,
            );
        } else {
            print!(
                "{}",
                explorer::format_explore_footer(
                    total_paths,
                    total_function_count,
                    total_covered,
                    total_lines,
                    &report_style,
                )
            );
        }
    }

    // Write exploration reports to -o files.
    for path in report_outputs {
        match crate::args::infer_output_format(path) {
            Ok(crate::args::StdoutFormat::Html) => {
                let html = shatter_core::report::wrap_explore_html(
                    &html_fragments,
                    total_function_count,
                    total_paths,
                    total_covered,
                    total_lines,
                );
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, html).map_err(|e| {
                    format!("failed to write HTML report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote HTML report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Markdown) => {
                let md = md_fragments.join("\n\n---\n\n");
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &md).map_err(|e| {
                    format!(
                        "failed to write markdown report to '{}': {e}",
                        path.display()
                    )
                })?;
                log::info!("Wrote markdown report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Text) => {
                let md = md_fragments.join("\n\n---\n\n");
                let text = shatter_core::report::strip_markdown_text(&md);
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(path, &text).map_err(|e| {
                    format!("failed to write text report to '{}': {e}", path.display())
                })?;
                log::info!("Wrote text report to {}", path.display());
            }
            Ok(crate::args::StdoutFormat::Json) => {
                // JSON output for explore writes spec bundle
                log::warn!(
                    "JSON output for explore writes spec bundle; use --spec-out for explicit spec output"
                );
                if let Some(first_bundle) = file_spec_bundles.first() {
                    shatter_core::spec::write_file_spec_bundle(first_bundle, path).map_err(
                        |e| format!("failed to write spec bundle to '{}': {e}", path.display()),
                    )?;
                    log::info!("Wrote spec bundle to {}", path.display());
                }
            }
            Err(e) => {
                log::error!("{e}");
            }
        }
    }

    // If files were written and --stdout was also requested, replay to stdout.
    if !report_outputs.is_empty() && stdout {
        let combined = md_fragments.join("\n\n---\n\n");
        match format {
            crate::args::StdoutFormat::Text => {
                print!("{}", shatter_core::report::strip_markdown_text(&combined));
            }
            _ => {
                print_markdown(&combined, use_color);
            }
        }
    }

    // Write collected file spec bundles to the output path as a single bundle.
    if let Some(out) = output_path
        && !file_spec_bundles.is_empty()
    {
        // Single-target is the primary Make use case; write the first bundle.
        {
            let _spec_write_span = tracing::info_span!("spec.write_bundle").entered();
            shatter_core::spec::write_file_spec_bundle(&file_spec_bundles[0], out)
                .map_err(|e| format!("failed to write spec bundle to {}: {e}", out.display()))?;
        }
        log::info!(
            "Wrote spec bundle ({} function(s)) to {}",
            file_spec_bundles[0].functions.len(),
            out.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        FuncExploreOutcome, emit_explore_progress, sanitize_artifact_component,
        write_explore_artifact,
    };
    use shatter_core::config::GeneticConfig;
    use shatter_core::protocol::{FunctionAnalysis, InvocationModel};
    use shatter_core::report::ProgressEvent;
    use shatter_core::types::TypeInfo;
    use std::time::Duration;

    #[test]
    fn progress_event_with_status_serializes() {
        let json = ProgressEvent::with_status("classifyNumber", 2, 5, 1234, "completed")
            .to_json()
            .expect("serialize");
        let event: ProgressEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event.status.as_deref(), Some("completed"));
        assert_eq!(event.current, 2);
        assert_eq!(event.total, 5);
    }

    #[test]
    fn emit_explore_progress_accepts_started_completed_and_failed() {
        emit_explore_progress("f", 1, 3, Duration::ZERO, "started");
        emit_explore_progress("f", 2, 3, Duration::from_millis(250), "completed");
        emit_explore_progress("f", 3, 3, Duration::from_millis(500), "failed");
    }

    #[test]
    fn write_explore_artifact_persists_completed_result() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = FuncExploreOutcome {
            work_index: 0,
            func: FunctionAnalysis {
                name: "load/user".to_string(),
                exported: true,
                start_line: 12,
                end_line: 20,
                params: vec![],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                literals: vec![],
                crypto_boundaries: vec![],
                loops: vec![],
                source_file: None,
                adapter_hints: vec![],
                invocation_model: InvocationModel::Direct,
            },
            mock_symbols: vec!["dep".to_string()],
            result: Ok(shatter_core::explorer::ObservationOutput {
                function_name: "load/user".to_string(),
                iterations: 1,
                unique_paths: 1,
                lines_covered: 3,
                total_lines: 8,
                new_path_executions: vec![],
                raw_results: vec![],
                discoveries: vec![],
                nondeterministic_fields: vec![],
                float_probe_results: vec![],
                boundary_results: vec![],
                shrunk_witnesses: std::collections::HashMap::new(),
                mcdc_summary: None,
                shrink_stats: shatter_core::shrink::ShrinkStats::default(),
                abandoned_frontiers: vec![],
                opaque_suggestions: vec![],
                stubbed_modules: vec![],
            }),
            wall_time: Duration::from_millis(25),
            genetic_config: GeneticConfig::default(),
        };

        let path =
            write_explore_artifact(dir.path(), "src/user.ts", &outcome).expect("write artifact");
        let json = std::fs::read_to_string(path).expect("read artifact");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");

        assert_eq!(value["status"], "completed");
        assert_eq!(value["function_name"], "load/user");
        assert_eq!(value["mock_symbols"][0], "dep");
        assert_eq!(value["observation"]["function_name"], "load/user");
    }

    #[test]
    fn sanitize_artifact_component_replaces_path_separators() {
        assert_eq!(sanitize_artifact_component("src/user.ts"), "src_user.ts");
        assert_eq!(sanitize_artifact_component(""), "unknown");
    }
}
