//! Disk cache for [`FunctionAnalysis`] results, enabling fast re-analysis of unchanged files.
//!
//! Each source file's analysis results are cached keyed by
//! `(file_path, content_hash, protocol_version, analyzer_version)`.
//! On lookup, an mtime fast-path avoids re-hashing when the file hasn't been touched.
//! A protocol version is stored with each entry — protocol bumps invalidate all cached data.
//! An analyzer version (the frontend's source/bundle hash) is also stored: when a frontend's
//! analyze behavior changes for unchanged source — a new build without a protocol bump — the
//! stored analyzer version no longer matches and the entry is invalidated, so the new behavior
//! is not silently masked by stale cached results (str-2cihu). This mirrors the wrapper-side
//! generatorVersion cache key (str-6jwyw, str-o09e).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache::CacheError;
use crate::protocol::{FunctionAnalysis, PROTOCOL_VERSION};

/// A single cached analysis entry for one source file.
#[derive(Debug, Serialize, Deserialize)]
struct AnalysisCacheEntry {
    /// Hex-encoded SHA-256 of the file contents at the time of caching.
    content_hash: String,
    /// File modification time in seconds since the Unix epoch.
    mtime_secs: i64,
    /// Protocol version used when the analysis was produced.
    protocol_version: String,
    /// Frontend analyzer version (source/bundle hash) that produced the analysis.
    ///
    /// Defaults to empty for entries written before this field existed, which
    /// then mismatch any non-empty caller version and invalidate cleanly.
    #[serde(default)]
    analyzer_version: String,
    /// The cached analysis results.
    analyses: Vec<FunctionAnalysis>,
}

/// Disk-backed cache for storing and loading [`FunctionAnalysis`] results.
#[derive(Debug)]
pub struct AnalysisCache {
    cache_dir: PathBuf,
}

impl AnalysisCache {
    /// Create a new analysis cache backed by the given directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    pub fn new(cache_dir: PathBuf) -> Result<Self, CacheError> {
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Look up cached analysis results for a source file.
    ///
    /// Returns `Ok(Some(analyses))` on cache hit, `Ok(None)` on cache miss.
    /// Uses an mtime fast-path: if the file's mtime matches the cached mtime,
    /// the content hash is not recomputed.
    ///
    /// `analyzer_version` is the frontend's source/bundle hash for the file's
    /// language. An entry produced by a different analyzer version is treated as
    /// a miss so that changed analyze behavior is not masked by stale results.
    pub fn lookup(
        &self,
        file_path: &Path,
        analyzer_version: &str,
    ) -> Result<Option<Vec<FunctionAnalysis>>, CacheError> {
        let cache_file = self.cache_path_for(file_path);

        let contents = match fs::read_to_string(&cache_file) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CacheError::Io(e)),
        };

        let mut entry: AnalysisCacheEntry = serde_json::from_str(&contents)?;

        // Protocol version mismatch invalidates the cache.
        if entry.protocol_version != PROTOCOL_VERSION {
            return Ok(None);
        }

        // Analyzer version mismatch invalidates the cache: the frontend's
        // analyze behavior may have changed even though the source did not.
        if entry.analyzer_version != analyzer_version {
            return Ok(None);
        }

        let file_mtime = mtime_secs(file_path)?;

        // Fast-path: mtime unchanged means file content hasn't changed.
        if file_mtime == entry.mtime_secs {
            return Ok(Some(entry.analyses));
        }

        // Slow path: mtime differs — compute content hash to check for real changes.
        let current_hash = sha256_file(file_path)?;

        if current_hash == entry.content_hash {
            // File was touched but content is identical. Update the cached mtime
            // so future lookups hit the fast-path.
            entry.mtime_secs = file_mtime;
            let json = serde_json::to_string_pretty(&entry)?;
            let tmp = cache_file.with_extension("json.tmp");
            fs::write(&tmp, json)?;
            fs::rename(&tmp, &cache_file)?;
            return Ok(Some(entry.analyses));
        }

        // Content actually changed — cache miss.
        Ok(None)
    }

    /// Store analysis results for a source file.
    ///
    /// `analyzer_version` is the frontend's source/bundle hash for the file's
    /// language; it is recorded so a later analyzer change invalidates the entry.
    pub fn store(
        &self,
        file_path: &Path,
        analyses: &[FunctionAnalysis],
        analyzer_version: &str,
    ) -> Result<(), CacheError> {
        let content_hash = sha256_file(file_path)?;
        let mtime_secs = mtime_secs(file_path)?;

        let entry = AnalysisCacheEntry {
            content_hash,
            mtime_secs,
            protocol_version: PROTOCOL_VERSION.to_string(),
            analyzer_version: analyzer_version.to_string(),
            analyses: analyses.to_vec(),
        };

        let cache_file = self.cache_path_for(file_path);
        let json = serde_json::to_string_pretty(&entry)?;
        let tmp = cache_file.with_extension("json.tmp");
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, &cache_file)?;

        Ok(())
    }

    /// Remove all cached analysis entries.
    ///
    /// Returns `(file_count, bytes_freed)` describing what was removed.
    /// Returns `(0, 0)` if the cache directory does not exist.
    pub fn clear(&self) -> Result<(u64, u64), CacheError> {
        if !self.cache_dir.exists() {
            return Ok((0, 0));
        }
        let (file_count, bytes) = count_dir_contents(&self.cache_dir)?;
        fs::remove_dir_all(&self.cache_dir)?;
        fs::create_dir_all(&self.cache_dir)?;
        Ok((file_count, bytes))
    }

    /// Default analysis cache directory: `<project_root>/.shatter-cache/analysis/`.
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter-cache").join("analysis")
    }

    /// Compute the cache file path for a given source file path.
    ///
    /// Uses SHA-256 of the file path string as the filename to avoid
    /// issues with path separators and special characters.
    fn cache_path_for(&self, file_path: &Path) -> PathBuf {
        let path_str = file_path.to_string_lossy();
        let hash = hex_sha256(path_str.as_bytes());
        self.cache_dir.join(format!("{hash}.json"))
    }
}

