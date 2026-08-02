//! SCM (source control) provider for querying changed files.
//!
//! Shells out to `git` with zero external dependencies. Used by `--changed`
//! and `--since` CLI flags to restrict scan scope to modified files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
use crate::discovery::Language;

/// Errors from SCM operations.
#[derive(Debug, thiserror::Error)]
pub enum ScmError {
    #[error("not a git repository (or any parent): {path}")]
    NotARepo { path: PathBuf },

    #[error("git executable not found")]
    GitNotFound,

    #[error("git command failed (exit {code}): {stderr}")]
    GitFailed { code: i32, stderr: String },

    #[error("I/O error running git: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait for querying changed files from source control.
pub trait ScmProvider {
    /// Files with uncommitted changes (staged + unstaged vs HEAD) under
    /// `root`. If `include_untracked` is true, also includes untracked files
    /// (excluding gitignored ones). Results are scoped to `root`: changes
    /// elsewhere in the repository are not reported, whether tracked or not
    /// (str-a2wkn).
    fn changed_files(&self, root: &Path, include_untracked: bool)
        -> Result<Vec<PathBuf>, ScmError>;

    /// Files changed between `base_ref` and HEAD (merge-base diff).
    fn diff_files(&self, root: &Path, base_ref: &str) -> Result<Vec<PathBuf>, ScmError>;

    /// Files changed between `since_ref` and `until_ref` (merge-base diff).
    fn diff_files_range(
        &self,
        root: &Path,
        since_ref: &str,
        until_ref: &str,
    ) -> Result<Vec<PathBuf>, ScmError>;

    /// Unified-zero diff hunks between `base_ref` and HEAD.
    fn diff_hunks(&self, root: &Path, base_ref: &str) -> Result<DiffHunkSet, ScmError>;

    /// Unified-zero diff hunks for staged changes.
    fn staged_diff_hunks(&self, root: &Path) -> Result<DiffHunkSet, ScmError>;
}

/// Git-based SCM provider. Shells out to `git` via `std::process::Command`.
#[derive(Debug)]
pub struct GitProvider;

impl ScmProvider for GitProvider {
    /// Scoping contract: **subdir-scoped**. Every returned path is under
    /// `root`; changes elsewhere in the repository are not reported.
    ///
    /// This is what callers already assume. `shatter scan --changed` feeds the
    /// result to `discovery::filter_file_list(&root, ...)`, which drops paths
    /// outside `root`; `shatter test` strips `project_root` and silently
    /// discards non-matches; the sibling `--until` path in `scan` treats a path
    /// outside `root` as a hard error. Nothing consumes repo-wide results.
    ///
    /// Enforcing it here rather than downstream keeps the two halves of this
    /// function consistent (str-a2wkn): `git diff --name-only` ignores cwd and
    /// reports repo-wide, while `git ls-files --others` only ever lists files
    /// under cwd. Without the explicit `-- .` pathspec on the diff calls, a
    /// tracked change outside the scan root came back but an untracked one did
    /// not. Paths are still printed repo-root-relative, so they are joined onto
    /// `repo_root`, not `root`.
    ///
    /// Existence contract: **only paths that exist on disk are returned**, so
    /// deletions are not reported. This is the second half of the same symmetry
    /// (str-a2wkn): `ls-files --others` structurally cannot name a file that
    /// isn't there, while `git diff` happily reports the old path of anything
    /// removed. The `-- .` pathspec makes that gap load-bearing — a pathspec
    /// suppresses git's rename pairing, so `git mv src/a.ts other/a.ts` viewed
    /// from `src` reports the vanished `src/a.ts` rather than collapsing to the
    /// new path the way an unscoped diff does. Plain deletions have the same
    /// shape and always did. Filtering on existence covers both, and matters
    /// because `discovery::filter_file_list` does not stat its input: a
    /// nonexistent `.ts` path flows straight through to a frontend as a scan
    /// target. Note `--no-renames` does *not* help here — with or without it,
    /// git reports the old path once a pathspec is in play.
    fn changed_files(
        &self,
        root: &Path,
        include_untracked: bool,
    ) -> Result<Vec<PathBuf>, ScmError> {
        let repo_root = repo_root(root)?;

        // Staged + unstaged changes vs HEAD, scoped to `root` via `-- .`.
        let output = run_git(root, &["diff", "--name-only", "HEAD", "--", "."])?;
        let mut files = parse_file_list(&output, &repo_root);

        // Also include staged-only changes (new files that are staged but not yet committed)
        let staged_output = run_git(root, &["diff", "--name-only", "--cached", "--", "."])?;
        let staged_files = parse_file_list(&staged_output, &repo_root);
        for f in staged_files {
            if !files.contains(&f) {
                files.push(f);
            }
        }

        if include_untracked {
            // --full-name: ls-files prints cwd-relative paths by default,
            // unlike `git diff --name-only` which is repo-root-relative. The
            // scan root may be a repo subdirectory (str-g9i4v).
            let untracked_output = run_git(
                root,
                &["ls-files", "--others", "--exclude-standard", "--full-name"],
            )?;
            let untracked = parse_file_list(&untracked_output, &repo_root);
            for f in untracked {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }

        // Drop paths git named that no longer exist (deletions, and the old
        // side of a rename that crossed the scan-root boundary). See the
        // existence contract above.
        files.retain(|path| path.exists());

        files.sort();
        files.dedup();
        Ok(files)
    }

    fn diff_files(&self, root: &Path, base_ref: &str) -> Result<Vec<PathBuf>, ScmError> {
        let repo_root = repo_root(root)?;

        // Three-dot diff: changes between merge-base(base_ref, HEAD) and HEAD
        let range = format!("{base_ref}...HEAD");
        let output = run_git(root, &["diff", "--name-only", &range])?;
        let mut files = parse_file_list(&output, &repo_root);
        files.sort();
        files.dedup();
        Ok(files)
    }

    fn diff_files_range(
        &self,
        root: &Path,
        since_ref: &str,
        until_ref: &str,
    ) -> Result<Vec<PathBuf>, ScmError> {
        let repo_root = repo_root(root)?;
        let range = format!("{since_ref}...{until_ref}");
        let output = run_git(root, &["diff", "--name-only", &range])?;
        let mut files = parse_file_list(&output, &repo_root);
        files.sort();
        files.dedup();
        Ok(files)
    }

    fn diff_hunks(&self, root: &Path, base_ref: &str) -> Result<DiffHunkSet, ScmError> {
        let repo_root = repo_root(root)?;
        let range = format!("{base_ref}...HEAD");
        let output = run_git(
            root,
            &[
                "diff",
                "--unified=0",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
                &range,
            ],
        )?;
        Ok(parse_diff_hunks(&output, &repo_root))
    }

    fn staged_diff_hunks(&self, root: &Path) -> Result<DiffHunkSet, ScmError> {
        let repo_root = repo_root(root)?;
        let output = run_git(
            root,
            &[
                "diff",
                "--cached",
                "--unified=0",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
            ],
        )?;
        Ok(parse_diff_hunks(&output, &repo_root))
    }
}

/// Parsed diff hunks grouped with file-level skips.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffHunkSet {
    /// Hunks for supported, non-deleted source files.
    pub hunks: Vec<DiffHunk>,
    /// Files that cannot produce function targets.
    pub skipped_files: Vec<DiffFileSkip>,
}

/// A single unified diff hunk's old/new line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// Absolute path to the new-side file.
    pub file_path: PathBuf,
    /// Old-side starting line from the hunk header.
    pub old_start: u32,
    /// Number of old-side lines in the hunk.
    pub old_count: u32,
    /// New-side starting line from the hunk header.
    pub new_start: u32,
    /// Number of new-side lines in the hunk.
    pub new_count: u32,
}

