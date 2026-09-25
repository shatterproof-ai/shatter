//! Frontend-independent wire fixtures for the v1 path predicate contract.

use serde::Deserialize;
use serde_json::Value;
use shatter_core::invariants::{
    PathObservation, PathPredicateRecord, PredicateEvaluation, evaluate_path_predicate,
    parse_path_predicate,
};

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    cases: Vec<FixtureCase>,
}

#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    predicate: Value,
    observation: PathObservation,
    expected: PredicateEvaluation,
}

#[test]
fn path_predicate_manifest_matches_v1_evaluation() {
    let manifest: Manifest =
        serde_json::from_str(include_str!("fixtures/path_invariant/manifest.json"))
            .expect("valid fixture manifest");
    assert_eq!(manifest.schema_version, 1);
    assert!(!manifest.cases.is_empty());

    for case in manifest.cases {
        let raw = serde_json::to_string(&case.predicate).expect("serializable predicate");
        let predicate: PathPredicateRecord = if case.name == "unsupported_schema" {
            // The parser rejects unknown versions. Construct the typed record to
            // exercise the evaluator's separate fail-closed guarantee.
            assert!(parse_path_predicate(&raw).is_err(), "{}", case.name);
            serde_json::from_value(case.predicate).expect("typed unknown-version predicate")
        } else {
            parse_path_predicate(&raw)
                .unwrap_or_else(|error| panic!("{}: predicate did not parse: {error}", case.name))
        };

        assert_eq!(
            evaluate_path_predicate(&predicate, &case.observation),
            case.expected,
            "fixture case: {}",
            case.name,
        );
    }
}
