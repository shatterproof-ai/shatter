use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shatter_core::cache::BehaviorMapCache;
use shatter_core::discovery::Language as DiscoveryLanguage;
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::log_level::LogLevel;

use crate::args::Language;

/// Lower bound on resolved parallelism. Avoids sub-saturating small machines and
/// keeps benchmarks comparable across hosts.
pub(crate) const PARALLELISM_FLOOR: usize = 4;

/// Upper bound on resolved parallelism. Each frontend may shell out to a
/// multi-process toolchain (e.g. `go build` with `GOMAXPROCS=nproc`); without a
/// ceiling, large hosts fork-bomb themselves into OOM. See str-eam2.
pub(crate) const PARALLELISM_CEILING: usize = 16;

/// Resolve the effective `--parallelism` from a user request.
///
/// `requested == 0` means "auto-detect": query `available_parallelism()` and
/// clamp into `[PARALLELISM_FLOOR, PARALLELISM_CEILING]`. An explicit non-zero
/// value is also clamped to the same range, with a warning logged when the
/// clamp changes the value so the user knows the bound is in effect.
///
/// User-overridable floor/ceiling are tracked separately (str-v01r).
pub(crate) fn resolve_parallelism(requested: usize) -> usize {
    if requested == 0 {
        let detected = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        return detected.clamp(PARALLELISM_FLOOR, PARALLELISM_CEILING);
    }
    let clamped = requested.clamp(PARALLELISM_FLOOR, PARALLELISM_CEILING);
    if clamped != requested {
        log::warn!(
            "--parallelism {requested} clamped to {clamped} (range [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}])"
        );
    }
    clamped
}

/// Per-language ceiling on auto-detected parallelism. Multi-process toolchains
/// (Go's `go build`, Rust's `cargo`) consume substantially more host resources
/// per worker than a single-process Node frontend, so the auto-detected
/// default must be capped tighter when these languages participate.
///
/// Explicit `--parallelism N` requests are NOT capped by this table — only the
/// auto-detect path is — so power users can opt past the per-language default
/// up to the global ceiling.
pub(crate) const TS_AUTODETECT_CAP: usize = usize::MAX;
pub(crate) const GO_AUTODETECT_CAP: usize = 8;
pub(crate) const RUST_AUTODETECT_CAP: usize = 8;

/// Per-language autodetect cap for a single language. See `TS_AUTODETECT_CAP`
/// et al. for rationale.
fn language_autodetect_cap(lang: DiscoveryLanguage) -> usize {
    match lang {
        DiscoveryLanguage::TypeScript => TS_AUTODETECT_CAP,
        DiscoveryLanguage::Go => GO_AUTODETECT_CAP,
        DiscoveryLanguage::Rust => RUST_AUTODETECT_CAP,
    }
}

/// Worst-case (minimum) per-language autodetect cap across `needed_langs`.
/// An empty set yields `usize::MAX` (no per-language cap), preserving the
/// pre-str-qp31 behavior when language detection has not yet happened.
pub(crate) fn per_language_autodetect_cap<'a, I>(needed_langs: I) -> usize
where
    I: IntoIterator<Item = &'a DiscoveryLanguage>,
{
    needed_langs
        .into_iter()
        .map(|l| language_autodetect_cap(*l))
        .min()
        .unwrap_or(usize::MAX)
}

/// Resolve effective parallelism for a scan, taking the participating
/// languages into account (str-qp31).
///
/// For `requested == 0` (auto-detect): take `available_parallelism()`, apply
/// the per-language cap (worst-case-wins for mixed-language scans), then apply
/// the global `[PARALLELISM_FLOOR, PARALLELISM_CEILING]` clamp from str-eam2.
///
/// For an explicit non-zero `requested`: only the global clamp applies — the
/// per-language table governs the *default*, not user-supplied values.
pub(crate) fn resolve_parallelism_for_langs<'a, I>(requested: usize, needed_langs: I) -> usize
where
    I: IntoIterator<Item = &'a DiscoveryLanguage>,
{
    if requested == 0 {
        let detected = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let lang_cap = per_language_autodetect_cap(needed_langs);
        let after_lang_cap = detected.min(lang_cap);
        return after_lang_cap.clamp(PARALLELISM_FLOOR, PARALLELISM_CEILING);
    }
    resolve_parallelism(requested)
}

