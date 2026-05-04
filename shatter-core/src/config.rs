//! Shatter configuration: `.shatter/config.yaml` parsing, hierarchical discovery, and merging.
//!
//! Users can place `.shatter/` directories at any level of their project tree.
//! Each directory can contain a `config.yaml` with per-function settings and
//! an `inputs/` subdirectory with candidate input files.
//!
//! Config resolution walks upward from each target file to the filesystem root,
//! collecting all `.shatter/config.yaml` files. The nearest config wins on
//! conflicts (closest to the target file takes precedence).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use serde::{Deserialize, Serialize};

use crate::mock_fixture::MockFixtureConfig;
use crate::protocol::{ExecutionProfile, SetupLevel};

/// Default setup timeout in seconds, applied when no explicit value is configured.
pub const DEFAULT_SETUP_TIMEOUT_SECS: u64 = 30;

/// Default scan budget constants — shared between CLI default resolution and project config.
pub const DEFAULT_SCAN_MAX_ITERATIONS: u32 = 100;
pub const DEFAULT_SCAN_TIMEOUT_TOTAL: u64 = 300;
pub const DEFAULT_SCAN_TIMEOUT_PER_FN: u64 = 30;
pub const DEFAULT_SCAN_EXEC_TIMEOUT: u64 = 10;
pub const DEFAULT_SCAN_PARALLELISM: usize = 0;

/// Default: adaptive strategy scoring enabled.
pub const DEFAULT_EXPLORATION_ADAPTIVE: bool = true;
/// Default sliding window size for outcome-based strategy scoring.
pub const DEFAULT_EXPLORATION_SCORE_WINDOW: usize = 100;
/// Default minimum candidates before a strategy can be deprioritized.
pub const DEFAULT_EXPLORATION_COLD_START: u64 = 20;
/// Default minimum allocation fraction per strategy (2%).
pub const DEFAULT_EXPLORATION_STRATEGY_FLOOR: f64 = 0.02;

/// Consecutive no-new-path executions before ending a fuzz phase.
pub const DEFAULT_FUZZ_PLATEAU_THRESHOLD: u32 = 50;
/// Maximum total executions per fuzz phase.
pub const DEFAULT_FUZZ_MAX_EXECUTIONS: u32 = 1000;
/// Wall-clock timeout in seconds per fuzz phase.
pub const DEFAULT_FUZZ_TIMEOUT_SECS: u32 = 30;
/// Maximum fuzz attempts per branch before giving up (bounded mode).
pub const DEFAULT_FUZZ_MAX_ATTEMPTS: u32 = 3;

/// Session-level setup configuration: a setup file run once before any file
/// in the test session.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SessionSetupConfig {
    /// Path to the session setup file, relative to the `.shatter/` directory.
    pub file: String,
    /// Timeout in seconds for session setup/teardown (overrides `setup_timeout`).
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Error type for config operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file '{path}': {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config YAML '{path}': {source}")]
    Parse {
        path: PathBuf,
        source: serde_yaml::Error,
    },

    #[error("failed to read candidate inputs '{path}': {source}")]
    InputIo {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse candidate inputs '{path}': {source}")]
    InputParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to parse project config '{path}': {source}")]
    ProjectConfigParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        pattern: String,
        source: globset::Error,
    },

    #[error("invalid strategy weights: {0}")]
    InvalidStrategyWeights(String),

    #[error("invalid --set override '{pair}': {reason}")]
    InvalidSetOverride { pair: String, reason: String },

    #[error("--set override produced invalid config: {source}")]
    SetOverrideDeserialize { source: serde_yaml::Error },
}

/// One entry in the `opaque_types` config list.
///
/// Supports both a bare type name (string shorthand) and an object form with
/// an optional user-supplied reason:
///
/// ```yaml
/// opaque_types:
///   - DatabaseConnection          # bare string — falls back to "user-configured opaque type"
///   - name: HttpClient
///     reason: "requires live HTTP connection"
/// ```
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CustomOpaqueType {
    /// `- TypeName` (bare string shorthand)
    Name(String),
    /// `- name: TypeName\n  reason: "..."` (object form with optional reason)
    Named {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl CustomOpaqueType {
    /// Returns the type name to match against parameter `type_name` fields.
    pub fn name(&self) -> &str {
        match self {
            CustomOpaqueType::Name(s) => s,
            CustomOpaqueType::Named { name, .. } => name,
        }
    }

    /// Returns the user-supplied reason text, if any.
    pub fn reason(&self) -> Option<&str> {
        match self {
            CustomOpaqueType::Name(_) => None,
            CustomOpaqueType::Named { reason, .. } => reason.as_deref(),
        }
    }
}

/// Top-level `.shatter/config.yaml` structure.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ShatterConfig {
    /// Default settings applied to all functions unless overridden.
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Per-function overrides, keyed by function pattern (e.g. `"src/auth.ts:validateToken"`).
    #[serde(default)]
    pub functions: HashMap<String, FunctionConfig>,

    /// Additional type names to treat as opaque (unexecutable).
    ///
    /// Each entry is either a bare type name string or an object with `name` and
    /// optional `reason` fields. See [`CustomOpaqueType`] for details.
    #[serde(default)]
    pub opaque_types: Vec<CustomOpaqueType>,

    /// User-declared nondeterminism: confirmed and rejected field declarations.
    #[serde(default)]
    pub nondeterminism: Option<NondeterminismConfig>,

    /// User-defined mock fixtures with three-level scoped resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mock_fixtures: Option<MockFixtureConfig>,
}

/// Filename for the project-level configuration file.
pub const PROJECT_CONFIG_FILENAME: &str = "shatter.config.json";

/// Project-level configuration loaded from `shatter.config.json`.
///
/// Contains **scan-global** settings only: file discovery, output, caching,
/// resource limits, and parallelism. Per-function settings (iterations,
/// timeouts, mocks, genetic, generators, setup) belong in
/// `.shatter/config.yaml` — see [`DefaultsConfig`] and [`FunctionConfig`].
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ProjectConfig {
    /// Glob patterns for files to include in scans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,

    /// Glob patterns for files to exclude from scans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    /// Language filter (typescript, go, or rust).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Maximum directory traversal depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,

    /// Total scan wall-clock timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_total: Option<u64>,

    /// Function execution timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec_timeout: Option<u64>,

    /// Number of parallel frontend subprocesses (0 = auto-detect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<usize>,

    /// Override the lower bound of the global parallelism clamp (built-in
    /// default: 4). Useful on tiny CI runners that need a lower floor than
    /// the built-in default. See str-v01r.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelism_min: Option<usize>,

    /// Override the upper bound of the global parallelism clamp (built-in
    /// default: 16). Useful on large dedicated machines with tuned
    /// `GOMAXPROCS` that can safely run more than 16 concurrent frontends.
    /// See str-v01r.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelism_max: Option<usize>,

    /// Number of observer subprocesses the random explorer fans candidate
    /// executions out to within a single function. Each slot is a separate
    /// frontend subprocess (frontends remain serial per process). `1`
    /// preserves the legacy single-process exploration path. See str-frc.3
    /// for the underlying primitive and str-frc.6 for the surfaced knob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observer_pool: Option<usize>,

    /// Override the bounded candidate queue capacity that sits between the
    /// candidate generator and the observer pool (str-frc.5). When unset, the
    /// capacity is auto-derived from `observer_pool` and `max_iterations`.
    /// Has no effect when `observer_pool <= 1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_queue_capacity: Option<usize>,

    /// Output preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputConfig>,

    /// Behavior map cache directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,

    /// Disable caching entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_cache: Option<bool>,

    /// Cross-function seed pool directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seeds_dir: Option<PathBuf>,

    /// Enable rich side-effect capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_side_effects: Option<bool>,
}

/// Output preferences for scan reports.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OutputConfig {
    /// Default stdout format: "markdown", "json", "html", or "text".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Report file paths (format inferred from extension).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathBuf>,

    /// Write report to stdout even when output files are specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<bool>,
}

/// Return the expected path for `shatter.config.json` in the given directory.
pub fn project_config_path(dir: &Path) -> PathBuf {
    dir.join(PROJECT_CONFIG_FILENAME)
}

/// Load project config from `shatter.config.json` in `dir`, if it exists.
///
/// Returns `Ok(None)` when the file is absent. Returns an error on I/O
/// failures or invalid JSON.
pub fn load_project_config(dir: &Path) -> Result<Option<ProjectConfig>, ConfigError> {
    let path = project_config_path(dir);
    if !path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    let config: ProjectConfig = serde_json::from_str(&contents)
        .map_err(|e| ConfigError::ProjectConfigParse { path, source: e })?;
    Ok(Some(config))
}

/// Configuration for the genetic algorithm explorer.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct GeneticConfig {
    /// Whether the genetic algorithm explorer is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Number of individuals in each generation.
    #[serde(default = "GeneticConfig::default_population_size")]
    pub population_size: u32,

    /// Maximum number of generations to evolve.
    #[serde(default = "GeneticConfig::default_max_generations")]
    pub max_generations: u32,

    /// Probability of mutating an individual (0.0–1.0).
    #[serde(default = "GeneticConfig::default_mutation_rate")]
    pub mutation_rate: f64,

    /// Probability of crossover between two individuals (0.0–1.0).
    #[serde(default = "GeneticConfig::default_crossover_rate")]
    pub crossover_rate: f64,

    /// Timeout in seconds for the entire genetic exploration.
    #[serde(default = "GeneticConfig::default_timeout_secs")]
    pub timeout_secs: u32,
}

impl GeneticConfig {
    fn default_population_size() -> u32 {
        50
    }
    fn default_max_generations() -> u32 {
        100
    }
    fn default_mutation_rate() -> f64 {
        0.3
    }
    fn default_crossover_rate() -> f64 {
        0.7
    }
    fn default_timeout_secs() -> u32 {
        300
    }
}

impl Default for GeneticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            population_size: Self::default_population_size(),
            max_generations: Self::default_max_generations(),
            mutation_rate: Self::default_mutation_rate(),
            crossover_rate: Self::default_crossover_rate(),
            timeout_secs: Self::default_timeout_secs(),
        }
    }
}

/// Configuration for the hybrid coverage-guided fuzzing phase.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FuzzConfig {
    /// Consecutive no-new-path executions before ending a fuzz phase.
    #[serde(default = "FuzzConfig::default_plateau_threshold")]
    pub plateau_threshold: Option<u32>,

    /// Maximum total executions per fuzz phase.
    #[serde(default = "FuzzConfig::default_max_executions")]
    pub max_executions: Option<u32>,

    /// Wall-clock timeout in seconds per fuzz phase.
    #[serde(default = "FuzzConfig::default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,

    /// Maximum fuzz attempts per branch before giving up (bounded mode).
    /// `None` means unlimited (indefinite mode).
    #[serde(default = "FuzzConfig::default_max_attempts")]
    pub max_attempts: Option<u32>,
}

impl FuzzConfig {
    fn default_plateau_threshold() -> Option<u32> {
        Some(DEFAULT_FUZZ_PLATEAU_THRESHOLD)
    }
    fn default_max_executions() -> Option<u32> {
        Some(DEFAULT_FUZZ_MAX_EXECUTIONS)
    }
    fn default_timeout_seconds() -> Option<u32> {
        Some(DEFAULT_FUZZ_TIMEOUT_SECS)
    }
    fn default_max_attempts() -> Option<u32> {
        Some(DEFAULT_FUZZ_MAX_ATTEMPTS)
    }
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            plateau_threshold: Self::default_plateau_threshold(),
            max_executions: Self::default_max_executions(),
            timeout_seconds: Self::default_timeout_seconds(),
            max_attempts: Self::default_max_attempts(),
        }
    }
}

/// Strategy meta-configuration for adaptive exploration.
///
/// Controls how the [`MetaStrategy`](crate::strategy::MetaStrategy) selects
/// among registered input strategies during exploration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ExplorationConfig {
    /// Enable adaptive strategy scoring. When false (and no strategy_weights),
    /// strategies are selected round-robin.
    #[serde(default = "ExplorationConfig::default_adaptive")]
    pub adaptive: bool,

    /// Sliding window size for outcome-based scoring.
    #[serde(default = "ExplorationConfig::default_score_window")]
    pub score_window: usize,

    /// Minimum candidates a strategy must supply before it can be deprioritized.
    #[serde(default = "ExplorationConfig::default_cold_start")]
    pub cold_start: u64,

    /// Minimum allocation fraction per strategy, 0.0–1.0.
    #[serde(default = "ExplorationConfig::default_strategy_floor")]
    pub strategy_floor: f64,

    /// Optional static weight distribution: strategy name → relative weight.
    /// When set, overrides adaptive scoring with fixed proportional selection.
    #[serde(default)]
    pub strategy_weights: Option<HashMap<String, f64>>,
}

