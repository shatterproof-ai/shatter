//! Run-start source snapshot and end-of-run validation (str-jeen.3).
//!
//! Long Shatter runs can become stale while still exiting normally if the
//! source set changes mid-run: files renamed, deleted, added, or modified
//! after the test order is fixed but before the run finalizes. The report
//! then references a source set that no longer exists, while looking
//! "complete".
//!
//! At the start of a run we capture a [`RunManifest`] — repo root, cwd,
//! git commit, scope/config hash, and a per-source-file snapshot (size +
//! mtime + content hash). At the end of the run we re-snapshot the same
//! paths and compute a [`ManifestDiff`]. If anything was added, removed,
//! or changed, the run is reported with status [`crate::scan_orchestrator::
//! ScanRunStatus::StaleSourceSet`] and the diff is attached to the summary.
//!
//! The manifest is also written to disk at `<scan_root>/manifest.json`
//! so external tooling can audit which source set produced a given report.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// On-disk manifest schema version. Bump on incompatible changes.
pub const RUN_MANIFEST_VERSION: u32 = 1;

/// Manifest filename written under `<scan_root>/`.
pub const RUN_MANIFEST_FILENAME: &str = "manifest.json";

/// Per-source-file snapshot captured at run start (and re-captured at end).
///
/// Files that fail to stat or read at capture time are recorded with
/// `size = 0`, `mtime_ns = None`, `content_hash = None` — that absence
/// is itself a signal: the path was already missing or unreadable when
/// the manifest was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFileSnapshot {
    /// Path as supplied by the scope (typically relative to project root,
    /// but absolute is permitted; manifest stores it verbatim).
    pub path: String,
    /// File size in bytes. Zero if the file could not be stat'd.
    pub size: u64,
    /// Last-modified time as nanoseconds since UNIX epoch. `None` if the
    /// platform cannot supply mtime or the file was missing at capture.
    pub mtime_ns: Option<u128>,
    /// SHA-256 of file contents, lowercase hex. `None` if the file could
    /// not be read.
    pub content_hash: Option<String>,
    /// Whole-file physical line count at capture time, derived from the
    /// same read used for `content_hash`. `None` when the file could not
    /// be read. Counted as `lines().count()` so an empty file is `0`, a
    /// single line without a trailing newline is `1`, and a final
    /// newline does not add a phantom empty line.
    ///
    /// Feeds `selected_source_lines` (str-jeen.17) — the run-JSON
    /// denominator that must reflect the manifest source set, not the
    /// per-discovered-function line tally.
    #[serde(default)]
    pub line_count: Option<u32>,
}

/// Run-start manifest. Captured once before the first function explores,
/// rewritten exactly once at scan finalization (with the diff attached).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunManifest {
    /// Schema version.
    pub version: u32,
    /// Scan ID (matches `ScanSummary::scan_id`).
    pub scan_id: String,
    /// Project root the scan anchored its source paths against. Relative
    /// `source_files` paths are resolved against this directory at both
    /// capture and diff time. `None` when the scan was launched without a
    /// detected project root (rare — typically only synthetic test runs).
    pub project_root: Option<String>,
    /// Detected git repository root (as reported by `git rev-parse
    /// --show-toplevel`). May differ from `project_root` when the scan
    /// is run from a subdirectory of a larger repo. `None` when the scan
    /// is not inside a git repo.
    pub repo_root: Option<String>,
    /// Process working directory at scan start.
    pub cwd: String,
    /// Short git commit hash at scan start, or `None` if git is
    /// unavailable / not a repo.
    pub git_commit: Option<String>,
    /// True if the working tree had uncommitted changes at scan start.
    /// `None` if git status could not be determined.
    pub git_dirty: Option<bool>,
    /// Stable hash of the scan configuration (parallelism, timeouts,
    /// iteration budget, etc.). Lets external tooling detect that two
    /// runs of the same source set used different config.
    pub scope_hash: String,
    /// Source-file snapshots, sorted by `path` for deterministic
    /// serialization.
    pub source_files: Vec<SourceFileSnapshot>,
    /// Capture timestamp as nanoseconds since UNIX epoch. Best-effort —
    /// not used for correctness, only for human auditing.
    pub captured_at_ns: u128,
}

