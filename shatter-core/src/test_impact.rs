//! Coverage-based test impact analysis.
//!
//! Maintains a coverage map recording which tests touch which source files,
//! then uses that map to select the minimal test set for a given set of changes.
//! The forward map (test → files) is the source of truth; the reverse map
//! (file → tests) is derived at load time and never persisted.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::file_lock::FileLock;
use crate::scm;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Relative path (within `.shatter/`) for the coverage map file.
pub const COVERAGE_MAP_REL_PATH: &str = "test-markers/coverage-map.yaml";

/// Relative path (within `.shatter/`) for tier marker directory.
pub const TIER_MARKER_DIR: &str = "test-markers/tiers";

/// Current schema version for the coverage map.
pub const COVERAGE_MAP_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from test-impact-analysis operations.
#[derive(Debug, thiserror::Error)]
pub enum TiaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("SCM error: {0}")]
    Scm(#[from] scm::ScmError),

    #[error("runner error: {message}")]
    Runner { message: String },

    #[error("no coverage map found at {path}")]
    NoCoverageMap { path: PathBuf },
}

// ---------------------------------------------------------------------------
// Coverage map data model
// ---------------------------------------------------------------------------

/// Serialized coverage map (forward direction only).
/// The reverse index is derived at load time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageMapData {
    pub version: u32,
    pub recorded_at: String,
    /// Forward map: test identifier → files it touches.
    pub entries: BTreeMap<String, TestEntry>,
}

/// Files touched by a single test, with content-addressable blob hashes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestEntry {
    /// Relative path → git blob hash at recording time.
    pub files: BTreeMap<String, String>,
}

/// In-memory coverage map with derived reverse index.
#[derive(Debug)]
pub struct CoverageMap {
    pub data: CoverageMapData,
    /// Derived: file path → test identifiers that touch it.
    reverse: HashMap<String, Vec<String>>,
}

/// Result of querying the coverage map for affected tests.
#[derive(Debug)]
pub struct ImpactQuery {
    /// Files that changed (relative paths).
    pub changed_files: Vec<String>,
    /// Tests affected by the changed files (deduplicated).
    pub affected_tests: Vec<String>,
    /// Tests whose recorded blob hashes no longer match current files.
    pub stale_entries: Vec<String>,
    /// Changed files not present in the coverage map.
    pub unmapped_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tier markers
// ---------------------------------------------------------------------------

/// Marker written after a test tier passes successfully.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TierMarker {
    pub tier: String,
    pub passed_at: String,
    pub git_commit: String,
}

// ---------------------------------------------------------------------------
// CoverageMap implementation
// ---------------------------------------------------------------------------

impl CoverageMap {
    /// Load the coverage map from `.shatter/<COVERAGE_MAP_REL_PATH>`.
    pub fn load(shatter_dir: &Path) -> Result<Self, TiaError> {
        let path = shatter_dir.join(COVERAGE_MAP_REL_PATH);
        if !path.exists() {
            return Err(TiaError::NoCoverageMap { path });
        }
        let contents = std::fs::read_to_string(&path)?;
        let data: CoverageMapData = serde_yaml::from_str(&contents)?;
        let reverse = build_reverse_index(&data.entries);
        Ok(Self { data, reverse })
    }

    /// Create an empty coverage map.
    pub fn empty() -> Self {
        let data = CoverageMapData {
            version: COVERAGE_MAP_VERSION,
            recorded_at: String::new(),
            entries: BTreeMap::new(),
        };
        Self {
            reverse: HashMap::new(),
            data,
        }
    }

