//! Concrete evaluator for symbolic expressions.
//!
//! Walks a [`SymExpr`] tree and evaluates it against concrete JSON parameter
//! values. Returns `None` for `Unknown` nodes, unresolvable params, or
//! unsupported operations. Used by the symbolic triage system to classify
//! constraints without invoking Z3.

use serde_json::Value;

use crate::sym_expr::{BinOpKind, ConstValue, SymExpr, UnOpKind};

/// Evaluate a symbolic expression against concrete parameter values.
///
/// `params` are positional JSON values for each parameter.
/// `param_names` maps parameter names to their index in `params`.
/// Returns `None` when the expression contains `Unknown` nodes, references
/// unresolvable parameters, or uses unsupported operations.
pub fn evaluate_constraint(
    expr: &SymExpr,
    params: &[Value],
    param_names: &[String],
) -> Option<Value> {
    match expr {
        SymExpr::Param { name, path } => resolve_param(name, path, params, param_names),
        SymExpr::Const(c) => Some(const_to_json(c)),
        SymExpr::BinOp { op, left, right } => eval_binop(*op, left, right, params, param_names),
        SymExpr::UnOp { op, operand } => eval_unop(*op, operand, params, param_names),
        SymExpr::Call {
            name,
            receiver,
            args,
        } => eval_call(name, receiver.as_deref(), args, params, param_names),
        SymExpr::Unknown => None,
    }
}

/// Resolve a parameter reference to a concrete JSON value.
fn resolve_param(
    name: &str,
    path: &[String],
    params: &[Value],
    param_names: &[String],
) -> Option<Value> {
    let idx = param_names.iter().position(|n| n == name)?;
    let mut val = params.get(idx)?;
    for segment in path {
        val = val.get(segment.as_str())?;
    }
    Some(val.clone())
}

/// Convert a `ConstValue` to a `serde_json::Value`.
fn const_to_json(c: &ConstValue) -> Value {
    match c {
        ConstValue::Int(i) => Value::from(*i),
        ConstValue::Float(f) => Value::from(*f),
        ConstValue::Str(s) => Value::from(s.as_str()),
        ConstValue::Bool(b) => Value::from(*b),
        ConstValue::Null | ConstValue::Undefined => Value::Null,
        ConstValue::Complex { repr, .. } => const_to_json(repr),
    }
}

/// JS-like truthiness: null, false, 0, 0.0, "" are falsy; everything else truthy.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i != 0
            } else if let Some(f) = n.as_f64() {
                f != 0.0
            } else {
                true
            }
        }
        Value::String(s) => !s.is_empty(),
        // Arrays and objects are always truthy in JS
        _ => true,
    }
}

/// Extract an f64 from a JSON number value.
fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// Evaluate a binary operation.
fn eval_binop(
    op: BinOpKind,
    left: &SymExpr,
    right: &SymExpr,
    params: &[Value],
    param_names: &[String],
) -> Option<Value> {
    // Short-circuit logical ops
    match op {
        BinOpKind::And => {
            let lv = evaluate_constraint(left, params, param_names)?;
            if !is_truthy(&lv) {
                return Some(lv);
            }
            return evaluate_constraint(right, params, param_names);
        }
        BinOpKind::Or => {
            let lv = evaluate_constraint(left, params, param_names)?;
            if is_truthy(&lv) {
                return Some(lv);
            }
            return evaluate_constraint(right, params, param_names);
        }
        _ => {}
    }

    let lv = evaluate_constraint(left, params, param_names)?;
    let rv = evaluate_constraint(right, params, param_names)?;

    match op {
        // Comparisons
        BinOpKind::Eq => eval_eq(&lv, &rv).map(Value::from),
        BinOpKind::Ne => eval_eq(&lv, &rv).map(|eq| Value::from(!eq)),
        BinOpKind::Lt => eval_order(&lv, &rv).map(|ord| Value::from(ord.is_lt())),
        BinOpKind::Le => eval_order(&lv, &rv).map(|ord| Value::from(ord.is_le())),
        BinOpKind::Gt => eval_order(&lv, &rv).map(|ord| Value::from(ord.is_gt())),
        BinOpKind::Ge => eval_order(&lv, &rv).map(|ord| Value::from(ord.is_ge())),

        // Arithmetic
        BinOpKind::Add => eval_add(&lv, &rv),
        BinOpKind::Sub => eval_arith(&lv, &rv, |a, b| a - b),
        BinOpKind::Mul => eval_arith(&lv, &rv, |a, b| a * b),
        BinOpKind::Div => {
            let b = as_f64(&rv)?;
            if b == 0.0 {
                return None;
            }
            eval_arith(&lv, &rv, |a, b| a / b)
        }
        BinOpKind::Mod => {
            let b = as_f64(&rv)?;
            if b == 0.0 {
                return None;
            }
            eval_arith(&lv, &rv, |a, b| a % b)
        }

        // Unsupported for triage
        BinOpKind::And | BinOpKind::Or => unreachable!(),
        BinOpKind::BitwiseAnd
        | BinOpKind::BitwiseOr
        | BinOpKind::BitwiseXor
        | BinOpKind::Shl
        | BinOpKind::Shr
        | BinOpKind::BitClear
        | BinOpKind::In
        | BinOpKind::InstanceOf => None,
    }
}

