use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shatter_core::batch_analyze;
use shatter_core::call_graph::CallGraph;
use shatter_core::config as shatter_config;
use shatter_core::discovery::{self, DiscoveryOptions, Language as DiscoveryLanguage};
use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::run_manifest::{self, RunManifest};
use shatter_core::status_export::{
    StatusArtifactLink, StatusExportInput, StatusFileInput, StatusFileStatus, StatusGateDecision,
    StatusLineWeightedFailureImpact, StatusReportValidity, StatusRollupInput, StatusTargetInput,
    StatusTargetOutcome, StatusTargetValidityImpact, StatusValidityReason, write_run_status_json,
};

/// Resolved scope settings from `shatter.config.json` for the `run` command.
///
/// The `run` command intentionally has no CLI flags for include/exclude or
/// language; it discovers source files from the project root and is meant to
/// honor whatever scope the project author has declared in
/// `shatter.config.json` (parity with `scan` — see str-mg2d).
#[derive(Debug, Clone, Default)]
pub(crate) struct RunScope {
    pub(crate) options: DiscoveryOptions,
    pub(crate) language_filter: Option<String>,
    pub(crate) coverage_budget_gates: shatter_core::config::CoverageBudgetGates,
}

/// Build a [`RunScope`] from the project config in `root` (if any).
///
/// Falls back to defaults when `shatter.config.json` is absent or unreadable.
/// On parse failure, logs a warning and returns defaults so `run` still
/// proceeds — a malformed config should not silently expand scope, but the
/// existing `scan` wiring also degrades to defaults on parse error and we
/// keep parity with that behavior.
pub(crate) fn run_scope_from_project_config(root: &Path) -> RunScope {
    let project_cfg = match shatter_config::load_project_config(root) {
        Ok(cfg) => cfg,
        Err(e) => {
            log::warn!("Failed to load project config from {}: {e}", root.display());
            None
        }
    };

    let Some(cfg) = project_cfg else {
        return RunScope::default();
    };

    RunScope {
        options: DiscoveryOptions {
            include_patterns: cfg.include.clone(),
            exclude_patterns: cfg.exclude.clone(),
            respect_gitignore: true,
            max_depth: cfg.max_depth,
        },
        language_filter: cfg.language.clone(),
        coverage_budget_gates: cfg.coverage_budget_gates.unwrap_or_default(),
    }
}

/// Filter discovered files by the project-configured language, if any.
///
/// Unknown language strings are ignored (no filter applied); the project
/// config schema does not validate the language string, and `run` should not
/// panic on bad config — it should still produce a usable report. A warning
/// is emitted so the misconfiguration is visible.
pub(crate) fn apply_language_filter(
    files: Vec<(PathBuf, DiscoveryLanguage)>,
    language_filter: Option<&str>,
) -> Vec<(PathBuf, DiscoveryLanguage)> {
    let Some(lang) = language_filter else {
        return files;
    };
    let target = match lang {
        "typescript" => DiscoveryLanguage::TypeScript,
        "go" => DiscoveryLanguage::Go,
        "rust" => DiscoveryLanguage::Rust,
        other => {
            log::warn!(
                "shatter.config.json language='{other}' is not a recognized language; ignoring filter"
            );
            return files;
        }
    };
    files.into_iter().filter(|(_, l)| *l == target).collect()
}

/// CLI coverage-budget overrides for `shatter run`.
#[derive(Debug, Clone, Default)]
pub(crate) struct CoverageBudgetGateOverrides {
    pub(crate) min_source_representation_percent: Option<f64>,
    pub(crate) max_failed_span_percent: Option<f64>,
    pub(crate) max_unsupported_span_percent: Option<f64>,
    pub(crate) fail_on_stale_source_set: bool,
    pub(crate) fail_on_missing_artifacts: bool,
    pub(crate) fail_on_low_report_validity: bool,
}

impl CoverageBudgetGateOverrides {
    fn apply_to(
        self,
        mut gates: shatter_core::config::CoverageBudgetGates,
    ) -> shatter_core::config::CoverageBudgetGates {
        if self.min_source_representation_percent.is_some() {
            gates.min_source_representation_percent = self.min_source_representation_percent;
        }
        if self.max_failed_span_percent.is_some() {
            gates.max_failed_span_percent = self.max_failed_span_percent;
        }
        if self.max_unsupported_span_percent.is_some() {
            gates.max_unsupported_span_percent = self.max_unsupported_span_percent;
        }
        if self.fail_on_stale_source_set {
            gates.fail_on_stale_source_set = Some(true);
        }
        if self.fail_on_missing_artifacts {
            gates.fail_on_missing_artifacts = Some(true);
        }
        if self.fail_on_low_report_validity {
            gates.fail_on_low_report_validity = Some(true);
        }
        gates
    }
}

use crate::commands::scan::{print_summary_report, write_analysis_report};
use crate::helpers::*;

