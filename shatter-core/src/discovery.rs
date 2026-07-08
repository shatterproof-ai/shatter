//! File discovery with glob/ignore support.
//!
//! Walks a directory tree to find source files for analysis, respecting
//! `.gitignore`, `.shatterignore`, and user-specified include/exclude patterns.
//! Also discovers convention-based setup files for the multi-level setup lifecycle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Supported language, detected by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    Go,
    Rust,
}

impl Language {
    /// Detect language from a file extension string (without the leading dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" => Some(Language::TypeScript),
            "go" => Some(Language::Go),
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    /// Returns the language name as used in the crypto registry TOML.
    pub fn as_registry_str(&self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Rust => "rust",
        }
    }
}

/// Options controlling which files are discovered.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// Glob patterns for files to include (e.g. `["**/*.ts"]`). Empty means all supported files.
    pub include_patterns: Vec<String>,
    /// Glob patterns for files to exclude (e.g. `["**/vendor/**"]`).
    pub exclude_patterns: Vec<String>,
    /// Directory that `include_patterns` are anchored to (str-1q12y).
    ///
    /// When `Some`, include patterns are matched against each file's path
    /// relative to this directory (the config file's directory / project root)
    /// instead of the scan root. This keeps project-root-anchored config
    /// patterns such as `web/src/**/*.ts` working when the scan root is a
    /// subdirectory like `web/src`. `None` (the default) means scan-root
    /// relative, which is correct for CLI `--include` flags.
    pub include_anchor: Option<PathBuf>,
    /// Directory that `exclude_patterns` are anchored to (str-1q12y). See
    /// [`DiscoveryOptions::include_anchor`]; `None` means scan-root relative.
    pub exclude_anchor: Option<PathBuf>,
    /// Whether to respect `.gitignore` files.
    pub respect_gitignore: bool,
    /// Maximum directory traversal depth. `None` means unlimited.
    pub max_depth: Option<usize>,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            include_patterns: vec![],
            exclude_patterns: vec![],
            include_anchor: None,
            exclude_anchor: None,
            respect_gitignore: true,
            max_depth: None,
        }
    }
}

/// Default directory/file patterns that are always excluded.
const DEFAULT_EXCLUDES: &[&str] = &[
    "**/node_modules/**",
    "**/vendor/**",
    "**/dist/**",
    "**/target/**",
    "**/__tests__/**",
    "**/*.test.ts",
    "**/*_test.go",
];

/// Errors that can occur during file discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        pattern: String,
        source: globset::Error,
    },
}

/// str-94cg: suggest a corrected `--include` pattern when the user-supplied
/// pattern matched no files because it duplicates the scan root prefix.
///
/// Include patterns are evaluated against paths relative to the scan root,
/// so an "absolute-looking" pattern such as `internal/runtime/*.go` will
/// never match when the scan root is already `<repo>/internal/runtime/`.
/// This helper returns the trailing fragment with the duplicate prefix
/// stripped, or `None` when no fixup is obvious.
///
/// Handles two common shapes:
/// - Pattern shares a suffix of the scan root's directory components
///   (e.g. root `…/zolem/internal/runtime`, pattern `internal/runtime/*.go`
///   → suggestion `*.go`).
/// - Pattern is the scan root's absolute path joined with a glob
///   (e.g. root `/x/y`, pattern `/x/y/*.go` → suggestion `*.go`).
#[must_use]
pub fn suggest_corrected_include_pattern(pattern: &str, scan_root: &Path) -> Option<String> {
    // Absolute-path pattern: strip the scan root prefix.
    if let Some(root_str) = scan_root.to_str() {
        let with_slash = format!("{root_str}/");
        if let Some(rest) = pattern.strip_prefix(&with_slash)
            && !rest.is_empty()
        {
            return Some(rest.to_string());
        }
    }

    // Relative pattern whose head matches a tail of the scan root.
    let comps: Vec<&str> = scan_root
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    for start in 0..comps.len() {
        let tail = comps[start..].join("/");
        if tail.is_empty() {
            continue;
        }
        let with_slash = format!("{tail}/");
        if let Some(rest) = pattern.strip_prefix(&with_slash)
            && !rest.is_empty()
        {
            return Some(rest.to_string());
        }
    }
    None
}

