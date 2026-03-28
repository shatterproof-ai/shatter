//! Loop analysis: induction variable detection, constraint rewriting, and state merging.
//!
//! Canonical counted loops (e.g., `for (i = 0; i < n; i++)`) produce O(k) redundant
//! backedge constraints per loop iteration. This module provides two complementary
//! techniques:
//!
//! - **Technique 5** ([`rewrite_loop_constraints`]): Collapses backedge constraints
//!   into O(1) direct induction variable constraints.
//! - **Technique 6** ([`merge_loop_states`]): Merges per-iteration constraints into
//!   ITE chains with a free iteration variable, allowing Z3 to reason about
//!   path-dependent behaviors across iterations.

use std::collections::{HashMap, HashSet};

use crate::execution_record::{ScopeEvent, TraceEvent};
use crate::protocol::{BoundOp, ExecuteResult, LoopInfo};
use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

/// Count the number of iterations per loop_id observed in execution trace events.
///
/// An iteration is counted each time a `LoopEnter` event is seen for a given loop_id.
pub fn count_loop_iterations(result: &ExecuteResult) -> HashMap<u32, u32> {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for event in &result.scope_events {
        if let TraceEvent::Scope {
            event: ScopeEvent::LoopEnter { loop_id },
        } = event
        {
            *counts.entry(*loop_id).or_insert(0) += 1;
        }
    }
    counts
}

/// Check whether a symbolic expression matches the condition pattern of a canonical loop.
///
/// For a loop `for (i = init; i < bound; i += step)`, the condition constraint
/// will typically be a `BinOp` with the induction variable on one side and the
/// bound on the other, using the comparison operator from `BoundOp`.
fn constraint_matches_loop_condition(expr: &SymExpr, loop_info: &LoopInfo) -> bool {
    let iv_name = &loop_info.induction_var.name;
    let expected_op = match loop_info.induction_var.bound_op {
        BoundOp::Lt => BinOpKind::Lt,
        BoundOp::Le => BinOpKind::Le,
        BoundOp::Gt => BinOpKind::Gt,
        BoundOp::Ge => BinOpKind::Ge,
    };

    match expr {
        SymExpr::BinOp { op, left, right } if *op == expected_op => {
            let left_is_iv = matches!(left.as_ref(), SymExpr::Param { name, .. } if name == iv_name);
            let right_is_iv = matches!(right.as_ref(), SymExpr::Param { name, .. } if name == iv_name);
            left_is_iv || right_is_iv
        }
        _ => false,
    }
}

/// Build a direct induction variable constraint: `iv == init + step * iteration_count`.
///
/// This replaces k backedge constraints with a single constraint that directly
/// encodes the iteration count.
fn build_direct_iv_constraint(loop_info: &LoopInfo, iteration_count: u32) -> SymExpr {
    let iv = &loop_info.induction_var;

    // step * iteration_count
    let step_times_k = SymExpr::BinOp {
        op: BinOpKind::Mul,
        left: Box::new(iv.step_expr.clone()),
        right: Box::new(SymExpr::Const(ConstValue::Int(i64::from(iteration_count)))),
    };

    // init + step * iteration_count
    let iv_value = SymExpr::BinOp {
        op: BinOpKind::Add,
        left: Box::new(iv.init_expr.clone()),
        right: Box::new(step_times_k),
    };

    // iv == init + step * iteration_count
    SymExpr::BinOp {
        op: BinOpKind::Eq,
        left: Box::new(SymExpr::Param {
            name: iv.name.clone(),
            path: vec![],
        }),
        right: Box::new(iv_value),
    }
}

