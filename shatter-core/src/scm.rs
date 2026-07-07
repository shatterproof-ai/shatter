//! SCM (source control) provider for querying changed files.
//!
//! Shells out to `git` with zero external dependencies. Used by `--changed`
//! and `--since` CLI flags to restrict scan scope to modified files.

use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// Files with uncommitted changes (staged + unstaged vs HEAD).
    /// If `include_untracked` is true, also includes untracked files
    /// (excluding gitignored ones).
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
}

/// Git-based SCM provider. Shells out to `git` via `std::process::Command`.
#[derive(Debug)]
pub struct GitProvider;

impl ScmProvider for GitProvider {
    fn changed_files(
        &self,
        root: &Path,
        include_untracked: bool,
    ) -> Result<Vec<PathBuf>, ScmError> {
        let repo_root = repo_root(root)?;

        // Staged + unstaged changes vs HEAD
        let output = run_git_paths(root, &["diff", "--name-only", "HEAD"])?;
        let mut files = parse_file_list(&output, &repo_root);

        // Also include staged-only changes (new files that are staged but not yet committed)
        let staged_output = run_git_paths(root, &["diff", "--name-only", "--cached"])?;
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
            let untracked_output = run_git_paths(
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

        files.sort();
        files.dedup();
        Ok(files)
    }

    fn diff_files(&self, root: &Path, base_ref: &str) -> Result<Vec<PathBuf>, ScmError> {
        let repo_root = repo_root(root)?;

        // Three-dot diff: changes between merge-base(base_ref, HEAD) and HEAD
        let range = format!("{base_ref}...HEAD");
        let output = run_git_paths(root, &["diff", "--name-only", &range])?;
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
        let output = run_git_paths(root, &["diff", "--name-only", &range])?;
        let mut files = parse_file_list(&output, &repo_root);
        files.sort();
        files.dedup();
        Ok(files)
    }
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
pub(crate) fn run_git(root: &Path, args: &[&str]) -> Result<String, ScmError> {
    let output = Command::new("git")
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

/// Run a git command that emits a pathname list, then return stdout.
///
/// Prepends `-c core.quotepath=false` so git does not C-quote (octal-escape)
/// non-ASCII bytes in pathnames. With the default `core.quotepath=true`, a path
/// like `文.ts` is printed as the literal `"\346\226\207.ts"`, which
/// `parse_file_list` would join verbatim into a nonexistent path — silently
/// dropping such files from `--changed`/`--since` (str-jz13q). The config flag
/// must precede the git subcommand.
fn run_git_paths(root: &Path, args: &[&str]) -> Result<String, ScmError> {
    let mut full: Vec<&str> = vec!["-c", "core.quotepath=false"];
    full.extend_from_slice(args);
    run_git(root, &full)
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
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo = dir.path();
        git_ok(repo, &["init", "-q"]);
        git_ok(repo, &["config", "user.email", "t@example.com"]);
        git_ok(repo, &["config", "user.name", "t"]);
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

        // Canonicalize to tolerate symlinked temp dirs.
        let canon: Vec<PathBuf> = files
            .iter()
            .filter_map(|f| f.canonicalize().ok())
            .collect();
        let tracked_canon = tracked.canonicalize().expect("canonicalize tracked");
        let untracked_canon = untracked.canonicalize().expect("canonicalize untracked");
        assert!(
            canon.contains(&tracked_canon),
            "tracked modified file missing: {files:?}"
        );
        assert!(
            canon.contains(&untracked_canon),
            "untracked file missing or mis-resolved: {files:?}"
        );
    }

    #[test]
    fn test_changed_files_non_ascii_path() {
        // str-jz13q: with default core.quotepath=true, git C-quotes non-ASCII
        // filenames in `diff --name-only` output (e.g. "\346\226\207.ts").
        // parse_file_list must see the real UTF-8 path, not the quoted literal,
        // otherwise --changed silently drops such files.
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo = dir.path();
        git_ok(repo, &["init", "-q"]);
        git_ok(repo, &["config", "user.email", "t@example.com"]);
        git_ok(repo, &["config", "user.name", "t"]);

        let file = repo.join("文.ts");
        fs::write(&file, "export const a = 1;\n").expect("write file");
        git_ok(repo, &["add", "."]);
        git_ok(repo, &["commit", "-q", "-m", "add"]);
        fs::write(&file, "export const a = 2;\n").expect("modify file");

        let provider = GitProvider;
        let files = provider
            .changed_files(repo, false)
            .expect("changed_files should succeed");

        // Canonicalize to tolerate symlinked temp dirs and to confirm the path
        // actually exists (a C-quoted literal would fail to canonicalize).
        let canon: Vec<PathBuf> = files
            .iter()
            .filter_map(|f| f.canonicalize().ok())
            .collect();
        let want = file.canonicalize().expect("canonicalize non-ASCII file");
        assert!(
            canon.contains(&want),
            "non-ASCII changed file missing or mis-resolved: {files:?}"
        );
    }

    #[test]
    fn test_diff_files_non_ascii_path() {
        // str-jz13q: same C-quoting hazard for the `diff --name-only <range>`
        // path used by --since.
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo = dir.path();
        git_ok(repo, &["init", "-q"]);
        git_ok(repo, &["config", "user.email", "t@example.com"]);
        git_ok(repo, &["config", "user.name", "t"]);

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

        let canon: Vec<PathBuf> = files
            .iter()
            .filter_map(|f| f.canonicalize().ok())
            .collect();
        let want = file.canonicalize().expect("canonicalize non-ASCII file");
        assert!(
            canon.contains(&want),
            "non-ASCII diffed file missing or mis-resolved: {files:?}"
        );
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
        let dir = tempfile::tempdir().expect("create temp dir");
        let repo = dir.path();
        let nested = repo.join("examples/standalone/ts");
        let changed = nested.join("22-opaque-predicate.ts");

        git_ok(repo, &["init"]);
        git_ok(repo, &["config", "user.name", "Test User"]);
        git_ok(repo, &["config", "user.email", "test@example.com"]);

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
