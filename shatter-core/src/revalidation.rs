//! Data model for revalidation verdicts and reports.
//!
//! When a previously-interesting input is re-executed against the current
//! version of a function, these types classify what changed and why.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::execution_record::BranchDecision;
use crate::interesting_pool::Severity;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_when_nothing_changed() {
        let v = classify_verdict(false, true, Severity::RarePath, Some(Severity::RarePath));
        assert_eq!(v, RevalidationVerdict::Confirmed);
    }

    #[test]
    fn confirmed_when_code_changed_but_behavior_identical() {
        let v = classify_verdict(true, true, Severity::HandledError, Some(Severity::HandledError));
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
        let v = classify_verdict(
            false,
            true,
            Severity::RarePath,
            Some(Severity::Crash),
        );
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
        let v = classify_verdict(
            true,
            false,
            Severity::Crash,
            Some(Severity::HandledError),
        );
        assert_eq!(v, RevalidationVerdict::SeverityDowngrade);
    }

    #[test]
    fn display_impl() {
        assert_eq!(RevalidationVerdict::Confirmed.to_string(), "confirmed");
        assert_eq!(RevalidationVerdict::ExpectedDrift.to_string(), "expected drift");
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
            }],
            observed_branch_path: vec![BranchDecision {
                branch_id: 1,
                taken: false,
                line: 5,
                constraint: SymConstraint::Unknown {
                    hint: "email.includes('@')".into(),
                },
            }],
            expected_severity: Severity::RarePath,
            observed_severity: Some(Severity::HandledError),
            verdict: RevalidationVerdict::SeverityUpgrade,
            timestamp_epoch_ms: 1_700_000_000_000,
        };

        let json = serde_json::to_string(&report).expect("serialize report");
        let restored: RevalidationReport =
            serde_json::from_str(&json).expect("deserialize report");
        assert_eq!(report, restored);
    }

    #[test]
    fn now_epoch_ms_returns_reasonable_value() {
        let ms = now_epoch_ms();
        // Should be after 2020-01-01 (1_577_836_800_000 ms).
        assert!(ms > 1_577_836_800_000);
    }
}
