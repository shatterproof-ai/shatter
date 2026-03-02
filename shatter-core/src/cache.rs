//! Disk cache for [`BehaviorMap`]s, enabling persistence across runs.
//!
//! When shatter explores a function, the resulting behavior map is stored to disk
//! so it can be reloaded in future runs — critical for compositional testing where
//! function B's behavior map is reused when testing function A.

use std::fs;
use std::path::{Path, PathBuf};

use crate::behavior::BehaviorMap;

/// Errors that can occur during cache operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cache serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
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
    /// Returns `Ok(None)` if no cached map exists for this function.
    pub fn load(&self, function_id: &str) -> Result<Option<BehaviorMap>, CacheError> {
        let path = self.path_for(function_id);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let map: BehaviorMap = serde_json::from_str(&contents)?;
                Ok(Some(map))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Store a behavior map to disk using atomic write (temp file + rename).
    pub fn store(&self, map: &BehaviorMap) -> Result<(), CacheError> {
        let path = self.path_for(&map.function_id);
        let json = serde_json::to_string_pretty(map)?;

        // Ensure parent directories exist for hierarchical cache paths.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Atomic write: write to a temp file in the same directory, then rename.
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Check whether a cached behavior map exists and its fingerprint matches.
    ///
    /// Returns `true` if the cache contains a map for `function_id` whose
    /// fingerprint equals `current_fingerprint`. Returns `false` if no cached
    /// map exists, the cached map has no fingerprint, or the fingerprints differ.
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

    /// Default cache directory relative to a project root: `<project_root>/.shatter/cache/`.
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter").join("cache")
    }

    /// Compute the file path for a given function ID.
    ///
    /// Mirrors the source tree structure:
    /// - `src/auth.ts:validateToken` → `src/auth.ts/validateToken.json`
    /// - `src/auth.ts:TokenValidator.validate` → `src/auth.ts/TokenValidator/validate.json`
    /// - `simpleFunc` (no colon) → `simpleFunc.json`
    fn path_for(&self, function_id: &str) -> PathBuf {
        let mut path = self.cache_dir.clone();

        let (file_part, func_part) = match function_id.split_once(':') {
            Some((f, func)) => (Some(f), func),
            None => (None, function_id),
        };

        // Append file path components (each sanitized individually).
        if let Some(file) = file_part {
            for component in file.split('/') {
                if !component.is_empty() {
                    path.push(sanitize_component(component));
                }
            }
        }

        // Split func_part on '.' for class.method.
        match func_part.split_once('.') {
            Some((class_name, method_name)) => {
                path.push(sanitize_component(class_name));
                path.push(format!("{}.json", sanitize_component(method_name)));
            }
            None => {
                path.push(format!("{}.json", sanitize_component(func_part)));
            }
        }

        path
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
            }],
            fingerprint: None,
        }
    }

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

        // No colon → simple file directly in cache_dir
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
        let root = Path::new("/home/user/myproject");
        let dir = BehaviorMapCache::default_dir(root);
        assert_eq!(dir, PathBuf::from("/home/user/myproject/.shatter/cache"));
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

        // Map without fingerprint
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
}
