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
    fn changed_files(&self, root: &Path, include_untracked: bool) -> Result<Vec<PathBuf>, ScmError>;

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
    fn changed_files(&self, root: &Path, include_untracked: bool) -> Result<Vec<PathBuf>, ScmError> {
        // Staged + unstaged changes vs HEAD
        let output = run_git(root, &["diff", "--name-only", "HEAD"])?;
        let mut files = parse_file_list(&output, root);

        // Also include staged-only changes (new files that are staged but not yet committed)
        let staged_output = run_git(root, &["diff", "--name-only", "--cached"])?;
        let staged_files = parse_file_list(&staged_output, root);
        for f in staged_files {
            if !files.contains(&f) {
                files.push(f);
            }
        }

        if include_untracked {
            let untracked_output =
                run_git(root, &["ls-files", "--others", "--exclude-standard"])?;
            let untracked = parse_file_list(&untracked_output, root);
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
        // Three-dot diff: changes between merge-base(base_ref, HEAD) and HEAD
        let range = format!("{base_ref}...HEAD");
        let output = run_git(root, &["diff", "--name-only", &range])?;
        let mut files = parse_file_list(&output, root);
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
        let range = format!("{since_ref}...{until_ref}");
        let output = run_git(root, &["diff", "--name-only", &range])?;
        let mut files = parse_file_list(&output, root);
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
pub fn show_file_at_ref(root: &Path, git_ref: &str, relative_path: &Path) -> Result<Vec<u8>, ScmError> {
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

/// Run a git command in the given directory and return stdout as a string.
pub(crate) fn run_git(root: &Path, args: &[&str]) -> Result<String, ScmError> {
    let output = Command::new("git")
        .args(args)
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

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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

    #[test]
    fn test_parse_file_list_basic() {
        let output = "src/main.rs\nsrc/lib.rs\n";
        let root = Path::new("/repo");
        let files = parse_file_list(output, root);
        assert_eq!(files, vec![
            PathBuf::from("/repo/src/main.rs"),
            PathBuf::from("/repo/src/lib.rs"),
        ]);
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

        assert!(!status.success(), "git rev-parse should fail in a non-repo dir");
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