impl ExplorationConfig {
    fn default_adaptive() -> bool {
        DEFAULT_EXPLORATION_ADAPTIVE
    }
    fn default_score_window() -> usize {
        DEFAULT_EXPLORATION_SCORE_WINDOW
    }
    fn default_cold_start() -> u64 {
        DEFAULT_EXPLORATION_COLD_START
    }
    fn default_strategy_floor() -> f64 {
        DEFAULT_EXPLORATION_STRATEGY_FLOOR
    }

    /// Parse a `--strategy-weights` CLI string like `"literals=0.3,random=0.5"`.
    pub fn parse_strategy_weights(s: &str) -> Result<HashMap<String, f64>, ConfigError> {
        let mut map = HashMap::new();
        for pair in s.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (name, weight_str) = pair.split_once('=').ok_or_else(|| {
                ConfigError::InvalidStrategyWeights(format!(
                    "expected 'name=weight' but got '{pair}'"
                ))
            })?;
            let name = name.trim();
            let weight_str = weight_str.trim();
            let weight: f64 = weight_str.parse().map_err(|_| {
                ConfigError::InvalidStrategyWeights(format!(
                    "invalid weight for '{name}': '{weight_str}' is not a number"
                ))
            })?;
            if weight < 0.0 {
                return Err(ConfigError::InvalidStrategyWeights(format!(
                    "weight for '{name}' must be non-negative, got {weight}"
                )));
            }
            map.insert(name.to_string(), weight);
        }
        if map.is_empty() {
            return Err(ConfigError::InvalidStrategyWeights(
                "no strategy weights specified".to_string(),
            ));
        }
        Ok(map)
    }

    /// Convert to the runtime [`MetaConfig`](crate::strategy::MetaConfig).
    pub fn to_meta_config(&self) -> crate::strategy::MetaConfig {
        crate::strategy::MetaConfig {
            window_size: self.score_window,
            cold_start_threshold: self.cold_start,
            floor: self.strategy_floor,
            adaptive: self.adaptive,
            static_weights: self
                .strategy_weights
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect()),
        }
    }
}

impl Default for ExplorationConfig {
    fn default() -> Self {
        Self {
            adaptive: Self::default_adaptive(),
            score_window: Self::default_score_window(),
            cold_start: Self::default_cold_start(),
            strategy_floor: Self::default_strategy_floor(),
            strategy_weights: None,
        }
    }
}

/// Default settings for all functions.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct DefaultsConfig {
    /// Maximum number of iterations per function.
    #[serde(default)]
    pub max_iterations: Option<u32>,

    /// Timeout in seconds per function.
    #[serde(default)]
    pub timeout: Option<u64>,

    /// Path to a setup file, relative to the `.shatter/` directory.
    #[serde(default)]
    pub setup: Option<String>,

    /// Lifecycle level controlling when setup runs (defaults to `Function`).
    #[serde(default)]
    pub setup_level: Option<SetupLevel>,

    /// Timeout in seconds for setup/teardown operations.
    #[serde(default)]
    pub setup_timeout: Option<u64>,

    /// Session-level setup configuration (runs once before any file).
    #[serde(default)]
    pub session_setup: Option<SessionSetupConfig>,

    /// File-level setup: glob pattern → setup file path (relative to `.shatter/`).
    /// Each matching source file gets its own setup invocation at `SetupLevel::File`.
    #[serde(default)]
    pub file_setup: Option<HashMap<String, String>>,

    /// Type-name-to-generator-file mappings (e.g. `"User": "./generators/user.js"`).
    #[serde(default)]
    pub generators: Option<HashMap<String, String>>,

    /// Param-name-to-generator-file mappings (e.g. `"authToken": "./generators/token.js"`).
    #[serde(default)]
    pub param_generators: Option<HashMap<String, String>>,

    /// Per-symbol mock overrides for auto-mocking (e.g. `"db.query": { return_values: [...] }`).
    #[serde(default)]
    pub mocks: Option<HashMap<String, crate::auto_mock::MockOverride>>,

    /// Genetic algorithm explorer settings.
    #[serde(default)]
    pub genetic: Option<GeneticConfig>,

    /// Strategy meta-configuration for adaptive exploration.
    #[serde(default)]
    pub exploration: Option<ExplorationConfig>,

    /// Hybrid coverage-guided fuzzing phase settings.
    #[serde(default)]
    pub fuzz: Option<FuzzConfig>,

    /// Ordered opaque execution adapter descriptors for this target family.
    #[serde(default)]
    pub execution_profile: Option<ExecutionProfile>,
}

/// Per-function configuration, matched by glob pattern against function identifiers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct FunctionConfig {
    /// Maximum iterations for this function.
    #[serde(default)]
    pub max_iterations: Option<u32>,

    /// Timeout in seconds for this function.
    #[serde(default)]
    pub timeout: Option<u64>,

    /// Path to a candidate inputs JSON file, relative to the `.shatter/` directory.
    #[serde(default)]
    pub inputs: Option<String>,

    /// Skip this function entirely.
    #[serde(default)]
    pub skip: Option<bool>,

    /// Path to a setup file, relative to the `.shatter/` directory.
    #[serde(default)]
    pub setup: Option<String>,

    /// Lifecycle level controlling when setup runs (overrides default).
    #[serde(default)]
    pub setup_level: Option<SetupLevel>,

    /// Timeout in seconds for setup/teardown operations (overrides default).
    #[serde(default)]
    pub setup_timeout: Option<u64>,

    /// Type-name-to-generator-file mappings, overriding defaults.
    #[serde(default)]
    pub generators: Option<HashMap<String, String>>,

    /// Param-name-to-generator-file mappings, overriding defaults.
    #[serde(default)]
    pub param_generators: Option<HashMap<String, String>>,

    /// Per-symbol mock overrides, overriding defaults.
    #[serde(default)]
    pub mocks: Option<HashMap<String, crate::auto_mock::MockOverride>>,

    /// Genetic algorithm explorer settings, overriding defaults.
    #[serde(default)]
    pub genetic: Option<GeneticConfig>,

    /// Strategy meta-configuration, overriding defaults.
    #[serde(default)]
    pub exploration: Option<ExplorationConfig>,

    /// Hybrid coverage-guided fuzzing phase settings, overriding defaults.
    #[serde(default)]
    pub fuzz: Option<FuzzConfig>,

    /// Ordered opaque execution adapter descriptors, overriding defaults.
    #[serde(default)]
    pub execution_profile: Option<ExecutionProfile>,
}

/// A single candidate input for a function.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CandidateInput {
    /// The argument values to pass to the function.
    pub args: Vec<serde_json::Value>,

    /// Optional human-readable label describing this input.
    #[serde(default)]
    pub label: Option<String>,
}

/// A user declaration that a specific field is (or is not) nondeterministic.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct NondeterminismDeclaration {
    /// Function name or pattern (e.g. `"createUser"`).
    pub function: String,
    /// JSONPath to the field (e.g. `"$.id"`).
    pub path: String,
    /// Human-readable explanation of why this field is/isn't nondeterministic.
    pub reason: String,
}

/// User-declared nondeterminism configuration.
///
/// `confirmed` entries become `NondeterminismEvidence::UserDeclared` (highest precedence).
/// `rejected` entries suppress re-flagging by heuristics.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NondeterminismConfig {
    #[serde(default)]
    pub confirmed: Vec<NondeterminismDeclaration>,
    #[serde(default)]
    pub rejected: Vec<NondeterminismDeclaration>,
}

/// Resolved configuration for a specific function, after merging hierarchical configs.
#[derive(Debug, Clone)]
pub struct ResolvedFunctionConfig {
    /// Maximum iterations. `None` means unbounded (run until timeout/interrupt).
    pub max_iterations: Option<u32>,

    /// Timeout in seconds (from config or CLI default).
    pub timeout: u64,

    /// Whether to skip this function.
    pub skip: bool,

    /// User-provided candidate inputs, if any.
    pub candidate_inputs: Vec<CandidateInput>,

    /// Resolved absolute path to the setup file, if any.
    pub setup: Option<PathBuf>,

    /// Lifecycle level controlling when setup runs (defaults to `Function`).
    pub setup_level: SetupLevel,

    /// Timeout in seconds for setup/teardown operations.
    pub setup_timeout: u64,

    /// Session-level setup configuration, if any.
    pub session_setup: Option<SessionSetupConfig>,

    /// File-level setup: glob pattern → resolved absolute path to setup file.
    pub file_setup: HashMap<String, PathBuf>,

    /// Merged type-name-to-generator-file mappings (absolute paths).
    pub generators: HashMap<String, PathBuf>,

    /// Merged param-name-to-generator-file mappings (absolute paths).
    pub param_generators: HashMap<String, PathBuf>,

    /// Merged per-symbol mock overrides for auto-mocking.
    pub mock_overrides: HashMap<String, crate::auto_mock::MockOverride>,

    /// Resolved strategy meta-configuration.
    pub exploration: ExplorationConfig,

    /// Resolved genetic algorithm configuration.
    pub genetic: GeneticConfig,

    /// Resolved hybrid fuzzing configuration.
    pub fuzz: FuzzConfig,

    /// Opaque execution profile to pass through to the frontend, if any.
    pub execution_profile: Option<ExecutionProfile>,
}

/// A config file found during hierarchical discovery, paired with its directory.
#[derive(Debug, Clone)]
struct DiscoveredConfig {
    /// The `.shatter/` directory containing this config.
    shatter_dir: PathBuf,
    /// The parsed config.
    config: ShatterConfig,
}

/// Build a synthetic [`ShatterConfig`] from `--set key=value` pairs.
///
/// Each pair must be `key=value` where `key` is a dotted YAML path matching
/// the `ShatterConfig` structure (e.g. `defaults.max_iterations`,
/// `defaults.exploration.adaptive`). Values are parsed as YAML scalars so
/// integers, floats, booleans, and strings all work naturally.
///
/// The returned config is intended to be prepended to the list of discovered
/// configs before calling [`merge_configs`], giving `--set` overrides the
/// highest YAML-layer priority.
pub fn parse_set_overrides(set_pairs: &[String]) -> Result<ShatterConfig, ConfigError> {
    if set_pairs.is_empty() {
        return Ok(ShatterConfig::default());
    }
    let mut root = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    for pair in set_pairs {
        let (key, val_str) =
            pair.split_once('=')
                .ok_or_else(|| ConfigError::InvalidSetOverride {
                    pair: pair.clone(),
                    reason: "missing '=' separator".to_string(),
                })?;
        let segments: Vec<&str> = key.split('.').collect();
        if segments.iter().any(|s| s.is_empty()) {
            return Err(ConfigError::InvalidSetOverride {
                pair: pair.clone(),
                reason: "empty path segment in key".to_string(),
            });
        }
        let val: serde_yaml::Value =
            serde_yaml::from_str(val_str).map_err(|e| ConfigError::InvalidSetOverride {
                pair: pair.clone(),
                reason: format!("invalid YAML value: {e}"),
            })?;
        set_dotted_path(&mut root, &segments, val).map_err(|reason| {
            ConfigError::InvalidSetOverride {
                pair: pair.clone(),
                reason,
            }
        })?;
    }
    serde_yaml::from_value(root).map_err(|e| ConfigError::SetOverrideDeserialize { source: e })
}

/// Navigate `root` (a YAML mapping) along `path`, creating intermediate
/// mappings as needed, and set the leaf to `val`.
fn set_dotted_path(
    root: &mut serde_yaml::Value,
    path: &[&str],
    val: serde_yaml::Value,
) -> Result<(), String> {
    let mut current = root;
    for &segment in &path[..path.len() - 1] {
        let key = serde_yaml::Value::String(segment.to_string());
        let mapping = current
            .as_mapping_mut()
            .ok_or_else(|| format!("path segment '{segment}' is not a mapping"))?;
        current = mapping
            .entry(key)
            .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let leaf = serde_yaml::Value::String(path.last().unwrap().to_string());
    current
        .as_mapping_mut()
        .ok_or_else(|| "leaf parent is not a mapping".to_string())?
        .insert(leaf, val);
    Ok(())
}

/// Parse a `.shatter/config.yaml` file.
pub fn parse_config(path: &Path) -> Result<ShatterConfig, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let config: ShatterConfig =
        serde_yaml::from_str(&contents).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(config)
}