    /// Save the coverage map atomically (tempfile + rename) under a file lock.
    pub fn save(&self, shatter_dir: &Path) -> Result<(), TiaError> {
        let path = shatter_dir.join(COVERAGE_MAP_REL_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let _lock = FileLock::acquire(&path)?;

        let yaml = serde_yaml::to_string(&self.data)?;
        let tmp_path = path.with_extension("yaml.tmp");
        std::fs::write(&tmp_path, yaml)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Update the forward map from coverage output, recompute blob hashes, rebuild reverse.
    pub fn update_from_coverage(
        &mut self,
        test_file_map: &BTreeMap<String, Vec<String>>,
        project_root: &Path,
    ) -> Result<(), TiaError> {
        for (test_id, files) in test_file_map {
            let mut file_hashes = BTreeMap::new();
            for file in files {
                let abs_path = project_root.join(file);
                if abs_path.exists() {
                    match scm::blob_hash(project_root, Path::new(file)) {
                        Ok(hash) => {
                            file_hashes.insert(file.clone(), hash);
                        }
                        Err(_) => {
                            // Skip files that can't be hashed (e.g. binary, missing)
                            continue;
                        }
                    }
                }
            }
            self.data
                .entries
                .insert(test_id.clone(), TestEntry { files: file_hashes });
        }

        self.data.recorded_at = now_iso8601();
        self.reverse = build_reverse_index(&self.data.entries);
        Ok(())
    }

    /// Query: given changed files (relative paths), return affected tests.
    pub fn query_affected(&self, changed_files: &[String]) -> ImpactQuery {
        let mut affected_set = HashSet::new();
        let mut unmapped = Vec::new();

        for file in changed_files {
            if let Some(tests) = self.reverse.get(file) {
                for t in tests {
                    affected_set.insert(t.clone());
                }
            } else {
                unmapped.push(file.clone());
            }
        }

        let mut affected_tests: Vec<String> = affected_set.into_iter().collect();
        affected_tests.sort();

        ImpactQuery {
            changed_files: changed_files.to_vec(),
            affected_tests,
            stale_entries: Vec::new(),
            unmapped_files: unmapped,
        }
    }

    /// Find tests whose recorded blob hashes no longer match current file content.
    pub fn find_stale_entries(&self, project_root: &Path) -> Vec<String> {
        let mut stale = Vec::new();
        for (test_id, entry) in &self.data.entries {
            for (file, recorded_hash) in &entry.files {
                match scm::blob_hash(project_root, Path::new(file)) {
                    Ok(current_hash) => {
                        if &current_hash != recorded_hash {
                            stale.push(test_id.clone());
                            break;
                        }
                    }
                    Err(_) => {
                        // File missing or not hashable → stale
                        stale.push(test_id.clone());
                        break;
                    }
                }
            }
        }
        stale.sort();
        stale
    }

    /// Access the reverse index.
    pub fn reverse_index(&self) -> &HashMap<String, Vec<String>> {
        &self.reverse
    }
}

// ---------------------------------------------------------------------------
// Reverse index builder (pure function)
// ---------------------------------------------------------------------------

/// Build the reverse index: file → [test_ids] from the forward map.
pub fn build_reverse_index(entries: &BTreeMap<String, TestEntry>) -> HashMap<String, Vec<String>> {
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for (test_id, entry) in entries {
        for file in entry.files.keys() {
            reverse
                .entry(file.clone())
                .or_default()
                .push(test_id.clone());
        }
    }
    // Sort each list for deterministic output
    for tests in reverse.values_mut() {
        tests.sort();
        tests.dedup();
    }
    reverse
}

// ---------------------------------------------------------------------------
// Tier marker operations
// ---------------------------------------------------------------------------

/// Write a tier marker after successful test run.
pub fn write_tier_marker(shatter_dir: &Path, tier: &str, git_commit: &str) -> Result<(), TiaError> {
    let dir = shatter_dir.join(TIER_MARKER_DIR);
    std::fs::create_dir_all(&dir)?;

    let marker = TierMarker {
        tier: tier.to_string(),
        passed_at: now_iso8601(),
        git_commit: git_commit.to_string(),
    };

    let path = dir.join(format!("{tier}.yaml"));
    let yaml = serde_yaml::to_string(&marker)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}

/// Read a tier marker, returning None if it doesn't exist.
pub fn read_tier_marker(shatter_dir: &Path, tier: &str) -> Result<Option<TierMarker>, TiaError> {
    let path = shatter_dir
        .join(TIER_MARKER_DIR)
        .join(format!("{tier}.yaml"));
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path)?;
    let marker: TierMarker = serde_yaml::from_str(&contents)?;
    Ok(Some(marker))
}

/// Check whether a tier marker is fresh (matches the given commit).
pub fn is_tier_fresh(
    shatter_dir: &Path,
    tier: &str,
    current_commit: &str,
) -> Result<bool, TiaError> {
    match read_tier_marker(shatter_dir, tier)? {
        Some(marker) => Ok(marker.git_commit == current_commit),
        None => Ok(false),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_iso8601() -> String {
    // Use a simple approach without chrono dependency
    let output = std::process::Command::new("date")
        .args(["--iso-8601=seconds"])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_reverse_index_basic() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "test_a".to_string(),
            TestEntry {
                files: BTreeMap::from([
                    ("src/foo.rs".to_string(), "aaa".to_string()),
                    ("src/bar.rs".to_string(), "bbb".to_string()),
                ]),
            },
        );
        entries.insert(
            "test_b".to_string(),
            TestEntry {
                files: BTreeMap::from([("src/foo.rs".to_string(), "aaa".to_string())]),
            },
        );

        let reverse = build_reverse_index(&entries);
        assert_eq!(reverse.get("src/foo.rs").map(|v| v.len()), Some(2));
        assert_eq!(reverse.get("src/bar.rs").map(|v| v.len()), Some(1));
        assert!(
            reverse
                .get("src/foo.rs")
                .unwrap()
                .contains(&"test_a".to_string())
        );
        assert!(
            reverse
                .get("src/foo.rs")
                .unwrap()
                .contains(&"test_b".to_string())
        );
    }

    #[test]
    fn build_reverse_index_empty() {
        let entries = BTreeMap::new();
        let reverse = build_reverse_index(&entries);
        assert!(reverse.is_empty());
    }

    #[test]
    fn query_affected_returns_union() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "test_a".to_string(),
            TestEntry {
                files: BTreeMap::from([("src/a.rs".to_string(), "h1".to_string())]),
            },
        );
        entries.insert(
            "test_b".to_string(),
            TestEntry {
                files: BTreeMap::from([("src/b.rs".to_string(), "h2".to_string())]),
            },
        );
        entries.insert(
            "test_c".to_string(),
            TestEntry {
                files: BTreeMap::from([
                    ("src/a.rs".to_string(), "h1".to_string()),
                    ("src/b.rs".to_string(), "h2".to_string()),
                ]),
            },
        );

