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
        TypeInfo::Int { .. } => Sort::Int,
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

/// Assert the value-range bounds implied by each sized integer parameter
/// (str-ddxe). Without these bounds Z3 would freely pick values like `926` for a
/// `u8` param, which then fail to deserialize into the narrow field. Only widths
/// whose range fits in `i64` are bounded (see [`TypeInfo::int_range`]); wider
/// types stay unconstrained.
fn assert_int_param_ranges(solver: &Solver, vars: &mut VarTable, param_infos: &[ParamInfo]) {
    for p in param_infos {
        if let Some((min, max)) = p.typ.int_range() {
            let v = vars.get_or_create_int(&p.name);
            solver.assert(v.ge(Int::from_i64(min)));
            solver.assert(v.le(Int::from_i64(max)));
        }
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
            BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge => {
                let l = infer_sort(left);
                if l != Sort::Int && l != Sort::Bool {
                    l
                } else {
                    let r = infer_sort(right);
                    if r != Sort::Int { r } else { l }
                }
            }
            BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Mod => {
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
        SymExpr::Ite { then_expr, .. } => infer_sort(then_expr),
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
            if l != Sort::Int { l } else { r }
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
fn to_z3_expr(vars: &mut VarTable, expr: &SymExpr, hint_sort: Sort) -> Result<Z3Ast, SolverError> {
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

        SymExpr::Ite {
            condition,
            then_expr,
            else_expr,
        } => {
            let cond_bool = to_z3_bool(vars, condition)?;
            let then_sort = infer_sort(then_expr);
            let then_z3 = to_z3_expr(vars, then_expr, then_sort)?;
            let else_z3 = to_z3_expr(vars, else_expr, then_sort)?;
            match (then_z3, else_z3) {
                (Z3Ast::Int(t), Z3Ast::Int(e)) => Ok(Z3Ast::Int(cond_bool.ite(&t, &e))),
                (Z3Ast::Real(t), Z3Ast::Real(e)) => Ok(Z3Ast::Real(cond_bool.ite(&t, &e))),
                (Z3Ast::Bool(t), Z3Ast::Bool(e)) => Ok(Z3Ast::Bool(cond_bool.ite(&t, &e))),
                (Z3Ast::Str(t), Z3Ast::Str(e)) => Ok(Z3Ast::Str(cond_bool.ite(&t, &e))),
                _ => Err(SolverError::TypeMismatch {
                    expected: "matching sorts for ITE branches".into(),
                    actual: "mismatched sorts".into(),
                }),
            }
        }

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

        BinOpKind::Eq
        | BinOpKind::Ne
        | BinOpKind::Lt
        | BinOpKind::Le
        | BinOpKind::Gt
        | BinOpKind::Ge => {
            let operand_sort = infer_comparison_sort(left, right, hint_sort);
            convert_comparison(vars, op, left, right, operand_sort)
        }

        BinOpKind::Add | BinOpKind::Sub | BinOpKind::Mul | BinOpKind::Div | BinOpKind::Mod => {
            let arith_sort = infer_arithmetic_sort(left, right, hint_sort);
            convert_arithmetic(vars, op, left, right, arith_sort)
        }

        BinOpKind::BitwiseAnd
        | BinOpKind::BitwiseOr
        | BinOpKind::BitwiseXor
        | BinOpKind::Shl
        | BinOpKind::Shr
        | BinOpKind::BitClear => Err(SolverError::Unsupported(format!(
            "bitwise operator {op:?} not yet supported in Z3 solver"
        ))),

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
                    )));
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
                    ));
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
///
/// # Unsupported operations (str-548q — deferred)
///
/// - **split()**: Z3 Seq theory has no split primitive. Modeling it would require
///   bounded unrolling to a configurable max segment count, which is expensive and
///   produces fragile constraints for variable-length results.
/// - **Regex (match/test/replace)**: Z3 supports only a decidable regex fragment
///   (`str.in_re`). JS/Go/Rust regex features — backreferences, lookahead/lookbehind,
///   named capture groups — fall outside this fragment and cannot be encoded.
///
/// Unrecognized calls return `SolverError::Unsupported`, causing the explorer to
/// fall back to random/mutation-based input generation for that path constraint.
/// Planned workaround: frontend-side structural candidate generation for
/// split/regex-heavy functions (generate plausible inputs from observed examples
/// rather than solving symbolically).
fn convert_string_call(
    vars: &mut VarTable,
    name: &str,
    receiver: Option<&SymExpr>,
    args: &[SymExpr],
) -> Result<Z3Ast, SolverError> {
    let op = resolve_string_op(name).ok_or_else(|| {
        SolverError::Unsupported(format!(
            "function call '{name}' cannot be represented in Z3"
        ))
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
            let result = result
                .ok_or_else(|| SolverError::Unsupported("Z3_mk_seq_index returned null".into()))?;
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

fn require_receiver<'a>(
    name: &str,
    receiver: Option<&'a SymExpr>,
) -> Result<&'a SymExpr, SolverError> {
    receiver.ok_or_else(|| {
        SolverError::Unsupported(format!("string method '{name}' requires a receiver"))
    })
}

fn require_first_arg<'a>(name: &str, args: &'a [SymExpr]) -> Result<&'a SymExpr, SolverError> {
    args.first().ok_or_else(|| {
        SolverError::Unsupported(format!(
            "string method '{name}' requires at least one argument"
        ))
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
fn extract_concrete_values(model: &z3::Model, vars: &VarTable) -> HashMap<String, ConcreteValue> {
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
                    result.insert(name.clone(), ConcreteValue::Float(num as f64 / den as f64));
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

/// Check that solved `ConcreteValue` types are compatible with declared `ParamInfo` types.
/// Used as an `#[ensures]` postcondition on `solve_for_new_path`. Returns `true` when
/// the result is an error, unsat, or when all solved values match their param types.
/// A mismatch (e.g. `ConcreteValue::Int` for a `TypeInfo::Str` param) returns `false`.
fn solved_values_match_param_types(
    result: Result<&SolveResult, &SolverError>,
    param_infos: &[ParamInfo],
) -> bool {
    let solved = match result {
        Ok(SolveResult::Sat(map)) => map,
        // Errors and Unsat trivially satisfy the postcondition.
        _ => return true,
    };
    let param_map: HashMap<&str, &TypeInfo> = param_infos
        .iter()
        .map(|p| (p.name.as_str(), &p.typ))
        .collect();
    for (name, value) in solved {
        if let Some(ty) = param_map.get(name.as_str())
            && !concrete_value_matches_type(value, ty)
        {
            return false;
        }
        // Solved variables not in param_infos are sub-paths (e.g. "config.timeout")
        // or synthesized — no type to check against.
    }
    true
}

/// Whether a `ConcreteValue` is compatible with a `TypeInfo`.
fn concrete_value_matches_type(value: &ConcreteValue, ty: &TypeInfo) -> bool {
    match (value, ty) {
        // Numeric types are interchangeable — Z3 may solve a Float param as Int
        // when the constraints only use integer comparisons, and vice versa.
        (ConcreteValue::Int(_) | ConcreteValue::Float(_), TypeInfo::Int { .. } | TypeInfo::Float) => {
            true
        }
        (ConcreteValue::Str(_), TypeInfo::Str) => true,
        (ConcreteValue::Bool(_), TypeInfo::Bool) => true,
        // Nullable<inner> — the solved value should match the inner type
        (_, TypeInfo::Nullable { inner }) => concrete_value_matches_type(value, inner),
        // Complex value wraps a repr that Z3 solved — accept any complex match.
        (ConcreteValue::Complex { .. }, TypeInfo::Complex { .. }) => true,
        // Int is the Z3 default for types it can't represent (arrays, objects, unions,
        // unknown, opaque). Accept Int for any non-primitive type.
        (ConcreteValue::Int(_), _) => !matches!(ty, TypeInfo::Str | TypeInfo::Bool),
        _ => false,
    }
}

fn validate_constraint_params(
    constraints: &[SymExpr],
    param_infos: &[ParamInfo],
) -> Result<(), SolverError> {
    if param_infos.is_empty() {
        return Ok(());
    }

    let known: std::collections::HashSet<&str> =
        param_infos.iter().map(|p| p.name.as_str()).collect();
    for constraint in constraints {
        for name in crate::sym_expr::extract_param_names(constraint) {
            // Sub-path params (e.g. "config.timeout") won't match top-level names.
            if !name.contains('.') && !known.contains(name.as_str()) {
                return Err(SolverError::Unsupported(format!(
                    "constraint references unknown param {name:?}, known params: {known:?}"
                )));
            }
        }
    }

    Ok(())
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
#[contracts::ensures(solved_values_match_param_types(ret.as_ref(), param_infos),
    "solved value types must be compatible with declared ParamInfo types")]
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

    // Verify constraint param names exist in param_infos when param_infos is
    // non-empty. Unknown params get default Int sort in Z3, which silently
    // produces wrong solutions for String/Bool params.
    validate_constraint_params(constraints, param_infos)?;

    let mut cfg = Config::new();
    if let Some(ms) = solver_timeout_ms {
        cfg.set_timeout_msec(ms);
    }
    let param_sorts = build_param_sorts(param_infos);
    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new(param_sorts.clone());

        // Constrain sized integer params to their type range (str-ddxe).
        assert_int_param_ranges(&solver, &mut vars, param_infos);

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

        // Constrain sized integer params to their type range (str-ddxe).
        assert_int_param_ranges(&solver, &mut vars, param_infos);

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

/// Solve for a condition-independence witness.
///
/// Given a decision's constraints and a target condition index, find inputs
/// where the target condition flips while all other conditions remain at
/// their observed values.
///
/// `prefix` — path constraints leading up to this decision (asserted as-is)
/// `conditions` — per-condition SymExprs for the compound decision
/// `observed` — observed truth values from a prior execution (None = masked)
/// `target_index` — which condition to flip
/// `solver_timeout_ms` — per-query Z3 timeout
/// `param_infos` — parameter type information
///
/// Returns `SolverError::Unsupported` if the target condition is `Unknown`
/// or if `target_index >= conditions.len()`. Returns `SolveResult::Unsat`
/// when no inputs can satisfy the independence constraint.
pub fn solve_for_mcdc_independence(
    prefix: &[SymExpr],
    conditions: &[SymExpr],
    observed: &[Option<bool>],
    target_index: usize,
    solver_timeout_ms: Option<u64>,
    param_infos: &[ParamInfo],
) -> Result<SolveResult, SolverError> {
    if target_index >= conditions.len() {
        return Err(SolverError::Unsupported(format!(
            "target_index {target_index} out of bounds (conditions len={})",
            conditions.len()
        )));
    }

    // The observed slice may be shorter than conditions if some conditions were
    // added after the observation was recorded. Treat out-of-range as None.
    let target_observed = observed.get(target_index).copied().flatten();

    // Can't flip a masked (unobserved) condition — no prior value to invert.
    let Some(_target_val) = target_observed else {
        return Err(SolverError::Unsupported(format!(
            "target condition {target_index} was masked (no observed value to flip)"
        )));
    };

    // Skip if target condition is Unknown — can't assert it in Z3.
    if matches!(conditions[target_index], SymExpr::Unknown) {
        return Err(SolverError::Unsupported(
            "target condition is Unknown; MC/DC analysis not possible for opaque conditions".into(),
        ));
    }

    let mut cfg = Config::new();
    if let Some(ms) = solver_timeout_ms {
        cfg.set_timeout_msec(ms);
    }
    let param_sorts = build_param_sorts(param_infos);

    z3::with_z3_config(&cfg, || {
        let solver = Solver::new();
        let mut vars = VarTable::new(param_sorts.clone());

        // Constrain sized integer params to their type range (str-ddxe).
        assert_int_param_ranges(&solver, &mut vars, param_infos);

        // Assert all prefix constraints (path leading up to this decision).
        for constraint in prefix {
            let sort = infer_operand_sort(constraint);
            let bool_expr = to_z3_bool_constraint(&mut vars, constraint, sort)?;
            solver.assert(&bool_expr);
        }

        // For each non-target condition: pin it to its observed value.
        // Skip Unknown conditions and masked (None) conditions.
        for (j, condition) in conditions.iter().enumerate() {
            if j == target_index {
                continue;
            }
            if matches!(condition, SymExpr::Unknown) {
                continue;
            }
            let Some(val) = observed.get(j).copied().flatten() else {
                continue;
            };
            let sort = infer_operand_sort(condition);
            let bool_expr = to_z3_bool_constraint(&mut vars, condition, sort)?;
            if val {
                solver.assert(&bool_expr);
            } else {
                solver.assert(bool_expr.not());
            }
        }

        // For the target condition: assert the OPPOSITE of the observed value.
        let target_condition = &conditions[target_index];
        let sort = infer_operand_sort(target_condition);
        let target_bool = to_z3_bool_constraint(&mut vars, target_condition, sort)?;
        // target_val is Some(_) — confirmed above; flip it.
        let target_val = target_observed.expect("checked above");
        if target_val {
            solver.assert(target_bool.not());
        } else {
            solver.assert(&target_bool);
        }

        check_and_extract(&solver, &vars)
    })
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
        let result =
            solve_for_new_path(&constraints, 1, None, &[]).expect("solver should not error");
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
        let result =
            solve_for_new_path(&constraints, 0, None, &[]).expect("solver should not error");
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
        let result = std::panic::catch_unwind(|| solve_for_new_path(&constraints, 5, None, &[]));
        assert!(
            result.is_err() || result.unwrap().is_err(),
            "out-of-bounds negate_index must fail"
        );
    }

    #[test]
    fn unknown_param_constraint_returns_error() {
        let constraints = vec![SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "owner_filter".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        }];
        let param_infos = vec![ParamInfo {
            name: "current".into(),
            typ: TypeInfo::Int {
                int_width: None,
                int_signed: None,
            },
            type_name: None,
        }];

        let result = solve_for_new_path(&constraints, 0, None, &param_infos);

        match result {
            Err(SolverError::Unsupported(message)) => {
                assert!(message.contains("owner_filter"), "{message}");
                assert!(message.contains("current"), "{message}");
            }
            other => panic!("expected unknown param to be unsupported, got {other:?}"),
        }
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
                assert!(
                    s.as_str() < "hello",
                    "expected s < \"hello\", got s=\"{s}\""
                );
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
        let result =
            solve_for_new_path(&constraints, 2, None, &[]).expect("solver should not error");
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
                assert!(
                    s.contains("hello"),
                    "expected s to contain 'hello', got '{s}'"
                );
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
                assert!(
                    s.contains("world"),
                    "expected s to contain 'world', got '{s}'"
                );
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
                assert!(
                    s.starts_with("pre"),
                    "expected s to start with 'pre', got '{s}'"
                );
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
                assert!(
                    s.starts_with("go_"),
                    "expected s to start with 'go_', got '{s}'"
                );
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
                assert!(
                    s.ends_with(".ts"),
                    "expected s to end with '.ts', got '{s}'"
                );
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
                assert!(
                    s.ends_with(".go"),
                    "expected s to end with '.go', got '{s}'"
                );
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
                args: vec![str_param("s"), SymExpr::Const(ConstValue::Str("x".into()))],
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
                assert!(len > 5 && len < 10, "expected 5 < length < 10, got {len}");
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
                assert!(
                    s.starts_with('a'),
                    "expected s to start with 'a', got '{s}'"
                );
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
        let operations = spec["operations"]
            .as_sequence()
            .expect("operations should be a list");
        for op_value in operations {
            let op_name = op_value["name"].as_str().unwrap();
            let z3_sort = op_value["z3_sort"].as_str().unwrap();
            let aliases = op_value["aliases"]
                .as_sequence()
                .expect("aliases should be a list");
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

        // -- Custom generators for well-typed constraint+param pairs --

        /// Which primitive sort to generate constraints for.
        #[derive(Debug, Clone, Copy)]
        enum PrimSort {
            Int,
            Str,
            Bool,
            Float,
        }

        fn arb_prim_sort() -> impl Strategy<Value = PrimSort> {
            prop_oneof![
                Just(PrimSort::Int),
                Just(PrimSort::Str),
                Just(PrimSort::Bool),
                Just(PrimSort::Float),
            ]
        }

        /// Comparison ops valid for the given sort.
        fn comparison_op_for(sort: PrimSort) -> BoxedStrategy<BinOpKind> {
            match sort {
                PrimSort::Int | PrimSort::Float | PrimSort::Str => prop_oneof![
                    Just(BinOpKind::Eq),
                    Just(BinOpKind::Ne),
                    Just(BinOpKind::Lt),
                    Just(BinOpKind::Le),
                    Just(BinOpKind::Gt),
                    Just(BinOpKind::Ge),
                ]
                .boxed(),
                PrimSort::Bool => prop_oneof![Just(BinOpKind::Eq), Just(BinOpKind::Ne),].boxed(),
            }
        }

        /// Generate a constant matching the given sort.
        fn const_for(sort: PrimSort) -> BoxedStrategy<ConstValue> {
            match sort {
                PrimSort::Int => (-1000i64..1000).prop_map(ConstValue::Int).boxed(),
                PrimSort::Float => (-100i32..100)
                    .prop_map(|n| ConstValue::Float(f64::from(n)))
                    .boxed(),
                PrimSort::Str => "[a-z]{1,8}".prop_map(ConstValue::Str).boxed(),
                PrimSort::Bool => any::<bool>().prop_map(ConstValue::Bool).boxed(),
            }
        }

        fn type_info_for(sort: PrimSort) -> TypeInfo {
            match sort {
                PrimSort::Int => TypeInfo::Int {
                    int_width: None,
                    int_signed: None,
                },
                PrimSort::Float => TypeInfo::Float,
                PrimSort::Str => TypeInfo::Str,
                PrimSort::Bool => TypeInfo::Bool,
            }
        }

        /// Generate a well-typed (constraint, ParamInfo) pair: `param op const` where
        /// the constant's type matches the param's declared type.
        fn arb_typed_constraint() -> impl Strategy<Value = (SymExpr, ParamInfo)> {
            (arb_prim_sort(), "[a-z]{1,6}").prop_flat_map(|(sort, name)| {
                (comparison_op_for(sort), const_for(sort)).prop_map(move |(op, cv)| {
                    let constraint = SymExpr::BinOp {
                        op,
                        left: Box::new(SymExpr::Param {
                            name: name.clone(),
                            path: vec![],
                        }),
                        right: Box::new(SymExpr::Const(cv)),
                    };
                    let param = ParamInfo {
                        name: name.clone(),
                        typ: type_info_for(sort),
                        type_name: None,
                    };
                    (constraint, param)
                })
            })
        }

        /// Check that a ConcreteValue matches the expected sort.
        fn concrete_matches_sort(value: &ConcreteValue, sort: PrimSort) -> bool {
            match (value, sort) {
                (ConcreteValue::Int(_), PrimSort::Int) => true,
                (ConcreteValue::Float(_), PrimSort::Float) => true,
                // Int is also acceptable for Float params (Z3 may return exact integers)
                (ConcreteValue::Int(_), PrimSort::Float) => true,
                (ConcreteValue::Str(_), PrimSort::Str) => true,
                (ConcreteValue::Bool(_), PrimSort::Bool) => true,
                _ => false,
            }
        }

        fn sort_for_type_info(ti: &TypeInfo) -> PrimSort {
            match ti {
                TypeInfo::Int { .. } => PrimSort::Int,
                TypeInfo::Float => PrimSort::Float,
                TypeInfo::Str => PrimSort::Str,
                TypeInfo::Bool => PrimSort::Bool,
                TypeInfo::Nullable { inner } => sort_for_type_info(inner),
                _ => PrimSort::Int,
            }
        }

        /// String method names that produce Bool constraints.
        fn arb_string_bool_method() -> impl Strategy<Value = &'static str> {
            prop_oneof![Just("includes"), Just("startsWith"), Just("endsWith"),]
        }

        proptest! {
            // -- Existing infer_sort properties --

            #[test]
            fn infer_sort_never_panics(expr in arb_sym_expr(4)) {
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

            // -- Property 1: type_info_to_sort consistency --

            #[test]
            fn type_info_to_sort_primitives(sort in arb_prim_sort()) {
                let ti = type_info_for(sort);
                let expected = match sort {
                    PrimSort::Int => Sort::Int,
                    PrimSort::Float => Sort::Real,
                    PrimSort::Str => Sort::Str,
                    PrimSort::Bool => Sort::Bool,
                };
                prop_assert_eq!(type_info_to_sort(&ti), expected);
            }

            #[test]
            fn type_info_to_sort_nullable_unwraps(sort in arb_prim_sort()) {
                let inner = type_info_for(sort);
                let nullable = TypeInfo::Nullable { inner: Box::new(inner.clone()) };
                prop_assert_eq!(type_info_to_sort(&nullable), type_info_to_sort(&inner));
            }

            // -- Property 2: solved values match ParamInfo types --
            // The key property that would have caught str-6ayh.

            #[test]
            fn solved_values_match_param_types(
                (constraint, param) in arb_typed_constraint()
            ) {
                let result = solve_constraints(
                    &[constraint],
                    Some(5000),
                    std::slice::from_ref(&param),
                );
                match result {
                    Ok(SolveResult::Sat(values)) => {
                        let expected_sort = sort_for_type_info(&param.typ);
                        for value in values.values() {
                            prop_assert!(
                                concrete_matches_sort(value, expected_sort),
                                "solved value {:?} doesn't match expected sort {:?} for param {:?}",
                                value, expected_sort, param
                            );
                        }
                    }
                    // Unsat or errors are acceptable — we only check type correctness on Sat
                    Ok(SolveResult::Unsat) => {}
                    Err(_) => {}
                }
            }

            // -- Property 3: to_z3_expr never panics on well-typed constraints --

            #[test]
            fn well_typed_constraints_never_panic(
                (constraint, param) in arb_typed_constraint()
            ) {
                // Should return Ok or a documented error, never panic
                let result = solve_constraints(
                    &[constraint],
                    Some(5000),
                    &[param],
                );
                match &result {
                    Ok(_) => {}
                    Err(SolverError::Unsupported(_)) => {}
                    Err(SolverError::TypeMismatch { .. }) => {}
                    Err(SolverError::Unsat) => {}
                    Err(SolverError::Unknown(_)) => {}
                }
            }

            // -- Property 4: negation index safety --

            #[test]
            fn negation_at_any_valid_index_never_panics(
                constraints in prop::collection::vec(
                    arb_typed_constraint().prop_map(|(c, _)| c),
                    1..=5
                ),
                idx_frac in 0.0f64..1.0
            ) {
                let negate_index = (idx_frac * constraints.len() as f64).floor() as usize;
                let negate_index = negate_index.min(constraints.len() - 1);
                let result = solve_for_new_path(
                    &constraints,
                    negate_index,
                    Some(5000),
                    &[],
                );
                // Any result variant is acceptable — no panics
                match result {
                    Ok(SolveResult::Sat(_)) => {}
                    Ok(SolveResult::Unsat) => {}
                    Err(_) => {}
                }
            }

            // -- Property 5: string theory — string params get string solutions --

            #[test]
            fn string_params_solved_as_strings(
                method in arb_string_bool_method(),
                needle in "[a-z]{1,5}",
                param_name in "[a-z]{1,6}",
                taken in any::<bool>(),
            ) {
                let constraint = SymExpr::BinOp {
                    op: BinOpKind::Eq,
                    left: Box::new(SymExpr::Call {
                        name: method.into(),
                        receiver: Some(Box::new(SymExpr::Param {
                            name: param_name.clone(),
                            path: vec![],
                        })),
                        args: vec![SymExpr::Const(ConstValue::Str(needle))],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Bool(taken))),
                };
                let param = ParamInfo {
                    name: param_name.clone(),
                    typ: TypeInfo::Str,
                    type_name: None,
                };
                let result = solve_constraints(
                    &[constraint],
                    Some(5000),
                    &[param],
                );
                if let Ok(SolveResult::Sat(values)) = result
                    && let Some(value) = values.get(&param_name)
                {
                    prop_assert!(
                        matches!(value, ConcreteValue::Str(_)),
                        "string param '{}' solved as {:?}, expected Str",
                        param_name, value
                    );
                }
            }

            // -- Property 6: solve→negate pipeline preserves type agreement --
            // For a satisfiable constraint, negating it should also produce
            // correctly-typed values (or Unsat).

            #[test]
            fn negate_preserves_type_correctness(
                (constraint, param) in arb_typed_constraint()
            ) {
                let constraints = vec![constraint];
                let result = solve_for_new_path(
                    &constraints,
                    0,
                    Some(5000),
                    std::slice::from_ref(&param),
                );
                if let Ok(SolveResult::Sat(values)) = result {
                    let expected_sort = sort_for_type_info(&param.typ);
                    for value in values.values() {
                        prop_assert!(
                            concrete_matches_sort(value, expected_sort),
                            "negated path produced {:?} for sort {:?}",
                            value, expected_sort
                        );
                    }
                }
            }

            // -- Property 7: multi-param constraints preserve per-param types --

            #[test]
            fn multi_param_type_preservation(
                pair1 in arb_typed_constraint(),
                pair2 in arb_typed_constraint(),
            ) {
                let (c1, p1) = pair1;
                let (c2, p2) = pair2;
                // Skip if same param name with different types (would conflict)
                if p1.name == p2.name {
                    return Ok(());
                }
                let result = solve_constraints(
                    &[c1, c2],
                    Some(5000),
                    &[p1.clone(), p2.clone()],
                );
                if let Ok(SolveResult::Sat(values)) = result {
                    for param in [&p1, &p2] {
                        if let Some(value) = values.get(&param.name) {
                            let expected = sort_for_type_info(&param.typ);
                            prop_assert!(
                                concrete_matches_sort(value, expected),
                                "param '{}' (type {:?}) solved as {:?}",
                                param.name, param.typ, value
                            );
                        }
                    }
                }
            }

            // -- Property 8: build_param_sorts roundtrip --
            // With duplicate names, last-writer-wins (HashMap semantics).

            #[test]
            fn build_param_sorts_maps_all_params(
                params in prop::collection::vec(arb_param_info(), 1..=5)
            ) {
                let sorts = build_param_sorts(&params);
                // Deduplicate: last occurrence of each name wins
                let mut expected: std::collections::HashMap<String, Sort> =
                    std::collections::HashMap::new();
                for p in &params {
                    expected.insert(p.name.clone(), type_info_to_sort(&p.typ));
                }
                for (name, sort) in &expected {
                    prop_assert!(
                        sorts.contains_key(name),
                        "param '{}' missing from sorts map", name
                    );
                    prop_assert_eq!(
                        sorts[name],
                        *sort,
                        "sort mismatch for param '{}'", name
                    );
                }
                prop_assert_eq!(sorts.len(), expected.len());
            }
        }
    }

    #[test]
    fn postcondition_accepts_matching_int_type() {
        let mut map = HashMap::new();
        map.insert("x".into(), ConcreteValue::Int(42));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Int {
                int_width: None,
                int_signed: None,
            },
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn u8_param_range_is_enforced() {
        // x > 900 is UNSAT for a u8 (max 255); x == 200 is SAT in-range (str-ddxe).
        let u8_param = vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Int {
                int_width: Some(8),
                int_signed: Some(false),
            },
            type_name: None,
        }];

        let gt_900 = vec![SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(900))),
        }];
        let res = solve_constraints(&gt_900, None, &u8_param).expect("solve");
        assert!(
            matches!(res, SolveResult::Unsat),
            "x>900 must be UNSAT for u8, got {res:?}"
        );

        let eq_200 = vec![SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(200))),
        }];
        let res = solve_constraints(&eq_200, None, &u8_param).expect("solve");
        match res {
            SolveResult::Sat(map) => {
                assert_eq!(map.get("x"), Some(&ConcreteValue::Int(200)));
            }
            other => panic!("x==200 must be SAT for u8, got {other:?}"),
        }
    }

    #[test]
    fn postcondition_accepts_matching_str_type() {
        let mut map = HashMap::new();
        map.insert("s".into(), ConcreteValue::Str("hello".into()));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_rejects_int_for_str_param() {
        let mut map = HashMap::new();
        map.insert("s".into(), ConcreteValue::Int(42));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        assert!(!solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_accepts_int_for_unknown_type() {
        let mut map = HashMap::new();
        map.insert("x".into(), ConcreteValue::Int(0));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Unknown,
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_accepts_nullable_inner_match() {
        let mut map = HashMap::new();
        map.insert("s".into(), ConcreteValue::Str("hello".into()));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        let params = vec![ParamInfo {
            name: "s".into(),
            typ: TypeInfo::Nullable {
                inner: Box::new(TypeInfo::Str),
            },
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_trivially_true_for_unsat() {
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Unsat);
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_trivially_true_for_error() {
        let result: Result<SolveResult, SolverError> = Err(SolverError::Unsat);
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: TypeInfo::Str,
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    #[test]
    fn postcondition_ignores_unknown_solved_vars() {
        let mut map = HashMap::new();
        map.insert("config.timeout".into(), ConcreteValue::Int(30));
        let result: Result<SolveResult, SolverError> = Ok(SolveResult::Sat(map));
        // No param named "config.timeout" — should pass
        let params = vec![ParamInfo {
            name: "config".into(),
            typ: TypeInfo::Object { fields: vec![] },
            type_name: None,
        }];
        assert!(solved_values_match_param_types(result.as_ref(), &params));
    }

    // ── solve_for_mcdc_independence tests ────────────────────────────────────

    /// Conditions: [x > 0, y < 10], observed [true, true], target=0
    /// Expected: x <= 0 (flipped), y < 10 (pinned).
    #[test]
    fn mcdc_flip_first_condition() {
        // x > 0
        let cond_x = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        // y < 10
        let cond_y = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "y".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };
        let conditions = vec![cond_x, cond_y];
        let observed = vec![Some(true), Some(true)];

        let result = solve_for_mcdc_independence(&[], &conditions, &observed, 0, None, &[])
            .expect("solver should not error");

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
                // target (x > 0) was true → should now be false: x <= 0
                assert!(x <= 0, "expected x <= 0 (flipped condition), got x={x}");
                // non-target (y < 10) was true → should remain true: y < 10
                assert!(y < 10, "expected y < 10 (pinned condition), got y={y}");
            }
            SolveResult::Unsat => panic!("expected SAT, got UNSAT"),
        }
    }

    /// Conditions: [x > 0, y < 10], observed [true, true], target=1
    /// Expected: x > 0 (pinned), y >= 10 (flipped).
    #[test]
    fn mcdc_flip_second_condition() {
        let cond_x = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        let cond_y = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "y".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };
        let conditions = vec![cond_x, cond_y];
        let observed = vec![Some(true), Some(true)];

        let result = solve_for_mcdc_independence(&[], &conditions, &observed, 1, None, &[])
            .expect("solver should not error");

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
                // non-target (x > 0) was true → pinned true: x > 0
                assert!(x > 0, "expected x > 0 (pinned), got x={x}");
                // target (y < 10) was true → flipped: y >= 10
                assert!(y >= 10, "expected y >= 10 (flipped), got y={y}");
            }
            SolveResult::Unsat => panic!("expected SAT, got UNSAT"),
        }
    }

    /// Coupled conditions (x > 0) and (x > 5): if observed both true and we
    /// try to flip (x > 5) while pinning (x > 0 = true), we need x <= 5 AND x > 0,
    /// which is satisfiable (e.g. x = 3).
    #[test]
    fn mcdc_coupled_conditions_sat() {
        // x > 0
        let cond_a = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        // x > 5
        let cond_b = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(5))),
        };
        let conditions = vec![cond_a, cond_b];
        // Both observed true (x > 5 implies x > 0).
        let observed = vec![Some(true), Some(true)];

        // Flip target=1 (x > 5): assert x > 0 (pinned) AND NOT (x > 5).
        // Satisfiable: 0 < x <= 5.
        let result = solve_for_mcdc_independence(&[], &conditions, &observed, 1, None, &[])
            .expect("solver should not error");

        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x > 0 && x <= 5, "expected 0 < x <= 5, got x={x}");
            }
            SolveResult::Unsat => panic!("expected SAT for 0 < x <= 5"),
        }
    }

    /// True UNSAT case: conditions (x > 0) and (x <= 0) with both observed true —
    /// impossible, but we test by requiring them both pinned to impossible values.
    /// Flip target=0: assert NOT (x > 0) [flip], AND (x <= 0) [pin true].
    /// NOT (x > 0) ≡ x <= 0, AND x <= 0 is just x <= 0 — SAT.
    /// Instead use a tighter UNSAT: conditions [x > 10, x < 5], observed [true, true].
    /// Pin x < 5 = true and flip x > 10 to NOT (x > 10) = x <= 10 → 0 < x < 5 SAT.
    /// For a genuine UNSAT: conditions [x == 3, x != 3], observed [true, false].
    /// target=0: flip (x == 3) to NOT (x == 3), pin (x != 3) = false → assert x == 3 AND x != 3 → UNSAT.
    #[test]
    fn mcdc_unsat_contradictory_pin_and_flip() {
        // x == 3
        let cond_eq = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(3))),
        };
        // x != 3
        let cond_ne = SymExpr::BinOp {
            op: BinOpKind::Ne,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(3))),
        };
        let conditions = vec![cond_eq, cond_ne];
        // observed: cond_eq = true (x==3), cond_ne = false (x==3 so x!=3 is false)
        let observed = vec![Some(true), Some(false)];

        // target=0: flip (x==3) to NOT(x==3)=x!=3, pin (x!=3) as false → assert x==3 AND x!=3 → UNSAT
        let result = solve_for_mcdc_independence(&[], &conditions, &observed, 0, None, &[])
            .expect("solver should not error");

        assert!(
            matches!(result, SolveResult::Unsat),
            "expected UNSAT for contradictory pin+flip, got {result:?}"
        );
    }

    /// Masked target: observed[target] is None → should return error.
    #[test]
    fn mcdc_masked_target_returns_error() {
        let cond = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        let conditions = vec![cond];
        let observed = vec![None]; // masked

        let result = solve_for_mcdc_independence(&[], &conditions, &observed, 0, None, &[]);
        assert!(
            result.is_err(),
            "expected error for masked target, got {result:?}"
        );
    }

    /// Out-of-bounds target_index returns error.
    #[test]
    fn mcdc_out_of_bounds_target_returns_error() {
        let cond = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
        };
        let conditions = vec![cond];
        let observed = vec![Some(true)];

        let result = solve_for_mcdc_independence(
            &[],
            &conditions,
            &observed,
            5, // out of bounds
            None,
            &[],
        );
        assert!(
            result.is_err(),
            "expected error for OOB target, got {result:?}"
        );
    }

    /// Proptest: valid inputs (non-masked target, non-Unknown conditions) never panic.
    #[cfg(test)]
    mod mcdc_proptest {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn no_panic_on_valid_conditions(
                val_a in any::<bool>(),
                val_b in any::<bool>(),
                target in 0usize..2,
                k in any::<i64>(),
            ) {
                let cond_a = SymExpr::BinOp {
                    op: BinOpKind::Gt,
                    left: Box::new(SymExpr::Param { name: "a".into(), path: vec![] }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(k))),
                };
                let cond_b = SymExpr::BinOp {
                    op: BinOpKind::Lt,
                    left: Box::new(SymExpr::Param { name: "b".into(), path: vec![] }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(k))),
                };
                let conditions = vec![cond_a, cond_b];
                let observed = vec![Some(val_a), Some(val_b)];
                // Should not panic — result may be SAT, UNSAT, or error.
                let _ = solve_for_mcdc_independence(
                    &[],
                    &conditions,
                    &observed,
                    target,
                    Some(1_000),
                    &[],
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Kani bounded model checking harnesses
// ---------------------------------------------------------------------------
// Separated from `#[cfg(test)]` — Kani runs its own verification passes.
// Each harness uses `kani::any()` for bounded symbolic inputs and proves
// invariants that proptest exercises probabilistically but cannot exhaustively
// guarantee.
//
// Run: `cd shatter-core && cargo kani --harness <name>`

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Build a non-recursive leaf `SymExpr` from a discriminant.
    /// Avoids heap-allocated Strings to keep CBMC's state space tractable.
    /// Covers all constant sorts (Int, Float, Bool, Str), Null, Undefined,
    /// Param, and Unknown — the full leaf vocabulary of `infer_sort`.
    fn leaf_sym_expr(tag: u8) -> SymExpr {
        match tag % 8 {
            0 => SymExpr::Const(ConstValue::Int(42)),
            1 => SymExpr::Const(ConstValue::Float(1.0)),
            2 => SymExpr::Const(ConstValue::Bool(true)),
            3 => SymExpr::Const(ConstValue::Bool(false)),
            4 => SymExpr::Const(ConstValue::Str(String::new())),
            5 => SymExpr::Const(ConstValue::Null),
            6 => SymExpr::Const(ConstValue::Undefined),
            7 => SymExpr::Param {
                name: String::new(),
                path: vec![],
            },
            _ => SymExpr::Unknown,
        }
    }

    // -- Harness 1: infer_sort returns a valid Sort for every leaf SymExpr ----
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_leaf_validity() {
        let tag: u8 = kani::any();
        kani::assume(tag < 8);
        let expr = leaf_sym_expr(tag);
        let sort = infer_sort(&expr);
        assert!(
            matches!(sort, Sort::Int | Sort::Real | Sort::Bool | Sort::Str),
            "infer_sort must return a valid Sort variant"
        );
    }

    // -- Harness 2: infer_sort constant → sort mapping is correct -------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_int_is_int() {
        assert_eq!(infer_sort(&SymExpr::Const(ConstValue::Int(0))), Sort::Int);
    }

    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_float_is_real() {
        assert_eq!(
            infer_sort(&SymExpr::Const(ConstValue::Float(0.0))),
            Sort::Real
        );
    }

    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_bool_is_bool() {
        let v: bool = kani::any();
        assert_eq!(infer_sort(&SymExpr::Const(ConstValue::Bool(v))), Sort::Bool);
    }

    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_str_is_str() {
        assert_eq!(
            infer_sort(&SymExpr::Const(ConstValue::Str(String::new()))),
            Sort::Str
        );
    }

    // -- Harness 3: logical And/Or always infer Bool --------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_logical_is_bool() {
        let use_and: bool = kani::any();
        let op = if use_and {
            BinOpKind::And
        } else {
            BinOpKind::Or
        };
        let left_tag: u8 = kani::any();
        let right_tag: u8 = kani::any();
        kani::assume(left_tag < 8);
        kani::assume(right_tag < 8);
        let expr = SymExpr::BinOp {
            op,
            left: Box::new(leaf_sym_expr(left_tag)),
            right: Box::new(leaf_sym_expr(right_tag)),
        };
        assert_eq!(
            infer_sort(&expr),
            Sort::Bool,
            "And/Or must always infer Bool"
        );
    }

    // -- Harness 4: Not always returns Bool -----------------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_not_is_bool() {
        let tag: u8 = kani::any();
        kani::assume(tag < 8);
        let expr = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(leaf_sym_expr(tag)),
        };
        assert_eq!(infer_sort(&expr), Sort::Bool, "Not must always infer Bool");
    }

    // -- Harness 5: TypeOf always returns Str ---------------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_typeof_is_str() {
        let tag: u8 = kani::any();
        kani::assume(tag < 8);
        let expr = SymExpr::UnOp {
            op: UnOpKind::TypeOf,
            operand: Box::new(leaf_sym_expr(tag)),
        };
        assert_eq!(infer_sort(&expr), Sort::Str, "TypeOf must always infer Str");
    }

    // -- Harness 6: Neg/BitwiseNot preserve operand sort ----------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_infer_sort_neg_preserves_sort() {
        let tag: u8 = kani::any();
        kani::assume(tag < 8);
        let leaf = leaf_sym_expr(tag);
        let expected = infer_sort(&leaf);
        let use_neg: bool = kani::any();
        let op = if use_neg {
            UnOpKind::Neg
        } else {
            UnOpKind::BitwiseNot
        };
        let expr = SymExpr::UnOp {
            op,
            operand: Box::new(leaf),
        };
        assert_eq!(
            infer_sort(&expr),
            expected,
            "Neg/BitwiseNot must preserve operand sort"
        );
    }
}
