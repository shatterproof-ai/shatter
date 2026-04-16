//! Coverage metrics for concolic exploration results.
//!
//! After exploration, reports what percentage of discovered branches were found
//! by each method (Z3 solving, random/boundary generation, user-provided inputs)
//! and what fraction of branch conditions the frontend expressed as symbolic
//! expressions vs opaque unknowns.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::execution_record::SymConstraint;
use crate::explorer::ObservationOutput;
use crate::orchestrator::ExploreResult;
use crate::protocol::{BranchInfo, BranchType, ExecuteResult, FunctionAnalysis, MockConfig};

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
    /// Found by parameter drilling on a stalled frontier.
    Drilled,
    /// Found by boundary search between true/false witnesses.
    BoundarySearch,
    /// Found by MC/DC-targeted Z3 query (condition-independence constraint).
    McdcTarget,
    /// Found by coverage-guided fuzzing on an opaque-constraint frontier.
    Fuzzed,
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
    /// Percentage of branches found by coverage-guided fuzzing.
    pub fuzz_pct: f64,
    /// Percentage of branches still uncovered.
    pub uncovered_pct: f64,
}

/// MC/DC coverage metrics for a function.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McdcMetrics {
    /// Total number of compound decisions tracked for MC/DC.
    pub total_decisions: usize,
    /// Total number of individual conditions across all tracked decisions.
    pub total_conditions: usize,
    /// Conditions that have a verified independence pair.
    pub independent_conditions: usize,
    /// Conditions that were always masked and could not be verified.
    pub opaque_conditions: usize,
    /// Conditions that appear in the decision list but were never observed
    /// (either always short-circuited away or never reached).
    pub always_masked: usize,
    /// Percentage of conditions with independence pairs: independent / (total - opaque) * 100.
    /// 0.0 when there are no verifiable conditions.
    pub mcdc_percentage: f64,
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
    /// Branches discovered by coverage-guided fuzzing.
    pub fuzz_found: usize,
    /// Branches that remain uncovered.
    pub uncovered: usize,
    /// Number of branch constraints expressed as SymExpr (solvable by Z3).
    pub symexpr_count: usize,
    /// Number of branch constraints that were Unknown (opaque to solver).
    pub unknown_count: usize,
    /// MC/DC coverage metrics. Present when MC/DC mode is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcdc_metrics: Option<McdcMetrics>,
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
                fuzz_pct: 0.0,
                uncovered_pct: 0.0,
            };
        }

        let total = self.total_branches as f64;
        MethodPercentages {
            z3_pct: self.z3_solved as f64 / total * 100.0,
            random_pct: self.random_found as f64 / total * 100.0,
            user_provided_pct: self.user_provided as f64 / total * 100.0,
            fuzz_pct: self.fuzz_found as f64 / total * 100.0,
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
        let mut fuzz_found = 0usize;

        for (_, method) in discoveries {
            match method {
                DiscoveryMethod::Z3 | DiscoveryMethod::McdcTarget => z3_solved += 1,
                DiscoveryMethod::Random
                | DiscoveryMethod::Drilled
                | DiscoveryMethod::BoundarySearch => random_found += 1,
                DiscoveryMethod::UserProvided => user_provided += 1,
                DiscoveryMethod::Fuzzed => fuzz_found += 1,
            }
        }

        let covered = z3_solved + random_found + user_provided + fuzz_found;
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
            fuzz_found,
            uncovered,
            symexpr_count,
            unknown_count,
            mcdc_metrics: None,
        }
    }

    /// Additively merge `other` into `self`.
    ///
    /// Each counter is summed. The caller must ensure no double-counting
    /// (e.g., each function appears in exactly one batch).
    /// MC/DC metrics are merged additively; if only one side has MC/DC data
    /// the result contains an `McdcMetrics` with just that side's counts.
    pub fn merge(&mut self, other: &CoverageMetrics) {
        self.total_branches += other.total_branches;
        self.z3_solved += other.z3_solved;
        self.random_found += other.random_found;
        self.user_provided += other.user_provided;
        self.fuzz_found += other.fuzz_found;
        self.uncovered += other.uncovered;
        self.symexpr_count += other.symexpr_count;
        self.unknown_count += other.unknown_count;
        match (&mut self.mcdc_metrics, &other.mcdc_metrics) {
            (Some(a), Some(b)) => {
                a.total_decisions += b.total_decisions;
                a.total_conditions += b.total_conditions;
                a.independent_conditions += b.independent_conditions;
                a.opaque_conditions += b.opaque_conditions;
                a.always_masked += b.always_masked;
                let verifiable = a.total_conditions.saturating_sub(a.opaque_conditions);
                a.mcdc_percentage = if verifiable == 0 {
                    0.0
                } else {
                    a.independent_conditions as f64 / verifiable as f64 * 100.0
                };
            }
            (None, Some(b)) => {
                self.mcdc_metrics = Some(b.clone());
            }
            _ => {}
        }
    }
}

