use std::path::Path;

use shatter_core::analysis_cache::AnalysisCache;
use shatter_core::cache::BehaviorMapCache;

use crate::args::CacheAction;

/// Dispatch a `CacheAction` to the appropriate handler.
pub(crate) fn run_cache_clear_from_action(
    action: &CacheAction,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CacheAction::Clear { analysis, results } => {
            run_cache_clear(*analysis, *results, project_root)
        }
    }
}

/// Run `shatter cache clear [--analysis] [--results]`.
///
/// When neither flag is given, both caches are cleared.
pub(crate) fn run_cache_clear(
    analysis: bool,
    results: bool,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let clear_all = !analysis && !results;

    if analysis || clear_all {
        let dir = AnalysisCache::default_dir(project_root);
        let cache = AnalysisCache::new(dir)?;
        let (files, bytes) = cache.clear()?;
        println!(
            "Analysis cache cleared: {} {}, {} freed",
            files,
            if files == 1 { "file" } else { "files" },
            fmt_bytes(bytes),
        );
    }

    if results || clear_all {
        let dir = BehaviorMapCache::default_dir(project_root);
        let cache = BehaviorMapCache::new(dir)?;
        let (files, bytes) = cache.clear()?;
        println!(
            "Results cache cleared: {} {}, {} freed",
            files,
            if files == 1 { "file" } else { "files" },
            fmt_bytes(bytes),
        );
    }

    Ok(())
}

/// Format a byte count as a human-readable string (e.g. `42.1 KB`, `1.0 MB`).
fn fmt_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_bytes_zero() {
        assert_eq!(fmt_bytes(0), "0 B");
    }

    #[test]
    fn fmt_bytes_bytes() {
        assert_eq!(fmt_bytes(512), "512 B");
    }

    #[test]
    fn fmt_bytes_kilobytes() {
        assert_eq!(fmt_bytes(1_024), "1.0 KB");
        assert_eq!(fmt_bytes(43_110), "42.1 KB");
    }

    #[test]
    fn fmt_bytes_megabytes() {
        assert_eq!(fmt_bytes(1_048_576), "1.0 MB");
    }

    #[test]
    fn fmt_bytes_gigabytes() {
        assert_eq!(fmt_bytes(1_073_741_824), "1.0 GB");
    }
}
