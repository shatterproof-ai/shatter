//! Storage API for semantic harness state, split by lifecycle.
//!
//! Three distinct roots prevent temp-scoped directories from holding reusable
//! build state, and give preserved outputs a stable project-relative location.
//!
//! - **Cache**: deterministic, project-scoped (`<project>/.shatter/cache/harness/`).
//!   Survives OS temp cleanup. Holds compiled harnesses, instrumented sources.
//! - **Scratch**: per-session ephemeral (`$TMPDIR/shatter-scratch-<id>/`).
//!   Disposable after each request.
//! - **Artifact**: preserved output (`<project>/shatter-artifacts/`).
//!   User-visible test exports, reports, etc.

use std::path::{Path, PathBuf};

/// Environment variable for the harness cache directory.
pub const ENV_HARNESS_CACHE: &str = "SHATTER_HARNESS_CACHE";
/// Environment variable for the harness scratch directory.
pub const ENV_HARNESS_SCRATCH: &str = "SHATTER_HARNESS_SCRATCH";
/// Environment variable for the artifact output directory.
pub const ENV_ARTIFACT_DIR: &str = "SHATTER_ARTIFACT_DIR";

/// Harness storage roots, split by lifecycle.
#[derive(Debug, Clone)]
pub struct HarnessStorage {
    cache_root: PathBuf,
    scratch_root: PathBuf,
    artifact_root: PathBuf,
}

impl HarnessStorage {
    /// Construct from explicit paths.
    pub fn new(cache_root: PathBuf, scratch_root: PathBuf, artifact_root: PathBuf) -> Self {
        Self {
            cache_root,
            scratch_root,
            artifact_root,
        }
    }

