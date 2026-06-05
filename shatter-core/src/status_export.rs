//! Authoritative run status export skeleton.
//!
//! `run-status.json` is the machine-readable entry point for broad-run
//! automation. This initial schema records stable run identity, the source
//! snapshot/manifest identity, and links to existing artifacts. Detailed
//! per-file rows for each source in the captured manifest, and
//! per-target rows for discovered or attempted targets.

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

/// Filename of the tab-separated projection written beside [`RUN_STATUS_FILENAME`].
pub const RUN_STATUS_TSV_FILENAME: &str = "run-status.tsv";

/// Stable column order for [`RUN_STATUS_TSV_FILENAME`].
///
/// Rows use `row_type` to distinguish the single run rollup row from file
/// and target projection rows. Empty cells mean the JSON field is absent or
/// not applicable for that row type.
pub const RUN_STATUS_TSV_COLUMNS: [&str; 38] = [
    "schema_version",
    "scan_id",
    "command",
    "config_hash",
    "row_type",
    "file_path",
    "language",
    "frontend",
    "source_bucket",
    "selected_line_count",
    "file_status",
    "discovered_target_count",
    "attempted_target_count",
    "completed_target_count",
    "failed_target_count",
    "unsupported_target_count",
    "target_id",
    "target_name",
    "start_line",
    "end_line",
    "target_outcome",
    "failure_reason",
    "validity_impact",
    "artifact_path",
    "artifact_sha256",
    "artifact_unavailable_reason",
    "report_validity",
    "validity_reason_codes",
    "selected_source_files",
    "selected_source_lines",
    "represented_source_lines",
    "unrepresented_failed_lines",
    "unrepresented_timed_out_lines",
    "unrepresented_unsupported_lines",
    "unrepresented_unavailable_frontend_lines",
    "unrepresented_no_target_lines",
    "unrepresented_undiscovered_lines",
    "source_set_hash",
];

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
    /// Per-target outcome rows.
    pub targets: &'a [StatusTargetInput],
    /// Optional caller-supplied rollup values that cannot be derived from
    /// manifest/file/target rows without re-running command-specific logic.
    pub rollups: StatusRollupInput,
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

/// Per-target status input supplied by the run/scan caller.
#[derive(Debug, Clone)]
pub struct StatusTargetInput {
    /// Stable target identity, usually `<source>::<function>`.
    pub target_id: String,
    /// Human-facing target name.
    pub name: String,
    /// Manifest source path for the target.
    pub source_file: String,
    /// First source line covered by the target.
    pub start_line: u32,
    /// Last source line covered by the target.
    pub end_line: u32,
    /// Target-level outcome.
    pub outcome: StatusTargetOutcome,
    /// Per-target artifact path, if an artifact was written.
    pub artifact_path: Option<PathBuf>,
    /// Failure or skip reason, if any.
    pub failure_reason: Option<String>,
    /// Reason the artifact is unavailable when no path exists.
    pub unavailable_reason: Option<String>,
    /// How this target affects report validity.
    pub validity_impact: StatusTargetValidityImpact,
}

/// Target-level outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusTargetOutcome {
    /// Target completed successfully and has an artifact.
    Completed,
    /// Target was attempted and failed.
    Failed,
    /// Target timed out during execution.
    TimedOut,
    /// Target was discovered but unsupported by current execution semantics.
    Unsupported,
    /// Target was skipped for a non-frontend reason.
    Skipped,
    /// Required frontend was unavailable.
    UnavailableFrontend,
}

/// Validity impact of a target outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusTargetValidityImpact {
    /// Target contributes represented evidence.
    Contributes,
    /// Target degrades validity because it failed or was unavailable.
    Degrades,
    /// Target is excluded from the validity denominator.
    Excluded,
}

/// Optional rollup inputs supplied by a command-specific caller.
#[derive(Debug, Clone, Default)]
pub struct StatusRollupInput {
    /// Report validity computed by the command's existing classifier.
    pub report_validity: Option<StatusReportValidity>,
    /// Machine-readable validity reason codes/details.
    pub validity_reasons: Vec<StatusValidityReason>,
    /// Line-weighted impact buckets from an existing command report.
    pub line_weighted_failure_impact: Option<StatusLineWeightedFailureImpact>,
    /// Reserved optional gate-decision slots for future threshold work.
    pub gate_decisions: Option<Vec<StatusGateDecision>>,
}

/// Report-level validity tier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusReportValidity {
    /// Report has no known validity degradation.
    #[default]
    High,
    /// Report is usable but materially incomplete.
    Degraded,
    /// Report has too little representation to treat as reliable.
    Low,
    /// Source files changed between manifest capture and run completion.
    StaleSourceSet,
    /// Referenced artifacts were missing or inconsistent.
    InvalidArtifacts,
}

/// One reason explaining a non-high report validity tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusValidityReason {
    /// Stable snake_case reason token.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
    /// Suggested operator action.
    pub recommended_action: String,
}

/// Reserved gate-decision record for future threshold gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusGateDecision {
    /// Stable gate name.
    pub gate: String,
    /// Stable gate status token.
    pub status: String,
    /// Threshold value, when numeric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<String>,
    /// Observed value, when numeric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<String>,
    /// Optional human-readable reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
    /// One status row per discovered or attempted target.
    #[serde(default)]
    pub targets: Vec<StatusTargetRow>,
    /// Broad-run rollup metrics derived from authoritative status rows and
    /// command-specific summary data.
    pub rollups: StatusRollups,
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
    /// SHA-256 of sorted `"<path>:<content_hash>"` pairs for all selected
    /// source files at run start (derived from the run manifest).
    pub source_set_hash: String,
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

