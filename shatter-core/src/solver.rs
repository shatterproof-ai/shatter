//! Z3-based constraint solver for concolic execution.
//!
//! Converts [`SymExpr`] trees into Z3 AST nodes, solves for new execution paths
//! by negating branch constraints, and extracts concrete values from Z3 models.

use std::collections::HashMap;
use std::str::FromStr;

use z3::ast::{Bool, Int, Real, String as Z3String};
use z3::{Config, SatResult, Solver};

use crate::sym_expr::{BinOpKind, ConstValue, SymExpr, UnOpKind};

/// Errors that can occur during constraint solving.
#[derive(Debug, thiserror::Error)]
pub enum SolverError {
    #[error("unsupported expression: {0}")]
    Unsupported(String),

    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("constraints are unsatisfiable")]
    Unsat,

    #[error("solver returned unknown: {0}")]
    Unknown(String),
}

/// A concrete value extracted from a Z3 model.
#[derive(Debug, Clone, PartialEq)]
pub enum ConcreteValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Complex {
        kind: crate::types::ComplexKind,
        repr: Box<ConcreteValue>,
    },
}

/// Result of attempting to solve for a new execution path.
#[derive(Debug)]
pub enum SolveResult {
    /// Found concrete values that satisfy the negated path.
    Sat(HashMap<String, ConcreteValue>),
    /// The negated path is unsatisfiable — no inputs can reach it.
    Unsat,
}

/// The Z3 sort a symbolic variable was declared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Int,
    Real,
    Bool,
    Str,
}

/// Intermediate Z3 AST representation that carries sort information.
enum Z3Ast {
    Int(Int),
    Real(Real),
    Bool(Bool),
    Str(Z3String),
}

/// Tracks declared Z3 variables and their sorts so we can extract values later.
struct VarTable {
    ints: HashMap<String, Int>,
    reals: HashMap<String, Real>,
    bools: HashMap<String, Bool>,
    strings: HashMap<String, Z3String>,
}

impl VarTable {
    fn new() -> Self {
        Self {
            ints: HashMap::new(),
            reals: HashMap::new(),
            bools: HashMap::new(),
            strings: HashMap::new(),
        }
    }

    fn get_or_create_int(&mut self, name: &str) -> Int {
        self.ints
            .entry(name.to_owned())
            .or_insert_with(|| Int::new_const(name))
            .clone()
    }

    fn get_or_create_real(&mut self, name: &str) -> Real {
        self.reals
            .entry(name.to_owned())
            .or_insert_with(|| Real::new_const(name))
            .clone()
    }

    fn get_or_create_bool(&mut self, name: &str) -> Bool {
        self.bools
            .entry(name.to_owned())
            .or_insert_with(|| Bool::new_const(name))
            .clone()
    }

    fn get_or_create_string(&mut self, name: &str) -> Z3String {
        self.strings
            .entry(name.to_owned())
            .or_insert_with(|| Z3String::new_const(name))
            .clone()
    }
}

/// Infer the sort a `SymExpr` should have, based on the constants and operators it contains.
fn infer_sort(expr: &SymExpr) -> Sort {
    match expr {
        SymExpr::Const(ConstValue::Int(_)) => Sort::Int,
        SymExpr::Const(ConstValue::Float(_)) => Sort::Real,
        SymExpr::Const(ConstValue::Bool(_)) => Sort::Bool,
        SymExpr::Const(ConstValue::Str(_)) => Sort::Str,
        SymExpr::BinOp { op, left, right } => match op {
            BinOpKind::And | BinOpKind::Or => Sort::Bool,
            BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt
            | BinOpKind::Ge => {
                let l = infer_sort(left);
                if l != Sort::Int && l != Sort::Bool {
                    l
                } else {
                    let r = infer_sort(right);
                    if r != Sort::Int {
                        r
                    } else {
                        l
                    }
                }
            }
            BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div
            | BinOpKind::Mod => {
                let l = infer_sort(left);
                if l == Sort::Real {
                    return Sort::Real;
                }
                let r = infer_sort(right);
                if r == Sort::Real {
                    Sort::Real
                } else {
                    Sort::Int
                }
            }
            _ => Sort::Int,
        },
        SymExpr::UnOp { op, operand } => match op {
            UnOpKind::Not => Sort::Bool,
            UnOpKind::Neg | UnOpKind::BitwiseNot => infer_sort(operand),
            UnOpKind::TypeOf => Sort::Str,
        },
        SymExpr::Param { .. } | SymExpr::Call { .. } | SymExpr::Unknown => Sort::Int,
        SymExpr::Const(ConstValue::Null | ConstValue::Undefined) => Sort::Int,
        // Complex constants unwrap to their repr's sort
        SymExpr::Const(ConstValue::Complex { repr, .. }) => {
            infer_sort(&SymExpr::Const(*repr.clone()))
        }
    }
}

