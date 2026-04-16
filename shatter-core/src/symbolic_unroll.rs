//! Bounded symbolic-unroll template extraction from observed loop snapshots.
//!
//! This module turns `LoopBodyState` snapshots for a supported loop into a
//! compact template keyed by an iteration variable `k`. Scope stays narrow:
//!
//! - identifier locals only
//! - canonical counted loops
//! - direct linear induction variables
//! - one accumulator pattern: `local = local +/- induction_var`

use std::collections::BTreeMap;

use crate::protocol::{LoopBodyState, LoopInfo};
use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

/// Maximum number of snapshots accepted for one extracted template.
pub const MAX_TEMPLATE_ITERATIONS: usize = 32;
/// Maximum number of tracked locals accepted for one extracted template.
pub const MAX_TEMPLATE_LOCALS: usize = 8;
/// Minimum number of snapshots required to generalize a loop.
pub const MIN_TEMPLATE_ITERATIONS: usize = 2;
const TRIANGULAR_DIVISOR: i64 = 2;

/// Extracted template for one observed loop.
#[derive(Debug, Clone, PartialEq)]
pub struct IterationTemplate {
    /// Matches `LoopInfo.loop_id` and `LoopBodyState.loop_id`.
    pub loop_id: u32,
    /// Free iteration variable used by the closed-form formula.
    pub iteration_var: String,
    /// Number of observed iterations the template is bounded to.
    pub iteration_count: u32,
    /// Per-local update templates.
    pub locals: BTreeMap<String, LocalTemplate>,
}

/// Template for one tracked local across loop iterations.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalTemplate {
    /// Exact expression observed at iteration 0.
    pub initial_expr: SymExpr,
    /// Supported update shape for this local.
    pub update: StateUpdate,
}

/// Supported update patterns extracted from snapshots.
#[derive(Debug, Clone, PartialEq)]
pub enum StateUpdate {
    /// `expr(k) = init + step * k`
    Linear { step_expr: SymExpr },
    /// `expr(k+1) = expr(k) +/- induction(k)`
    Accumulator {
        source_local: String,
        operator: AccumulatorOp,
    },
}

/// Supported accumulator operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccumulatorOp {
    Add,
    Sub,
}

/// Closed-form symbolic formula derived from an iteration template.
#[derive(Debug, Clone, PartialEq)]
pub struct UnrolledFormula {
    /// Free iteration variable.
    pub iteration_var: String,
    /// Bounds the free iteration variable to observed iterations.
    pub iteration_bound: SymExpr,
    /// Closed-form symbolic state at iteration `k`.
    pub locals: BTreeMap<String, SymExpr>,
}

/// Deterministic template extraction failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateExtractionError {
    TooFewSnapshots {
        found: usize,
    },
    TooManySnapshots {
        found: usize,
        max: usize,
    },
    TooManyLocals {
        found: usize,
        max: usize,
    },
    MixedLoopIds {
        expected_loop_id: u32,
        found_loop_id: u32,
        iteration: u32,
    },
    NonConsecutiveIterations {
        expected: u32,
        found: u32,
    },
    IterationOutOfRange {
        requested: u32,
        iteration_count: u32,
    },
    MissingLocal {
        local: String,
        iteration: u32,
    },
    UnsupportedLocalPattern {
        local: String,
    },
    InvalidAccumulatorSource {
        local: String,
        source_local: String,
    },
}

