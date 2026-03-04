//! Data structures for the cross-function interesting input pool.
//!
//! Values discovered during exploration of one function are pooled and reused
//! as seeds for other functions with matching parameter types. Entry identity
//! is the `(ty, value)` pair — behaviors accumulate across functions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::execution_record::ErrorInfo;
use crate::types::TypeInfo;

/// How severe the behavior triggered by an input was.
///
/// Ordered low-to-high so that [`Ord`] gives natural severity comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Novel path exercised by very few inputs, no error.
    RarePath = 1,
    /// Thrown error with an application-defined exception type.
    HandledError = 2,
    /// Thrown error with a runtime error type (TypeError, panic, etc.).
    UnhandledError = 3,
    /// Frontend process died, timed out, or protocol error.
    Crash = 4,
}

/// A single behavior observed when running a particular input against a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorObservation {
    /// Fully qualified function identifier.
    pub function: String,
    /// Branch point that was exercised.
    pub branch_id: u32,
    /// Severity of the observed behavior.
    pub severity: Severity,
}

/// Grouping key for deduplication and eviction decisions.
///
/// Two observations with the same `BehaviorSig` are considered redundant
/// witnesses to the same behavior.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BehaviorSig {
    /// Fully qualified function identifier.
    pub function_id: String,
    /// Branch point that was exercised.
    pub branch_id: u32,
    /// Severity of the observed behavior.
    pub severity: Severity,
}

impl From<&BehaviorObservation> for BehaviorSig {
    fn from(obs: &BehaviorObservation) -> Self {
        Self {
            function_id: obs.function.clone(),
            branch_id: obs.branch_id,
            severity: obs.severity,
        }
    }
}

/// A single entry in the interesting input pool.
///
/// Identity is the `(ty, value)` pair. When the same value is observed to
/// trigger interesting behavior in a different function, its `behaviors`
/// vector grows rather than creating a duplicate entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolEntry {
    /// The concrete input value (JSON-encoded).
    pub value: serde_json::Value,
    /// Type of the value, used for matching against function parameters.
    #[serde(rename = "type")]
    pub ty: TypeInfo,
    /// All interesting behaviors this value has triggered across functions.
    pub behaviors: Vec<BehaviorObservation>,
    /// Epoch at which this entry was first added to the pool.
    pub discovered_epoch: u64,
    /// Most recent epoch at which this entry triggered a new behavior.
    pub last_hit_epoch: u64,
}

/// Known runtime error type names per language. An error whose `error_type`
/// matches any of these (case-insensitive) is classified as `UnhandledError`
/// rather than `HandledError`.
const RUNTIME_ERROR_TYPES: &[&str] = &[
    // JavaScript / TypeScript
    "typeerror",
    "referenceerror",
    "rangeerror",
    "syntaxerror",
    "urierror",
    "evalerror",
    // Go
    "runtime_error",
    "panic",
    // Java / JVM
    "nullpointerexception",
    "classcastexception",
    "arrayindexoutofboundsexception",
    "stackoverflowerror",
    // Rust
    "panic",
    // Python
    "attributeerror",
    "indexerror",
    "keyerror",
    "zerodivisionerror",
];

/// Classify the severity of an execution result.
///
/// - `Crash` if the frontend itself failed (indicated by `is_crash`).
/// - `UnhandledError` if thrown_error matches a known runtime error type.
/// - `HandledError` if thrown_error is present but not a known runtime type.
/// - `RarePath` if no error occurred and the path is novel.
pub fn classify_severity(thrown_error: Option<&ErrorInfo>, is_crash: bool) -> Severity {
    if is_crash {
        return Severity::Crash;
    }
    match thrown_error {
        Some(err) => {
            let lower = err.error_type.to_lowercase();
            if RUNTIME_ERROR_TYPES.contains(&lower.as_str()) {
                Severity::UnhandledError
            } else {
                Severity::HandledError
            }
        }
        None => Severity::RarePath,
    }
}