/// Infer the sort that a param in a comparison should use, by looking at the other operand.
fn infer_operand_sort(expr: &SymExpr) -> Sort {
    match expr {
        SymExpr::BinOp {
            op:
                BinOpKind::Eq
                | BinOpKind::Ne
                | BinOpKind::Lt
                | BinOpKind::Le
                | BinOpKind::Gt
                | BinOpKind::Ge,
            left,
            right,
        } => {
            let l = infer_sort(left);
            let r = infer_sort(right);
            if l != Sort::Int {
                l
            } else {
                r
            }
        }
        _ => infer_sort(expr),
    }
}

/// Flatten a param name with its path into a single Z3 variable name.
fn param_var_name(name: &str, path: &[String]) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{}.{}", name, path.join("."))
    }
}

/// Convert a `SymExpr` into a Z3 AST node, declaring variables as needed.
///
/// `hint_sort` is used to determine the sort of `Param` nodes when no other
/// information is available.
///
/// Must be called within a `z3::with_z3_config` block or with the default
/// thread-local Z3 context active.
fn to_z3_expr(
    vars: &mut VarTable,
    expr: &SymExpr,
    hint_sort: Sort,
) -> Result<Z3Ast, SolverError> {
    match expr {
        SymExpr::Param { name, path } => {
            let var_name = param_var_name(name, path);
            match hint_sort {
                Sort::Int => Ok(Z3Ast::Int(vars.get_or_create_int(&var_name))),
                Sort::Real => Ok(Z3Ast::Real(vars.get_or_create_real(&var_name))),
                Sort::Bool => Ok(Z3Ast::Bool(vars.get_or_create_bool(&var_name))),
                Sort::Str => Ok(Z3Ast::Str(vars.get_or_create_string(&var_name))),
            }
        }

        SymExpr::Const(c) => match c {
            ConstValue::Int(v) => Ok(Z3Ast::Int(Int::from_i64(*v))),
            ConstValue::Float(v) => {
                let scaled = (*v * 1_000_000.0).round() as i64;
                Ok(Z3Ast::Real(Real::from_rational(scaled, 1_000_000)))
            }
            ConstValue::Str(s) => Ok(Z3Ast::Str(Z3String::from_str(s).map_err(|_| {
                SolverError::Unsupported("failed to create Z3 string constant".into())
            })?)),
            ConstValue::Bool(b) => Ok(Z3Ast::Bool(Bool::from_bool(*b))),
            ConstValue::Null | ConstValue::Undefined => Ok(Z3Ast::Int(Int::from_i64(0))),
            // Complex constants unwrap to their repr for solving
            ConstValue::Complex { repr, .. } => {
                let unwrapped = SymExpr::Const(*repr.clone());
                to_z3_expr(vars, &unwrapped, hint_sort)
            }
        },

        SymExpr::BinOp { op, left, right } => convert_binop(vars, *op, left, right, hint_sort),

        SymExpr::UnOp { op, operand } => convert_unop(vars, *op, operand, hint_sort),

        SymExpr::Call { name, .. } => Err(SolverError::Unsupported(format!(
            "function call '{name}' cannot be represented in Z3"
        ))),

        SymExpr::Unknown => Err(SolverError::Unsupported(
            "unknown expression cannot be represented in Z3".into(),
        )),
    }
}