/// Resolve the project root: explicit `project_dir` wins, otherwise auto-detect from `reference_path`.
pub(crate) fn resolve_project_root(
    project_dir: Option<&Path>,
    reference_path: &Path,
) -> Option<String> {
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

/// Check for a custom-built frontend binary at `.shatter-cache/bin/shatter-{lang}-custom`.
///
/// Also checks legacy `.shatter/bin/` for backward compatibility.
pub(crate) fn find_custom_binary(shatter_dir: Option<&Path>, lang: &str) -> Option<PathBuf> {
    let binary_name = format!("shatter-{lang}-custom");
    // Check new location: .shatter-cache/bin/
    let cache_bin = PathBuf::from(".shatter-cache")
        .join("bin")
        .join(&binary_name);
    if cache_bin.is_file() {
        return Some(cache_bin);
    }
    // Fall back to legacy .shatter/bin/
    let bin = shatter_dir?.join("bin").join(&binary_name);
    bin.is_file().then_some(bin)
}

/// Search PATH for a binary by name, returning the first match.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn frontend_config(
    language: Language,
    timeout: Duration,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
    memory_limit: Option<u64>,
    shatter_dir: Option<&Path>,
    timing_enabled: bool,
    release: bool,
) -> Result<FrontendConfig, String> {
    let (command, mut args) = match language {
        Language::TypeScript => {
            let bundle_path = crate::embedded_frontend::ensure_extracted()?;
            (
                PathBuf::from("node"),
                vec![
                    "--no-warnings".to_string(),
                    bundle_path.to_string_lossy().into_owned(),
                ],
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
    apply_frontend_env(&mut config, log_level, exec_timeout, build_timeout, release);
    if timing_enabled {
        config.capabilities.push("timing".to_string());
    }

    if let Some(mb) = memory_limit
        && language == Language::Go
    {
        let bytes = mb * 1024 * 1024;
        config
            .env_vars
            .push(("GOMEMLIMIT".to_string(), format!("{bytes}B")));
    }

    Ok(config)
}

/// Apply harness storage environment variables to a frontend config.
pub(crate) fn apply_storage_env(
    config: &mut FrontendConfig,
    storage: &shatter_core::harness_storage::HarnessStorage,
) {
    for (key, value) in storage.env_vars() {
        config.env_vars.push((key, value));
    }
}

/// Apply project-scoped harness storage env vars to a frontend config.
///
/// When `project_root` is `Some`, creates a [`HarnessStorage`] with
/// project-scoped cache and artifact directories.  When `None`, the storage
/// roots fall back to temp-based paths (no durable cache).
pub(crate) fn apply_project_storage(config: &mut FrontendConfig, project_root: Option<&str>) {
    if let Some(root) = project_root {
        let storage = shatter_core::harness_storage::HarnessStorage::for_project(Path::new(root));
        apply_storage_env(config, &storage);
    }
}

/// Apply standard environment variables to a frontend config.
pub(crate) fn apply_frontend_env(
    config: &mut FrontendConfig,
    log_level: LogLevel,
    exec_timeout: u64,
    build_timeout: u64,
    release: bool,
) {
    config.env_vars.push((
        LogLevel::ENV_VAR.to_string(),
        log_level.as_str().to_string(),
    ));
    config
        .env_vars
        .push(("SHATTER_EXEC_TIMEOUT".to_string(), exec_timeout.to_string()));
    config.env_vars.push((
        "SHATTER_BUILD_TIMEOUT".to_string(),
        build_timeout.to_string(),
    ));
    if release {
        config
            .env_vars
            .push(("SHATTER_HARNESS_RELEASE".to_string(), "1".to_string()));
    }
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

/// Default max-iterations when the user does not provide `--max-iterations`.
pub(crate) const DEFAULT_MAX_ITERATIONS: u32 = 100;
/// Default total-timeout (seconds) when the user does not provide `--timeout`.
pub(crate) const DEFAULT_TIMEOUT: u64 = 60;

/// Resolved exploration budget, accounting for MC/DC multipliers.
pub(crate) struct ResolvedBudgets {
    /// Effective max-iterations. `None` means unbounded (run until timeout/interrupt).
    pub max_iterations: Option<u32>,
    /// Effective wall-clock timeout in seconds (user value or MC/DC-scaled default).
    pub timeout: u64,
    /// Effective per-query solver timeout in seconds (user value, or 10s under MC/DC, or None).
    pub solver_timeout: Option<u64>,
}

/// Resolve exploration budgets from optional user-provided values, applying MC/DC
/// multipliers to any parameter the user did not explicitly set.
///
/// When `mcdc` is true and a parameter is `None` (not user-provided), the MC/DC
/// default is used (5× for iterations, 5× for timeout, 10 s for solver timeout).
/// When a parameter is `Some`, the user-provided value is used unchanged.
/// When `mcdc` is false and `max_iterations` is `None`, returns
/// `Some(DEFAULT_MAX_ITERATIONS)` (bounded by default).
///
/// Pass `--max-iterations 0` to opt into unbounded exploration.
pub(crate) fn resolve_mcdc_budgets(
    max_iterations: Option<u32>,
    timeout: Option<u64>,
    solver_timeout: Option<u64>,
    mcdc: bool,
) -> ResolvedBudgets {
    ResolvedBudgets {
        max_iterations: match max_iterations {
            Some(0) => None, // explicit opt-in to unbounded
            Some(n) => Some(n),
            None if mcdc => Some(DEFAULT_MAX_ITERATIONS * 5),
            None => Some(DEFAULT_MAX_ITERATIONS),
        },
        timeout: timeout.unwrap_or(if mcdc {
            DEFAULT_TIMEOUT * 5
        } else {
            DEFAULT_TIMEOUT
        }),
        solver_timeout: if mcdc && solver_timeout.is_none() {
            Some(10)
        } else {
            solver_timeout
        },
    }
}

#[cfg(test)]
mod mcdc_budget_tests {
    use super::*;

    #[test]
    fn mcdc_default_budgets_are_scaled() {
        let b = resolve_mcdc_budgets(None, None, None, true);
        assert_eq!(
            b.max_iterations,
            Some(DEFAULT_MAX_ITERATIONS * 5),
            "max_iterations should be 5x"
        );
        assert_eq!(b.timeout, DEFAULT_TIMEOUT * 5, "timeout should be 5x");
        assert_eq!(
            b.solver_timeout,
            Some(10),
            "solver_timeout should default to 10s under mcdc"
        );
    }

    #[test]
    fn non_mcdc_default_budgets_are_bounded() {
        let b = resolve_mcdc_budgets(None, None, None, false);
        assert_eq!(
            b.max_iterations,
            Some(DEFAULT_MAX_ITERATIONS),
            "no --max-iterations defaults to DEFAULT_MAX_ITERATIONS"
        );
        assert_eq!(b.timeout, DEFAULT_TIMEOUT);
        assert_eq!(b.solver_timeout, None);
    }

    #[test]
    fn zero_max_iterations_means_unbounded() {
        let b = resolve_mcdc_budgets(Some(0), None, None, false);
        assert_eq!(b.max_iterations, None, "--max-iterations 0 means unbounded");
    }

    #[test]
    fn user_provided_values_override_mcdc_defaults() {
        let b = resolve_mcdc_budgets(Some(42), Some(30), Some(5), true);
        assert_eq!(
            b.max_iterations,
            Some(42),
            "user-provided max_iterations must not be multiplied"
        );
        assert_eq!(
            b.timeout, 30,
            "user-provided timeout must not be multiplied"
        );
        assert_eq!(
            b.solver_timeout,
            Some(5),
            "user-provided solver_timeout must not be changed"
        );
    }

    #[test]
    fn partial_user_override_with_mcdc() {
        // User provides max_iterations but not timeout or solver_timeout
        let b = resolve_mcdc_budgets(Some(200), None, None, true);
        assert_eq!(b.max_iterations, Some(200), "user value wins");
        assert_eq!(
            b.timeout,
            DEFAULT_TIMEOUT * 5,
            "unspecified timeout gets mcdc scaling"
        );
        assert_eq!(
            b.solver_timeout,
            Some(10),
            "unspecified solver_timeout gets mcdc default"
        );
    }
}

#[cfg(test)]
mod cli_parity_tests {
    use clap::Parser;

    use super::*;
    use crate::args::{Cli, CliCommand};

    /// CLI parity contract: the canonical list of environment variables the CLI must
    /// set for every frontend invocation, with their expected default values when the
    /// user does not provide the corresponding flag.
    ///
    /// Governed commands: `explore`, `scan`, and other frontend-spawning subcommands
    /// that do not have intentionally elevated defaults (e.g. `observe` uses 30s/60s
    /// because it executes many inputs in a single session — that divergence is
    /// documented in PARITY.md).
    const GOVERNED_ENV_VARS: &[&str] = &[
        "SHATTER_LOG_LEVEL",
        "SHATTER_EXEC_TIMEOUT",
        "SHATTER_BUILD_TIMEOUT",
    ];
    /// Canonical CLI default for `--exec-timeout` (seconds) across governed commands.
    const CLI_EXEC_TIMEOUT_DEFAULT_SECS: u64 = 10;
    /// Canonical CLI default for `--build-timeout` (seconds) across governed commands.
    const CLI_BUILD_TIMEOUT_DEFAULT_SECS: u64 = 30;
    /// Canonical CLI default for `--log-level`.
    const CLI_LOG_LEVEL_DEFAULT: &str = "info";

    /// Every governed env var must appear in the env_vars vector produced by
    /// `apply_frontend_env`. This is the minimal contract: if a var is missing,
    /// the frontend never receives it regardless of what the CLI flag says.
    #[test]
    fn apply_frontend_env_sets_all_governed_vars() {
        let mut config = FrontendConfig::new(std::path::PathBuf::from("dummy"));
        apply_frontend_env(
            &mut config,
            LogLevel::Info,
            CLI_EXEC_TIMEOUT_DEFAULT_SECS,
            CLI_BUILD_TIMEOUT_DEFAULT_SECS,
            false,
        );
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in GOVERNED_ENV_VARS {
            assert!(
                keys.contains(var),
                "apply_frontend_env must set governed env var {var} — \
                 add it to apply_frontend_env() in helpers.rs"
            );
        }
    }

    /// The governed env vars must carry the correct values matching the contract
    /// constants, not arbitrary defaults.
    #[test]
    fn apply_frontend_env_values_match_contract_defaults() {
        let mut config = FrontendConfig::new(std::path::PathBuf::from("dummy"));
        apply_frontend_env(
            &mut config,
            LogLevel::Info,
            CLI_EXEC_TIMEOUT_DEFAULT_SECS,
            CLI_BUILD_TIMEOUT_DEFAULT_SECS,
            false,
        );
        let env_map: std::collections::HashMap<&str, &str> = config
            .env_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        assert_eq!(
            env_map.get("SHATTER_LOG_LEVEL").copied(),
            Some(CLI_LOG_LEVEL_DEFAULT),
            "SHATTER_LOG_LEVEL default must be {CLI_LOG_LEVEL_DEFAULT}"
        );
        assert_eq!(
            env_map.get("SHATTER_EXEC_TIMEOUT").copied(),
            Some(CLI_EXEC_TIMEOUT_DEFAULT_SECS.to_string().as_str()),
            "SHATTER_EXEC_TIMEOUT default must be {CLI_EXEC_TIMEOUT_DEFAULT_SECS}"
        );
        assert_eq!(
            env_map.get("SHATTER_BUILD_TIMEOUT").copied(),
            Some(CLI_BUILD_TIMEOUT_DEFAULT_SECS.to_string().as_str()),
            "SHATTER_BUILD_TIMEOUT default must be {CLI_BUILD_TIMEOUT_DEFAULT_SECS}"
        );
    }

    /// The `explore` subcommand must expose `--exec-timeout` and `--build-timeout`
    /// with the governed defaults. If a future edit changes the default_value_t,
    /// this test fails and forces a PARITY.md update.
    #[test]
    fn explore_defaults_match_parity_contract() {
        let cli = Cli::parse_from(["shatter", "explore", "dummy.ts"]);
        match cli.command {
            CliCommand::Explore {
                exec_timeout,
                build_timeout,
                ..
            } => {
                assert_eq!(
                    exec_timeout, CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                    "`explore --exec-timeout` default ({exec_timeout}s) diverges from \
                     parity contract ({CLI_EXEC_TIMEOUT_DEFAULT_SECS}s); \
                     update the contract constant or restore the arg default"
                );
                assert_eq!(
                    build_timeout, CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                    "`explore --build-timeout` default ({build_timeout}s) diverges from \
                     parity contract ({CLI_BUILD_TIMEOUT_DEFAULT_SECS}s); \
                     update the contract constant or restore the arg default"
                );
            }
            _ => panic!("expected Explore command"),
        }
    }

    /// The `scan` subcommand must expose the same governed defaults as `explore`.
    /// exec_timeout is Option<u64> (resolved via project config chain); when
    /// no flag is passed, the built-in default in shatter-core matches the
    /// parity contract.
    #[test]
    fn scan_defaults_match_parity_contract() {
        let cli = Cli::parse_from(["shatter", "scan", "src/"]);
        match cli.command {
            CliCommand::Scan {
                exec_timeout,
                build_timeout,
                ..
            } => {
                // exec_timeout is None when not explicitly passed; the built-in
                // default (resolved at runtime) matches the parity contract.
                assert_eq!(
                    exec_timeout.unwrap_or(shatter_core::config::DEFAULT_SCAN_EXEC_TIMEOUT),
                    CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                    "`scan --exec-timeout` resolved default diverges from \
                     parity contract ({CLI_EXEC_TIMEOUT_DEFAULT_SECS}s)"
                );
                assert_eq!(
                    build_timeout, CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                    "`scan --build-timeout` default ({build_timeout}s) diverges from \
                     parity contract ({CLI_BUILD_TIMEOUT_DEFAULT_SECS}s)"
                );
            }
            _ => panic!("expected Scan command"),
        }
    }

    /// Every language frontend config must include all governed env vars.
    /// Tests TypeScript and Go (Rust frontend requires the binary on PATH so is
    /// skipped here; its env-var handling is tested in shatter-rust unit tests).
    #[test]
    fn frontend_config_propagates_all_governed_vars() {
        for lang in [Language::TypeScript, Language::Go] {
            let config = frontend_config(
                lang,
                shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
                LogLevel::Info,
                CLI_EXEC_TIMEOUT_DEFAULT_SECS,
                CLI_BUILD_TIMEOUT_DEFAULT_SECS,
                None,
                None,
                false,
                false,
            )
            .unwrap_or_else(|e| panic!("frontend_config({lang:?}) failed: {e}"));

            let keys: std::collections::HashSet<&str> =
                config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
            for var in GOVERNED_ENV_VARS {
                assert!(
                    keys.contains(var),
                    "frontend_config({lang:?}) must propagate governed env var {var}"
                );
            }
        }
    }

    /// `apply_storage_env` must set all three storage env vars.
    #[test]
    fn apply_storage_env_sets_all_storage_vars() {
        use shatter_core::harness_storage::{
            ENV_ARTIFACT_DIR, ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, HarnessStorage,
        };
        let storage = HarnessStorage::for_project(Path::new("/tmp/test"));
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_storage_env(&mut config, &storage);
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in [ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, ENV_ARTIFACT_DIR] {
            assert!(keys.contains(var), "apply_storage_env must set {var}");
        }
    }

    /// `apply_project_storage` sets storage vars when a project root is provided.
    #[test]
    fn apply_project_storage_with_root() {
        use shatter_core::harness_storage::{
            ENV_ARTIFACT_DIR, ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH,
        };
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_project_storage(&mut config, Some("/tmp/project"));
        let keys: std::collections::HashSet<&str> =
            config.env_vars.iter().map(|(k, _)| k.as_str()).collect();
        for var in [ENV_HARNESS_CACHE, ENV_HARNESS_SCRATCH, ENV_ARTIFACT_DIR] {
            assert!(keys.contains(var), "apply_project_storage must set {var}");
        }
    }

    /// `apply_project_storage` is a no-op when project root is None.
    #[test]
    fn apply_project_storage_without_root() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        let before = config.env_vars.len();
        apply_project_storage(&mut config, None);
        assert_eq!(
            config.env_vars.len(),
            before,
            "no vars should be added when project_root is None"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_parallelism_clamps_explicit_above_ceiling() {
        assert_eq!(resolve_parallelism(32), PARALLELISM_CEILING);
        assert_eq!(resolve_parallelism(PARALLELISM_CEILING + 1), PARALLELISM_CEILING);
    }

    #[test]
    fn resolve_parallelism_clamps_explicit_below_floor() {
        assert_eq!(resolve_parallelism(1), PARALLELISM_FLOOR);
        assert_eq!(resolve_parallelism(PARALLELISM_FLOOR - 1), PARALLELISM_FLOOR);
    }

    #[test]
    fn resolve_parallelism_passes_explicit_in_range() {
        let mid = (PARALLELISM_FLOOR + PARALLELISM_CEILING) / 2;
        assert_eq!(resolve_parallelism(mid), mid);
        assert_eq!(resolve_parallelism(PARALLELISM_FLOOR), PARALLELISM_FLOOR);
        assert_eq!(resolve_parallelism(PARALLELISM_CEILING), PARALLELISM_CEILING);
    }

    #[test]
    fn resolve_parallelism_autodetect_is_in_range() {
        let v = resolve_parallelism(0);
        assert!((PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "auto-detected parallelism {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]");
    }

    // ---- str-qp31: per-language parallelism defaults ----

    #[test]
    fn per_lang_cap_ts_only_is_unbounded() {
        let langs = [DiscoveryLanguage::TypeScript];
        assert_eq!(per_language_autodetect_cap(&langs), TS_AUTODETECT_CAP);
        assert_eq!(TS_AUTODETECT_CAP, usize::MAX);
    }

    #[test]
    fn per_lang_cap_go_only_is_eight() {
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(per_language_autodetect_cap(&langs), GO_AUTODETECT_CAP);
        assert_eq!(GO_AUTODETECT_CAP, 8);
    }

    #[test]
    fn per_lang_cap_rust_only_is_eight() {
        let langs = [DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&langs), RUST_AUTODETECT_CAP);
        assert_eq!(RUST_AUTODETECT_CAP, 8);
    }

    #[test]
    fn per_lang_cap_mixed_takes_worst_case() {
        // TS is unbounded, Go is 8 → mixed must be 8 (the tighter cap wins).
        let mixed_ts_go = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Go];
        assert_eq!(per_language_autodetect_cap(&mixed_ts_go), 8);

        let mixed_ts_rust = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&mixed_ts_rust), 8);

        let mixed_go_rust = [DiscoveryLanguage::Go, DiscoveryLanguage::Rust];
        assert_eq!(per_language_autodetect_cap(&mixed_go_rust), 8);

        let all_three = [
            DiscoveryLanguage::TypeScript,
            DiscoveryLanguage::Go,
            DiscoveryLanguage::Rust,
        ];
        assert_eq!(per_language_autodetect_cap(&all_three), 8);
    }

    #[test]
    fn per_lang_cap_empty_is_unbounded() {
        let langs: [DiscoveryLanguage; 0] = [];
        assert_eq!(per_language_autodetect_cap(&langs), usize::MAX);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_ts_only_uses_global_clamp() {
        // TS-only autodetect: capped only by [FLOOR, CEILING].
        let langs = [DiscoveryLanguage::TypeScript];
        let v = resolve_parallelism_for_langs(0, &langs);
        assert!(
            (PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "TS-only autodetect {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]"
        );
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_go_only_capped_at_eight() {
        let langs = [DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs);
        assert!(
            (PARALLELISM_FLOOR..=GO_AUTODETECT_CAP).contains(&v),
            "Go-only autodetect {v} outside [{PARALLELISM_FLOOR}, {GO_AUTODETECT_CAP}]"
        );
        // The global ceiling is still 16, but the Go cap is tighter — we must
        // never exceed 8 in this branch even on a 32-core host.
        assert!(v <= GO_AUTODETECT_CAP);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_rust_only_capped_at_eight() {
        let langs = [DiscoveryLanguage::Rust];
        let v = resolve_parallelism_for_langs(0, &langs);
        assert!(
            (PARALLELISM_FLOOR..=RUST_AUTODETECT_CAP).contains(&v),
            "Rust-only autodetect {v} outside [{PARALLELISM_FLOOR}, {RUST_AUTODETECT_CAP}]"
        );
        assert!(v <= RUST_AUTODETECT_CAP);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_mixed_takes_worst_case() {
        let langs = [DiscoveryLanguage::TypeScript, DiscoveryLanguage::Go];
        let v = resolve_parallelism_for_langs(0, &langs);
        // Mixed TS+Go: worst case (Go=8) wins.
        assert!(v <= GO_AUTODETECT_CAP);
        assert!(v >= PARALLELISM_FLOOR);
    }

    #[test]
    fn resolve_parallelism_for_langs_explicit_value_ignores_lang_cap() {
        // Per spec: per-language table governs the *default*, not explicit
        // user requests. An explicit --parallelism 12 with Go should still
        // produce 12 (clamped only by the global [4, 16] range).
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(resolve_parallelism_for_langs(12, &langs), 12);
        assert_eq!(resolve_parallelism_for_langs(16, &langs), 16);
    }

    #[test]
    fn resolve_parallelism_for_langs_explicit_value_still_clamps_to_global() {
        // Explicit values must still pass through the [4, 16] clamp from str-eam2.
        let langs = [DiscoveryLanguage::Go];
        assert_eq!(resolve_parallelism_for_langs(32, &langs), PARALLELISM_CEILING);
        assert_eq!(resolve_parallelism_for_langs(1, &langs), PARALLELISM_FLOOR);
    }

    #[test]
    fn resolve_parallelism_for_langs_autodetect_empty_set_matches_legacy() {
        // An empty needed_langs set must behave like the language-agnostic
        // resolve_parallelism: clamped only by the global range.
        let langs: [DiscoveryLanguage; 0] = [];
        let v = resolve_parallelism_for_langs(0, &langs);
        assert!(
            (PARALLELISM_FLOOR..=PARALLELISM_CEILING).contains(&v),
            "empty-langs autodetect {v} outside [{PARALLELISM_FLOOR}, {PARALLELISM_CEILING}]"
        );
    }

    #[test]
    fn frontend_config_passes_timeout_env_vars() {
        let mut config = FrontendConfig::new(PathBuf::from("dummy"));
        apply_frontend_env(&mut config, LogLevel::Info, 20, 45, false);
        let env_map: std::collections::HashMap<_, _> = config.env_vars.iter().cloned().collect();
        assert_eq!(
            env_map.get("SHATTER_EXEC_TIMEOUT").map(|s| s.as_str()),
            Some("20")
        );
        assert_eq!(
            env_map.get("SHATTER_BUILD_TIMEOUT").map(|s| s.as_str()),
            Some("45")
        );
    }

    #[test]
    fn frontend_config_typescript_uses_embedded_bundle() {
        let config = frontend_config(
            Language::TypeScript,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT,
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
        assert_eq!(config.command, PathBuf::from("node"));
        assert_eq!(
            config.request_timeout,
            shatter_core::frontend::DEFAULT_REQUEST_TIMEOUT
        );
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
        let config = frontend_config(
            Language::Go,
            Duration::from_secs(45),
            LogLevel::Info,
            10,
            30,
            None,
            None,
            false,
            false,
        )
        .unwrap();
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
            false,
        )
        .unwrap();
        assert!(config.capabilities.iter().any(|cap| cap == "timing"));
    }

    #[test]
    fn build_meta_config_defaults() {
        let config = build_meta_config(false, None, None, None, None).unwrap();
        assert!(config.adaptive);
        assert_eq!(
            config.window_size,
            shatter_core::config::DEFAULT_EXPLORATION_SCORE_WINDOW
        );
        assert_eq!(
            config.cold_start_threshold,
            shatter_core::config::DEFAULT_EXPLORATION_COLD_START
        );
        assert!(
            (config.floor - shatter_core::config::DEFAULT_EXPLORATION_STRATEGY_FLOOR).abs()
                < f64::EPSILON
        );
        assert!(config.static_weights.is_none());
    }

    #[test]
    fn build_meta_config_with_overrides() {
        let config = build_meta_config(
            true,
            Some(50),
            Some(10),
            Some(0.05),
            Some("random=0.8,literals=0.2"),
        )
        .unwrap();
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
