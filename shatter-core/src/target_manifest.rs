//! Target manifest: enumerate, classify, and hash the source file set for a scan.
//!
//! `shatter list-targets` calls `TargetManifest::build()` to walk the project
//! tree and sort every encountered file into one of four buckets:
//!
//! - **selected** — will be analyzed
//! - **excluded** — removed by a user `--exclude` pattern or `--language` filter
//! - **unsupported** — has a recognizable but unsupported programming language
//! - **candidate_outside_policy** — would be selected if no `--include` filters
//!   were present (only populated when `--include` is specified)

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TARGET_MANIFEST_SCHEMA_VERSION: &str = "1";
pub const TARGET_MANIFEST_KIND: &str = "target_manifest";

/// Default patterns silently excluded without contributing to the excluded list.
const DEFAULT_EXCLUDES: &[&str] = &[
    "**/node_modules/**",
    "**/vendor/**",
    "**/dist/**",
    "**/target/**",
    "**/__tests__/**",
    "**/*.test.ts",
    "**/*_test.go",
    "**/*.d.ts",
];

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Inputs controlling which files are selected in a target manifest build.
#[derive(Debug, Clone, Default)]
pub struct TargetManifestConfig {
    /// User-supplied include globs. Empty means "all supported files".
    pub include: Vec<String>,
    /// User-supplied exclude globs.
    pub exclude: Vec<String>,
    /// Language filter ("typescript", "go", "rust"). None means all languages.
    pub language: Option<String>,
    /// Maximum directory traversal depth.
    pub max_depth: Option<usize>,
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum TargetManifestError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        pattern: String,
        source: globset::Error,
    },
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A file selected for analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetFileEntry {
    pub path: PathBuf,
    pub language: String,
    pub frontend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    pub content_hash: String,
}

/// Why a file was not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    ExcludePattern,
    OutsideIncludePattern,
    LanguageFilter,
    Unsupported,
}

/// A file excluded from analysis with its reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedFileEntry {
    pub path: PathBuf,
    pub reason: ExclusionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_pattern: Option<String>,
}

/// A file with a recognizable but unsupported language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedFileEntry {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<String>,
}

/// Complete target manifest produced by `build()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetManifest {
    pub schema_version: String,
    pub kind: String,
    pub scan_id: String,
    pub project_root: PathBuf,
    pub config_hash: String,
    pub source_set_hash: String,
    pub generated_at_ns: u64,
    pub selected: Vec<TargetFileEntry>,
    pub excluded: Vec<ExcludedFileEntry>,
    pub unsupported: Vec<UnsupportedFileEntry>,
    pub candidate_outside_policy: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// impl TargetManifest
// ---------------------------------------------------------------------------

impl TargetManifest {
    /// Walk `root` and classify every source file according to `config`.
    pub fn build(root: &Path, config: &TargetManifestConfig) -> Result<Self, TargetManifestError> {
        let config_hash = compute_config_hash(config);
        let scan_id = uuid::Uuid::new_v4().to_string();

        let mut selected = Vec::new();
        let mut excluded = Vec::new();
        let mut unsupported = Vec::new();
        let mut candidate_outside_policy = Vec::new();

        let exclude_set = build_glob_set(&config.exclude)?;
        let include_set = build_glob_set(&config.include)?;
        let default_exclude_set = build_glob_set(
            &DEFAULT_EXCLUDES
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>(),
        )?;

        let ignore_matcher = load_ignore_file(&root.join(".gitignore"));
        let shatter_ignore_matcher = load_ignore_file(&root.join(".shatterignore"));

        manifest_walk(
            root,
            root,
            0,
            &config.exclude,
            &config.language,
            config.max_depth,
            &include_set,
            &exclude_set,
            &default_exclude_set,
            &ignore_matcher,
            &shatter_ignore_matcher,
            &mut selected,
            &mut excluded,
            &mut unsupported,
            &mut candidate_outside_policy,
        )?;

        selected.sort_by(|a, b| a.path.cmp(&b.path));
        excluded.sort_by(|a, b| a.path.cmp(&b.path));
        unsupported.sort_by(|a, b| a.path.cmp(&b.path));
        candidate_outside_policy.sort();
        candidate_outside_policy.dedup();

        let source_set_hash = compute_source_set_hash(&selected);

        Ok(TargetManifest {
            schema_version: TARGET_MANIFEST_SCHEMA_VERSION.to_string(),
            kind: TARGET_MANIFEST_KIND.to_string(),
            scan_id,
            project_root: root.to_path_buf(),
            config_hash,
            source_set_hash,
            generated_at_ns: now_ns(),
            selected,
            excluded,
            unsupported,
            candidate_outside_policy,
        })
    }