fn convert_binop(
    vars: &mut VarTable,
    op: BinOpKind,
    left: &SymExpr,
    right: &SymExpr,
    hint_sort: Sort,
) -> Result<Z3Ast, SolverError> {
    match op {
        BinOpKind::And => {
            let l = to_z3_bool(vars, left)?;
            let r = to_z3_bool(vars, right)?;
            Ok(Z3Ast::Bool(Bool::and(&[&l, &r])))
        }
        BinOpKind::Or => {
            let l = to_z3_bool(vars, left)?;
            let r = to_z3_bool(vars, right)?;
            Ok(Z3Ast::Bool(Bool::or(&[&l, &r])))
        }

        BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt
        | BinOpKind::Ge => {
            let operand_sort = infer_comparison_sort(left, right, hint_sort);
            convert_comparison(vars, op, left, right, operand_sort)
        }

        BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Mod => {
            let arith_sort = infer_arithmetic_sort(left, right, hint_sort);
            convert_arithmetic(vars, op, left, right, arith_sort)
        }

        BinOpKind::BitwiseAnd | BinOpKind::BitwiseOr | BinOpKind::BitwiseXor
        | BinOpKind::Shl | BinOpKind::Shr | BinOpKind::BitClear => {
            Err(SolverError::Unsupported(format!(
                "bitwise operator {op:?} not yet supported in Z3 solver"
            )))
        }

        BinOpKind::In | BinOpKind::InstanceOf => Err(SolverError::Unsupported(format!(
            "JS operator {op:?} not supported in Z3 solver"
        ))),
    }
}

fn infer_comparison_sort(left: &SymExpr, right: &SymExpr, _hint: Sort) -> Sort {
    let l = infer_sort(left);
    let r = infer_sort(right);
    if l != Sort::Int {
        l
    } else if r != Sort::Int {
        r
    } else {
        // Default to Int for comparisons — the hint from outer context (e.g. Bool from And)
        // should not affect what sort the comparison operands use.
        Sort::Int
    }
}

fn infer_arithmetic_sort(left: &SymExpr, right: &SymExpr, hint: Sort) -> Sort {
    let l = infer_sort(left);
    let r = infer_sort(right);
    if l == Sort::Real || r == Sort::Real || hint == Sort::Real {
        Sort::Real
    } else {
        Sort::Int
    }
}

fn convert_comparison(
    vars: &mut VarTable,
    op: BinOpKind,
    left: &SymExpr,
    right: &SymExpr,
    sort: Sort,
) -> Result<Z3Ast, SolverError> {
    match sort {
        Sort::Int => {
            let l = to_z3_int(vars, left)?;
            let r = to_z3_int(vars, right)?;
            Ok(Z3Ast::Bool(match op {
                BinOpKind::Eq => l.eq(&r),
                BinOpKind::Ne => l.eq(&r).not(),
                BinOpKind::Lt => l.lt(&r),
                BinOpKind::Le => l.le(&r),
                BinOpKind::Gt => l.gt(&r),
                BinOpKind::Ge => l.ge(&r),
                _ => unreachable!(),
            }))
        }
        Sort::Real => {
            let l = to_z3_real(vars, left)?;
            let r = to_z3_real(vars, right)?;
            Ok(Z3Ast::Bool(match op {
                BinOpKind::Eq => l.eq(&r),
                BinOpKind::Ne => l.eq(&r).not(),
                BinOpKind::Lt => l.lt(&r),
                BinOpKind::Le => l.le(&r),
                BinOpKind::Gt => l.gt(&r),
                BinOpKind::Ge => l.ge(&r),
                _ => unreachable!(),
            }))
        }
        Sort::Bool => {
            let l = to_z3_bool(vars, left)?;
            let r = to_z3_bool(vars, right)?;
            Ok(Z3Ast::Bool(match op {
                BinOpKind::Eq => l.eq(&r),
                BinOpKind::Ne => l.eq(&r).not(),
                _ => {
                    return Err(SolverError::Unsupported(format!(
                        "comparison {op:?} not supported on booleans"
                    )))
                }
            }))
        }
        Sort::Str => {
            let l = to_z3_string(vars, left)?;
            let r = to_z3_string(vars, right)?;
            Ok(Z3Ast::Bool(match op {
                BinOpKind::Eq => l.eq(&r),
                BinOpKind::Ne => l.eq(&r).not(),
                BinOpKind::Lt => l.str_lt(&r),
                BinOpKind::Le => l.str_le(&r),
                BinOpKind::Gt => l.str_gt(&r),
                BinOpKind::Ge => l.str_ge(&r),
                _ => unreachable!(),
            }))
        }
    }
}

