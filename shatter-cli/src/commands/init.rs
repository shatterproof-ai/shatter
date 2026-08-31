use std::path::{Path, PathBuf};

use crate::generated_paths::{
    collect_generated_ignore_entries, sync_gitignore, unignored_generated_paths, GitignoreOutcome,
};
use crate::helpers::Colors;

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
///
/// An explicit `shatter init` invocation from the user (via this function)
/// always writes/refreshes the `.gitignore` block, exactly as documented
/// above.
pub(crate) fn run_init(
    directory: Option<&Path>,
    colors: &Colors,
) -> Result<(), Box<dyn std::error::Error>> {
    run_init_impl(directory, colors, /* implicit = */ false)
}

/// Same as [`run_init`], but for the implicit init that `scan`/`explore`/
/// `analyze` trigger on their own when `.shatter/` is missing (str-w5jt9).
///
/// A plain `shatter scan`/`explore`/`analyze` must never dirty a file that is
/// already tracked in git — the user did not ask for that write. This variant
/// still creates `.shatter/` and a *new* `.gitignore` exactly like an explicit
/// `init` (that preserves today's first-run ergonomics), but if `.gitignore`
/// is already tracked in git, the managed block is left untouched rather than
/// appended to it. Re-running `shatter init` explicitly still repairs/refreshes
/// a tracked `.gitignore` as before.
pub(crate) fn run_implicit_init(
    directory: Option<&Path>,
    colors: &Colors,
) -> Result<(), Box<dyn std::error::Error>> {
    run_init_impl(directory, colors, /* implicit = */ true)
}