/// Per-target status row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusTargetRow {
    /// Stable target identity, usually `<source>::<function>`.
    pub target_id: String,
    /// Human-facing target name.
    pub name: String,
    /// Manifest source path for the target.
    pub source_file: String,
    /// Source language inferred from the file extension when supported.
    pub language: Option<String>,
    /// Frontend selected for this target when supported.
    pub frontend: Option<String>,
    /// First source line covered by the target.
    pub start_line: u32,
    /// Last source line covered by the target.
    pub end_line: u32,
    /// Target-level outcome.
    pub outcome: StatusTargetOutcome,
    /// Failure or skip reason, if any.
    pub failure_reason: Option<String>,
    /// How this target affects report validity.
    pub validity_impact: StatusTargetValidityImpact,
    /// Target artifact contract.
    pub artifact: Option<StatusTargetArtifact>,
}

/// Target artifact path or structured unavailability reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusTargetArtifact {
    /// Artifact path, relative to the status export directory when possible.
    pub path: Option<String>,
    /// SHA-256 of the artifact contents.
    pub sha256: Option<String>,
    /// Reason no artifact exists for this target.
    pub unavailable_reason: Option<String>,
}

/// Broad-run status rollups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusRollups {
    /// Source and target denominators for broad-run analysis.
    pub source_denominators: StatusSourceDenominators,
    /// Selected source files and lines per path-classification bucket.
    pub source_buckets: Vec<StatusSourceBucketRollup>,
    /// Report validity tier and reason codes.
    pub validity: StatusValidityRollup,
    /// Frontend availability and preflight counts.
    pub frontend_availability: Vec<StatusFrontendAvailabilityRollup>,
    /// Line-weighted representation/failure impact buckets.
    pub line_weighted_failure_impact: StatusLineWeightedFailureImpact,
    /// Reserved optional threshold gate decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_decisions: Option<Vec<StatusGateDecision>>,
}

/// Source and target denominator rollups.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSourceDenominators {
    /// Files captured in the run manifest.
    pub selected_source_files: usize,
    /// Lines captured in the run manifest.
    pub selected_source_lines: u64,
    /// File rows represented in the status export.
    pub status_file_rows: usize,
    /// Targets discovered by analysis.
    pub discovered_targets: u64,
    /// Targets attempted by exploration.
    pub attempted_targets: u64,
    /// Targets completed successfully.
    pub completed_targets: u64,
    /// Targets that failed or timed out after attempt.
    pub failed_targets: u64,
    /// Targets excluded as unsupported.
    pub unsupported_targets: u64,
    /// Targets skipped for non-frontend reasons.
    pub skipped_targets: u64,
    /// Targets blocked by frontend/preflight availability.
    pub unavailable_frontend_targets: u64,
}

/// Selected source-set totals for one bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSourceBucketRollup {
    /// Source-set bucket.
    pub source_bucket: SourceBucket,
    /// Selected manifest files in this bucket.
    pub selected_file_count: usize,
    /// Selected manifest lines in this bucket.
    pub selected_line_count: u64,
}

/// Report validity rollup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusValidityRollup {
    /// Report-level validity tier.
    pub report_validity: StatusReportValidity,
    /// Machine-readable reasons for the tier.
    pub reasons: Vec<StatusValidityReason>,
}

/// Frontend availability rollup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusFrontendAvailabilityRollup {
    /// Source language handled by this frontend.
    pub language: String,
    /// Frontend implementation name.
    pub frontend: String,
    /// Selected manifest files routed to this frontend.
    pub selected_file_count: usize,
    /// Selected manifest lines routed to this frontend.
    pub selected_line_count: u64,
    /// Status target rows routed to this frontend.
    pub target_count: u64,
    /// Targets blocked by unavailable frontend/preflight.
    pub unavailable_target_count: u64,
    /// Targets whose reason indicates a preflight failure.
    pub preflight_failed_target_count: u64,
}

/// Line-weighted source representation and failure impact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusLineWeightedFailureImpact {
    /// Selected source lines represented by successful exploration.
    pub represented_source_lines: u64,
    /// Selected source lines blocked by ordinary failures.
    pub unrepresented_failed_lines: u64,
    /// Selected source lines blocked by timeouts.
    pub unrepresented_timed_out_lines: u64,
    /// Selected source lines excluded as unsupported.
    pub unrepresented_unsupported_lines: u64,
    /// Selected source lines blocked by frontend/preflight availability.
    pub unrepresented_unavailable_frontend_lines: u64,
    /// Selected source lines in files with no discovered targets.
    pub unrepresented_no_target_lines: u64,
    /// Selected source lines outside discovered spans.
    pub unrepresented_undiscovered_lines: u64,
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
            source_set_hash: compute_manifest_source_set_hash(manifest),
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
        targets: build_target_rows(output_dir, input.targets),
        rollups: build_rollups(manifest, input.files, input.targets, &input.rollups),
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

/// Write `run-status.json` and its TSV projection under `output_dir` using
/// atomic rename for each artifact.
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
    for (target, input_target) in status.targets.iter_mut().zip(input.targets.iter()) {
        if let Some(path) = input_target.artifact_path.as_deref()
            && let Some(artifact) = target.artifact.as_mut()
        {
            artifact.sha256 = Some(hash_file(path)?);
        }
    }

    let json = serde_json::to_string_pretty(&status).map_err(StatusExportError::Serialize)?;
    let path = output_dir.join(RUN_STATUS_FILENAME);
    write_atomic_status_file(&path, &json)?;

    let tsv = render_run_status_tsv(&status);
    let tsv_path = output_dir.join(RUN_STATUS_TSV_FILENAME);
    write_atomic_status_file(&tsv_path, &tsv)?;
    Ok(())
}

