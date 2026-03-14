//! Branch-distance fitness function for genetic search.
//!
//! Scores an [`ExecuteResult`] on six axes — branch coverage, proximity
//! to uncovered target branches, unknown-branch exploration bonus, path
//! novelty, execution depth, and branch rarity — and combines them into
//! a single 0.0–1.0 fitness value.

use std::collections::HashSet;

use crate::execution_record::{BranchDecision, SymConstraint};
use crate::orchestrator::hash_branch_path;
use crate::protocol::ExecuteResult;
use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

/// Half-saturation constant for depth normalization.
/// At k=10, a path of length 10 scores 0.5; length 30 scores 0.75.
const DEPTH_HALF_SATURATION: f64 = 10.0;

/// Weights for combining the six fitness components.
///
/// Defaults: coverage 0.20, proximity 0.30, unknown_bonus 0.10, novelty 0.10, depth 0.15, rarity 0.15.
#[derive(Debug, Clone)]
pub struct FitnessWeights {
    pub coverage: f64,
    pub proximity: f64,
    pub unknown_bonus: f64,
    pub novelty: f64,
    pub depth: f64,
    pub rarity: f64,
}

impl Default for FitnessWeights {
    fn default() -> Self {
        Self {
            coverage: 0.20,
            proximity: 0.30,
            unknown_bonus: 0.10,
            novelty: 0.10,
            depth: 0.15,
            rarity: 0.15,
        }
    }
}

/// Breakdown of the six fitness components before weighting.
#[derive(Debug, Clone)]
pub struct FitnessBreakdown {
    /// Fraction of unique branches hit relative to target count (0.0–1.0).
    pub coverage: f64,
    /// Average closeness to flipping target branches (0.0–1.0).
    pub proximity: f64,
    /// Bonus for reaching `SymConstraint::Unknown` branches (0.0–1.0).
    pub unknown_bonus: f64,
    /// 1.0 if this is a novel path, 0.0 if previously seen.
    pub novelty: f64,
    /// How deep into nested control flow the execution penetrated ([0.0, 1.0)).
    pub depth: f64,
    /// Average rarity of branches in the execution path (0.0–1.0).
    /// 0.0 when no profile is available.
    pub rarity: f64,
    /// Weighted combination of the six components (0.0–1.0).
    pub total: f64,
}

/// Mutable state tracking previously seen path hashes for novelty scoring.
#[derive(Debug, Clone, Default)]
pub struct FitnessContext {
    seen_paths: HashSet<u64>,
}

impl FitnessContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `FitnessContext` pre-seeded with already-seen path hashes.
    ///
    /// This allows sharing novelty state with the orchestrator's `covered_paths`,
    /// so paths already discovered before fitness scoring starts are not scored
    /// as novel.
    pub fn from_seen_paths(seen_paths: HashSet<u64>) -> Self {
        Self { seen_paths }
    }

    /// Record a path hash as seen, returning true if it was new.
    pub fn mark_seen(&mut self, path_hash: u64) -> bool {
        self.seen_paths.insert(path_hash)
    }
}

/// Score an execution result against a set of target (uncovered) branch IDs.
///
/// When a [`BranchProfile`] is provided, the rarity component rewards
/// executions that reach rarely-observed branches. Without a profile,
/// the rarity component is 0.0 and its weight is excluded from normalization.
pub fn score(
    result: &ExecuteResult,
    target_branches: &HashSet<u32>,
    context: &mut FitnessContext,
    weights: &FitnessWeights,
    profile: Option<&crate::branch_profile::BranchProfile>,
) -> FitnessBreakdown {
    let coverage = coverage_score(&result.branch_path, target_branches.len());
    let proximity = proximity_score(&result.branch_path, target_branches);
    let unknown = unknown_bonus_score(&result.branch_path);
    let novelty = novelty_score(&result.branch_path, context);
    let depth = depth_score(&result.branch_path);
    let rarity = rarity_score(&result.branch_path, profile);

    let rarity_weight = if profile.is_some() { weights.rarity } else { 0.0 };
    let weight_sum =
        weights.coverage + weights.proximity + weights.unknown_bonus + weights.novelty + weights.depth + rarity_weight;
    let total = if weight_sum > 0.0 {
        (weights.coverage * coverage
            + weights.proximity * proximity
            + weights.unknown_bonus * unknown
            + weights.novelty * novelty
            + weights.depth * depth
            + rarity_weight * rarity)
            / weight_sum
    } else {
        0.0
    };

    FitnessBreakdown {
        coverage,
        proximity,
        unknown_bonus: unknown,
        novelty,
        depth,
        rarity,
        total,
    }
}

