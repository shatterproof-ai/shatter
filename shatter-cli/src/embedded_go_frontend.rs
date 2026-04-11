use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The Go frontend binary, embedded at compile time.
const BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shatter-go"));

/// SHA-256 hash of the binary, used for cache-busting.
const BINARY_HASH: &str = env!("GO_FRONTEND_HASH");

/// Ensure the embedded Go frontend binary is extracted to disk, returning its path.
///
/// The binary is written to `~/.cache/shatter/go-frontend-<hash>`. If the file
/// already exists (matching hash), extraction is skipped.
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

/// Extract the binary to a specific cache directory. Returns the path to the binary.
fn extract_to(cache_dir: &Path) -> Result<PathBuf, String> {
    let binary_path = cache_dir.join(format!("go-frontend-{BINARY_HASH}"));

    if binary_path.exists() {
        return Ok(binary_path);
    }

    fs::create_dir_all(cache_dir).map_err(|e| {
        format!(
            "failed to create cache directory {}: {e}",
            cache_dir.display()
        )
    })?;

    // Write atomically: write to a temp file then rename to avoid partial reads
    let tmp_path = cache_dir.join(format!("go-frontend-{BINARY_HASH}.tmp"));
    fs::write(&tmp_path, BINARY).map_err(|e| format!("failed to write go frontend binary: {e}"))?;

    // Make executable
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("failed to set permissions on go frontend binary: {e}"))?;

    fs::rename(&tmp_path, &binary_path).map_err(|e| {
        format!(
            "failed to rename {} -> {}: {e}",
            tmp_path.display(),
            binary_path.display()
        )
    })?;

    // Clean up old binaries (different hash)
    cleanup_old_binaries(cache_dir, &binary_path);

    Ok(binary_path)
}

/// Return the shatter cache directory (`~/.cache/shatter/`).
fn cache_dir() -> Result<PathBuf, String> {
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

/// Remove old Go frontend binaries that don't match the current hash.
fn cleanup_old_binaries(cache_dir: &Path, current: &Path) {
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
            && name.starts_with("go-frontend-")
            && !name.ends_with(".tmp")
        {
            let _ = fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_is_embedded() {
        assert!(
            BINARY.len() > 100_000,
            "embedded go binary too small: {} bytes",
            BINARY.len()
        );
    }

    #[test]
    fn binary_hash_is_64_hex_chars() {
        assert_eq!(BINARY_HASH.len(), 64, "expected SHA-256 hex string");
        assert!(
            BINARY_HASH.chars().all(|c| c.is_ascii_hexdigit()),
            "hash contains non-hex characters: {BINARY_HASH}"
        );
    }

    #[test]
    fn extract_to_writes_binary_to_cache() {
        let tmp = std::env::temp_dir().join("shatter-test-go-extract");
        let _ = fs::remove_dir_all(&tmp);

        let path = extract_to(&tmp).expect("extraction failed");

        assert!(path.exists());
        assert_eq!(
            fs::read(&path).unwrap().len(),
            BINARY.len(),
            "extracted binary size mismatch"
        );

        // Verify it's executable
        let perms = fs::metadata(&path).unwrap().permissions();
        assert!(perms.mode() & 0o111 != 0, "binary should be executable");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_to_is_idempotent() {
        let tmp = std::env::temp_dir().join("shatter-test-go-idempotent");
        let _ = fs::remove_dir_all(&tmp);

        let path1 = extract_to(&tmp).expect("first extraction failed");
        let path2 = extract_to(&tmp).expect("second extraction failed");

        assert_eq!(path1, path2);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_to_falls_back_when_cache_unwritable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join("shatter-test-unwritable-go");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Make the cache directory read-only
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o555)).unwrap();

        let result = extract_to(&tmp);

        // Restore permissions for cleanup
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&tmp);

        assert!(
            result.is_err(),
            "expected failure on unwritable cache dir (pre-fix behavior)"
        );
    }

    #[test]
    fn ensure_extracted_falls_back_on_unwritable_cache() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = std::env::temp_dir().join("shatter-test-fallback-go");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Make it unwritable
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o555)).unwrap();

        let result = ensure_extracted_with_fallback(&tmp);

        // Restore permissions for cleanup
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&tmp);

        let path = result.expect("extraction should succeed via fallback");
        assert!(
            path.exists(),
            "extracted binary should exist at fallback location"
        );
    }

    #[test]
    fn extract_to_cleans_up_old_binaries() {
        let tmp = std::env::temp_dir().join("shatter-test-go-cleanup");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Plant a fake old binary
        let old_binary = tmp
            .join("go-frontend-0000000000000000000000000000000000000000000000000000000000000000");
        fs::write(&old_binary, b"old").unwrap();

        let path = extract_to(&tmp).expect("extraction failed");

        assert!(path.exists());
        assert!(
            !old_binary.exists(),
            "old binary should have been cleaned up"
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