/// Run the run command: discover, analyze, build call graph, explore, and report.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_run(
    path: &str,
    output_dir: Option<&Path>,
    max_iterations: u32,
    timeout: u64,
    analyze_only: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
    log_level: LogLevel,
    memory_limit: Option<u64>,
    project_dir: Option<&Path>,
    use_color: bool,
    coverage_budget_overrides: CoverageBudgetGateOverrides,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    // Check for URL input (not yet supported)
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("git@") {
        return Err(
            "URL/git clone input is not yet supported. Please provide a local directory path."
                .into(),
        );
    }

    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()).into());
    }

    let root = root
        .canonicalize()
        .map_err(|e| format!("failed to resolve path '{}': {e}", path))?;

    // Per-run scan id used when writing the run-level JSON summary and any
    // manifest artifact alongside it. Time-based + nanosecond counter is
    // enough resolution for "two `run` invocations don't collide" without
    // pulling in a uuid dependency.
    let scan_id = format!(
        "run-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    let project_root_str = resolve_project_root(project_dir, &root);

    if let Some(ref pr) = project_root_str {
        log::debug!("Project root: {pr}");
    }
    log::debug!("Shatter run: {}", root.display());

    // Step 1: Discover files
    //
    // str-mg2d: honor `shatter.config.json` scope (include/exclude/max_depth/
    // language) so `run` analyzes the same file set as `scan`. The `run`
    // command exposes no CLI scope flags, so the project config is the sole
    // source of scope filtering; without this, fixture trees excluded by the
    // project author leak into discovery and produce noisy preflight warnings.
    log::debug!("Discovering source files...");
    let scope = run_scope_from_project_config(&root);
    let coverage_budget_gates =
        coverage_budget_overrides.apply_to(scope.coverage_budget_gates.clone());
    if !scope.options.include_patterns.is_empty()
        || !scope.options.exclude_patterns.is_empty()
        || scope.options.max_depth.is_some()
        || scope.language_filter.is_some()
    {
        log::debug!(
            "Applying project scope: include={:?} exclude={:?} max_depth={:?} language={:?}",
            scope.options.include_patterns,
            scope.options.exclude_patterns,
            scope.options.max_depth,
            scope.language_filter,
        );
    }
    let files = discovery::discover_files(&root, &scope.options)
        .map_err(|e| format!("file discovery failed: {e}"))?;
    let files = apply_language_filter(files, scope.language_filter.as_deref());

    if files.is_empty() {
        log::info!("No supported source files found in {}", root.display());
        // Even an empty discovery is a valid run — write a JSON summary
        // (str-jeen.17) so the denominator (zero) is on disk for tooling.
        if let Some(dir) = output_dir {
            let manifest = run_manifest::capture(&scan_id, &scope_hash(&scope), &[], Some(&root));
            run_manifest::write_manifest(dir, &manifest);
            let mut run_summary = build_run_summary(&scan_id, &manifest, 0, 0, 0, &[]);
            run_summary.gate_decisions =
                evaluate_coverage_budget_gates(&run_summary, &coverage_budget_gates);
            write_run_summary_json(dir, &run_summary)?;
            write_run_status_export(
                dir,
                &manifest,
                &[("run_summary", dir.join(RUN_SUMMARY_FILENAME))],
                &[],
                &[],
                &run_status_rollup_input_from_summary(&run_summary),
            )?;
            if coverage_budget_failed(&run_summary.gate_decisions) {
                return Err("coverage budget gates failed".into());
            }
        }
        return Ok(());
    }

    // str-jeen.17: capture a run-start manifest snapshot of the discovered
    // source set *before* any analyze/explore work runs. The
    // `selected_source_files` / `selected_source_lines` denominators in
    // the run JSON come from this snapshot, so they reflect the whole
    // source set independent of how many functions later get discovered,
    // attempted, or completed.
    let manifest_paths: Vec<String> = files
        .iter()
        .map(|(p, _)| {
            p.strip_prefix(&root)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let run_manifest =
        run_manifest::capture(&scan_id, &scope_hash(&scope), &manifest_paths, Some(&root));
    if let Some(dir) = output_dir {
        run_manifest::write_manifest(dir, &run_manifest);
    }

    // Group by language for reporting
    let mut ts_files = Vec::new();
    let mut go_files = Vec::new();
    let mut rs_files = Vec::new();
    for (p, lang) in &files {
        match lang {
            DiscoveryLanguage::TypeScript => ts_files.push(p.clone()),
            DiscoveryLanguage::Go => go_files.push(p.clone()),
            DiscoveryLanguage::Rust => rs_files.push(p.clone()),
        }
    }

    log::debug!("Found {} file(s):", files.len());
    if !ts_files.is_empty() {
        log::debug!("  TypeScript: {}", ts_files.len());
    }
    if !go_files.is_empty() {
        log::debug!("  Go: {}", go_files.len());
    }
    if !rs_files.is_empty() {
        log::debug!("  Rust: {}", rs_files.len());
    }

    // Filter to languages we can actually analyze (TS, Go)
    let analyzable_files: Vec<(PathBuf, DiscoveryLanguage)> = files
        .into_iter()
        .filter(|(_, lang)| discovery_lang_to_cli_lang(*lang).is_some())
        .collect();

    if analyzable_files.is_empty() {
        log::info!("No analyzable source files found (supported: TypeScript, Go, Rust).");
        return Ok(());
    }

    // Step 2: Spawn frontends for each language
    let req_timeout = Duration::from_secs(request_timeout);
    let mut frontends: HashMap<DiscoveryLanguage, Frontend> = HashMap::new();

    let needed_langs: std::collections::HashSet<DiscoveryLanguage> =
        analyzable_files.iter().map(|(_, lang)| *lang).collect();

    for lang in &needed_langs {
        let cli_lang =
            discovery_lang_to_cli_lang(*lang).ok_or_else(|| format!("no frontend for {lang:?}"))?;
        let mut config = frontend_config(
            cli_lang,
            req_timeout,
            log_level,
            exec_timeout,
            build_timeout,
            memory_limit,
            None,
            false,
            release,
        )?;
        apply_project_storage(&mut config, project_root_str.as_deref());
        let frontend = Frontend::spawn(&config)
            .await
            .map_err(|e| format!("failed to spawn {lang:?} frontend: {e}"))?;
        log::debug!(
            "Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );
        frontends.insert(*lang, frontend);
    }

    // Step 3: Batch analyze
    log::debug!("Analyzing {} file(s)...", analyzable_files.len());
    // `run` does not use the analysis cache, so analyzer versions are
    // irrelevant here — pass an empty map (str-2cihu).
    let registry = batch_analyze::batch_analyze(
        &mut frontends,
        &analyzable_files,
        None,
        &std::collections::HashMap::new(),
        project_root_str.as_deref(),
    )
    .await
    .map_err(|e| format!("batch analyze failed: {e}"))?;

    let total_functions = registry.len();
    let total_branches: usize = registry.entries().iter().map(|e| e.branch_count).sum();

    log::debug!(
        "Found {} function(s) with {} total branch(es)",
        total_functions,
        total_branches
    );

    if total_functions == 0 {
        log::info!("No functions found to explore.");
        // Honest denominator (str-jeen.17): even when discovery returns
        // zero functions, the source-set denominators must reflect the
        // selected files in the manifest snapshot, not collapse to zero.
        if let Some(dir) = output_dir {
            let run_summary = build_run_summary(&scan_id, &run_manifest, 0, 0, 0, &[]);
            write_run_summary_json(dir, &run_summary)?;
            let no_failures = HashMap::new();
            let status_files =
                run_status_file_inputs_from_registry(&registry, &root, &[], &no_failures);
            write_run_status_export(
                dir,
                &run_manifest,
                &[("run_summary", dir.join(RUN_SUMMARY_FILENAME))],
                &status_files,
                &[],
                &run_status_rollup_input_from_summary(&run_summary),
            )?;
        }
        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 4: Build call graph
    log::debug!("Building call graph...");
    let call_graph = CallGraph::from_registry(&registry);
    let layers = call_graph.topological_layers();
    let cycles = call_graph.cycle_groups();

    log::debug!(
        "{} node(s), {} edge(s), {} layer(s), {} cycle(s)",
        call_graph.node_count(),
        call_graph.edge_count(),
        layers.len(),
        cycles.len(),
    );

    if analyze_only {
        // str-z06h: `shatter run` does not filter by visibility — it includes
        // every analyzed function. Pass `all_functions = true` so the summary
        // does not falsely suggest unexported targets were omitted.
        print_summary_report(
            &root,
            &ts_files,
            &go_files,
            &rs_files,
            &registry,
            &call_graph,
            &layers,
            &cycles,
            &[],
            start.elapsed(),
            use_color,
            true,
        );

        if let Some(dir) = output_dir {
            write_analysis_report(dir, &registry, &call_graph, &root)?;
            // analyze-only still produces a JSON summary so the
            // selected-source denominator is recorded even without
            // exploration outcomes.
            let spans = registry_spans(&registry, &root);
            let run_summary = build_run_summary(&scan_id, &run_manifest, 0, 0, 0, &spans);
            write_run_summary_json(dir, &run_summary)?;
            let no_failures = HashMap::new();
            let status_files =
                run_status_file_inputs_from_registry(&registry, &root, &[], &no_failures);
            let status_targets = run_status_target_inputs_from_registry(
                &registry,
                &root,
                dir,
                &[],
                &no_failures,
                Some("analyze-only run did not explore target"),
            );
            write_run_status_export(
                dir,
                &run_manifest,
                &[
                    ("analysis_report", dir.join("analysis-summary.md")),
                    ("run_summary", dir.join(RUN_SUMMARY_FILENAME)),
                ],
                &status_files,
                &status_targets,
                &run_status_rollup_input_from_summary(&run_summary),
            )?;
        }

        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 5: Explore in dependency order (layer by layer)
    log::debug!("Exploring functions in dependency order...");

    let mut exploration_results: Vec<(String, explorer::ObservationOutput)> = Vec::new();
    let mut exploration_failures: HashMap<String, String> = HashMap::new();

    for (layer_idx, layer) in layers.iter().enumerate() {
        log::debug!("Layer {} ({} function(s)):", layer_idx, layer.len());

        for qualified_name in layer {
            let entry = match registry.get(qualified_name) {
                Some(e) => e,
                None => continue,
            };

            // Determine the language of this file
            let ext = entry
                .file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let disc_lang = match DiscoveryLanguage::from_extension(ext) {
                Some(l) => l,
                None => continue,
            };

            let frontend = match frontends.get_mut(&disc_lang) {
                Some(f) => f,
                None => continue,
            };

            // Analyze to get FunctionAnalysis (needed for explore_function)
            let analyze_response = frontend
                .send(ProtoCommand::Analyze {
                    file: entry.file_path.to_string_lossy().into_owned(),
                    function: Some(entry.name.clone()),
                    project_root: project_root_str.clone(),
                    execution_profile: None,
                })
                .await
                .map_err(|e| format!("analyze failed for {qualified_name}: {e}"))?;

            let func_analysis = match &analyze_response.result {
                ResponseResult::Analyze { functions } => {
                    functions.iter().find(|f| f.name == entry.name).cloned()
                }
                _ => None,
            };

            let Some(func_analysis) = func_analysis else {
                log::warn!("Skipping {}: could not get analysis", entry.name);
                exploration_failures
                    .insert(qualified_name.clone(), "could not get analysis".to_string());
                continue;
            };

            log::debug!("Exploring {}...", entry.name);

            let explore_config = ExploreConfig {
                file: entry.file_path.to_string_lossy().into_owned(),
                execution_profile: None,
                max_iterations: Some(max_iterations),
                observer_pool: 1,
                observer_frontend_config: None,
                candidate_queue_capacity: None,
                seed: None,
                mocks: vec![],
                mock_params: vec![],
                setup_file: None,
                setup_level: shatter_core::protocol::SetupLevel::Function,
                value_sources: vec![],
                capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
                user_seeds: vec![],
                candidate_inputs: vec![],
                pool_seeds: vec![],
                project_root: project_root_str.clone(),
                loop_buckets: explorer::LoopBuckets::default(),
                timeout_explore: None,
                meta_config: shatter_core::strategy::MetaConfig::default(),
                shrink_budget: shatter_core::orchestrator::DEFAULT_SHRINK_BUDGET,
                isolation: shatter_core::explorer::IsolationMode::None,
                capture_side_effects: false,
                budget_surplus: None,
                claim_policy: shatter_core::scan_orchestrator::ClaimPolicy::default(),
                planner: None,
                default_execute_plan: None,
            prepare_id_override: None,
            };

            match explorer::explore_function(frontend, &func_analysis, &explore_config, None, None)
                .await
            {
                Ok(result) => {
                    log::debug!(
                        "{}: {} path(s), {}/{} lines",
                        entry.name,
                        result.unique_paths,
                        result.lines_covered,
                        result.total_lines
                    );
                    exploration_results.push((qualified_name.clone(), result));
                }
                Err(e) => {
                    log::debug!("{}: error: {e}", entry.name);
                    exploration_failures.insert(qualified_name.clone(), e.to_string());
                }
            }

            // Check overall timeout
            if start.elapsed() > Duration::from_secs(timeout) {
                log::warn!("Timeout reached ({timeout}s), stopping exploration.");
                break;
            }
        }

        if start.elapsed() > Duration::from_secs(timeout) {
            break;
        }
    }

    println!();

    // str-jeen.5: build the run summary, run a per-function .md
    // self-check (when `output_dir` is set and reports exist on disk
    // already from a prior partial write — none expected at this
    // point but the call is cheap), and a manifest diff so we can
    // print the validity verdict above the per-function detail.
    // The final summary (with potentially-revised reasons) is
    // written to `run.json` after `write_run_report` below.
    let attempted = total_functions;
    let completed_count = exploration_results.len();
    let failed_count = attempted.saturating_sub(completed_count);
    let representation_spans = registry_representation_spans(
        &registry,
        &root,
        &exploration_results,
        &exploration_failures,
    );
    let mut run_summary = build_run_summary_with_representation(
        &scan_id,
        &run_manifest,
        completed_count,
        failed_count,
        0,
        &representation_spans,
    );
    let source_diff = run_manifest::diff_against(&run_manifest, &manifest_paths);
    let (validity_top, reasons_top) = classify_validity(&run_summary, Some(&source_diff), &[]);
    run_summary.report_validity = validity_top;
    run_summary.validity_reasons = reasons_top.clone();
    let validity_md = render_validity_markdown(validity_top, &reasons_top);
    print_markdown(&validity_md, use_color);

    // Step 6: Print summary report. `shatter run` is the full-exploration
    // path and does not filter by visibility (see analyze_only call above);
    // pass `all_functions = true` so the unexported-omission hint doesn't fire.
    print_summary_report(
        &root,
        &ts_files,
        &go_files,
        &rs_files,
        &registry,
        &call_graph,
        &layers,
        &cycles,
        &exploration_results,
        start.elapsed(),
        use_color,
        true,
    );

    let budget_failed;
    // Step 7: Write output files if requested
    if let Some(dir) = output_dir {
        write_run_report(dir, &call_graph, &exploration_results)?;
        // str-jeen.5: re-classify after `write_run_report` so the
        // per-function .md self-check can fire `invalid-artifacts`
        // when expected files are missing on disk. Source-set diff
        // and representation tier are unchanged from the stdout
        // pass; only the missing-artifacts signal is new here.
        // str-ux7q: feed the shared cross-command artifact-reference
        // validator instead of just checking per-function .md
        // presence — this also catches exploration failures that
        // never recorded an unavailable-reason token, and stale
        // unreferenced .md files left under `output_dir` from a
        // previous run.
        let validation_issues =
            validate_run_artifact_references(dir, &exploration_results, &exploration_failures);
        let (validity_final, reasons_final) =
            classify_validity(&run_summary, Some(&source_diff), &validation_issues);
        run_summary.report_validity = validity_final;
        run_summary.validity_reasons = reasons_final;
        run_summary.gate_decisions =
            evaluate_coverage_budget_gates(&run_summary, &coverage_budget_gates);
        budget_failed = coverage_budget_failed(&run_summary.gate_decisions);
        print_markdown(
            &render_gate_decisions_markdown(&run_summary.gate_decisions),
            use_color,
        );
        write_run_summary_json(dir, &run_summary)?;
        let status_files = run_status_file_inputs_from_registry(
            &registry,
            &root,
            &exploration_results,
            &exploration_failures,
        );
        let status_targets = run_status_target_inputs_from_registry(
            &registry,
            &root,
            dir,
            &exploration_results,
            &exploration_failures,
            None,
        );
        write_run_status_export(
            dir,
            &run_manifest,
            &[("run_summary", dir.join(RUN_SUMMARY_FILENAME))],
            &status_files,
            &status_targets,
            &run_status_rollup_input_from_summary(&run_summary),
        )?;
    } else {
        run_summary.gate_decisions =
            evaluate_coverage_budget_gates(&run_summary, &coverage_budget_gates);
        budget_failed = coverage_budget_failed(&run_summary.gate_decisions);
        print_markdown(
            &render_gate_decisions_markdown(&run_summary.gate_decisions),
            use_color,
        );
    }

    shutdown_all_frontends(frontends).await;
    if budget_failed {
        return Err("coverage budget gates failed".into());
    }
    if let Some(reason) = run_invalidity_failure(&run_summary) {
        eprintln!(
            "\nERROR: shatter run produced an invalid report ({reason}). \
             Exiting nonzero; do not treat this run as successful."
        );
        return Err(format!("invalid run: {reason}").into());
    }
    Ok(())
}

/// str-jeen.84: a run is considered invalid for exit-code purposes when
/// the report is explicitly stale-source-set (the snapshot the run was
/// classified against drifted) or when exploration was attempted but
/// zero functions completed. Either shape is dangerous to treat as
/// success in CI, so `shatter run` exits nonzero unconditionally.
pub(crate) fn run_invalidity_failure(summary: &RunSummary) -> Option<&'static str> {
    if summary.report_validity == ReportValidity::StaleSourceSet {
        return Some("report_validity is stale-source-set");
    }
    let attempted = summary.completed_functions
        + summary.failed_functions
        + summary.skipped_functions;
    if attempted > 0 && summary.completed_functions == 0 {
        return Some("attempted functions nonzero but completed=0");
    }
    None
}

/// Write full run report with per-function files to output directory.
fn write_run_report(
    dir: &Path,
    call_graph: &CallGraph,
    exploration_results: &[(String, explorer::ObservationOutput)],
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("failed to create output dir '{}': {e}", dir.display()))?;

    for (qname, result) in exploration_results {
        let safe_name = run_report_safe_name(qname);
        let func_path = dir.join(format!("{safe_name}.md"));

        let pct = if result.total_lines > 0 {
            (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let mut content = String::new();
        content.push_str(&format!("# {qname}\n\n"));
        content.push_str(&format!("- **Iterations**: {}\n", result.iterations));
        content.push_str(&format!("- **Unique paths**: {}\n", result.unique_paths));
        content.push_str(&format!(
            "- **Line coverage**: {}/{} ({pct:.0}%)\n",
            result.lines_covered, result.total_lines
        ));

        if !result.new_path_executions.is_empty() {
            content.push_str("\n## Discovered Paths\n\n");
            for (i, exec) in result.new_path_executions.iter().enumerate() {
                let inputs_str: Vec<String> = exec.inputs.iter().map(|v| v.to_string()).collect();
                let outcome = if let Some(ref err) = exec.thrown_error {
                    format!("THROWS {err}")
                } else {
                    match &exec.return_value {
                        Some(v) if !v.is_null() => format!("returns {v}"),
                        _ => "returns void".to_string(),
                    }
                };
                content.push_str(&format!(
                    "{}. Input: ({}) -> {}\n",
                    i + 1,
                    inputs_str.join(", "),
                    outcome
                ));
            }
        }

        // Add call graph info
        let callees = call_graph.callees_of(qname);
        let callers = call_graph.callers_of(qname);
        if !callees.is_empty() || !callers.is_empty() {
            content.push_str("\n## Call Graph\n\n");
            if !callees.is_empty() {
                content.push_str(&format!("- **Calls**: {}\n", callees.join(", ")));
            }
            if !callers.is_empty() {
                content.push_str(&format!("- **Called by**: {}\n", callers.join(", ")));
            }
        }

        std::fs::write(&func_path, &content)
            .map_err(|e| format!("failed to write {}: {e}", func_path.display()))?;
    }

    log::info!(
        "Wrote {} per-function report(s) to {}",
        exploration_results.len(),
        dir.display()
    );

    Ok(())
}

/// Filename of the run-level JSON summary written under `output_dir`.
pub(crate) const RUN_SUMMARY_FILENAME: &str = "run.json";

/// On-disk schema version for [`RunSummary`]. Bumped when fields are
/// removed or change meaning (additive fields use `#[serde(default)]`
/// and don't require a bump).
pub(crate) const RUN_SUMMARY_VERSION: u32 = 1;

/// str-jeen.5: representation-percent threshold at or above which a
/// run is eligible for `report_validity = high`. Below this, the
/// run is at best `degraded`.
pub(crate) const HIGH_REPRESENTATION_PCT: f64 = 75.0;

/// str-jeen.5: representation-percent threshold below which a run
/// is `low`. Captures the Kapow case where the completed-function
/// denominator is a tiny fraction of the selected source set.
pub(crate) const LOW_REPRESENTATION_PCT: f64 = 25.0;

/// str-jeen.5: combined unrepresented-failure share (failed +
/// timed-out + unsupported lines, as a percent of selected source
/// lines) at or above which a run drops from `high` to `degraded`
/// even when overall representation looks healthy. Captures runs
/// where most of the source set is technically "represented" but a
/// large slice of it is anchored on failed exploration.
pub(crate) const DEGRADED_UNREPRESENTED_PCT: f64 = 25.0;

/// str-jeen.5: report-level reliability tag. Single-valued; the
/// `validity_reasons` list explains why a run landed at the chosen
/// tier. Order from best to worst follows
/// [`report_validity_severity`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReportValidity {
    #[default]
    High,
    Degraded,
    Low,
    StaleSourceSet,
    InvalidArtifacts,
}

/// Severity ranking used when multiple validity signals apply. The
/// worst (highest) tier wins.
fn report_validity_severity(v: ReportValidity) -> u8 {
    match v {
        ReportValidity::High => 0,
        ReportValidity::Degraded => 1,
        ReportValidity::Low => 2,
        ReportValidity::StaleSourceSet => 3,
        ReportValidity::InvalidArtifacts => 4,
    }
}

/// str-jeen.5: one machine-readable explanation for why a run's
/// `report_validity` is what it is. `code` is a closed-set
/// snake_case token; `detail` is human-readable; `recommended_action`
/// tells the operator what to do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ValidityReason {
    pub code: String,
    pub detail: String,
    pub recommended_action: String,
}

/// Run-level JSON summary written by `shatter run` to
/// `<output_dir>/run.json` (str-jeen.17).
///
/// `selected_source_files` and `selected_source_lines` come from the
/// run-start manifest snapshot, **not** from completed-function span
/// totals. That's the whole point of this struct: the denominator must
/// reflect the source set the run selected, even when most targets
/// fail or skip during exploration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RunSummary {
    /// Schema version. See [`RUN_SUMMARY_VERSION`].
    pub version: u32,
    /// Run identifier; matches the captured manifest's `scan_id`.
    pub scan_id: String,
    /// Number of files in the manifest source set
    /// ([`RunManifest::selected_source_files`]).
    pub selected_source_files: usize,
    /// Sum of per-file line counts across the manifest source set
    /// ([`RunManifest::selected_source_lines`]).
    pub selected_source_lines: u64,
    /// Functions that finished exploration without a recorded error.
    pub completed_functions: usize,
    /// Functions whose exploration failed (recorded error) or were not
    /// completed for any other non-skip reason.
    pub failed_functions: usize,
    /// Functions whose exploration was intentionally skipped before
    /// attempt. The `run` command does not currently surface skips
    /// distinctly from "not completed"; the field is reserved so adding
    /// skip tracking later is a non-breaking change.
    #[serde(default)]
    pub skipped_functions: usize,
    /// Sum of per-file line counts for selected source files where no
    /// exploration target was discovered (str-jeen.43). A file
    /// contributes its full line count when the registry holds zero
    /// `FunctionEntry` records pointing at it. Source of truth is the
    /// run-start manifest snapshot + source-set classification, NOT
    /// completed-function spans, so the bucket survives runs where
    /// every attempted target later fails or is skipped.
    #[serde(default)]
    pub no_target_file_lines: u64,
    /// Sum of selected source lines that fall outside any discovered
    /// function span (str-jeen.43). For files that do contain at least
    /// one discovered function, this is `line_count - covered`, where
    /// `covered` is the line count of the union of discovered
    /// `[start_line, end_line]` spans intersected with `[1, line_count]`.
    /// Files with zero discovered targets contribute nothing here —
    /// their lines are already attributed to `no_target_file_lines`.
    #[serde(default)]
    pub undiscovered_source_lines: u64,
    /// Selected source lines represented by completed exploration
    /// (str-jeen.44). This is a source-set metric: a line contributes
    /// at most once even if overlapping function spans point at it.
    #[serde(default)]
    pub represented_source_lines: u64,
    /// `represented_source_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub represented_source_percent: f64,
    /// Selected source lines in discovered spans whose final outcome
    /// failed without a more specific timeout or unsupported bucket.
    #[serde(default)]
    pub unrepresented_failed_lines: u64,
    /// `unrepresented_failed_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub unrepresented_failed_percent: f64,
    /// Selected source lines in discovered spans whose final outcome
    /// timed out.
    #[serde(default)]
    pub unrepresented_timed_out_lines: u64,
    /// `unrepresented_timed_out_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub unrepresented_timed_out_percent: f64,
    /// Selected source lines in discovered spans that were unsupported
    /// by the frontend or value-generation path.
    #[serde(default)]
    pub unrepresented_unsupported_lines: u64,
    /// `unrepresented_unsupported_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub unrepresented_unsupported_percent: f64,
    /// Alias of `no_target_file_lines` grouped under the str-jeen.44
    /// unrepresented-source namespace.
    #[serde(default)]
    pub unrepresented_no_target_lines: u64,
    /// `unrepresented_no_target_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub unrepresented_no_target_percent: f64,
    /// Alias of `undiscovered_source_lines` grouped under the str-jeen.44
    /// unrepresented-source namespace.
    #[serde(default)]
    pub unrepresented_undiscovered_lines: u64,
    /// `unrepresented_undiscovered_lines / selected_source_lines * 100`.
    #[serde(default)]
    pub unrepresented_undiscovered_percent: f64,
    /// str-jeen.5: report-level reliability tag, separate from
    /// process exit code and per-target status. Defaults to `high`
    /// for legacy `run.json` payloads predating str-jeen.5 — callers
    /// that read older files will see a `high` validity even though
    /// no classifier ran.
    #[serde(default)]
    pub report_validity: ReportValidity,
    /// str-jeen.5: machine-readable explanations for the chosen
    /// `report_validity`. Empty for `high` runs; populated when any
    /// reason code (representation tier, kapow denominator, stale
    /// source set, missing artifacts) fires.
    #[serde(default)]
    pub validity_reasons: Vec<ValidityReason>,
    /// Optional coverage budget gate decisions evaluated for this run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_decisions: Vec<StatusGateDecision>,
}

/// Derive a stable hash of the discovery scope so the manifest's
/// `scope_hash` field changes when include/exclude/language/max_depth
/// change. Uses the std hasher for portability — collision resistance
/// is not required, only reproducibility within a single Shatter
/// version.
pub(crate) fn scope_hash(scope: &RunScope) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    scope.options.include_patterns.hash(&mut hasher);
    scope.options.exclude_patterns.hash(&mut hasher);
    scope.options.respect_gitignore.hash(&mut hasher);
    scope.options.max_depth.hash(&mut hasher);
    scope.language_filter.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Build the `DiscoveredSpan` list passed to [`build_run_summary`]
/// from the analyzed function registry, using the same path
/// normalization the run-start manifest used (relative to `root` when
/// possible). Keeping path normalization in lock-step with
/// `manifest_paths` is what makes the line-bucket classification
/// match by manifest path.
pub(crate) fn registry_spans(
    registry: &batch_analyze::FunctionRegistry,
    root: &Path,
) -> Vec<DiscoveredSpan> {
    registry
        .entries()
        .iter()
        .map(|e| {
            let path = manifest_path_for(&e.file_path, root);
            DiscoveredSpan {
                path,
                start_line: e.start_line,
                end_line: e.end_line,
            }
        })
        .collect()
}

fn manifest_path_for(file_path: &Path, root: &Path) -> String {
    file_path
        .strip_prefix(root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn registry_representation_spans(
    registry: &batch_analyze::FunctionRegistry,
    root: &Path,
    exploration_results: &[(String, explorer::ObservationOutput)],
    exploration_failures: &HashMap<String, String>,
) -> Vec<SourceRepresentationSpan> {
    let results_by_name: HashMap<&str, &explorer::ObservationOutput> = exploration_results
        .iter()
        .map(|(name, output)| (name.as_str(), output))
        .collect();

    registry
        .entries()
        .iter()
        .map(|entry| {
            let qualified_name =
                batch_analyze::FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
            let outcome = match results_by_name.get(qualified_name.as_str()) {
                Some(output) if output.timed_out => SourceRepresentationOutcome::TimedOut,
                Some(_) => SourceRepresentationOutcome::Represented,
                None => exploration_failures
                    .get(qualified_name.as_str())
                    .map(|reason| source_representation_outcome_from_failure_reason(reason))
                    .unwrap_or(SourceRepresentationOutcome::Failed),
            };
            SourceRepresentationSpan {
                path: manifest_path_for(&entry.file_path, root),
                start_line: entry.start_line,
                end_line: entry.end_line,
                outcome,
            }
        })
        .collect()
}

fn source_representation_outcome_from_failure_reason(reason: &str) -> SourceRepresentationOutcome {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        SourceRepresentationOutcome::TimedOut
    } else if lower.contains("unsupported") || lower.contains("unexecutable") {
        SourceRepresentationOutcome::Unsupported
    } else {
        SourceRepresentationOutcome::Failed
    }
}

#[derive(Debug, Default)]
struct RunStatusFileCounts {
    discovered: u64,
    attempted: u64,
    completed: u64,
    failed: u64,
    unsupported: u64,
}

fn run_status_file_inputs_from_registry(
    registry: &batch_analyze::FunctionRegistry,
    root: &Path,
    exploration_results: &[(String, explorer::ObservationOutput)],
    exploration_failures: &HashMap<String, String>,
) -> Vec<StatusFileInput> {
    let results_by_name: HashMap<&str, &explorer::ObservationOutput> = exploration_results
        .iter()
        .map(|(name, output)| (name.as_str(), output))
        .collect();
    let mut by_path: BTreeMap<String, RunStatusFileCounts> = BTreeMap::new();

    for entry in registry.entries() {
        let qualified_name =
            batch_analyze::FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
        let path = manifest_path_for(&entry.file_path, root);
        let counts = by_path.entry(path).or_default();
        counts.discovered += 1;

        if let Some(output) = results_by_name.get(qualified_name.as_str()) {
            counts.attempted += 1;
            if output.timed_out {
                counts.failed += 1;
            } else {
                counts.completed += 1;
            }
        } else if let Some(reason) = exploration_failures.get(qualified_name.as_str()) {
            if is_unsupported_failure_reason(reason) {
                counts.unsupported += 1;
            } else {
                counts.attempted += 1;
                counts.failed += 1;
            }
        }
    }

    by_path
        .into_iter()
        .map(|(path, counts)| StatusFileInput {
            path,
            discovered_targets: counts.discovered,
            attempted_targets: counts.attempted,
            completed_targets: counts.completed,
            failed_targets: counts.failed,
            unsupported_targets: counts.unsupported,
            status: run_status_file_status_from_counts(&counts),
        })
        .collect()
}

fn is_unsupported_failure_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("unsupported") || lower.contains("unexecutable")
}

fn run_status_file_status_from_counts(counts: &RunStatusFileCounts) -> StatusFileStatus {
    if counts.discovered == 0 {
        StatusFileStatus::NoTarget
    } else if counts.completed == counts.discovered {
        StatusFileStatus::Completed
    } else if counts.completed > 0 {
        StatusFileStatus::Partial
    } else if counts.failed > 0 {
        StatusFileStatus::Failed
    } else if counts.unsupported > 0 {
        StatusFileStatus::Unsupported
    } else {
        StatusFileStatus::Skipped
    }
}

fn run_status_target_inputs_from_registry(
    registry: &batch_analyze::FunctionRegistry,
    root: &Path,
    output_dir: &Path,
    exploration_results: &[(String, explorer::ObservationOutput)],
    exploration_failures: &HashMap<String, String>,
    default_skip_reason: Option<&str>,
) -> Vec<StatusTargetInput> {
    let results_by_name: HashMap<&str, &explorer::ObservationOutput> = exploration_results
        .iter()
        .map(|(name, output)| (name.as_str(), output))
        .collect();

    registry
        .entries()
        .iter()
        .map(|entry| {
            let qualified_name =
                batch_analyze::FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
            let source_file = manifest_path_for(&entry.file_path, root);
            let result = results_by_name.get(qualified_name.as_str()).copied();
            let explicit_failure = exploration_failures.get(qualified_name.as_str());
            let failure_reason = explicit_failure
                .cloned()
                .or_else(|| default_skip_reason.map(str::to_string));
            let (outcome, validity_impact) =
                run_status_target_outcome(result, failure_reason.as_deref());
            let artifact_path = (outcome == StatusTargetOutcome::Completed)
                .then(|| output_dir.join(format!("{}.md", run_report_safe_name(&qualified_name))));

            StatusTargetInput {
                target_id: qualified_name,
                name: entry.name.clone(),
                source_file,
                start_line: entry.start_line,
                end_line: entry.end_line,
                outcome,
                artifact_path,
                failure_reason: failure_reason.clone(),
                unavailable_reason: (outcome != StatusTargetOutcome::Completed).then(|| {
                    failure_reason
                        .clone()
                        .unwrap_or_else(|| "target artifact unavailable".to_string())
                }),
                validity_impact,
            }
        })
        .collect()
}

fn run_status_target_outcome(
    result: Option<&explorer::ObservationOutput>,
    failure_reason: Option<&str>,
) -> (StatusTargetOutcome, StatusTargetValidityImpact) {
    if let Some(output) = result {
        if output.timed_out {
            return (
                StatusTargetOutcome::TimedOut,
                StatusTargetValidityImpact::Degrades,
            );
        }
        return (
            StatusTargetOutcome::Completed,
            StatusTargetValidityImpact::Contributes,
        );
    }

    let Some(reason) = failure_reason else {
        return (
            StatusTargetOutcome::Skipped,
            StatusTargetValidityImpact::Excluded,
        );
    };
    if is_unsupported_failure_reason(reason) {
        (
            StatusTargetOutcome::Unsupported,
            StatusTargetValidityImpact::Excluded,
        )
    } else if is_timeout_failure_reason(reason) {
        (
            StatusTargetOutcome::TimedOut,
            StatusTargetValidityImpact::Degrades,
        )
    } else if is_unavailable_frontend_failure_reason(reason) {
        (
            StatusTargetOutcome::UnavailableFrontend,
            StatusTargetValidityImpact::Degrades,
        )
    } else {
        (
            StatusTargetOutcome::Failed,
            StatusTargetValidityImpact::Degrades,
        )
    }
}

fn is_timeout_failure_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
}

fn is_unavailable_frontend_failure_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("frontend") || lower.contains("preflight")
}