// ---------------------------------------------------------------------------
// Component scorers
// ---------------------------------------------------------------------------

/// Asymptotic depth score: `len / (len + k)` where k = [`DEPTH_HALF_SATURATION`].
///
/// Returns 0.0 for empty paths, approaches 1.0 for very long paths, never reaches 1.0.
fn depth_score(branch_path: &[BranchDecision]) -> f64 {
    let len = branch_path.len() as f64;
    len / (len + DEPTH_HALF_SATURATION)
}

/// Fraction of unique branches hit relative to `target_count`.
fn coverage_score(branch_path: &[BranchDecision], target_count: usize) -> f64 {
    if target_count == 0 {
        return 1.0;
    }
    let unique: HashSet<u32> = branch_path.iter().map(|bd| bd.branch_id).collect();
    let ratio = unique.len() as f64 / target_count as f64;
    ratio.min(1.0)
}

/// Average closeness to flipping each target branch.
///
/// For each target: if reached in the trace, score = 1 − branch_distance.
/// If never reached, contributes 0.
fn proximity_score(branch_path: &[BranchDecision], targets: &HashSet<u32>) -> f64 {
    if targets.is_empty() {
        return 1.0;
    }
    let mut total = 0.0;
    for &target_id in targets {
        let closest = branch_path
            .iter()
            .filter(|bd| bd.branch_id == target_id)
            .map(|bd| branch_distance(&bd.constraint))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(dist) = closest {
            total += 1.0 - dist;
        }
    }
    total / targets.len() as f64
}

/// Distance for a single branch constraint (0.0 = at target, 1.0 = far away).
fn branch_distance(constraint: &SymConstraint) -> f64 {
    match constraint {
        SymConstraint::Unknown { .. } => 0.5,
        SymConstraint::Expr { expr } => expr_distance(expr),
    }
}

/// Distance derived from a symbolic expression.
fn expr_distance(expr: &SymExpr) -> f64 {
    match expr {
        SymExpr::BinOp { op, left, right } => {
            match (eval_to_f64(left), eval_to_f64(right)) {
                (Some(l), Some(r)) => comparison_distance(*op, l, r),
                _ => 0.5, // cannot evaluate — moderate distance
            }
        }
        SymExpr::UnOp {
            op: crate::sym_expr::UnOpKind::Not,
            operand,
        } => 1.0 - expr_distance(operand),
        _ => 0.5,
    }
}

/// Standard branch-distance formula for comparison operators.
///
/// Uses the Korel (1990) approach: raw numeric distance normalized via
/// `d / (d + 1)` so the result is always in [0, 1).
fn comparison_distance(op: BinOpKind, left: f64, right: f64) -> f64 {
    let raw = match op {
        BinOpKind::Eq => (left - right).abs(),
        BinOpKind::Ne => {
            if (left - right).abs() < f64::EPSILON {
                1.0
            } else {
                0.0
            }
        }
        BinOpKind::Lt => {
            if left < right {
                0.0
            } else {
                left - right + 1.0
            }
        }
        BinOpKind::Le => {
            if left <= right {
                0.0
            } else {
                left - right
            }
        }
        BinOpKind::Gt => {
            if left > right {
                0.0
            } else {
                right - left + 1.0
            }
        }
        BinOpKind::Ge => {
            if left >= right {
                0.0
            } else {
                right - left
            }
        }
        _ => return 0.5, // non-comparison operator
    };
    normalize_distance(raw)
}