/// Extract a bounded symbolic-unroll template from observed loop snapshots.
pub fn extract_iteration_template(
    loop_info: &LoopInfo,
    snapshots: &[LoopBodyState],
) -> Result<IterationTemplate, TemplateExtractionError> {
    validate_snapshots(loop_info.loop_id, snapshots)?;

    let first_snapshot = &snapshots[0];
    if first_snapshot.locals.len() > MAX_TEMPLATE_LOCALS {
        return Err(TemplateExtractionError::TooManyLocals {
            found: first_snapshot.locals.len(),
            max: MAX_TEMPLATE_LOCALS,
        });
    }

    let iteration_var = iteration_var_name(loop_info.loop_id);
    let mut locals = BTreeMap::new();
    let induction_name = loop_info.induction_var.name.clone();
    let induction_initial = required_local(first_snapshot, &induction_name)?;
    locals.insert(
        induction_name.clone(),
        LocalTemplate {
            initial_expr: induction_initial.clone(),
            update: StateUpdate::Linear {
                step_expr: loop_info.induction_var.step_expr.clone(),
            },
        },
    );

    let induction_values = collect_local_values(snapshots, &induction_name)?;

    for local_name in first_snapshot.locals.keys() {
        if local_name == &induction_name {
            continue;
        }

        let local_values = collect_local_values(snapshots, local_name)?;
        let initial_expr = local_values[0].clone();

        if all_equal(&local_values) {
            locals.insert(
                local_name.clone(),
                LocalTemplate {
                    initial_expr,
                    update: StateUpdate::Linear {
                        step_expr: zero_int_expr(),
                    },
                },
            );
            continue;
        }

        if matches_accumulator(&local_values, &induction_values, BinOpKind::Add) {
            locals.insert(
                local_name.clone(),
                LocalTemplate {
                    initial_expr,
                    update: StateUpdate::Accumulator {
                        source_local: induction_name.clone(),
                        operator: AccumulatorOp::Add,
                    },
                },
            );
            continue;
        }

        if matches_accumulator(&local_values, &induction_values, BinOpKind::Sub) {
            locals.insert(
                local_name.clone(),
                LocalTemplate {
                    initial_expr,
                    update: StateUpdate::Accumulator {
                        source_local: induction_name.clone(),
                        operator: AccumulatorOp::Sub,
                    },
                },
            );
            continue;
        }

        return Err(TemplateExtractionError::UnsupportedLocalPattern {
            local: local_name.clone(),
        });
    }

    Ok(IterationTemplate {
        loop_id: loop_info.loop_id,
        iteration_var,
        iteration_count: snapshots.len() as u32,
        locals,
    })
}

/// Build a bounded closed-form symbolic formula over the template's iteration variable.
pub fn build_unrolled_formula(
    template: &IterationTemplate,
) -> Result<UnrolledFormula, TemplateExtractionError> {
    let iteration_param = SymExpr::Param {
        name: template.iteration_var.clone(),
        path: vec![],
    };
    let iteration_bound = build_iteration_bound(&template.iteration_var, template.iteration_count);
    let mut locals = BTreeMap::new();

    for (name, local) in &template.locals {
        let expr = match &local.update {
            StateUpdate::Linear { step_expr } => build_linear_expr(
                local.initial_expr.clone(),
                step_expr.clone(),
                iteration_param.clone(),
            ),
            StateUpdate::Accumulator {
                source_local,
                operator,
            } => {
                let source = template.locals.get(source_local).ok_or_else(|| {
                    TemplateExtractionError::InvalidAccumulatorSource {
                        local: name.clone(),
                        source_local: source_local.clone(),
                    }
                })?;
                let source_step = match &source.update {
                    StateUpdate::Linear { step_expr } => step_expr.clone(),
                    StateUpdate::Accumulator { .. } => {
                        return Err(TemplateExtractionError::InvalidAccumulatorSource {
                            local: name.clone(),
                            source_local: source_local.clone(),
                        });
                    }
                };
                let source_initial = source.initial_expr.clone();
                build_accumulator_expr(
                    local.initial_expr.clone(),
                    source_initial,
                    source_step,
                    iteration_param.clone(),
                    *operator,
                )
            }
        };
        locals.insert(name.clone(), expr);
    }

    Ok(UnrolledFormula {
        iteration_var: template.iteration_var.clone(),
        iteration_bound,
        locals,
    })
}

