//! Disk cache for [`BehaviorMap`]s and [`FunctionSpec`]s, enabling persistence across runs.
//!
//! When shatter explores a function, the resulting behavior map is stored to disk
//! so it can be reloaded in future runs — critical for compositional testing where
//! function B's behavior map is reused when testing function A.
//!
//! Cache entries include the protocol version so that protocol upgrades
//! automatically invalidate stale entries.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::behavior::BehaviorMap;
use crate::protocol::PROTOCOL_VERSION;
use crate::spec::FunctionSpec;

/// Errors that can occur during cache operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cache serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Versioned envelope for cached BehaviorMap entries.
/// Protocol version changes invalidate all cached entries.
#[derive(Debug, Serialize, Deserialize)]
struct BehaviorMapCacheEntry {
    protocol_version: String,
    behavior_map: BehaviorMap,
}

/// Versioned envelope for cached FunctionSpec entries.
#[derive(Debug, Serialize, Deserialize)]
struct SpecCacheEntry {
    protocol_version: String,
    spec: FunctionSpec,
}

/// Compute the base path (without extension) for a function ID within a cache directory.
///
/// Mirrors the source tree structure:
/// - `src/auth.ts:validateToken` → `src/auth.ts/validateToken`
/// - `src/auth.ts:TokenValidator.validate` → `src/auth.ts/TokenValidator/validate`
/// - `simpleFunc` → `simpleFunc`
fn cache_base_path(cache_dir: &Path, function_id: &str) -> PathBuf {
    let mut path = cache_dir.to_path_buf();

    let (file_part, func_part) = match function_id.split_once(':') {
        Some((f, func)) => (Some(f), func),
        None => (None, function_id),
    };

    if let Some(file) = file_part {
        for component in file.split('/') {
            if !component.is_empty() {
                path.push(sanitize_component(component));
            }
        }
    }

    match func_part.split_once('.') {
        Some((class_name, method_name)) => {
            path.push(sanitize_component(class_name));
            path.push(sanitize_component(method_name));
        }
        None => {
            path.push(sanitize_component(func_part));
        }
    }

    path
}

/// Disk-backed cache for storing and loading [`BehaviorMap`]s.
#[derive(Debug)]
pub struct BehaviorMapCache {
    cache_dir: PathBuf,
}

