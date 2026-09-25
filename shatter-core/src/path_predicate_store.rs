//! Versioned, standalone storage for path predicate evidence.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::invariants::{PathPredicateRecord, PredicateValidationError, validate_path_predicate};

/// Wire version for a collection of path predicate records.
pub const PATH_PREDICATE_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// A standalone collection. Callers supply context and coordinate concurrent updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPredicateBundle {
    pub schema_version: u32,
    pub predicates: Vec<PathPredicateRecord>,
}

/// A malformed bundle or a failed filesystem operation.
#[derive(Debug, thiserror::Error)]
pub enum PathPredicateStoreError {
    #[error("path predicate bundle I/O: {0}")]
    Io(#[from] io::Error),
    #[error("invalid path predicate bundle JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported path predicate bundle version")]
    UnsupportedVersion,
    #[error("duplicate path predicate ID: {0}")]
    DuplicateId(String),
    #[error("invalid path predicate: {0}")]
    InvalidPredicate(#[from] PredicateValidationError),
}

fn validate_bundle(bundle: &PathPredicateBundle) -> Result<(), PathPredicateStoreError> {
    if bundle.schema_version != PATH_PREDICATE_BUNDLE_SCHEMA_VERSION {
        return Err(PathPredicateStoreError::UnsupportedVersion);
    }
    let mut seen = HashSet::new();
    for record in &bundle.predicates {
        validate_path_predicate(record)?;
        if !seen.insert(&record.predicate_id) {
            return Err(PathPredicateStoreError::DuplicateId(
                record.predicate_id.clone(),
            ));
        }
    }
    Ok(())
}

/// Read and validate an existing bundle. A missing path returns `Io(NotFound)`.
/// On-disk record order does not affect validity.
pub fn read_path_predicate_bundle(
    path: &Path,
) -> Result<PathPredicateBundle, PathPredicateStoreError> {
    let bytes = fs::read(path)?;
    let bundle: PathPredicateBundle = serde_json::from_slice(&bytes)?;
    validate_bundle(&bundle)?;
    Ok(bundle)
}

/// Replace a bundle atomically after validating and fully writing a unique sibling.
/// This guarantees replacement visibility, not crash durability. Callers must
/// serialize read-modify-write operations when multiple writers share a path.
pub fn write_path_predicate_bundle(
    path: &Path,
    bundle: &PathPredicateBundle,
) -> Result<(), PathPredicateStoreError> {
    write_bundle_with_before_persist(path, bundle, || Ok(()))
}

fn write_bundle_with_before_persist(
    path: &Path,
    bundle: &PathPredicateBundle,
    before_persist: impl FnOnce() -> io::Result<()>,
) -> Result<(), PathPredicateStoreError> {
    validate_bundle(bundle)?;
    let mut sorted = bundle.clone();
    sorted
        .predicates
        .sort_by(|left, right| left.predicate_id.cmp(&right.predicate_id));
    let bytes = serde_json::to_vec_pretty(&sorted)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    before_persist()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invariants::{
        InputPath, InputPathKind, ObservationPoint, PATH_PREDICATE_SCHEMA_VERSION,
        PATH_PREDICATE_SEMANTICS_VERSION, PathCompareOp, PathExpression, PathObservation,
        PathOutcome, PathPredicateScope, PathPredicateTarget, PathSegment, PredicateEvidence,
        PredicateLifecycle, canonical_path_predicate_id, record_path_predicate_observation,
    };

    fn record(op: PathCompareOp) -> PathPredicateRecord {
        let operand = InputPath {
            kind: InputPathKind::InputPath,
            parameter: 0,
            path: vec![PathSegment::Field { value: "x".into() }],
        };
        let mut record = PathPredicateRecord {
            schema_version: PATH_PREDICATE_SCHEMA_VERSION,
            predicate_id: String::new(),
            target: PathPredicateTarget {
                qualified_function: "example.f".into(),
                frontend: "synthetic".into(),
                source_fingerprint: "source-v1".into(),
            },
            scope: PathPredicateScope {
                path_prefix: vec![],
                observation_point: ObservationPoint::Entry,
                context_fingerprint: "context-v1".into(),
            },
            expression: PathExpression::Compare {
                op,
                left: operand.clone(),
                right: operand,
            },
            semantics_version: PATH_PREDICATE_SEMANTICS_VERSION.into(),
            lifecycle: PredicateLifecycle::Candidate,
            evidence: PredicateEvidence::default(),
        };
        record.predicate_id = canonical_path_predicate_id(&record).expect("ID");
        record
    }

    #[test]
    fn path_predicate_bundle_round_trip_sorts_on_write() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("nested/predicates.json");
        let mut first = record(PathCompareOp::Eq);
        first.lifecycle = PredicateLifecycle::Frozen;
        first.evidence.eligible = 1;
        first.evidence.holds = 1;
        first.evidence.supporting_witnesses.push("witness".into());
        let second = record(PathCompareOp::Ne);
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![first, second],
        };
        write_path_predicate_bundle(&path, &bundle).expect("write");
        let loaded = read_path_predicate_bundle(&path).expect("read");
        let mut expected = bundle;
        expected
            .predicates
            .sort_by(|a, b| a.predicate_id.cmp(&b.predicate_id));
        assert_eq!(loaded, expected);
        assert!(
            loaded
                .predicates
                .windows(2)
                .all(|pair| pair[0].predicate_id < pair[1].predicate_id)
        );
    }

