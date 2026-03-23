use std::fs;
use std::path::{Path, PathBuf};

/// The esbuild-bundled TypeScript frontend, embedded at compile time.
const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/frontend-bundle.js"));

/// The esbuild-bundled worker thread, embedded at compile time.
const WORKER_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/frontend-worker-bundle.js"));

/// SHA-256 hash of the bundle, used for cache-busting.
const BUNDLE_HASH: &str = env!("FRONTEND_BUNDLE_HASH");

/// Ensure the embedded TS frontend bundle is extracted to disk, returning its path.
///
/// The bundle is written to `~/.cache/shatter/frontend-<hash>.js`. If the file
/// already exists (matching hash), extraction is skipped. The worker bundle is
/// extracted alongside as `worker.js` so the main bundle can find it via __dirname.
pub fn ensure_extracted() -> Result<PathBuf, String> {
    let cache_dir = cache_dir()?;
    extract_to(&cache_dir)
}

/// Extract the bundle to a specific cache directory. Returns the path to the bundle file.
fn extract_to(cache_dir: &Path) -> Result<PathBuf, String> {
    let bundle_path = cache_dir.join(format!("frontend-{BUNDLE_HASH}.js"));

    if !bundle_path.exists() {
        fs::create_dir_all(cache_dir)
            .map_err(|e| format!("failed to create cache directory {}: {e}", cache_dir.display()))?;

        // Write atomically: write to a temp file then rename to avoid partial reads
        let tmp_path = cache_dir.join(format!("frontend-{BUNDLE_HASH}.js.tmp"));
        fs::write(&tmp_path, BUNDLE)
            .map_err(|e| format!("failed to write frontend bundle: {e}"))?;
        fs::rename(&tmp_path, &bundle_path).map_err(|e| {
            format!(
                "failed to rename {} -> {}: {e}",
                tmp_path.display(),
                bundle_path.display()
            )
        })?;

        // Clean up old bundles (different hash)
        cleanup_old_bundles(cache_dir, &bundle_path);
    }

    // Extract worker bundle alongside the main bundle. The main bundle's
    // InstrumentationWorker resolves worker.js relative to __dirname.
    let worker_path = cache_dir.join("worker.js");
    if !worker_path.exists() {
        let tmp_worker = cache_dir.join("worker.js.tmp");
        fs::write(&tmp_worker, WORKER_BUNDLE)
            .map_err(|e| format!("failed to write worker bundle: {e}"))?;
        fs::rename(&tmp_worker, &worker_path).map_err(|e| {
            format!(
                "failed to rename {} -> {}: {e}",
                tmp_worker.display(),
                worker_path.display()
            )
        })?;
    }

    Ok(bundle_path)
}

/// Return the shatter cache directory (`~/.cache/shatter/`).
fn cache_dir() -> Result<PathBuf, String> {
    // Respect XDG_CACHE_HOME if set, otherwise default to ~/.cache
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs_cache_fallback()?,
    };
    Ok(base.join("shatter"))
}

/// Fallback: ~/.cache
fn dirs_cache_fallback() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache"))
        .ok_or_else(|| "cannot determine home directory (HOME not set)".to_string())
}

/// Remove old frontend bundles that don't match the current hash.
fn cleanup_old_bundles(cache_dir: &Path, current: &Path) {
    let entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path == current {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with("frontend-")
            && name.ends_with(".js")
        {
            let _ = fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_is_embedded() {
        assert!(
            BUNDLE.len() > 100_000,
            "embedded bundle too small: {} bytes",
            BUNDLE.len()
        );
    }

    #[test]
    fn bundle_hash_is_64_hex_chars() {
        assert_eq!(BUNDLE_HASH.len(), 64, "expected SHA-256 hex string");
        assert!(
            BUNDLE_HASH.chars().all(|c| c.is_ascii_hexdigit()),
            "hash contains non-hex characters: {BUNDLE_HASH}"
        );
    }

    #[test]
    fn extract_to_writes_bundle_to_cache() {
        let tmp = std::env::temp_dir().join("shatter-test-extract");
        let _ = fs::remove_dir_all(&tmp);

        let path = extract_to(&tmp).expect("extraction failed");

        assert!(path.exists());
        assert_eq!(
            fs::read(&path).unwrap().len(),
            BUNDLE.len(),
            "extracted bundle size mismatch"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_to_is_idempotent() {
        let tmp = std::env::temp_dir().join("shatter-test-idempotent");
        let _ = fs::remove_dir_all(&tmp);

        let path1 = extract_to(&tmp).expect("first extraction failed");
        let path2 = extract_to(&tmp).expect("second extraction failed");

        assert_eq!(path1, path2);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_to_cleans_up_old_bundles() {
        let tmp = std::env::temp_dir().join("shatter-test-cleanup");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Plant a fake old bundle
        let old_bundle = tmp.join("frontend-0000000000000000000000000000000000000000000000000000000000000000.js");
        fs::write(&old_bundle, b"old").unwrap();

        let path = extract_to(&tmp).expect("extraction failed");

        assert!(path.exists());
        assert!(!old_bundle.exists(), "old bundle should have been cleaned up");

        let _ = fs::remove_dir_all(&tmp);
    }
}