/// Discover source files under `root`, returning each file's path and detected language.
///
/// Applies default exclusions, user-specified include/exclude patterns,
/// `.gitignore` rules (if `options.respect_gitignore` is true), and
/// `.shatterignore` rules.
pub fn discover_files(
    root: &Path,
    options: &DiscoveryOptions,
) -> Result<Vec<(PathBuf, Language)>, DiscoveryError> {
    let include_set = build_glob_set(&options.include_patterns)?;
    let exclude_set = build_glob_set(&options.exclude_patterns)?;
    let default_exclude_set = build_glob_set(
        &DEFAULT_EXCLUDES
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>(),
    )?;

    let ignore_matcher = if options.respect_gitignore {
        load_ignore_file(&root.join(".gitignore"))
    } else {
        None
    };
    let shatter_ignore_matcher = load_ignore_file(&root.join(".shatterignore"));

    let mut results = Vec::new();
    walk_dir(
        root,
        root,
        0,
        &WalkConfig {
            include_set: &include_set,
            exclude_set: &exclude_set,
            default_exclude_set: &default_exclude_set,
            ignore_matcher: ignore_matcher.as_ref(),
            shatter_ignore_matcher: shatter_ignore_matcher.as_ref(),
            include_anchor: options.include_anchor.as_deref(),
            exclude_anchor: options.exclude_anchor.as_deref(),
            max_depth: options.max_depth,
        },
        &mut results,
    )?;

    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Select the path used to match user include/exclude glob patterns
/// (str-1q12y).
///
/// When `anchor` is set (config-file patterns anchored at the config
/// directory / project root) and the absolute `path` lives under it, patterns
/// are matched against the anchor-relative path. Otherwise they fall back to
/// `scan_relative` (relative to the scan root), which is the correct default
/// for CLI `--include`/`--exclude` flags. The anchor is expected to be an
/// ancestor of the scan root; the `scan_relative` fallback keeps matching
/// well-defined even if it is not.
fn pattern_relative<'p>(
    path: &'p Path,
    scan_relative: &'p Path,
    anchor: Option<&Path>,
) -> &'p Path {
    if let Some(anchor) = anchor
        && let Ok(rel) = path.strip_prefix(anchor)
    {
        rel
    } else {
        scan_relative
    }
}

struct WalkConfig<'a> {
    include_set: &'a Option<GlobSet>,
    exclude_set: &'a Option<GlobSet>,
    default_exclude_set: &'a Option<GlobSet>,
    ignore_matcher: Option<&'a GlobSet>,
    shatter_ignore_matcher: Option<&'a GlobSet>,
    /// Anchor directory for `include_set` matching (str-1q12y).
    include_anchor: Option<&'a Path>,
    /// Anchor directory for `exclude_set` matching (str-1q12y).
    exclude_anchor: Option<&'a Path>,
    max_depth: Option<usize>,
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    depth: usize,
    config: &WalkConfig<'_>,
    results: &mut Vec<(PathBuf, Language)>,
) -> Result<(), DiscoveryError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(DiscoveryError::Io(e)),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path);

        if path.is_dir() {
            // Respect max_depth if set
            if let Some(max) = config.max_depth
                && depth >= max
            {
                continue;
            }
            // Check if directory is excluded by default patterns
            // For directory matching, append a trailing slash sentinel
            let dir_pattern_path = PathBuf::from(format!("{}/sentinel", relative.display()));
            if let Some(set) = config.default_exclude_set
                && set.is_match(&dir_pattern_path)
            {
                continue;
            }
            // User excludes match against the exclude anchor (str-1q12y): a
            // config pattern like `web/src/vendor/**` must still prune the
            // `web/src/vendor` directory when the scan root is `web/src`.
            let exclude_dir_rel = pattern_relative(&path, relative, config.exclude_anchor);
            let exclude_dir_pattern =
                PathBuf::from(format!("{}/sentinel", exclude_dir_rel.display()));
            if let Some(set) = config.exclude_set
                && set.is_match(&exclude_dir_pattern)
            {
                continue;
            }
            if let Some(set) = config.ignore_matcher
                && set.is_match(&dir_pattern_path)
            {
                continue;
            }
            if let Some(set) = config.shatter_ignore_matcher
                && set.is_match(&dir_pattern_path)
            {
                continue;
            }
            walk_dir(base, &path, depth + 1, config, results)?;
            continue;
        }

        // Check default excludes
        if let Some(set) = config.default_exclude_set
            && set.is_match(relative)
        {
            continue;
        }

        // Check user excludes (matched against the exclude anchor, str-1q12y)
        let exclude_rel = pattern_relative(&path, relative, config.exclude_anchor);
        if let Some(set) = config.exclude_set
            && set.is_match(exclude_rel)
        {
            continue;
        }

        // Check .gitignore
        if let Some(set) = config.ignore_matcher
            && set.is_match(relative)
        {
            continue;
        }

        // Check .shatterignore
        if let Some(set) = config.shatter_ignore_matcher
            && set.is_match(relative)
        {
            continue;
        }

        // Detect language
        let ext = path.extension().and_then(|e| e.to_str());
        let language = ext.and_then(Language::from_extension);
        let Some(language) = language else {
            continue;
        };

        // Check user includes (if specified, file must match at least one;
        // matched against the include anchor, str-1q12y)
        let include_rel = pattern_relative(&path, relative, config.include_anchor);
        if let Some(set) = config.include_set
            && !set.is_match(include_rel)
        {
            continue;
        }

        results.push((path, language));
    }

    Ok(())
}