impl McdcMetrics {
    /// Build `McdcMetrics` from an MC/DC summary triple `(total, independent, opaque)`.
    ///
    /// `total` is the total number of conditions tracked.
    /// `independent` is the number with a verified independence pair.
    /// `opaque` is the number that were always masked or otherwise unverifiable.
    ///
    /// The percentage is computed over analyzable conditions (`total - opaque`).
    pub fn from_mcdc_summary(total: usize, independent: usize, opaque: usize) -> Self {
        let analyzable = total.saturating_sub(opaque);
        let mcdc_percentage = if analyzable > 0 {
            (independent as f64 / analyzable as f64) * 100.0
        } else {
            0.0
        };
        Self {
            total_decisions: 0, // not tracked at summary level yet
            total_conditions: total,
            independent_conditions: independent,
            opaque_conditions: opaque,
            always_masked: 0, // not tracked at summary level yet
            mcdc_percentage,
        }
    }
}

/// Format coverage metrics as a human-readable summary block.
pub fn format_coverage_metrics(
    metrics: &CoverageMetrics,
    style: &crate::report_style::ReportStyle,
) -> String {
    let mut out = String::new();
    let pct = metrics.percentages();

    let covered = metrics.total_branches.saturating_sub(metrics.uncovered);
    let branch_pct = if metrics.total_branches > 0 {
        covered as f64 / metrics.total_branches as f64 * 100.0
    } else {
        0.0
    };

    if metrics.total_branches == 0 {
        out.push_str(&format!(
            "  {dim}Branches: 0 (no branches){reset}\n",
            dim = style.dim,
            reset = style.reset,
        ));
        return out;
    }

    out.push_str(&format!(
        "  Branches: {covered}/{total} ({pct}) {indicator}\n",
        total = metrics.total_branches,
        pct = style.color_coverage_pct(branch_pct),
        indicator = style.coverage_indicator(branch_pct),
    ));

    // Detailed breakdown in dim text
    let mut details = Vec::new();
    if metrics.z3_solved > 0 {
        details.push(format!("Z3: {} ({:.0}%)", metrics.z3_solved, pct.z3_pct));
    }
    if metrics.random_found > 0 {
        details.push(format!(
            "random: {} ({:.0}%)",
            metrics.random_found, pct.random_pct
        ));
    }
    if metrics.user_provided > 0 {
        details.push(format!(
            "user: {} ({:.0}%)",
            metrics.user_provided, pct.user_provided_pct
        ));
    }
    if metrics.fuzz_found > 0 {
        details.push(format!(
            "fuzz: {} ({:.0}%)",
            metrics.fuzz_found, pct.fuzz_pct
        ));
    }
    if metrics.uncovered > 0 {
        details.push(format!(
            "{red}uncovered: {} ({:.0}%){reset}",
            metrics.uncovered,
            pct.uncovered_pct,
            red = style.red,
            reset = style.reset,
        ));
    }
    if !details.is_empty() {
        out.push_str(&format!(
            "  {dim}[{details}]{reset}\n",
            details = details.join(", "),
            dim = style.dim,
            reset = style.reset,
        ));
    }

    let constraint_total = metrics.symexpr_count + metrics.unknown_count;
    if constraint_total > 0 {
        let ratio_pct = metrics.symexpr_ratio() * 100.0;
        out.push_str(&format!(
            "  {dim}Symbolic: {}/{} constraints ({:.0}%){reset}\n",
            metrics.symexpr_count,
            constraint_total,
            ratio_pct,
            dim = style.dim,
            reset = style.reset,
        ));
    }

    if let Some(ref mcdc) = metrics.mcdc_metrics {
        let analyzable = mcdc.total_conditions.saturating_sub(mcdc.opaque_conditions);
        out.push_str("  MC/DC Coverage\n");
        if mcdc.opaque_conditions > 0 {
            out.push_str(&format!(
                "  \u{251c}\u{2500} Conditions: {total} total, {opaque} opaque\n",
                total = mcdc.total_conditions,
                opaque = mcdc.opaque_conditions,
            ));
        } else {
            out.push_str(&format!(
                "  \u{251c}\u{2500} Conditions: {total} total\n",
                total = mcdc.total_conditions,
            ));
        }
        out.push_str(&format!(
            "  \u{251c}\u{2500} Independent: {independent}/{analyzable} analyzable ({pct})\n",
            independent = mcdc.independent_conditions,
            pct = style.color_coverage_pct(mcdc.mcdc_percentage),
        ));
        out.push_str(&format!(
            "  \u{2514}\u{2500} {bar} {pct:.1}%\n",
            bar = style.coverage_bar(mcdc.mcdc_percentage),
            pct = mcdc.mcdc_percentage,
        ));
    }

    out
}