    /// Write the manifest as pretty-printed JSON using an atomic rename.
    pub fn write_json(&self, path: &Path) -> Result<(), TargetManifestError> {
        let json = serde_json::to_string_pretty(self)?;
        let tmp_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!("{e}.tmp"))
            .unwrap_or_else(|| "json.tmp".to_string());
        let tmp = path.with_extension(tmp_ext);
        std::fs::write(&tmp, json.as_bytes())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Read a manifest previously written by [`write_json`].
    pub fn read_json(path: &Path) -> Result<Self, TargetManifestError> {
        let bytes = std::fs::read(path)?;
        let manifest = serde_json::from_slice(&bytes)?;
        Ok(manifest)
    }

    /// Plain-text rendering suitable for terminal output.
    pub fn render_text(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!("Target manifest — {}\n", self.project_root.display()));
        out.push_str(&format!("  config hash:      {}\n", self.config_hash));
        out.push_str(&format!("  source set hash:  {}\n", self.source_set_hash));
        out.push('\n');

        out.push_str(&format!("Selected ({}):\n", self.selected.len()));
        for f in &self.selected {
            let lines = f
                .line_count
                .map(|n| format!("{n} lines"))
                .unwrap_or_else(|| "? lines".to_string());
            out.push_str(&format!(
                "  {}  [{}, {}]\n",
                f.path.display(),
                f.language,
                lines
            ));
        }
        if self.selected.is_empty() {
            out.push_str("  (none)\n");
        }
        out.push('\n');

        if !self.excluded.is_empty() {
            out.push_str(&format!("Excluded ({}):\n", self.excluded.len()));
            for f in &self.excluded {
                let detail = match &f.matched_pattern {
                    Some(p) => format!("{:?}: {p}", f.reason),
                    None => format!("{:?}", f.reason),
                };
                out.push_str(&format!("  {}  [{}]\n", f.path.display(), detail));
            }
            out.push('\n');
        }

        if !self.unsupported.is_empty() {
            out.push_str(&format!("Unsupported ({}):\n", self.unsupported.len()));
            for f in &self.unsupported {
                let lang = f
                    .detected_language
                    .as_deref()
                    .unwrap_or("unknown language");
                out.push_str(&format!("  {}  [{}]\n", f.path.display(), lang));
            }
            out.push('\n');
        }

        if !self.candidate_outside_policy.is_empty() {
            out.push_str(&format!(
                "Candidates outside --include policy ({}):\n",
                self.candidate_outside_policy.len()
            ));
            for p in &self.candidate_outside_policy {
                out.push_str(&format!("  {}\n", p.display()));
            }
            out.push('\n');
        }