/// Filter a pre-supplied list of file paths through the same exclude/include/language
/// stack used by [`discover_files`]. Useful when an external source (e.g. SCM provider)
/// supplies the candidate file list instead of directory walking.
///
/// Paths in `files` should be absolute. Paths that don't start with `root` are skipped.
/// Returns `(absolute_path, Language)` pairs, sorted by path.
pub fn filter_file_list(
    root: &Path,
    files: Vec<PathBuf>,
    options: &DiscoveryOptions,
) -> Result<Vec<(PathBuf, Language)>, DiscoveryError> {
    let include_set = build_glob_set(&options.include_patterns)?;
    let exclude_set = build_glob_set(&options.exclude_patterns)?;
    let default_exclude_set = build_glob_set(
        &DEFAULT_EXCLUDES
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>(),
    )?;

    let ignore_matcher = if options.respect_gitignore {
        load_ignore_file(&root.join(".gitignore"))
    } else {
        None
    };
    let shatter_ignore_matcher = load_ignore_file(&root.join(".shatterignore"));

    let mut results = Vec::new();

    for path in files {
        let relative = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Skip directories
        if path.is_dir() {
            continue;
        }

        if let Some(ref set) = default_exclude_set
            && set.is_match(relative)
        {
            continue;
        }

        // User excludes match against the exclude anchor (str-1q12y).
        let exclude_rel = pattern_relative(&path, relative, options.exclude_anchor.as_deref());
        if let Some(ref set) = exclude_set
            && set.is_match(exclude_rel)
        {
            continue;
        }

        if let Some(ref set) = ignore_matcher
            && set.is_match(relative)
        {
            continue;
        }

        if let Some(ref set) = shatter_ignore_matcher
            && set.is_match(relative)
        {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str());
        let Some(language) = ext.and_then(Language::from_extension) else {
            continue;
        };

        // User includes match against the include anchor (str-1q12y).
        let include_rel = pattern_relative(&path, relative, options.include_anchor.as_deref());
        if let Some(ref set) = include_set
            && !set.is_match(include_rel)
        {
            continue;
        }

        results.push((path, language));
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Build a `GlobSet` from pattern strings. Returns `None` if patterns is empty.
fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>, DiscoveryError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| DiscoveryError::InvalidPattern {
            pattern: pattern.clone(),
            source: e,
        })?;
        builder.add(glob);
    }
    let set = builder
        .build()
        .map_err(|e| DiscoveryError::InvalidPattern {
            pattern: patterns.join(", "),
            source: e,
        })?;
    Ok(Some(set))
}