/// Why a branch was selected as a GA target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetReason {
    /// Branch was never discovered by any method.
    Uncovered,
    /// Branch was observed but its constraint is Unknown (opaque to solver).
    OpaqueConstraint,
}

/// A branch that the Genetic Algorithm should target for coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetBranch {
    /// Unique branch identifier within the function.
    pub branch_id: u32,
    /// Source line number of the branch.
    pub line: u32,
    /// Why this branch is targeted.
    pub reason: TargetReason,
    /// Hint about the constraint to guide mutation strategy.
    pub constraint_hint: Option<String>,
}

/// A branch target identified at discovery time, before exploration begins.
///
/// Unlike [`TargetBranch`] (computed post-exploration from runtime discoveries),
/// this represents the initial set of targets derived from static analysis.
/// Every branch in a function is a known target until exploration proves it
/// covered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownTarget {
    /// Unique branch identifier within the function.
    pub branch_id: u32,
    /// Source line number of the branch.
    pub line: u32,
    /// Estimated nesting depth: count of preceding flow-control branches.
    /// Higher depth means the branch is harder to reach (more constraints
    /// must be satisfied).
    pub nesting_depth: u32,
}

/// Compute known uncovered targets from static analysis.
///
/// Before any exploration, every branch in a function is uncovered. This
/// function converts the static branch list into [`KnownTarget`]s with
/// estimated nesting depths. Returns an empty vec for functions with no
/// branches — such functions have no concrete targets to explore.
pub fn discover_known_targets(analysis: &FunctionAnalysis) -> Vec<KnownTarget> {
    analysis
        .branches
        .iter()
        .map(|branch| KnownTarget {
            branch_id: branch.id,
            line: branch.line,
            nesting_depth: estimate_nesting_depth(branch, &analysis.branches),
        })
        .collect()
}

/// Upper-bound estimate of a branch's nesting depth within a function.
///
/// Counts flow-control branches (`If`, `While`, `For`, `Select`) that appear
/// on earlier source lines. Expression-level branches (`ElseIf`, `Ternary`,
/// `LogicalAnd`, `LogicalOr`, `Switch`) don't create new nesting levels.
///
/// This is an upper bound: it counts all preceding flow-control, not just
/// enclosing flow-control. For scheduler prioritisation this is sufficient —
/// it distinguishes top-level branches (depth 0) from deeply nested ones.
fn estimate_nesting_depth(branch: &BranchInfo, all_branches: &[BranchInfo]) -> u32 {
    all_branches
        .iter()
        .filter(|b| b.line < branch.line && is_nesting_branch(&b.branch_type))
        .count() as u32
}

/// Returns `true` for branch types that create a new nesting level in
/// control flow. Expression-level branches do not nest.
fn is_nesting_branch(bt: &BranchType) -> bool {
    matches!(
        bt,
        BranchType::If | BranchType::While | BranchType::For | BranchType::Select
    )
}

/// Extract unsolved branch targets from a random exploration result.
///
/// Compares the static branch list from `analysis` against branches discovered
/// during exploration to identify targets the GA should focus on.
pub fn extract_targets(
    analysis: &FunctionAnalysis,
    result: &ObservationOutput,
) -> Vec<TargetBranch> {
    extract_targets_inner(&analysis.branches, &result.discoveries, &result.raw_results)
}

/// Extract unsolved branch targets from a concolic exploration result.
///
/// Same logic as [`extract_targets`] but accepts the orchestrator's result type.
pub fn extract_targets_concolic(
    analysis: &FunctionAnalysis,
    result: &ExploreResult,
) -> Vec<TargetBranch> {
    extract_targets_inner(&analysis.branches, &result.discoveries, &result.raw_results)
}

