//! Data model for nondeterminism detection.
//!
//! Presence in the nondeterministic field list means "we have evidence
//! this is nondeterministic." Absence does NOT assert determinism.

use serde::{Deserialize, Serialize};

/// How nondeterminism was detected for a field or parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NondeterminismEvidence {
    /// Explicitly declared by the user (e.g., config or annotation).
    UserDeclared,
    /// Different outputs observed for the same input within a single run.
    ObservedWithinRun,
    /// Different outputs observed for the same input across separate runs.
    ObservedAcrossRuns,
    /// Matched a known nondeterministic API pattern (e.g., `Date.now()`, `Math.random()`).
    PatternMatch { pattern: String },
    /// Name heuristic suggests nondeterminism (e.g., "timestamp", "random", "uuid").
    NameHeuristic { matched_name: String },
}

/// Confidence that a field is nondeterministic, based on accumulated evidence.
///
/// Ordered low-to-high so that [`Ord`] gives natural confidence comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// A parameter or field identified as potentially nondeterministic.
///
/// The `evidence` vector accumulates over time — multiple detection methods
/// may independently flag the same field, increasing confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NondeterministicField {
    /// Path to the field (e.g., "param0", "param1.timestamp", "return.id").
    pub field_path: String,
    /// Evidence supporting the nondeterminism classification.
    pub evidence: Vec<NondeterminismEvidence>,
    /// Overall confidence derived from the evidence.
    pub confidence: Confidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_nondeterministic_field() {
        let field = NondeterministicField {
            field_path: "param0.timestamp".into(),
            evidence: vec![NondeterminismEvidence::ObservedAcrossRuns],
            confidence: Confidence::Medium,
        };
        assert_eq!(field.field_path, "param0.timestamp");
        assert_eq!(field.evidence.len(), 1);
        assert_eq!(field.confidence, Confidence::Medium);
    }

    #[test]
    fn serialize_deserialize_round_trip() {
        let field = NondeterministicField {
            field_path: "return.id".into(),
            evidence: vec![
                NondeterminismEvidence::PatternMatch {
                    pattern: "Math.random()".into(),
                },
                NondeterminismEvidence::NameHeuristic {
                    matched_name: "random".into(),
                },
            ],
            confidence: Confidence::High,
        };

        let json = serde_json::to_string(&field).expect("serialize");
        let restored: NondeterministicField =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(field, restored);
    }

    #[test]
    fn evidence_variants_round_trip() {
        let variants = vec![
            NondeterminismEvidence::UserDeclared,
            NondeterminismEvidence::ObservedWithinRun,
            NondeterminismEvidence::ObservedAcrossRuns,
            NondeterminismEvidence::PatternMatch {
                pattern: "Date.now()".into(),
            },
            NondeterminismEvidence::NameHeuristic {
                matched_name: "uuid".into(),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).expect("serialize");
            let restored: NondeterminismEvidence =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*variant, restored);
        }
    }

    #[test]
    fn confidence_ordering() {
        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
    }

    #[test]
    fn multiple_evidence_accumulates() {
        let mut field = NondeterministicField {
            field_path: "param0".into(),
            evidence: vec![NondeterminismEvidence::NameHeuristic {
                matched_name: "timestamp".into(),
            }],
            confidence: Confidence::Low,
        };

        field
            .evidence
            .push(NondeterminismEvidence::ObservedAcrossRuns);
        field.confidence = Confidence::High;

        assert_eq!(field.evidence.len(), 2);
        assert_eq!(field.confidence, Confidence::High);
    }
}