/// Equality comparison with JS-like semantics.
fn eval_eq(a: &Value, b: &Value) -> Option<bool> {
    match (a, b) {
        (Value::Null, Value::Null) => Some(true),
        (Value::Bool(x), Value::Bool(y)) => Some(x == y),
        (Value::Number(_), Value::Number(_)) => {
            let fa = as_f64(a)?;
            let fb = as_f64(b)?;
            Some(fa == fb)
        }
        (Value::String(x), Value::String(y)) => Some(x == y),
        _ => None,
    }
}

/// Ordering comparison for numbers and strings.
fn eval_order(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => {
            let fa = as_f64(a)?;
            let fb = as_f64(b)?;
            fa.partial_cmp(&fb)
        }
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Add: numeric addition or string concatenation.
fn eval_add(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::String(x), Value::String(y)) => {
            let mut result = x.clone();
            result.push_str(y);
            Some(Value::from(result))
        }
        _ => eval_arith(a, b, |x, y| x + y),
    }
}

/// Arithmetic on numeric JSON values.
fn eval_arith(a: &Value, b: &Value, f: fn(f64, f64) -> f64) -> Option<Value> {
    let fa = as_f64(a)?;
    let fb = as_f64(b)?;
    let result = f(fa, fb);
    // Preserve integer results when both inputs are integers and the result is exact
    if a.is_i64() && b.is_i64() && let Some(i) = i64_from_f64(result) {
        return Some(Value::from(i));
    }
    Some(Value::from(result))
}

/// Safely convert f64 to i64 when the value is an exact integer.
fn i64_from_f64(f: f64) -> Option<i64> {
    if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
        Some(f as i64)
    } else {
        None
    }
}

/// Evaluate a unary operation.
fn eval_unop(
    op: UnOpKind,
    operand: &SymExpr,
    params: &[Value],
    param_names: &[String],
) -> Option<Value> {
    let val = evaluate_constraint(operand, params, param_names)?;
    match op {
        UnOpKind::Not => Some(Value::from(!is_truthy(&val))),
        UnOpKind::Neg => {
            let f = as_f64(&val)?;
            if val.is_i64() && let Some(i) = i64_from_f64(-f) {
                return Some(Value::from(i));
            }
            Some(Value::from(-f))
        }
        UnOpKind::BitwiseNot | UnOpKind::TypeOf => None,
    }
}

