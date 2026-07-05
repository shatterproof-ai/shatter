//! `shatter doctor` — diagnose the local install.
//!
//! Currently focused on embedded-frontend staleness (str-o09e): the CLI embeds
//! a compiled Go frontend at build time. If a developer edits `shatter-go/`
//! sources but the binary was not rebuilt, the running CLI silently uses the
//! old frontend. `doctor` recomputes the Go source hash from the checkout and
//! compares it against the hash baked in at build time, surfacing the drift.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::helpers::Colors;

/// Build-time hash of the Go frontend source tree (see `build.rs`).
const GO_FRONTEND_SOURCE_HASH: &str = env!("GO_FRONTEND_SOURCE_HASH");
/// Build-time hash of the compiled Go frontend binary that is embedded.
const GO_FRONTEND_BINARY_HASH: &str = env!("GO_FRONTEND_HASH");
/// Build-time hash of the embedded TypeScript bundle pair.
const TS_FRONTEND_BUNDLE_HASH: &str = env!("FRONTEND_BUNDLE_HASH");
/// Path to the `shatter-go/` source tree at build time. In an installed binary
/// this directory typically does not exist, in which case the staleness check
/// is skipped.
const GO_FRONTEND_SOURCE_DIR: &str = env!("GO_FRONTEND_SOURCE_DIR");

/// Run `shatter doctor`. Returns `Ok(true)` when healthy, `Ok(false)` when a
/// stale embedded frontend or an un-ignored generated project path is detected
/// (so the caller can exit non-zero).
///
/// `directory` selects the project to check for `.gitignore` coverage of
/// generated output paths (str-1fwt); when `None` the project root is
/// auto-detected from the current directory.
pub fn run_doctor(
    directory: Option<&Path>,
    colors: &Colors,
) -> Result<bool, Box<dyn std::error::Error>> {
    crate::helpers::print_stdout(&format!("{}Shatter doctor{}\n\n", colors.bold, colors.reset));

    crate::helpers::print_stdout(&format!(
        "shatter version:          {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    crate::helpers::print_stdout(&format!(
        "go-frontend source hash:  {GO_FRONTEND_SOURCE_HASH}\n"
    ));
    crate::helpers::print_stdout(&format!(
        "go-frontend binary hash:  {GO_FRONTEND_BINARY_HASH}\n"
    ));
    crate::helpers::print_stdout(&format!(
        "ts-frontend bundle hash:  {TS_FRONTEND_BUNDLE_HASH}\n\n"
    ));

    let frontend_ok = check_embedded_frontend(colors);
    crate::helpers::print_stdout("\n");
    let gitignore_ok = check_generated_paths_ignored(directory, colors);

    Ok(frontend_ok && gitignore_ok)
}

/// Check whether the embedded Go frontend is up to date with the on-disk
/// `shatter-go/` sources. Returns `true` when healthy or when the check is not
/// applicable (installed binary outside its source checkout).
fn check_embedded_frontend(colors: &Colors) -> bool {
    let source_dir = Path::new(GO_FRONTEND_SOURCE_DIR);
    if !source_dir.is_dir() {
        crate::helpers::print_stdout(
            "Embedded Go frontend: source tree not available — staleness check skipped.\n",
        );
        crate::helpers::print_stdout(
            "  (This is expected for an installed binary outside its source checkout.)\n",
        );
        return true;
    }

    let current_hash = match hash_go_source_tree(source_dir) {
        Ok(h) => h,
        Err(e) => {
            crate::helpers::print_stdout(&format!(
                "Embedded Go frontend: could not hash source tree ({e}) — staleness check skipped.\n"
            ));
            return true;
        }
    };

    if current_hash == GO_FRONTEND_SOURCE_HASH {
        crate::helpers::print_stdout("Embedded Go frontend: up to date.\n");
        true
    } else {
        crate::helpers::print_stdout(&format!(
            "{}Embedded Go frontend is stale{} — the binary was built from a different \
             `shatter-go/` than what is on disk.\n",
            colors.bold, colors.reset
        ));
        crate::helpers::print_stdout(&format!("  embedded source hash: {GO_FRONTEND_SOURCE_HASH}\n"));
        crate::helpers::print_stdout(&format!("  current  source hash: {current_hash}\n"));
        crate::helpers::print_stdout(
            "  Run `cargo build -p shatter-cli` to rebuild the embedded frontend.\n",
        );
        false
    }
}

/// Check that the target project's `.gitignore` covers every output path
/// Shatter's config says it generates (str-1fwt). Returns `true` when clean or
/// not applicable (no `.shatter/` project), `false` when any configured path is
/// un-ignored so the caller exits non-zero.
fn check_generated_paths_ignored(directory: Option<&Path>, colors: &Colors) -> bool {
    let project_root = resolve_project_root(directory);

    // Only projects that have been `shatter init`-ed carry generated paths to
    // worry about. Outside a project, this check is not applicable.
    if !project_root.join(".shatter").exists() {
        crate::helpers::print_stdout(
            "Generated-path gitignore: no `.shatter/` project here — check skipped.\n",
        );
        return true;
    }

    let unignored = crate::generated_paths::unignored_generated_paths(&project_root);
    if unignored.is_empty() {
        crate::helpers::print_stdout(
            "Generated-path gitignore: all configured output paths are ignored.\n",
        );
        true
    } else {
        crate::helpers::print_stdout(&format!(
            "{}Generated paths are not ignored{} — {} configured output path(s) would \
             pollute `git status`:\n",
            colors.bold,
            colors.reset,
            unignored.len()
        ));
        for entry in &unignored {
            crate::helpers::print_stdout(&format!("  {entry}\n"));
        }
        crate::helpers::print_stdout(
            "  Run `shatter init` to write the managed `.gitignore` block.\n",
        );
        false
    }
}