impl BehaviorMapCache {
    /// Create a new cache backed by the given directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    pub fn new(cache_dir: PathBuf) -> Result<Self, CacheError> {
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Load a behavior map for the given function ID, if one exists.
    ///
    /// Returns `Ok(None)` on cache miss, protocol version mismatch,
    /// or deserialization failure (gracefully handles old cache format).
    pub fn load(&self, function_id: &str) -> Result<Option<BehaviorMap>, CacheError> {
        let path = self.path_for(function_id);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let entry: BehaviorMapCacheEntry = match serde_json::from_str(&contents) {
                    Ok(e) => e,
                    // Old bare-JSON format or corrupt entry → cache miss
                    Err(_) => return Ok(None),
                };
                if entry.protocol_version != PROTOCOL_VERSION {
                    return Ok(None);
                }
                Ok(Some(entry.behavior_map))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Store a behavior map to disk using atomic write (temp file + rename).
    pub fn store(&self, map: &BehaviorMap) -> Result<(), CacheError> {
        let path = self.path_for(&map.function_id);
        let entry = BehaviorMapCacheEntry {
            protocol_version: PROTOCOL_VERSION.to_string(),
            behavior_map: map.clone(),
        };
        let json = serde_json::to_string_pretty(&entry)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Check whether a cached behavior map exists and its fingerprint matches.
    ///
    /// Returns `true` if the cache contains a version-compatible map for
    /// `function_id` whose fingerprint equals `current_fingerprint`.
    pub fn is_fresh(
        &self,
        function_id: &str,
        current_fingerprint: &str,
    ) -> Result<bool, CacheError> {
        match self.load(function_id)? {
            Some(map) => Ok(map
                .fingerprint
                .as_deref()
                .is_some_and(|fp| fp == current_fingerprint)),
            None => Ok(false),
        }
    }

    /// Load all cached behavior maps whose function_id starts with `file_prefix:`.
    ///
    /// Scans the cache subdirectory corresponding to `file_prefix` and loads every
    /// `.json` entry that deserializes successfully and matches the current protocol
    /// version. Returns an empty vec when the directory doesn't exist.
    pub fn load_all_for_file(&self, file_prefix: &str) -> Result<Vec<BehaviorMap>, CacheError> {
        // Build the subdirectory path by reusing the same logic as path_for,
        // but only the file portion (no function suffix).
        let mut dir = self.cache_dir.clone();
        for component in file_prefix.split('/') {
            if !component.is_empty() {
                dir.push(sanitize_component(component));
            }
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(CacheError::Io(e)),
        };

        let mut maps = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            // Only load .json files (skip .spec.json, directories, etc.)
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".spec"))
            {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&path)
                && let Ok(entry) = serde_json::from_str::<BehaviorMapCacheEntry>(&contents)
                && entry.protocol_version == PROTOCOL_VERSION
            {
                maps.push(entry.behavior_map);
            }
        }
        Ok(maps)
    }

    /// Remove all cached behavior map and spec entries.
    ///
    /// Returns `(file_count, bytes_freed)` describing what was removed.
    /// Returns `(0, 0)` if the cache directory does not exist.
    pub fn clear(&self) -> Result<(u64, u64), CacheError> {
        if !self.cache_dir.exists() {
            return Ok((0, 0));
        }
        let (file_count, bytes) =
            crate::analysis_cache::count_dir_contents(&self.cache_dir)?;
        fs::remove_dir_all(&self.cache_dir)?;
        fs::create_dir_all(&self.cache_dir)?;
        Ok((file_count, bytes))
    }

    /// Load every behavior map stored in this cache directory.
    ///
    /// Walks the entire cache directory tree, loading all `.json` entries
    /// (excluding `.spec.json` files) that match the current protocol version.
    /// Silently skips corrupt or version-mismatched entries.
    ///
    /// Intended for `nondeterminism review`, which needs all cached maps to
    /// surface candidates without knowing specific function IDs in advance.
    pub fn load_all(&self) -> Result<Vec<BehaviorMap>, CacheError> {
        if !self.cache_dir.exists() {
            return Ok(vec![]);
        }
        let mut maps = Vec::new();
        collect_all_maps(&self.cache_dir, &mut maps)?;
        Ok(maps)
    }

    /// Default cache directory relative to a project root: `<project_root>/.shatter-cache/behavior-maps/`.
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter-cache").join("behavior-maps")
    }

    fn path_for(&self, function_id: &str) -> PathBuf {
        let mut p = cache_base_path(&self.cache_dir, function_id);
        p.set_extension("json");
        p
    }
}

/// Recursively walk `dir`, loading all `.json` behavior map entries (excluding
/// `.spec.json`) that match the current protocol version into `out`.
fn collect_all_maps(dir: &Path, out: &mut Vec<BehaviorMap>) -> Result<(), CacheError> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(CacheError::Io(e)),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_all_maps(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("json")
            && !path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".spec"))
            && let Ok(contents) = fs::read_to_string(&path)
            && let Ok(entry) = serde_json::from_str::<BehaviorMapCacheEntry>(&contents)
            && entry.protocol_version == PROTOCOL_VERSION
        {
            out.push(entry.behavior_map);
        }
    }
    Ok(())
}

/// Disk-backed cache for storing and loading [`FunctionSpec`]s.
///
/// Colocated with behavior map cache entries using `.spec.json` extension.
#[derive(Debug)]
pub struct SpecCache {
    cache_dir: PathBuf,
}