/// A discovered function span anchored to a manifest source path.
///
/// `path` is matched verbatim against [`SourceFileSnapshot::path`] in
/// the run manifest, so callers must pre-normalize registry file paths
/// (typically by stripping the project root prefix the same way the
/// manifest captured them).
#[derive(Debug, Clone)]
pub(crate) struct DiscoveredSpan {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Two manifest-driven line buckets reported alongside the
/// `selected_source_lines` denominator (str-jeen.43).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LineBuckets {
    pub no_target_file_lines: u64,
    pub undiscovered_source_lines: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRepresentationOutcome {
    Represented,
    Failed,
    TimedOut,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRepresentationSpan {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub outcome: SourceRepresentationOutcome,
}

/// Source-set representation buckets (str-jeen.44). Unlike function-span
/// counters, these are line partitions over the selected source manifest:
/// a selected line contributes to at most one bucket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SourceRepresentationBuckets {
    pub represented_source_lines: u64,
    pub unrepresented_failed_lines: u64,
    pub unrepresented_timed_out_lines: u64,
    pub unrepresented_unsupported_lines: u64,
    pub unrepresented_no_target_lines: u64,
    pub unrepresented_undiscovered_lines: u64,
}

/// Compute the `no_target_file_lines` and `undiscovered_source_lines`
/// buckets from a manifest snapshot and the set of discovered function
/// spans (str-jeen.43).
///
/// The classification is purely manifest-driven: a selected file with
/// zero matching spans contributes its whole `line_count` to
/// `no_target_file_lines`; a selected file with at least one matching
/// span contributes `line_count - covered` to
/// `undiscovered_source_lines`, where `covered` is the line count of
/// the union of `[start_line, end_line]` spans clamped to
/// `[1, line_count]`. Files whose manifest snapshot has no
/// `line_count` (unreadable at capture time) contribute zero.
pub(crate) fn compute_line_buckets(
    manifest: &RunManifest,
    spans: &[DiscoveredSpan],
) -> LineBuckets {
    // Group spans by manifest path.
    let mut spans_by_path: BTreeMap<&str, Vec<(u32, u32)>> = BTreeMap::new();
    for s in spans {
        spans_by_path
            .entry(s.path.as_str())
            .or_default()
            .push((s.start_line, s.end_line));
    }

    let mut buckets = LineBuckets::default();
    for snap in &manifest.source_files {
        let Some(line_count) = snap.line_count else {
            continue;
        };
        let line_count_u64 = u64::from(line_count);
        match spans_by_path.get(snap.path.as_str()) {
            None => {
                // No discovered targets in this file at all — every
                // selected line is in the "no target file" bucket.
                buckets.no_target_file_lines =
                    buckets.no_target_file_lines.saturating_add(line_count_u64);
            }
            Some(file_spans) => {
                let covered = covered_lines(file_spans, line_count);
                let undiscovered = line_count_u64.saturating_sub(covered);
                buckets.undiscovered_source_lines = buckets
                    .undiscovered_source_lines
                    .saturating_add(undiscovered);
            }
        }
    }
    buckets
}

/// Count the lines covered by the union of `spans`, clamped to
/// `[1, line_count]`. Spans are merged so overlapping ranges aren't
/// double-counted; degenerate spans (`start > end`, `start == 0`,
/// `start > line_count`) are skipped rather than treated as errors —
/// frontends emit a wide range of span shapes and this bucket should
/// not panic on edge cases.
fn covered_lines(spans: &[(u32, u32)], line_count: u32) -> u64 {
    if line_count == 0 {
        return 0;
    }
    let mut clamped: Vec<(u32, u32)> = spans
        .iter()
        .filter_map(|&(start, end)| {
            if start == 0 || start > line_count {
                return None;
            }
            let s = start;
            let e = end.min(line_count).max(s);
            Some((s, e))
        })
        .collect();
    clamped.sort_by_key(|&(s, _)| s);
    let mut total: u64 = 0;
    let mut cursor: Option<(u32, u32)> = None;
    for (s, e) in clamped {
        match cursor {
            None => cursor = Some((s, e)),
            Some((cs, ce)) => {
                if s <= ce.saturating_add(1) {
                    cursor = Some((cs, ce.max(e)));
                } else {
                    total += u64::from(ce - cs + 1);
                    cursor = Some((s, e));
                }
            }
        }
    }
    if let Some((cs, ce)) = cursor {
        total += u64::from(ce - cs + 1);
    }
    total
}

pub(crate) fn compute_source_representation(
    manifest: &RunManifest,
    spans: &[SourceRepresentationSpan],
) -> SourceRepresentationBuckets {
    let mut spans_by_path: BTreeMap<&str, Vec<&SourceRepresentationSpan>> = BTreeMap::new();
    for span in spans {
        spans_by_path
            .entry(span.path.as_str())
            .or_default()
            .push(span);
    }

    let mut buckets = SourceRepresentationBuckets::default();
    for snap in &manifest.source_files {
        let Some(line_count) = snap.line_count else {
            continue;
        };
        let Some(file_spans) = spans_by_path.get(snap.path.as_str()) else {
            buckets.unrepresented_no_target_lines = buckets
                .unrepresented_no_target_lines
                .saturating_add(u64::from(line_count));
            continue;
        };

        let mut line_classes = vec![LineClass::Undiscovered; line_count as usize];
        for span in file_spans {
            let Some((start, end)) = clamped_span_range(span.start_line, span.end_line, line_count)
            else {
                continue;
            };
            let class = LineClass::from_outcome(span.outcome);
            for line_class in line_classes
                .iter_mut()
                .take(end as usize)
                .skip((start - 1) as usize)
            {
                if class.precedence() > line_class.precedence() {
                    *line_class = class;
                }
            }
        }

        for class in line_classes {
            match class {
                LineClass::Undiscovered => {
                    buckets.unrepresented_undiscovered_lines =
                        buckets.unrepresented_undiscovered_lines.saturating_add(1);
                }
                LineClass::Failed => {
                    buckets.unrepresented_failed_lines =
                        buckets.unrepresented_failed_lines.saturating_add(1);
                }
                LineClass::TimedOut => {
                    buckets.unrepresented_timed_out_lines =
                        buckets.unrepresented_timed_out_lines.saturating_add(1);
                }
                LineClass::Unsupported => {
                    buckets.unrepresented_unsupported_lines =
                        buckets.unrepresented_unsupported_lines.saturating_add(1);
                }
                LineClass::Represented => {
                    buckets.represented_source_lines =
                        buckets.represented_source_lines.saturating_add(1);
                }
            }
        }
    }
    buckets
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineClass {
    Undiscovered,
    Failed,
    TimedOut,
    Unsupported,
    Represented,
}

impl LineClass {
    fn from_outcome(outcome: SourceRepresentationOutcome) -> Self {
        match outcome {
            SourceRepresentationOutcome::Represented => Self::Represented,
            SourceRepresentationOutcome::Failed => Self::Failed,
            SourceRepresentationOutcome::TimedOut => Self::TimedOut,
            SourceRepresentationOutcome::Unsupported => Self::Unsupported,
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Self::Undiscovered => 0,
            Self::Failed => 1,
            Self::Unsupported => 2,
            Self::TimedOut => 3,
            Self::Represented => 4,
        }
    }
}

fn clamped_span_range(start: u32, end: u32, line_count: u32) -> Option<(u32, u32)> {
    if line_count == 0 || start == 0 || start > line_count {
        return None;
    }
    Some((start, end.min(line_count).max(start)))
}

/// Build a [`RunSummary`] from a captured manifest, the run's outcome
/// counters, and the discovered function spans used to bucket
/// no-target / undiscovered source lines (str-jeen.43). Pure function
/// — easy to unit-test without spinning up frontends.
pub(crate) fn build_run_summary(
    scan_id: &str,
    manifest: &RunManifest,
    completed: usize,
    failed: usize,
    skipped: usize,
    spans: &[DiscoveredSpan],
) -> RunSummary {
    let buckets = compute_line_buckets(manifest, spans);
    let representation = SourceRepresentationBuckets {
        unrepresented_no_target_lines: buckets.no_target_file_lines,
        unrepresented_undiscovered_lines: buckets.undiscovered_source_lines,
        ..SourceRepresentationBuckets::default()
    };
    build_run_summary_from_buckets(
        scan_id,
        manifest,
        completed,
        failed,
        skipped,
        representation,
    )
}

pub(crate) fn build_run_summary_with_representation(
    scan_id: &str,
    manifest: &RunManifest,
    completed: usize,
    failed: usize,
    skipped: usize,
    spans: &[SourceRepresentationSpan],
) -> RunSummary {
    let representation = compute_source_representation(manifest, spans);
    build_run_summary_from_buckets(
        scan_id,
        manifest,
        completed,
        failed,
        skipped,
        representation,
    )
}

fn build_run_summary_from_buckets(
    scan_id: &str,
    manifest: &RunManifest,
    completed: usize,
    failed: usize,
    skipped: usize,
    representation: SourceRepresentationBuckets,
) -> RunSummary {
    let selected_source_lines = manifest.selected_source_lines();
    RunSummary {
        version: RUN_SUMMARY_VERSION,
        scan_id: scan_id.to_string(),
        selected_source_files: manifest.selected_source_files(),
        selected_source_lines,
        completed_functions: completed,
        failed_functions: failed,
        skipped_functions: skipped,
        no_target_file_lines: representation.unrepresented_no_target_lines,
        undiscovered_source_lines: representation.unrepresented_undiscovered_lines,
        represented_source_lines: representation.represented_source_lines,
        represented_source_percent: percent_of_source(
            representation.represented_source_lines,
            selected_source_lines,
        ),
        unrepresented_failed_lines: representation.unrepresented_failed_lines,
        unrepresented_failed_percent: percent_of_source(
            representation.unrepresented_failed_lines,
            selected_source_lines,
        ),
        unrepresented_timed_out_lines: representation.unrepresented_timed_out_lines,
        unrepresented_timed_out_percent: percent_of_source(
            representation.unrepresented_timed_out_lines,
            selected_source_lines,
        ),
        unrepresented_unsupported_lines: representation.unrepresented_unsupported_lines,
        unrepresented_unsupported_percent: percent_of_source(
            representation.unrepresented_unsupported_lines,
            selected_source_lines,
        ),
        unrepresented_no_target_lines: representation.unrepresented_no_target_lines,
        unrepresented_no_target_percent: percent_of_source(
            representation.unrepresented_no_target_lines,
            selected_source_lines,
        ),
        unrepresented_undiscovered_lines: representation.unrepresented_undiscovered_lines,
        unrepresented_undiscovered_percent: percent_of_source(
            representation.unrepresented_undiscovered_lines,
            selected_source_lines,
        ),
        report_validity: ReportValidity::High,
        validity_reasons: Vec::new(),
        gate_decisions: Vec::new(),
    }
}

/// str-jeen.5: pure classifier producing a `(ReportValidity, reasons)`
/// pair from the run summary plus optional end-of-run signals.
///
/// - `summary` carries the manifest-driven denominators and
///   representation buckets already populated by
///   `build_run_summary_*`.
/// - `source_diff`, when `Some` and stale, raises validity to at
///   least `stale-source-set`.
/// - `validation_issues` is the list of artifact-reference contract
///   violation diagnostics surfaced by the shared validator (see
///   [`validate_run_artifact_references`], str-ux7q). This includes
///   missing per-function `<safe_name>.md` files, exploration
///   failures with no recorded reason token, and unreferenced
///   `*.md` extras under `output_dir`. Non-empty raises validity to
///   `invalid-artifacts`. Historically (str-jeen.5) this slot only
///   carried the per-function `.md` presence check.
///
/// The function is total and deterministic so tests can pin tier
/// transitions at the threshold boundaries.
pub(crate) fn classify_validity(
    summary: &RunSummary,
    source_diff: Option<&run_manifest::ManifestDiff>,
    validation_issues: &[String],
) -> (ReportValidity, Vec<ValidityReason>) {
    let mut reasons: Vec<ValidityReason> = Vec::new();
    let mut tier = ReportValidity::High;

    let rep_pct = summary.represented_source_percent;
    if rep_pct < LOW_REPRESENTATION_PCT {
        tier = worst(tier, ReportValidity::Low);
        reasons.push(ValidityReason {
            code: "low_representation".to_string(),
            detail: format!(
                "represented_source_percent={rep_pct:.1} below low threshold {LOW_REPRESENTATION_PCT:.1}"
            ),
            recommended_action:
                "Investigate failed/timed-out/unsupported buckets in unrepresented_*_lines and re-run after addressing root causes; do not treat as success.".to_string(),
        });
    } else if rep_pct < HIGH_REPRESENTATION_PCT {
        tier = worst(tier, ReportValidity::Degraded);
        reasons.push(ValidityReason {
            code: "degraded_representation".to_string(),
            detail: format!(
                "represented_source_percent={rep_pct:.1} below high threshold {HIGH_REPRESENTATION_PCT:.1}"
            ),
            recommended_action:
                "Inspect unrepresented_*_lines buckets and broaden frontend coverage where feasible.".to_string(),
        });
    }

    let unrepresented_share = summary.unrepresented_failed_percent
        + summary.unrepresented_timed_out_percent
        + summary.unrepresented_unsupported_percent;
    if rep_pct >= HIGH_REPRESENTATION_PCT && unrepresented_share >= DEGRADED_UNREPRESENTED_PCT {
        tier = worst(tier, ReportValidity::Degraded);
        reasons.push(ValidityReason {
            code: "high_unrepresented_failures".to_string(),
            detail: format!(
                "unrepresented_failed+timed_out+unsupported share={unrepresented_share:.1}% at or above {DEGRADED_UNREPRESENTED_PCT:.1}%"
            ),
            recommended_action:
                "Triage failure root causes for the largest unrepresented bucket before relying on this report.".to_string(),
        });
    }

    // Kapow case: completed denominator is zero but exploration was
    // attempted. Tier is already `low` via the rep% rule; the reason
    // code documents the specific failure shape.
    if summary.completed_functions == 0 && summary.failed_functions > 0 {
        reasons.push(ValidityReason {
            code: "kapow_tiny_denominator".to_string(),
            detail: format!(
                "completed=0 of attempted={}; rep%={rep_pct:.1}",
                summary.completed_functions + summary.failed_functions + summary.skipped_functions
            ),
            recommended_action:
                "All attempted functions failed exploration; do not treat as partial-success. Re-run after addressing failure root causes.".to_string(),
        });
    }

    if let Some(diff) = source_diff
        && diff.is_stale()
    {
        tier = worst(tier, ReportValidity::StaleSourceSet);
        if !diff.added.is_empty() {
            reasons.push(ValidityReason {
                code: "stale_source_set_added".to_string(),
                detail: format!(
                    "{} source path(s) added after manifest capture",
                    diff.added.len()
                ),
                recommended_action:
                    "Re-run on a quiesced source tree so the manifest snapshot reflects the explored set."
                        .to_string(),
            });
        }
        if !diff.removed.is_empty() {
            reasons.push(ValidityReason {
                code: "stale_source_set_removed".to_string(),
                detail: format!(
                    "{} source path(s) removed after manifest capture",
                    diff.removed.len()
                ),
                recommended_action:
                    "Re-run on a quiesced source tree; removed files invalidate per-file buckets."
                        .to_string(),
            });
        }
        if !diff.changed.is_empty() {
            reasons.push(ValidityReason {
                code: "stale_source_set_changed".to_string(),
                detail: format!(
                    "{} source path(s) changed content during run",
                    diff.changed.len()
                ),
                recommended_action:
                    "Re-run on a quiesced source tree; mid-run edits make line buckets unreliable."
                        .to_string(),
            });
        }
    }

    if !validation_issues.is_empty() {
        tier = worst(tier, ReportValidity::InvalidArtifacts);
        let preview: Vec<String> = validation_issues.iter().take(3).cloned().collect();
        reasons.push(ValidityReason {
            code: "invalid_artifacts_missing".to_string(),
            detail: format!(
                "{} artifact-reference contract violation(s) (e.g. {})",
                validation_issues.len(),
                preview.join("; ")
            ),
            recommended_action:
                "Inspect output directory for I/O failures or stale per-function reports; the run references artifacts that do not exist or that lack a recorded reason."
                    .to_string(),
        });
    }

    (tier, reasons)
}

/// Evaluate opt-in coverage budget gates against a finalized run summary.
pub(crate) fn evaluate_coverage_budget_gates(
    summary: &RunSummary,
    gates: &shatter_core::config::CoverageBudgetGates,
) -> Vec<StatusGateDecision> {
    let mut decisions = Vec::new();

    if let Some(threshold) = gates.min_source_representation_percent {
        let observed = summary.represented_source_percent;
        decisions.push(threshold_decision(
            "min_source_representation_percent",
            observed >= threshold,
            threshold,
            observed,
            "represented_source_percent met the configured minimum",
            "represented_source_percent is below the configured minimum",
        ));
    }

    if let Some(threshold) = gates.max_failed_span_percent {
        let observed = (summary.unrepresented_failed_percent
            + summary.unrepresented_timed_out_percent)
            .clamp(0.0, 100.0);
        decisions.push(threshold_decision(
            "max_failed_span_percent",
            observed <= threshold,
            threshold,
            observed,
            "failed/timed-out source span percent is within budget",
            "failed/timed-out source span percent exceeds budget",
        ));
    }

    if let Some(threshold) = gates.max_unsupported_span_percent {
        let observed = summary.unrepresented_unsupported_percent;
        decisions.push(threshold_decision(
            "max_unsupported_span_percent",
            observed <= threshold,
            threshold,
            observed,
            "unsupported source span percent is within budget",
            "unsupported source span percent exceeds budget",
        ));
    }

    if gates.fail_on_stale_source_set.unwrap_or(false) {
        let failed = summary.report_validity == ReportValidity::StaleSourceSet;
        decisions.push(boolean_gate_decision(
            "fail_on_stale_source_set",
            !failed,
            report_validity_label(summary.report_validity),
            "report validity is not stale-source-set",
            "report validity is stale-source-set",
        ));
    }

    if gates.fail_on_missing_artifacts.unwrap_or(false) {
        let failed = summary.report_validity == ReportValidity::InvalidArtifacts;
        decisions.push(boolean_gate_decision(
            "fail_on_missing_artifacts",
            !failed,
            report_validity_label(summary.report_validity),
            "report validity is not invalid-artifacts",
            "report validity is invalid-artifacts",
        ));
    }

    if gates.fail_on_low_report_validity.unwrap_or(false) {
        let failed = report_validity_severity(summary.report_validity)
            >= report_validity_severity(ReportValidity::Low);
        decisions.push(boolean_gate_decision(
            "fail_on_low_report_validity",
            !failed,
            report_validity_label(summary.report_validity),
            "report validity is above low",
            "report validity is low or worse",
        ));
    }

    decisions
}

fn threshold_decision(
    gate: &str,
    passed: bool,
    threshold: f64,
    observed: f64,
    passed_reason: &str,
    failed_reason: &str,
) -> StatusGateDecision {
    StatusGateDecision {
        gate: gate.to_string(),
        status: gate_status(passed).to_string(),
        threshold: Some(format_percent_value(threshold)),
        observed: Some(format_percent_value(observed)),
        reason: Some(if passed { passed_reason } else { failed_reason }.to_string()),
    }
}

fn boolean_gate_decision(
    gate: &str,
    passed: bool,
    observed: &str,
    passed_reason: &str,
    failed_reason: &str,
) -> StatusGateDecision {
    StatusGateDecision {
        gate: gate.to_string(),
        status: gate_status(passed).to_string(),
        threshold: Some("true".to_string()),
        observed: Some(observed.to_string()),
        reason: Some(if passed { passed_reason } else { failed_reason }.to_string()),
    }
}

fn gate_status(passed: bool) -> &'static str {
    if passed { "passed" } else { "failed" }
}

fn format_percent_value(value: f64) -> String {
    format!("{value:.1}")
}

fn coverage_budget_failed(decisions: &[StatusGateDecision]) -> bool {
    decisions
        .iter()
        .any(|decision| decision.status.as_str() == "failed")
}

fn render_gate_decisions_markdown(decisions: &[StatusGateDecision]) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    if decisions.is_empty() {
        return md;
    }
    writeln!(md, "## Coverage Budget Gates").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "| Gate | Status | Threshold | Observed | Reason |").unwrap();
    writeln!(md, "|---|---|---|---|---|").unwrap();
    for decision in decisions {
        let threshold = decision.threshold.as_deref().unwrap_or("");
        let observed = decision.observed.as_deref().unwrap_or("");
        let reason = decision.reason.as_deref().unwrap_or("").replace('|', "\\|");
        writeln!(
            md,
            "| {} | {} | {} | {} | {} |",
            decision.gate, decision.status, threshold, observed, reason
        )
        .unwrap();
    }
    writeln!(md).unwrap();
    md
}