fn convert_arithmetic(
    vars: &mut VarTable,
    op: BinOpKind,
    left: &SymExpr,
    right: &SymExpr,
    sort: Sort,
) -> Result<Z3Ast, SolverError> {
    match sort {
        Sort::Int => {
            let l = to_z3_int(vars, left)?;
            let r = to_z3_int(vars, right)?;
            Ok(Z3Ast::Int(match op {
                BinOpKind::Add => Int::add(&[&l, &r]),
                BinOpKind::Sub => Int::sub(&[&l, &r]),
                BinOpKind::Mul => Int::mul(&[&l, &r]),
                BinOpKind::Div => l.div(&r),
                BinOpKind::Mod => l.modulo(&r),
                _ => unreachable!(),
            }))
        }
        Sort::Real => {
            let l = to_z3_real(vars, left)?;
            let r = to_z3_real(vars, right)?;
            Ok(Z3Ast::Real(match op {
                BinOpKind::Add => Real::add(&[&l, &r]),
                BinOpKind::Sub => Real::sub(&[&l, &r]),
                BinOpKind::Mul => Real::mul(&[&l, &r]),
                BinOpKind::Div => l.div(&r),
                BinOpKind::Mod => {
                    return Err(SolverError::Unsupported(
                        "modulo not supported on reals".into(),
                    ))
                }
                _ => unreachable!(),
            }))
        }
        _ => Err(SolverError::Unsupported(format!(
            "arithmetic not supported on {sort:?} values"
        ))),
    }
}

fn convert_unop(
    vars: &mut VarTable,
    op: UnOpKind,
    operand: &SymExpr,
    hint_sort: Sort,
) -> Result<Z3Ast, SolverError> {
    match op {
        UnOpKind::Not => {
            let inner = to_z3_bool(vars, operand)?;
            Ok(Z3Ast::Bool(inner.not()))
        }
        UnOpKind::Neg => {
            let sort = infer_sort(operand);
            if sort == Sort::Real || hint_sort == Sort::Real {
                let inner = to_z3_real(vars, operand)?;
                Ok(Z3Ast::Real(inner.unary_minus()))
            } else {
                let inner = to_z3_int(vars, operand)?;
                Ok(Z3Ast::Int(inner.unary_minus()))
            }
        }
        UnOpKind::BitwiseNot => {
            // Approximate bitwise NOT (~x) as -(x + 1), which is equivalent for
            // two's complement integers. This avoids needing Z3 bit-vectors.
            let inner = to_z3_int(vars, operand)?;
            let one = Int::from_i64(1);
            let plus_one = Int::add(&[&inner, &one]);
            Ok(Z3Ast::Int(plus_one.unary_minus()))
        }
        UnOpKind::TypeOf => {
            // Return a string constant based on the inferred Z3 sort of the operand.
            let sort = infer_sort(operand);
            let type_name = match sort {
                Sort::Int | Sort::Real => "number",
                Sort::Bool => "boolean",
                Sort::Str => "string",
            };
            Ok(Z3Ast::Str(Z3String::from_str(type_name).map_err(|_| {
                SolverError::Unsupported("failed to create Z3 string for typeof".into())
            })?))
        }
    }
}

