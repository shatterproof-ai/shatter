//! Loop analysis: induction variable detection and constraint rewriting.
//!
//! Canonical counted loops (e.g., `for (i = 0; i < n; i++)`) produce O(k) redundant
//! backedge constraints per loop iteration. This module detects such loops from
//! [`LoopInfo`] metadata (populated by frontends during analysis) and rewrites
//! the constraint vector to collapse backedge constraints into O(1) direct
//! induction variable constraints.

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

    mod proptests {
        use proptest::prelude::*;

        use crate::test_arbitraries::{arb_induction_var, arb_loop_info};

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
        }
    }
}
