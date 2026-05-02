use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shatter_core::batch_analyze;
use shatter_core::call_graph::CallGraph;
use shatter_core::config as shatter_config;
use shatter_core::discovery::{self, DiscoveryOptions, Language as DiscoveryLanguage};
use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::Frontend;
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

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
        return Ok(());
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
    use shatter_core::discovery;
    use std::fs;

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
}