// ── Coercion helpers ─────────────────────────────────────────────────────────

fn to_z3_int(vars: &mut VarTable, expr: &SymExpr) -> Result<Int, SolverError> {
    match to_z3_expr(vars, expr, Sort::Int)? {
        Z3Ast::Int(i) => Ok(i),
        Z3Ast::Real(_) => Err(SolverError::TypeMismatch {
            expected: "Int".into(),
            actual: "Real".into(),
        }),
        Z3Ast::Bool(_) => Err(SolverError::TypeMismatch {
            expected: "Int".into(),
            actual: "Bool".into(),
        }),
        Z3Ast::Str(_) => Err(SolverError::TypeMismatch {
            expected: "Int".into(),
            actual: "Str".into(),
        }),
    }
}

fn to_z3_real(vars: &mut VarTable, expr: &SymExpr) -> Result<Real, SolverError> {
    match to_z3_expr(vars, expr, Sort::Real)? {
        Z3Ast::Real(r) => Ok(r),
        Z3Ast::Int(i) => Ok(Real::from_int(&i)),
        Z3Ast::Bool(_) => Err(SolverError::TypeMismatch {
            expected: "Real".into(),
            actual: "Bool".into(),
        }),
        Z3Ast::Str(_) => Err(SolverError::TypeMismatch {
            expected: "Real".into(),
            actual: "Str".into(),
        }),
    }
}

fn to_z3_bool(vars: &mut VarTable, expr: &SymExpr) -> Result<Bool, SolverError> {
    match to_z3_expr(vars, expr, Sort::Bool)? {
        Z3Ast::Bool(b) => Ok(b),
        Z3Ast::Int(_) => Err(SolverError::TypeMismatch {
            expected: "Bool".into(),
            actual: "Int".into(),
        }),
        Z3Ast::Real(_) => Err(SolverError::TypeMismatch {
            expected: "Bool".into(),
            actual: "Real".into(),
        }),
        Z3Ast::Str(_) => Err(SolverError::TypeMismatch {
            expected: "Bool".into(),
            actual: "Str".into(),
        }),
    }
}