/// Default maximum entries per type bucket.
pub const DEFAULT_BUCKET_CAP: usize = 50;

/// Coverage tier for eviction decisions.
///
/// Tier 1 entries are sole witnesses to at least one behavior; tier 0 entries
/// have all behaviors covered by other witnesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverageTier {
    /// All behaviors have other witnesses — safe to evict first.
    Redundant = 0,
    /// Sole witness to at least one behavior — evict only as last resort.
    UniqueWitness = 1,
}

/// Serialize a `TypeInfo` into the canonical string key used for bucket lookup.
///
/// `TypeInfo` contains `serde_json::Value` (in `Complex.metadata`) which does
/// not implement `Hash`, so we use a deterministic JSON serialization as the
/// map key instead.
fn type_key(ty: &TypeInfo) -> String {
    serde_json::to_string(ty).unwrap_or_else(|_| format!("{ty:?}"))
}

/// The interesting input pool: type-keyed buckets of inputs that triggered
/// interesting behaviors during exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestingPool {
    /// Pool format version.
    pub version: u32,
    /// Monotonic counter incremented each scan.
    pub epoch: u64,
    /// Type-keyed buckets of interesting inputs. Keys are canonical JSON
    /// serializations of [`TypeInfo`] (see [`type_key`]).
    pub buckets: HashMap<String, Vec<PoolEntry>>,
    /// Maximum entries per type bucket.
    #[serde(default = "default_bucket_cap")]
    pub bucket_cap: usize,
}

fn default_bucket_cap() -> usize {
    DEFAULT_BUCKET_CAP
}

impl Default for InterestingPool {
    fn default() -> Self {
        Self {
            version: 2,
            epoch: 0,
            buckets: HashMap::new(),
            bucket_cap: DEFAULT_BUCKET_CAP,
        }
    }
}

impl InterestingPool {
    /// Compute the coverage tier for an entry within its bucket.
    ///
    /// An entry is `UniqueWitness` if at least one of its behaviors has no
    /// other witness in the same bucket.
    fn coverage_tier(
        entry_idx: usize,
        bucket: &[PoolEntry],
        sig_witnesses: &HashMap<BehaviorSig, Vec<usize>>,
    ) -> CoverageTier {
        for obs in &bucket[entry_idx].behaviors {
            let sig = BehaviorSig::from(obs);
            if let Some(witnesses) = sig_witnesses.get(&sig)
                && witnesses.len() == 1
                && witnesses[0] == entry_idx
            {
                return CoverageTier::UniqueWitness;
            }
        }
        CoverageTier::Redundant
    }

    /// Build the behavior→witness index for a bucket.
    fn build_sig_index(bucket: &[PoolEntry]) -> HashMap<BehaviorSig, Vec<usize>> {
        let mut index: HashMap<BehaviorSig, Vec<usize>> = HashMap::new();
        for (i, entry) in bucket.iter().enumerate() {
            for obs in &entry.behaviors {
                index.entry(BehaviorSig::from(obs)).or_default().push(i);
            }
        }
        index
    }

    /// Quality score for eviction ordering (higher = better, keep longer).
    ///
    /// `severity × breadth` where breadth = log2(1 + distinct_function_count).
    fn quality_score(entry: &PoolEntry) -> f64 {
        let max_severity = entry
            .behaviors
            .iter()
            .map(|b| b.severity as u32)
            .max()
            .unwrap_or(0);
        let distinct_fns: std::collections::HashSet<&str> =
            entry.behaviors.iter().map(|b| b.function.as_str()).collect();
        let breadth = (1.0 + distinct_fns.len() as f64).log2();
        max_severity as f64 * breadth
    }

