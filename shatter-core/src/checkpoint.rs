//! Scan checkpoint for resume support.
//!
//! Persists the set of completed functions and their deep fingerprints after
//! each layer so an interrupted scan can resume without re-exploring finished
//! functions. The behavior map data itself lives in [`BehaviorMapCache`]; the
//! checkpoint is a lightweight index on top.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::SystemTime;

use crate::cache::BehaviorMapCache;

/// Errors that can occur during checkpoint operations.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("checkpoint parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Persistent state for resuming an interrupted scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCheckpoint {
    /// Format version (currently "1").
    pub version: String,
    /// Stable hash of the scan's input set (sorted file paths).
    /// Used to detect stale checkpoints from a different scan scope.
    pub scan_id: String,
    /// Map of function name → deep fingerprint for completed functions.
    pub completed: HashMap<String, String>,
    /// Index of the last fully completed layer.
    pub layer_index: usize,
    /// RFC 3339 timestamp of last save.
    pub timestamp: String,
}

impl ScanCheckpoint {
    /// Create a new empty checkpoint for the given scan ID.
    pub fn new(scan_id: String) -> Self {
        Self {
            version: "1".to_string(),
            scan_id,
            completed: HashMap::new(),
            layer_index: 0,
            timestamp: String::new(),
        }
    }

    /// Load a checkpoint from disk. Returns `Ok(None)` if the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>, CheckpointError> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                let cp: ScanCheckpoint = serde_json::from_str(&contents)?;
                Ok(Some(cp))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CheckpointError::Io(e)),
        }
    }

    /// Persist the checkpoint atomically (temp file + rename).
    pub fn save(&mut self, path: &Path) -> Result<(), CheckpointError> {
        self.timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| format!("{}", d.as_secs()))
            .unwrap_or_default();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("checkpoint.tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Compute a stable scan ID from the set of source file paths.
    ///
    /// Sorts paths for determinism, so the same set of files always
    /// produces the same ID regardless of iteration order.
    pub fn compute_scan_id(file_paths: &[&str]) -> String {
        let mut sorted: Vec<&str> = file_paths.to_vec();
        sorted.sort();

        let mut hasher = Sha256::new();
        hasher.update(b"scan_id_v1:");
        for p in &sorted {
            hasher.update(p.as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }

    /// Check whether a function should be treated as already completed.
    ///
    /// Returns `true` only if all three conditions hold:
    /// 1. The checkpoint has an entry for this function
    /// 2. The stored deep fingerprint matches `current_deep_fp`
    /// 3. The behavior map still exists in the cache
    pub fn is_completed(
        &self,
        func_name: &str,
        current_deep_fp: &str,
        cache: &BehaviorMapCache,
    ) -> bool {
        if let Some(stored_fp) = self.completed.get(func_name) {
            stored_fp == current_deep_fp
                && cache.load(func_name).ok().flatten().is_some()
        } else {
            false
        }
    }

    /// Record a function as completed with its deep fingerprint.
    pub fn mark_completed(&mut self, func_name: &str, deep_fp: &str) {
        self.completed
            .insert(func_name.to_string(), deep_fp.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::Behavior;
    use crate::behavior::BehaviorMap;
    use serde_json::json;

    fn sample_map(function_id: &str, fingerprint: Option<&str>) -> BehaviorMap {
        BehaviorMap {
            function_id: function_id.to_string(),
            behaviors: vec![Behavior {
                id: 0,
                input_args: vec![json!(1)],
                return_value: Some(json!(2)),
                thrown_error: None,
                branch_path: vec![],
                side_effects: vec![],
                dependency_trace: None,
            }],
            fingerprint: fingerprint.map(String::from),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan.checkpoint");

        let mut cp = ScanCheckpoint::new("test-scan-id".into());
        cp.mark_completed("funcA", "fp_a");
        cp.mark_completed("funcB", "fp_b");
        cp.layer_index = 3;
        cp.save(&path).unwrap();

        let loaded = ScanCheckpoint::load(&path).unwrap().unwrap();
        assert_eq!(loaded.version, "1");
        assert_eq!(loaded.scan_id, "test-scan-id");
        assert_eq!(loaded.completed.len(), 2);
        assert_eq!(loaded.completed.get("funcA").unwrap(), "fp_a");
        assert_eq!(loaded.completed.get("funcB").unwrap(), "fp_b");
        assert_eq!(loaded.layer_index, 3);
        assert!(!loaded.timestamp.is_empty());
    }

    #[test]
    fn load_returns_none_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.checkpoint");
        let result = ScanCheckpoint::load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn is_completed_true_when_all_conditions_met() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        // Store a behavior map in the cache.
        let map = sample_map("funcA", Some("deep_fp_a"));
        cache.store(&map).unwrap();

        let mut cp = ScanCheckpoint::new("id".into());
        cp.mark_completed("funcA", "deep_fp_a");

        assert!(cp.is_completed("funcA", "deep_fp_a", &cache));
    }

    #[test]
    fn is_completed_false_when_fp_changed() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("funcA", Some("deep_fp_a"));
        cache.store(&map).unwrap();

        let mut cp = ScanCheckpoint::new("id".into());
        cp.mark_completed("funcA", "deep_fp_a");

        // Current deep FP is different from what was stored.
        assert!(!cp.is_completed("funcA", "deep_fp_CHANGED", &cache));
    }

    #[test]
    fn is_completed_false_when_cache_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();
        // No behavior map stored in cache.

        let mut cp = ScanCheckpoint::new("id".into());
        cp.mark_completed("funcA", "deep_fp_a");

        assert!(!cp.is_completed("funcA", "deep_fp_a", &cache));
    }

    #[test]
    fn is_completed_false_when_not_in_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("funcA", Some("fp"));
        cache.store(&map).unwrap();

        let cp = ScanCheckpoint::new("id".into());
        assert!(!cp.is_completed("funcA", "fp", &cache));
    }

    #[test]
    fn compute_scan_id_deterministic_regardless_of_order() {
        let id1 = ScanCheckpoint::compute_scan_id(&["src/a.ts", "src/b.ts", "src/c.ts"]);
        let id2 = ScanCheckpoint::compute_scan_id(&["src/c.ts", "src/a.ts", "src/b.ts"]);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 64);
    }

    #[test]
    fn compute_scan_id_differs_for_different_files() {
        let id1 = ScanCheckpoint::compute_scan_id(&["src/a.ts"]);
        let id2 = ScanCheckpoint::compute_scan_id(&["src/b.ts"]);
        assert_ne!(id1, id2);
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("scan.checkpoint");

        let mut cp = ScanCheckpoint::new("id".into());
        cp.save(&path).unwrap();
        assert!(path.exists());
    }
}
