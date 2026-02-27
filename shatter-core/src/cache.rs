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

        // Atomic write: write to a temp file in the same directory, then rename.
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Default cache directory relative to a project root: `<project_root>/.shatter/cache/`.
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter").join("cache")
    }

    /// Compute the file path for a given function ID.
    fn path_for(&self, function_id: &str) -> PathBuf {
        let sanitized = sanitize_filename(function_id);
        self.cache_dir.join(format!("{sanitized}.json"))
    }
}

/// Sanitize a function ID for use as a filename.
///
/// Replaces path separators and other unsafe characters with underscores
/// to prevent path traversal attacks.
fn sanitize_filename(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            '.' if id.starts_with('.') => '_', // prevent hidden files for leading dots
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
    fn sanitizes_function_id() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        // Function ID with path separators should be sanitized
        let map = sample_map("src/utils:helper");
        cache.store(&map).unwrap();

        let loaded = cache.load("src/utils:helper").unwrap();
        assert_eq!(loaded, Some(map));

        // Verify the actual file uses sanitized name
        let expected_file = dir.path().join("src_utils_helper.json");
        assert!(expected_file.exists());
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
}
