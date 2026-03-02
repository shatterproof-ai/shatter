use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use shatter_core::analysis_cache::AnalysisCache;
use shatter_core::batch_analyze::{self, FunctionRegistry};
use shatter_core::behavior::BehaviorMap;
use shatter_core::cache::BehaviorMapCache;
use shatter_core::call_graph::CallGraph;
use shatter_core::config::{self as shatter_config, ShatterConfig};
use shatter_core::discovery::{self, DiscoveryOptions, Language as DiscoveryLanguage};
use shatter_core::explorer::{self, ExploreConfig, ReportOptions};
use shatter_core::executability;
use shatter_core::export;
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::report;
use shatter_core::scan_orchestrator::{self, ScanConfig, SkippedFunction};
use shatter_core::spec::FileSpecBundle;
use shatter_core::scope::{ScopeConfig, ScopeMatcher};
use shatter_core::snapshot;

mod embedded_frontend;
mod embedded_go_frontend;

/// Shatter: automatic exploratory testing via concolic execution.
#[derive(Parser, Debug)]
#[command(name = "shatter", version, about)]
struct Cli {
    /// Log verbosity level: error, warn, info (default), debug, trace.
    #[arg(long, global = true, default_value = "info")]
    log_level: LogLevel,

    /// Increase verbosity (-v = debug, -vv = trace).
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Decrease verbosity to warnings and errors only.
    #[arg(short = 'q', long = "quiet", global = true)]
    quiet: bool,

    /// Show per-function performance stats.
    #[arg(long, global = true)]
    perf: bool,

    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    /// Resolve the effective log level from --log-level, -v, and -q flags.
    fn effective_log_level(&self) -> LogLevel {
        if self.quiet {
            return LogLevel::Warn;
        }
        match self.verbose {
            0 => self.log_level,
            1 => LogLevel::Debug,
            _ => LogLevel::Trace,
        }
    }
}

/// Terminal color support based on TTY detection.
struct Colors {
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
}

impl Colors {
    fn detect() -> Self {
        if std::io::stdout().is_terminal() {
            Colors {
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                reset: "\x1b[0m",
            }
        } else {
            Colors {
                bold: "",
                dim: "",
                reset: "",
            }
        }
    }
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Explore a function by analyzing its branches and generating test inputs.
    Explore {
        /// Targets to explore, in <file>:<function> format.
        /// The file extension determines the language frontend (.ts = TypeScript, .go = Go).
        #[arg(required = true)]
        targets: Vec<String>,

        /// Maximum number of iterations for the concolic loop.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Timeout in seconds for the entire exploration.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Path to a scope configuration YAML file (shatter.scope.yaml).
        #[arg(long)]
        scope: Option<PathBuf>,

        /// Only run the analyze phase (skip exploration).
        #[arg(long)]
        analyze_only: bool,

        /// Show behavior clusters after exploration.
        #[arg(long)]
        show_clusters: bool,

        /// Directory for caching behavior maps across runs.
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter/cache/`.
        #[arg(long, env = "SHATTER_CACHE_DIR")]
        cache_dir: Option<PathBuf>,

        /// Disable behavior map caching entirely.
        #[arg(long)]
        no_cache: bool,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Path to a candidate inputs JSON file (overrides .shatter/ config inputs).
        #[arg(long)]
        inputs: Option<PathBuf>,

        /// Execution timeout in seconds for each function invocation in the frontend
        /// (e.g., how long a single Go function call may run). Default: 10s.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        /// Default: 30s.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,

        /// Path to a .shatter/config.yaml file (bypasses hierarchical discovery).
        #[arg(long = "config")]
        config_path: Option<PathBuf>,

        /// Write per-file spec JSON to a file (implies --spec-json).
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Output a behavioral specification (markdown by default, JSON with --spec-json).
        #[arg(long)]
        spec: bool,

        /// Output the behavioral specification as JSON instead of markdown.
        #[arg(long)]
        spec_json: bool,

        /// Disable built-in boundary values as seed inputs.
        #[arg(long)]
        no_boundary_values: bool,

        /// Enable Daikon-style invariant detection on explored functions.
        #[arg(long)]
        invariants: bool,
    },

    /// Scan a directory for source files, analyze and explore all functions in
    /// dependency order, using behavior maps as mocks.
    Scan {
        /// Directory to scan for source files.
        #[arg(required = true)]
        directory: String,

        /// Language to scan: typescript, go. Auto-detected from file extensions if omitted.
        #[arg(long)]
        language: Option<String>,

        /// Glob patterns for files to include (e.g. "**/*.ts"). May be repeated.
        #[arg(long)]
        include: Vec<String>,

        /// Glob patterns for files to exclude (e.g. "**/vendor/**"). May be repeated.
        #[arg(long)]
        exclude: Vec<String>,

        /// Scan all functions, including non-exported ones.
        #[arg(long)]
        all: bool,

        /// Maximum directory traversal depth.
        #[arg(long)]
        max_depth: Option<usize>,

        /// Per-function exploration timeout in seconds. Functions exceeding this
        /// limit are skipped without aborting the scan. Default: 30s.
        #[arg(long, default_value_t = 30)]
        timeout_per_fn: u64,

        /// Total scan timeout in seconds. Default: 300s.
        #[arg(long, default_value_t = 300)]
        timeout_total: u64,

        /// Number of parallel frontend subprocesses for exploration.
        /// Default: number of available CPUs (0 = auto-detect).
        #[arg(long, default_value_t = 0)]
        parallelism: usize,

        /// Path to a mock configuration YAML file.
        #[arg(long)]
        mock_config: Option<PathBuf>,

        /// Output directory for reports (default: ./shatter-report/).
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Report format: json (default), markdown, or both.
        #[arg(long, default_value = "json")]
        format: String,

        /// Generate test files after scan. Framework: jest, vitest, or gotest.
        #[arg(long)]
        emit_tests: Option<String>,

        /// Show what would be scanned without executing.
        #[arg(long)]
        dry_run: bool,

        /// Resume a previous scan from a state file.
        #[arg(long)]
        resume: Option<PathBuf>,

        /// Emit progress events to stderr during scan.
        #[arg(long)]
        progress: bool,

        /// Select a representative core sample of functions to explore.
        /// Accepts a percentage (e.g. "50%") or absolute count (e.g. "20").
        #[arg(long)]
        core_sample: Option<String>,

        /// Seed for deterministic core sample selection.
        /// Default: hash of (directory + git HEAD).
        #[arg(long)]
        seed: Option<u64>,

        /// Stratum filter: explore only specific call graph layers.
        /// Examples: "0" (leaves), "0..3", "-2..-0" (top 3 layers), "3.."
        #[arg(long)]
        stratum: Option<String>,

        /// Maximum number of iterations per function.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Directory for caching behavior maps across runs.
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter/cache/`.
        #[arg(long, env = "SHATTER_CACHE_DIR")]
        cache_dir: Option<PathBuf>,

        /// Disable behavior map caching entirely.
        #[arg(long)]
        no_cache: bool,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation in the frontend.
        /// Default: 10s.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        /// Default: 30s.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,
    },

    /// Export generated tests from behavior maps produced by exploration.
    ///
    /// Runs exploration on the given targets, then generates test files in the
    /// specified framework format.
    ExportTests {
        /// Targets to explore and export tests for, in <file>:<function> format.
        #[arg(required = true)]
        targets: Vec<String>,

        /// Test framework to generate: jest, vitest, or gotest.
        #[arg(long, default_value = "jest")]
        framework: String,

        /// Module path for imports (Jest: relative path; Go: package name).
        #[arg(long, default_value = ".")]
        module_path: String,

        /// Write output to a file instead of stdout.
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Maximum number of iterations for the concolic loop.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Timeout in seconds for the entire exploration.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Path to a scope configuration YAML file.
        #[arg(long)]
        scope: Option<PathBuf>,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation in the frontend.
        /// Default: 10s.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        /// Default: 30s.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,
    },