/// Load an ignore file and parse its patterns into a `GlobSet`.
fn load_ignore_file(path: &Path) -> Option<GlobSet> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut builder = GlobSetBuilder::new();
    let mut has_patterns = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Convert gitignore-style patterns to glob patterns
        let pattern = if line.ends_with('/') {
            // Directory pattern: match everything underneath
            format!("**/{line}**")
        } else if line.contains('/') {
            line.to_string()
        } else {
            format!("**/{line}")
        };
        if let Ok(glob) = Glob::new(&pattern) {
            builder.add(glob);
            has_patterns = true;
        }
    }

    if !has_patterns {
        return None;
    }
    builder.build().ok()
}

// ---------------------------------------------------------------------------
// Convention-based setup file discovery
// ---------------------------------------------------------------------------

/// Filename for session-level setup in the project root: `shatter.setup.{ext}`.
const SESSION_SETUP_ROOT_PREFIX: &str = "shatter.setup";

/// Filename for session-level setup inside `.shatter/`: `setup.{ext}`.
const SESSION_SETUP_DIR_NAME: &str = "setup";

/// Infix marker for file-level setup files co-located with source:
/// `<stem>.shatter.setup.{ext}`.
const FILE_SETUP_INFIX: &str = ".shatter.setup.";

/// The `.shatter` directory name.
const SHATTER_DIR: &str = ".shatter";

/// Discovered setup files organized by lifecycle level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveredSetupFiles {
    /// Session-level setup file (at most one).
    pub session: Option<PathBuf>,
    /// File-level setup entries: source file path → co-located setup file path.
    pub file_level: HashMap<PathBuf, PathBuf>,
}

/// Config overrides that suppress convention-based discovery at specific levels.
#[derive(Debug, Clone, Default)]
pub struct SetupConfigOverride {
    /// If `Some`, use this path instead of convention-discovered session setup.
    /// Convention discovery for session level is skipped entirely.
    pub session_file: Option<PathBuf>,
    /// If `Some`, use these mappings instead of convention-discovered file-level setup.
    /// Keys are source file paths, values are setup file paths.
    pub file_level: Option<HashMap<PathBuf, PathBuf>>,
}

/// Discover setup files by convention, with optional config overrides.
///
/// Convention rules:
/// - **Session**: `{root}/shatter.setup.{ext}` (root-level takes precedence)
///   or `{root}/.shatter/setup.{ext}`.
/// - **File-level**: `{dir}/{stem}.shatter.setup.{ext}` co-located with each
///   source file in `source_files`.
///
/// When `config_override` specifies a value for a level, convention discovery
/// for that level is skipped and the override is used directly.
pub fn discover_setup_files(
    project_root: &Path,
    language_ext: &str,
    source_files: &[PathBuf],
    config_override: &SetupConfigOverride,
) -> DiscoveredSetupFiles {
    let session = if let Some(ref override_path) = config_override.session_file {
        Some(override_path.clone())
    } else {
        discover_session_setup(project_root, language_ext)
    };

    let file_level = if let Some(ref override_map) = config_override.file_level {
        override_map.clone()
    } else {
        discover_file_setup(language_ext, source_files)
    };

    DiscoveredSetupFiles {
        session,
        file_level,
    }
}

/// Look for a session-level setup file by convention.
/// Root-level `shatter.setup.{ext}` takes precedence over `.shatter/setup.{ext}`.
fn discover_session_setup(project_root: &Path, language_ext: &str) -> Option<PathBuf> {
    let root_candidate = project_root.join(format!("{SESSION_SETUP_ROOT_PREFIX}.{language_ext}"));
    if root_candidate.is_file() {
        return Some(root_candidate);
    }

    let dir_candidate = project_root
        .join(SHATTER_DIR)
        .join(format!("{SESSION_SETUP_DIR_NAME}.{language_ext}"));
    if dir_candidate.is_file() {
        return Some(dir_candidate);
    }

    None
}