/// Replay a template up to one observed iteration and rebuild the symbolic state.
pub fn materialize_iteration_state(
    template: &IterationTemplate,
    iteration: u32,
) -> Result<BTreeMap<String, SymExpr>, TemplateExtractionError> {
    if iteration >= template.iteration_count {
        return Err(TemplateExtractionError::IterationOutOfRange {
            requested: iteration,
            iteration_count: template.iteration_count,
        });
    }

    let mut state = BTreeMap::new();

    for (name, local) in &template.locals {
        state.insert(name.clone(), local.initial_expr.clone());
    }

    for current_iteration in 0..iteration {
        for (name, local) in &template.locals {
            if let StateUpdate::Accumulator {
                source_local,
                operator,
            } = &local.update
            {
                let previous = state
                    .get(name)
                    .cloned()
                    .expect("accumulator local must already be initialized");
                let source_value = state
                    .get(source_local)
                    .cloned()
                    .expect("accumulator source local must already be initialized");
                let combined = match operator {
                    AccumulatorOp::Add => binary_expr(BinOpKind::Add, previous, source_value),
                    AccumulatorOp::Sub => binary_expr(BinOpKind::Sub, previous, source_value),
                };
                state.insert(name.clone(), combined);
            }
        }

        let next_iteration = current_iteration + 1;
        for (name, local) in &template.locals {
            if let StateUpdate::Linear { step_expr } = &local.update {
                state.insert(
                    name.clone(),
                    simplify_int_expr(build_linear_expr(
                        local.initial_expr.clone(),
                        step_expr.clone(),
                        int_const(i64::from(next_iteration)),
                    )),
                );
            }
        }
    }

    Ok(state)
}

fn validate_snapshots(
    expected_loop_id: u32,
    snapshots: &[LoopBodyState],
) -> Result<(), TemplateExtractionError> {
    if snapshots.len() < MIN_TEMPLATE_ITERATIONS {
        return Err(TemplateExtractionError::TooFewSnapshots {
            found: snapshots.len(),
        });
    }
    if snapshots.len() > MAX_TEMPLATE_ITERATIONS {
        return Err(TemplateExtractionError::TooManySnapshots {
            found: snapshots.len(),
            max: MAX_TEMPLATE_ITERATIONS,
        });
    }

    for (expected_iteration, snapshot) in snapshots.iter().enumerate() {
        if snapshot.loop_id != expected_loop_id {
            return Err(TemplateExtractionError::MixedLoopIds {
                expected_loop_id,
                found_loop_id: snapshot.loop_id,
                iteration: snapshot.iteration,
            });
        }
        if snapshot.iteration != expected_iteration as u32 {
            return Err(TemplateExtractionError::NonConsecutiveIterations {
                expected: expected_iteration as u32,
                found: snapshot.iteration,
            });
        }
    }

    Ok(())
}

fn collect_local_values(
    snapshots: &[LoopBodyState],
    local: &str,
) -> Result<Vec<SymExpr>, TemplateExtractionError> {
    snapshots
        .iter()
        .map(|snapshot| required_local(snapshot, local))
        .collect()
}

fn required_local(
    snapshot: &LoopBodyState,
    local: &str,
) -> Result<SymExpr, TemplateExtractionError> {
    snapshot
        .locals
        .get(local)
        .cloned()
        .ok_or_else(|| TemplateExtractionError::MissingLocal {
            local: local.to_string(),
            iteration: snapshot.iteration,
        })
}

fn all_equal(values: &[SymExpr]) -> bool {
    values.windows(2).all(|window| window[0] == window[1])
}

fn matches_accumulator(
    local_values: &[SymExpr],
    induction_values: &[SymExpr],
    operator: BinOpKind,
) -> bool {
    if local_values.len() != induction_values.len() {
        return false;
    }

    for index in 1..local_values.len() {
        let expected = binary_expr(
            operator,
            local_values[index - 1].clone(),
            induction_values[index - 1].clone(),
        );
        if local_values[index] != expected {
            return false;
        }
    }

    true
}

fn iteration_var_name(loop_id: u32) -> String {
    format!("__loop_{loop_id}_iter")
}

fn build_iteration_bound(iter_var: &str, count: u32) -> SymExpr {
    let iter_param = SymExpr::Param {
        name: iter_var.to_string(),
        path: vec![],
    };
    binary_expr(
        BinOpKind::And,
        binary_expr(BinOpKind::Le, int_const(0), iter_param.clone()),
        binary_expr(BinOpKind::Lt, iter_param, int_const(i64::from(count))),
    )
}

fn build_linear_expr(
    initial_expr: SymExpr,
    step_expr: SymExpr,
    iteration_expr: SymExpr,
) -> SymExpr {
    binary_expr(
        BinOpKind::Add,
        initial_expr,
        binary_expr(BinOpKind::Mul, step_expr, iteration_expr),
    )
}