/// Walk a directory tree and count all files and their total size in bytes.
///
/// Returns `(file_count, total_bytes)`. Ignores unreadable entries.
pub(crate) fn count_dir_contents(dir: &std::path::Path) -> Result<(u64, u64), CacheError> {
    let mut file_count: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                file_count += 1;
                total_bytes += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok((file_count, total_bytes))
}

/// Compute hex-encoded SHA-256 of a byte slice.
fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Compute hex-encoded SHA-256 of a file's contents.
fn sha256_file(path: &Path) -> Result<String, CacheError> {
    let contents = fs::read(path)?;
    Ok(hex_sha256(&contents))
}

/// Get a file's modification time as seconds since the Unix epoch.
fn mtime_secs(path: &Path) -> Result<i64, CacheError> {
    let metadata = fs::metadata(path)?;
    let mtime = metadata.modified()?;
    let duration = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BranchInfo, BranchType};
    use crate::types::{ParamInfo, TypeInfo};
    use std::thread;
    use std::time::Duration;

    /// A representative frontend analyzer version used across cache tests.
    const AV: &str = "analyzer-v1";

    pub(super) fn sample_analyses() -> Vec<FunctionAnalysis> {
        vec![FunctionAnalysis {
            name: "add".to_string(),
            exported: true,
            params: vec![
                ParamInfo {
                    name: "a".into(),
                    typ: TypeInfo::Int { int_width: None, int_signed: None },
                    type_name: None,
                },
                ParamInfo {
                    name: "b".into(),
                    typ: TypeInfo::Int { int_width: None, int_signed: None },
                    type_name: None,
                },
            ],
            branches: vec![BranchInfo {
                id: 0,
                line: 3,
                condition_text: "a > 0".into(),
                condition: None,
                branch_type: BranchType::If,
            }],
            dependencies: vec![],
            return_type: TypeInfo::Int { int_width: None, int_signed: None },
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }]
    }

    fn create_source_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn cache_hit_returns_cached_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(
            dir.path(),
            "test.ts",
            "function add(a, b) { return a + b; }",
        );
        let analyses = sample_analyses();

        cache.store(&src, &analyses, AV).unwrap();
        let result = cache.lookup(&src, AV).unwrap();

        assert!(result.is_some());
        let cached = result.unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "add");
        assert_eq!(cached[0].params.len(), 2);
    }

    #[test]
    fn cache_miss_on_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(
            dir.path(),
            "test.ts",
            "function add(a, b) { return a + b; }",
        );
        cache.store(&src, &sample_analyses(), AV).unwrap();

        // Modify the file content — ensure mtime changes.
        // Use filetime to guarantee mtime differs even on filesystems with coarse granularity.
        thread::sleep(Duration::from_millis(1100));
        fs::write(&src, "function subtract(a, b) { return a - b; }").unwrap();

        let result = cache.lookup(&src, AV).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn timestamp_fast_path_unchanged_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        cache.store(&src, &sample_analyses(), AV).unwrap();

        // Lookup without modifying file — should hit mtime fast-path.
        let result = cache.lookup(&src, AV).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].name, "add");
    }

    #[test]
    fn touch_without_content_change_is_cache_hit() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let content = "function identity(x) { return x; }";
        let src = create_source_file(dir.path(), "test.ts", content);
        cache.store(&src, &sample_analyses(), AV).unwrap();

        // Touch the file (rewrite same content) after a brief delay to change mtime.
        thread::sleep(Duration::from_millis(1100));
        fs::write(&src, content).unwrap();

        // mtime differs but content hash matches → cache hit.
        let result = cache.lookup(&src, AV).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].name, "add");
    }

    #[test]
    fn protocol_version_change_invalidates_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        cache.store(&src, &sample_analyses(), AV).unwrap();

        // Manually tamper with the cached entry's protocol version.
        let cache_file = cache.cache_path_for(&src);
        let json = fs::read_to_string(&cache_file).unwrap();
        let tampered = json.replace(PROTOCOL_VERSION, "0.0.0-invalid");
        fs::write(&cache_file, tampered).unwrap();

        let result = cache.lookup(&src, AV).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn analyzer_version_change_invalidates_cache() {
        // str-2cihu: a frontend whose analyze behavior changed for unchanged
        // source (new build, same protocol) must not serve stale cached results.
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        cache.store(&src, &sample_analyses(), "analyzer-old").unwrap();

        // Same source, same protocol, but a newer analyzer version → miss.
        let result = cache.lookup(&src, "analyzer-new").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn analyzer_version_unchanged_is_cache_hit() {
        // Acceptance: no invalidation when the frontend hash is unchanged.
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        cache.store(&src, &sample_analyses(), "analyzer-v1").unwrap();

        // Identical analyzer version → the entry is served from cache.
        let result = cache.lookup(&src, "analyzer-v1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].name, "add");
    }

    #[test]
    fn legacy_entry_without_analyzer_version_invalidates_against_versioned_caller() {
        // Entries written before the analyzer_version field existed deserialize
        // with an empty version (serde default) and must miss any real version.
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        cache.store(&src, &sample_analyses(), "analyzer-v1").unwrap();

        // Strip the analyzer_version field to emulate a pre-str-2cihu entry.
        let cache_file = cache.cache_path_for(&src);
        let json = fs::read_to_string(&cache_file).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("analyzer_version");
        fs::write(&cache_file, serde_json::to_string_pretty(&value).unwrap()).unwrap();

        // A versioned caller must not accept the legacy entry.
        assert!(cache.lookup(&src, "analyzer-v1").unwrap().is_none());
        // An empty-version caller still matches the legacy default.
        assert!(cache.lookup(&src, "").unwrap().is_some());
    }

    #[test]
    fn clear_removes_all_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src1 = create_source_file(dir.path(), "a.ts", "const a = 1;");
        let src2 = create_source_file(dir.path(), "b.ts", "const b = 2;");

        cache.store(&src1, &sample_analyses(), AV).unwrap();
        cache.store(&src2, &sample_analyses(), AV).unwrap();

        assert!(cache.lookup(&src1, AV).unwrap().is_some());
        assert!(cache.lookup(&src2, AV).unwrap().is_some());

        let (file_count, _bytes) = cache.clear().unwrap();
        assert_eq!(file_count, 2);

        assert!(cache.lookup(&src1, AV).unwrap().is_none());
        assert!(cache.lookup(&src2, AV).unwrap().is_none());
    }

    #[test]
    fn lookup_returns_none_for_uncached_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = AnalysisCache::new(cache_dir).unwrap();

        let src = create_source_file(dir.path(), "test.ts", "const x = 1;");
        let result = cache.lookup(&src, AV).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn default_dir_path() {
        let root = Path::new("/home/user/project");
        let dir = AnalysisCache::default_dir(root);
        assert_eq!(
            dir,
            PathBuf::from("/home/user/project/.shatter-cache/analysis")
        );
    }

    #[test]
    fn new_creates_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("analysis");
        assert!(!nested.exists());

        let _cache = AnalysisCache::new(nested.clone()).unwrap();
        assert!(nested.exists());
    }
}

#[cfg(test)]
mod proptests {
    use super::tests::sample_analyses;
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Cache-key invariant: with source and protocol held fixed, a stored
        /// entry is served iff the lookup analyzer version equals the stored one.
        /// This is the str-2cihu guarantee — a changed analyzer contract
        /// invalidates, an unchanged one does not.
        #[test]
        fn analyzer_version_gates_hit(
            stored in "[a-z0-9]{0,16}",
            queried in "[a-z0-9]{0,16}",
        ) {
            let dir = tempfile::tempdir().unwrap();
            let cache = AnalysisCache::new(dir.path().join("cache")).unwrap();
            let src = dir.path().join("src.ts");
            fs::write(&src, "const x = 1;").unwrap();

            cache.store(&src, &sample_analyses(), &stored).unwrap();
            let hit = cache.lookup(&src, &queried).unwrap();

            prop_assert_eq!(hit.is_some(), stored == queried);
        }
    }
}