fn to_z3_string(vars: &mut VarTable, expr: &SymExpr) -> Result<Z3String, SolverError> {
    match to_z3_expr(vars, expr, Sort::Str)? {
        Z3Ast::Str(s) => Ok(s),
        Z3Ast::Int(_) => Err(SolverError::TypeMismatch {
            expected: "Str".into(),
            actual: "Int".into(),
        }),
        Z3Ast::Real(_) => Err(SolverError::TypeMismatch {
            expected: "Str".into(),
            actual: "Real".into(),
        }),
        Z3Ast::Bool(_) => Err(SolverError::TypeMismatch {
            expected: "Str".into(),
            actual: "Bool".into(),
        }),
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Extract concrete values for all declared variables from a Z3 model.
fn extract_concrete_values(
    model: &z3::Model,
    vars: &VarTable,
) -> HashMap<String, ConcreteValue> {
    let mut result = HashMap::new();

    for (name, ast) in &vars.ints {
        if let Some(val) = model.eval(ast, true)
            && let Some(i) = val.as_i64()
        {
            result.insert(name.clone(), ConcreteValue::Int(i));
        }
    }

    for (name, ast) in &vars.reals {
        if let Some(val) = model.eval(ast, true) {
            if let Some((num, den)) = val.as_rational() {
                if den != 0 {
                    result.insert(
                        name.clone(),
                        ConcreteValue::Float(num as f64 / den as f64),
                    );
                }
            } else {
                let s = val.to_string();
                if let Ok(f) = s.parse::<f64>() {
                    result.insert(name.clone(), ConcreteValue::Float(f));
                }
            }
        }
    }

    for (name, ast) in &vars.bools {
        if let Some(val) = model.eval(ast, true)
            && let Some(b) = val.as_bool()
        {
            result.insert(name.clone(), ConcreteValue::Bool(b));
        }
    }

    for (name, ast) in &vars.strings {
        if let Some(val) = model.eval(ast, true)
            && let Some(s) = val.as_string()
        {
            result.insert(name.clone(), ConcreteValue::Str(s));
        }
    }

    result
}

/// Solve a list of path constraints, negating the constraint at `negate_index`
/// to explore a new execution path.
///
/// All constraints before `negate_index` are asserted as-is (the prefix), and
/// the constraint at `negate_index` is negated. This is the standard concolic
/// strategy: keep the path prefix and flip the last branch.
///
/// Returns `SolveResult::Sat` with concrete variable assignments if satisfiable,
/// or `SolveResult::Unsat` if no inputs can reach the negated path.
pub fn solve_for_new_path(
    constraints: &[SymExpr],
    negate_index: usize,
    solver_timeout_ms: Option<u64>,
) -> Result<SolveResult, SolverError> {
    if negate_index >= constraints.len() {
        return Err(SolverError::Unsupported(format!(
            "negate_index {negate_index} out of bounds (len={})",
            constraints.len()
        )));
    }

    let mut cfg = Config::new();
    if let Some(ms) = solver_timeout_ms {
        cfg.set_timeout_msec(ms);
    }
    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new();

        // Assert the prefix constraints as-is.
        for constraint in &constraints[..negate_index] {
            let sort = infer_operand_sort(constraint);
            let bool_expr = to_z3_bool_constraint(&mut vars, constraint, sort)?;
            solver.assert(&bool_expr);
        }

        // Negate the target constraint.
        let target = &constraints[negate_index];
        let sort = infer_operand_sort(target);
        let target_bool = to_z3_bool_constraint(&mut vars, target, sort)?;
        solver.assert(target_bool.not());

        check_and_extract(&solver, &vars)
    })
}

/// Solve a set of constraints directly (without negation).
///
/// Useful for checking satisfiability of a complete constraint set.
pub fn solve_constraints(
    constraints: &[SymExpr],
    solver_timeout_ms: Option<u64>,
) -> Result<SolveResult, SolverError> {
    let mut cfg = Config::new();
    if let Some(ms) = solver_timeout_ms {
        cfg.set_timeout_msec(ms);
    }
    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new();

        for constraint in constraints {
            let sort = infer_operand_sort(constraint);
            let bool_expr = to_z3_bool_constraint(&mut vars, constraint, sort)?;
            solver.assert(&bool_expr);
        }

        check_and_extract(&solver, &vars)
    })
}

fn to_z3_bool_constraint(
    vars: &mut VarTable,
    expr: &SymExpr,
    sort: Sort,
) -> Result<Bool, SolverError> {
    let z3_expr = to_z3_expr(vars, expr, sort)?;
    match z3_expr {
        Z3Ast::Bool(b) => Ok(b),
        _ => Err(SolverError::TypeMismatch {
            expected: "Bool".into(),
            actual: "non-Bool constraint".into(),
        }),
    }
}