    /// Construct with default roots for a project.
    ///
    /// - Cache: `<project_root>/.shatter/cache/harness/`
    /// - Scratch: `<tempdir>/shatter-scratch-<pid>-<counter>/`
    /// - Artifact: `<project_root>/shatter-artifacts/`
    pub fn for_project(project_root: &Path) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let scratch_id = format!(
            "{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        Self {
            cache_root: Self::default_cache_dir(project_root),
            scratch_root: std::env::temp_dir().join(format!("shatter-scratch-{scratch_id}")),
            artifact_root: Self::default_artifact_dir(project_root),
        }
    }

    /// Default cache directory: `<project_root>/.shatter/cache/harness/`.
    pub fn default_cache_dir(project_root: &Path) -> PathBuf {
        project_root.join(".shatter").join("cache").join("harness")
    }

    /// Default artifact directory: `<project_root>/shatter-artifacts/`.
    pub fn default_artifact_dir(project_root: &Path) -> PathBuf {
        project_root.join("shatter-artifacts")
    }

    /// Resolve the artifact root, honoring `SHATTER_ARTIFACT_DIR` if set.
    ///
    /// Used by explore/scan output writers so callers (the gauntlet, CI,
    /// external audit runs) can redirect repo-local artifact writes to a
    /// temporary directory without modifying the project layout.
    pub fn resolve_artifact_root(project_root: &Path) -> PathBuf {
        if let Some(override_dir) = std::env::var_os(ENV_ARTIFACT_DIR) {
            let p = PathBuf::from(override_dir);
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
        Self::default_artifact_dir(project_root)
    }

    /// Reusable harness/build cache directory.
    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    /// Per-session ephemeral scratch directory.
    pub fn scratch_root(&self) -> &Path {
        &self.scratch_root
    }

    /// Preserved artifact output directory.
    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }

    /// Create the cache directory (and parents) if it doesn't exist.
    pub fn ensure_cache(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.cache_root)
    }

    /// Create the scratch directory (and parents) if it doesn't exist.
    pub fn ensure_scratch(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.scratch_root)
    }

    /// Create the artifact directory (and parents) if it doesn't exist.
    pub fn ensure_artifact(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.artifact_root)
    }

    /// Return environment variable pairs for propagating storage roots to frontends.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            (
                ENV_HARNESS_CACHE.to_string(),
                self.cache_root.to_string_lossy().into_owned(),
            ),
            (
                ENV_HARNESS_SCRATCH.to_string(),
                self.scratch_root.to_string_lossy().into_owned(),
            ),
            (
                ENV_ARTIFACT_DIR.to_string(),
                self.artifact_root.to_string_lossy().into_owned(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cache_dir_path() {
        let root = Path::new("/home/user/project");
        assert_eq!(
            HarnessStorage::default_cache_dir(root),
            PathBuf::from("/home/user/project/.shatter/cache/harness")
        );
    }

    #[test]
    fn default_artifact_dir_path() {
        let root = Path::new("/home/user/project");
        assert_eq!(
            HarnessStorage::default_artifact_dir(root),
            PathBuf::from("/home/user/project/shatter-artifacts")
        );
    }

    // Env-var mutating tests share process state. Serialize them so they
    // don't race with one another or with the env_vars test.
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn resolve_artifact_root_falls_back_to_default_when_env_unset() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_TEST_LOCK; no other thread reads/writes
        // SHATTER_ARTIFACT_DIR while this test runs.
        unsafe { std::env::remove_var(ENV_ARTIFACT_DIR) };
        let root = Path::new("/home/user/project");
        assert_eq!(
            HarnessStorage::resolve_artifact_root(root),
            PathBuf::from("/home/user/project/shatter-artifacts")
        );
    }

    #[test]
    fn resolve_artifact_root_honors_env_var_when_set() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_TEST_LOCK.
        unsafe { std::env::set_var(ENV_ARTIFACT_DIR, "/tmp/gauntlet-artifacts") };
        let root = Path::new("/home/user/project");
        let resolved = HarnessStorage::resolve_artifact_root(root);
        // SAFETY: serialized via ENV_TEST_LOCK.
        unsafe { std::env::remove_var(ENV_ARTIFACT_DIR) };
        assert_eq!(resolved, PathBuf::from("/tmp/gauntlet-artifacts"));
    }

    #[test]
    fn resolve_artifact_root_ignores_empty_env_var() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_TEST_LOCK.
        unsafe { std::env::set_var(ENV_ARTIFACT_DIR, "") };
        let root = Path::new("/home/user/project");
        let resolved = HarnessStorage::resolve_artifact_root(root);
        // SAFETY: serialized via ENV_TEST_LOCK.
        unsafe { std::env::remove_var(ENV_ARTIFACT_DIR) };
        assert_eq!(
            resolved,
            PathBuf::from("/home/user/project/shatter-artifacts")
        );
    }

    #[test]
    fn for_project_uses_correct_roots() {
        let root = Path::new("/home/user/project");
        let storage = HarnessStorage::for_project(root);
        assert_eq!(
            storage.cache_root(),
            Path::new("/home/user/project/.shatter/cache/harness")
        );
        assert!(
            storage
                .scratch_root()
                .to_string_lossy()
                .contains("shatter-scratch-")
        );
        assert_eq!(
            storage.artifact_root(),
            Path::new("/home/user/project/shatter-artifacts")
        );
    }

    #[test]
    fn new_constructs_from_explicit_paths() {
        let storage = HarnessStorage::new(
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c"),
        );
        assert_eq!(storage.cache_root(), Path::new("/a"));
        assert_eq!(storage.scratch_root(), Path::new("/b"));
        assert_eq!(storage.artifact_root(), Path::new("/c"));
    }

    #[test]
    fn scratch_root_is_unique_per_instance() {
        let root = Path::new("/tmp/proj");
        let s1 = HarnessStorage::for_project(root);
        let s2 = HarnessStorage::for_project(root);
        assert_ne!(s1.scratch_root(), s2.scratch_root());
    }

    #[test]
    fn env_vars_returns_three_pairs() {
        let storage = HarnessStorage::for_project(Path::new("/tmp/proj"));
        let vars = storage.env_vars();
        assert_eq!(vars.len(), 3);
        let keys: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&ENV_HARNESS_CACHE));
        assert!(keys.contains(&ENV_HARNESS_SCRATCH));
        assert!(keys.contains(&ENV_ARTIFACT_DIR));
    }

    #[test]
    fn ensure_cache_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("cache").join("harness");
        let storage = HarnessStorage::new(
            cache_path.clone(),
            dir.path().join("scratch"),
            dir.path().join("artifacts"),
        );
        assert!(!cache_path.exists());
        storage.ensure_cache().unwrap();
        assert!(cache_path.exists());
    }

    #[test]
    fn ensure_scratch_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let scratch_path = dir.path().join("scratch");
        let storage = HarnessStorage::new(
            dir.path().join("cache"),
            scratch_path.clone(),
            dir.path().join("artifacts"),
        );
        assert!(!scratch_path.exists());
        storage.ensure_scratch().unwrap();
        assert!(scratch_path.exists());
    }

    #[test]
    fn ensure_artifact_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let artifact_path = dir.path().join("artifacts");
        let storage = HarnessStorage::new(
            dir.path().join("cache"),
            dir.path().join("scratch"),
            artifact_path.clone(),
        );
        assert!(!artifact_path.exists());
        storage.ensure_artifact().unwrap();
        assert!(artifact_path.exists());
    }
}