impl DiffHunk {
    fn intersects_function(&self, function: &FunctionEntry) -> bool {
        let start = self.new_start;
        let end = self
            .new_start
            .saturating_add(self.new_count.saturating_sub(1));

        if self.new_count == 0 {
            // Deletion-only hunks anchor to the surrounding new-side line.
            // Git emits new_start=0 for a deletion at the very top of the
            // file (no preceding line exists); anchor that case to line 1
            // so a function starting at the top of the file still matches.
            let start = start.max(1);
            start >= function.start_line && start <= function.end_line
        } else {
            start <= function.end_line && end >= function.start_line
        }
    }
}

/// Why a diff file cannot be mapped to functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFileSkip {
    /// Absolute path when the diff names a concrete file.
    pub file_path: PathBuf,
    /// Machine-readable skip reason.
    pub reason: DiffFileSkipReason,
}

/// File-level diff skip reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFileSkipReason {
    /// The file was deleted in the diff.
    FileDeleted,
    /// Git reported a binary file diff.
    Binary,
    /// Git reported a submodule diff.
    Submodule,
    /// The file extension is not supported by a Shatter frontend.
    UnsupportedLanguage,
}

/// Select functions whose source ranges intersect changed diff hunks.
#[must_use]
pub fn functions_for_diff_hunks<'a>(
    registry: &'a FunctionRegistry,
    hunks: &[DiffHunk],
) -> Vec<&'a FunctionEntry> {
    let mut by_file: BTreeMap<&Path, Vec<&DiffHunk>> = BTreeMap::new();
    for hunk in hunks {
        by_file
            .entry(hunk.file_path.as_path())
            .or_default()
            .push(hunk);
    }

    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for entry in registry.entries() {
        let Some(file_hunks) = by_file.get(entry.file_path.as_path()) else {
            continue;
        };
        if file_hunks
            .iter()
            .any(|hunk| hunk.intersects_function(entry))
        {
            let key = FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
            if seen.insert(key) {
                selected.push(entry);
            }
        }
    }
    selected
}

