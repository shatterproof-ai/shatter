use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use shatter_core::explorer;
use shatter_core::log_level::LogLevel;
use shatter_core::timing::{TimingConfig, TimingFormat, TimingMode, TimingOutput};

/// Execution isolation level for `--isolation`.
///
/// Controls how function executions are isolated from each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum IsolationModeArg {
    /// No isolation (default). All executions share a single frontend process.
    /// Assumes functions are side-effect-safe and stateless.
    #[default]
    None,
    /// Each function invocation gets a fresh execution context (new process or sandbox).
    Function,
    /// Functions run sequentially (no parallelism), sharing a single process.
    Serial,
}

impl From<IsolationModeArg> for shatter_core::explorer::IsolationMode {
    fn from(value: IsolationModeArg) -> Self {
        match value {
            IsolationModeArg::None => Self::None,
            IsolationModeArg::Function => Self::Function,
            IsolationModeArg::Serial => Self::Serial,
        }
    }
}

/// Terminal output format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    /// Markdown rendered via termimad (default). Use `--color never` for raw Markdown.
    #[default]
    Md,
    /// Legacy plain ANSI text output (deprecated).
    Plain,
}

/// Output format for report files and stdout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum StdoutFormat {
    /// Markdown (default).
    #[default]
    Markdown,
    /// JSON.
    Json,
    /// Self-contained HTML.
    Html,
    /// Plain text (markdown with formatting stripped).
    Text,
}

/// Infer the output format from a file's extension.
///
/// Returns an error if the extension is unsupported or missing.
pub(crate) fn infer_output_format(path: &std::path::Path) -> Result<StdoutFormat, String> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => Ok(StdoutFormat::Html),
        Some("md") => Ok(StdoutFormat::Markdown),
        Some("json") => Ok(StdoutFormat::Json),
        Some("txt") => Ok(StdoutFormat::Text),
        Some(ext) => Err(format!(
            "unknown output format for extension '.{ext}' — supported: .html, .md, .json, .txt"
        )),
        None => Err(
            "output file has no extension — use .html, .md, .json, or .txt to specify format"
                .to_string(),
        ),
    }
}

/// Shatter: automatic exploratory testing via concolic execution.
#[derive(Parser, Debug)]
#[command(name = "shatter", version, about)]
pub(crate) struct Cli {
    /// Log verbosity level: error, warn, info (default), debug, trace.
    #[arg(long, global = true, default_value = "info")]
    pub(crate) log_level: LogLevel,

    /// Increase verbosity (-v = debug, -vv = trace).
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,

    /// Decrease verbosity to warnings and errors only.
    #[arg(short = 'q', long = "quiet", global = true)]
    pub(crate) quiet: bool,

    /// Timing output mode.
    #[arg(long, global = true, default_value = "off")]
    pub(crate) timing: TimingModeArg,

    /// Format for timing output. `json` and `both` are intended for persisted artifacts.
    #[arg(long, global = true, default_value = "text")]
    pub(crate) timing_format: TimingFormatArg,

    /// Write one timing artifact JSON file to this path.
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        conflicts_with = "timing_output_dir"
    )]
    pub(crate) timing_output: Option<PathBuf>,

    /// Write timing artifact JSON files into this directory.
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        conflicts_with = "timing_output"
    )]
    pub(crate) timing_output_dir: Option<PathBuf>,

    /// Override auto-detected project root directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub(crate) project_dir: Option<std::path::PathBuf>,

    /// Override config values using dotted-path key=value pairs (repeatable).
    ///
    /// Example: `--set defaults.max_iterations=200 --set defaults.exploration.adaptive=false`
    ///
    /// Keys follow the `.shatter/config.yaml` YAML structure. Values are parsed as YAML
    /// scalars (integers, floats, booleans, strings). Precedence: above any
    /// `.shatter/config.yaml` file but below dedicated flags like `--max-iterations`.
    #[arg(long = "set", global = true, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
    pub(crate) set_overrides: Vec<String>,

    /// When to use terminal colors: always, auto (default), or never.
    /// Respects the NO_COLOR environment variable (auto treats it as never).
    #[arg(long, global = true, default_value = "auto", value_name = "WHEN")]
    pub(crate) color: ColorMode,

    /// Terminal rendering mode: md (default, rendered via termimad) or plain (legacy ANSI).
    #[arg(
        long = "render",
        global = true,
        default_value = "md",
        value_name = "MODE"
    )]
    pub(crate) render: OutputFormat,

    #[command(subcommand)]
    pub(crate) command: CliCommand,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ColorMode {
    Always,
    Auto,
    Never,
}

impl ColorMode {
    pub(crate) fn use_color(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
            }
        }
    }
}

impl Cli {
    /// Resolve the effective log level from --log-level, -v, and -q flags.
    pub(crate) fn effective_log_level(&self) -> LogLevel {
        if self.quiet {
            return LogLevel::Warn;
        }
        match self.verbose {
            0 => self.log_level,
            1 => LogLevel::Debug,
            _ => LogLevel::Trace,
        }
    }