    /// Discover, analyze, and explore an entire repository in one shot.
    ///
    /// Accepts a local directory path, discovers all supported source files,
    /// analyzes them, builds a call graph, and explores functions in dependency
    /// order (leaves first). Outputs a markdown summary report.
    Run {
        /// Path to the repository root (local directory).
        #[arg(required = true)]
        path: String,

        /// Write per-function reports to this directory.
        #[arg(long)]
        output_dir: Option<PathBuf>,

        /// Maximum number of iterations per function (default: 50).
        #[arg(long, default_value_t = 50)]
        max_iterations: u32,

        /// Overall timeout in seconds (default: 300).
        #[arg(long, default_value_t = 300)]
        timeout: u64,

        /// Only discover and analyze, skip exploration.
        #[arg(long)]
        analyze_only: bool,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation in the frontend.
        /// Default: 10s.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        /// Default: 30s.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,
    },

    /// Compare current behaviors against a previous snapshot to detect regressions.
    ///
    /// Exit code is 0 when all behaviors match, nonzero when regressions are found.
    Diff {
        /// Path to the previous snapshot JSON file.
        #[arg(required = true)]
        snapshot: PathBuf,

        /// Path to the current snapshot JSON file to compare against.
        #[arg(required = true)]
        current: PathBuf,

        /// Output the diff result as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Compare two function specifications and report behavioral changes.
    ///
    /// Accepts two spec JSON files (as produced by `explore --spec-json`).
    /// Exit code is 0 when specs are equivalent, nonzero when regressions are found.
    #[command(name = "spec-diff")]
    SpecDiff {
        /// Path to the old (baseline) spec JSON file.
        #[arg(required = true)]
        old: PathBuf,

        /// Path to the new (current) spec JSON file.
        #[arg(required = true)]
        new: PathBuf,

        /// Output the diff result as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

/// A parsed `<file>:<function>` target.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    file: PathBuf,
    function: Option<String>,
    language: Language,
}

/// Supported language frontends.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Language {
    TypeScript,
    Go,
}

impl Language {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" => Some(Language::TypeScript),
            "go" => Some(Language::Go),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Go => "go",
        }
    }
}

/// Parse a `<file>:<function>` target string.
///
/// If there is no colon, the entire string is treated as a file path (analyze all functions).
fn parse_target(target: &str) -> Result<Target, String> {
    let (file_str, function) = match target.rsplit_once(':') {
        Some((f, func)) if !func.is_empty() => (f, Some(func.to_string())),
        _ => (target, None),
    };

    let file = PathBuf::from(file_str);
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| format!("cannot detect language: no file extension in '{file_str}'"))?;

    let language = Language::from_extension(ext)
        .ok_or_else(|| format!("unsupported file extension '.{ext}' in '{file_str}'"))?;

    Ok(Target {
        file,
        function,
        language,
    })
}

/// Build a `FrontendConfig` for the given language, with log level propagation.
fn frontend_config(
    language: Language,
    timeout: Duration,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
) -> Result<FrontendConfig, String> {
    let (command, args) = match language {
        Language::TypeScript => {
            let bundle_path = embedded_frontend::ensure_extracted()?;
            (
                PathBuf::from("node"),
                vec![bundle_path.to_string_lossy().into_owned()],
            )
        }
        Language::Go => {
            let binary_path = embedded_go_frontend::ensure_extracted()?;
            (binary_path, vec![])
        }
    };

    let mut config = FrontendConfig::new(command);
    config.args = args;
    config.request_timeout = timeout;
    apply_frontend_env(&mut config, log_level, exec_timeout, build_timeout);
    Ok(config)
}

/// Apply standard environment variables to a frontend config.
fn apply_frontend_env(
    config: &mut FrontendConfig,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
) {
    config.env_vars.push((
        LogLevel::ENV_VAR.to_string(),
        log_level.as_str().to_string(),
    ));
    config.env_vars.push((
        "SHATTER_EXEC_TIMEOUT".to_string(),
        exec_timeout.to_string(),
    ));
    config.env_vars.push((
        "SHATTER_BUILD_TIMEOUT".to_string(),
        build_timeout.to_string(),
    ));
}

