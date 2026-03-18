use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shatter_core::cache::BehaviorMapCache;
use shatter_core::discovery::Language as DiscoveryLanguage;
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;

use crate::args::Language;

/// Resolve the project root: explicit `project_dir` wins, otherwise auto-detect from `reference_path`.
pub(crate) fn resolve_project_root(project_dir: Option<&Path>, reference_path: &Path) -> Option<String> {
    if let Some(dir) = project_dir {
        Some(dir.to_string_lossy().into_owned())
    } else {
        shatter_core::project::detect_project_root(reference_path)
            .map(|r| r.path.to_string_lossy().into_owned())
    }
}

/// Strip `root` prefix from `path` to produce a relative path string.
/// Falls back to the full path if stripping fails.
pub(crate) fn relativize_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Terminal color support.
pub(crate) struct Colors {
    pub(crate) bold: &'static str,
    pub(crate) dim: &'static str,
    pub(crate) reset: &'static str,
}

impl Colors {
    pub(crate) fn new(use_color: bool) -> Self {
        if use_color {
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

/// Print Markdown to stdout, rendered with termimad formatting when `use_color` is true.
pub(crate) fn print_markdown(md: &str, use_color: bool) {
    if use_color {
        termimad::print_text(md);
    } else {
        print!("{md}");
    }
}

/// Check for a custom-built frontend binary at `.shatter/bin/shatter-{lang}-custom`.
pub(crate) fn find_custom_binary(shatter_dir: Option<&Path>, lang: &str) -> Option<PathBuf> {
    let bin = shatter_dir?.join("bin").join(format!("shatter-{lang}-custom"));
    bin.is_file().then_some(bin)
}

/// Search PATH for a binary by name, returning the first match.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

pub(crate) fn frontend_config(
    language: Language,
    timeout: Duration,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
    memory_limit: Option<u64>,
    shatter_dir: Option<&Path>,
    timing_enabled: bool,
) -> Result<FrontendConfig, String> {
    let (command, mut args) = match language {
        Language::TypeScript => {
            let bundle_path = crate::embedded_frontend::ensure_extracted()?;
            (
                PathBuf::from("node"),
                vec!["--no-warnings".to_string(), bundle_path.to_string_lossy().into_owned()],
            )
        }
        Language::Go => {
            if let Some(custom) = find_custom_binary(shatter_dir, "go") {
                (custom, vec![])
            } else {
                let binary_path = crate::embedded_go_frontend::ensure_extracted()?;
                (binary_path, vec![])
            }
        }
        Language::Rust => {
            if let Some(custom) = find_custom_binary(shatter_dir, "rust") {
                (custom, vec![])
            } else if let Some(path) = find_on_path("shatter-rust") {
                (path, vec![])
            } else {
                // shatter-rust is outside the workspace, so check both locations
                let candidates = [
                    PathBuf::from("./shatter-rust/target/debug/shatter-rust"),
                    PathBuf::from("./target/debug/shatter-rust"),
                ];
                if let Some(path) = candidates.iter().find(|p| p.is_file()) {
                    (path.clone(), vec![])
                } else {
                    return Err("shatter-rust frontend not found: install it on PATH or build with `cargo build --manifest-path shatter-rust/Cargo.toml`".to_string());
                }
            }
        }
    };

    // Apply memory limit: for TS, --max-old-space-size must come before the script
    if let Some(mb) = memory_limit {
        match language {
            Language::TypeScript => {
                args.insert(0, format!("--max-old-space-size={mb}"));
            }
            Language::Go | Language::Rust => {
                // Go: GOMEMLIMIT is set via env_vars below
                // Rust: no memory limit mechanism yet
            }
        }
    }

    let mut config = FrontendConfig::new(command);
    config.args = args;
    config.request_timeout = timeout;
    apply_frontend_env(&mut config, log_level, exec_timeout, build_timeout);
    if timing_enabled {
        config.capabilities.push("timing".to_string());
    }

    if let Some(mb) = memory_limit
        && language == Language::Go
    {
        let bytes = mb * 1024 * 1024;
        config.env_vars.push(("GOMEMLIMIT".to_string(), format!("{bytes}B")));
    }

    Ok(config)
}

/// Apply standard environment variables to a frontend config.
pub(crate) fn apply_frontend_env(
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

/// For each function's external dependencies that are NOT in the current file,
/// attempts to load the callee's cached behavior map and extract its stored
/// fingerprint. Returns a map from callee name to deep fingerprint.
pub(crate) fn load_external_fingerprints(
    functions: &[shatter_core::protocol::FunctionAnalysis],
    cache: Option<&BehaviorMapCache>,
) -> std::collections::HashMap<String, String> {
    let mut external_fps = std::collections::HashMap::new();
    let cache = match cache {
        Some(c) => c,
        None => return external_fps,
    };

    let local_names: std::collections::HashSet<&str> =
        functions.iter().map(|f| f.name.as_str()).collect();

    for func in functions {
        for dep in &func.dependencies {
            if local_names.contains(dep.symbol.as_str()) {
                continue;
            }
            if external_fps.contains_key(&dep.symbol) {
                continue;
            }
            if let Ok(Some(cached_map)) = cache.load(&dep.symbol)
                && let Some(fp) = cached_map.fingerprint
            {
                external_fps.insert(dep.symbol.clone(), fp);
            }
        }
    }

    external_fps
}

/// Build a [`MetaConfig`] from CLI flags, applying overrides on top of defaults.
pub(crate) fn build_meta_config(
    no_adaptive: bool,
    score_window: Option<usize>,
    cold_start: Option<u64>,
    strategy_floor: Option<f64>,
    strategy_weights: Option<&str>,
) -> Result<shatter_core::strategy::MetaConfig, Box<dyn std::error::Error>> {
    let mut config = shatter_core::config::ExplorationConfig::default();
    if no_adaptive {
        config.adaptive = false;
    }
    if let Some(w) = score_window {
        config.score_window = w;
    }
    if let Some(c) = cold_start {
        config.cold_start = c;
    }
    if let Some(f) = strategy_floor {
        config.strategy_floor = f;
    }
    if let Some(weights_str) = strategy_weights {
        config.strategy_weights =
            Some(shatter_core::config::ExplorationConfig::parse_strategy_weights(weights_str)?);
    }
    Ok(config.to_meta_config())
}

/// Map discovery Language to CLI Language for frontend_config.
pub(crate) fn discovery_lang_to_cli_lang(lang: DiscoveryLanguage) -> Option<Language> {
    match lang {
        DiscoveryLanguage::TypeScript => Some(Language::TypeScript),
        DiscoveryLanguage::Go => Some(Language::Go),
        DiscoveryLanguage::Rust => Some(Language::Rust),
    }
}

/// Shutdown all frontends in a map.
pub(crate) async fn shutdown_all_frontends(frontends: HashMap<DiscoveryLanguage, Frontend>) {
    for (_, frontend) in frontends {
        if let Err(e) = frontend.shutdown().await {
            log::warn!("frontend shutdown error: {e}");
        }
    }
}

pub(crate) async fn shutdown_frontend(frontend: Frontend) {
    if let Err(e) = frontend.shutdown().await {
        log::warn!("frontend shutdown error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_config_passes_timeout_env_vars() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_frontend_env(&mut config, LogLevel::Info, 20, 45);
        let env_map: std::collections::HashMap<_, _> = config.env_vars.iter().cloned().collect();
        assert_eq!(env_map.get("SHATTER_EXEC_TIMEOUT").map(|s| s.as_str()), Some("20"));
        assert_eq!(env_map.get("SHATTER_BUILD_TIMEOUT").map(|s| s.as_str()), Some("45"));
    }

    #[test]
    fn frontend_config_typescript_uses_embedded_bundle() {
        let config = frontend_config(Language::TypeScript, shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT, LogLevel::Info, 10, 30, None, None, false).unwrap();
        assert_eq!(config.command, PathBuf::from("node"));
        assert_eq!(config.request_timeout, shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT);
        // First arg suppresses Node warnings, second is the extracted bundle
        assert_eq!(config.args.len(), 2);
        assert_eq!(config.args[0], "--no-warnings");
        assert!(
            config.args[1].contains("frontend-"),
            "expected embedded bundle path, got: {}",
            config.args[1]
        );
    }

    #[test]
    fn frontend_config_go_uses_embedded_binary() {
        let config = frontend_config(Language::Go, Duration::from_secs(45), LogLevel::Info, 10, 30, None, None, false).unwrap();
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
    fn frontend_config_adds_timing_capability_when_enabled() {
        let config = frontend_config(
            Language::TypeScript,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            true,
        )
        .unwrap();
        assert!(config.capabilities.iter().any(|cap| cap == "timing"));
    }

    #[test]
    fn build_meta_config_defaults() {
        let config = build_meta_config(false, None, None, None, None).unwrap();
        assert!(config.adaptive);
        assert_eq!(config.window_size, shatter_core::config::DEFAULT_EXPLORATION_SCORE_WINDOW);
        assert_eq!(config.cold_start_threshold, shatter_core::config::DEFAULT_EXPLORATION_COLD_START);
        assert!((config.floor - shatter_core::config::DEFAULT_EXPLORATION_STRATEGY_FLOOR).abs() < f64::EPSILON);
        assert!(config.static_weights.is_none());
    }

    #[test]
    fn build_meta_config_with_overrides() {
        let config = build_meta_config(
            true, Some(50), Some(10), Some(0.05), Some("random=0.8,literals=0.2"),
        ).unwrap();
        assert!(!config.adaptive);
        assert_eq!(config.window_size, 50);
        assert_eq!(config.cold_start_threshold, 10);
        assert!((config.floor - 0.05).abs() < f64::EPSILON);
        let weights = config.static_weights.unwrap();
        assert_eq!(weights.len(), 2);
    }

    #[test]
    fn build_meta_config_invalid_weights() {
        let result = build_meta_config(false, None, None, None, Some("bad"));
        assert!(result.is_err());
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
            Some(Language::Rust)
        );
    }
}