    #[test]
    fn path_predicate_bundle_rejects_invalid_records_and_unknown_fields() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        let original = record(PathCompareOp::Eq);
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![original.clone(), original.clone()],
        };
        assert!(matches!(
            write_path_predicate_bundle(&path, &bundle),
            Err(PathPredicateStoreError::DuplicateId(_))
        ));
        assert!(!path.exists());
        let mut malformed = original.clone();
        malformed
            .evidence
            .supporting_witnesses
            .push("orphan".into());
        let bundle = PathPredicateBundle {
            predicates: vec![malformed],
            ..bundle
        };
        assert!(matches!(
            write_path_predicate_bundle(&path, &bundle),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        let unknown = serde_json::json!({"schema_version": 1, "predicates": [], "extra": true});
        fs::write(&path, unknown.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::Json(_))
        ));
        let mut nested = serde_json::to_value(&original).expect("record JSON");
        nested["extra"] = serde_json::json!(true);
        let unknown_nested = serde_json::json!({"schema_version": 1, "predicates": [nested]});
        fs::write(&path, unknown_nested.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::Json(_))
        ));
        let duplicates = serde_json::json!({"schema_version": 1, "predicates": [original.clone(), original.clone()]});
        fs::write(&path, duplicates.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::DuplicateId(_))
        ));
        let mut bad_id = original.clone();
        bad_id.predicate_id = "incorrect".into();
        let invalid = serde_json::json!({"schema_version": 1, "predicates": [bad_id]});
        fs::write(&path, invalid.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        let mut bad_version = original;
        bad_version.schema_version = 2;
        let invalid = serde_json::json!({"schema_version": 1, "predicates": [bad_version]});
        fs::write(&path, invalid.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        let unknown_version = serde_json::json!({"schema_version": 2, "predicates": []});
        fs::write(&path, unknown_version.to_string()).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::UnsupportedVersion)
        ));
    }

    #[test]
    fn path_predicate_bundle_rejects_invalid_witness_lists() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        let mut predicate = record(PathCompareOp::Eq);
        predicate.evidence.eligible = 17;
        predicate.evidence.holds = 17;
        for index in 0..17 {
            predicate
                .evidence
                .supporting_witnesses
                .push(format!("witness-{index}"));
        }
        let mut bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![predicate.clone()],
        };
        assert!(matches!(
            write_path_predicate_bundle(&path, &bundle),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        fs::write(&path, serde_json::to_vec(&bundle).expect("fixture JSON")).expect("fixture");
        assert!(matches!(
            read_path_predicate_bundle(&path),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        predicate.evidence.supporting_witnesses.truncate(2);
        predicate.evidence.supporting_witnesses[1] =
            predicate.evidence.supporting_witnesses[0].clone();
        bundle.predicates = vec![predicate.clone()];
        assert!(matches!(
            write_path_predicate_bundle(&path, &bundle),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
        predicate.evidence.supporting_witnesses[1].clear();
        bundle.predicates = vec![predicate];
        assert!(matches!(
            write_path_predicate_bundle(&path, &bundle),
            Err(PathPredicateStoreError::InvalidPredicate(_))
        ));
    }

    #[test]
    fn path_predicate_bundle_failed_write_preserves_existing_file_and_cleans_temp() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![],
        };
        write_path_predicate_bundle(&path, &bundle).expect("initial write");
        let original = fs::read(&path).expect("original bytes");
        let updated = PathPredicateBundle {
            predicates: vec![record(PathCompareOp::Eq)],
            ..bundle
        };
        assert!(matches!(
            write_bundle_with_before_persist(&path, &updated, || Err(io::Error::other("injected"))),
            Err(PathPredicateStoreError::Io(_))
        ));
        assert_eq!(fs::read(&path).expect("read after failure"), original);
        assert_eq!(fs::read_dir(directory.path()).expect("entries").count(), 1);
    }

    #[test]
    fn path_predicate_bundle_missing_and_empty_are_distinct() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        assert!(
            matches!(read_path_predicate_bundle(&path), Err(PathPredicateStoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound)
        );
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![],
        };
        write_path_predicate_bundle(&path, &bundle).expect("write empty");
        assert_eq!(
            read_path_predicate_bundle(&path).expect("read empty"),
            bundle
        );
    }

    #[test]
    fn path_predicate_bundle_accumulates_after_reload() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![record(PathCompareOp::Eq)],
        };
        write_path_predicate_bundle(&path, &bundle).expect("write");
        let observation = PathObservation {
            inputs: vec![serde_json::json!({"x": 7})],
            branch_path: vec![],
            observation_point: ObservationPoint::Entry,
            context_fingerprint: Some("context-v1".into()),
            outcome: PathOutcome::Return {
                value: serde_json::Value::Null,
            },
        };
        let mut loaded = read_path_predicate_bundle(&path).expect("first read");
        record_path_predicate_observation(&mut loaded.predicates[0], &observation, Some("first"))
            .expect("first observation");
        write_path_predicate_bundle(&path, &loaded).expect("second write");
        let mut loaded = read_path_predicate_bundle(&path).expect("second read");
        record_path_predicate_observation(&mut loaded.predicates[0], &observation, Some("second"))
            .expect("second observation");
        write_path_predicate_bundle(&path, &loaded).expect("third write");
        let reloaded = read_path_predicate_bundle(&path).expect("third read");
        assert_eq!(reloaded.predicates[0].evidence.eligible, 2);
        assert_eq!(reloaded.predicates[0].evidence.holds, 2);
        assert_eq!(
            reloaded.predicates[0].evidence.supporting_witnesses,
            ["first", "second"]
        );
    }

    #[test]
    fn path_predicate_bundle_round_trip_property() {
        use proptest::prelude::*;
        use proptest::test_runner::{Config, RngSeed, TestRunner};

        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        let mut runner = TestRunner::new(Config {
            cases: 128,
            rng_seed: RngSeed::Fixed(0x5a77_2028),
            ..Config::default()
        });
        runner
            .run(&proptest::collection::vec(0u8..6, 0..6), |indices| {
                let mut indices = indices;
                indices.sort_unstable();
                indices.dedup();
                let predicates = indices
                    .into_iter()
                    .map(|index| {
                        record(match index {
                            0 => PathCompareOp::Eq,
                            1 => PathCompareOp::Ne,
                            2 => PathCompareOp::Lt,
                            3 => PathCompareOp::Le,
                            4 => PathCompareOp::Gt,
                            _ => PathCompareOp::Ge,
                        })
                    })
                    .collect();
                let bundle = PathPredicateBundle {
                    schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
                    predicates,
                };
                write_path_predicate_bundle(&path, &bundle).expect("write");
                let loaded = read_path_predicate_bundle(&path).expect("read");
                let mut expected = bundle;
                expected
                    .predicates
                    .sort_by(|left, right| left.predicate_id.cmp(&right.predicate_id));
                prop_assert!(
                    loaded
                        .predicates
                        .windows(2)
                        .all(|pair| pair[0].predicate_id < pair[1].predicate_id)
                );
                prop_assert_eq!(loaded, expected);
                Ok(())
            })
            .expect("round-trip property");
    }
}