    pub(crate) fn timing_config(&self) -> Result<TimingConfig, String> {
        let output = match (&self.timing_output, &self.timing_output_dir) {
            (Some(path), None) => Some(TimingOutput::File { path: path.clone() }),
            (None, Some(path)) => Some(TimingOutput::Directory { path: path.clone() }),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("clap enforces conflicts"),
        };

        let mode: TimingMode = self.timing.into();
        if output.is_some() && matches!(mode, TimingMode::Off) {
            return Err("timing output requires --timing summary|detailed".to_string());
        }

        Ok(TimingConfig {
            mode,
            format: self.timing_format.into(),
            output,
        })
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum TimingModeArg {
    Off,
    Summary,
    Detailed,
}

impl From<TimingModeArg> for TimingMode {
    fn from(value: TimingModeArg) -> Self {
        match value {
            TimingModeArg::Off => TimingMode::Off,
            TimingModeArg::Summary => TimingMode::Summary,
            TimingModeArg::Detailed => TimingMode::Detailed,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum TimingFormatArg {
    Text,
    Json,
    Both,
}

impl From<TimingFormatArg> for TimingFormat {
    fn from(value: TimingFormatArg) -> Self {
        match value {
            TimingFormatArg::Text => TimingFormat::Text,
            TimingFormatArg::Json => TimingFormat::Json,
            TimingFormatArg::Both => TimingFormat::Both,
        }
    }
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)] // CLI command enums are parsed once, size is not a concern
pub(crate) enum CliCommand {
    /// Explore functions by analyzing their branches and generating test inputs.
    Explore {
        /// Targets to explore: <file>:<function> for a single function, or just
        /// <file> to explore all exported functions.
        /// The file extension determines the language frontend (.ts = TypeScript, .go = Go).
        #[arg(required = true)]
        targets: Vec<String>,

        /// Maximum number of iterations per function [default: 100].
        /// Pass 0 for unbounded exploration (run until timeout or interrupt).
        #[arg(long)]
        max_iterations: Option<u32>,

        /// Timeout in seconds for the entire exploration.
        #[arg(long)]
        timeout: Option<u64>,

        /// Per-function exploration wall-clock timeout in seconds. If both
        /// --max-iterations and --timeout-explore are set, whichever triggers
        /// first stops exploration for that function.
        #[arg(long)]
        timeout_explore: Option<f64>,

        /// Total wall-clock time limit in seconds for the entire explore run.
        /// Stops launching new functions once this limit is reached.
        /// Unlike --timeout-explore (per-function), this bounds the whole run.
        #[arg(long, value_name = "SECONDS")]
        time_limit: Option<f64>,

        /// Stop exploration when aggregate branch coverage reaches this
        /// percentage (0.0–100.0). Checked after each function completes.
        #[arg(long, value_name = "PERCENT")]
        coverage_threshold: Option<f64>,

        /// Maximum total execute calls across all functions. Unlike
        /// --max-iterations (per-function iteration cap), this is a global
        /// budget shared across the entire explore run.
        #[arg(long, value_name = "COUNT")]
        max_executions: Option<u64>,

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
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter-cache/behavior-maps/`.
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

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Path to a .shatter/config.yaml file (bypasses hierarchical discovery).
        #[arg(long = "config")]
        config_path: Option<PathBuf>,

        /// Write per-file spec JSON to a file (implies --spec-json).
        #[arg(long = "spec-out")]
        spec_out: Option<PathBuf>,

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

        /// Use the concolic (Z3-backed) explorer instead of the random explorer.
        #[arg(long)]
        concolic: bool,

        /// Enable the genetic algorithm explorer.
        #[arg(long)]
        genetic: bool,

        /// Population size for the genetic algorithm (default: 50).
        #[arg(long)]
        genetic_population: Option<u32>,

        /// Maximum generations for the genetic algorithm (default: 100).
        #[arg(long)]
        genetic_generations: Option<u32>,

        /// Timeout in seconds for the genetic algorithm (default: 300).
        #[arg(long)]
        genetic_timeout: Option<u32>,

        /// Disable adaptive strategy scoring (use round-robin instead).
        #[arg(long)]
        no_adaptive: bool,

        /// Sliding window size for strategy outcome scoring.
        #[arg(long)]
        score_window: Option<usize>,

        /// Minimum candidates before a strategy can be deprioritized.
        #[arg(long)]
        cold_start: Option<u64>,

        /// Minimum allocation fraction per strategy (0.0–1.0).
        #[arg(long)]
        strategy_floor: Option<f64>,

        /// Static strategy weight distribution (e.g. "literals=0.3,random=0.5,boundary=0.2").
        #[arg(long)]
        strategy_weights: Option<String>,

        /// Select a frontend-provided invocation planner by name. When set to
        /// `go`, consults the Go frontend's invocation planner
        /// (get_invocation_plan) before exploring each target. The returned
        /// InvocationPlan is fed as seeds and attached to every Execute request
        /// so method targets dispatch into a real constructor.
        #[arg(long)]
        planner: Option<String>,

        /// Z3 solver timeout in seconds per query. Default: no limit.
        #[arg(long)]
        solver_timeout: Option<u64>,

        /// Memory limit in MB for the frontend process. For TS, sets --max-old-space-size; for Go, sets GOMEMLIMIT.
        #[arg(long)]
        memory_limit: Option<u64>,

        /// Ignore existing spec file and force full re-exploration (no incremental reuse).
        #[arg(long)]
        clean: bool,

        /// Analyze and compare fingerprints, print stale/fresh/removed functions, then exit
        /// without exploring. Requires --output.
        #[arg(long)]
        dry_run: bool,

        /// Loop iteration bucket boundaries for path hashing (comma-separated).
        /// Controls how loop iteration counts affect path identity.
        /// Default "0,1,2,5" gives 5 levels: 0, 1, 2, 3–5, 6+ iterations.
        /// Use "none" to disable bucketing (only branch profiles matter).
        #[arg(long, default_value = "0,1,2,5")]
        loop_buckets: String,

        /// Directory for cross-function seed pool (default: .shatter/seeds/).
        /// Falls back to SHATTER_SEEDS_DIR env var.
        #[arg(long, default_value = ".shatter/seeds", env = "SHATTER_SEEDS_DIR")]
        seeds_dir: PathBuf,

        /// Disable loading and saving the cross-function seed pool.
        #[arg(long)]
        no_seeds: bool,

        /// Override all setup/teardown timeouts (seconds). Sets SHATTER_SETUP_TIMEOUT
        /// env var before spawning frontends.
        #[arg(long)]
        setup_timeout: Option<u64>,

        /// Treat setup failures as fatal errors (abort exploration immediately).
        #[arg(long)]
        fail_on_setup_error: bool,

        /// Record external dependency I/O (passthrough mode). Saves observed
        /// call data to shatter-artifacts/recorded-mocks/ as seed fixtures for future runs.
        #[arg(long)]
        record: bool,

        /// Write raw observation data (Stage 1 output) to a directory for offline
        /// analysis with `shatter analyze`. One JSON file per function.
        #[arg(long)]
        observe_output: Option<PathBuf>,

        /// Persist canonical stage JSON artifacts for each explored function.
        /// Writes `observe.json`, `analyze.json`, `solve.json`, and `specify.json`
        /// under a per-function directory so later CLI runs can reuse them.
        #[arg(long, value_name = "DIR")]
        persist_stages: Option<PathBuf>,

        /// Replay previously recorded mock fixtures from shatter-artifacts/recorded-mocks/.
        /// When set, auto-detects recorded mocks for each file+function pair and
        /// uses observed return values as seed mock configs.
        #[arg(long)]
        replay_recorded: bool,

        /// Disable auto-detection of recorded mocks (overrides --replay-recorded).
        #[arg(long)]
        no_replay: bool,

        /// Per-boundary refinement budget (number of executions). After discovery,
        /// binary-searches between witness pairs to find precise transition points.
        /// Set to 0 to disable. Default: 20.
        #[arg(long, default_value_t = 20)]
        refine_budget: usize,

        /// Cap total shrink attempts per witness. Set to 0 to disable. Default: 20.
        #[arg(long, default_value_t = 20)]
        shrink_budget: usize,

        /// Disable the shrink phase entirely (equivalent to --shrink-budget 0).
        #[arg(long)]
        no_shrink: bool,

        /// Enable MC/DC (Modified Condition/Decision Coverage) analysis.
        /// Decomposes compound boolean decisions into individual conditions
        /// and targets condition-independence witnesses. Implies increased
        /// iteration/execution/plateau budgets.
        #[arg(long)]
        mcdc: bool,

        /// Execution isolation level.
        ///
        /// - none     (default): executions share a single frontend process; assumes
        ///   functions are side-effect-safe and stateless.
        /// - function: each function invocation gets a fresh execution context.
        /// - serial:  functions run sequentially (no parallelism) in a shared process.
        #[arg(long, value_enum, default_value = "none")]
        isolation: IsolationModeArg,

        /// Enable rich side-effect capture during exploration.
        ///
        /// When set, the frontend records console output, file writes, network
        /// requests, environment reads, global mutations, and thrown errors for
        /// each execution. Disabled by default because capture adds overhead on
        /// every execute call; enable only when you need the side-effect data.
        #[arg(long, default_value_t = false)]
        capture_side_effects: bool,

        /// Write exploration report to file; format inferred from extension (.html, .md, .json, .txt).
        /// May be repeated to write multiple formats simultaneously.
        #[arg(long = "output", short = 'o', value_name = "PATH")]
        report_outputs: Vec<PathBuf>,

        /// Write report to stdout in addition to any -o files.
        /// When no -o flags are given, stdout is the default output.
        #[arg(long)]
        stdout: bool,

        /// Format for stdout output. One of: markdown (default), json, html, text.
        #[arg(long, default_value = "markdown")]
        format: StdoutFormat,

        /// Maximum number of parallel exploration workers.
        /// Each worker spawns its own frontend process.
        /// Default: number of available CPUs (0 = auto-detect).
        #[arg(long, short = 'w', default_value_t = 0, alias = "jobs")]
        workers: usize,

        /// Override the lower bound of the parallelism clamp (built-in
        /// default: 4). Useful on tiny CI runners. See str-v01r.
        #[arg(long)]
        parallelism_min: Option<usize>,

        /// Override the upper bound of the parallelism clamp (built-in
        /// default: 16). Useful on large dedicated machines with tuned
        /// `GOMAXPROCS`. See str-v01r.
        #[arg(long)]
        parallelism_max: Option<usize>,

        /// Finalize a previous explore run from saved artifacts on disk.
        /// Skips exploration and produces reports/specs from saved per-function
        /// result files. Use when a prior run wrote artifacts but crashed before
        /// final assembly.
        #[arg(long)]
        from_artifacts: Option<PathBuf>,
    },

    /// Analyze Stage 1 (Observe) output: produce equivalence classes, behavior map,
    /// coverage metrics, and optional behavioral specification. No frontend or solver
    /// required — pure offline computation on serialized observation data.
    Analyze {
        /// Path to a Stage 1 observation JSON file (produced by `shatter explore --observe-output`).
        #[arg(required = true)]
        input: PathBuf,

        /// Write Stage 2 analysis output to a JSON file (for downstream stages).
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Output a behavioral specification in markdown.
        #[arg(long)]
        spec: bool,

        /// Output the behavioral specification as JSON instead of markdown.
        #[arg(long)]
        spec_json: bool,

        /// Enable Daikon-style invariant detection.
        #[arg(long)]
        invariants: bool,
    },

    /// Run the observation stage: execute a function with generated inputs and write
    /// ObserveStageOutput JSON to a file or stdout. Use `shatter analyze` to process
    /// the output offline, or `shatter specify` to build a behavioral spec.
    Observe {
        /// Target: <file>:<function>. The function name is required.
        #[arg(required = true, value_name = "TARGET")]
        target: String,

        /// Use concolic (Z3-backed) exploration instead of random.
        #[arg(long)]
        concolic: bool,

        /// Maximum number of iterations.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Total timeout in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Per-request timeout in seconds.
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Per-execution timeout in seconds.
        #[arg(long, default_value_t = 30)]
        exec_timeout: u64,

        /// Build timeout in seconds.
        #[arg(long, default_value_t = 60)]
        build_timeout: u64,

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Write ObserveStageOutput JSON to this file. If omitted, writes to stdout.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,
    },

    /// Solve uncovered branches: read Stage 1 observation output and use Z3 constraint
    /// solver to find inputs that trigger uncovered branch directions. No frontend
    /// needed — pure offline computation on serialized observation data.
    Solve {
        /// Path to a Stage 1 observation JSON file (produced by `shatter observe`).
        #[arg(required = true)]
        input: PathBuf,

        /// Write Stage 3 solve output to a JSON file (for downstream stages).
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,

        /// Z3 solver timeout in milliseconds per branch.
        #[arg(long, default_value_t = 5000)]
        solver_timeout: u64,
    },

    /// Build a FunctionSpec from an observation file produced by `shatter observe`.
    Specify {
        /// Path to an ObserveStageOutput JSON file.
        #[arg(value_name = "OBSERVATION_FILE")]
        observation_file: PathBuf,

        /// Path to an AnalyzeStageOutput JSON file from `shatter analyze --output`.
        /// If omitted, the analyze stage runs inline.
        #[arg(long, value_name = "FILE")]
        analyze_file: Option<PathBuf>,

        /// Output spec as JSON instead of markdown.
        #[arg(long, conflicts_with = "yaml")]
        json: bool,

        /// Output spec as YAML with human-friendly property descriptions instead of markdown.
        /// Invariants are rendered as `property:` descriptions (requires --invariants to populate them).
        #[arg(long, conflicts_with = "json")]
        yaml: bool,

        /// Path to a SolveStageOutput JSON file from `shatter solve --output`.
        /// When provided, enriches the spec with Z3-proven provenance and
        /// coverage completeness accounting.
        #[arg(long, value_name = "FILE")]
        solve_file: Option<PathBuf>,

        /// Detect and include function-wide invariants.
        #[arg(long)]
        invariants: bool,

        /// Write spec to this file. If omitted, writes to stdout.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
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

        /// Scan only files with uncommitted changes (staged + unstaged).
        #[arg(long, conflicts_with = "since")]
        changed: bool,

        /// Scan only files changed between <ref> and HEAD.
        #[arg(long, conflicts_with = "changed")]
        since: Option<String>,

        /// End boundary for --since range. Analyzes files as they existed at
        /// this ref instead of the current working tree. Defaults to HEAD.
        #[arg(long, requires = "since")]
        until: Option<String>,

        /// Include untracked files when using --changed.
        #[arg(long, requires = "changed")]
        include_untracked: bool,

        /// Scan all functions, including non-exported ones.
        #[arg(long)]
        all: bool,

        /// Maximum directory traversal depth.
        #[arg(long)]
        max_depth: Option<usize>,

        /// Per-function exploration timeout in seconds. Functions exceeding this
        /// limit are skipped without aborting the scan. Default: 30s.
        /// Overridden by .shatter/config.yaml `defaults.timeout` when not explicitly set.
        #[arg(long)]
        timeout_per_fn: Option<u64>,

        /// Total scan timeout in seconds. Default: 300s.
        /// Overridden by shatter.config.json when not explicitly set.
        #[arg(long)]
        timeout_total: Option<u64>,

        /// Number of parallel frontend subprocesses for exploration.
        /// Default: number of available CPUs (0 = auto-detect).
        /// Overridden by shatter.config.json when not explicitly set.
        #[arg(long)]
        parallelism: Option<usize>,

        /// Override the lower bound of the parallelism clamp (built-in
        /// default: 4). Useful on tiny CI runners. May also be set via
        /// `parallelism_min` in shatter.config.json.
        #[arg(long)]
        parallelism_min: Option<usize>,

        /// Override the upper bound of the parallelism clamp (built-in
        /// default: 16). Useful on large dedicated machines with tuned
        /// `GOMAXPROCS`. May also be set via `parallelism_max` in
        /// shatter.config.json.
        #[arg(long)]
        parallelism_max: Option<usize>,

        /// Path to a mock configuration YAML file.
        #[arg(long)]
        mock_config: Option<PathBuf>,

        /// Write report to file; format inferred from extension (.html, .md, .json, .txt).
        /// May be repeated to write multiple formats simultaneously.
        #[arg(long = "output", short = 'o', value_name = "PATH")]
        outputs: Vec<PathBuf>,

        /// Write report to stdout in addition to any -o files.
        /// When no -o flags are given, stdout is the default output.
        #[arg(long)]
        stdout: bool,

        /// Format for stdout output. One of: markdown (default), json, html, text.
        #[arg(long, default_value = "markdown")]
        format: StdoutFormat,

        /// Show what would be scanned without executing.
        #[arg(long)]
        dry_run: bool,

        /// Resume a previous scan from a checkpoint file, or pass "auto" to
        /// discover the checkpoint from the scan artifact directory.
        #[arg(long)]
        resume: Option<String>,

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

        /// Progressive batch index for core sample.
        /// "0" (first batch), "next" (auto-detect), "0-2" (run batches 0 through 2).
        /// Requires --core-sample.
        #[arg(long)]
        batch: Option<String>,

        /// Stratum filter: explore only specific call graph layers.
        /// Examples: "0" (leaves), "0..3", "-2..-0" (top 3 layers), "3.."
        #[arg(long)]
        stratum: Option<String>,

        /// Maximum number of iterations per function [default: 100].
        /// Pass 0 for unbounded exploration (run until timeout or interrupt).
        /// Overridden by .shatter/config.yaml `defaults.max_iterations` when not explicitly set.
        #[arg(long)]
        max_iterations: Option<u32>,

        /// Per-function exploration wall-clock timeout in seconds. If both
        /// --max-iterations and --timeout-explore are set, whichever triggers
        /// first stops exploration for that function.
        #[arg(long)]
        timeout_explore: Option<f64>,

        /// Directory for caching behavior maps across runs.
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter-cache/behavior-maps/`.
        #[arg(long, env = "SHATTER_CACHE_DIR")]
        cache_dir: Option<PathBuf>,

        /// Disable behavior map caching entirely.
        #[arg(long)]
        no_cache: bool,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation in the frontend.
        /// Default: 10s. Overridden by shatter.config.json when not explicitly set.
        #[arg(long)]
        exec_timeout: Option<u64>,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        /// Default: 30s.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Enable the genetic algorithm explorer.
        #[arg(long)]
        genetic: bool,

        /// Population size for the genetic algorithm (default: 50).
        #[arg(long)]
        genetic_population: Option<u32>,

        /// Maximum generations for the genetic algorithm (default: 100).
        #[arg(long)]
        genetic_generations: Option<u32>,

        /// Timeout in seconds for the genetic algorithm (default: 300).
        #[arg(long)]
        genetic_timeout: Option<u32>,

        /// Disable adaptive strategy scoring (use round-robin instead).
        #[arg(long)]
        no_adaptive: bool,

        /// Sliding window size for strategy outcome scoring.
        #[arg(long)]
        score_window: Option<usize>,

        /// Minimum candidates before a strategy can be deprioritized.
        #[arg(long)]
        cold_start: Option<u64>,

        /// Minimum allocation fraction per strategy (0.0–1.0).
        #[arg(long)]
        strategy_floor: Option<f64>,

        /// Static strategy weight distribution (e.g. "literals=0.3,random=0.5,boundary=0.2").
        #[arg(long)]
        strategy_weights: Option<String>,

        /// Z3 solver timeout in seconds per query. Default: no limit.
        #[arg(long)]
        solver_timeout: Option<u64>,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,

        /// Loop iteration bucket boundaries for path hashing (comma-separated).
        /// Controls how loop iteration counts affect path identity.
        /// Default "0,1,2,5" gives 5 levels: 0, 1, 2, 3–5, 6+ iterations.
        /// Use "none" to disable bucketing (only branch profiles matter).
        #[arg(long, default_value = "0,1,2,5")]
        loop_buckets: String,

        /// Directory for cross-function seed pool (default: .shatter/seeds/).
        /// Falls back to SHATTER_SEEDS_DIR env var.
        #[arg(long, default_value = ".shatter/seeds", env = "SHATTER_SEEDS_DIR")]
        seeds_dir: PathBuf,

        /// Disable loading and saving the cross-function seed pool.
        #[arg(long)]
        no_seeds: bool,

        /// Override all setup/teardown timeouts (seconds). Sets SHATTER_SETUP_TIMEOUT
        /// env var before spawning frontends.
        #[arg(long)]
        setup_timeout: Option<u64>,

        /// Treat setup failures as fatal errors (abort scan immediately).
        #[arg(long)]
        fail_on_setup_error: bool,

        /// Scheduling policy: controls which exploration tasks may overlap.
        /// "layer-parallel" (default): functions within the same topological
        /// layer run concurrently. "serial": one function at a time.
        #[arg(long, default_value = "layer-parallel")]
        scheduler_policy: String,

        /// Execution isolation level.
        ///
        /// - none     (default): executions share a single frontend process; assumes
        ///   functions are side-effect-safe and stateless.
        /// - function: each function invocation gets a fresh execution context.
        /// - serial:  functions run sequentially (no parallelism) in a shared process.
        #[arg(long, value_enum, default_value = "none")]
        isolation: IsolationModeArg,

        /// Enable rich side-effect capture during scan.
        ///
        /// When set, the frontend records console output, file writes, network
        /// requests, environment reads, global mutations, and thrown errors for
        /// each execution. Disabled by default because capture adds overhead on
        /// every execute call; enable only when you need the side-effect data.
        #[arg(long, default_value_t = false)]
        capture_side_effects: bool,

        /// Number of workers to assign per function in shared-pool mode (--isolation none).
        ///
        /// When > 1, each function is explored by this many workers simultaneously,
        /// each with a different random seed derived from the base seed. The total
        /// iteration budget is split evenly across workers so exploration effort stays
        /// constant. Useful when the layer has fewer functions than `--parallelism`,
        /// allowing idle workers to contribute to the same function. Default: 1.
        #[arg(long, default_value_t = 1)]
        workers_per_fn: usize,
    },

    /// Discover and export behavioral properties and invariants as a YAML spec.
    ///
    /// Runs analysis and exploration on the given targets to discover invariants,
    /// then outputs the behavioral spec enriched with property descriptions.
    Properties {
        /// Target files or functions (e.g. src/math.ts or src/math.ts:add).
        #[arg(required = true)]
        targets: Vec<String>,

        /// Write output to FILE instead of stdout.
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Output format (currently only 'yaml' is supported).
        #[arg(long, default_value = "yaml")]
        output_format: String,

        /// Maximum exploration iterations per function.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Overall timeout in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Path to a scope configuration YAML file.
        #[arg(long)]
        scope: Option<PathBuf>,

        /// Per-request timeout in seconds (how long to wait for a single frontend response).
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation in the frontend.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code in the frontend.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,
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

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Z3 solver timeout in seconds per query. Default: no limit.
        #[arg(long)]
        solver_timeout: Option<u64>,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,
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

    /// Compare two function implementations across languages by input/output behavior.
    ///
    /// Ignores branch paths (which are language-specific) and compares only
    /// concrete examples: same inputs should produce same outputs.
    /// Accepts two spec JSON files (as produced by `explore --spec-json`).
    /// Exit code is 0 when all shared behaviors match, nonzero when divergences are found.
    Compare {
        /// Path to the first spec JSON file (e.g., TypeScript implementation).
        #[arg(required = true)]
        spec_a: PathBuf,

        /// Path to the second spec JSON file (e.g., Go implementation).
        #[arg(required = true)]
        spec_b: PathBuf,

        /// Output the comparison result as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Build a custom frontend binary with user-provided native generators.
    ///
    /// Reads generator paths from `.shatter/config.yaml`, compiles a custom
    /// frontend binary that includes native generator functions, and writes
    /// it to `.shatter-cache/bin/`.
    #[command(name = "build-frontend")]
    BuildFrontend {
        /// Target language: "go" or "rust".
        #[arg(required = true)]
        language: String,

        /// Path to the `.shatter/` directory (auto-discovers if omitted).
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output directory (default: `.shatter-cache/bin/`).
        #[arg(long, short)]
        output: Option<PathBuf>,
    },

    /// Discover external network dependencies using strace (Linux-only diagnostic).
    ///
    /// Runs a command under strace, captures all network-related syscalls, and
    /// produces a report of discovered endpoints. Useful for finding dependencies
    /// that static analysis misses.
    #[command(name = "discover-deps")]
    DiscoverDeps {
        /// Command and arguments to trace (e.g., "node src/app.js").
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,

        /// Use strace for syscall-level network discovery.
        #[arg(long)]
        strace: bool,

        /// Working directory for the traced command.
        #[arg(long)]
        working_dir: Option<PathBuf>,

        /// Output as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Check which functions in a source file are stale relative to a spec file.
    ///
    /// Analyzes the source file, computes fingerprints, and compares against the
    /// spec. Exit code: 0 = all fresh, 1 = some stale or removed.
    Stale {
        /// Source file to analyze (e.g., "src/math.ts").
        #[arg(required = true)]
        source: String,

        /// Path to the existing spec JSON file.
        #[arg(required = true)]
        spec: PathBuf,

        /// Output format: "text" (default) or "json".
        #[arg(long = "output-format", default_value = "text")]
        output_format: String,

        /// Per-request timeout in seconds for frontend communication.
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,

        /// Cache directory for loading cross-file dependency fingerprints.
        #[arg(long)]
        cache_dir: Option<PathBuf>,

        /// Disable cache (skip cross-file dependency tracking).
        #[arg(long)]
        no_cache: bool,
    },

    /// Re-execute cached behaviors to detect regressions or drift.
    ///
    /// Loads behavior maps from the cache for the given source file, spawns a
    /// frontend to replay each recorded input, and compares the observed behavior
    /// against the cached expectation. Exit code 0 = no regressions, 1 = issues found.
    Revalidate {
        /// Source file whose cached behaviors to revalidate.
        #[arg(required = true)]
        source: String,

        /// Cache directory for loading behavior maps.
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter-cache/behavior-maps/`.
        #[arg(long, env = "SHATTER_CACHE_DIR")]
        cache_dir: Option<PathBuf>,

        /// Per-request timeout in seconds for frontend communication.
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds for compiling instrumented code.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,

        /// Compile harnesses in release mode (optimized but slower compilation).
        /// Default is debug mode for faster compilation.
        #[arg(long, env = "SHATTER_HARNESS_RELEASE")]
        release: bool,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,

        /// Output format: "text" (default) or "json".
        #[arg(long = "output-format", default_value = "text")]
        output_format: String,
    },

    /// Run tests with impact analysis: only execute tests affected by changed files.
    ///
    /// Uses a coverage map to determine which tests touch which source files,
    /// then queries git for changes and runs only the affected subset.
    Test {
        /// Run all tests, bypassing impact analysis.
        #[arg(long)]
        all: bool,

        /// Force coverage recording to refresh the coverage map.
        #[arg(long)]
        record: bool,

        /// Run a specific test tier and write a success marker.
        #[arg(long)]
        tier: Option<String>,

        /// Base git ref for change detection (default: HEAD).
        #[arg(long, default_value = "HEAD")]
        base: String,

        /// Include untracked files in change detection.
        #[arg(long)]
        include_untracked: bool,

        /// Dry run: show which tests would run without executing them.
        #[arg(long)]
        dry_run: bool,

        /// Prioritize test execution order by marginal coverage per unit time.
        #[arg(long)]
        prioritize: bool,

        /// Time budget for test execution (e.g. "10s", "2m"). Tests beyond this
        /// cumulative time are skipped. Implies --prioritize.
        #[arg(long, value_parser = parse_budget_flag)]
        budget: Option<std::time::Duration>,
    },

    /// Initialize a repository for persistent Shatter project state.
    ///
    /// Creates `.shatter/config.yaml` with sensible defaults and establishes the
    /// repo-local configuration root. Other commands may also create
    /// `.shatter-cache/` and `shatter-artifacts/` when using the initialized
    /// project path. Safe to run on an already-initialized project (idempotent).
    #[command(name = "init")]
    Init {
        /// Directory to initialize (default: auto-detected project root).
        #[arg(short, long)]
        directory: Option<PathBuf>,
    },

    /// Manage anonymous usage telemetry.
    Telemetry {
        #[command(subcommand)]
        action: TelemetryAction,
    },

    /// Manage the on-disk shatter cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Manage the Go frontend artifact workspace.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },

    /// Review and classify suspected-nondeterministic fields.
    Nondeterminism {
        #[command(subcommand)]
        action: NondeterminismAction,
    },

    /// Run benchmarks against a manifest of canonical scenarios.
    ///
    /// Reads a benchmark manifest (default: benchmarks/sample-manifest.json),
    /// selects a tier (smoke/standard/full), and explores each function
    /// multiple times to produce a structured JSON timing bundle.
    Bench {
        /// Path to the benchmark manifest JSON file.
        #[arg(long, default_value = "benchmarks/sample-manifest.json")]
        manifest: std::path::PathBuf,

        /// Benchmark tier to run: smoke, standard, or full.
        #[arg(long, default_value = "smoke")]
        tier: String,

        /// Number of measured repetitions per function.
        #[arg(long, default_value_t = 5)]
        repeats: u32,

        /// Number of warmup runs to discard before measuring.
        #[arg(long, default_value_t = 1)]
        warmups: u32,

        /// Maximum iterations for each explore run (fixed for reproducibility).
        #[arg(long, default_value_t = 20)]
        max_iterations: u32,

        /// Write the benchmark bundle JSON to this file path.
        /// Without this flag, output goes to stdout only.
        #[arg(long, short = 'o')]
        output: Option<std::path::PathBuf>,

        /// Per-request timeout in seconds.
        #[arg(long, default_value_t = 30)]
        request_timeout: u64,

        /// Execution timeout in seconds for each function invocation.
        #[arg(long, default_value_t = 10)]
        exec_timeout: u64,

        /// Build timeout in seconds.
        #[arg(long, default_value_t = 30)]
        build_timeout: u64,
    },
}

/// Sub-subcommands for `shatter nondeterminism`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum NondeterminismAction {
    /// Interactively review nondeterminism candidates from the most recent scan.
    ///
    /// Presents each suspected-nondeterministic field one at a time with evidence.
    /// Respond with:
    ///   y — confirm (add to .shatter/config.yaml nondeterminism.confirmed)
    ///   n — reject  (add to .shatter/config.yaml nondeterminism.rejected)
    ///   s — skip    (defer to next review session)
    ///   ? — show full evidence detail
    ///   q — quit and save progress
    ///
    /// Candidates already confirmed or rejected in .shatter/config.yaml are
    /// suppressed, so only new or escalated candidates appear.
    Review {
        /// Cache directory containing behavior maps from the most recent scan.
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter-cache/behavior-maps/`.
        #[arg(long, env = "SHATTER_CACHE_DIR")]
        cache_dir: Option<std::path::PathBuf>,
    },
}

/// Sub-subcommands for `shatter telemetry`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum TelemetryAction {
    /// Show telemetry consent state, config location, and queue info.
    Status,
    /// Disable anonymous telemetry.
    Off,
    /// Enable anonymous telemetry.
    On,
    /// Regenerate the anonymous ID.
    ResetId,
}

/// Sub-subcommands for `shatter cache`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum CacheAction {
    /// Clear cached analysis and/or exploration results.
    ///
    /// Clears both analysis cache and results cache when no flags are given.
    Clear {
        /// Clear only the analysis cache (`.shatter-cache/analysis/`).
        #[arg(long)]
        analysis: bool,

        /// Clear only the results cache (`.shatter-cache/behavior-maps/`).
        #[arg(long)]
        results: bool,
    },
}

/// Sub-subcommands for `shatter workspace`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum WorkspaceAction {
    /// Prune old runs and cap total workspace disk use.
    Gc {
        /// List candidates without deleting.
        #[arg(long)]
        dry_run: bool,

        /// Keep the N most recent runs.
        #[arg(long, default_value_t = 20)]
        keep: u32,

        /// Delete runs older than this many days.
        #[arg(long = "max-age-days", default_value_t = 14)]
        max_age_days: u32,

        /// Hard cap on total runs/ size (human-readable, e.g. 5GiB, 512MiB).
        #[arg(long = "max-runs-size", default_value = "5GiB")]
        max_runs_size: String,

        /// Per-cache-dir size cap (human-readable, e.g. 5GiB, 512MiB).
        #[arg(long = "max-cache-size", default_value = "5GiB")]
        max_cache_size: String,
    },
}

/// A parsed target: `<file>:<function>` for a single function, or `<file>` for all.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) file: PathBuf,
    pub(crate) function: Option<String>,
    pub(crate) language: Language,
}

/// Supported language frontends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Language {
    TypeScript,
    Go,
    Rust,
}

impl Language {
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" => Some(Language::TypeScript),
            "go" => Some(Language::Go),
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Rust => "rust",
        }
    }
}

