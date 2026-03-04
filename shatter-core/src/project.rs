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
    use std::fs;

    #[test]
    fn detects_typescript_project_via_package_json() {
        // examples/typescript/ has package.json
        let examples_ts = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/typescript/src/01-simple-branch.ts");
        let result = detect_project_root(&examples_ts);
        assert!(result.is_some(), "should detect project root");
        let root = result.unwrap();
        assert_eq!(root.kind, ProjectKind::TypeScript);
        assert!(root.path.join("package.json").exists());
    }

    #[test]
    fn detects_go_project_via_go_mod() {
        // examples/go/ has go.mod
        let examples_go = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/go/01-simple-branch.go");
        let result = detect_project_root(&examples_go);
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
        assert!(result.is_none(), "isolated file should have no project root");
    }

    #[test]
    fn accepts_directory_as_input() {
        let examples_ts = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/typescript");
        let result = detect_project_root(&examples_ts);
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, ProjectKind::TypeScript);
    }
}
