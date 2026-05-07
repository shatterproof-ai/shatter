//! Authoritative run status export skeleton.
//!
//! `run-status.json` is the machine-readable entry point for broad-run
//! automation. This initial schema records stable run identity, the source
//! snapshot/manifest identity, and links to existing artifacts. Detailed
//! per-file rows for each source in the captured manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::run_manifest::RunManifest;
use crate::source_bucket::{SourceBucket, classify_path};

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
    /// Per-file target accounting keyed by manifest path.
    pub files: &'a [StatusFileInput],
}

/// One artifact link requested by the status export caller.
#[derive(Debug, Clone, Copy)]
pub struct StatusArtifactLink<'a> {
    /// Stable artifact kind token, such as `run_summary` or `scan_summary`.
    pub kind: &'a str,
    /// Path to the artifact.
    pub path: &'a Path,
}

/// Per-file status input supplied by the run/scan caller.
#[derive(Debug, Clone)]
pub struct StatusFileInput {
    /// Manifest path for the source file.
    pub path: String,
    /// Number of targets discovered in this file.
    pub discovered_targets: u64,
    /// Number of discovered targets attempted.
    pub attempted_targets: u64,
    /// Number of attempted targets completed successfully.
    pub completed_targets: u64,
    /// Number of attempted targets that failed.
    pub failed_targets: u64,
    /// Number of targets skipped because Shatter cannot support them yet.
    pub unsupported_targets: u64,
    /// File-level status.
    pub status: StatusFileStatus,
}

/// File-level run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusFileStatus {
    /// Every discovered target completed successfully.
    Completed,
    /// Some targets completed and some did not.
    Partial,
    /// Targets were attempted but none completed successfully.
    Failed,
    /// The selected file has no discovered targets.
    NoTarget,
    /// The file is outside the current frontend support boundary.
    Unsupported,
    /// The required frontend was unavailable for this file.
    UnavailableFrontend,
    /// The file was skipped for a non-frontend reason.
    Skipped,
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
    /// One status row per selected source file in the manifest.
    #[serde(default)]
    pub files: Vec<StatusFileRow>,
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

/// Per-source-file status row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusFileRow {
    /// Manifest path for the selected source file.
    pub path: String,
    /// Source language inferred from the file extension when supported.
    pub language: Option<String>,
    /// Frontend selected for this file when supported.
    pub frontend: Option<String>,
    /// Source-set bucket used for denominator accounting.
    pub source_bucket: SourceBucket,
    /// Selected physical line count from the run manifest.
    pub selected_line_count: u64,
    /// Number of targets discovered in this file.
    pub discovered_target_count: u64,
    /// Number of discovered targets attempted.
    pub attempted_target_count: u64,
    /// Number of attempted targets completed successfully.
    pub completed_target_count: u64,
    /// Number of attempted targets that failed.
    pub failed_target_count: u64,
    /// Number of targets skipped because Shatter cannot support them yet.
    pub unsupported_target_count: u64,
    /// File-level status.
    pub status: StatusFileStatus,
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
        files: build_file_rows(manifest, input.files),
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

fn build_file_rows(manifest: &RunManifest, files: &[StatusFileInput]) -> Vec<StatusFileRow> {
    let by_path: BTreeMap<&str, &StatusFileInput> = files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();

    manifest
        .source_files
        .iter()
        .map(|source_file| {
            let source_bucket = classify_path(&source_file.path);
            let frontend = frontend_for_path(&source_file.path);
            let input = by_path.get(source_file.path.as_str()).copied();
            StatusFileRow {
                path: source_file.path.clone(),
                language: frontend.map(|info| info.language.to_string()),
                frontend: frontend.map(|info| info.frontend.to_string()),
                source_bucket,
                selected_line_count: u64::from(source_file.line_count.unwrap_or(0)),
                discovered_target_count: input.map_or(0, |file| file.discovered_targets),
                attempted_target_count: input.map_or(0, |file| file.attempted_targets),
                completed_target_count: input.map_or(0, |file| file.completed_targets),
                failed_target_count: input.map_or(0, |file| file.failed_targets),
                unsupported_target_count: input.map_or(0, |file| file.unsupported_targets),
                status: input
                    .map_or_else(|| default_file_status(source_bucket), |file| file.status),
            }
        })
        .collect()
}