fn check_and_extract(solver: &Solver, vars: &VarTable) -> Result<SolveResult, SolverError> {
    match solver.check() {
        SatResult::Sat => {
            let model = solver.get_model().ok_or_else(|| {
                SolverError::Unknown("solver returned sat but no model available".into())
            })?;
            let values = extract_concrete_values(&model, vars);
            Ok(SolveResult::Sat(values))
        }
        SatResult::Unsat => Ok(SolveResult::Unsat),
        SatResult::Unknown => Err(SolverError::Unknown(
            solver
                .get_reason_unknown()
                .unwrap_or_else(|| "no reason given".into()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};

    fn x_gt_10() -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        }
    }

    fn x_lt_20() -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(20))),
        }
    }

    fn x_lt_5() -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
        }
    }

    #[test]
    fn satisfiable_int_constraints() {
        let constraints = vec![x_gt_10(), x_lt_20()];
        let result = solve_constraints(&constraints, None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x > 10 && x < 20, "expected 10 < x < 20, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat, got unsat"),
        }
    }

    #[test]
    fn unsatisfiable_int_constraints() {
        let constraints = vec![x_gt_10(), x_lt_5()];
        let result = solve_constraints(&constraints, None).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Unsat),
            "expected unsat, got {result:?}"
        );
    }

    #[test]
    fn negate_last_branch_finds_new_path() {
        let constraints = vec![x_gt_10(), x_lt_20()];
        let result = solve_for_new_path(&constraints, 1, None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x >= 20, "expected x >= 20, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat, got unsat"),
        }
    }

    #[test]
    fn negate_first_branch() {
        let constraints = vec![x_gt_10(), x_lt_20()];
        let result = solve_for_new_path(&constraints, 0, None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x <= 10, "expected x <= 10, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat, got unsat"),
        }
    }

    #[test]
    fn bool_constraint_satisfiable() {
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "flag".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                assert_eq!(values.get("flag"), Some(&ConcreteValue::Bool(true)));
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_equality_satisfiable() {
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "name".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("alice".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                assert_eq!(
                    values.get("name"),
                    Some(&ConcreteValue::Str("alice".into()))
                );
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_inequality_unsatisfiable() {
        let eq = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "name".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("alice".into()))),
        };
        let ne = SymExpr::BinOp {
            op: BinOpKind::Ne,
            left: Box::new(SymExpr::Param {
                name: "name".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("alice".into()))),
        };
        let result = solve_constraints(&[eq, ne], None).expect("solver should not error");
        assert!(matches!(result, SolveResult::Unsat));
    }

    #[test]
    fn nested_param_path_creates_distinct_variable() {
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "config".into(),
                path: vec!["timeout".into()],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(30))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let v = match values.get("config.timeout") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for config.timeout, got {other:?}"),
                };
                assert!(v > 30, "expected config.timeout > 30, got {v}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn arithmetic_in_constraints() {
        // x + 5 > 15 → x > 10
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Add,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(5))),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(15))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x > 10, "expected x > 10, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn logical_and_constraint() {
        // (x > 0) AND (x < 3)
        let constraint = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            }),
            right: Box::new(SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(3))),
            }),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x > 0 && x < 3, "expected 0 < x < 3, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn negation_unop() {
        // -x > 5 → x < -5
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::Neg,
                operand: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x < -5, "expected x < -5, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn not_unop() {
        // NOT(x == true) → x == false
        let constraint = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
            }),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                assert_eq!(values.get("x"), Some(&ConcreteValue::Bool(false)));
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn multi_variable_constraints() {
        // x > 0 AND y > x AND y < 10
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "y".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
            },
            SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "y".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(10))),
            },
        ];
        let result = solve_constraints(&constraints, None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                let y = match values.get("y") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for y, got {other:?}"),
                };
                assert!(x > 0, "expected x > 0, got x={x}");
                assert!(y > x, "expected y > x, got x={x}, y={y}");
                assert!(y < 10, "expected y < 10, got y={y}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn negate_index_out_of_bounds_returns_error() {
        let constraints = vec![x_gt_10()];
        let result = solve_for_new_path(&constraints, 5, None);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_expr_returns_error() {
        let constraints = vec![SymExpr::Unknown];
        let result = solve_constraints(&constraints, None);
        assert!(result.is_err());
    }

    #[test]
    fn call_expr_returns_error() {
        let constraint = SymExpr::Call {
            name: "Math.random".into(),
            receiver: None,
            args: vec![],
        };
        let result = solve_constraints(&[constraint], None);
        assert!(result.is_err());
    }

    #[test]
    fn string_ordering_lt() {
        // s < "hello" should be solvable
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "s".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("hello".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s < "hello".to_string(), "expected s < \"hello\", got s=\"{s}\"");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_ordering_ge() {
        // s >= "abc" should be solvable — Z3 uses its own lexicographic order
        // so we just verify satisfiability, not the concrete value against Rust ordering
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Ge,
            left: Box::new(SymExpr::Param {
                name: "s".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("abc".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                assert!(
                    matches!(values.get("s"), Some(ConcreteValue::Str(_))),
                    "expected Str for s, got {:?}",
                    values.get("s")
                );
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_ordering_contradictory_is_unsat() {
        // s > "z" AND s < "a" — unsatisfiable
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "s".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Str("z".into()))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "s".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Str("a".into()))),
            },
        ];
        let result = solve_constraints(&constraints, None).expect("solver should not error");
        assert!(matches!(result, SolveResult::Unsat));
    }

    #[test]
    fn nested_arithmetic_comparison() {
        // (x + 1) * 2 > 10 → x >= 5
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Mul,
                left: Box::new(SymExpr::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(SymExpr::Param {
                        name: "x".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(1))),
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(2))),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x >= 5, "expected x >= 5, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn mixed_int_float_comparison() {
        // x > 3.5 (float constant promotes to Real)
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Float(3.5))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Float(v)) => *v,
                    other => panic!("expected Float for x, got {other:?}"),
                };
                assert!(x > 3.5, "expected x > 3.5, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn solve_for_new_path_multi_constraint() {
        // Path: x > 0, x < 50, x != 25
        // Negate index 2 (x != 25) → should find x == 25 with x > 0 and x < 50
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(50))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Ne,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(25))),
            },
        ];
        let result = solve_for_new_path(&constraints, 2, None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                // Negating x != 25 gives x == 25, with prefix x > 0 && x < 50
                assert_eq!(x, 25, "expected x == 25, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn bitwise_not_constraint() {
        // ~x > 5 → -(x+1) > 5 → x < -6
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::BitwiseNot,
                operand: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                // ~x > 5 means -(x+1) > 5, so x+1 < -5, so x < -6
                assert!(x < -6, "expected x < -6, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn bitwise_not_specific_value() {
        // ~x == 0 → -(x+1) == 0 → x == -1
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::BitwiseNot,
                operand: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert_eq!(x, -1, "expected x == -1, got x={x}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn typeof_int_param() {
        // typeof(x) == "number" where x is an int param — should be sat
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::TypeOf,
                operand: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("number".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Sat(_)),
            "expected sat for typeof(int) == 'number'"
        );
    }

    #[test]
    fn typeof_string_constant() {
        // typeof("hello") == "string" — should be sat
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::TypeOf,
                operand: Box::new(SymExpr::Const(ConstValue::Str("hello".into()))),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("string".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Sat(_)),
            "expected sat for typeof(string) == 'string'"
        );
    }

    #[test]
    fn typeof_bool_param() {
        // typeof(true) == "boolean" — should be sat
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::TypeOf,
                operand: Box::new(SymExpr::Const(ConstValue::Bool(true))),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("boolean".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Sat(_)),
            "expected sat for typeof(bool) == 'boolean'"
        );
    }

    #[test]
    fn typeof_mismatch_is_unsat() {
        // typeof(42) == "string" — should be unsat since 42 is Int → "number"
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::UnOp {
                op: UnOpKind::TypeOf,
                operand: Box::new(SymExpr::Const(ConstValue::Int(42))),
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("string".into()))),
        };
        let result = solve_constraints(&[constraint], None).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Unsat),
            "expected unsat for typeof(int) == 'string'"
        );
    }

    #[test]
    fn solver_timeout_does_not_panic() {
        // A simple satisfiable constraint: x == 42
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(42))),
        };

        // With a 1ms timeout, Z3 may solve it instantly or time out — both are OK.
        let result = solve_constraints(&[constraint], Some(1));
        match result {
            Ok(SolveResult::Sat(_)) => {} // solved before timeout
            Err(SolverError::Unknown(reason)) => {
                assert!(
                    reason.contains("timeout") || reason.contains("canceled"),
                    "unexpected unknown reason: {reason}"
                );
            }
            other => panic!("expected Sat or Unknown(timeout), got: {other:?}"),
        }
    }
}