/// Pick the worse of two [`ReportValidity`] tiers per
/// [`report_validity_severity`].
fn worst(a: ReportValidity, b: ReportValidity) -> ReportValidity {
    if report_validity_severity(b) > report_validity_severity(a) {
        b
    } else {
        a
    }
}

/// Return the kebab-case wire form of a [`ReportValidity`] for
/// embedding in stdout markdown. Matches the JSON serde rename so
/// machine-readable output and human-readable output stay aligned.
fn report_validity_label(v: ReportValidity) -> &'static str {
    match v {
        ReportValidity::High => "high",
        ReportValidity::Degraded => "degraded",
        ReportValidity::Low => "low",
        ReportValidity::StaleSourceSet => "stale-source-set",
        ReportValidity::InvalidArtifacts => "invalid-artifacts",
    }
}

/// Render the str-jeen.5 validity block as a markdown fragment.
/// Empty `reasons` collapses to a single-line "no issues detected"
/// note so `high` runs still get an explicit verdict.
pub(crate) fn render_validity_markdown(
    validity: ReportValidity,
    reasons: &[ValidityReason],
) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    let label = report_validity_label(validity);
    writeln!(md, "## Report Validity: {label}").unwrap();
    writeln!(md).unwrap();
    if reasons.is_empty() {
        writeln!(md, "No validity issues detected.").unwrap();
        writeln!(md).unwrap();
        return md;
    }
    writeln!(md, "| Reason | Detail | Recommended action |").unwrap();
    writeln!(md, "|---|---|---|").unwrap();
    for r in reasons {
        // Pipes inside cells would break the table; replace just in
        // case future detail strings carry one.
        let detail = r.detail.replace('|', "\\|");
        let action = r.recommended_action.replace('|', "\\|");
        writeln!(md, "| {} | {} | {} |", r.code, detail, action).unwrap();
    }
    writeln!(md).unwrap();
    md
}

/// str-ux7q: derive the on-disk filename stem from a qualified
/// function name. Single source of truth used by both
/// [`write_run_report`] and the validator so the two cannot drift.
pub(crate) fn run_report_safe_name(qname: &str) -> String {
    qname.replace("::", "__").replace('/', "_")
}