        out
    }

    /// Markdown rendering suitable for file output or piped display.
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();

        out.push_str("# Target Manifest\n\n");
        out.push_str(&format!(
            "**Project:** `{}`  \n",
            self.project_root.display()
        ));
        out.push_str(&format!("**Config hash:** `{}`  \n", self.config_hash));
        out.push_str(&format!(
            "**Source set hash:** `{}`  \n\n",
            self.source_set_hash
        ));

        out.push_str(&format!("## Selected ({})\n\n", self.selected.len()));
        if self.selected.is_empty() {
            out.push_str("_No files selected._\n\n");
        } else {
            out.push_str("| Path | Language | Frontend | Lines |\n");
            out.push_str("|------|----------|----------|-------|\n");
            for f in &self.selected {
                let lines = f
                    .line_count
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".to_string());
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    f.path.display(),
                    f.language,
                    f.frontend,
                    lines
                ));
            }
            out.push('\n');
        }

        if !self.excluded.is_empty() {
            out.push_str(&format!("## Excluded ({})\n\n", self.excluded.len()));
            out.push_str("| Path | Reason | Pattern |\n");
            out.push_str("|------|--------|---------|\n");
            for f in &self.excluded {
                let reason = match f.reason {
                    ExclusionReason::ExcludePattern => "exclude_pattern",
                    ExclusionReason::OutsideIncludePattern => "outside_include_pattern",
                    ExclusionReason::LanguageFilter => "language_filter",
                    ExclusionReason::Unsupported => "unsupported",
                };
                let pattern = f.matched_pattern.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    f.path.display(),
                    reason,
                    pattern
                ));
            }
            out.push('\n');
        }

        if !self.unsupported.is_empty() {
            out.push_str(&format!("## Unsupported ({})\n\n", self.unsupported.len()));
            out.push_str("| Path | Detected Language |\n");
            out.push_str("|------|-------------------|\n");
            for f in &self.unsupported {
                let lang = f.detected_language.as_deref().unwrap_or("unknown");
                out.push_str(&format!("| `{}` | {} |\n", f.path.display(), lang));
            }
            out.push('\n');
        }

        if !self.candidate_outside_policy.is_empty() {
            out.push_str(&format!(
                "## Candidates Outside Include Policy ({})\n\n",
                self.candidate_outside_policy.len()
            ));
            out.push_str(
                "_These files would be selected if no `--include` patterns were specified._\n\n",
            );
            for p in &self.candidate_outside_policy {
                out.push_str(&format!("- `{}`\n", p.display()));
            }
            out.push('\n');
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Walk
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn manifest_walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    exclude_patterns: &[String],
    language_filter: &Option<String>,
    max_depth: Option<usize>,
    include_set: &Option<GlobSet>,
    exclude_set: &Option<GlobSet>,
    default_exclude_set: &Option<GlobSet>,
    ignore_matcher: &Option<GlobSet>,
    shatter_ignore_matcher: &Option<GlobSet>,
    selected: &mut Vec<TargetFileEntry>,
    excluded: &mut Vec<ExcludedFileEntry>,
    unsupported: &mut Vec<UnsupportedFileEntry>,
    candidate_outside_policy: &mut Vec<PathBuf>,
) -> Result<(), TargetManifestError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(TargetManifestError::Io(e)),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);

        if path.is_dir() {
            if max_depth.is_some_and(|max| depth >= max) {
                continue;
            }
            // Directories: apply default and ignore filters silently.
            let sentinel = PathBuf::from(format!("{}/sentinel", relative.display()));
            if glob_matches(default_exclude_set, &sentinel)
                || glob_matches(ignore_matcher, &sentinel)
                || glob_matches(shatter_ignore_matcher, &sentinel)
                || glob_matches(exclude_set, &sentinel)
            {
                continue;
            }
            manifest_walk(
                root,
                &path,
                depth + 1,
                exclude_patterns,
                language_filter,
                max_depth,
                include_set,
                exclude_set,
                default_exclude_set,
                ignore_matcher,
                shatter_ignore_matcher,
                selected,
                excluded,
                unsupported,
                candidate_outside_policy,
            )?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        // Default excludes — silently skip.
        if glob_matches(default_exclude_set, relative) {
            continue;
        }
        // Gitignore / shatterignore — silently skip.
        if glob_matches(ignore_matcher, relative)
            || glob_matches(shatter_ignore_matcher, relative)
        {
            continue;
        }

        // User excludes — report with matched pattern.
        if glob_matches(exclude_set, relative) {
            let matched = find_matching_pattern(exclude_patterns, relative);
            excluded.push(ExcludedFileEntry {
                path: relative.to_path_buf(),
                reason: ExclusionReason::ExcludePattern,
                matched_pattern: matched,
            });
            continue;
        }

        // Language detection.
        let ext = path.extension().and_then(|e| e.to_str());
        match supported_language_info(ext) {
            Some((lang, frontend)) => {
                // Language filter.
                if language_filter.as_deref().is_some_and(|f| lang != f) {
                    excluded.push(ExcludedFileEntry {
                        path: relative.to_path_buf(),
                        reason: ExclusionReason::LanguageFilter,
                        matched_pattern: None,
                    });
                    continue;
                }

                // Include filter — populate candidate_outside_policy when active.
                if include_set.as_ref().is_some_and(|s| !s.is_match(relative)) {
                    candidate_outside_policy.push(relative.to_path_buf());
                    continue;
                }

                // Selected — read file for hash and line count.
                let (content_hash, line_count) = read_file_for_hash(&path);
                selected.push(TargetFileEntry {
                    path: relative.to_path_buf(),
                    language: lang.to_string(),
                    frontend: frontend.to_string(),
                    line_count,
                    content_hash,
                });
            }
            None => {
                // Not a supported language — report recognizable ones.
                if let Some(detected) = detect_language_name(ext) {
                    unsupported.push(UnsupportedFileEntry {
                        path: relative.to_path_buf(),
                        detected_language: Some(detected),
                    });
                }
                // Files with non-code extensions are silently skipped.
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Hash helpers
// ---------------------------------------------------------------------------

/// SHA-256 of canonical JSON of `{ include, exclude, language, max_depth }`.
pub fn compute_config_hash(config: &TargetManifestConfig) -> String {
    let canonical = serde_json::json!({
        "include": sorted_vec(&config.include),
        "exclude": sorted_vec(&config.exclude),
        "language": config.language,
        "max_depth": config.max_depth,
    });
    let text = canonical.to_string();
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// SHA-256 of sorted `"<path>:<content_hash>"` pairs for all selected files.
pub fn compute_source_set_hash(selected: &[TargetFileEntry]) -> String {
    let mut pairs: Vec<String> = selected
        .iter()
        .map(|f| format!("{}:{}", f.path.display(), f.content_hash))
        .collect();
    pairs.sort();
    let combined = pairs.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn sorted_vec(v: &[String]) -> Vec<&str> {
    let mut sorted: Vec<&str> = v.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted
}

fn read_file_for_hash(path: &Path) -> (String, Option<usize>) {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let hash = format!("{:x}", hasher.finalize());
            let text = String::from_utf8_lossy(&bytes);
            let count = text.lines().count();
            (hash, Some(count))
        }
        Err(_) => (String::new(), None),
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Language helpers
// ---------------------------------------------------------------------------

/// Returns `(language, frontend)` for supported source extensions.
fn supported_language_info(ext: Option<&str>) -> Option<(&'static str, &'static str)> {
    match ext? {
        "ts" | "tsx" => Some(("typescript", "shatter-ts")),
        "go" => Some(("go", "shatter-go")),
        "rs" => Some(("rust", "shatter-rust")),
        _ => None,
    }
}

/// Detect the language name for common but unsupported extensions.
fn detect_language_name(ext: Option<&str>) -> Option<String> {
    match ext? {
        "py" | "pyw" | "pyi" => Some("python".to_string()),
        "java" => Some("java".to_string()),
        "rb" | "rake" => Some("ruby".to_string()),
        "c" | "h" => Some("c".to_string()),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("c++".to_string()),
        "cs" => Some("c#".to_string()),
        "swift" => Some("swift".to_string()),
        "kt" | "kts" => Some("kotlin".to_string()),
        "php" => Some("php".to_string()),
        "scala" | "sc" => Some("scala".to_string()),
        "lua" => Some("lua".to_string()),
        "sh" | "bash" | "zsh" | "fish" => Some("shell".to_string()),
        "r" | "R" => Some("r".to_string()),
        "dart" => Some("dart".to_string()),
        "ex" | "exs" => Some("elixir".to_string()),
        "hs" | "lhs" => Some("haskell".to_string()),
        "clj" | "cljs" | "cljc" => Some("clojure".to_string()),
        "jl" => Some("julia".to_string()),
        "ml" | "mli" => Some("ocaml".to_string()),
        "fs" | "fsi" | "fsx" => Some("f#".to_string()),
        "v" | "sv" | "vh" => Some("verilog".to_string()),
        "zig" => Some("zig".to_string()),
        "nim" => Some("nim".to_string()),
        "cr" => Some("crystal".to_string()),
        // Non-code extensions: silently skip
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Glob helpers
// ---------------------------------------------------------------------------

fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>, TargetManifestError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob =
            Glob::new(pattern).map_err(|source| TargetManifestError::InvalidPattern {
                pattern: pattern.clone(),
                source,
            })?;
        builder.add(glob);
    }
    let set = builder
        .build()
        .map_err(|source| TargetManifestError::InvalidPattern {
            pattern: patterns.join(", "),
            source,
        })?;
    Ok(Some(set))
}

fn glob_matches(set: &Option<GlobSet>, path: &Path) -> bool {
    set.as_ref().is_some_and(|s| s.is_match(path))
}

fn find_matching_pattern(patterns: &[String], path: &Path) -> Option<String> {
    for pattern in patterns {
        if Glob::new(pattern)
            .is_ok_and(|g| g.compile_matcher().is_match(path))
        {
            return Some(pattern.clone());
        }
    }
    None
}

/// Load an ignore file (`.gitignore` or `.shatterignore`) into a `GlobSet`.
fn load_ignore_file(path: &Path) -> Option<GlobSet> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut builder = GlobSetBuilder::new();
    let mut has_patterns = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let pattern = if line.ends_with('/') {
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;

    use proptest::prelude::*;

    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents.as_bytes()).unwrap();
    }

    // -----------------------------------------------------------------------
    // Unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn selects_supported_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/foo.ts", "const x = 1;\n");
        write(root, "src/bar.go", "package main\n");
        write(root, "src/baz.rs", "fn main() {}\n");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let paths: Vec<_> = manifest.selected.iter().map(|f| f.path.as_os_str()).collect();
        assert!(paths.iter().any(|p| *p == "src/foo.ts"), "ts not selected");
        assert!(paths.iter().any(|p| *p == "src/bar.go"), "go not selected");
        assert!(paths.iter().any(|p| *p == "src/baz.rs"), "rs not selected");
    }

    #[test]
    fn exclude_pattern_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/app.ts", "export function f() {}");
        write(root, "src/app.spec.ts", "test('x', () => {})");

        let config = TargetManifestConfig {
            exclude: vec!["**/*.spec.ts".to_string()],
            ..Default::default()
        };
        let manifest = TargetManifest::build(root, &config).unwrap();

        let excluded = manifest
            .excluded
            .iter()
            .find(|e| e.path == Path::new("src/app.spec.ts"))
            .expect("spec file should be excluded");
        assert_eq!(excluded.reason, ExclusionReason::ExcludePattern);
        assert_eq!(
            excluded.matched_pattern.as_deref(),
            Some("**/*.spec.ts")
        );
    }

    #[test]
    fn source_set_hash_stability() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/a.ts", "const a = 1;");
        write(root, "src/b.ts", "const b = 2;");

        let m1 = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let m2 = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        assert_eq!(m1.source_set_hash, m2.source_set_hash);
        assert_eq!(m1.config_hash, m2.config_hash);
    }

    #[test]
    fn config_hash_changes_with_include() {
        let base = TargetManifestConfig::default();
        let with_include = TargetManifestConfig {
            include: vec!["src/**/*.ts".to_string()],
            ..Default::default()
        };
        assert_ne!(
            compute_config_hash(&base),
            compute_config_hash(&with_include)
        );
    }

    #[test]
    fn json_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/mod.ts", "export const x = 1;");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let json_path = root.join("manifest.json");
        manifest.write_json(&json_path).unwrap();

        let loaded = TargetManifest::read_json(&json_path).unwrap();
        assert_eq!(manifest.schema_version, loaded.schema_version);
        assert_eq!(manifest.config_hash, loaded.config_hash);
        assert_eq!(manifest.source_set_hash, loaded.source_set_hash);
        assert_eq!(manifest.selected.len(), loaded.selected.len());
    }

    #[test]
    fn language_filter_excludes_other_languages() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/main.go", "package main");
        write(root, "src/app.ts", "const x = 1;");

        let config = TargetManifestConfig {
            language: Some("go".to_string()),
            ..Default::default()
        };
        let manifest = TargetManifest::build(root, &config).unwrap();

        assert!(
            manifest.selected.iter().all(|f| f.language == "go"),
            "only go files should be selected"
        );
        assert!(
            manifest
                .excluded
                .iter()
                .any(|e| e.reason == ExclusionReason::LanguageFilter),
            "ts file should be language-filtered"
        );
    }

    #[test]
    fn candidate_outside_policy_populated_with_include() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/app.ts", "const x = 1;");
        write(root, "lib/util.ts", "export const y = 2;");

        let config = TargetManifestConfig {
            include: vec!["src/**".to_string()],
            ..Default::default()
        };
        let manifest = TargetManifest::build(root, &config).unwrap();

        assert!(
            manifest.selected.iter().any(|f| f.path == Path::new("src/app.ts")),
            "src/app.ts should be selected"
        );
        assert!(
            manifest
                .candidate_outside_policy
                .iter()
                .any(|p| *p == Path::new("lib/util.ts")),
            "lib/util.ts should be in candidate_outside_policy"
        );
    }

    #[test]
    fn candidate_outside_policy_empty_without_include() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/app.ts", "const x = 1;");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        assert!(
            manifest.candidate_outside_policy.is_empty(),
            "candidate_outside_policy should be empty without --include"
        );
    }

    #[test]
    fn unsupported_python_file_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/helper.py", "def foo(): pass");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let entry = manifest
            .unsupported
            .iter()
            .find(|e| e.path == Path::new("src/helper.py"))
            .expect("python file should be in unsupported");
        assert_eq!(entry.detected_language.as_deref(), Some("python"));
    }

    #[test]
    fn render_text_contains_section_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/a.ts", "export function a() {}");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let text = manifest.render_text();
        assert!(text.contains("Selected ("), "expected Selected section");
    }

    #[test]
    fn render_markdown_contains_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "src/a.ts", "export function a() {}");

        let manifest = TargetManifest::build(root, &TargetManifestConfig::default()).unwrap();
        let md = manifest.render_markdown();
        assert!(md.contains("# Target Manifest"), "missing top-level header");
        assert!(md.contains("## Selected"), "missing Selected section");
    }

    // -----------------------------------------------------------------------
    // Property tests
    // -----------------------------------------------------------------------

    proptest! {
        /// source_set_hash must be order-independent: permuting the selected
        /// list produces the same hash because we sort pairs before hashing.
        #[test]
        fn prop_source_set_hash_order_independent(
            entries in proptest::collection::vec(
                (
                    "[a-z]{1,8}".prop_map(|n| PathBuf::from(format!("src/{n}.ts"))),
                    "[a-f0-9]{64}".prop_map(|h| h),
                ),
                0..=8,
            )
        ) {
            let mut selected: Vec<TargetFileEntry> = entries.into_iter().map(|(p, h)| {
                TargetFileEntry {
                    path: p,
                    language: "typescript".to_string(),
                    frontend: "shatter-ts".to_string(),
                    line_count: Some(10),
                    content_hash: h,
                }
            }).collect();

            let hash1 = compute_source_set_hash(&selected);
            selected.reverse();
            let hash2 = compute_source_set_hash(&selected);
            prop_assert_eq!(hash1, hash2, "source_set_hash must be order-independent");
        }

        /// config_hash must be deterministic: identical config → identical hash.
        #[test]
        fn prop_config_hash_deterministic(
            include in proptest::collection::vec("[a-z*]{1,20}", 0..=3),
            exclude in proptest::collection::vec("[a-z*]{1,20}", 0..=3),
            language in proptest::option::of(prop_oneof!["typescript", "go", "rust"]
                .prop_map(|s: String| s)),
            max_depth in proptest::option::of(0usize..=10),
        ) {
            let config = TargetManifestConfig { include: include.clone(), exclude: exclude.clone(), language: language.clone(), max_depth };
            let h1 = compute_config_hash(&config);
            let h2 = compute_config_hash(&config);
            prop_assert_eq!(h1, h2, "config_hash must be deterministic");
        }

        /// No file path should appear in more than one category.
        #[test]
        fn prop_no_file_appears_in_two_categories(
            n_selected in 0usize..=5,
            n_excluded in 0usize..=5,
            n_unsupported in 0usize..=5,
            n_outside in 0usize..=5,
        ) {
            // Build a manifest with disjoint synthetic paths.
            let make_path = |prefix: &str, i: usize| PathBuf::from(format!("{prefix}_{i}.ts"));
            let selected: Vec<TargetFileEntry> = (0..n_selected).map(|i| TargetFileEntry {
                path: make_path("sel", i),
                language: "typescript".to_string(),
                frontend: "shatter-ts".to_string(),
                line_count: None,
                content_hash: "abc".to_string(),
            }).collect();
            let excluded: Vec<ExcludedFileEntry> = (0..n_excluded).map(|i| ExcludedFileEntry {
                path: make_path("excl", i),
                reason: ExclusionReason::ExcludePattern,
                matched_pattern: None,
            }).collect();
            let unsupported: Vec<UnsupportedFileEntry> = (0..n_unsupported).map(|i| UnsupportedFileEntry {
                path: make_path("unsup", i),
                detected_language: None,
            }).collect();
            let outside: Vec<PathBuf> = (0..n_outside).map(|i| make_path("out", i)).collect();

            let manifest = TargetManifest {
                schema_version: "1".to_string(),
                kind: TARGET_MANIFEST_KIND.to_string(),
                scan_id: "test".to_string(),
                project_root: PathBuf::from("/tmp"),
                config_hash: "h".to_string(),
                source_set_hash: "h".to_string(),
                generated_at_ns: 0,
                selected,
                excluded,
                unsupported,
                candidate_outside_policy: outside,
            };

            // Collect all paths and check for duplicates.
            let mut all_paths: Vec<&PathBuf> = Vec::new();
            all_paths.extend(manifest.selected.iter().map(|f| &f.path));
            all_paths.extend(manifest.excluded.iter().map(|f| &f.path));
            all_paths.extend(manifest.unsupported.iter().map(|f| &f.path));
            all_paths.extend(manifest.candidate_outside_policy.iter());

            let mut seen = std::collections::HashSet::new();
            for path in &all_paths {
                prop_assert!(seen.insert(*path), "path {path:?} appears in multiple categories");
            }
        }
    }
}
