//! Input triage: predict whether a candidate input will produce a novel path
//! without executing it.
//!
//! Contains a concrete evaluator for symbolic expressions ([`evaluate_constraint`])
//! and the [`TriageState`] that accumulates observed branch traces and predicts
//! verdicts for candidate inputs.

use std::collections::HashSet;

use serde_json::Value;

use crate::execution_record::{BranchDecision, SymConstraint};
use crate::orchestrator::hash_branch_path;
use crate::sym_expr::{BinOpKind, ConstValue, SymExpr, UnOpKind};

/// Maximum number of traces stored in [`TriageState`].
pub const MAX_TRACES: usize = 64;

/// Fraction of indeterminate branches above which a trace is too uncertain.
const INDETERMINATE_THRESHOLD: f64 = 0.5;

/// Every Nth skip verdict, execute anyway to validate the prediction.
pub const TRIAGE_SAMPLE_INTERVAL: usize = 20;

/// Minimum verdicts before evaluating whether triage is useful.
pub const MIN_VERDICTS_FOR_EVAL: usize = 20;

/// Minimum fraction of Skip verdicts for triage to remain active.
pub const MIN_SKIP_RATE: f64 = 0.10;

/// Maximum fraction of sampled skips that turned out wrong before disabling.
pub const MAX_MISPREDICTION_RATE: f64 = 0.25;

/// Predicted direction for a single branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchPrediction {
    Taken,
    NotTaken,
    Indeterminate,
}

/// Verdict from triaging a candidate input against observed traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageVerdict {
    /// Predicted path already covered — skip execution.
    Skip,
    /// At least one branch predicted to take a novel direction.
    Execute {
        novel_count: usize,
        first_novel_depth: usize,
    },
    /// Too many unknowns or no matching traces to predict.
    Indeterminate,
}

/// Why triage was disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageDisableReason {
    /// Fewer than [`MIN_SKIP_RATE`] of verdicts were Skip.
    LowSkipRate,
    /// More than [`MAX_MISPREDICTION_RATE`] of sampled skips were wrong.
    HighMisprediction,
}

/// Accumulates branch traces from observed executions and predicts whether
/// candidate inputs will produce novel paths.
pub struct TriageState {
    /// Observed branch traces, deduped by branch-ID sequence.
    traces: Vec<Vec<BranchDecision>>,
    /// Dedup keys: branch-ID sequences already stored.
    trace_keys: HashSet<Vec<u32>>,
    /// Per-branch observed (branch_id, taken) pairs.
    observed_directions: HashSet<(u32, bool)>,
    /// Parameter names for constraint evaluation.
    param_names: Vec<String>,
    /// Total verdicts issued.
    total_verdicts: usize,
    /// Number of Skip verdicts issued.
    skip_verdicts: usize,
    /// Number of sampled skip verdicts validated by actual execution.
    samples_taken: usize,
    /// Number of samples where predicted path hash differed from actual.
    mispredictions: usize,
    /// Set when triage disables itself.
    disable_reason: Option<TriageDisableReason>,
}

impl TriageState {
    pub fn new(param_names: Vec<String>) -> Self {
        Self {
            traces: Vec::new(),
            trace_keys: HashSet::new(),
            observed_directions: HashSet::new(),
            param_names,
            total_verdicts: 0,
            skip_verdicts: 0,
            samples_taken: 0,
            mispredictions: 0,
            disable_reason: None,
        }
    }

    /// Record a new execution trace. Deduplicates by branch-ID sequence and
    /// caps storage at [`MAX_TRACES`] (drops oldest on overflow).
    pub fn update(&mut self, branch_path: &[BranchDecision]) {
        // Record all observed directions.
        for d in branch_path {
            self.observed_directions.insert((d.branch_id, d.taken));
        }

        // Dedup by branch-ID sequence.
        let key: Vec<u32> = branch_path.iter().map(|d| d.branch_id).collect();
        if !self.trace_keys.insert(key) {
            return;
        }

        // Cap at MAX_TRACES — drop oldest.
        if self.traces.len() >= MAX_TRACES {
            let removed = self.traces.remove(0);
            let removed_key: Vec<u32> = removed.iter().map(|d| d.branch_id).collect();
            self.trace_keys.remove(&removed_key);
        }

        self.traces.push(branch_path.to_vec());
    }

