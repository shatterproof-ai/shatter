//! Scan checkpoint for resume support.
//!
//! Persists the set of completed functions and their deep fingerprints after
//! each layer so an interrupted scan can resume without re-exploring finished
//! functions. The behavior map data itself lives in [`BehaviorMapCache`]; the
//! checkpoint is a lightweight index on top.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Hash of scan configuration (iterations, timeouts, parallelism, isolation).
    /// Used for soft drift detection — a mismatch logs a warning but does not
    /// invalidate the checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
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
            config_hash: None,
        }
    }

    /// Create a new empty checkpoint with a config hash for drift detection.
    pub fn new_with_config(scan_id: String, config_hash: String) -> Self {
        Self {
            version: "1".to_string(),
            scan_id,
            completed: HashMap::new(),
            layer_index: 0,
            timestamp: String::new(),
            config_hash: Some(config_hash),
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

    /// Check whether this checkpoint is compatible with the current scan.
    ///
    /// Returns `None` if compatible, or `Some(reason)` if the checkpoint
    /// should be discarded. This is a hard check — scan_id mismatch means
    /// the file set changed and cached results are not trustworthy.
    pub fn check_compatibility(&self, current_scan_id: &str) -> Option<String> {
        if self.scan_id != current_scan_id {
            Some(format!(
                "checkpoint scan_id mismatch: expected {}, found {}",
                current_scan_id, self.scan_id
            ))
        } else {
            None
        }
    }

    /// Check whether the scan configuration has drifted since the checkpoint
    /// was saved. Returns `None` if no drift, or `Some(reason)` describing
    /// the change.
    ///
    /// This is a soft warning — config drift does not invalidate the
    /// checkpoint because completed functions' results are still valid
    /// (the source code hasn't changed). The user may want to re-explore
    /// with different iteration counts, but skipping already-completed
    /// functions is still correct.
    pub fn check_config_drift(&self, current_config_hash: &str) -> Option<String> {
        match &self.config_hash {
            Some(stored) if stored != current_config_hash => Some(format!(
                "scan config changed since checkpoint (stored: {}, current: {}); \
                 completed functions will be reused, pending functions use new config",
                &stored[..stored.len().min(12)],
                &current_config_hash[..current_config_hash.len().min(12)]
            )),
            _ => None,
        }
    }

    /// Auto-discover a checkpoint file in the standard artifact directory.
    ///
    /// Looks for `checkpoint.json` in
    /// `<project_root>/shatter-artifacts/scan-results/<scan_id>/`.
    /// Returns `Some(path)` if found, `None` otherwise.
    pub fn auto_discover(project_root: Option<&str>, scan_id: &str) -> Option<PathBuf> {
        let path = Self::default_path(project_root, scan_id);
        if path.exists() { Some(path) } else { None }
    }

    /// Default checkpoint file path in the artifact directory.
    pub fn default_path(project_root: Option<&str>, scan_id: &str) -> PathBuf {
        let root = project_root.unwrap_or(".");
        PathBuf::from(root)
            .join("shatter-artifacts")
            .join("scan-results")
            .join(&scan_id[..scan_id.len().min(16)])
            .join("checkpoint.json")
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
            stored_fp == current_deep_fp && cache.load(func_name).ok().flatten().is_some()
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
                mock_values: vec![],
            }],
            fingerprint: fingerprint.map(String::from),
            nondeterministic_fields: vec![],
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
        let path = dir
            .path()
            .join("nested")
            .join("deep")
            .join("scan.checkpoint");

        let mut cp = ScanCheckpoint::new("id".into());
        cp.save(&path).unwrap();
        assert!(path.exists());
    }

    // --- New tests for str-7pkp.8 ---

    #[test]
    fn round_trip_with_config_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan.checkpoint");

        let mut cp = ScanCheckpoint::new_with_config("scan-1".into(), "cfg_abc123".into());
        cp.mark_completed("funcA", "fp_a");
        cp.save(&path).unwrap();

        let loaded = ScanCheckpoint::load(&path).unwrap().unwrap();
        assert_eq!(loaded.config_hash, Some("cfg_abc123".to_string()));
        assert_eq!(loaded.completed.len(), 1);
    }

    #[test]
    fn backward_compatible_load_without_config_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan.checkpoint");

        // Write a checkpoint without config_hash (simulates old format).
        let json = r#"{
            "version": "1",
            "scan_id": "old-scan",
            "completed": {"f": "fp"},
            "layer_index": 2,
            "timestamp": "12345"
        }"#;
        fs::write(&path, json).unwrap();

        let loaded = ScanCheckpoint::load(&path).unwrap().unwrap();
        assert_eq!(loaded.scan_id, "old-scan");
        assert_eq!(loaded.config_hash, None);
        assert_eq!(loaded.completed.len(), 1);
    }

    #[test]
    fn check_compatibility_matching() {
        let cp = ScanCheckpoint::new("scan-abc".into());
        assert!(cp.check_compatibility("scan-abc").is_none());
    }

    #[test]
    fn check_compatibility_mismatched() {
        let cp = ScanCheckpoint::new("scan-abc".into());
        let reason = cp.check_compatibility("scan-xyz").unwrap();
        assert!(reason.contains("mismatch"));
        assert!(reason.contains("scan-xyz"));
        assert!(reason.contains("scan-abc"));
    }

    #[test]
    fn check_config_drift_no_stored_hash() {
        let cp = ScanCheckpoint::new("id".into());
        // No config_hash stored → no drift detected.
        assert!(cp.check_config_drift("any_hash").is_none());
    }

    #[test]
    fn check_config_drift_matching() {
        let cp = ScanCheckpoint::new_with_config("id".into(), "hash_a".into());
        assert!(cp.check_config_drift("hash_a").is_none());
    }

    #[test]
    fn check_config_drift_changed() {
        let cp = ScanCheckpoint::new_with_config("id".into(), "hash_a".into());
        let reason = cp.check_config_drift("hash_b").unwrap();
        assert!(reason.contains("config changed"));
    }

    #[test]
    fn auto_discover_finds_existing_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let scan_id = "abcdef1234567890abcdef";
        let truncated = &scan_id[..16];

        // Create the expected directory structure.
        let checkpoint_dir = dir
            .path()
            .join("shatter-artifacts")
            .join("scan-results")
            .join(truncated);
        fs::create_dir_all(&checkpoint_dir).unwrap();
        fs::write(checkpoint_dir.join("checkpoint.json"), "{}").unwrap();

        let result = ScanCheckpoint::auto_discover(Some(dir.path().to_str().unwrap()), scan_id);
        assert!(result.is_some());
    }

    #[test]
    fn auto_discover_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let result =
            ScanCheckpoint::auto_discover(Some(dir.path().to_str().unwrap()), "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn default_path_structure() {
        let path = ScanCheckpoint::default_path(Some("/proj"), "abcdef1234567890rest");
        assert_eq!(
            path,
            PathBuf::from("/proj/shatter-artifacts/scan-results/abcdef1234567890/checkpoint.json")
        );
    }

    #[test]
    fn default_path_with_no_project_root() {
        let path = ScanCheckpoint::default_path(None, "abcdef1234567890rest");
        assert!(path.starts_with("./shatter-artifacts"));
    }
}