/// str-ux7q: validate the run command's artifact-reference contract
/// using the shared cross-command validator from `explore.rs`.
///
/// The run command's on-disk shape is:
/// * one `<safe_name>.md` per completed exploration (written by
///   [`write_run_report`]),
/// * a top-level `run.json` summary (written by
///   [`write_run_summary_json`] after this validator runs, so the
///   stale-extra scan skips it via the `run_defaults` skip list),
/// * one logical "no artifact, must have a reason token" reference
///   per exploration failure surfaced in `exploration_failures`.
///
/// Returns the list of issue diagnostics (one per violation) suitable
/// for feeding into [`classify_validity`]'s `validation_issues`
/// parameter. Empty list = clean run.
pub(crate) fn validate_run_artifact_references(
    output_dir: &Path,
    exploration_results: &[(String, explorer::ObservationOutput)],
    exploration_failures: &HashMap<String, String>,
) -> Vec<String> {
    use crate::commands::explore::{
        ArtifactRef, ArtifactValidationOptions, validate_artifact_refs,
    };

    let mut refs: Vec<ArtifactRef> = Vec::new();
    for (qname, _) in exploration_results {
        let safe_name = run_report_safe_name(qname);
        refs.push(ArtifactRef {
            file: qname.clone(),
            function_name: qname.clone(),
            status: "completed".to_string(),
            artifact: Some(format!("{safe_name}.md")),
            reason: None,
        });
    }
    for (qname, failure_reason) in exploration_failures {
        refs.push(ArtifactRef {
            file: qname.clone(),
            function_name: qname.clone(),
            status: "failed".to_string(),
            artifact: None,
            reason: Some(failure_reason.clone()),
        });
    }
    let report = validate_artifact_refs(
        output_dir,
        &refs,
        &ArtifactValidationOptions::run_defaults(),
    );
    report.issues.iter().map(|i| i.to_string()).collect()
}

fn percent_of_source(lines: u64, selected_source_lines: u64) -> f64 {
    if selected_source_lines == 0 {
        0.0
    } else {
        ((lines as f64 / selected_source_lines as f64) * 100.0).clamp(0.0, 100.0)
    }
}

/// Write `run.json` under `output_dir` using atomic rename.
pub(crate) fn write_run_summary_json(
    output_dir: &Path,
    summary: &RunSummary,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir).map_err(|e| {
        format!(
            "failed to create output dir '{}': {e}",
            output_dir.display()
        )
    })?;
    let path = output_dir.join(RUN_SUMMARY_FILENAME);
    let json = serde_json::to_string_pretty(summary)
        .map_err(|e| format!("failed to serialize run summary: {e}"))?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .map_err(|e| format!("failed to write run summary temp file: {e}"))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("failed to finalize run summary: {e}"))?;
    log::info!("Wrote run summary to {}", path.display());
    Ok(())
}

fn run_status_rollup_input_from_summary(summary: &RunSummary) -> StatusRollupInput {
    StatusRollupInput {
        report_validity: Some(status_report_validity(summary.report_validity)),
        validity_reasons: summary
            .validity_reasons
            .iter()
            .map(|reason| StatusValidityReason {
                code: reason.code.clone(),
                detail: reason.detail.clone(),
                recommended_action: reason.recommended_action.clone(),
            })
            .collect(),
        line_weighted_failure_impact: Some(StatusLineWeightedFailureImpact {
            represented_source_lines: summary.represented_source_lines,
            unrepresented_failed_lines: summary.unrepresented_failed_lines,
            unrepresented_timed_out_lines: summary.unrepresented_timed_out_lines,
            unrepresented_unsupported_lines: summary.unrepresented_unsupported_lines,
            unrepresented_unavailable_frontend_lines: 0,
            unrepresented_no_target_lines: summary.unrepresented_no_target_lines,
            unrepresented_undiscovered_lines: summary.unrepresented_undiscovered_lines,
        }),
        gate_decisions: if summary.gate_decisions.is_empty() {
            None
        } else {
            Some(summary.gate_decisions.clone())
        },
    }
}

fn status_report_validity(validity: ReportValidity) -> StatusReportValidity {
    match validity {
        ReportValidity::High => StatusReportValidity::High,
        ReportValidity::Degraded => StatusReportValidity::Degraded,
        ReportValidity::Low => StatusReportValidity::Low,
        ReportValidity::StaleSourceSet => StatusReportValidity::StaleSourceSet,
        ReportValidity::InvalidArtifacts => StatusReportValidity::InvalidArtifacts,
    }
}

fn write_run_status_export(
    output_dir: &Path,
    manifest: &RunManifest,
    artifact_paths: &[(&str, PathBuf)],
    files: &[StatusFileInput],
    targets: &[StatusTargetInput],
    rollups: &StatusRollupInput,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = output_dir.join(run_manifest::RUN_MANIFEST_FILENAME);
    let artifacts: Vec<StatusArtifactLink<'_>> = artifact_paths
        .iter()
        .map(|(kind, path)| StatusArtifactLink {
            kind,
            path: path.as_path(),
        })
        .collect();
    write_run_status_json(
        output_dir,
        &StatusExportInput {
            command: "run",
            manifest,
            manifest_path: &manifest_path,
            artifacts: &artifacts,
            files,
            targets,
            rollups: rollups.clone(),
        },
    )
    .map_err(|e| format!("failed to write run status export: {e}").into())
}

#[cfg(test)]
mod tests {
    //! str-mg2d: regression tests for `run` honoring `shatter.config.json` scope.
    //!
    //! These tests exercise the helpers that translate project config into the
    //! discovery filters used by `run_run`. They cover the wiring fix
    //! (previously `run_run` used `DiscoveryOptions::default()` and ignored
    //! the project config entirely) by asserting that, on a tree containing
    //! files the project author has excluded, discovery + language filtering
    //! using the helper produces only the included file set.
    use super::*;
    use proptest::prelude::*;
    use shatter_core::discovery;
    use std::{collections::HashMap, fs};