    /// Number of stored traces.
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    /// Number of observed (branch_id, taken) pairs.
    pub fn observed_direction_count(&self) -> usize {
        self.observed_directions.len()
    }

    /// Whether triage has been disabled.
    pub fn is_disabled(&self) -> bool {
        self.disable_reason.is_some()
    }

    /// Why triage was disabled, if at all.
    pub fn disable_reason(&self) -> Option<TriageDisableReason> {
        self.disable_reason
    }

    /// Record a verdict and check whether triage should disable itself
    /// due to low skip rate. Call this after every `triage_candidate` call.
    pub fn record_verdict(&mut self, verdict: &TriageVerdict) {
        if self.is_disabled() {
            return;
        }
        self.total_verdicts += 1;
        if *verdict == TriageVerdict::Skip {
            self.skip_verdicts += 1;
        }
        if self.total_verdicts >= MIN_VERDICTS_FOR_EVAL {
            let skip_rate = self.skip_verdicts as f64 / self.total_verdicts as f64;
            if skip_rate < MIN_SKIP_RATE {
                self.disable_reason = Some(TriageDisableReason::LowSkipRate);
            }
        }
    }

    /// Whether the current skip verdict should be sampled (executed anyway)
    /// to validate prediction accuracy. Returns true every
    /// [`TRIAGE_SAMPLE_INTERVAL`]th skip verdict.
    pub fn should_sample(&self) -> bool {
        self.skip_verdicts > 0 && self.skip_verdicts.is_multiple_of(TRIAGE_SAMPLE_INTERVAL)
    }

    /// Record the result of a sampled skip execution. If the actual path hash
    /// differs from the predicted one, it counts as a misprediction.
    /// Disables triage when the misprediction rate exceeds
    /// [`MAX_MISPREDICTION_RATE`].
    pub fn record_sample(&mut self, predicted_path_hash: u64, actual_path_hash: u64) {
        if self.is_disabled() {
            return;
        }
        self.samples_taken += 1;
        if predicted_path_hash != actual_path_hash {
            self.mispredictions += 1;
        }
        let misprediction_rate = self.mispredictions as f64 / self.samples_taken as f64;
        if misprediction_rate > MAX_MISPREDICTION_RATE {
            self.disable_reason = Some(TriageDisableReason::HighMisprediction);
        }
    }

    /// Predict whether executing `inputs` would produce a novel path.
    /// Returns `Indeterminate` immediately if triage has been disabled.
    pub fn triage_candidate(
        &self,
        inputs: &[Value],
        covered_paths: &HashSet<u64>,
    ) -> TriageVerdict {
        if self.is_disabled() {
            return TriageVerdict::Indeterminate;
        }
        if self.traces.is_empty() {
            return TriageVerdict::Indeterminate;
        }

        let mut any_skip = false;

        for trace in &self.traces {
            if trace.is_empty() {
                continue;
            }

            let mut predicted_decisions = Vec::with_capacity(trace.len());
            let mut indeterminate_count = 0usize;
            let mut novel_count = 0usize;
            let mut first_novel_depth = None;

            for (depth, decision) in trace.iter().enumerate() {
                let prediction = predict_branch(decision, inputs, &self.param_names);

                let predicted_taken = match prediction {
                    BranchPrediction::Taken => true,
                    BranchPrediction::NotTaken => false,
                    BranchPrediction::Indeterminate => {
                        indeterminate_count += 1;
                        // Use original direction as fallback for path hash.
                        decision.taken
                    }
                };

                // Check if this predicted direction is novel.
                if prediction != BranchPrediction::Indeterminate
                    && !self
                        .observed_directions
                        .contains(&(decision.branch_id, predicted_taken))
                {
                    novel_count += 1;
                    if first_novel_depth.is_none() {
                        first_novel_depth = Some(depth);
                    }
                }

                predicted_decisions.push(BranchDecision {
                    branch_id: decision.branch_id,
                    line: decision.line,
                    taken: predicted_taken,
                    constraint: decision.constraint.clone(),
                    conditions: None,
                });
            }

            // Too many unknowns — skip this trace.
            let total = trace.len();
            if total > 0
                && (indeterminate_count as f64 / total as f64) > INDETERMINATE_THRESHOLD
            {
                continue;
            }

            // Novel direction detected.
            if novel_count > 0 {
                return TriageVerdict::Execute {
                    novel_count,
                    first_novel_depth: first_novel_depth.unwrap_or(0),
                };
            }

            // Check if predicted path is already covered.
            let path_hash = hash_branch_path(&predicted_decisions);
            if covered_paths.contains(&path_hash) {
                any_skip = true;
            } else {
                // Predicted path is not covered and no novel directions detected
                // from observed_directions perspective — but it's a new path hash,
                // so it's worth executing.
                return TriageVerdict::Execute {
                    novel_count: 0,
                    first_novel_depth: 0,
                };
            }
        }

        if any_skip {
            TriageVerdict::Skip
        } else {
            TriageVerdict::Indeterminate
        }
    }
}

