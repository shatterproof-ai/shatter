//! Single source of truth for the output paths Shatter generates in a project
//! and the managed `.gitignore` block that keeps them out of `git status`
//! (str-1fwt).
//!
//! Both `shatter init` (writer) and `shatter doctor` (checker) drive off the
//! same `collect_generated_ignore_entries` computation so the two commands can
//! never disagree about which paths are generated. `init` writes/refreshes the
//! managed block; `doctor` reports any configured-but-unignored path so an
//! existing repo that added `.gitignore` entries by hand (and missed one, e.g.
//! `seeds_dir`) is flagged rather than silently polluting `git status`.

use std::path::{Path, PathBuf};

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

/// Harness storage cache directory, relative to the project root. Every
/// scan/explore/run session writes a project-scoped harness build cache under
/// `<project_root>/.shatter/cache/harness/` (see
/// `shatter_core::harness_storage::HarnessStorage::default_cache_dir`). It is a
/// fixed default with no `shatter.config.json` override today. Ignoring the
/// parent `.shatter/cache/` keeps the whole harness cache tree out of
/// `git status` — this is a sibling of the seed pool under `.shatter/`, and was
/// the second generated path missed alongside `seeds_dir` (str-1fwt).
const DEFAULT_HARNESS_CACHE_DIR: &str = ".shatter/cache";

/// Opening marker for the Shatter-managed block in `.gitignore`.
pub(crate) const GITIGNORE_BEGIN: &str = "# >>> shatter generated paths (managed by `shatter init`)";

/// Closing marker for the Shatter-managed block in `.gitignore`.
pub(crate) const GITIGNORE_END: &str = "# <<< shatter generated paths";