    /// Insert an entry into the pool, evicting if the bucket is at capacity.
    ///
    /// Returns `true` if the entry was inserted (or merged), `false` if
    /// it was rejected because all existing entries have higher priority.
    pub fn insert(&mut self, entry: PoolEntry) -> bool {
        let key = type_key(&entry.ty);
        let bucket = self.buckets.entry(key).or_default();

        // Check if this value already exists in the bucket
        if let Some(existing) = bucket.iter_mut().find(|e| e.value == entry.value) {
            // Merge behaviors
            for obs in entry.behaviors {
                let sig = BehaviorSig::from(&obs);
                let already = existing
                    .behaviors
                    .iter()
                    .any(|b| BehaviorSig::from(b) == sig);
                if !already {
                    existing.behaviors.push(obs);
                }
            }
            existing.last_hit_epoch = existing.last_hit_epoch.max(entry.last_hit_epoch);
            return true;
        }

        // If bucket has room, just insert
        if bucket.len() < self.bucket_cap {
            bucket.push(entry);
            return true;
        }

        // Eviction: find the lowest-priority entry to replace
        let sig_index = Self::build_sig_index(bucket);
        let mut worst_idx = None;
        let mut worst_tier = CoverageTier::UniqueWitness;
        let mut worst_quality = f64::INFINITY;

        for i in 0..bucket.len() {
            let tier = Self::coverage_tier(i, bucket, &sig_index);
            let quality = Self::quality_score(&bucket[i]);
            if (tier, quality as u64) < (worst_tier, worst_quality as u64) {
                worst_idx = Some(i);
                worst_tier = tier;
                worst_quality = quality;
            }
        }

        // Only evict if the new entry has higher quality than the worst
        let new_quality = Self::quality_score(&entry);
        if let Some(idx) = worst_idx
            && (worst_tier == CoverageTier::Redundant || new_quality > worst_quality)
        {
            bucket[idx] = entry;
            return true;
        }

        false
    }
}