/// Predict which direction a branch will take for the given inputs.
pub fn predict_branch(
    decision: &BranchDecision,
    inputs: &[Value],
    param_names: &[String],
) -> BranchPrediction {
    let expr = match &decision.constraint {
        SymConstraint::Expr { expr } => expr,
        SymConstraint::Unknown { .. } => return BranchPrediction::Indeterminate,
    };

    match evaluate_constraint(expr, inputs, param_names) {
        Some(val) => {
            if is_truthy(&val) {
                BranchPrediction::Taken
            } else {
                BranchPrediction::NotTaken
            }
        }
        None => BranchPrediction::Indeterminate,
    }
}

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
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::sym_expr::ConstValue;
    use crate::test_arbitraries::{arb_branch_decision, arb_json_value, arb_sym_expr};
    use proptest::prelude::*;
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

    // ========================================================================
    // TriageState and triage_candidate tests
    // ========================================================================

    fn make_decision(branch_id: u32, taken: bool, expr: SymExpr) -> BranchDecision {
        BranchDecision {
            branch_id,
            line: branch_id * 10,
            taken,
            constraint: SymConstraint::Expr { expr },
            conditions: None,
        }
    }

    fn make_unknown_decision(branch_id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id,
            line: branch_id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: "opaque".into(),
            },
            conditions: None,
        }
    }

    // x > 0
    fn x_gt_0() -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(param("x")),
            right: Box::new(int_const(0)),
        }
    }

    // x == 10
    fn x_eq_10() -> SymExpr {
        SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(param("x")),
            right: Box::new(int_const(10)),
        }
    }

    #[test]
    fn triage_state_trace_accumulation() {
        let mut state = TriageState::new(names(&["x"]));
        assert_eq!(state.trace_count(), 0);

        let trace = vec![make_decision(1, true, x_gt_0())];
        state.update(&trace);
        assert_eq!(state.trace_count(), 1);

        let trace2 = vec![
            make_decision(1, true, x_gt_0()),
            make_decision(2, false, x_eq_10()),
        ];
        state.update(&trace2);
        assert_eq!(state.trace_count(), 2);
    }

    #[test]
    fn triage_state_dedup_same_branch_id_sequence() {
        let mut state = TriageState::new(names(&["x"]));

        // Same branch IDs [1, 2] but different taken values.
        let trace_a = vec![
            make_decision(1, true, x_gt_0()),
            make_decision(2, true, x_eq_10()),
        ];
        let trace_b = vec![
            make_decision(1, false, x_gt_0()),
            make_decision(2, false, x_eq_10()),
        ];
        state.update(&trace_a);
        state.update(&trace_b);
        assert_eq!(state.trace_count(), 1);
    }

    #[test]
    fn triage_state_cap_at_64() {
        let mut state = TriageState::new(names(&["x"]));

        for i in 0..65u32 {
            let trace = vec![make_decision(i, true, x_gt_0())];
            state.update(&trace);
        }
        assert_eq!(state.trace_count(), MAX_TRACES);
    }

    #[test]
    fn triage_state_observed_directions_updated() {
        let mut state = TriageState::new(names(&["x"]));

        let trace = vec![
            make_decision(1, true, x_gt_0()),
            make_decision(2, false, x_eq_10()),
        ];
        state.update(&trace);

        assert_eq!(state.observed_direction_count(), 2);
        // The observed directions should include (1, true) and (2, false).
    }

    #[test]
    fn triage_verdict_indeterminate_no_traces() {
        let state = TriageState::new(names(&["x"]));
        let covered = HashSet::new();
        assert_eq!(
            state.triage_candidate(&[json!(5)], &covered),
            TriageVerdict::Indeterminate
        );
    }

    #[test]
    fn triage_verdict_skip_when_path_covered() {
        let mut state = TriageState::new(names(&["x"]));

        // Observe trace: branch 1 taken (x > 0)
        let trace = vec![make_decision(1, true, x_gt_0())];
        state.update(&trace);

        // Mark the predicted path as covered.
        // For x=5, branch 1 will predict Taken → hash matches the observed trace.
        let predicted = vec![BranchDecision {
            branch_id: 1,
            line: 10,
            taken: true,
            constraint: SymConstraint::Expr { expr: x_gt_0() },
            conditions: None,
        }];
        let path_hash = hash_branch_path(&predicted);
        let mut covered = HashSet::new();
        covered.insert(path_hash);

        assert_eq!(
            state.triage_candidate(&[json!(5)], &covered),
            TriageVerdict::Skip
        );
    }

    #[test]
    fn triage_verdict_execute_novel_direction() {
        let mut state = TriageState::new(names(&["x"]));

        // Observe only the taken=true direction for branch 1 (x > 0).
        let trace = vec![make_decision(1, true, x_gt_0())];
        state.update(&trace);

        // Candidate x=-1 predicts branch 1 NotTaken — novel direction.
        let covered = HashSet::new();
        let verdict = state.triage_candidate(&[json!(-1)], &covered);
        assert_eq!(
            verdict,
            TriageVerdict::Execute {
                novel_count: 1,
                first_novel_depth: 0,
            }
        );
    }

    #[test]
    fn triage_verdict_execute_novel_path_hash() {
        let mut state = TriageState::new(names(&["x"]));

        // Observe trace: branch 1 taken, branch 2 taken.
        let trace = vec![
            make_decision(1, true, x_gt_0()),
            make_decision(2, true, x_eq_10()),
        ];
        state.update(&trace);
        // Also observe the not-taken directions so they aren't "novel".
        state.observed_directions.insert((1, false));
        state.observed_directions.insert((2, false));

        // For x=-1: branch 1 NotTaken, branch 2 NotTaken.
        // Both directions are already observed, but the combination might be a new path hash.
        // With empty covered_paths, this should be Execute (new path hash).
        let covered = HashSet::new();
        let verdict = state.triage_candidate(&[json!(-1)], &covered);
        match verdict {
            TriageVerdict::Execute { .. } => {} // expected
            other => panic!("expected Execute, got {other:?}"),
        }
    }

    #[test]
    fn triage_verdict_indeterminate_too_many_unknowns() {
        let mut state = TriageState::new(names(&["x"]));

        // All branches have Unknown constraints → all indeterminate.
        let trace = vec![
            make_unknown_decision(1, true),
            make_unknown_decision(2, false),
            make_unknown_decision(3, true),
        ];
        state.update(&trace);

        let covered = HashSet::new();
        assert_eq!(
            state.triage_candidate(&[json!(5)], &covered),
            TriageVerdict::Indeterminate
        );
    }

    #[test]
    fn predict_branch_taken() {
        let decision = make_decision(1, true, x_gt_0());
        assert_eq!(
            predict_branch(&decision, &[json!(5)], &names(&["x"])),
            BranchPrediction::Taken
        );
    }

    #[test]
    fn predict_branch_not_taken() {
        let decision = make_decision(1, true, x_gt_0());
        assert_eq!(
            predict_branch(&decision, &[json!(-1)], &names(&["x"])),
            BranchPrediction::NotTaken
        );
    }

    #[test]
    fn predict_branch_indeterminate_unknown_constraint() {
        let decision = make_unknown_decision(1, true);
        assert_eq!(
            predict_branch(&decision, &[json!(5)], &names(&["x"])),
            BranchPrediction::Indeterminate
        );
    }

    #[test]
    fn predict_branch_indeterminate_unresolvable_expr() {
        // Constraint references param "y" but only "x" is provided.
        let expr = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "y".into(),
                path: vec![],
            }),
            right: Box::new(int_const(0)),
        };
        let decision = make_decision(1, true, expr);
        assert_eq!(
            predict_branch(&decision, &[json!(5)], &names(&["x"])),
            BranchPrediction::Indeterminate
        );
    }

    #[test]
    fn triage_dedup_preserves_observed_directions_from_both() {
        let mut state = TriageState::new(names(&["x"]));

        let trace_a = vec![make_decision(1, true, x_gt_0())];
        let trace_b = vec![make_decision(1, false, x_gt_0())];

        state.update(&trace_a);
        state.update(&trace_b); // deduped — but directions still recorded

        assert_eq!(state.trace_count(), 1);
        assert_eq!(state.observed_direction_count(), 2);
    }

    #[test]
    fn triage_multi_trace_first_novel_wins() {
        let mut state = TriageState::new(names(&["x"]));

        // Trace 1: branch 1 taken (x > 0)
        let trace1 = vec![make_decision(1, true, x_gt_0())];
        state.update(&trace1);

        // Trace 2: branch 1 taken, branch 2 taken (x > 0 && x == 10)
        let trace2 = vec![
            make_decision(1, true, x_gt_0()),
            make_decision(2, true, x_eq_10()),
        ];
        state.update(&trace2);

        // x=-1: trace1 predicts branch 1 NotTaken → novel.
        // Should return Execute from the first matching trace.
        let covered = HashSet::new();
        let verdict = state.triage_candidate(&[json!(-1)], &covered);
        assert_eq!(
            verdict,
            TriageVerdict::Execute {
                novel_count: 1,
                first_novel_depth: 0,
            }
        );
    }

    // --- Adaptive self-disabling ---

    #[test]
    fn no_disable_before_min_verdicts() {
        let mut state = TriageState::new(names(&["x"]));
        // Issue 19 Execute verdicts (all non-skip) — still below threshold.
        for _ in 0..MIN_VERDICTS_FOR_EVAL - 1 {
            state.record_verdict(&TriageVerdict::Execute {
                novel_count: 1,
                first_novel_depth: 0,
            });
        }
        assert!(!state.is_disabled());
    }

    #[test]
    fn low_skip_rate_disables_triage() {
        let mut state = TriageState::new(names(&["x"]));
        // 1 skip + 19 executes = 5% skip rate < MIN_SKIP_RATE (10%).
        state.record_verdict(&TriageVerdict::Skip);
        for _ in 0..MIN_VERDICTS_FOR_EVAL - 1 {
            state.record_verdict(&TriageVerdict::Execute {
                novel_count: 1,
                first_novel_depth: 0,
            });
        }
        assert!(state.is_disabled());
        assert_eq!(
            state.disable_reason(),
            Some(TriageDisableReason::LowSkipRate)
        );
    }

    #[test]
    fn sufficient_skip_rate_stays_enabled() {
        let mut state = TriageState::new(names(&["x"]));
        // 4 skips + 16 executes = 20% skip rate > MIN_SKIP_RATE (10%).
        for _ in 0..4 {
            state.record_verdict(&TriageVerdict::Skip);
        }
        for _ in 0..16 {
            state.record_verdict(&TriageVerdict::Execute {
                novel_count: 1,
                first_novel_depth: 0,
            });
        }
        assert!(!state.is_disabled());
    }

    #[test]
    fn should_sample_at_interval() {
        let mut state = TriageState::new(names(&["x"]));
        assert!(!state.should_sample()); // 0 skips

        for i in 1..=TRIAGE_SAMPLE_INTERVAL * 2 {
            state.record_verdict(&TriageVerdict::Skip);
            if i % TRIAGE_SAMPLE_INTERVAL == 0 {
                assert!(state.should_sample(), "should sample at skip #{i}");
            } else {
                assert!(!state.should_sample(), "should not sample at skip #{i}");
            }
        }
    }

    #[test]
    fn misprediction_disables_triage() {
        let mut state = TriageState::new(names(&["x"]));
        // 3 correct + 2 wrong = 40% misprediction > MAX_MISPREDICTION_RATE (25%).
        state.record_sample(100, 100);
        state.record_sample(100, 100);
        state.record_sample(100, 100);
        assert!(!state.is_disabled());

        state.record_sample(100, 200); // 1/4 = 25%, not > 25%
        assert!(!state.is_disabled());

        state.record_sample(100, 200); // 2/5 = 40% > 25%
        assert!(state.is_disabled());
        assert_eq!(
            state.disable_reason(),
            Some(TriageDisableReason::HighMisprediction)
        );
    }

    #[test]
    fn misprediction_within_tolerance_stays_enabled() {
        let mut state = TriageState::new(names(&["x"]));
        // 4 correct, 1 wrong → 20% misprediction < MAX_MISPREDICTION_RATE (25%).
        for _ in 0..4 {
            state.record_sample(100, 100);
        }
        state.record_sample(100, 200);
        assert!(!state.is_disabled());
    }

    #[test]
    fn disabled_triage_returns_indeterminate() {
        let mut state = TriageState::new(names(&["x"]));
        let trace = vec![make_decision(1, true, x_gt_0())];
        state.update(&trace);

        // Force disable via misprediction.
        state.record_sample(1, 2);
        assert!(state.is_disabled());

        let covered = HashSet::new();
        let verdict = state.triage_candidate(&[json!(5)], &covered);
        assert_eq!(verdict, TriageVerdict::Indeterminate);
    }

    #[test]
    fn record_verdict_noop_when_disabled() {
        let mut state = TriageState::new(names(&["x"]));
        state.record_sample(1, 2); // disables
        assert!(state.is_disabled());

        let before_total = state.total_verdicts;
        state.record_verdict(&TriageVerdict::Skip);
        assert_eq!(state.total_verdicts, before_total);
    }

    #[test]
    fn record_sample_noop_when_disabled() {
        let mut state = TriageState::new(names(&["x"]));
        state.record_sample(1, 2); // disables
        let before_samples = state.samples_taken;

        state.record_sample(3, 4);
        assert_eq!(state.samples_taken, before_samples);
    }

    #[test]
    fn indeterminate_verdicts_count_toward_total() {
        let mut state = TriageState::new(names(&["x"]));
        // All indeterminate = 0% skip rate.
        for _ in 0..MIN_VERDICTS_FOR_EVAL {
            state.record_verdict(&TriageVerdict::Indeterminate);
        }
        assert!(state.is_disabled());
        assert_eq!(
            state.disable_reason(),
            Some(TriageDisableReason::LowSkipRate)
        );
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    proptest! {
        /// evaluate_constraint is deterministic: same inputs always produce the
        /// same result.
        #[test]
        fn evaluate_constraint_deterministic(
            expr in arb_sym_expr(2),
            params in prop::collection::vec(arb_json_value(), 0..=3),
            param_names in prop::collection::vec("[a-z]{1,4}", 0..=3),
        ) {
            let r1 = evaluate_constraint(&expr, &params, &param_names);
            let r2 = evaluate_constraint(&expr, &params, &param_names);
            prop_assert_eq!(r1, r2);
        }

        /// evaluate_constraint never panics on arbitrary SymExpr trees.
        #[test]
        fn evaluate_constraint_no_panic(
            expr in arb_sym_expr(3),
            params in prop::collection::vec(arb_json_value(), 0..=4),
            param_names in prop::collection::vec("[a-z]{1,4}", 0..=4),
        ) {
            let _ = evaluate_constraint(&expr, &params, &param_names);
        }

        /// predict_branch result is consistent with evaluate_constraint.
        #[test]
        fn predict_branch_consistent_with_evaluator(
            decision in arb_branch_decision(),
            params in prop::collection::vec(arb_json_value(), 0..=3),
            param_names in prop::collection::vec("[a-z]{1,4}", 0..=3),
        ) {
            let prediction = predict_branch(&decision, &params, &param_names);
            let expr = match &decision.constraint {
                SymConstraint::Expr { expr } => expr,
                SymConstraint::Unknown { .. } => {
                    prop_assert_eq!(prediction, BranchPrediction::Indeterminate);
                    return Ok(());
                }
            };
            match evaluate_constraint(expr, &params, &param_names) {
                Some(val) => {
                    let expected = if is_truthy(&val) {
                        BranchPrediction::Taken
                    } else {
                        BranchPrediction::NotTaken
                    };
                    prop_assert_eq!(prediction, expected);
                }
                None => {
                    prop_assert_eq!(prediction, BranchPrediction::Indeterminate);
                }
            }
        }

        /// trace_count() never exceeds MAX_TRACES.
        #[test]
        fn triage_state_trace_count_bounded(
            traces in prop::collection::vec(
                prop::collection::vec(arb_branch_decision(), 1..=5),
                0..=100,
            ),
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            for trace in &traces {
                state.update(trace);
            }
            prop_assert!(state.trace_count() <= MAX_TRACES);
        }

        /// observed_direction_count monotonically increases with updates.
        #[test]
        fn observed_directions_monotonic(
            traces in prop::collection::vec(
                prop::collection::vec(arb_branch_decision(), 1..=3),
                1..=10,
            ),
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            let mut prev_count = 0;
            for trace in &traces {
                state.update(trace);
                let new_count = state.observed_direction_count();
                prop_assert!(new_count >= prev_count);
                prev_count = new_count;
            }
        }

        /// Duplicate traces don't increase trace_count.
        #[test]
        fn update_idempotent_for_same_branch_ids(
            trace in prop::collection::vec(arb_branch_decision(), 1..=5),
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            state.update(&trace);
            let count_after_first = state.trace_count();
            state.update(&trace);
            prop_assert_eq!(state.trace_count(), count_after_first);
        }

        /// Once disabled, triage_candidate always returns Indeterminate.
        #[test]
        fn disabled_always_indeterminate(
            params in prop::collection::vec(arb_json_value(), 1..=3),
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            // Force disable via misprediction.
            state.record_sample(1, 2);
            prop_assert!(state.is_disabled());

            let covered = HashSet::new();
            let verdict = state.triage_candidate(&params, &covered);
            prop_assert_eq!(verdict, TriageVerdict::Indeterminate);
        }

        /// With no traces, verdict is always Indeterminate.
        #[test]
        fn empty_traces_indeterminate(
            params in prop::collection::vec(arb_json_value(), 0..=3),
        ) {
            let state = TriageState::new(vec!["x".into()]);
            let covered = HashSet::new();
            let verdict = state.triage_candidate(&params, &covered);
            prop_assert_eq!(verdict, TriageVerdict::Indeterminate);
        }

        /// Recording only non-Skip verdicts eventually disables triage
        /// (once MIN_VERDICTS_FOR_EVAL is reached).
        #[test]
        fn all_execute_verdicts_disable(
            extra in 0..50usize,
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            let total = MIN_VERDICTS_FOR_EVAL + extra;
            for _ in 0..total {
                state.record_verdict(&TriageVerdict::Execute {
                    novel_count: 1,
                    first_novel_depth: 0,
                });
            }
            prop_assert!(state.is_disabled());
            prop_assert_eq!(
                state.disable_reason(),
                Some(TriageDisableReason::LowSkipRate)
            );
        }

        /// Once disabled, record_verdict and record_sample are no-ops —
        /// counters freeze.
        #[test]
        fn disabled_freezes_counters(
            n_verdicts in 1..20usize,
            n_samples in 1..10usize,
        ) {
            let mut state = TriageState::new(vec!["x".into()]);
            state.record_sample(1, 2); // disables
            prop_assert!(state.is_disabled());

            let tv = state.total_verdicts;
            let st = state.samples_taken;
            for _ in 0..n_verdicts {
                state.record_verdict(&TriageVerdict::Skip);
            }
            for _ in 0..n_samples {
                state.record_sample(0, 1);
            }
            prop_assert_eq!(state.total_verdicts, tv);
            prop_assert_eq!(state.samples_taken, st);
        }
    }
}