fn default_file_status(source_bucket: SourceBucket) -> StatusFileStatus {
    if source_bucket == SourceBucket::Unsupported {
        StatusFileStatus::Unsupported
    } else {
        StatusFileStatus::NoTarget
    }
}

#[derive(Debug, Clone, Copy)]
struct FrontendInfo {
    language: &'static str,
    frontend: &'static str,
}

fn frontend_for_path(path: &str) -> Option<FrontendInfo> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        return Some(FrontendInfo {
            language: "typescript",
            frontend: "shatter-ts",
        });
    }
    if lower.ends_with(".go") {
        return Some(FrontendInfo {
            language: "go",
            frontend: "shatter-go",
        });
    }
    if lower.ends_with(".rs") {
        return Some(FrontendInfo {
            language: "rust",
            frontend: "shatter-rust",
        });
    }
    None
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
    use crate::run_manifest::{RUN_MANIFEST_VERSION, RunManifest, SourceFileSnapshot};

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
                files: &[],
                targets: &[],
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

    #[test]
    fn writes_per_file_status_rows_for_manifest_sources() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-files".to_string(),
            project_root: Some(root.display().to_string()),
            repo_root: Some(root.display().to_string()),
            cwd: root.display().to_string(),
            git_commit: None,
            git_dirty: Some(false),
            scope_hash: "scope-hash".to_string(),
            source_files: vec![
                source_file("src/app.ts", Some(12)),
                source_file("pkg/handler.go", Some(20)),
                source_file("scripts/build.py", Some(5)),
                source_file("crates/missing.rs", None),
                source_file("src/no_targets.ts", Some(3)),
            ],
            captured_at_ns: 42,
        };

        let manifest_path = root.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("manifest json"),
        )
        .expect("write manifest");

        let status = build_run_status(
            root,
            &StatusExportInput {
                command: "run",
                manifest: &manifest,
                manifest_path: &manifest_path,
                artifacts: &[],
                files: &[
                    StatusFileInput {
                        path: "src/app.ts".to_string(),
                        discovered_targets: 3,
                        attempted_targets: 3,
                        completed_targets: 3,
                        failed_targets: 0,
                        unsupported_targets: 0,
                        status: StatusFileStatus::Completed,
                    },
                    StatusFileInput {
                        path: "pkg/handler.go".to_string(),
                        discovered_targets: 4,
                        attempted_targets: 4,
                        completed_targets: 2,
                        failed_targets: 2,
                        unsupported_targets: 0,
                        status: StatusFileStatus::Partial,
                    },
                    StatusFileInput {
                        path: "crates/missing.rs".to_string(),
                        discovered_targets: 0,
                        attempted_targets: 0,
                        completed_targets: 0,
                        failed_targets: 0,
                        unsupported_targets: 0,
                        status: StatusFileStatus::UnavailableFrontend,
                    },
                ],
                targets: &[],
            },
        );

        assert_eq!(status.files.len(), 5);

        let completed = status
            .files
            .iter()
            .find(|file| file.path == "src/app.ts")
            .expect("completed row");
        assert_eq!(completed.language.as_deref(), Some("typescript"));
        assert_eq!(completed.frontend.as_deref(), Some("shatter-ts"));
        assert_eq!(completed.source_bucket.as_wire_str(), "production_ish");
        assert_eq!(completed.selected_line_count, 12);
        assert_eq!(completed.discovered_target_count, 3);
        assert_eq!(completed.attempted_target_count, 3);
        assert_eq!(completed.completed_target_count, 3);
        assert_eq!(completed.failed_target_count, 0);
        assert_eq!(completed.unsupported_target_count, 0);
        assert_eq!(completed.status, StatusFileStatus::Completed);

        let partial = status
            .files
            .iter()
            .find(|file| file.path == "pkg/handler.go")
            .expect("partial row");
        assert_eq!(partial.language.as_deref(), Some("go"));
        assert_eq!(partial.frontend.as_deref(), Some("shatter-go"));
        assert_eq!(partial.completed_target_count, 2);
        assert_eq!(partial.failed_target_count, 2);
        assert_eq!(partial.status, StatusFileStatus::Partial);

        let unsupported = status
            .files
            .iter()
            .find(|file| file.path == "scripts/build.py")
            .expect("unsupported row");
        assert_eq!(unsupported.language, None);
        assert_eq!(unsupported.frontend, None);
        assert_eq!(unsupported.source_bucket.as_wire_str(), "unsupported");
        assert_eq!(unsupported.selected_line_count, 5);
        assert_eq!(unsupported.status, StatusFileStatus::Unsupported);

        let unavailable = status
            .files
            .iter()
            .find(|file| file.path == "crates/missing.rs")
            .expect("unavailable row");
        assert_eq!(unavailable.language.as_deref(), Some("rust"));
        assert_eq!(unavailable.frontend.as_deref(), Some("shatter-rust"));
        assert_eq!(unavailable.selected_line_count, 0);
        assert_eq!(unavailable.status, StatusFileStatus::UnavailableFrontend);

        let no_targets = status
            .files
            .iter()
            .find(|file| file.path == "src/no_targets.ts")
            .expect("no target row");
        assert_eq!(no_targets.discovered_target_count, 0);
        assert_eq!(no_targets.status, StatusFileStatus::NoTarget);
    }

    #[test]
    fn writes_per_target_status_rows_and_validates_artifacts() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-targets".to_string(),
            project_root: Some(root.display().to_string()),
            repo_root: Some(root.display().to_string()),
            cwd: root.display().to_string(),
            git_commit: None,
            git_dirty: Some(false),
            scope_hash: "scope-hash".to_string(),
            source_files: vec![
                source_file("src/app.ts", Some(12)),
                source_file("pkg/handler.go", Some(20)),
                source_file("crates/missing.rs", None),
            ],
            captured_at_ns: 42,
        };
        let manifest_path = root.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("manifest json"),
        )
        .expect("write manifest");
        let artifact_dir = root.join("functions");
        fs::create_dir_all(&artifact_dir).expect("artifact dir");
        let completed_artifact = artifact_dir.join("00001_src_app_ts_doThing.json");
        fs::write(&completed_artifact, br#"{"ok":true}"#).expect("write artifact");

        write_run_status_json(
            root,
            &StatusExportInput {
                command: "scan",
                manifest: &manifest,
                manifest_path: &manifest_path,
                artifacts: &[],
                files: &[],
                targets: &[
                    StatusTargetInput {
                        target_id: "src/app.ts::doThing".to_string(),
                        name: "doThing".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 3,
                        end_line: 8,
                        outcome: StatusTargetOutcome::Completed,
                        artifact_path: Some(completed_artifact.clone()),
                        failure_reason: None,
                        unavailable_reason: None,
                        validity_impact: StatusTargetValidityImpact::Contributes,
                    },
                    StatusTargetInput {
                        target_id: "pkg/handler.go::Handle".to_string(),
                        name: "Handle".to_string(),
                        source_file: "pkg/handler.go".to_string(),
                        start_line: 10,
                        end_line: 20,
                        outcome: StatusTargetOutcome::Failed,
                        artifact_path: None,
                        failure_reason: Some("runtime failed".to_string()),
                        unavailable_reason: Some("artifact unavailable after failure".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                    StatusTargetInput {
                        target_id: "pkg/handler.go::Unsupported".to_string(),
                        name: "Unsupported".to_string(),
                        source_file: "pkg/handler.go".to_string(),
                        start_line: 30,
                        end_line: 35,
                        outcome: StatusTargetOutcome::Unsupported,
                        artifact_path: None,
                        failure_reason: Some("unsupported parameter type".to_string()),
                        unavailable_reason: Some("unsupported target".to_string()),
                        validity_impact: StatusTargetValidityImpact::Excluded,
                    },
                    StatusTargetInput {
                        target_id: "crates/missing.rs::needsFrontend".to_string(),
                        name: "needsFrontend".to_string(),
                        source_file: "crates/missing.rs".to_string(),
                        start_line: 1,
                        end_line: 1,
                        outcome: StatusTargetOutcome::UnavailableFrontend,
                        artifact_path: None,
                        failure_reason: Some("frontend preflight failed".to_string()),
                        unavailable_reason: Some("frontend unavailable".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                ],
            },
        )
        .expect("write status");

        let status_path = root.join(RUN_STATUS_FILENAME);
        let bytes = fs::read(status_path).expect("read status");
        let status: RunStatus = serde_json::from_slice(&bytes).expect("parse status");
        assert_eq!(status.targets.len(), 4);

        let completed = status
            .targets
            .iter()
            .find(|target| target.target_id == "src/app.ts::doThing")
            .expect("completed target");
        assert_eq!(completed.name, "doThing");
        assert_eq!(completed.source_file, "src/app.ts");
        assert_eq!(completed.language.as_deref(), Some("typescript"));
        assert_eq!(completed.frontend.as_deref(), Some("shatter-ts"));
        assert_eq!(completed.start_line, 3);
        assert_eq!(completed.end_line, 8);
        assert_eq!(completed.outcome, StatusTargetOutcome::Completed);
        assert_eq!(completed.validity_impact, StatusTargetValidityImpact::Contributes);
        let artifact = completed.artifact.as_ref().expect("completed artifact");
        assert_eq!(artifact.path.as_deref(), Some("functions/00001_src_app_ts_doThing.json"));
        assert!(artifact.sha256.is_some());
        assert_eq!(artifact.unavailable_reason, None);

        let failed = status
            .targets
            .iter()
            .find(|target| target.target_id == "pkg/handler.go::Handle")
            .expect("failed target");
        assert_eq!(failed.outcome, StatusTargetOutcome::Failed);
        assert_eq!(failed.failure_reason.as_deref(), Some("runtime failed"));
        assert_eq!(
            failed
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.unavailable_reason.as_deref()),
            Some("artifact unavailable after failure")
        );

        let unsupported = status
            .targets
            .iter()
            .find(|target| target.target_id == "pkg/handler.go::Unsupported")
            .expect("unsupported target");
        assert_eq!(unsupported.outcome, StatusTargetOutcome::Unsupported);
        assert_eq!(unsupported.validity_impact, StatusTargetValidityImpact::Excluded);

        let unavailable = status
            .targets
            .iter()
            .find(|target| target.target_id == "crates/missing.rs::needsFrontend")
            .expect("unavailable target");
        assert_eq!(unavailable.outcome, StatusTargetOutcome::UnavailableFrontend);
        assert_eq!(unavailable.language.as_deref(), Some("rust"));
        assert_eq!(unavailable.frontend.as_deref(), Some("shatter-rust"));
    }

    #[test]
    fn rejects_missing_target_artifact_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-missing-target-artifact".to_string(),
            project_root: Some(root.display().to_string()),
            repo_root: Some(root.display().to_string()),
            cwd: root.display().to_string(),
            git_commit: None,
            git_dirty: Some(false),
            scope_hash: "scope-hash".to_string(),
            source_files: vec![source_file("src/app.ts", Some(12))],
            captured_at_ns: 42,
        };
        let manifest_path = root.join("manifest.json");
        fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("manifest json"),
        )
        .expect("write manifest");

        let err = write_run_status_json(
            root,
            &StatusExportInput {
                command: "scan",
                manifest: &manifest,
                manifest_path: &manifest_path,
                artifacts: &[],
                files: &[],
                targets: &[StatusTargetInput {
                    target_id: "src/app.ts::doThing".to_string(),
                    name: "doThing".to_string(),
                    source_file: "src/app.ts".to_string(),
                    start_line: 3,
                    end_line: 8,
                    outcome: StatusTargetOutcome::Completed,
                    artifact_path: Some(root.join("functions/missing.json")),
                    failure_reason: None,
                    unavailable_reason: None,
                    validity_impact: StatusTargetValidityImpact::Contributes,
                }],
            },
        )
        .expect_err("missing target artifact must fail status export");

        assert!(matches!(err, StatusExportError::ReadArtifact { .. }));
        assert!(
            !root.join(RUN_STATUS_FILENAME).exists(),
            "status export must not be finalized when target artifact validation fails"
        );
    }

    fn source_file(path: &str, line_count: Option<u32>) -> SourceFileSnapshot {
        SourceFileSnapshot {
            path: path.to_string(),
            size: 1,
            mtime_ns: Some(10),
            content_hash: Some("hash".to_string()),
            line_count,
        }
    }
}
