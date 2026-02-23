use clap::Parser;
use std::path::PathBuf;

/// Shatter: automatic exploratory testing via concolic execution.
#[derive(Parser, Debug)]
#[command(name = "shatter", version, about)]
struct Cli {
    /// Path to the source file to analyze.
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Name of the function to test.
    #[arg(short = 'F', long)]
    function: Option<String>,

    /// Maximum number of paths to explore.
    #[arg(short, long, default_value_t = 100)]
    max_paths: u32,
}

fn main() {
    let cli = Cli::parse();

    if cli.file.is_none() {
        eprintln!("No file specified. Run with --help for usage.");
        std::process::exit(1);
    }

    println!("shatter: analyzing {:?}", cli.file.unwrap());
    if let Some(func) = cli.function {
        println!("  function: {func}");
    }
    println!("  max_paths: {}", cli.max_paths);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_help() {
        // Verify the CLI struct derives correctly by attempting a parse
        // with --help (which exits), so instead just verify default values.
        let cli = Cli::parse_from(["shatter", "--file", "test.ts"]);
        assert_eq!(cli.file, Some(PathBuf::from("test.ts")));
        assert_eq!(cli.max_paths, 100);
        assert!(cli.function.is_none());
    }

    #[test]
    fn cli_parses_all_args() {
        let cli = Cli::parse_from([
            "shatter",
            "--file",
            "src/app.ts",
            "--function",
            "processOrder",
            "--max-paths",
            "50",
        ]);
        assert_eq!(cli.file, Some(PathBuf::from("src/app.ts")));
        assert_eq!(cli.function.as_deref(), Some("processOrder"));
        assert_eq!(cli.max_paths, 50);
    }
}
