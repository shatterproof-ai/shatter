//! Revalidation loop: re-execute previously-interesting inputs and classify drift.
//!
//! When a previously-interesting input is re-executed against the current
//! version of a function, these types classify what changed and why.
//! The [`revalidate_behaviors`] function drives the loop: for each behavior
//! in a [`BehaviorMap`], it replays the input via a frontend subprocess,
//! compares observed vs. recorded branch paths (masking nondeterministic
//! fields), and emits a [`RevalidationReport`] with a verdict.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::behavior::BehaviorMap;
use crate::execution_record::BranchDecision;
use crate::frontend::{Frontend, FrontendError};
use crate::interesting_pool::{Severity, classify_severity};
use crate::nondeterminism::NondeterministicField;
use crate::protocol::{Command as ProtoCommand, ResponseResult};

/// Classification of what happened when replaying a previously-interesting input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationVerdict {
    /// Behavior unchanged — same branch path and severity.
    Confirmed,
    /// Code fingerprint changed and behavior changed — expected drift.
    ExpectedDrift,
    /// Code unchanged but behavior changed — nondeterminism or environment.
    Flaky,
    /// Code changed and previously-interesting behavior vanished.
    PotentialRegression,
    /// Behavior became less severe (potential silent fix).
    SeverityDowngrade,
    /// Behavior became more severe (potential new bug).
    SeverityUpgrade,
}

impl fmt::Display for RevalidationVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Confirmed => write!(f, "confirmed"),
            Self::ExpectedDrift => write!(f, "expected drift"),
            Self::Flaky => write!(f, "flaky"),
            Self::PotentialRegression => write!(f, "potential regression"),
            Self::SeverityDowngrade => write!(f, "severity downgrade"),
            Self::SeverityUpgrade => write!(f, "severity upgrade"),
        }
    }
}

/// Result of re-executing a previously-interesting input against the current code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevalidationReport {
    /// Fully qualified function name.
    pub function_name: String,
    /// The input vector that was replayed.
    pub input_vector: Vec<serde_json::Value>,
    /// Branch path from the original exploration.
    pub expected_branch_path: Vec<BranchDecision>,
    /// Branch path observed during revalidation.
    pub observed_branch_path: Vec<BranchDecision>,
    /// Severity from the original exploration.
    pub expected_severity: Severity,
    /// Severity observed during revalidation, or `None` if the behavior vanished.
    pub observed_severity: Option<Severity>,
    /// Classification of the revalidation outcome.
    pub verdict: RevalidationVerdict,
    /// Milliseconds since Unix epoch when the revalidation was performed.
    pub timestamp_epoch_ms: u64,
}

/// Returns the current time as milliseconds since Unix epoch.
pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Classify a revalidation outcome into a verdict.
///
/// Priority order: severity changes take precedence over path changes when
/// both the path and severity differ, because severity shifts have more
/// actionable signal. When only the path changed, we distinguish code-change
/// drift from flaky nondeterminism.
pub fn classify_verdict(
    code_changed: bool,
    path_matches: bool,
    expected_severity: Severity,
    observed_severity: Option<Severity>,
) -> RevalidationVerdict {
    match observed_severity {
        // Behavior vanished entirely.
        None => {
            if code_changed {
                RevalidationVerdict::PotentialRegression
            } else {
                RevalidationVerdict::Flaky
            }
        }
        Some(observed) => {
            // Check severity shift first — more actionable than path changes.
            if observed < expected_severity {
                return RevalidationVerdict::SeverityDowngrade;
            }
            if observed > expected_severity {
                return RevalidationVerdict::SeverityUpgrade;
            }
            // Same severity — classify based on path match.
            if path_matches {
                RevalidationVerdict::Confirmed
            } else if code_changed {
                RevalidationVerdict::ExpectedDrift
            } else {
                RevalidationVerdict::Flaky
            }
        }
    }
}

/// Derive severity from a recorded behavior's thrown_error field.
fn severity_from_behavior(behavior: &crate::behavior::Behavior) -> Severity {
    classify_severity(behavior.thrown_error.as_ref(), false)
}

/// Compare two branch paths, ignoring branches whose divergence is explained
/// by nondeterministic fields.
///
/// Two paths match if they have the same length and each pair shares the same
/// `(branch_id, taken)`. Constraint text is ignored — it is symbolic metadata,
/// not behavioral output. If any nondeterministic field has path prefix `"branch"`,
/// all branch divergences are masked (the whole path is considered nondeterministic).
pub fn branch_paths_match(
    expected: &[BranchDecision],
    observed: &[BranchDecision],
    nondeterministic_fields: &[NondeterministicField],
) -> bool {
    // If any nondeterministic field covers branches wholesale, skip comparison.
    if nondeterministic_fields
        .iter()
        .any(|f| f.field_path == "branch" || f.field_path.starts_with("branch."))
    {
        return true;
    }

    if expected.len() != observed.len() {
        return false;
    }
    expected
        .iter()
        .zip(observed.iter())
        .all(|(e, o)| e.branch_id == o.branch_id && e.taken == o.taken)
}

