use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use shatter_core::behavior::BehaviorMap;
use shatter_core::cache::BehaviorMapCache;
use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};
use shatter_core::scan_orchestrator::{self, ScanConfig};
use shatter_core::scope::{ScopeConfig, ScopeMatcher};

mod embedded_frontend;

/// Shatter: automatic exploratory testing via concolic execution.
#[derive(Parser, Debug)]
#[command(name = "shatter", version, about)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
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

/// Build a `FrontendConfig` for the given language.
fn frontend_config(language: Language, timeout: Duration) -> Result<FrontendConfig, String> {
    let (command, args) = match language {
        Language::TypeScript => {
            let bundle_path = embedded_frontend::ensure_extracted()?;
            (
                PathBuf::from("node"),
                vec![bundle_path.to_string_lossy().into_owned()],
            )
        }
        Language::Go => (
            PathBuf::from("shatter-go/shatter-go"),
            vec![],
        ),
    };

    let mut config = FrontendConfig::new(command);
    config.args = args;
    config.request_timeout = timeout;
    Ok(config)
}

/// Run the explore command.
async fn run_explore(
    targets: &[String],
    max_iterations: u32,
    timeout: u64,
    scope_path: Option<&Path>,
    analyze_only: bool,
    _show_clusters: bool,
    cache_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope_config = match scope_path {
        Some(path) => {
            let config = ScopeConfig::from_file(path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            println!("Loaded scope config from {}", path.display());
            config
        }
        None => ScopeConfig::default(),
    };

    let _scope_matcher = ScopeMatcher::new(&scope_config)
        .map_err(|e| format!("invalid scope config: {e}"))?;

    let cache = {
        let dir = match cache_dir {
            Some(p) => p.to_path_buf(),
            None => BehaviorMapCache::default_dir(&std::env::current_dir()?),
        };
        BehaviorMapCache::new(dir).map_err(|e| format!("failed to initialize cache: {e}"))?
    };

    let parsed: Vec<Target> = targets
        .iter()
        .map(|t| parse_target(t))
        .collect::<Result<Vec<_>, _>>()?;

    let request_timeout = Duration::from_secs(timeout);

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        let func_display = target
            .function
            .as_deref()
            .unwrap_or("(all)");

        println!(
            "Exploring {file_str}:{func_display} [language={}, max_iterations={max_iterations}]",
            target.language.label()
        );

        let config = frontend_config(target.language, request_timeout)?;
        let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
            format!(
                "failed to spawn {} frontend: {e}",
                target.language.label()
            )
        })?;

        println!(
            "  Frontend connected (language={})",
            frontend.language().unwrap_or("unknown")
        );

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
                println!("  Found {} function(s):", functions.len());
                for func in functions {
                    println!("    - {} ({} params, {} branches)",
                        func.name,
                        func.params.len(),
                        func.branches.len(),
                    );
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
            shutdown_frontend(frontend).await;
            continue;
        }

        // Exploration phase: generate random inputs and execute
        let explore_config = ExploreConfig {
            max_iterations,
            seed: None,
            mocks: vec![],
        };

        for func in &functions {
            println!("\n  Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config).await {
                Ok(result) => {
                    print!("{}", explorer::format_exploration_report(&result));

                    let behavior_map =
                        BehaviorMap::from_exploration_result(&func.name, &result);
                    if let Err(e) = cache.store(&behavior_map) {
                        eprintln!("  Warning: failed to cache behavior map for {}: {e}", func.name);
                    }
                }
                Err(e) => {
                    eprintln!("  Exploration error for {}: {e}", func.name);
                }
            }
        }

        shutdown_frontend(frontend).await;
        println!();
    }

    Ok(())
}

/// Run the scan command: explore multiple functions in dependency order.
async fn run_scan(
    targets: &[String],
    max_iterations: u32,
    timeout: u64,
    scope_path: Option<&Path>,
    analyze_only: bool,
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

    let request_timeout = Duration::from_secs(timeout);
    let config = frontend_config(first_lang, request_timeout)?;
    let mut frontend = Frontend::spawn(&config).await.map_err(|e| {
        format!(
            "failed to spawn {} frontend: {e}",
            first_lang.label()
        )
    })?;

    println!(
        "Frontend connected (language={})",
        frontend.language().unwrap_or("unknown")
    );

    // Analyze all targets to collect FunctionAnalysis data.
    let mut all_analyses = Vec::new();

    for target in &parsed {
        let file_str = target.file.to_string_lossy();
        println!(
            "Analyzing {file_str}:{}",
            target.function.as_deref().unwrap_or("(all)")
        );

        let analyze_response = frontend
            .send(ProtoCommand::Analyze {
                file: file_str.to_string(),
                function: target.function.clone(),
            })
            .await
            .map_err(|e| format!("analyze failed: {e}"))?;

        match analyze_response.result {
            ResponseResult::Analyze { functions } => {
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

    if all_analyses.is_empty() {
        eprintln!("No functions found to scan.");
        shutdown_frontend(frontend).await;
        return Ok(());
    }

    println!(
        "\nScanning {} function(s) in dependency order...\n",
        all_analyses.len()
    );

    let scan_config = ScanConfig {
        max_iterations_per_function: max_iterations,
        seed: None,
    };

    match scan_orchestrator::scan(&mut frontend, &all_analyses, &scan_config).await {
        Ok(result) => {
            print!("{}", scan_orchestrator::format_scan_report(&result));
        }
        Err(e) => {
            eprintln!("Scan error: {e}");
        }
    }

    shutdown_frontend(frontend).await;
    Ok(())
}

async fn shutdown_frontend(frontend: Frontend) {
    if let Err(e) = frontend.shutdown().await {
        eprintln!("  Warning: frontend shutdown error: {e}");
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        CliCommand::Explore {
            targets,
            max_iterations,
            timeout,
            scope,
            analyze_only,
            show_clusters,
            cache_dir,
        } => {
            run_explore(
                &targets,
                max_iterations,
                timeout,
                scope.as_deref(),
                analyze_only,
                show_clusters,
                cache_dir.as_deref(),
            )
            .await
        }
        CliCommand::Scan {
            targets,
            max_iterations,
            timeout,
            scope,
            analyze_only,
        } => {
            run_scan(
                &targets,
                max_iterations,
                timeout,
                scope.as_deref(),
                analyze_only,
            )
            .await
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
            } => {
                assert_eq!(targets, vec!["test.ts:myFunc"]);
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 60);
                assert!(scope.is_none());
                assert!(!analyze_only);
                assert!(!show_clusters);
                assert!(cache_dir.is_none());
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
            } => {
                assert_eq!(targets, vec!["a.ts:fn1", "b.go:Fn2"]);
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 120);
                assert!(scope.is_none());
                assert!(analyze_only);
                assert!(!show_clusters);
                assert!(cache_dir.is_none());
            }
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
            } => {
                assert_eq!(targets, vec!["test.ts"]);
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 120);
                assert!(scope.is_none());
                assert!(!analyze_only);
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
                ..
            } => {
                assert_eq!(targets, vec!["a.ts", "b.ts:helperFn"]);
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 300);
                assert!(analyze_only);
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
        let config = frontend_config(Language::TypeScript, Duration::from_secs(30)).unwrap();
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
    fn frontend_config_go_defaults() {
        let config = frontend_config(Language::Go, Duration::from_secs(45)).unwrap();
        assert_eq!(config.command, PathBuf::from("shatter-go/shatter-go"));
        assert_eq!(config.request_timeout, Duration::from_secs(45));
    }
}