/// Evaluate a string method/function call against concrete values.
///
/// Supports the 8 canonical string operations defined in `data/string-ops.yaml`,
/// matching all cross-language aliases.
fn eval_call(
    name: &str,
    receiver: Option<&SymExpr>,
    args: &[SymExpr],
    params: &[Value],
    param_names: &[String],
) -> Option<Value> {
    match classify_string_op(name) {
        Some(StringOp::Contains) => {
            let (haystack, needle) = eval_string_pair(name, receiver, args, params, param_names)?;
            Some(Value::from(haystack.contains(needle.as_str())))
        }
        Some(StringOp::Prefix) => {
            let (haystack, needle) = eval_string_pair(name, receiver, args, params, param_names)?;
            Some(Value::from(haystack.starts_with(needle.as_str())))
        }
        Some(StringOp::Suffix) => {
            let (haystack, needle) = eval_string_pair(name, receiver, args, params, param_names)?;
            Some(Value::from(haystack.ends_with(needle.as_str())))
        }
        Some(StringOp::IndexOf) => {
            let (haystack, needle) = eval_string_pair(name, receiver, args, params, param_names)?;
            let idx = haystack
                .find(needle.as_str())
                .map_or(-1i64, |i| i as i64);
            Some(Value::from(idx))
        }
        Some(StringOp::Length) => {
            // receiver-style: "str".length  OR  free-style: len("str")
            let s = if let Some(recv) = receiver {
                eval_receiver_str(recv, params, param_names)?
            } else {
                eval_arg_str(args.first()?, params, param_names)?
            };
            Some(Value::from(s.len() as i64))
        }
        Some(StringOp::CharAt) => {
            let recv = eval_receiver_str(receiver?, params, param_names)?;
            let idx_val = evaluate_constraint(args.first()?, params, param_names)?;
            let idx = idx_val.as_i64()? as usize;
            let ch = recv.chars().nth(idx)?;
            Some(Value::from(ch.to_string()))
        }
        Some(StringOp::Substr) => {
            let recv = eval_receiver_str(receiver?, params, param_names)?;
            let start_val = evaluate_constraint(args.first()?, params, param_names)?;
            let start = start_val.as_i64()?.max(0) as usize;
            if start >= recv.len() {
                return Some(Value::from(""));
            }
            if let Some(end_expr) = args.get(1) {
                let end_val = evaluate_constraint(end_expr, params, param_names)?;
                let end = (end_val.as_i64()?.max(0) as usize).min(recv.len());
                if end <= start {
                    return Some(Value::from(""));
                }
                Some(Value::from(&recv[start..end]))
            } else {
                Some(Value::from(&recv[start..]))
            }
        }
        Some(StringOp::Concat) => {
            let recv = eval_receiver_str(receiver?, params, param_names)?;
            let arg = eval_arg_str(args.first()?, params, param_names)?;
            let mut result = recv;
            result.push_str(&arg);
            Some(Value::from(result))
        }
        None => None,
    }
}

/// Canonical string operations matching `data/string-ops.yaml`.
enum StringOp {
    Contains,
    Prefix,
    Suffix,
    IndexOf,
    Length,
    CharAt,
    Substr,
    Concat,
}

/// Map a call name to its canonical string operation.
fn classify_string_op(name: &str) -> Option<StringOp> {
    match name {
        "includes" | "Contains" | "strings.Contains" | "contains" => Some(StringOp::Contains),
        "startsWith" | "HasPrefix" | "strings.HasPrefix" | "starts_with" => Some(StringOp::Prefix),
        "endsWith" | "HasSuffix" | "strings.HasSuffix" | "ends_with" => Some(StringOp::Suffix),
        "indexOf" | "Index" | "strings.Index" | "find" | "index_of" => Some(StringOp::IndexOf),
        "length" | "len" => Some(StringOp::Length),
        "charAt" | "char_at" => Some(StringOp::CharAt),
        "slice" | "substring" | "substr" => Some(StringOp::Substr),
        "concat" => Some(StringOp::Concat),
        _ => None,
    }
}

/// Evaluate a receiver expression as a string.
fn eval_receiver_str(
    receiver: &SymExpr,
    params: &[Value],
    param_names: &[String],
) -> Option<String> {
    let val = evaluate_constraint(receiver, params, param_names)?;
    val.as_str().map(String::from)
}

/// Evaluate an argument expression as a string.
fn eval_arg_str(arg: &SymExpr, params: &[Value], param_names: &[String]) -> Option<String> {
    let val = evaluate_constraint(arg, params, param_names)?;
    val.as_str().map(String::from)
}

