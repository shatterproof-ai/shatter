use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shatter_core::analysis_cache::AnalysisCache;
use shatter_core::batch_analyze::{self, FunctionRegistry};
use shatter_core::cache::BehaviorMapCache;
use shatter_core::call_graph::CallGraph;
use shatter_core::config as shatter_config;
use shatter_core::discovery::{self, DiscoveryOptions, Language as DiscoveryLanguage};
use shatter_core::executability;
use shatter_core::explorer;
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::report;
use shatter_core::scan_orchestrator::{self, ScanConfig, SkippedFunction};

use crate::commands::export::emit_test_files;
use crate::helpers::*;

/// Run the scan command: explore multiple functions in dependency order.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_scan(
    directory: &str,
    language_filter: Option<&str>,
    include_patterns: &[String],
    exclude_patterns: &[String],
    changed: bool,
    since: Option<&str>,
    include_untracked: bool,
    all_functions: bool,
    max_depth: Option<usize>,
    max_iterations: u32,
    timeout_total: u64,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    parallelism: usize,
    timeout_per_fn: u64,
    timeout_explore: Option<f64>,
    output_dir: Option<&Path>,
    report_format_str: &str,
    progress: bool,
    emit_tests: Option<&str>,
    dry_run: bool,
    resume: Option<&Path>,
    mock_config: Option<&Path>,
    core_sample_spec: Option<&str>,
    core_sample_seed: Option<u64>,
    batch_spec: Option<&str>,
    stratum_spec: Option<&str>,
    log_level: LogLevel,
    memory_limit: Option<u64>,
    project_dir: Option<&Path>,
    use_color: bool,
    output_format: crate::args::OutputFormat,
    seeds_dir: &Path,
    no_seeds: bool,
    scheduler_policy: shatter_core::scheduler_policy::SchedulerPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    let scan_pool_path = if no_seeds {
        None
    } else if seeds_dir.is_absolute() {
        Some(seeds_dir.join("pool.json"))
    } else {
        Some(std::path::PathBuf::from(directory).join(seeds_dir).join("pool.json"))
    };
    let report_format: report::ReportFormat = report_format_str
        .parse()
        .map_err(|e: String| -> Box<dyn std::error::Error> { e.into() })?;

    // Validate --emit-tests framework early.
    if let Some(framework) = emit_tests
        && framework != "jest" && framework != "vitest" && framework != "gotest"
    {
        return Err(format!(
            "unsupported framework '{framework}': expected 'jest', 'vitest', or 'gotest'"
        )
        .into());
    }

    // Validate --language if specified.
    if let Some(lang) = language_filter
        && lang != "typescript" && lang != "go" && lang != "rust"
    {
        return Err(format!(
            "unsupported language '{lang}': expected 'typescript', 'go', or 'rust'"
        )
        .into());
    }

    // Resolve directory.
    let root = PathBuf::from(directory);
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()).into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("failed to resolve path '{}': {e}", directory))?;

    let project_root_str = resolve_project_root(project_dir, &root);

    if let Some(ref pr) = project_root_str {
        log::debug!("Project root: {pr}");
    }

    // Discover source files.
    let options = DiscoveryOptions {
        include_patterns: include_patterns.to_vec(),
        exclude_patterns: exclude_patterns.to_vec(),
        respect_gitignore: true,
        max_depth,
    };
    let files = if changed || since.is_some() {
        use shatter_core::scm::{ScmProvider, detect_provider};
        let provider = detect_provider(&root)
            .map_err(|e| format!("SCM detection failed: {e}"))?;
        let scm_files = if let Some(base_ref) = since {
            provider.diff_files(&root, base_ref)
        } else {
            provider.changed_files(&root, include_untracked)
        }
        .map_err(|e| format!("SCM file query failed: {e}"))?;

        if scm_files.is_empty() {
            log::info!("No changed files found");
            return Ok(());
        }
        log::info!("SCM reports {} changed file(s)", scm_files.len());

        discovery::filter_file_list(&root, scm_files, &options)
            .map_err(|e| format!("file filtering failed: {e}"))?
    } else {
        discovery::discover_files(&root, &options)
            .map_err(|e| format!("file discovery failed: {e}"))?
    };

    // Filter by language if specified.
    let files: Vec<(PathBuf, DiscoveryLanguage)> = if let Some(lang) = language_filter {
        let target_lang = match lang {
            "typescript" => DiscoveryLanguage::TypeScript,
            "go" => DiscoveryLanguage::Go,
            "rust" => DiscoveryLanguage::Rust,
            _ => unreachable!(),
        };
        files
            .into_iter()
            .filter(|(_, l)| *l == target_lang)
            .collect()
    } else {
        files
    };

    // Filter to languages we can actually analyze (TS, Go).
    let analyzable_files: Vec<(PathBuf, DiscoveryLanguage)> = files
        .into_iter()
        .filter(|(_, lang)| discovery_lang_to_cli_lang(*lang).is_some())
        .collect();

    if analyzable_files.is_empty() {
        log::info!("No supported source files found in {}", root.display());
        return Ok(());
    }

    log::info!(
        "Discovered {} source file(s) in {}",
        analyzable_files.len(),
        root.display(),
    );

    // Spawn frontends for each language.
    let req_timeout = Duration::from_secs(request_timeout);
    let needed_langs: std::collections::HashSet<DiscoveryLanguage> = analyzable_files
        .iter()
        .map(|(_, lang)| *lang)
        .collect();

    let mut frontends: HashMap<DiscoveryLanguage, Frontend> = HashMap::new();
    for lang in &needed_langs {
        let cli_lang = discovery_lang_to_cli_lang(*lang)
            .ok_or_else(|| format!("no frontend for {lang:?}"))?;
        let config = frontend_config(cli_lang, req_timeout, log_level, exec_timeout, build_timeout, memory_limit, None, false)?;
        let frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {lang:?} frontend: {e}")
        })?;
        log::debug!(
            "Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );
        frontends.insert(*lang, frontend);
    }

    // Build analysis cache if caching is enabled.
    let analysis_cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(p) => p.join("analysis"),
            None => AnalysisCache::default_dir(&std::env::current_dir()?),
        };
        Some(AnalysisCache::new(dir).map_err(|e| format!("failed to initialize analysis cache: {e}"))?)
    };

    // Batch analyze all files.
    let registry = batch_analyze::batch_analyze(
        &mut frontends,
        &analyzable_files,
        analysis_cache.as_ref(),
        project_root_str.as_deref(),
    )
    .await
    .map_err(|e| format!("batch analyze failed: {e}"))?;

    log::debug!(
        "Found {} function(s) across {} file(s)",
        registry.len(),
        analyzable_files.len(),
    );

    // Collect analyses and file map from the registry.
    let mut all_analyses = Vec::new();
    let mut file_map: HashMap<String, String> = HashMap::new();

    for entry in registry.entries() {
        // Skip non-exported functions unless --all is specified.
        if !all_functions && !entry.exported {
            continue;
        }

        file_map.insert(
            entry.name.clone(),
            entry.file_path.to_string_lossy().into_owned(),
        );
        all_analyses.push(shatter_core::protocol::FunctionAnalysis {
            name: entry.name.clone(),
            params: entry.params.clone(),
            return_type: entry.return_type.clone(),
            branches: vec![],
            dependencies: entry.dependencies.clone(),
            exported: entry.exported,
            start_line: entry.start_line,
            end_line: entry.end_line,
            literals: vec![],
            crypto_boundaries: vec![],
        });
    }

    // Filter out functions with unexecutable parameter types.
    let mut skipped_for_executability: Vec<SkippedFunction> = Vec::new();
    all_analyses.retain(|func| {
        let reasons = executability::check_executability(&func.params, &[]);
        if reasons.is_empty() {
            true
        } else {
            let reason = reasons
                .iter()
                .map(|r| format!("param {:?} has opaque type {}", r.param_name, r.opaque_label))
                .collect::<Vec<_>>()
                .join("; ");
            skipped_for_executability.push(SkippedFunction {
                function_name: func.name.clone(),
                reason,
                category: shatter_core::scan_orchestrator::SkipCategory::Expected,
            });
            false
        }
    });

    if !skipped_for_executability.is_empty() {
        log::info!(
            "Skipped {} function(s) (unexecutable parameter types):",
            skipped_for_executability.len()
        );
        for skip in &skipped_for_executability {
            log::info!("  {}: {}", skip.function_name, skip.reason);
        }
    }

    // Parse --stratum spec early so core-sample budget operates on the
    // stratum-filtered set (not the full population).
    let parsed_stratum = if let Some(spec_str) = stratum_spec {
        Some(
            shatter_core::stratum::parse_stratum_spec(spec_str)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    // When both --stratum and --core-sample are set, pre-filter analyses by
    // stratum so the core-sample budget is computed against the narrowed set.
    let stratum_pre_applied = if let (Some(spec), Some(_)) = (&parsed_stratum, &core_sample_spec) {
        let cg = CallGraph::from_registry(&registry);
        let layers = cg.topological_layers();
        let max_layer = if layers.is_empty() { 0 } else { layers.len() - 1 };
        let range = shatter_core::stratum::resolve_range(spec, max_layer)?;
        // filter_layers returns qualified names (file::func); extract bare names.
        let selected: std::collections::HashSet<String> =
            shatter_core::stratum::filter_layers(&layers, &range)
                .into_iter()
                .flat_map(|(_, funcs)| funcs.iter().cloned())
                .map(|qn| {
                    // Qualified names are "file_path::name"; extract the bare name.
                    qn.rsplit_once("::").map_or(qn.clone(), |(_, name)| name.to_string())
                })
                .collect();
        let before = all_analyses.len();
        all_analyses.retain(|a| selected.contains(&a.name));
        log::info!(
            "Stratum filter: {} of {} function(s) in selected layers",
            all_analyses.len(),
            before,
        );
        true
    } else {
        false
    };

    // Apply core sample selection if --core-sample is set.
    let mut effective_batch_index: Option<usize> = None;
    let mut total_scope_functions: usize = all_analyses.len();
    let sampling_context = if let Some(spec) = core_sample_spec {
        let budget = shatter_core::core_sample::parse_sample_budget(spec)
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let cg = CallGraph::from_registry(&registry);
        let seed = core_sample_seed
            .unwrap_or_else(|| shatter_core::core_sample::default_seed(directory));
        let cs_config = shatter_core::core_sample::CoreSampleConfig {
            budget,
            seed,
            scan_root: directory.to_string(),
        };
        let entries: Vec<shatter_core::batch_analyze::FunctionEntry> = registry
            .entries()
            .iter()
            .filter(|e| all_analyses.iter().any(|a| a.name == e.name))
            .cloned()
            .collect();
        // Select using batch mode or standard core sample.
        let result = if let Some(batch_str) = batch_spec {
            let parsed_batch = shatter_core::core_sample::parse_batch_spec(batch_str)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let batch_index = match parsed_batch {
                shatter_core::core_sample::BatchSpec::Single(idx) => idx,
                shatter_core::core_sample::BatchSpec::Next => {
                    let cache_for_detect = if no_cache {
                        None
                    } else {
                        let dir = match cache_dir {
                            Some(d) => d.to_path_buf(),
                            None => shatter_core::cache::BehaviorMapCache::default_dir(
                                &std::env::current_dir()?,
                            ),
                        };
                        shatter_core::cache::BehaviorMapCache::new(dir).ok()
                    };
                    shatter_core::core_sample::detect_next_batch(
                        &entries,
                        &cs_config,
                        cache_for_detect.as_ref(),
                    )
                }
                shatter_core::core_sample::BatchSpec::Range(start, end) => {
                    log::info!(
                        "Batch range {start}-{end}: running batch {start} \
                         (run subsequent batches with --batch {}..{})",
                        start + 1, end,
                    );
                    start
                }
            };
            effective_batch_index = Some(batch_index);
            log::info!("Using batch {batch_index} of core sample");
            shatter_core::core_sample::select_batch(&entries, &cg, &cs_config, batch_index)
        } else {
            shatter_core::core_sample::select_core_sample(&entries, &cg, &cs_config)
        };
        let included = result.all_included();
        let before = all_analyses.len();
        total_scope_functions = before;
        // included contains qualified names (file_path::name); match using file_map.
        all_analyses.retain(|a| {
            if let Some(file) = file_map.get(&a.name) {
                let qn = format!("{}::{}", file, a.name);
                included.contains(&qn)
            } else {
                // No file mapping — fall back to bare name match.
                included.contains(&a.name)
            }
        });
        log::info!(
            "Core sample: selected {} of {} function(s) ({} sampled + {} dependency closure)",
            included.len(),
            before,
            result.selected.len(),
            result.dependency_closure.len(),
        );
        Some(scan_orchestrator::SamplingContext {
            total_functions: before,
            sampled_functions: result.selected.len(),
            closure_functions: result.dependency_closure.len(),
            strata_summary: result.strata_summary,
        })
    } else {
        if batch_spec.is_some() {
            log::warn!("--batch requires --core-sample; ignoring --batch");
        }
        None
    };

    if all_analyses.is_empty() {
        log::info!("No functions found to scan.");
        for frontend in frontends.into_values() {
            shutdown_frontend(frontend).await;
        }
        return Ok(());
    }

    // Shut down analysis frontends before starting parallel exploration.
    for frontend in frontends.into_values() {
        shutdown_frontend(frontend).await;
    }

    // Resolve effective parallelism: 0 means auto-detect (CPU count).
    let effective_parallelism = if parallelism == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        parallelism
    };

    // Dry run: show the full exploration plan without executing.
    if dry_run {
        let scan_config = ScanConfig {
            max_iterations_per_function: max_iterations,
            seed: None,
            file_map: file_map.clone(),
            parallelism: effective_parallelism,
            timeout_per_fn: Duration::from_secs(timeout_per_fn),
            cache: None,
            stratum: if stratum_pre_applied { None } else { parsed_stratum.clone() },
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: scan_pool_path.clone(),
            project_root: project_root_str.clone(),
            config_dir: Some(std::path::PathBuf::from(directory)),
            timeout_explore: timeout_explore.map(Duration::from_secs_f64),
            setup_manager: None,
            policy: scheduler_policy,
        };
        let plan = scan_orchestrator::format_dry_run_plan(
            &all_analyses,
            &skipped_for_executability,
            &scan_config,
        )
        .map_err(|e| format!("failed to build dry-run plan: {e}"))?;
        print!("{plan}");
        return Ok(());
    }

    log::debug!(
        "Scanning {} function(s) in dependency order ({} worker(s), {}s/fn)...",
        all_analyses.len(),
        effective_parallelism,
        timeout_per_fn,
    );

    let cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(d) => d.to_path_buf(),
            None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
        };
        Some(std::sync::Arc::new(
            BehaviorMapCache::new(dir).map_err(|e| format!("failed to initialize cache: {e}"))?,
        ))
    };

    // We need a single frontend config for parallel_scan. Pick from the first language.
    let first_lang = needed_langs.iter().next().copied().unwrap();
    let cli_lang = discovery_lang_to_cli_lang(first_lang)
        .ok_or_else(|| format!("no frontend for {first_lang:?}"))?;
    let fe_config = frontend_config(cli_lang, req_timeout, log_level, exec_timeout, build_timeout, memory_limit, None, false)?;

    // Load mock overrides from --mock-config (or .shatter/config.yaml defaults).
    let mock_overrides = if let Some(mc_path) = mock_config {
        let cfg = shatter_config::parse_config(mc_path)
            .map_err(|e| format!("failed to load mock config: {e}"))?;
        cfg.defaults.mocks.unwrap_or_default()
    } else {
        // Try loading from the scanned directory's .shatter/config.yaml
        let config_path = PathBuf::from(directory).join(".shatter").join("config.yaml");
        if config_path.exists() {
            shatter_config::parse_config(&config_path)
                .ok()
                .and_then(|cfg| cfg.defaults.mocks)
                .unwrap_or_default()
        } else {
            HashMap::new()
        }
    };

    let scan_config = ScanConfig {
        max_iterations_per_function: max_iterations,
        seed: None,
        file_map,
        parallelism: effective_parallelism,
        timeout_per_fn: Duration::from_secs(timeout_per_fn),
        cache,
        stratum: if stratum_pre_applied { None } else { parsed_stratum },
        mock_overrides,
        resume_path: resume.map(|p| p.to_path_buf()),
        timeout_total: if timeout_total == 0 { None } else { Some(Duration::from_secs(timeout_total)) },
        pool_path: scan_pool_path,
        project_root: project_root_str.clone(),
        config_dir: Some(std::path::PathBuf::from(directory)),
        timeout_explore: timeout_explore.map(Duration::from_secs_f64),
        setup_manager: None,
        policy: scheduler_policy,
    };

    let scan_start = Instant::now();
    let total_functions = all_analyses.len();

    log::info!(
        "Scanning {} function(s) in dependency order...",
        total_functions,
    );

    match scan_orchestrator::parallel_scan(&fe_config, &all_analyses, &scan_config).await {
        Ok(mut result) => {
            result.sampling = sampling_context;
            let elapsed = scan_start.elapsed();

            for (i, fr) in result.function_results.iter().enumerate() {
                if progress {
                    let elapsed_ms = elapsed.as_millis() as u64;
                    let event = report::ProgressEvent::new(
                        &fr.function_name,
                        i + 1,
                        total_functions,
                        elapsed_ms,
                    );
                    if let Some(json) = event.to_json() {
                        eprintln!("{json}");
                    }
                } else {
                    log::info!(
                        "[{}/{}] {} ({:.1}s elapsed)",
                        i + 1,
                        total_functions,
                        fr.function_name,
                        elapsed.as_secs_f64(),
                    );
                }
            }

            if output_format == crate::args::OutputFormat::Md {
                let view = crate::render::scan_view(&result);
                print_markdown(&crate::render::render_scan(&view), use_color);
            } else {
                print_markdown(&scan_orchestrator::format_parallel_scan_report(&result), use_color);
            }

            // Record batch state and print cumulative progress.
            let batch_state = if let Some(batch_idx) = effective_batch_index {
                let batch_state_path = PathBuf::from(directory)
                    .join(".shatter")
                    .join("batch-state.json");

                let file_paths: Vec<&str> = scan_config
                    .file_map
                    .values()
                    .map(|s| s.as_str())
                    .collect();
                let scan_id =
                    shatter_core::checkpoint::ScanCheckpoint::compute_scan_id(&file_paths);

                let mut state = match shatter_core::batch_state::BatchState::load(&batch_state_path) {
                    Ok(Some(s)) if s.scan_id == scan_id => s,
                    Ok(Some(_)) => {
                        log::info!("batch state scan_id mismatch, starting fresh");
                        shatter_core::batch_state::BatchState::new(
                            scan_id,
                            total_scope_functions,
                        )
                    }
                    Ok(None) => shatter_core::batch_state::BatchState::new(
                        scan_id,
                        total_scope_functions,
                    ),
                    Err(e) => {
                        log::warn!("failed to load batch state: {e}, starting fresh");
                        shatter_core::batch_state::BatchState::new(
                            "unknown".to_string(),
                            total_scope_functions,
                        )
                    }
                };

                let summary =
                    shatter_core::batch_state::BatchSummary::from_scan_result(batch_idx, &result);
                state.record_batch(summary);

                if let Err(e) = state.save(&batch_state_path) {
                    log::warn!("failed to save batch state: {e}");
                }

                print_markdown(
                    &shatter_core::batch_state::format_cumulative_batch_section(&state, batch_idx),
                    use_color,
                );

                Some(state)
            } else {
                None
            };

            let report_dir = output_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./shatter-report/"));
            let scan_report = report::generate_report(
                &result,
                &scan_config.file_map,
                batch_state.as_ref(),
            );

            match report_format {
                report::ReportFormat::Json => {
                    match report::write_report(&scan_report, &report_dir) {
                        Ok(path) => log::info!("Wrote JSON report to {}", path.display()),
                        Err(e) => log::error!("Failed to write JSON report: {e}"),
                    }
                }
                report::ReportFormat::Markdown => {
                    match report::write_markdown_report(&scan_report, &report_dir) {
                        Ok(path) => log::info!("Wrote markdown report to {}", path.display()),
                        Err(e) => log::error!("Failed to write markdown report: {e}"),
                    }
                }
                report::ReportFormat::Both => {
                    match report::write_report(&scan_report, &report_dir) {
                        Ok(path) => log::info!("Wrote JSON report to {}", path.display()),
                        Err(e) => log::error!("Failed to write JSON report: {e}"),
                    }
                    match report::write_markdown_report(&scan_report, &report_dir) {
                        Ok(path) => log::info!("Wrote markdown report to {}", path.display()),
                        Err(e) => log::error!("Failed to write markdown report: {e}"),
                    }
                }
            }

            // Emit test files if --emit-tests was specified.
            if let Some(framework) = emit_tests {
                let tests_dir = output_dir
                    .map(PathBuf::from)
                    .unwrap_or_else(|| report_dir.clone());

                if let Err(e) = emit_test_files(&result, &scan_config.file_map, framework, &tests_dir) {
                    log::error!("Failed to emit test files: {e}");
                }
            }
        }
        Err(e) => {
            log::error!("Scan error: {e}");
        }
    }

    Ok(())
}

