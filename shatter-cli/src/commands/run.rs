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
            write_run_summary_json(dir, &build_run_summary(&scan_id, &manifest, 0, 0, 0, &[]))?;
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
    let registry = batch_analyze::batch_analyze(
        &mut frontends,
        &analyzable_files,
        None,
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
            write_run_summary_json(
                dir,
                &build_run_summary(&scan_id, &run_manifest, 0, 0, 0, &[]),
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
        );

        if let Some(dir) = output_dir {
            write_analysis_report(dir, &registry, &call_graph, &root)?;
            // analyze-only still produces a JSON summary so the
            // selected-source denominator is recorded even without
            // exploration outcomes.
            let spans = registry_spans(&registry, &root);
            write_run_summary_json(
                dir,
                &build_run_summary(&scan_id, &run_manifest, 0, 0, 0, &spans),
            )?;
        }

        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 5: Explore in dependency order (layer by layer)
    log::debug!("Exploring functions in dependency order...");

    let mut exploration_results: Vec<(String, explorer::ObservationOutput)> = Vec::new();

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

    // Step 6: Print summary report
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
    );

    // Step 7: Write output files if requested
    if let Some(dir) = output_dir {
        write_run_report(dir, &call_graph, &exploration_results)?;
        // str-jeen.17: emit run-level JSON with manifest-driven
        // selected-source denominators alongside per-function reports.
        let completed = exploration_results.len();
        // The run command surfaces only "completed" outcomes (errors are
        // logged but not pushed). Until the run command tracks per-target
        // failure / skip outcomes, derive failed = attempted - completed
        // from the discovery total so the summary still reflects the
        // ratio of completed to attempted.
        let attempted = total_functions;
        let failed = attempted.saturating_sub(completed);
        let spans = registry_spans(&registry, &root);
        write_run_summary_json(
            dir,
            &build_run_summary(&scan_id, &run_manifest, completed, failed, 0, &spans),
        )?;
    }

    shutdown_all_frontends(frontends).await;
    Ok(())
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
        let safe_name = qname.replace("::", "__").replace('/', "_");
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

/// Run-level JSON summary written by `shatter run` to
/// `<output_dir>/run.json` (str-jeen.17).
///
/// `selected_source_files` and `selected_source_lines` come from the
/// run-start manifest snapshot, **not** from completed-function span
/// totals. That's the whole point of this struct: the denominator must
/// reflect the source set the run selected, even when most targets
/// fail or skip during exploration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
            let path = e
                .file_path
                .strip_prefix(root)
                .unwrap_or(&e.file_path)
                .to_string_lossy()
                .into_owned();
            DiscoveredSpan {
                path,
                start_line: e.start_line,
                end_line: e.end_line,
            }
        })
        .collect()
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
    RunSummary {
        version: RUN_SUMMARY_VERSION,
        scan_id: scan_id.to_string(),
        selected_source_files: manifest.selected_source_files(),
        selected_source_lines: manifest.selected_source_lines(),
        completed_functions: completed,
        failed_functions: failed,
        skipped_functions: skipped,
        no_target_file_lines: buckets.no_target_file_lines,
        undiscovered_source_lines: buckets.undiscovered_source_lines,
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
}
