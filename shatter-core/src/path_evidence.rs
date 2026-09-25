//! Projection of exploration results into path predicate evidence.

use crate::explorer::ObservationOutput;
use crate::invariants::{
    ObservationPoint, PathObservation, PathOutcome, PathPredicateTarget, PredicateEvidenceError,
    record_path_predicate_observation,
};
use crate::path_predicate_store::{PathPredicateBundle, PathPredicateStoreError, validate_bundle};

/// Caller-supplied context and stable witness identity for one raw execution.
/// The raw tuple does not retain either value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionEvidenceMetadata {
    pub context_fingerprint: Option<String>,
    pub witness: Option<String>,
}

/// A rejected exploration evidence collection.
#[derive(Debug, thiserror::Error)]
pub enum PathEvidenceCollectionError {
    #[error("invalid path predicate bundle: {0}")]
    InvalidBundle(#[from] PathPredicateStoreError),
    #[error("metadata length does not match raw result count")]
    MetadataLengthMismatch,
    #[error("witness reference must not be empty")]
    EmptyWitness,
    #[error("path predicate evidence update failed: {0}")]
    Evidence(#[from] PredicateEvidenceError),
}

/// Record every raw result for predicates with the caller's exact target.
///
/// The caller must bind `target` to `output`: `function_name` is an
/// analyzer name and need not equal the qualified target name. This function
/// does not infer identity from mocks or path hashes. It also does not filter
/// lifecycle states or deduplicate probe executions; callers choose which
/// records and raw results to submit. Each record keeps its lifecycle.
#[must_use = "the updated bundle contains the collected evidence"]
pub fn collect_path_predicate_evidence(
    bundle: &PathPredicateBundle,
    output: &ObservationOutput,
    target: &PathPredicateTarget,
    metadata: &[ExecutionEvidenceMetadata],
) -> Result<PathPredicateBundle, PathEvidenceCollectionError> {
    validate_bundle(bundle)?;
    if metadata.len() != output.raw_results.len() {
        return Err(PathEvidenceCollectionError::MetadataLengthMismatch);
    }
    if metadata
        .iter()
        .any(|entry| entry.witness.as_deref() == Some(""))
    {
        return Err(PathEvidenceCollectionError::EmptyWitness);
    }

    let mut updated = bundle.clone();
    for ((inputs, _mocks, result), entry) in output.raw_results.iter().zip(metadata) {
        let outcome = if result.thrown_error.is_some() {
            PathOutcome::Thrown
        } else if let Some(value) = &result.return_value {
            PathOutcome::Return {
                value: value.clone(),
            }
        } else {
            PathOutcome::Unavailable
        };
        let observation = PathObservation {
            inputs: inputs.clone(),
            branch_path: result
                .branch_path
                .iter()
                .map(|decision| (decision.branch_id, decision.taken))
                .collect(),
            observation_point: ObservationPoint::Entry,
            context_fingerprint: entry.context_fingerprint.clone(),
            outcome,
        };
        for predicate in &mut updated.predicates {
            if &predicate.target == target {
                record_path_predicate_observation(
                    predicate,
                    &observation,
                    entry.witness.as_deref(),
                )?;
            }
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
    use crate::invariants::{
        InputPath, InputPathKind, ObservationPoint, PATH_PREDICATE_SCHEMA_VERSION,
        PATH_PREDICATE_SEMANTICS_VERSION, PathCompareOp, PathExpression, PathPredicateRecord,
        PathPredicateScope, PredicateEvidence, PredicateLifecycle, canonical_path_predicate_id,
    };
    use crate::path_predicate_store::PATH_PREDICATE_BUNDLE_SCHEMA_VERSION;
    use crate::path_predicate_store::{
        PathPredicateStoreError, read_path_predicate_bundle, write_path_predicate_bundle,
    };
    use crate::protocol::ExecuteResult;
    use proptest::prelude::*;
    use serde_json::json;

    fn target(name: &str) -> PathPredicateTarget {
        PathPredicateTarget {
            qualified_function: name.into(),
            frontend: "synthetic".into(),
            source_fingerprint: "source-v1".into(),
        }
    }

    fn predicate(target: PathPredicateTarget, path: Vec<(u32, bool)>) -> PathPredicateRecord {
        let mut record = PathPredicateRecord {
            schema_version: PATH_PREDICATE_SCHEMA_VERSION,
            predicate_id: String::new(),
            target,
            scope: PathPredicateScope {
                path_prefix: path,
                observation_point: ObservationPoint::Entry,
                context_fingerprint: "context-v1".into(),
            },
            expression: PathExpression::Compare {
                op: PathCompareOp::Lt,
                left: InputPath {
                    kind: InputPathKind::InputPath,
                    parameter: 0,
                    path: vec![],
                },
                right: InputPath {
                    kind: InputPathKind::InputPath,
                    parameter: 1,
                    path: vec![],
                },
            },
            semantics_version: PATH_PREDICATE_SEMANTICS_VERSION.into(),
            lifecycle: PredicateLifecycle::Candidate,
            evidence: PredicateEvidence::default(),
        };
        record.predicate_id = canonical_path_predicate_id(&record).expect("ID");
        record
    }

    fn decision(branch_id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id,
            line: 10,
            taken,
            constraint: SymConstraint::default(),
            conditions: None,
        }
    }

    fn execution(
        inputs: Vec<serde_json::Value>,
        path: Vec<BranchDecision>,
        returned: Option<serde_json::Value>,
        thrown: bool,
    ) -> (
        Vec<serde_json::Value>,
        Vec<crate::protocol::MockConfig>,
        ExecuteResult,
    ) {
        let result = ExecuteResult {
            return_value: returned,
            thrown_error: thrown.then(|| ErrorInfo {
                error_type: "Error".into(),
                message: "failed".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: path,
            ..ExecuteResult::default()
        };
        (inputs, vec![], result)
    }

    #[test]
    fn collects_matching_exploration_results_in_order() {
        let path = vec![(7, true), (7, false)];
        let matching = predicate(target("example.f"), path.clone());
        let unrelated = predicate(target("example.other"), path);
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![matching, unrelated.clone()],
        };
        let execution_path = vec![decision(7, true), decision(7, false)];
        let output = ObservationOutput {
            function_name: "example.f".into(),
            raw_results: vec![
                execution(
                    vec![json!(1), json!(2)],
                    execution_path.clone(),
                    Some(json!(0)),
                    false,
                ),
                execution(
                    vec![json!(3), json!(2)],
                    execution_path.clone(),
                    Some(json!(0)),
                    false,
                ),
                execution(
                    vec![json!(1), json!(2)],
                    execution_path,
                    Some(json!(0)),
                    true,
                ),
            ],
            ..ObservationOutput::default()
        };
        let metadata = ["first", "second", "third"].map(|witness| ExecutionEvidenceMetadata {
            context_fingerprint: Some("context-v1".into()),
            witness: Some(witness.into()),
        });

        let updated =
            collect_path_predicate_evidence(&bundle, &output, &target("example.f"), &metadata)
                .expect("collect evidence");
        assert_eq!(updated.predicates[0].evidence.eligible, 2);
        assert_eq!(updated.predicates[0].evidence.holds, 1);
        assert_eq!(updated.predicates[0].evidence.violated, 1);
        assert_eq!(updated.predicates[0].evidence.not_applicable, 1);
        assert_eq!(
            updated.predicates[0].evidence.supporting_witnesses,
            ["first"]
        );
        assert_eq!(
            updated.predicates[0].evidence.refuting_witnesses,
            ["second"]
        );
        assert_eq!(updated.predicates[1], unrelated);
        assert_eq!(bundle.predicates[0].evidence, PredicateEvidence::default());

        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("predicates.json");
        write_path_predicate_bundle(&path, &updated).expect("persist collected evidence");
        let reloaded = read_path_predicate_bundle(&path).expect("reload collected evidence");
        let reloaded_matching = reloaded
            .predicates
            .iter()
            .find(|record| record.target == target("example.f"))
            .expect("matching target");
        assert_eq!(reloaded_matching.evidence, updated.predicates[0].evidence);
    }

    #[test]
    fn unavailable_context_return_and_path_are_inapplicable() {
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![predicate(target("example.f"), vec![(7, true)])],
        };
        let path = vec![decision(7, true)];
        let output = ObservationOutput {
            function_name: "example.f".into(),
            raw_results: vec![
                execution(
                    vec![json!(1), json!(2)],
                    path.clone(),
                    Some(json!(0)),
                    false,
                ),
                execution(
                    vec![json!(1), json!(2)],
                    path.clone(),
                    Some(json!(0)),
                    false,
                ),
                execution(vec![json!(1), json!(2)], path.clone(), None, false),
                execution(
                    vec![json!(1), json!(2)],
                    vec![decision(7, false)],
                    Some(json!(0)),
                    false,
                ),
                execution(vec![json!(1), json!(2)], path, Some(json!(null)), false),
            ],
            ..ObservationOutput::default()
        };
        let metadata = [
            None,
            Some("different".into()),
            Some("context-v1".into()),
            Some("context-v1".into()),
            Some("context-v1".into()),
        ]
        .map(|context_fingerprint| ExecutionEvidenceMetadata {
            context_fingerprint,
            witness: None,
        });
        let updated =
            collect_path_predicate_evidence(&bundle, &output, &target("example.f"), &metadata)
                .expect("collect evidence");
        assert_eq!(updated.predicates[0].evidence.eligible, 1);
        assert_eq!(updated.predicates[0].evidence.holds, 1);
        assert_eq!(updated.predicates[0].evidence.not_applicable, 4);
    }

    #[test]
    fn rejects_invalid_bundle_metadata_and_empty_witness() {
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![predicate(target("example.f"), vec![])],
        };
        let output = ObservationOutput {
            function_name: "example.f".into(),
            raw_results: vec![execution(
                vec![json!(1), json!(2)],
                vec![],
                Some(json!(0)),
                false,
            )],
            ..ObservationOutput::default()
        };
        let mut invalid = bundle.clone();
        invalid.schema_version += 1;
        assert!(matches!(
            collect_path_predicate_evidence(
                &invalid,
                &output,
                &target("example.f"),
                &[ExecutionEvidenceMetadata::default()],
            ),
            Err(PathEvidenceCollectionError::InvalidBundle(
                PathPredicateStoreError::UnsupportedVersion
            ))
        ));
        assert!(matches!(
            collect_path_predicate_evidence(&bundle, &output, &target("example.f"), &[]),
            Err(PathEvidenceCollectionError::MetadataLengthMismatch)
        ));
        assert!(matches!(
            collect_path_predicate_evidence(
                &bundle,
                &output,
                &target("example.f"),
                &[ExecutionEvidenceMetadata {
                    context_fingerprint: Some("context-v1".into()),
                    witness: Some(String::new()),
                }],
            ),
            Err(PathEvidenceCollectionError::EmptyWitness)
        ));
        assert_eq!(bundle.predicates[0].evidence, PredicateEvidence::default());
    }

    #[test]
    fn late_counter_overflow_leaves_input_bundle_unchanged() {
        let mut record = predicate(target("example.f"), vec![]);
        record.evidence.eligible = u32::MAX - 1;
        record.evidence.holds = u32::MAX - 1;
        let bundle = PathPredicateBundle {
            schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
            predicates: vec![record],
        };
        let observation = execution(vec![json!(1), json!(2)], vec![], Some(json!(0)), false);
        let output = ObservationOutput {
            function_name: "example.f".into(),
            raw_results: vec![observation.clone(), observation],
            ..ObservationOutput::default()
        };
        let metadata = vec![
            ExecutionEvidenceMetadata {
                context_fingerprint: Some("context-v1".into()),
                witness: None,
            };
            2
        ];
        let original = bundle.clone();
        assert!(matches!(
            collect_path_predicate_evidence(&bundle, &output, &target("example.f"), &metadata),
            Err(PathEvidenceCollectionError::Evidence(
                PredicateEvidenceError::Overflow
            ))
        ));
        assert_eq!(bundle, original);
    }

    #[test]
    fn collection_preserves_each_record_lifecycle() {
        let lifecycles = [
            PredicateLifecycle::Candidate,
            PredicateLifecycle::Frozen,
            PredicateLifecycle::Refuted,
            PredicateLifecycle::Stale,
        ];
        let output = ObservationOutput {
            function_name: "f".into(),
            raw_results: vec![execution(
                vec![json!(1), json!(2)],
                vec![],
                Some(json!(0)),
                false,
            )],
            ..ObservationOutput::default()
        };
        let metadata = [ExecutionEvidenceMetadata {
            context_fingerprint: Some("context-v1".into()),
            witness: None,
        }];
        for lifecycle in lifecycles {
            let mut record = predicate(target("module.f"), vec![]);
            record.lifecycle = lifecycle;
            let bundle = PathPredicateBundle {
                schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
                predicates: vec![record],
            };
            let updated =
                collect_path_predicate_evidence(&bundle, &output, &target("module.f"), &metadata)
                    .expect("collect evidence");
            assert_eq!(updated.predicates[0].lifecycle, lifecycle);
            assert_eq!(updated.predicates[0].evidence.eligible, 1);
        }
    }

    proptest! {
        #[test]
        fn collected_counts_match_raw_results_or_fail_atomically(
            pairs in proptest::collection::vec((0u8..10, 0u8..10), 1..20),
            overflow in any::<bool>(),
        ) {
            let mut matching = predicate(target("example.f"), vec![(7, true)]);
            if overflow {
                matching.evidence.eligible = u32::MAX;
                matching.evidence.holds = u32::MAX;
            }
            let unrelated = predicate(target("example.other"), vec![(7, true)]);
            let bundle = PathPredicateBundle {
                schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
                predicates: vec![matching, unrelated.clone()],
            };
            let output = ObservationOutput {
                function_name: "example.f".into(),
                raw_results: pairs.iter().map(|(left, right)| {
                    execution(
                        vec![json!(left), json!(right)],
                        vec![decision(7, true)],
                        Some(json!(null)),
                        false,
                    )
                }).collect(),
                ..ObservationOutput::default()
            };
            let metadata = vec![ExecutionEvidenceMetadata {
                context_fingerprint: Some("context-v1".into()),
                witness: None,
            }; pairs.len()];
            let original = bundle.clone();
            let result = collect_path_predicate_evidence(
                &bundle,
                &output,
                &target("example.f"),
                &metadata,
            );
            if overflow {
                prop_assert!(matches!(
                    result,
                    Err(PathEvidenceCollectionError::Evidence(
                        PredicateEvidenceError::Overflow
                    ))
                ));
                prop_assert_eq!(bundle, original);
            } else {
                let updated = result.expect("valid projection");
                let evidence = &updated.predicates[0].evidence;
                let expected_holds = pairs.iter().filter(|(left, right)| left < right).count();
                prop_assert_eq!(evidence.eligible as usize, pairs.len());
                prop_assert_eq!(evidence.holds as usize, expected_holds);
                prop_assert_eq!(evidence.violated as usize, pairs.len() - expected_holds);
                prop_assert_eq!(evidence.not_applicable, 0);
                prop_assert_eq!(&updated.predicates[1], &unrelated);
                prop_assert_eq!(bundle, original);
            }
        }
    }
}