/// Render the stable TSV projection of an already-built [`RunStatus`].
///
/// The renderer does not derive status semantics of its own. It flattens the
/// JSON-facing status value into one run row, one row per file, and one row
/// per target using [`RUN_STATUS_TSV_COLUMNS`]. Tabs and line breaks in cell
/// values are normalized to spaces so each JSON row remains one TSV row.
#[must_use]
pub fn render_run_status_tsv(status: &RunStatus) -> String {
    let mut lines = Vec::with_capacity(status.files.len().saturating_add(status.targets.len()) + 2);
    lines.push(RUN_STATUS_TSV_COLUMNS.join("\t"));

    let mut run_row = base_tsv_row(status, "run");
    add_rollup_tsv_cells(&mut run_row, status);
    lines.push(render_tsv_row(&run_row));

    for file in &status.files {
        let mut row = base_tsv_row(status, "file");
        row.insert("file_path", file.path.clone());
        row.insert("language", file.language.clone().unwrap_or_default());
        row.insert("frontend", file.frontend.clone().unwrap_or_default());
        row.insert(
            "source_bucket",
            file.source_bucket.as_wire_str().to_string(),
        );
        row.insert("selected_line_count", file.selected_line_count.to_string());
        row.insert("file_status", file_status_wire(file.status).to_string());
        row.insert(
            "discovered_target_count",
            file.discovered_target_count.to_string(),
        );
        row.insert(
            "attempted_target_count",
            file.attempted_target_count.to_string(),
        );
        row.insert(
            "completed_target_count",
            file.completed_target_count.to_string(),
        );
        row.insert("failed_target_count", file.failed_target_count.to_string());
        row.insert(
            "unsupported_target_count",
            file.unsupported_target_count.to_string(),
        );
        lines.push(render_tsv_row(&row));
    }

    for target in &status.targets {
        let mut row = base_tsv_row(status, "target");
        row.insert("file_path", target.source_file.clone());
        row.insert("language", target.language.clone().unwrap_or_default());
        row.insert("frontend", target.frontend.clone().unwrap_or_default());
        row.insert("target_id", target.target_id.clone());
        row.insert("target_name", target.name.clone());
        row.insert("start_line", target.start_line.to_string());
        row.insert("end_line", target.end_line.to_string());
        row.insert(
            "target_outcome",
            target_outcome_wire(target.outcome).to_string(),
        );
        row.insert(
            "failure_reason",
            target.failure_reason.clone().unwrap_or_default(),
        );
        row.insert(
            "validity_impact",
            validity_impact_wire(target.validity_impact).to_string(),
        );
        if let Some(artifact) = &target.artifact {
            row.insert("artifact_path", artifact.path.clone().unwrap_or_default());
            row.insert(
                "artifact_sha256",
                artifact.sha256.clone().unwrap_or_default(),
            );
            row.insert(
                "artifact_unavailable_reason",
                artifact.unavailable_reason.clone().unwrap_or_default(),
            );
        }
        lines.push(render_tsv_row(&row));
    }

    lines.push(String::new());
    lines.join("\n")
}

fn write_atomic_status_file(path: &Path, contents: &str) -> Result<(), StatusExportError> {
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("status")
    ));
    std::fs::write(&tmp_path, contents).map_err(|source| StatusExportError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, path).map_err(|source| StatusExportError::Finalize {
        path: path.to_path_buf(),
        source,
    })
}

fn base_tsv_row(status: &RunStatus, row_type: &'static str) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("schema_version", status.schema_version.to_string()),
        ("scan_id", status.run.scan_id.clone()),
        ("command", status.command.name.clone()),
        ("config_hash", status.command.config_hash.clone()),
        ("source_set_hash", status.command.source_set_hash.clone()),
        ("row_type", row_type.to_string()),
    ])
}

fn add_rollup_tsv_cells(row: &mut BTreeMap<&'static str, String>, status: &RunStatus) {
    let denominators = status.rollups.source_denominators;
    let impact = status.rollups.line_weighted_failure_impact;
    row.insert(
        "report_validity",
        report_validity_wire(status.rollups.validity.report_validity).to_string(),
    );
    row.insert(
        "validity_reason_codes",
        status
            .rollups
            .validity
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    row.insert(
        "selected_source_files",
        denominators.selected_source_files.to_string(),
    );
    row.insert(
        "selected_source_lines",
        denominators.selected_source_lines.to_string(),
    );
    row.insert(
        "represented_source_lines",
        impact.represented_source_lines.to_string(),
    );
    row.insert(
        "unrepresented_failed_lines",
        impact.unrepresented_failed_lines.to_string(),
    );
    row.insert(
        "unrepresented_timed_out_lines",
        impact.unrepresented_timed_out_lines.to_string(),
    );
    row.insert(
        "unrepresented_unsupported_lines",
        impact.unrepresented_unsupported_lines.to_string(),
    );
    row.insert(
        "unrepresented_unavailable_frontend_lines",
        impact.unrepresented_unavailable_frontend_lines.to_string(),
    );
    row.insert(
        "unrepresented_no_target_lines",
        impact.unrepresented_no_target_lines.to_string(),
    );
    row.insert(
        "unrepresented_undiscovered_lines",
        impact.unrepresented_undiscovered_lines.to_string(),
    );
}

fn render_tsv_row(row: &BTreeMap<&'static str, String>) -> String {
    RUN_STATUS_TSV_COLUMNS
        .iter()
        .map(|column| tsv_cell(row.get(column).map_or("", String::as_str)))
        .collect::<Vec<_>>()
        .join("\t")
}

fn tsv_cell(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            _ => character,
        })
        .collect()
}

fn file_status_wire(status: StatusFileStatus) -> &'static str {
    match status {
        StatusFileStatus::Completed => "completed",
        StatusFileStatus::Partial => "partial",
        StatusFileStatus::Failed => "failed",
        StatusFileStatus::NoTarget => "no-target",
        StatusFileStatus::Unsupported => "unsupported",
        StatusFileStatus::UnavailableFrontend => "unavailable-frontend",
        StatusFileStatus::Skipped => "skipped",
    }
}

