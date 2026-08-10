//! Parameter drilling: focused mutation of blocking parameters on stalled frontiers.
//!
//! When the Z3 solver returns Unsat/Error for a branch and naive fuzzing hasn't
//! flipped it, the branch stalls. Drilling pins non-blocking parameters at their
//! best-known values and applies intensive type-aware mutation only to the
//! parameter(s) appearing in the branch's symbolic constraint.

use rand::Rng;
use serde_json::Value;

use crate::execution_record::SymConstraint;
use crate::input_gen::mutate_value;
use crate::sym_expr::extract_param_names;
use crate::types::ParamInfo;

/// Number of drilled mutations generated per blocking parameter per frontier.
pub const DRILL_MUTATIONS_PER_PARAM: usize = 4;

/// Stall count threshold before drilling activates for a frontier.
pub const DRILL_STALL_THRESHOLD: u32 = 2;

/// Maximum number of stalled frontiers to drill per exploration round.
pub const MAX_FRONTIERS_PER_ROUND: usize = 3;

/// Map parameter names from a branch constraint to their indices in `param_infos`.
///
/// For `SymConstraint::Expr`, walks the symbolic expression tree to extract
/// referenced parameter names, then resolves each to its positional index.
/// Returns an empty vec for `Unknown` constraints (no symbolic info available).
pub fn identify_blocking_params(
    constraint: &SymConstraint,
    param_infos: &[ParamInfo],
) -> Vec<usize> {
    let expr = match constraint {
        SymConstraint::Expr { expr } => expr,
        SymConstraint::Unknown { .. } => return vec![],
    };

    let names = extract_param_names(expr);
    let mut indices: Vec<usize> = names
        .iter()
        .filter_map(|name| param_infos.iter().position(|p| p.name == *name))
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Generate drilled inputs: pin non-blocking params at `best_prefix` values,
/// intensively mutate only the blocking parameter(s).
///
/// If `blocking_params` is empty, falls back to mutating all parameters.
/// Returns up to `count` mutated input vectors.
pub fn generate_drilled_inputs(
    best_prefix: &[Value],
    blocking_params: &[usize],
    param_infos: &[ParamInfo],
    count: usize,
    rng: &mut impl Rng,
) -> Vec<Vec<Value>> {
    if best_prefix.is_empty() || param_infos.is_empty() {
        return vec![];
    }

    let targets: Vec<usize> = if blocking_params.is_empty() {
        // No blocking info — mutate all params as fallback.
        (0..best_prefix.len().min(param_infos.len())).collect()
    } else {
        blocking_params
            .iter()
            .copied()
            .filter(|&i| i < best_prefix.len() && i < param_infos.len())
            .collect()
    };

    if targets.is_empty() {
        return vec![];
    }

    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let mut inputs = best_prefix.to_vec();
        for &idx in &targets {
            inputs[idx] = mutate_value(&best_prefix[idx], &param_infos[idx].typ, &[], rng);
        }
        results.push(inputs);
    }
    results
}