/// Outcome of synchronizing the Shatter-managed `.gitignore` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitignoreOutcome {
    /// `.gitignore` did not exist and was created with the managed block.
    Created,
    /// `.gitignore` existed and the managed block was added or refreshed.
    Updated,
    /// `.gitignore` already contained the managed block verbatim.
    AlreadyCurrent,
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
pub(crate) fn collect_generated_ignore_entries(project_root: &Path) -> Vec<String> {
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

    // Harness storage build cache (fixed default under `.shatter/`, not
    // configurable today). Sibling of the seed pool; must be ignored too.
    if let Some(entry) = dir_ignore_entry(Path::new(DEFAULT_HARNESS_CACHE_DIR)) {
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

/// Return the generated `.gitignore` entries that the project's current
/// `.gitignore` does **not** already cover.
///
/// This is the checker side of str-1fwt: `init` writes the managed block, but
/// an existing repo may have added entries by hand and missed one (the refute
/// `seeds_dir` case). A path counts as covered when the `.gitignore` contains
/// an entry equal to it or to an ancestor directory of it (so `.shatter/`
/// covers `.shatter/seeds/`), whether inside or outside the managed block.
/// Returns entries in the same order as `collect_generated_ignore_entries`.
pub(crate) fn unignored_generated_paths(project_root: &Path) -> Vec<String> {
    let entries = collect_generated_ignore_entries(project_root);
    let gitignore = std::fs::read_to_string(project_root.join(".gitignore")).unwrap_or_default();
    entries
        .into_iter()
        .filter(|entry| !gitignore_covers(&gitignore, entry))
        .collect()
}

/// Whether `gitignore` contents contain a pattern that covers `entry`.
///
/// Covered means some non-comment, non-blank line matches `entry` exactly
/// (ignoring a trailing `/` on either side) or names an ancestor directory of
/// `entry`. Only plain path patterns are understood — negations (`!`),
/// anchored globs, and wildcards are treated as non-matching, which is the
/// safe direction for a diagnostic (it may over-report, never under-report the
/// documented defaults `init` writes).
fn gitignore_covers(gitignore: &str, entry: &str) -> bool {
    let target = entry.trim_end_matches('/');
    gitignore.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            return false;
        }
        let pat = line.trim_end_matches('/');
        if pat.is_empty() {
            return false;
        }
        // Exact match, or `pat` is an ancestor directory of `target`.
        target == pat || target.starts_with(&format!("{pat}/"))
    })
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
pub(crate) fn sync_gitignore(
    project_root: &Path,
    entries: &[String],
) -> std::io::Result<GitignoreOutcome> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn read_gitignore(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(".gitignore")).unwrap()
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
        // Harness storage cache is fixed too (no config field today) and must
        // be ignored even when cache_dir/seeds_dir are overridden.
        assert!(entries.contains(&".shatter/cache/".to_string()));
        // Report file paths are listed verbatim (no trailing slash).
        assert!(entries.contains(&"shatter-report/scan.html".to_string()));
        assert!(entries.contains(&"shatter-report/scan.json".to_string()));
        // The defaults must NOT appear once overridden.
        assert!(!entries.contains(&".shatter-cache/".to_string()));
        assert!(!entries.contains(&".shatter/seeds/".to_string()));
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
    fn normalize_relative_skips_absolute_paths() {
        assert_eq!(normalize_relative(Path::new("/abs/path")), None);
        assert_eq!(
            normalize_relative(Path::new("rel/dir/")),
            Some("rel/dir".to_string())
        );
    }

    #[test]
    fn unignored_reports_missing_seed_dir_and_nothing_when_ignored() {
        let dir = tempfile::tempdir().unwrap();
        // The refute failure mode: behavior-map cache and artifacts ignored by
        // hand, but the two `.shatter/` siblings (seed pool + harness cache)
        // missed. No managed block, so nothing auto-covers them.
        std::fs::write(
            dir.path().join(".gitignore"),
            ".shatter-cache/\nshatter-artifacts/\n",
        )
        .unwrap();

        let missing = unignored_generated_paths(dir.path());
        assert_eq!(
            missing,
            vec![".shatter/seeds/".to_string(), ".shatter/cache/".to_string()],
            "both un-ignored `.shatter/` generated dirs must be flagged"
        );

        // After init writes its managed block, nothing is flagged.
        let entries = collect_generated_ignore_entries(dir.path());
        sync_gitignore(dir.path(), &entries).unwrap();
        assert!(
            unignored_generated_paths(dir.path()).is_empty(),
            "a synced .gitignore must leave no un-ignored generated path"
        );
    }

    #[test]
    fn unignored_treats_ancestor_dir_as_coverage() {
        let dir = tempfile::tempdir().unwrap();
        // Ignoring the whole `.shatter/` tree covers the default seeds dir
        // `.shatter/seeds/` without an exact entry.
        std::fs::write(
            dir.path().join(".gitignore"),
            ".shatter/\n.shatter-cache/\nshatter-artifacts/\n",
        )
        .unwrap();

        let missing = unignored_generated_paths(dir.path());
        assert!(
            !missing.iter().any(|e| e == ".shatter/seeds/"),
            "an ancestor `.shatter/` entry must cover `.shatter/seeds/`, got {missing:?}"
        );
    }

    #[test]
    fn unignored_missing_gitignore_flags_all_defaults() {
        let dir = tempfile::tempdir().unwrap();
        // No .gitignore at all: every generated default is un-ignored.
        let missing = unignored_generated_paths(dir.path());
        assert_eq!(missing, collect_generated_ignore_entries(dir.path()));
    }

    #[test]
    fn gitignore_covers_ignores_negations_and_comments() {
        assert!(!gitignore_covers("# .shatter/seeds/\n", ".shatter/seeds/"));
        assert!(!gitignore_covers("!.shatter/seeds/\n", ".shatter/seeds/"));
        assert!(gitignore_covers(".shatter/seeds\n", ".shatter/seeds/"));
        assert!(gitignore_covers(".shatter/seeds/\n", ".shatter/seeds"));
    }

    // A relative path component: non-empty, no slashes/backslashes, not `.`
    // or `..`, no leading `!`/`#`/whitespace that would change gitignore
    // meaning.
    fn path_component() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,8}".prop_filter("no dot dirs", |s| s != "." && s != "..")
    }

    proptest! {
        // Writer/checker parity (str-1fwt): whatever generated entries `init`
        // renders into the managed block, `doctor`'s coverage check must
        // certify as ignored — regardless of any pre-existing `.gitignore`
        // content. This is the core invariant linking the two code paths: a
        // freshly `init`-ed repo can never leave a generated path un-ignored.
        #[test]
        fn synced_block_leaves_nothing_unignored(
            components in prop::collection::vec(path_component(), 1..5),
            preexisting in prop::collection::vec(path_component(), 0..4),
        ) {
            // Build directory entries from unique relative paths.
            let entries: Vec<String> = {
                let mut seen = std::collections::HashSet::new();
                components
                    .iter()
                    .filter(|c| seen.insert((*c).clone()))
                    .map(|c| format!("{c}/"))
                    .collect()
            };

            // Arbitrary unrelated pre-existing content that must not be
            // mistaken for coverage of our entries.
            let existing: String = preexisting
                .iter()
                .map(|c| format!("unrelated-{c}/\n"))
                .collect();
            let updated = replace_or_append_block(&existing, &render_block(&entries));

            for entry in &entries {
                prop_assert!(
                    gitignore_covers(&updated, entry),
                    "entry {entry:?} not covered by\n{updated}"
                );
            }
            // And exactly one managed block regardless of prior content.
            prop_assert_eq!(updated.matches(GITIGNORE_BEGIN).count(), 1);
        }
    }
}