fn target_outcome_wire(outcome: StatusTargetOutcome) -> &'static str {
    match outcome {
        StatusTargetOutcome::Completed => "completed",
        StatusTargetOutcome::Failed => "failed",
        StatusTargetOutcome::TimedOut => "timed-out",
        StatusTargetOutcome::Unsupported => "unsupported",
        StatusTargetOutcome::Skipped => "skipped",
        StatusTargetOutcome::UnavailableFrontend => "unavailable-frontend",
    }
}

fn validity_impact_wire(impact: StatusTargetValidityImpact) -> &'static str {
    match impact {
        StatusTargetValidityImpact::Contributes => "contributes",
        StatusTargetValidityImpact::Degrades => "degrades",
        StatusTargetValidityImpact::Excluded => "excluded",
    }
}

fn report_validity_wire(validity: StatusReportValidity) -> &'static str {
    match validity {
        StatusReportValidity::High => "high",
        StatusReportValidity::Degraded => "degraded",
        StatusReportValidity::Low => "low",
        StatusReportValidity::StaleSourceSet => "stale-source-set",
        StatusReportValidity::InvalidArtifacts => "invalid-artifacts",
    }
}

fn build_rollups(
    manifest: &RunManifest,
    files: &[StatusFileInput],
    targets: &[StatusTargetInput],
    input: &StatusRollupInput,
) -> StatusRollups {
    StatusRollups {
        source_denominators: build_source_denominators(manifest, files, targets),
        source_buckets: build_source_bucket_rollups(manifest),
        validity: StatusValidityRollup {
            report_validity: input.report_validity.unwrap_or_default(),
            reasons: input.validity_reasons.clone(),
        },
        frontend_availability: build_frontend_availability(manifest, targets),
        line_weighted_failure_impact: input
            .line_weighted_failure_impact
            .unwrap_or_else(|| derive_line_weighted_failure_impact(manifest, targets)),
        gate_decisions: input.gate_decisions.clone(),
    }
}

fn build_source_denominators(
    manifest: &RunManifest,
    files: &[StatusFileInput],
    targets: &[StatusTargetInput],
) -> StatusSourceDenominators {
    let discovered_from_files: u64 = files.iter().map(|file| file.discovered_targets).sum();
    let attempted_from_files: u64 = files.iter().map(|file| file.attempted_targets).sum();
    let completed_from_files: u64 = files.iter().map(|file| file.completed_targets).sum();
    let failed_from_files: u64 = files.iter().map(|file| file.failed_targets).sum();
    let unsupported_from_files: u64 = files.iter().map(|file| file.unsupported_targets).sum();

    let target_discovered = targets.len() as u64;
    let target_attempted = targets
        .iter()
        .filter(|target| {
            matches!(
                target.outcome,
                StatusTargetOutcome::Completed
                    | StatusTargetOutcome::Failed
                    | StatusTargetOutcome::TimedOut
            )
        })
        .count() as u64;
    let target_completed = targets
        .iter()
        .filter(|target| target.outcome == StatusTargetOutcome::Completed)
        .count() as u64;
    let target_failed = targets
        .iter()
        .filter(|target| {
            matches!(
                target.outcome,
                StatusTargetOutcome::Failed | StatusTargetOutcome::TimedOut
            )
        })
        .count() as u64;
    let target_unsupported = targets
        .iter()
        .filter(|target| target.outcome == StatusTargetOutcome::Unsupported)
        .count() as u64;

    StatusSourceDenominators {
        selected_source_files: manifest.selected_source_files(),
        selected_source_lines: manifest.selected_source_lines(),
        status_file_rows: files.len(),
        discovered_targets: prefer_file_count(discovered_from_files, files, target_discovered),
        attempted_targets: prefer_file_count(attempted_from_files, files, target_attempted),
        completed_targets: prefer_file_count(completed_from_files, files, target_completed),
        failed_targets: prefer_file_count(failed_from_files, files, target_failed),
        unsupported_targets: prefer_file_count(unsupported_from_files, files, target_unsupported),
        skipped_targets: targets
            .iter()
            .filter(|target| target.outcome == StatusTargetOutcome::Skipped)
            .count() as u64,
        unavailable_frontend_targets: targets
            .iter()
            .filter(|target| target.outcome == StatusTargetOutcome::UnavailableFrontend)
            .count() as u64,
    }
}

fn prefer_file_count(file_count: u64, files: &[StatusFileInput], target_count: u64) -> u64 {
    if files.is_empty() {
        target_count
    } else {
        file_count
    }
}

