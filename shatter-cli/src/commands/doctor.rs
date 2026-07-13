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
/// stale embedded frontend is detected (so the caller can exit non-zero).
pub fn run_doctor(colors: &Colors) -> Result<bool, Box<dyn std::error::Error>> {
    crate::helpers::print_stdout(&format!(
        "{}Shatter doctor{}\n\n",
        colors.bold, colors.reset
    ));

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

    // Project configuration report (str-mktn). Printed before the embedded
    // frontend staleness check so it is visible to installed binaries too —
    // integrators run an installed `shatter`, where the staleness section
    // early-returns below.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for line in config_report_lines(&detect_config_presence(&cwd)) {
        crate::helpers::print_stdout(&format!("{line}\n"));
    }
    crate::helpers::print_stdout("\n");

    let source_dir = Path::new(GO_FRONTEND_SOURCE_DIR);
    if !source_dir.is_dir() {
        crate::helpers::print_stdout(
            "Embedded Go frontend: source tree not available — staleness check skipped.\n",
        );
        crate::helpers::print_stdout(
            "  (This is expected for an installed binary outside its source checkout.)\n",
        );
        return Ok(true);
    }

    let current_hash = match hash_go_source_tree(source_dir) {
        Ok(h) => h,
        Err(e) => {
            crate::helpers::print_stdout(&format!(
                "Embedded Go frontend: could not hash source tree ({e}) — staleness check skipped.\n"
            ));
            return Ok(true);
        }
    };

    if current_hash == GO_FRONTEND_SOURCE_HASH {
        crate::helpers::print_stdout("Embedded Go frontend: up to date.\n");
        Ok(true)
    } else {
        crate::helpers::print_stdout(&format!(
            "{}Embedded Go frontend is stale{} — the binary was built from a different \
             `shatter-go/` than what is on disk.\n",
            colors.bold, colors.reset
        ));
        crate::helpers::print_stdout(&format!(
            "  embedded source hash: {GO_FRONTEND_SOURCE_HASH}\n"
        ));
        crate::helpers::print_stdout(&format!("  current  source hash: {current_hash}\n"));
        crate::helpers::print_stdout(
            "  Run `cargo build -p shatter-cli` to rebuild the embedded frontend.\n",
        );
        Ok(false)
    }
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

/// Which of the two project-config files are present in a checkout (str-mktn).
///
/// Shatter reads two distinct config files with non-overlapping ownership;
/// `doctor` reports both so an integrating repo can see which are in effect and
/// how they rank. See the "Project Configuration" section of `README.md`.
struct ConfigPresence {
    /// `shatter.config.json` at the project root (scan-global settings).
    project_json: bool,
    /// `.shatter/config.yaml` at the project root (per-function settings).
    yaml: bool,
}

/// Detect the two project-config files relative to `root`.
fn detect_config_presence(root: &Path) -> ConfigPresence {
    ConfigPresence {
        project_json: root
            .join(shatter_core::config::PROJECT_CONFIG_FILENAME)
            .is_file(),
        yaml: root.join(".shatter").join("config.yaml").is_file(),
    }
}

/// Human-readable lines describing the two config files and their precedence.
///
/// Kept pure (no I/O, no printing) so it can be unit-tested. The precedence
/// line mirrors `README.md`'s "Override Precedence"; keep the two in sync.
fn config_report_lines(presence: &ConfigPresence) -> Vec<String> {
    let mark = |present: bool| if present { "present" } else { "not found" };
    vec![
        "Project configuration".to_string(),
        format!(
            "  shatter.config.json:   {:<9}  scan-global: discovery, output, caching, resource limits",
            mark(presence.project_json)
        ),
        format!(
            "  .shatter/config.yaml:  {:<9}  per-function: iterations, timeouts, mocks, generators, setup, opaque types",
            mark(presence.yaml)
        ),
        "  Precedence: CLI flags > --set overrides > .shatter/config.yaml (nearest wins) \
         > shatter.config.json > built-in defaults"
            .to_string(),
        "  The two files do not overlap; see README \"Project Configuration\".".to_string(),
    ]
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
        std::env::temp_dir().join(format!(
            "shatter-doctor-{name}-{}-{unique}",
            std::process::id()
        ))
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
        assert_eq!(
            before, after,
            "test files must not affect the staleness hash"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_report_marks_present_and_absent_files() {
        let dir = isolated_dir("config-presence");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".shatter")).unwrap();

        // Neither file present.
        let none = detect_config_presence(&dir);
        assert!(!none.project_json && !none.yaml);
        let lines = config_report_lines(&none);
        assert!(lines[1].contains("not found"), "json line: {}", lines[1]);
        assert!(lines[2].contains("not found"), "yaml line: {}", lines[2]);
        // Precedence line is always present so integrators see ordering even
        // when neither file exists.
        assert!(lines.iter().any(|l| l.contains("Precedence: CLI flags")));

        // Both files present.
        std::fs::write(dir.join("shatter.config.json"), b"{}\n").unwrap();
        std::fs::write(dir.join(".shatter").join("config.yaml"), b"defaults: {}\n").unwrap();
        let both = detect_config_presence(&dir);
        assert!(both.project_json && both.yaml);
        let lines = config_report_lines(&both);
        assert!(lines[1].contains("present"), "json line: {}", lines[1]);
        assert!(lines[2].contains("present"), "yaml line: {}", lines[2]);

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
}