    /// Files outside `include`/excluded by `exclude` must not appear in the
    /// discovered set when the project config is loaded by `run`.
    #[test]
    fn run_scope_honors_project_config_excludes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        // Project author wants Go only and explicitly excludes a TS fixture
        // tree. Mirrors the Refute audit case from str-mg2d.
        fs::write(
            root.join("shatter.config.json"),
            r#"{
                "include": ["**/*.go"],
                "exclude": ["testdata/fixtures/typescript/**", "testdata/fixtures/rust/**"]
            }"#,
        )
        .expect("write config");

        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(root.join("src/main.go"), "package main\nfunc main() {}\n").expect("write go");

        let ts_dir = root.join("testdata/fixtures/typescript");
        fs::create_dir_all(&ts_dir).expect("mkdir ts");
        fs::write(ts_dir.join("noisy.ts"), "export const x = 1;\n").expect("write ts");

        let rs_dir = root.join("testdata/fixtures/rust");
        fs::create_dir_all(&rs_dir).expect("mkdir rs");
        fs::write(rs_dir.join("noisy.rs"), "fn main() {}\n").expect("write rs");

        let scope = run_scope_from_project_config(root);
        assert_eq!(scope.options.include_patterns, vec!["**/*.go".to_string()]);
        assert_eq!(
            scope.options.exclude_patterns,
            vec![
                "testdata/fixtures/typescript/**".to_string(),
                "testdata/fixtures/rust/**".to_string(),
            ]
        );

        let files = discovery::discover_files(root, &scope.options).expect("discover");
        let files = apply_language_filter(files, scope.language_filter.as_deref());

        let paths: Vec<String> = files
            .iter()
            .map(|(p, _)| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("main.go")),
            "expected main.go in discovered files, got {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.ends_with("noisy.ts")),
            "excluded TS fixture leaked into run scope: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.ends_with("noisy.rs")),
            "excluded Rust fixture leaked into run scope: {paths:?}"
        );
    }

    /// Without a project config, `run` falls back to default discovery
    /// (no include/exclude filters, gitignore respected). This pins the
    /// no-config baseline so the fix does not silently change behavior for
    /// repos without `shatter.config.json`.
    #[test]
    fn run_scope_defaults_without_project_config() {
        let dir = tempfile::tempdir().expect("tempdir");
        let scope = run_scope_from_project_config(dir.path());
        assert!(scope.options.include_patterns.is_empty());
        assert!(scope.options.exclude_patterns.is_empty());
        assert!(scope.options.max_depth.is_none());
        assert!(scope.language_filter.is_none());
        assert!(scope.options.respect_gitignore);
    }

    /// `language` in project config restricts discovered files to the matching
    /// language only — covers the case where the project author scopes
    /// shatter to a single language.
    #[test]
    fn run_scope_language_filter_drops_other_languages() {
        let mixed = vec![
            (PathBuf::from("a.ts"), DiscoveryLanguage::TypeScript),
            (PathBuf::from("b.go"), DiscoveryLanguage::Go),
            (PathBuf::from("c.rs"), DiscoveryLanguage::Rust),
        ];
        let go_only = apply_language_filter(mixed.clone(), Some("go"));
        assert_eq!(go_only.len(), 1);
        assert_eq!(go_only[0].0, PathBuf::from("b.go"));

        // Unknown language string is a no-op (warning logged), not a filter.
        let unfiltered = apply_language_filter(mixed.clone(), Some("cobol"));
        assert_eq!(unfiltered.len(), 3);
    }

    // -------------------------------------------------------------------
    // str-jeen.17: whole-source denominator regression tests.
    //
    // The bug: prior to this fix, the run produced no JSON summary and
    // no manifest-driven `selected_source_files` / `selected_source_lines`
    // fields existed. Coverage tooling could only read per-discovered-
    // function span totals, which collapse to small numbers when most
    // exploration fails — a dishonest denominator (parent epic str-jeen).
    //
    // The fix: capture a run-start [`RunManifest`] of the discovered
    // source set and emit `selected_source_files` / `selected_source_lines`
    // from it, independent of exploration outcome.
    // -------------------------------------------------------------------

    /// Whole-source denominators must equal the manifest source-set
    /// totals even when zero functions completed (i.e. every attempted
    /// target failed). This is the failing-test-first regression for
    /// str-jeen.17: before the fix, no `RunSummary` existed and
    /// coverage tooling collapsed the denominator to the (empty)
    /// completed-function tally.
    #[test]
    fn run_summary_denominator_holds_when_all_targets_fail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // Three Go files with known line counts: 4, 6, and 0.
        fs::write(root.join("a.go"), "package a\n\nfunc A() {}\n").expect("write a.go"); // 3 lines
        fs::write(
            root.join("b.go"),
            "package b\n\nfunc B() int {\n    return 1\n}\n",
        )
        .expect("write b.go"); // 5 lines
        fs::write(root.join("c.go"), "").expect("write c.go"); // 0 lines

        let scope = RunScope::default();
        let paths = vec!["a.go".to_string(), "b.go".to_string(), "c.go".to_string()];
        let manifest =
            shatter_core::run_manifest::capture("scan-1", &scope_hash(&scope), &paths, Some(root));

        // Simulate the worst case: a discovered set of 10 functions but
        // every attempted target failed (completed=0, failed=10).
        let summary = build_run_summary("scan-1", &manifest, 0, 10, 0, &[]);

        // The denominators must equal the manifest source-set totals,
        // not collapse to zero just because nothing completed.
        assert_eq!(
            summary.selected_source_files, 3,
            "selected_source_files must equal manifest file count, not completed-function file count"
        );
        assert_eq!(
            summary.selected_source_lines, 8,
            "selected_source_lines must equal sum of manifest per-file line counts (3+5+0)"
        );
        assert_eq!(summary.completed_functions, 0);
        assert_eq!(summary.failed_functions, 10);
    }

    /// str-jeen.84: a run with zero completed but nonzero attempted
    /// functions must signal exit-nonzero. Mirrors the pickpackit
    /// regression where `shatter run` exited 0 despite
    /// `completed_functions=0, failed_functions=183`.
    #[test]
    fn run_invalidity_failure_flags_zero_complete_nonzero_attempted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("only.go"), "package a\nfunc A() {}\n").expect("write");
        let manifest = shatter_core::run_manifest::capture(
            "scan-zc",
            "cfg-h",
            &["only.go".to_string()],
            Some(root),
        );
        let mut summary = build_run_summary("scan-zc", &manifest, 0, 183, 0, &[]);
        // Validity left at the default `High` to isolate the
        // zero-complete trigger from the stale-source-set trigger.
        summary.report_validity = ReportValidity::High;

        let reason = run_invalidity_failure(&summary)
            .expect("zero-complete nonzero-attempted run must fail exit code");
        assert!(
            reason.contains("completed=0"),
            "reason must explain the zero-completed signal, got: {reason}"
        );
    }

    /// str-jeen.84: `stale-source-set` validity must trigger nonzero
    /// exit regardless of completed/attempted counts. The pickpackit
    /// case carried both signals; this isolates the stale-source-set
    /// branch so it cannot regress independently.
    #[test]
    fn run_invalidity_failure_flags_stale_source_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("only.go"), "package a\nfunc A() {}\n").expect("write");
        let manifest = shatter_core::run_manifest::capture(
            "scan-ss",
            "cfg-h",
            &["only.go".to_string()],
            Some(root),
        );
        // Successful-looking counts: completed > 0, no failures. The
        // failure trigger here is purely the validity verdict.
        let mut summary = build_run_summary("scan-ss", &manifest, 5, 0, 0, &[]);
        summary.report_validity = ReportValidity::StaleSourceSet;

        let reason = run_invalidity_failure(&summary)
            .expect("stale-source-set run must fail exit code");
        assert!(
            reason.contains("stale-source-set"),
            "reason must name the stale-source-set signal, got: {reason}"
        );
    }

    /// str-jeen.84: a healthy run (high validity, completed>0) must
    /// not trip the new exit-code policy. This pins the baseline so
    /// the new failure conditions don't accidentally fire on
    /// successful runs.
    #[test]
    fn run_invalidity_failure_passes_healthy_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("only.go"), "package a\nfunc A() {}\n").expect("write");
        let manifest = shatter_core::run_manifest::capture(
            "scan-ok",
            "cfg-h",
            &["only.go".to_string()],
            Some(root),
        );
        let mut summary = build_run_summary("scan-ok", &manifest, 10, 0, 0, &[]);
        summary.report_validity = ReportValidity::High;

        assert!(
            run_invalidity_failure(&summary).is_none(),
            "healthy run must not trip exit-code failure"
        );
    }

    /// str-jeen.84: an empty-discovery run (zero attempted, zero
    /// completed) must not trip the new exit-code policy — the
    /// command already special-cases this path and we only want to
    /// flag runs that actually attempted exploration.
    #[test]
    fn run_invalidity_failure_passes_zero_attempted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let manifest = shatter_core::run_manifest::capture("scan-empty", "cfg-h", &[], Some(root));
        let mut summary = build_run_summary("scan-empty", &manifest, 0, 0, 0, &[]);
        summary.report_validity = ReportValidity::High;

        assert!(
            run_invalidity_failure(&summary).is_none(),
            "zero-attempted run must not trip exit-code failure"
        );
    }

    /// `write_run_summary_json` round-trips through `run.json` so
    /// downstream tooling can deserialize the artifact unchanged.
    #[test]
    fn run_summary_json_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("only.go"), "package main\nfunc M() {}\n").expect("write");
        let manifest = shatter_core::run_manifest::capture(
            "scan-rt",
            "cfg-h",
            &["only.go".to_string()],
            Some(root),
        );
        let summary = build_run_summary("scan-rt", &manifest, 1, 0, 0, &[]);

        let out_dir = dir.path().join("out");
        write_run_summary_json(&out_dir, &summary).expect("write summary");

        let bytes = fs::read(out_dir.join(RUN_SUMMARY_FILENAME)).expect("read summary");
        let parsed: RunSummary = serde_json::from_slice(&bytes).expect("parse summary");
        assert_eq!(parsed, summary);
        assert_eq!(parsed.selected_source_files, 1);
        assert_eq!(parsed.selected_source_lines, 2);
    }

    // -------------------------------------------------------------------
    // str-jeen.43: no-target / undiscovered line bucket regression tests.
    //
    // The honest-denominator story (str-jeen) needs the run JSON to
    // attribute selected source lines to *why* they aren't covered:
    //   - selected file with zero discovered targets -> no_target_file_lines
    //   - line outside any discovered span in a file with targets ->
    //     undiscovered_source_lines
    // Source of truth is the run-start manifest snapshot + discovered
    // function spans, NOT completed-function outcomes.
    // -------------------------------------------------------------------

    /// A selected file with no matching discovered span contributes its
    /// whole `line_count` to `no_target_file_lines`. This is the
    /// "frontend chose not to / could not extract any target from this
    /// file" case — exactly the case the markdown denominator was
    /// silently dropping before str-jeen.
    #[test]
    fn no_target_file_lines_counts_files_without_any_span() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // Two selected files; only `b.go` has a discovered function.
        fs::write(root.join("a.go"), "package a\nfunc A() {}\nfunc B() {}\n").expect("write a.go"); // 3 lines, NO discovered targets
        fs::write(
            root.join("b.go"),
            "package b\n\nfunc B() int {\n    return 1\n}\n",
        )
        .expect("write b.go"); // 5 lines, one full-span target

        let scope = RunScope::default();
        let paths = vec!["a.go".to_string(), "b.go".to_string()];
        let manifest =
            shatter_core::run_manifest::capture("scan-1", &scope_hash(&scope), &paths, Some(root));

        // Only `b.go` has a discovered span and it covers every line.
        let spans = vec![DiscoveredSpan {
            path: "b.go".to_string(),
            start_line: 1,
            end_line: 5,
        }];
        let summary = build_run_summary("scan-1", &manifest, 1, 0, 0, &spans);

        assert_eq!(
            summary.no_target_file_lines, 3,
            "a.go (3 lines) has no discovered target, so all of its lines \
             must be in no_target_file_lines"
        );
        assert_eq!(
            summary.undiscovered_source_lines, 0,
            "b.go is fully covered by its discovered span; no gap lines"
        );
        // Sanity: the new buckets do not change the denominator.
        assert_eq!(summary.selected_source_lines, 8);
    }

    /// A file with discovered spans contributes only its *gap* lines —
    /// `line_count - covered` — to `undiscovered_source_lines`.
    /// Overlapping and adjacent spans must be merged so they aren't
    /// double-counted.
    #[test]
    fn undiscovered_source_lines_counts_gap_outside_discovered_spans() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // 10 lines total. Two functions cover lines 2-4 and 6-7
        // (5 covered lines), leaving 5 lines outside any discovered span.
        let body: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        fs::write(root.join("only.rs"), &body).expect("write only.rs");

        let scope = RunScope::default();
        let paths = vec!["only.rs".to_string()];
        let manifest =
            shatter_core::run_manifest::capture("scan-1", &scope_hash(&scope), &paths, Some(root));

        let spans = vec![
            DiscoveredSpan {
                path: "only.rs".to_string(),
                start_line: 2,
                end_line: 4,
            },
            DiscoveredSpan {
                path: "only.rs".to_string(),
                start_line: 6,
                end_line: 7,
            },
        ];
        let summary = build_run_summary("scan-1", &manifest, 0, 2, 0, &spans);

        assert_eq!(
            summary.no_target_file_lines, 0,
            "only.rs has discovered targets, so it contributes nothing to \
             no_target_file_lines"
        );
        assert_eq!(
            summary.undiscovered_source_lines, 5,
            "10-line file with spans 2-4 + 6-7 leaves 5 gap lines (1, 5, 8, 9, 10)"
        );
        assert_eq!(summary.selected_source_lines, 10);
    }

    /// `compute_line_buckets` must merge overlapping / adjacent spans
    /// rather than double-counting their union. Regression for the
    /// trivial `sum(end-start+1)` mistake.
    #[test]
    fn compute_line_buckets_merges_overlapping_and_adjacent_spans() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let body: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        fs::write(root.join("f.rs"), &body).expect("write f.rs");

        let manifest =
            shatter_core::run_manifest::capture("s", "h", &["f.rs".to_string()], Some(root));

        // Spans 1-3, 3-5 (overlap), 6-7 (adjacent to 3-5 → merge to 1-7),
        // and a redundant 2-4. Naive sum would say 3+3+2+3=11; the
        // correct union covers lines 1..=7 (7 lines), gap = 3 (8,9,10).
        let spans = vec![
            DiscoveredSpan {
                path: "f.rs".to_string(),
                start_line: 1,
                end_line: 3,
            },
            DiscoveredSpan {
                path: "f.rs".to_string(),
                start_line: 3,
                end_line: 5,
            },
            DiscoveredSpan {
                path: "f.rs".to_string(),
                start_line: 6,
                end_line: 7,
            },
            DiscoveredSpan {
                path: "f.rs".to_string(),
                start_line: 2,
                end_line: 4,
            },
        ];
        let buckets = compute_line_buckets(&manifest, &spans);
        assert_eq!(buckets.no_target_file_lines, 0);
        assert_eq!(buckets.undiscovered_source_lines, 3);
    }

    /// Spans that extend past the file's `line_count` are clamped, and
    /// degenerate spans (start = 0 or start > line_count) are skipped
    /// rather than panicking — frontends emit a wide range of span
    /// shapes and the bucket must be defensive.
    #[test]
    fn compute_line_buckets_clamps_and_skips_degenerate_spans() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let body: String = (1..=5).map(|i| format!("line{i}\n")).collect();
        fs::write(root.join("g.rs"), &body).expect("write g.rs");

        let manifest =
            shatter_core::run_manifest::capture("s", "h", &["g.rs".to_string()], Some(root));
        let spans = vec![
            // Clamps to 3..=5 (covers 3 lines).
            DiscoveredSpan {
                path: "g.rs".to_string(),
                start_line: 3,
                end_line: 99,
            },
            // start = 0 is skipped.
            DiscoveredSpan {
                path: "g.rs".to_string(),
                start_line: 0,
                end_line: 2,
            },
            // start > line_count is skipped.
            DiscoveredSpan {
                path: "g.rs".to_string(),
                start_line: 100,
                end_line: 200,
            },
        ];
        let buckets = compute_line_buckets(&manifest, &spans);
        // Lines 1 and 2 are the gap.
        assert_eq!(buckets.undiscovered_source_lines, 2);
        // g.rs has at least one valid discovered span, so no_target_file_lines = 0.
        assert_eq!(buckets.no_target_file_lines, 0);
    }

    /// Spans whose `path` does not match any manifest entry are
    /// ignored — they cannot magically register the file as having a
    /// target. This catches path-normalization mismatches between
    /// discovery and the manifest snapshot.
    #[test]
    fn compute_line_buckets_ignores_spans_pointing_outside_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("in.rs"), "a\nb\nc\n").expect("write in.rs"); // 3 lines

        let manifest =
            shatter_core::run_manifest::capture("s", "h", &["in.rs".to_string()], Some(root));
        let spans = vec![DiscoveredSpan {
            // Wrong path — does not match the manifest entry.
            path: "out.rs".to_string(),
            start_line: 1,
            end_line: 3,
        }];
        let buckets = compute_line_buckets(&manifest, &spans);
        assert_eq!(
            buckets.no_target_file_lines, 3,
            "in.rs has no matching span, so all 3 lines go to no_target_file_lines"
        );
        assert_eq!(buckets.undiscovered_source_lines, 0);
    }

    /// The real run path builds spans from `FunctionRegistry`, whose
    /// paths can be absolute even though manifest paths are stored
    /// relative to the run root. If this normalization drifts, a file
    /// with discovered functions looks like a no-target file.
    #[test]
    fn registry_spans_normalizes_paths_to_manifest_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src");
        let file_path = src_dir.join("a.rs");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").expect("write a.rs");

        let entry = batch_analyze::FunctionEntry {
            file_path: file_path.clone(),
            name: "a".to_string(),
            exported: true,
            params: vec![],
            return_type: shatter_core::types::TypeInfo::Unknown,
            dependencies: vec![],
            crypto_boundaries: vec![],
            branch_count: 0,
            start_line: 2,
            end_line: 3,
        };
        let qualified =
            batch_analyze::FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
        let mut index = HashMap::new();
        index.insert(qualified, 0);
        let registry = batch_analyze::FunctionRegistry::from_raw(vec![entry], index);

        let spans = registry_spans(&registry, root);
        assert_eq!(spans[0].path, "src/a.rs");

        let manifest = shatter_core::run_manifest::capture(
            "scan-1",
            "cfg",
            &["src/a.rs".to_string()],
            Some(root),
        );
        let summary = build_run_summary("scan-1", &manifest, 1, 0, 0, &spans);
        assert_eq!(summary.no_target_file_lines, 0);
        assert_eq!(summary.undiscovered_source_lines, 2);
    }

    proptest! {
        #[test]
        fn line_buckets_never_exceed_selected_source_lines(
            line_count in 0u32..200,
            raw_spans in proptest::collection::vec((0u32..250, 0u32..250), 0..50),
        ) {
            let manifest = RunManifest {
                version: shatter_core::run_manifest::RUN_MANIFEST_VERSION,
                scan_id: "prop".to_string(),
                project_root: None,
                repo_root: None,
                cwd: String::new(),
                git_commit: None,
                git_dirty: None,
                scope_hash: "scope".to_string(),
                source_files: vec![shatter_core::run_manifest::SourceFileSnapshot {
                    path: "f.rs".to_string(),
                    size: 0,
                    mtime_ns: None,
                    content_hash: None,
                    line_count: Some(line_count),
                }],
                captured_at_ns: 0,
            };
            let spans: Vec<DiscoveredSpan> = raw_spans
                .into_iter()
                .map(|(start_line, end_line)| DiscoveredSpan {
                    path: "f.rs".to_string(),
                    start_line,
                    end_line,
                })
                .collect();

            let buckets = compute_line_buckets(&manifest, &spans);
            let attributed = buckets
                .no_target_file_lines
                .saturating_add(buckets.undiscovered_source_lines);

            prop_assert!(
                attributed <= u64::from(line_count),
                "line buckets must not exceed selected source lines"
            );
            if spans.is_empty() {
                prop_assert_eq!(buckets.no_target_file_lines, u64::from(line_count));
                prop_assert_eq!(buckets.undiscovered_source_lines, 0);
            } else {
                prop_assert_eq!(buckets.no_target_file_lines, 0);
            }
        }

        #[test]
        fn source_representation_buckets_never_exceed_selected_source_lines(
            line_count in 0u32..200,
            raw_spans in proptest::collection::vec((0u32..250, 0u32..250, 0u8..4), 0..50),
        ) {
            let manifest = RunManifest {
                version: shatter_core::run_manifest::RUN_MANIFEST_VERSION,
                scan_id: "prop".to_string(),
                project_root: None,
                repo_root: None,
                cwd: String::new(),
                git_commit: None,
                git_dirty: None,
                scope_hash: "scope".to_string(),
                source_files: vec![shatter_core::run_manifest::SourceFileSnapshot {
                    path: "f.rs".to_string(),
                    size: 0,
                    mtime_ns: None,
                    content_hash: None,
                    line_count: Some(line_count),
                }],
                captured_at_ns: 0,
            };
            let spans: Vec<SourceRepresentationSpan> = raw_spans
                .into_iter()
                .map(|(start_line, end_line, outcome)| SourceRepresentationSpan {
                    path: "f.rs".to_string(),
                    start_line,
                    end_line,
                    outcome: match outcome {
                        0 => SourceRepresentationOutcome::Represented,
                        1 => SourceRepresentationOutcome::Failed,
                        2 => SourceRepresentationOutcome::TimedOut,
                        _ => SourceRepresentationOutcome::Unsupported,
                    },
                })
                .collect();

            let buckets = compute_source_representation(&manifest, &spans);
            let attributed = buckets
                .represented_source_lines
                .saturating_add(buckets.unrepresented_failed_lines)
                .saturating_add(buckets.unrepresented_timed_out_lines)
                .saturating_add(buckets.unrepresented_unsupported_lines)
                .saturating_add(buckets.unrepresented_no_target_lines)
                .saturating_add(buckets.unrepresented_undiscovered_lines);

            prop_assert!(
                attributed <= u64::from(line_count),
                "source representation buckets must not exceed selected source lines"
            );
            if spans.is_empty() {
                prop_assert_eq!(buckets.unrepresented_no_target_lines, u64::from(line_count));
                prop_assert_eq!(buckets.unrepresented_undiscovered_lines, 0);
            } else {
                prop_assert_eq!(buckets.unrepresented_no_target_lines, 0);
            }
        }
    }

    /// `RunSummary` round-trips the new bucket fields through `run.json`
    /// — protects the JSON schema additivity contract (str-jeen.43).
    #[test]
    fn run_summary_round_trips_line_buckets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("a.rs"), "1\n2\n3\n").expect("write a.rs"); // 3 lines, no targets
        fs::write(root.join("b.rs"), "1\n2\n3\n4\n5\n").expect("write b.rs"); // 5 lines, span 1-2

        let manifest = shatter_core::run_manifest::capture(
            "scan-rt",
            "cfg",
            &["a.rs".to_string(), "b.rs".to_string()],
            Some(root),
        );
        let spans = vec![DiscoveredSpan {
            path: "b.rs".to_string(),
            start_line: 1,
            end_line: 2,
        }];
        let summary = build_run_summary("scan-rt", &manifest, 1, 0, 0, &spans);
        assert_eq!(summary.no_target_file_lines, 3);
        assert_eq!(summary.undiscovered_source_lines, 3);

        let out_dir = dir.path().join("out");
        write_run_summary_json(&out_dir, &summary).expect("write summary");
        let bytes = fs::read(out_dir.join(RUN_SUMMARY_FILENAME)).expect("read summary");
        let parsed: RunSummary = serde_json::from_slice(&bytes).expect("parse summary");
        assert_eq!(parsed, summary);

        // Backward-compat: a JSON missing the new fields must still
        // parse (additive fields use #[serde(default)]).
        let legacy = serde_json::json!({
            "version": RUN_SUMMARY_VERSION,
            "scan_id": "legacy",
            "selected_source_files": 2,
            "selected_source_lines": 8,
            "completed_functions": 0,
            "failed_functions": 0,
        });
        let parsed_legacy: RunSummary =
            serde_json::from_value(legacy).expect("parse legacy summary");
        assert_eq!(parsed_legacy.no_target_file_lines, 0);
        assert_eq!(parsed_legacy.undiscovered_source_lines, 0);
        assert_eq!(parsed_legacy.skipped_functions, 0);
    }

    // -------------------------------------------------------------------
    // str-jeen.44: source representation metrics.
    //
    // Broad-run coverage must show how much of the selected source set
    // is represented by completed exploration, and how the unrepresented
    // remainder splits across failure, timeout, unsupported, no-target,
    // and undiscovered source lines.
    // -------------------------------------------------------------------

    #[test]
    fn source_representation_metrics_partition_mixed_source_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("completed.rs"), "1\n2\n3\n4\n").expect("write completed.rs"); // 4
        fs::write(root.join("failed.rs"), "1\n2\n3\n").expect("write failed.rs"); // 3
        fs::write(root.join("timeout.rs"), "1\n2\n").expect("write timeout.rs"); // 2
        fs::write(root.join("unsupported.rs"), "1\n").expect("write unsupported.rs"); // 1
        fs::write(root.join("no_target.rs"), "1\n2\n3\n4\n").expect("write no_target.rs"); // 4
        fs::write(root.join("gap.rs"), "1\n2\n3\n4\n5\n6\n").expect("write gap.rs"); // 6, span 1-2

        let paths = vec![
            "completed.rs".to_string(),
            "failed.rs".to_string(),
            "timeout.rs".to_string(),
            "unsupported.rs".to_string(),
            "no_target.rs".to_string(),
            "gap.rs".to_string(),
        ];
        let manifest = shatter_core::run_manifest::capture("scan-1", "cfg", &paths, Some(root));

        let spans = vec![
            SourceRepresentationSpan {
                path: "completed.rs".to_string(),
                start_line: 1,
                end_line: 4,
                outcome: SourceRepresentationOutcome::Represented,
            },
            SourceRepresentationSpan {
                path: "failed.rs".to_string(),
                start_line: 1,
                end_line: 3,
                outcome: SourceRepresentationOutcome::Failed,
            },
            SourceRepresentationSpan {
                path: "timeout.rs".to_string(),
                start_line: 1,
                end_line: 2,
                outcome: SourceRepresentationOutcome::TimedOut,
            },
            SourceRepresentationSpan {
                path: "unsupported.rs".to_string(),
                start_line: 1,
                end_line: 1,
                outcome: SourceRepresentationOutcome::Unsupported,
            },
            SourceRepresentationSpan {
                path: "gap.rs".to_string(),
                start_line: 1,
                end_line: 2,
                outcome: SourceRepresentationOutcome::Represented,
            },
        ];

        let summary = build_run_summary_with_representation("scan-1", &manifest, 2, 1, 0, &spans);

        assert_eq!(summary.selected_source_lines, 20);
        assert_eq!(summary.represented_source_lines, 6);
        assert_eq!(summary.unrepresented_failed_lines, 3);
        assert_eq!(summary.unrepresented_timed_out_lines, 2);
        assert_eq!(summary.unrepresented_unsupported_lines, 1);
        assert_eq!(summary.unrepresented_no_target_lines, 4);
        assert_eq!(summary.unrepresented_undiscovered_lines, 4);

        assert_percent(summary.represented_source_percent, 30.0);
        assert_percent(summary.unrepresented_failed_percent, 15.0);
        assert_percent(summary.unrepresented_timed_out_percent, 10.0);
        assert_percent(summary.unrepresented_unsupported_percent, 5.0);
        assert_percent(summary.unrepresented_no_target_percent, 20.0);
        assert_percent(summary.unrepresented_undiscovered_percent, 20.0);
    }

    fn assert_percent(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    /// str-jeen.42: acceptance-level guard that every documented source-line
    /// bucket sees nonzero traffic in a mixed run AND that the six buckets
    /// partition `selected_source_lines` exactly. This is the test that
    /// must fail if any failure or unsupported span gets dropped from the
    /// denominator instead of being attributed to a visible bucket.
    ///
    /// Differs from `source_representation_metrics_partition_mixed_source_set`:
    /// that test pins per-bucket totals on a small fixture but never asserts
    /// the partition-sum invariant. This one constructs a fixture covering
    /// every bucket — including separate `build_failed` and `runtime_failed`
    /// reason strings routed through
    /// [`source_representation_outcome_from_failure_reason`] so the
    /// reason-string classifier is exercised end-to-end — and asserts the
    /// sum equals the manifest denominator.
    ///
    /// The issue's bucket list names `build_failed` and `runtime_failed`
    /// separately. The implementation collapses both into the single
    /// [`SourceRepresentationOutcome::Failed`] bucket
    /// (`unrepresented_failed_lines`); this test verifies that both
    /// reason flavors land in that bucket rather than vanishing.
    #[test]
    fn mixed_outcome_bucket_partition_holds_for_every_outcome() {
        // Per-file line counts. Distinct primes/small numbers make
        // attribution failures easy to read in the assertion message.
        const COMPLETED_LINES: u32 = 5;
        const BUILD_FAILED_LINES: u32 = 4;
        const RUNTIME_FAILED_LINES: u32 = 3;
        const TIMED_OUT_LINES: u32 = 2;
        const UNSUPPORTED_LINES: u32 = 6;
        const NO_TARGET_LINES: u32 = 7;
        const GAP_FILE_LINES: u32 = 8;
        // Span on `gap.rs` covers the first three lines, leaving five lines
        // unattributed to any discovered span — those land in the
        // `undiscovered` bucket.
        const GAP_REPRESENTED_LINES: u32 = 3;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_lines(root, "completed.rs", COMPLETED_LINES);
        write_lines(root, "build_failed.rs", BUILD_FAILED_LINES);
        write_lines(root, "runtime_failed.rs", RUNTIME_FAILED_LINES);
        write_lines(root, "timed_out.rs", TIMED_OUT_LINES);
        write_lines(root, "unsupported.rs", UNSUPPORTED_LINES);
        write_lines(root, "no_target.rs", NO_TARGET_LINES);
        write_lines(root, "gap.rs", GAP_FILE_LINES);

        let paths = vec![
            "completed.rs".to_string(),
            "build_failed.rs".to_string(),
            "runtime_failed.rs".to_string(),
            "timed_out.rs".to_string(),
            "unsupported.rs".to_string(),
            "no_target.rs".to_string(),
            "gap.rs".to_string(),
        ];
        let manifest = shatter_core::run_manifest::capture("scan-1", "cfg", &paths, Some(root));

        // Reason strings exercise the reason → outcome classifier the
        // production path uses. Build- and runtime-failure reasons must
        // both resolve to `Failed`; "timed out" → `TimedOut`;
        // "unsupported" → `Unsupported`.
        let build_failed_outcome =
            source_representation_outcome_from_failure_reason("build failed: cargo error");
        let runtime_failed_outcome =
            source_representation_outcome_from_failure_reason("runtime panic: divide by zero");
        let timed_out_outcome =
            source_representation_outcome_from_failure_reason("timed out after 5s");
        let unsupported_outcome =
            source_representation_outcome_from_failure_reason("unsupported syntax in frontend");
        assert_eq!(
            build_failed_outcome,
            SourceRepresentationOutcome::Failed,
            "build-failure reason must classify as Failed",
        );
        assert_eq!(
            runtime_failed_outcome,
            SourceRepresentationOutcome::Failed,
            "runtime-failure reason must classify as Failed",
        );
        assert_eq!(
            timed_out_outcome,
            SourceRepresentationOutcome::TimedOut,
            "timeout reason must classify as TimedOut",
        );
        assert_eq!(
            unsupported_outcome,
            SourceRepresentationOutcome::Unsupported,
            "unsupported reason must classify as Unsupported",
        );

        let spans = vec![
            SourceRepresentationSpan {
                path: "completed.rs".to_string(),
                start_line: 1,
                end_line: COMPLETED_LINES,
                outcome: SourceRepresentationOutcome::Represented,
            },
            SourceRepresentationSpan {
                path: "build_failed.rs".to_string(),
                start_line: 1,
                end_line: BUILD_FAILED_LINES,
                outcome: build_failed_outcome,
            },
            SourceRepresentationSpan {
                path: "runtime_failed.rs".to_string(),
                start_line: 1,
                end_line: RUNTIME_FAILED_LINES,
                outcome: runtime_failed_outcome,
            },
            SourceRepresentationSpan {
                path: "timed_out.rs".to_string(),
                start_line: 1,
                end_line: TIMED_OUT_LINES,
                outcome: timed_out_outcome,
            },
            SourceRepresentationSpan {
                path: "unsupported.rs".to_string(),
                start_line: 1,
                end_line: UNSUPPORTED_LINES,
                outcome: unsupported_outcome,
            },
            // `no_target.rs` is omitted from spans on purpose — that's how
            // a selected file with zero discovered targets reaches the
            // `unrepresented_no_target_lines` bucket.
            SourceRepresentationSpan {
                path: "gap.rs".to_string(),
                start_line: 1,
                end_line: GAP_REPRESENTED_LINES,
                outcome: SourceRepresentationOutcome::Represented,
            },
        ];

        let summary = build_run_summary_with_representation("scan-1", &manifest, 2, 2, 0, &spans);

        let expected_selected: u64 = u64::from(COMPLETED_LINES)
            + u64::from(BUILD_FAILED_LINES)
            + u64::from(RUNTIME_FAILED_LINES)
            + u64::from(TIMED_OUT_LINES)
            + u64::from(UNSUPPORTED_LINES)
            + u64::from(NO_TARGET_LINES)
            + u64::from(GAP_FILE_LINES);
        assert_eq!(
            summary.selected_source_lines, expected_selected,
            "manifest denominator must equal the sum of per-file line counts",
        );

        // Every documented bucket must see traffic — a regression that
        // dropped (say) unsupported spans on the floor would zero this
        // bucket while the others still totalled the denominator.
        assert!(
            summary.represented_source_lines > 0,
            "represented_source_lines must be > 0 in a mixed run; got {}",
            summary.represented_source_lines,
        );
        assert!(
            summary.unrepresented_failed_lines > 0,
            "unrepresented_failed_lines must be > 0; build_failed and \
             runtime_failed spans must count somewhere visible. got {}",
            summary.unrepresented_failed_lines,
        );
        assert!(
            summary.unrepresented_timed_out_lines > 0,
            "unrepresented_timed_out_lines must be > 0; got {}",
            summary.unrepresented_timed_out_lines,
        );
        assert!(
            summary.unrepresented_unsupported_lines > 0,
            "unrepresented_unsupported_lines must be > 0; unsupported \
             spans must count somewhere visible. got {}",
            summary.unrepresented_unsupported_lines,
        );
        assert!(
            summary.unrepresented_no_target_lines > 0,
            "unrepresented_no_target_lines must be > 0; got {}",
            summary.unrepresented_no_target_lines,
        );
        assert!(
            summary.unrepresented_undiscovered_lines > 0,
            "unrepresented_undiscovered_lines must be > 0; got {}",
            summary.unrepresented_undiscovered_lines,
        );

        // Per-bucket totals match the fixture. Pinning these alongside
        // the partition-sum check makes a regression point at the
        // exact bucket that drifted.
        assert_eq!(
            summary.represented_source_lines,
            u64::from(COMPLETED_LINES + GAP_REPRESENTED_LINES)
        );
        assert_eq!(
            summary.unrepresented_failed_lines,
            u64::from(BUILD_FAILED_LINES + RUNTIME_FAILED_LINES),
            "both build-failed and runtime-failed spans must contribute \
             to unrepresented_failed_lines",
        );
        assert_eq!(
            summary.unrepresented_timed_out_lines,
            u64::from(TIMED_OUT_LINES)
        );
        assert_eq!(
            summary.unrepresented_unsupported_lines,
            u64::from(UNSUPPORTED_LINES)
        );
        assert_eq!(
            summary.unrepresented_no_target_lines,
            u64::from(NO_TARGET_LINES)
        );
        assert_eq!(
            summary.unrepresented_undiscovered_lines,
            u64::from(GAP_FILE_LINES - GAP_REPRESENTED_LINES),
        );

        // The partition invariant: the six source-line buckets must sum
        // to `selected_source_lines`. A regression that omits any bucket
        // from the denominator — failed or unsupported spans dropping
        // out, for instance — fails here.
        let bucket_sum = summary.represented_source_lines
            + summary.unrepresented_failed_lines
            + summary.unrepresented_timed_out_lines
            + summary.unrepresented_unsupported_lines
            + summary.unrepresented_no_target_lines
            + summary.unrepresented_undiscovered_lines;
        assert_eq!(
            bucket_sum,
            summary.selected_source_lines,
            "documented six-bucket partition must sum to \
             selected_source_lines; bucket_sum={bucket_sum}, \
             selected_source_lines={}, per-bucket: \
             represented={}, failed={}, timed_out={}, unsupported={}, \
             no_target={}, undiscovered={}",
            summary.selected_source_lines,
            summary.represented_source_lines,
            summary.unrepresented_failed_lines,
            summary.unrepresented_timed_out_lines,
            summary.unrepresented_unsupported_lines,
            summary.unrepresented_no_target_lines,
            summary.unrepresented_undiscovered_lines,
        );

        // Percent fields must be consistent with their line counts.
        let expected_represented_percent =
            (summary.represented_source_lines as f64 / expected_selected as f64) * 100.0;
        assert_percent(
            summary.represented_source_percent,
            expected_represented_percent,
        );
    }

    /// Write a file with `lines` newline-terminated lines so its
    /// `line_count` snapshot matches `lines` exactly.
    fn write_lines(root: &Path, name: &str, lines: u32) {
        let mut content = String::new();
        for i in 1..=lines {
            content.push_str(&i.to_string());
            content.push('\n');
        }
        fs::write(root.join(name), content).unwrap_or_else(|e| panic!("write {name}: {e}"));
    }

    /// Scope hash is deterministic for the same scope and changes when
    /// any scope field changes — so a `run` whose include/exclude/
    /// language/max_depth differs gets a distinct manifest fingerprint.
    #[test]
    fn scope_hash_is_stable_and_change_sensitive() {
        let mut a = RunScope::default();
        a.options.include_patterns = vec!["**/*.go".to_string()];
        let mut b = a.clone();
        assert_eq!(scope_hash(&a), scope_hash(&b));
        b.options.exclude_patterns.push("vendor/**".to_string());
        assert_ne!(scope_hash(&a), scope_hash(&b));
    }

    // ---------------------------------------------------------------
    // str-jeen.5: report validity classifier and renderer
    // ---------------------------------------------------------------

    /// Build a baseline summary anchored on a 100-line denominator so
    /// `represented_source_percent` is just `represented_source_lines`.
    fn synth_summary(rep_lines: u64, completed: usize, failed: usize) -> RunSummary {
        RunSummary {
            version: RUN_SUMMARY_VERSION,
            scan_id: "synth".to_string(),
            selected_source_files: 1,
            selected_source_lines: 100,
            completed_functions: completed,
            failed_functions: failed,
            skipped_functions: 0,
            no_target_file_lines: 0,
            undiscovered_source_lines: 0,
            represented_source_lines: rep_lines,
            represented_source_percent: rep_lines as f64,
            unrepresented_failed_lines: 0,
            unrepresented_failed_percent: 0.0,
            unrepresented_timed_out_lines: 0,
            unrepresented_timed_out_percent: 0.0,
            unrepresented_unsupported_lines: 0,
            unrepresented_unsupported_percent: 0.0,
            unrepresented_no_target_lines: 0,
            unrepresented_no_target_percent: 0.0,
            unrepresented_undiscovered_lines: 0,
            unrepresented_undiscovered_percent: 0.0,
            report_validity: ReportValidity::High,
            validity_reasons: Vec::new(),
            gate_decisions: Vec::new(),
        }
    }

    fn reason_codes(reasons: &[ValidityReason]) -> Vec<&str> {
        reasons.iter().map(|r| r.code.as_str()).collect()
    }

    #[test]
    fn classify_validity_high_when_clean_and_well_represented() {
        let summary = synth_summary(90, 9, 1);
        let (tier, reasons) = classify_validity(&summary, None, &[]);
        assert_eq!(tier, ReportValidity::High);
        assert!(reasons.is_empty(), "got {reasons:?}");
    }

    #[test]
    fn classify_validity_degraded_below_high_threshold() {
        let summary = synth_summary(50, 5, 5);
        let (tier, reasons) = classify_validity(&summary, None, &[]);
        assert_eq!(tier, ReportValidity::Degraded);
        assert!(reason_codes(&reasons).contains(&"degraded_representation"));
    }

    #[test]
    fn classify_validity_low_below_low_threshold() {
        let summary = synth_summary(10, 1, 9);
        let (tier, reasons) = classify_validity(&summary, None, &[]);
        assert_eq!(tier, ReportValidity::Low);
        assert!(reason_codes(&reasons).contains(&"low_representation"));
    }

    #[test]
    fn classify_validity_kapow_attaches_reason_code_at_low_tier() {
        // Kapow: zero completed but exploration was attempted with
        // many failures. rep% = 0 → tier `low`; reason code documents
        // the specific shape so callers don't read it as success.
        let summary = synth_summary(0, 0, 42);
        let (tier, reasons) = classify_validity(&summary, None, &[]);
        assert_eq!(tier, ReportValidity::Low);
        let codes = reason_codes(&reasons);
        assert!(codes.contains(&"low_representation"));
        assert!(codes.contains(&"kapow_tiny_denominator"));
    }

    #[test]
    fn classify_validity_demotes_to_degraded_on_high_unrepresented_failures() {
        // Representation is healthy (rep% >= 75) but failed-lines
        // share crosses the bar — degrades to `degraded`.
        let mut summary = synth_summary(80, 8, 2);
        summary.unrepresented_failed_percent = 30.0;
        let (tier, reasons) = classify_validity(&summary, None, &[]);
        assert_eq!(tier, ReportValidity::Degraded);
        assert!(reason_codes(&reasons).contains(&"high_unrepresented_failures"));
    }

    #[test]
    fn classify_validity_stale_source_set_overrides_representation_tier() {
        let summary = synth_summary(95, 9, 1); // would be `high`
        let mut diff = run_manifest::ManifestDiff::default();
        diff.changed.push("src/foo.rs".to_string());
        let (tier, reasons) = classify_validity(&summary, Some(&diff), &[]);
        assert_eq!(tier, ReportValidity::StaleSourceSet);
        assert!(reason_codes(&reasons).contains(&"stale_source_set_changed"));
    }

    #[test]
    fn classify_validity_invalid_artifacts_is_worst_tier() {
        // Even with stale source set + low representation, missing
        // artifacts wins because the report references files that
        // don't exist.
        let summary = synth_summary(5, 0, 50);
        let mut diff = run_manifest::ManifestDiff::default();
        diff.removed.push("src/dropped.rs".to_string());
        let missing = vec!["pkg__mod__fn.md".to_string()];
        let (tier, reasons) = classify_validity(&summary, Some(&diff), &missing);
        assert_eq!(tier, ReportValidity::InvalidArtifacts);
        assert!(reason_codes(&reasons).contains(&"invalid_artifacts_missing"));
    }

    #[test]
    fn coverage_budget_gates_emit_failed_threshold_decisions() {
        let mut summary = synth_summary(40, 4, 6);
        summary.unrepresented_failed_lines = 15;
        summary.unrepresented_timed_out_lines = 5;
        summary.unrepresented_failed_percent = 15.0;
        summary.unrepresented_timed_out_percent = 5.0;
        summary.unrepresented_unsupported_lines = 12;
        summary.unrepresented_unsupported_percent = 12.0;
        summary.report_validity = ReportValidity::Low;

        let gates = shatter_core::config::CoverageBudgetGates {
            min_source_representation_percent: Some(50.0),
            max_failed_span_percent: Some(10.0),
            max_unsupported_span_percent: Some(5.0),
            fail_on_low_report_validity: Some(true),
            ..shatter_core::config::CoverageBudgetGates::default()
        };

        let decisions = evaluate_coverage_budget_gates(&summary, &gates);
        assert_eq!(decisions.len(), 4);
        assert!(decisions.iter().all(|decision| decision.status == "failed"));
        assert!(decisions.iter().any(|decision| {
            decision.gate == "min_source_representation_percent"
                && decision.threshold.as_deref() == Some("50.0")
                && decision.observed.as_deref() == Some("40.0")
        }));
        assert!(decisions.iter().any(|decision| {
            decision.gate == "max_failed_span_percent"
                && decision.threshold.as_deref() == Some("10.0")
                && decision.observed.as_deref() == Some("20.0")
        }));
        assert!(decisions.iter().any(|decision| {
            decision.gate == "max_unsupported_span_percent"
                && decision.threshold.as_deref() == Some("5.0")
                && decision.observed.as_deref() == Some("12.0")
        }));
        assert!(decisions.iter().any(|decision| {
            decision.gate == "fail_on_low_report_validity"
                && decision.observed.as_deref() == Some("low")
        }));
    }

    #[test]
    fn run_summary_gate_decisions_round_trip_through_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut summary = synth_summary(90, 9, 1);
        summary.gate_decisions = vec![StatusGateDecision {
            gate: "min_source_representation_percent".to_string(),
            status: "passed".to_string(),
            threshold: Some("75.0".to_string()),
            observed: Some("90.0".to_string()),
            reason: None,
        }];

        write_run_summary_json(dir.path(), &summary).expect("write");
        let bytes = fs::read(dir.path().join(RUN_SUMMARY_FILENAME)).expect("read");
        let parsed: RunSummary = serde_json::from_slice(&bytes).expect("parse");

        assert_eq!(parsed.gate_decisions.len(), 1);
        assert_eq!(
            parsed.gate_decisions[0].gate,
            "min_source_representation_percent"
        );

        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).expect("reparse");
        legacy
            .as_object_mut()
            .expect("object")
            .remove("gate_decisions");
        let legacy_parsed: RunSummary = serde_json::from_value(legacy).expect("legacy parse");
        assert!(legacy_parsed.gate_decisions.is_empty());
    }

    #[test]
    fn classify_validity_high_threshold_boundary_is_inclusive() {
        // rep% exactly at the high threshold should classify as high
        // (no representation reason). Below it strictly demotes.
        let at = synth_summary(75, 7, 3);
        let (tier_at, reasons_at) = classify_validity(&at, None, &[]);
        assert_eq!(tier_at, ReportValidity::High);
        assert!(reasons_at.is_empty());

        let mut just_below = synth_summary(75, 7, 3);
        just_below.represented_source_percent = 74.99;
        let (tier_below, _) = classify_validity(&just_below, None, &[]);
        assert_eq!(tier_below, ReportValidity::Degraded);
    }

    #[test]
    fn render_validity_markdown_emits_verdict_and_reasons() {
        let reasons = vec![ValidityReason {
            code: "degraded_representation".to_string(),
            detail: "rep%=50.0".to_string(),
            recommended_action: "Inspect buckets.".to_string(),
        }];
        let md = render_validity_markdown(ReportValidity::Degraded, &reasons);
        assert!(md.contains("## Report Validity: degraded"));
        assert!(md.contains("degraded_representation"));
        assert!(md.contains("rep%=50.0"));
        assert!(md.contains("Inspect buckets."));
    }

    #[test]
    fn render_validity_markdown_high_run_collapses_to_no_issues() {
        let md = render_validity_markdown(ReportValidity::High, &[]);
        assert!(md.contains("## Report Validity: high"));
        assert!(md.contains("No validity issues detected."));
    }

    #[test]
    fn render_validity_markdown_escapes_pipe_in_detail() {
        // Detail strings carrying a pipe character must not break the
        // markdown table layout.
        let reasons = vec![ValidityReason {
            code: "x".to_string(),
            detail: "a|b".to_string(),
            recommended_action: "c|d".to_string(),
        }];
        let md = render_validity_markdown(ReportValidity::Low, &reasons);
        assert!(md.contains("a\\|b"));
        assert!(md.contains("c\\|d"));
    }

    #[test]
    fn run_summary_validity_round_trips_through_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(root.join("a.rs"), "x\n").expect("write");
        let manifest =
            shatter_core::run_manifest::capture("scan-v", "h", &["a.rs".to_string()], Some(root));
        let mut summary = build_run_summary("scan-v", &manifest, 1, 0, 0, &[]);
        summary.report_validity = ReportValidity::Degraded;
        summary.validity_reasons.push(ValidityReason {
            code: "degraded_representation".to_string(),
            detail: "rt".to_string(),
            recommended_action: "act".to_string(),
        });
        let out_dir = root.join("out");
        write_run_summary_json(&out_dir, &summary).expect("write");
        let bytes = fs::read(out_dir.join(RUN_SUMMARY_FILENAME)).expect("read");
        let parsed: RunSummary = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(parsed.report_validity, ReportValidity::Degraded);
        assert_eq!(parsed.validity_reasons.len(), 1);
        assert_eq!(parsed.validity_reasons[0].code, "degraded_representation");

        // Legacy payload (no validity fields) deserializes as `high`
        // with empty reasons — the additive default covers the
        // backward-compat contract.
        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).expect("reparse");
        let obj = legacy.as_object_mut().unwrap();
        obj.remove("report_validity");
        obj.remove("validity_reasons");
        let legacy_bytes = serde_json::to_vec(&legacy).expect("legacy ser");
        let legacy_parsed: RunSummary =
            serde_json::from_slice(&legacy_bytes).expect("legacy parse");
        assert_eq!(legacy_parsed.report_validity, ReportValidity::High);
        assert!(legacy_parsed.validity_reasons.is_empty());
    }

    /// str-ux7q: the shared artifact-reference validator must flag a
    /// per-function `<safe_name>.md` that `write_run_report` was
    /// supposed to produce but that is absent on disk. Replaces the
    /// older `missing_run_report_artifacts_flags_absent_files` test —
    /// same scenario, now driven through the cross-command validator
    /// so the `invalid_artifacts_missing` reason fires on the same
    /// signal in both commands.
    #[test]
    fn validate_run_artifact_references_flags_missing_per_function_md() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path();
        let results: Vec<(String, explorer::ObservationOutput)> = vec![
            ("pkg::a".to_string(), explorer::ObservationOutput::default()),
            ("pkg::b".to_string(), explorer::ObservationOutput::default()),
        ];
        fs::write(out.join("pkg__a.md"), "").expect("write a");
        let failures: HashMap<String, String> = HashMap::new();
        let issues = validate_run_artifact_references(out, &results, &failures);
        assert_eq!(issues.len(), 1, "got issues={issues:?}");
        let only = &issues[0];
        assert!(only.starts_with("missing_artifact:"), "issue={only}");
        assert!(only.contains("path=pkg__b.md"), "issue={only}");
    }

    /// str-ux7q: an exploration failure with an empty reason token
    /// is a contract violation — downstream consumers can't classify
    /// the row. The shared validator treats this the same way it
    /// does for explore-shaped rows that lack an `unavailable_reason`.
    #[test]
    fn validate_run_artifact_references_flags_failure_with_empty_reason() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path();
        let results: Vec<(String, explorer::ObservationOutput)> = Vec::new();
        let mut failures: HashMap<String, String> = HashMap::new();
        // Empty reason = no token recorded. Validator must surface
        // this so the run report can't claim the failure was
        // categorized.
        failures.insert("pkg::broken".to_string(), String::new());
        let issues = validate_run_artifact_references(out, &results, &failures);
        assert_eq!(issues.len(), 1, "got issues={issues:?}");
        assert!(
            issues[0].starts_with("missing_unavailable_reason:"),
            "issue={}",
            issues[0]
        );
    }

    /// str-ux7q: an unreferenced `*.md` left over from a prior run is
    /// surfaced as `stale_extra` so the report can't quietly include
    /// out-of-band files. `run.json` is on the validator's skip list
    /// (it's the run summary control file, not a per-function
    /// artifact), so it must not trip the stale-extra rule.
    #[test]
    fn validate_run_artifact_references_flags_stale_extras_but_skips_run_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path();
        let results: Vec<(String, explorer::ObservationOutput)> =
            vec![("pkg::a".to_string(), explorer::ObservationOutput::default())];
        fs::write(out.join("pkg__a.md"), "").expect("write a");
        // Leftover from a prior run that referenced `pkg::stale` —
        // the current run does not.
        fs::write(out.join("pkg__stale.md"), "").expect("write stale");
        // Control file: must be ignored by the stale-extra scan.
        fs::write(out.join("run.json"), "{}").expect("write run.json");
        let failures: HashMap<String, String> = HashMap::new();
        let issues = validate_run_artifact_references(out, &results, &failures);
        assert_eq!(issues.len(), 1, "got issues={issues:?}");
        assert!(issues[0].starts_with("stale_extra:"), "issue={}", issues[0]);
        assert!(issues[0].contains("pkg__stale.md"), "issue={}", issues[0]);
    }

    /// str-ux7q: classify_validity escalates to `invalid-artifacts`
    /// on any non-empty validation-issues slice, regardless of the
    /// underlying issue shape (missing artifact, missing reason,
    /// stale extra). Pins the wiring between the richer validator
    /// and the validity classifier.
    #[test]
    fn classify_validity_escalates_on_stale_extra_validation_issue() {
        let summary = synth_summary(95, 9, 1); // would be `high`
        let stale = vec!["stale_extra: path=/tmp/out/old.md".to_string()];
        let (tier, reasons) = classify_validity(&summary, None, &stale);
        assert_eq!(tier, ReportValidity::InvalidArtifacts);
        assert!(reason_codes(&reasons).contains(&"invalid_artifacts_missing"));
    }

    #[test]
    fn classify_validity_detects_real_stale_manifest_diff() {
        // End-to-end: capture a manifest, mutate the file on disk,
        // diff, and assert the classifier escalates to
        // stale-source-set with a `changed` reason.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let path = root.join("src.rs");
        fs::write(&path, "alpha\n").expect("write initial");
        let manifest_paths = vec!["src.rs".to_string()];
        let manifest =
            shatter_core::run_manifest::capture("scan-stale", "h", &manifest_paths, Some(root));
        // Mutate the source file content; the snapshot compares
        // size + content hash so no sleep is needed.
        fs::write(&path, "alpha\nbeta\n").expect("rewrite");
        let diff = shatter_core::run_manifest::diff_against(&manifest, &manifest_paths);
        assert!(diff.is_stale(), "expected stale diff, got {diff:?}");

        let summary = synth_summary(95, 9, 1);
        let (tier, reasons) = classify_validity(&summary, Some(&diff), &[]);
        assert_eq!(tier, ReportValidity::StaleSourceSet);
        assert!(reason_codes(&reasons).contains(&"stale_source_set_changed"));
    }

    proptest! {
        /// Validity tier is monotonic non-improving as
        /// `represented_source_percent` decreases (with stale and
        /// artifact signals held clean). Encodes the precedence rule
        /// that lower representation never produces a better tag.
        #[test]
        fn classify_validity_is_monotonic_in_representation_percent(
            high_pct in 0.0f64..=100.0,
            low_pct in 0.0f64..=100.0,
        ) {
            let (a, b) = if high_pct >= low_pct {
                (high_pct, low_pct)
            } else {
                (low_pct, high_pct)
            };
            let mut sum_high = synth_summary(0, 1, 0);
            sum_high.represented_source_percent = a;
            let mut sum_low = synth_summary(0, 1, 0);
            sum_low.represented_source_percent = b;
            let (tier_high, _) = classify_validity(&sum_high, None, &[]);
            let (tier_low, _) = classify_validity(&sum_low, None, &[]);
            prop_assert!(
                report_validity_severity(tier_high) <= report_validity_severity(tier_low),
                "rep%={a} produced {tier_high:?} but rep%={b} produced {tier_low:?}",
            );
        }

        /// Whenever the source-set diff is stale the validity tag is
        /// at least `stale-source-set` — never `high`/`degraded`/`low`
        /// outranking it. Holds independently of representation %.
        #[test]
        fn stale_source_set_forces_at_least_stale_tier(
            rep_pct in 0.0f64..=100.0,
            n_added in 0usize..5,
            n_removed in 0usize..5,
            n_changed in 0usize..5,
        ) {
            // At least one bucket non-empty so the diff is stale.
            let n_added = if n_added + n_removed + n_changed == 0 { 1 } else { n_added };
            let mut diff = run_manifest::ManifestDiff::default();
            for i in 0..n_added { diff.added.push(format!("a{i}.rs")); }
            for i in 0..n_removed { diff.removed.push(format!("r{i}.rs")); }
            for i in 0..n_changed { diff.changed.push(format!("c{i}.rs")); }
            prop_assume!(diff.is_stale());

            let mut summary = synth_summary(0, 1, 0);
            summary.represented_source_percent = rep_pct;
            let (tier, _) = classify_validity(&summary, Some(&diff), &[]);
            prop_assert!(
                report_validity_severity(tier) >= report_validity_severity(ReportValidity::StaleSourceSet),
                "stale diff produced {tier:?}, expected >= StaleSourceSet",
            );
        }
    }
}
