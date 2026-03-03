//! Coverage metrics for concolic exploration results.
//!
//! After exploration, reports what percentage of discovered branches were found
//! by each method (Z3 solving, random/boundary generation, user-provided inputs)
//! and what fraction of branch conditions the frontend expressed as symbolic
//! expressions vs opaque unknowns.

use serde::{Deserialize, Serialize};

use crate::execution_record::SymConstraint;

/// How a branch was discovered during exploration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    /// Found by Z3 constraint solving (negating a path constraint).
    Z3,
    /// Found by random or boundary input generation.
    Random,
    /// Found via user-provided inputs.
    UserProvided,
}

/// Percentage breakdown of discovery methods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodPercentages {
    /// Percentage of branches found by Z3 solving.
    pub z3_pct: f64,
    /// Percentage of branches found by random/boundary generation.
    pub random_pct: f64,
    /// Percentage of branches found by user-provided inputs.
    pub user_provided_pct: f64,
    /// Percentage of branches still uncovered.
    pub uncovered_pct: f64,
}

/// Coverage metrics summarizing how effective the concolic approach was.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverageMetrics {
    /// Total number of branch points in the function.
    pub total_branches: usize,
    /// Branches discovered by Z3 constraint solving.
    pub z3_solved: usize,
    /// Branches discovered by random/boundary generation.
    pub random_found: usize,
    /// Branches discovered via user-provided inputs.
    pub user_provided: usize,
    /// Branches that remain uncovered.
    pub uncovered: usize,
    /// Number of branch constraints expressed as SymExpr (solvable by Z3).
    pub symexpr_count: usize,
    /// Number of branch constraints that were Unknown (opaque to solver).
    pub unknown_count: usize,
}

impl CoverageMetrics {
    /// Compute percentage breakdown by discovery method.
    ///
    /// Returns zero percentages when there are no branches (avoids division by zero).
    pub fn percentages(&self) -> MethodPercentages {
        if self.total_branches == 0 {
            return MethodPercentages {
                z3_pct: 0.0,
                random_pct: 0.0,
                user_provided_pct: 0.0,
                uncovered_pct: 0.0,
            };
        }

        let total = self.total_branches as f64;
        MethodPercentages {
            z3_pct: self.z3_solved as f64 / total * 100.0,
            random_pct: self.random_found as f64 / total * 100.0,
            user_provided_pct: self.user_provided as f64 / total * 100.0,
            uncovered_pct: self.uncovered as f64 / total * 100.0,
        }
    }

    /// Symbolic expression coverage ratio: what fraction of branch conditions
    /// the frontend could express as SymExpr (vs Unknown).
    ///
    /// Returns 0.0 if there are no constraints at all.
    pub fn symexpr_ratio(&self) -> f64 {
        let total = self.symexpr_count + self.unknown_count;
        if total == 0 {
            return 0.0;
        }
        self.symexpr_count as f64 / total as f64
    }

    /// Build metrics from a list of per-branch discovery results and constraints.
    ///
    /// `discoveries` maps each covered branch to the method that first found it.
    /// `total_branches` is the total number of branch points (from static analysis).
    /// `constraints` is the set of all constraints observed across executions.
    pub fn from_exploration(
        total_branches: usize,
        discoveries: &[(u32, DiscoveryMethod)],
        constraints: &[SymConstraint],
    ) -> Self {
        let mut z3_solved = 0usize;
        let mut random_found = 0usize;
        let mut user_provided = 0usize;

        for (_, method) in discoveries {
            match method {
                DiscoveryMethod::Z3 => z3_solved += 1,
                DiscoveryMethod::Random => random_found += 1,
                DiscoveryMethod::UserProvided => user_provided += 1,
            }
        }

        let covered = z3_solved + random_found + user_provided;
        let uncovered = total_branches.saturating_sub(covered);

        let mut symexpr_count = 0usize;
        let mut unknown_count = 0usize;

        for constraint in constraints {
            match constraint {
                SymConstraint::Expr { .. } => symexpr_count += 1,
                SymConstraint::Unknown { .. } => unknown_count += 1,
            }
        }

        Self {
            total_branches,
            z3_solved,
            random_found,
            user_provided,
            uncovered,
            symexpr_count,
            unknown_count,
        }
    }
}