fn build_source_bucket_rollups(manifest: &RunManifest) -> Vec<StatusSourceBucketRollup> {
    const SOURCE_BUCKETS: [SourceBucket; 7] = [
        SourceBucket::ProductionIsh,
        SourceBucket::TestSpec,
        SourceBucket::Generated,
        SourceBucket::DeclarationOnly,
        SourceBucket::FixtureSample,
        SourceBucket::PolicyExcluded,
        SourceBucket::Unsupported,
    ];

    SOURCE_BUCKETS
        .iter()
        .map(|bucket| {
            let mut selected_file_count = 0usize;
            let mut selected_line_count = 0u64;
            for source_file in &manifest.source_files {
                if classify_path(&source_file.path) == *bucket {
                    selected_file_count = selected_file_count.saturating_add(1);
                    selected_line_count = selected_line_count
                        .saturating_add(u64::from(source_file.line_count.unwrap_or(0)));
                }
            }
            StatusSourceBucketRollup {
                source_bucket: *bucket,
                selected_file_count,
                selected_line_count,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
struct FrontendAvailabilityBuilder {
    language: String,
    frontend: String,
    selected_file_count: usize,
    selected_line_count: u64,
    target_count: u64,
    unavailable_target_count: u64,
    preflight_failed_target_count: u64,
}

fn build_frontend_availability(
    manifest: &RunManifest,
    targets: &[StatusTargetInput],
) -> Vec<StatusFrontendAvailabilityRollup> {
    let mut by_frontend: BTreeMap<String, FrontendAvailabilityBuilder> = BTreeMap::new();
    for source_file in &manifest.source_files {
        let Some(info) = frontend_for_path(&source_file.path) else {
            continue;
        };
        let builder = by_frontend
            .entry(info.frontend.to_string())
            .or_insert_with(|| FrontendAvailabilityBuilder {
                language: info.language.to_string(),
                frontend: info.frontend.to_string(),
                ..FrontendAvailabilityBuilder::default()
            });
        builder.selected_file_count = builder.selected_file_count.saturating_add(1);
        builder.selected_line_count = builder
            .selected_line_count
            .saturating_add(u64::from(source_file.line_count.unwrap_or(0)));
    }

    for target in targets {
        let Some(info) = frontend_for_path(&target.source_file) else {
            continue;
        };
        let builder = by_frontend
            .entry(info.frontend.to_string())
            .or_insert_with(|| FrontendAvailabilityBuilder {
                language: info.language.to_string(),
                frontend: info.frontend.to_string(),
                ..FrontendAvailabilityBuilder::default()
            });
        builder.target_count = builder.target_count.saturating_add(1);
        if target.outcome == StatusTargetOutcome::UnavailableFrontend {
            builder.unavailable_target_count = builder.unavailable_target_count.saturating_add(1);
        }
        if target
            .failure_reason
            .as_deref()
            .is_some_and(is_preflight_reason)
            || target
                .unavailable_reason
                .as_deref()
                .is_some_and(is_preflight_reason)
        {
            builder.preflight_failed_target_count =
                builder.preflight_failed_target_count.saturating_add(1);
        }
    }

    by_frontend
        .into_values()
        .map(|builder| StatusFrontendAvailabilityRollup {
            language: builder.language,
            frontend: builder.frontend,
            selected_file_count: builder.selected_file_count,
            selected_line_count: builder.selected_line_count,
            target_count: builder.target_count,
            unavailable_target_count: builder.unavailable_target_count,
            preflight_failed_target_count: builder.preflight_failed_target_count,
        })
        .collect()
}

fn derive_line_weighted_failure_impact(
    manifest: &RunManifest,
    targets: &[StatusTargetInput],
) -> StatusLineWeightedFailureImpact {
    let mut impact = StatusLineWeightedFailureImpact::default();
    for target in targets {
        let lines = u64::from(target_span_line_count(target));
        match target.outcome {
            StatusTargetOutcome::Completed => {
                impact.represented_source_lines =
                    impact.represented_source_lines.saturating_add(lines);
            }
            StatusTargetOutcome::Failed => {
                impact.unrepresented_failed_lines =
                    impact.unrepresented_failed_lines.saturating_add(lines);
            }
            StatusTargetOutcome::TimedOut => {
                impact.unrepresented_timed_out_lines =
                    impact.unrepresented_timed_out_lines.saturating_add(lines);
            }
            StatusTargetOutcome::Unsupported => {
                impact.unrepresented_unsupported_lines =
                    impact.unrepresented_unsupported_lines.saturating_add(lines);
            }
            StatusTargetOutcome::UnavailableFrontend => {
                impact.unrepresented_unavailable_frontend_lines = impact
                    .unrepresented_unavailable_frontend_lines
                    .saturating_add(lines);
            }
            StatusTargetOutcome::Skipped => {}
        }
    }

    let target_source_files: std::collections::BTreeSet<&str> = targets
        .iter()
        .map(|target| target.source_file.as_str())
        .collect();
    for source_file in &manifest.source_files {
        if !target_source_files.contains(source_file.path.as_str()) {
            impact.unrepresented_no_target_lines = impact
                .unrepresented_no_target_lines
                .saturating_add(u64::from(source_file.line_count.unwrap_or(0)));
        }
    }
    impact
}

fn target_span_line_count(target: &StatusTargetInput) -> u32 {
    if target.start_line == 0 || target.end_line < target.start_line {
        0
    } else {
        target.end_line - target.start_line + 1
    }
}

fn is_preflight_reason(reason: &str) -> bool {
    reason.to_ascii_lowercase().contains("preflight")
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

fn build_target_rows(output_dir: &Path, targets: &[StatusTargetInput]) -> Vec<StatusTargetRow> {
    targets
        .iter()
        .map(|target| {
            let frontend = frontend_for_path(&target.source_file);
            StatusTargetRow {
                target_id: target.target_id.clone(),
                name: target.name.clone(),
                source_file: target.source_file.clone(),
                language: frontend.map(|info| info.language.to_string()),
                frontend: frontend.map(|info| info.frontend.to_string()),
                start_line: target.start_line,
                end_line: target.end_line,
                outcome: target.outcome,
                failure_reason: target.failure_reason.clone(),
                validity_impact: target.validity_impact,
                artifact: target_artifact(output_dir, target),
            }
        })
        .collect()
}

fn target_artifact(output_dir: &Path, target: &StatusTargetInput) -> Option<StatusTargetArtifact> {
    if target.artifact_path.is_none() && target.unavailable_reason.is_none() {
        return None;
    }
    Some(StatusTargetArtifact {
        path: target
            .artifact_path
            .as_deref()
            .map(|path| display_path(output_dir, path)),
        sha256: None,
        unavailable_reason: target.unavailable_reason.clone(),
    })
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

/// Compute source_set_hash from a RunManifest: SHA-256 of sorted
/// `"<path>:<content_hash>"` pairs for all source files with a known hash.
fn compute_manifest_source_set_hash(manifest: &RunManifest) -> String {
    let mut pairs: Vec<String> = manifest
        .source_files
        .iter()
        .filter_map(|f| {
            f.content_hash
                .as_ref()
                .map(|h| format!("{}:{}", f.path, h))
        })
        .collect();
    pairs.sort();
    let combined = pairs.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Write a markdown run summary to `path`.
///
/// Renders a concise human-readable overview of the `RunStatus` for
/// integration into artifact directories alongside `run-status.json`.
pub fn write_run_summary_md(path: &Path, status: &RunStatus) -> Result<(), StatusExportError> {
    let md = render_run_summary_md(status);
    let tmp_path = path.with_extension("md.tmp");
    std::fs::write(&tmp_path, md.as_bytes()).map_err(|source| StatusExportError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, path).map_err(|source| StatusExportError::Finalize {
        path: path.to_path_buf(),
        source,
    })
}

/// Render a markdown run summary from a `RunStatus` (without writing to disk).
#[must_use]
pub fn render_run_summary_md(status: &RunStatus) -> String {
    let d = &status.rollups.source_denominators;
    let v = &status.rollups.validity;
    let mut out = String::new();
    out.push_str("# Run Summary\n\n");
    out.push_str(&format!("**Scan ID:** `{}`  \n", status.run.scan_id));
    out.push_str(&format!("**Command:** `{}`  \n", status.command.name));
    out.push_str(&format!(
        "**Config hash:** `{}`  \n",
        status.command.config_hash
    ));
    out.push_str(&format!(
        "**Source set hash:** `{}`  \n\n",
        status.command.source_set_hash
    ));
    out.push_str("## Source\n\n");
    out.push_str(&format!(
        "- Selected files: {}\n",
        d.selected_source_files
    ));
    out.push_str(&format!("- Selected lines: {}\n\n", d.selected_source_lines));
    out.push_str("## Targets\n\n");
    out.push_str(&format!("- Discovered: {}\n", d.discovered_targets));
    out.push_str(&format!("- Attempted: {}\n", d.attempted_targets));
    out.push_str(&format!("- Completed: {}\n", d.completed_targets));
    out.push_str(&format!("- Failed: {}\n", d.failed_targets));
    out.push_str(&format!("- Unsupported: {}\n\n", d.unsupported_targets));
    out.push_str("## Validity\n\n");
    out.push_str(&format!(
        "**Report validity:** {}\n\n",
        report_validity_wire(v.report_validity)
    ));
    if !v.reasons.is_empty() {
        out.push_str("| Code | Detail |\n|------|--------|\n");
        for reason in &v.reasons {
            out.push_str(&format!("| {} | {} |\n", reason.code, reason.detail));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::run_manifest::{RUN_MANIFEST_VERSION, RunManifest, SourceFileSnapshot};
    use proptest::prelude::*;

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
                rollups: StatusRollupInput::default(),
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
    fn writes_tsv_projection_matching_json_rows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-tsv".to_string(),
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

        let artifact_dir = root.join("functions");
        fs::create_dir_all(&artifact_dir).expect("artifact dir");
        let completed_artifact = artifact_dir.join("00001_src_app_ts_ok.json");
        fs::write(&completed_artifact, br#"{"ok":true}"#).expect("write artifact");

        write_run_status_json(
            root,
            &StatusExportInput {
                command: "scan",
                manifest: &manifest,
                manifest_path: &manifest_path,
                artifacts: &[],
                files: &[StatusFileInput {
                    path: "src/app.ts".to_string(),
                    discovered_targets: 2,
                    attempted_targets: 2,
                    completed_targets: 1,
                    failed_targets: 1,
                    unsupported_targets: 0,
                    status: StatusFileStatus::Partial,
                }],
                targets: &[
                    StatusTargetInput {
                        target_id: "src/app.ts::ok".to_string(),
                        name: "ok".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 2,
                        end_line: 4,
                        outcome: StatusTargetOutcome::Completed,
                        artifact_path: Some(completed_artifact.clone()),
                        failure_reason: None,
                        unavailable_reason: None,
                        validity_impact: StatusTargetValidityImpact::Contributes,
                    },
                    StatusTargetInput {
                        target_id: "src/app.ts::fails".to_string(),
                        name: "fails".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 8,
                        end_line: 12,
                        outcome: StatusTargetOutcome::Failed,
                        artifact_path: None,
                        failure_reason: Some("runtime failed".to_string()),
                        unavailable_reason: Some("artifact unavailable after failure".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                ],
                rollups: StatusRollupInput {
                    report_validity: Some(StatusReportValidity::Degraded),
                    validity_reasons: vec![StatusValidityReason {
                        code: "failed_target".to_string(),
                        detail: "one target failed".to_string(),
                        recommended_action: "inspect target artifact".to_string(),
                    }],
                    line_weighted_failure_impact: Some(StatusLineWeightedFailureImpact {
                        represented_source_lines: 3,
                        unrepresented_failed_lines: 5,
                        unrepresented_timed_out_lines: 0,
                        unrepresented_unsupported_lines: 0,
                        unrepresented_unavailable_frontend_lines: 0,
                        unrepresented_no_target_lines: 0,
                        unrepresented_undiscovered_lines: 4,
                    }),
                    gate_decisions: None,
                },
            },
        )
        .expect("write status");

        let status_bytes = fs::read(root.join(RUN_STATUS_FILENAME)).expect("read status");
        let status: RunStatus = serde_json::from_slice(&status_bytes).expect("parse status");
        let tsv = fs::read_to_string(root.join(RUN_STATUS_TSV_FILENAME)).expect("read status tsv");
        let rows = parse_tsv(&tsv);

        assert_eq!(
            tsv.lines().next(),
            Some(RUN_STATUS_TSV_COLUMNS.join("\t").as_str())
        );
        assert_eq!(rows.len(), 4);

        let run_row = rows
            .iter()
            .find(|row| row["row_type"] == "run")
            .expect("run row");
        assert_eq!(run_row["schema_version"], status.schema_version.to_string());
        assert_eq!(run_row["scan_id"], status.run.scan_id);
        assert_eq!(run_row["command"], status.command.name);
        assert_eq!(run_row["config_hash"], status.command.config_hash);
        assert_eq!(
            run_row["report_validity"], "degraded",
            "run validity should project JSON rollups, not markdown status"
        );
        assert_eq!(run_row["validity_reason_codes"], "failed_target");
        assert_eq!(
            run_row["represented_source_lines"],
            status
                .rollups
                .line_weighted_failure_impact
                .represented_source_lines
                .to_string()
        );

        let json_file = status.files.first().expect("json file row");
        let file_row = rows
            .iter()
            .find(|row| row["row_type"] == "file")
            .expect("file row");
        assert_eq!(file_row["file_path"], json_file.path);
        assert_eq!(
            file_row["language"],
            json_file.language.as_deref().unwrap_or("")
        );
        assert_eq!(
            file_row["frontend"],
            json_file.frontend.as_deref().unwrap_or("")
        );
        assert_eq!(
            file_row["source_bucket"],
            json_file.source_bucket.as_wire_str()
        );
        assert_eq!(
            file_row["selected_line_count"],
            json_file.selected_line_count.to_string()
        );
        assert_eq!(file_row["file_status"], "partial");
        assert_eq!(
            file_row["discovered_target_count"],
            json_file.discovered_target_count.to_string()
        );
        assert_eq!(
            file_row["attempted_target_count"],
            json_file.attempted_target_count.to_string()
        );
        assert_eq!(
            file_row["completed_target_count"],
            json_file.completed_target_count.to_string()
        );
        assert_eq!(
            file_row["failed_target_count"],
            json_file.failed_target_count.to_string()
        );

        let json_target = status
            .targets
            .iter()
            .find(|target| target.target_id == "src/app.ts::fails")
            .expect("json target row");
        let target_row = rows
            .iter()
            .find(|row| row["target_id"] == json_target.target_id)
            .expect("target row");
        assert_eq!(target_row["row_type"], "target");
        assert_eq!(target_row["file_path"], json_target.source_file);
        assert_eq!(target_row["target_name"], json_target.name);
        assert_eq!(target_row["start_line"], json_target.start_line.to_string());
        assert_eq!(target_row["end_line"], json_target.end_line.to_string());
        assert_eq!(target_row["target_outcome"], "failed");
        assert_eq!(target_row["failure_reason"], "runtime failed");
        assert_eq!(target_row["validity_impact"], "degrades");
        assert_eq!(
            target_row["artifact_unavailable_reason"],
            "artifact unavailable after failure"
        );
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
                rollups: StatusRollupInput::default(),
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
                rollups: StatusRollupInput::default(),
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
        assert_eq!(
            completed.validity_impact,
            StatusTargetValidityImpact::Contributes
        );
        let artifact = completed.artifact.as_ref().expect("completed artifact");
        assert_eq!(
            artifact.path.as_deref(),
            Some("functions/00001_src_app_ts_doThing.json")
        );
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
        assert_eq!(
            unsupported.validity_impact,
            StatusTargetValidityImpact::Excluded
        );

        let unavailable = status
            .targets
            .iter()
            .find(|target| target.target_id == "crates/missing.rs::needsFrontend")
            .expect("unavailable target");
        assert_eq!(
            unavailable.outcome,
            StatusTargetOutcome::UnavailableFrontend
        );
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
                rollups: StatusRollupInput::default(),
            },
        )
        .expect_err("missing target artifact must fail status export");

        assert!(matches!(err, StatusExportError::ReadArtifact { .. }));
        assert!(
            !root.join(RUN_STATUS_FILENAME).exists(),
            "status export must not be finalized when target artifact validation fails"
        );
    }

    #[test]
    fn writes_status_rollup_metrics() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let manifest = RunManifest {
            version: RUN_MANIFEST_VERSION,
            scan_id: "scan-rollups".to_string(),
            project_root: Some(root.display().to_string()),
            repo_root: Some(root.display().to_string()),
            cwd: root.display().to_string(),
            git_commit: None,
            git_dirty: Some(false),
            scope_hash: "scope-hash".to_string(),
            source_files: vec![
                source_file("src/app.ts", Some(10)),
                source_file("pkg/handler.go", Some(20)),
                source_file("tests/app.test.ts", Some(4)),
                source_file("generated/schema.gen.ts", Some(8)),
                source_file("README.md", Some(7)),
            ],
            captured_at_ns: 42,
        };
        let manifest_path = root.join("manifest.json");

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
                        discovered_targets: 2,
                        attempted_targets: 2,
                        completed_targets: 1,
                        failed_targets: 1,
                        unsupported_targets: 0,
                        status: StatusFileStatus::Partial,
                    },
                    StatusFileInput {
                        path: "pkg/handler.go".to_string(),
                        discovered_targets: 2,
                        attempted_targets: 1,
                        completed_targets: 0,
                        failed_targets: 1,
                        unsupported_targets: 1,
                        status: StatusFileStatus::Failed,
                    },
                ],
                targets: &[
                    StatusTargetInput {
                        target_id: "src/app.ts::ok".to_string(),
                        name: "ok".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 2,
                        end_line: 6,
                        outcome: StatusTargetOutcome::Completed,
                        artifact_path: None,
                        failure_reason: None,
                        unavailable_reason: None,
                        validity_impact: StatusTargetValidityImpact::Contributes,
                    },
                    StatusTargetInput {
                        target_id: "src/app.ts::fail".to_string(),
                        name: "fail".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 7,
                        end_line: 10,
                        outcome: StatusTargetOutcome::Failed,
                        artifact_path: None,
                        failure_reason: Some("runtime failed".to_string()),
                        unavailable_reason: Some("runtime failed".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                    StatusTargetInput {
                        target_id: "pkg/handler.go::slow".to_string(),
                        name: "slow".to_string(),
                        source_file: "pkg/handler.go".to_string(),
                        start_line: 3,
                        end_line: 8,
                        outcome: StatusTargetOutcome::TimedOut,
                        artifact_path: None,
                        failure_reason: Some("timed out".to_string()),
                        unavailable_reason: Some("timed out".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                    StatusTargetInput {
                        target_id: "pkg/handler.go::unsupported".to_string(),
                        name: "unsupported".to_string(),
                        source_file: "pkg/handler.go".to_string(),
                        start_line: 9,
                        end_line: 12,
                        outcome: StatusTargetOutcome::Unsupported,
                        artifact_path: None,
                        failure_reason: Some("unsupported parameter".to_string()),
                        unavailable_reason: Some("unsupported parameter".to_string()),
                        validity_impact: StatusTargetValidityImpact::Excluded,
                    },
                    StatusTargetInput {
                        target_id: "src/app.ts::preflight".to_string(),
                        name: "preflight".to_string(),
                        source_file: "src/app.ts".to_string(),
                        start_line: 1,
                        end_line: 1,
                        outcome: StatusTargetOutcome::UnavailableFrontend,
                        artifact_path: None,
                        failure_reason: Some("preflight failed".to_string()),
                        unavailable_reason: Some("preflight failed".to_string()),
                        validity_impact: StatusTargetValidityImpact::Degrades,
                    },
                ],
                rollups: StatusRollupInput {
                    report_validity: Some(StatusReportValidity::Degraded),
                    validity_reasons: vec![StatusValidityReason {
                        code: "degraded_representation".to_string(),
                        detail: "represented_source_percent=50.0".to_string(),
                        recommended_action: "inspect failed buckets".to_string(),
                    }],
                    line_weighted_failure_impact: Some(StatusLineWeightedFailureImpact {
                        represented_source_lines: 5,
                        unrepresented_failed_lines: 4,
                        unrepresented_timed_out_lines: 6,
                        unrepresented_unsupported_lines: 4,
                        unrepresented_unavailable_frontend_lines: 1,
                        unrepresented_no_target_lines: 7,
                        unrepresented_undiscovered_lines: 22,
                    }),
                    gate_decisions: None,
                },
            },
        );

        assert_eq!(status.rollups.source_denominators.selected_source_files, 5);
        assert_eq!(status.rollups.source_denominators.selected_source_lines, 49);
        assert_eq!(status.rollups.source_denominators.discovered_targets, 4);
        assert_eq!(status.rollups.source_denominators.attempted_targets, 3);
        assert_eq!(status.rollups.source_denominators.completed_targets, 1);
        assert_eq!(status.rollups.source_denominators.failed_targets, 2);
        assert_eq!(status.rollups.source_denominators.unsupported_targets, 1);

        let production = status
            .rollups
            .source_buckets
            .iter()
            .find(|bucket| bucket.source_bucket == SourceBucket::ProductionIsh)
            .expect("production bucket");
        assert_eq!(production.selected_file_count, 2);
        assert_eq!(production.selected_line_count, 30);
        let unsupported = status
            .rollups
            .source_buckets
            .iter()
            .find(|bucket| bucket.source_bucket == SourceBucket::Unsupported)
            .expect("unsupported bucket");
        assert_eq!(unsupported.selected_file_count, 1);
        assert_eq!(unsupported.selected_line_count, 7);

        assert_eq!(
            status.rollups.validity.report_validity,
            StatusReportValidity::Degraded
        );
        assert_eq!(
            status.rollups.validity.reasons[0].code,
            "degraded_representation"
        );

        let ts_frontend = status
            .rollups
            .frontend_availability
            .iter()
            .find(|frontend| frontend.frontend == "shatter-ts")
            .expect("typescript frontend");
        assert_eq!(ts_frontend.selected_file_count, 3);
        assert_eq!(ts_frontend.selected_line_count, 22);
        assert_eq!(ts_frontend.target_count, 3);
        assert_eq!(ts_frontend.unavailable_target_count, 1);
        assert_eq!(ts_frontend.preflight_failed_target_count, 1);

        assert_eq!(
            status
                .rollups
                .line_weighted_failure_impact
                .unrepresented_timed_out_lines,
            6
        );
        assert!(status.rollups.gate_decisions.is_none());
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

    proptest! {
        #[test]
        fn tsv_cells_never_contain_row_or_column_separators(
            value in proptest::collection::vec(any::<char>(), 0..128)
                .prop_map(|chars| chars.into_iter().collect::<String>())
        ) {
            let cell = tsv_cell(&value);
            prop_assert!(!cell.contains('\t'));
            prop_assert!(!cell.contains('\n'));
            prop_assert!(!cell.contains('\r'));
        }
    }

    fn parse_tsv(tsv: &str) -> Vec<std::collections::BTreeMap<String, String>> {
        let mut lines = tsv.lines();
        let header: Vec<&str> = lines.next().expect("tsv header").split('\t').collect();
        lines
            .map(|line| {
                let values: Vec<&str> = line.split('\t').collect();
                header
                    .iter()
                    .zip(values)
                    .map(|(name, value)| ((*name).to_string(), value.to_string()))
                    .collect()
            })
            .collect()
    }
}