/// End-of-run diff between the captured manifest and a fresh snapshot
/// of the source paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestDiff {
    /// Paths present at end-of-run that were not in the original manifest.
    pub added: Vec<String>,
    /// Paths in the original manifest that no longer exist or are
    /// unreadable at end-of-run.
    pub removed: Vec<String>,
    /// Paths whose size, mtime, or content hash changed between capture
    /// and finalization.
    pub changed: Vec<String>,
}

impl RunManifest {
    /// Number of files captured in this manifest. Used as the
    /// `selected_source_files` denominator in the run JSON
    /// (str-jeen.17) — independent of how many functions were later
    /// discovered, attempted, or completed.
    pub fn selected_source_files(&self) -> usize {
        self.source_files.len()
    }

    /// Sum of [`SourceFileSnapshot::line_count`] across the manifest's
    /// source files. Files whose `line_count` is `None` (unreadable or
    /// from a legacy manifest written before str-jeen.17) contribute
    /// zero. Used as the `selected_source_lines` denominator in the
    /// run JSON.
    pub fn selected_source_lines(&self) -> u64 {
        self.source_files
            .iter()
            .map(|s| s.line_count.unwrap_or(0) as u64)
            .sum()
    }
}

impl ManifestDiff {
    /// True when the source set or contents changed during the run.
    pub fn is_stale(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }
}

/// Snapshot a single file. Best-effort — never panics or returns Err;
/// missing/unreadable files become a "tombstone" snapshot whose absence
/// of mtime and hash signals the failure.
pub fn snapshot_file(path: &Path) -> SourceFileSnapshot {
    let path_str = path.to_string_lossy().to_string();
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => {
            return SourceFileSnapshot {
                path: path_str,
                size: 0,
                mtime_ns: None,
                content_hash: None,
                line_count: None,
            };
        }
    };
    let size = metadata.len();
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos());
    let (content_hash, line_count) = match fs::read(path) {
        Ok(bytes) => {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let hash = format!("{:x}", hasher.finalize());
            // Count physical lines via std lines() so the count matches
            // what a downstream "wc -l on text files" intuition would
            // report: empty file = 0, "a\nb\n" = 2, "a\nb" = 2. Lossy
            // UTF-8 conversion is fine — any binary noise still yields a
            // well-defined line count and the manifest does not
            // round-trip the lines themselves.
            let text = String::from_utf8_lossy(&bytes);
            let count = text.lines().count();
            let count_u32 = u32::try_from(count).unwrap_or(u32::MAX);
            (Some(hash), Some(count_u32))
        }
        Err(_) => (None, None),
    };
    SourceFileSnapshot {
        path: path_str,
        size,
        mtime_ns,
        content_hash,
        line_count,
    }
}

/// Capture a [`RunManifest`] for a set of source paths.
///
/// `source_paths` is the canonical set of files the scan will touch — the
/// values of `ScanConfig::file_map`, deduplicated. Paths are snapshotted
/// in sorted order so the manifest is deterministic.
pub fn capture(
    scan_id: &str,
    scope_hash: &str,
    source_paths: &[String],
    project_root: Option<&Path>,
) -> RunManifest {
    let mut sorted_paths: Vec<String> = source_paths.to_vec();
    sorted_paths.sort();
    sorted_paths.dedup();

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| String::new());

    let git_root = project_root.and_then(crate::scm::repo_root_or_none);
    let git_commit = project_root.and_then(|root| crate::scm::head_commit(root).ok());
    let git_dirty = project_root.map(|root| crate::scm::working_tree_dirty(root).unwrap_or(false));

    let source_files = sorted_paths
        .iter()
        .map(|p| {
            let resolved = resolve_path(p, project_root);
            let mut snap = snapshot_file(&resolved);
            // Store the original (possibly relative) path verbatim so the
            // manifest doesn't leak absolute paths from the build host.
            snap.path = p.clone();
            snap
        })
        .collect();

    let captured_at_ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    RunManifest {
        version: RUN_MANIFEST_VERSION,
        scan_id: scan_id.to_string(),
        project_root: project_root.map(|p| p.to_string_lossy().to_string()),
        repo_root: git_root.map(|p| p.to_string_lossy().to_string()),
        cwd,
        git_commit,
        git_dirty,
        scope_hash: scope_hash.to_string(),
        source_files,
        captured_at_ns,
    }
}