/// Format coverage metrics as a human-readable summary block.
pub fn format_coverage_metrics(metrics: &CoverageMetrics) -> String {
    let mut out = String::new();
    let pct = metrics.percentages();

    out.push_str("  Coverage metrics:\n");
    out.push_str(&format!(
        "    Branches: {} total",
        metrics.total_branches
    ));

    if metrics.total_branches == 0 {
        out.push_str(" (no branches)\n");
        return out;
    }

    out.push('\n');

    if metrics.z3_solved > 0 {
        out.push_str(&format!(
            "    Z3 solved:     {:>3} ({:.0}%)\n",
            metrics.z3_solved, pct.z3_pct
        ));
    }
    if metrics.random_found > 0 {
        out.push_str(&format!(
            "    Random/bound:  {:>3} ({:.0}%)\n",
            metrics.random_found, pct.random_pct
        ));
    }
    if metrics.user_provided > 0 {
        out.push_str(&format!(
            "    User-provided: {:>3} ({:.0}%)\n",
            metrics.user_provided, pct.user_provided_pct
        ));
    }
    if metrics.uncovered > 0 {
        out.push_str(&format!(
            "    Uncovered:     {:>3} ({:.0}%)\n",
            metrics.uncovered, pct.uncovered_pct
        ));
    }

    let constraint_total = metrics.symexpr_count + metrics.unknown_count;
    if constraint_total > 0 {
        let ratio_pct = metrics.symexpr_ratio() * 100.0;
        out.push_str(&format!(
            "    Symbolic expr: {}/{} constraints ({:.0}%)\n",
            metrics.symexpr_count, constraint_total, ratio_pct
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::SymConstraint;
    use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

    #[test]
    fn all_branches_found_by_z3_gives_100_percent_z3() {
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Z3),
            (2, DiscoveryMethod::Z3),
        ];
        let constraints = vec![];
        let metrics = CoverageMetrics::from_exploration(3, &discoveries, &constraints);

        assert_eq!(metrics.z3_solved, 3);
        assert_eq!(metrics.random_found, 0);
        assert_eq!(metrics.user_provided, 0);
        assert_eq!(metrics.uncovered, 0);

        let pct = metrics.percentages();
        assert!((pct.z3_pct - 100.0).abs() < f64::EPSILON);
        assert!(pct.random_pct.abs() < f64::EPSILON);
        assert!(pct.uncovered_pct.abs() < f64::EPSILON);
    }

    #[test]
    fn mixed_discovery_methods_give_correct_percentages() {
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Random),
            (2, DiscoveryMethod::UserProvided),
            (3, DiscoveryMethod::Random),
        ];
        let constraints = vec![];
        let metrics = CoverageMetrics::from_exploration(5, &discoveries, &constraints);

        assert_eq!(metrics.z3_solved, 1);
        assert_eq!(metrics.random_found, 2);
        assert_eq!(metrics.user_provided, 1);
        assert_eq!(metrics.uncovered, 1);

        let pct = metrics.percentages();
        assert!((pct.z3_pct - 20.0).abs() < f64::EPSILON);
        assert!((pct.random_pct - 40.0).abs() < f64::EPSILON);
        assert!((pct.user_provided_pct - 20.0).abs() < f64::EPSILON);
        assert!((pct.uncovered_pct - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_branches_gives_zero_metrics_no_division_by_zero() {
        let metrics = CoverageMetrics::from_exploration(0, &[], &[]);

        assert_eq!(metrics.total_branches, 0);
        assert_eq!(metrics.uncovered, 0);

        let pct = metrics.percentages();
        assert!(pct.z3_pct.abs() < f64::EPSILON);
        assert!(pct.random_pct.abs() < f64::EPSILON);
        assert!(pct.user_provided_pct.abs() < f64::EPSILON);
        assert!(pct.uncovered_pct.abs() < f64::EPSILON);
    }

    #[test]
    fn symexpr_vs_unknown_ratio_calculation() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };

        let constraints = vec![
            SymConstraint::Expr { expr: expr.clone() },
            SymConstraint::Expr { expr: expr.clone() },
            SymConstraint::Unknown {
                hint: "regex".into(),
            },
            SymConstraint::Expr { expr },
        ];

        let metrics = CoverageMetrics::from_exploration(4, &[], &constraints);

        assert_eq!(metrics.symexpr_count, 3);
        assert_eq!(metrics.unknown_count, 1);
        assert!((metrics.symexpr_ratio() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn symexpr_ratio_with_no_constraints_returns_zero() {
        let metrics = CoverageMetrics::from_exploration(0, &[], &[]);
        assert!(metrics.symexpr_ratio().abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_from_empty_exploration() {
        let metrics = CoverageMetrics::from_exploration(10, &[], &[]);

        assert_eq!(metrics.total_branches, 10);
        assert_eq!(metrics.z3_solved, 0);
        assert_eq!(metrics.random_found, 0);
        assert_eq!(metrics.user_provided, 0);
        assert_eq!(metrics.uncovered, 10);
        assert_eq!(metrics.symexpr_count, 0);
        assert_eq!(metrics.unknown_count, 0);
    }

    #[test]
    fn all_discovery_methods_represented() {
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Random),
            (2, DiscoveryMethod::UserProvided),
        ];
        let expr = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(42))),
        };
        let constraints = vec![
            SymConstraint::Expr { expr },
            SymConstraint::Unknown {
                hint: "dynamic check".into(),
            },
        ];

        let metrics = CoverageMetrics::from_exploration(3, &discoveries, &constraints);

        assert_eq!(metrics.z3_solved, 1);
        assert_eq!(metrics.random_found, 1);
        assert_eq!(metrics.user_provided, 1);
        assert_eq!(metrics.uncovered, 0);
        assert_eq!(metrics.symexpr_count, 1);
        assert_eq!(metrics.unknown_count, 1);
        assert!((metrics.symexpr_ratio() - 0.5).abs() < f64::EPSILON);

        let pct = metrics.percentages();
        let sum = pct.z3_pct + pct.random_pct + pct.user_provided_pct + pct.uncovered_pct;
        assert!((sum - 100.0).abs() < 0.01);
    }

    #[test]
    fn format_coverage_metrics_with_no_branches() {
        let metrics = CoverageMetrics::from_exploration(0, &[], &[]);
        let output = format_coverage_metrics(&metrics);
        assert!(output.contains("0 total"));
        assert!(output.contains("no branches"));
    }

    #[test]
    fn format_coverage_metrics_shows_all_sections() {
        let expr = SymExpr::Const(ConstValue::Bool(true));
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Random),
            (2, DiscoveryMethod::UserProvided),
        ];
        let constraints = vec![
            SymConstraint::Expr { expr },
            SymConstraint::Unknown {
                hint: "opaque".into(),
            },
        ];
        let metrics = CoverageMetrics::from_exploration(5, &discoveries, &constraints);
        let output = format_coverage_metrics(&metrics);

        assert!(output.contains("5 total"));
        assert!(output.contains("Z3 solved"));
        assert!(output.contains("Random/bound"));
        assert!(output.contains("User-provided"));
        assert!(output.contains("Uncovered"));
        assert!(output.contains("Symbolic expr"));
        assert!(output.contains("1/2"));
    }

    #[test]
    fn format_coverage_metrics_omits_zero_categories() {
        let discoveries = vec![
            (0, DiscoveryMethod::Random),
            (1, DiscoveryMethod::Random),
        ];
        let metrics = CoverageMetrics::from_exploration(2, &discoveries, &[]);
        let output = format_coverage_metrics(&metrics);

        assert!(output.contains("Random/bound"));
        assert!(!output.contains("Z3 solved"));
        assert!(!output.contains("User-provided"));
        assert!(!output.contains("Uncovered"));
    }

    #[test]
    fn discovery_method_serialization_round_trips() {
        let methods = [
            DiscoveryMethod::Z3,
            DiscoveryMethod::Random,
            DiscoveryMethod::UserProvided,
        ];
        for method in methods {
            let json = serde_json::to_string(&method).expect("serialize");
            let deserialized: DiscoveryMethod =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(method, deserialized);
        }
    }

    #[test]
    fn coverage_metrics_serialization_round_trips() {
        let metrics = CoverageMetrics {
            total_branches: 10,
            z3_solved: 3,
            random_found: 4,
            user_provided: 1,
            uncovered: 2,
            symexpr_count: 7,
            unknown_count: 3,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let deserialized: CoverageMetrics =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(metrics, deserialized);
    }
}
