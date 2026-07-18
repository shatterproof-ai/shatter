use std::path::{Path, PathBuf};

use crate::helpers::Colors;

/// Default behavior-map / analysis cache directory, relative to the project
/// root. Mirrors the runtime default applied when `shatter.config.json`
/// leaves `cache_dir` unset (see `shatter_core::cache` and
/// `shatter_core::analysis_cache`).
const DEFAULT_CACHE_DIR: &str = ".shatter-cache";

/// Default cross-function seed pool directory, relative to the project root.
/// Mirrors the runtime default applied when `shatter.config.json` leaves
/// `seeds_dir` unset (see `shatter_core::config` and `README.md`).
const DEFAULT_SEEDS_DIR: &str = ".shatter/seeds";

/// Preserved-artifacts directory, relative to the project root. This is a
/// fixed default with no `shatter.config.json` override today (see
/// `shatter_core::harness_storage`).
const DEFAULT_ARTIFACTS_DIR: &str = "shatter-artifacts";

/// Opening marker for the Shatter-managed block in `.gitignore`.
const GITIGNORE_BEGIN: &str = "# >>> shatter generated paths (managed by `shatter init`)";

/// Closing marker for the Shatter-managed block in `.gitignore`.
const GITIGNORE_END: &str = "# <<< shatter generated paths";

/// Outcome of synchronizing the Shatter-managed `.gitignore` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitignoreOutcome {
    /// `.gitignore` did not exist and was created with the managed block.
    Created,
    /// `.gitignore` existed and the managed block was added or refreshed.
    Updated,
    /// `.gitignore` already contained the managed block verbatim.
    AlreadyCurrent,
}

/// Initialize persistent Shatter project state in the target directory.
///
/// Creates `.shatter/config.yaml` with auto-detected language and sensible
/// defaults. This establishes the repo-local Shatter configuration root.
/// Idempotent: if `.shatter/` already exists, reports status without
/// overwriting the config.
///
/// Regardless of whether the project was freshly initialized, this also
/// writes (or verifies) a managed `.gitignore` block covering every output
/// path Shatter generates — cache, seed pool, preserved artifacts, and any
/// configured report outputs — so generated files never pollute `git status`
/// (str-1fwt). The entries are driven by `shatter.config.json` values when
/// present, falling back to the documented defaults.
pub(crate) fn run_init(
    directory: Option<&Path>,
    _colors: &Colors,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the target directory.
    let resolved_dir: PathBuf = if let Some(dir) = directory {
        dir.to_path_buf()
    } else if let Some(root) = shatter_core::project::detect_project_root(
        &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    ) {
        root.path
    } else {
        std::env::current_dir()?
    };

    let shatter_dir = resolved_dir.join(".shatter");
    let already_initialized = shatter_dir.exists();

    // If .shatter/ already exists, report without overwriting the config.
    // We still verify the .gitignore block below so re-running init repairs a
    // missing entry (e.g. seeds_dir added after the fact).
    if already_initialized {
        println!("Project already initialized at {}", shatter_dir.display());
        // Report which files exist inside .shatter/.
        if let Ok(entries) = std::fs::read_dir(&shatter_dir) {
            for entry in entries.flatten() {
                println!("  {}", entry.path().display());
            }
        }
    } else {
        // Create .shatter/ directory.
        std::fs::create_dir_all(&shatter_dir)?;
        println!("  Created  .shatter/");

        // Detect language from marker files.
        let language = detect_language(&resolved_dir);

        // Write config.yaml.
        let config_path = shatter_dir.join("config.yaml");
        let config_content = build_config_yaml(&language);
        std::fs::write(&config_path, config_content)?;
        println!("  Created  .shatter/config.yaml  (detected language: {language})");
    }

    // Write or verify the managed .gitignore block for generated output paths.
    let ignore_entries = collect_generated_ignore_entries(&resolved_dir);
    match sync_gitignore(&resolved_dir, &ignore_entries)? {
        GitignoreOutcome::Created => println!(
            "  Created  .gitignore  ({} generated path(s) ignored)",
            ignore_entries.len()
        ),
        GitignoreOutcome::Updated => println!(
            "  Updated  .gitignore  ({} generated path(s) ignored)",
            ignore_entries.len()
        ),
        GitignoreOutcome::AlreadyCurrent => {}
    }

    if !already_initialized {
        println!("Initialized Shatter project at {}", resolved_dir.display());
    }

    Ok(())
}