/// Resolve the depth (position in branch path) of a branch_id from an
/// execution's branch_path. Returns 0 if not found.
pub fn branch_depth(
    branch_path: &[crate::execution_record::BranchDecision],
    branch_id: u32,
) -> u32 {
    branch_path
        .iter()
        .position(|d| d.branch_id == branch_id)
        .map_or(0, |i| i as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym_expr::{BinOpKind, SymExpr};
    use crate::types::TypeInfo;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use serde_json::json;

    fn make_param(name: &str, typ: TypeInfo) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ,
            type_name: None,
        }
    }

    #[test]
    fn blocking_params_from_single_param_expr() {
        let constraint = SymConstraint::Expr {
            expr: SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(crate::sym_expr::ConstValue::Int(5))),
            },
        };
        let params = vec![
            make_param("x", TypeInfo::Int { int_width: None, int_signed: None }),
            make_param("y", TypeInfo::Str),
        ];
        assert_eq!(identify_blocking_params(&constraint, &params), vec![0]);
    }

    #[test]
    fn blocking_params_from_multi_param_expr() {
        let constraint = SymConstraint::Expr {
            expr: SymExpr::BinOp {
                op: BinOpKind::And,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Param {
                    name: "y".into(),
                    path: vec![],
                }),
            },
        };
        let params = vec![
            make_param("x", TypeInfo::Int { int_width: None, int_signed: None }),
            make_param("y", TypeInfo::Bool),
            make_param("z", TypeInfo::Str),
        ];
        assert_eq!(identify_blocking_params(&constraint, &params), vec![0, 1]);
    }

    #[test]
    fn blocking_params_unknown_returns_empty() {
        let constraint = SymConstraint::Unknown {
            hint: "opaque".into(),
        };
        let params = vec![make_param("x", TypeInfo::Int { int_width: None, int_signed: None })];
        assert_eq!(
            identify_blocking_params(&constraint, &params),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn blocking_params_unresolvable_name_skipped() {
        let constraint = SymConstraint::Expr {
            expr: SymExpr::Param {
                name: "unknown_param".into(),
                path: vec![],
            },
        };
        let params = vec![make_param("x", TypeInfo::Int { int_width: None, int_signed: None })];
        assert_eq!(
            identify_blocking_params(&constraint, &params),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn drilled_inputs_pins_non_blocking() {
        let mut rng = StdRng::seed_from_u64(42);
        let best_prefix = vec![json!(10), json!("hello"), json!(true)];
        let params = vec![
            make_param("a", TypeInfo::Int { int_width: None, int_signed: None }),
            make_param("b", TypeInfo::Str),
            make_param("c", TypeInfo::Bool),
        ];
        let blocking = vec![1]; // only "b" is blocking

        let results = generate_drilled_inputs(&best_prefix, &blocking, &params, 8, &mut rng);

        assert_eq!(results.len(), 8);
        for inputs in &results {
            // Non-blocking params must be pinned at original values.
            assert_eq!(inputs[0], json!(10), "param 'a' should be pinned");
            assert_eq!(inputs[2], json!(true), "param 'c' should be pinned");
        }
    }

    #[test]
    fn drilled_inputs_empty_blocking_mutates_all() {
        let mut rng = StdRng::seed_from_u64(99);
        let best_prefix = vec![json!(5), json!("test")];
        let params = vec![
            make_param("a", TypeInfo::Int { int_width: None, int_signed: None }),
            make_param("b", TypeInfo::Str),
        ];

        let results = generate_drilled_inputs(&best_prefix, &[], &params, 4, &mut rng);
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn drilled_inputs_empty_prefix_returns_empty() {
        let mut rng = StdRng::seed_from_u64(1);
        let results = generate_drilled_inputs(&[], &[0], &[], 4, &mut rng);
        assert!(results.is_empty());
    }

    #[test]
    fn drilled_inputs_out_of_bounds_index_skipped() {
        let mut rng = StdRng::seed_from_u64(1);
        let best_prefix = vec![json!(1)];
        let params = vec![make_param("a", TypeInfo::Int { int_width: None, int_signed: None })];
        // blocking index 5 is out of bounds
        let results = generate_drilled_inputs(&best_prefix, &[5], &params, 4, &mut rng);
        assert!(results.is_empty());
    }

    #[test]
    fn drilled_inputs_multiple_blocking_params() {
        let mut rng = StdRng::seed_from_u64(77);
        let best_prefix = vec![json!(1), json!("a"), json!(false)];
        let params = vec![
            make_param("x", TypeInfo::Int { int_width: None, int_signed: None }),
            make_param("y", TypeInfo::Str),
            make_param("z", TypeInfo::Bool),
        ];
        let blocking = vec![0, 2]; // x and z are blocking

        let results = generate_drilled_inputs(&best_prefix, &blocking, &params, 6, &mut rng);
        assert_eq!(results.len(), 6);
        for inputs in &results {
            // Only param 1 ("y") should be pinned.
            assert_eq!(inputs[1], json!("a"), "param 'y' should be pinned");
        }
    }
}
