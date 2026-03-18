use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use shatter_core::behavior::BehaviorMap;
use shatter_core::cache::BehaviorMapCache;
use shatter_core::config::{self as shatter_config, ShatterConfig};
use shatter_core::executability;
use shatter_core::explorer::{self, ExploreConfig, ReportOptions};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::scope::{ScopeConfig, ScopeMatcher};
use shatter_core::spec::FileSpecBundle;
use tracing::Instrument;

use crate::args::*;
use crate::helpers::*;

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
    meta_config: &shatter_core::strategy::MetaConfig,
    observe_output: Option<&Path>,
    replay_recorded: bool,
    no_replay: bool,
    refine_budget: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let _explore_span = tracing::info_span!("core.explore_command").entered();
    let pool_path = if no_seeds { None } else { Some(seeds_dir.join("pool.json")) };
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

    let _scope_matcher = ScopeMatcher::new(&scope_config)
        .map_err(|e| format!("invalid scope config: {e}"))?;

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

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target
            .function
            .as_deref()
            .unwrap_or("(all)");

        let project_root_str = resolve_project_root(project_dir, &target.file);

        if let Some(ref root) = project_root_str {
            log::debug!("Project root: {root}");
        }
        log::debug!(
            "Exploring {file_str}:{func_display} [language={}, max_iterations={max_iterations}]",
            target.language.label()
        );

        let config = frontend_config(target.language, req_timeout, log_level, exec_timeout, build_timeout, memory_limit, None, timing_enabled)?;
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!(
                "failed to spawn {} frontend: {e}",
                target.language.label()
            )
        })?;

        log::debug!(
            "Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );

        // Analyze phase
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
                project_root: project_root_str.clone(),
            })
            .instrument(tracing::info_span!("frontend.analyze"))
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match &analyze_response.result {
            ResponseResult::Analyze { functions } => {
                log::debug!("Found {} function(s):", functions.len());
                for func in functions {
                    log::debug!("  - {} ({} params, {} branches)",
                        func.name,
                        func.params.len(),
                        func.branches.len(),
                    );
                }
            }
            ResponseResult::Error { code, message, .. } => {
                log::error!("Analyze error ({code:?}): {message}");
                shutdown_frontend(frontend).await;
                continue;
            }
            other => {
                log::error!("Unexpected analyze response: {other:?}");
                shutdown_frontend(frontend).await;
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
                }
            }
            shutdown_frontend(frontend).await;
            continue;
        }

        // Load cached fingerprints for cross-file dependencies.
        let external_fingerprints = {
            let _cache_load_span = tracing::info_span!("cache.load_external_fingerprints").entered();
            load_external_fingerprints(&functions, cache.as_ref())
        };

        // Incremental plan: compare fingerprints against existing spec when --output is set
        let incremental_plan = if let Some(out) = output_path
            && !clean
            && out.exists()
        {
            match shatter_core::spec::read_file_spec_bundle(out) {
                Ok(existing) => {
                    match shatter_core::spec::compute_incremental_plan(&target.file, &functions, &existing, &external_fingerprints) {
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
                println!("No existing spec to compare against — all {} function(s) are stale.", functions.len());
                for func in &functions {
                    println!("  {}", func.name);
                }
            }
            shutdown_frontend(frontend).await;
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
            shatter_core::fingerprint::compute_deep_fingerprints(&target.file, &functions, &external_fingerprints)
                .unwrap_or_default();

        // Track function count for header/footer.
        total_function_count += functions.len();

        // Print header on first non-analyze-only target.
        if !analyze_only && !header_printed && log::log_enabled!(log::Level::Info) {
            print!(
                "\n{bold}\u{2550}\u{2550}\u{2550} Shatter Explore \u{2550}\u{2550}\u{2550}{reset}\n\n",
                bold = report_style.bold,
                reset = report_style.reset,
            );
            header_printed = true;
        }

        // Exploration phase: generate random inputs and execute
        let mut skipped_unexecutable: Vec<(String, Vec<executability::SkipReason>)> = Vec::new();
        let mut file_specs: Vec<shatter_core::spec::FunctionSpec> = Vec::new();
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
            )
            .map_err(|e| format!("config resolution error for {}: {e}", func.name))?;

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
                let passthrough = shatter_core::recorded_mocks::build_passthrough_mocks(
                    &func.dependencies,
                );
                (passthrough, vec![])
            } else {
                // Check for recorded mock fixtures to seed from prior --record runs.
                let recorded_configs = if !no_replay {
                    let shatter_dir = std::path::Path::new(".shatter");
                    let should_replay = replay_recorded || shatter_dir.join(shatter_core::recorded_mocks::RECORDED_MOCKS_DIR).is_dir();
                    if should_replay {
                        if let Some(mock_path) = shatter_core::recorded_mocks::find_recorded_mocks(
                            shatter_dir,
                            &file_str,
                            &func.name,
                        ) {
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
                let params = shatter_core::auto_mock::build_mock_params(
                    &func.dependencies,
                    &mocks,
                );
                (mocks, params)
            };
            let mock_symbols: Vec<String> = auto_mocks.iter().map(|m| m.symbol.clone()).collect();

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
                capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
                user_seeds: vec![],
                candidate_inputs: resolved.candidate_inputs
                    .iter()
                    .map(|input| input.args.clone())
                    .collect(),
                pool_seeds: match &pool_path {
                    Some(pp) => match shatter_core::interesting_pool::load_pool(pp) {
                        Ok(Some(pool)) => shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool),
                        _ => vec![],
                    },
                    None => vec![],
                },
                project_root: project_root_str.clone(),
                loop_buckets: loop_buckets.clone(),
                timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                meta_config: meta_config.clone(), shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
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

            let func_start = Instant::now();

            // Choose exploration strategy: concolic (Z3-backed) or random.
            let explore_result: Result<shatter_core::explorer::ObservationOutput, shatter_core::explorer::ExploreError> = if use_concolic {
                let mut seed_inputs = shatter_core::boundary_dict::generate_boundary_inputs(&func.params);
                let user_inputs: Vec<Vec<serde_json::Value>> = resolved.candidate_inputs
                    .iter()
                    .map(|input| input.args.clone())
                    .collect();

                // Add pool-derived seeds for concolic mode
                if let Some(ref pp) = pool_path
                    && let Ok(Some(pool)) = shatter_core::interesting_pool::load_pool(pp)
                {
                    let pool_candidates = shatter_core::input_gen::pool_to_candidate_inputs(&func.params, &pool);
                    seed_inputs.extend(pool_candidates);
                }

                // Literal-derived seeds: string/number constants from static analysis
                let literal_candidates = shatter_core::input_gen::literals_to_candidate_inputs(&func.params, &func.literals);
                seed_inputs.extend(literal_candidates);

                let concolic_config = shatter_core::orchestrator::ExploreConfig {
                    max_iterations: explore_config.max_iterations as usize,
                    max_executions: (explore_config.max_iterations as usize) * 5,
                    plateau_threshold: 20,
                    mocks: explore_config.mocks.clone(),
                    mock_params: explore_config.mock_params.clone(),
                    solver_timeout_ms: solver_timeout.map(|s| s * 1000),
                    timeout_explore: timeout_explore.map(Duration::from_secs_f64),
                    branch_profile: None, // standalone concolic has no prior random phase
                    meta_config: meta_config.clone(),
                    loop_convergence_window: 3,
                    refine_budget: if refine_budget > 0 { Some(refine_budget) } else { None },
                    shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
                };

                match shatter_core::orchestrator::explore(
                    &mut frontend,
                    &func.name,
                    seed_inputs,
                    user_inputs,
                    &func.params,
                    &concolic_config,
                    None,
                ).await {
                    Ok(mut concolic_result) => {
                        // Fallback: concolic path doesn't call instrument, so no
                        // instrumentable_line_count available. Use raw span for now.
                        concolic_result.total_lines = func.end_line.saturating_sub(func.start_line) + 1;

                        let obs: shatter_core::explorer::ObservationOutput = concolic_result.into();
                        Ok(obs)
                    }
                    Err(shatter_core::orchestrator::ExploreError::Frontend(fe)) => {
                        Err(shatter_core::explorer::ExploreError::Frontend(fe))
                    }
                }
            } else {
                explorer::explore_function(&mut frontend, func, &explore_config, None)
                    .instrument(tracing::info_span!("explore.function"))
                    .await
            };

            match explore_result {
                Ok(result) => {
                    let wall_time = func_start.elapsed();

                    // Harvest interesting inputs into the cross-function pool.
                    // Applies to both concolic and random explorer paths — provenance doesn't matter.
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
                                &func.name,
                                &file_str,
                                behaviors,
                            );
                            let shatter_dir = std::path::Path::new(".shatter");
                            match shatter_core::recorded_mocks::save_recorded_mocks(
                                &mock_file,
                                shatter_dir,
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

                    // Run the Analyze stage to get coverage metrics and eq classes.
                    let analyze_output = {
                        let _pipeline_analyze_span = tracing::info_span!("pipeline.analyze").entered();
                        shatter_core::pipeline::analyze(&result, func)
                    };

                    // Save raw observation data for offline analysis if requested.
                    if let Some(obs_dir) = observe_output {
                        let safe_name = func.name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
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
                                    log::error!("Failed to write observe output for {}: {e}", func.name);
                                } else {
                                    log::info!("Wrote observe output: {}", obs_path.display());
                                }
                            }
                            Err(e) => log::error!("Failed to serialize observe output for {}: {e}", func.name),
                        }
                    }

                    if log::log_enabled!(log::Level::Info) {
                        if log::log_enabled!(log::Level::Trace) {
                            let report = {
                                let _report_span = tracing::info_span!("report.render").entered();
                                explorer::format_exploration_report_verbose(&result)
                            };
                            print!("{report}");
                        } else {
                            let report_opts = ReportOptions {
                                location: Some(format!("{file_str}:{}-{}", func.start_line, func.end_line)),
                                show_perf,
                                wall_time: Some(wall_time),
                                coverage_metrics: Some(analyze_output.coverage_metrics.clone()),
                                style: report_style.clone(),
                            };
                            let report = {
                                let _report_span = tracing::info_span!("report.render").entered();
                                explorer::format_exploration_report(&result, &report_opts)
                            };
                            print!("{report}");
                        }
                        if !mock_symbols.is_empty() {
                            println!("  Mocks used: {}", mock_symbols.join(", "));
                        }
                        if use_concolic {
                            println!("  Explorer: concolic (Z3-backed)");
                        }
                        println!();
                    }

                    // Spec output: use eq classes from analyze stage
                    if show_spec || detect_invariants {
                        let eq_classes = &analyze_output.eq_classes;
                        let location = Some(format!("{file_str}:{}-{}", func.start_line, func.end_line));

                        // Use deep fingerprint (call-graph-aware) for spec output.
                        let fingerprint = deep_fingerprints.get(&func.name).cloned();

                        let spec = {
                            let _spec_span = tracing::info_span!("spec.build").entered();
                            if detect_invariants {
                                shatter_core::spec::build_spec_with_invariants(
                                    &result, eq_classes, location, fingerprint,
                                )
                            } else {
                                shatter_core::spec::build_spec(&result, eq_classes, location, fingerprint)
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
                            print_markdown(&shatter_core::spec::format_spec_markdown(&spec), use_color);
                        }
                    }

                    let behavior_map =
                        BehaviorMap::from_exploration_result(&func.name, &result);
                    if let Some(ref cache) = cache {
                        let cache_result = {
                            let _cache_store_span = tracing::info_span!("cache.store").entered();
                            cache.store(&behavior_map)
                        };
                        if let Err(e) = cache_result {
                            log::warn!("failed to cache behavior map for {}: {e}", func.name);
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
                    log::info!(
                        "  {name}: param {:?} has opaque type {}",
                        reason.param_name, reason.opaque_label
                    );
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

        shutdown_frontend(frontend).await;
    }

    // Print summary footer.
    if header_printed && log::log_enabled!(log::Level::Info) {
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
