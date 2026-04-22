use std::process;

use crate::args::WorkspaceAction;

/// Parse a human-readable byte size string (e.g. "5GiB", "512MiB", "1024") into a byte count.
///
/// Accepts the following suffixes (case-insensitive): GiB, MiB, KiB, GB, MB, KB. Bare
/// integers are treated as bytes. Returns `Err` if the string is not parseable.
fn parse_human_bytes(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let suffixes: &[(&str, i64)] = &[
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
        ("GB", 1_000_000_000),
        ("MB", 1_000_000),
        ("KB", 1_000),
    ];
    let lower_s = s.to_lowercase();
    for (suffix, multiplier) in suffixes {
        let lower_suffix = suffix.to_lowercase();
        if let Some(numeric_part) = lower_s.strip_suffix(lower_suffix.as_str()) {
            let original_numeric = &s[..numeric_part.len()];
            let number: f64 = original_numeric
                .trim()
                .parse()
                .map_err(|_| format!("invalid size value in {s:?}"))?;
            return Ok((number * *multiplier as f64) as i64);
        }
    }
    // No suffix — treat as raw bytes.
    s.parse::<i64>()
        .map_err(|_| format!("invalid size value {s:?}: expected a number or a value with a unit suffix (GiB, MiB, KiB, GB, MB, KB)"))
}

/// Dispatch a `WorkspaceAction` by forwarding to the embedded Go frontend binary.
///
/// The Go binary's `workspace` subcommand handles all workspace management
/// operations. This function resolves the Go binary, rebuilds the argument
/// list, and inherits stdin/stdout/stderr so the output reaches the terminal
/// unchanged.
pub(crate) fn run_workspace(action: &WorkspaceAction) -> Result<(), Box<dyn std::error::Error>> {
    let go_binary = crate::embedded_go_frontend::ensure_extracted()
        .map_err(|e| format!("failed to locate Go frontend binary: {e}"))?;

    let mut command_args: Vec<String> = vec!["workspace".to_string()];

    match action {
        WorkspaceAction::Gc {
            dry_run,
            keep,
            max_age_days,
            max_runs_size,
            max_cache_size,
        } => {
            command_args.push("gc".to_string());

            if *dry_run {
                command_args.push("--dry-run".to_string());
            }

            command_args.push(format!("--keep={keep}"));
            command_args.push(format!("--max-age-days={max_age_days}"));

            let runs_bytes = parse_human_bytes(max_runs_size)
                .map_err(|e| format!("--max-runs-size: {e}"))?;
            command_args.push(format!("--max-runs-bytes={runs_bytes}"));

            let cache_bytes = parse_human_bytes(max_cache_size)
                .map_err(|e| format!("--max-cache-size: {e}"))?;
            command_args.push(format!("--max-cache-bytes={cache_bytes}"));
        }
    }

    let status = process::Command::new(&go_binary)
        .args(&command_args)
        .stdin(process::Stdio::inherit())
        .stdout(process::Stdio::inherit())
        .stderr(process::Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run Go frontend binary {}: {e}", go_binary.display()))?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        return Err(format!("workspace gc exited with code {code}").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_human_bytes_gib() {
        assert_eq!(parse_human_bytes("5GiB").unwrap(), 5 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_human_bytes_mib() {
        assert_eq!(parse_human_bytes("512MiB").unwrap(), 512 * 1024 * 1024);
    }

    #[test]
    fn parse_human_bytes_kib() {
        assert_eq!(parse_human_bytes("1KiB").unwrap(), 1024);
    }

    #[test]
    fn parse_human_bytes_gb() {
        assert_eq!(parse_human_bytes("1GB").unwrap(), 1_000_000_000);
    }

    #[test]
    fn parse_human_bytes_raw() {
        assert_eq!(parse_human_bytes("1048576").unwrap(), 1_048_576);
    }

    #[test]
    fn parse_human_bytes_invalid() {
        assert!(parse_human_bytes("not-a-size").is_err());
    }

    #[test]
    fn parse_human_bytes_fractional_gib() {
        let result = parse_human_bytes("1.5GiB").unwrap();
        // 1.5 * 1024^3 = 1610612736
        assert_eq!(result, 1_610_612_736);
    }

    #[test]
    fn parse_human_bytes_whitespace_trimmed() {
        assert_eq!(
            parse_human_bytes("  2GiB  ").unwrap(),
            2 * 1024 * 1024 * 1024
        );
    }
}