/// Print a markdown-style summary report to stdout, rendered with termimad when `use_color` is true.
#[allow(clippy::too_many_arguments)]
pub(crate) fn print_summary_report(
    root: &Path,
    ts_files: &[PathBuf],
    go_files: &[PathBuf],
    rs_files: &[PathBuf],
    registry: &FunctionRegistry,
    call_graph: &CallGraph,
    layers: &[Vec<String>],
    cycles: &[Vec<String>],
    exploration_results: &[(String, explorer::ObservationOutput)],
    elapsed: Duration,
    use_color: bool,
) {
    use std::fmt::Write;
    let mut md = String::new();

    writeln!(md, "# Shatter Run Report").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "**Repository**: {}", root.display()).unwrap();
    writeln!(md, "**Elapsed**: {:.1}s", elapsed.as_secs_f64()).unwrap();
    writeln!(md).unwrap();

    // Files discovered
    let total_files = ts_files.len() + go_files.len() + rs_files.len();
    writeln!(md, "## Files Discovered").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "| Language | Files |").unwrap();
    writeln!(md, "|----------|-------|").unwrap();
    if !ts_files.is_empty() {
        writeln!(md, "| TypeScript | {} |", ts_files.len()).unwrap();
    }
    if !go_files.is_empty() {
        writeln!(md, "| Go | {} |", go_files.len()).unwrap();
    }
    if !rs_files.is_empty() {
        writeln!(md, "| Rust | {} |", rs_files.len()).unwrap();
    }
    writeln!(md, "| **Total** | **{total_files}** |").unwrap();
    writeln!(md).unwrap();

    // Functions analyzed
    let total_branches: usize = registry.entries().iter().map(|e| e.branch_count).sum();
    writeln!(md, "## Functions Analyzed").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "- **Total functions**: {}", registry.len()).unwrap();
    writeln!(md, "- **Total branches**: {total_branches}").unwrap();
    writeln!(md, "- **Exported functions**: {}", registry.exported_functions().len()).unwrap();
    writeln!(md).unwrap();

    // Call graph summary
    writeln!(md, "## Call Graph").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "- **Nodes**: {}", call_graph.node_count()).unwrap();
    writeln!(md, "- **Edges**: {}", call_graph.edge_count()).unwrap();
    writeln!(md, "- **Topological layers**: {}", layers.len()).unwrap();
    writeln!(md, "- **Cycles**: {}", cycles.len()).unwrap();
    if !cycles.is_empty() {
        writeln!(md).unwrap();
        for (i, cycle) in cycles.iter().enumerate() {
            writeln!(md, "  Cycle {}: {}", i + 1, cycle.join(" <-> ")).unwrap();
        }
    }
    writeln!(md).unwrap();

    // Exploration results
    if !exploration_results.is_empty() {
        writeln!(md, "## Exploration Results").unwrap();
        writeln!(md).unwrap();
        writeln!(md, "| Function | Paths | Lines Covered | Coverage |").unwrap();
        writeln!(md, "|----------|-------|---------------|----------|").unwrap();

        let mut total_paths = 0;
        let mut total_covered = 0;
        let mut total_lines = 0u32;

        for (qname, result) in exploration_results {
            let pct = if result.total_lines > 0 {
                (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            writeln!(
                md,
                "| {qname} | {} | {}/{} | {pct:.0}% |",
                result.unique_paths, result.lines_covered, result.total_lines
            ).unwrap();
            total_paths += result.unique_paths;
            total_covered += result.lines_covered;
            total_lines += result.total_lines;
        }

        let total_pct = if total_lines > 0 {
            (total_covered as f64 / total_lines as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        writeln!(
            md,
            "| **Total** | **{total_paths}** | **{total_covered}/{total_lines}** | **{total_pct:.0}%** |",
        ).unwrap();
        writeln!(md).unwrap();
    }

    print_markdown(&md, use_color);
}

/// Write analysis-only report to output directory.
pub(crate) fn write_analysis_report(
    dir: &Path,
    registry: &FunctionRegistry,
    call_graph: &CallGraph,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("failed to create output dir '{}': {e}", dir.display()))?;

    let summary_path = dir.join("analysis-summary.md");
    let mut content = String::new();
    content.push_str("# Analysis Summary\n\n");
    content.push_str(&format!("- Functions: {}\n", registry.len()));
    content.push_str(&format!("- Call graph edges: {}\n", call_graph.edge_count()));
    content.push_str(&format!("- Cycles: {}\n", call_graph.cycle_groups().len()));
    content.push_str("\n## Functions\n\n");

    for entry in registry.entries() {
        content.push_str(&format!(
            "- **{}** ({}): {} branches, {} deps\n",
            entry.name,
            relativize_path(&entry.file_path, root),
            entry.branch_count,
            entry.dependencies.len(),
        ));
    }

    std::fs::write(&summary_path, &content)
        .map_err(|e| format!("failed to write summary: {e}"))?;
    log::info!("Wrote analysis report to {}", summary_path.display());

    Ok(())
}