impl SpecCache {
    /// Create a new spec cache backed by the given directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    pub fn new(cache_dir: PathBuf) -> Result<Self, CacheError> {
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Load a function spec for the given function ID, if one exists.
    ///
    /// Returns `Ok(None)` on cache miss, protocol version mismatch,
    /// or deserialization failure.
    pub fn load(&self, function_id: &str) -> Result<Option<FunctionSpec>, CacheError> {
        let path = self.path_for(function_id);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let entry: SpecCacheEntry = match serde_json::from_str(&contents) {
                    Ok(e) => e,
                    Err(_) => return Ok(None),
                };
                if entry.protocol_version != PROTOCOL_VERSION {
                    return Ok(None);
                }
                Ok(Some(entry.spec))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Store a function spec to disk using atomic write (temp file + rename).
    pub fn store(&self, function_id: &str, spec: &FunctionSpec) -> Result<(), CacheError> {
        let path = self.path_for(function_id);
        let entry = SpecCacheEntry {
            protocol_version: PROTOCOL_VERSION.to_string(),
            spec: spec.clone(),
        };
        let json = serde_json::to_string_pretty(&entry)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("spec.json.tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Check whether a cached spec exists and its fingerprint matches.
    pub fn is_fresh(
        &self,
        function_id: &str,
        current_fingerprint: &str,
    ) -> Result<bool, CacheError> {
        match self.load(function_id)? {
            Some(spec) => Ok(spec
                .fingerprint
                .as_deref()
                .is_some_and(|fp| fp == current_fingerprint)),
            None => Ok(false),
        }
    }

    /// Default spec cache directory (same as behavior map cache).
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter-cache").join("behavior-maps")
    }

    fn path_for(&self, function_id: &str) -> PathBuf {
        let mut p = cache_base_path(&self.cache_dir, function_id);
        p.set_extension("spec.json");
        p
    }
}

/// Sanitize a single path component for safe use as a filename.
///
/// Replaces unsafe characters with underscores to prevent path traversal attacks.
/// Called per-component after splitting on `/` and `:`, so those delimiters
/// are not expected in the input.
fn sanitize_component(component: &str) -> String {
    component
        .chars()
        .enumerate()
        .map(|(i, c)| match c {
            '\\' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            '.' if i == 0 => '_', // prevent hidden files for leading dots
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::Behavior;
    use serde_json::json;

    fn sample_map(function_id: &str) -> BehaviorMap {
        BehaviorMap {
            function_id: function_id.to_string(),
            behaviors: vec![Behavior {
                id: 0,
                input_args: vec![json!(42)],
                return_value: Some(json!("positive")),
                thrown_error: None,
                branch_path: vec![],
                side_effects: vec![],
                dependency_trace: None,
                mock_values: vec![],
            }],
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    fn sample_spec(function_name: &str) -> FunctionSpec {
        FunctionSpec {
            function_name: function_name.to_string(),
            location: Some("test.ts:1".to_string()),
            classes: vec![],
            iterations: 10,
            lines_covered: 5,
            total_lines: 10,
            invariants: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    // --- BehaviorMapCache tests ---

    #[test]
    fn store_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let original = sample_map("classifyNumber");
        cache.store(&original).unwrap();

        let loaded = cache.load("classifyNumber").unwrap();
        assert_eq!(loaded, Some(original));
    }

    #[test]
    fn load_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let result = cache.load("nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn store_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let v1 = sample_map("myFunc");
        cache.store(&v1).unwrap();

        let mut v2 = sample_map("myFunc");
        v2.behaviors.push(Behavior {
            id: 1,
            input_args: vec![json!(-1)],
            return_value: Some(json!("negative")),
            thrown_error: None,
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        });
        cache.store(&v2).unwrap();

        let loaded = cache.load("myFunc").unwrap().unwrap();
        assert_eq!(loaded.behaviors.len(), 2);
        assert_eq!(loaded, v2);
    }

    #[test]
    fn hierarchical_path_free_function() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("src/utils:helper");
        cache.store(&map).unwrap();

        let loaded = cache.load("src/utils:helper").unwrap();
        assert_eq!(loaded, Some(map));

        // src/utils:helper → src/utils/helper.json
        let expected_file = dir.path().join("src").join("utils").join("helper.json");
        assert!(expected_file.exists());
    }

    #[test]
    fn hierarchical_path_class_method() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("src/auth.ts:TokenValidator.validate");
        cache.store(&map).unwrap();

        let loaded = cache.load("src/auth.ts:TokenValidator.validate").unwrap();
        assert_eq!(loaded, Some(map));

        // src/auth.ts:TokenValidator.validate → src/auth.ts/TokenValidator/validate.json
        let expected_file = dir
            .path()
            .join("src")
            .join("auth.ts")
            .join("TokenValidator")
            .join("validate.json");
        assert!(expected_file.exists());
    }

    #[test]
    fn hierarchical_path_no_colon() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("simpleFunc");
        cache.store(&map).unwrap();

        let loaded = cache.load("simpleFunc").unwrap();
        assert_eq!(loaded, Some(map));

        let expected_file = dir.path().join("simpleFunc.json");
        assert!(expected_file.exists());
    }

    #[test]
    fn no_collision_same_func_different_files() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map_a = sample_map("src/a.ts:parse");
        let map_b = sample_map("src/b.ts:parse");
        cache.store(&map_a).unwrap();
        cache.store(&map_b).unwrap();

        let loaded_a = cache.load("src/a.ts:parse").unwrap();
        let loaded_b = cache.load("src/b.ts:parse").unwrap();
        assert_eq!(loaded_a, Some(map_a));
        assert_eq!(loaded_b, Some(map_b));
    }

    #[test]
    fn default_dir_is_relative_to_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = BehaviorMapCache::default_dir(dir.path());
        assert_eq!(cache_dir, dir.path().join(".shatter-cache").join("behavior-maps"));
    }

    #[test]
    fn new_creates_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("cache");
        assert!(!nested.exists());

        let _cache = BehaviorMapCache::new(nested.clone()).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn is_fresh_returns_true_when_fingerprint_matches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let mut map = sample_map("myFunc");
        map.fingerprint = Some("abc123".to_string());
        cache.store(&map).unwrap();

        assert!(cache.is_fresh("myFunc", "abc123").unwrap());
    }

    #[test]
    fn is_fresh_returns_false_when_fingerprint_differs() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let mut map = sample_map("myFunc");
        map.fingerprint = Some("abc123".to_string());
        cache.store(&map).unwrap();

        assert!(!cache.is_fresh("myFunc", "different").unwrap());
    }