/// Find file-level setup files co-located with source files.
fn discover_file_setup(language_ext: &str, source_files: &[PathBuf]) -> HashMap<PathBuf, PathBuf> {
    let mut result = HashMap::new();
    for source in source_files {
        let Some(stem) = source.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(parent) = source.parent() else {
            continue;
        };
        let setup_name = format!("{stem}{FILE_SETUP_INFIX}{language_ext}");
        let setup_path = parent.join(&setup_name);
        if setup_path.is_file() {
            result.insert(source.clone(), setup_path);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a file at the given path, creating parent directories as needed.
    fn create_file(base: &Path, relative: &str) {
        let path = base.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(&path, "// placeholder").expect("write file");
    }

    // ── suggest_corrected_include_pattern tests (str-94cg) ──

    #[test]
    fn suggest_strips_repo_relative_prefix_matching_scan_root_tail() {
        let root = Path::new("/home/u/proj/zolem/internal/runtime");
        assert_eq!(
            suggest_corrected_include_pattern("internal/runtime/*.go", root).as_deref(),
            Some("*.go"),
        );
    }

    #[test]
    fn suggest_strips_single_component_tail() {
        let root = Path::new("/x/y/runtime");
        assert_eq!(
            suggest_corrected_include_pattern("runtime/**/*.go", root).as_deref(),
            Some("**/*.go"),
        );
    }

    #[test]
    fn suggest_strips_absolute_scan_root_prefix() {
        let root = Path::new("/home/u/proj/zolem/internal/runtime");
        assert_eq!(
            suggest_corrected_include_pattern(
                "/home/u/proj/zolem/internal/runtime/*.go",
                root,
            )
            .as_deref(),
            Some("*.go"),
        );
    }

    #[test]
    fn suggest_returns_none_when_pattern_is_unrelated() {
        let root = Path::new("/x/y/runtime");
        assert_eq!(
            suggest_corrected_include_pattern("**/*.rs", root),
            None
        );
        assert_eq!(
            suggest_corrected_include_pattern("src/*.go", root),
            None
        );
    }

    #[test]
    fn suggest_returns_none_when_stripping_leaves_empty() {
        let root = Path::new("/x/runtime");
        // Pattern is exactly the prefix with no trailing fragment → no useful suggestion.
        assert_eq!(
            suggest_corrected_include_pattern("runtime/", root),
            None
        );
    }

    #[test]
    fn discovers_typescript_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/utils.tsx");
        create_file(dir.path(), "README.md");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"app.ts"));
        assert!(names.contains(&"utils.tsx"));
        assert!(!names.contains(&"README.md"));
        assert!(
            results
                .iter()
                .all(|(_, lang)| *lang == Language::TypeScript)
        );
    }

    #[test]
    fn discovers_go_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "pkg/handler.go");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, Language::Go);
    }

    #[test]
    fn discovers_rust_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/lib.rs");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, Language::Rust);
    }

    #[test]
    fn excludes_node_modules_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "node_modules/pkg/index.ts");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn excludes_vendor_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "main.go");
        create_file(dir.path(), "vendor/dep/dep.go");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn excludes_dist_and_target_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "dist/bundle.ts");
        create_file(dir.path(), "target/debug/main.rs");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn excludes_test_files_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/app.test.ts");
        create_file(dir.path(), "pkg/handler.go");
        create_file(dir.path(), "pkg/handler_test.go");
        create_file(dir.path(), "__tests__/integration.ts");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"app.ts"));
        assert!(names.contains(&"handler.go"));
        assert!(!names.contains(&"app.test.ts"));
        assert!(!names.contains(&"handler_test.go"));
        assert!(!names.contains(&"integration.ts"));
    }

    #[test]
    fn include_patterns_filter_results() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/handler.go");

        let options = DiscoveryOptions {
            include_patterns: vec!["**/*.ts".to_string()],
            ..Default::default()
        };
        let results = discover_files(dir.path(), &options).expect("discover");

        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    // ── config-pattern anchoring for subdirectory scan roots (str-1q12y) ──

    #[test]
    fn exclude_anchor_makes_project_root_pattern_match_subdir_scan() {
        // Repro of str-1q12y: a config exclude anchored at the project root
        // (`web/src/**/*.test.tsx`) must still exclude the test file when the
        // scan root is the `web/src` subdirectory.
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "web/src/features/widget.tsx");
        create_file(dir.path(), "web/src/features/widget.test.tsx");
        let scan_root = dir.path().join("web/src");

        // Without an anchor (old behavior) the project-root pattern misses.
        let unanchored = DiscoveryOptions {
            exclude_patterns: vec!["web/src/**/*.test.tsx".to_string()],
            ..Default::default()
        };
        let results = discover_files(&scan_root, &unanchored).expect("discover");
        assert_eq!(
            results.len(),
            2,
            "unanchored project-root pattern should not match scan-root-relative paths"
        );

        // With the exclude anchored at the project root, the pattern matches.
        let anchored = DiscoveryOptions {
            exclude_patterns: vec!["web/src/**/*.test.tsx".to_string()],
            exclude_anchor: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let results = discover_files(&scan_root, &anchored).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["widget.tsx"],
            "anchored exclude should drop the test file"
        );
    }

    #[test]
    fn include_anchor_makes_project_root_pattern_match_subdir_scan() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "web/src/features/widget.tsx");
        create_file(dir.path(), "web/src/features/other.ts");
        let scan_root = dir.path().join("web/src");

        let anchored = DiscoveryOptions {
            include_patterns: vec!["web/src/**/*.tsx".to_string()],
            include_anchor: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let results = discover_files(&scan_root, &anchored).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["widget.tsx"], "anchored include keeps only .tsx");
    }

    #[test]
    fn exclude_anchor_prunes_project_root_directory_pattern() {
        // Directory-level pruning must also honor the anchor.
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "web/src/app.tsx");
        create_file(dir.path(), "web/src/vendor/lib.tsx");
        let scan_root = dir.path().join("web/src");

        let anchored = DiscoveryOptions {
            exclude_patterns: vec!["web/src/vendor/**".to_string()],
            exclude_anchor: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let results = discover_files(&scan_root, &anchored).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["app.tsx"],
            "anchored dir exclude should prune vendor/"
        );
    }

    #[test]
    fn cli_relative_pattern_unaffected_when_no_anchor() {
        // CLI flags stay scan-root-relative: `**/*.test.tsx` still works and
        // no anchor is set.
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "web/src/features/widget.tsx");
        create_file(dir.path(), "web/src/features/widget.test.tsx");
        let scan_root = dir.path().join("web/src");

        let options = DiscoveryOptions {
            exclude_patterns: vec!["**/*.test.tsx".to_string()],
            ..Default::default()
        };
        let results = discover_files(&scan_root, &options).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["widget.tsx"]);
    }

    #[test]
    fn filter_file_list_honors_exclude_anchor() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "web/src/features/widget.tsx");
        create_file(dir.path(), "web/src/features/widget.test.tsx");
        let scan_root = dir.path().join("web/src");
        let files = vec![
            scan_root.join("features/widget.tsx"),
            scan_root.join("features/widget.test.tsx"),
        ];

        let anchored = DiscoveryOptions {
            exclude_patterns: vec!["web/src/**/*.test.tsx".to_string()],
            exclude_anchor: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let results = filter_file_list(&scan_root, files, &anchored).expect("filter");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["widget.tsx"]);
    }

    #[test]
    fn exclude_patterns_remove_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/generated.ts");

        let options = DiscoveryOptions {
            exclude_patterns: vec!["**/generated.*".to_string()],
            ..Default::default()
        };
        let results = discover_files(dir.path(), &options).expect("discover");

        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn respects_gitignore() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/secret.ts");
        fs::write(dir.path().join(".gitignore"), "secret.ts\n").expect("write gitignore");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn respects_shatterignore() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/legacy.ts");
        fs::write(dir.path().join(".shatterignore"), "legacy.ts\n").expect("write shatterignore");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn gitignore_disabled_when_respect_gitignore_is_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/secret.ts");
        fs::write(dir.path().join(".gitignore"), "secret.ts\n").expect("write gitignore");

        let options = DiscoveryOptions {
            respect_gitignore: false,
            ..Default::default()
        };
        let results = discover_files(dir.path(), &options).expect("discover");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn language_from_extension_all_variants() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("py"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn empty_directory_returns_empty_vec() {
        let dir = tempfile::tempdir().expect("tempdir");
        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        assert!(results.is_empty());
    }

    #[test]
    fn invalid_glob_pattern_returns_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let options = DiscoveryOptions {
            include_patterns: vec!["[invalid".to_string()],
            ..Default::default()
        };
        let result = discover_files(dir.path(), &options);
        assert!(result.is_err());
    }

    #[test]
    fn results_are_sorted_by_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "z.ts");
        create_file(dir.path(), "a.ts");
        create_file(dir.path(), "m.ts");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.ts", "m.ts", "z.ts"]);
    }

    // --- filter_file_list tests ---

    #[test]
    fn filter_keeps_source_files_and_detects_language() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/handler.go");
        create_file(dir.path(), "README.md");

        let files = vec![
            dir.path().join("src/app.ts"),
            dir.path().join("src/handler.go"),
            dir.path().join("README.md"),
        ];

        let results =
            filter_file_list(dir.path(), files, &DiscoveryOptions::default()).expect("filter");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|(_, l)| *l == Language::TypeScript));
        assert!(results.iter().any(|(_, l)| *l == Language::Go));
    }

    #[test]
    fn filter_applies_default_excludes() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "node_modules/pkg/index.ts");

        let files = vec![
            dir.path().join("src/app.ts"),
            dir.path().join("node_modules/pkg/index.ts"),
        ];

        let results =
            filter_file_list(dir.path(), files, &DiscoveryOptions::default()).expect("filter");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn filter_applies_user_excludes() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/generated.ts");

        let files = vec![
            dir.path().join("src/app.ts"),
            dir.path().join("src/generated.ts"),
        ];

        let options = DiscoveryOptions {
            exclude_patterns: vec!["**/generated.*".to_string()],
            ..Default::default()
        };
        let results = filter_file_list(dir.path(), files, &options).expect("filter");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn filter_applies_user_includes() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/handler.go");

        let files = vec![
            dir.path().join("src/app.ts"),
            dir.path().join("src/handler.go"),
        ];

        let options = DiscoveryOptions {
            include_patterns: vec!["**/*.ts".to_string()],
            ..Default::default()
        };
        let results = filter_file_list(dir.path(), files, &options).expect("filter");
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("app.ts"));
    }

    #[test]
    fn filter_skips_paths_outside_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");

        let files = vec![
            dir.path().join("src/app.ts"),
            PathBuf::from("/somewhere/else/foo.ts"),
        ];

        let results =
            filter_file_list(dir.path(), files, &DiscoveryOptions::default()).expect("filter");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn filter_results_are_sorted() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "z.ts");
        create_file(dir.path(), "a.ts");
        create_file(dir.path(), "m.ts");

        let files = vec![
            dir.path().join("z.ts"),
            dir.path().join("a.ts"),
            dir.path().join("m.ts"),
        ];

        let results =
            filter_file_list(dir.path(), files, &DiscoveryOptions::default()).expect("filter");
        let names: Vec<_> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.ts", "m.ts", "z.ts"]);
    }

    #[test]
    fn filter_empty_list_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let results =
            filter_file_list(dir.path(), vec![], &DiscoveryOptions::default()).expect("filter");
        assert!(results.is_empty());
    }

    // --- setup file discovery tests ---

    #[test]
    fn setup_discovers_session_in_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "shatter.setup.ts");

        let result = discover_setup_files(dir.path(), "ts", &[], &SetupConfigOverride::default());
        assert_eq!(result.session, Some(dir.path().join("shatter.setup.ts")));
        assert!(result.file_level.is_empty());
    }

    #[test]
    fn setup_discovers_session_in_shatter_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), ".shatter/setup.ts");

        let result = discover_setup_files(dir.path(), "ts", &[], &SetupConfigOverride::default());
        assert_eq!(result.session, Some(dir.path().join(".shatter/setup.ts")));
    }

    #[test]
    fn setup_root_takes_precedence_over_shatter_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "shatter.setup.ts");
        create_file(dir.path(), ".shatter/setup.ts");

        let result = discover_setup_files(dir.path(), "ts", &[], &SetupConfigOverride::default());
        assert_eq!(result.session, Some(dir.path().join("shatter.setup.ts")));
    }

    #[test]
    fn setup_discovers_file_level_colocated() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/auth.ts");
        create_file(dir.path(), "src/auth.shatter.setup.ts");

        let sources = vec![dir.path().join("src/auth.ts")];
        let result =
            discover_setup_files(dir.path(), "ts", &sources, &SetupConfigOverride::default());
        assert!(result.session.is_none());
        assert_eq!(result.file_level.len(), 1);
        assert_eq!(
            result.file_level[&dir.path().join("src/auth.ts")],
            dir.path().join("src/auth.shatter.setup.ts")
        );
    }

    #[test]
    fn setup_config_overrides_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Convention file exists but should be ignored
        create_file(dir.path(), "shatter.setup.ts");

        let override_path = dir.path().join("custom/my-setup.ts");
        let result = discover_setup_files(
            dir.path(),
            "ts",
            &[],
            &SetupConfigOverride {
                session_file: Some(override_path.clone()),
                ..Default::default()
            },
        );
        assert_eq!(result.session, Some(override_path));
    }

    #[test]
    fn setup_config_overrides_file_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/auth.ts");
        // Convention file exists but should be ignored
        create_file(dir.path(), "src/auth.shatter.setup.ts");

        let mut file_overrides = HashMap::new();
        let custom = dir.path().join("setups/auth-setup.ts");
        file_overrides.insert(dir.path().join("src/auth.ts"), custom.clone());

        let result = discover_setup_files(
            dir.path(),
            "ts",
            &[dir.path().join("src/auth.ts")],
            &SetupConfigOverride {
                file_level: Some(file_overrides),
                ..Default::default()
            },
        );
        assert_eq!(result.file_level[&dir.path().join("src/auth.ts")], custom);
    }

    #[test]
    fn setup_no_files_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");

        let result = discover_setup_files(dir.path(), "ts", &[], &SetupConfigOverride::default());
        assert_eq!(result, DiscoveredSetupFiles::default());
    }

    #[test]
    fn setup_multiple_levels_simultaneously() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "shatter.setup.go");
        create_file(dir.path(), "pkg/handler.go");
        create_file(dir.path(), "pkg/handler.shatter.setup.go");

        let sources = vec![dir.path().join("pkg/handler.go")];
        let result =
            discover_setup_files(dir.path(), "go", &sources, &SetupConfigOverride::default());
        assert_eq!(result.session, Some(dir.path().join("shatter.setup.go")));
        assert_eq!(result.file_level.len(), 1);
        assert_eq!(
            result.file_level[&dir.path().join("pkg/handler.go")],
            dir.path().join("pkg/handler.shatter.setup.go")
        );
    }

    #[test]
    fn setup_ignores_source_without_colocated_setup() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/auth.ts");
        create_file(dir.path(), "src/db.ts");
        create_file(dir.path(), "src/auth.shatter.setup.ts");
        // No setup for db.ts

        let sources = vec![dir.path().join("src/auth.ts"), dir.path().join("src/db.ts")];
        let result =
            discover_setup_files(dir.path(), "ts", &sources, &SetupConfigOverride::default());
        assert_eq!(result.file_level.len(), 1);
        assert!(
            result
                .file_level
                .contains_key(&dir.path().join("src/auth.ts"))
        );
        assert!(
            !result
                .file_level
                .contains_key(&dir.path().join("src/db.ts"))
        );
    }

    #[test]
    fn setup_wrong_extension_not_discovered() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Setup file is .ts but we're looking for .go
        create_file(dir.path(), "shatter.setup.ts");

        let result = discover_setup_files(dir.path(), "go", &[], &SetupConfigOverride::default());
        assert!(result.session.is_none());
    }

    #[test]
    fn setup_empty_sources_returns_empty_file_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "shatter.setup.ts");

        let result = discover_setup_files(dir.path(), "ts", &[], &SetupConfigOverride::default());
        assert!(result.session.is_some());
        assert!(result.file_level.is_empty());
    }
}
