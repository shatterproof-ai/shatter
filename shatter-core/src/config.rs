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

use crate::protocol::SetupLevel;

/// Default setup timeout in seconds, applied when no explicit value is configured.
pub const DEFAULT_SETUP_TIMEOUT_SECS: u64 = 30;

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

    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        pattern: String,
        source: globset::Error,
    },
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
    #[serde(default)]
    pub opaque_types: Vec<String>,

    /// User-declared nondeterminism: confirmed and rejected field declarations.
    #[serde(default)]
    pub nondeterminism: Option<NondeterminismConfig>,
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
    fn default_population_size() -> u32 { 50 }
    fn default_max_generations() -> u32 { 100 }
    fn default_mutation_rate() -> f64 { 0.3 }
    fn default_crossover_rate() -> f64 { 0.7 }
    fn default_timeout_secs() -> u32 { 300 }
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
    /// Maximum iterations (from config or CLI default).
    pub max_iterations: u32,

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
}

/// A config file found during hierarchical discovery, paired with its directory.
#[derive(Debug, Clone)]
struct DiscoveredConfig {
    /// The `.shatter/` directory containing this config.
    shatter_dir: PathBuf,
    /// The parsed config.
    config: ShatterConfig,
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

    // Merge generators and file_setup maps: start from farthest, overlay nearer.
    // This lets a near config override specific keys while inheriting the rest.
    let mut generators: Option<HashMap<String, String>> = None;
    let mut param_generators: Option<HashMap<String, String>> = None;
    let mut file_setup: Option<HashMap<String, String>> = None;