    #[test]
    fn is_fresh_returns_false_when_no_cached_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("myFunc");
        cache.store(&map).unwrap();

        assert!(!cache.is_fresh("myFunc", "abc123").unwrap());
    }

    #[test]
    fn is_fresh_returns_false_when_no_cached_map() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        assert!(!cache.is_fresh("nonexistent", "abc123").unwrap());
    }

    #[test]
    fn store_and_load_preserves_nondeterministic_fields() {
        use crate::nondeterminism::{Confidence, NondeterministicField, NondeterminismEvidence};

        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let mut map = sample_map("nondetFunc");
        map.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.id".into(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        cache.store(&map).unwrap();
        let loaded = cache.load("nondetFunc").unwrap().unwrap();

        assert_eq!(loaded.nondeterministic_fields.len(), 1);
        assert_eq!(loaded.nondeterministic_fields[0].field_path, "return.id");
        assert_eq!(loaded.nondeterministic_fields[0].confidence, Confidence::High);
    }

    #[test]
    fn old_bare_json_format_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().to_path_buf();
        let cache = BehaviorMapCache::new(cache_dir.clone()).unwrap();

        // Write bare JSON without versioned envelope (simulates pre-upgrade cache).
        let json = r#"{"function_id":"oldFunc","behaviors":[]}"#;
        std::fs::write(cache_dir.join("oldFunc.json"), json).unwrap();

        // Old format gracefully becomes a cache miss.
        let loaded = cache.load("oldFunc").unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn protocol_version_mismatch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("myFunc");
        cache.store(&map).unwrap();

        // Manually overwrite with a different protocol version.
        let path = dir.path().join("myFunc.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        let tampered = contents.replace(PROTOCOL_VERSION, "0.0.0-fake");
        std::fs::write(&path, tampered).unwrap();

        let loaded = cache.load("myFunc").unwrap();
        assert_eq!(loaded, None);
    }

    // --- SpecCache tests ---

    #[test]
    fn spec_store_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        let spec = sample_spec("myFunc");
        cache.store("myFunc", &spec).unwrap();

        let loaded = cache.load("myFunc").unwrap();
        assert_eq!(loaded, Some(spec));
    }

    #[test]
    fn spec_load_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(cache.load("nonexistent").unwrap(), None);
    }

    #[test]
    fn spec_protocol_version_mismatch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        let spec = sample_spec("myFunc");
        cache.store("myFunc", &spec).unwrap();

        let path = dir.path().join("myFunc.spec.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        let tampered = contents.replace(PROTOCOL_VERSION, "0.0.0-fake");
        std::fs::write(&path, tampered).unwrap();

        assert_eq!(cache.load("myFunc").unwrap(), None);
    }

    #[test]
    fn spec_is_fresh_with_matching_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        let mut spec = sample_spec("myFunc");
        spec.fingerprint = Some("fp123".to_string());
        cache.store("myFunc", &spec).unwrap();

        assert!(cache.is_fresh("myFunc", "fp123").unwrap());
        assert!(!cache.is_fresh("myFunc", "different").unwrap());
    }

    #[test]
    fn spec_hierarchical_path() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        let spec = sample_spec("validate");
        cache.store("src/auth.ts:TokenValidator.validate", &spec).unwrap();

        let loaded = cache.load("src/auth.ts:TokenValidator.validate").unwrap();
        assert_eq!(loaded, Some(spec));

        let expected = dir
            .path()
            .join("src")
            .join("auth.ts")
            .join("TokenValidator")
            .join("validate.spec.json");
        assert!(expected.exists());
    }

    #[test]
    fn spec_and_behavior_map_coexist() {
        let dir = tempfile::tempdir().unwrap();
        let bm_cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();
        let spec_cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("src/math.ts:add");
        let spec = sample_spec("add");

        bm_cache.store(&map).unwrap();
        spec_cache.store("src/math.ts:add", &spec).unwrap();

        // Both can be loaded independently.
        assert_eq!(bm_cache.load("src/math.ts:add").unwrap(), Some(map));
        assert_eq!(spec_cache.load("src/math.ts:add").unwrap(), Some(spec));

        // Separate files exist.
        let bm_file = dir.path().join("src").join("math.ts").join("add.json");
        let spec_file = dir.path().join("src").join("math.ts").join("add.spec.json");
        assert!(bm_file.exists());
        assert!(spec_file.exists());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::test_arbitraries::{arb_behavior_map, arb_function_spec};
    use proptest::prelude::*;

    proptest! {
        /// BehaviorMap survives store → load roundtrip.
        #[test]
        fn behavior_map_cache_roundtrip(map in arb_behavior_map()) {
            let dir = tempfile::tempdir().unwrap();
            let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&map).unwrap();
            let loaded = cache.load(&map.function_id).unwrap();

            prop_assert_eq!(loaded, Some(map));
        }

        /// FunctionSpec survives store → load roundtrip.
        #[test]
        fn spec_cache_roundtrip(spec in arb_function_spec()) {
            let dir = tempfile::tempdir().unwrap();
            let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

            // Use a safe function_id (no path separators that might conflict).
            let function_id = "test_file.ts:testFunc";
            cache.store(function_id, &spec).unwrap();
            let loaded = cache.load(function_id).unwrap();

            prop_assert_eq!(loaded, Some(spec));
        }

        /// Freshness check is consistent: store with fingerprint, then is_fresh matches.
        #[test]
        fn behavior_map_freshness_consistent(
            mut map in arb_behavior_map(),
            fp in "[a-f0-9]{64}",
        ) {
            let dir = tempfile::tempdir().unwrap();
            let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

            map.fingerprint = Some(fp.clone());
            cache.store(&map).unwrap();

            prop_assert!(cache.is_fresh(&map.function_id, &fp).unwrap());
        }

        /// Spec freshness check is consistent.
        #[test]
        fn spec_freshness_consistent(
            mut spec in arb_function_spec(),
            fp in "[a-f0-9]{64}",
        ) {
            let dir = tempfile::tempdir().unwrap();
            let cache = SpecCache::new(dir.path().to_path_buf()).unwrap();

            spec.fingerprint = Some(fp.clone());
            let fid = "test.ts:func";
            cache.store(fid, &spec).unwrap();

            prop_assert!(cache.is_fresh(fid, &fp).unwrap());
        }
    }
}
