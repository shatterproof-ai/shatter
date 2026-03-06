//! File discovery with glob/ignore support.
//!
//! Walks a directory tree to find source files for analysis, respecting
//! `.gitignore`, `.shatterignore`, and user-specified include/exclude patterns.

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
        &DEFAULT_EXCLUDES.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
    )?;

    let ignore_matcher = if options.respect_gitignore {
        load_ignore_file(&root.join(".gitignore"))
    } else {
        None
    };
    let shatter_ignore_matcher = load_ignore_file(&root.join(".shatterignore"));

    let mut results = Vec::new();
    walk_dir(root, root, 0, &WalkConfig {
        include_set: &include_set,
        exclude_set: &exclude_set,
        default_exclude_set: &default_exclude_set,
        ignore_matcher: ignore_matcher.as_ref(),
        shatter_ignore_matcher: shatter_ignore_matcher.as_ref(),
        max_depth: options.max_depth,
    }, &mut results)?;

    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

struct WalkConfig<'a> {
    include_set: &'a Option<GlobSet>,
    exclude_set: &'a Option<GlobSet>,
    default_exclude_set: &'a Option<GlobSet>,
    ignore_matcher: Option<&'a GlobSet>,
    shatter_ignore_matcher: Option<&'a GlobSet>,
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
            if let Some(set) = config.exclude_set
                && set.is_match(&dir_pattern_path)
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

        // Check user excludes
        if let Some(set) = config.exclude_set
            && set.is_match(relative)
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

        // Check user includes (if specified, file must match at least one)
        if let Some(set) = config.include_set
            && !set.is_match(relative)
        {
            continue;
        }

        results.push((path, language));
    }

    Ok(())
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
    let set = builder.build().map_err(|e| DiscoveryError::InvalidPattern {
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

    #[test]
    fn discovers_typescript_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        create_file(dir.path(), "src/app.ts");
        create_file(dir.path(), "src/utils.tsx");
        create_file(dir.path(), "README.md");

        let results = discover_files(dir.path(), &DiscoveryOptions::default()).expect("discover");
        let names: Vec<_> = results.iter().map(|(p, _)| p.file_name().unwrap().to_str().unwrap()).collect();

        assert!(names.contains(&"app.ts"));
        assert!(names.contains(&"utils.tsx"));
        assert!(!names.contains(&"README.md"));
        assert!(results.iter().all(|(_, lang)| *lang == Language::TypeScript));
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
        let names: Vec<_> = results.iter().map(|(p, _)| p.file_name().unwrap().to_str().unwrap()).collect();

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
        let names: Vec<_> = results.iter().map(|(p, _)| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["a.ts", "m.ts", "z.ts"]);
    }
}