/// Normalize a raw distance to [0.0, 1.0) via `d / (d + 1)`.
fn normalize_distance(d: f64) -> f64 {
    if d <= 0.0 {
        0.0
    } else {
        d / (d + 1.0)
    }
}

/// Try to extract a numeric value from a constant expression.
fn eval_to_f64(expr: &SymExpr) -> Option<f64> {
    match expr {
        SymExpr::Const(ConstValue::Int(i)) => Some(*i as f64),
        SymExpr::Const(ConstValue::Float(f)) => Some(*f),
        SymExpr::Const(ConstValue::Bool(b)) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Reward executions that reach `Unknown` constraints (unsolvable by Z3).
///
/// Saturates at 5 unknown branches.
fn unknown_bonus_score(branch_path: &[BranchDecision]) -> f64 {
    let count = branch_path
        .iter()
        .filter(|bd| matches!(&bd.constraint, SymConstraint::Unknown { .. }))
        .count();
    if count == 0 {
        0.0
    } else {
        (count as f64).min(5.0) / 5.0
    }
}

/// 1.0 for a never-before-seen path hash, 0.0 for a repeat.
fn novelty_score(branch_path: &[BranchDecision], context: &mut FitnessContext) -> f64 {
    let path_hash = hash_branch_path(branch_path);
    if context.seen_paths.insert(path_hash) {
        1.0
    } else {
        0.0
    }
}

/// Average rarity of branches in the execution path.
///
/// Returns 0.0 when no profile is provided or the branch path is empty.
fn rarity_score(
    branch_path: &[BranchDecision],
    profile: Option<&crate::branch_profile::BranchProfile>,
) -> f64 {
    let Some(profile) = profile else { return 0.0 };
    if branch_path.is_empty() {
        return 0.0;
    }
    // Deduplicate: each branch_id contributes once.
    let unique: HashSet<u32> = branch_path.iter().map(|bd| bd.branch_id).collect();
    let sum: f64 = unique.iter().map(|&id| profile.rarity(id)).sum();
    sum / unique.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::protocol::{ExecuteResult, PerformanceMetrics};

    /// Helper to build an `ExecuteResult` with a given branch path.
    fn make_result(branch_path: Vec<BranchDecision>) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path,
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 0.0,
                cpu_time_us: 0,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            },
        }
    }

    /// Helper to build a `BranchDecision` with a numeric comparison constraint.
    fn numeric_branch(id: u32, taken: bool, op: BinOpKind, left: i64, right: i64) -> BranchDecision {
        BranchDecision {
            branch_id: id,
            line: id * 10,
            taken,
            constraint: SymConstraint::Expr {
                expr: SymExpr::BinOp {
                    op,
                    left: Box::new(SymExpr::Const(ConstValue::Int(left))),
                    right: Box::new(SymExpr::Const(ConstValue::Int(right))),
                },
            },
        }
    }

    fn unknown_branch(id: u32) -> BranchDecision {
        BranchDecision {
            branch_id: id,
            line: id * 10,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "regex".into(),
            },
        }
    }

    // -----------------------------------------------------------------------
    // Normalize distance
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_distance_zero_gives_zero() {
        assert_eq!(normalize_distance(0.0), 0.0);
    }

    #[test]
    fn normalize_distance_negative_gives_zero() {
        assert_eq!(normalize_distance(-5.0), 0.0);
    }

    #[test]
    fn normalize_distance_large_gives_near_one() {
        let d = normalize_distance(1000.0);
        assert!(d > 0.99 && d < 1.0, "expected near 1.0, got {d}");
    }

    // -----------------------------------------------------------------------
    // Branch distance — comparison operators
    // -----------------------------------------------------------------------

    #[test]
    fn branch_distance_eq_exact_match() {
        assert_eq!(comparison_distance(BinOpKind::Eq, 5.0, 5.0), 0.0);
    }

    #[test]
    fn branch_distance_eq_far_apart() {
        let d = comparison_distance(BinOpKind::Eq, 0.0, 100.0);
        assert!(d > 0.9, "expected high distance, got {d}");
    }

    #[test]
    fn branch_distance_lt_satisfied() {
        assert_eq!(comparison_distance(BinOpKind::Lt, 3.0, 10.0), 0.0);
    }

    #[test]
    fn branch_distance_lt_not_satisfied() {
        let d = comparison_distance(BinOpKind::Lt, 10.0, 3.0);
        assert!(d > 0.0, "expected positive distance, got {d}");
    }

    #[test]
    fn branch_distance_ne_equal_values() {
        let d = comparison_distance(BinOpKind::Ne, 7.0, 7.0);
        assert_eq!(d, 0.5);
    }

    #[test]
    fn branch_distance_ne_different_values() {
        assert_eq!(comparison_distance(BinOpKind::Ne, 3.0, 7.0), 0.0);
    }

    #[test]
    fn unknown_constraint_gives_moderate_distance() {
        let d = branch_distance(&SymConstraint::Unknown {
            hint: "regex".into(),
        });
        assert_eq!(d, 0.5);
    }

    // -----------------------------------------------------------------------
    // Coverage score
    // -----------------------------------------------------------------------

    #[test]
    fn empty_execution_coverage_zero() {
        assert_eq!(coverage_score(&[], 5), 0.0);
    }

    #[test]
    fn coverage_no_targets_is_perfect() {
        assert_eq!(coverage_score(&[], 0), 1.0);
    }

    // -----------------------------------------------------------------------
    // Unknown bonus
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_bonus_none() {
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        assert_eq!(unknown_bonus_score(&path), 0.0);
    }

    #[test]
    fn unknown_bonus_saturates_at_five() {
        let path: Vec<_> = (0..10).map(|i| unknown_branch(i)).collect();
        assert_eq!(unknown_bonus_score(&path), 1.0);
    }

    // -----------------------------------------------------------------------
    // Novelty
    // -----------------------------------------------------------------------

    #[test]
    fn novelty_new_path_scores_one() {
        let mut ctx = FitnessContext::new();
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        assert_eq!(novelty_score(&path, &mut ctx), 1.0);
    }

    #[test]
    fn novelty_repeated_path_scores_zero() {
        let mut ctx = FitnessContext::new();
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        novelty_score(&path, &mut ctx); // first time
        assert_eq!(novelty_score(&path, &mut ctx), 0.0);
    }

    // -----------------------------------------------------------------------
    // Full score integration
    // -----------------------------------------------------------------------

    #[test]
    fn empty_execution_scores_low() {
        let result = make_result(vec![]);
        let targets: HashSet<u32> = [1, 2, 3].into_iter().collect();
        let mut ctx = FitnessContext::new();
        let breakdown = score(&result, &targets, &mut ctx, &FitnessWeights::default(), None);

        assert_eq!(breakdown.coverage, 0.0);
        assert_eq!(breakdown.proximity, 0.0);
        assert_eq!(breakdown.unknown_bonus, 0.0);
        assert!(breakdown.total < 0.3, "expected low total, got {}", breakdown.total);
    }

    #[test]
    fn all_targets_hit_scores_high() {
        let path = vec![
            numeric_branch(1, true, BinOpKind::Eq, 5, 5),
            numeric_branch(2, true, BinOpKind::Eq, 10, 10),
        ];
        let result = make_result(path);
        let targets: HashSet<u32> = [1, 2].into_iter().collect();
        let mut ctx = FitnessContext::new();
        let breakdown = score(&result, &targets, &mut ctx, &FitnessWeights::default(), None);

        assert_eq!(breakdown.coverage, 1.0);
        assert_eq!(breakdown.proximity, 1.0);
        assert!(breakdown.total > 0.7, "expected high total, got {}", breakdown.total);
    }

    #[test]
    fn custom_weights_affect_total() {
        let path = vec![numeric_branch(1, true, BinOpKind::Eq, 5, 5)];
        let result = make_result(path);
        let targets: HashSet<u32> = [1].into_iter().collect();
        let mut ctx = FitnessContext::new();

        // Only proximity weight
        let weights = FitnessWeights {
            coverage: 0.0,
            proximity: 1.0,
            unknown_bonus: 0.0,
            novelty: 0.0,
            depth: 0.0,
            rarity: 0.0,
        };
        let breakdown = score(&result, &targets, &mut ctx, &weights, None);
        assert_eq!(breakdown.total, breakdown.proximity);
    }

    #[test]
    fn default_weights_sum_to_one() {
        let w = FitnessWeights::default();
        let sum = w.coverage + w.proximity + w.unknown_bonus + w.novelty + w.depth + w.rarity;
        assert!((sum - 1.0).abs() < f64::EPSILON, "weights sum to {sum}, expected 1.0");
    }

    #[test]
    fn fitness_always_in_zero_one_range() {
        let scenarios: Vec<Vec<BranchDecision>> = vec![
            vec![],
            vec![numeric_branch(1, true, BinOpKind::Gt, 100, 0)],
            vec![unknown_branch(1), unknown_branch(2), unknown_branch(3)],
            vec![
                numeric_branch(1, false, BinOpKind::Lt, 50, 10),
                numeric_branch(2, true, BinOpKind::Eq, 7, 7),
                unknown_branch(3),
            ],
        ];
        let targets: HashSet<u32> = [1, 2, 3, 4].into_iter().collect();

        for (i, path) in scenarios.into_iter().enumerate() {
            let mut ctx = FitnessContext::new();
            let result = make_result(path);
            let b = score(&result, &targets, &mut ctx, &FitnessWeights::default(), None);
            assert!(
                b.total >= 0.0 && b.total <= 1.0,
                "scenario {i}: total {:.4} out of range",
                b.total
            );
            assert!(b.coverage >= 0.0 && b.coverage <= 1.0);
            assert!(b.proximity >= 0.0 && b.proximity <= 1.0);
            assert!(b.unknown_bonus >= 0.0 && b.unknown_bonus <= 1.0);
            assert!(b.novelty >= 0.0 && b.novelty <= 1.0);
        }
    }

    #[test]
    fn zero_weights_give_zero_total() {
        let result = make_result(vec![numeric_branch(1, true, BinOpKind::Eq, 5, 5)]);
        let targets: HashSet<u32> = [1].into_iter().collect();
        let mut ctx = FitnessContext::new();
        let weights = FitnessWeights {
            coverage: 0.0,
            proximity: 0.0,
            unknown_bonus: 0.0,
            novelty: 0.0,
            depth: 0.0,
            rarity: 0.0,
        };
        let b = score(&result, &targets, &mut ctx, &weights, None);
        assert_eq!(b.total, 0.0);
    }

    // -----------------------------------------------------------------------
    // Depth score
    // -----------------------------------------------------------------------

    #[test]
    fn depth_score_empty_path() {
        assert_eq!(depth_score(&[]), 0.0);
    }

    #[test]
    fn depth_score_monotonically_increases() {
        let short: Vec<_> = (0..3).map(|i| numeric_branch(i, true, BinOpKind::Gt, 10, 5)).collect();
        let long: Vec<_> = (0..20).map(|i| numeric_branch(i, true, BinOpKind::Gt, 10, 5)).collect();
        assert!(depth_score(&short) < depth_score(&long));
    }

    #[test]
    fn depth_score_bounded_below_one() {
        let very_long: Vec<_> = (0..10_000).map(|i| numeric_branch(i, true, BinOpKind::Gt, 10, 5)).collect();
        let score = depth_score(&very_long);
        assert!(score < 1.0, "expected < 1.0, got {score}");
        assert!(score > 0.99, "expected near 1.0, got {score}");
    }

    #[test]
    fn depth_at_half_saturation() {
        let path: Vec<_> = (0..DEPTH_HALF_SATURATION as u32)
            .map(|i| numeric_branch(i, true, BinOpKind::Gt, 10, 5))
            .collect();
        let score = depth_score(&path);
        assert!((score - 0.5).abs() < f64::EPSILON, "expected 0.5, got {score}");
    }

    #[test]
    fn fitness_range_includes_depth() {
        let scenarios: Vec<Vec<BranchDecision>> = vec![
            vec![],
            (0..30).map(|i| numeric_branch(i, true, BinOpKind::Gt, 10, 5)).collect(),
        ];
        let targets: HashSet<u32> = [1, 2, 3, 4].into_iter().collect();

        for (i, path) in scenarios.into_iter().enumerate() {
            let mut ctx = FitnessContext::new();
            let result = make_result(path);
            let b = score(&result, &targets, &mut ctx, &FitnessWeights::default(), None);
            assert!(
                b.depth >= 0.0 && b.depth < 1.0,
                "scenario {i}: depth {:.4} out of range",
                b.depth
            );
            assert!(
                b.total >= 0.0 && b.total <= 1.0,
                "scenario {i}: total {:.4} out of range",
                b.total
            );
        }
    }

    // -----------------------------------------------------------------------
    // Rarity scoring
    // -----------------------------------------------------------------------

    #[test]
    fn rarity_score_without_profile_is_zero() {
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        assert_eq!(rarity_score(&path, None), 0.0);
    }

    #[test]
    fn rarity_score_empty_path_is_zero() {
        let profile = crate::branch_profile::BranchProfile::new(HashMap::new());
        assert_eq!(rarity_score(&[], Some(&profile)), 0.0);
    }

    #[test]
    fn rarity_score_rare_branch_is_high() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.1); // rarely seen → rarity 0.9
        let profile = crate::branch_profile::BranchProfile::new(freqs);
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        let r = rarity_score(&path, Some(&profile));
        assert!((r - 0.9).abs() < f64::EPSILON, "expected 0.9, got {r}");
    }

    #[test]
    fn rarity_score_common_branch_is_low() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.9); // commonly seen → rarity 0.1
        let profile = crate::branch_profile::BranchProfile::new(freqs);
        let path = vec![numeric_branch(1, true, BinOpKind::Gt, 10, 5)];
        let r = rarity_score(&path, Some(&profile));
        assert!((r - 0.1).abs() < f64::EPSILON, "expected 0.1, got {r}");
    }

    #[test]
    fn rarity_score_unknown_branch_is_one() {
        let profile = crate::branch_profile::BranchProfile::new(HashMap::new());
        let path = vec![numeric_branch(99, true, BinOpKind::Gt, 10, 5)];
        let r = rarity_score(&path, Some(&profile));
        assert_eq!(r, 1.0);
    }

    #[test]
    fn score_with_profile_boosts_rare_branches() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.1); // rare
        freqs.insert(2, 0.9); // common
        let profile = crate::branch_profile::BranchProfile::new(freqs);

        let rare_path = vec![numeric_branch(1, true, BinOpKind::Eq, 5, 5)];
        let common_path = vec![numeric_branch(2, true, BinOpKind::Eq, 5, 5)];

        let targets: HashSet<u32> = [1, 2].into_iter().collect();
        let weights = FitnessWeights {
            coverage: 0.0,
            proximity: 0.0,
            unknown_bonus: 0.0,
            novelty: 0.0,
            depth: 0.0,
            rarity: 1.0,
        };

        let mut ctx = FitnessContext::new();
        let rare_score = score(&make_result(rare_path), &targets, &mut ctx, &weights, Some(&profile));
        let mut ctx2 = FitnessContext::new();
        let common_score = score(&make_result(common_path), &targets, &mut ctx2, &weights, Some(&profile));

        assert!(rare_score.total > common_score.total,
            "rare branch score ({}) should exceed common ({})",
            rare_score.total, common_score.total);
    }

    #[test]
    fn fitness_with_rarity_in_zero_one_range() {
        let mut freqs = HashMap::new();
        freqs.insert(1, 0.3);
        freqs.insert(2, 0.7);
        freqs.insert(3, 0.0);
        let profile = crate::branch_profile::BranchProfile::new(freqs);

        let scenarios: Vec<Vec<BranchDecision>> = vec![
            vec![],
            vec![numeric_branch(1, true, BinOpKind::Gt, 100, 0)],
            vec![unknown_branch(1), unknown_branch(2), unknown_branch(3)],
        ];
        let targets: HashSet<u32> = [1, 2, 3].into_iter().collect();

        for (i, path) in scenarios.into_iter().enumerate() {
            let mut ctx = FitnessContext::new();
            let result = make_result(path);
            let b = score(&result, &targets, &mut ctx, &FitnessWeights::default(), Some(&profile));
            assert!(
                b.total >= 0.0 && b.total <= 1.0,
                "scenario {i}: total {:.4} out of range",
                b.total
            );
            assert!(b.rarity >= 0.0 && b.rarity <= 1.0,
                "scenario {i}: rarity {:.4} out of range", b.rarity);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn depth_score_monotonic(len1 in 0usize..1000, delta in 1usize..1000) {
            let len2 = len1 + delta;
            let s1 = len1 as f64 / (len1 as f64 + DEPTH_HALF_SATURATION);
            let s2 = len2 as f64 / (len2 as f64 + DEPTH_HALF_SATURATION);
            prop_assert!(s1 <= s2, "depth({len1})={s1} > depth({len2})={s2}");
        }

        #[test]
        fn depth_score_in_zero_one(len in 0usize..100_000) {
            let s = len as f64 / (len as f64 + DEPTH_HALF_SATURATION);
            prop_assert!(s >= 0.0);
            prop_assert!(s < 1.0);
        }

        #[test]
        fn default_weights_sum_is_one(
            _dummy in 0u8..1
        ) {
            let w = FitnessWeights::default();
            let sum = w.coverage + w.proximity + w.unknown_bonus + w.novelty + w.depth + w.rarity;
            prop_assert!((sum - 1.0).abs() < 1e-10, "weights sum to {sum}");
        }

        #[test]
        fn total_fitness_bounded(
            n_branches in 0usize..50,
            n_targets in 1usize..20,
        ) {
            use crate::execution_record::BranchDecision;
            use crate::protocol::{ExecuteResult, PerformanceMetrics};

            let path: Vec<BranchDecision> = (0..n_branches as u32)
                .map(|i| BranchDecision {
                    branch_id: i,
                    line: i * 10,
                    taken: true,
                    constraint: crate::execution_record::SymConstraint::Unknown {
                        hint: "test".into(),
                    },
                })
                .collect();

            let result = ExecuteResult {
                return_value: None,
                thrown_error: None,
                branch_path: path,
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![], connection_failures: vec![],
                performance: PerformanceMetrics {
                    wall_time_ms: 0.0,
                    cpu_time_us: 0,
                    heap_used_bytes: 0,
                    heap_allocated_bytes: 0,
                },
            };

            let targets: std::collections::HashSet<u32> =
                (0..n_targets as u32).collect();
            let mut ctx = FitnessContext::new();
            let b = score(&result, &targets, &mut ctx, &FitnessWeights::default(), None);

            prop_assert!(b.total >= 0.0 && b.total <= 1.0, "total={}", b.total);
            prop_assert!(b.depth >= 0.0 && b.depth < 1.0, "depth={}", b.depth);
            prop_assert!(b.rarity >= 0.0 && b.rarity <= 1.0, "rarity={}", b.rarity);
            prop_assert!(b.coverage >= 0.0 && b.coverage <= 1.0);
            prop_assert!(b.proximity >= 0.0 && b.proximity <= 1.0);
            prop_assert!(b.unknown_bonus >= 0.0 && b.unknown_bonus <= 1.0);
            prop_assert!(b.novelty >= 0.0 && b.novelty <= 1.0);
        }
    }
}