fn parse_diff_hunks(output: &str, repo_root: &Path) -> DiffHunkSet {
    let mut result = DiffHunkSet::default();
    let mut current_path: Option<PathBuf> = None;
    let mut current_skipped: Option<DiffFileSkipReason> = None;

    for line in output.lines() {
        if let Some((_, new_path)) = parse_diff_git_paths(line) {
            current_path = new_path.map(|path| repo_root.join(path));
            current_skipped = current_path
                .as_deref()
                .and_then(skip_reason_for_supported_path);
            continue;
        }

        if line.starts_with("deleted file mode") || line == "+++ /dev/null" {
            current_skipped = Some(DiffFileSkipReason::FileDeleted);
            continue;
        }

        if line.starts_with("Binary files ") {
            let path = current_path
                .clone()
                .or_else(|| parse_binary_diff_path(line, repo_root));
            push_skip(&mut result, path, DiffFileSkipReason::Binary);
            current_skipped = Some(DiffFileSkipReason::Binary);
            continue;
        }

        if line.starts_with("Submodule ") {
            let path = current_path
                .clone()
                .or_else(|| parse_submodule_diff_path(line, repo_root));
            push_skip(&mut result, path, DiffFileSkipReason::Submodule);
            current_skipped = Some(DiffFileSkipReason::Submodule);
            continue;
        }

        let Some(hunk) = parse_hunk_header(line) else {
            continue;
        };
        let Some(file_path) = current_path.clone() else {
            continue;
        };
        if let Some(reason) = current_skipped {
            push_skip(&mut result, Some(file_path), reason);
            continue;
        }
        result.hunks.push(DiffHunk {
            file_path,
            old_start: hunk.old_start,
            old_count: hunk.old_count,
            new_start: hunk.new_start,
            new_count: hunk.new_count,
        });
    }

    result
        .skipped_files
        .sort_by(|a, b| a.file_path.cmp(&b.file_path));
    result
        .skipped_files
        .dedup_by(|a, b| a.file_path == b.file_path && a.reason == b.reason);
    result
}

fn push_skip(result: &mut DiffHunkSet, file_path: Option<PathBuf>, reason: DiffFileSkipReason) {
    let Some(file_path) = file_path else {
        return;
    };
    result
        .skipped_files
        .push(DiffFileSkip { file_path, reason });
}

fn skip_reason_for_supported_path(path: &Path) -> Option<DiffFileSkipReason> {
    let ext = path.extension().and_then(|ext| ext.to_str())?;
    if Language::from_extension(ext).is_some() {
        None
    } else {
        Some(DiffFileSkipReason::UnsupportedLanguage)
    }
}

fn parse_diff_git_paths(line: &str) -> Option<(Option<PathBuf>, Option<PathBuf>)> {
    let rest = line.strip_prefix("diff --git ")?;
    let mut parts = rest.split_whitespace();
    let old = parts.next().and_then(parse_prefixed_diff_path);
    let new = parts.next().and_then(parse_prefixed_diff_path);
    Some((old, new))
}

fn parse_prefixed_diff_path(path: &str) -> Option<PathBuf> {
    if path == "/dev/null" {
        return None;
    }
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .map(PathBuf::from)
}

fn parse_binary_diff_path(line: &str, repo_root: &Path) -> Option<PathBuf> {
    let rest = line.strip_prefix("Binary files ")?;
    let path = rest
        .split(" and ")
        .nth(1)
        .and_then(|right| right.strip_suffix(" differ"))
        .and_then(parse_prefixed_diff_path)?;
    Some(repo_root.join(path))
}

