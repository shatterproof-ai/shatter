//! Authoritative run status export skeleton.
//!
//! `run-status.json` is the machine-readable entry point for broad-run
//! automation. This initial schema records stable run identity, the source
//! snapshot/manifest identity, and links to existing artifacts. Detailed
//! per-file/per-target rows are intentionally deferred to the follow-up
//! `str-jeen.16` children.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::run_manifest::RunManifest;

/// Filename of the authoritative status export written under a run artifact root.
pub const RUN_STATUS_FILENAME: &str = "run-status.json";

/// On-disk schema version for [`RunStatus`].
pub const RUN_STATUS_SCHEMA_VERSION: u32 = 1;

/// Input needed to build a run status export.
pub struct StatusExportInput<'a> {
    /// User-facing command that produced the run, such as `run` or `scan`.
    pub command: &'a str,
    /// Captured run-start source snapshot.
    pub manifest: &'a RunManifest,
    /// Path to the manifest artifact.
    pub manifest_path: &'a Path,
    /// Existing artifacts this status export should link.
    pub artifacts: &'a [StatusArtifactLink<'a>],
}

/// One artifact link requested by the status export caller.
#[derive(Debug, Clone, Copy)]
pub struct StatusArtifactLink<'a> {
    /// Stable artifact kind token, such as `run_summary` or `scan_summary`.
    pub kind: &'a str,
    /// Path to the artifact.
    pub path: &'a Path,
}

/// Top-level status export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStatus {
    /// Schema version. See [`RUN_STATUS_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Export generation timestamp as nanoseconds since UNIX epoch.
    pub generated_at_ns: u128,
    /// Stable run identity.
    pub run: RunIdentity,
    /// Command/config identity.
    pub command: CommandIdentity,
    /// Manifest artifact link.
    pub manifest: StatusArtifact,
    /// Source snapshot identity copied from the manifest.
    pub source_snapshot: SourceSnapshotIdentity,
    /// Linked artifacts available for downstream consumers.
    pub artifacts: Vec<StatusArtifact>,
}

/// Stable run identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunIdentity {
    /// Scan/run identifier shared by manifest and summary artifacts.
    pub scan_id: String,
}

/// Command and configuration identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    /// Command that produced the artifact set.
    pub name: String,
    /// Stable hash of the scope/config used for the run.
    pub config_hash: String,
}

/// Link to an artifact with its content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusArtifact {
    /// Stable artifact kind token.
    pub kind: String,
    /// Artifact path, relative to the status export directory when possible.
    pub path: String,
    /// SHA-256 of the artifact contents.
    pub sha256: Option<String>,
}

/// Source snapshot identity copied from [`RunManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshotIdentity {
    /// Project root recorded at run start.
    pub project_root: Option<String>,
    /// Git repository root recorded at run start.
    pub repo_root: Option<String>,
    /// Process cwd recorded at run start.
    pub cwd: String,
    /// Git commit recorded at run start.
    pub git_commit: Option<String>,
    /// Whether the tree was dirty at run start.
    pub git_dirty: Option<bool>,
    /// Manifest capture timestamp as nanoseconds since UNIX epoch.
    pub manifest_captured_at_ns: u128,
    /// Number of selected source files in the manifest.
    pub selected_source_files: usize,
    /// Number of selected source lines in the manifest.
    pub selected_source_lines: u64,
}