    for config in configs.iter().rev() {
        if let Some(ref g) = config.defaults.generators {
            generators.get_or_insert_with(HashMap::new).extend(
                g.iter().map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        if let Some(ref pg) = config.defaults.param_generators {
            param_generators.get_or_insert_with(HashMap::new).extend(
                pg.iter().map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        if let Some(ref fs) = config.defaults.file_setup {
            file_setup.get_or_insert_with(HashMap::new).extend(
                fs.iter().map(|(k, v)| (k.clone(), v.clone())),
            );
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
    }

    // Merge function configs: collect all, nearest first (already in order).
    // Later entries only fill in if the pattern hasn't been seen.
    let mut functions: HashMap<String, FunctionConfig> = HashMap::new();
    for config in configs {
        for (pattern, func_config) in &config.functions {
            functions.entry(pattern.clone()).or_insert_with(|| func_config.clone());
        }
    }

    let mut opaque_types = Vec::new();
    for config in configs {
        for t in &config.opaque_types {
            if !opaque_types.contains(t) {
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
                if !confirmed.iter().any(|d| d.function == decl.function && d.path == decl.path) {
                    confirmed.push(decl.clone());
                }
            }
            for decl in &nd.rejected {
                if !rejected.iter().any(|d| d.function == decl.function && d.path == decl.path) {
                    rejected.push(decl.clone());
                }
            }
        }
    }
    let nondeterminism = if has_any {
        Some(NondeterminismConfig { confirmed, rejected })
    } else {
        None
    };

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
            mocks: None,
            genetic: None,
        },
        functions,
        opaque_types,
        nondeterminism,
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
    cli_max_iterations: u32,
    cli_timeout: u64,
) -> Result<ResolvedFunctionConfig, ConfigError> {
    let merged = merge_configs(configs);
    resolve_from_merged(function_id, &merged, cli_max_iterations, cli_timeout)
}

/// Resolve config for a function from an already-merged config.
fn resolve_from_merged(
    function_id: &str,
    config: &ShatterConfig,
    cli_max_iterations: u32,
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

    // Resolution order: function config > defaults > CLI flags
    let max_iterations = func_config
        .and_then(|fc| fc.max_iterations)
        .or(config.defaults.max_iterations)
        .unwrap_or(cli_max_iterations);

    let timeout = func_config
        .and_then(|fc| fc.timeout)
        .or(config.defaults.timeout)
        .unwrap_or(cli_timeout);

    let skip = func_config
        .and_then(|fc| fc.skip)
        .unwrap_or(false);

    let setup_level = func_config
        .and_then(|fc| fc.setup_level)
        .or(config.defaults.setup_level)
        .unwrap_or(SetupLevel::Function);

    let setup_timeout = func_config
        .and_then(|fc| fc.setup_timeout)
        .or(config.defaults.setup_timeout)
        .unwrap_or(DEFAULT_SETUP_TIMEOUT_SECS);

    // Merge mock overrides: defaults first, then function-level overrides on top.
    let mut mock_overrides: HashMap<String, crate::auto_mock::MockOverride> = config
        .defaults
        .mocks
        .clone()
        .unwrap_or_default();
    if let Some(fc) = func_config
        && let Some(ref func_mocks) = fc.mocks
    {
        for (k, v) in func_mocks {
            mock_overrides.insert(k.clone(), v.clone());
        }
    }

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
    })
}

/// Resolve config for a function and load candidate inputs if specified.
///
/// `shatter_dir` is the `.shatter/` directory used to resolve relative input paths.
/// If `explicit_inputs` is provided (from `--inputs` CLI flag), it takes precedence.
pub fn resolve_function_config_with_inputs(
    function_id: &str,
    start_dir: &Path,
    explicit_inputs: Option<&Path>,
    cli_max_iterations: u32,
    cli_timeout: u64,
) -> Result<ResolvedFunctionConfig, ConfigError> {
    let discovered = discover_configs_with_paths(start_dir)?;
    let configs: Vec<ShatterConfig> = discovered.iter().map(|d| d.config.clone()).collect();
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
                        resolved.candidate_inputs =
                            parse_candidate_inputs(&inputs_path)?;
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
                resolved.file_setup.insert(glob_pattern.clone(), dc.shatter_dir.join(setup_rel));
            }
        }
    }

    // Resolve generators: merge defaults then overlay function-level.
    // Walk farthest-to-nearest so nearer configs override.
    for dc in discovered.iter().rev() {
        if let Some(ref g) = dc.config.defaults.generators {
            for (type_name, gen_rel) in g {
                resolved.generators.insert(type_name.clone(), dc.shatter_dir.join(gen_rel));
            }
        }
        if let Some(ref pg) = dc.config.defaults.param_generators {
            for (param_name, gen_rel) in pg {
                resolved.param_generators.insert(param_name.clone(), dc.shatter_dir.join(gen_rel));
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
                        resolved.generators.insert(type_name.clone(), dc.shatter_dir.join(gen_rel));
                    }
                }
                if let Some(ref pg) = fc.param_generators {
                    for (param_name, gen_rel) in pg {
                        resolved.param_generators.insert(param_name.clone(), dc.shatter_dir.join(gen_rel));
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
        assert_eq!(inputs[0].args, vec![serde_json::json!(42), serde_json::json!("hello")]);
        assert_eq!(inputs[0].label.as_deref(), Some("typical usage"));
        assert_eq!(inputs[1].args, vec![serde_json::json!(-1), serde_json::json!("")]);
    }

    #[test]
    fn parse_candidate_inputs_without_labels() {
        let dir = TempDir::new().unwrap();
        let inputs_path = dir.path().join("candidates.json");
        fs::write(
            &inputs_path,
            r#"[{ "args": [1, 2, 3] }]"#,
        )
        .unwrap();

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
            resolve_function_config("src/auth.ts:validateToken", &[config], 50, 30).unwrap();
        assert_eq!(resolved.max_iterations, 500);
        assert_eq!(resolved.timeout, 120);
        assert!(!resolved.skip);
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

        let resolved =
            resolve_function_config("some/func", &[config], 50, 30).unwrap();
        assert_eq!(resolved.max_iterations, 200); // from config defaults
        assert_eq!(resolved.timeout, 30); // from CLI (config default is None)
    }

    #[test]
    fn resolve_function_config_uses_cli_defaults_when_no_config() {
        let resolved =
            resolve_function_config("any/func", &[], 100, 60).unwrap();
        assert_eq!(resolved.max_iterations, 100);
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
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions,
            ..ShatterConfig::default()
        };

        let resolved =
            resolve_function_config("src/generated/api.ts:handler", &[config.clone()], 100, 60)
                .unwrap();
        assert!(resolved.skip);

        let resolved2 =
            resolve_function_config("src/auth.ts:login", &[config], 100, 60).unwrap();
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

        let resolved = resolve_function_config_with_inputs(
            "myFunc",
            root.path(),
            None,
            50,
            30,
        )
        .unwrap();

        assert_eq!(resolved.candidate_inputs.len(), 1);
        assert_eq!(resolved.candidate_inputs[0].args, vec![serde_json::json!(42)]);
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
        fs::write(
            &explicit_path,
            r#"[{"args": [99]}]"#,
        )
        .unwrap();

        let resolved = resolve_function_config_with_inputs(
            "myFunc",
            root.path(),
            Some(&explicit_path),
            50,
            30,
        )
        .unwrap();

        assert_eq!(resolved.candidate_inputs.len(), 1);
        assert_eq!(resolved.candidate_inputs[0].args, vec![serde_json::json!(99)]);
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
            resolve_function_config("src/auth/login.ts:validateToken", &configs, 100, 60).unwrap();
        assert_eq!(resolved.max_iterations, 500); // from sub defaults
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
        fs::write(
            &config_path,
            "defaults:\n  max_iterations: 100\n",
        )
        .unwrap();

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
                param_generators: Some(HashMap::from([
                    ("token".to_string(), "./gen/token.ts".to_string()),
                ])),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                generators: Some(HashMap::from([
                    ("User".to_string(), "./gen/custom_user.ts".to_string()),
                ])),
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

        let resolved = resolve_function_config_with_inputs(
            "myFunc",
            root.path(),
            None,
            100,
            60,
        )
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

        let resolved = resolve_function_config_with_inputs(
            "anyFunc",
            root.path(),
            None,
            100,
            60,
        )
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

        let resolved = resolve_function_config_with_inputs(
            "myFunc",
            root.path(),
            None,
            100,
            60,
        )
        .unwrap();

        // Function-level User overrides default
        assert_eq!(resolved.generators[&"User".to_string()], shatter_dir.join("./gen/custom_user.ts"));
        // Default Order is inherited
        assert_eq!(resolved.generators[&"Order".to_string()], shatter_dir.join("./gen/order.ts"));
        // Both param generators present
        assert_eq!(resolved.param_generators[&"token".to_string()], shatter_dir.join("./gen/token.ts"));
        assert_eq!(resolved.param_generators[&"sessionId".to_string()], shatter_dir.join("./gen/session.ts"));
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

        let resolved = resolve_function_config_with_inputs(
            "anyFunc",
            root.path(),
            None,
            100,
            60,
        )
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
        let resolved =
            resolve_function_config("any/func", &[], 100, 60).unwrap();
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
        assert_eq!(config.opaque_types, vec![
            "DatabasePool".to_string(),
            "RedisClient".to_string(),
            "KafkaProducer".to_string(),
        ]);
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
            opaque_types: vec!["DatabasePool".to_string(), "RedisClient".to_string()],
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            opaque_types: vec!["RedisClient".to_string(), "KafkaProducer".to_string()],
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        assert_eq!(merged.opaque_types, vec![
            "DatabasePool".to_string(),
            "RedisClient".to_string(),
            "KafkaProducer".to_string(),
        ]);
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
        let nd = config.nondeterminism.as_ref().expect("nondeterminism present");
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
                file_setup: Some(HashMap::from([
                    ("src/**/*.ts".to_string(), "./setup/near-ts.ts".to_string()),
                ])),
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

        let resolved = resolve_function_config_with_inputs(
            "anyFunc",
            root.path(),
            None,
            100,
            60,
        )
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

        let resolved = resolve_function_config_with_inputs(
            "anyFunc",
            root.path(),
            None,
            100,
            60,
        )
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
                file_setup: Some(HashMap::from([
                    ("src/**/*.ts".to_string(), "./setup/ts.ts".to_string()),
                ])),
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
            (
                "[a-z./]{1,30}",
                proptest::option::of(1u64..3600),
            )
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
                    |(max_iterations, timeout, setup, setup_level, setup_timeout, session_setup, file_setup)| {
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
                        }
                    },
                )
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
}