/// For operations that take receiver + first arg as strings (or first two args for free-style).
fn eval_string_pair(
    _name: &str,
    receiver: Option<&SymExpr>,
    args: &[SymExpr],
    params: &[Value],
    param_names: &[String],
) -> Option<(String, String)> {
    if let Some(recv) = receiver {
        // receiver-style: recv.method(arg)
        let haystack = eval_receiver_str(recv, params, param_names)?;
        let needle = eval_arg_str(args.first()?, params, param_names)?;
        Some((haystack, needle))
    } else {
        // free-style: method(haystack, needle)
        let haystack = eval_arg_str(args.first()?, params, param_names)?;
        let needle = eval_arg_str(args.get(1)?, params, param_names)?;
        Some((haystack, needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym_expr::ConstValue;
    use serde_json::json;

    fn names(ns: &[&str]) -> Vec<String> {
        ns.iter().map(|s| s.to_string()).collect()
    }

    fn param(name: &str) -> SymExpr {
        SymExpr::Param {
            name: name.into(),
            path: vec![],
        }
    }

    fn param_path(name: &str, path: &[&str]) -> SymExpr {
        SymExpr::Param {
            name: name.into(),
            path: path.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn int_const(i: i64) -> SymExpr {
        SymExpr::Const(ConstValue::Int(i))
    }

    fn str_const(s: &str) -> SymExpr {
        SymExpr::Const(ConstValue::Str(s.into()))
    }

    fn bool_const(b: bool) -> SymExpr {
        SymExpr::Const(ConstValue::Bool(b))
    }

    // --- Param resolution ---

    #[test]
    fn param_lookup_by_name() {
        let result = evaluate_constraint(&param("x"), &[json!(42)], &names(&["x"]));
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn param_lookup_second_param() {
        let result = evaluate_constraint(
            &param("y"),
            &[json!(1), json!("hello")],
            &names(&["x", "y"]),
        );
        assert_eq!(result, Some(json!("hello")));
    }

    #[test]
    fn param_with_field_path() {
        let obj = json!({"timeout": 30, "host": "localhost"});
        let result = evaluate_constraint(
            &param_path("config", &["timeout"]),
            &[obj],
            &names(&["config"]),
        );
        assert_eq!(result, Some(json!(30)));
    }

    #[test]
    fn param_nested_field_path() {
        let obj = json!({"a": {"b": {"c": true}}});
        let result = evaluate_constraint(
            &param_path("x", &["a", "b", "c"]),
            &[obj],
            &names(&["x"]),
        );
        assert_eq!(result, Some(json!(true)));
    }

    #[test]
    fn param_missing_name_returns_none() {
        let result = evaluate_constraint(&param("missing"), &[json!(1)], &names(&["x"]));
        assert_eq!(result, None);
    }

    #[test]
    fn param_missing_field_returns_none() {
        let obj = json!({"a": 1});
        let result = evaluate_constraint(
            &param_path("x", &["nonexistent"]),
            &[obj],
            &names(&["x"]),
        );
        assert_eq!(result, None);
    }

    // --- ConstValue ---

    #[test]
    fn const_int() {
        let result = evaluate_constraint(&int_const(42), &[], &[]);
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn const_float() {
        let result = evaluate_constraint(
            &SymExpr::Const(ConstValue::Float(3.14)),
            &[],
            &[],
        );
        assert_eq!(result, Some(json!(3.14)));
    }

    #[test]
    fn const_string() {
        let result = evaluate_constraint(&str_const("hello"), &[], &[]);
        assert_eq!(result, Some(json!("hello")));
    }

    #[test]
    fn const_bool() {
        assert_eq!(evaluate_constraint(&bool_const(true), &[], &[]), Some(json!(true)));
        assert_eq!(evaluate_constraint(&bool_const(false), &[], &[]), Some(json!(false)));
    }

    #[test]
    fn const_null_and_undefined() {
        assert_eq!(
            evaluate_constraint(&SymExpr::Const(ConstValue::Null), &[], &[]),
            Some(Value::Null)
        );
        assert_eq!(
            evaluate_constraint(&SymExpr::Const(ConstValue::Undefined), &[], &[]),
            Some(Value::Null)
        );
    }

    #[test]
    fn const_complex_uses_repr() {
        use crate::types::ComplexKind;
        let expr = SymExpr::Const(ConstValue::Complex {
            kind: ComplexKind::Date,
            repr: Box::new(ConstValue::Int(1704067200000)),
        });
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(1704067200000i64)));
    }

    // --- Unknown ---

    #[test]
    fn unknown_returns_none() {
        assert_eq!(evaluate_constraint(&SymExpr::Unknown, &[], &[]), None);
    }

    // --- Comparisons ---

    #[test]
    fn binop_eq_integers() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(param("x")),
            right: Box::new(int_const(10)),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!(10)], &names(&["x"])),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!(5)], &names(&["x"])),
            Some(json!(false))
        );
    }

    #[test]
    fn binop_ne_strings() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Ne,
            left: Box::new(param("s")),
            right: Box::new(str_const("hello")),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("world")], &names(&["s"])),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["s"])),
            Some(json!(false))
        );
    }

    #[test]
    fn binop_lt_gt_le_ge() {
        let params = &[json!(5)];
        let pn = &names(&["x"]);

        let lt = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(param("x")),
            right: Box::new(int_const(10)),
        };
        assert_eq!(evaluate_constraint(&lt, params, pn), Some(json!(true)));

        let gt = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(param("x")),
            right: Box::new(int_const(10)),
        };
        assert_eq!(evaluate_constraint(&gt, params, pn), Some(json!(false)));

        let le = SymExpr::BinOp {
            op: BinOpKind::Le,
            left: Box::new(param("x")),
            right: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&le, params, pn), Some(json!(true)));

        let ge = SymExpr::BinOp {
            op: BinOpKind::Ge,
            left: Box::new(param("x")),
            right: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&ge, params, pn), Some(json!(true)));
    }

    #[test]
    fn binop_eq_null() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(param("x")),
            right: Box::new(SymExpr::Const(ConstValue::Null)),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[Value::Null], &names(&["x"])),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!(0)], &names(&["x"])),
            None
        );
    }

    #[test]
    fn binop_mixed_types_returns_none() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(param("x")),
            right: Box::new(str_const("hello")),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!(5)], &names(&["x"])),
            None
        );
    }

    // --- Arithmetic ---

    #[test]
    fn binop_add_integers() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Add,
            left: Box::new(param("x")),
            right: Box::new(int_const(3)),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!(7)], &names(&["x"])),
            Some(json!(10))
        );
    }

    #[test]
    fn binop_add_strings() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Add,
            left: Box::new(str_const("hello ")),
            right: Box::new(str_const("world")),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!("hello world")));
    }

    #[test]
    fn binop_sub_mul() {
        let sub = SymExpr::BinOp {
            op: BinOpKind::Sub,
            left: Box::new(int_const(10)),
            right: Box::new(int_const(3)),
        };
        assert_eq!(evaluate_constraint(&sub, &[], &[]), Some(json!(7)));

        let mul = SymExpr::BinOp {
            op: BinOpKind::Mul,
            left: Box::new(int_const(4)),
            right: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&mul, &[], &[]), Some(json!(20)));
    }

    #[test]
    fn binop_div_and_mod() {
        let div = SymExpr::BinOp {
            op: BinOpKind::Div,
            left: Box::new(int_const(10)),
            right: Box::new(int_const(3)),
        };
        // 10 / 3 = 3.333... → f64
        let result = evaluate_constraint(&div, &[], &[]);
        assert!(result.is_some());
        let f = result.unwrap().as_f64().unwrap();
        assert!((f - 10.0 / 3.0).abs() < f64::EPSILON);

        let modop = SymExpr::BinOp {
            op: BinOpKind::Mod,
            left: Box::new(int_const(10)),
            right: Box::new(int_const(3)),
        };
        assert_eq!(evaluate_constraint(&modop, &[], &[]), Some(json!(1)));
    }

    #[test]
    fn binop_div_by_zero_returns_none() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Div,
            left: Box::new(int_const(10)),
            right: Box::new(int_const(0)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }

    #[test]
    fn binop_mod_by_zero_returns_none() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Mod,
            left: Box::new(int_const(10)),
            right: Box::new(int_const(0)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }

    // --- Logical ---

    #[test]
    fn binop_and_short_circuit() {
        // false && Unknown → false (short-circuit, doesn't evaluate right)
        let expr = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(bool_const(false)),
            right: Box::new(SymExpr::Unknown),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(false)));
    }

    #[test]
    fn binop_and_evaluates_right_when_truthy() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(bool_const(true)),
            right: Box::new(int_const(42)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(42)));
    }

    #[test]
    fn binop_or_short_circuit() {
        // true || Unknown → true
        let expr = SymExpr::BinOp {
            op: BinOpKind::Or,
            left: Box::new(bool_const(true)),
            right: Box::new(SymExpr::Unknown),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(true)));
    }

    #[test]
    fn binop_or_evaluates_right_when_falsy() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Or,
            left: Box::new(bool_const(false)),
            right: Box::new(int_const(99)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(99)));
    }

    // --- UnOp ---

    #[test]
    fn unop_not() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(bool_const(true)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(false)));

        let expr2 = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(int_const(0)),
        };
        assert_eq!(evaluate_constraint(&expr2, &[], &[]), Some(json!(true)));
    }

    #[test]
    fn unop_neg() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::Neg,
            operand: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!(-5)));
    }

    #[test]
    fn unop_bitwise_not_returns_none() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::BitwiseNot,
            operand: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }

    #[test]
    fn unop_typeof_returns_none() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::TypeOf,
            operand: Box::new(param("x")),
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["x"])),
            None
        );
    }

    // --- String calls ---

    #[test]
    fn call_contains_receiver_style() {
        let expr = SymExpr::Call {
            name: "includes".into(),
            receiver: Some(Box::new(param("s"))),
            args: vec![str_const("world")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello world")], &names(&["s"])),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["s"])),
            Some(json!(false))
        );
    }

    #[test]
    fn call_contains_free_style() {
        // Go-style: strings.Contains(haystack, needle)
        let expr = SymExpr::Call {
            name: "strings.Contains".into(),
            receiver: None,
            args: vec![param("s"), str_const("x")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("axb")], &names(&["s"])),
            Some(json!(true))
        );
    }

    #[test]
    fn call_starts_with() {
        let expr = SymExpr::Call {
            name: "startsWith".into(),
            receiver: Some(Box::new(param("s"))),
            args: vec![str_const("he")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["s"])),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!("world")], &names(&["s"])),
            Some(json!(false))
        );
    }

    #[test]
    fn call_ends_with() {
        let expr = SymExpr::Call {
            name: "endsWith".into(),
            receiver: Some(Box::new(param("s"))),
            args: vec![str_const("ld")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("world")], &names(&["s"])),
            Some(json!(true))
        );
    }

    #[test]
    fn call_index_of() {
        let expr = SymExpr::Call {
            name: "indexOf".into(),
            receiver: Some(Box::new(param("s"))),
            args: vec![str_const("ll")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["s"])),
            Some(json!(2))
        );
        // Not found → -1
        assert_eq!(
            evaluate_constraint(&expr, &[json!("world")], &names(&["s"])),
            Some(json!(-1))
        );
    }

    #[test]
    fn call_length_receiver_style() {
        let expr = SymExpr::Call {
            name: "length".into(),
            receiver: Some(Box::new(param("s"))),
            args: vec![],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello")], &names(&["s"])),
            Some(json!(5))
        );
    }

    #[test]
    fn call_length_free_style() {
        // Go-style: len("hello")
        let expr = SymExpr::Call {
            name: "len".into(),
            receiver: None,
            args: vec![param("s")],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("abc")], &names(&["s"])),
            Some(json!(3))
        );
    }

    #[test]
    fn call_char_at() {
        let expr = SymExpr::Call {
            name: "charAt".into(),
            receiver: Some(Box::new(str_const("hello"))),
            args: vec![int_const(1)],
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!("e")));
    }

    #[test]
    fn call_substr() {
        let expr = SymExpr::Call {
            name: "substring".into(),
            receiver: Some(Box::new(str_const("hello world"))),
            args: vec![int_const(6), int_const(11)],
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!("world")));
    }

    #[test]
    fn call_substr_no_end() {
        let expr = SymExpr::Call {
            name: "slice".into(),
            receiver: Some(Box::new(str_const("hello"))),
            args: vec![int_const(2)],
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!("llo")));
    }

    #[test]
    fn call_concat() {
        let expr = SymExpr::Call {
            name: "concat".into(),
            receiver: Some(Box::new(str_const("hello "))),
            args: vec![str_const("world")],
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), Some(json!("hello world")));
    }

    #[test]
    fn call_unknown_name_returns_none() {
        let expr = SymExpr::Call {
            name: "unknownMethod".into(),
            receiver: Some(Box::new(param("x"))),
            args: vec![],
        };
        assert_eq!(
            evaluate_constraint(&expr, &[json!("test")], &names(&["x"])),
            None
        );
    }

    // --- Nested expressions ---

    #[test]
    fn nested_and_with_comparisons() {
        // x > 0 && x < 100
        let expr = SymExpr::BinOp {
            op: BinOpKind::And,
            left: Box::new(SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(param("x")),
                right: Box::new(int_const(0)),
            }),
            right: Box::new(SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(param("x")),
                right: Box::new(int_const(100)),
            }),
        };
        let pn = &names(&["x"]);
        // x=50 → true && true → true
        assert_eq!(
            evaluate_constraint(&expr, &[json!(50)], pn),
            Some(json!(true))
        );
        // x=0 → false (short-circuit)
        assert_eq!(
            evaluate_constraint(&expr, &[json!(0)], pn),
            Some(json!(false))
        );
        // x=200 → true && false → false
        assert_eq!(
            evaluate_constraint(&expr, &[json!(200)], pn),
            Some(json!(false))
        );
    }

    #[test]
    fn nested_string_length_comparison() {
        // s.length > 5
        let expr = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Call {
                name: "length".into(),
                receiver: Some(Box::new(param("s"))),
                args: vec![],
            }),
            right: Box::new(int_const(5)),
        };
        let pn = &names(&["s"]);
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hello world")], pn),
            Some(json!(true))
        );
        assert_eq!(
            evaluate_constraint(&expr, &[json!("hi")], pn),
            Some(json!(false))
        );
    }

    // --- Bitwise ops return None ---

    #[test]
    fn bitwise_ops_return_none() {
        for op in [
            BinOpKind::BitwiseAnd,
            BinOpKind::BitwiseOr,
            BinOpKind::BitwiseXor,
            BinOpKind::Shl,
            BinOpKind::Shr,
            BinOpKind::BitClear,
        ] {
            let expr = SymExpr::BinOp {
                op,
                left: Box::new(int_const(5)),
                right: Box::new(int_const(3)),
            };
            assert_eq!(evaluate_constraint(&expr, &[], &[]), None, "op {op:?} should return None");
        }
    }

    #[test]
    fn in_and_instanceof_return_none() {
        for op in [BinOpKind::In, BinOpKind::InstanceOf] {
            let expr = SymExpr::BinOp {
                op,
                left: Box::new(str_const("key")),
                right: Box::new(param("obj")),
            };
            assert_eq!(
                evaluate_constraint(&expr, &[json!({})], &names(&["obj"])),
                None,
            );
        }
    }

    // --- Truthiness ---

    #[test]
    fn truthiness_edge_cases() {
        // Empty string is falsy
        let not_empty = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(str_const("")),
        };
        assert_eq!(evaluate_constraint(&not_empty, &[], &[]), Some(json!(true)));

        // Zero is falsy
        let not_zero = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(int_const(0)),
        };
        assert_eq!(evaluate_constraint(&not_zero, &[], &[]), Some(json!(true)));

        // Non-empty string is truthy
        let not_str = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(str_const("a")),
        };
        assert_eq!(evaluate_constraint(&not_str, &[], &[]), Some(json!(false)));
    }

    // --- Propagation of None ---

    #[test]
    fn unknown_in_binop_propagates_none() {
        let expr = SymExpr::BinOp {
            op: BinOpKind::Add,
            left: Box::new(SymExpr::Unknown),
            right: Box::new(int_const(5)),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }

    #[test]
    fn unknown_in_unop_propagates_none() {
        let expr = SymExpr::UnOp {
            op: UnOpKind::Not,
            operand: Box::new(SymExpr::Unknown),
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }

    #[test]
    fn string_op_with_non_string_returns_none() {
        let expr = SymExpr::Call {
            name: "includes".into(),
            receiver: Some(Box::new(int_const(42))),
            args: vec![str_const("x")],
        };
        assert_eq!(evaluate_constraint(&expr, &[], &[]), None);
    }
}