/// Run the explore command.
// Each argument corresponds to a CLI flag; grouping into a struct would add indirection
// without improving clarity since this is only called from one callsite.
#[allow(clippy::too_many_arguments)]
async fn run_explore(
    targets: &[String],
    max_iterations: u32,
    timeout: u64,
    scope_path: Option<&Path>,
    analyze_only: bool,
    _show_clusters: bool,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    inputs_path: Option<&Path>,
    config_path: Option<&Path>,
    output_path: Option<&Path>,
    log_level: LogLevel,
    show_perf: bool,
    colors: &Colors,
    show_spec: bool,
    spec_as_json: bool,
    detect_invariants: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            if log_level >= LogLevel::Info {
                eprintln!("Loaded scope config from {}", path.display());
            }
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

    let req_timeout = Duration::from_secs(request_timeout);

    let mut file_spec_bundles: Vec<FileSpecBundle> = Vec::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target
            .function
            .as_deref()
            .unwrap_or("(all)");

        if log_level >= LogLevel::Debug {
            eprintln!(
                "[debug] Exploring {file_str}:{func_display} [language={}, max_iterations={max_iterations}]",
                target.language.label()
            );
        }

        let config = frontend_config(target.language, req_timeout, log_level, exec_timeout, build_timeout)?;
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!(
                "failed to spawn {} frontend: {e}",
                target.language.label()
            )
        })?;

        if log_level >= LogLevel::Debug {
            eprintln!(
                "[debug] Frontend connected (language={})",
                frontend.language().unwrap_or("unknown")
            );
        }

        // Analyze phase
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
            })
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match &analyze_response.result {
            ResponseResult::Analyze { functions } => {
                if log_level >= LogLevel::Debug {
                    eprintln!("  Found {} function(s):", functions.len());
                    for func in functions {
                        eprintln!("    - {} ({} params, {} branches)",
                            func.name,
                            func.params.len(),
                            func.branches.len(),
                        );
                    }
                }
            }
            ResponseResult::Error { code, message, .. } => {
                eprintln!("  Analyze error ({code:?}): {message}");
                shutdown_frontend(frontend).await;
                continue;
            }
            other => {
                eprintln!("  Unexpected analyze response: {other:?}");
                shutdown_frontend(frontend).await;
                continue;
            }
        }

        let functions = match &analyze_response.result {
            ResponseResult::Analyze { functions } => functions.clone(),
            _ => unreachable!("already matched above"),
        };

        if analyze_only {
            if log_level >= LogLevel::Info {
                for func in &functions {
                    eprintln!(
                        "{}{}{}  ({file_str}:{})",
                        colors.bold, func.name, colors.reset, func.start_line
                    );
                    eprintln!(
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

        // Load .shatter/ config for this target
        let shatter_configs: Vec<ShatterConfig> = if let Some(cp) = config_path {
            // Explicit config bypasses discovery
            let cfg = shatter_config::parse_config(cp)
                .map_err(|e| format!("failed to load config: {e}"))?;
            if log_level >= LogLevel::Debug {
                eprintln!("[debug] Loaded config from {}", cp.display());
            }
            vec![cfg]
        } else {
            // Hierarchical discovery from target file's directory
            let target_dir = target.file.parent().unwrap_or(Path::new("."));
            shatter_config::discover_configs(target_dir)
                .map_err(|e| format!("config discovery error: {e}"))?
        };

        // Exploration phase: generate random inputs and execute
        let mut skipped_unexecutable: Vec<(String, Vec<executability::SkipReason>)> = Vec::new();
        let mut file_specs: Vec<shatter_core::spec::FunctionSpec> = Vec::new();
        for func in &functions {
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
                if log_level >= LogLevel::Debug {
                    eprintln!("\n  [debug] Skipping {} (skip=true in config)", func.name);
                }
                continue;
            }

            // Check for unexecutable parameter types (opaque types like net.Socket).
            let skip_reasons = executability::check_executability(&func.params, &[]);
            if !skip_reasons.is_empty() {
                if log_level >= LogLevel::Debug {
                    eprintln!("\n  [debug] Skipping {} (unexecutable parameter types)", func.name);
                }
                skipped_unexecutable.push((func.name.clone(), skip_reasons));
                continue;
            }

            // Generate auto-mocks for external dependencies.
            let auto_mocks = shatter_core::auto_mock::generate_auto_mocks(
                &func.dependencies,
                None,
                &resolved.mock_overrides,
                &[],
            );

            let explore_config = ExploreConfig {
                file: file_str.to_string(),
                max_iterations: resolved.max_iterations,
                seed: None,
                mocks: auto_mocks,
                setup_file: resolved.setup.as_ref().map(|p| p.display().to_string()),
                setup_mode: resolved.setup_mode,
                value_sources: shatter_core::input_gen::resolve_value_sources(
                    &func.params,
                    &resolved.param_generators,
                    &resolved.generators,
                ),
                capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
            };

            // Convert candidate inputs for logging
            if log_level >= LogLevel::Debug {
                if !resolved.candidate_inputs.is_empty() {
                    eprintln!(
                        "\n  [debug] Exploring {} ({} candidate input(s) from config)...",
                        func.name,
                        resolved.candidate_inputs.len()
                    );
                } else {
                    eprintln!("\n  [debug] Exploring {}...", func.name);
                }
            }

            // Store candidate inputs on the ExploreConfig is not needed;
            // they are used by the explorer via its own mechanism.
            // For now, we just log that they exist. The actual wiring into
            // the orchestrator's worklist happens when the concolic orchestrator
            // is used. For the random explorer (explore_function), we pass them
            // as extra seed inputs.
            let _ = &shatter_configs; // suppress unused warning

            let func_start = Instant::now();

            match explorer::explore_function(&mut frontend, func, &explore_config).await {
                Ok(result) => {
                    let wall_time = func_start.elapsed();

                    if log_level >= LogLevel::Info {
                        if log_level >= LogLevel::Trace {
                            eprint!("{}", explorer::format_exploration_report_verbose(&result));
                        } else {
                            let report_opts = ReportOptions {
                                location: Some(format!("{file_str}:{}", func.start_line)),
                                show_perf,
                                wall_time: Some(wall_time),
                                coverage_metrics: None,
                            };
                            eprint!("{}", explorer::format_exploration_report(&result, &report_opts));
                        }
                        eprintln!();
                    }

                    // Spec output: build equivalence classes and spec
                    if show_spec || detect_invariants {
                        let eq_classes =
                            shatter_core::equivalence::group_into_classes(&result.raw_results);
                        let location = Some(format!("{file_str}:{}", func.start_line));
                        let spec = if detect_invariants {
                            shatter_core::spec::build_spec_with_invariants(
                                &result, &eq_classes, location, None,
                            )
                        } else {
                            shatter_core::spec::build_spec(&result, &eq_classes, location, None)
                        };
                        if output_path.is_some() {
                            // Collect for file-level bundle output
                            file_specs.push(spec);
                        } else if spec_as_json {
                            match shatter_core::spec::format_spec_json(&spec) {
                                Ok(json) => println!("{json}"),
                                Err(e) => eprintln!("  Error serializing spec: {e}"),
                            }
                        } else {
                            print!("{}", shatter_core::spec::format_spec_markdown(&spec));
                        }
                    }

                    let behavior_map =
                        BehaviorMap::from_exploration_result(&func.name, &result);
                    if let Some(ref cache) = cache
                        && let Err(e) = cache.store(&behavior_map)
                    {
                        eprintln!("  Warning: failed to cache behavior map for {}: {e}", func.name);
                    }
                }
                Err(e) => {
                    eprintln!("  Exploration error for {}: {e}", func.name);
                }
            }
        }

        // Print summary of skipped unexecutable functions.
        if !skipped_unexecutable.is_empty() && log_level >= LogLevel::Info {
            eprintln!(
                "Skipped {} function(s) (unexecutable parameter types):",
                skipped_unexecutable.len()
            );
            for (name, reasons) in &skipped_unexecutable {
                for reason in reasons {
                    eprintln!(
                        "  {name}: param {:?} has opaque type {}",
                        reason.param_name, reason.opaque_label
                    );
                }
            }
        }

        // Collect file-level spec bundle when --output is set.
        if output_path.is_some() && !file_specs.is_empty() {
            file_spec_bundles.push(FileSpecBundle {
                file: file_str.to_string(),
                functions: file_specs,
            });
        }

        shutdown_frontend(frontend).await;
    }

    // Write collected file spec bundles to the output path.
    if let Some(out) = output_path
        && !file_spec_bundles.is_empty()
    {
        let json = shatter_core::spec::format_file_spec_json(&file_spec_bundles)
            .map_err(|e| format!("failed to serialize file spec bundles: {e}"))?;
        std::fs::write(out, &json)
            .map_err(|e| format!("failed to write output file {}: {e}", out.display()))?;
        if log_level >= LogLevel::Info {
            eprintln!(
                "Wrote {} file spec bundle(s) to {}",
                file_spec_bundles.len(),
                out.display()
            );
        }
    }

    Ok(())
}

/// Run the scan command: explore multiple functions in dependency order.
#[allow(clippy::too_many_arguments)]
async fn run_scan(
    directory: &str,
    language_filter: Option<&str>,
    include_patterns: &[String],
    exclude_patterns: &[String],
    all_functions: bool,
    max_depth: Option<usize>,
    max_iterations: u32,
    _timeout_total: u64,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    parallelism: usize,
    timeout_per_fn: u64,
    output_dir: Option<&Path>,
    report_format_str: &str,
    progress: bool,
    emit_tests: Option<&str>,
    dry_run: bool,
    _resume: Option<&Path>,
    mock_config: Option<&Path>,
    core_sample_spec: Option<&str>,
    core_sample_seed: Option<u64>,
    stratum_spec: Option<&str>,
    log_level: LogLevel,
) -> Result<(), Box<dyn std::error::Error>> {
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
        && lang != "typescript" && lang != "go"
    {
        return Err(format!(
            "unsupported language '{lang}': expected 'typescript' or 'go'"
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

    // Discover source files.
    let options = DiscoveryOptions {
        include_patterns: include_patterns.to_vec(),
        exclude_patterns: exclude_patterns.to_vec(),
        respect_gitignore: true,
        max_depth,
    };
    let files = discovery::discover_files(&root, &options)
        .map_err(|e| format!("file discovery failed: {e}"))?;

    // Filter by language if specified.
    let files: Vec<(PathBuf, DiscoveryLanguage)> = if let Some(lang) = language_filter {
        let target_lang = match lang {
            "typescript" => DiscoveryLanguage::TypeScript,
            "go" => DiscoveryLanguage::Go,
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
        eprintln!("No supported source files found in {}", root.display());
        return Ok(());
    }

    if log_level >= LogLevel::Info {
        eprintln!(
            "Discovered {} source file(s) in {}",
            analyzable_files.len(),
            root.display(),
        );
    }

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
        let config = frontend_config(cli_lang, req_timeout, log_level, exec_timeout, build_timeout)?;
        let frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {lang:?} frontend: {e}")
        })?;
        if log_level >= LogLevel::Debug {
            eprintln!(
                "[debug] Frontend connected (language={})",
                frontend.language().unwrap_or("unknown")
            );
        }
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
    )
    .await
    .map_err(|e| format!("batch analyze failed: {e}"))?;

    if log_level >= LogLevel::Debug {
        eprintln!(
            "  Found {} function(s) across {} file(s)",
            registry.len(),
            analyzable_files.len(),
        );
    }

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
            entry.file_path.to_string_lossy().to_string(),
        );
        all_analyses.push(shatter_core::protocol::FunctionAnalysis {
            name: entry.name.clone(),
            params: entry.params.clone(),
            return_type: entry.return_type.clone(),
            branches: vec![],
            dependencies: entry.dependencies.clone(),
            exported: entry.exported,
            start_line: 0,
            end_line: 0,
            literals: vec![],
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
            });
            false
        }
    });

    if !skipped_for_executability.is_empty() && log_level >= LogLevel::Info {
        eprintln!(
            "Skipped {} function(s) (unexecutable parameter types):",
            skipped_for_executability.len()
        );
        for skip in &skipped_for_executability {
            eprintln!("  {}: {}", skip.function_name, skip.reason);
        }
    }

    // Apply core sample selection if --core-sample is set.
    if let Some(spec) = core_sample_spec {
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
        let result = shatter_core::core_sample::select_core_sample(&entries, &cg, &cs_config);
        let included = result.all_included();
        let before = all_analyses.len();
        all_analyses.retain(|a| included.contains(&a.name));
        if log_level >= LogLevel::Info {
            eprintln!(
                "Core sample: selected {} of {} function(s) ({} sampled + {} dependency closure)",
                included.len(),
                before,
                result.selected.len(),
                result.dependency_closure.len(),
            );
        }
    }

    if all_analyses.is_empty() {
        eprintln!("No functions found to scan.");
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

    // Parse --stratum spec if provided.
    let parsed_stratum = if let Some(spec_str) = stratum_spec {
        Some(
            shatter_core::stratum::parse_stratum_spec(spec_str)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
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
            stratum: parsed_stratum.clone(),
            mock_overrides: HashMap::new(),
        };
        let plan = scan_orchestrator::format_dry_run_plan(
            &all_analyses,
            &skipped_for_executability,
            &scan_config,
        )
        .map_err(|e| format!("failed to build dry-run plan: {e}"))?;
        eprint!("{plan}");
        return Ok(());
    }

    if log_level >= LogLevel::Debug {
        eprintln!(
            "\n[debug] Scanning {} function(s) in dependency order ({} worker(s), {}s/fn)...\n",
            all_analyses.len(),
            effective_parallelism,
            timeout_per_fn,
        );
    }

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
    let fe_config = frontend_config(cli_lang, req_timeout, log_level, exec_timeout, build_timeout)?;

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
        stratum: parsed_stratum,
        mock_overrides,
    };

    let scan_start = Instant::now();
    let total_functions = all_analyses.len();

    if log_level >= LogLevel::Info {
        eprintln!(
            "Scanning {} function(s) in dependency order...",
            total_functions,
        );
    }

    match scan_orchestrator::parallel_scan(&fe_config, &all_analyses, &scan_config).await {
        Ok(result) => {
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
                } else if log_level >= LogLevel::Info {
                    eprintln!(
                        "  [{}/{}] {} ({:.1}s elapsed)",
                        i + 1,
                        total_functions,
                        fr.function_name,
                        elapsed.as_secs_f64(),
                    );
                }
            }

            print!("{}", scan_orchestrator::format_parallel_scan_report(&result));

            let report_dir = output_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./shatter-report/"));
            let scan_report = report::generate_report(&result, &scan_config.file_map);

            match report_format {
                report::ReportFormat::Json => {
                    match report::write_report(&scan_report, &report_dir) {
                        Ok(path) => eprintln!("Wrote JSON report to {}", path.display()),
                        Err(e) => eprintln!("Failed to write JSON report: {e}"),
                    }
                }
                report::ReportFormat::Markdown => {
                    match report::write_markdown_report(&scan_report, &report_dir) {
                        Ok(path) => eprintln!("Wrote markdown report to {}", path.display()),
                        Err(e) => eprintln!("Failed to write markdown report: {e}"),
                    }
                }
                report::ReportFormat::Both => {
                    match report::write_report(&scan_report, &report_dir) {
                        Ok(path) => eprintln!("Wrote JSON report to {}", path.display()),
                        Err(e) => eprintln!("Failed to write JSON report: {e}"),
                    }
                    match report::write_markdown_report(&scan_report, &report_dir) {
                        Ok(path) => eprintln!("Wrote markdown report to {}", path.display()),
                        Err(e) => eprintln!("Failed to write markdown report: {e}"),
                    }
                }
            }

            // Emit test files if --emit-tests was specified.
            if let Some(framework) = emit_tests {
                let tests_dir = output_dir
                    .map(PathBuf::from)
                    .unwrap_or_else(|| report_dir.clone());

                if let Err(e) = emit_test_files(&result, &scan_config.file_map, framework, &tests_dir) {
                    eprintln!("Failed to emit test files: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("Scan error: {e}");
        }
    }

    Ok(())
}