/// Resolve the project root for the gitignore check: the explicit directory if
/// given, else the auto-detected project root, else the current directory.
fn resolve_project_root(directory: Option<&Path>) -> PathBuf {
    if let Some(dir) = directory {
        return dir.to_path_buf();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    shatter_core::project::detect_project_root(&cwd)
        .map(|root| root.path)
        .unwrap_or(cwd)
}

/// Whether a file contributes to the Go frontend build. Must mirror
/// `is_go_source_file` in `build.rs` exactly so the runtime hash matches the
/// build-time hash.
fn is_go_source_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name == "go.mod" || name == "go.sum" {
        return true;
    }
    name.ends_with(".go") && !name.ends_with("_test.go")
}

/// Recompute the Go source-tree hash using the same algorithm as `build.rs`'s
/// `hash_source_tree`: files sorted by path, each folded in as
/// `relpath \0 len(le) content \0`, hashed with SHA-256.
fn hash_go_source_tree(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && is_go_source_file(&path) {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut buf: Vec<u8> = Vec::new();
    for file in &files {
        let rel: PathBuf = file.strip_prefix(root).unwrap_or(file).to_path_buf();
        buf.extend_from_slice(rel.to_string_lossy().as_bytes());
        buf.push(0);
        let bytes = std::fs::read(file).map_err(|e| format!("read {}: {e}", file.display()))?;
        buf.extend_from_slice(&bytes.len().to_le_bytes());
        buf.extend_from_slice(&bytes);
        buf.push(0);
    }
    sha256_hex(&buf)
}

/// SHA-256 hex digest via the `sha256sum` command, matching `build.rs`.
fn sha256_hex(data: &[u8]) -> Result<String, String> {
    let mut child = std::process::Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run sha256sum: {e}"))?;

    child
        .stdin
        .as_mut()
        .ok_or("failed to open sha256sum stdin")?
        .write_all(data)
        .map_err(|e| format!("failed to write to sha256sum: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("sha256sum failed: {e}"))?;
    let stdout = String::from_utf8(output.stdout).map_err(|e| format!("invalid utf8: {e}"))?;
    stdout
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| "empty sha256sum output".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn isolated_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("shatter-doctor-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn predicate_includes_go_sources_excludes_tests_and_others() {
        assert!(is_go_source_file(Path::new("/x/main.go")));
        assert!(is_go_source_file(Path::new("/x/go.mod")));
        assert!(is_go_source_file(Path::new("/x/go.sum")));
        // _test.go is excluded: `go build .` ignores it, so it must not affect
        // the staleness hash.
        assert!(!is_go_source_file(Path::new("/x/main_test.go")));
        assert!(!is_go_source_file(Path::new("/x/README.md")));
        assert!(!is_go_source_file(Path::new("/x/Taskfile.yml")));
    }

    #[test]
    fn hash_is_deterministic_and_edit_sensitive() {
        let dir = isolated_dir("hash");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("main.go"), b"package main\n").unwrap();
        std::fs::write(dir.join("go.mod"), b"module x\n").unwrap();
        std::fs::write(dir.join("sub").join("a.go"), b"package sub\n").unwrap();

        let h1 = hash_go_source_tree(&dir).unwrap();
        let h2 = hash_go_source_tree(&dir).unwrap();
        assert_eq!(h1, h2, "hash must be stable for unchanged tree");

        // Editing a source file changes the hash.
        std::fs::write(dir.join("main.go"), b"package main // edit\n").unwrap();
        let h3 = hash_go_source_tree(&dir).unwrap();
        assert_ne!(h1, h3, "edit to a .go file must change the hash");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_ignores_test_files() {
        let dir = isolated_dir("ignore-tests");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.go"), b"package main\n").unwrap();

        let before = hash_go_source_tree(&dir).unwrap();
        // Adding a *_test.go file must NOT change the hash, since it does not
        // enter the built binary.
        std::fs::write(dir.join("main_test.go"), b"package main\n").unwrap();
        let after = hash_go_source_tree(&dir).unwrap();
        assert_eq!(before, after, "test files must not affect the staleness hash");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_detects_added_source_file() {
        let dir = isolated_dir("add-source");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.go"), b"package main\n").unwrap();

        let before = hash_go_source_tree(&dir).unwrap();
        std::fs::write(dir.join("extra.go"), b"package main\n").unwrap();
        let after = hash_go_source_tree(&dir).unwrap();
        assert_ne!(before, after, "adding a .go file must change the hash");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generated_paths_check_skips_when_no_shatter_project() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        // No `.shatter/` dir: the check is not applicable and reports healthy.
        assert!(check_generated_paths_ignored(Some(dir.path()), &colors));
    }

    #[test]
    fn generated_paths_check_flags_unignored_seeds_dir() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        // An initialized project whose hand-written .gitignore covers cache and
        // artifacts but misses the default seeds dir (the refute failure mode).
        std::fs::create_dir_all(dir.path().join(".shatter")).unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            ".shatter-cache/\nshatter-artifacts/\n",
        )
        .unwrap();

        assert!(
            !check_generated_paths_ignored(Some(dir.path()), &colors),
            "an un-ignored generated path must be reported unhealthy"
        );
    }

    #[test]
    fn generated_paths_check_passes_when_all_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        std::fs::create_dir_all(dir.path().join(".shatter")).unwrap();
        // Write the managed block exactly as `shatter init` would.
        let entries = crate::generated_paths::collect_generated_ignore_entries(dir.path());
        crate::generated_paths::sync_gitignore(dir.path(), &entries).unwrap();

        assert!(
            check_generated_paths_ignored(Some(dir.path()), &colors),
            "a fully-ignored project must be reported healthy"
        );
    }
}
