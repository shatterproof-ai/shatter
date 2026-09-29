//! Revalidation loop: re-execute previously-interesting inputs and classify drift.
//!
//! When a previously-interesting input is re-executed against the current
//! version of a function, these types classify what changed and why.
//! The [`revalidate_behaviors`] function drives the loop: for each behavior
//! in a [`BehaviorMap`], it replays the input via a frontend subprocess,
//! compares observed vs. recorded branch paths and outputs (masking
//! nondeterministic fields), and emits a [`RevalidationReport`] with a verdict.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::behavior::BehaviorMap;
use crate::execution_record::BranchDecision;
use crate::frontend::{Frontend, FrontendError};
use crate::interesting_pool::{Severity, classify_severity};
use crate::nondeterminism::{NondeterministicField, outputs_match};
use crate::protocol::{Command as ProtoCommand, ResponseResult};

/// Classification of what happened when replaying a previously-interesting input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationVerdict {
    /// Behavior unchanged — same branch path, severity and output.
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
    /// Same severity, but the return value or thrown error differs from the
    /// recorded one (after nondeterminism masking). Always a regression.
    OutputChanged,
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
            Self::OutputChanged => write!(f, "output changed"),
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
    /// Whether the observed return value / thrown error matched the recorded
    /// one after nondeterminism masking.
    #[serde(default = "default_output_matches")]
    pub output_matches: bool,
    /// Classification of the revalidation outcome.
    pub verdict: RevalidationVerdict,
    /// Milliseconds since Unix epoch when the revalidation was performed.
    pub timestamp_epoch_ms: u64,
}

fn default_output_matches() -> bool {
    true
}

/// Aggregate verdict counts for a set of [`RevalidationReport`]s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevalidationSummary {
    /// Behaviors with verdict [`RevalidationVerdict::Confirmed`].
    pub confirmed: usize,
    /// Behaviors with verdict [`RevalidationVerdict::ExpectedDrift`].
    pub expected_drift: usize,
    /// Behaviors with any other verdict (regressions and flakiness).
    pub regressed: usize,
    /// Total behaviors replayed.
    pub total: usize,
}

impl RevalidationSummary {
    /// Count verdicts across `reports`.
    pub fn from_reports(reports: &[RevalidationReport]) -> Self {
        let mut summary = Self {
            total: reports.len(),
            ..Self::default()
        };
        for r in reports {
            match r.verdict {
                RevalidationVerdict::Confirmed => summary.confirmed += 1,
                RevalidationVerdict::ExpectedDrift => summary.expected_drift += 1,
                _ => summary.regressed += 1,
            }
        }
        summary
    }