/// Collect the relative `.gitignore` entries for every path Shatter generates
/// in this project, driven by `shatter.config.json` when present and falling
/// back to the documented defaults.
///
/// Directory outputs (cache, seed pool, artifacts) get a trailing `/`; report
/// file outputs declared in `output.paths` are listed verbatim. Absolute
/// paths are skipped — they live outside the repo and cannot be expressed as
/// a portable `.gitignore` entry. The result is de-duplicated with insertion
/// order preserved.
fn collect_generated_ignore_entries(project_root: &Path) -> Vec<String> {
    let config = shatter_core::config::load_project_config(project_root)
        .ok()
        .flatten();

    let mut entries: Vec<String> = Vec::new();

    // Behavior-map / analysis cache directory (config override or default).
    let cache_dir = config
        .as_ref()
        .and_then(|c| c.cache_dir.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_DIR));
    if let Some(entry) = dir_ignore_entry(&cache_dir) {
        entries.push(entry);
    }

    // Cross-function seed pool directory (config override or default).
    let seeds_dir = config
        .as_ref()
        .and_then(|c| c.seeds_dir.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SEEDS_DIR));
    if let Some(entry) = dir_ignore_entry(&seeds_dir) {
        entries.push(entry);
    }

    // Preserved-artifacts directory (fixed default, not configurable today).
    if let Some(entry) = dir_ignore_entry(Path::new(DEFAULT_ARTIFACTS_DIR)) {
        entries.push(entry);
    }

    // Configured report output paths, if any.
    if let Some(output) = config.as_ref().and_then(|c| c.output.as_ref()) {
        for path in &output.paths {
            if let Some(entry) = file_ignore_entry(path) {
                entries.push(entry);
            }
        }
    }

    // De-duplicate while preserving insertion order.
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.clone()));
    entries
}

/// Normalize a directory path into a `.gitignore` entry (forward slashes,
/// trailing `/`). Returns `None` for absolute paths, which cannot be expressed
/// portably in a repo-root `.gitignore`.
fn dir_ignore_entry(path: &Path) -> Option<String> {
    let normalized = normalize_relative(path)?;
    Some(format!("{normalized}/"))
}

/// Normalize a file path into a `.gitignore` entry (forward slashes, no
/// trailing `/`). Returns `None` for absolute paths.
fn file_ignore_entry(path: &Path) -> Option<String> {
    normalize_relative(path)
}