/// Parse a target string: `<file>:<function>` or just `<file>`.
///
/// If there is no colon, the entire string is treated as a file path (analyze all functions).
pub(crate) fn parse_target(target: &str) -> Result<Target, String> {
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

/// Validate that all parsed targets refer to existing files.
pub(crate) fn validate_targets(targets: &[Target]) -> Result<(), String> {
    for target in targets {
        if !target.file.exists() {
            return Err(format!("file not found: '{}'", target.file.display()));
        }
    }
    Ok(())
}

/// Parse a `--loop-buckets` CLI string into `LoopBuckets`.
/// Accepts "none" (disables bucketing) or comma-separated u32 values like "0,1,2,5".
pub(crate) fn parse_loop_buckets(
    s: &str,
) -> Result<explorer::LoopBuckets, Box<dyn std::error::Error>> {
    let trimmed = s.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(explorer::LoopBuckets::none());
    }
    let boundaries: Vec<u32> = trimmed
        .split(',')
        .map(|v| v.trim().parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid --loop-buckets value \"{s}\": {e}"))?;
    Ok(explorer::LoopBuckets::from_boundaries(boundaries))
}

/// Clap value_parser for `--budget`: delegates to test_prioritization::parse_budget.
pub(crate) fn parse_budget_flag(s: &str) -> Result<std::time::Duration, String> {
    shatter_core::test_prioritization::parse_budget(s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn timing_defaults_to_off() {
        let cli = Cli::try_parse_from(["shatter", "explore", "file.ts:fn"]).unwrap();
        let timing = cli.timing_config().unwrap();
        assert_eq!(timing.mode, TimingMode::Off);
    }

    #[test]
    fn timing_output_requires_enabled_mode() {
        let cli = Cli::try_parse_from([
            "shatter",
            "--timing-output-dir",
            "/tmp/timing",
            "explore",
            "file.ts:fn",
        ])
        .unwrap();
        let err = cli.timing_config().unwrap_err();
        assert!(err.contains("timing output requires"));
    }

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
    fn parse_target_go_file_and_function() {
        let target = parse_target("pkg/math.go:Add").unwrap();
        assert_eq!(target.file, PathBuf::from("pkg/math.go"));
        assert_eq!(target.function.as_deref(), Some("Add"));
        assert_eq!(target.language, Language::Go);
    }

    #[test]
    fn parse_target_go_file_only() {
        let target = parse_target("pkg/math.go").unwrap();
        assert_eq!(target.file, PathBuf::from("pkg/math.go"));
        assert!(target.function.is_none());
        assert_eq!(target.language, Language::Go);
    }

    #[test]
    fn parse_target_rust_file_and_function() {
        let target = parse_target("src/lib.rs:classify_number").unwrap();
        assert_eq!(target.file, PathBuf::from("src/lib.rs"));
        assert_eq!(target.function.as_deref(), Some("classify_number"));
        assert_eq!(target.language, Language::Rust);
    }

    #[test]
    fn parse_target_rust_file_only() {
        let target = parse_target("src/lib.rs").unwrap();
        assert_eq!(target.file, PathBuf::from("src/lib.rs"));
        assert!(target.function.is_none());
        assert_eq!(target.language, Language::Rust);
    }

    #[test]
    fn language_from_extension_recognizes_rs() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    }

    #[test]
    fn parse_target_trailing_colon_treated_as_file_only() {
        // A trailing colon with empty function name falls through to the file-only path.
        // "src/app.ts:" becomes the file path; OS sees ".ts:" as extension → unsupported.
        let err = parse_target("src/app.ts:").unwrap_err();
        assert!(
            err.contains("unsupported file extension"),
            "expected extension error, got: {err}"
        );
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
        let target =
            parse_target("examples/typescript/src/01-arithmetic.ts:classifyNumber").unwrap();
        assert_eq!(
            target.file,
            PathBuf::from("examples/typescript/src/01-arithmetic.ts")
        );
        assert_eq!(target.function.as_deref(), Some("classifyNumber"));
    }

    #[test]
    fn validate_targets_rejects_nonexistent_file() {
        let targets = vec![Target {
            file: PathBuf::from("nonexistent.ts"),
            function: None,
            language: Language::TypeScript,
        }];
        let err = validate_targets(&targets).unwrap_err();
        assert!(
            err.contains("file not found"),
            "expected 'file not found', got: {err}"
        );
    }

    #[test]
    fn validate_targets_accepts_existing_file() {
        let tmp = std::env::temp_dir().join("shatter_test_validate.ts");
        std::fs::write(&tmp, "").unwrap();
        let targets = vec![Target {
            file: tmp.clone(),
            function: None,
            language: Language::TypeScript,
        }];
        assert!(validate_targets(&targets).is_ok());
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn language_from_extension_recognizes_tsx() {
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
    }

    #[test]
    fn language_labels_are_correct() {
        assert_eq!(Language::TypeScript.label(), "typescript");
        assert_eq!(Language::Go.label(), "go");
        assert_eq!(Language::Rust.label(), "rust");
    }

    #[test]
    fn cli_parses_explore_subcommand() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
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
                assert_eq!(max_iterations, None);
                assert_eq!(timeout, None);
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
            "--scope",
            "shatter.scope.yaml",
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
            "--max-iterations",
            "50",
            "--timeout",
            "120",
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
                assert_eq!(max_iterations, Some(50));
                assert_eq!(timeout, Some(120));
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
            "--cache-dir",
            "/tmp/foo",
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
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
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
            "--request-timeout",
            "10",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                request_timeout,
                timeout,
                ..
            } => {
                assert_eq!(request_timeout, 10);
                assert_eq!(timeout, None);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_inputs_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--inputs",
            "candidates.json",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                inputs,
                config_path,
                ..
            } => {
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
            "--config",
            ".shatter/config.yaml",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                inputs,
                config_path,
                ..
            } => {
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
            "--request-timeout",
            "15",
            "--timeout-total",
            "200",
            "test_dir",
        ]);
        match cli.command {
            CliCommand::Scan {
                request_timeout,
                timeout_total,
                ..
            } => {
                assert_eq!(request_timeout, 15);
                assert_eq!(timeout_total, Some(200));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_run_with_request_timeout() {
        let cli = Cli::parse_from(["shatter", "run", "--request-timeout", "45", "/tmp/repo"]);
        match cli.command {
            CliCommand::Run {
                request_timeout,
                timeout,
                ..
            } => {
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
            "--exec-timeout",
            "20",
            "--build-timeout",
            "45",
            "test.go:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                exec_timeout,
                build_timeout,
                ..
            } => {
                assert_eq!(exec_timeout, 20);
                assert_eq!(build_timeout, 45);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_explore_exec_timeout_defaults() {
        let cli = Cli::parse_from(["shatter", "explore", "test.go:myFunc"]);
        match cli.command {
            CliCommand::Explore {
                exec_timeout,
                build_timeout,
                ..
            } => {
                assert_eq!(exec_timeout, 10);
                assert_eq!(build_timeout, 30);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_concolic_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--concolic", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { concolic, .. } => {
                assert!(concolic);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_concolic_defaults_to_false() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { concolic, .. } => {
                assert!(!concolic);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_record_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--record", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { record, .. } => {
                assert!(record);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_record_defaults_to_false() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { record, .. } => {
                assert!(!record);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_replay_recorded_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--replay-recorded", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore {
                replay_recorded,
                no_replay,
                ..
            } => {
                assert!(replay_recorded);
                assert!(!no_replay);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_no_replay_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--no-replay", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore {
                replay_recorded,
                no_replay,
                ..
            } => {
                assert!(!replay_recorded);
                assert!(no_replay);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_replay_recorded_defaults_to_false() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore {
                replay_recorded,
                no_replay,
                ..
            } => {
                assert!(!replay_recorded);
                assert!(!no_replay);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_parallelism_overrides() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--parallelism-min",
            "2",
            "--parallelism-max",
            "32",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                parallelism_min,
                parallelism_max,
                ..
            } => {
                assert_eq!(parallelism_min, Some(2));
                assert_eq!(parallelism_max, Some(32));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_parallelism_overrides() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--parallelism-min",
            "2",
            "--parallelism-max",
            "32",
            "test.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore {
                parallelism_min,
                parallelism_max,
                ..
            } => {
                assert_eq!(parallelism_min, Some(2));
                assert_eq!(parallelism_max, Some(32));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_scan_parallelism_overrides_default_to_none() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan {
                parallelism_min,
                parallelism_max,
                ..
            } => {
                assert_eq!(parallelism_min, None);
                assert_eq!(parallelism_max, None);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_exec_timeout() {
        let cli = Cli::parse_from(["shatter", "scan", "--exec-timeout", "15", "test_dir"]);
        match cli.command {
            CliCommand::Scan {
                exec_timeout,
                build_timeout,
                ..
            } => {
                assert_eq!(exec_timeout, Some(15));
                assert_eq!(build_timeout, 30);
            }
            _ => panic!("expected Scan command"),
        }
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
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
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
                assert_eq!(max_iterations, None);
                assert_eq!(timeout_total, None);
                assert!(!no_cache);
                assert_eq!(request_timeout, 30);
                assert_eq!(parallelism, None);
                assert_eq!(timeout_per_fn, None);
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
            "--max-iterations",
            "50",
            "--timeout-total",
            "600",
            "--dry-run",
            "--language",
            "typescript",
            "--include",
            "**/*.ts",
            "--exclude",
            "**/vendor/**",
            "--all",
            "--max-depth",
            "3",
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
                assert_eq!(max_iterations, Some(50));
                assert_eq!(timeout_total, Some(600));
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
        let cli = Cli::parse_from(["shatter", "scan", "--output", "/tmp/report", "src/"]);
        match cli.command {
            CliCommand::Scan { outputs, .. } => {
                assert_eq!(outputs, vec![PathBuf::from("/tmp/report")]);
            }
            _ => panic!("expected Scan command"),
        }

        // Default: no outputs
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan { outputs, .. } => {
                assert!(outputs.is_empty());
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
    fn cli_parses_explore_with_no_cache() {
        let cli = Cli::parse_from(["shatter", "explore", "--no-cache", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { no_cache, .. } => {
                assert!(no_cache);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_no_cache() {
        let cli = Cli::parse_from(["shatter", "scan", "--no-cache", "src/"]);
        match cli.command {
            CliCommand::Scan { no_cache, .. } => {
                assert!(no_cache);
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_no_cache_defaults_to_false_for_explore() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { no_cache, .. } => {
                assert!(!no_cache);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_no_cache_defaults_to_false_for_scan() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan { no_cache, .. } => {
                assert!(!no_cache);
            }
            _ => panic!("expected Scan command"),
        }
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
        let cli = Cli::parse_from(["shatter", "diff", "--json", "old.json", "new.json"]);
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
        let cli = Cli::parse_from(["shatter", "spec-diff", "specs/old.json", "specs/new.json"]);
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
    fn cli_parses_compare_subcommand() {
        let cli = Cli::parse_from(["shatter", "compare", "spec_a.json", "spec_b.json"]);
        match cli.command {
            CliCommand::Compare {
                spec_a,
                spec_b,
                json,
            } => {
                assert_eq!(spec_a, PathBuf::from("spec_a.json"));
                assert_eq!(spec_b, PathBuf::from("spec_b.json"));
                assert!(!json);
            }
            _ => panic!("expected Compare command"),
        }
    }

    #[test]
    fn cli_parses_compare_with_json_flag() {
        let cli = Cli::parse_from(["shatter", "compare", "--json", "spec_a.json", "spec_b.json"]);
        match cli.command {
            CliCommand::Compare { json, .. } => {
                assert!(json);
            }
            _ => panic!("expected Compare command"),
        }
    }

    #[test]
    fn cli_compare_requires_both_arguments() {
        let result = Cli::try_parse_from(["shatter", "compare"]);
        assert!(result.is_err());

        let result = Cli::try_parse_from(["shatter", "compare", "only-one.json"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_parses_run_subcommand() {
        let cli = Cli::parse_from(["shatter", "run", "/tmp/my-repo"]);
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
            "--output-dir",
            "/tmp/output",
            "--max-iterations",
            "25",
            "--timeout",
            "120",
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
    fn cli_scan_new_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--progress",
            "--resume",
            "/tmp/state.json",
            "--mock-config",
            "/tmp/mocks.yaml",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                progress,
                resume,
                mock_config,
                ..
            } => {
                assert!(progress);
                assert_eq!(resume, Some("/tmp/state.json".to_string()));
                assert_eq!(mock_config, Some(PathBuf::from("/tmp/mocks.yaml")));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_until_requires_since() {
        // --until without --since should fail
        let result = Cli::try_parse_from(["shatter", "scan", "--until", "HEAD~2", "src/"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_scan_since_with_until() {
        let cli = Cli::parse_from([
            "shatter", "scan", "--since", "HEAD~5", "--until", "HEAD~2", "src/",
        ]);
        match cli.command {
            CliCommand::Scan { since, until, .. } => {
                assert_eq!(since, Some("HEAD~5".to_string()));
                assert_eq!(until, Some("HEAD~2".to_string()));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_scan_since_without_until() {
        let cli = Cli::parse_from(["shatter", "scan", "--since", "HEAD~5", "src/"]);
        match cli.command {
            CliCommand::Scan { since, until, .. } => {
                assert_eq!(since, Some("HEAD~5".to_string()));
                assert!(until.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_core_sample() {
        let cli = Cli::parse_from(["shatter", "scan", "--core-sample", "50%", "src/"]);
        match cli.command {
            CliCommand::Scan {
                core_sample, seed, ..
            } => {
                assert_eq!(core_sample, Some("50%".to_string()));
                assert!(seed.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_core_sample_absolute() {
        let cli = Cli::parse_from(["shatter", "scan", "--core-sample", "20", "src/"]);
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
            "--core-sample",
            "50%",
            "--seed",
            "12345",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                core_sample, seed, ..
            } => {
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
            CliCommand::Scan {
                core_sample, seed, ..
            } => {
                assert!(core_sample.is_none());
                assert!(seed.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_core_sample_and_stratum() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--core-sample",
            "50%",
            "--stratum",
            "0..2",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                core_sample,
                stratum,
                ..
            } => {
                assert_eq!(core_sample, Some("50%".to_string()));
                assert_eq!(stratum, Some("0..2".to_string()));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_spec_out_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--spec-out",
            "spec.json",
            "src/app.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore { spec_out, .. } => {
                assert_eq!(spec_out, Some(PathBuf::from("spec.json")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_persist_stages_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--persist-stages",
            "stage-cache",
            "src/app.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore { persist_stages, .. } => {
                assert_eq!(persist_stages, Some(PathBuf::from("stage-cache")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_persist_stages_defaults_to_none() {
        let cli = Cli::parse_from(["shatter", "explore", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore { persist_stages, .. } => {
                assert!(persist_stages.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_spec_out_defaults_to_none() {
        let cli = Cli::parse_from(["shatter", "explore", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore { spec_out, .. } => {
                assert!(spec_out.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_genetic_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--genetic", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore {
                genetic,
                genetic_population,
                genetic_generations,
                genetic_timeout,
                ..
            } => {
                assert!(genetic);
                assert!(genetic_population.is_none());
                assert!(genetic_generations.is_none());
                assert!(genetic_timeout.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_genetic_defaults_to_false() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { genetic, .. } => {
                assert!(!genetic);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_genetic_options() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--genetic",
            "--genetic-population",
            "200",
            "--genetic-generations",
            "500",
            "--genetic-timeout",
            "600",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                genetic,
                genetic_population,
                genetic_generations,
                genetic_timeout,
                ..
            } => {
                assert!(genetic);
                assert_eq!(genetic_population, Some(200));
                assert_eq!(genetic_generations, Some(500));
                assert_eq!(genetic_timeout, Some(600));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_scan_with_genetic_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--genetic",
            "--genetic-population",
            "100",
            "test_dir",
        ]);
        match cli.command {
            CliCommand::Scan {
                genetic,
                genetic_population,
                genetic_generations,
                genetic_timeout,
                ..
            } => {
                assert!(genetic);
                assert_eq!(genetic_population, Some(100));
                assert!(genetic_generations.is_none());
                assert!(genetic_timeout.is_none());
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_clean_flag() {
        let cli = Cli::parse_from(["shatter", "explore", "--clean", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { clean, dry_run, .. } => {
                assert!(clean);
                assert!(!dry_run);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_dry_run_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--dry-run",
            "--spec-out",
            "spec.json",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore {
                clean,
                dry_run,
                spec_out,
                ..
            } => {
                assert!(!clean);
                assert!(dry_run);
                assert_eq!(spec_out, Some(PathBuf::from("spec.json")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_clean_and_dry_run_default_to_false() {
        let cli = Cli::parse_from(["shatter", "explore", "test.ts:myFunc"]);
        match cli.command {
            CliCommand::Explore { clean, dry_run, .. } => {
                assert!(!clean);
                assert!(!dry_run);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_stale_subcommand() {
        let cli = Cli::parse_from(["shatter", "stale", "src/math.ts", "spec.json"]);
        match cli.command {
            CliCommand::Stale {
                source,
                spec,
                output_format,
                request_timeout,
                ..
            } => {
                assert_eq!(source, "src/math.ts");
                assert_eq!(spec, PathBuf::from("spec.json"));
                assert_eq!(output_format, "text");
                assert_eq!(request_timeout, 30);
            }
            _ => panic!("expected Stale command"),
        }
    }

    #[test]
    fn cli_parses_stale_with_json_format() {
        let cli = Cli::parse_from([
            "shatter",
            "stale",
            "--output-format",
            "json",
            "src/math.ts",
            "spec.json",
        ]);
        match cli.command {
            CliCommand::Stale { output_format, .. } => {
                assert_eq!(output_format, "json");
            }
            _ => panic!("expected Stale command"),
        }
    }

    #[test]
    fn cli_parses_test_subcommand_defaults() {
        let cli = Cli::parse_from(["shatter", "test"]);
        match cli.command {
            CliCommand::Test {
                all,
                record,
                tier,
                base,
                include_untracked,
                dry_run,
                prioritize,
                budget,
            } => {
                assert!(!all);
                assert!(!record);
                assert!(tier.is_none());
                assert_eq!(base, "HEAD");
                assert!(!include_untracked);
                assert!(!dry_run);
                assert!(!prioritize);
                assert!(budget.is_none());
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_all() {
        let cli = Cli::parse_from(["shatter", "test", "--all"]);
        match cli.command {
            CliCommand::Test { all, .. } => {
                assert!(all);
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_record() {
        let cli = Cli::parse_from(["shatter", "test", "--record"]);
        match cli.command {
            CliCommand::Test { record, .. } => {
                assert!(record);
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_tier() {
        let cli = Cli::parse_from(["shatter", "test", "--tier", "quick"]);
        match cli.command {
            CliCommand::Test { tier, .. } => {
                assert_eq!(tier, Some("quick".to_string()));
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_dry_run() {
        let cli = Cli::parse_from(["shatter", "test", "--dry-run", "--include-untracked"]);
        match cli.command {
            CliCommand::Test {
                dry_run,
                include_untracked,
                ..
            } => {
                assert!(dry_run);
                assert!(include_untracked);
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_prioritize() {
        let cli = Cli::parse_from(["shatter", "test", "--prioritize"]);
        match cli.command {
            CliCommand::Test {
                prioritize, budget, ..
            } => {
                assert!(prioritize);
                assert!(budget.is_none());
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_budget_seconds() {
        let cli = Cli::parse_from(["shatter", "test", "--budget", "10s"]);
        match cli.command {
            CliCommand::Test { budget, .. } => {
                assert_eq!(budget, Some(std::time::Duration::from_secs(10)));
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_budget_minutes() {
        let cli = Cli::parse_from(["shatter", "test", "--budget", "2m"]);
        match cli.command {
            CliCommand::Test { budget, .. } => {
                assert_eq!(budget, Some(std::time::Duration::from_secs(120)));
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_test_budget_combined() {
        let cli = Cli::parse_from(["shatter", "test", "--budget", "1m30s"]);
        match cli.command {
            CliCommand::Test { budget, .. } => {
                assert_eq!(budget, Some(std::time::Duration::from_secs(90)));
            }
            _ => panic!("expected Test command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_no_adaptive() {
        let cli = Cli::parse_from(["shatter", "explore", "--no-adaptive", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore { no_adaptive, .. } => {
                assert!(no_adaptive);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_strategy_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--score-window",
            "50",
            "--cold-start",
            "10",
            "--strategy-floor",
            "0.05",
            "--strategy-weights",
            "literals=0.3,random=0.7",
            "src/app.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore {
                score_window,
                cold_start,
                strategy_floor,
                strategy_weights,
                no_adaptive,
                ..
            } => {
                assert!(!no_adaptive);
                assert_eq!(score_window, Some(50));
                assert_eq!(cold_start, Some(10));
                assert!((strategy_floor.unwrap() - 0.05).abs() < f64::EPSILON);
                assert_eq!(
                    strategy_weights,
                    Some("literals=0.3,random=0.7".to_string())
                );
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_strategy_flags_default_to_none() {
        let cli = Cli::parse_from(["shatter", "explore", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore {
                no_adaptive,
                score_window,
                cold_start,
                strategy_floor,
                strategy_weights,
                ..
            } => {
                assert!(!no_adaptive);
                assert!(score_window.is_none());
                assert!(cold_start.is_none());
                assert!(strategy_floor.is_none());
                assert!(strategy_weights.is_none());
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_scan_parses_strategy_flags() {
        let cli = Cli::parse_from([
            "shatter",
            "scan",
            "--no-adaptive",
            "--score-window",
            "200",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                no_adaptive,
                score_window,
                ..
            } => {
                assert!(no_adaptive);
                assert_eq!(score_window, Some(200));
            }
            _ => panic!("expected Scan command"),
        }
    }

    #[test]
    fn cli_parses_set_overrides_repeatable() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--set",
            "defaults.max_iterations=200",
            "--set",
            "defaults.exploration.adaptive=false",
            "target.ts:fn",
        ]);
        assert_eq!(
            cli.set_overrides,
            vec![
                "defaults.max_iterations=200".to_string(),
                "defaults.exploration.adaptive=false".to_string(),
            ]
        );
    }

    #[test]
    fn cli_set_overrides_empty_by_default() {
        let cli = Cli::parse_from(["shatter", "explore", "target.ts:fn"]);
        assert!(cli.set_overrides.is_empty());
    }
}

#[cfg(test)]
mod output_format_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_infer_html() {
        assert_eq!(
            infer_output_format(Path::new("report.html")).unwrap(),
            StdoutFormat::Html
        );
    }

    #[test]
    fn test_infer_markdown() {
        assert_eq!(
            infer_output_format(Path::new("report.md")).unwrap(),
            StdoutFormat::Markdown
        );
    }

    #[test]
    fn test_infer_json() {
        assert_eq!(
            infer_output_format(Path::new("report.json")).unwrap(),
            StdoutFormat::Json
        );
    }

    #[test]
    fn test_infer_text() {
        assert_eq!(
            infer_output_format(Path::new("report.txt")).unwrap(),
            StdoutFormat::Text
        );
    }

    #[test]
    fn test_infer_unknown_extension() {
        let err = infer_output_format(Path::new("report.foo")).unwrap_err();
        assert!(err.contains("unknown output format for extension '.foo'"));
        assert!(err.contains(".html, .md, .json, .txt"));
    }

    #[test]
    fn test_infer_no_extension() {
        let err = infer_output_format(Path::new("report")).unwrap_err();
        assert!(err.contains("no extension"));
    }
}