/// Re-execute each behavior in a [`BehaviorMap`] against the current code
/// and classify the result.
///
/// `current_fingerprint` is the freshly-computed fingerprint of the function's
/// source. If it differs from `behavior_map.fingerprint`, the code has changed.
/// Nondeterministic fields from the behavior map are used to mask expected
/// flakiness in branch path comparisons.
///
/// Returns one [`RevalidationReport`] per behavior. Frontend errors during
/// individual executions produce a `None` observed_severity (behavior vanished).
pub async fn revalidate_behaviors(
    frontend: &mut Frontend,
    behavior_map: &BehaviorMap,
    current_fingerprint: Option<&str>,
) -> Result<Vec<RevalidationReport>, FrontendError> {
    let code_changed = match (&behavior_map.fingerprint, current_fingerprint) {
        (Some(old), Some(new)) => old != new,
        // Missing fingerprint on either side → conservative: treat as changed.
        _ => true,
    };

    let nondet_fields = &behavior_map.nondeterministic_fields;
    let mut reports = Vec::with_capacity(behavior_map.behaviors.len());

    for behavior in &behavior_map.behaviors {
        let expected_severity = severity_from_behavior(behavior);
        let expected_branch_path = &behavior.branch_path;

        let exec_result = frontend
            .send(ProtoCommand::Execute {
                function: behavior_map.function_id.clone(),
                inputs: behavior.input_args.clone(),
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            })
            .await;

        let (observed_branch_path, observed_severity) = match exec_result {
            Ok(response) => match response.result {
                ResponseResult::Execute(exec) => {
                    let sev = classify_severity(exec.thrown_error.as_ref(), false);
                    (exec.branch_path.clone(), Some(sev))
                }
                ResponseResult::Error { .. } => {
                    // Frontend returned a protocol-level error — behavior vanished.
                    (vec![], None)
                }
                // Other response types are unexpected for an Execute command.
                _ => (vec![], None),
            },
            Err(_) => {
                // Communication failure — treat as behavior vanished.
                (vec![], None)
            }
        };

        let path_matches =
            branch_paths_match(expected_branch_path, &observed_branch_path, nondet_fields);

        let verdict = classify_verdict(
            code_changed,
            path_matches,
            expected_severity,
            observed_severity,
        );

        reports.push(RevalidationReport {
            function_name: behavior_map.function_id.clone(),
            input_vector: behavior.input_args.clone(),
            expected_branch_path: expected_branch_path.clone(),
            observed_branch_path,
            expected_severity,
            observed_severity,
            verdict,
            timestamp_epoch_ms: now_epoch_ms(),
        });
    }

    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::SymConstraint;

    #[test]
    fn confirmed_when_nothing_changed() {
        let v = classify_verdict(false, true, Severity::RarePath, Some(Severity::RarePath));
        assert_eq!(v, RevalidationVerdict::Confirmed);
    }

    #[test]
    fn confirmed_when_code_changed_but_behavior_identical() {
        let v = classify_verdict(
            true,
            true,
            Severity::HandledError,
            Some(Severity::HandledError),
        );
        assert_eq!(v, RevalidationVerdict::Confirmed);
    }

    #[test]
    fn expected_drift_when_code_changed_and_path_differs() {
        let v = classify_verdict(true, false, Severity::RarePath, Some(Severity::RarePath));
        assert_eq!(v, RevalidationVerdict::ExpectedDrift);
    }

    #[test]
    fn flaky_when_code_unchanged_but_path_differs() {
        let v = classify_verdict(false, false, Severity::RarePath, Some(Severity::RarePath));
        assert_eq!(v, RevalidationVerdict::Flaky);
    }

    #[test]
    fn potential_regression_when_code_changed_and_behavior_vanished() {
        let v = classify_verdict(true, false, Severity::UnhandledError, None);
        assert_eq!(v, RevalidationVerdict::PotentialRegression);
    }

    #[test]
    fn flaky_when_code_unchanged_and_behavior_vanished() {
        let v = classify_verdict(false, false, Severity::Crash, None);
        assert_eq!(v, RevalidationVerdict::Flaky);
    }

    #[test]
    fn severity_downgrade() {
        let v = classify_verdict(
            true,
            false,
            Severity::UnhandledError,
            Some(Severity::RarePath),
        );
        assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
    }

    #[test]
    fn severity_upgrade() {
        let v = classify_verdict(false, true, Severity::RarePath, Some(Severity::Crash));
        assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
    }

    #[test]
    fn severity_upgrade_takes_precedence_over_path_match() {
        // Even though path matches, severity increased — report as upgrade.
        let v = classify_verdict(
            false,
            true,
            Severity::HandledError,
            Some(Severity::UnhandledError),
        );
        assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
    }

    #[test]
    fn severity_downgrade_takes_precedence_over_drift() {
        // Code changed and path differs, but severity decreased — report as downgrade.
        let v = classify_verdict(true, false, Severity::Crash, Some(Severity::HandledError));
        assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
    }

    #[test]
    fn display_impl() {
        assert_eq!(RevalidationVerdict::Confirmed.to_string(), "confirmed");
        assert_eq!(
            RevalidationVerdict::ExpectedDrift.to_string(),
            "expected drift"
        );
        assert_eq!(RevalidationVerdict::Flaky.to_string(), "flaky");
        assert_eq!(
            RevalidationVerdict::PotentialRegression.to_string(),
            "potential regression"
        );
        assert_eq!(
            RevalidationVerdict::SeverityDowngrade.to_string(),
            "severity downgrade"
        );
        assert_eq!(
            RevalidationVerdict::SeverityUpgrade.to_string(),
            "severity upgrade"
        );
    }

    #[test]
    fn verdict_serde_round_trip() {
        let verdicts = [
            RevalidationVerdict::Confirmed,
            RevalidationVerdict::ExpectedDrift,
            RevalidationVerdict::Flaky,
            RevalidationVerdict::PotentialRegression,
            RevalidationVerdict::SeverityDowngrade,
            RevalidationVerdict::SeverityUpgrade,
        ];
        for v in &verdicts {
            let json = serde_json::to_string(v).expect("serialize verdict");
            let restored: RevalidationVerdict =
                serde_json::from_str(&json).expect("deserialize verdict");
            assert_eq!(*v, restored);
        }
    }

    #[test]
    fn report_serde_round_trip() {
        use crate::execution_record::SymConstraint;

        let report = RevalidationReport {
            function_name: "validateEmail".into(),
            input_vector: vec![serde_json::json!("test@example.com")],
            expected_branch_path: vec![BranchDecision {
                branch_id: 1,
                taken: true,
                line: 5,
                constraint: SymConstraint::Unknown {
                    hint: "email.includes('@')".into(),
                },
                conditions: None,
            }],
            observed_branch_path: vec![BranchDecision {
                branch_id: 1,
                taken: false,
                line: 5,
                constraint: SymConstraint::Unknown {
                    hint: "email.includes('@')".into(),
                },
                conditions: None,
            }],
            expected_severity: Severity::RarePath,
            observed_severity: Some(Severity::HandledError),
            verdict: RevalidationVerdict::SeverityUpgrade,
            timestamp_epoch_ms: 1_700_000_000_000,
        };

        let json = serde_json::to_string(&report).expect("serialize report");
        let restored: RevalidationReport = serde_json::from_str(&json).expect("deserialize report");
        assert_eq!(report, restored);
    }

    #[test]
    fn now_epoch_ms_returns_reasonable_value() {
        let ms = now_epoch_ms();
        // Should be after 2020-01-01 (1_577_836_800_000 ms).
        assert!(ms > 1_577_836_800_000);
    }

    // -- branch_paths_match tests --

    fn make_branch(id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id: id,
            line: 1,
            taken,
            constraint: SymConstraint::Unknown {
                hint: String::new(),
            },
            conditions: None,
        }
    }

    #[test]
    fn paths_match_identical() {
        let path = vec![make_branch(1, true), make_branch(2, false)];
        assert!(branch_paths_match(&path, &path, &[]));
    }

    #[test]
    fn paths_differ_in_taken() {
        let a = vec![make_branch(1, true)];
        let b = vec![make_branch(1, false)];
        assert!(!branch_paths_match(&a, &b, &[]));
    }

    #[test]
    fn paths_differ_in_length() {
        let a = vec![make_branch(1, true)];
        let b = vec![make_branch(1, true), make_branch(2, false)];
        assert!(!branch_paths_match(&a, &b, &[]));
    }

    #[test]
    fn paths_differ_in_branch_id() {
        let a = vec![make_branch(1, true)];
        let b = vec![make_branch(2, true)];
        assert!(!branch_paths_match(&a, &b, &[]));
    }

    #[test]
    fn paths_match_ignores_constraint_text() {
        let a = vec![BranchDecision {
            branch_id: 1,
            line: 5,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "x > 0".into(),
            },
            conditions: None,
        }];
        let b = vec![BranchDecision {
            branch_id: 1,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "different".into(),
            },
            conditions: None,
        }];
        assert!(branch_paths_match(&a, &b, &[]));
    }

    #[test]
    fn paths_masked_by_nondeterministic_branch_field() {
        use crate::nondeterminism::{Confidence, NondeterministicField};
        let a = vec![make_branch(1, true)];
        let b = vec![make_branch(1, false)]; // Different!
        let nondet = vec![NondeterministicField {
            field_path: "branch".into(),
            evidence: vec![],
            confidence: Confidence::High,
        }];
        assert!(branch_paths_match(&a, &b, &nondet));
    }

    #[test]
    fn paths_masked_by_nondeterministic_branch_subfield() {
        use crate::nondeterminism::{Confidence, NondeterministicField};
        let a = vec![make_branch(1, true)];
        let b = vec![make_branch(2, true)]; // Different!
        let nondet = vec![NondeterministicField {
            field_path: "branch.condition".into(),
            evidence: vec![],
            confidence: Confidence::Medium,
        }];
        assert!(branch_paths_match(&a, &b, &nondet));
    }

    #[test]
    fn both_empty_paths_match() {
        assert!(branch_paths_match(&[], &[], &[]));
    }

    // -- severity_from_behavior tests --

    #[test]
    fn severity_rare_path_from_behavior() {
        let b = crate::behavior::Behavior {
            id: 0,
            input_args: vec![],
            return_value: Some(serde_json::json!(0)),
            thrown_error: None,
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        };
        assert_eq!(severity_from_behavior(&b), Severity::RarePath);
    }

    #[test]
    fn severity_unhandled_from_behavior() {
        use crate::execution_record::ErrorInfo;
        let b = crate::behavior::Behavior {
            id: 0,
            input_args: vec![],
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "TypeError".into(),
                message: "oops".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        };
        assert_eq!(severity_from_behavior(&b), Severity::UnhandledError);
    }

    #[test]
    fn severity_handled_from_behavior() {
        use crate::execution_record::ErrorInfo;
        let b = crate::behavior::Behavior {
            id: 0,
            input_args: vec![],
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "ValidationError".into(),
                message: "bad input".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        };
        assert_eq!(severity_from_behavior(&b), Severity::HandledError);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::test_arbitraries::arb_branch_decision;
    use proptest::prelude::*;

    fn arb_severity() -> impl Strategy<Value = Severity> {
        prop_oneof![
            Just(Severity::RarePath),
            Just(Severity::HandledError),
            Just(Severity::UnhandledError),
            Just(Severity::Crash),
        ]
    }

    proptest! {
        /// classify_verdict always returns Confirmed when path matches and severity is unchanged.
        #[test]
        fn confirmed_when_path_and_severity_match(
            code_changed in any::<bool>(),
            severity in arb_severity(),
        ) {
            let v = classify_verdict(code_changed, true, severity, Some(severity));
            prop_assert_eq!(v, RevalidationVerdict::Confirmed);
        }

        /// Severity upgrade always wins regardless of path or code_changed.
        #[test]
        fn severity_upgrade_always_wins(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
        ) {
            let v = classify_verdict(code_changed, path_matches, Severity::RarePath, Some(Severity::Crash));
            prop_assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
        }

        /// Severity downgrade always wins regardless of path or code_changed.
        #[test]
        fn severity_downgrade_always_wins(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
        ) {
            let v = classify_verdict(code_changed, path_matches, Severity::Crash, Some(Severity::RarePath));
            prop_assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
        }

        /// None observed_severity produces PotentialRegression or Flaky (never Confirmed).
        #[test]
        fn vanished_never_confirmed(
            code_changed in any::<bool>(),
            severity in arb_severity(),
        ) {
            let v = classify_verdict(code_changed, false, severity, None);
            prop_assert_ne!(v, RevalidationVerdict::Confirmed);
        }

        /// branch_paths_match is reflexive: any path matches itself.
        #[test]
        fn branch_paths_match_reflexive(
            path in prop::collection::vec(arb_branch_decision(), 0..=5),
        ) {
            prop_assert!(branch_paths_match(&path, &path, &[]));
        }

        /// branch_paths_match: different lengths never match (without nondet mask).
        #[test]
        fn branch_paths_different_length_never_match(
            a in prop::collection::vec(arb_branch_decision(), 1..=3),
            extra in arb_branch_decision(),
        ) {
            let mut b = a.clone();
            b.push(extra);
            prop_assert!(!branch_paths_match(&a, &b, &[]));
        }
    }
}