/// Convert a relative path to a forward-slash string with any trailing slash
/// trimmed. Returns `None` for absolute or empty paths.
fn normalize_relative(path: &Path) -> Option<String> {
    if path.is_absolute() {
        return None;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Render the managed block (markers plus one entry per line).
fn render_block(entries: &[String]) -> String {
    let mut block = String::new();
    block.push_str(GITIGNORE_BEGIN);
    block.push('\n');
    for entry in entries {
        block.push_str(entry);
        block.push('\n');
    }
    block.push_str(GITIGNORE_END);
    block
}

/// Write or refresh the Shatter-managed block in `<project_root>/.gitignore`.
///
/// If the markers are already present, the lines between them (inclusive) are
/// replaced with the freshly computed block, leaving all other content
/// untouched. If the markers are absent, the block is appended. The file is
/// only rewritten when its contents would actually change.
fn sync_gitignore(project_root: &Path, entries: &[String]) -> std::io::Result<GitignoreOutcome> {
    if entries.is_empty() {
        return Ok(GitignoreOutcome::AlreadyCurrent);
    }

    let gitignore_path = project_root.join(".gitignore");
    let block = render_block(entries);

    let existing = match std::fs::read_to_string(&gitignore_path) {
        Ok(contents) => Some(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };

    let Some(existing) = existing else {
        // Fresh file: the block plus a trailing newline.
        std::fs::write(&gitignore_path, format!("{block}\n"))?;
        return Ok(GitignoreOutcome::Created);
    };

    let updated = replace_or_append_block(&existing, &block);
    if updated == existing {
        Ok(GitignoreOutcome::AlreadyCurrent)
    } else {
        std::fs::write(&gitignore_path, updated)?;
        Ok(GitignoreOutcome::Updated)
    }
}

/// Replace an existing managed block in `existing` with `block`, or append the
/// block if no markers are found. Preserves all content outside the markers.
fn replace_or_append_block(existing: &str, block: &str) -> String {
    if let (Some(begin), Some(end_marker)) =
        (existing.find(GITIGNORE_BEGIN), existing.find(GITIGNORE_END))
    {
        // Extend `end` to the end of the line containing the closing marker.
        let after_marker = end_marker + GITIGNORE_END.len();
        let end = existing[after_marker..]
            .find('\n')
            .map(|rel| after_marker + rel)
            .unwrap_or(existing.len());

        let mut result = String::with_capacity(existing.len());
        result.push_str(&existing[..begin]);
        result.push_str(block);
        result.push_str(&existing[end..]);
        return result;
    }

    // No managed block yet: append, ensuring a separating newline.
    let mut result = String::with_capacity(existing.len() + block.len() + 2);
    result.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        result.push('\n');
    }
    if !existing.is_empty() {
        result.push('\n');
    }
    result.push_str(block);
    result.push('\n');
    result
}

/// Detect the project language from marker files in the given directory.
fn detect_language(dir: &Path) -> String {
    if dir.join("package.json").exists() {
        "typescript".to_string()
    } else if dir.join("go.mod").exists() {
        "go".to_string()
    } else if dir.join("Cargo.toml").exists() {
        "rust".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Build the YAML content for `.shatter/config.yaml`.
fn build_config_yaml(language: &str) -> String {
    format!(
        r#"# Shatter project configuration
# Generated by `shatter init`
#
# Place this file alongside your source code in a `.shatter/` directory.
# Shatter discovers config files by walking upward from each target file;
# the nearest config wins when settings conflict.
#
# This file owns PER-FUNCTION analysis behavior (iterations, timeouts, mocks,
# generators, setup, opaque types). SCAN-GLOBAL settings (file discovery,
# output, caching, resource limits, seeds_dir) live in `shatter.config.json`
# at the project root. The two files do not overlap.
# Precedence (highest first):
#   CLI flags > --set overrides > .shatter/config.yaml (nearest wins)
#     > shatter.config.json > built-in defaults
# See the "Project Configuration" section of README.md for details.

# ── Global defaults ──────────────────────────────────────────────────────
# These apply to every function unless overridden below.
defaults:
  max_iterations: 100        # exploration iterations per function
  timeout: 60                # seconds before a single exploration times out

# language: {language}  # auto-detected; uncomment to override
# frontend: ~            # use bundled default

# ── Type generators ───────────────────────────────────────────────────────
# Map a type name to a file exporting a function of the same name that
# returns a seed value for that type.
#
# defaults:
#   generators:
#     MyType:
#       kind: object
#       fields:
#         field1: {{kind: string}}

# ── Per-function overrides ───────────────────────────────────────────────
# Keys are "relative/path.<ext>:functionName" patterns (globs supported).
#
# functions:
#   "src/my-file.ts:myFunction":
#     skip: true
"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_typescript() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_language(dir.path()), "typescript");
    }

    #[test]
    fn detect_language_go() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module foo").unwrap();
        assert_eq!(detect_language(dir.path()), "go");
    }

    #[test]
    fn detect_language_rust() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_language(dir.path()), "rust");
    }

    #[test]
    fn detect_language_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_language(dir.path()), "unknown");
    }

    #[test]
    fn detect_language_prefers_typescript_over_others() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("go.mod"), "module foo").unwrap();
        // package.json wins (checked first)
        assert_eq!(detect_language(dir.path()), "typescript");
    }

    #[test]
    fn run_init_creates_shatter_dir_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        run_init(Some(dir.path()), &colors).unwrap();

        assert!(dir.path().join(".shatter").exists());
        assert!(dir.path().join(".shatter").join("config.yaml").exists());

        let content =
            std::fs::read_to_string(dir.path().join(".shatter").join("config.yaml")).unwrap();
        assert!(content.contains("max_iterations: 100"));
        assert!(content.contains("timeout: 60"));
        // str-mktn: the generated config ships the ownership/precedence note so
        // integrators can tell which file owns what without reading the docs.
        assert!(
            content.contains("shatter.config.json"),
            "config.yaml header must reference the sibling scan-global config"
        );
        assert!(
            content.contains("Precedence"),
            "config.yaml header must state the override precedence"
        );
    }

    #[test]
    fn run_init_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);

        // First call creates.
        run_init(Some(dir.path()), &colors).unwrap();
        // Modify the config to detect whether it is overwritten.
        let config_path = dir.path().join(".shatter").join("config.yaml");
        std::fs::write(&config_path, "# custom content").unwrap();

        // Second call must not overwrite.
        run_init(Some(dir.path()), &colors).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(
            content, "# custom content",
            "idempotent: must not overwrite existing config"
        );
    }

    #[test]
    fn run_init_detects_language_in_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let colors = Colors::new(false);
        run_init(Some(dir.path()), &colors).unwrap();

        let content =
            std::fs::read_to_string(dir.path().join(".shatter").join("config.yaml")).unwrap();
        assert!(content.contains("typescript"));
    }

    fn read_gitignore(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(".gitignore")).unwrap()
    }

    #[test]
    fn run_init_creates_gitignore_with_default_generated_paths() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        run_init(Some(dir.path()), &colors).unwrap();

        let gitignore = read_gitignore(dir.path());
        assert!(gitignore.contains(GITIGNORE_BEGIN));
        assert!(gitignore.contains(GITIGNORE_END));
        // All documented default output paths must be present with a trailing /.
        assert!(gitignore.contains("\n.shatter-cache/\n"));
        assert!(gitignore.contains("\n.shatter/seeds/\n"));
        assert!(gitignore.contains("\nshatter-artifacts/\n"));
    }

    #[test]
    fn collect_entries_uses_config_overrides_and_report_paths() {
        let dir = tempfile::tempdir().unwrap();
        // A shatter.config.json that overrides cache_dir / seeds_dir and
        // declares report output paths.
        std::fs::write(
            dir.path().join("shatter.config.json"),
            r#"{
              "cache_dir": ".my-cache",
              "seeds_dir": ".my-seeds",
              "output": { "paths": ["shatter-report/scan.html", "shatter-report/scan.json"] }
            }"#,
        )
        .unwrap();

        let entries = collect_generated_ignore_entries(dir.path());
        assert!(entries.contains(&".my-cache/".to_string()));
        assert!(entries.contains(&".my-seeds/".to_string()));
        // Artifacts dir is fixed (no config field today).
        assert!(entries.contains(&"shatter-artifacts/".to_string()));
        // Report file paths are listed verbatim (no trailing slash).
        assert!(entries.contains(&"shatter-report/scan.html".to_string()));
        assert!(entries.contains(&"shatter-report/scan.json".to_string()));
        // The defaults must NOT appear once overridden.
        assert!(!entries.contains(&".shatter-cache/".to_string()));
        assert!(!entries.contains(&".shatter/seeds/".to_string()));
    }

    #[test]
    fn run_init_appends_block_preserving_existing_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        let colors = Colors::new(false);
        run_init(Some(dir.path()), &colors).unwrap();

        let gitignore = read_gitignore(dir.path());
        // Pre-existing content is preserved.
        assert!(gitignore.contains("node_modules/"));
        assert!(gitignore.contains("*.log"));
        // Managed block is appended.
        assert!(gitignore.contains(GITIGNORE_BEGIN));
        assert!(gitignore.contains(".shatter/seeds/"));
    }

    #[test]
    fn sync_gitignore_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect_generated_ignore_entries(dir.path());

        let first = sync_gitignore(dir.path(), &entries).unwrap();
        assert_eq!(first, GitignoreOutcome::Created);
        let after_first = read_gitignore(dir.path());

        let second = sync_gitignore(dir.path(), &entries).unwrap();
        assert_eq!(second, GitignoreOutcome::AlreadyCurrent);
        let after_second = read_gitignore(dir.path());

        assert_eq!(
            after_first, after_second,
            "re-running must not change a current .gitignore"
        );
        // No duplicated markers.
        assert_eq!(after_second.matches(GITIGNORE_BEGIN).count(), 1);
    }

    #[test]
    fn sync_gitignore_refreshes_stale_block_in_place() {
        let dir = tempfile::tempdir().unwrap();
        // Simulate an older block missing the seeds entry, with content on
        // both sides of the markers.
        let stale = format!(
            "node_modules/\n{GITIGNORE_BEGIN}\n.shatter-cache/\nshatter-artifacts/\n{GITIGNORE_END}\n# trailing user content\n",
        );
        std::fs::write(dir.path().join(".gitignore"), &stale).unwrap();

        let entries = collect_generated_ignore_entries(dir.path());
        let outcome = sync_gitignore(dir.path(), &entries).unwrap();
        assert_eq!(outcome, GitignoreOutcome::Updated);

        let gitignore = read_gitignore(dir.path());
        // The missing default seeds entry is now present.
        assert!(gitignore.contains(".shatter/seeds/"));
        // Surrounding content on both sides is preserved.
        assert!(gitignore.starts_with("node_modules/\n"));
        assert!(gitignore.contains("# trailing user content"));
        // Still exactly one managed block.
        assert_eq!(gitignore.matches(GITIGNORE_BEGIN).count(), 1);
    }

    #[test]
    fn run_init_repairs_gitignore_when_already_initialized() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        // Pre-create .shatter/ so init takes the already-initialized path.
        std::fs::create_dir_all(dir.path().join(".shatter")).unwrap();

        run_init(Some(dir.path()), &colors).unwrap();

        // Even on the already-initialized path, the gitignore block is written.
        let gitignore = read_gitignore(dir.path());
        assert!(gitignore.contains(".shatter/seeds/"));
    }

    #[test]
    fn normalize_relative_skips_absolute_paths() {
        assert_eq!(normalize_relative(Path::new("/abs/path")), None);
        assert_eq!(
            normalize_relative(Path::new("rel/dir/")),
            Some("rel/dir".to_string())
        );
    }
}