    /// Whether revalidation passes (exit 0). Regressions always fail; expected
    /// drift fails unless `allow_drift` is set.
    pub fn passes(&self, allow_drift: bool) -> bool {
        self.regressed == 0 && (allow_drift || self.expected_drift == 0)
    }
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
/// Priority order: severity changes take precedence over output and path
/// changes, because severity shifts have more actionable signal. A changed
/// output (`output_matches == false`) at the same severity is always
/// [`RevalidationVerdict::OutputChanged`], whether or not the path or code
/// changed. When only the path changed, we distinguish code-change drift from
/// flaky nondeterminism.
pub fn classify_verdict(
    code_changed: bool,
    path_matches: bool,
    output_matches: bool,
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
            if !output_matches {
                return RevalidationVerdict::OutputChanged;
            }
            // Same severity, same output — classify based on path match.
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
/// flakiness in branch path and output comparisons.
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

        let (observed_branch_path, observed_severity, output_matches) = match exec_result {
            Ok(response) => match response.result {
                ResponseResult::Execute(exec) => {
                    let sev = classify_severity(exec.thrown_error.as_ref(), false);
                    let output_ok = outputs_match(
                        behavior.return_value.as_ref(),
                        behavior.thrown_error.as_ref(),
                        exec.return_value.as_ref(),
                        exec.thrown_error.as_ref(),
                        nondet_fields,
                    );
                    (exec.branch_path.clone(), Some(sev), output_ok)
                }
                ResponseResult::Error { .. } => {
                    // Frontend returned a protocol-level error — behavior vanished.
                    (vec![], None, true)
                }
                // Other response types are unexpected for an Execute command.
                _ => (vec![], None, true),
            },
            Err(_) => {
                // Communication failure — treat as behavior vanished.
                (vec![], None, true)
            }
        };

        let path_matches =
            branch_paths_match(expected_branch_path, &observed_branch_path, nondet_fields);

        let verdict = classify_verdict(
            code_changed,
            path_matches,
            output_matches,
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
            output_matches,
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
        let v = classify_verdict(
            false,
            true,
            true,
            Severity::RarePath,
            Some(Severity::RarePath),
        );
        assert_eq!(v, RevalidationVerdict::Confirmed);
    }

    #[test]
    fn confirmed_when_code_changed_but_behavior_identical() {
        let v = classify_verdict(
            true,
            true,
            true,
            Severity::HandledError,
            Some(Severity::HandledError),
        );
        assert_eq!(v, RevalidationVerdict::Confirmed);
    }

    #[test]
    fn expected_drift_when_code_changed_and_path_differs() {
        let v = classify_verdict(
            true,
            false,
            true,
            Severity::RarePath,
            Some(Severity::RarePath),
        );
        assert_eq!(v, RevalidationVerdict::ExpectedDrift);
    }

    #[test]
    fn flaky_when_code_unchanged_but_path_differs() {
        let v = classify_verdict(
            false,
            false,
            true,
            Severity::RarePath,
            Some(Severity::RarePath),
        );
        assert_eq!(v, RevalidationVerdict::Flaky);
    }

    #[test]
    fn potential_regression_when_code_changed_and_behavior_vanished() {
        let v = classify_verdict(true, false, true, Severity::UnhandledError, None);
        assert_eq!(v, RevalidationVerdict::PotentialRegression);
    }

    #[test]
    fn flaky_when_code_unchanged_and_behavior_vanished() {
        let v = classify_verdict(false, false, true, Severity::Crash, None);
        assert_eq!(v, RevalidationVerdict::Flaky);
    }

    #[test]
    fn severity_downgrade() {
        let v = classify_verdict(
            true,
            false,
            true,
            Severity::UnhandledError,
            Some(Severity::RarePath),
        );
        assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
    }

    #[test]
    fn severity_upgrade() {
        let v = classify_verdict(false, true, true, Severity::RarePath, Some(Severity::Crash));
        assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
    }

    #[test]
    fn severity_upgrade_takes_precedence_over_path_match() {
        // Even though path matches, severity increased — report as upgrade.
        let v = classify_verdict(
            false,
            true,
            true,
            Severity::HandledError,
            Some(Severity::UnhandledError),
        );
        assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
    }

    #[test]
    fn severity_downgrade_takes_precedence_over_drift() {
        // Code changed and path differs, but severity decreased — report as downgrade.
        let v = classify_verdict(
            true,
            false,
            true,
            Severity::Crash,
            Some(Severity::HandledError),
        );
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
        assert_eq!(
            RevalidationVerdict::OutputChanged.to_string(),
            "output changed"
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
            RevalidationVerdict::OutputChanged,
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
            output_matches: true,
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
    // -- output comparison tests (str-49drv.14) --

    use crate::execution_record::ErrorInfo;
    use crate::nondeterminism::{
        Confidence, NondeterministicField, detect_within_run_nondeterminism,
    };
    use serde_json::json;

    fn mask(path: &str) -> NondeterministicField {
        NondeterministicField {
            field_path: path.into(),
            evidence: vec![],
            confidence: Confidence::High,
        }
    }

    fn err(t: &str, m: &str, stack: Option<&str>) -> ErrorInfo {
        ErrorInfo {
            error_type: t.into(),
            message: m.into(),
            stack: stack.map(Into::into),
            error_category: None,
        }
    }

    #[test]
    fn output_changed_when_path_matches_and_output_differs() {
        for code_changed in [false, true] {
            let v = classify_verdict(
                code_changed,
                true,
                false,
                Severity::RarePath,
                Some(Severity::RarePath),
            );
            assert_eq!(v, RevalidationVerdict::OutputChanged);
        }
    }

    #[test]
    fn changed_primitive_return_is_a_mismatch() {
        let (a, b) = (json!("zero"), json!("nil"));
        assert!(!outputs_match(Some(&a), None, Some(&b), None, &[]));
    }

    #[test]
    fn return_mask_hides_primitive_return_change() {
        let (a, b) = (json!("zero"), json!("nil"));
        assert!(outputs_match(
            Some(&a),
            None,
            Some(&b),
            None,
            &[mask("return")]
        ));
    }

    #[test]
    fn return_field_mask_hides_only_that_field() {
        let a = json!({"id": 1, "name": "x"});
        let b = json!({"id": 2, "name": "x"});
        assert!(outputs_match(
            Some(&a),
            None,
            Some(&b),
            None,
            &[mask("return.id")]
        ));
        assert!(!outputs_match(
            Some(&a),
            None,
            Some(&b),
            None,
            &[mask("return.name")]
        ));
    }

    #[test]
    fn return_field_mask_does_not_hide_other_field_change() {
        let a = json!({"id": 1, "name": "x"});
        let b = json!({"id": 2, "name": "y"});
        assert!(!outputs_match(
            Some(&a),
            None,
            Some(&b),
            None,
            &[mask("return.id")]
        ));
    }

    #[test]
    fn thrown_error_mask_hides_message_change_only_when_present() {
        let (a, b) = (err("E", "one", None), err("E", "two", None));
        assert!(!outputs_match(None, Some(&a), None, Some(&b), &[]));
        assert!(outputs_match(
            None,
            Some(&a),
            None,
            Some(&b),
            &[mask("thrown_error")]
        ));
    }

    #[test]
    fn errors_with_different_stacks_are_equal() {
        let a = err("E", "boom", Some("at a.ts:1"));
        let b = err("E", "boom", Some("at b.ts:99"));
        assert!(outputs_match(None, Some(&a), None, Some(&b), &[]));
    }

    #[test]
    fn outcome_flip_needs_outcome_mask() {
        let v = json!(1);
        let e = err("E", "boom", None);
        assert!(!outputs_match(Some(&v), None, None, Some(&e), &[]));
        assert!(!outputs_match(
            Some(&v),
            None,
            None,
            Some(&e),
            &[mask("return")]
        ));
        assert!(outputs_match(
            Some(&v),
            None,
            None,
            Some(&e),
            &[mask("<outcome>")]
        ));
    }

    #[test]
    fn producer_written_masks_are_honoured_by_consumer() {
        use crate::protocol::ExecuteResult;
        fn exec(ret: Option<serde_json::Value>) -> ExecuteResult {
            ExecuteResult {
                return_value: ret,
                ..ExecuteResult::default()
            }
        }
        let original = exec(Some(json!({"ts": 1, "v": 7})));
        let reexecs = vec![exec(Some(json!({"ts": 2, "v": 7})))];
        let report = detect_within_run_nondeterminism(&[(original, reexecs)]);
        assert_eq!(report.nondeterministic_fields.len(), 1);
        let expected = json!({"ts": 10, "v": 7});
        let same_v = json!({"ts": 99, "v": 7});
        let changed_v = json!({"ts": 99, "v": 8});
        let fields = &report.nondeterministic_fields;
        assert!(outputs_match(
            Some(&expected),
            None,
            Some(&same_v),
            None,
            fields
        ));
        assert!(!outputs_match(
            Some(&expected),
            None,
            Some(&changed_v),
            None,
            fields
        ));
    }

    #[test]
    fn summary_counts_and_pass_policy() {
        fn rep(v: RevalidationVerdict) -> RevalidationReport {
            RevalidationReport {
                function_name: "f".into(),
                input_vector: vec![],
                expected_branch_path: vec![],
                observed_branch_path: vec![],
                expected_severity: Severity::RarePath,
                observed_severity: Some(Severity::RarePath),
                output_matches: true,
                verdict: v,
                timestamp_epoch_ms: 0,
            }
        }
        let clean = RevalidationSummary::from_reports(&[rep(RevalidationVerdict::Confirmed)]);
        assert!(clean.passes(false) && clean.passes(true));

        let drift = RevalidationSummary::from_reports(&[
            rep(RevalidationVerdict::Confirmed),
            rep(RevalidationVerdict::ExpectedDrift),
        ]);
        assert_eq!(
            (
                drift.confirmed,
                drift.expected_drift,
                drift.regressed,
                drift.total
            ),
            (1, 1, 0, 2)
        );
        assert!(!drift.passes(false), "drift-only fails by default");
        assert!(drift.passes(true), "allow_drift restores exit 0");

        let changed = RevalidationSummary::from_reports(&[
            rep(RevalidationVerdict::ExpectedDrift),
            rep(RevalidationVerdict::OutputChanged),
        ]);
        assert!(
            !changed.passes(true),
            "output change fails even with allow_drift"
        );
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
            let v = classify_verdict(code_changed, true, true, severity, Some(severity));
            prop_assert_eq!(v, RevalidationVerdict::Confirmed);
        }

        /// Severity upgrade always wins regardless of path or code_changed.
        #[test]
        fn severity_upgrade_always_wins(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
        ) {
            let v = classify_verdict(code_changed, path_matches, true, Severity::RarePath, Some(Severity::Crash));
            prop_assert_eq!(v, RevalidationVerdict::SeverityUpgrade);
        }

        /// Severity downgrade always wins regardless of path or code_changed.
        #[test]
        fn severity_downgrade_always_wins(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
        ) {
            let v = classify_verdict(code_changed, path_matches, true, Severity::Crash, Some(Severity::RarePath));
            prop_assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
        }

        /// None observed_severity produces PotentialRegression or Flaky (never Confirmed).
        #[test]
        fn vanished_never_confirmed(
            code_changed in any::<bool>(),
            severity in arb_severity(),
        ) {
            let v = classify_verdict(code_changed, false, true, severity, None);
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

        /// A changed output at unchanged severity is always OutputChanged.
        #[test]
        fn output_change_at_same_severity_is_output_changed(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
            severity in arb_severity(),
        ) {
            let v = classify_verdict(code_changed, path_matches, false, severity, Some(severity));
            prop_assert_eq!(v, RevalidationVerdict::OutputChanged);
        }

        /// Confirmed is only reachable when the output matches.
        #[test]
        fn confirmed_implies_output_matches(
            code_changed in any::<bool>(),
            path_matches in any::<bool>(),
            output_matches in any::<bool>(),
            expected in arb_severity(),
            observed in arb_severity(),
        ) {
            let v = classify_verdict(code_changed, path_matches, output_matches, expected, Some(observed));
            if v == RevalidationVerdict::Confirmed {
                prop_assert!(output_matches && path_matches && expected == observed);
            }
        }

        /// A value always matches itself, and a whole-return mask hides any
        /// difference between two returns.
        #[test]
        fn outputs_match_reflexive_and_return_mask_total(
            a in any::<i64>(),
            b in any::<i64>(),
        ) {
            let (va, vb) = (serde_json::json!(a), serde_json::json!(b));
            prop_assert!(outputs_match(Some(&va), None, Some(&va), None, &[]));
            let mask = crate::nondeterminism::NondeterministicField {
                field_path: "return".into(),
                evidence: vec![],
                confidence: crate::nondeterminism::Confidence::High,
            };
            prop_assert!(outputs_match(Some(&va), None, Some(&vb), None, &[mask]));
            prop_assert_eq!(outputs_match(Some(&va), None, Some(&vb), None, &[]), a == b);
        }

        /// Summary passes(allow_drift=false) implies passes(true); regressions
        /// fail under both policies.
        #[test]
        fn summary_policy_monotone(
            confirmed in 0usize..5,
            drift in 0usize..5,
            regressed in 0usize..5,
        ) {
            let s = RevalidationSummary { confirmed, expected_drift: drift, regressed, total: confirmed + drift + regressed };
            if s.passes(false) { prop_assert!(s.passes(true)); }
            if regressed > 0 { prop_assert!(!s.passes(true)); }
        }
    }
}