/// Update the nondeterminism section of a `.shatter/config.yaml` file.
///
/// Merges `new_confirmed` and `new_rejected` entries into the existing config,
/// deduplicating by `(function, path)`. Writes the result back atomically
/// (temp file + rename). Creates the config file if it does not exist.
///
/// Only the `nondeterminism` section is mutated; all other config is preserved.
pub fn update_nondeterminism_config(
    config_path: &Path,
    new_confirmed: &[NondeterminismDeclaration],
    new_rejected: &[NondeterminismDeclaration],
) -> Result<(), ConfigError> {
    // Load existing config, or start from default if the file does not exist.
    let mut config = if config_path.exists() {
        parse_config(config_path)?
    } else {
        ShatterConfig::default()
    };

    let nd = config
        .nondeterminism
        .get_or_insert_with(NondeterminismConfig::default);

    // Merge confirmed: add entries not already present (keyed by function + path).
    for decl in new_confirmed {
        if !nd
            .confirmed
            .iter()
            .any(|d| d.function == decl.function && d.path == decl.path)
        {
            nd.confirmed.push(decl.clone());
        }
    }

    // Merge rejected: add entries not already present.
    for decl in new_rejected {
        if !nd
            .rejected
            .iter()
            .any(|d| d.function == decl.function && d.path == decl.path)
        {
            nd.rejected.push(decl.clone());
        }
    }

    // Remove the nondeterminism section entirely if both lists are empty
    // (avoids writing an empty `nondeterminism: {confirmed: [], rejected: []}` block).
    if nd.confirmed.is_empty() && nd.rejected.is_empty() {
        config.nondeterminism = None;
    }

    // Serialize and write atomically.
    let yaml = serde_yaml::to_string(&config).map_err(|e| ConfigError::Parse {
        path: config_path.to_path_buf(),
        source: e,
    })?;

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
            path: config_path.to_path_buf(),
            source: e,
        })?;
    }

    let tmp_path = config_path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, &yaml).map_err(|e| ConfigError::Io {
        path: tmp_path.clone(),
        source: e,
    })?;
    std::fs::rename(&tmp_path, config_path).map_err(|e| ConfigError::Io {
        path: config_path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

/// Parse a candidate inputs JSON file.
pub fn parse_candidate_inputs(path: &Path) -> Result<Vec<CandidateInput>, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::InputIo {
        path: path.to_path_buf(),
        source: e,
    })?;
    let inputs: Vec<CandidateInput> =
        serde_json::from_str(&contents).map_err(|e| ConfigError::InputParse {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(inputs)
}

/// Discover `.shatter/config.yaml` files by walking upward from `start_dir`.
///
/// Returns configs ordered from nearest (highest priority) to farthest (lowest priority).
pub fn discover_configs(start_dir: &Path) -> Result<Vec<ShatterConfig>, ConfigError> {
    let discovered = discover_configs_with_paths(start_dir)?;
    Ok(discovered.into_iter().map(|d| d.config).collect())
}

/// Internal: discover configs with their `.shatter/` directory paths preserved.
fn discover_configs_with_paths(start_dir: &Path) -> Result<Vec<DiscoveredConfig>, ConfigError> {
    let mut configs = Vec::new();
    let mut current = Some(start_dir.to_path_buf());

    while let Some(dir) = current {
        let shatter_dir = dir.join(".shatter");
        let config_path = shatter_dir.join("config.yaml");

        if config_path.is_file() {
            let config = parse_config(&config_path)?;
            configs.push(DiscoveredConfig {
                shatter_dir,
                config,
            });
        }

        current = dir.parent().map(Path::to_path_buf);
    }

    Ok(configs)
}

/// Merge multiple configs where the first config in the slice has highest priority.
///
/// For defaults, the nearest non-None value wins.
/// For function patterns, the nearest matching pattern wins.
pub fn merge_configs(configs: &[ShatterConfig]) -> ShatterConfig {
    if configs.is_empty() {
        return ShatterConfig::default();
    }
    if configs.len() == 1 {
        return configs[0].clone();
    }

    // Merge defaults: nearest non-None wins.
    let mut max_iterations = None;
    let mut timeout = None;
    let mut setup = None;
    let mut setup_level = None;
    let mut setup_timeout = None;
    let mut session_setup = None;
    let mut execution_profile = None;
    let mut exploration = None;
    let mut genetic = None;
    let mut fuzz = None;

    // Merge generators and file_setup maps: start from farthest, overlay nearer.
    // This lets a near config override specific keys while inheriting the rest.
    let mut generators: Option<HashMap<String, String>> = None;
    let mut param_generators: Option<HashMap<String, String>> = None;
    let mut file_setup: Option<HashMap<String, String>> = None;
    let mut mocks: Option<HashMap<String, crate::auto_mock::MockOverride>> = None;

    for config in configs.iter().rev() {
        if let Some(ref g) = config.defaults.generators {
            generators
                .get_or_insert_with(HashMap::new)
                .extend(g.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(ref pg) = config.defaults.param_generators {
            param_generators
                .get_or_insert_with(HashMap::new)
                .extend(pg.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(ref fs) = config.defaults.file_setup {
            file_setup
                .get_or_insert_with(HashMap::new)
                .extend(fs.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(ref mock_overrides) = config.defaults.mocks {
            mocks
                .get_or_insert_with(HashMap::new)
                .extend(mock_overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    for config in configs {
        if max_iterations.is_none() {
            max_iterations = config.defaults.max_iterations;
        }
        if timeout.is_none() {
            timeout = config.defaults.timeout;
        }
        if setup.is_none() {
            setup = config.defaults.setup.clone();
        }
        if setup_level.is_none() {
            setup_level = config.defaults.setup_level;
        }
        if setup_timeout.is_none() {
            setup_timeout = config.defaults.setup_timeout;
        }
        if session_setup.is_none() {
            session_setup = config.defaults.session_setup.clone();
        }
        if execution_profile.is_none() {
            execution_profile = config.defaults.execution_profile.clone();
        }
        if exploration.is_none() {
            exploration = config.defaults.exploration.clone();
        }
        if genetic.is_none() {
            genetic = config.defaults.genetic.clone();
        }
        if fuzz.is_none() {
            fuzz = config.defaults.fuzz.clone();
        }
    }

    // Merge function configs: collect all, nearest first (already in order).
    // Later entries only fill in if the pattern hasn't been seen.
    let mut functions: HashMap<String, FunctionConfig> = HashMap::new();
    for config in configs {
        for (pattern, func_config) in &config.functions {
            functions
                .entry(pattern.clone())
                .or_insert_with(|| func_config.clone());
        }
    }

    let mut opaque_types: Vec<CustomOpaqueType> = Vec::new();
    for config in configs {
        for t in &config.opaque_types {
            // Deduplicate by name — nearest config wins on duplicates.
            if !opaque_types
                .iter()
                .any(|existing| existing.name() == t.name())
            {
                opaque_types.push(t.clone());
            }
        }
    }

    // Merge nondeterminism declarations: union all confirmed/rejected, dedup by (function, path).
    let mut confirmed = Vec::<NondeterminismDeclaration>::new();
    let mut rejected = Vec::<NondeterminismDeclaration>::new();
    let mut has_any = false;
    for config in configs {
        if let Some(ref nd) = config.nondeterminism {
            has_any = true;
            for decl in &nd.confirmed {
                if !confirmed
                    .iter()
                    .any(|d| d.function == decl.function && d.path == decl.path)
                {
                    confirmed.push(decl.clone());
                }
            }
            for decl in &nd.rejected {
                if !rejected
                    .iter()
                    .any(|d| d.function == decl.function && d.path == decl.path)
                {
                    rejected.push(decl.clone());
                }
            }
        }
    }
    let nondeterminism = if has_any {
        Some(NondeterminismConfig {
            confirmed,
            rejected,
        })
    } else {
        None
    };

    // Mock fixtures: nearest non-None wins (no deep merge for now).
    let mock_fixtures = configs.iter().find_map(|c| c.mock_fixtures.clone());

    ShatterConfig {
        defaults: DefaultsConfig {
            max_iterations,
            timeout,
            setup,
            setup_level,
            setup_timeout,
            session_setup,
            file_setup,
            generators,
            param_generators,
            mocks,
            genetic,
            exploration,
            fuzz,
            execution_profile,
        },
        functions,
        opaque_types,
        nondeterminism,
        mock_fixtures,
    }
}

/// Resolve the effective configuration for a specific function identifier.
///
/// `function_id` is typically in `"file:function"` format.
/// `configs` should be ordered nearest-first (as returned by `discover_configs`).
/// `cli_max_iterations` and `cli_timeout` are the CLI-provided defaults.
pub fn resolve_function_config(
    function_id: &str,
    configs: &[ShatterConfig],
    cli_max_iterations: Option<u32>,
    cli_timeout: u64,
) -> Result<ResolvedFunctionConfig, ConfigError> {
    let merged = merge_configs(configs);
    resolve_from_merged(function_id, &merged, cli_max_iterations, cli_timeout)
}

/// Resolve config for a function from an already-merged config.
fn resolve_from_merged(
    function_id: &str,
    config: &ShatterConfig,
    cli_max_iterations: Option<u32>,
    cli_timeout: u64,
) -> Result<ResolvedFunctionConfig, ConfigError> {
    // Find the first matching function pattern.
    let mut func_config: Option<&FunctionConfig> = None;

    for (pattern, fc) in &config.functions {
        let matcher = compile_pattern(pattern)?;
        if matcher.is_match(function_id) {
            func_config = Some(fc);
            break;
        }
    }

    // Resolution order: function config > defaults > CLI flags.
    // Any layer that provides a concrete value wins; if all are None, stays unbounded.
    let max_iterations = func_config
        .and_then(|fc| fc.max_iterations)
        .or(config.defaults.max_iterations)
        .or(cli_max_iterations);

    let timeout = func_config
        .and_then(|fc| fc.timeout)
        .or(config.defaults.timeout)
        .unwrap_or(cli_timeout);

    let skip = func_config.and_then(|fc| fc.skip).unwrap_or(false);

    let setup_level = func_config
        .and_then(|fc| fc.setup_level)
        .or(config.defaults.setup_level)
        .unwrap_or(SetupLevel::Function);

    let setup_timeout = func_config
        .and_then(|fc| fc.setup_timeout)
        .or(config.defaults.setup_timeout)
        .unwrap_or(DEFAULT_SETUP_TIMEOUT_SECS);

    // Merge mock overrides: defaults first, then function-level overrides on top.
    let mut mock_overrides: HashMap<String, crate::auto_mock::MockOverride> =
        config.defaults.mocks.clone().unwrap_or_default();
    if let Some(fc) = func_config
        && let Some(ref func_mocks) = fc.mocks
    {
        for (k, v) in func_mocks {
            mock_overrides.insert(k.clone(), v.clone());
        }
    }

    // Resolve exploration config: function > defaults > built-in defaults.
    let exploration = func_config
        .and_then(|fc| fc.exploration.clone())
        .or_else(|| config.defaults.exploration.clone())
        .unwrap_or_default();

    // Resolve genetic config: function > defaults > built-in defaults.
    let genetic = func_config
        .and_then(|fc| fc.genetic.clone())
        .or_else(|| config.defaults.genetic.clone())
        .unwrap_or_default();

    // Resolve fuzz config: function > defaults > built-in defaults.
    let fuzz = func_config
        .and_then(|fc| fc.fuzz.clone())
        .or_else(|| config.defaults.fuzz.clone())
        .unwrap_or_default();

    let execution_profile = func_config
        .and_then(|fc| fc.execution_profile.clone())
        .or_else(|| config.defaults.execution_profile.clone());

    Ok(ResolvedFunctionConfig {
        max_iterations,
        timeout,
        skip,
        candidate_inputs: Vec::new(),
        // Setup and generator paths are resolved to absolute paths by
        // resolve_function_config_with_inputs, which has access to .shatter/ dirs.
        setup: None,
        setup_level,
        setup_timeout,
        session_setup: config.defaults.session_setup.clone(),
        file_setup: HashMap::new(),
        generators: HashMap::new(),
        param_generators: HashMap::new(),
        mock_overrides,
        exploration,
        genetic,
        fuzz,
        execution_profile,
    })
}

/// Resolve config for a function and load candidate inputs if specified.
///
/// `shatter_dir` is the `.shatter/` directory used to resolve relative input paths.
/// If `explicit_inputs` is provided (from `--inputs` CLI flag), it takes precedence.
///
/// `set_overrides` contains `key=value` pairs from the CLI `--set` flag. They are
/// applied as the highest-priority YAML layer (above any `.shatter/config.yaml` files)
/// but below dedicated CLI flag defaults (`cli_max_iterations`, `cli_timeout`).
pub fn resolve_function_config_with_inputs(
    function_id: &str,
    start_dir: &Path,
    explicit_inputs: Option<&Path>,
    cli_max_iterations: Option<u32>,
    cli_timeout: u64,
    set_overrides: &[String],
) -> Result<ResolvedFunctionConfig, ConfigError> {
    let discovered = discover_configs_with_paths(start_dir)?;
    let mut configs: Vec<ShatterConfig> = discovered.iter().map(|d| d.config.clone()).collect();
    if !set_overrides.is_empty() {
        let set_config = parse_set_overrides(set_overrides)?;
        configs.insert(0, set_config);
    }
    let merged = merge_configs(&configs);

    let mut resolved = resolve_from_merged(function_id, &merged, cli_max_iterations, cli_timeout)?;

    // Load candidate inputs.
    if let Some(inputs_path) = explicit_inputs {
        resolved.candidate_inputs = parse_candidate_inputs(inputs_path)?;
    } else {
        // Check function config for an inputs path.
        for dc in &discovered {
            for (pattern, fc) in &dc.config.functions {
                let matcher = compile_pattern(pattern)?;
                if matcher.is_match(function_id)
                    && let Some(inputs_rel) = &fc.inputs
                {
                    let inputs_path = dc.shatter_dir.join(inputs_rel);
                    if inputs_path.is_file() {
                        resolved.candidate_inputs = parse_candidate_inputs(&inputs_path)?;
                    }
                    break;
                }
            }
        }
    }

    // Resolve setup file path: function-level > defaults, nearest config wins.
    for dc in &discovered {
        if resolved.setup.is_some() {
            break;
        }
        for (pattern, fc) in &dc.config.functions {
            let matcher = compile_pattern(pattern)?;
            if matcher.is_match(function_id)
                && let Some(setup_rel) = &fc.setup
            {
                resolved.setup = Some(dc.shatter_dir.join(setup_rel));
                break;
            }
        }
        if resolved.setup.is_none()
            && let Some(setup_rel) = &dc.config.defaults.setup
        {
            resolved.setup = Some(dc.shatter_dir.join(setup_rel));
        }
    }

    // Resolve file_setup: walk farthest-to-nearest so nearer configs override.
    for dc in discovered.iter().rev() {
        if let Some(ref fs) = dc.config.defaults.file_setup {
            for (glob_pattern, setup_rel) in fs {
                resolved
                    .file_setup
                    .insert(glob_pattern.clone(), dc.shatter_dir.join(setup_rel));
            }
        }
    }

    // Resolve generators: merge defaults then overlay function-level.
    // Walk farthest-to-nearest so nearer configs override.
    for dc in discovered.iter().rev() {
        if let Some(ref g) = dc.config.defaults.generators {
            for (type_name, gen_rel) in g {
                resolved
                    .generators
                    .insert(type_name.clone(), dc.shatter_dir.join(gen_rel));
            }
        }
        if let Some(ref pg) = dc.config.defaults.param_generators {
            for (param_name, gen_rel) in pg {
                resolved
                    .param_generators
                    .insert(param_name.clone(), dc.shatter_dir.join(gen_rel));
            }
        }
    }
    // Function-level generators overlay defaults (nearest matching function wins).
    for dc in &discovered {
        for (pattern, fc) in &dc.config.functions {
            let matcher = compile_pattern(pattern)?;
            if matcher.is_match(function_id) {
                if let Some(ref g) = fc.generators {
                    for (type_name, gen_rel) in g {
                        resolved
                            .generators
                            .insert(type_name.clone(), dc.shatter_dir.join(gen_rel));
                    }
                }
                if let Some(ref pg) = fc.param_generators {
                    for (param_name, gen_rel) in pg {
                        resolved
                            .param_generators
                            .insert(param_name.clone(), dc.shatter_dir.join(gen_rel));
                    }
                }
                break;
            }
        }
    }

    Ok(resolved)
}

fn compile_pattern(pattern: &str) -> Result<GlobMatcher, ConfigError> {
    let glob = Glob::new(pattern).map_err(|e| ConfigError::InvalidPattern {
        pattern: pattern.to_string(),
        source: e,
    })?;
    Ok(glob.compile_matcher())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ExecutionAdapter, ExecutionAdapterApply};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_config_basic() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults:
  max_iterations: 200
  timeout: 30

functions:
  "src/auth.ts:validateToken":
    max_iterations: 500
    timeout: 120
    inputs: ./inputs/validateToken/candidates.json
  "src/generated/**":
    skip: true
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();
        assert_eq!(config.defaults.max_iterations, Some(200));
        assert_eq!(config.defaults.timeout, Some(30));
        assert_eq!(config.functions.len(), 2);

        let auth_config = &config.functions["src/auth.ts:validateToken"];
        assert_eq!(auth_config.max_iterations, Some(500));
        assert_eq!(auth_config.timeout, Some(120));
        assert_eq!(
            auth_config.inputs.as_deref(),
            Some("./inputs/validateToken/candidates.json")
        );

        let gen_config = &config.functions["src/generated/**"];
        assert_eq!(gen_config.skip, Some(true));
    }

    #[test]
    fn parse_config_minimal() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(&config_path, "defaults: {}\n").unwrap();

        let config = parse_config(&config_path).unwrap();
        assert_eq!(config.defaults.max_iterations, None);
        assert_eq!(config.defaults.timeout, None);
        assert!(config.functions.is_empty());
    }

    #[test]
    fn parse_config_missing_file_returns_error() {
        let result = parse_config(Path::new("/nonexistent/config.yaml"));
        assert!(matches!(result, Err(ConfigError::Io { .. })));
    }

    #[test]
    fn parse_config_invalid_yaml_returns_error() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(&config_path, "not: [valid: yaml: {{").unwrap();

        let result = parse_config(&config_path);
        assert!(matches!(result, Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn parse_candidate_inputs_basic() {
        let dir = TempDir::new().unwrap();
        let inputs_path = dir.path().join("candidates.json");
        fs::write(
            &inputs_path,
            r#"[
  { "args": [42, "hello"], "label": "typical usage" },
  { "args": [-1, ""], "label": "edge: negative with empty" }
]"#,
        )
        .unwrap();

        let inputs = parse_candidate_inputs(&inputs_path).unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(
            inputs[0].args,
            vec![serde_json::json!(42), serde_json::json!("hello")]
        );
        assert_eq!(inputs[0].label.as_deref(), Some("typical usage"));
        assert_eq!(
            inputs[1].args,
            vec![serde_json::json!(-1), serde_json::json!("")]
        );
    }

    #[test]
    fn parse_candidate_inputs_without_labels() {
        let dir = TempDir::new().unwrap();
        let inputs_path = dir.path().join("candidates.json");
        fs::write(&inputs_path, r#"[{ "args": [1, 2, 3] }]"#).unwrap();

        let inputs = parse_candidate_inputs(&inputs_path).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].label, None);
    }

    #[test]
    fn parse_candidate_inputs_missing_file() {
        let result = parse_candidate_inputs(Path::new("/nonexistent/inputs.json"));
        assert!(matches!(result, Err(ConfigError::InputIo { .. })));
    }

    #[test]
    fn parse_candidate_inputs_invalid_json() {
        let dir = TempDir::new().unwrap();
        let inputs_path = dir.path().join("candidates.json");
        fs::write(&inputs_path, "not json").unwrap();

        let result = parse_candidate_inputs(&inputs_path);
        assert!(matches!(result, Err(ConfigError::InputParse { .. })));
    }

    #[test]
    fn discover_configs_finds_nearest_first() {
        let root = TempDir::new().unwrap();

        // Create root-level .shatter/config.yaml
        let root_shatter = root.path().join(".shatter");
        fs::create_dir_all(&root_shatter).unwrap();
        fs::write(
            root_shatter.join("config.yaml"),
            "defaults:\n  max_iterations: 50\n",
        )
        .unwrap();

        // Create nested .shatter/config.yaml
        let sub = root.path().join("src").join("auth");
        fs::create_dir_all(&sub).unwrap();
        let sub_shatter = sub.join(".shatter");
        fs::create_dir_all(&sub_shatter).unwrap();
        fs::write(
            sub_shatter.join("config.yaml"),
            "defaults:\n  max_iterations: 500\n",
        )
        .unwrap();

        let configs = discover_configs(&sub).unwrap();
        assert_eq!(configs.len(), 2);
        // Nearest first
        assert_eq!(configs[0].defaults.max_iterations, Some(500));
        assert_eq!(configs[1].defaults.max_iterations, Some(50));
    }

    #[test]
    fn discover_configs_empty_when_no_shatter_dirs() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("some").join("path");
        fs::create_dir_all(&sub).unwrap();

        let configs = discover_configs(&sub).unwrap();
        assert!(configs.is_empty());
    }

    #[test]
    fn merge_configs_nearest_wins_for_defaults() {
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(500),
                timeout: None,
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(50),
                timeout: Some(120),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        assert_eq!(merged.defaults.max_iterations, Some(500)); // nearest wins
        assert_eq!(merged.defaults.timeout, Some(120)); // falls through to far
    }

    #[test]
    fn merge_configs_mocks_near_overrides_far() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                mocks: Some(HashMap::from([
                    (
                        "db.query".to_string(),
                        crate::auto_mock::MockOverride {
                            return_values: Some(vec![serde_json::json!({"rows": [1]})]),
                            behavior: Some(crate::protocol::MockBehavior::RepeatLast),
                        },
                    ),
                    (
                        "email.send".to_string(),
                        crate::auto_mock::MockOverride {
                            return_values: Some(vec![serde_json::json!({"accepted": true})]),
                            behavior: Some(crate::protocol::MockBehavior::Passthrough),
                        },
                    ),
                ])),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                mocks: Some(HashMap::from([
                    (
                        "db.query".to_string(),
                        crate::auto_mock::MockOverride {
                            return_values: Some(vec![serde_json::json!({"rows": [2]})]),
                            behavior: Some(crate::protocol::MockBehavior::ThrowError),
                        },
                    ),
                    (
                        "cache.get".to_string(),
                        crate::auto_mock::MockOverride {
                            return_values: Some(vec![serde_json::json!("hit")]),
                            behavior: Some(crate::protocol::MockBehavior::RepeatLast),
                        },
                    ),
                ])),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let mocks = merged.defaults.mocks.expect("merged mock defaults");

        assert_eq!(mocks.len(), 3);
        assert_eq!(
            mocks.get("db.query"),
            Some(&crate::auto_mock::MockOverride {
                return_values: Some(vec![serde_json::json!({"rows": [2]})]),
                behavior: Some(crate::protocol::MockBehavior::ThrowError),
            })
        );
        assert_eq!(
            mocks.get("email.send"),
            Some(&crate::auto_mock::MockOverride {
                return_values: Some(vec![serde_json::json!({"accepted": true})]),
                behavior: Some(crate::protocol::MockBehavior::Passthrough),
            })
        );
        assert_eq!(
            mocks.get("cache.get"),
            Some(&crate::auto_mock::MockOverride {
                return_values: Some(vec![serde_json::json!("hit")]),
                behavior: Some(crate::protocol::MockBehavior::RepeatLast),
            })
        );
    }

    #[test]
    fn merge_configs_nearest_wins_for_function_patterns() {
        let mut near_funcs = HashMap::new();
        near_funcs.insert(
            "src/auth.ts:*".to_string(),
            FunctionConfig {
                max_iterations: Some(1000),
                timeout: None,
                inputs: None,
                skip: None,
                setup: None,
                setup_level: None,
                setup_timeout: None,
                generators: None,
                param_generators: None,
                mocks: None,
                genetic: None,
                exploration: None,
                fuzz: None,
                execution_profile: None,
            },
        );

        let mut far_funcs = HashMap::new();
        far_funcs.insert(
            "src/auth.ts:*".to_string(),
            FunctionConfig {
                max_iterations: Some(50),
                timeout: Some(10),
                inputs: None,
                skip: None,
                setup: None,
                setup_level: None,
                setup_timeout: None,
                generators: None,
                param_generators: None,
                mocks: None,
                genetic: None,
                exploration: None,
                fuzz: None,
                execution_profile: None,
            },
        );

        let near = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions: near_funcs,
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions: far_funcs,
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let auth = &merged.functions["src/auth.ts:*"];
        assert_eq!(auth.max_iterations, Some(1000)); // nearest wins
    }

    #[test]
    fn merge_configs_empty_returns_default() {
        let merged = merge_configs(&[]);
        assert_eq!(merged, ShatterConfig::default());
    }

    #[test]
    fn merge_configs_nearest_wins_for_execution_profile() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                execution_profile: Some(ExecutionProfile {
                    adapters: vec![ExecutionAdapter {
                        id: "ts/browser-dom".to_string(),
                        apply: Some(ExecutionAdapterApply::Suggest),
                        options: Some(serde_json::json!({"impl": "happy-dom"})),
                    }],
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                execution_profile: Some(ExecutionProfile {
                    adapters: vec![
                        ExecutionAdapter {
                            id: "ts/module-resolution/tsconfig-paths".to_string(),
                            apply: Some(ExecutionAdapterApply::Auto),
                            options: None,
                        },
                        ExecutionAdapter {
                            id: "ts/react-hooks".to_string(),
                            apply: Some(ExecutionAdapterApply::Suggest),
                            options: Some(serde_json::json!({"mode": "callable_return"})),
                        },
                    ],
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let profile = merged
            .defaults
            .execution_profile
            .expect("execution profile should be present");
        assert_eq!(profile.adapters.len(), 2);
        assert_eq!(
            profile.adapters[0].id,
            "ts/module-resolution/tsconfig-paths"
        );
        assert_eq!(profile.adapters[1].id, "ts/react-hooks");
        assert_eq!(
            profile.adapters[1].options,
            Some(serde_json::json!({"mode": "callable_return"}))
        );
    }

    #[test]
    fn resolve_function_config_uses_function_override() {
        let mut functions = HashMap::new();
        functions.insert(
            "src/auth.ts:validateToken".to_string(),
            FunctionConfig {
                max_iterations: Some(500),
                timeout: Some(120),
                inputs: None,
                skip: None,
                setup: None,
                setup_level: None,
                setup_timeout: None,
                generators: None,
                param_generators: None,
                mocks: None,
                genetic: None,
                exploration: None,
                fuzz: None,
                execution_profile: None,
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(100),
                timeout: Some(60),
                ..DefaultsConfig::default()
            },
            functions,
            ..ShatterConfig::default()
        };

        let resolved =
            resolve_function_config("src/auth.ts:validateToken", &[config], Some(50), 30).unwrap();
        assert_eq!(resolved.max_iterations, Some(500));
        assert_eq!(resolved.timeout, 120);
        assert!(!resolved.skip);
    }

    #[test]
    fn resolve_function_config_prefers_function_execution_profile_over_defaults() {
        let mut functions = HashMap::new();
        functions.insert(
            "src/auth.ts:validateToken".to_string(),
            FunctionConfig {
                max_iterations: None,
                timeout: None,
                inputs: None,
                skip: None,
                setup: None,
                setup_level: None,
                setup_timeout: None,
                generators: None,
                param_generators: None,
                mocks: None,
                genetic: None,
                exploration: None,
                fuzz: None,
                execution_profile: Some(ExecutionProfile {
                    adapters: vec![ExecutionAdapter {
                        id: "ts/react-hooks".to_string(),
                        apply: Some(ExecutionAdapterApply::Required),
                        options: Some(serde_json::json!({"providers": ["teamStore"]})),
                    }],
                }),
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig {
                execution_profile: Some(ExecutionProfile {
                    adapters: vec![ExecutionAdapter {
                        id: "ts/browser-dom".to_string(),
                        apply: Some(ExecutionAdapterApply::Auto),
                        options: None,
                    }],
                }),
                ..DefaultsConfig::default()
            },
            functions,
            ..ShatterConfig::default()
        };

        let resolved =
            resolve_function_config("src/auth.ts:validateToken", &[config], Some(50), 30).unwrap();
        let profile = resolved
            .execution_profile
            .expect("resolved execution profile should be present");
        assert_eq!(profile.adapters.len(), 1);
        assert_eq!(profile.adapters[0].id, "ts/react-hooks");
        assert_eq!(
            profile.adapters[0].apply,
            Some(ExecutionAdapterApply::Required)
        );
    }

    #[test]
    fn resolve_function_config_falls_through_to_defaults() {
        let config = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(200),
                timeout: None,
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };

        let resolved = resolve_function_config("some/func", &[config], Some(50), 30).unwrap();
        assert_eq!(resolved.max_iterations, Some(200)); // from config defaults
        assert_eq!(resolved.timeout, 30); // from CLI (config default is None)
    }

    #[test]
    fn resolve_function_config_uses_cli_defaults_when_no_config() {
        let resolved = resolve_function_config("any/func", &[], Some(100), 60).unwrap();
        assert_eq!(resolved.max_iterations, Some(100));
        assert_eq!(resolved.timeout, 60);
    }

    #[test]
    fn resolve_function_config_glob_pattern_matching() {
        let mut functions = HashMap::new();
        functions.insert(
            "src/generated/**".to_string(),
            FunctionConfig {
                max_iterations: None,
                timeout: None,
                inputs: None,
                skip: Some(true),
                setup: None,
                setup_level: None,
                setup_timeout: None,
                generators: None,
                param_generators: None,
                mocks: None,
                genetic: None,
                exploration: None,
                fuzz: None,
                execution_profile: None,
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions,
            ..ShatterConfig::default()
        };

        let resolved = resolve_function_config(
            "src/generated/api.ts:handler",
            std::slice::from_ref(&config),
            Some(100),
            60,
        )
        .unwrap();
        assert!(resolved.skip);

        let resolved2 =
            resolve_function_config("src/auth.ts:login", &[config], Some(100), 60).unwrap();
        assert!(!resolved2.skip);
    }

    #[test]
    fn resolve_function_config_with_inputs_loads_candidates() {
        let root = TempDir::new().unwrap();

        // Create .shatter/ directory with config and inputs
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();
        let inputs_dir = shatter_dir.join("inputs");
        fs::create_dir_all(&inputs_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  max_iterations: 100
functions:
  "myFunc":
    inputs: ./inputs/candidates.json
"#,
        )
        .unwrap();

        fs::write(
            inputs_dir.join("candidates.json"),
            r#"[{"args": [42], "label": "the answer"}]"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("myFunc", root.path(), None, Some(50), 30, &[])
                .unwrap();

        assert_eq!(resolved.candidate_inputs.len(), 1);
        assert_eq!(
            resolved.candidate_inputs[0].args,
            vec![serde_json::json!(42)]
        );
        assert_eq!(
            resolved.candidate_inputs[0].label.as_deref(),
            Some("the answer")
        );
    }

    #[test]
    fn resolve_function_config_explicit_inputs_overrides_config() {
        let root = TempDir::new().unwrap();

        // Create .shatter/config.yaml pointing to one file
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();
        fs::write(
            shatter_dir.join("config.yaml"),
            "defaults: {}\nfunctions:\n  \"myFunc\":\n    inputs: ./nonexistent.json\n",
        )
        .unwrap();

        // Create the explicit inputs file
        let explicit_path = root.path().join("explicit.json");
        fs::write(&explicit_path, r#"[{"args": [99]}]"#).unwrap();

        let resolved = resolve_function_config_with_inputs(
            "myFunc",
            root.path(),
            Some(&explicit_path),
            Some(50),
            30,
            &[],
        )
        .unwrap();

        assert_eq!(resolved.candidate_inputs.len(), 1);
        assert_eq!(
            resolved.candidate_inputs[0].args,
            vec![serde_json::json!(99)]
        );
    }

    #[test]
    fn hierarchical_merge_integration() {
        let root = TempDir::new().unwrap();

        // Root config: conservative defaults
        let root_shatter = root.path().join(".shatter");
        fs::create_dir_all(&root_shatter).unwrap();
        fs::write(
            root_shatter.join("config.yaml"),
            "defaults:\n  max_iterations: 50\n  timeout: 30\n",
        )
        .unwrap();

        // Sub-project config: override max_iterations for auth functions
        let sub = root.path().join("src").join("auth");
        fs::create_dir_all(&sub).unwrap();
        let sub_shatter = sub.join(".shatter");
        fs::create_dir_all(&sub_shatter).unwrap();
        fs::write(
            sub_shatter.join("config.yaml"),
            r#"
defaults:
  max_iterations: 500
functions:
  "src/auth/*:validate*":
    timeout: 120
"#,
        )
        .unwrap();

        let configs = discover_configs(&sub).unwrap();
        assert_eq!(configs.len(), 2);

        let merged = merge_configs(&configs);
        assert_eq!(merged.defaults.max_iterations, Some(500)); // nearest
        assert_eq!(merged.defaults.timeout, Some(30)); // falls through

        let resolved =
            resolve_function_config("src/auth/login.ts:validateToken", &configs, Some(100), 60)
                .unwrap();
        assert_eq!(resolved.max_iterations, Some(500)); // from sub defaults
        assert_eq!(resolved.timeout, 120); // from function pattern
    }

    #[test]
    fn setup_level_serialization_round_trip() {
        use crate::protocol::SetupLevel;

        for (level, expected_json) in [
            (SetupLevel::Session, "\"session\""),
            (SetupLevel::File, "\"file\""),
            (SetupLevel::Function, "\"function\""),
            (SetupLevel::Execution, "\"execution\""),
        ] {
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: SetupLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, level);
        }
    }

    #[test]
    fn setup_level_yaml_round_trip() {
        use crate::protocol::SetupLevel;

        for (yaml, expected) in [
            ("session", SetupLevel::Session),
            ("file", SetupLevel::File),
            ("function", SetupLevel::Function),
            ("execution", SetupLevel::Execution),
        ] {
            let level: SetupLevel = serde_yaml::from_str(yaml).unwrap();
            assert_eq!(level, expected);
        }
    }

    #[test]
    fn parse_config_with_setup_and_generators() {
        use crate::protocol::SetupLevel;

        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults:
  max_iterations: 100
  setup: ./setup/global.ts
  setup_level: execution
  setup_timeout: 45
  session_setup:
    file: ./setup/session.ts
    timeout: 60
  file_setup:
    "src/**/*.ts": ./setup/ts-files.ts
  generators:
    User: ./generators/user.ts
    Order: ./generators/order.ts
  param_generators:
    authToken: ./generators/token.ts

functions:
  "src/auth.ts:*":
    setup: ./setup/auth.ts
    setup_level: function
    setup_timeout: 20
    generators:
      User: ./generators/auth_user.ts
    param_generators:
      sessionId: ./generators/session.ts
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();

        // Defaults
        assert_eq!(config.defaults.setup.as_deref(), Some("./setup/global.ts"));
        assert_eq!(config.defaults.setup_level, Some(SetupLevel::Execution));
        assert_eq!(config.defaults.setup_timeout, Some(45));
        let session = config.defaults.session_setup.as_ref().unwrap();
        assert_eq!(session.file, "./setup/session.ts");
        assert_eq!(session.timeout, Some(60));
        let file_setup = config.defaults.file_setup.as_ref().unwrap();
        assert_eq!(file_setup["src/**/*.ts"], "./setup/ts-files.ts");
        let generators = config.defaults.generators.as_ref().unwrap();
        assert_eq!(generators.len(), 2);
        assert_eq!(generators["User"], "./generators/user.ts");
        assert_eq!(generators["Order"], "./generators/order.ts");
        let param_gens = config.defaults.param_generators.as_ref().unwrap();
        assert_eq!(param_gens["authToken"], "./generators/token.ts");

        // Function overrides
        let auth = &config.functions["src/auth.ts:*"];
        assert_eq!(auth.setup.as_deref(), Some("./setup/auth.ts"));
        assert_eq!(auth.setup_level, Some(SetupLevel::Function));
        assert_eq!(auth.setup_timeout, Some(20));
        let auth_gens = auth.generators.as_ref().unwrap();
        assert_eq!(auth_gens["User"], "./generators/auth_user.ts");
        let auth_pgens = auth.param_generators.as_ref().unwrap();
        assert_eq!(auth_pgens["sessionId"], "./generators/session.ts");
    }

    #[test]
    fn parse_config_without_new_fields_still_works() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(&config_path, "defaults:\n  max_iterations: 100\n").unwrap();

        let config = parse_config(&config_path).unwrap();
        assert_eq!(config.defaults.setup, None);
        assert_eq!(config.defaults.setup_level, None);
        assert_eq!(config.defaults.setup_timeout, None);
        assert_eq!(config.defaults.session_setup, None);
        assert_eq!(config.defaults.file_setup, None);
        assert_eq!(config.defaults.generators, None);
        assert_eq!(config.defaults.param_generators, None);
    }

    #[test]
    fn merge_configs_generators_near_overrides_far() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                generators: Some(HashMap::from([
                    ("User".to_string(), "./gen/user.ts".to_string()),
                    ("Order".to_string(), "./gen/order.ts".to_string()),
                ])),
                param_generators: Some(HashMap::from([(
                    "token".to_string(),
                    "./gen/token.ts".to_string(),
                )])),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                generators: Some(HashMap::from([(
                    "User".to_string(),
                    "./gen/custom_user.ts".to_string(),
                )])),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let gens = merged.defaults.generators.unwrap();
        // Near overrides User
        assert_eq!(gens["User"], "./gen/custom_user.ts");
        // Far's Order is inherited
        assert_eq!(gens["Order"], "./gen/order.ts");
        // Param generators from far survive
        let pgens = merged.defaults.param_generators.unwrap();
        assert_eq!(pgens["token"], "./gen/token.ts");
    }

    #[test]
    fn merge_configs_setup_nearest_wins() {
        use crate::protocol::SetupLevel;

        let near = ShatterConfig {
            defaults: DefaultsConfig {
                setup: Some("./setup/near.ts".to_string()),
                setup_level: Some(SetupLevel::Execution),
                setup_timeout: Some(45),
                session_setup: Some(SessionSetupConfig {
                    file: "./setup/near-session.ts".into(),
                    timeout: Some(90),
                }),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                setup: Some("./setup/far.ts".to_string()),
                setup_level: Some(SetupLevel::Function),
                setup_timeout: Some(20),
                session_setup: Some(SessionSetupConfig {
                    file: "./setup/far-session.ts".into(),
                    timeout: None,
                }),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        assert_eq!(merged.defaults.setup.as_deref(), Some("./setup/near.ts"));
        assert_eq!(merged.defaults.setup_level, Some(SetupLevel::Execution));
        assert_eq!(merged.defaults.setup_timeout, Some(45));
        let session = merged.defaults.session_setup.as_ref().unwrap();
        assert_eq!(session.file, "./setup/near-session.ts");
    }

    #[test]
    fn resolve_config_with_setup_from_function() {
        use crate::protocol::SetupLevel;

        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  setup: ./setup/default.ts
  setup_level: function
  setup_timeout: 45
functions:
  "myFunc":
    setup: ./setup/custom.ts
    setup_level: execution
    setup_timeout: 10
"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("myFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        // Function-level setup overrides defaults
        assert_eq!(resolved.setup, Some(shatter_dir.join("./setup/custom.ts")));
        assert_eq!(resolved.setup_level, SetupLevel::Execution);
        assert_eq!(resolved.setup_timeout, 10);
    }

    #[test]
    fn resolve_config_with_setup_from_defaults() {
        use crate::protocol::SetupLevel;

        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  setup: ./setup/default.ts
  setup_level: execution
"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("anyFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        assert_eq!(resolved.setup, Some(shatter_dir.join("./setup/default.ts")));
        assert_eq!(resolved.setup_level, SetupLevel::Execution);
    }

    #[test]
    fn resolve_config_generators_function_overrides_defaults() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  generators:
    User: ./gen/default_user.ts
    Order: ./gen/order.ts
  param_generators:
    token: ./gen/token.ts
functions:
  "myFunc":
    generators:
      User: ./gen/custom_user.ts
    param_generators:
      sessionId: ./gen/session.ts
"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("myFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        // Function-level User overrides default
        assert_eq!(
            resolved.generators[&"User".to_string()],
            shatter_dir.join("./gen/custom_user.ts")
        );
        // Default Order is inherited
        assert_eq!(
            resolved.generators[&"Order".to_string()],
            shatter_dir.join("./gen/order.ts")
        );
        // Both param generators present
        assert_eq!(
            resolved.param_generators[&"token".to_string()],
            shatter_dir.join("./gen/token.ts")
        );
        assert_eq!(
            resolved.param_generators[&"sessionId".to_string()],
            shatter_dir.join("./gen/session.ts")
        );
    }

    #[test]
    fn resolve_config_no_setup_returns_none() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();
        fs::write(
            shatter_dir.join("config.yaml"),
            "defaults:\n  max_iterations: 100\n",
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("anyFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        assert_eq!(resolved.setup, None);
        assert_eq!(resolved.setup_level, SetupLevel::Function); // default
        assert_eq!(resolved.setup_timeout, DEFAULT_SETUP_TIMEOUT_SECS);
        assert!(resolved.session_setup.is_none());
        assert!(resolved.file_setup.is_empty());
        assert!(resolved.generators.is_empty());
        assert!(resolved.param_generators.is_empty());
    }

    #[test]
    fn resolve_function_config_setup_level_defaults_to_function() {
        let resolved = resolve_function_config("any/func", &[], Some(100), 60).unwrap();
        assert_eq!(resolved.setup_level, SetupLevel::Function);
        assert_eq!(resolved.setup_timeout, DEFAULT_SETUP_TIMEOUT_SECS);
    }

    #[test]
    fn parse_config_with_opaque_types() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults:
  max_iterations: 100
opaque_types:
  - DatabasePool
  - RedisClient
  - KafkaProducer
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();
        assert_eq!(
            config.opaque_types,
            vec![
                CustomOpaqueType::Name("DatabasePool".to_string()),
                CustomOpaqueType::Name("RedisClient".to_string()),
                CustomOpaqueType::Name("KafkaProducer".to_string()),
            ]
        );
    }

    #[test]
    fn parse_config_with_opaque_types_object_form() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
opaque_types:
  - DatabaseConnection
  - name: HttpClient
    reason: "requires live HTTP connection"
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();
        assert_eq!(
            config.opaque_types,
            vec![
                CustomOpaqueType::Name("DatabaseConnection".to_string()),
                CustomOpaqueType::Named {
                    name: "HttpClient".to_string(),
                    reason: Some("requires live HTTP connection".to_string()),
                },
            ]
        );
        assert_eq!(config.opaque_types[0].name(), "DatabaseConnection");
        assert_eq!(config.opaque_types[0].reason(), None);
        assert_eq!(config.opaque_types[1].name(), "HttpClient");
        assert_eq!(
            config.opaque_types[1].reason(),
            Some("requires live HTTP connection")
        );
    }

    #[test]
    fn parse_config_without_opaque_types_defaults_to_empty() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(&config_path, "defaults:\n  max_iterations: 100\n").unwrap();

        let config = parse_config(&config_path).unwrap();
        assert!(config.opaque_types.is_empty());
    }

    #[test]
    fn merge_configs_combines_opaque_types_and_deduplicates() {
        let near = ShatterConfig {
            opaque_types: vec![
                CustomOpaqueType::Name("DatabasePool".to_string()),
                CustomOpaqueType::Name("RedisClient".to_string()),
            ],
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            opaque_types: vec![
                CustomOpaqueType::Name("RedisClient".to_string()),
                CustomOpaqueType::Name("KafkaProducer".to_string()),
            ],
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        assert_eq!(
            merged.opaque_types,
            vec![
                CustomOpaqueType::Name("DatabasePool".to_string()),
                CustomOpaqueType::Name("RedisClient".to_string()),
                CustomOpaqueType::Name("KafkaProducer".to_string()),
            ]
        );
    }

    #[test]
    fn merge_configs_opaque_types_empty_when_no_configs_have_them() {
        let a = ShatterConfig::default();
        let b = ShatterConfig::default();
        let merged = merge_configs(&[a, b]);
        assert!(merged.opaque_types.is_empty());
    }

    #[test]
    fn genetic_config_defaults() {
        let config = GeneticConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.population_size, 50);
        assert_eq!(config.max_generations, 100);
        assert!((config.mutation_rate - 0.3).abs() < f64::EPSILON);
        assert!((config.crossover_rate - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.timeout_secs, 300);
    }

    #[test]
    fn genetic_config_serde_roundtrip() {
        let config = GeneticConfig {
            enabled: true,
            population_size: 200,
            max_generations: 500,
            mutation_rate: 0.5,
            crossover_rate: 0.8,
            timeout_secs: 600,
        };
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: GeneticConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn genetic_config_partial_yaml_uses_defaults() {
        let yaml = "enabled: true\npopulation_size: 75\n";
        let config: GeneticConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.population_size, 75);
        assert_eq!(config.max_generations, 100);
        assert!((config.mutation_rate - 0.3).abs() < f64::EPSILON);
        assert!((config.crossover_rate - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.timeout_secs, 300);
    }

    #[test]
    fn shatter_config_with_genetic_section() {
        let yaml = r#"
defaults:
  max_iterations: 50
  genetic:
    enabled: true
    population_size: 100
functions:
  "src/math.ts:add":
    genetic:
      enabled: true
      max_generations: 200
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let defaults_genetic = config.defaults.genetic.unwrap();
        assert!(defaults_genetic.enabled);
        assert_eq!(defaults_genetic.population_size, 100);

        let func_config = config.functions.get("src/math.ts:add").unwrap();
        let func_genetic = func_config.genetic.as_ref().unwrap();
        assert!(func_genetic.enabled);
        assert_eq!(func_genetic.max_generations, 200);
    }

    #[test]
    fn parse_config_with_nondeterminism() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults: {}
nondeterminism:
  confirmed:
    - function: createUser
      path: '$.id'
      reason: 'UUID generated per call'
  rejected:
    - function: processOrder
      path: '$.orderId'
      reason: 'deterministic sequence from input'
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();
        let nd = config
            .nondeterminism
            .as_ref()
            .expect("nondeterminism present");
        assert_eq!(nd.confirmed.len(), 1);
        assert_eq!(nd.confirmed[0].function, "createUser");
        assert_eq!(nd.confirmed[0].path, "$.id");
        assert_eq!(nd.confirmed[0].reason, "UUID generated per call");
        assert_eq!(nd.rejected.len(), 1);
        assert_eq!(nd.rejected[0].function, "processOrder");
        assert_eq!(nd.rejected[0].path, "$.orderId");
    }

    #[test]
    fn parse_config_without_nondeterminism_is_none() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(&config_path, "defaults: {}\n").unwrap();

        let config = parse_config(&config_path).unwrap();
        assert!(config.nondeterminism.is_none());
    }

    #[test]
    fn nondeterminism_config_round_trip() {
        let config = ShatterConfig {
            nondeterminism: Some(NondeterminismConfig {
                confirmed: vec![NondeterminismDeclaration {
                    function: "createUser".into(),
                    path: "$.id".into(),
                    reason: "UUID".into(),
                }],
                rejected: vec![],
            }),
            ..ShatterConfig::default()
        };

        let yaml = serde_yaml::to_string(&config).expect("serialize");
        let restored: ShatterConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(config, restored);
    }

    #[test]
    fn merge_configs_unions_nondeterminism() {
        let near = ShatterConfig {
            nondeterminism: Some(NondeterminismConfig {
                confirmed: vec![NondeterminismDeclaration {
                    function: "createUser".into(),
                    path: "$.id".into(),
                    reason: "UUID".into(),
                }],
                rejected: vec![],
            }),
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            nondeterminism: Some(NondeterminismConfig {
                confirmed: vec![
                    // Duplicate (function, path) — should be deduped, near wins
                    NondeterminismDeclaration {
                        function: "createUser".into(),
                        path: "$.id".into(),
                        reason: "overridden reason".into(),
                    },
                    NondeterminismDeclaration {
                        function: "getTime".into(),
                        path: "$.now".into(),
                        reason: "clock".into(),
                    },
                ],
                rejected: vec![NondeterminismDeclaration {
                    function: "processOrder".into(),
                    path: "$.orderId".into(),
                    reason: "deterministic".into(),
                }],
            }),
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let nd = merged.nondeterminism.expect("merged nondeterminism");
        assert_eq!(nd.confirmed.len(), 2);
        // Near wins for duplicate key
        assert_eq!(nd.confirmed[0].reason, "UUID");
        assert_eq!(nd.confirmed[1].function, "getTime");
        assert_eq!(nd.rejected.len(), 1);
    }

    #[test]
    fn merge_configs_no_nondeterminism_stays_none() {
        let a = ShatterConfig::default();
        let b = ShatterConfig::default();
        let merged = merge_configs(&[a, b]);
        assert!(merged.nondeterminism.is_none());
    }

    #[test]
    fn session_setup_config_yaml_round_trip() {
        let config = SessionSetupConfig {
            file: "./setup/session.ts".into(),
            timeout: Some(120),
        };
        let yaml = serde_yaml::to_string(&config).expect("serialize");
        let restored: SessionSetupConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(config, restored);
    }

    #[test]
    fn session_setup_config_without_timeout() {
        let yaml = "file: ./setup/session.ts\n";
        let config: SessionSetupConfig = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(config.file, "./setup/session.ts");
        assert_eq!(config.timeout, None);
    }

    #[test]
    fn parse_config_with_file_setup() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults:
  file_setup:
    "src/**/*.ts": ./setup/ts-files.ts
    "src/**/*.go": ./setup/go-files.go
"#,
        )
        .unwrap();

        let config = parse_config(&config_path).unwrap();
        let file_setup = config.defaults.file_setup.as_ref().unwrap();
        assert_eq!(file_setup.len(), 2);
        assert_eq!(file_setup["src/**/*.ts"], "./setup/ts-files.ts");
        assert_eq!(file_setup["src/**/*.go"], "./setup/go-files.go");
    }

    #[test]
    fn merge_configs_file_setup_near_overrides_far() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                file_setup: Some(HashMap::from([
                    ("src/**/*.ts".to_string(), "./setup/far-ts.ts".to_string()),
                    ("src/**/*.go".to_string(), "./setup/go.go".to_string()),
                ])),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                file_setup: Some(HashMap::from([(
                    "src/**/*.ts".to_string(),
                    "./setup/near-ts.ts".to_string(),
                )])),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let file_setup = merged.defaults.file_setup.unwrap();
        assert_eq!(file_setup["src/**/*.ts"], "./setup/near-ts.ts");
        assert_eq!(file_setup["src/**/*.go"], "./setup/go.go");
    }

    #[test]
    fn resolve_config_with_file_setup() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  file_setup:
    "src/**/*.ts": ./setup/ts-files.ts
"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("anyFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        assert_eq!(
            resolved.file_setup["src/**/*.ts"],
            shatter_dir.join("./setup/ts-files.ts")
        );
    }

    #[test]
    fn resolve_config_session_setup_from_defaults() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  session_setup:
    file: ./setup/session.ts
    timeout: 120
"#,
        )
        .unwrap();

        let resolved =
            resolve_function_config_with_inputs("anyFunc", root.path(), None, Some(100), 60, &[])
                .unwrap();

        let session = resolved.session_setup.as_ref().unwrap();
        assert_eq!(session.file, "./setup/session.ts");
        assert_eq!(session.timeout, Some(120));
    }

    #[test]
    fn defaults_config_round_trip_with_all_setup_fields() {
        let config = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(200),
                setup: Some("./setup/func.ts".into()),
                setup_level: Some(SetupLevel::Execution),
                setup_timeout: Some(45),
                session_setup: Some(SessionSetupConfig {
                    file: "./setup/session.ts".into(),
                    timeout: Some(90),
                }),
                file_setup: Some(HashMap::from([(
                    "src/**/*.ts".to_string(),
                    "./setup/ts.ts".to_string(),
                )])),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let yaml = serde_yaml::to_string(&config).expect("serialize");
        let restored: ShatterConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(config, restored);
    }

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::arb_setup_level;
        use proptest::prelude::*;

        fn arb_session_setup_config() -> impl Strategy<Value = SessionSetupConfig> {
            ("[a-z./]{1,30}", proptest::option::of(1u64..3600))
                .prop_map(|(file, timeout)| SessionSetupConfig { file, timeout })
        }

        fn arb_file_setup_map() -> impl Strategy<Value = HashMap<String, String>> {
            proptest::collection::hash_map("[a-z*/.]{1,20}", "[a-z./]{1,20}", 0..3)
        }

        fn arb_defaults_config() -> impl Strategy<Value = DefaultsConfig> {
            (
                proptest::option::of(1u32..10000),
                proptest::option::of(1u64..3600),
                proptest::option::of("[a-z./]{1,30}"),
                proptest::option::of(arb_setup_level()),
                proptest::option::of(1u64..3600),
                proptest::option::of(arb_session_setup_config()),
                proptest::option::of(arb_file_setup_map()),
            )
                .prop_map(
                    |(
                        max_iterations,
                        timeout,
                        setup,
                        setup_level,
                        setup_timeout,
                        session_setup,
                        file_setup,
                    )| {
                        DefaultsConfig {
                            max_iterations,
                            timeout,
                            setup,
                            setup_level,
                            setup_timeout,
                            session_setup,
                            file_setup,
                            generators: None,
                            param_generators: None,
                            mocks: None,
                            genetic: None,
                            exploration: None,
                            fuzz: None,
                            execution_profile: None,
                        }
                    },
                )
        }

        // --- ProjectConfig tests ---

        #[test]
        fn project_config_parse_full() {
            let json = r#"{
                "include": ["src/**/*.ts"],
                "exclude": ["**/*.test.ts"],
                "language": "typescript",
                "max_depth": 5,
                "timeout_total": 600,
                "exec_timeout": 15,
                "parallelism": 4,
                "parallelism_min": 2,
                "parallelism_max": 32,
                "observer_pool": 6,
                "candidate_queue_capacity": 24,
                "output": {
                    "format": "json",
                    "paths": ["reports/scan.html"],
                    "stdout": true
                },
                "capture_side_effects": true,
                "no_cache": true,
                "cache_dir": ".my-cache",
                "seeds_dir": ".my-seeds"
            }"#;
            let config: ProjectConfig = serde_json::from_str(json).unwrap();
            assert_eq!(config.observer_pool, Some(6));
            assert_eq!(config.candidate_queue_capacity, Some(24));
            assert_eq!(config.include, vec!["src/**/*.ts"]);
            assert_eq!(config.exclude, vec!["**/*.test.ts"]);
            assert_eq!(config.language.as_deref(), Some("typescript"));
            assert_eq!(config.max_depth, Some(5));
            assert_eq!(config.timeout_total, Some(600));
            assert_eq!(config.exec_timeout, Some(15));
            assert_eq!(config.parallelism, Some(4));
            assert_eq!(config.parallelism_min, Some(2));
            assert_eq!(config.parallelism_max, Some(32));
            assert_eq!(config.capture_side_effects, Some(true));
            assert_eq!(config.no_cache, Some(true));
            assert_eq!(
                config.output.as_ref().and_then(|o| o.format.as_deref()),
                Some("json")
            );
            assert_eq!(config.output.as_ref().map(|o| o.paths.len()), Some(1));
            assert_eq!(config.output.as_ref().and_then(|o| o.stdout), Some(true));
        }

        #[test]
        fn project_config_parse_minimal() {
            let json = "{}";
            let config: ProjectConfig = serde_json::from_str(json).unwrap();
            assert!(config.include.is_empty());
            assert!(config.exclude.is_empty());
            assert!(config.language.is_none());
            assert!(config.timeout_total.is_none());
            assert!(config.output.is_none());
        }

        #[test]
        fn project_config_load_missing_file() {
            let tmp = tempfile::tempdir().unwrap();
            let result = load_project_config(tmp.path()).unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn project_config_load_valid() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join(PROJECT_CONFIG_FILENAME);
            std::fs::write(&path, r#"{"timeout_total": 600}"#).unwrap();
            let config = load_project_config(tmp.path()).unwrap().unwrap();
            assert_eq!(config.timeout_total, Some(600));
        }

        #[test]
        fn project_config_load_invalid_json() {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join(PROJECT_CONFIG_FILENAME);
            std::fs::write(&path, "not json at all {{{").unwrap();
            let result = load_project_config(tmp.path());
            assert!(result.is_err());
        }

        #[test]
        fn project_config_ignores_unknown_fields() {
            let json = r#"{"unknown_field": true, "timeout_total": 42}"#;
            let config: ProjectConfig = serde_json::from_str(json).unwrap();
            assert_eq!(config.timeout_total, Some(42));
        }

        #[test]
        fn project_config_json_roundtrip() {
            let config = ProjectConfig {
                include: vec!["src/**/*.ts".to_string()],
                exclude: vec!["**/test/**".to_string()],
                language: Some("typescript".to_string()),
                timeout_total: Some(600),
                exec_timeout: Some(15),
                parallelism: Some(4),
                parallelism_min: Some(2),
                parallelism_max: Some(32),
                output: Some(OutputConfig {
                    format: Some("json".to_string()),
                    paths: vec![std::path::PathBuf::from("report.html")],
                    stdout: Some(true),
                }),
                max_depth: None,
                cache_dir: None,
                no_cache: None,
                seeds_dir: None,
                capture_side_effects: Some(true),
                observer_pool: None,
                candidate_queue_capacity: None,
            };
            let json = serde_json::to_string(&config).unwrap();
            let restored: ProjectConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(config, restored);
        }

        #[test]
        fn fuzz_config_yaml_roundtrip() {
            let yaml = r#"
plateau_threshold: 100
max_executions: 2000
timeout_seconds: 60
max_attempts: 5
"#;
            let config: FuzzConfig = serde_yaml::from_str(yaml).unwrap();
            assert_eq!(config.plateau_threshold, Some(100));
            assert_eq!(config.max_executions, Some(2000));
            assert_eq!(config.timeout_seconds, Some(60));
            assert_eq!(config.max_attempts, Some(5));

            // Roundtrip
            let serialized = serde_yaml::to_string(&config).unwrap();
            let restored: FuzzConfig = serde_yaml::from_str(&serialized).unwrap();
            assert_eq!(config, restored);
        }

        #[test]
        fn fuzz_config_defaults_when_absent() {
            let yaml = r#"
defaults:
  max_iterations: 50
functions:
  "src/math.ts:add":
    max_iterations: 100
"#;
            let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
            // No fuzz section — defaults should be None at parse level.
            assert!(config.defaults.fuzz.is_none());

            // Resolution should produce built-in defaults.
            let resolved = resolve_function_config("src/math.ts:add", &[config], None, 30);
            let resolved = resolved.unwrap();
            let fuzz = &resolved.fuzz;
            assert_eq!(fuzz.plateau_threshold, Some(DEFAULT_FUZZ_PLATEAU_THRESHOLD));
            assert_eq!(fuzz.max_executions, Some(DEFAULT_FUZZ_MAX_EXECUTIONS));
            assert_eq!(fuzz.timeout_seconds, Some(DEFAULT_FUZZ_TIMEOUT_SECS));
            assert_eq!(fuzz.max_attempts, Some(DEFAULT_FUZZ_MAX_ATTEMPTS));
        }

        proptest! {
            #[test]
            fn session_setup_config_yaml_roundtrip(config in arb_session_setup_config()) {
                let yaml = serde_yaml::to_string(&config).expect("serialize");
                let restored: SessionSetupConfig =
                    serde_yaml::from_str(&yaml).expect("deserialize");
                prop_assert_eq!(config, restored);
            }

            #[test]
            fn defaults_config_yaml_roundtrip(defaults in arb_defaults_config()) {
                let yaml = serde_yaml::to_string(&defaults).expect("serialize");
                let restored: DefaultsConfig =
                    serde_yaml::from_str(&yaml).expect("deserialize");
                prop_assert_eq!(defaults, restored);
            }

            #[test]
            fn setup_level_in_config_roundtrip(level in arb_setup_level()) {
                let config = DefaultsConfig {
                    setup_level: Some(level),
                    ..DefaultsConfig::default()
                };
                let yaml = serde_yaml::to_string(&config).expect("serialize");
                let restored: DefaultsConfig =
                    serde_yaml::from_str(&yaml).expect("deserialize");
                prop_assert_eq!(config.setup_level, restored.setup_level);
            }

            /// Merging a single config is identity.
            #[test]
            fn merge_single_config_is_identity(defaults in arb_defaults_config()) {
                let config = ShatterConfig {
                    defaults: defaults.clone(),
                    ..ShatterConfig::default()
                };
                let merged = merge_configs(&[config]);
                prop_assert_eq!(merged.defaults.setup_level, defaults.setup_level);
                prop_assert_eq!(merged.defaults.setup_timeout, defaults.setup_timeout);
                prop_assert_eq!(merged.defaults.session_setup, defaults.session_setup);
            }

            /// Near config's setup fields always win over far config's.
            #[test]
            fn merge_near_setup_wins(
                near_defaults in arb_defaults_config(),
                far_defaults in arb_defaults_config(),
            ) {
                let near = ShatterConfig {
                    defaults: near_defaults.clone(),
                    ..ShatterConfig::default()
                };
                let far = ShatterConfig {
                    defaults: far_defaults,
                    ..ShatterConfig::default()
                };
                let merged = merge_configs(&[near, far]);

                // Nearest non-None wins for scalar fields.
                if near_defaults.setup_level.is_some() {
                    prop_assert_eq!(merged.defaults.setup_level, near_defaults.setup_level);
                }
                if near_defaults.setup_timeout.is_some() {
                    prop_assert_eq!(merged.defaults.setup_timeout, near_defaults.setup_timeout);
                }
                if near_defaults.session_setup.is_some() {
                    prop_assert_eq!(merged.defaults.session_setup, near_defaults.session_setup);
                }
            }
        }
    }

    #[test]
    fn exploration_config_yaml_roundtrip_all_fields() {
        let yaml = r#"
defaults:
  exploration:
    adaptive: false
    score_window: 50
    cold_start: 10
    strategy_floor: 0.05
    strategy_weights:
      literals: 0.3
      random: 0.5
      boundary: 0.2
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let exp = config.defaults.exploration.unwrap();
        assert!(!exp.adaptive);
        assert_eq!(exp.score_window, 50);
        assert_eq!(exp.cold_start, 10);
        assert!((exp.strategy_floor - 0.05).abs() < f64::EPSILON);
        let weights = exp.strategy_weights.unwrap();
        assert_eq!(weights.len(), 3);
        assert!((weights["literals"] - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn exploration_config_yaml_defaults_fill_in() {
        let yaml = r#"
defaults:
  exploration:
    adaptive: false
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let exp = config.defaults.exploration.unwrap();
        assert!(!exp.adaptive);
        assert_eq!(exp.score_window, DEFAULT_EXPLORATION_SCORE_WINDOW);
        assert_eq!(exp.cold_start, DEFAULT_EXPLORATION_COLD_START);
        assert!((exp.strategy_floor - DEFAULT_EXPLORATION_STRATEGY_FLOOR).abs() < f64::EPSILON);
        assert!(exp.strategy_weights.is_none());
    }

    #[test]
    fn exploration_config_absent_means_none() {
        let yaml = "defaults: {}\n";
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.defaults.exploration.is_none());
    }

    #[test]
    fn merge_configs_exploration_near_overrides_far() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                exploration: Some(ExplorationConfig {
                    adaptive: true,
                    score_window: 250,
                    cold_start: 30,
                    strategy_floor: 0.03,
                    strategy_weights: None,
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                exploration: Some(ExplorationConfig {
                    adaptive: false,
                    score_window: 50,
                    cold_start: 10,
                    strategy_floor: 0.10,
                    strategy_weights: Some(HashMap::from([
                        ("boundary".to_string(), 0.75),
                        ("random".to_string(), 0.25),
                    ])),
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let exploration = merged
            .defaults
            .exploration
            .expect("merged exploration defaults");
        assert!(!exploration.adaptive);
        assert_eq!(exploration.score_window, 50);
        assert_eq!(exploration.cold_start, 10);
        assert!((exploration.strategy_floor - 0.10).abs() < f64::EPSILON);
        assert_eq!(
            exploration.strategy_weights,
            Some(HashMap::from([
                ("boundary".to_string(), 0.75),
                ("random".to_string(), 0.25),
            ]))
        );
    }

    #[test]
    fn exploration_config_to_meta_config_conversion() {
        let mut weights = HashMap::new();
        weights.insert("random".to_string(), 0.7);
        weights.insert("literals".to_string(), 0.3);
        let exp = ExplorationConfig {
            adaptive: false,
            score_window: 42,
            cold_start: 5,
            strategy_floor: 0.1,
            strategy_weights: Some(weights),
        };
        let meta = exp.to_meta_config();
        assert!(!meta.adaptive);
        assert_eq!(meta.window_size, 42);
        assert_eq!(meta.cold_start_threshold, 5);
        assert!((meta.floor - 0.1).abs() < f64::EPSILON);
        let sw = meta.static_weights.unwrap();
        assert_eq!(sw.len(), 2);
    }

    #[test]
    fn exploration_config_function_overrides_defaults() {
        let yaml = r#"
defaults:
  exploration:
    adaptive: true
    score_window: 200
functions:
  "src/hot.ts:*":
    exploration:
      adaptive: false
      score_window: 50
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved =
            resolve_function_config("src/hot.ts:hotPath", &[config], Some(100), 60).unwrap();
        assert!(!resolved.exploration.adaptive);
        assert_eq!(resolved.exploration.score_window, 50);
    }

    #[test]
    fn exploration_config_falls_through_to_defaults() {
        let yaml = r#"
defaults:
  exploration:
    adaptive: false
    score_window: 200
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolve_function_config("any:func", &[config], Some(100), 60).unwrap();
        assert!(!resolved.exploration.adaptive);
        assert_eq!(resolved.exploration.score_window, 200);
    }

    #[test]
    fn genetic_config_from_defaults() {
        let yaml = r#"
defaults:
  genetic:
    enabled: true
    population_size: 200
    max_generations: 500
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolve_function_config("any:func", &[config], Some(100), 60).unwrap();
        assert!(resolved.genetic.enabled);
        assert_eq!(resolved.genetic.population_size, 200);
        assert_eq!(resolved.genetic.max_generations, 500);
        // timeout_secs falls back to built-in default
        assert_eq!(
            resolved.genetic.timeout_secs,
            GeneticConfig::default().timeout_secs
        );
    }

    #[test]
    fn merge_configs_genetic_near_overrides_far() {
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                genetic: Some(GeneticConfig {
                    enabled: true,
                    population_size: 200,
                    max_generations: 300,
                    mutation_rate: 0.40,
                    crossover_rate: 0.50,
                    timeout_secs: 45,
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                genetic: Some(GeneticConfig {
                    enabled: false,
                    population_size: 25,
                    max_generations: 80,
                    mutation_rate: 0.10,
                    crossover_rate: 0.90,
                    timeout_secs: 15,
                }),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        let genetic = merged.defaults.genetic.expect("merged genetic defaults");
        assert!(!genetic.enabled);
        assert_eq!(genetic.population_size, 25);
        assert_eq!(genetic.max_generations, 80);
        assert!((genetic.mutation_rate - 0.10).abs() < f64::EPSILON);
        assert!((genetic.crossover_rate - 0.90).abs() < f64::EPSILON);
        assert_eq!(genetic.timeout_secs, 15);
    }

    #[test]
    fn genetic_config_function_overrides_defaults() {
        let yaml = r#"
defaults:
  genetic:
    enabled: true
    population_size: 200
functions:
  "src/hot.ts:*":
    genetic:
      enabled: false
      population_size: 50
"#;
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved =
            resolve_function_config("src/hot.ts:hotPath", &[config], Some(100), 60).unwrap();
        assert!(!resolved.genetic.enabled);
        assert_eq!(resolved.genetic.population_size, 50);
    }

    #[test]
    fn genetic_config_absent_uses_builtin_defaults() {
        let yaml = "defaults: {}\n";
        let config: ShatterConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolve_function_config("any:func", &[config], Some(100), 60).unwrap();
        assert!(!resolved.genetic.enabled);
        assert_eq!(resolved.genetic, GeneticConfig::default());
    }

    #[test]
    fn parse_strategy_weights_valid() {
        let weights =
            ExplorationConfig::parse_strategy_weights("literals=0.3,random=0.5,boundary=0.2")
                .unwrap();
        assert_eq!(weights.len(), 3);
        assert!((weights["literals"] - 0.3).abs() < f64::EPSILON);
        assert!((weights["random"] - 0.5).abs() < f64::EPSILON);
        assert!((weights["boundary"] - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_strategy_weights_trims_whitespace() {
        let weights =
            ExplorationConfig::parse_strategy_weights("  random = 0.8 , literals = 0.2 ").unwrap();
        assert_eq!(weights.len(), 2);
    }

    #[test]
    fn parse_strategy_weights_rejects_missing_equals() {
        let result = ExplorationConfig::parse_strategy_weights("random:0.5");
        assert!(result.is_err());
    }

    #[test]
    fn parse_strategy_weights_rejects_non_numeric() {
        let result = ExplorationConfig::parse_strategy_weights("random=abc");
        assert!(result.is_err());
    }

    #[test]
    fn parse_strategy_weights_rejects_negative() {
        let result = ExplorationConfig::parse_strategy_weights("random=-0.5");
        assert!(result.is_err());
    }

    #[test]
    fn parse_strategy_weights_rejects_empty() {
        let result = ExplorationConfig::parse_strategy_weights("");
        assert!(result.is_err());
    }

    // --- parse_set_overrides tests ---

    #[test]
    fn parse_set_overrides_empty_returns_default() {
        let result = parse_set_overrides(&[]).unwrap();
        assert_eq!(result, ShatterConfig::default());
    }

    #[test]
    fn parse_set_overrides_scalar_types() {
        let pairs = vec![
            "defaults.max_iterations=200".to_string(),
            "defaults.timeout=60".to_string(),
            "defaults.exploration.adaptive=false".to_string(),
            "defaults.exploration.strategy_floor=0.1".to_string(),
        ];
        let result = parse_set_overrides(&pairs).unwrap();
        assert_eq!(result.defaults.max_iterations, Some(200));
        assert_eq!(result.defaults.timeout, Some(60));
        let exp = result.defaults.exploration.unwrap();
        assert!(!exp.adaptive);
        assert!((exp.strategy_floor - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_set_overrides_missing_equals_returns_error() {
        let result = parse_set_overrides(&["defaults.max_iterations".to_string()]);
        assert!(matches!(
            result,
            Err(ConfigError::InvalidSetOverride { .. })
        ));
    }

    #[test]
    fn parse_set_overrides_empty_segment_returns_error() {
        let result = parse_set_overrides(&["defaults..max_iterations=200".to_string()]);
        assert!(matches!(
            result,
            Err(ConfigError::InvalidSetOverride { .. })
        ));
    }

    #[test]
    fn parse_set_overrides_precedence_over_yaml() {
        // --set config at index 0 should win over discovered YAML config at index 1.
        let yaml_config = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(100),
                ..DefaultsConfig::default()
            },
            ..ShatterConfig::default()
        };
        let set_config = parse_set_overrides(&["defaults.max_iterations=200".to_string()]).unwrap();
        // Prepend set_config as highest-priority layer.
        let merged = merge_configs(&[set_config, yaml_config]);
        assert_eq!(merged.defaults.max_iterations, Some(200));
    }

    #[test]
    fn parse_set_overrides_unknown_field_is_ignored_by_serde() {
        // serde_yaml uses `deny_unknown_fields` by default only if annotated; ShatterConfig
        // derives Deserialize without it, so unknown keys are silently ignored.
        // This verifies no panic occurs on unknown keys.
        let result = parse_set_overrides(&["unknown.field=42".to_string()]);
        // May succeed (ignored) or fail at deserialize — either is acceptable, but no panic.
        let _ = result;
    }
}

#[cfg(test)]
mod config_proptests {
    use proptest::prelude::*;

    use super::{
        NondeterminismDeclaration, parse_config, parse_set_overrides, update_nondeterminism_config,
    };

    proptest! {
        #[test]
        fn parse_set_overrides_never_panics(pairs in proptest::collection::vec(
            "[a-z][a-z0-9_]*\\.[a-z][a-z0-9_]*=[a-z0-9]+",
            0..8,
        )) {
            // Must not panic — only Ok or Err.
            let _ = parse_set_overrides(&pairs);
        }

        /// `update_nondeterminism_config` never panics for any combination of
        /// function names and paths, and the resulting file round-trips cleanly.
        #[test]
        fn update_nondeterminism_config_roundtrip(
            confirmed in proptest::collection::vec(
                ("[a-z][a-z0-9]{0,15}", "\\$\\.[a-z][a-z0-9]{0,15}", "[a-z ]{1,40}"),
                0..5usize,
            ),
            rejected in proptest::collection::vec(
                ("[a-z][a-z0-9]{0,15}", "\\$\\.[a-z][a-z0-9]{0,15}", "[a-z ]{1,40}"),
                0..5usize,
            ),
        ) {
            let tmp = tempfile::tempdir().unwrap();
            let config_path = tmp.path().join(".shatter").join("config.yaml");

            let confirmed_decls: Vec<NondeterminismDeclaration> = confirmed
                .into_iter()
                .map(|(function, path, reason)| NondeterminismDeclaration { function, path, reason })
                .collect();
            let rejected_decls: Vec<NondeterminismDeclaration> = rejected
                .into_iter()
                .map(|(function, path, reason)| NondeterminismDeclaration { function, path, reason })
                .collect();

            // Must not panic or error.
            update_nondeterminism_config(&config_path, &confirmed_decls, &rejected_decls)
                .expect("update must succeed");

            // File must be parseable.
            let loaded = parse_config(&config_path).expect("config must be parseable");

            // Confirmed entries must all be present.
            if let Some(nd) = loaded.nondeterminism {
                for decl in &confirmed_decls {
                    prop_assert!(
                        nd.confirmed.iter().any(|d| d.function == decl.function && d.path == decl.path),
                        "confirmed entry missing after update"
                    );
                }
                for decl in &rejected_decls {
                    prop_assert!(
                        nd.rejected.iter().any(|d| d.function == decl.function && d.path == decl.path),
                        "rejected entry missing after update"
                    );
                }
            } else {
                // nondeterminism section is absent only when both lists are empty.
                prop_assert!(confirmed_decls.is_empty() && rejected_decls.is_empty());
            }
        }

        /// Calling `update_nondeterminism_config` twice with the same entry must not duplicate it.
        #[test]
        fn update_nondeterminism_config_idempotent(
            function in "[a-z][a-z0-9]{0,15}",
            path in "\\$\\.[a-z][a-z0-9]{0,15}",
        ) {
            let tmp = tempfile::tempdir().unwrap();
            let config_path = tmp.path().join(".shatter").join("config.yaml");

            let decl = NondeterminismDeclaration {
                function: function.clone(),
                path: path.clone(),
                reason: "test".to_string(),
            };

            update_nondeterminism_config(&config_path, std::slice::from_ref(&decl), &[]).unwrap();
            update_nondeterminism_config(&config_path, &[decl], &[]).unwrap();

            let loaded = parse_config(&config_path).unwrap();
            let nd = loaded.nondeterminism.unwrap();
            let count = nd.confirmed.iter().filter(|d| d.function == function && d.path == path).count();
            prop_assert_eq!(count, 1, "must not duplicate on repeated write");
        }

        /// ProjectConfig roundtrips through JSON serialization.
        #[test]
        fn project_config_proptest_roundtrip(
            timeout_total in proptest::option::of(1u64..3600),
            exec_timeout in proptest::option::of(1u64..120),
            parallelism in proptest::option::of(0usize..64),
            parallelism_min in proptest::option::of(1usize..64),
            parallelism_max in proptest::option::of(1usize..64),
            capture in proptest::option::of(proptest::bool::ANY),
        ) {
            let config = super::ProjectConfig {
                timeout_total,
                exec_timeout,
                parallelism,
                parallelism_min,
                parallelism_max,
                capture_side_effects: capture,
                ..Default::default()
            };
            let json = serde_json::to_string(&config).expect("serialize");
            let restored: super::ProjectConfig = serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(config, restored);
        }

        /// Random bytes never panic when parsed as ProjectConfig — only Ok or Err.
        #[test]
        fn project_config_parse_never_panics(data in "[\\x00-\\xff]{0,256}") {
            let _ = serde_json::from_str::<super::ProjectConfig>(&data);
        }
    }
}