fn build_accumulator_expr(
    initial_expr: SymExpr,
    addend_initial_expr: SymExpr,
    addend_step_expr: SymExpr,
    iteration_expr: SymExpr,
    operator: AccumulatorOp,
) -> SymExpr {
    let scaled_initial = binary_expr(BinOpKind::Mul, iteration_expr.clone(), addend_initial_expr);
    let triangular_term = binary_expr(
        BinOpKind::Div,
        binary_expr(
            BinOpKind::Mul,
            binary_expr(
                BinOpKind::Mul,
                iteration_expr.clone(),
                binary_expr(BinOpKind::Sub, iteration_expr, int_const(1)),
            ),
            addend_step_expr,
        ),
        int_const(TRIANGULAR_DIVISOR),
    );

    let delta = binary_expr(BinOpKind::Add, scaled_initial, triangular_term);
    match operator {
        AccumulatorOp::Add => binary_expr(BinOpKind::Add, initial_expr, delta),
        AccumulatorOp::Sub => binary_expr(BinOpKind::Sub, initial_expr, delta),
    }
}

fn binary_expr(op: BinOpKind, left: SymExpr, right: SymExpr) -> SymExpr {
    SymExpr::BinOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn int_const(value: i64) -> SymExpr {
    SymExpr::Const(ConstValue::Int(value))
}

fn zero_int_expr() -> SymExpr {
    int_const(0)
}

fn simplify_int_expr(expr: SymExpr) -> SymExpr {
    match expr {
        SymExpr::BinOp { op, left, right } => {
            let left = simplify_int_expr(*left);
            let right = simplify_int_expr(*right);

            match (&left, &right) {
                (
                    SymExpr::Const(ConstValue::Int(left_int)),
                    SymExpr::Const(ConstValue::Int(right_int)),
                ) => match op {
                    BinOpKind::Add => int_const(left_int + right_int),
                    BinOpKind::Sub => int_const(left_int - right_int),
                    BinOpKind::Mul => int_const(left_int * right_int),
                    BinOpKind::Div if *right_int != 0 => int_const(left_int / right_int),
                    _ => SymExpr::BinOp {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                },
                _ => SymExpr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }
        }
        SymExpr::UnOp { op, operand } => SymExpr::UnOp {
            op,
            operand: Box::new(simplify_int_expr(*operand)),
        },
        SymExpr::Call {
            name,
            receiver,
            args,
        } => SymExpr::Call {
            name,
            receiver: receiver.map(|expr| Box::new(simplify_int_expr(*expr))),
            args: args.into_iter().map(simplify_int_expr).collect(),
        },
        SymExpr::Ite {
            condition,
            then_expr,
            else_expr,
        } => SymExpr::Ite {
            condition: Box::new(simplify_int_expr(*condition)),
            then_expr: Box::new(simplify_int_expr(*then_expr)),
            else_expr: Box::new(simplify_int_expr(*else_expr)),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BoundOp, InductionVar};

    fn make_loop_info(name: &str, init: i64, step: i64) -> LoopInfo {
        LoopInfo {
            loop_id: 0,
            line: 10,
            induction_var: InductionVar {
                name: name.to_string(),
                init_expr: int_const(init),
                step_expr: int_const(step),
                bound_expr: SymExpr::Param {
                    name: "n".into(),
                    path: vec![],
                },
                bound_op: BoundOp::Lt,
            },
        }
    }

    fn make_snapshot(iteration: u32, locals: &[(&str, SymExpr)]) -> LoopBodyState {
        LoopBodyState {
            loop_id: 0,
            iteration,
            locals: locals
                .iter()
                .map(|(name, expr)| ((*name).to_string(), expr.clone()))
                .collect(),
        }
    }

    fn const_loop_snapshots(iterations: u32, init: i64, step: i64) -> Vec<LoopBodyState> {
        (0..iterations)
            .map(|iteration| {
                let current = init + (step * i64::from(iteration));
                make_snapshot(iteration, &[("i", int_const(current))])
            })
            .collect()
    }

    fn accumulator_snapshots(
        iterations: u32,
        init: i64,
        step: i64,
        total_init: i64,
    ) -> Vec<LoopBodyState> {
        let mut snapshots = Vec::new();
        let mut total = int_const(total_init);

        for iteration in 0..iterations {
            let induction_value = int_const(init + (step * i64::from(iteration)));
            snapshots.push(make_snapshot(
                iteration,
                &[("i", induction_value.clone()), ("total", total.clone())],
            ));
            total = binary_expr(BinOpKind::Add, total, induction_value);
        }

        snapshots
    }

    #[test]
    fn extracts_counted_loop_template() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = const_loop_snapshots(3, 0, 1);

        let template =
            extract_iteration_template(&loop_info, &snapshots).expect("extract template");

        assert_eq!(template.iteration_count, 3);
        assert_eq!(template.iteration_var, "__loop_0_iter");
        assert_eq!(
            template.locals["i"],
            LocalTemplate {
                initial_expr: int_const(0),
                update: StateUpdate::Linear {
                    step_expr: int_const(1),
                },
            }
        );
    }

    #[test]
    fn extracts_accumulator_template() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = accumulator_snapshots(3, 0, 1, 0);

        let template =
            extract_iteration_template(&loop_info, &snapshots).expect("extract template");

        assert_eq!(
            template.locals["total"],
            LocalTemplate {
                initial_expr: int_const(0),
                update: StateUpdate::Accumulator {
                    source_local: "i".into(),
                    operator: AccumulatorOp::Add,
                },
            }
        );
    }

    #[test]
    fn rejects_unsupported_local_pattern() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = vec![
            make_snapshot(0, &[("i", int_const(0)), ("total", int_const(0))]),
            make_snapshot(1, &[("i", int_const(1)), ("total", int_const(1))]),
            make_snapshot(2, &[("i", int_const(2)), ("total", int_const(4))]),
        ];

        let err = extract_iteration_template(&loop_info, &snapshots).expect_err("should reject");

        assert_eq!(
            err,
            TemplateExtractionError::UnsupportedLocalPattern {
                local: "total".into(),
            }
        );
    }

    #[test]
    fn rejects_non_consecutive_iterations() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = vec![
            make_snapshot(0, &[("i", int_const(0))]),
            make_snapshot(2, &[("i", int_const(2))]),
        ];

        let err = extract_iteration_template(&loop_info, &snapshots).expect_err("should reject");

        assert_eq!(
            err,
            TemplateExtractionError::NonConsecutiveIterations {
                expected: 1,
                found: 2,
            }
        );
    }

    #[test]
    fn builds_bounded_formula() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = accumulator_snapshots(3, 0, 1, 0);
        let template =
            extract_iteration_template(&loop_info, &snapshots).expect("extract template");

        let formula = build_unrolled_formula(&template).expect("build formula");

        assert_eq!(formula.iteration_var, "__loop_0_iter");
        assert!(formula.locals.contains_key("i"));
        assert!(formula.locals.contains_key("total"));
    }

    #[test]
    fn materialized_state_matches_observed_snapshots() {
        let loop_info = make_loop_info("i", 0, 1);
        let snapshots = accumulator_snapshots(4, 0, 1, 0);
        let template =
            extract_iteration_template(&loop_info, &snapshots).expect("extract template");

        for snapshot in &snapshots {
            let materialized =
                materialize_iteration_state(&template, snapshot.iteration).expect("materialize");
            assert_eq!(materialized, snapshot.locals);
        }
    }

    mod proptests {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #[test]
            fn counted_loop_template_replays_observed_snapshots(
                init in -5i64..=5,
                step in -3i64..=3,
                iterations in 2u32..=6,
            ) {
                prop_assume!(step != 0);
                let loop_info = make_loop_info("i", init, step);
                let snapshots = const_loop_snapshots(iterations, init, step);
                let template = extract_iteration_template(&loop_info, &snapshots).expect("extract template");

                for snapshot in &snapshots {
                    let materialized =
                        materialize_iteration_state(&template, snapshot.iteration).expect("materialize");
                    prop_assert_eq!(materialized, snapshot.locals.clone());
                }
            }

            #[test]
            fn accumulator_template_replays_observed_snapshots(
                init in -5i64..=5,
                step in 1i64..=3,
                total_init in -5i64..=5,
                iterations in 2u32..=6,
            ) {
                let loop_info = make_loop_info("i", init, step);
                let snapshots = accumulator_snapshots(iterations, init, step, total_init);
                let template = extract_iteration_template(&loop_info, &snapshots).expect("extract template");

                for snapshot in &snapshots {
                    let materialized =
                        materialize_iteration_state(&template, snapshot.iteration).expect("materialize");
                    prop_assert_eq!(materialized, snapshot.locals.clone());
                }
            }
        }
    }
}
