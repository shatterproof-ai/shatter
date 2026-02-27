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

/// When to run the setup file relative to function executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    /// Run setup once before all executions of a function (default).
    PerFunction,
    /// Run setup before each individual execution.
    PerExecution,
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

    /// When to run the setup file.
    #[serde(default)]
    pub setup_mode: Option<SetupMode>,

    /// Type-name-to-generator-file mappings (e.g. `"User": "./generators/user.js"`).
    #[serde(default)]
    pub generators: Option<HashMap<String, String>>,

    /// Param-name-to-generator-file mappings (e.g. `"authToken": "./generators/token.js"`).
    #[serde(default)]
    pub param_generators: Option<HashMap<String, String>>,
}


/// Per-function configuration, matched by glob pattern against function identifiers.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
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

    /// When to run the setup file.
    #[serde(default)]
    pub setup_mode: Option<SetupMode>,

    /// Type-name-to-generator-file mappings, overriding defaults.
    #[serde(default)]
    pub generators: Option<HashMap<String, String>>,

    /// Param-name-to-generator-file mappings, overriding defaults.
    #[serde(default)]
    pub param_generators: Option<HashMap<String, String>>,
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

    /// When to run the setup file (defaults to `PerFunction`).
    pub setup_mode: SetupMode,

    /// Merged type-name-to-generator-file mappings (absolute paths).
    pub generators: HashMap<String, PathBuf>,

    /// Merged param-name-to-generator-file mappings (absolute paths).
    pub param_generators: HashMap<String, PathBuf>,
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
    let mut setup_mode = None;

    // Merge generators maps: start from farthest, overlay nearer.
    // This lets a near config override specific keys while inheriting the rest.
    let mut generators: Option<HashMap<String, String>> = None;
    let mut param_generators: Option<HashMap<String, String>> = None;

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
        if setup_mode.is_none() {
            setup_mode = config.defaults.setup_mode;
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

    ShatterConfig {
        defaults: DefaultsConfig {
            max_iterations,
            timeout,
            setup,
            setup_mode,
            generators,
            param_generators,
        },
        functions,
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

    let setup_mode = func_config
        .and_then(|fc| fc.setup_mode)
        .or(config.defaults.setup_mode)
        .unwrap_or(SetupMode::PerFunction);

    Ok(ResolvedFunctionConfig {
        max_iterations,
        timeout,
        skip,
        candidate_inputs: Vec::new(),
        // Setup and generator paths are resolved to absolute paths by
        // resolve_function_config_with_inputs, which has access to .shatter/ dirs.
        setup: None,
        setup_mode,
        generators: HashMap::new(),
        param_generators: HashMap::new(),
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
                setup_mode: None,
                generators: None,
                param_generators: None,
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
                setup_mode: None,
                generators: None,
                param_generators: None,
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
                setup_mode: None,
                generators: None,
                param_generators: None,
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
                setup_mode: None,
                generators: None,
                param_generators: None,
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
    fn setup_mode_serialization_round_trip() {
        let per_func = SetupMode::PerFunction;
        let json = serde_json::to_string(&per_func).unwrap();
        assert_eq!(json, "\"per_function\"");
        let deserialized: SetupMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SetupMode::PerFunction);

        let per_exec = SetupMode::PerExecution;
        let json = serde_json::to_string(&per_exec).unwrap();
        assert_eq!(json, "\"per_execution\"");
        let deserialized: SetupMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SetupMode::PerExecution);
    }

    #[test]
    fn setup_mode_yaml_round_trip() {
        let yaml = "per_function";
        let mode: SetupMode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mode, SetupMode::PerFunction);

        let yaml = "per_execution";
        let mode: SetupMode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mode, SetupMode::PerExecution);
    }

    #[test]
    fn parse_config_with_setup_and_generators() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.yaml");
        fs::write(
            &config_path,
            r#"
defaults:
  max_iterations: 100
  setup: ./setup/global.ts
  setup_mode: per_execution
  generators:
    User: ./generators/user.ts
    Order: ./generators/order.ts
  param_generators:
    authToken: ./generators/token.ts

functions:
  "src/auth.ts:*":
    setup: ./setup/auth.ts
    setup_mode: per_function
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
        assert_eq!(config.defaults.setup_mode, Some(SetupMode::PerExecution));
        let generators = config.defaults.generators.as_ref().unwrap();
        assert_eq!(generators.len(), 2);
        assert_eq!(generators["User"], "./generators/user.ts");
        assert_eq!(generators["Order"], "./generators/order.ts");
        let param_gens = config.defaults.param_generators.as_ref().unwrap();
        assert_eq!(param_gens["authToken"], "./generators/token.ts");

        // Function overrides
        let auth = &config.functions["src/auth.ts:*"];
        assert_eq!(auth.setup.as_deref(), Some("./setup/auth.ts"));
        assert_eq!(auth.setup_mode, Some(SetupMode::PerFunction));
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
        assert_eq!(config.defaults.setup_mode, None);
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
        let near = ShatterConfig {
            defaults: DefaultsConfig {
                setup: Some("./setup/near.ts".to_string()),
                setup_mode: Some(SetupMode::PerExecution),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                setup: Some("./setup/far.ts".to_string()),
                setup_mode: Some(SetupMode::PerFunction),
                ..DefaultsConfig::default()
            },
            functions: HashMap::new(),
            ..ShatterConfig::default()
        };

        let merged = merge_configs(&[near, far]);
        assert_eq!(merged.defaults.setup.as_deref(), Some("./setup/near.ts"));
        assert_eq!(merged.defaults.setup_mode, Some(SetupMode::PerExecution));
    }

    #[test]
    fn resolve_config_with_setup_from_function() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  setup: ./setup/default.ts
  setup_mode: per_function
functions:
  "myFunc":
    setup: ./setup/custom.ts
    setup_mode: per_execution
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
        assert_eq!(resolved.setup_mode, SetupMode::PerExecution);
    }

    #[test]
    fn resolve_config_with_setup_from_defaults() {
        let root = TempDir::new().unwrap();
        let shatter_dir = root.path().join(".shatter");
        fs::create_dir_all(&shatter_dir).unwrap();

        fs::write(
            shatter_dir.join("config.yaml"),
            r#"
defaults:
  setup: ./setup/default.ts
  setup_mode: per_execution
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
        assert_eq!(resolved.setup_mode, SetupMode::PerExecution);
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
        assert_eq!(resolved.setup_mode, SetupMode::PerFunction); // default
        assert!(resolved.generators.is_empty());
        assert!(resolved.param_generators.is_empty());
    }

    #[test]
    fn resolve_function_config_setup_mode_defaults_to_per_function() {
        let resolved =
            resolve_function_config("any/func", &[], 100, 60).unwrap();
        assert_eq!(resolved.setup_mode, SetupMode::PerFunction);
    }
}