fn run_init_impl(
    directory: Option<&Path>,
    _colors: &Colors,
    implicit: bool,
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
    //
    // An implicit init (str-w5jt9) must not modify a `.gitignore` that is
    // already tracked in git — a bare `shatter scan` should never dirty a
    // tracked file the user didn't ask to change. A brand-new `.gitignore`
    // (untracked, or not yet created) is still written so first-run
    // ergonomics are unchanged; only the "append to an existing tracked file"
    // case is skipped.
    let gitignore_relative = Path::new(".gitignore");
    let skip_tracked_gitignore =
        implicit && shatter_core::scm::is_path_tracked(&resolved_dir, gitignore_relative);
    // str-vr7vq: an implicit init must not create a brand-new, untracked
    // `.gitignore` file when every generated path is already covered —
    // whether by an ancestor `.gitignore` up to the enclosing git root (the
    // common case for a nested fixture/example directory inside a monorepo
    // whose root `.gitignore` already ignores `.shatter/`,
    // `.shatter-cache/`, and `shatter-artifacts/` recursively), or by the
    // local file's own non-managed patterns. `unignored_generated_paths` is
    // the same ancestor-aware coverage check `doctor` already uses; reusing
    // it here means a plain `scan`/`explore` against an already-covered
    // nested project leaves no stray file behind at all, rather than
    // writing a redundant local `.gitignore` whose entries were never
    // actually uncovered.
    let skip_already_covered_gitignore =
        implicit && unignored_generated_paths(&resolved_dir).is_empty();
    if skip_tracked_gitignore {
        println!(
            "  Skipped  .gitignore  (tracked in git; run `shatter init` explicitly to refresh it)"
        );
    } else if skip_already_covered_gitignore {
        println!(
            "  Skipped  .gitignore  (generated paths already ignored by an ancestor .gitignore)"
        );
    } else {
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
    }

    if !already_initialized {
        println!("Initialized Shatter project at {}", resolved_dir.display());
    }

    Ok(())
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
    use crate::generated_paths::{GITIGNORE_BEGIN, GITIGNORE_END};

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
        // The harness storage cache under `.shatter/` (str-1fwt).
        assert!(gitignore.contains("\n.shatter/cache/\n"));
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

    // --- str-w5jt9: implicit init must never touch a tracked .gitignore ---

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .status()
            .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    fn init_git_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "test@example.com"]);
        git(dir, &["config", "user.name", "Test User"]);
    }

    #[test]
    fn run_implicit_init_skips_a_gitignore_already_tracked_in_git() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            eprintln!("skipping: git not available on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]);
        git(dir.path(), &["commit", "-q", "-m", "track gitignore"]);

        let colors = Colors::new(false);
        run_implicit_init(Some(dir.path()), &colors).unwrap();

        // .shatter/ is still created — implicit init's first-run ergonomics
        // are unchanged.
        assert!(dir.path().join(".shatter").exists());

        // But the tracked .gitignore is byte-for-byte untouched: no managed
        // block appended.
        let gitignore = read_gitignore(dir.path());
        assert_eq!(gitignore, "node_modules/\n*.log\n");
        assert!(!gitignore.contains(GITIGNORE_BEGIN));
    }

    #[test]
    fn run_implicit_init_still_creates_a_brand_new_gitignore() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            eprintln!("skipping: git not available on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        // No .gitignore committed at all.

        let colors = Colors::new(false);
        run_implicit_init(Some(dir.path()), &colors).unwrap();

        // No pre-existing tracked file to protect, so a fresh .gitignore with
        // the managed block is written exactly like an explicit `init`.
        let gitignore = read_gitignore(dir.path());
        assert!(gitignore.contains(GITIGNORE_BEGIN));
        assert!(gitignore.contains(".shatter/seeds/"));
    }

    #[test]
    fn run_implicit_init_still_updates_an_untracked_gitignore() {
        // No git repo at all: nothing is "tracked", so the untracked
        // .gitignore in a bare directory is still synced as before.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();

        let colors = Colors::new(false);
        run_implicit_init(Some(dir.path()), &colors).unwrap();

        let gitignore = read_gitignore(dir.path());
        assert!(gitignore.contains("node_modules/"));
        assert!(gitignore.contains(GITIGNORE_BEGIN));
    }

    #[test]
    fn run_init_explicit_still_refreshes_a_tracked_gitignore() {
        // The explicit `shatter init` entry point (run_init, not
        // run_implicit_init) must keep writing exactly as before — only the
        // implicit path skips tracked files.
        if std::process::Command::new("git").arg("--version").output().is_err() {
            eprintln!("skipping: git not available on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]);
        git(dir.path(), &["commit", "-q", "-m", "track gitignore"]);

        let colors = Colors::new(false);
        run_init(Some(dir.path()), &colors).unwrap();

        let gitignore = read_gitignore(dir.path());
        assert!(
            gitignore.contains(GITIGNORE_BEGIN),
            "explicit `shatter init` must still refresh a tracked .gitignore"
        );
    }

    #[test]
    fn run_implicit_init_skips_gitignore_when_ancestor_already_covers_generated_paths() {
        // str-vr7vq regression: a nested fixture/example directory inside a
        // monorepo whose root .gitignore already recursively ignores every
        // generated path via bare, unanchored patterns must not get a
        // brand-new, untracked local .gitignore file from a plain
        // `scan`/`explore` — there is nothing left to add. (This repo's own
        // root .gitignore relies on a `**/.shatter/*` glob for the same
        // recursive intent; real git honors that, but `unignored_generated_paths`'s
        // deliberately non-glob-aware matcher cannot, so it still reports
        // `examples/**/.shatter/{seeds,cache}/` as uncovered there — a
        // pre-existing gap tracked separately, not fixed by this test.)
        if std::process::Command::new("git").arg("--version").output().is_err() {
            eprintln!("skipping: git not available on PATH");
            return;
        }
        let repo = tempfile::tempdir().unwrap();
        init_git_repo(repo.path());
        // Bare, unanchored (no internal `/`) entries — matching this crate's
        // own `unignored_finds_gitignore_at_enclosing_git_root` coverage
        // test in `generated_paths.rs` — so each pattern matches its named
        // path component at any depth. A *slash-containing* entry (e.g. the
        // literal `.shatter/seeds/` this repo's own root-level managed block
        // writes) is anchored to the `.gitignore`'s own directory and would
        // NOT cover a nested directory's `.shatter/seeds/` — see
        // `gitignore_file_covers`'s doc comment and
        // `unignored_does_not_let_a_root_anchored_pattern_cover_a_nested_package`.
        std::fs::write(
            repo.path().join(".gitignore"),
            ".shatter\n.shatter-cache\nshatter-artifacts\n",
        )
        .unwrap();
        git(repo.path(), &["add", ".gitignore"]);
        git(repo.path(), &["commit", "-q", "-m", "root gitignore"]);

        let nested = repo.path().join("examples").join("go").join("fixture");
        std::fs::create_dir_all(&nested).unwrap();

        let colors = Colors::new(false);
        run_implicit_init(Some(&nested), &colors).unwrap();

        // .shatter/ is still created — first-run ergonomics preserved.
        assert!(nested.join(".shatter").exists());

        // But no local .gitignore is written: the root .gitignore already
        // covers every generated path recursively.
        assert!(
            !nested.join(".gitignore").exists(),
            "implicit init must not create a redundant local .gitignore when \
             an ancestor .gitignore already covers all generated paths"
        );
    }
}
