//! Z3-based constraint solver for concolic execution.
//!
//! Converts [`SymExpr`] trees into Z3 AST nodes, solves for new execution paths
//! by negating branch constraints, and extracts concrete values from Z3 models.

use contracts::requires;
use std::collections::HashMap;
use std::str::FromStr;

use z3::ast::{Ast, Bool, Int, Real, String as Z3String};
use z3::{Config, SatResult, Solver};

use crate::sym_expr::{BinOpKind, ConstValue, SymExpr, UnOpKind};
use crate::types::{ParamInfo, TypeInfo};

include!(concat!(env!("OUT_DIR"), "/string_ops_generated.rs"));

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
    /// Known sorts for top-level parameters, derived from `ParamInfo` type data.
    /// Used to override `infer_sort` which defaults `Param` to `Sort::Int`.
    param_sorts: HashMap<String, Sort>,
}

impl VarTable {
    fn new(param_sorts: HashMap<String, Sort>) -> Self {
        Self {
            ints: HashMap::new(),
            reals: HashMap::new(),
            bools: HashMap::new(),
            strings: HashMap::new(),
            param_sorts,
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

/// Two sorts are "compatible" if they're in the same category and the comparison
/// context's hint should take precedence. Numeric sorts (Int, Real) are compatible
/// with each other; Str and Bool are only compatible with themselves.
fn sorts_compatible(a: Sort, b: Sort) -> bool {
    matches!(
        (a, b),
        (Sort::Int | Sort::Real, Sort::Int | Sort::Real)
            | (Sort::Str, Sort::Str)
            | (Sort::Bool, Sort::Bool)
    )
}

/// Map `TypeInfo` from frontend analysis to the Z3 `Sort` used for variable declaration.
fn type_info_to_sort(ty: &TypeInfo) -> Sort {
    match ty {
        TypeInfo::Str => Sort::Str,
        TypeInfo::Int => Sort::Int,
        TypeInfo::Float => Sort::Real,
        TypeInfo::Bool => Sort::Bool,
        // Nullable<inner> uses the inner type's sort for Z3 purposes
        TypeInfo::Nullable { inner } => type_info_to_sort(inner),
        // Default to Int for types Z3 can't represent (arrays, objects, unions, etc.)
        _ => Sort::Int,
    }
}

/// Build a param-name → Sort map from `ParamInfo` slices.
fn build_param_sorts(param_infos: &[ParamInfo]) -> HashMap<String, Sort> {
    param_infos
        .iter()
        .map(|p| (p.name.clone(), type_info_to_sort(&p.typ)))
        .collect()
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
        SymExpr::Call { name, .. } => infer_call_sort(name),
        SymExpr::Param { .. } | SymExpr::Unknown => Sort::Int,
        SymExpr::Const(ConstValue::Null | ConstValue::Undefined) => Sort::Int,
        // Complex constants unwrap to their repr's sort
        SymExpr::Const(ConstValue::Complex { repr, .. }) => {
            infer_sort(&SymExpr::Const(*repr.clone()))
        }
    }
}

/// Infer the Z3 sort for a Call expression based on the method name.
/// Recognized string methods return Bool, Int, or Str; unknown calls default to Int.
fn infer_call_sort(name: &str) -> Sort {
    resolve_string_op(name).map_or(Sort::Int, |op| op.z3_sort())
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
            // Use declared param sort to prevent sort mismatches (e.g. string params
            // declared as Int when they appear inside .length). Only override hint_sort
            // when the declared sort is categorically different (Str vs numeric/bool).
            // For numeric sorts (Int/Real/Float), prefer the comparison context's hint
            // since TS `number` maps to Float but constraints often use Int comparisons.
            let sort = match vars.param_sorts.get(&var_name).copied() {
                Some(declared) if !sorts_compatible(declared, hint_sort) => declared,
                _ => hint_sort,
            };
            match sort {
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

        SymExpr::Call {
            name,
            receiver,
            args,
        } => convert_string_call(vars, name, receiver.as_deref(), args),

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

/// Map a `SymExpr::Call` to Z3 string operations.
///
/// Dispatches via `StringOp` enum (generated from `data/string-ops.yaml`).
/// Supports cross-language method names: TS uses `includes`, `indexOf`, `startsWith`, etc.;
/// Go uses `strings.Contains`, `strings.Index`, `strings.HasPrefix`, etc.
/// Returns `SolverError::Unsupported` for unrecognized call names.
fn convert_string_call(
    vars: &mut VarTable,
    name: &str,
    receiver: Option<&SymExpr>,
    args: &[SymExpr],
) -> Result<Z3Ast, SolverError> {
    let op = resolve_string_op(name).ok_or_else(|| {
        SolverError::Unsupported(format!("function call '{name}' cannot be represented in Z3"))
    })?;

    match op {
        StringOp::Contains => {
            let (haystack, needle) = receiver_and_first_arg(vars, name, receiver, args)?;
            Ok(Z3Ast::Bool(haystack.contains(&needle)))
        }

        // Z3's `prefix` checks whether `self` is a prefix OF the argument,
        // so `receiver.startsWith(arg)` maps to `arg.prefix(&receiver)`.
        StringOp::Prefix => {
            let (haystack, prefix) = receiver_and_first_arg(vars, name, receiver, args)?;
            Ok(Z3Ast::Bool(prefix.prefix(&haystack)))
        }

        // Same inversion as prefix: `receiver.endsWith(arg)` → `arg.suffix(&receiver)`.
        StringOp::Suffix => {
            let (haystack, suffix) = receiver_and_first_arg(vars, name, receiver, args)?;
            Ok(Z3Ast::Bool(suffix.suffix(&haystack)))
        }

        // Z3_mk_seq_index(ctx, s, substr, offset) returns Int (-1 when not found).
        // Not wrapped in the z3 crate, so we call z3-sys directly.
        StringOp::IndexOf => {
            let (haystack, needle) = receiver_and_first_arg(vars, name, receiver, args)?;
            // Offset arg position differs: TS-style has offset at args[1], Go-style at args[2]
            let offset_arg_index = if receiver.is_none() { 2 } else { 1 };
            let offset = if args.len() > offset_arg_index {
                to_z3_int(vars, &args[offset_arg_index])?
            } else {
                Int::from_i64(0)
            };
            let ctx_ref = haystack.get_ctx();
            let raw_ctx = ctx_ref.get_z3_context();
            let result = unsafe {
                z3_sys::Z3_mk_seq_index(
                    raw_ctx,
                    haystack.get_z3_ast(),
                    needle.get_z3_ast(),
                    offset.get_z3_ast(),
                )
            };
            let result = result.ok_or_else(|| {
                SolverError::Unsupported("Z3_mk_seq_index returned null".into())
            })?;
            Ok(Z3Ast::Int(unsafe { Int::wrap(ctx_ref, result) }))
        }

        StringOp::Length => {
            let recv = require_receiver(name, receiver)?;
            let s = to_z3_string(vars, recv)?;
            Ok(Z3Ast::Int(s.length()))
        }

        StringOp::CharAt => {
            let recv = require_receiver(name, receiver)?;
            let s = to_z3_string(vars, recv)?;
            let index = require_first_arg(name, args)?;
            let idx = to_z3_int(vars, index)?;
            Ok(Z3Ast::Str(s.at(idx)))
        }

        // All three (slice/substring/substr) map to Z3's `substr(offset, length)`.
        // JS `slice(start, end?)` and `substring(start, end?)` use end index;
        // we compute length = end - start. If no end, we use str.length - start.
        StringOp::Substr => {
            let recv = require_receiver(name, receiver)?;
            let s = to_z3_string(vars, recv)?;
            let start_expr = require_first_arg(name, args)?;
            let start = to_z3_int(vars, start_expr)?;
            let length = if args.len() >= 2 {
                let end = to_z3_int(vars, &args[1])?;
                Int::sub(&[&end, &start])
            } else {
                let total_len = s.length();
                Int::sub(&[&total_len, &start])
            };
            Ok(Z3Ast::Str(s.substr(start, length)))
        }

        StringOp::Concat => {
            let recv = require_receiver(name, receiver)?;
            let s = to_z3_string(vars, recv)?;
            let mut parts = vec![s];
            for arg in args {
                parts.push(to_z3_string(vars, arg)?);
            }
            let refs: Vec<&Z3String> = parts.iter().collect();
            Ok(Z3Ast::Str(Z3String::concat(&refs)))
        }
    }
}

/// Extract the receiver string and first argument string for binary string operations.
fn receiver_and_first_arg(
    vars: &mut VarTable,
    name: &str,
    receiver: Option<&SymExpr>,
    args: &[SymExpr],
) -> Result<(Z3String, Z3String), SolverError> {
    // Go-style: no receiver, two positional args (e.g. strings.Contains(s, substr))
    if receiver.is_none() && args.len() >= 2 {
        let haystack = to_z3_string(vars, &args[0])?;
        let needle = to_z3_string(vars, &args[1])?;
        return Ok((haystack, needle));
    }
    let recv = require_receiver(name, receiver)?;
    let s = to_z3_string(vars, recv)?;
    let arg = require_first_arg(name, args)?;
    let a = to_z3_string(vars, arg)?;
    Ok((s, a))
}

fn require_receiver<'a>(name: &str, receiver: Option<&'a SymExpr>) -> Result<&'a SymExpr, SolverError> {
    receiver.ok_or_else(|| {
        SolverError::Unsupported(format!("string method '{name}' requires a receiver"))
    })
}

fn require_first_arg<'a>(name: &str, args: &'a [SymExpr]) -> Result<&'a SymExpr, SolverError> {
    args.first().ok_or_else(|| {
        SolverError::Unsupported(format!("string method '{name}' requires at least one argument"))
    })
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
#[requires(negate_index < constraints.len(), "negate_index must be within constraints bounds")]
pub fn solve_for_new_path(
    constraints: &[SymExpr],
    negate_index: usize,
    solver_timeout_ms: Option<u64>,
    param_infos: &[ParamInfo],
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
    let param_sorts = build_param_sorts(param_infos);
    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new(param_sorts.clone());

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
    param_infos: &[ParamInfo],
) -> Result<SolveResult, SolverError> {
    let mut cfg = Config::new();
    if let Some(ms) = solver_timeout_ms {
        cfg.set_timeout_msec(ms);
    }
    let param_sorts = build_param_sorts(param_infos);
    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new(param_sorts.clone());

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
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Unsat),
            "expected unsat, got {result:?}"
        );
    }

    #[test]
    fn negate_last_branch_finds_new_path() {
        let constraints = vec![x_gt_10(), x_lt_20()];
        let result = solve_for_new_path(&constraints, 1, None, &[]).expect("solver should not error");
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
        let result = solve_for_new_path(&constraints, 0, None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[eq, ne], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
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
        // In debug builds, the `#[requires]` contract catches this as a panic
        // before the function body's manual check returns Err.
        let result = std::panic::catch_unwind(|| {
            solve_for_new_path(&constraints, 5, None, &[])
        });
        assert!(
            result.is_err() || result.unwrap().is_err(),
            "out-of-bounds negate_index must fail"
        );
    }

    #[test]
    fn unknown_expr_returns_error() {
        let constraints = vec![SymExpr::Unknown];
        let result = solve_constraints(&constraints, None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn call_expr_returns_error() {
        let constraint = SymExpr::Call {
            name: "Math.random".into(),
            receiver: None,
            args: vec![],
        };
        let result = solve_constraints(&[constraint], None, &[]);
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_for_new_path(&constraints, 2, None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
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
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Unsat),
            "expected unsat for typeof(int) == 'string'"
        );
    }

    // ── String method call tests ──────────────────────────────────────────

    /// Helper: create `s.includes("needle") == true` constraint.
    fn str_param(name: &str) -> SymExpr {
        SymExpr::Param {
            name: name.into(),
            path: vec![],
        }
    }

    #[test]
    fn string_includes_sat() {
        // s.includes("hello") == true → s must contain "hello"
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "includes".into(),
                receiver: Some(Box::new(str_param("s"))),
                args: vec![SymExpr::Const(ConstValue::Str("hello".into()))],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.contains("hello"), "expected s to contain 'hello', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_contains_go_style() {
        // strings.Contains(s, "world") == true — Go-style (no receiver, two args)
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "strings.Contains".into(),
                receiver: None,
                args: vec![
                    str_param("s"),
                    SymExpr::Const(ConstValue::Str("world".into())),
                ],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.contains("world"), "expected s to contain 'world', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_starts_with_sat() {
        // s.startsWith("pre") == true
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "startsWith".into(),
                receiver: Some(Box::new(str_param("s"))),
                args: vec![SymExpr::Const(ConstValue::Str("pre".into()))],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.starts_with("pre"), "expected s to start with 'pre', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_has_prefix_go_style() {
        // strings.HasPrefix(s, "go_") == true
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "strings.HasPrefix".into(),
                receiver: None,
                args: vec![
                    str_param("s"),
                    SymExpr::Const(ConstValue::Str("go_".into())),
                ],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.starts_with("go_"), "expected s to start with 'go_', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_ends_with_sat() {
        // s.endsWith(".ts") == true
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "endsWith".into(),
                receiver: Some(Box::new(str_param("s"))),
                args: vec![SymExpr::Const(ConstValue::Str(".ts".into()))],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.ends_with(".ts"), "expected s to end with '.ts', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_has_suffix_go_style() {
        // strings.HasSuffix(s, ".go") == true
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "strings.HasSuffix".into(),
                receiver: None,
                args: vec![
                    str_param("s"),
                    SymExpr::Const(ConstValue::Str(".go".into())),
                ],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.ends_with(".go"), "expected s to end with '.go', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_index_of_found() {
        // s.indexOf("needle") >= 0 AND s.indexOf("needle") < 5
        let index_call = SymExpr::Call {
            name: "indexOf".into(),
            receiver: Some(Box::new(str_param("s"))),
            args: vec![SymExpr::Const(ConstValue::Str("needle".into()))],
        };
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Ge,
                left: Box::new(index_call.clone()),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(index_call),
                right: Box::new(SymExpr::Const(ConstValue::Int(5))),
            },
        ];
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                let idx = s.find("needle").expect("s should contain 'needle'");
                assert!(idx < 5, "expected indexOf < 5, got {idx}");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_index_of_not_found() {
        // s == "abc" AND s.indexOf("xyz") == -1
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(str_param("s")),
                right: Box::new(SymExpr::Const(ConstValue::Str("abc".into()))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Call {
                    name: "indexOf".into(),
                    receiver: Some(Box::new(str_param("s"))),
                    args: vec![SymExpr::Const(ConstValue::Str("xyz".into()))],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(-1))),
            },
        ];
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Sat(_)),
            "expected sat (abc does not contain xyz)"
        );
    }

    #[test]
    fn string_index_go_style() {
        // strings.Index(s, "x") >= 0 — Go style
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Ge,
            left: Box::new(SymExpr::Call {
                name: "strings.Index".into(),
                receiver: None,
                args: vec![
                    str_param("s"),
                    SymExpr::Const(ConstValue::Str("x".into())),
                ],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.contains('x'), "expected s to contain 'x', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_length_constraint() {
        // s.length > 5 AND s.length < 10
        let len_call = SymExpr::Call {
            name: "length".into(),
            receiver: Some(Box::new(str_param("s"))),
            args: vec![],
        };
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(len_call.clone()),
                right: Box::new(SymExpr::Const(ConstValue::Int(5))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(len_call),
                right: Box::new(SymExpr::Const(ConstValue::Int(10))),
            },
        ];
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                let len = s.len();
                assert!(
                    len > 5 && len < 10,
                    "expected 5 < length < 10, got {len}"
                );
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_char_at_constraint() {
        // s.charAt(0) == "a"
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "charAt".into(),
                receiver: Some(Box::new(str_param("s"))),
                args: vec![SymExpr::Const(ConstValue::Int(0))],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("a".into()))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert!(s.starts_with('a'), "expected s to start with 'a', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_slice_constraint() {
        // s == "hello world" AND s.slice(0, 5) == "hello"
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(str_param("s")),
                right: Box::new(SymExpr::Const(ConstValue::Str("hello world".into()))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Call {
                    name: "slice".into(),
                    receiver: Some(Box::new(str_param("s"))),
                    args: vec![
                        SymExpr::Const(ConstValue::Int(0)),
                        SymExpr::Const(ConstValue::Int(5)),
                    ],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Str("hello".into()))),
            },
        ];
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Sat(_)),
            "expected sat — 'hello world'.slice(0,5) == 'hello'"
        );
    }

    #[test]
    fn string_concat_constraint() {
        // s.concat(" world") == "hello world" → s must be "hello"
        let constraint = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Call {
                name: "concat".into(),
                receiver: Some(Box::new(str_param("s"))),
                args: vec![SymExpr::Const(ConstValue::Str(" world".into()))],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Str("hello world".into()))),
        };
        let result = solve_constraints(&[constraint], None, &[]).expect("solver should not error");
        match result {
            SolveResult::Sat(values) => {
                let s = match values.get("s") {
                    Some(ConcreteValue::Str(v)) => v.clone(),
                    other => panic!("expected Str for s, got {other:?}"),
                };
                assert_eq!(s, "hello", "expected s == 'hello', got '{s}'");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    #[test]
    fn string_includes_negated_unsat() {
        // s == "abc" AND NOT s.includes("b") — should be unsat since "abc" contains "b"
        let constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(str_param("s")),
                right: Box::new(SymExpr::Const(ConstValue::Str("abc".into()))),
            },
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Call {
                    name: "includes".into(),
                    receiver: Some(Box::new(str_param("s"))),
                    args: vec![SymExpr::Const(ConstValue::Str("b".into()))],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Bool(false))),
            },
        ];
        let result = solve_constraints(&constraints, None, &[]).expect("solver should not error");
        assert!(
            matches!(result, SolveResult::Unsat),
            "expected unsat — 'abc' always contains 'b'"
        );
    }

    #[test]
    fn unrecognized_call_returns_error() {
        let constraint = SymExpr::Call {
            name: "Math.random".into(),
            receiver: None,
            args: vec![],
        };
        let result = solve_constraints(&[constraint], None, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn spec_aliases_match_generated_code() {
        // Parse the YAML spec at test time and verify every alias resolves via resolve_string_op().
        let yaml_src = include_str!("../data/string-ops.yaml");
        let spec: serde_yaml::Value =
            serde_yaml::from_str(yaml_src).expect("failed to parse string-ops.yaml");
        let operations = spec["operations"].as_sequence().expect("operations should be a list");
        for op_value in operations {
            let op_name = op_value["name"].as_str().unwrap();
            let z3_sort = op_value["z3_sort"].as_str().unwrap();
            let aliases = op_value["aliases"].as_sequence().expect("aliases should be a list");
            for alias in aliases {
                let method = alias["method"].as_str().unwrap();
                let resolved = resolve_string_op(method);
                assert!(
                    resolved.is_some(),
                    "method '{method}' (operation '{op_name}') not found in generated resolve_string_op()"
                );
                let expected_sort = match z3_sort {
                    "Bool" => Sort::Bool,
                    "Int" => Sort::Int,
                    "Str" => Sort::Str,
                    other => panic!("unknown sort '{other}' in spec"),
                };
                assert_eq!(
                    resolved.unwrap().z3_sort(),
                    expected_sort,
                    "method '{method}' (operation '{op_name}'): expected sort {z3_sort}, got {:?}",
                    resolved.unwrap().z3_sort(),
                );
            }
        }
    }

    #[test]
    fn all_operations_produce_declared_sort() {
        // For each canonical operation, build a minimal SymExpr::Call, convert it,
        // and assert the Z3Ast variant matches the declared sort.
        let yaml_src = include_str!("../data/string-ops.yaml");
        let spec: serde_yaml::Value =
            serde_yaml::from_str(yaml_src).expect("failed to parse string-ops.yaml");
        let operations = spec["operations"].as_sequence().unwrap();

        for op_value in operations {
            let op_name = op_value["name"].as_str().unwrap();
            let z3_sort = op_value["z3_sort"].as_str().unwrap();
            let aliases = op_value["aliases"].as_sequence().unwrap();
            // Pick first alias to test
            let first_alias = &aliases[0];
            let method = first_alias["method"].as_str().unwrap();
            let style = first_alias["style"].as_str().unwrap();

            // Build minimal Call expression with appropriate receiver/args
            let s_param = SymExpr::Param {
                name: "s".into(),
                path: vec![],
            };
            let arg_str = SymExpr::Const(ConstValue::Str("x".into()));
            let arg_int = SymExpr::Const(ConstValue::Int(0));

            let call = match (op_name, style) {
                ("length", _) => SymExpr::Call {
                    name: method.into(),
                    receiver: Some(Box::new(s_param)),
                    args: vec![],
                },
                ("char_at", _) => SymExpr::Call {
                    name: method.into(),
                    receiver: Some(Box::new(s_param)),
                    args: vec![arg_int],
                },
                ("substr", _) => SymExpr::Call {
                    name: method.into(),
                    receiver: Some(Box::new(s_param)),
                    args: vec![arg_int.clone(), SymExpr::Const(ConstValue::Int(1))],
                },
                ("concat", _) => SymExpr::Call {
                    name: method.into(),
                    receiver: Some(Box::new(s_param)),
                    args: vec![arg_str],
                },
                (_, "receiver") => SymExpr::Call {
                    name: method.into(),
                    receiver: Some(Box::new(s_param)),
                    args: vec![arg_str],
                },
                (_, "free") => SymExpr::Call {
                    name: method.into(),
                    receiver: None,
                    args: vec![s_param, arg_str],
                },
                _ => panic!("unhandled style '{style}' for '{op_name}'"),
            };

            // Wrap in a constraint so we can solve it
            let expected_sort_enum = match z3_sort {
                "Bool" => Sort::Bool,
                "Int" => Sort::Int,
                "Str" => Sort::Str,
                other => panic!("unknown sort '{other}'"),
            };

            // Just check that infer_call_sort returns the right sort
            assert_eq!(
                infer_call_sort(method),
                expected_sort_enum,
                "infer_call_sort('{method}') for operation '{op_name}' returned wrong sort"
            );

            // Also verify the call can be converted without error (basic smoke test)
            let constraint = match z3_sort {
                "Bool" => SymExpr::BinOp {
                    op: BinOpKind::Eq,
                    left: Box::new(call),
                    right: Box::new(SymExpr::Const(ConstValue::Bool(true))),
                },
                "Int" => SymExpr::BinOp {
                    op: BinOpKind::Ge,
                    left: Box::new(call),
                    right: Box::new(SymExpr::Const(ConstValue::Int(0))),
                },
                "Str" => SymExpr::BinOp {
                    op: BinOpKind::Eq,
                    left: Box::new(call),
                    right: Box::new(SymExpr::Const(ConstValue::Str("x".into()))),
                },
                _ => unreachable!(),
            };
            let result = solve_constraints(&[constraint], None, &[]);
            assert!(
                result.is_ok(),
                "operation '{op_name}' via method '{method}' failed to convert: {:?}",
                result.err()
            );
        }
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
        let result = solve_constraints(&[constraint], Some(1), &[]);
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

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn infer_sort_never_panics(expr in arb_sym_expr(4)) {
                // Any well-formed SymExpr should produce a valid Sort without panicking.
                let _ = infer_sort(&expr);
            }

            #[test]
            fn int_const_infers_int(v in -1_000_000i64..1_000_000i64) {
                let expr = SymExpr::Const(ConstValue::Int(v));
                prop_assert_eq!(infer_sort(&expr), Sort::Int);
            }

            #[test]
            fn str_const_infers_str(s in ".{0,20}") {
                let expr = SymExpr::Const(ConstValue::Str(s));
                prop_assert_eq!(infer_sort(&expr), Sort::Str);
            }

            #[test]
            fn bool_const_infers_bool(b in any::<bool>()) {
                let expr = SymExpr::Const(ConstValue::Bool(b));
                prop_assert_eq!(infer_sort(&expr), Sort::Bool);
            }

            #[test]
            fn float_const_infers_real(
                f in (-1000.0f64..1000.0).prop_filter("finite", |f| f.is_finite())
            ) {
                let expr = SymExpr::Const(ConstValue::Float(f));
                prop_assert_eq!(infer_sort(&expr), Sort::Real);
            }
        }
    }
}
