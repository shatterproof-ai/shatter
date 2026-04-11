//! Data structures for the cross-function interesting input pool.
//!
//! Values discovered during exploration of one function are pooled and reused
//! as seeds for other functions with matching parameter types. Entry identity
//! is the `(ty, value)` pair — behaviors accumulate across functions.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::boundary_dict;
use crate::execution_record::ErrorInfo;
use crate::explorer;
use crate::protocol::{ExecuteResult, MockConfig};
use crate::types::{ParamInfo, TypeInfo};

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

/// Maximum distinct representatives (witnessing entries) retained per behavior
/// class (`BehaviorSig`). Once this many entries across the pool witness the
/// same `(function_id, branch_id, severity)`, further attempts to introduce
/// *new* witnesses for that class are dropped. The first representatives for
/// a class are preserved — they are never displaced by this cap.
pub const MAX_REPRESENTATIVES_PER_BEHAVIOR: usize = 10;

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
        let distinct_fns: std::collections::HashSet<&str> = entry
            .behaviors
            .iter()
            .map(|b| b.function.as_str())
            .collect();
        let breadth = (1.0 + distinct_fns.len() as f64).log2();
        max_severity as f64 * breadth
    }

    /// Return all pool values matching the given type.
    ///
    /// Used to inject cross-function seeds: values that triggered interesting
    /// behavior in other functions with compatible parameter types.
    pub fn values_for_type(&self, ty: &TypeInfo) -> Vec<serde_json::Value> {
        let key = type_key(ty);
        self.buckets
            .get(&key)
            .map(|entries| entries.iter().map(|e| e.value.clone()).collect())
            .unwrap_or_default()
    }

    /// Return all pool entries matching the given type, with full metadata.
    ///
    /// Unlike [`values_for_type`] which returns only values, this exposes
    /// [`BehaviorObservation`] metadata so callers can filter by source function.
    pub fn entries_for_type(&self, ty: &TypeInfo) -> &[PoolEntry] {
        let key = type_key(ty);
        self.buckets.get(&key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Count how many entries across all buckets witness `sig`.
    ///
    /// A `PoolEntry` witnesses a behavior class iff its `behaviors` vector
    /// contains a `BehaviorObservation` whose `BehaviorSig` equals `sig`.
    fn witness_count(&self, sig: &BehaviorSig) -> usize {
        self.buckets
            .values()
            .flat_map(|bucket| bucket.iter())
            .filter(|e| e.behaviors.iter().any(|o| BehaviorSig::from(o) == *sig))
            .count()
    }

    /// Insert an entry into the pool, evicting if the bucket is at capacity.
    ///
    /// Returns `true` if the entry was inserted (or merged), `false` if
    /// it was rejected because all existing entries have higher priority,
    /// or because every observation in the entry was dropped by the
    /// per-behavior-class cap (`MAX_REPRESENTATIVES_PER_BEHAVIOR`).
    pub fn insert(&mut self, mut entry: PoolEntry) -> bool {
        // Per-behavior cap: drop any observation whose behavior class already
        // has the maximum number of distinct witnesses, unless an entry with
        // the same value already witnesses that class (in which case the merge
        // is a no-op and does not add a new witness). We compute this *before*
        // the per-type-bucket mutable borrow to keep the witness scan cheap
        // and avoid aliasing.
        let existing_sigs_for_value: HashSet<BehaviorSig> = self
            .buckets
            .get(&type_key(&entry.ty))
            .and_then(|bucket| bucket.iter().find(|e| e.value == entry.value))
            .map(|existing| existing.behaviors.iter().map(BehaviorSig::from).collect())
            .unwrap_or_default();

        entry.behaviors.retain(|obs| {
            let sig = BehaviorSig::from(obs);
            if existing_sigs_for_value.contains(&sig) {
                // Value-merge into an entry that already witnesses this class:
                // preserving the observation is a no-op (dedup by sig happens
                // below), so it cannot add a new witness. Keep it.
                return true;
            }
            self.witness_count(&sig) < MAX_REPRESENTATIVES_PER_BEHAVIOR
        });

        if entry.behaviors.is_empty() {
            return false;
        }

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
/// Acquires an exclusive flock to prevent concurrent reads during a write.
/// Returns `Ok(None)` if the file does not exist (without acquiring a lock).
pub fn load_pool(path: &std::path::Path) -> Result<Option<InterestingPool>, std::io::Error> {
    if !path.exists() {
        return Ok(None);
    }
    let _lock = crate::file_lock::FileLock::acquire(path)?;
    load_pool_unlocked(path)
}

/// Load without acquiring a lock (for use when caller already holds the lock).
fn load_pool_unlocked(path: &std::path::Path) -> Result<Option<InterestingPool>, std::io::Error> {
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
/// Acquires an exclusive flock, then uses atomic write (temp file + rename).
/// Creates parent directories on first write. Keys are sorted for deterministic output.
pub fn save_pool(pool: &InterestingPool, path: &std::path::Path) -> Result<(), std::io::Error> {
    let _lock = crate::file_lock::FileLock::acquire(path)?;
    save_pool_unlocked(pool, path)
}

/// Best-effort save: skips if another process holds the lock.
pub fn save_pool_best_effort(
    pool: &InterestingPool,
    path: &std::path::Path,
) -> Result<bool, std::io::Error> {
    match crate::file_lock::FileLock::try_acquire(path)? {
        Some(_lock) => {
            save_pool_unlocked(pool, path)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

fn save_pool_unlocked(
    pool: &InterestingPool,
    path: &std::path::Path,
) -> Result<(), std::io::Error> {
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

    let content = serde_json::to_string_pretty(&wrapper).map_err(std::io::Error::other)?;

    // Atomic write: temp file + rename
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Paths exercised by at most this many inputs qualify as rare during harvesting.
pub const DEFAULT_RARITY_THRESHOLD: u32 = 2;

/// Harvest interesting inputs from raw exploration results into the pool.
///
/// Decomposes each execution's input vector into individual `(value, TypeInfo)`
/// pairs, filters out boundary-dict values (already tried everywhere), classifies
/// severity, and inserts into the pool. For error-triggering executions all inputs
/// are harvested; for non-error executions only those on rare paths (exercised by
/// ≤ `DEFAULT_RARITY_THRESHOLD` inputs) are kept.
///
/// Returns the number of pool entries inserted or merged.
pub fn harvest_from_exploration(
    pool: &mut InterestingPool,
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    params: &[ParamInfo],
    function_name: &str,
) -> usize {
    if raw_results.is_empty() || params.is_empty() {
        return 0;
    }

    // Build path frequency map for rarity classification.
    let mut path_counts: HashMap<u64, u32> = HashMap::new();
    let hashes: Vec<u64> = raw_results
        .iter()
        .map(|(_, _mocks, result)| {
            let h = explorer::path_hash(result, &explorer::LoopBuckets::default());
            *path_counts.entry(h).or_default() += 1;
            h
        })
        .collect();

    // Pre-compute boundary value sets per parameter position for filtering.
    let boundary_sets: Vec<HashSet<serde_json::Value>> = params
        .iter()
        .map(|p| {
            boundary_dict::get_boundary_values(&p.typ)
                .into_iter()
                .map(|entry| entry.value)
                .collect()
        })
        .collect();

    let epoch = pool.epoch;
    let mut inserted = 0;

    for (idx, (inputs, _mocks, exec_result)) in raw_results.iter().enumerate() {
        let severity = classify_severity(exec_result.thrown_error.as_ref(), false);

        // For RarePath, only harvest if the path is actually rare.
        if severity == Severity::RarePath {
            let count = path_counts.get(&hashes[idx]).copied().unwrap_or(0);
            if count > DEFAULT_RARITY_THRESHOLD {
                continue;
            }
        }

        // Determine branch_id from execution's branch path.
        let branch_id = exec_result
            .branch_path
            .last()
            .map(|d| d.branch_id)
            .unwrap_or(0);

        let obs = BehaviorObservation {
            function: function_name.to_string(),
            branch_id,
            severity,
        };

        // Decompose input vector into individual (value, type) entries.
        for (i, value) in inputs.iter().enumerate() {
            if i >= params.len() {
                break;
            }

            // Skip boundary-dict values — they're tried everywhere already.
            if boundary_sets[i].contains(value) {
                continue;
            }

            let entry = PoolEntry {
                value: value.clone(),
                ty: params[i].typ.clone(),
                behaviors: vec![obs.clone()],
                discovered_epoch: epoch,
                last_hit_epoch: epoch,
            };

            if pool.insert(entry) {
                inserted += 1;
            }
        }

        // For multi-param functions with compound types, also store the full
        // input vector so correlated multi-arg patterns are preserved.
        if params.len() > 1
            && inputs
                .iter()
                .zip(params.iter())
                .any(|(_, p)| matches!(p.typ, TypeInfo::Object { .. } | TypeInfo::Array { .. }))
        {
            let compound_type = TypeInfo::Array {
                element: Box::new(TypeInfo::Unknown),
            };
            let compound_entry = PoolEntry {
                value: serde_json::Value::Array(inputs.clone()),
                ty: compound_type,
                behaviors: vec![obs],
                discovered_epoch: epoch,
                last_hit_epoch: epoch,
            };
            if pool.insert(compound_entry) {
                inserted += 1;
            }
        }
    }

    inserted
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
        assert_eq!(
            classify_severity(Some(&err), false),
            Severity::UnhandledError
        );
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
        assert_eq!(
            classify_severity(Some(&err), false),
            Severity::UnhandledError
        );
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

        let mut pool = InterestingPool {
            epoch: 3,
            ..Default::default()
        };
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

    #[test]
    fn values_for_type_returns_matching_entries() {
        let mut pool = InterestingPool::default();
        pool.insert(make_entry(serde_json::json!(42), "foo", Severity::RarePath));
        pool.insert(make_entry(serde_json::json!(99), "bar", Severity::Crash));

        let values = pool.values_for_type(&TypeInfo::Int);
        assert_eq!(values.len(), 2);
        assert!(values.contains(&serde_json::json!(42)));
        assert!(values.contains(&serde_json::json!(99)));
    }

    #[test]
    fn values_for_type_returns_empty_for_missing_type() {
        let mut pool = InterestingPool::default();
        pool.insert(make_entry(serde_json::json!(42), "foo", Severity::RarePath));

        let values = pool.values_for_type(&TypeInfo::Str);
        assert!(values.is_empty());
    }

    #[test]
    fn entries_for_type_returns_full_entries() {
        let mut pool = InterestingPool::default();
        pool.insert(make_entry(serde_json::json!(42), "foo", Severity::RarePath));
        pool.insert(make_entry(serde_json::json!(99), "bar", Severity::Crash));

        let entries = pool.entries_for_type(&TypeInfo::Int);
        assert_eq!(entries.len(), 2);
        // Verify behavior metadata is accessible.
        let functions: Vec<&str> = entries
            .iter()
            .flat_map(|e| e.behaviors.iter().map(|b| b.function.as_str()))
            .collect();
        assert!(functions.contains(&"foo"));
        assert!(functions.contains(&"bar"));
    }

    #[test]
    fn entries_for_type_returns_empty_for_missing_type() {
        let mut pool = InterestingPool::default();
        pool.insert(make_entry(serde_json::json!(42), "foo", Severity::RarePath));

        let entries = pool.entries_for_type(&TypeInfo::Str);
        assert!(entries.is_empty());
    }

    // -- harvest_from_exploration tests --

    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::{ExecuteResult, PerformanceMetrics};

    fn make_params(types: &[TypeInfo]) -> Vec<ParamInfo> {
        types
            .iter()
            .enumerate()
            .map(|(i, ty)| ParamInfo {
                name: format!("p{i}"),
                typ: ty.clone(),
                type_name: None,
            })
            .collect()
    }

    fn make_exec_result_ok(branch_id: u32) -> ExecuteResult {
        ExecuteResult {
            return_value: Some(serde_json::json!(0)),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id,
                line: 1,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
        }
    }

    fn make_exec_result_error(error_type: &str, branch_id: u32) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: error_type.into(),
                message: "test error".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![BranchDecision {
                branch_id,
                line: 1,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
        }
    }

    #[test]
    fn harvest_handles_empty_results() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let count = harvest_from_exploration(&mut pool, &[], &make_params(&[TypeInfo::Int]), "f");
        assert_eq!(count, 0);
        assert!(pool.buckets.is_empty());
    }

    #[test]
    fn harvest_inserts_error_inputs() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Int]);
        // Value 999 is not a boundary value for Int
        let raw = vec![(
            vec![serde_json::json!(999)],
            vec![],
            make_exec_result_error("TypeError", 5),
        )];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "myFunc");
        assert_eq!(count, 1);
        let key = type_key(&TypeInfo::Int);
        let bucket = &pool.buckets[&key];
        assert_eq!(bucket.len(), 1);
        assert_eq!(bucket[0].value, serde_json::json!(999));
        assert_eq!(bucket[0].behaviors[0].severity, Severity::UnhandledError);
        assert_eq!(bucket[0].behaviors[0].function, "myFunc");
        assert_eq!(bucket[0].behaviors[0].branch_id, 5);
    }

    #[test]
    fn harvest_inserts_rare_path_inputs() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Str]);
        // Single execution → path count = 1, which is ≤ DEFAULT_RARITY_THRESHOLD
        let raw = vec![(
            vec![serde_json::json!("rare_value")],
            vec![],
            make_exec_result_ok(3),
        )];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "f");
        assert_eq!(count, 1);
    }

    #[test]
    fn harvest_skips_common_paths() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Int]);
        // Same branch path for all 3 executions → count = 3 > threshold
        let exec = make_exec_result_ok(1);
        let raw = vec![
            (vec![serde_json::json!(100)], vec![], exec.clone()),
            (vec![serde_json::json!(200)], vec![], exec.clone()),
            (vec![serde_json::json!(300)], vec![], exec),
        ];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "f");
        assert_eq!(count, 0);
    }

    #[test]
    fn harvest_skips_boundary_values() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Int]);
        // 0 and -1 are boundary values for Int; should be skipped even on error paths
        let raw = vec![
            (
                vec![serde_json::json!(0)],
                vec![],
                make_exec_result_error("TypeError", 1),
            ),
            (
                vec![serde_json::json!(-1)],
                vec![],
                make_exec_result_error("TypeError", 2),
            ),
        ];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "f");
        assert_eq!(count, 0);
    }

    #[test]
    fn harvest_decomposes_vectors() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Int, TypeInfo::Str]);
        let raw = vec![(
            vec![serde_json::json!(42), serde_json::json!("hello")],
            vec![],
            make_exec_result_error("RangeError", 1),
        )];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "f");
        // 42 is not a boundary Int, "hello" is not a boundary Str → both inserted
        assert_eq!(count, 2);
        let int_key = type_key(&TypeInfo::Int);
        let str_key = type_key(&TypeInfo::Str);
        assert_eq!(pool.buckets[&int_key].len(), 1);
        assert_eq!(pool.buckets[&str_key].len(), 1);
    }

    #[test]
    fn harvest_merges_same_value() {
        let mut pool = InterestingPool {
            epoch: 1,
            ..Default::default()
        };
        let params = make_params(&[TypeInfo::Int]);
        // Same value (999) from two different error executions
        let raw = vec![
            (
                vec![serde_json::json!(999)],
                vec![],
                make_exec_result_error("TypeError", 1),
            ),
            (
                vec![serde_json::json!(999)],
                vec![],
                make_exec_result_error("RangeError", 2),
            ),
        ];
        let count = harvest_from_exploration(&mut pool, &raw, &params, "f");
        // Both insert calls succeed (second merges), so count = 2
        assert_eq!(count, 2);
        let key = type_key(&TypeInfo::Int);
        // But only one entry in the bucket (merged)
        assert_eq!(pool.buckets[&key].len(), 1);
        assert_eq!(pool.buckets[&key][0].behaviors.len(), 2);
    }

    // -- MAX_REPRESENTATIVES_PER_BEHAVIOR cap tests --

    /// Build a pool large enough that the per-bucket cap never interferes
    /// with per-behavior-cap tests.
    fn uncapped_pool() -> InterestingPool {
        InterestingPool {
            bucket_cap: 10_000,
            ..Default::default()
        }
    }

    fn behavior_entry(value: i64, function: &str, branch_id: u32, severity: Severity) -> PoolEntry {
        PoolEntry {
            value: serde_json::json!(value),
            ty: TypeInfo::Int,
            behaviors: vec![BehaviorObservation {
                function: function.into(),
                branch_id,
                severity,
            }],
            discovered_epoch: 0,
            last_hit_epoch: 0,
        }
    }

    #[test]
    fn insert_caps_representatives_per_behavior() {
        let mut pool = uncapped_pool();
        // Attempt to insert MAX + 5 distinct values all witnessing the same class.
        let attempts = MAX_REPRESENTATIVES_PER_BEHAVIOR + 5;
        for i in 0..attempts {
            let inserted =
                pool.insert(behavior_entry(i as i64, "foo", 1, Severity::UnhandledError));
            if i < MAX_REPRESENTATIVES_PER_BEHAVIOR {
                assert!(inserted, "entry {i} under cap should insert");
            } else {
                assert!(!inserted, "entry {i} over cap should be rejected");
            }
        }
        let sig = BehaviorSig {
            function_id: "foo".into(),
            branch_id: 1,
            severity: Severity::UnhandledError,
        };
        assert_eq!(pool.witness_count(&sig), MAX_REPRESENTATIVES_PER_BEHAVIOR);
    }

    #[test]
    fn insert_first_representatives_preserved_after_cap_hits() {
        let mut pool = uncapped_pool();
        // Insert the first batch of representatives and record their values.
        let mut pioneers = Vec::new();
        for i in 0..MAX_REPRESENTATIVES_PER_BEHAVIOR {
            pool.insert(behavior_entry(i as i64, "foo", 1, Severity::RarePath));
            pioneers.push(serde_json::json!(i as i64));
        }
        // Keep trying to push more — these must all be rejected.
        for i in 100..120 {
            assert!(!pool.insert(behavior_entry(i, "foo", 1, Severity::RarePath)));
        }
        // Every pioneer value is still present.
        let bucket = &pool.buckets[&type_key(&TypeInfo::Int)];
        for pioneer in &pioneers {
            assert!(
                bucket.iter().any(|e| &e.value == pioneer),
                "pioneer {pioneer:?} should still be present",
            );
        }
    }

    #[test]
    fn insert_below_cap_never_silently_drops() {
        let mut pool = uncapped_pool();
        let under_cap = MAX_REPRESENTATIVES_PER_BEHAVIOR - 1;
        for i in 0..under_cap {
            assert!(pool.insert(behavior_entry(i as i64, "bar", 2, Severity::HandledError)));
        }
        let sig = BehaviorSig {
            function_id: "bar".into(),
            branch_id: 2,
            severity: Severity::HandledError,
        };
        assert_eq!(pool.witness_count(&sig), under_cap);
    }

    #[test]
    fn insert_allows_distinct_behaviors_independently() {
        let mut pool = uncapped_pool();
        // Fill class A to cap.
        for i in 0..MAX_REPRESENTATIVES_PER_BEHAVIOR {
            pool.insert(behavior_entry(i as i64, "A", 1, Severity::Crash));
        }
        // A different class B should still accept new witnesses.
        let inserted = pool.insert(behavior_entry(1_000, "B", 1, Severity::Crash));
        assert!(inserted);
        let sig_b = BehaviorSig {
            function_id: "B".into(),
            branch_id: 1,
            severity: Severity::Crash,
        };
        assert_eq!(pool.witness_count(&sig_b), 1);
    }

    #[test]
    fn insert_rejects_when_all_observations_capped() {
        let mut pool = uncapped_pool();
        for i in 0..MAX_REPRESENTATIVES_PER_BEHAVIOR {
            pool.insert(behavior_entry(i as i64, "foo", 9, Severity::Crash));
        }
        // Entry with only a capped observation and a novel value — dropped.
        let e = behavior_entry(999, "foo", 9, Severity::Crash);
        assert!(!pool.insert(e));
        let sig = BehaviorSig {
            function_id: "foo".into(),
            branch_id: 9,
            severity: Severity::Crash,
        };
        assert_eq!(pool.witness_count(&sig), MAX_REPRESENTATIVES_PER_BEHAVIOR);
    }

    #[test]
    fn insert_merges_capped_class_into_existing_witness() {
        // An entry whose value already exists in the pool and already
        // witnesses the capped class should still be able to merge
        // unrelated observations that are themselves below cap.
        let mut pool = uncapped_pool();
        for i in 0..MAX_REPRESENTATIVES_PER_BEHAVIOR {
            pool.insert(behavior_entry(i as i64, "foo", 1, Severity::Crash));
        }
        // Re-insert the very first witness with a brand-new, uncapped class
        // and the *already-present* capped class. The capped class is
        // preserved (value-merge → no new witness). The uncapped class
        // is added.
        let mut merged = behavior_entry(0, "foo", 1, Severity::Crash);
        merged.behaviors.push(BehaviorObservation {
            function: "bar".into(),
            branch_id: 7,
            severity: Severity::RarePath,
        });
        assert!(pool.insert(merged));
        let bucket = &pool.buckets[&type_key(&TypeInfo::Int)];
        let first = bucket
            .iter()
            .find(|e| e.value == serde_json::json!(0))
            .unwrap();
        assert_eq!(first.behaviors.len(), 2);
    }

    #[test]
    fn save_load_roundtrip_with_capped_class() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("seeds/pool.json");
        let mut pool = uncapped_pool();
        pool.epoch = 7;
        for i in 0..MAX_REPRESENTATIVES_PER_BEHAVIOR + 3 {
            pool.insert(behavior_entry(i as i64, "foo", 1, Severity::UnhandledError));
        }
        save_pool(&pool, &path).expect("save pool");
        let loaded = load_pool(&path).expect("load pool").expect("pool exists");

        let sig = BehaviorSig {
            function_id: "foo".into(),
            branch_id: 1,
            severity: Severity::UnhandledError,
        };
        assert_eq!(loaded.witness_count(&sig), MAX_REPRESENTATIVES_PER_BEHAVIOR);
        assert_eq!(loaded.epoch, 7);
    }

    // -- Property-based tests for the per-behavior cap --

    use proptest::prelude::*;

    /// Small value space (0..20) and small behavior-sig space (3 funcs × 3
    /// branches × 2 severities = 18 classes). With 40 attempts many classes
    /// will hit the cap.
    fn arb_insert_op() -> impl Strategy<Value = (i64, String, u32, Severity)> {
        (
            0i64..20,
            prop_oneof![
                Just("f0".to_string()),
                Just("f1".to_string()),
                Just("f2".to_string())
            ],
            0u32..3,
            prop_oneof![Just(Severity::RarePath), Just(Severity::UnhandledError)],
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Cap invariant: no behavior class ever exceeds
        /// `MAX_REPRESENTATIVES_PER_BEHAVIOR` witnesses in the pool.
        #[test]
        fn prop_witness_count_never_exceeds_cap(
            ops in proptest::collection::vec(arb_insert_op(), 0..40usize),
        ) {
            let mut pool = InterestingPool {
                bucket_cap: 10_000,
                ..Default::default()
            };
            for (value, func, branch, severity) in &ops {
                let _ = pool.insert(behavior_entry(*value, func, *branch, *severity));
            }
            // Collect every unique BehaviorSig that appears in the pool.
            let mut sigs = std::collections::HashSet::new();
            for bucket in pool.buckets.values() {
                for entry in bucket {
                    for obs in &entry.behaviors {
                        sigs.insert(BehaviorSig::from(obs));
                    }
                }
            }
            for sig in &sigs {
                prop_assert!(
                    pool.witness_count(sig) <= MAX_REPRESENTATIVES_PER_BEHAVIOR,
                    "class {:?} exceeded cap",
                    sig,
                );
            }
        }

        /// Pioneer invariant: the first entry whose insertion is reported
        /// successful for a given class stays in the pool, even if later
        /// attempts for that class are rejected.
        #[test]
        fn prop_first_representative_preserved(
            ops in proptest::collection::vec(arb_insert_op(), 0..40usize),
        ) {
            let mut pool = InterestingPool {
                bucket_cap: 10_000,
                ..Default::default()
            };
            // Track the first (value, class) pair that was actually inserted
            // for each class.
            let mut pioneers: HashMap<BehaviorSig, serde_json::Value> = HashMap::new();
            for (value, func, branch, severity) in &ops {
                let sig = BehaviorSig {
                    function_id: func.clone(),
                    branch_id: *branch,
                    severity: *severity,
                };
                let before = pool.witness_count(&sig);
                let inserted = pool.insert(behavior_entry(*value, func, *branch, *severity));
                let after = pool.witness_count(&sig);
                // Only record a pioneer when this call actually added a new
                // witness for the class. Value-dedup merges don't count.
                if inserted && after > before && !pioneers.contains_key(&sig) {
                    pioneers.insert(sig, serde_json::json!(*value));
                }
            }
            for (sig, pioneer_value) in &pioneers {
                let present = pool
                    .buckets
                    .values()
                    .flat_map(|b| b.iter())
                    .any(|e| {
                        e.value == *pioneer_value
                            && e.behaviors.iter().any(|o| BehaviorSig::from(o) == *sig)
                    });
                prop_assert!(
                    present,
                    "pioneer {:?} for class {:?} was lost",
                    pioneer_value,
                    sig,
                );
            }
        }

        /// Under-cap preservation: if a class receives strictly fewer than
        /// `MAX_REPRESENTATIVES_PER_BEHAVIOR` distinct-value insertions, every
        /// such distinct value still witnesses the class at the end.
        #[test]
        fn prop_under_cap_never_silently_drops(
            ops in proptest::collection::vec(arb_insert_op(), 0..40usize),
        ) {
            let mut pool = InterestingPool {
                bucket_cap: 10_000,
                ..Default::default()
            };
            // Group unique values attempted per class.
            let mut attempts_per_class: HashMap<BehaviorSig, HashSet<i64>> = HashMap::new();
            for (value, func, branch, severity) in &ops {
                let sig = BehaviorSig {
                    function_id: func.clone(),
                    branch_id: *branch,
                    severity: *severity,
                };
                attempts_per_class.entry(sig).or_default().insert(*value);
            }
            for (value, func, branch, severity) in &ops {
                let _ = pool.insert(behavior_entry(*value, func, *branch, *severity));
            }
            for (sig, values) in &attempts_per_class {
                if values.len() < MAX_REPRESENTATIVES_PER_BEHAVIOR {
                    for v in values {
                        let jv = serde_json::json!(*v);
                        let witnesses = pool
                            .buckets
                            .values()
                            .flat_map(|b| b.iter())
                            .any(|e| {
                                e.value == jv
                                    && e.behaviors.iter().any(|o| BehaviorSig::from(o) == *sig)
                            });
                        prop_assert!(
                            witnesses,
                            "under-cap class {:?} lost distinct value {}",
                            sig,
                            v,
                        );
                    }
                }
            }
        }
    }
}