/// Emit test files from a parallel scan result.
fn emit_test_files(
    result: &scan_orchestrator::ParallelScanResult,
    file_map: &std::collections::HashMap<String, String>,
    framework: &str,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;

    for func_result in &result.function_results {
        let module_path = file_map
            .get(&func_result.function_name)
            .map(|s| s.as_str())
            .unwrap_or("./module");

        let test_code = match framework {
            "jest" => export::generate_jest_tests(&func_result.behavior_map, &func_result.function_name, module_path),
            "vitest" => export::generate_vitest_tests(&func_result.behavior_map, &func_result.function_name, module_path),
            "gotest" => export::generate_go_tests(&func_result.behavior_map, &func_result.function_name, module_path),
            _ => continue,
        };

        let ext = if framework == "gotest" { "go" } else { "ts" };
        let filename = format!("{}.test.{ext}", func_result.function_name);
        let path = output_dir.join(&filename);
        std::fs::write(&path, &test_code)?;
        eprintln!("  Wrote {}", path.display());
    }

    Ok(())
}

/// Run the diff command: compare two snapshots and report regressions.
///
/// Returns `Ok(true)` if there are regressions (nonzero exit), `Ok(false)` if clean.
fn run_diff(
    snapshot_path: &Path,
    current_path: &Path,
    output_json: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let previous = snapshot::Snapshot::read_from_file(snapshot_path)
        .map_err(|e| format!("failed to read previous snapshot '{}': {e}", snapshot_path.display()))?;
    let current = snapshot::Snapshot::read_from_file(current_path)
        .map_err(|e| format!("failed to read current snapshot '{}': {e}", current_path.display()))?;

    let result = snapshot::diff(&previous, &current);

    if output_json {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("failed to serialize diff result: {e}"))?;
        println!("{json}");
    } else {
        print!("{}", result.format_report());
    }

    Ok(result.has_regressions())
}

/// Run the spec-diff command: compare two function specifications.
///
/// Returns `Ok(true)` if there are regressions (nonzero exit), `Ok(false)` if clean.
fn run_spec_diff(
    old_path: &Path,
    new_path: &Path,
    output_json: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let old_contents = std::fs::read_to_string(old_path)
        .map_err(|e| format!("failed to read old spec '{}': {e}", old_path.display()))?;
    let new_contents = std::fs::read_to_string(new_path)
        .map_err(|e| format!("failed to read new spec '{}': {e}", new_path.display()))?;

    let old_spec: shatter_core::spec::FunctionSpec = serde_json::from_str(&old_contents)
        .map_err(|e| format!("failed to parse old spec '{}': {e}", old_path.display()))?;
    let new_spec: shatter_core::spec::FunctionSpec = serde_json::from_str(&new_contents)
        .map_err(|e| format!("failed to parse new spec '{}': {e}", new_path.display()))?;

    let result = shatter_core::spec_diff::diff_specs(&old_spec, &new_spec);

    if output_json {
        let json = shatter_core::spec_diff::format_spec_diff_json(&result)
            .map_err(|e| format!("failed to serialize spec diff: {e}"))?;
        println!("{json}");
    } else {
        print!("{}", shatter_core::spec_diff::format_spec_diff_text(&result));
    }

    Ok(result.has_regressions())
}

