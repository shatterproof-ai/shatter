//! Opt-in path predicate evidence collection for `shatter observe`.
//!
//! The provenance file is an operator assertion about the target source and
//! execution context. Shatter validates its shape and binds its target to the
//! analyzed function and frontend by exact string equality; it does not verify
//! `source_fingerprint` or the semantic correctness of the asserted context.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use shatter_core::explorer::ObservationOutput;
use shatter_core::invariants::PathPredicateTarget;
use shatter_core::path_evidence::{ExecutionEvidenceMetadata, collect_path_predicate_evidence};
use shatter_core::path_predicate_store::{
    PathPredicateBundle, PathPredicateStoreError, read_path_predicate_bundle,
    write_path_predicate_bundle,
};

/// The operator-supplied provenance file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    target: PathPredicateTarget,
    context_fingerprint: String,
}

/// A validated request to fold one observe run into an existing bundle.
#[derive(Debug)]
pub(crate) struct PathEvidenceRequest {
    bundle_path: PathBuf,
    bundle: PathPredicateBundle,
    target: PathPredicateTarget,
    context_fingerprint: String,
}

impl PathEvidenceRequest {
    /// Read and validate the provenance file and bundle. Runs before any
    /// frontend is spawned.
    pub(crate) fn load(bundle_path: &Path, provenance_path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(provenance_path).map_err(|e| {
            format!(
                "cannot read path evidence provenance {}: {e}",
                provenance_path.display()
            )
        })?;
        let provenance: Provenance = serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "invalid path evidence provenance {}: {e}",
                provenance_path.display()
            )
        })?;
        for (field, value) in [
            ("target.qualified_function", &provenance.target.qualified_function),
            ("target.frontend", &provenance.target.frontend),
            ("target.source_fingerprint", &provenance.target.source_fingerprint),
            ("context_fingerprint", &provenance.context_fingerprint),
        ] {
            if value.is_empty() {
                return Err(format!(
                    "invalid path evidence provenance {}: {field} must not be empty",
                    provenance_path.display()
                ));
            }
        }

        let bundle = read_path_predicate_bundle(bundle_path).map_err(|e| {
            format!(
                "cannot use path predicate bundle {}: {e}",
                bundle_path.display()
            )
        })?;
        if !bundle.predicates.iter().any(|p| p.target == provenance.target) {
            return Err(format!(
                "path predicate bundle {} has no predicate for target {}/{}/{}",
                bundle_path.display(),
                provenance.target.frontend,
                provenance.target.qualified_function,
                provenance.target.source_fingerprint,
            ));
        }
        Ok(Self {
            bundle_path: bundle_path.to_path_buf(),
            bundle,
            target: provenance.target,
            context_fingerprint: provenance.context_fingerprint,
        })
    }

    /// Require exact equality with Analyze's function name and the selected
    /// frontend label. No suffix or case normalization.
    pub(crate) fn check_binding(
        &self,
        analyzed_name: &str,
        frontend_label: &str,
    ) -> Result<(), String> {
        if self.target.qualified_function != analyzed_name {
            return Err(format!(
                "provenance target.qualified_function '{}' does not exactly match analyzed function name '{analyzed_name}'",
                self.target.qualified_function
            ));
        }
        if self.target.frontend != frontend_label {
            return Err(format!(
                "provenance target.frontend '{}' does not match selected frontend '{frontend_label}'",
                self.target.frontend
            ));
        }
        Ok(())
    }

    /// Collect evidence from `observation` and atomically replace the bundle.
    pub(crate) fn collect_and_write(&self, observation: &ObservationOutput) -> Result<(), String> {
        self.collect_and_write_with(observation, write_path_predicate_bundle)
    }

    fn collect_and_write_with(
        &self,
        observation: &ObservationOutput,
        write: impl FnOnce(&Path, &PathPredicateBundle) -> Result<(), PathPredicateStoreError>,
    ) -> Result<(), String> {
        let metadata = vec![
            ExecutionEvidenceMetadata {
                context_fingerprint: Some(self.context_fingerprint.clone()),
                witness: None,
            };
            observation.raw_results.len()
        ];
        let updated =
            collect_path_predicate_evidence(&self.bundle, observation, &self.target, &metadata)
                .map_err(|e| format!("path evidence collection failed: {e}"))?;
        write(&self.bundle_path, &updated).map_err(|e| {
            format!(
                "cannot write path predicate bundle {}: {e}",
                self.bundle_path.display()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::invariants::{
        InputPath, InputPathKind, ObservationPoint, PATH_PREDICATE_SCHEMA_VERSION,
        PATH_PREDICATE_SEMANTICS_VERSION, PathCompareOp, PathExpression, PathPredicateRecord,
        PathPredicateScope, PredicateEvidence, PredicateLifecycle, canonical_path_predicate_id,
    };
    use shatter_core::path_predicate_store::PATH_PREDICATE_BUNDLE_SCHEMA_VERSION;
    use proptest::prelude::*;

    fn target() -> PathPredicateTarget {
        PathPredicateTarget {
            qualified_function: "example.compare".into(),
            frontend: "typescript".into(),
            source_fingerprint: "source-v1".into(),
        }
    }

    fn bundle() -> PathPredicateBundle {
        let operand = |parameter| InputPath {
            kind: InputPathKind::InputPath,
            parameter,
            path: vec![],
        };
        let mut record = PathPredicateRecord {
            schema_version: PATH_PREDICATE_SCHEMA_VERSION,
            predicate_id: String::new(),
            target: target(),
            scope: PathPredicateScope {
                path_prefix: vec![(0, true)],
                observation_point: ObservationPoint::Entry,
                context_fingerprint: "context-v1".into(),
            },
            expression: PathExpression::Compare {
                op: PathCompareOp::Lt,
                left: operand(0),
                right: operand(1),
            },
            semantics_version: PATH_PREDICATE_SEMANTICS_VERSION.into(),
            lifecycle: PredicateLifecycle::Candidate,
            evidence: PredicateEvidence::default(),
        };
        record.predicate_id = canonical_path_predicate_id(&record).expect("ID");
        PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![record],
        }
    }

    const GOOD_PROVENANCE: &str = r#"{"target":{"qualified_function":"example.compare","frontend":"typescript","source_fingerprint":"source-v1"},"context_fingerprint":"context-v1"}"#;

    /// Write a bundle and provenance into a tempdir; return (dir, bundle path, provenance path).
    fn fixture(provenance: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let bundle_path = dir.path().join("bundle.json");
        write_path_predicate_bundle(&bundle_path, &bundle()).expect("write bundle");
        let provenance_path = dir.path().join("provenance.json");
        std::fs::write(&provenance_path, provenance).expect("write provenance");
        (dir, bundle_path, provenance_path)
    }

    #[test]
    fn load_accepts_matching_provenance() {
        let (_dir, bundle_path, provenance_path) = fixture(GOOD_PROVENANCE);
        let request = PathEvidenceRequest::load(&bundle_path, &provenance_path).expect("load");
        assert_eq!(request.context_fingerprint, "context-v1");
        assert_eq!(request.target, target());
    }

    #[test]
    fn load_rejects_malformed_and_empty_provenance() {
        let cases = [
            "not json",
            "{}",
            r#"{"target":{"qualified_function":"example.compare","frontend":"typescript","source_fingerprint":"source-v1"}}"#,
            r#"{"target":{"qualified_function":"example.compare","frontend":"typescript","source_fingerprint":"source-v1"},"context_fingerprint":"context-v1","extra":1}"#,
            r#"{"target":{"qualified_function":"","frontend":"typescript","source_fingerprint":"source-v1"},"context_fingerprint":"context-v1"}"#,
            r#"{"target":{"qualified_function":"example.compare","frontend":"","source_fingerprint":"source-v1"},"context_fingerprint":"context-v1"}"#,
            r#"{"target":{"qualified_function":"example.compare","frontend":"typescript","source_fingerprint":""},"context_fingerprint":"context-v1"}"#,
            r#"{"target":{"qualified_function":"example.compare","frontend":"typescript","source_fingerprint":"source-v1"},"context_fingerprint":""}"#,
        ];
        for case in cases {
            let (_dir, bundle_path, provenance_path) = fixture(case);
            assert!(
                PathEvidenceRequest::load(&bundle_path, &provenance_path).is_err(),
                "accepted provenance: {case}"
            );
        }
    }

    #[test]
    fn load_rejects_missing_files_and_absent_target() {
        let (dir, bundle_path, provenance_path) = fixture(GOOD_PROVENANCE);
        assert!(PathEvidenceRequest::load(&dir.path().join("missing.json"), &provenance_path).is_err());
        assert!(PathEvidenceRequest::load(&bundle_path, &dir.path().join("missing.json")).is_err());

        let other = GOOD_PROVENANCE.replace("source-v1", "source-v2");
        std::fs::write(&provenance_path, other).expect("rewrite provenance");
        let error = PathEvidenceRequest::load(&bundle_path, &provenance_path).unwrap_err();
        assert!(error.contains("no predicate for target"), "{error}");
    }

    #[test]
    fn check_binding_requires_exact_name_and_frontend() {
        let (_dir, bundle_path, provenance_path) = fixture(GOOD_PROVENANCE);
        let request = PathEvidenceRequest::load(&bundle_path, &provenance_path).expect("load");
        request.check_binding("example.compare", "typescript").expect("exact");
        for name in ["compare", "example.compare ", "Example.compare", "example.compare2"] {
            assert!(request.check_binding(name, "typescript").is_err(), "{name}");
        }
        for label in ["go", "rust", "TypeScript", ""] {
            assert!(request.check_binding("example.compare", label).is_err(), "{label}");
        }
    }

    #[test]
    fn write_failure_after_valid_read_leaves_bundle_unchanged() {
        let (_dir, bundle_path, provenance_path) = fixture(GOOD_PROVENANCE);
        let before = std::fs::read(&bundle_path).expect("read before");
        let request = PathEvidenceRequest::load(&bundle_path, &provenance_path).expect("load");
        let error = request
            .collect_and_write_with(&ObservationOutput::default(), |_, _| {
                Err(PathPredicateStoreError::Io(std::io::Error::other("injected")))
            })
            .unwrap_err();
        assert!(error.contains("cannot write path predicate bundle"), "{error}");
        assert_eq!(std::fs::read(&bundle_path).expect("read after"), before);
    }

    #[test]
    fn collect_and_write_persists_updated_bundle() {
        let (_dir, bundle_path, provenance_path) = fixture(GOOD_PROVENANCE);
        let request = PathEvidenceRequest::load(&bundle_path, &provenance_path).expect("load");
        request
            .collect_and_write(&ObservationOutput::default())
            .expect("collect");
        read_path_predicate_bundle(&bundle_path).expect("readable after write");
    }

    proptest::proptest! {
        /// `load` accepts a provenance file if and only if every field is
        /// nonempty (untrusted-input boundary; not just the hand-picked cases
        /// above). The bundle always has a predicate for `target()`, so
        /// acceptance depends only on the provenance content, not the bundle.
        #[test]
        fn load_accepts_iff_all_fields_nonempty(
            qualified_function in "\\PC{0,12}",
            frontend in "\\PC{0,12}",
            source_fingerprint in "\\PC{0,12}",
            context_fingerprint in "\\PC{0,12}",
        ) {
            let provenance = serde_json::json!({
                "target": {
                    "qualified_function": qualified_function,
                    "frontend": frontend,
                    "source_fingerprint": source_fingerprint,
                },
                "context_fingerprint": context_fingerprint,
            })
            .to_string();
            let (_dir, bundle_path, provenance_path) = fixture(&provenance);
            let all_nonempty = !qualified_function.is_empty()
                && !frontend.is_empty()
                && !source_fingerprint.is_empty()
                && !context_fingerprint.is_empty();
            // Only the exact target() combination has a matching predicate;
            // anything else fails on "no predicate for target" instead, which
            // is also required to be an error.
            let matches_bundle_target = qualified_function == "example.compare"
                && frontend == "typescript"
                && source_fingerprint == "source-v1";
            let result = PathEvidenceRequest::load(&bundle_path, &provenance_path);
            if all_nonempty && matches_bundle_target {
                prop_assert!(result.is_ok());
            } else {
                prop_assert!(result.is_err());
            }
        }
    }
}
