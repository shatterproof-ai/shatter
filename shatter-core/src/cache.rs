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
    ///
    /// Persists whatever fingerprint is already on `map` (may be `None`). Prefer
    /// [`store_with_fingerprint`](Self::store_with_fingerprint) for any caller
    /// that knows the current function fingerprint — that path guarantees the
    /// stored entry carries a fingerprint and is therefore eligible for
    /// body-change invalidation by [`is_fresh`](Self::is_fresh).
    pub fn store(&self, map: &BehaviorMap) -> Result<(), CacheError> {
        self.write_entry(&map.function_id, map)
    }

    /// Store a behavior map with an explicit fingerprint for staleness tracking.
    ///
    /// Overwrites `map.fingerprint` with `fingerprint` before serializing, so the
    /// persisted entry always carries the caller's current fingerprint regardless
    /// of what was on the in-memory `BehaviorMap`. This is the only store path
    /// that lets [`is_fresh`](Self::is_fresh) detect body changes; callers that
    /// have the current fingerprint available (explore, scan) should use this.
    pub fn store_with_fingerprint(
        &self,
        map: &BehaviorMap,
        fingerprint: &str,
    ) -> Result<(), CacheError> {
        let mut stamped = map.clone();
        stamped.set_fingerprint(fingerprint);
        self.write_entry(&stamped.function_id.clone(), &stamped)
    }

    fn write_entry(&self, function_id: &str, map: &BehaviorMap) -> Result<(), CacheError> {
        let path = self.path_for(function_id);
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

    /// Check whether a cached behavior map is fresh under `current_fingerprint`,
    /// and drop the on-disk entry when it is stale.
    ///
    /// Returns `true` iff the cache contains a version-compatible map for
    /// `function_id` whose stored fingerprint equals `current_fingerprint`.
    ///
    /// Side effect: when a map is loaded but its stored fingerprint is either
    /// missing (legacy entry written before the fingerprint contract) or
    /// differs from `current_fingerprint`, the backing file is unlinked before
    /// returning `false`. This is the mechanism by which function-body changes
    /// drop stale behavior maps: the caller computes a new fingerprint from the
    /// current source, `is_fresh` sees the mismatch, and the disk entry is
    /// removed so subsequent loads are cache misses. A failure to unlink is
    /// logged via `log::warn!` but does not propagate as an error — the caller
    /// still gets the correct `false` freshness answer and will re-explore.
    ///
    /// Unrelated functions stored under different `function_id`s are untouched
    /// because [`path_for`](Self::path_for) maps each function ID to its own
    /// file under a hierarchical directory structure.
    pub fn is_fresh(
        &self,
        function_id: &str,
        current_fingerprint: &str,
    ) -> Result<bool, CacheError> {
        let path = self.path_for(function_id);
        let map = match self.load(function_id)? {
            Some(m) => m,
            None => return Ok(false),
        };
        let fresh = map
            .fingerprint
            .as_deref()
            .is_some_and(|fp| fp == current_fingerprint);
        if !fresh
            && path.exists()
            && let Err(e) = fs::remove_file(&path)
        {
            log::warn!(
                "failed to drop stale behavior map {} at {}: {e}",
                function_id,
                path.display()
            );
        }
        Ok(fresh)
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
                .is_some_and(|s| s.ends_with(".spec") || s.contains(".scheduler."))
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
        let (file_count, bytes) = crate::analysis_cache::count_dir_contents(&self.cache_dir)?;
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
                .is_some_and(|s| s.ends_with(".spec") || s.contains(".scheduler."))
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

// ---------------------------------------------------------------------------
// Scheduler-state cache (str-bo4z.5)
// ---------------------------------------------------------------------------

/// Default mode tag used when no explicit scheduler mode is supplied.
///
/// Scheduler state is namespaced by mode so that downstream work (str-bo4z.6
/// split-by-mode) can keep independent records per exploration mode without a
/// schema migration. Today every caller passes this constant.
pub const DEFAULT_SCHEDULER_MODE: &str = "default";

/// Schema version for the scheduler-state sidecar file.
///
/// Bumped independently of `PROTOCOL_VERSION` when the [`SchedulerState`] field
/// layout changes in a way existing readers cannot tolerate. Additive changes
/// are absorbed by `#[serde(default)]` on every field and do not require a
/// bump; removing or retyping a field does.
pub const SCHEDULER_SCHEMA_VERSION: u32 = 1;

/// Per-function scheduler metadata persisted between explore runs.
///
/// **Advisory and reconstructible**: the explore loop must work correctly when
/// the persisted state is missing, corrupt, or rejected by schema validation.
/// Canonical sources of truth remain [`BehaviorMap`] and stored inputs; this
/// record is a cache/hint that lets the round-robin scheduler resume a run
/// without re-learning per-function batch progress from scratch.
///
/// Every field is `#[serde(default)]` so future optional additions do not
/// invalidate older files — a file written by today's binary can be read by a
/// future binary that has added new fields, and vice versa (the new reader
/// fills defaults; the old reader drops unknown fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SchedulerState {
    /// Function identifier this record describes (e.g. `src/auth.ts:login`).
    pub function_id: String,
    /// Body fingerprint at the time this record was written. Enables
    /// body-change invalidation (see [`SchedulerStateCache::is_fresh`]).
    pub fingerprint: Option<String>,
    /// Total iterations consumed across all batches so far.
    pub iterations_consumed: u32,
    /// Number of batches that completed for this function.
    pub batches_completed: u32,
    /// Whether the caller marked this function exhausted (fully explored or
    /// out of budget).
    pub exhausted: bool,
    /// Optional exploration-mode tag recorded inside the state for diagnostics.
    /// The on-disk path already namespaces by mode; this field is informational.
    pub mode: Option<String>,
    /// Branches the scheduler believes remain uncovered. Placeholder for
    /// str-bo4z.2 / str-bo4z.7; empty today.
    pub uncovered_branches: Vec<String>,
}

/// Versioned envelope for cached [`SchedulerState`] entries.
///
/// A protocol-version or schema-version mismatch invalidates the entry
/// silently on read, mirroring the behavior map and spec caches.
#[derive(Debug, Serialize, Deserialize)]
struct SchedulerStateCacheEntry {
    protocol_version: String,
    schema_version: u32,
    scheduler_state: SchedulerState,
}

/// Disk-backed cache for storing and loading per-function [`SchedulerState`].
///
/// Sidecar layout: colocated with the behavior map under the same hierarchical
/// path, with extension `scheduler.<mode>.json`. Multiple modes per function
/// coexist as distinct files so downstream split-by-mode work is a parameter
/// change, not a schema change.
#[derive(Debug)]
pub struct SchedulerStateCache {
    cache_dir: PathBuf,
}

impl SchedulerStateCache {
    /// Create a new scheduler-state cache backed by the given directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    pub fn new(cache_dir: PathBuf) -> Result<Self, CacheError> {
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Default scheduler-state cache directory: colocated with behavior maps
    /// under `<project_root>/.shatter-cache/behavior-maps/`.
    pub fn default_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter-cache").join("behavior-maps")
    }

    /// Load the scheduler state for `(function_id, mode)`, if one exists.
    ///
    /// Returns `Ok(None)` on cache miss, non-UTF8 contents, parse error,
    /// protocol version mismatch, or schema version mismatch. The only `Err`
    /// path is a genuine filesystem I/O failure other than `NotFound` /
    /// `InvalidData`. This aligns with the "advisory and reconstructible"
    /// contract: a corrupt persisted file must degrade to a cache miss so the
    /// engine can rebuild scheduler state from scratch.
    pub fn load(
        &self,
        function_id: &str,
        mode: &str,
    ) -> Result<Option<SchedulerState>, CacheError> {
        let path = self.path_for(function_id, mode);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CacheError::Io(e)),
        };
        let contents = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        let entry: SchedulerStateCacheEntry = match serde_json::from_str(contents) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        if entry.protocol_version != PROTOCOL_VERSION
            || entry.schema_version != SCHEDULER_SCHEMA_VERSION
        {
            return Ok(None);
        }
        Ok(Some(entry.scheduler_state))
    }

    /// Store scheduler state to disk using atomic write (temp file + rename).
    ///
    /// The file path is derived from `state.function_id` and `mode`; callers
    /// set the mode explicitly (typically [`DEFAULT_SCHEDULER_MODE`]).
    pub fn store(&self, state: &SchedulerState, mode: &str) -> Result<(), CacheError> {
        let path = self.path_for(&state.function_id, mode);
        let entry = SchedulerStateCacheEntry {
            protocol_version: PROTOCOL_VERSION.to_string(),
            schema_version: SCHEDULER_SCHEMA_VERSION,
            scheduler_state: state.clone(),
        };
        let json = serde_json::to_string_pretty(&entry)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Append `.tmp` to the full filename so concurrent writes for
        // different modes do not collide on the same temp file (a plain
        // `with_extension("tmp")` would replace the `.scheduler.<mode>.json`
        // suffix entirely).
        let mut tmp_os = path.clone().into_os_string();
        tmp_os.push(".tmp");
        let tmp_path = PathBuf::from(tmp_os);
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }

    /// Check whether a cached scheduler state is fresh under
    /// `current_fingerprint`, and drop the on-disk entry when it is stale.
    ///
    /// Returns `true` iff the cache contains a version-compatible record for
    /// `(function_id, mode)` whose stored fingerprint equals
    /// `current_fingerprint`. On mismatch — including legacy entries written
    /// without a fingerprint — the backing file is unlinked before returning
    /// `false`, mirroring [`BehaviorMapCache::is_fresh`]. A failure to unlink
    /// is logged but does not propagate as an error.
    pub fn is_fresh(
        &self,
        function_id: &str,
        mode: &str,
        current_fingerprint: &str,
    ) -> Result<bool, CacheError> {
        let path = self.path_for(function_id, mode);
        let state = match self.load(function_id, mode)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let fresh = state
            .fingerprint
            .as_deref()
            .is_some_and(|fp| fp == current_fingerprint);
        if !fresh
            && path.exists()
            && let Err(e) = fs::remove_file(&path)
        {
            log::warn!(
                "failed to drop stale scheduler state {} at {}: {e}",
                function_id,
                path.display()
            );
        }
        Ok(fresh)
    }

    /// Load cached scheduler state for `(function_id, mode)` only if its
    /// stored fingerprint matches `current_fingerprint`.
    ///
    /// Returns `Ok(Some(state))` when the cached fingerprint exactly equals
    /// `current_fingerprint`. On any kind of staleness — fingerprint mismatch,
    /// legacy entry written without a fingerprint, version-skewed envelope, or
    /// unparseable bytes — the on-disk file is unlinked (best-effort) and
    /// `Ok(None)` is returned, so the caller starts the function as
    /// effectively unexplored. A genuine filesystem I/O failure other than
    /// `NotFound` is the only `Err` path.
    ///
    /// This is the body-change invalidation hook for str-bo4z.2: callers in
    /// [`crate::scan_orchestrator`] use it instead of [`Self::load`] when they
    /// have a current function fingerprint, so a function whose body has
    /// changed since the last run does not resume from stale scheduler memory.
    pub fn load_if_fresh(
        &self,
        function_id: &str,
        mode: &str,
        current_fingerprint: &str,
    ) -> Result<Option<SchedulerState>, CacheError> {
        let Some(state) = self.load(function_id, mode)? else {
            return Ok(None);
        };
        let fresh = state
            .fingerprint
            .as_deref()
            .is_some_and(|fp| fp == current_fingerprint);
        if !fresh {
            if let Err(e) = self.clear_function(function_id, mode) {
                log::warn!(
                    "failed to drop stale scheduler state {function_id} (mode={mode}): {e}"
                );
            }
            return Ok(None);
        }
        Ok(Some(state))
    }

    /// Remove the on-disk entry for `(function_id, mode)` if present.
    ///
    /// Missing files are not an error — this is a best-effort cleanup.
    pub fn clear_function(&self, function_id: &str, mode: &str) -> Result<(), CacheError> {
        let path = self.path_for(function_id, mode);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    fn path_for(&self, function_id: &str, mode: &str) -> PathBuf {
        let base = cache_base_path(&self.cache_dir, function_id);
        let safe_mode = sanitize_component(mode);
        base.with_extension(format!("scheduler.{safe_mode}.json"))
    }
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
        assert_eq!(
            cache_dir,
            dir.path().join(".shatter-cache").join("behavior-maps")
        );
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
    fn store_with_fingerprint_stamps_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        // Start with no fingerprint on the in-memory map — store_with_fingerprint
        // must attach it before persisting.
        let map = sample_map("stampedFn");
        assert!(map.fingerprint.is_none());

        cache.store_with_fingerprint(&map, "stamp-fp").unwrap();

        let loaded = cache.load("stampedFn").unwrap().unwrap();
        assert_eq!(loaded.fingerprint.as_deref(), Some("stamp-fp"));
        assert!(cache.is_fresh("stampedFn", "stamp-fp").unwrap());
    }

    #[test]
    fn store_with_fingerprint_overwrites_existing_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let mut map = sample_map("overwriteFn");
        map.fingerprint = Some("old-fp".to_string());
        cache.store_with_fingerprint(&map, "new-fp").unwrap();

        let loaded = cache.load("overwriteFn").unwrap().unwrap();
        assert_eq!(loaded.fingerprint.as_deref(), Some("new-fp"));
    }

    #[test]
    fn is_fresh_drops_stale_entry_on_body_change() {
        // Regression: str-bo4z.1 — body change must drop the cached map so
        // subsequent loads do not silently return stale behaviors.
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("src/auth.ts:validateToken");
        cache
            .store_with_fingerprint(&map, "fingerprint-before-edit")
            .unwrap();

        // The file exists before the staleness check.
        let path = dir
            .path()
            .join("src")
            .join("auth.ts")
            .join("validateToken.json");
        assert!(path.exists(), "stored entry file should exist pre-check");

        // A new fingerprint simulates a body change.
        assert!(
            !cache
                .is_fresh("src/auth.ts:validateToken", "fingerprint-after-edit")
                .unwrap()
        );

        // Side effect: the stale file is dropped.
        assert!(
            !path.exists(),
            "is_fresh should have removed the stale entry"
        );

        // A subsequent load is now a clean cache miss.
        assert_eq!(cache.load("src/auth.ts:validateToken").unwrap(), None);
    }

    #[test]
    fn is_fresh_preserves_unrelated_function_on_body_change() {
        // Regression: str-bo4z.1 — invalidating one function must not cascade
        // into unrelated siblings, even when they share a file prefix.
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let changed = sample_map("src/auth.ts:validateToken");
        let unchanged = sample_map("src/auth.ts:hashPassword");
        cache
            .store_with_fingerprint(&changed, "changed-fp")
            .unwrap();
        cache
            .store_with_fingerprint(&unchanged, "unchanged-fp")
            .unwrap();

        // Invalidate only validateToken by checking with a mismatched fingerprint.
        assert!(
            !cache
                .is_fresh("src/auth.ts:validateToken", "different-fp")
                .unwrap()
        );

        // hashPassword's cached entry is untouched: still fresh, still loadable.
        assert!(
            cache
                .is_fresh("src/auth.ts:hashPassword", "unchanged-fp")
                .unwrap()
        );
        let preserved = cache
            .load("src/auth.ts:hashPassword")
            .unwrap()
            .expect("unrelated function should still be cached");
        assert_eq!(preserved.function_id, "src/auth.ts:hashPassword");
        assert_eq!(preserved.fingerprint.as_deref(), Some("unchanged-fp"));
    }

    #[test]
    fn is_fresh_drops_legacy_none_fingerprint_entry() {
        // Legacy writers (the plain `store` path) persist maps with
        // fingerprint=None. These must be treated as stale and cleaned up the
        // first time a fingerprinted freshness check visits them, so the cache
        // self-heals without requiring a separate migration step.
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("legacyFn");
        assert!(map.fingerprint.is_none());
        cache.store(&map).unwrap();

        let path = dir.path().join("legacyFn.json");
        assert!(path.exists());

        assert!(!cache.is_fresh("legacyFn", "any-fp").unwrap());
        assert!(
            !path.exists(),
            "legacy None-fingerprint entry should be dropped"
        );
    }

    #[test]
    fn is_fresh_keeps_fresh_entry_on_disk() {
        // The delete-on-stale side effect must not touch fresh entries — a
        // matching fingerprint leaves the file in place so repeat checks stay
        // cheap.
        let dir = tempfile::tempdir().unwrap();
        let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("keeperFn");
        cache.store_with_fingerprint(&map, "keep-fp").unwrap();

        let path = dir.path().join("keeperFn.json");
        assert!(path.exists());

        assert!(cache.is_fresh("keeperFn", "keep-fp").unwrap());
        assert!(path.exists(), "fresh entry should not be deleted");

        // And a second check still returns true.
        assert!(cache.is_fresh("keeperFn", "keep-fp").unwrap());
    }

    #[test]
    fn store_and_load_preserves_nondeterministic_fields() {
        use crate::nondeterminism::{Confidence, NondeterminismEvidence, NondeterministicField};

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
        assert_eq!(
            loaded.nondeterministic_fields[0].confidence,
            Confidence::High
        );
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
        cache
            .store("src/auth.ts:TokenValidator.validate", &spec)
            .unwrap();

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

    // --- SchedulerStateCache tests (str-bo4z.5) ---

    fn sample_scheduler_state(function_id: &str) -> SchedulerState {
        SchedulerState {
            function_id: function_id.to_string(),
            fingerprint: Some("fp-v1".to_string()),
            iterations_consumed: 125,
            batches_completed: 3,
            exhausted: false,
            mode: Some(DEFAULT_SCHEDULER_MODE.to_string()),
            uncovered_branches: vec!["branch-a".to_string(), "branch-b".to_string()],
        }
    }

    #[test]
    fn scheduler_state_store_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("src/auth.ts:login");
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let loaded = cache
            .load("src/auth.ts:login", DEFAULT_SCHEDULER_MODE)
            .unwrap();
        assert_eq!(loaded, Some(state));
    }

    #[test]
    fn scheduler_state_load_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(
            cache.load("nonexistent", DEFAULT_SCHEDULER_MODE).unwrap(),
            None
        );
    }

    #[test]
    fn scheduler_state_hierarchical_path() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("src/auth.ts:TokenValidator.validate");
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let expected = dir
            .path()
            .join("src")
            .join("auth.ts")
            .join("TokenValidator")
            .join("validate.scheduler.default.json");
        assert!(expected.exists(), "sidecar should exist at {expected:?}");
    }

    #[test]
    fn scheduler_state_modes_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut random = sample_scheduler_state("myFunc");
        random.iterations_consumed = 10;
        random.mode = Some("random".to_string());

        let mut concolic = sample_scheduler_state("myFunc");
        concolic.iterations_consumed = 50;
        concolic.mode = Some("concolic".to_string());

        cache.store(&random, "random").unwrap();
        cache.store(&concolic, "concolic").unwrap();

        let loaded_random = cache.load("myFunc", "random").unwrap().unwrap();
        let loaded_concolic = cache.load("myFunc", "concolic").unwrap().unwrap();
        assert_eq!(loaded_random.iterations_consumed, 10);
        assert_eq!(loaded_concolic.iterations_consumed, 50);

        // A third mode that was never written is a cache miss, not cross-talk.
        assert_eq!(cache.load("myFunc", "other").unwrap(), None);
    }

    #[test]
    fn scheduler_state_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("myFunc");
        state.iterations_consumed = 10;
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        state.iterations_consumed = 100;
        state.batches_completed = 10;
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let loaded = cache
            .load("myFunc", DEFAULT_SCHEDULER_MODE)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.iterations_consumed, 100);
        assert_eq!(loaded.batches_completed, 10);
    }

    #[test]
    fn scheduler_state_corrupt_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        // Write garbage directly to the expected path.
        let path = dir.path().join("badFunc.scheduler.default.json");
        std::fs::write(&path, b"\x00\xff not json at all").unwrap();

        assert_eq!(
            cache.load("badFunc", DEFAULT_SCHEDULER_MODE).unwrap(),
            None,
            "corrupt file must degrade to cache miss"
        );
    }

    #[test]
    fn scheduler_state_truncated_json_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("truncFunc");
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("truncFunc.scheduler.default.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        // Chop the last 10 characters, guaranteed to break the closing braces.
        let truncated = &contents[..contents.len().saturating_sub(10)];
        std::fs::write(&path, truncated).unwrap();

        assert_eq!(
            cache.load("truncFunc", DEFAULT_SCHEDULER_MODE).unwrap(),
            None
        );
    }

    #[test]
    fn scheduler_state_protocol_version_mismatch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("oldFunc");
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("oldFunc.scheduler.default.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        let tampered = contents.replace(PROTOCOL_VERSION, "0.0.0-fake");
        std::fs::write(&path, tampered).unwrap();

        assert_eq!(cache.load("oldFunc", DEFAULT_SCHEDULER_MODE).unwrap(), None);
    }

    #[test]
    fn scheduler_state_schema_version_mismatch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("schemaDrift");
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        // Rewrite with a bumped schema_version so the reader rejects it.
        let path = dir.path().join("schemaDrift.scheduler.default.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        let tampered = contents.replace(
            &format!("\"schema_version\": {SCHEDULER_SCHEMA_VERSION}"),
            &format!("\"schema_version\": {}", SCHEDULER_SCHEMA_VERSION + 1),
        );
        assert_ne!(
            contents, tampered,
            "schema_version substitution should have replaced something"
        );
        std::fs::write(&path, tampered).unwrap();

        assert_eq!(
            cache.load("schemaDrift", DEFAULT_SCHEDULER_MODE).unwrap(),
            None
        );
    }

    #[test]
    fn scheduler_state_forward_compat_unknown_field() {
        // A future schema version that adds a field must not break today's
        // reader when `schema_version` is unchanged — `#[serde(default)]` on
        // every field, combined with serde's default behavior of ignoring
        // unknown fields, absorbs additive extensions.
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let path = dir.path().join("futureFunc.scheduler.default.json");
        let payload = format!(
            r#"{{
                "protocol_version": "{PROTOCOL_VERSION}",
                "schema_version": {SCHEDULER_SCHEMA_VERSION},
                "future_envelope_field": "ignored",
                "scheduler_state": {{
                    "function_id": "futureFunc",
                    "iterations_consumed": 7,
                    "batches_completed": 2,
                    "exhausted": false,
                    "uncovered_branches": [],
                    "hypothetical_future_field": {{"nested": true}}
                }}
            }}"#
        );
        std::fs::write(&path, payload).unwrap();

        let loaded = cache
            .load("futureFunc", DEFAULT_SCHEDULER_MODE)
            .unwrap()
            .expect("forward-compat file should load, not be a cache miss");
        assert_eq!(loaded.function_id, "futureFunc");
        assert_eq!(loaded.iterations_consumed, 7);
        assert_eq!(loaded.batches_completed, 2);
        // fingerprint was omitted → default (None) applied.
        assert!(loaded.fingerprint.is_none());
    }

    #[test]
    fn scheduler_state_is_fresh_drops_stale_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("edited");
        state.fingerprint = Some("before".to_string());
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("edited.scheduler.default.json");
        assert!(path.exists());

        assert!(
            !cache
                .is_fresh("edited", DEFAULT_SCHEDULER_MODE, "after")
                .unwrap()
        );
        assert!(!path.exists(), "stale entry should be unlinked");
        assert_eq!(cache.load("edited", DEFAULT_SCHEDULER_MODE).unwrap(), None);
    }

    #[test]
    fn scheduler_state_is_fresh_returns_true_when_fingerprint_matches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("fresh");
        state.fingerprint = Some("fp".to_string());
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        assert!(
            cache
                .is_fresh("fresh", DEFAULT_SCHEDULER_MODE, "fp")
                .unwrap()
        );
        let path = dir.path().join("fresh.scheduler.default.json");
        assert!(path.exists(), "fresh entry must not be deleted");
    }

    #[test]
    fn scheduler_state_is_fresh_drops_legacy_none_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("legacy");
        state.fingerprint = None;
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("legacy.scheduler.default.json");
        assert!(path.exists());

        assert!(
            !cache
                .is_fresh("legacy", DEFAULT_SCHEDULER_MODE, "any")
                .unwrap()
        );
        assert!(!path.exists());
    }

    #[test]
    fn scheduler_state_clear_function_removes_only_target() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let a = sample_scheduler_state("funcA");
        let b = sample_scheduler_state("funcB");
        cache.store(&a, DEFAULT_SCHEDULER_MODE).unwrap();
        cache.store(&b, DEFAULT_SCHEDULER_MODE).unwrap();

        cache
            .clear_function("funcA", DEFAULT_SCHEDULER_MODE)
            .unwrap();
        assert_eq!(cache.load("funcA", DEFAULT_SCHEDULER_MODE).unwrap(), None);
        assert!(
            cache
                .load("funcB", DEFAULT_SCHEDULER_MODE)
                .unwrap()
                .is_some()
        );

        // Clearing a missing entry is not an error.
        cache
            .clear_function("never-stored", DEFAULT_SCHEDULER_MODE)
            .unwrap();
    }

    #[test]
    fn scheduler_state_load_if_fresh_returns_state_when_fingerprint_matches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("matchFunc");
        state.fingerprint = Some("fp-current".to_string());
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let loaded = cache
            .load_if_fresh("matchFunc", DEFAULT_SCHEDULER_MODE, "fp-current")
            .unwrap();
        assert_eq!(loaded, Some(state));

        let path = dir.path().join("matchFunc.scheduler.default.json");
        assert!(path.exists(), "fresh entry must not be unlinked");
    }

    #[test]
    fn scheduler_state_load_if_fresh_returns_none_and_unlinks_when_stale() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("staleFunc");
        state.fingerprint = Some("fp-old".to_string());
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("staleFunc.scheduler.default.json");
        assert!(path.exists());

        let loaded = cache
            .load_if_fresh("staleFunc", DEFAULT_SCHEDULER_MODE, "fp-new")
            .unwrap();
        assert_eq!(loaded, None);
        assert!(!path.exists(), "stale entry must be unlinked");
        assert_eq!(
            cache.load("staleFunc", DEFAULT_SCHEDULER_MODE).unwrap(),
            None
        );
    }

    #[test]
    fn scheduler_state_load_if_fresh_returns_none_for_legacy_no_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("legacyFunc");
        state.fingerprint = None;
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let path = dir.path().join("legacyFunc.scheduler.default.json");
        assert!(path.exists());

        let loaded = cache
            .load_if_fresh("legacyFunc", DEFAULT_SCHEDULER_MODE, "any-fp")
            .unwrap();
        assert_eq!(loaded, None);
        assert!(!path.exists(), "legacy entry must be treated as stale");
    }

    #[test]
    fn scheduler_state_load_if_fresh_returns_none_on_cache_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let loaded = cache
            .load_if_fresh("never-stored", DEFAULT_SCHEDULER_MODE, "fp")
            .unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn scheduler_state_load_if_fresh_idempotent_on_stale() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut state = sample_scheduler_state("idemFunc");
        state.fingerprint = Some("fp-old".to_string());
        cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        let first = cache
            .load_if_fresh("idemFunc", DEFAULT_SCHEDULER_MODE, "fp-new")
            .unwrap();
        let second = cache
            .load_if_fresh("idemFunc", DEFAULT_SCHEDULER_MODE, "fp-new")
            .unwrap();
        assert_eq!(first, None);
        assert_eq!(second, None);
    }

    #[test]
    fn scheduler_state_load_if_fresh_does_not_touch_other_functions() {
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let mut a = sample_scheduler_state("funcA");
        a.fingerprint = Some("fp-a".to_string());
        let mut b = sample_scheduler_state("funcB");
        b.fingerprint = Some("fp-b".to_string());
        cache.store(&a, DEFAULT_SCHEDULER_MODE).unwrap();
        cache.store(&b, DEFAULT_SCHEDULER_MODE).unwrap();

        // Invalidate funcA with a wrong fingerprint.
        let dropped = cache
            .load_if_fresh("funcA", DEFAULT_SCHEDULER_MODE, "fp-changed")
            .unwrap();
        assert_eq!(dropped, None);

        // funcB is still recoverable under its own correct fingerprint.
        let kept = cache
            .load_if_fresh("funcB", DEFAULT_SCHEDULER_MODE, "fp-b")
            .unwrap();
        assert_eq!(kept, Some(b));
    }

    #[test]
    fn scheduler_state_sidecar_does_not_leak_into_behavior_map_cache() {
        // str-bo4z.5: SchedulerStateCache sidecars must be ignored by
        // BehaviorMapCache::{load_all, load_all_for_file}. A regression here
        // would have the behavior map iterator try to deserialize scheduler
        // JSON as a BehaviorMap and silently drop real entries on parse error.
        let dir = tempfile::tempdir().unwrap();
        let bm_cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();
        let sched_cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let map = sample_map("src/math.ts:add");
        bm_cache.store(&map).unwrap();

        let state = sample_scheduler_state("src/math.ts:add");
        sched_cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

        // Both sidecars on disk.
        let bm_file = dir.path().join("src").join("math.ts").join("add.json");
        let sched_file = dir
            .path()
            .join("src")
            .join("math.ts")
            .join("add.scheduler.default.json");
        assert!(bm_file.exists());
        assert!(sched_file.exists());

        // load_all returns only the behavior map.
        let all = bm_cache.load_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].function_id, "src/math.ts:add");

        // load_all_for_file returns only the behavior map.
        let for_file = bm_cache.load_all_for_file("src/math.ts").unwrap();
        assert_eq!(for_file.len(), 1);
        assert_eq!(for_file[0].function_id, "src/math.ts:add");
    }

    #[test]
    fn scheduler_state_mode_is_sanitized() {
        // Path-unsafe characters in the mode tag must not escape the cache
        // directory — sanitize_component maps them to `_`.
        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

        let state = sample_scheduler_state("safeFunc");
        cache.store(&state, "weird*mode?").unwrap();

        let expected = dir.path().join("safeFunc.scheduler.weird_mode_.json");
        assert!(expected.exists(), "mode should be sanitized");
        let loaded = cache.load("safeFunc", "weird*mode?").unwrap();
        assert_eq!(loaded, Some(state));
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

        /// Seed inputs survive store → load roundtrip.
        #[test]
        fn cached_seed_inputs_survive_roundtrip(map in arb_behavior_map()) {
            let dir = tempfile::tempdir().unwrap();
            let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

            let original_seeds = map.extract_seed_inputs();
            cache.store(&map).unwrap();
            let loaded = cache.load(&map.function_id).unwrap().unwrap();
            let loaded_seeds = loaded.extract_seed_inputs();

            prop_assert_eq!(original_seeds, loaded_seeds);
        }

        /// str-bo4z.1 invariant: store_with_fingerprint always stamps the map,
        /// and a subsequent is_fresh check under a mismatched fingerprint
        /// invalidates the on-disk entry without touching unrelated siblings.
        #[test]
        fn body_change_drops_only_affected_entry(
            map in arb_behavior_map(),
            fp_a in "[a-f0-9]{64}",
            fp_b in "[a-f0-9]{64}",
        ) {
            prop_assume!(fp_a != fp_b);
            let dir = tempfile::tempdir().unwrap();
            let cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();

            // A sibling map with a distinct function_id and fingerprint.
            let mut sibling = map.clone();
            sibling.function_id = format!("{}__sibling", map.function_id);
            let sibling_fp = "sibling-fingerprint";

            cache.store_with_fingerprint(&map, &fp_a).unwrap();
            cache.store_with_fingerprint(&sibling, sibling_fp).unwrap();

            // is_fresh with a new fingerprint reports stale and drops the entry.
            prop_assert!(!cache.is_fresh(&map.function_id, &fp_b).unwrap());
            prop_assert_eq!(cache.load(&map.function_id).unwrap(), None);

            // The sibling is untouched.
            prop_assert!(cache.is_fresh(&sibling.function_id, sibling_fp).unwrap());
            let reloaded = cache.load(&sibling.function_id).unwrap();
            prop_assert!(reloaded.is_some());
            let reloaded_map = reloaded.unwrap();
            prop_assert_eq!(reloaded_map.fingerprint.as_deref(), Some(sibling_fp));
        }
    }

    // --- SchedulerStateCache proptests (str-bo4z.5) ---

    fn arb_scheduler_state() -> impl Strategy<Value = SchedulerState> {
        (
            // Restrict to realistic function_id shapes:
            // `<name>` or `<name>:<name>`, with names made of letters,
            // digits, and underscores. This avoids path-unsafe values like
            // "/", "..", "./x", or leading colons which would be rejected
            // by `cache_base_path` / filesystem rules.
            "[a-zA-Z][a-zA-Z0-9_]{0,20}(:[a-zA-Z][a-zA-Z0-9_]{0,20})?",
            proptest::option::of("[a-f0-9]{16,32}"),
            any::<u32>(),
            any::<u32>(),
            any::<bool>(),
            proptest::option::of("[a-z_]{1,16}"),
            proptest::collection::vec("[a-zA-Z0-9_.]{1,20}", 0..8),
        )
            .prop_map(
                |(
                    function_id,
                    fingerprint,
                    iterations_consumed,
                    batches_completed,
                    exhausted,
                    mode,
                    uncovered_branches,
                )| {
                    SchedulerState {
                        function_id,
                        fingerprint,
                        iterations_consumed,
                        batches_completed,
                        exhausted,
                        mode,
                        uncovered_branches,
                    }
                },
            )
    }

    proptest! {
        /// Invariant 1: SchedulerState survives store → load roundtrip with
        /// full semantic equality.
        #[test]
        fn scheduler_state_roundtrip(state in arb_scheduler_state()) {
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let loaded = cache
                .load(&state.function_id, DEFAULT_SCHEDULER_MODE)
                .unwrap();
            prop_assert_eq!(loaded, Some(state));
        }

        /// Invariant 2 (reconstruction safety): arbitrary bytes written to a
        /// scheduler-state file never panic on load and always degrade to a
        /// cache miss. Advisory-and-reconstructible: callers can rebuild.
        #[test]
        fn scheduler_state_corrupt_bytes_never_panic(
            bytes in proptest::collection::vec(any::<u8>(), 0..512),
        ) {
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            let path = dir.path().join("corruptFn.scheduler.default.json");
            std::fs::write(&path, &bytes).unwrap();

            // Must not panic. Must return Ok(Some(..)) only on the vanishingly
            // rare case that random bytes happen to form a valid envelope;
            // otherwise Ok(None).
            let loaded = cache
                .load("corruptFn", DEFAULT_SCHEDULER_MODE)
                .expect("load must never return Err on corrupt data");
            if loaded.is_some() {
                // If it did parse, re-storing and reloading must still round-trip.
                let round = cache
                    .load("corruptFn", DEFAULT_SCHEDULER_MODE)
                    .unwrap();
                prop_assert_eq!(loaded, round);
            }
        }

        /// Invariant 3 (sidecar isolation): a SchedulerState stored for any
        /// function must not appear as, or displace, a BehaviorMap entry in
        /// load_all / load_all_for_file. Regression guard for the filter
        /// extension in collect_all_maps / load_all_for_file.
        #[test]
        fn scheduler_sidecar_isolated_from_behavior_map_cache(
            state in arb_scheduler_state(),
            map in arb_behavior_map(),
        ) {
            let dir = tempfile::tempdir().unwrap();
            let bm_cache = BehaviorMapCache::new(dir.path().to_path_buf()).unwrap();
            let sched_cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            bm_cache.store(&map).unwrap();
            sched_cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

            // load_all sees the behavior map, never the scheduler sidecar.
            let all = bm_cache.load_all().unwrap();
            prop_assert!(
                all.iter().any(|m| m.function_id == map.function_id),
                "behavior map should appear in load_all"
            );
            prop_assert!(
                all.iter().all(|m| !m.function_id.is_empty()),
                "no spurious entries from scheduler sidecar"
            );
            // Count: exactly the behavior maps we stored (scheduler sidecar
            // never increments the count, even if state.function_id happens to
            // equal map.function_id).
            let distinct_bm_ids: std::collections::HashSet<_> =
                all.iter().map(|m| m.function_id.clone()).collect();
            prop_assert_eq!(distinct_bm_ids.len(), 1);
            prop_assert!(distinct_bm_ids.contains(&map.function_id));
        }

        /// Invariant 4 (stale invalidation): storing with fingerprint X, then
        /// checking freshness under Y != X, must return false AND unlink the
        /// on-disk entry so the next load is a clean cache miss.
        #[test]
        fn scheduler_state_body_change_drops_entry(
            mut state in arb_scheduler_state(),
            fp_a in "[a-f0-9]{32}",
            fp_b in "[a-f0-9]{32}",
        ) {
            prop_assume!(fp_a != fp_b);
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            state.fingerprint = Some(fp_a.clone());
            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();

            prop_assert!(
                !cache
                    .is_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp_b)
                    .unwrap()
            );
            prop_assert_eq!(
                cache.load(&state.function_id, DEFAULT_SCHEDULER_MODE).unwrap(),
                None
            );
        }

        /// Invariant 5 (per-mode independence): storing state for (fn, "a")
        /// must not affect load(fn, "b") or load(fn, DEFAULT_SCHEDULER_MODE).
        #[test]
        fn scheduler_state_modes_independent(
            mut a in arb_scheduler_state(),
            mut b in arb_scheduler_state(),
        ) {
            // Force a shared function_id so we can stress cross-mode isolation.
            b.function_id = a.function_id.clone();
            a.mode = Some("mode_a".to_string());
            b.mode = Some("mode_b".to_string());

            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&a, "mode_a").unwrap();
            cache.store(&b, "mode_b").unwrap();

            let loaded_a = cache.load(&a.function_id, "mode_a").unwrap();
            let loaded_b = cache.load(&b.function_id, "mode_b").unwrap();
            prop_assert_eq!(loaded_a, Some(a.clone()));
            prop_assert_eq!(loaded_b, Some(b));

            // A mode that was never written is a clean miss.
            prop_assert_eq!(
                cache.load(&a.function_id, DEFAULT_SCHEDULER_MODE).unwrap(),
                None
            );
        }

        /// Invariant 6 (forward-compat): a stored envelope survives an extra
        /// unknown top-level field and an extra unknown payload field without
        /// degrading to cache miss. Future schemas that add fields must
        /// remain readable by today's binary at the same schema_version.
        #[test]
        fn scheduler_state_forward_compat_additive_fields(
            state in arb_scheduler_state(),
            extra_key in "[a-z_]{1,12}",
        ) {
            prop_assume!(extra_key != "protocol_version" && extra_key != "schema_version"
                && extra_key != "scheduler_state" && extra_key != "function_id"
                && extra_key != "fingerprint" && extra_key != "iterations_consumed"
                && extra_key != "batches_completed" && extra_key != "exhausted"
                && extra_key != "mode" && extra_key != "uncovered_branches");

            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            // Build a canonical envelope, inject two unknown fields, write it.
            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let path = cache.path_for(&state.function_id, DEFAULT_SCHEDULER_MODE);
            let contents = std::fs::read_to_string(&path).unwrap();
            let mut value: serde_json::Value = serde_json::from_str(&contents).unwrap();
            value[&extra_key] = serde_json::json!("future top-level");
            value["scheduler_state"][&extra_key] = serde_json::json!({"nested": 42});
            std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

            let loaded = cache
                .load(&state.function_id, DEFAULT_SCHEDULER_MODE)
                .unwrap();
            prop_assert_eq!(loaded, Some(state));
        }

        /// str-bo4z.2 invariant 1: storing with fingerprint X then loading
        /// via load_if_fresh under the same X must return the original state
        /// unchanged. "Unchanged functions keep their state."
        #[test]
        fn scheduler_state_load_if_fresh_returns_state_when_fresh(
            mut state in arb_scheduler_state(),
            fp in "[a-f0-9]{32}",
        ) {
            state.fingerprint = Some(fp.clone());
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let loaded = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp)
                .unwrap();
            prop_assert_eq!(loaded, Some(state.clone()));

            // Fresh path must not unlink the file.
            let path = cache.path_for(&state.function_id, DEFAULT_SCHEDULER_MODE);
            prop_assert!(path.exists(), "fresh entry must remain on disk");
        }

        /// str-bo4z.2 invariant 2: storing with fingerprint X and loading via
        /// load_if_fresh under any Y != X must return None AND remove the
        /// on-disk entry. "After body change, scheduler state for that
        /// function is empty."
        #[test]
        fn scheduler_state_load_if_fresh_invalidates_on_body_change(
            mut state in arb_scheduler_state(),
            fp_old in "[a-f0-9]{32}",
            fp_new in "[a-f0-9]{32}",
        ) {
            prop_assume!(fp_old != fp_new);
            state.fingerprint = Some(fp_old);
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let loaded = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp_new)
                .unwrap();
            prop_assert_eq!(loaded, None);

            // The on-disk file must be gone, and a follow-up plain load is a
            // clean cache miss.
            let path = cache.path_for(&state.function_id, DEFAULT_SCHEDULER_MODE);
            prop_assert!(!path.exists(), "stale entry must be unlinked");
            prop_assert_eq!(
                cache.load(&state.function_id, DEFAULT_SCHEDULER_MODE).unwrap(),
                None
            );
        }

        /// str-bo4z.2 invariant 3: load_if_fresh is idempotent on the stale
        /// path. Calling it twice with a mismatched fingerprint is
        /// indistinguishable from calling it once: both calls return None and
        /// neither panics or errors. The first call removes the file; the
        /// second sees a cache miss and short-circuits.
        #[test]
        fn scheduler_state_load_if_fresh_idempotent_on_stale(
            mut state in arb_scheduler_state(),
            fp_old in "[a-f0-9]{32}",
            fp_new in "[a-f0-9]{32}",
        ) {
            prop_assume!(fp_old != fp_new);
            state.fingerprint = Some(fp_old);
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let first = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp_new)
                .unwrap();
            let second = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp_new)
                .unwrap();
            prop_assert_eq!(&first, &None);
            prop_assert_eq!(&second, &None);
        }

        /// str-bo4z.2 invariant 4: load_if_fresh is idempotent on the fresh
        /// path. Two consecutive calls with the matching fingerprint must
        /// return identical state and leave the file on disk both times.
        #[test]
        fn scheduler_state_load_if_fresh_idempotent_on_fresh(
            mut state in arb_scheduler_state(),
            fp in "[a-f0-9]{32}",
        ) {
            state.fingerprint = Some(fp.clone());
            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&state, DEFAULT_SCHEDULER_MODE).unwrap();
            let first = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp)
                .unwrap();
            let second = cache
                .load_if_fresh(&state.function_id, DEFAULT_SCHEDULER_MODE, &fp)
                .unwrap();
            prop_assert_eq!(&first, &Some(state.clone()));
            prop_assert_eq!(&second, &Some(state));
        }

        /// str-bo4z.2 invariant 5: invalidating one function via
        /// load_if_fresh with a wrong fingerprint must not touch any
        /// unrelated function's persisted state. Cross-function blast radius
        /// is exactly zero.
        #[test]
        fn scheduler_state_load_if_fresh_does_not_touch_other_functions(
            mut a in arb_scheduler_state(),
            mut b in arb_scheduler_state(),
            fp_a in "[a-f0-9]{32}",
            fp_b in "[a-f0-9]{32}",
            fp_changed in "[a-f0-9]{32}",
        ) {
            // Force distinct function ids so the two records own separate files.
            prop_assume!(a.function_id != b.function_id);
            prop_assume!(fp_a != fp_changed);
            a.fingerprint = Some(fp_a);
            b.fingerprint = Some(fp_b.clone());

            let dir = tempfile::tempdir().unwrap();
            let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();

            cache.store(&a, DEFAULT_SCHEDULER_MODE).unwrap();
            cache.store(&b, DEFAULT_SCHEDULER_MODE).unwrap();

            // Invalidate `a` with a wrong fingerprint.
            let dropped = cache
                .load_if_fresh(&a.function_id, DEFAULT_SCHEDULER_MODE, &fp_changed)
                .unwrap();
            prop_assert_eq!(dropped, None);

            // `b` survives untouched.
            let kept = cache
                .load_if_fresh(&b.function_id, DEFAULT_SCHEDULER_MODE, &fp_b)
                .unwrap();
            prop_assert_eq!(kept, Some(b));
        }
    }
}