/// Run the export-tests command: explore targets and generate test code.
#[allow(clippy::too_many_arguments)]
async fn run_export_tests(
    targets: &[String],
    framework: &str,
    module_path: &str,
    output_path: Option<&Path>,
    max_iterations: u32,
    _timeout: u64,
    scope_path: Option<&Path>,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    _log_level: LogLevel,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate framework
    if framework != "jest" && framework != "vitest" && framework != "gotest" {
        return Err(format!("unsupported framework '{framework}': expected 'jest', 'vitest', or 'gotest'").into());
    }

    let _scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            eprintln!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;

    let req_timeout = Duration::from_secs(request_timeout);
    let mut all_output = String::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target.function.as_deref().unwrap_or("(all)");

        eprintln!("Exploring {file_str}:{func_display} for test export...");

        let config = frontend_config(target.language, req_timeout, LogLevel::Warn, exec_timeout, build_timeout)?;
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {} frontend: {e}", target.language.label())
        })?;

        // Analyze
        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
            })
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        let functions = match &analyze_response.result {
            ResponseResult::Analyze { functions } => functions.clone(),
            ResponseResult::Error { code, message, .. } => {
                eprintln!("  Analyze error ({code:?}): {message}");
                shutdown_frontend(frontend).await;
                continue;
            }
            other => {
                eprintln!("  Unexpected analyze response: {other:?}");
                shutdown_frontend(frontend).await;
                continue;
            }
        };

        // Explore each function and generate tests
        let explore_config = ExploreConfig {
            file: file_str.to_string(),
            max_iterations,
            seed: None,
            mocks: vec![],
            setup_file: None,
            setup_mode: shatter_core::config::SetupMode::PerFunction,
            value_sources: vec![],
            capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
        };

        for func in &functions {
            eprintln!("  Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config).await {
                Ok(result) => {
                    let behavior_map = BehaviorMap::from_exploration_result(&func.name, &result);

                    let test_code = match framework {
                        "jest" => export::generate_jest_tests(&behavior_map, &func.name, module_path),
                        "vitest" => export::generate_vitest_tests(&behavior_map, &func.name, module_path),
                        "gotest" => export::generate_go_tests(&behavior_map, &func.name, module_path),
                        _ => unreachable!("validated above"),
                    };

                    all_output.push_str(&test_code);
                    all_output.push('\n');
                }
                Err(e) => {
                    eprintln!("  Exploration error for {}: {e}", func.name);
                }
            }
        }

        shutdown_frontend(frontend).await;
    }

    match output_path {
        Some(path) => {
            std::fs::write(path, &all_output)
                .map_err(|e| format!("failed to write to '{}': {e}", path.display()))?;
            eprintln!("Wrote tests to {}", path.display());
        }
        None => {
            print!("{all_output}");
        }
    }

    Ok(())
}

/// Map discovery Language to CLI Language for frontend_config.
fn discovery_lang_to_cli_lang(lang: DiscoveryLanguage) -> Option<Language> {
    match lang {
        DiscoveryLanguage::TypeScript => Some(Language::TypeScript),
        DiscoveryLanguage::Go => Some(Language::Go),
        DiscoveryLanguage::Rust => None, // Rust frontend not yet supported for exploration
    }
}

/// Run the run command: discover, analyze, build call graph, explore, and report.
#[allow(clippy::too_many_arguments)]
async fn run_run(
    path: &str,
    output_dir: Option<&Path>,
    max_iterations: u32,
    timeout: u64,
    analyze_only: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    log_level: LogLevel,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    // Check for URL input (not yet supported)
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("git@") {
        return Err("URL/git clone input is not yet supported. Please provide a local directory path.".into());
    }

    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()).into());
    }

    let root = root.canonicalize()
        .map_err(|e| format!("failed to resolve path '{}': {e}", path))?;

    if log_level >= LogLevel::Debug {
        eprintln!("Shatter run: {}", root.display());
        eprintln!();
    }

    // Step 1: Discover files
    if log_level >= LogLevel::Debug {
        eprintln!("Discovering source files...");
    }
    let options = DiscoveryOptions::default();
    let files = discovery::discover_files(&root, &options)
        .map_err(|e| format!("file discovery failed: {e}"))?;

    if files.is_empty() {
        eprintln!("No supported source files found in {}", root.display());
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

    if log_level >= LogLevel::Debug {
        eprintln!("  Found {} file(s):", files.len());
        if !ts_files.is_empty() {
            eprintln!("    TypeScript: {}", ts_files.len());
        }
        if !go_files.is_empty() {
            eprintln!("    Go: {}", go_files.len());
        }
        if !rs_files.is_empty() {
            eprintln!("    Rust: {} (analysis not yet supported)", rs_files.len());
        }
        eprintln!();
    }

    // Filter to languages we can actually analyze (TS, Go)
    let analyzable_files: Vec<(PathBuf, DiscoveryLanguage)> = files
        .into_iter()
        .filter(|(_, lang)| discovery_lang_to_cli_lang(*lang).is_some())
        .collect();

    if analyzable_files.is_empty() {
        eprintln!("No analyzable source files found (only TypeScript and Go are supported).");
        return Ok(());
    }

    // Step 2: Spawn frontends for each language
    let req_timeout = Duration::from_secs(request_timeout);
    let mut frontends: HashMap<DiscoveryLanguage, Frontend> = HashMap::new();

    let needed_langs: std::collections::HashSet<DiscoveryLanguage> = analyzable_files
        .iter()
        .map(|(_, lang)| *lang)
        .collect();

    for lang in &needed_langs {
        let cli_lang = discovery_lang_to_cli_lang(*lang)
            .ok_or_else(|| format!("no frontend for {lang:?}"))?;
        let config = frontend_config(cli_lang, req_timeout, log_level, exec_timeout, build_timeout)?;
        let frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!("failed to spawn {lang:?} frontend: {e}")
        })?;
        if log_level >= LogLevel::Debug {
            eprintln!(
                "Frontend connected (language={})",
                frontend.language().unwrap_or("unknown")
            );
        }
        frontends.insert(*lang, frontend);
    }

    // Step 3: Batch analyze
    if log_level >= LogLevel::Debug {
        eprintln!();
        eprintln!("Analyzing {} file(s)...", analyzable_files.len());
    }
    let registry = batch_analyze::batch_analyze(
        &mut frontends,
        &analyzable_files,
        None,
    )
    .await
    .map_err(|e| format!("batch analyze failed: {e}"))?;

    let total_functions = registry.len();
    let total_branches: usize = registry.entries().iter().map(|e| e.branch_count).sum();

    if log_level >= LogLevel::Debug {
        eprintln!("  Found {} function(s) with {} total branch(es)", total_functions, total_branches);
        eprintln!();
    }

    if total_functions == 0 {
        eprintln!("No functions found to explore.");
        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 4: Build call graph
    if log_level >= LogLevel::Debug {
        eprintln!("Building call graph...");
    }
    let call_graph = CallGraph::from_registry(&registry);
    let layers = call_graph.topological_layers();
    let cycles = call_graph.cycle_groups();

    if log_level >= LogLevel::Debug {
        eprintln!(
            "  {} node(s), {} edge(s), {} layer(s), {} cycle(s)",
            call_graph.node_count(),
            call_graph.edge_count(),
            layers.len(),
            cycles.len(),
        );
        eprintln!();
    }

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
        );

        if let Some(dir) = output_dir {
            write_analysis_report(dir, &registry, &call_graph)?;
        }

        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 5: Explore in dependency order (layer by layer)
    if log_level >= LogLevel::Debug {
        eprintln!("Exploring functions in dependency order...");
        eprintln!();
    }

    let mut exploration_results: Vec<(String, explorer::ExplorationResult)> = Vec::new();

    for (layer_idx, layer) in layers.iter().enumerate() {
        if log_level >= LogLevel::Debug {
            eprintln!("  Layer {} ({} function(s)):", layer_idx, layer.len());
        }

        for qualified_name in layer {
            let entry = match registry.get(qualified_name) {
                Some(e) => e,
                None => continue,
            };

            // Determine the language of this file
            let ext = entry.file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
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
                eprintln!("    Skipping {}: could not get analysis", entry.name);
                continue;
            };

            if log_level >= LogLevel::Debug {
                eprint!("    Exploring {}...", entry.name);
            }

            let explore_config = ExploreConfig {
                file: entry.file_path.to_string_lossy().into_owned(),
                max_iterations,
                seed: None,
                mocks: vec![],
                setup_file: None,
                setup_mode: shatter_core::config::SetupMode::PerFunction,
                value_sources: vec![],
                capabilities: shatter_core::orchestrator::FrontendCapabilities::default(),
                };

            match explorer::explore_function(frontend, &func_analysis, &explore_config).await {
                Ok(result) => {
                    if log_level >= LogLevel::Debug {
                        eprintln!(
                            " {} path(s), {}/{} lines",
                            result.unique_paths, result.lines_covered, result.total_lines
                        );
                    }
                    exploration_results.push((qualified_name.clone(), result));
                }
                Err(e) => {
                    if log_level >= LogLevel::Debug {
                        eprintln!(" error: {e}");
                    }
                }
            }

            // Check overall timeout
            if start.elapsed() > Duration::from_secs(timeout) {
                eprintln!("\nTimeout reached ({timeout}s), stopping exploration.");
                break;
            }
        }

        if start.elapsed() > Duration::from_secs(timeout) {
            break;
        }
    }

    eprintln!();

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
    );

    // Step 7: Write output files if requested
    if let Some(dir) = output_dir {
        write_run_report(dir, &call_graph, &exploration_results)?;
    }

    shutdown_all_frontends(frontends).await;
    Ok(())
}