/// Status export write failure.
#[derive(Debug, Error)]
pub enum StatusExportError {
    /// Artifact read failed.
    #[error("failed to read status artifact '{}': {source}", path.display())]
    ReadArtifact {
        /// Path that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Output directory creation failed.
    #[error("failed to create status output directory '{}': {source}", path.display())]
    CreateOutputDir {
        /// Directory that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Serialization failed.
    #[error("failed to serialize run status: {0}")]
    Serialize(serde_json::Error),
    /// Temp-file write failed.
    #[error("failed to write status temp file '{}': {source}", path.display())]
    WriteTemp {
        /// Temp path that could not be written.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Atomic rename failed.
    #[error("failed to finalize status export '{}': {source}", path.display())]
    Finalize {
        /// Destination path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
}

/// Build a run status export from a manifest and existing artifact links.
#[must_use]
pub fn build_run_status(output_dir: &Path, input: &StatusExportInput<'_>) -> RunStatus {
    let manifest = input.manifest;
    RunStatus {
        schema_version: RUN_STATUS_SCHEMA_VERSION,
        generated_at_ns: now_ns(),
        run: RunIdentity {
            scan_id: manifest.scan_id.clone(),
        },
        command: CommandIdentity {
            name: input.command.to_string(),
            config_hash: manifest.scope_hash.clone(),
        },
        manifest: StatusArtifact {
            kind: "manifest".to_string(),
            path: display_path(output_dir, input.manifest_path),
            sha256: None,
        },
        source_snapshot: SourceSnapshotIdentity {
            project_root: manifest.project_root.clone(),
            repo_root: manifest.repo_root.clone(),
            cwd: manifest.cwd.clone(),
            git_commit: manifest.git_commit.clone(),
            git_dirty: manifest.git_dirty,
            manifest_captured_at_ns: manifest.captured_at_ns,
            selected_source_files: manifest.selected_source_files(),
            selected_source_lines: manifest.selected_source_lines(),
        },
        artifacts: input
            .artifacts
            .iter()
            .map(|artifact| StatusArtifact {
                kind: artifact.kind.to_string(),
                path: display_path(output_dir, artifact.path),
                sha256: None,
            })
            .collect(),
    }
}

/// Write `run-status.json` under `output_dir` using atomic rename.
pub fn write_run_status_json(
    output_dir: &Path,
    input: &StatusExportInput<'_>,
) -> Result<(), StatusExportError> {
    std::fs::create_dir_all(output_dir).map_err(|source| StatusExportError::CreateOutputDir {
        path: output_dir.to_path_buf(),
        source,
    })?;

    let mut status = build_run_status(output_dir, input);
    status.manifest.sha256 = Some(hash_file(input.manifest_path)?);
    for (artifact, link) in status.artifacts.iter_mut().zip(input.artifacts.iter()) {
        artifact.sha256 = Some(hash_file(link.path)?);
    }

    let json = serde_json::to_string_pretty(&status).map_err(StatusExportError::Serialize)?;
    let path = output_dir.join(RUN_STATUS_FILENAME);
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json).map_err(|source| StatusExportError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, &path).map_err(|source| StatusExportError::Finalize {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, StatusExportError> {
    let bytes = std::fs::read(path).map_err(|source| StatusExportError::ReadArtifact {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn display_path(output_dir: &Path, path: &Path) -> String {
    path.strip_prefix(output_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::run_manifest::{RunManifest, RUN_MANIFEST_VERSION};

    #[test]
    fn writes_status_export_skeleton_with_manifest_and_artifact_links() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-status".to_string(),
            project_root: Some(root.display().to_string()),
            repo_root: Some(root.display().to_string()),
            cwd: root.display().to_string(),
            git_commit: Some("abc1234".to_string()),
            git_dirty: Some(false),
            scope_hash: "scope-hash".to_string(),
            source_files: Vec::new(),
            captured_at_ns: 42,
        };

        let manifest_path = root.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("manifest json"),
        )
        .expect("write manifest");
        let summary_path = root.join("run.json");
        fs::write(&summary_path, br#"{"version":1}"#).expect("write run summary");

        write_run_status_json(
            root,
            &StatusExportInput {
                command: "run",
                manifest: &manifest,
                manifest_path: &manifest_path,
                artifacts: &[StatusArtifactLink {
                    kind: "run_summary",
                    path: &summary_path,
                }],
            },
        )
        .expect("write status");

        let status_path = root.join(RUN_STATUS_FILENAME);
        let bytes = fs::read(status_path).expect("read status");
        let status: RunStatus = serde_json::from_slice(&bytes).expect("parse status");

        assert_eq!(status.schema_version, RUN_STATUS_SCHEMA_VERSION);
        assert_eq!(status.run.scan_id, "scan-status");
        assert_eq!(status.command.name, "run");
        assert_eq!(status.command.config_hash, "scope-hash");
        assert_eq!(
            status.source_snapshot.git_commit.as_deref(),
            Some("abc1234")
        );
        assert_eq!(status.source_snapshot.manifest_captured_at_ns, 42);
        assert_eq!(status.manifest.path, "manifest.json");
        assert!(status.manifest.sha256.is_some());
        assert_eq!(status.artifacts[0].kind, "run_summary");
        assert_eq!(status.artifacts[0].path, "run.json");
        assert!(status.artifacts[0].sha256.is_some());
        assert!(status.generated_at_ns > 0);
    }
}
