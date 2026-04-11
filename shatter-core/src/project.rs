//! Project root detection for language frontends.
//!
//! Walks up from a target file looking for project marker files (package.json,
//! tsconfig.json, go.mod, Cargo.toml). The detected root is passed to frontends
//! so they can load project configuration (tsconfig, go.sum, etc.).

use std::path::{Path, PathBuf};

/// Marker files checked in priority order during project root detection.
const MARKERS: &[(&str, ProjectKind)] = &[
    ("package.json", ProjectKind::TypeScript),
    ("tsconfig.json", ProjectKind::TypeScript),
    ("go.mod", ProjectKind::Go),
    ("Cargo.toml", ProjectKind::Rust),
];

/// The kind of project detected, derived from which marker file was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    TypeScript,
    Go,
    Rust,
}

/// A detected project root directory with its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub path: PathBuf,
    pub kind: ProjectKind,
}

/// Walk up from `file_path` looking for project marker files.
///
/// Returns the first directory containing a marker, or `None` for isolated
/// files with no project structure. The search starts from the file's parent
/// directory and stops at the filesystem root.
pub fn detect_project_root(file_path: &Path) -> Option<ProjectRoot> {
    let start_dir = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path.parent()?.to_path_buf()
    };

    let mut dir = start_dir;
    loop {
        for &(marker, kind) in MARKERS {
            if dir.join(marker).exists() {
                return Some(ProjectRoot {
                    path: dir.clone(),
                    kind,
                });
            }
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn examples_root() -> PathBuf {
        if let Some(path) = env::var_os("SHATTER_EXAMPLES_DIR") {
            return PathBuf::from(path);
        }

        let fallback = env::temp_dir().join("shatter-examples-main");
        assert!(
            fallback.exists(),
            "examples checkout not found. Set SHATTER_EXAMPLES_DIR or run python3 scripts/examples_checkout.py."
        );
        fallback
    }

    #[test]
    fn detects_typescript_project_via_package_json() {
        // examples/typescript/ has package.json
        let examples_ts = examples_root().join("typescript/src/01-simple-branch.ts");
        let result = detect_project_root(&examples_ts);
        assert!(result.is_some(), "should detect project root");
        let root = result.unwrap();
        assert_eq!(root.kind, ProjectKind::TypeScript);
        assert!(root.path.join("package.json").exists());
    }

    #[test]
    fn detects_go_project_via_go_mod() {
        // Self-contained temp fixture — doesn't depend on external example files
        // that may move or be removed.
        let tmp = tempfile::tempdir().unwrap();
        let go_dir = tmp.path().join("mygoproject");
        fs::create_dir_all(&go_dir).unwrap();
        fs::write(
            go_dir.join("go.mod"),
            "module example.com/test\n\ngo 1.21\n",
        )
        .unwrap();
        let go_file = go_dir.join("main.go");
        fs::write(&go_file, "package main\nfunc main() {}\n").unwrap();

        let result = detect_project_root(&go_file);
        assert!(result.is_some(), "should detect project root");
        let root = result.unwrap();
        assert_eq!(root.kind, ProjectKind::Go);
        assert!(root.path.join("go.mod").exists());
    }

    #[test]
    fn returns_none_for_isolated_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("isolated.ts");
        fs::write(&file, "export function f() {}").unwrap();
        let result = detect_project_root(&file);
        assert!(
            result.is_none(),
            "isolated file should have no project root"
        );
    }

    #[test]
    fn accepts_directory_as_input() {
        let examples_ts = examples_root().join("typescript");
        let result = detect_project_root(&examples_ts);
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, ProjectKind::TypeScript);
    }
}