/// Rewrite loop backedge constraints for canonical counted loops.
///
/// For each canonical loop with k observed iterations, this replaces k redundant
/// condition constraints (e.g., `i < n` repeated k times) with a single direct
/// constraint (`i == init + step * k`).
///
/// Constraints that don't match any canonical loop are passed through unchanged.
pub fn rewrite_loop_constraints(
    constraints: &[Option<SymExpr>],
    loops: &[LoopInfo],
    result: &ExecuteResult,
) -> Vec<Option<SymExpr>> {
    if loops.is_empty() {
        return constraints.to_vec();
    }

    let iteration_counts = count_loop_iterations(result);
    if iteration_counts.is_empty() {
        return constraints.to_vec();
    }

    let loop_map: HashMap<u32, &LoopInfo> = loops.iter().map(|l| (l.loop_id, l)).collect();

    // Track which backedge constraints we've already replaced per loop.
    // We keep the first occurrence and replace it with the direct constraint,
    // then remove all subsequent occurrences for the same loop.
    let mut replaced_loops: HashSet<u32> = HashSet::new();
    let mut rewritten: Vec<Option<SymExpr>> = Vec::with_capacity(constraints.len());

    for constraint in constraints {
        match constraint {
            Some(expr) => {
                // Check if this constraint matches any canonical loop's condition
                let mut matched_loop: Option<u32> = None;
                for (&loop_id, &loop_info) in &loop_map {
                    if constraint_matches_loop_condition(expr, loop_info)
                        && iteration_counts.contains_key(&loop_id)
                    {
                        matched_loop = Some(loop_id);
                        break;
                    }
                }

                if let Some(loop_id) = matched_loop {
                    if replaced_loops.insert(loop_id) {
                        // First backedge for this loop — replace with direct constraint
                        let count = iteration_counts[&loop_id];
                        let direct = build_direct_iv_constraint(loop_map[&loop_id], count);
                        rewritten.push(Some(direct));
                    }
                    // Subsequent backedges for same loop — skip (collapse O(k) → O(1))
                } else {
                    // Not a loop backedge — pass through
                    rewritten.push(constraint.clone());
                }
            }
            None => {
                rewritten.push(None);
            }
        }
    }

    rewritten
}

/// Maximum number of loop iterations to merge via ITE chains.
/// Beyond this, constraints pass through unmodified (Technique 5 may still handle them).
const MAX_MERGE_DEPTH: u32 = 10;

/// Result of merging per-iteration constraints into an ITE chain.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedLoopState {
    /// Which loop this merge applies to.
    pub loop_id: u32,
    /// Name of the free iteration variable (e.g., `__loop_0_iter`).
    pub iteration_var: String,
    /// The merged ITE-chain constraint (includes iteration bound).
    pub merged_constraint: SymExpr,
    /// Number of iterations observed (and merged).
    pub iteration_count: u32,
}

/// Build the iteration variable name for a given loop.
fn iteration_var_name(loop_id: u32) -> String {
    format!("__loop_{loop_id}_iter")
}

/// Build the bounding constraint: `0 <= iter_var && iter_var < N`.
fn build_iteration_bound(iter_var: &str, n: u32) -> SymExpr {
    let iter_param = SymExpr::Param {
        name: iter_var.to_string(),
        path: vec![],
    };
    let lower = SymExpr::BinOp {
        op: BinOpKind::Le,
        left: Box::new(SymExpr::Const(ConstValue::Int(0))),
        right: Box::new(iter_param.clone()),
    };
    let upper = SymExpr::BinOp {
        op: BinOpKind::Lt,
        left: Box::new(iter_param),
        right: Box::new(SymExpr::Const(ConstValue::Int(i64::from(n)))),
    };
    SymExpr::BinOp {
        op: BinOpKind::And,
        left: Box::new(lower),
        right: Box::new(upper),
    }
}

/// Build an ITE chain from per-iteration constraints.
///
/// For constraints `[c0, c1, c2]` and iteration variable `iter`:
/// ```text
/// ite(iter == 0, c0, ite(iter == 1, c1, c2))
/// ```
///
/// The last constraint becomes the else branch of the innermost ITE.
fn build_ite_chain(iter_var: &str, constraints: &[SymExpr]) -> Option<SymExpr> {
    if constraints.is_empty() {
        return None;
    }
    if constraints.len() == 1 {
        return Some(constraints[0].clone());
    }

    let iter_param = SymExpr::Param {
        name: iter_var.to_string(),
        path: vec![],
    };

    // Fold from the right: last constraint is the base else-branch.
    let mut result = constraints.last()?.clone();

    for (i, constraint) in constraints.iter().enumerate().rev().skip(1) {
        let condition = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(iter_param.clone()),
            right: Box::new(SymExpr::Const(ConstValue::Int(i as i64))),
        };
        result = SymExpr::Ite {
            condition: Box::new(condition),
            then_expr: Box::new(constraint.clone()),
            else_expr: Box::new(result),
        };
    }

    Some(result)
}