/// Re-snapshot the original manifest's source set plus any new paths
/// supplied by the caller, then diff against the captured manifest.
///
/// `current_paths` is the set the scan reports as having been explored.
/// Any path in `current_paths` but absent from `manifest.source_files`
/// is recorded as `added`. Any path in the manifest whose end-of-run
/// snapshot differs (size, mtime, or content hash) is `changed`. Any
/// path in the manifest that no longer exists is `removed`.
pub fn diff_against(manifest: &RunManifest, current_paths: &[String]) -> ManifestDiff {
    let original: BTreeMap<&str, &SourceFileSnapshot> = manifest
        .source_files
        .iter()
        .map(|s| (s.path.as_str(), s))
        .collect();

    let mut current_set: BTreeMap<String, ()> = BTreeMap::new();
    for p in current_paths {
        current_set.insert(p.clone(), ());
    }

    let project_root = manifest
        .project_root
        .as_deref()
        .or(manifest.repo_root.as_deref())
        .map(Path::new);

    let mut diff = ManifestDiff::default();

    // Re-snapshot original paths to detect removed/changed.
    for (path, original_snap) in &original {
        let resolved = resolve_path(path, project_root);
        let exists = resolved.exists();
        if !exists {
            diff.removed.push((*path).to_string());
            continue;
        }
        let mut fresh = snapshot_file(&resolved);
        fresh.path = (*path).to_string();
        if &&fresh != original_snap {
            diff.changed.push((*path).to_string());
        }
    }

    // Detect added paths: in current_paths but not in the original manifest.
    for path in current_set.keys() {
        if !original.contains_key(path.as_str()) {
            diff.added.push(path.clone());
        }
    }

    diff.added.sort();
    diff.removed.sort();
    diff.changed.sort();
    diff
}

/// Resolve a manifest path relative to `project_root` if it is relative,
/// otherwise return it as-is.
fn resolve_path(path: &str, project_root: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(root) = project_root {
        root.join(p)
    } else {
        p.to_path_buf()
    }
}