/// Core target extraction logic operating on slices.
///
/// A branch is targeted if:
/// - It was never discovered (not in `discoveries`) → `TargetReason::Uncovered`
/// - It was discovered but its runtime constraint is `Unknown` → `TargetReason::OpaqueConstraint`
fn extract_targets_inner(
    branches: &[BranchInfo],
    discoveries: &[(u32, DiscoveryMethod)],
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
) -> Vec<TargetBranch> {
    let discovered: HashSet<u32> = discoveries.iter().map(|(id, _)| *id).collect();

    // Collect opaque constraint hints from runtime branch decisions.
    let mut opaque_hints: HashMap<u32, String> = HashMap::new();
    for (_, _mocks, exec) in raw_results {
        for decision in &exec.branch_path {
            if let SymConstraint::Unknown { hint } = &decision.constraint {
                opaque_hints
                    .entry(decision.branch_id)
                    .or_insert_with(|| hint.clone());
            }
        }
    }

    let mut targets: Vec<TargetBranch> = Vec::new();
    for branch in branches {
        if !discovered.contains(&branch.id) {
            targets.push(TargetBranch {
                branch_id: branch.id,
                line: branch.line,
                reason: TargetReason::Uncovered,
                constraint_hint: if branch.condition_text.is_empty() {
                    None
                } else {
                    Some(branch.condition_text.clone())
                },
            });
        } else if let Some(hint) = opaque_hints.get(&branch.id) {
            targets.push(TargetBranch {
                branch_id: branch.id,
                line: branch.line,
                reason: TargetReason::OpaqueConstraint,
                constraint_hint: if hint.is_empty() {
                    None
                } else {
                    Some(hint.clone())
                },
            });
        }
    }

    targets.sort_by_key(|t| t.branch_id);
    targets
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
        let style = crate::report_style::ReportStyle::default();
        let metrics = CoverageMetrics::from_exploration(0, &[], &[]);
        let output = format_coverage_metrics(&metrics, &style);
        assert!(output.contains("0"));
        assert!(output.contains("no branches"));
    }

    #[test]
    fn format_coverage_metrics_shows_all_sections() {
        let style = crate::report_style::ReportStyle::default();
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
        let output = format_coverage_metrics(&metrics, &style);

        assert!(output.contains("Branches:"));
        assert!(output.contains("Z3:"));
        assert!(output.contains("random:"));
        assert!(output.contains("uncovered:"));
        assert!(output.contains("Symbolic:"));
        assert!(output.contains("1/2"));
    }

    #[test]
    fn format_coverage_metrics_omits_zero_categories() {
        let style = crate::report_style::ReportStyle::default();
        let discoveries = vec![(0, DiscoveryMethod::Random), (1, DiscoveryMethod::Random)];
        let metrics = CoverageMetrics::from_exploration(2, &discoveries, &[]);
        let output = format_coverage_metrics(&metrics, &style);

        assert!(output.contains("random:"));
        assert!(!output.contains("Z3:"));
        assert!(!output.contains("user:"));
        assert!(!output.contains("uncovered:"));
    }

    #[test]
    fn discovery_method_serialization_round_trips() {
        let methods = [
            DiscoveryMethod::Z3,
            DiscoveryMethod::Random,
            DiscoveryMethod::UserProvided,
            DiscoveryMethod::Drilled,
            DiscoveryMethod::BoundarySearch,
            DiscoveryMethod::McdcTarget,
            DiscoveryMethod::Fuzzed,
        ];
        for method in methods {
            let json = serde_json::to_string(&method).expect("serialize");
            let deserialized: DiscoveryMethod = serde_json::from_str(&json).expect("deserialize");
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
            fuzz_found: 0,
            uncovered: 2,
            symexpr_count: 7,
            unknown_count: 3,
            mcdc_metrics: None,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let deserialized: CoverageMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(metrics, deserialized);
    }

    #[test]
    fn mcdc_metrics_serialization_round_trips() {
        let metrics = McdcMetrics {
            total_decisions: 3,
            total_conditions: 7,
            independent_conditions: 5,
            opaque_conditions: 1,
            always_masked: 0,
            mcdc_percentage: 83.33,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let deserialized: McdcMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(metrics, deserialized);
    }

    #[test]
    fn coverage_metrics_with_mcdc_round_trips() {
        let metrics = CoverageMetrics {
            total_branches: 5,
            z3_solved: 2,
            random_found: 1,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 2,
            symexpr_count: 3,
            unknown_count: 2,
            mcdc_metrics: Some(McdcMetrics {
                total_decisions: 2,
                total_conditions: 4,
                independent_conditions: 3,
                opaque_conditions: 0,
                always_masked: 0,
                mcdc_percentage: 75.0,
            }),
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let deserialized: CoverageMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(metrics, deserialized);
    }

    #[test]
    fn coverage_metrics_mcdc_none_omitted_in_json() {
        let metrics = CoverageMetrics {
            total_branches: 2,
            z3_solved: 1,
            random_found: 0,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 1,
            unknown_count: 1,
            mcdc_metrics: None,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        assert!(
            !json.contains("mcdc_metrics"),
            "None mcdc_metrics must not appear in JSON"
        );
    }

    // --- extract_targets tests ---

    use crate::execution_record::BranchDecision;
    use crate::protocol::{
        BranchInfo, BranchType, ExecuteResult, FunctionAnalysis, PerformanceMetrics,
    };
    use crate::types::{ParamInfo, TypeInfo};

    fn make_branch(id: u32, line: u32, condition: &str) -> BranchInfo {
        BranchInfo {
            id,
            line,
            condition_text: condition.to_string(),
            condition: None,
            branch_type: BranchType::If,
        }
    }

    fn make_analysis(branches: Vec<BranchInfo>) -> FunctionAnalysis {
        FunctionAnalysis {
            name: "test_fn".to_string(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".to_string(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches,
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 20,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }
    }

    fn make_exec_with_branches(decisions: Vec<BranchDecision>) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: decisions,
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 0.0,
                cpu_time_us: 0,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
        }
    }

    #[test]
    fn extract_targets_all_uncovered() {
        let analysis = make_analysis(vec![
            make_branch(0, 5, "x > 0"),
            make_branch(1, 10, "x < 100"),
            make_branch(2, 15, "x == 42"),
        ]);
        let discoveries = vec![];
        let raw_results = vec![];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);

        assert_eq!(targets.len(), 3);
        for target in &targets {
            assert_eq!(target.reason, TargetReason::Uncovered);
        }
        assert_eq!(targets[0].branch_id, 0);
        assert_eq!(targets[0].constraint_hint.as_deref(), Some("x > 0"));
        assert_eq!(targets[1].branch_id, 1);
        assert_eq!(targets[2].branch_id, 2);
    }

    #[test]
    fn extract_targets_all_discovered_solvable() {
        let analysis = make_analysis(vec![
            make_branch(0, 5, "x > 0"),
            make_branch(1, 10, "x < 100"),
        ]);
        let discoveries = vec![(0, DiscoveryMethod::Z3), (1, DiscoveryMethod::Random)];
        // No Unknown constraints in executions
        let expr = SymExpr::Const(ConstValue::Bool(true));
        let exec = make_exec_with_branches(vec![
            BranchDecision {
                branch_id: 0,
                line: 5,
                taken: true,
                constraint: SymConstraint::Expr { expr: expr.clone() },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 10,
                taken: true,
                constraint: SymConstraint::Expr { expr },
                conditions: None,
            },
        ]);
        let raw_results = vec![(vec![serde_json::json!(5)], vec![], exec)];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);
        assert!(targets.is_empty());
    }

    #[test]
    fn extract_targets_mixed_uncovered_and_opaque() {
        let analysis = make_analysis(vec![
            make_branch(0, 5, "x > 0"),
            make_branch(1, 10, "x < 100"),
            make_branch(2, 15, "isValid(x)"),
            make_branch(3, 20, "x == 42"),
            make_branch(4, 25, "x != 0"),
        ]);
        // Branches 0, 1, 2 discovered; 3 and 4 not discovered
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Random),
            (2, DiscoveryMethod::Random),
        ];
        // Branch 2 has Unknown constraint at runtime
        let exec = make_exec_with_branches(vec![BranchDecision {
            branch_id: 2,
            line: 15,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "dynamic validation call".to_string(),
            },
            conditions: None,
        }]);
        let raw_results = vec![(vec![serde_json::json!(10)], vec![], exec)];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);

        assert_eq!(targets.len(), 3);
        // Branch 2: OpaqueConstraint
        assert_eq!(targets[0].branch_id, 2);
        assert_eq!(targets[0].reason, TargetReason::OpaqueConstraint);
        assert_eq!(
            targets[0].constraint_hint.as_deref(),
            Some("dynamic validation call")
        );
        // Branch 3: Uncovered
        assert_eq!(targets[1].branch_id, 3);
        assert_eq!(targets[1].reason, TargetReason::Uncovered);
        assert_eq!(targets[1].constraint_hint.as_deref(), Some("x == 42"));
        // Branch 4: Uncovered
        assert_eq!(targets[2].branch_id, 4);
        assert_eq!(targets[2].reason, TargetReason::Uncovered);
    }

    #[test]
    fn extract_targets_empty_branches() {
        let analysis = make_analysis(vec![]);
        let discoveries = vec![(0, DiscoveryMethod::Z3)];
        let raw_results = vec![];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);
        assert!(targets.is_empty());
    }

    #[test]
    fn extract_targets_opaque_hint_from_runtime_not_static() {
        let analysis = make_analysis(vec![make_branch(0, 5, "static condition text")]);
        let discoveries = vec![(0, DiscoveryMethod::Random)];
        let exec = make_exec_with_branches(vec![BranchDecision {
            branch_id: 0,
            line: 5,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "runtime opaque hint".to_string(),
            },
            conditions: None,
        }]);
        let raw_results = vec![(vec![serde_json::json!(1)], vec![], exec)];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].reason, TargetReason::OpaqueConstraint);
        // Hint comes from runtime Unknown, not from BranchInfo.condition_text
        assert_eq!(
            targets[0].constraint_hint.as_deref(),
            Some("runtime opaque hint")
        );
    }

    #[test]
    fn extract_targets_uncovered_hint_from_condition_text() {
        let analysis = make_analysis(vec![make_branch(0, 5, "x > threshold")]);
        let discoveries = vec![];
        let raw_results = vec![];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].reason, TargetReason::Uncovered);
        assert_eq!(targets[0].constraint_hint.as_deref(), Some("x > threshold"));
    }

    #[test]
    fn extract_targets_sorted_by_branch_id() {
        let analysis = make_analysis(vec![
            make_branch(5, 50, "e"),
            make_branch(1, 10, "a"),
            make_branch(3, 30, "c"),
        ]);
        let discoveries = vec![];
        let raw_results = vec![];

        let targets = extract_targets_inner(&analysis.branches, &discoveries, &raw_results);

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].branch_id, 1);
        assert_eq!(targets[1].branch_id, 3);
        assert_eq!(targets[2].branch_id, 5);
    }

    #[test]
    fn target_branch_serialization_round_trips() {
        let target = TargetBranch {
            branch_id: 7,
            line: 42,
            reason: TargetReason::OpaqueConstraint,
            constraint_hint: Some("dynamic check".to_string()),
        };
        let json = serde_json::to_string(&target).expect("serialize");
        let deserialized: TargetBranch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(target, deserialized);

        let target_uncovered = TargetBranch {
            branch_id: 3,
            line: 10,
            reason: TargetReason::Uncovered,
            constraint_hint: None,
        };
        let json = serde_json::to_string(&target_uncovered).expect("serialize");
        let deserialized: TargetBranch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(target_uncovered, deserialized);
    }

    // --- merge tests ---

    #[test]
    fn merge_adds_all_counters() {
        let mut a = CoverageMetrics {
            total_branches: 10,
            z3_solved: 3,
            random_found: 2,
            user_provided: 1,
            fuzz_found: 0,
            uncovered: 4,
            symexpr_count: 6,
            unknown_count: 4,
            mcdc_metrics: None,
        };
        let b = CoverageMetrics {
            total_branches: 8,
            z3_solved: 2,
            random_found: 3,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 3,
            symexpr_count: 5,
            unknown_count: 3,
            mcdc_metrics: None,
        };
        a.merge(&b);
        assert_eq!(a.total_branches, 18);
        assert_eq!(a.z3_solved, 5);
        assert_eq!(a.random_found, 5);
        assert_eq!(a.user_provided, 1);
        assert_eq!(a.uncovered, 7);
        assert_eq!(a.symexpr_count, 11);
        assert_eq!(a.unknown_count, 7);
    }

    #[test]
    fn merge_with_default_is_identity() {
        let original = CoverageMetrics {
            total_branches: 5,
            z3_solved: 2,
            random_found: 1,
            user_provided: 1,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 3,
            unknown_count: 2,
            mcdc_metrics: None,
        };
        let mut merged = CoverageMetrics::default();
        merged.merge(&original);
        assert_eq!(merged, original);
    }

    #[test]
    fn merge_is_additive() {
        let b = CoverageMetrics {
            total_branches: 4,
            z3_solved: 1,
            random_found: 1,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 2,
            symexpr_count: 2,
            unknown_count: 2,
            mcdc_metrics: None,
        };
        let c = CoverageMetrics {
            total_branches: 6,
            z3_solved: 2,
            random_found: 2,
            user_provided: 1,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 4,
            unknown_count: 2,
            mcdc_metrics: None,
        };
        // Merge B then C
        let mut sequential = CoverageMetrics::default();
        sequential.merge(&b);
        sequential.merge(&c);
        // Merge (B+C) at once
        let mut combined = b.clone();
        combined.merge(&c);
        assert_eq!(sequential, combined);
    }

    // --- McdcMetrics::from_mcdc_summary tests ---

    #[test]
    fn from_mcdc_summary_full_coverage() {
        let m = McdcMetrics::from_mcdc_summary(9, 9, 0);
        assert_eq!(m.total_conditions, 9);
        assert_eq!(m.independent_conditions, 9);
        assert_eq!(m.opaque_conditions, 0);
        assert_eq!(m.total_decisions, 0);
        assert_eq!(m.always_masked, 0);
        assert!((m.mcdc_percentage - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn from_mcdc_summary_zero_coverage() {
        let m = McdcMetrics::from_mcdc_summary(9, 0, 0);
        assert_eq!(m.total_conditions, 9);
        assert_eq!(m.independent_conditions, 0);
        assert!((m.mcdc_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn from_mcdc_summary_with_opaque() {
        // 11 total, 2 opaque → 9 analyzable; 7 independent → 77.8%
        let m = McdcMetrics::from_mcdc_summary(11, 7, 2);
        assert_eq!(m.total_conditions, 11);
        assert_eq!(m.independent_conditions, 7);
        assert_eq!(m.opaque_conditions, 2);
        let expected_pct = 7.0_f64 / 9.0 * 100.0;
        assert!((m.mcdc_percentage - expected_pct).abs() < 0.001);
    }

    #[test]
    fn from_mcdc_summary_all_opaque_gives_zero_pct() {
        // All conditions are opaque — no analyzable conditions, percentage should be 0.0
        let m = McdcMetrics::from_mcdc_summary(5, 0, 5);
        assert_eq!(m.total_conditions, 5);
        assert_eq!(m.opaque_conditions, 5);
        assert!((m.mcdc_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn from_mcdc_summary_zero_total_gives_zero_pct() {
        let m = McdcMetrics::from_mcdc_summary(0, 0, 0);
        assert_eq!(m.total_conditions, 0);
        assert!((m.mcdc_percentage - 0.0).abs() < f64::EPSILON);
    }

    // --- format_coverage_metrics MC/DC section tests ---

    #[test]
    fn format_coverage_metrics_shows_mcdc_section_when_present() {
        let style = crate::report_style::ReportStyle::default();
        let metrics = CoverageMetrics {
            total_branches: 4,
            z3_solved: 3,
            random_found: 0,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 2,
            unknown_count: 1,
            mcdc_metrics: Some(McdcMetrics::from_mcdc_summary(11, 7, 2)),
        };
        let output = format_coverage_metrics(&metrics, &style);
        assert!(
            output.contains("MC/DC Coverage"),
            "expected MC/DC Coverage section"
        );
        assert!(output.contains("11 total"), "expected total conditions");
        assert!(output.contains("2 opaque"), "expected opaque conditions");
        assert!(
            output.contains("7/9 analyzable"),
            "expected independent/analyzable"
        );
    }

    #[test]
    fn format_coverage_metrics_omits_mcdc_section_when_none() {
        let style = crate::report_style::ReportStyle::default();
        let metrics = CoverageMetrics {
            total_branches: 4,
            z3_solved: 3,
            random_found: 0,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 2,
            unknown_count: 1,
            mcdc_metrics: None,
        };
        let output = format_coverage_metrics(&metrics, &style);
        assert!(
            !output.contains("MC/DC Coverage"),
            "MC/DC section must be absent when mcdc_metrics is None"
        );
    }

    #[test]
    fn format_coverage_metrics_mcdc_no_opaque_omits_opaque_text() {
        let style = crate::report_style::ReportStyle::default();
        let metrics = CoverageMetrics {
            total_branches: 2,
            z3_solved: 1,
            random_found: 0,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 1,
            unknown_count: 1,
            mcdc_metrics: Some(McdcMetrics::from_mcdc_summary(5, 4, 0)),
        };
        let output = format_coverage_metrics(&metrics, &style);
        assert!(output.contains("MC/DC Coverage"));
        assert!(
            !output.contains("opaque"),
            "opaque text must not appear when opaque_conditions == 0"
        );
    }

    // --- DiscoveryMethod::McdcTarget counts toward z3_solved ---

    #[test]
    fn mcdc_target_discovery_counts_toward_z3_solved() {
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::McdcTarget),
            (2, DiscoveryMethod::Random),
        ];
        let metrics = CoverageMetrics::from_exploration(3, &discoveries, &[]);
        assert_eq!(
            metrics.z3_solved, 2,
            "Z3 and McdcTarget both count toward z3_solved"
        );
        assert_eq!(metrics.random_found, 1);
        assert_eq!(metrics.uncovered, 0);
    }

    // --- DiscoveryMethod::Fuzzed counts toward fuzz_found ---

    #[test]
    fn fuzzed_discovery_counts_toward_fuzz_found() {
        let discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Fuzzed),
            (2, DiscoveryMethod::Random),
            (3, DiscoveryMethod::Fuzzed),
            (4, DiscoveryMethod::UserProvided),
        ];
        let metrics = CoverageMetrics::from_exploration(5, &discoveries, &[]);
        assert_eq!(
            metrics.fuzz_found, 2,
            "Fuzzed discoveries count toward fuzz_found"
        );
        let pct = metrics.percentages();
        assert!(
            (pct.fuzz_pct - 40.0).abs() < f64::EPSILON,
            "fuzz_pct should be 40.0, got {}",
            pct.fuzz_pct
        );
        // Other counters are correct too
        assert_eq!(metrics.z3_solved, 1);
        assert_eq!(metrics.random_found, 1);
        assert_eq!(metrics.user_provided, 1);
        assert_eq!(metrics.uncovered, 0);
    }

    // --- discover_known_targets tests ---

    fn make_branch_typed(id: u32, line: u32, bt: BranchType) -> BranchInfo {
        BranchInfo {
            id,
            line,
            condition_text: format!("cond_{id}"),
            condition: None,
            branch_type: bt,
        }
    }

    #[test]
    fn discover_known_targets_empty_for_zero_branches() {
        let analysis = make_analysis(vec![]);
        let targets = discover_known_targets(&analysis);
        assert!(targets.is_empty());
    }

    #[test]
    fn discover_known_targets_returns_all_branches() {
        let analysis = make_analysis(vec![
            make_branch(0, 5, "x > 0"),
            make_branch(1, 10, "x < 100"),
            make_branch(2, 15, "x == 42"),
        ]);
        let targets = discover_known_targets(&analysis);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].branch_id, 0);
        assert_eq!(targets[1].branch_id, 1);
        assert_eq!(targets[2].branch_id, 2);
    }

    #[test]
    fn nesting_depth_first_branch_is_zero() {
        let analysis = make_analysis(vec![make_branch(0, 5, "x > 0")]);
        let targets = discover_known_targets(&analysis);
        assert_eq!(targets[0].nesting_depth, 0);
    }

    #[test]
    fn nesting_depth_counts_preceding_flow_control() {
        let analysis = make_analysis(vec![
            make_branch_typed(0, 5, BranchType::If),
            make_branch_typed(1, 10, BranchType::If),
            make_branch_typed(2, 15, BranchType::While),
        ]);
        let targets = discover_known_targets(&analysis);
        // First If at line 5: nothing precedes it
        assert_eq!(targets[0].nesting_depth, 0);
        // Second If at line 10: one If precedes it
        assert_eq!(targets[1].nesting_depth, 1);
        // While at line 15: two flow-control branches precede it
        assert_eq!(targets[2].nesting_depth, 2);
    }

    #[test]
    fn nesting_depth_ignores_expression_level_branches() {
        let analysis = make_analysis(vec![
            make_branch_typed(0, 5, BranchType::If),
            make_branch_typed(1, 8, BranchType::LogicalAnd),
            make_branch_typed(2, 10, BranchType::Ternary),
            make_branch_typed(3, 15, BranchType::For),
        ]);
        let targets = discover_known_targets(&analysis);
        // For at line 15: only If(5) counts; LogicalAnd(8) and Ternary(10) don't
        assert_eq!(targets[3].nesting_depth, 1);
    }

    #[test]
    fn nesting_depth_for_and_select_count() {
        let analysis = make_analysis(vec![
            make_branch_typed(0, 2, BranchType::For),
            make_branch_typed(1, 5, BranchType::Select),
            make_branch_typed(2, 10, BranchType::If),
        ]);
        let targets = discover_known_targets(&analysis);
        // If at line 10: For(2) and Select(5) both count
        assert_eq!(targets[2].nesting_depth, 2);
    }
}
