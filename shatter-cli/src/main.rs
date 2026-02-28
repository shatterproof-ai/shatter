use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

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

        /// Output a behavioral specification (markdown by default, JSON with --spec-json).
        #[arg(long)]
        spec: bool,

        /// Output the behavioral specification as JSON instead of markdown.
        #[arg(long)]
        spec_json: bool,
    },

    /// Scan multiple functions in dependency order, using behavior maps as mocks.
    Scan {
        /// Targets to scan, in <file>:<function> format or just <file> for all functions.
        #[arg(required = true)]
        targets: Vec<String>,

        /// Maximum number of iterations per function.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Timeout in seconds for the entire scan.
        #[arg(long, default_value_t = 120)]
        timeout: u64,

        /// Path to a scope configuration YAML file.
        #[arg(long)]
        scope: Option<PathBuf>,

        /// Only run the analyze phase (skip exploration).
        #[arg(long)]
        analyze_only: bool,

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

        /// Number of parallel frontend subprocesses for exploration.
        /// Default: number of available CPUs (0 = auto-detect).
        #[arg(long, default_value_t = 0)]
        parallelism: usize,

        /// Per-function exploration timeout in seconds. Functions exceeding this
        /// limit are skipped without aborting the scan. Default: 30s.
        #[arg(long, default_value_t = 30)]
        timeout_per_fn: u64,

        /// Write JSON report to this directory (default: ./shatter-report/).
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },

    /// Export generated tests from behavior maps produced by exploration.
    ///
    /// Runs exploration on the given targets, then generates test files in the
    /// specified framework format.
    ExportTests {
        /// Targets to explore and export tests for, in <file>:<function> format.
        #[arg(required = true)]
        targets: Vec<String>,

        /// Test framework to generate: jest or gotest.
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
    Ok(config)
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
    log_level: LogLevel,
    show_perf: bool,
    colors: &Colors,
    show_spec: bool,
    spec_as_json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            if log_level >= LogLevel::Info {
                println!("Loaded scope config from {}", path.display());
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

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target
            .function
            .as_deref()
            .unwrap_or("(all)");

        if log_level >= LogLevel::Debug {
            println!(
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
            println!(
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
                    println!("  Found {} function(s):", functions.len());
                    for func in functions {
                        println!("    - {} ({} params, {} branches)",
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

        // Load .shatter/ config for this target
        let shatter_configs: Vec<ShatterConfig> = if let Some(cp) = config_path {
            // Explicit config bypasses discovery
            let cfg = shatter_config::parse_config(cp)
                .map_err(|e| format!("failed to load config: {e}"))?;
            if log_level >= LogLevel::Debug {
                println!("[debug] Loaded config from {}", cp.display());
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
                    println!("\n  [debug] Skipping {} (skip=true in config)", func.name);
                }
                continue;
            }

            // Check for unexecutable parameter types (opaque types like net.Socket).
            let skip_reasons = executability::check_executability(&func.params, &[]);
            if !skip_reasons.is_empty() {
                if log_level >= LogLevel::Debug {
                    println!("\n  [debug] Skipping {} (unexecutable parameter types)", func.name);
                }
                skipped_unexecutable.push((func.name.clone(), skip_reasons));
                continue;
            }

            let explore_config = ExploreConfig {
                file: file_str.to_string(),
                max_iterations: resolved.max_iterations,
                seed: None,
                mocks: vec![],
            };

            // Convert candidate inputs for logging
            if log_level >= LogLevel::Debug {
                if !resolved.candidate_inputs.is_empty() {
                    println!(
                        "\n  [debug] Exploring {} ({} candidate input(s) from config)...",
                        func.name,
                        resolved.candidate_inputs.len()
                    );
                } else {
                    println!("\n  [debug] Exploring {}...", func.name);
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
                            print!("{}", explorer::format_exploration_report_verbose(&result));
                        } else {
                            let report_opts = ReportOptions {
                                location: Some(format!("{file_str}:{}", func.start_line)),
                                show_perf,
                                wall_time: Some(wall_time),
                                coverage_metrics: None,
                            };
                            print!("{}", explorer::format_exploration_report(&result, &report_opts));
                        }
                        println!();
                    }

                    // Spec output: build equivalence classes and spec
                    if show_spec {
                        let eq_classes =
                            shatter_core::equivalence::group_into_classes(&result.raw_results);
                        let location = Some(format!("{file_str}:{}", func.start_line));
                        let spec = shatter_core::spec::build_spec(&result, &eq_classes, location);
                        if spec_as_json {
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
            println!(
                "Skipped {} function(s) (unexecutable parameter types):",
                skipped_unexecutable.len()
            );
            for (name, reasons) in &skipped_unexecutable {
                for reason in reasons {
                    println!(
                        "  {name}: param {:?} has opaque type {}",
                        reason.param_name, reason.opaque_label
                    );
                }
            }
        }

        shutdown_frontend(frontend).await;
    }

    Ok(())
}

/// Run the scan command: explore multiple functions in dependency order.
#[allow(clippy::too_many_arguments)]
async fn run_scan(
    targets: &[String],
    max_iterations: u32,
    _timeout: u64,
    scope_path: Option<&Path>,
    analyze_only: bool,
    cache_dir: Option<&Path>,
    no_cache: bool,
    request_timeout: u64,
    exec_timeout: u64,
    build_timeout: u64,
    parallelism: usize,
    timeout_per_fn: u64,
    output_dir: Option<&Path>,
    log_level: LogLevel,
) -> Result<(), Box<dyn std::error::Error>> {
    let _scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            println!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;

    // Group targets by language (scan operates on one frontend at a time).
    let first_lang = parsed.first().map(|t| t.language).ok_or("no targets")?;
    if parsed.iter().any(|t| t.language != first_lang) {
        return Err("scan currently requires all targets to use the same language frontend".into());
    }

    let req_timeout = Duration::from_secs(request_timeout);
    let fe_config = frontend_config(first_lang, req_timeout, log_level, exec_timeout, build_timeout)?;
    let mut frontend = Frontend::spawn(&fe_config).await.map_err(|e| {
        format!(
            "failed to spawn {} frontend: {e}",
            first_lang.label()
        )
    })?;

    if log_level >= LogLevel::Debug {
        println!(
            "[debug] Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );
    }

    // Analyze all targets to collect FunctionAnalysis data.
    let mut all_analyses = Vec::new();
    let mut file_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        if log_level >= LogLevel::Debug {
            println!(
                "[debug] Analyzing {file_str}:{}",
                target.function.as_deref().unwrap_or("(all)")
            );
        }

        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
            })
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match analyze_response.result {
            ResponseResult::Analyze { functions } => {
                if log_level >= LogLevel::Debug {
                    println!("  Found {} function(s):", functions.len());
                    for func in &functions {
                        println!(
                            "    - {} ({} params, {} branches, {} deps)",
                            func.name,
                            func.params.len(),
                            func.branches.len(),
                            func.dependencies.len(),
                        );
                    }
                }
                for func in &functions {
                    file_map.insert(func.name.clone(), file_str.to_string());
                }
                all_analyses.extend(functions);
            }
            ResponseResult::Error { code, message, .. } => {
                eprintln!("  Analyze error ({code:?}): {message}");
            }
            other => {
                eprintln!("  Unexpected analyze response: {other:?}");
            }
        }
    }

    if analyze_only {
        shutdown_frontend(frontend).await;
        return Ok(());
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
        println!(
            "Skipped {} function(s) (unexecutable parameter types):",
            skipped_for_executability.len()
        );
        for skip in &skipped_for_executability {
            println!("  {}: {}", skip.function_name, skip.reason);
        }
    }

    if all_analyses.is_empty() {
        eprintln!("No functions found to scan.");
        shutdown_frontend(frontend).await;
        return Ok(());
    }

    // Shut down the analysis frontend before starting parallel exploration.
    shutdown_frontend(frontend).await;

    // Resolve effective parallelism: 0 means auto-detect (CPU count).
    let effective_parallelism = if parallelism == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        parallelism
    };

    if log_level >= LogLevel::Debug {
        println!(
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

    let scan_config = ScanConfig {
        max_iterations_per_function: max_iterations,
        seed: None,
        file_map,
        parallelism: effective_parallelism,
        timeout_per_fn: Duration::from_secs(timeout_per_fn),
        cache,
    };

    match scan_orchestrator::parallel_scan(&fe_config, &all_analyses, &scan_config).await {
        Ok(result) => {
            print!("{}", scan_orchestrator::format_parallel_scan_report(&result));

            // Generate and write JSON report if output_dir is specified.
            let report_dir = output_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./shatter-report/"));
            let json_report = report::generate_report(&result, &scan_config.file_map);
            match report::write_report(&json_report, &report_dir) {
                Ok(path) => {
                    println!("Wrote JSON report to {}", path.display());
                }
                Err(e) => {
                    eprintln!("Failed to write JSON report: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("Scan error: {e}");
        }
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
    if framework != "jest" && framework != "gotest" {
        return Err(format!("unsupported framework '{framework}': expected 'jest' or 'gotest'").into());
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
        };

        for func in &functions {
            eprintln!("  Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config).await {
                Ok(result) => {
                    let behavior_map = BehaviorMap::from_exploration_result(&func.name, &result);

                    let test_code = match framework {
                        "jest" => export::generate_jest_tests(&behavior_map, &func.name, module_path),
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
        println!("Shatter run: {}", root.display());
        println!();
    }

    // Step 1: Discover files
    if log_level >= LogLevel::Debug {
        println!("Discovering source files...");
    }
    let options = DiscoveryOptions::default();
    let files = discovery::discover_files(&root, &options)
        .map_err(|e| format!("file discovery failed: {e}"))?;

    if files.is_empty() {
        println!("No supported source files found in {}", root.display());
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
        println!("  Found {} file(s):", files.len());
        if !ts_files.is_empty() {
            println!("    TypeScript: {}", ts_files.len());
        }
        if !go_files.is_empty() {
            println!("    Go: {}", go_files.len());
        }
        if !rs_files.is_empty() {
            println!("    Rust: {} (analysis not yet supported)", rs_files.len());
        }
        println!();
    }

    // Filter to languages we can actually analyze (TS, Go)
    let analyzable_files: Vec<(PathBuf, DiscoveryLanguage)> = files
        .into_iter()
        .filter(|(_, lang)| discovery_lang_to_cli_lang(*lang).is_some())
        .collect();

    if analyzable_files.is_empty() {
        println!("No analyzable source files found (only TypeScript and Go are supported).");
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
            println!(
                "Frontend connected (language={})",
                frontend.language().unwrap_or("unknown")
            );
        }
        frontends.insert(*lang, frontend);
    }

    // Step 3: Batch analyze
    if log_level >= LogLevel::Debug {
        println!();
        println!("Analyzing {} file(s)...", analyzable_files.len());
    }
    let registry = batch_analyze::batch_analyze(&mut frontends, &analyzable_files)
        .await
        .map_err(|e| format!("batch analyze failed: {e}"))?;

    let total_functions = registry.len();
    let total_branches: usize = registry.entries().iter().map(|e| e.branch_count).sum();

    if log_level >= LogLevel::Debug {
        println!("  Found {} function(s) with {} total branch(es)", total_functions, total_branches);
        println!();
    }

    if total_functions == 0 {
        println!("No functions found to explore.");
        shutdown_all_frontends(frontends).await;
        return Ok(());
    }

    // Step 4: Build call graph
    if log_level >= LogLevel::Debug {
        println!("Building call graph...");
    }
    let call_graph = CallGraph::from_registry(&registry);
    let layers = call_graph.topological_layers();
    let cycles = call_graph.cycle_groups();

    if log_level >= LogLevel::Debug {
        println!(
            "  {} node(s), {} edge(s), {} layer(s), {} cycle(s)",
            call_graph.node_count(),
            call_graph.edge_count(),
            layers.len(),
            cycles.len(),
        );
        println!();
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
        println!("Exploring functions in dependency order...");
        println!();
    }

    let mut exploration_results: Vec<(String, explorer::ExplorationResult)> = Vec::new();

    for (layer_idx, layer) in layers.iter().enumerate() {
        if log_level >= LogLevel::Debug {
            println!("  Layer {} ({} function(s)):", layer_idx, layer.len());
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
                print!("    Exploring {}...", entry.name);
            }

            let explore_config = ExploreConfig {
                file: entry.file_path.to_string_lossy().into_owned(),
                max_iterations,
                seed: None,
                mocks: vec![],
            };

            match explorer::explore_function(frontend, &func_analysis, &explore_config).await {
                Ok(result) => {
                    if log_level >= LogLevel::Debug {
                        println!(
                            " {} path(s), {}/{} lines",
                            result.unique_paths, result.lines_covered, result.total_lines
                        );
                    }
                    exploration_results.push((qualified_name.clone(), result));
                }
                Err(e) => {
                    if log_level >= LogLevel::Debug {
                        println!(" error: {e}");
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
    println!("Wrote analysis report to {}", summary_path.display());

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

    println!(
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
            spec,
            spec_json,
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
                log_level,
                cli.perf,
                &colors,
                spec || spec_json,
                spec_json,
            )
            .await
        }
        CliCommand::Scan {
            targets,
            max_iterations,
            timeout,
            scope,
            analyze_only,
            cache_dir,
            no_cache,
            request_timeout,
            exec_timeout,
            build_timeout,
            parallelism,
            timeout_per_fn,
            output_dir,
        } => {
            run_scan(
                &targets,
                max_iterations,
                timeout,
                scope.as_deref(),
                analyze_only,
                cache_dir.as_deref(),
                no_cache,
                request_timeout,
                exec_timeout,
                build_timeout,
                parallelism,
                timeout_per_fn,
                output_dir.as_deref(),
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
            "--timeout", "200",
            "test.ts",
        ]);
        match cli.command {
            CliCommand::Scan { request_timeout, timeout, .. } => {
                assert_eq!(request_timeout, 15);
                assert_eq!(timeout, 200);
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
            "test.go",
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
        let config = frontend_config(Language::Go, Duration::from_secs(30), LogLevel::Info, 20, 45).unwrap();
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
            "test.ts",
        ]);
        match cli.command {
            CliCommand::Scan {
                targets,
                max_iterations,
                timeout,
                scope,
                analyze_only,
                no_cache,
                request_timeout,
                parallelism,
                timeout_per_fn,
                ..
            } => {
                assert_eq!(targets, vec!["test.ts"]);
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 120);
                assert!(scope.is_none());
                assert!(!analyze_only);
                assert!(!no_cache);
                assert_eq!(request_timeout, 30);
                assert_eq!(parallelism, 0);
                assert_eq!(timeout_per_fn, 30);
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
            "--timeout", "300",
            "--analyze-only",
            "a.ts",
            "b.ts:helperFn",
        ]);
        match cli.command {
            CliCommand::Scan {
                targets,
                max_iterations,
                timeout,
                analyze_only,
                no_cache,
                ..
            } => {
                assert_eq!(targets, vec!["a.ts", "b.ts:helperFn"]);
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 300);
                assert!(analyze_only);
                assert!(!no_cache);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_output_dir_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--output-dir", "/tmp/report",
            "test.ts",
        ]);
        match cli.command {
            CliCommand::Scan { output_dir, .. } => {
                assert_eq!(output_dir, Some(PathBuf::from("/tmp/report")));
            }
            _ => panic!("expected Scan command"),
        }

        // Default: no output_dir
        let cli = Cli::parse_from(["shatter", "scan", "test.ts"]);
        match cli.command {
            CliCommand::Scan { output_dir, .. } => {
                assert!(output_dir.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_requires_at_least_one_target() {
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
            "test.ts",
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
            "test.ts",
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
}
