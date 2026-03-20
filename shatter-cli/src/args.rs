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
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "timing_output_dir")]
    pub(crate) timing_output: Option<PathBuf>,

    /// Write timing artifact JSON files into this directory.
    #[arg(long, global = true, value_name = "DIR", conflicts_with = "timing_output")]
    pub(crate) timing_output_dir: Option<PathBuf>,

    /// Override auto-detected project root directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub(crate) project_dir: Option<std::path::PathBuf>,

    /// When to use terminal colors: always, auto (default), or never.
    /// Respects the NO_COLOR environment variable (auto treats it as never).
    #[arg(long, global = true, default_value = "auto", value_name = "WHEN")]
    pub(crate) color: ColorMode,

    /// Terminal output format: md (default, rendered via termimad) or plain (legacy ANSI).
    #[arg(long, global = true, default_value = "md", value_name = "FORMAT")]
    pub(crate) format: OutputFormat,

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

        /// Maximum number of iterations for the concolic loop.
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
        /// call data to .shatter/recorded-mocks/ as seed fixtures for future runs.
        #[arg(long)]
        record: bool,

        /// Write raw observation data (Stage 1 output) to a directory for offline
        /// analysis with `shatter analyze`. One JSON file per function.
        #[arg(long)]
        observe_output: Option<PathBuf>,

        /// Replay previously recorded mock fixtures from .shatter/recorded-mocks/.
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

        /// Write ObserveStageOutput JSON to this file. If omitted, writes to stdout.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,

        /// Memory limit in MB for the frontend process.
        #[arg(long)]
        memory_limit: Option<u64>,
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
        #[arg(long)]
        json: bool,

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
        #[arg(long = "report-format", default_value = "json")]
        report_format: String,

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

        /// Progressive batch index for core sample.
        /// "0" (first batch), "next" (auto-detect), "0-2" (run batches 0 through 2).
        /// Requires --core-sample.
        #[arg(long)]
        batch: Option<String>,

        /// Stratum filter: explore only specific call graph layers.
        /// Examples: "0" (leaves), "0..3", "-2..-0" (top 3 layers), "3.."
        #[arg(long)]
        stratum: Option<String>,

        /// Maximum number of iterations per function.
        #[arg(long, default_value_t = 100)]
        max_iterations: u32,

        /// Per-function exploration wall-clock timeout in seconds. If both
        /// --max-iterations and --timeout-explore are set, whichever triggers
        /// first stops exploration for that function.
        #[arg(long)]
        timeout_explore: Option<f64>,

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

    /// Build a custom frontend binary with user-provided native generators.
    ///
    /// Reads generator paths from `.shatter/config.yaml`, compiles a custom
    /// frontend binary that includes native generator functions, and writes
    /// it to `.shatter/bin/`.
    #[command(name = "build-frontend")]
    BuildFrontend {
        /// Target language: "go" or "rust".
        #[arg(required = true)]
        language: String,

        /// Path to the `.shatter/` directory (auto-discovers if omitted).
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output directory (default: `.shatter/bin/`).
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
        /// Falls back to SHATTER_CACHE_DIR env var, then `.shatter/cache/`.
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

    /// Manage anonymous usage telemetry.
    Telemetry {
        #[command(subcommand)]
        action: TelemetryAction,
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

/// A parsed target: `<file>:<function>` for a single function, or `<file>` for all.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) file: PathBuf,
    pub(crate) function: Option<String>,
    pub(crate) language: Language,
}

/// Supported language frontends.
#[derive(Debug, Clone, Copy, PartialEq)]
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
pub(crate) fn parse_loop_buckets(s: &str) -> Result<explorer::LoopBuckets, Box<dyn std::error::Error>> {
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
        ]).unwrap();
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
        assert!(err.contains("unsupported file extension"), "expected extension error, got: {err}");
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
        let target = parse_target("examples/typescript/src/01-arithmetic.ts:classifyNumber").unwrap();
        assert_eq!(target.file, PathBuf::from("examples/typescript/src/01-arithmetic.ts"));
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
        assert!(err.contains("file not found"), "expected 'file not found', got: {err}");
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
    fn cli_parses_explore_with_concolic_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--concolic",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { concolic, .. } => {
                assert!(concolic);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_concolic_defaults_to_false() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { concolic, .. } => {
                assert!(!concolic);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_record_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--record",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { record, .. } => {
                assert!(record);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_record_defaults_to_false() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { record, .. } => {
                assert!(!record);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_replay_recorded_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--replay-recorded",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { replay_recorded, no_replay, .. } => {
                assert!(replay_recorded);
                assert!(!no_replay);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_parses_explore_with_no_replay_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--no-replay",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { replay_recorded, no_replay, .. } => {
                assert!(!replay_recorded);
                assert!(no_replay);
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_replay_recorded_defaults_to_false() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { replay_recorded, no_replay, .. } => {
                assert!(!replay_recorded);
                assert!(!no_replay);
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
            "--report-format", "markdown",
            "--resume", "/tmp/state.json",
            "--mock-config", "/tmp/mocks.yaml",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan {
                progress,
                report_format,
                resume,
                mock_config,
                ..
            } => {
                assert!(progress);
                assert_eq!(report_format, "markdown");
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

    #[test]
    fn cli_parses_explore_with_genetic_flag() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--genetic",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { genetic, genetic_population, genetic_generations, genetic_timeout, .. } => {
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
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
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
            "--genetic-population", "200",
            "--genetic-generations", "500",
            "--genetic-timeout", "600",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { genetic, genetic_population, genetic_generations, genetic_timeout, .. } => {
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
            "--genetic-population", "100",
            "test_dir",
        ]);
        match cli.command {
            CliCommand::Scan { genetic, genetic_population, genetic_generations, genetic_timeout, .. } => {
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
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "--clean",
            "test.ts:myFunc",
        ]);
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
            "--output", "spec.json",
            "test.ts:myFunc",
        ]);
        match cli.command {
            CliCommand::Explore { clean, dry_run, output, .. } => {
                assert!(!clean);
                assert!(dry_run);
                assert_eq!(output, Some(PathBuf::from("spec.json")));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_clean_and_dry_run_default_to_false() {
        let cli = Cli::parse_from([
            "shatter",
            "explore",
            "test.ts:myFunc",
        ]);
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
        let cli = Cli::parse_from([
            "shatter",
            "stale",
            "src/math.ts",
            "spec.json",
        ]);
        match cli.command {
            CliCommand::Stale { source, spec, output_format, request_timeout, .. } => {
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
            "--output-format", "json",
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
            CliCommand::Test { all, record, tier, base, include_untracked, dry_run, prioritize, budget } => {
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
            CliCommand::Test { dry_run, include_untracked, .. } => {
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
            CliCommand::Test { prioritize, budget, .. } => {
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
        let cli = Cli::parse_from([
            "shatter", "explore", "--no-adaptive", "src/app.ts:foo",
        ]);
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
            "shatter", "explore",
            "--score-window", "50",
            "--cold-start", "10",
            "--strategy-floor", "0.05",
            "--strategy-weights", "literals=0.3,random=0.7",
            "src/app.ts:foo",
        ]);
        match cli.command {
            CliCommand::Explore {
                score_window, cold_start, strategy_floor, strategy_weights, no_adaptive, ..
            } => {
                assert!(!no_adaptive);
                assert_eq!(score_window, Some(50));
                assert_eq!(cold_start, Some(10));
                assert!((strategy_floor.unwrap() - 0.05).abs() < f64::EPSILON);
                assert_eq!(strategy_weights, Some("literals=0.3,random=0.7".to_string()));
            }
            _ => panic!("expected Explore command"),
        }
    }

    #[test]
    fn cli_strategy_flags_default_to_none() {
        let cli = Cli::parse_from(["shatter", "explore", "src/app.ts:foo"]);
        match cli.command {
            CliCommand::Explore {
                no_adaptive, score_window, cold_start, strategy_floor, strategy_weights, ..
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
            "shatter", "scan",
            "--no-adaptive",
            "--score-window", "200",
            "src/",
        ]);
        match cli.command {
            CliCommand::Scan { no_adaptive, score_window, .. } => {
                assert!(no_adaptive);
                assert_eq!(score_window, Some(200));
            }
            _ => panic!("expected Scan command"),
        }
    }
}
