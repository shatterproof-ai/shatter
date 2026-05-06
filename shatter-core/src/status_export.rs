#[cfg(test)]
mod tests {
    use std::fs;

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
        fs::write(&manifest_path, serde_json::to_vec(&manifest).expect("manifest json"))
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
        assert_eq!(status.source_snapshot.git_commit.as_deref(), Some("abc1234"));
        assert_eq!(status.source_snapshot.manifest_captured_at_ns, 42);
        assert_eq!(status.manifest.path, "manifest.json");
        assert!(status.manifest.sha256.is_some());
        assert_eq!(status.artifacts[0].kind, "run_summary");
        assert_eq!(status.artifacts[0].path, "run.json");
        assert!(status.artifacts[0].sha256.is_some());
        assert!(status.generated_at_ns > 0);
    }
}