        let map = CoverageMap {
            data: CoverageMapData {
                version: COVERAGE_MAP_VERSION,
                recorded_at: "now".to_string(),
                entries,
            },
            reverse: HashMap::new(),
        };
        let map = CoverageMap {
            reverse: build_reverse_index(&map.data.entries),
            ..map
        };

        let result = map.query_affected(&["src/a.rs".to_string()]);
        assert!(result.affected_tests.contains(&"test_a".to_string()));
        assert!(result.affected_tests.contains(&"test_c".to_string()));
        assert!(!result.affected_tests.contains(&"test_b".to_string()));

        // Query both files → all three tests
        let result = map.query_affected(&["src/a.rs".to_string(), "src/b.rs".to_string()]);
        assert_eq!(result.affected_tests.len(), 3);
    }

    #[test]
    fn query_affected_deduplicates() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "test_x".to_string(),
            TestEntry {
                files: BTreeMap::from([
                    ("src/a.rs".to_string(), "h1".to_string()),
                    ("src/b.rs".to_string(), "h2".to_string()),
                ]),
            },
        );

        let reverse = build_reverse_index(&entries);
        let map = CoverageMap {
            data: CoverageMapData {
                version: COVERAGE_MAP_VERSION,
                recorded_at: "now".to_string(),
                entries,
            },
            reverse,
        };

        // Both files map to test_x — should appear only once
        let result = map.query_affected(&["src/a.rs".to_string(), "src/b.rs".to_string()]);
        assert_eq!(result.affected_tests, vec!["test_x".to_string()]);
    }

    #[test]
    fn query_affected_tracks_unmapped_files() {
        let map = CoverageMap::empty();
        let result = map.query_affected(&["unknown.rs".to_string()]);
        assert!(result.affected_tests.is_empty());
        assert_eq!(result.unmapped_files, vec!["unknown.rs".to_string()]);
    }

    #[test]
    fn coverage_map_yaml_roundtrip() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "crate::mod::test_fn".to_string(),
            TestEntry {
                files: BTreeMap::from([
                    ("src/lib.rs".to_string(), "abc123".to_string()),
                    ("src/util.rs".to_string(), "def456".to_string()),
                ]),
            },
        );

        let data = CoverageMapData {
            version: COVERAGE_MAP_VERSION,
            recorded_at: "2026-03-07T23:00:00+00:00".to_string(),
            entries,
        };

        let yaml = serde_yaml::to_string(&data).unwrap();
        let parsed: CoverageMapData = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(data, parsed);
    }

    #[test]
    fn coverage_map_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let shatter_dir = dir.path().join(".shatter");

        let mut map = CoverageMap::empty();
        map.data.entries.insert(
            "test_1".to_string(),
            TestEntry {
                files: BTreeMap::from([("src/main.rs".to_string(), "hash1".to_string())]),
            },
        );
        map.data.recorded_at = "2026-03-07".to_string();
        map.reverse = build_reverse_index(&map.data.entries);

        map.save(&shatter_dir).unwrap();
        let loaded = CoverageMap::load(&shatter_dir).unwrap();
        assert_eq!(map.data, loaded.data);
        assert_eq!(loaded.reverse.get("src/main.rs").map(|v| v.len()), Some(1));
    }

    #[test]
    fn tier_marker_write_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let shatter_dir = dir.path().join(".shatter");

        write_tier_marker(&shatter_dir, "quick", "abc123").unwrap();
        let marker = read_tier_marker(&shatter_dir, "quick")
            .unwrap()
            .expect("marker should exist");
        assert_eq!(marker.tier, "quick");
        assert_eq!(marker.git_commit, "abc123");
    }

    #[test]
    fn tier_marker_freshness() {
        let dir = tempfile::tempdir().unwrap();
        let shatter_dir = dir.path().join(".shatter");

        assert!(!is_tier_fresh(&shatter_dir, "quick", "abc").unwrap());

        write_tier_marker(&shatter_dir, "quick", "abc").unwrap();
        assert!(is_tier_fresh(&shatter_dir, "quick", "abc").unwrap());
        assert!(!is_tier_fresh(&shatter_dir, "quick", "def").unwrap());
    }

    #[test]
    fn read_nonexistent_tier_marker() {
        let dir = tempfile::tempdir().unwrap();
        let result = read_tier_marker(dir.path(), "e2e").unwrap();
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_test_entry() -> impl Strategy<Value = TestEntry> {
        prop::collection::btree_map("[a-z/]{1,20}\\.rs", "[0-9a-f]{6,12}", 0..=5)
            .prop_map(|files| TestEntry { files })
    }

    fn arb_coverage_map_data() -> impl Strategy<Value = CoverageMapData> {
        prop::collection::btree_map("[a-z_:]{1,30}", arb_test_entry(), 0..=10).prop_map(|entries| {
            CoverageMapData {
                version: COVERAGE_MAP_VERSION,
                recorded_at: "2026-01-01T00:00:00Z".to_string(),
                entries,
            }
        })
    }

    proptest! {
        /// YAML roundtrip preserves all data.
        #[test]
        fn yaml_roundtrip(data in arb_coverage_map_data()) {
            let yaml = serde_yaml::to_string(&data).unwrap();
            let parsed: CoverageMapData = serde_yaml::from_str(&yaml).unwrap();
            prop_assert_eq!(data, parsed);
        }

        /// Reverse index invariant: every (test, file) pair in forward map
        /// has file→test in reverse.
        #[test]
        fn reverse_index_covers_forward(data in arb_coverage_map_data()) {
            let reverse = build_reverse_index(&data.entries);
            for (test_id, entry) in &data.entries {
                for file in entry.files.keys() {
                    let tests = reverse.get(file).unwrap();
                    prop_assert!(tests.contains(test_id));
                }
            }
        }

        /// Query monotonicity: query(A ∪ B) ⊇ query(A).
        #[test]
        fn query_monotonicity(
            data in arb_coverage_map_data(),
            extra_files in prop::collection::vec("[a-z/]{1,15}\\.rs", 0..=3),
        ) {
            let reverse = build_reverse_index(&data.entries);
            let map = CoverageMap { data, reverse };

            // Collect all known files
            let known_files: Vec<String> = map.reverse.keys().cloned().collect();
            if known_files.is_empty() {
                return Ok(());
            }

            // Take a subset A
            let a: Vec<String> = known_files.iter().take(1).cloned().collect();
            // B = A ∪ extra
            let mut b = a.clone();
            b.extend(extra_files);

            let result_a = map.query_affected(&a);
            let result_b = map.query_affected(&b);

            let set_a: HashSet<_> = result_a.affected_tests.iter().collect();
            let set_b: HashSet<_> = result_b.affected_tests.iter().collect();

            for t in &set_a {
                prop_assert!(set_b.contains(t), "monotonicity violated: {t} in A but not in A∪B");
            }
        }

        /// Reverse index has no duplicates per file.
        #[test]
        fn reverse_index_no_duplicates(data in arb_coverage_map_data()) {
            let reverse = build_reverse_index(&data.entries);
            for tests in reverse.values() {
                let unique: HashSet<_> = tests.iter().collect();
                prop_assert_eq!(tests.len(), unique.len());
            }
        }
    }
}
