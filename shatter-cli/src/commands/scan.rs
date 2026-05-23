use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

use crate::helpers::*;

/// Derive the source language of a function from its `source_file` path.
///
/// Used to partition analyses for per-language scan passes (str-14en); the
/// scan loop calls one frontend per language so files are never dispatched
/// to a parser for a different language.
fn analysis_language(
    analysis: &shatter_core::protocol::FunctionAnalysis,
) -> Option<DiscoveryLanguage> {
    let src = analysis.source_file.as_ref()?;
    let ext = std::path::Path::new(src).extension()?.to_str()?;
    DiscoveryLanguage::from_extension(ext)
}

fn compute_scan_id_from_file_map(file_map: &HashMap<String, String>) -> String {
    let targets: Vec<(&str, &str)> = file_map
        .iter()
        .map(|(qualified_id, file_path)| (qualified_id.as_str(), file_path.as_str()))
        .collect();
    shatter_core::checkpoint::ScanCheckpoint::compute_scan_id_for_targets(&targets)
}

/// Run the scan command: explore multiple functions in dependency order.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_scan(
    directory: &str,
    language_filter: Option<&str>,
    include_patterns: &[String],
    exclude_patterns: &[String],
    changed: bool,
    since: Option<&str>,
    until: Option<&str>,
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
    release: bool,
    parallelism: usize,
    timeout_per_fn: u64,
    timeout_explore: Option<f64>,
    outputs: &[std::path::PathBuf],
    stdout: bool,
    format: crate::args::StdoutFormat,
    progress: bool,
    dry_run: bool,
    resume: Option<&str>,
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
    isolation: shatter_core::explorer::IsolationMode,
    capture_side_effects: bool,
    workers_per_fn: usize,
    genetic_config: &shatter_core::config::GeneticConfig,
    parallelism_bounds: crate::helpers::ParallelismBounds,
    require_rust: bool,
    failure_policy: shatter_core::scan_orchestrator::ScanFailurePolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    let scan_pool_path = if no_seeds {
        None
    } else if seeds_dir.is_absolute() {
        Some(seeds_dir.join("pool.json"))
    } else {
        Some(
            std::path::PathBuf::from(directory)
                .join(seeds_dir)
                .join("pool.json"),
        )
    };

    // str-1wcl: when the caller passes explicit external `-o` outputs AND
    // disables both caches and the seed pool, treat this as a clean
    // external-audit run: no project-local artifact directory, no
    // `.shatter-cache/`, no `.shatter/` writes, and harness storage env
    // vars point under the OS temp dir instead of `<project>/shatter-
    // artifacts/`. The user already has their report at the explicit
    // external path, so writing additional copies under the audited
    // project tree just leaves litter behind.
    let external_audit_mode = !outputs.is_empty() && no_cache && no_seeds;
    if external_audit_mode {
        log::debug!(
            "scan: external audit mode active (-o + --no-cache + --no-seeds); \
             suppressing project-local artifact and harness storage writes",
        );
    }
    // Validate --language if specified.
    if let Some(lang) = language_filter
        && lang != "typescript"
        && lang != "go"
        && lang != "rust"
    {
        return Err(format!(
            "unsupported language '{lang}': expected 'typescript', 'go', or 'rust'"
        )
        .into());
    }

    // str-nnty: `--dry-run` supports both markdown and JSON output. JSON
    // emits a machine-readable scan plan (function inventory, layers,
    // estimated time, config, skipped reasons) so agents and CI can
    // estimate scan scope without running exploration. Routing happens
    // at the dry-run branch below — both stdout and `-o file.{json,md}`
    // honor the requested format.

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
    // When --until is specified, we need to:
    // 1. Validate the ref
    // 2. Check out historical file contents into a temp directory
    // 3. Use that as the analysis root with isolated .shatter state
    let _until_temp_dir: Option<tempfile::TempDir> = None;
    let (effective_root, _until_temp_dir) = if let (Some(base_ref), Some(until_ref)) =
        (since, until)
    {
        use shatter_core::scm::{ScmProvider, detect_provider, show_file_at_ref, validate_ref};

        // Validate the until ref resolves to a real commit.
        let resolved = validate_ref(&root, until_ref)
            .map_err(|e| format!("--until ref '{until_ref}' is not valid: {e}"))?;
        log::info!(
            "Time-travel analysis: examining code at {} (resolved: {})",
            until_ref,
            &resolved[..std::cmp::min(12, resolved.len())],
        );
        log::warn!(
            "Results are for historical code at '{}', not the current working tree. \
             Seeds and specs are isolated and will not affect HEAD state.",
            until_ref,
        );

        let provider = detect_provider(&root).map_err(|e| format!("SCM detection failed: {e}"))?;
        let scm_files = provider
            .diff_files_range(&root, base_ref, until_ref)
            .map_err(|e| format!("SCM file query failed: {e}"))?;

        if scm_files.is_empty() {
            log::info!("No changed files found between '{base_ref}' and '{until_ref}'");
            return Ok(());
        }
        log::info!(
            "SCM reports {} changed file(s) between '{}' and '{}'",
            scm_files.len(),
            base_ref,
            until_ref,
        );

        // Create temp directory and extract historical file contents.
        let temp_dir = tempfile::TempDir::new()
            .map_err(|e| format!("failed to create temp directory: {e}"))?;
        let temp_root = temp_dir.path().to_path_buf();

        for file in &scm_files {
            let rel_path = file.strip_prefix(&root).map_err(|_| {
                format!(
                    "file '{}' is not under root '{}'",
                    file.display(),
                    root.display()
                )
            })?;

            let content = match show_file_at_ref(&root, until_ref, rel_path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    // File may have been deleted at until_ref; skip it.
                    log::debug!("Skipping {}: {e}", rel_path.display());
                    continue;
                }
            };

            let dest = temp_root.join(rel_path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create dir '{}': {e}", parent.display()))?;
            }
            std::fs::write(&dest, &content)
                .map_err(|e| format!("failed to write '{}': {e}", dest.display()))?;
        }

        log::info!(
            "Extracted {} file(s) at '{}' into temporary directory",
            scm_files.len(),
            until_ref,
        );

        let effective = temp_root;
        (effective, Some(temp_dir))
    } else {
        (root.clone(), None)
    };

    // When using --until, isolate .shatter state so HEAD seeds/specs aren't clobbered.
    let scan_pool_path = if until.is_some() && !no_seeds {
        Some(
            effective_root
                .join(".shatter")
                .join("seeds")
                .join("pool.json"),
        )
    } else {
        scan_pool_path
    };

    // Re-resolve discovery options against the effective root.
    let files = if changed || since.is_some() {
        if until.is_some() {
            // Files already extracted into effective_root; discover from there.
            discovery::discover_files(&effective_root, &options)
                .map_err(|e| format!("file discovery failed: {e}"))?
        } else {
            use shatter_core::scm::{ScmProvider, detect_provider};
            let provider =
                detect_provider(&root).map_err(|e| format!("SCM detection failed: {e}"))?;
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
        }
    } else {
        discovery::discover_files(&effective_root, &options)
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

    // str-bnsw: precheck frontend availability per discovered language so the
    // scan reports a clear frontend-unavailable status instead of a generic
    // per-target spawn failure. For mixed-language scans we filter out files
    // whose frontend is unavailable and continue with the rest; if every
    // discovered language is unavailable we return early with the install
    // hint.
    let analyzable_files: Vec<(PathBuf, DiscoveryLanguage)> = {
        let mut langs_in_scan: std::collections::HashSet<DiscoveryLanguage> =
            analyzable_files.iter().map(|(_, l)| *l).collect();
        let mut unavailable: std::collections::HashMap<DiscoveryLanguage, String> =
            std::collections::HashMap::new();
        for lang in langs_in_scan.iter().copied().collect::<Vec<_>>() {
            let cli_lang = discovery_lang_to_cli_lang(lang)
                .expect("discovery_lang_to_cli_lang already filtered above");
            let availability = crate::helpers::check_frontend_availability(cli_lang, None);
            if let Some(msg) = availability.unavailable_message() {
                unavailable.insert(lang, msg);
                langs_in_scan.remove(&lang);
            }
        }
        if !unavailable.is_empty() {
            // str-jeen.13: emit one structured `skipped_by_unavailable_frontend`
            // STATUS line per blocked file so broad-run wrappers (Kapow re-runs,
            // etc.) can classify the row as environmental rather than as a hard
            // target failure. Then warn (one line per language) and either
            // hard-fail (no available frontend / --require-rust set) or
            // continue with the available subset.
            for (lang, msg) in &unavailable {
                let cli_lang = discovery_lang_to_cli_lang(*lang)
                    .expect("discovery_lang_to_cli_lang already filtered above");
                let skipped: Vec<&PathBuf> = analyzable_files
                    .iter()
                    .filter(|(_, l)| l == lang)
                    .map(|(p, _)| p)
                    .collect();
                log::warn!(
                    "skipping {} {:?} file(s) — shatter-{} frontend not on PATH (the main \
                     CLI is working; this is expected after a workspace-root build that only \
                     built the main CLI binary): {} (run will continue with available \
                     languages; pass --require-rust to fail instead)",
                    skipped.len(),
                    lang,
                    cli_lang.label(),
                    msg,
                );
                let install_hint = match cli_lang {
                    crate::args::Language::Rust => crate::helpers::RUST_FRONTEND_INSTALL_HINT,
                    _ => msg.as_str(),
                };
                for p in skipped {
                    crate::helpers::emit_skipped_unavailable_frontend(p, cli_lang, install_hint);
                }
            }
            let require_rust_violated =
                require_rust && unavailable.contains_key(&DiscoveryLanguage::Rust);
            if langs_in_scan.is_empty() || require_rust_violated {
                let combined: Vec<String> = unavailable.values().cloned().collect();
                let prefix = if require_rust_violated {
                    "rust frontend unavailable and --require-rust is set"
                } else {
                    "no available frontends for discovered files"
                };
                return Err(format!("{prefix}: {}", combined.join("; ")).into());
            }
            analyzable_files
                .into_iter()
                .filter(|(_, l)| !unavailable.contains_key(l))
                .collect()
        } else {
            analyzable_files
        }
    };

    if analyzable_files.is_empty() {
        // str-94cg: when the user passed `--include` patterns, explain
        // that patterns are evaluated relative to the scan root and
        // (where possible) suggest a corrected pattern with the
        // duplicated scan-root prefix stripped. Without this hint users
        // see only "No supported source files found" and have no idea
        // their `internal/runtime/*.go` pattern never matched because
        // the scan root is already `<repo>/internal/runtime`.
        if !include_patterns.is_empty() {
            for pat in include_patterns {
                let suggestion = discovery::suggest_corrected_include_pattern(pat, &effective_root);
                match suggestion {
                    Some(corrected) => log::warn!(
                        "--include '{pat}' matched 0 files. \
                         Patterns are evaluated relative to scan root: {}. \
                         Try: --include '{corrected}'",
                        effective_root.display(),
                    ),
                    None => log::warn!(
                        "--include '{pat}' matched 0 files. \
                         Patterns are evaluated relative to scan root: {} \
                         (not the repo root). Try a pattern like '*.go' or '**/*.go'.",
                        effective_root.display(),
                    ),
                }
            }
        } else {
            log::info!("No supported source files found in {}", root.display());
        }
        return Ok(());
    }

    log::info!(
        "Discovered {} source file(s) in {}",
        analyzable_files.len(),
        root.display(),
    );

    // Spawn frontends for each language.
    let req_timeout = Duration::from_secs(request_timeout);
    let needed_langs: std::collections::HashSet<DiscoveryLanguage> =
        analyzable_files.iter().map(|(_, lang)| *lang).collect();

    let mut frontends: HashMap<DiscoveryLanguage, Frontend> = HashMap::new();
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
        if external_audit_mode {
            apply_external_audit_storage(&mut config);
        } else {
            apply_project_storage(&mut config, project_root_str.as_deref());
        }
        if no_cache {
            disable_frontend_analysis_cache(&mut config);
        }
        let frontend = Frontend::spawn(&config)
            .await
            .map_err(|e| format!("failed to spawn {lang:?} frontend: {e}"))?;
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
        Some(
            AnalysisCache::new(dir)
                .map_err(|e| format!("failed to initialize analysis cache: {e}"))?,
        )
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

    // str-z06h: log the visibility split so the omission policy is visible in
    // the scan log even when callers consume `format_scan_report` output
    // rather than `print_summary_report`. The actual filter runs inside
    // `rebuild_analyses_from_registry` below; this is purely informational.
    let exported_total = registry.exported_functions().len();
    let unexported_total = registry.len().saturating_sub(exported_total);
    if unexported_total > 0 {
        if all_functions {
            log::info!(
                "Including {unexported_total} unexported function(s) in scan scope (--all set)"
            );
        } else {
            log::info!(
                "Omitting {unexported_total} unexported function(s) by default; \
                 re-run with --all to include them"
            );
        }
    }

    // Collect analyses and file map from the registry. Pulls full analyses
    // (branches/literals/loops/source_file/adapter_hints/invocation_model)
    // from the registry rather than rebuilding empty placeholders — see
    // str-jeen.45.
    let (mut all_analyses, file_map) = rebuild_analyses_from_registry(&registry, all_functions);

    // Filter out functions with unexecutable parameter types.
    let mut skipped_for_executability: Vec<SkippedFunction> = Vec::new();
    all_analyses.retain(|func| {
        let reasons = executability::check_executability(&func.params, &[]);
        if reasons.is_empty() {
            true
        } else {
            let reason = reasons
                .iter()
                .map(|r| r.format_human())
                .collect::<Vec<_>>()
                .join("; ");
            skipped_for_executability.push(SkippedFunction {
                function_name: func.name.clone(),
                reason,
                category: shatter_core::scan_orchestrator::SkipCategory::Unsupported,
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
        Some(shatter_core::stratum::parse_stratum_spec(spec_str).map_err(|e| e.to_string())?)
    } else {
        None
    };

    // When both --stratum and --core-sample are set, pre-filter analyses by
    // stratum so the core-sample budget is computed against the narrowed set.
    let stratum_pre_applied = if let (Some(spec), Some(_)) = (&parsed_stratum, &core_sample_spec) {
        let cg = CallGraph::from_registry(&registry);
        let layers = cg.topological_layers();
        let max_layer = if layers.is_empty() {
            0
        } else {
            layers.len() - 1
        };
        let range = shatter_core::stratum::resolve_range(spec, max_layer)?;
        // filter_layers returns qualified names (file::func); extract bare names.
        let selected: std::collections::HashSet<String> =
            shatter_core::stratum::filter_layers(&layers, &range)
                .into_iter()
                .flat_map(|(_, funcs)| funcs.iter().cloned())
                .map(|qn| {
                    // Qualified names are "file_path::name"; extract the bare name.
                    qn.rsplit_once("::")
                        .map_or(qn.clone(), |(_, name)| name.to_string())
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
        let seed =
            core_sample_seed.unwrap_or_else(|| shatter_core::core_sample::default_seed(directory));
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
                        start + 1,
                        end,
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
        // `included` contains qualified names (file_path::name) emitted by
        // core_sample. After str-fuhw, file_map is also keyed by qualified
        // ID, so look up via the analysis's `source_file` (populated in
        // `rebuild_analyses_from_registry`) rather than its bare name.
        all_analyses.retain(|a| {
            if let Some(file) = a.source_file.as_deref() {
                let qn = format!("{}::{}", file, a.name);
                included.contains(&qn)
            } else {
                // No source file — fall back to bare-name match for tests
                // and synthesized records.
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

    // Capture frontend capabilities before shutting down analysis frontends.
    let frontend_caps = frontends
        .values()
        .next()
        .map(|fe| shatter_core::orchestrator::FrontendCapabilities::from_raw(fe.capabilities()))
        .unwrap_or_default();

    // Shut down analysis frontends before starting parallel exploration.
    for frontend in frontends.into_values() {
        shutdown_frontend(frontend).await;
    }

    // Resolve effective parallelism: 0 means auto-detect, otherwise honor the
    // user value. Auto-detect first applies a per-language cap based on
    // `needed_langs` (str-qp31: TS unbounded, Go/Rust capped at 8 because
    // their frontends shell out to multi-process toolchains; mixed-language
    // scans take the worst-case cap), then the global [floor, ceiling] clamp
    // from str-eam2. Explicit non-zero values pass through the global clamp
    // only.
    let effective_parallelism =
        resolve_parallelism_for_langs(parallelism, &needed_langs, parallelism_bounds);

    // Dry run: show the full exploration plan without executing.
    if dry_run {
        let scan_config = ScanConfig {
            max_iterations_per_function: max_iterations,
            seed: None,
            file_map: file_map.clone(),
            parallelism: effective_parallelism,
            timeout_per_fn: Duration::from_secs(timeout_per_fn),
            build_timeout: Duration::from_secs(build_timeout),
            cache: None,
            stratum: if stratum_pre_applied {
                None
            } else {
                parsed_stratum.clone()
            },
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: scan_pool_path.clone(),
            project_root: project_root_str.clone(),
            config_dir: Some(std::path::PathBuf::from(directory)),
            timeout_explore: timeout_explore.map(Duration::from_secs_f64),
            setup_manager: None,
            policy: scheduler_policy,
            isolation,
            capture_side_effects,
            workers_per_fn,
            capabilities: frontend_caps.clone(),
            genetic_config: genetic_config.clone(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: shatter_core::interesting_pool::CoverageMode::Branch,
            write_artifacts: !external_audit_mode,
        };
        // str-nnty: emit JSON or markdown per requested format. JSON
        // mode covers both `--format json --stdout` and `-o file.json`;
        // markdown mode keeps the existing human-readable plan path.
        emit_dry_run_plan(
            &all_analyses,
            &skipped_for_executability,
            &scan_config,
            &registry,
            outputs,
            stdout,
            format,
            use_color,
            effective_parallelism,
            timeout_per_fn,
        )?;
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
    // Stored-inputs sidecar cache (str-bo4z.3). Colocated with behavior maps
    // so a single `ls` of the cache dir shows both artifacts. Disabled by the
    // same `--no-cache` flag.
    let stored_inputs_cache = if no_cache {
        None
    } else {
        let dir = match cache_dir {
            Some(d) => d.to_path_buf(),
            None => shatter_core::cache::StoredInputsCache::default_dir(&std::env::current_dir()?),
        };
        shatter_core::cache::StoredInputsCache::new(dir)
            .map_err(|e| {
                log::warn!("failed to initialize stored-inputs cache: {e}");
                e
            })
            .ok()
            .map(std::sync::Arc::new)
    };

    // str-14en: build one frontend config per discovered language. The
    // previous code picked a single config from `needed_langs.iter().next()`
    // — a HashSet, so the choice was non-deterministic — and reused it for
    // every file in `parallel_scan_with_progress`. Mixed Rust+TS scans then
    // routed TS source through the Rust frontend's `syn::parse_file`,
    // surfacing as `instrument error (ParseError): failed to parse file:
    // cannot parse string into token stream`. The scan loop below now runs
    // `parallel_scan` once per language with the matching frontend.
    let mut lang_fe_configs: Vec<(DiscoveryLanguage, shatter_core::frontend::FrontendConfig)> =
        Vec::with_capacity(needed_langs.len());
    for &lang in &needed_langs {
        let cli_lang = discovery_lang_to_cli_lang(lang)
            .ok_or_else(|| format!("no frontend for {lang:?}"))?;
        let mut cfg = frontend_config(
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
        if external_audit_mode {
            apply_external_audit_storage(&mut cfg);
        } else {
            apply_project_storage(&mut cfg, project_root_str.as_deref());
        }
        if no_cache {
            disable_frontend_analysis_cache(&mut cfg);
        }
        lang_fe_configs.push((lang, cfg));
    }
    // Deterministic order across runs so checkpoint/artifact writes don't
    // depend on HashSet iteration order.
    lang_fe_configs.sort_by_key(|(l, _)| l.as_registry_str());

    // Load mock overrides from --mock-config (or .shatter/config.yaml defaults).
    let mock_overrides = if let Some(mc_path) = mock_config {
        let cfg = shatter_config::parse_config(mc_path)
            .map_err(|e| format!("failed to load mock config: {e}"))?;
        cfg.defaults.mocks.unwrap_or_default()
    } else {
        // Try loading from the scanned directory's .shatter/config.yaml
        let config_path = PathBuf::from(directory)
            .join(".shatter")
            .join("config.yaml");
        if config_path.exists() {
            shatter_config::parse_config(&config_path)
                .ok()
                .and_then(|cfg| cfg.defaults.mocks)
                .unwrap_or_default()
        } else {
            HashMap::new()
        }
    };

    // Resolve --resume: "auto" discovers from artifact dir, otherwise treat as path.
    let resolved_resume_path: Option<std::path::PathBuf> = match resume {
        Some("auto") => {
            let sid = compute_scan_id_from_file_map(&file_map);
            match shatter_core::checkpoint::ScanCheckpoint::auto_discover(
                project_root_str.as_deref(),
                &sid,
            ) {
                Some(p) => {
                    log::info!("auto-discovered checkpoint at {}", p.display());
                    Some(p)
                }
                None => {
                    let default = shatter_core::checkpoint::ScanCheckpoint::default_path(
                        project_root_str.as_deref(),
                        &sid,
                    );
                    log::info!(
                        "no existing checkpoint found, will create at {}",
                        default.display()
                    );
                    Some(default)
                }
            }
        }
        Some(path) => Some(std::path::PathBuf::from(path)),
        None => None,
    };

    let scan_config = ScanConfig {
        max_iterations_per_function: max_iterations,
        seed: None,
        file_map,
        parallelism: effective_parallelism,
        timeout_per_fn: Duration::from_secs(timeout_per_fn),
        build_timeout: Duration::from_secs(build_timeout),
        cache,
        stratum: if stratum_pre_applied {
            None
        } else {
            parsed_stratum
        },
        mock_overrides,
        resume_path: resolved_resume_path,
        timeout_total: if timeout_total == 0 {
            None
        } else {
            Some(Duration::from_secs(timeout_total))
        },
        pool_path: scan_pool_path,
        project_root: project_root_str.clone(),
        config_dir: Some(std::path::PathBuf::from(directory)),
        timeout_explore: timeout_explore.map(Duration::from_secs_f64),
        setup_manager: None,
        policy: scheduler_policy,
        isolation,
        capture_side_effects,
        workers_per_fn,
        capabilities: frontend_caps,
        genetic_config: genetic_config.clone(),
        batch_size: None,
        scheduler_state_cache: None,
        stored_inputs_cache,
        coverage_mode: shatter_core::interesting_pool::CoverageMode::Branch,
        write_artifacts: !external_audit_mode,
    };

    let scan_start = Instant::now();
    let total_functions = all_analyses.len();

    log::info!(
        "Scanning {} function(s) in dependency order...",
        total_functions,
    );

    let progress_handler = progress.then(|| {
        Arc::new(|update: scan_orchestrator::ScanProgressUpdate| {
            let event = report::ProgressEvent::with_qualified_status(
                &update.function_name,
                update.current,
                update.total,
                update.elapsed.as_millis() as u64,
                update.status.as_str(),
            );
            if let Some(json) = event.to_json() {
                eprintln!("{json}");
            }
        }) as scan_orchestrator::ProgressHandler
    });

    // str-14en: group analyses by language and run parallel_scan once per
    // language with the matching frontend config. For single-language scans
    // the loop runs once, exactly as before. For mixed scans the previous
    // code would dispatch every file through whichever frontend `needed_langs
    // .iter().next()` happened to pick.
    let mut analyses_by_lang: std::collections::BTreeMap<&'static str, Vec<shatter_core::protocol::FunctionAnalysis>> =
        std::collections::BTreeMap::new();
    let fallback_lang_key: Option<&'static str> = if lang_fe_configs.len() == 1 {
        Some(lang_fe_configs[0].0.as_registry_str())
    } else {
        None
    };
    for analysis in all_analyses {
        let lang = analysis_language(&analysis).map(|l| l.as_registry_str())
            .or(fallback_lang_key);
        match lang {
            Some(key) => analyses_by_lang.entry(key).or_default().push(analysis),
            None => {
                log::warn!(
                    "scan: dropping function '{}' — unknown language for source_file {:?}",
                    analysis.name,
                    analysis.source_file,
                );
            }
        }
    }

    let merge_result = scan_orchestrator::ParallelScanResult {
        function_results: Vec::new(),
        test_order: Vec::new(),
        skipped: Vec::new(),
        workers_used: 0,
        workers_reaped: 0,
        sampling: None,
        source_files: Vec::new(),
    };
    let mut merged_or_err: Result<scan_orchestrator::ParallelScanResult, String> = Ok(merge_result);

    for (lang, lang_fe_config) in &lang_fe_configs {
        let key = lang.as_registry_str();
        let lang_analyses = match analyses_by_lang.remove(key) {
            Some(v) if !v.is_empty() => v,
            _ => continue,
        };
        log::debug!(
            "scan: running {} function(s) through {} frontend",
            lang_analyses.len(),
            key,
        );
        let lang_result = scan_orchestrator::parallel_scan_with_progress(
            lang_fe_config,
            &lang_analyses,
            &scan_config,
            progress_handler.clone(),
        )
        .await;
        match (lang_result, &mut merged_or_err) {
            (Ok(lang_result), Ok(merged)) => {
                merged.function_results.extend(lang_result.function_results);
                merged.test_order.extend(lang_result.test_order);
                merged.skipped.extend(lang_result.skipped);
                merged.workers_used = merged.workers_used.max(lang_result.workers_used);
                merged.workers_reaped += lang_result.workers_reaped;
                if merged.sampling.is_none() {
                    merged.sampling = lang_result.sampling;
                }
                merged.source_files.extend(lang_result.source_files);
            }
            (Err(e), _) => {
                merged_or_err = Err(e.to_string());
                break;
            }
            (_, Err(_)) => break,
        }
    }

    match merged_or_err {
        Ok(mut result) => {
            result.sampling = sampling_context;
            // str-jeen.46: surface unsupported targets (filtered out
            // pre-attempt for unexecutable parameter types) in the scan
            // result so the report's `unsupported_functions` count and
            // skipped table reflect them. They are NOT counted as
            // attempted — the report builder discriminates by
            // SkipCategory::Unsupported.
            result.skipped.append(&mut skipped_for_executability);
            let elapsed = scan_start.elapsed();

            if !progress {
                for (i, fr) in result.function_results.iter().enumerate() {
                    log::info!(
                        "[{}/{}] {} ({:.1}s elapsed)",
                        i + 1,
                        total_functions,
                        fr.function_name,
                        elapsed.as_secs_f64(),
                    );
                }
            }

            // str-tzbr: when stdout is the JSON target, skip the
            // preliminary Markdown dump — the JSON write below is the
            // authoritative stdout content. Logs/progress already go to
            // stderr.
            let json_to_stdout =
                (outputs.is_empty() || stdout) && format == crate::args::StdoutFormat::Json;
            if !json_to_stdout {
                if output_format == crate::args::OutputFormat::Md {
                    let view = crate::render::scan_view(&result);
                    print_markdown(&crate::render::render_scan(&view), use_color);
                } else {
                    print_markdown(
                        &scan_orchestrator::format_parallel_scan_report(&result),
                        use_color,
                    );
                }
            }

            // Record batch state and print cumulative progress.
            let batch_state = if let Some(batch_idx) = effective_batch_index {
                let batch_state_path = PathBuf::from(directory)
                    .join(".shatter-cache")
                    .join("batch-state.json");

                let scan_id = compute_scan_id_from_file_map(&scan_config.file_map);

                let mut state = match shatter_core::batch_state::BatchState::load(&batch_state_path)
                {
                    Ok(Some(s)) if s.scan_id == scan_id => s,
                    Ok(Some(_)) => {
                        log::info!("batch state scan_id mismatch, starting fresh");
                        shatter_core::batch_state::BatchState::new(scan_id, total_scope_functions)
                    }
                    Ok(None) => {
                        shatter_core::batch_state::BatchState::new(scan_id, total_scope_functions)
                    }
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

                if !json_to_stdout {
                    print_markdown(
                        &shatter_core::batch_state::format_cumulative_batch_section(
                            &state, batch_idx,
                        ),
                        use_color,
                    );
                }

                Some(state)
            } else {
                None
            };

            let scan_report =
                report::generate_report(&result, &scan_config.file_map, batch_state.as_ref());

            // Write to each -o file (format inferred from extension).
            for path in outputs {
                let content = match crate::args::infer_output_format(path) {
                    Ok(crate::args::StdoutFormat::Markdown) => {
                        report::format_markdown_report(&scan_report)
                    }
                    Ok(crate::args::StdoutFormat::Json) => {
                        match serde_json::to_string_pretty(&scan_report) {
                            Ok(s) => s,
                            Err(e) => {
                                log::error!("failed to serialize report: {e}");
                                continue;
                            }
                        }
                    }
                    Ok(crate::args::StdoutFormat::Html) => report::generate_html_scan_report(
                        &scan_report,
                        project_root_str.as_deref().map(std::path::Path::new),
                    ),
                    Ok(crate::args::StdoutFormat::Text) => report::format_text_report(&scan_report),
                    Err(e) => {
                        log::error!("{e}");
                        continue;
                    }
                };
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    log::error!("failed to create output directory: {e}");
                    continue;
                }
                match std::fs::write(path, &content) {
                    Ok(()) => log::info!("Wrote report to {}", path.display()),
                    Err(e) => log::error!("failed to write report to '{}': {e}", path.display()),
                }
            }

            // Write to stdout if no files given, or if --stdout is explicit.
            if outputs.is_empty() || stdout {
                let content = match format {
                    crate::args::StdoutFormat::Markdown => {
                        report::format_markdown_report(&scan_report)
                    }
                    crate::args::StdoutFormat::Json => serde_json::to_string_pretty(&scan_report)
                        .unwrap_or_else(|e| {
                            format!("{{\"error\": \"failed to serialize report: {e}\"}}")
                        }),
                    crate::args::StdoutFormat::Html => report::generate_html_scan_report(
                        &scan_report,
                        project_root_str.as_deref().map(std::path::Path::new),
                    ),
                    crate::args::StdoutFormat::Text => report::format_text_report(&scan_report),
                };
                // str-tzbr: when emitting JSON or plain text, write raw
                // bytes — termimad-rendered Markdown would corrupt JSON
                // and add ANSI noise to text consumers.
                match format {
                    crate::args::StdoutFormat::Json => {
                        crate::helpers::print_stdout(&content);
                        crate::helpers::print_stdout("\n");
                    }
                    crate::args::StdoutFormat::Text => {
                        crate::helpers::print_stdout(&content);
                    }
                    _ => {
                        print_markdown(&content, use_color);
                    }
                }
            }

            if result.has_scan_failure() {
                let attempted = result.function_results.len() + result.skipped.len();
                return Err(format!(
                    "scan failed: {attempted} function(s) attempted but 0 explored successfully"
                )
                .into());
            }

            // str-izhn: apply opt-in failure policy after rendering the
            // report. The default policy stays permissive — wrappers that
            // want CI to fail on any failure pass `--fail-on-failures` or
            // `--failure-threshold`. The summary above already names the
            // completed/failed/unsupported counts.
            if let Some(reason) = result.evaluate_failure_policy(failure_policy) {
                return Err(format!("scan failed: {reason}").into());
            }
        }
        Err(e) => {
            return Err(format!("Scan error: {e}").into());
        }
    }

    Ok(())
}

/// Rebuild full `FunctionAnalysis` records from a [`FunctionRegistry`] for
/// str-nnty: route the dry-run plan to the requested sink(s).
///
/// JSON output covers `--format json --stdout` (and the default-stdout
/// case where no `-o` is given) and `-o <file>.json`. Markdown output
/// covers the same flag/file shapes with `.md` / `--format markdown`.
/// If both JSON and Markdown sinks are requested simultaneously (e.g.
/// `-o plan.json -o plan.md`), both are written.
#[allow(clippy::too_many_arguments)]
fn emit_dry_run_plan(
    analyses: &[shatter_core::protocol::FunctionAnalysis],
    skipped: &[SkippedFunction],
    scan_config: &ScanConfig,
    registry: &shatter_core::batch_analyze::FunctionRegistry,
    outputs: &[std::path::PathBuf],
    stdout: bool,
    format: crate::args::StdoutFormat,
    use_color: bool,
    effective_parallelism: usize,
    timeout_per_fn: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::args::StdoutFormat;

    let json_files: Vec<&std::path::PathBuf> = outputs
        .iter()
        .filter(|p| {
            matches!(
                crate::args::infer_output_format(p),
                Ok(StdoutFormat::Json)
            )
        })
        .collect();
    let md_files: Vec<&std::path::PathBuf> = outputs
        .iter()
        .filter(|p| {
            matches!(
                crate::args::infer_output_format(p),
                Ok(StdoutFormat::Markdown)
            )
        })
        .collect();

    // Stdout target uses --format when --stdout is set or when no -o
    // sinks are given (the default-stdout case).
    let stdout_target = if stdout || outputs.is_empty() {
        Some(format)
    } else {
        None
    };
    let want_json = !json_files.is_empty() || matches!(stdout_target, Some(StdoutFormat::Json));
    let want_md = !md_files.is_empty() || matches!(stdout_target, Some(StdoutFormat::Markdown));

    if want_json {
        let plan = build_dry_run_plan_json(
            analyses,
            skipped,
            scan_config,
            registry,
            effective_parallelism,
            timeout_per_fn,
        );
        let serialized = serde_json::to_string_pretty(&plan)
            .map_err(|e| format!("failed to serialize dry-run plan: {e}"))?;
        for path in &json_files {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create output directory '{}': {e}", parent.display())
                })?;
            }
            std::fs::write(path, &serialized).map_err(|e| {
                format!("failed to write dry-run plan to '{}': {e}", path.display())
            })?;
            log::info!("Wrote dry-run plan to {}", path.display());
        }
        if matches!(stdout_target, Some(StdoutFormat::Json)) {
            crate::helpers::print_stdout(&serialized);
            crate::helpers::print_stdout("\n");
        }
    }

    if want_md {
        let plan = scan_orchestrator::format_dry_run_plan(analyses, skipped, scan_config)
            .map_err(|e| format!("failed to build dry-run plan: {e}"))?;
        for path in &md_files {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create output directory '{}': {e}", parent.display())
                })?;
            }
            std::fs::write(path, &plan).map_err(|e| {
                format!("failed to write dry-run plan to '{}': {e}", path.display())
            })?;
            log::info!("Wrote dry-run plan to {}", path.display());
        }
        if matches!(stdout_target, Some(StdoutFormat::Markdown)) {
            print_markdown(&plan, use_color);
        }
    }

    // Unsupported stdout formats for dry-run (HTML, Text) fall back to
    // markdown so callers don't silently get an empty stream.
    if let Some(fmt) = stdout_target
        && !matches!(fmt, StdoutFormat::Json | StdoutFormat::Markdown)
    {
        let plan = scan_orchestrator::format_dry_run_plan(analyses, skipped, scan_config)
            .map_err(|e| format!("failed to build dry-run plan: {e}"))?;
        print_markdown(&plan, use_color);
    }

    Ok(())
}

/// str-nnty: build a machine-readable JSON representation of the dry-run
/// scan plan. Mirrors the markdown plan's content — function inventory,
/// topological layers, skipped functions, estimated time, and selected
/// config values — but as a stable, parseable structure for agents and CI.
fn build_dry_run_plan_json(
    analyses: &[shatter_core::protocol::FunctionAnalysis],
    skipped: &[SkippedFunction],
    scan_config: &ScanConfig,
    registry: &shatter_core::batch_analyze::FunctionRegistry,
    effective_parallelism: usize,
    timeout_per_fn: u64,
) -> serde_json::Value {
    use serde_json::json;
    use std::collections::HashSet;

    // Topological layers from the registry-wide call graph. Filter each
    // layer to the analyses actually in scope (post unexported / opaque
    // skips) so the plan reflects what scan would run.
    let cg = CallGraph::from_registry(registry);
    let raw_layers = cg.topological_layers();

    // Build a qualified-id index over the in-scope analyses so we can
    // surface the rich per-function record (signature, branches, file)
    // even though the call graph keys are qualified ids.
    let analysis_qids: Vec<String> = analyses
        .iter()
        .map(shatter_core::behavior::node_id_for_analysis)
        .collect();
    let analysis_by_qid: HashMap<&str, &shatter_core::protocol::FunctionAnalysis> = analysis_qids
        .iter()
        .zip(analyses.iter())
        .map(|(qid, a)| (qid.as_str(), a))
        .collect();
    let scope_set: HashSet<&str> = analysis_qids.iter().map(String::as_str).collect();

    let mut layers_json: Vec<serde_json::Value> = Vec::new();
    let mut selected_function_count = 0usize;
    for (idx, layer) in raw_layers.iter().enumerate() {
        let in_scope: Vec<&str> = layer
            .iter()
            .filter(|qid| scope_set.contains(qid.as_str()))
            .map(String::as_str)
            .collect();
        if in_scope.is_empty() {
            continue;
        }
        selected_function_count += in_scope.len();

        let funcs_json: Vec<serde_json::Value> = in_scope
            .iter()
            .filter_map(|qid| {
                let analysis = analysis_by_qid.get(qid)?;
                let (file_part, bare) = shatter_core::behavior::split_qualified_id(qid);
                let params: Vec<serde_json::Value> = analysis
                    .params
                    .iter()
                    .map(|p| {
                        json!({
                            "name": p.name,
                            "type": format_type_label(&p.typ),
                        })
                    })
                    .collect();
                let internal_deps: Vec<&str> = cg
                    .callees_of(qid)
                    .into_iter()
                    .filter(|c| scope_set.contains(c))
                    .collect();
                Some(json!({
                    "qualified_id": qid,
                    "name": bare,
                    "file": file_part,
                    "params": params,
                    "return_type": format_type_label(&analysis.return_type),
                    "branches": analysis.branches.len(),
                    "internal_deps": internal_deps,
                    "external_deps": analysis
                        .dependencies
                        .iter()
                        .map(|d| json!({
                            "symbol": d.symbol,
                            "source_module": d.source_module,
                        }))
                        .collect::<Vec<_>>(),
                }))
            })
            .collect();

        layers_json.push(json!({
            "index": idx,
            "parallelizable": funcs_json.len() > 1,
            "functions": funcs_json,
        }));
    }

    // Estimated time: each layer is sequential, functions inside run in
    // parallel up to `parallelism` workers.
    let workers = effective_parallelism.max(1);
    let mut estimated_seconds: u64 = 0;
    for layer in &layers_json {
        let n = layer["functions"].as_array().map_or(0, |a| a.len()) as u64;
        let batches = n.div_ceil(workers as u64);
        estimated_seconds += batches * timeout_per_fn;
    }

    let file_count = scan_config
        .file_map
        .values()
        .collect::<HashSet<_>>()
        .len();
    let exported_total = registry.exported_functions().len();
    let unexported_total = registry.len().saturating_sub(exported_total);

    let skipped_json: Vec<serde_json::Value> = skipped
        .iter()
        .map(|s| {
            json!({
                "function": s.function_name,
                "reason": s.reason,
                "category": "unsupported",
            })
        })
        .collect();

    json!({
        "schema_version": 1,
        "kind": "scan_dry_run_plan",
        "summary": {
            "total_functions_discovered": analyses.len() + skipped.len(),
            "included_functions": analyses.len(),
            "skipped_unsupported_functions": skipped.len(),
            "omitted_unexported_functions": unexported_total,
            "exported_functions_in_registry": exported_total,
            "file_count": file_count,
            "layer_count": layers_json.len(),
            "selected_function_count": selected_function_count,
            "workers": workers,
            "timeout_per_function_seconds": timeout_per_fn,
            "estimated_total_seconds": estimated_seconds,
        },
        "config": {
            "parallelism": workers,
            "timeout_per_function_seconds": timeout_per_fn,
            "max_iterations_per_function": scan_config.max_iterations_per_function,
            "capture_side_effects": scan_config.capture_side_effects,
            "workers_per_function": scan_config.workers_per_fn,
            "isolation": format!("{:?}", scan_config.isolation),
            "coverage_mode": format!("{:?}", scan_config.coverage_mode),
            "write_artifacts": scan_config.write_artifacts,
            "stratum": scan_config.stratum.as_ref().map(|s| format!("{s:?}")),
        },
        "layers": layers_json,
        "skipped": skipped_json,
    })
}

/// Render a TypeInfo as a short label for the dry-run JSON plan.
fn format_type_label(t: &shatter_core::types::TypeInfo) -> String {
    use shatter_core::types::TypeInfo;
    match t {
        TypeInfo::Int => "int".to_string(),
        TypeInfo::Float => "float".to_string(),
        TypeInfo::Str => "string".to_string(),
        TypeInfo::Bool => "bool".to_string(),
        TypeInfo::Array { .. } => "array".to_string(),
        TypeInfo::Object { .. } => "object".to_string(),
        TypeInfo::Union { .. } => "union".to_string(),
        TypeInfo::Nullable { .. } => "nullable".to_string(),
        TypeInfo::Complex { kind, .. } => format!("{kind:?}"),
        TypeInfo::Opaque { label, .. } => label.clone(),
        TypeInfo::Unknown => "unknown".to_string(),
    }
}

/// `FunctionEntry` is a summary view: it stores `branch_count` but discards
/// the underlying `branches`, `literals`, `loops`, `source_file`,
/// `adapter_hints`, and `invocation_model` from the original
/// `FunctionAnalysis`. The scan orchestrator and downstream dry-run report
/// require the full record. We rehydrate from the registry's preserved
/// analyses (`registry.analysis(qn)`); registries built without analyses
/// (e.g. `FunctionRegistry::from_raw` in tests) fall back to a synthesized
/// analysis that mirrors the prior, lossy behavior — preserving test
/// compatibility while fixing real-world scans (str-jeen.45).
///
/// Returns `(analyses, file_map)` where `file_map` maps function name →
/// source file path.
pub(crate) fn rebuild_analyses_from_registry(
    registry: &shatter_core::batch_analyze::FunctionRegistry,
    all_functions: bool,
) -> (
    Vec<shatter_core::protocol::FunctionAnalysis>,
    HashMap<String, String>,
) {
    let mut all_analyses = Vec::new();
    let mut file_map: HashMap<String, String> = HashMap::new();

    for entry in registry.entries() {
        if !all_functions && !entry.exported {
            continue;
        }

        let file_path_string = entry.file_path.to_string_lossy().into_owned();
        let qualified = shatter_core::batch_analyze::FunctionRegistry::qualified_name(
            &entry.file_path,
            &entry.name,
        );

        // str-fuhw: file_map is keyed by qualified ID `"<file>::<name>"`
        // rather than bare name so two functions sharing a name across
        // files (Write, Generate, ServeHTTP, ...) do not overwrite each
        // other's file path. Downstream lookups in the orchestrator and
        // report are migrated to use qualified IDs.
        file_map.insert(qualified.clone(), file_path_string.clone());

        let mut analysis = match registry.analysis(&qualified) {
            Some(a) => a.clone(),
            None => shatter_core::protocol::FunctionAnalysis {
                name: entry.name.clone(),
                params: entry.params.clone(),
                return_type: entry.return_type.clone(),
                branches: vec![],
                dependencies: entry.dependencies.clone(),
                exported: entry.exported,
                start_line: entry.start_line,
                end_line: entry.end_line,
                literals: vec![],
                crypto_boundaries: entry.crypto_boundaries.clone(),
                loops: vec![],
                source_file: None,
                adapter_hints: vec![],
                invocation_model: shatter_core::protocol::InvocationModel::Direct,
            },
        };
        // str-fuhw: ensure `source_file` reflects the analysis's true on-disk
        // location so `behavior::CallGraph::from_analyses` can produce
        // qualified node IDs and disambiguate duplicate bare names. If a
        // frontend already populated `source_file` (re-export indirection),
        // preserve that value.
        if analysis.source_file.is_none() {
            analysis.source_file = Some(file_path_string);
        }
        all_analyses.push(analysis);
    }

    (all_analyses, file_map)
}

/// Print a markdown-style summary report to stdout, rendered with termimad when `use_color` is true.
///
/// `all_functions` reflects the CLI `--all` flag (see `shatter-cli/src/args.rs`)
/// and is reported faithfully in the summary so the reader can tell whether
/// unexported functions were included or intentionally omitted (str-z06h).
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
    all_functions: bool,
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
    let exported_count = registry.exported_functions().len();
    let unexported_count = registry.len().saturating_sub(exported_count);
    writeln!(md, "- **Exported functions**: {exported_count}").unwrap();
    // str-z06h: report unexported counts so readers can tell whether private
    // functions were explored or intentionally omitted. The filter lives in
    // `rebuild_analyses_from_registry` and is controlled by the `--all` CLI flag.
    writeln!(md, "- **Unexported functions**: {unexported_count}").unwrap();
    if unexported_count > 0 && !all_functions {
        writeln!(
            md,
            "  - _Omitted from this run. Re-run with `--all` to include them._",
        )
        .unwrap();
    }
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
            )
            .unwrap();
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
    content.push_str(&format!(
        "- Call graph edges: {}\n",
        call_graph.edge_count()
    ));
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

    std::fs::write(&summary_path, &content).map_err(|e| format!("failed to write summary: {e}"))?;
    log::info!("Wrote analysis report to {}", summary_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::batch_analyze::{FunctionEntry, FunctionRegistry};
    use shatter_core::protocol::{BranchInfo, BranchType, FunctionAnalysis, InvocationModel};
    use shatter_core::types::{ParamInfo, TypeInfo};

    fn analysis_with_source(source_file: Option<&str>) -> FunctionAnalysis {
        FunctionAnalysis {
            name: "f".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: source_file.map(str::to_string),
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        }
    }

    /// str-14en regression: a mixed Rust + TypeScript scan previously
    /// dispatched every file through whichever frontend
    /// `needed_langs.iter().next()` happened to pick (a HashSet — order is
    /// non-deterministic), so TS files were routinely fed to the Rust
    /// frontend's `syn::parse_file` and surfaced as `cannot parse string
    /// into token stream` errors. The scan loop now groups analyses by
    /// `analysis_language` and runs one orchestrator pass per language.
    /// This test pins the language-detection contract those groups rely on.
    #[test]
    fn analysis_language_classifies_by_source_extension() {
        assert_eq!(
            analysis_language(&analysis_with_source(Some("web/src/api/client.ts"))),
            Some(DiscoveryLanguage::TypeScript),
        );
        assert_eq!(
            analysis_language(&analysis_with_source(Some("ui/Shell.tsx"))),
            Some(DiscoveryLanguage::TypeScript),
        );
        assert_eq!(
            analysis_language(&analysis_with_source(Some("crates/foo/src/lib.rs"))),
            Some(DiscoveryLanguage::Rust),
        );
        assert_eq!(
            analysis_language(&analysis_with_source(Some("pkg/scan/scan.go"))),
            Some(DiscoveryLanguage::Go),
        );
        assert_eq!(
            analysis_language(&analysis_with_source(Some("notes.md"))),
            None,
        );
        assert_eq!(analysis_language(&analysis_with_source(None)), None);
    }

    /// str-jeen.45 regression: a Go function with a `range` loop + an `if`
    /// branch must report two branches in the dry-run plan, not zero. The
    /// fix lives in `rebuild_analyses_from_registry`, which now reads the
    /// preserved `FunctionAnalysis` from the registry instead of
    /// synthesizing a stripped-down record with `branches: vec![]`.
    #[test]
    fn rebuild_analyses_preserves_go_range_and_if_branches() {
        const GO_FILE_PATH: &str = "/src/loop_with_branch.go";
        const FUNCTION_NAME: &str = "ScanItems";
        const RANGE_LOOP_LINE: u32 = 3;
        const IF_BRANCH_LINE: u32 = 4;
        const EXPECTED_BRANCH_COUNT: usize = 2;

        let analysis = FunctionAnalysis {
            name: FUNCTION_NAME.into(),
            exported: true,
            params: vec![ParamInfo {
                name: "items".into(),
                typ: TypeInfo::Array {
                    element: Box::new(TypeInfo::Int),
                },
                type_name: None,
            }],
            branches: vec![
                BranchInfo {
                    id: 0,
                    line: RANGE_LOOP_LINE,
                    condition_text: "for _, v := range items".into(),
                    condition: None,
                    branch_type: BranchType::For,
                },
                BranchInfo {
                    id: 1,
                    line: IF_BRANCH_LINE,
                    condition_text: "v > 0".into(),
                    condition: None,
                    branch_type: BranchType::If,
                },
            ],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 8,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        };

        let entry = FunctionEntry {
            file_path: PathBuf::from(GO_FILE_PATH),
            name: FUNCTION_NAME.into(),
            exported: true,
            params: analysis.params.clone(),
            return_type: analysis.return_type.clone(),
            dependencies: vec![],
            crypto_boundaries: vec![],
            branch_count: EXPECTED_BRANCH_COUNT,
            start_line: analysis.start_line,
            end_line: analysis.end_line,
        };

        let qualified =
            FunctionRegistry::qualified_name(&PathBuf::from(GO_FILE_PATH), FUNCTION_NAME);
        let mut index = HashMap::new();
        index.insert(qualified.clone(), 0);
        let mut analyses = HashMap::new();
        analyses.insert(qualified, analysis);
        let registry = FunctionRegistry::from_raw_with_analyses(vec![entry], index, analyses);

        let (rebuilt, file_map) = rebuild_analyses_from_registry(&registry, false);
        assert_eq!(rebuilt.len(), 1);
        assert_eq!(
            rebuilt[0].branches.len(),
            EXPECTED_BRANCH_COUNT,
            "branches must survive registry round-trip; \
             scan dry-run reported `Branches: 0` before str-jeen.45 fix",
        );
        assert!(matches!(
            rebuilt[0].branches[0].branch_type,
            BranchType::For
        ));
        assert!(matches!(rebuilt[0].branches[1].branch_type, BranchType::If));
        // str-fuhw: file_map is keyed by qualified ID, not bare name.
        let qualified_key = format!("{GO_FILE_PATH}::{FUNCTION_NAME}");
        assert_eq!(
            file_map.get(&qualified_key).map(String::as_str),
            Some(GO_FILE_PATH),
        );
        // The rehydrated analysis carries its source_file so the call
        // graph in `behavior::CallGraph::from_analyses` can produce
        // qualified node IDs.
        assert_eq!(rebuilt[0].source_file.as_deref(), Some(GO_FILE_PATH),);
    }

    /// str-z06h: the visibility filter inside `rebuild_analyses_from_registry`
    /// drops unexported functions when `all_functions = false` and keeps them
    /// when `all_functions = true`. This is the seam that enforces the
    /// documented Go private-function opt-in policy
    /// (see `docs/go-frontend-scope-limits.md`).
    #[test]
    fn rebuild_analyses_visibility_filter_honors_all_functions_flag() {
        fn make_entry(name: &str, exported: bool) -> FunctionEntry {
            FunctionEntry {
                file_path: PathBuf::from(format!("/src/{name}.go")),
                name: name.into(),
                exported,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![],
                crypto_boundaries: vec![],
                branch_count: 0,
                start_line: 1,
                end_line: 2,
            }
        }
        let entries = vec![
            make_entry("Exported", true),
            make_entry("hidden", false),
        ];
        let mut index = HashMap::new();
        index.insert("/src/Exported.go::Exported".to_string(), 0);
        index.insert("/src/hidden.go::hidden".to_string(), 1);
        let registry = FunctionRegistry::from_raw(entries, index);

        let (default_run, _) = rebuild_analyses_from_registry(&registry, false);
        let default_names: Vec<&str> = default_run.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            default_names,
            vec!["Exported"],
            "default scan must omit unexported functions",
        );

        let (opt_in_run, _) = rebuild_analyses_from_registry(&registry, true);
        let opt_in_names: Vec<&str> = opt_in_run.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            opt_in_names,
            vec!["Exported", "hidden"],
            "--all scan must include unexported functions",
        );
    }

    /// Registries built without analyses (e.g. via `FunctionRegistry::from_raw`)
    /// fall back to a synthesized analysis. The synthesized record has
    /// `branches: vec![]`, matching the prior behavior — this preserves
    /// compatibility with tests in `shatter-core` that build registries by
    /// hand.
    #[test]
    fn rebuild_analyses_falls_back_when_registry_lacks_analyses() {
        let entry = FunctionEntry {
            file_path: PathBuf::from("/src/a.ts"),
            name: "raw".into(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Int,
            dependencies: vec![],
            crypto_boundaries: vec![],
            branch_count: 7,
            start_line: 1,
            end_line: 10,
        };
        let mut index = HashMap::new();
        index.insert("/src/a.ts::raw".to_string(), 0);
        let registry = FunctionRegistry::from_raw(vec![entry], index);

        let (rebuilt, _) = rebuild_analyses_from_registry(&registry, false);
        assert_eq!(rebuilt.len(), 1);
        assert!(rebuilt[0].branches.is_empty());
        assert_eq!(rebuilt[0].name, "raw");
    }
}
