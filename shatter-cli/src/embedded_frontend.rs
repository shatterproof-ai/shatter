use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The esbuild-bundled TypeScript frontend, embedded at compile time.
const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/frontend-bundle.js"));

/// The esbuild-bundled worker thread, embedded at compile time.
const WORKER_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/frontend-worker-bundle.js"));

/// SHA-256 hash of the bundle, used for cache-busting.
const BUNDLE_HASH: &str = env!("FRONTEND_BUNDLE_HASH");

static EXTRACT_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Ensure the embedded TS frontend bundle is extracted to disk, returning its path.
///
/// The bundle is written to `~/.cache/shatter/frontend-<hash>.js`. If the file
/// already exists (matching hash), extraction is skipped. The worker bundle is
/// extracted alongside as `worker.js` so the main bundle can find it via __dirname.
///
/// If the primary cache directory is unwritable, falls back to a temp directory.
pub fn ensure_extracted() -> Result<PathBuf, String> {
    let cache_dir = cache_dir()?;
    ensure_extracted_with_fallback(&cache_dir)
}

/// Try extracting to `primary_cache`; on failure, fall back to a temp directory.
fn ensure_extracted_with_fallback(primary_cache: &Path) -> Result<PathBuf, String> {
    match extract_to(primary_cache) {
        Ok(path) => Ok(path),
        Err(_) => {
            let fallback = std::env::temp_dir().join("shatter-cache");
            extract_to(&fallback)
        }
    }
}

/// Extract the bundle to a specific cache directory. Returns the path to the bundle file.
fn extract_to(cache_dir: &Path) -> Result<PathBuf, String> {
    let bundle_path = cache_dir.join(format!("frontend-{BUNDLE_HASH}.js"));

    if !bundle_path.exists() {
        fs::create_dir_all(cache_dir)
            .map_err(|e| format!("failed to create cache directory {}: {e}", cache_dir.display()))?;

        // Write atomically: use a unique temp file so concurrent callers do not
        // race on the same `.tmp` path. If another caller wins the race to the
        // final destination first, treat that as success.
        write_atomic_file(cache_dir, &bundle_path, BUNDLE, "frontend bundle")?;

        // Clean up old bundles (different hash)
        cleanup_old_bundles(cache_dir, &bundle_path);
    }

    // Extract worker bundle alongside the main bundle. The main bundle's
    // InstrumentationWorker resolves worker.js relative to __dirname.
    let worker_path = cache_dir.join("worker.js");
    if !worker_path.exists() {
        write_atomic_file(cache_dir, &worker_path, WORKER_BUNDLE, "worker bundle")?;
    }

    Ok(bundle_path)
}

fn write_atomic_file(
    cache_dir: &Path,
    destination: &Path,
    contents: &[u8],
    label: &str,
) -> Result<(), String> {
    let tmp_path = unique_tmp_path(cache_dir, destination);
    fs::write(&tmp_path, contents).map_err(|e| format!("failed to write {label}: {e}"))?;

    match fs::rename(&tmp_path, destination) {
        Ok(()) => Ok(()),
        Err(e) if destination.exists() => {
            let _ = fs::remove_file(&tmp_path);
            Ok(())
        }
        Err(e) => Err(format!(
            "failed to rename {} -> {}: {e}",
            tmp_path.display(),
            destination.display()
        )),
    }
}

fn unique_tmp_path(cache_dir: &Path, destination: &Path) -> PathBuf {
    let suffix = EXTRACT_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .expect("extraction destination must have a UTF-8 file name");
    cache_dir.join(format!("{file_name}.{}.{}.tmp", std::process::id(), suffix))
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
    use std::sync::{Arc, Barrier};
    use std::thread;

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

    #[test]
    fn extract_to_falls_back_when_cache_unwritable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join("shatter-test-unwritable-ts");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Make the cache directory read-only so writes fail
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o555)).unwrap();

        // extract_to should fail on an unwritable directory
        let result = extract_to(&tmp);

        // Restore permissions for cleanup
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&tmp);

        // Current behavior: this errors. After the fix, ensure_extracted()
        // should fall back to a temp dir and succeed.
        assert!(result.is_err(), "expected failure on unwritable cache dir (pre-fix behavior)");
    }

    #[test]
    fn ensure_extracted_falls_back_on_unwritable_cache() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join("shatter-test-fallback-ts");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Make it unwritable
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o555)).unwrap();

        // Use a custom env var to point cache_dir at our unwritable dir
        // This tests the public ensure_extracted_with_fallback path
        let result = ensure_extracted_with_fallback(&tmp);

        // Restore permissions for cleanup
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&tmp);

        // After the fix, this should succeed via temp dir fallback
        let path = result.expect("extraction should succeed via fallback");
        assert!(path.exists(), "extracted bundle should exist at fallback location");
    }

    #[test]
    fn extract_to_is_safe_under_concurrent_calls() {
        let tmp = std::env::temp_dir().join("shatter-test-concurrent-extract");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let dir = Arc::new(tmp);
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();

        for _ in 0..8 {
            let dir = Arc::clone(&dir);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                extract_to(dir.as_path())
            }));
        }

        for handle in handles {
            let path = handle.join().expect("thread panicked").expect("extraction failed");
            assert!(path.exists(), "bundle path should exist after extraction");
        }

        let _ = fs::remove_dir_all(dir.as_path());
    }
}