fn parse_submodule_diff_path(line: &str, repo_root: &Path) -> Option<PathBuf> {
    let path = line.strip_prefix("Submodule ")?.split_whitespace().next()?;
    Some(repo_root.join(path))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedHunkHeader {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
}

fn parse_hunk_header(line: &str) -> Option<ParsedHunkHeader> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let (old_start, old_count) = parse_hunk_range(old)?;
    let (new_start, new_count) = parse_hunk_range(new)?;
    Some(ParsedHunkHeader {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

fn parse_hunk_range(range: &str) -> Option<(u32, u32)> {
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start, count),
        None => (range, "1"),
    };
    Some((start.parse().ok()?, count.parse().ok()?))
}

/// Detect the SCM provider for the given directory.
/// Currently only supports Git.
pub fn detect_provider(root: &Path) -> Result<GitProvider, ScmError> {
    // Clear GIT_DIR / GIT_WORK_TREE so the child process discovers the repo
    // from `root` rather than inheriting stale values (e.g. from git hooks).
    let status = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(GitProvider),
        Ok(_) => Err(ScmError::NotARepo {
            path: root.to_path_buf(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ScmError::GitNotFound),
        Err(e) => Err(ScmError::Io(e)),
    }
}

/// Compute the git blob hash for a file (content-addressable identifier).
/// Uses `git hash-object` which hashes the file content as git would store it.
pub fn blob_hash(root: &Path, file: &Path) -> Result<String, ScmError> {
    let file_str = file.to_string_lossy();
    let output = run_git(root, &["hash-object", &file_str])?;
    Ok(output.trim().to_string())
}

/// Retrieve file contents at a specific git ref.
///
/// `relative_path` must be relative to the repository root.
/// Returns the raw bytes of the file as it existed at `git_ref`.
pub fn show_file_at_ref(
    root: &Path,
    git_ref: &str,
    relative_path: &Path,
) -> Result<Vec<u8>, ScmError> {
    let path_str = relative_path.to_string_lossy();
    let spec = format!("{git_ref}:{path_str}");
    let output = Command::new("git")
        .args(["show", &spec])
        .current_dir(root)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ScmError::GitNotFound
            } else {
                ScmError::Io(e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output.status.code().unwrap_or(-1);
        return Err(ScmError::GitFailed { code, stderr });
    }

    Ok(output.stdout)
}

/// Check whether a git ref resolves to a valid commit.
pub fn validate_ref(root: &Path, git_ref: &str) -> Result<String, ScmError> {
    let output = run_git(root, &["rev-parse", "--verify", git_ref])?;
    Ok(output.trim().to_string())
}

/// Get the current HEAD commit hash (short form).
pub fn head_commit(root: &Path) -> Result<String, ScmError> {
    let output = run_git(root, &["rev-parse", "--short", "HEAD"])?;
    Ok(output.trim().to_string())
}

/// Return the git repository root for `root`, or `None` if `root` is not
/// inside a git repo or git is unavailable. Convenience wrapper around the
/// private `repo_root` for callers that prefer Option to ScmError.
pub fn repo_root_or_none(root: &Path) -> Option<PathBuf> {
    repo_root(root).ok()
}

/// Return whether the working tree at `root` has uncommitted changes
/// (staged, unstaged, or untracked-but-not-ignored). Returns `Err` when
/// git is unavailable or the path is not inside a repo.
pub fn working_tree_dirty(root: &Path) -> Result<bool, ScmError> {
    let output = run_git(root, &["status", "--porcelain"])?;
    Ok(!output.trim().is_empty())
}

/// Run a git command in the given directory and return stdout as a string.
///
/// Always prepends `-c core.quotepath=false` so git never C-quotes
/// (octal-escapes) non-ASCII bytes in the pathnames it prints. With the default
/// `core.quotepath=true`, a path like `文.ts` is emitted as the literal
/// `"\346\226\207.ts"`, which `parse_file_list` would join verbatim into a
/// nonexistent path — silently dropping such files from `--changed`/`--since`
/// (str-jz13q). Setting it here rather than at individual path-listing call
/// sites means future callers can't forget it (str-k6e61); the flag is inert
/// for commands that don't print pathnames (e.g. `rev-parse`, `hash-object`).
/// The config flag must precede the git subcommand.
pub(crate) fn run_git(root: &Path, args: &[&str]) -> Result<String, ScmError> {
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ScmError::GitNotFound
            } else {
                ScmError::Io(e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output.status.code().unwrap_or(-1);
        return Err(ScmError::GitFailed { code, stderr });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn repo_root(root: &Path) -> Result<PathBuf, ScmError> {
    Ok(PathBuf::from(
        run_git(root, &["rev-parse", "--show-toplevel"])?.trim(),
    ))
}

/// Parse newline-separated file paths from git output into absolute paths.
fn parse_file_list(output: &str, root: &Path) -> Vec<PathBuf> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| root.join(line.trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
    use crate::types::TypeInfo;
    use proptest::prelude::*;
    use std::collections::HashMap;
    use std::fs;
    use std::process::Command;

    fn git_ok(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            // Clear git hook-injected env vars so commands operate on `cwd`'s
            // repo, not the ambient shatter repo that's running the hook.
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .status()
            .expect("git command should run");
        assert!(status.success(), "git {:?} failed", args);
    }

    /// Create a temp git repo with user identity configured. The returned
    /// `TempDir` owns the repo directory — keep it in scope for the repo's
    /// lifetime and use `.path()` for the repo root.
    ///
    /// Sets repo-local `core.quotepath=true` (git's own default) so path-listing
    /// tests exercise the C-quoting hazard regardless of ambient global config:
    /// on a machine with `core.quotepath=false` set globally the fix would
    /// otherwise pass trivially even if it regressed (str-k6e61).
    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo = dir.path();
        git_ok(repo, &["init", "-q"]);
        git_ok(repo, &["config", "user.email", "t@example.com"]);
        git_ok(repo, &["config", "user.name", "t"]);
        git_ok(repo, &["config", "core.quotepath", "true"]);
        dir
    }

    /// Assert that `files` contains `want` after canonicalizing both sides.
    /// Canonicalizing tolerates symlinked temp dirs and, because it resolves
    /// against the real filesystem, also proves `want` actually exists — a
    /// C-quoted literal path would fail to canonicalize and never match.
    fn assert_contains_canonicalized(files: &[PathBuf], want: &Path) {
        let canon: Vec<PathBuf> = files.iter().filter_map(|f| f.canonicalize().ok()).collect();
        let want_canon = want.canonicalize().expect("canonicalize expected path");
        assert!(
            canon.contains(&want_canon),
            "expected file missing or mis-resolved: want={want:?} got={files:?}"
        );
    }

    fn entry(file: &str, name: &str, start_line: u32, end_line: u32) -> FunctionEntry {
        FunctionEntry {
            file_path: PathBuf::from(file),
            name: name.to_string(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Unknown,
            dependencies: vec![],
            crypto_boundaries: vec![],
            branch_count: 0,
            start_line,
            end_line,
        }
    }

    fn registry(entries: Vec<FunctionEntry>) -> FunctionRegistry {
        let index = entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                (
                    FunctionRegistry::qualified_name(&entry.file_path, &entry.name),
                    idx,
                )
            })
            .collect::<HashMap<_, _>>();
        FunctionRegistry::from_raw(entries, index)
    }

    #[test]
    fn test_parse_file_list_basic() {
        let output = "src/main.rs\nsrc/lib.rs\n";
        let root = Path::new("/repo");
        let files = parse_file_list(output, root);
        assert_eq!(
            files,
            vec![
                PathBuf::from("/repo/src/main.rs"),
                PathBuf::from("/repo/src/lib.rs"),
            ]
        );
    }

    #[test]
    fn test_parse_file_list_empty() {
        let files = parse_file_list("", Path::new("/repo"));
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_file_list_trailing_whitespace() {
        let output = "  src/foo.ts  \nbar.go\n";
        let root = Path::new("/repo");
        let files = parse_file_list(output, root);
        assert_eq!(files.len(), 2);
        // trim() handles whitespace
        assert_eq!(files[0], PathBuf::from("/repo/src/foo.ts"));
        assert_eq!(files[1], PathBuf::from("/repo/bar.go"));
    }

    #[test]
    fn test_parse_file_list_blank_lines() {
        let output = "a.ts\n\nb.ts\n\n";
        let files = parse_file_list(output, Path::new("/r"));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn parse_diff_hunk_with_added_lines_maps_to_function() {
        let diff = "\
diff --git a/src/app.ts b/src/app.ts
index 1111111..2222222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -4,0 +5,2 @@ export function changed() {
+  const x = 1;
+  return x;
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));
        assert_eq!(parsed.skipped_files, Vec::new());
        assert_eq!(
            parsed.hunks,
            vec![DiffHunk {
                file_path: PathBuf::from("/repo/src/app.ts"),
                old_start: 4,
                old_count: 0,
                new_start: 5,
                new_count: 2,
            }]
        );

        let registry = registry(vec![
            entry("/repo/src/app.ts", "unchanged", 1, 3),
            entry("/repo/src/app.ts", "changed", 4, 8),
        ]);
        let selected = functions_for_diff_hunks(&registry, &parsed.hunks);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "changed");
    }

    #[test]
    fn deletion_only_hunk_maps_to_enclosing_function_anchor() {
        let diff = "\
diff --git a/src/app.ts b/src/app.ts
index 1111111..2222222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -6,2 +6,0 @@ export function changed() {
-  const x = 1;
-  return x;
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));
        assert_eq!(
            parsed.hunks,
            vec![DiffHunk {
                file_path: PathBuf::from("/repo/src/app.ts"),
                old_start: 6,
                old_count: 2,
                new_start: 6,
                new_count: 0,
            }]
        );

        let registry = registry(vec![
            entry("/repo/src/app.ts", "before", 1, 5),
            entry("/repo/src/app.ts", "changed", 6, 10),
        ]);
        let selected = functions_for_diff_hunks(&registry, &parsed.hunks);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "changed");
    }

    #[test]
    fn deletion_only_hunk_at_top_of_file_maps_to_leading_function() {
        // Git emits new_start=0 for a deletion-only hunk with no preceding
        // line (i.e. the deleted lines were at the very top of the file).
        let diff = "\
diff --git a/src/app.ts b/src/app.ts
index 1111111..2222222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,2 +0,0 @@
-  const x = 1;
-  return x;
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));
        assert_eq!(parsed.hunks[0].new_start, 0);
        assert_eq!(parsed.hunks[0].new_count, 0);

        let registry = registry(vec![entry("/repo/src/app.ts", "leading", 1, 5)]);
        let selected = functions_for_diff_hunks(&registry, &parsed.hunks);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "leading");
    }

    #[test]
    fn deleted_file_produces_skip_without_hunks() {
        let diff = "\
diff --git a/src/dead.ts b/src/dead.ts
deleted file mode 100644
index 1111111..0000000
--- a/src/dead.ts
+++ /dev/null
@@ -1,3 +0,0 @@
-export function dead() {
-  return 1;
-}
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));

        assert!(parsed.hunks.is_empty());
        assert_eq!(
            parsed.skipped_files,
            vec![DiffFileSkip {
                file_path: PathBuf::from("/repo/src/dead.ts"),
                reason: DiffFileSkipReason::FileDeleted,
            }]
        );
    }

    #[test]
    fn binary_and_submodule_diffs_produce_file_skips() {
        let diff = "\
diff --git a/assets/logo.png b/assets/logo.png
index 1111111..2222222 100644
Binary files a/assets/logo.png and b/assets/logo.png differ
diff --git a/vendor/lib b/vendor/lib
index 1111111..2222222 160000
--- a/vendor/lib
+++ b/vendor/lib
Submodule vendor/lib 1111111..2222222:
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));

        assert!(parsed.hunks.is_empty());
        assert_eq!(
            parsed.skipped_files,
            vec![
                DiffFileSkip {
                    file_path: PathBuf::from("/repo/assets/logo.png"),
                    reason: DiffFileSkipReason::Binary,
                },
                DiffFileSkip {
                    file_path: PathBuf::from("/repo/vendor/lib"),
                    reason: DiffFileSkipReason::Submodule,
                },
            ]
        );
    }

    #[test]
    fn unsupported_language_file_produces_skip() {
        let diff = "\
diff --git a/README.md b/README.md
index 1111111..2222222 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
";
        let parsed = parse_diff_hunks(diff, Path::new("/repo"));

        assert!(parsed.hunks.is_empty());
        assert_eq!(
            parsed.skipped_files,
            vec![DiffFileSkip {
                file_path: PathBuf::from("/repo/README.md"),
                reason: DiffFileSkipReason::UnsupportedLanguage,
            }]
        );
    }

    proptest! {
        #[test]
        fn functions_for_diff_hunks_matches_range_intersection(
            function_start in 1u32..1_000,
            function_len in 0u32..100,
            hunk_start in 1u32..1_100,
            hunk_count in 0u32..100,
        ) {
            let function_end = function_start + function_len;
            let registry = registry(vec![entry(
                "/repo/src/app.ts",
                "candidate",
                function_start,
                function_end,
            )]);
            let hunk = DiffHunk {
                file_path: PathBuf::from("/repo/src/app.ts"),
                old_start: hunk_start,
                old_count: hunk_count,
                new_start: hunk_start,
                new_count: hunk_count,
            };

            let hunk_end = hunk_start + hunk_count.saturating_sub(1);
            let expected = if hunk_count == 0 {
                hunk_start >= function_start && hunk_start <= function_end
            } else {
                hunk_start <= function_end && hunk_end >= function_start
            };
            let selected = functions_for_diff_hunks(&registry, &[hunk]);

            prop_assert_eq!(selected.len() == 1, expected);
        }
    }

    #[test]
    fn test_detect_provider_in_git_repo() {
        // This test runs in the shatter repo, which is a git repo
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = detect_provider(root);
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_provider_not_a_repo() {
        // Verify that git rev-parse fails in a directory with no repo.
        // We use GIT_CEILING_DIRECTORIES on the subprocess (not process-wide)
        // to prevent git from ascending into a parent repo, which happens
        // when tests run inside a git worktree.
        let dir = tempfile::tempdir().expect("create temp dir");
        let dir_path = dir.path();
        let parent = dir_path.parent().unwrap_or(dir_path);

        // Clear GIT_DIR/GIT_WORK_TREE which git hooks inject into the env —
        // without this, the subprocess inherits them and finds the repo anyway.
        let status = std::process::Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(dir_path)
            .env("GIT_CEILING_DIRECTORIES", parent)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git command should run");

        assert!(
            !status.success(),
            "git rev-parse should fail in a non-repo dir"
        );
    }

    #[test]
    fn test_changed_files_untracked_from_subdir_root() {
        // str-g9i4v: `git ls-files --others` prints cwd-relative paths while
        // `git diff --name-only` prints repo-root-relative paths. When the
        // scan root is a repo subdirectory, untracked files must still resolve
        // to their true absolute paths.
        let dir = init_repo();
        let repo = dir.path();
        git_ok(repo, &["commit", "-q", "--allow-empty", "-m", "init"]);

        let subdir = repo.join("src");
        fs::create_dir(&subdir).expect("create subdir");
        let tracked = subdir.join("tracked.ts");
        fs::write(&tracked, "export const a = 1;\n").expect("write tracked");
        git_ok(repo, &["add", "src/tracked.ts"]);
        git_ok(repo, &["commit", "-q", "-m", "add tracked"]);
        fs::write(&tracked, "export const a = 2;\n").expect("modify tracked");

        let untracked = subdir.join("untracked.ts");
        fs::write(&untracked, "export const b = 1;\n").expect("write untracked");

        let provider = GitProvider;
        let files = provider
            .changed_files(&subdir, true)
            .expect("changed_files should succeed");

        assert_contains_canonicalized(&files, &tracked);
        assert_contains_canonicalized(&files, &untracked);
    }

    #[test]
    fn test_changed_files_scoping_is_symmetric_for_subdir_root() {
        // str-a2wkn: `git diff --name-only HEAD` ignores cwd and reports
        // repo-wide, while `git ls-files --others` only lists files under cwd.
        // changed_files must apply one consistent scope: everything under the
        // scan root is reported, everything outside it is not — regardless of
        // whether the change is tracked or untracked.
        let dir = init_repo();
        let repo = dir.path();

        let inside = repo.join("src");
        let outside = repo.join("other");
        fs::create_dir(&inside).expect("create src");
        fs::create_dir(&outside).expect("create other");

        let tracked_inside = inside.join("tracked.ts");
        let tracked_outside = outside.join("tracked.ts");
        fs::write(&tracked_inside, "export const a = 1;\n").expect("write");
        fs::write(&tracked_outside, "export const b = 1;\n").expect("write");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "init"]);

        // Tracked modification on both sides of the scan-root boundary.
        fs::write(&tracked_inside, "export const a = 2;\n").expect("modify");
        fs::write(&tracked_outside, "export const b = 2;\n").expect("modify");

        // Untracked file on both sides of the scan-root boundary.
        let untracked_inside = inside.join("untracked.ts");
        let untracked_outside = outside.join("untracked.ts");
        fs::write(&untracked_inside, "export const c = 1;\n").expect("write");
        fs::write(&untracked_outside, "export const d = 1;\n").expect("write");

        let provider = GitProvider;
        let files = provider
            .changed_files(&inside, true)
            .expect("changed_files should succeed");

        // Both in-scope changes are reported...
        assert_contains_canonicalized(&files, &tracked_inside);
        assert_contains_canonicalized(&files, &untracked_inside);

        // ...and neither out-of-scope change is, tracked or untracked.
        let canon: Vec<PathBuf> = files.iter().filter_map(|f| f.canonicalize().ok()).collect();
        for unwanted in [&tracked_outside, &untracked_outside] {
            let unwanted_canon = unwanted.canonicalize().expect("canonicalize");
            assert!(
                !canon.contains(&unwanted_canon),
                "file outside scan root should not be reported: {unwanted:?} got={files:?}"
            );
        }
    }

    #[test]
    fn test_changed_files_staged_changes_are_subdir_scoped() {
        // The `--cached` call inside changed_files needs the same pathspec as
        // the working-tree call; without it a staged change outside the scan
        // root would leak back in through the staged-only merge step.
        let dir = init_repo();
        let repo = dir.path();
        git_ok(repo, &["commit", "-q", "--allow-empty", "-m", "init"]);

        let inside = repo.join("src");
        let outside = repo.join("other");
        fs::create_dir(&inside).expect("create src");
        fs::create_dir(&outside).expect("create other");

        let staged_inside = inside.join("new.ts");
        let staged_outside = outside.join("new.ts");
        fs::write(&staged_inside, "export const a = 1;\n").expect("write");
        fs::write(&staged_outside, "export const b = 1;\n").expect("write");
        git_ok(repo, &["add", "."]);

        let provider = GitProvider;
        let files = provider
            .changed_files(&inside, false)
            .expect("changed_files should succeed");

        assert_contains_canonicalized(&files, &staged_inside);
        let canon: Vec<PathBuf> = files.iter().filter_map(|f| f.canonicalize().ok()).collect();
        let unwanted = staged_outside.canonicalize().expect("canonicalize");
        assert!(
            !canon.contains(&unwanted),
            "staged file outside scan root should not be reported: got={files:?}"
        );
    }

    /// Assert no returned path canonicalizes to `unwanted`, and — for paths
    /// that no longer exist and therefore cannot canonicalize — that none is
    /// literally equal to it either.
    fn assert_not_reported(files: &[PathBuf], unwanted: &Path) {
        let canon: Vec<PathBuf> = files.iter().filter_map(|f| f.canonicalize().ok()).collect();
        if let Ok(unwanted_canon) = unwanted.canonicalize() {
            assert!(
                !canon.contains(&unwanted_canon),
                "path should not be reported: want-absent={unwanted:?} got={files:?}"
            );
        }
        assert!(
            !files.contains(&unwanted.to_path_buf()),
            "path should not be reported: want-absent={unwanted:?} got={files:?}"
        );
    }

    #[test]
    fn test_changed_files_rename_out_of_root_omits_vanished_path() {
        // str-a2wkn review: a pathspec suppresses git's rename pairing, so a
        // file renamed OUT of the scan root is reported at its old path, which
        // no longer exists. `filter_file_list` does not stat its input, so such
        // a path would reach a frontend as a scan target. Note `--no-renames`
        // does not change this — git reports the old path either way.
        let dir = init_repo();
        let repo = dir.path();

        let inside = repo.join("src");
        let outside = repo.join("other");
        fs::create_dir(&inside).expect("create src");
        fs::create_dir(&outside).expect("create other");
        let original = inside.join("moved.ts");
        fs::write(&original, "export const a = 1;\n").expect("write");
        // A second in-root change so the result is non-empty for the right reason.
        let untouched = inside.join("stays.ts");
        fs::write(&untouched, "export const b = 1;\n").expect("write");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "init"]);

        git_ok(repo, &["mv", "src/moved.ts", "other/moved.ts"]);
        fs::write(&untouched, "export const b = 2;\n").expect("modify");

        let provider = GitProvider;
        let files = provider
            .changed_files(&inside, true)
            .expect("changed_files should succeed");

        // The genuine in-root change is still reported.
        assert_contains_canonicalized(&files, &untouched);
        // The vanished old path is not.
        assert!(!original.exists(), "precondition: old path is gone");
        assert_not_reported(&files, &original);
        // Nor is the new path, which now lives outside the scan root.
        assert_not_reported(&files, &outside.join("moved.ts"));
        // Every reported path must be openable by a downstream consumer.
        for f in &files {
            assert!(f.exists(), "reported a nonexistent path: {f:?} in {files:?}");
        }
    }

    #[test]
    fn test_changed_files_rename_into_root_reports_new_path() {
        // The mirror direction: a file renamed INTO the scan root is reported
        // at its new path, which exists, and its old path outside the root is
        // not reported.
        let dir = init_repo();
        let repo = dir.path();

        let inside = repo.join("src");
        let outside = repo.join("other");
        fs::create_dir(&inside).expect("create src");
        fs::create_dir(&outside).expect("create other");
        let original = outside.join("moved.ts");
        fs::write(&original, "export const a = 1;\n").expect("write");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "init"]);

        git_ok(repo, &["mv", "other/moved.ts", "src/moved.ts"]);
        let destination = inside.join("moved.ts");

        let provider = GitProvider;
        let files = provider
            .changed_files(&inside, true)
            .expect("changed_files should succeed");

        assert_contains_canonicalized(&files, &destination);
        assert_not_reported(&files, &original);
        for f in &files {
            assert!(f.exists(), "reported a nonexistent path: {f:?} in {files:?}");
        }
    }

    #[test]
    fn test_changed_files_omits_plain_deletions() {
        // Same shape as the rename-out case without any rename: `git diff`
        // names the deleted path, which cannot be scanned. Holds at the repo
        // root, where no pathspec scoping is involved at all.
        let dir = init_repo();
        let repo = dir.path();

        let deleted = repo.join("gone.ts");
        let kept = repo.join("kept.ts");
        fs::write(&deleted, "export const a = 1;\n").expect("write");
        fs::write(&kept, "export const b = 1;\n").expect("write");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "init"]);

        git_ok(repo, &["rm", "-q", "gone.ts"]);
        fs::write(&kept, "export const b = 2;\n").expect("modify");

        let provider = GitProvider;
        let files = provider
            .changed_files(repo, true)
            .expect("changed_files should succeed");

        assert_contains_canonicalized(&files, &kept);
        assert_not_reported(&files, &deleted);
        for f in &files {
            assert!(f.exists(), "reported a nonexistent path: {f:?} in {files:?}");
        }
    }

    #[test]
    fn test_changed_files_non_ascii_path() {
        // str-jz13q: with default core.quotepath=true, git C-quotes non-ASCII
        // filenames in `diff --name-only` output (e.g. "\346\226\207.ts").
        // parse_file_list must see the real UTF-8 path, not the quoted literal,
        // otherwise --changed silently drops such files. init_repo() pins
        // core.quotepath=true so the hazard is present regardless of ambient config.
        let dir = init_repo();
        let repo = dir.path();

        let file = repo.join("文.ts");
        fs::write(&file, "export const a = 1;\n").expect("write file");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "add"]);
        fs::write(&file, "export const a = 2;\n").expect("modify file");

        let provider = GitProvider;
        let files = provider
            .changed_files(repo, false)
            .expect("changed_files should succeed");

        assert_contains_canonicalized(&files, &file);
    }

    #[test]
    fn test_diff_files_non_ascii_path() {
        // str-jz13q: same C-quoting hazard for the `diff --name-only <range>`
        // path used by --since. init_repo() pins core.quotepath=true so the
        // hazard is present regardless of ambient config.
        let dir = init_repo();
        let repo = dir.path();

        let file = repo.join("变.go");
        fs::write(&file, "package main\n").expect("write file");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "initial"]);
        fs::write(&file, "package main // changed\n").expect("modify file");
        git_ok(repo, &["commit", "-q", "-am", "change"]);

        let provider = GitProvider;
        let files = provider
            .diff_files(repo, "HEAD~1")
            .expect("diff_files should succeed");

        assert_contains_canonicalized(&files, &file);
    }

    #[test]
    fn test_changed_files_runs_without_error() {
        // Smoke test: changed_files should not panic in a real git repo
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let provider = detect_provider(root).expect("should be a git repo");
        let result = provider.changed_files(root, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_diff_files_against_head() {
        // HEAD...HEAD should produce no changes
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let provider = detect_provider(root).expect("should be a git repo");
        let result = provider.diff_files(root, "HEAD");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_diff_files_from_nested_root_returns_repo_root_paths() {
        let dir = init_repo();
        let repo = dir.path();
        let nested = repo.join("examples/standalone/ts");
        let changed = nested.join("22-opaque-predicate.ts");

        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(&changed, "export const classify = () => 1;\n").expect("write initial file");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-m", "initial"]);

        fs::write(&changed, "export const classify = () => 2;\n").expect("write updated file");
        git_ok(repo, &["commit", "-am", "change"]);

        let provider = detect_provider(&nested).expect("nested path should still detect git repo");
        let files = provider.diff_files(&nested, "HEAD~1").expect("diff files");

        assert_eq!(files, vec![changed]);
    }

    #[test]
    fn test_diff_files_range_same_ref() {
        // HEAD...HEAD range should produce no changes
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let provider = detect_provider(root).expect("should be a git repo");
        let result = provider.diff_files_range(root, "HEAD", "HEAD");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_show_file_at_ref() {
        // shatter-core/Cargo.toml relative to repo root
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = show_file_at_ref(root, "HEAD", Path::new("shatter-core/Cargo.toml"));
        assert!(result.is_ok());
        let content = String::from_utf8(result.unwrap()).expect("valid utf-8");
        assert!(content.contains("[package]"));
    }

    #[test]
    fn test_show_file_at_ref_nonexistent() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = show_file_at_ref(root, "HEAD", Path::new("nonexistent-file.xyz"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_ref_head() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = validate_ref(root, "HEAD");
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_ref_invalid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = validate_ref(root, "nonexistent-ref-abc123");
        assert!(result.is_err());
    }
}
