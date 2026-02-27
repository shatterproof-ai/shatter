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

    for config in configs {
        if max_iterations.is_none() {
            max_iterations = config.defaults.max_iterations;
        }
        if timeout.is_none() {
            timeout = config.defaults.timeout;
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

    Ok(ResolvedFunctionConfig {
        max_iterations,
        timeout,
        skip,
        candidate_inputs: Vec::new(),
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
                    return Ok(resolved);
                }
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
            },
            functions: HashMap::new(),
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(50),
                timeout: Some(120),
            },
            functions: HashMap::new(),
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
            },
        );

        let near = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions: near_funcs,
        };
        let far = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions: far_funcs,
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
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig {
                max_iterations: Some(100),
                timeout: Some(60),
            },
            functions,
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
            },
            functions: HashMap::new(),
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
            },
        );

        let config = ShatterConfig {
            defaults: DefaultsConfig::default(),
            functions,
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
}