/// Write the manifest to `<scan_root>/manifest.json` using atomic rename.
pub fn write_manifest(scan_root: &Path, manifest: &RunManifest) {
    let path = scan_root.join(RUN_MANIFEST_FILENAME);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        log::warn!("failed to create run manifest dir: {e}");
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(manifest) else {
        log::warn!("failed to serialize run manifest");
        return;
    };
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = fs::write(&tmp_path, &json) {
        log::warn!("failed to write run manifest temp file: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        log::warn!("failed to finalize run manifest: {e}");
    }
}

/// Read a manifest from `<scan_root>/manifest.json`.
pub fn read_manifest(scan_root: &Path) -> Option<RunManifest> {
    let path = scan_root.join(RUN_MANIFEST_FILENAME);
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = File::create(&path).expect("create");
        f.write_all(contents).expect("write");
        path
    }

    #[test]
    fn snapshot_records_size_and_hash() {
        let tmp = TempDir::new().unwrap();
        let path = write_file(tmp.path(), "a.txt", b"hello");
        let snap = snapshot_file(&path);
        assert_eq!(snap.size, 5);
        assert!(snap.mtime_ns.is_some());
        // SHA-256 of "hello"
        assert_eq!(
            snap.content_hash.as_deref(),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn snapshot_missing_file_is_tombstone() {
        let tmp = TempDir::new().unwrap();
        let snap = snapshot_file(&tmp.path().join("absent"));
        assert_eq!(snap.size, 0);
        assert!(snap.mtime_ns.is_none());
        assert!(snap.content_hash.is_none());
        assert!(snap.line_count.is_none());
    }

    #[test]
    fn snapshot_records_line_count() {
        let tmp = TempDir::new().unwrap();
        // Empty file -> 0 lines, no phantom line for missing trailing newline.
        let empty = write_file(tmp.path(), "empty.rs", b"");
        assert_eq!(snapshot_file(&empty).line_count, Some(0));
        // Single line without trailing newline -> 1.
        let one_no_nl = write_file(tmp.path(), "one.rs", b"fn a() {}");
        assert_eq!(snapshot_file(&one_no_nl).line_count, Some(1));
        // Two lines with trailing newline -> 2 (no phantom empty line).
        let two = write_file(tmp.path(), "two.rs", b"a\nb\n");
        assert_eq!(snapshot_file(&two).line_count, Some(2));
        // Two lines without trailing newline -> 2.
        let two_no_nl = write_file(tmp.path(), "two_no_nl.rs", b"a\nb");
        assert_eq!(snapshot_file(&two_no_nl).line_count, Some(2));
    }

    #[test]
    fn manifest_aggregates_selected_source_totals() {
        let tmp = TempDir::new().unwrap();
        // Three files with known line counts: 3, 5, and 0.
        write_file(tmp.path(), "a.rs", b"l1\nl2\nl3\n");
        write_file(tmp.path(), "b.rs", b"l1\nl2\nl3\nl4\nl5");
        write_file(tmp.path(), "c.rs", b"");
        let paths = vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
        ];
        let m = capture("scan-1", "cfg-h", &paths, Some(tmp.path()));
        // Whole-source totals come from the manifest snapshot, not from
        // per-discovered-function spans (str-jeen.17).
        assert_eq!(m.selected_source_files(), 3);
        assert_eq!(m.selected_source_lines(), 8);
    }

    #[test]
    fn capture_then_diff_unchanged_is_clean() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", b"fn a() {}");
        write_file(tmp.path(), "b.rs", b"fn b() {}");
        let paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let m = capture("scan-1", "cfg-h", &paths, Some(tmp.path()));
        let diff = diff_against(&m, &paths);
        assert!(!diff.is_stale(), "unchanged source set must not be stale");
    }

    #[test]
    fn diff_detects_added_removed_changed() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", b"fn a() {}");
        write_file(tmp.path(), "b.rs", b"fn b() {}");
        let original_paths = vec!["a.rs".to_string(), "b.rs".to_string()];
        let m = capture("scan-1", "cfg-h", &original_paths, Some(tmp.path()));

        // Modify a.rs (changed), delete b.rs (removed), add c.rs (added).
        write_file(tmp.path(), "a.rs", b"fn a() { 1 }");
        fs::remove_file(tmp.path().join("b.rs")).unwrap();
        write_file(tmp.path(), "c.rs", b"fn c() {}");

        let current_paths = vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
        ];
        let diff = diff_against(&m, &current_paths);
        assert!(diff.is_stale());
        assert_eq!(diff.changed, vec!["a.rs".to_string()]);
        assert_eq!(diff.removed, vec!["b.rs".to_string()]);
        assert_eq!(diff.added, vec!["c.rs".to_string()]);
    }

    #[test]
    fn diff_detects_only_mtime_change() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", b"same contents");
        let paths = vec!["a.rs".to_string()];
        let m = capture("scan-1", "cfg-h", &paths, Some(tmp.path()));

        // Touch the mtime by rewriting identical contents after a delay.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_file(tmp.path(), "a.rs", b"different");
        let diff = diff_against(&m, &paths);
        assert_eq!(diff.changed, vec!["a.rs".to_string()]);
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", b"x");
        let paths = vec!["a.rs".to_string()];
        let m = capture("scan-rt", "cfg", &paths, Some(tmp.path()));
        write_manifest(tmp.path(), &m);
        let read = read_manifest(tmp.path()).expect("manifest read");
        assert_eq!(m, read);
    }

    #[test]
    fn capture_sorts_and_dedups_paths() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", b"a");
        write_file(tmp.path(), "b.rs", b"b");
        let unsorted_with_dupe = vec![
            "b.rs".to_string(),
            "a.rs".to_string(),
            "b.rs".to_string(),
        ];
        let m = capture("s", "h", &unsorted_with_dupe, Some(tmp.path()));
        let paths: Vec<&str> = m.source_files.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "b.rs"]);
    }
}