/// Load the interesting pool from a JSON file at the given path.
///
/// Returns `Ok(None)` if the file does not exist.
pub fn load_pool(path: &std::path::Path) -> Result<Option<InterestingPool>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let pool: InterestingPool = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(Some(pool))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Save the interesting pool to a JSON file at the given path.
///
/// Uses atomic write (temp file + rename) and creates parent directories
/// on first write. Keys are sorted for deterministic output.
pub fn save_pool(pool: &InterestingPool, path: &std::path::Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Sort buckets by type key for deterministic output
    let sorted: std::collections::BTreeMap<&String, &Vec<PoolEntry>> =
        pool.buckets.iter().collect();

    let wrapper = serde_json::json!({
        "version": pool.version,
        "epoch": pool.epoch,
        "bucket_cap": pool.bucket_cap,
        "buckets": sorted,
    });

    let content = serde_json::to_string_pretty(&wrapper)
        .map_err(std::io::Error::other)?;

    // Atomic write: temp file + rename
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::RarePath < Severity::HandledError);
        assert!(Severity::HandledError < Severity::UnhandledError);
        assert!(Severity::UnhandledError < Severity::Crash);
    }

    #[test]
    fn behavior_sig_from_observation() {
        let obs = BehaviorObservation {
            function: "myModule.foo".into(),
            branch_id: 3,
            severity: Severity::UnhandledError,
        };
        let sig = BehaviorSig::from(&obs);
        assert_eq!(sig.function_id, "myModule.foo");
        assert_eq!(sig.branch_id, 3);
        assert_eq!(sig.severity, Severity::UnhandledError);
    }

    #[test]
    fn pool_entry_serde_round_trip() {
        let entry = PoolEntry {
            value: serde_json::json!(42),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "mod.bar".into(),
                branch_id: 1,
                severity: Severity::Crash,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 1,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let back: PoolEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, back);
    }

    #[test]
    fn behavior_sig_hash_equality() {
        use std::collections::HashSet;
        let sig1 = BehaviorSig {
            function_id: "f".into(),
            branch_id: 1,
            severity: Severity::RarePath,
        };
        let sig2 = sig1.clone();
        let mut set = HashSet::new();
        set.insert(sig1);
        assert!(set.contains(&sig2));
    }

    // -- classify_severity tests --

    #[test]
    fn classify_crash_overrides_error() {
        let err = ErrorInfo {
            error_type: "TypeError".into(),
            message: "oops".into(),
            stack: None,
            error_category: None,
        };
        assert_eq!(classify_severity(Some(&err), true), Severity::Crash);
    }

    #[test]
    fn classify_runtime_error_as_unhandled() {
        let err = ErrorInfo {
            error_type: "TypeError".into(),
            message: "cannot read property".into(),
            stack: None,
            error_category: None,
        };
        assert_eq!(classify_severity(Some(&err), false), Severity::UnhandledError);
    }

    #[test]
    fn classify_custom_error_as_handled() {
        let err = ErrorInfo {
            error_type: "ValidationError".into(),
            message: "invalid input".into(),
            stack: None,
            error_category: None,
        };
        assert_eq!(classify_severity(Some(&err), false), Severity::HandledError);
    }

    #[test]
    fn classify_no_error_as_rare_path() {
        assert_eq!(classify_severity(None, false), Severity::RarePath);
    }

    #[test]
    fn classify_case_insensitive() {
        let err = ErrorInfo {
            error_type: "REFERENCEERROR".into(),
            message: "x is not defined".into(),
            stack: None,
            error_category: None,
        };
        assert_eq!(classify_severity(Some(&err), false), Severity::UnhandledError);
    }

    // -- InterestingPool tests --

    fn make_entry(value: serde_json::Value, function: &str, severity: Severity) -> PoolEntry {
        PoolEntry {
            value: value.clone(),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: function.into(),
                branch_id: 1,
                severity,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 0,
        }
    }

    #[test]
    fn pool_insert_and_merge() {
        let mut pool = InterestingPool::default();
        let e1 = make_entry(serde_json::json!(42), "foo", Severity::RarePath);
        assert!(pool.insert(e1));

        // Same value, different behavior — should merge
        let e2 = PoolEntry {
            value: serde_json::json!(42),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: "bar".into(),
                branch_id: 2,
                severity: Severity::UnhandledError,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 1,
        };
        assert!(pool.insert(e2));

        let bucket = &pool.buckets[&type_key(&TypeInfo::Int)];
        assert_eq!(bucket.len(), 1);
        assert_eq!(bucket[0].behaviors.len(), 2);
        assert_eq!(bucket[0].last_hit_epoch, 1);
    }

    #[test]
    fn pool_evicts_redundant_first() {
        let mut pool = InterestingPool {
            bucket_cap: 2,
            ..Default::default()
        };

        // Entry A: unique witness to behavior (foo, 1)
        pool.insert(make_entry(serde_json::json!(1), "foo", Severity::Crash));
        // Entry B: also witnesses (foo, 1) — redundant
        pool.insert(make_entry(serde_json::json!(2), "foo", Severity::Crash));
        // Entry C: should evict B (redundant), not A (unique witness)
        let inserted = pool.insert(make_entry(serde_json::json!(3), "bar", Severity::RarePath));

        assert!(inserted);
        let bucket = &pool.buckets[&type_key(&TypeInfo::Int)];
        assert_eq!(bucket.len(), 2);
    }

    // -- Persistence tests --

    #[test]
    fn pool_save_load_round_trip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("seeds/pool.json");

        let mut pool = InterestingPool::default();
        pool.epoch = 3;
        pool.insert(make_entry(serde_json::json!(42), "foo", Severity::RarePath));

        save_pool(&pool, &path).expect("save pool");

        let loaded = load_pool(&path).expect("load pool").expect("pool exists");
        assert_eq!(loaded.epoch, 3);
        assert_eq!(loaded.version, 2);
        assert!(!loaded.buckets.is_empty());
    }

    #[test]
    fn load_pool_missing_returns_none() {
        let path = std::path::Path::new("/nonexistent/pool.json");
        let result = load_pool(path).expect("should not error");
        assert!(result.is_none());
    }
}
