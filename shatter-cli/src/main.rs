use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use shatter_core::explorer::{self, ExploreConfig};
use shatter_core::frontend::{Frontend, FrontendConfig};
use shatter_core::protocol::{Command as ProtoCommand, ResponseResult};

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

        /// Only run the analyze phase (skip exploration).
        #[arg(long)]
        analyze_only: bool,

        /// Show behavior clusters after exploration.
        #[arg(long)]
        show_clusters: bool,
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
fn frontend_config(language: Language, timeout: Duration) -> FrontendConfig {
    let (command, args) = match language {
        Language::TypeScript => (
            PathBuf::from("node"),
            vec!["shatter-ts/dist/main.js".to_string()],
        ),
        Language::Go => (
            PathBuf::from("shatter-go/shatter-go"),
            vec![],
        ),
    };

    let mut config = FrontendConfig::new(command);
    config.args = args;
    config.request_timeout = timeout;
    config
}

/// Run the explore command.
async fn run_explore(
    targets: &[String],
    max_iterations: u32,
    timeout: u64,
    analyze_only: bool,
    _show_clusters: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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

        let config = frontend_config(target.language, request_timeout);
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
        };

        for func in &functions {
            println!("\n  Exploring {}...", func.name);

            match explorer::explore_function(&mut frontend, func, &explore_config).await {
                Ok(result) => {
                    print!("{}", explorer::format_exploration_report(&result));
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
            analyze_only,
            show_clusters,
        } => {
            run_explore(&targets, max_iterations, timeout, analyze_only, show_clusters).await
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
                analyze_only,
                show_clusters,
            } => {
                assert_eq!(targets, vec!["test.ts:myFunc"]);
                assert_eq!(max_iterations, 100);
                assert_eq!(timeout, 60);
                assert!(!analyze_only);
                assert!(!show_clusters);
            }
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
                analyze_only,
                show_clusters,
            } => {
                assert_eq!(targets, vec!["a.ts:fn1", "b.go:Fn2"]);
                assert_eq!(max_iterations, 50);
                assert_eq!(timeout, 120);
                assert!(analyze_only);
                assert!(!show_clusters);
            }
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
    fn frontend_config_typescript_defaults() {
        let config = frontend_config(Language::TypeScript, Duration::from_secs(30));
        assert_eq!(config.command, PathBuf::from("node"));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
    }

    #[test]
    fn frontend_config_go_defaults() {
        let config = frontend_config(Language::Go, Duration::from_secs(45));
        assert_eq!(config.command, PathBuf::from("shatter-go/shatter-go"));
        assert_eq!(config.request_timeout, Duration::from_secs(45));
    }
}