/// Merge per-iteration loop constraints into ITE chains (Technique 6).
///
/// For each canonical loop with 2..=[`MAX_MERGE_DEPTH`] observed iterations, this
/// collects per-iteration constraints, builds an ITE chain with a free iteration
/// variable, and replaces the original per-iteration constraints with a single
/// merged constraint.
///
/// Constraints not matching any loop pass through unchanged. Loops with more than
/// [`MAX_MERGE_DEPTH`] iterations or fewer than 2 constraints are left unmodified.
pub fn merge_loop_states(
    constraints: &[Option<SymExpr>],
    loops: &[LoopInfo],
    result: &ExecuteResult,
) -> Vec<Option<SymExpr>> {
    if loops.is_empty() {
        return constraints.to_vec();
    }

    let iteration_counts = count_loop_iterations(result);
    if iteration_counts.is_empty() {
        return constraints.to_vec();
    }

    let loop_map: HashMap<u32, &LoopInfo> = loops.iter().map(|l| (l.loop_id, l)).collect();

    // Collect per-iteration constraints for each eligible loop.
    // A constraint is "per-iteration" if it matches the loop's condition pattern.
    let mut loop_constraints: HashMap<u32, Vec<(usize, SymExpr)>> = HashMap::new();

    for (idx, constraint) in constraints.iter().enumerate() {
        if let Some(expr) = constraint {
            for (&loop_id, &loop_info) in &loop_map {
                let count = iteration_counts.get(&loop_id).copied().unwrap_or(0);
                if (2..=MAX_MERGE_DEPTH).contains(&count)
                    && constraint_matches_loop_condition(expr, loop_info)
                {
                    loop_constraints
                        .entry(loop_id)
                        .or_default()
                        .push((idx, expr.clone()));
                    break;
                }
            }
        }
    }

    // Filter to loops with >= 2 per-iteration constraints.
    let mergeable: HashMap<u32, Vec<(usize, SymExpr)>> = loop_constraints
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .collect();

    if mergeable.is_empty() {
        return constraints.to_vec();
    }

    // Collect all indices that will be replaced.
    let mut replaced_indices: HashSet<usize> = HashSet::new();
    let mut merged_states: Vec<MergedLoopState> = Vec::new();

    for (&loop_id, per_iter) in &mergeable {
        let iter_var = iteration_var_name(loop_id);
        let per_iter_exprs: Vec<SymExpr> = per_iter.iter().map(|(_, e)| e.clone()).collect();
        let count = per_iter_exprs.len() as u32;

        if let Some(ite_chain) = build_ite_chain(&iter_var, &per_iter_exprs) {
            let bound = build_iteration_bound(&iter_var, count);
            let merged = SymExpr::BinOp {
                op: BinOpKind::And,
                left: Box::new(bound),
                right: Box::new(ite_chain),
            };

            for &(idx, _) in per_iter {
                replaced_indices.insert(idx);
            }

            merged_states.push(MergedLoopState {
                loop_id,
                iteration_var: iter_var,
                merged_constraint: merged,
                iteration_count: count,
            });
        }
    }

    // Build the output: pass through non-replaced constraints, insert merged ones.
    let mut output: Vec<Option<SymExpr>> = Vec::with_capacity(constraints.len());
    let mut first_replaced_per_loop: HashMap<u32, bool> = HashMap::new();

    for (idx, constraint) in constraints.iter().enumerate() {
        if replaced_indices.contains(&idx) {
            // Find which loop this index belongs to and insert merged constraint once.
            for ms in &merged_states {
                let is_member = mergeable
                    .get(&ms.loop_id)
                    .is_some_and(|v| v.iter().any(|(i, _)| *i == idx));
                if is_member && !first_replaced_per_loop.contains_key(&ms.loop_id) {
                    first_replaced_per_loop.insert(ms.loop_id, true);
                    output.push(Some(ms.merged_constraint.clone()));
                    break;
                }
            }
            // Subsequent indices for the same loop are dropped (collapsed).
        } else {
            output.push(constraint.clone());
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::InductionVar;

    fn make_execute_result_with_loops(loop_iterations: &[(u32, u32)]) -> ExecuteResult {
        let mut scope_events = Vec::new();
        for &(loop_id, count) in loop_iterations {
            for _ in 0..count {
                scope_events.push(TraceEvent::Scope {
                    event: ScopeEvent::LoopEnter { loop_id },
                });
                scope_events.push(TraceEvent::Scope {
                    event: ScopeEvent::LoopExit { loop_id },
                });
            }
        }

        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events,
            performance: Default::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
        }
    }

    fn make_loop_info(loop_id: u32, iv_name: &str) -> LoopInfo {
        LoopInfo {
            loop_id,
            line: 10,
            induction_var: InductionVar {
                name: iv_name.to_string(),
                init_expr: SymExpr::Const(ConstValue::Int(0)),
                step_expr: SymExpr::Const(ConstValue::Int(1)),
                bound_expr: SymExpr::Param {
                    name: "n".to_string(),
                    path: vec![],
                },
                bound_op: BoundOp::Lt,
            },
        }
    }

    fn make_backedge_constraint(iv_name: &str, bound_name: &str) -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: iv_name.to_string(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Param {
                name: bound_name.to_string(),
                path: vec![],
            }),
        }
    }

    #[test]
    fn count_loop_iterations_empty_trace() {
        let result = make_execute_result_with_loops(&[]);
        let counts = count_loop_iterations(&result);
        assert!(counts.is_empty());
    }

    #[test]
    fn count_loop_iterations_single_loop() {
        let result = make_execute_result_with_loops(&[(0, 5)]);
        let counts = count_loop_iterations(&result);
        assert_eq!(counts.get(&0), Some(&5));
    }

    #[test]
    fn count_loop_iterations_multiple_loops() {
        let result = make_execute_result_with_loops(&[(0, 3), (1, 7)]);
        let counts = count_loop_iterations(&result);
        assert_eq!(counts.get(&0), Some(&3));
        assert_eq!(counts.get(&1), Some(&7));
    }

    #[test]
    fn rewrite_no_loops_passes_through() {
        let constraints = vec![
            Some(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            None,
        ];
        let result = make_execute_result_with_loops(&[]);
        let rewritten = rewrite_loop_constraints(&constraints, &[], &result);
        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0], constraints[0]);
        assert!(rewritten[1].is_none());
    }

    #[test]
    fn rewrite_collapses_backedge_constraints() {
        let backedge = make_backedge_constraint("i", "n");
        let non_loop_constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };

        let constraints: Vec<Option<SymExpr>> = vec![
            Some(non_loop_constraint.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
        ];

        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 5)]);

        let rewritten = rewrite_loop_constraints(&constraints, &[loop_info], &result);

        // Should have 2 constraints: the non-loop one + 1 collapsed loop constraint
        assert_eq!(rewritten.len(), 2);
        assert_eq!(rewritten[0], Some(non_loop_constraint));
        let direct = rewritten[1].as_ref().expect("should have direct constraint");
        match direct {
            SymExpr::BinOp { op, .. } => assert_eq!(*op, BinOpKind::Eq),
            other => panic!("expected BinOp(eq), got {:?}", other),
        }
    }

    #[test]
    fn rewrite_non_matching_constraints_unchanged() {
        let non_matching = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };

        let constraints = vec![Some(non_matching.clone()), None, Some(non_matching.clone())];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 3)]);

        let rewritten = rewrite_loop_constraints(&constraints, &[loop_info], &result);
        assert_eq!(rewritten.len(), 3);
        assert_eq!(rewritten, constraints);
    }

    #[test]
    fn rewrite_idempotent() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints = vec![Some(backedge.clone()), Some(backedge.clone()), Some(backedge)];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 3)]);

        let first_rewrite = rewrite_loop_constraints(&constraints, &[loop_info.clone()], &result);
        let second_rewrite = rewrite_loop_constraints(&first_rewrite, &[loop_info], &result);
        assert_eq!(first_rewrite, second_rewrite);
    }

    #[test]
    fn constraint_matches_positive() {
        let loop_info = make_loop_info(0, "i");
        let expr = make_backedge_constraint("i", "n");
        assert!(constraint_matches_loop_condition(&expr, &loop_info));
    }

    #[test]
    fn constraint_matches_wrong_variable() {
        let loop_info = make_loop_info(0, "i");
        let expr = make_backedge_constraint("j", "n");
        assert!(!constraint_matches_loop_condition(&expr, &loop_info));
    }

    #[test]
    fn constraint_matches_wrong_op() {
        let loop_info = make_loop_info(0, "i");
        let expr = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "i".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Param {
                name: "n".into(),
                path: vec![],
            }),
        };
        assert!(!constraint_matches_loop_condition(&expr, &loop_info));
    }

    #[test]
    fn build_direct_constraint_structure() {
        let loop_info = make_loop_info(0, "i");
        let direct = build_direct_iv_constraint(&loop_info, 5);

        // Should be: i == 0 + 1 * 5
        match &direct {
            SymExpr::BinOp {
                op, left, right, ..
            } => {
                assert_eq!(*op, BinOpKind::Eq);
                match left.as_ref() {
                    SymExpr::Param { name, .. } => assert_eq!(name, "i"),
                    other => panic!("expected Param, got {:?}", other),
                }
                match right.as_ref() {
                    SymExpr::BinOp {
                        op: add_op,
                        left: init,
                        right: step_k,
                    } => {
                        assert_eq!(*add_op, BinOpKind::Add);
                        assert_eq!(**init, SymExpr::Const(ConstValue::Int(0)));
                        match step_k.as_ref() {
                            SymExpr::BinOp {
                                op: mul_op,
                                left: step,
                                right: k,
                            } => {
                                assert_eq!(*mul_op, BinOpKind::Mul);
                                assert_eq!(**step, SymExpr::Const(ConstValue::Int(1)));
                                assert_eq!(**k, SymExpr::Const(ConstValue::Int(5)));
                            }
                            other => panic!("expected BinOp(mul), got {:?}", other),
                        }
                    }
                    other => panic!("expected BinOp(add), got {:?}", other),
                }
            }
            other => panic!("expected BinOp(eq), got {:?}", other),
        }
    }

    // --- merge_loop_states tests ---

    #[test]
    fn merge_empty_loops_returns_unchanged() {
        let constraints = vec![
            Some(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            None,
        ];
        let result = make_execute_result_with_loops(&[]);
        let merged = merge_loop_states(&constraints, &[], &result);
        assert_eq!(merged, constraints);
    }

    #[test]
    fn merge_single_iteration_skipped() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints = vec![Some(backedge)];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 1)]);
        let merged = merge_loop_states(&constraints, &[loop_info], &result);
        // Single iteration: not worth merging, pass through
        assert_eq!(merged, constraints);
    }

    #[test]
    fn merge_two_iterations_produces_ite() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints = vec![Some(backedge.clone()), Some(backedge.clone())];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 2)]);

        let merged = merge_loop_states(&constraints, &[loop_info], &result);

        // Should collapse to 1 constraint
        assert_eq!(merged.len(), 1);
        let merged_expr = merged[0].as_ref().expect("should have merged constraint");

        // Top level is And(bound, ite_chain)
        match merged_expr {
            SymExpr::BinOp {
                op: BinOpKind::And,
                right,
                ..
            } => {
                // The right side should be an ITE
                match right.as_ref() {
                    SymExpr::Ite { condition, .. } => {
                        // Condition should be: __loop_0_iter == 0
                        match condition.as_ref() {
                            SymExpr::BinOp {
                                op: BinOpKind::Eq,
                                left,
                                right,
                            } => {
                                match left.as_ref() {
                                    SymExpr::Param { name, .. } => {
                                        assert_eq!(name, "__loop_0_iter")
                                    }
                                    other => panic!("expected Param, got {:?}", other),
                                }
                                assert_eq!(**right, SymExpr::Const(ConstValue::Int(0)));
                            }
                            other => panic!("expected BinOp(Eq), got {:?}", other),
                        }
                    }
                    other => panic!("expected Ite, got {:?}", other),
                }
            }
            other => panic!("expected BinOp(And), got {:?}", other),
        }
    }

    #[test]
    fn merge_three_iterations_nested_ite() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints = vec![
            Some(backedge.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
        ];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 3)]);

        let merged = merge_loop_states(&constraints, &[loop_info], &result);
        assert_eq!(merged.len(), 1);

        // Verify the ITE chain has 2 levels of nesting
        let merged_expr = merged[0].as_ref().expect("should have merged constraint");
        match merged_expr {
            SymExpr::BinOp {
                op: BinOpKind::And,
                right,
                ..
            } => match right.as_ref() {
                SymExpr::Ite { else_expr, .. } => match else_expr.as_ref() {
                    SymExpr::Ite { .. } => {} // Good: nested ITE
                    other => panic!("expected nested Ite, got {:?}", other),
                },
                other => panic!("expected Ite, got {:?}", other),
            },
            other => panic!("expected BinOp(And), got {:?}", other),
        }
    }

    #[test]
    fn merge_exceeds_cap_passes_through() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints: Vec<Option<SymExpr>> = (0..11).map(|_| Some(backedge.clone())).collect();
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 11)]);

        let merged = merge_loop_states(&constraints, &[loop_info], &result);
        // Exceeds MAX_MERGE_DEPTH (10), should pass through unchanged
        assert_eq!(merged.len(), 11);
        assert_eq!(merged, constraints);
    }

    #[test]
    fn merge_preserves_non_loop_constraints() {
        let backedge = make_backedge_constraint("i", "n");
        let non_loop = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };

        let constraints = vec![
            Some(non_loop.clone()),
            Some(backedge.clone()),
            Some(backedge.clone()),
            None,
        ];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 2)]);

        let merged = merge_loop_states(&constraints, &[loop_info], &result);

        // non_loop + merged + None = 3
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0], Some(non_loop));
        assert!(merged[1].is_some()); // merged ITE
        assert!(merged[2].is_none());
    }

    #[test]
    fn merge_iteration_bound_present() {
        let backedge = make_backedge_constraint("i", "n");
        let constraints = vec![Some(backedge.clone()), Some(backedge.clone())];
        let loop_info = make_loop_info(0, "i");
        let result = make_execute_result_with_loops(&[(0, 2)]);

        let merged = merge_loop_states(&constraints, &[loop_info], &result);
        let merged_expr = merged[0].as_ref().expect("should have merged constraint");

        // Top level is And(bound, ite_chain)
        // bound is And(Le(0, iter), Lt(iter, 2))
        match merged_expr {
            SymExpr::BinOp {
                op: BinOpKind::And,
                left: bound,
                ..
            } => match bound.as_ref() {
                SymExpr::BinOp {
                    op: BinOpKind::And,
                    left: lower,
                    right: upper,
                } => {
                    match lower.as_ref() {
                        SymExpr::BinOp {
                            op: BinOpKind::Le, ..
                        } => {}
                        other => panic!("expected Le bound, got {:?}", other),
                    }
                    match upper.as_ref() {
                        SymExpr::BinOp {
                            op: BinOpKind::Lt,
                            right,
                            ..
                        } => {
                            assert_eq!(**right, SymExpr::Const(ConstValue::Int(2)));
                        }
                        other => panic!("expected Lt bound, got {:?}", other),
                    }
                }
                other => panic!("expected And bound, got {:?}", other),
            },
            other => panic!("expected BinOp(And), got {:?}", other),
        }
    }

    #[test]
    fn build_ite_chain_empty_returns_none() {
        assert!(build_ite_chain("iter", &[]).is_none());
    }

    #[test]
    fn build_ite_chain_single_returns_expr() {
        let expr = SymExpr::Const(ConstValue::Int(42));
        let chain = build_ite_chain("iter", &[expr.clone()]);
        assert_eq!(chain, Some(expr));
    }

    #[test]
    fn build_ite_chain_two_produces_single_ite() {
        let c0 = SymExpr::Const(ConstValue::Int(0));
        let c1 = SymExpr::Const(ConstValue::Int(1));
        let chain = build_ite_chain("iter", &[c0.clone(), c1.clone()]).unwrap();
        match &chain {
            SymExpr::Ite {
                condition,
                then_expr,
                else_expr,
            } => {
                // condition: iter == 0
                match condition.as_ref() {
                    SymExpr::BinOp {
                        op: BinOpKind::Eq,
                        right,
                        ..
                    } => assert_eq!(**right, SymExpr::Const(ConstValue::Int(0))),
                    other => panic!("expected Eq, got {:?}", other),
                }
                assert_eq!(**then_expr, c0);
                assert_eq!(**else_expr, c1);
            }
            other => panic!("expected Ite, got {:?}", other),
        }
    }

    mod proptests {
        use proptest::prelude::*;

        use super::*;
        use crate::sym_expr::extract_param_names;
        use crate::test_arbitraries::{arb_induction_var, arb_loop_info, arb_sym_expr};

        proptest! {
            #[test]
            fn induction_var_roundtrip(iv in arb_induction_var()) {
                let json = serde_json::to_string(&iv).unwrap();
                let back: crate::protocol::InductionVar = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(iv, back);
            }

            #[test]
            fn loop_info_roundtrip(li in arb_loop_info()) {
                let json = serde_json::to_string(&li).unwrap();
                let back: crate::protocol::LoopInfo = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(li, back);
            }

            /// ITE chain length never exceeds input length.
            #[test]
            fn ite_chain_preserves_constraints(
                constraints in prop::collection::vec(arb_sym_expr(1), 1..=12)
            ) {
                let chain = build_ite_chain("__test_iter", &constraints);
                prop_assert!(chain.is_some());
            }

            /// Merged constraint always references the iteration variable.
            #[test]
            fn merged_constraint_contains_iteration_var(
                constraints in prop::collection::vec(arb_sym_expr(1), 2..=8)
            ) {
                let iter_var = "__loop_99_iter";
                if let Some(chain) = build_ite_chain(iter_var, &constraints) {
                    let params = extract_param_names(&chain);
                    prop_assert!(
                        params.contains(iter_var),
                        "ITE chain should reference iteration var, got params: {:?}",
                        params
                    );
                }
            }

            /// ITE chain round-trips through serde.
            #[test]
            fn ite_chain_serde_roundtrip(
                constraints in prop::collection::vec(arb_sym_expr(1), 2..=6)
            ) {
                if let Some(chain) = build_ite_chain("__loop_0_iter", &constraints) {
                    let json = serde_json::to_string(&chain).unwrap();
                    let back: SymExpr = serde_json::from_str(&json).unwrap();
                    prop_assert_eq!(chain, back);
                }
            }

            /// Iteration bound is well-formed: And(Le(0, var), Lt(var, N)).
            #[test]
            fn iteration_bound_well_formed(n in 2u32..=20) {
                let bound = build_iteration_bound("__loop_0_iter", n);
                match &bound {
                    SymExpr::BinOp { op: BinOpKind::And, left, right } => {
                        match left.as_ref() {
                            SymExpr::BinOp { op: BinOpKind::Le, .. } => {}
                            other => prop_assert!(false, "expected Le, got {:?}", other),
                        }
                        match right.as_ref() {
                            SymExpr::BinOp { op: BinOpKind::Lt, right: upper, .. } => {
                                prop_assert_eq!(
                                    upper.as_ref(),
                                    &SymExpr::Const(ConstValue::Int(i64::from(n)))
                                );
                            }
                            other => prop_assert!(false, "expected Lt, got {:?}", other),
                        }
                    }
                    other => prop_assert!(false, "expected And, got {:?}", other),
                }
            }

            /// merge_loop_states never produces more constraints than the input.
            #[test]
            fn merge_output_not_larger_than_input(count in 2u32..=10) {
                let backedge = make_backedge_constraint("i", "n");
                let constraints: Vec<Option<SymExpr>> =
                    (0..count).map(|_| Some(backedge.clone())).collect();
                let loop_info = make_loop_info(0, "i");
                let result = make_execute_result_with_loops(&[(0, count)]);
                let merged = merge_loop_states(&constraints, &[loop_info], &result);
                prop_assert!(
                    merged.len() <= constraints.len(),
                    "merged ({}) should not exceed input ({})",
                    merged.len(),
                    constraints.len()
                );
            }
        }
    }
}