/// Print a markdown-style summary report to stdout.
#[allow(clippy::too_many_arguments)]
fn print_summary_report(
    root: &Path,
    ts_files: &[PathBuf],
    go_files: &[PathBuf],
    rs_files: &[PathBuf],
    registry: &FunctionRegistry,
    call_graph: &CallGraph,
    layers: &[Vec<String>],
    cycles: &[Vec<String>],
    exploration_results: &[(String, explorer::ExplorationResult)],
    elapsed: Duration,
) {
    println!("# Shatter Run Report");
    println!();
    println!("**Repository**: {}", root.display());
    println!("**Elapsed**: {:.1}s", elapsed.as_secs_f64());
    println!();

    // Files discovered
    let total_files = ts_files.len() + go_files.len() + rs_files.len();
    println!("## Files Discovered");
    println!();
    println!("| Language | Files |");
    println!("|----------|-------|");
    if !ts_files.is_empty() {
        println!("| TypeScript | {} |", ts_files.len());
    }
    if !go_files.is_empty() {
        println!("| Go | {} |", go_files.len());
    }
    if !rs_files.is_empty() {
        println!("| Rust | {} |", rs_files.len());
    }
    println!("| **Total** | **{total_files}** |");
    println!();

    // Functions analyzed
    let total_branches: usize = registry.entries().iter().map(|e| e.branch_count).sum();
    println!("## Functions Analyzed");
    println!();
    println!("- **Total functions**: {}", registry.len());
    println!("- **Total branches**: {total_branches}");
    println!("- **Exported functions**: {}", registry.exported_functions().len());
    println!();

    // Call graph summary
    println!("## Call Graph");
    println!();
    println!("- **Nodes**: {}", call_graph.node_count());
    println!("- **Edges**: {}", call_graph.edge_count());
    println!("- **Topological layers**: {}", layers.len());
    println!("- **Cycles**: {}", cycles.len());
    if !cycles.is_empty() {
        println!();
        for (i, cycle) in cycles.iter().enumerate() {
            println!("  Cycle {}: {}", i + 1, cycle.join(" <-> "));
        }
    }
    println!();

    // Exploration results
    if !exploration_results.is_empty() {
        println!("## Exploration Results");
        println!();
        println!("| Function | Paths | Lines Covered | Coverage |");
        println!("|----------|-------|---------------|----------|");

        let mut total_paths = 0;
        let mut total_covered = 0;
        let mut total_lines = 0u32;

        for (qname, result) in exploration_results {
            let pct = if result.total_lines > 0 {
                (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            println!(
                "| {qname} | {} | {}/{} | {pct:.0}% |",
                result.unique_paths, result.lines_covered, result.total_lines
            );
            total_paths += result.unique_paths;
            total_covered += result.lines_covered;
            total_lines += result.total_lines;
        }

        let total_pct = if total_lines > 0 {
            (total_covered as f64 / total_lines as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        println!(
            "| **Total** | **{total_paths}** | **{total_covered}/{total_lines}** | **{total_pct:.0}%** |",
        );
        println!();
    }
}

/// Write analysis-only report to output directory.
fn write_analysis_report(
    dir: &Path,
    registry: &FunctionRegistry,
    call_graph: &CallGraph,
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
            entry.file_path.display(),
            entry.branch_count,
            entry.dependencies.len(),
        ));
    }

    std::fs::write(&summary_path, &content)
        .map_err(|e| format!("failed to write summary: {e}"))?;
    eprintln!("Wrote analysis report to {}", summary_path.display());

    Ok(())
}

/// Write full run report with per-function files to output directory.
fn write_run_report(
    dir: &Path,
    call_graph: &CallGraph,
    exploration_results: &[(String, explorer::ExplorationResult)],
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

    eprintln!(
        "Wrote {} per-function report(s) to {}",
        exploration_results.len(),
        dir.display()
    );

    Ok(())
}

/// Shutdown all frontends in a map.
async fn shutdown_all_frontends(frontends: HashMap<DiscoveryLanguage, Frontend>) {
    for (_, frontend) in frontends {
        if let Err(e) = frontend.shutdown().await {
            eprintln!("  Warning: frontend shutdown error: {e}");
        }
    }
}

async fn shutdown_frontend(frontend: Frontend) {
    if let Err(e) = frontend.shutdown().await {
        eprintln!("  Warning: frontend shutdown error: {e}");
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let log_level = cli.effective_log_level();
    let colors = Colors::detect();

    let result = match cli.command {
        CliCommand::Explore {
            targets,
            max_iterations,
            timeout,
            scope,
            analyze_only,
            show_clusters,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            inputs,
            config_path,
            output,
            spec,
            spec_json,
            invariants,
            no_boundary_values: _,
        } => {
            run_explore(
                &targets,
                max_iterations,
                timeout,
                scope.as_deref(),
                analyze_only,
                show_clusters,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                inputs.as_deref(),
                config_path.as_deref(),
                output.as_deref(),
                log_level,
                cli.perf,
                &colors,
                spec || spec_json || output.is_some() || invariants,
                spec_json || output.is_some(),
                invariants,
            )
            .await
        }
        CliCommand::Scan {
            directory,
            language,
            include,
            exclude,
            all,
            max_depth,
            timeout_per_fn,
            timeout_total,
            parallelism,
            mock_config,
            output,
            format,
            emit_tests,
            dry_run,
            resume,
            progress,
            core_sample,
            seed,
            max_iterations,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            stratum,
        } => {
            run_scan(
                &directory,
                language.as_deref(),
                &include,
                &exclude,
                all,
                max_depth,
                max_iterations,
                timeout_total,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                parallelism,
                timeout_per_fn,
                output.as_deref(),
                &format,
                progress,
                emit_tests.as_deref(),
                dry_run,
                resume.as_deref(),
                mock_config.as_deref(),
                core_sample.as_deref(),
                seed,
                stratum.as_deref(),
                log_level,
            )
            .await
        }
        CliCommand::ExportTests {
            targets,
            framework,
            module_path,
            output,
            max_iterations,
            timeout,
            scope,
            request_timeout,
            exec_timeout,
            build_timeout,
        } => {
            run_export_tests(
                &targets,
                &framework,
                &module_path,
                output.as_deref(),
                max_iterations,
                timeout,
                scope.as_deref(),
                request_timeout,
                exec_timeout,
                build_timeout,
                log_level,
            )
            .await
        }
        CliCommand::Run {
            path,
            output_dir,
            max_iterations,
            timeout,
            analyze_only,
            request_timeout,
            exec_timeout,
            build_timeout,
        } => {
            run_run(
                &path,
                output_dir.as_deref(),
                max_iterations,
                timeout,
                analyze_only,
                request_timeout,
                exec_timeout,
                build_timeout,
                log_level,
            )
            .await
        }
        CliCommand::Diff {
            snapshot,
            current,
            json,
        } => {
            match run_diff(&snapshot, &current, json) {
                Ok(has_regressions) => {
                    return if has_regressions {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    };
                }
                Err(e) => Err(e),
            }
        }
        CliCommand::SpecDiff { old, new, json } => {
            match run_spec_diff(&old, &new, json) {
                Ok(has_regressions) => {
                    return if has_regressions {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    };
                }
                Err(e) => Err(e),
            }
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parse_target_file_and_function() {
        let target = parse_target("src/app.ts:processOrder").unwrap();
        assert_eq!(target.file, PathBuf::from("src/app.ts"));
        assert_eq!(target.function.as_deref(), Some("processOrder"));
        assert_eq!(target.language, Language::TypeScript);
    }

    #[test]
    fn parse_target_file_only() {
        let target = parse_target("src/app.ts").unwrap();
        assert_eq!(target.file, PathBuf::from("src/app.ts"));
        assert!(target.function.is_none());
        assert_eq!(target.language, Language::TypeScript);
    }

    #[test]
    fn parse_target_go_file() {
        let target = parse_target("pkg/math.go:Add").unwrap();
        assert_eq!(target.file, PathBuf::from("pkg/math.go"));
        assert_eq!(target.function.as_deref(), Some("Add"));
        assert_eq!(target.language, Language::Go);
    }

    #[test]
    fn parse_target_unsupported_extension() {
        let err = parse_target("main.py:foo").unwrap_err();
        assert!(err.contains("unsupported file extension"));
    }

    #[test]
    fn parse_target_no_extension() {
        let err = parse_target("Makefile").unwrap_err();
        assert!(err.contains("no file extension"));
    }

    #[test]
    fn parse_target_path_with_colons_uses_last_colon() {
        // Windows-style paths or weird filenames: rsplit_once picks the last colon
        let target = parse_target("examples/typescript/src/01-arithmetic.ts:classifyNumber").unwrap();
        assert_eq!(target.file, PathBuf::from("examples/typescript/src/01-arithmetic.ts"));
        assert_eq!(target.function.as_deref(), Some("classifyNumber"));
    }

    #[test]
    fn language_from_extension_recognizes_tsx() {
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
    }

    #[test]
    fn language_labels_are_correct() {
        assert_eq!(Language::TypeScript.label(), "typescript");
        assert_eq!(Language::Go.label(), "go");
    }

    #[test]
    fn cli_parses_explore_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                targets,
                max_iterations,
                timeout,
                scope,
                analyze_only,
                show_clusters,
                cache_dir,
                no_cache,
                request_timeout,
                inputs,
                config_path,
                ..
            } => {
                assert_eq!(targets, vec!["test.ts:myFunc"]);
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 60);
                assert!(scope.is_none());
                assert!(!analyze_only);
                assert!(!show_clusters);
                assert!(cache_dir.is_none());
                assert!(!no_cache);
                assert_eq!(request_timeout, 30);
                assert!(inputs.is_none());
                assert!(config_path.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_scope_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--scope", "shatter.scope.yaml",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { scope, .. } => {
                assert_eq!(scope, Some(PathBuf::from("shatter.scope.yaml")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--max-iterations", "50",
            "--timeout", "120",
            "--analyze-only",
            "a.ts:fn1",
            "b.go:Fn2",
        ]);
        match cli.command {
            CliCommand::Explore {
                targets,
                max_iterations,
                timeout,
                scope,
                analyze_only,
                show_clusters,
                cache_dir,
                no_cache,
                request_timeout,
                inputs,
                config_path,
                ..
            } => {
                assert_eq!(targets, vec!["a.ts:fn1", "b.go:Fn2"]);
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 120);
                assert!(scope.is_none());
                assert!(analyze_only);
                assert!(!show_clusters);
                assert!(cache_dir.is_none());
                assert!(!no_cache);
                assert_eq!(request_timeout, 30);
                assert!(inputs.is_none());
                assert!(config_path.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_cache_dir_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--cache-dir", "/tmp/foo",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { cache_dir, .. } => {
                assert_eq!(cache_dir, Some(PathBuf::from("/tmp/foo")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_cache_dir_defaults_to_none() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { cache_dir, .. } => {
                assert!(cache_dir.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_request_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--request-timeout", "10",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { request_timeout, timeout, .. } => {
                assert_eq!(request_timeout, 10);
                assert_eq!(timeout, 60);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_inputs_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--inputs", "candidates.json",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { inputs, config_path, .. } => {
                assert_eq!(inputs, Some(PathBuf::from("candidates.json")));
                assert!(config_path.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_config_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--config", ".shatter/config.yaml",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { inputs, config_path, .. } => {
                assert!(inputs.is_none());
                assert_eq!(config_path, Some(PathBuf::from(".shatter/config.yaml")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_request_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--request-timeout", "15",
            "--timeout-total", "200",
            "test_dir",
        ]);
        match cli.command {
            CliCommand::Scan { request_timeout, timeout_total, .. } => {
                assert_eq!(request_timeout, 15);
                assert_eq!(timeout_total, 200);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_export_tests_with_request_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "export-tests",
            "--request-timeout", "5",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::ExportTests { request_timeout, .. } => {
                assert_eq!(request_timeout, 5);
            }
            _ => panic!("expected ExportTests command"),
        }
    }

    #[test]
    fn cli_parses_run_with_request_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "run",
            "--request-timeout", "45",
            "/tmp/repo",
        ]);
        match cli.command {
            CliCommand::Run { request_timeout, timeout, .. } => {
                assert_eq!(request_timeout, 45);
                assert_eq!(timeout, 300);
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_exec_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--exec-timeout", "20",
            "--build-timeout", "45",
            "test.go:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { exec_timeout, build_timeout, .. } => {
                assert_eq!(exec_timeout, 20);
                assert_eq!(build_timeout, 45);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_explore_exec_timeout_defaults() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.go:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { exec_timeout, build_timeout, .. } => {
                assert_eq!(exec_timeout, 10);
                assert_eq!(build_timeout, 30);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_exec_timeout() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--exec-timeout", "15",
            "test_dir",
        ]);
        match cli.command {
            CliCommand::Scan { exec_timeout, build_timeout, .. } => {
                assert_eq!(exec_timeout, 15);
                assert_eq!(build_timeout, 30);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn frontend_config_passes_timeout_env_vars() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_frontend_env(&mut config, LogLevel::Info, 20, 45);
        let env_map: std::collections::HashMap<_, _> = config.env_vars.iter().cloned().collect();
        assert_eq!(env_map.get("SHATTER_EXEC_TIMEOUT").map(|s| s.as_str()), Some("20"));
        assert_eq!(env_map.get("SHATTER_BUILD_TIMEOUT").map(|s| s.as_str()), Some("45"));
    }

    #[test]
    fn cli_explore_requires_at_least_one_target() {
        let result = Cli::try_parse_from(["shatter", "explore"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn cli_requires_subcommand() {
        let result = Cli::try_parse_from(["shatter"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parses_scan_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                directory,
                max_iterations,
                timeout_total,
                no_cache,
                request_timeout,
                parallelism,
                timeout_per_fn,
                dry_run,
                progress,
                ..
            } => {
                assert_eq!(directory, "src/");
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout_total, 300);
                assert!(!no_cache);
                assert_eq!(request_timeout, 30);
                assert_eq!(parallelism, 0);
                assert_eq!(timeout_per_fn, 30);
                assert!(!dry_run);
                assert!(!progress);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--max-iterations", "50",
            "--timeout-total", "600",
            "--dry-run",
            "--language", "typescript",
            "--include", "**/*.ts",
            "--exclude", "**/vendor/**",
            "--all",
            "--max-depth", "3",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                directory,
                max_iterations,
                timeout_total,
                dry_run,
                language,
                include,
                exclude,
                all,
                max_depth,
                no_cache,
                ..
            } => {
                assert_eq!(directory, "src/");
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout_total, 600);
                assert!(dry_run);
                assert_eq!(language, Some("typescript".to_string()));
                assert_eq!(include, vec!["**/*.ts"]);
                assert_eq!(exclude, vec!["**/vendor/**"]);
                assert!(all);
                assert_eq!(max_depth, Some(3));
                assert!(!no_cache);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_output_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--output", "/tmp/report",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { output, .. } => {
                assert_eq!(output, Some(PathBuf::from("/tmp/report")));
            }
            _ => panic!("expected Scan command"),
        }

        // Default: no output
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan { output, .. } => {
                assert!(output.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_requires_directory() {
        let result = Cli::try_parse_from(["shatter", "scan"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn frontend_config_typescript_uses_embedded_bundle() {
        let config = frontend_config(Language::TypeScript, Duration::from_secs(30), LogLevel::Info, 10, 30).unwrap();
        assert_eq!(config.command, PathBuf::from("node"));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        // The arg should point to the extracted bundle, not a relative dev path
        assert_eq!(config.args.len(), 1);
        assert!(
            config.args[0].contains("frontend-"),
            "expected embedded bundle path, got: {}",
            config.args[0]
        );
    }

    #[test]
    fn frontend_config_go_uses_embedded_binary() {
        let config = frontend_config(Language::Go, Duration::from_secs(45), LogLevel::Info, 10, 30).unwrap();
        assert_eq!(config.request_timeout, Duration::from_secs(45));
        assert!(config.args.is_empty());
        // The command should point to the extracted binary, not a relative dev path
        let cmd_str = config.command.to_string_lossy();
        assert!(
            cmd_str.contains("go-frontend-"),
            "expected embedded binary path, got: {cmd_str}",
        );
    }

    #[test]
    fn cli_parses_explore_with_no_cache() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--no-cache",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { no_cache, .. } => {
                assert!(no_cache);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_no_cache() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--no-cache",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { no_cache, .. } => {
                assert!(no_cache);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_no_cache_defaults_to_false_for_explore() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { no_cache, .. } => {
                assert!(!no_cache);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_no_cache_defaults_to_false_for_scan() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { no_cache, .. } => {
                assert!(!no_cache);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_export_tests_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "export-tests",
            "--framework", "gotest",
            "--module-path", "examples",
            "test.go:Add",
        ]);
        match cli.command {
            CliCommand::ExportTests {
                targets,
                framework,
                module_path,
                output,
                max_iterations,
                timeout,
                scope,
                request_timeout,
                ..
            } => {
                assert_eq!(targets, vec!["test.go:Add"]);
                assert_eq!(framework, "gotest");
                assert_eq!(module_path, "examples");
                assert!(output.is_none());
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 60);
                assert!(scope.is_none());
                assert_eq!(request_timeout, 30);
            }
            _ => panic!("expected ExportTests command"),
        }
    }

    #[test]
    fn cli_export_tests_defaults() {
        let cli = Cli::parse_from([
            "shatter",
            "export-tests",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::ExportTests {
                framework,
                module_path,
                output,
                ..
            } => {
                assert_eq!(framework, "jest");
                assert_eq!(module_path, ".");
                assert!(output.is_none());
            }
            _ => panic!("expected ExportTests command"),
        }
    }

    #[test]
    fn cli_export_tests_with_output_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "export-tests",
            "--output", "tests/generated_test.go",
            "--framework", "gotest",
            "test.go:Add",
        ]);
        match cli.command {
            CliCommand::ExportTests { output, .. } => {
                assert_eq!(output, Some(PathBuf::from("tests/generated_test.go")));
            }
            _ => panic!("expected ExportTests command"),
        }
    }

    #[test]
    fn cli_export_tests_requires_at_least_one_target() {
        let result = Cli::try_parse_from(["shatter", "export-tests"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn cli_parses_diff_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "diff",
            "snapshots/old.json",
            "snapshots/new.json",
        ]);
        match cli.command {
            CliCommand::Diff {
                snapshot,
                current,
                json,
            } => {
                assert_eq!(snapshot, PathBuf::from("snapshots/old.json"));
                assert_eq!(current, PathBuf::from("snapshots/new.json"));
                assert!(!json);
            }
            _ => panic!("expected Diff command"),
        }
    }

    #[test]
    fn cli_parses_diff_with_json_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "diff",
            "--json",
            "old.json",
            "new.json",
        ]);
        match cli.command {
            CliCommand::Diff { json, .. } => {
                assert!(json);
            }
            _ => panic!("expected Diff command"),
        }
    }

    #[test]
    fn cli_diff_requires_both_arguments() {
        let result = Cli::try_parse_from(["shatter", "diff"]);
        assert!(result.is_err());

        let result = Cli::try_parse_from(["shatter", "diff", "only-one.json"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parses_spec_diff_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "spec-diff",
            "specs/old.json",
            "specs/new.json",
        ]);
        match cli.command {
            CliCommand::SpecDiff { old, new, json } => {
                assert_eq!(old, PathBuf::from("specs/old.json"));
                assert_eq!(new, PathBuf::from("specs/new.json"));
                assert!(!json);
            }
            _ => panic!("expected SpecDiff command"),
        }
    }

    #[test]
    fn cli_parses_spec_diff_with_json_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "spec-diff",
            "--json",
            "specs/old.json",
            "specs/new.json",
        ]);
        match cli.command {
            CliCommand::SpecDiff { json, .. } => {
                assert!(json);
            }
            _ => panic!("expected SpecDiff command"),
        }
    }

    #[test]
    fn cli_spec_diff_requires_both_arguments() {
        let result = Cli::try_parse_from(["shatter", "spec-diff"]);
        assert!(result.is_err());

        let result = Cli::try_parse_from(["shatter", "spec-diff", "only-one.json"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parses_run_subcommand() {
        let cli = Cli::parse_from([
            "shatter",
            "run",
            "/tmp/my-repo",
        ]);
        match cli.command {
            CliCommand::Run {
                path,
                output_dir,
                max_iterations,
                timeout,
                analyze_only,
                request_timeout,
                ..
            } => {
                assert_eq!(path, "/tmp/my-repo");
                assert!(output_dir.is_none());
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 300);
                assert!(!analyze_only);
                assert_eq!(request_timeout, 30);
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn cli_parses_run_with_all_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "run",
            "--output-dir", "/tmp/output",
            "--max-iterations", "25",
            "--timeout", "120",
            "--analyze-only",
            ".",
        ]);
        match cli.command {
            CliCommand::Run {
                path,
                output_dir,
                max_iterations,
                timeout,
                analyze_only,
                request_timeout,
                ..
            } => {
                assert_eq!(path, ".");
                assert_eq!(output_dir, Some(PathBuf::from("/tmp/output")));
                assert_eq!(max_iterations, 25);
                assert_eq!(timeout, 120);
                assert!(analyze_only);
                assert_eq!(request_timeout, 30);
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn cli_run_requires_path_argument() {
        let result = Cli::try_parse_from(["shatter", "run"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn discovery_lang_to_cli_lang_maps_correctly() {
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::TypeScript),
            Some(Language::TypeScript)
        );
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::Go),
            Some(Language::Go)
        );
        assert_eq!(
            discovery_lang_to_cli_lang(DiscoveryLanguage::Rust),
            None
        );
    }

    #[test]
    fn cli_scan_emit_tests_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--emit-tests", "jest",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { emit_tests, .. } => {
                assert_eq!(emit_tests, Some("jest".to_string()));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_emit_tests_gotest() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--emit-tests", "gotest",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { emit_tests, .. } => {
                assert_eq!(emit_tests, Some("gotest".to_string()));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_emit_tests_defaults_to_none() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan { emit_tests, .. } => {
                assert!(emit_tests.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_new_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--progress",
            "--format", "markdown",
            "--resume", "/tmp/state.json",
            "--mock-config", "/tmp/mocks.yaml",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                progress,
                format,
                resume,
                mock_config,
                ..
            } => {
                assert!(progress);
                assert_eq!(format, "markdown");
                assert_eq!(resume, Some(PathBuf::from("/tmp/state.json")));
                assert_eq!(mock_config, Some(PathBuf::from("/tmp/mocks.yaml")));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_core_sample() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--core-sample", "50%",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { core_sample, seed, .. } => {
                assert_eq!(core_sample, Some("50%".to_string()));
                assert!(seed.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_core_sample_absolute() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--core-sample", "20",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { core_sample, .. } => {
                assert_eq!(core_sample, Some("20".to_string()));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_seed() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--core-sample", "50%",
            "--seed", "12345",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { core_sample, seed, .. } => {
                assert_eq!(core_sample, Some("50%".to_string()));
                assert_eq!(seed, Some(12345));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_core_sample_defaults_to_none() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan { core_sample, seed, .. } => {
                assert!(core_sample.is_none());
                assert!(seed.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_output_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--output", "spec.json",
            "src/app.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore { output, .. } => {
                assert_eq!(output, Some(PathBuf::from("spec.json")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_output_defaults_to_none() {
        let cli = Cli::parse_from(["shatter", "explore", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore { output, .. } => {
                assert!(output.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }
}
