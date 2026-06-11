use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The Go frontend binary, embedded at compile time.
const BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shatter-go"));

/// SHA-256 hash of the binary, used for cache-busting.
const BINARY_HASH: &str = env!("GO_FRONTEND_HASH");

/// Permission bits applied to the extracted Go frontend binary so it is
/// executable by user/group/other.
const EXECUTABLE_PERMISSIONS: u32 = 0o755;

/// Per-process counter used to mint unique tmp file names so concurrent
/// extractors never race on a shared `.tmp` path. Combined with the process
/// id, this gives every extraction attempt within a process its own staging
/// file even when many threads are extracting at once.
static EXTRACT_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
///
/// Shares the same atomic staging/rename strategy as
/// `embedded_frontend::extract_to`. Unlike the TypeScript bundle extractor,
/// this path intentionally preserves other versioned Go binaries because live
/// scans from another CLI build may still need them for frontend respawns.
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

    // Write atomically using a per-call unique tmp path so concurrent
    // extractors do not race on the same staging file. A shared `.tmp` path
    // is what produced `Text file busy (os error 26)` in str-6p7b: one
    // process renamed the staged inode into place and exec'd it while a
    // peer still held it open for writing.
    write_executable_atomic(cache_dir, &binary_path)?;

    // Keep other versioned binaries in place. A long-running scan from another
    // CLI build may still need its embedded frontend path for respawns.

    Ok(binary_path)
}

/// Stage `BINARY` into `cache_dir` under a unique tmp name, mark it
/// executable, then atomically rename it to `destination`. If a concurrent
/// caller wins the race to `destination` first, treat that as success and
/// drop the local staging file.
fn write_executable_atomic(cache_dir: &Path, destination: &Path) -> Result<(), String> {
    let tmp_path = unique_tmp_path(cache_dir, destination);

    fs::write(&tmp_path, BINARY).map_err(|e| format!("failed to write go frontend binary: {e}"))?;

    fs::set_permissions(
        &tmp_path,
        fs::Permissions::from_mode(EXECUTABLE_PERMISSIONS),
    )
    .map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("failed to set permissions on go frontend binary: {e}")
    })?;

    match fs::rename(&tmp_path, destination) {
        Ok(()) => Ok(()),
        Err(_) if destination.exists() => {
            let _ = fs::remove_file(&tmp_path);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            Err(format!(
                "failed to rename {} -> {}: {e}",
                tmp_path.display(),
                destination.display()
            ))
        }
    }
}

/// Build a per-process, per-call tmp path for staging an extraction. The PID
/// disambiguates concurrent OS processes; the atomic counter disambiguates
/// concurrent threads within a process.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn isolated_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shatter-test-go-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    /// Number of concurrent extractors used in the race regression test.
    /// Eight matches the parallel test in `embedded_frontend.rs` and is
    /// enough threads to reliably interleave the write/rename steps.
    const CONCURRENT_EXTRACT_THREADS: usize = 8;

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
        let tmp = isolated_temp_dir("extract");
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
        let tmp = isolated_temp_dir("idempotent");
        let _ = fs::remove_dir_all(&tmp);

        let path1 = extract_to(&tmp).expect("first extraction failed");
        let path2 = extract_to(&tmp).expect("second extraction failed");

        assert_eq!(path1, path2);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_to_falls_back_when_cache_unwritable() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = isolated_temp_dir("unwritable-go");
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

        let tmp = isolated_temp_dir("fallback-go");
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
    fn extract_to_preserves_other_versioned_binaries() {
        let tmp = isolated_temp_dir("cleanup");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Plant a fake binary from another CLI build. Live scans from that
        // build may still need to respawn it while this build extracts its
        // own hash.
        let other_binary = tmp
            .join("go-frontend-0000000000000000000000000000000000000000000000000000000000000000");
        fs::write(&other_binary, b"other").unwrap();

        let path = extract_to(&tmp).expect("extraction failed");

        assert!(path.exists());
        assert!(
            other_binary.exists(),
            "other versioned binary should remain available for live respawns"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// Regression for str-6p7b: concurrent `extract_to` calls must not race on
    /// a shared `.tmp` path. The pre-fix code wrote every caller's payload to
    /// the same `go-frontend-<hash>.tmp` file then renamed it onto the final
    /// binary path; on a real system that produced `Text file busy (os error
    /// 26)` when one process tried to exec the binary while another still had
    /// the underlying inode open for write. In-process the same race shows up
    /// as `set_permissions` or `rename` returning ENOENT after a peer renamed
    /// the shared tmp out from under us.
    #[test]
    fn extract_to_is_safe_under_concurrent_calls() {
        let tmp = isolated_temp_dir("concurrent-extract");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let dir = Arc::new(tmp);
        let barrier = Arc::new(Barrier::new(CONCURRENT_EXTRACT_THREADS));
        let mut handles = Vec::new();

        for _ in 0..CONCURRENT_EXTRACT_THREADS {
            let dir = Arc::clone(&dir);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                extract_to(dir.as_path())
            }));
        }

        for handle in handles {
            let path = handle
                .join()
                .expect("thread panicked")
                .expect("concurrent extraction must not error");
            assert!(path.exists(), "binary path should exist after extraction");
            let perms = fs::metadata(&path).unwrap().permissions();
            assert!(
                perms.mode() & 0o111 != 0,
                "binary should remain executable across concurrent extracts"
            );
        }

        let _ = fs::remove_dir_all(dir.as_path());
    }
}
