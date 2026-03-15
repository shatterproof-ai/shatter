//! Value shrinking for minimal witness discovery and boundary refinement.
//!
//! Given a value and its `TypeInfo`, produces progressively simpler variants.
//! Conceptual inverse of `mutate_value` in `input_gen.rs`: mutation goes toward
//! novelty, shrinking goes toward simplicity.

use serde_json::{json, Value};

use crate::orchestrator::hash_branch_path;
use crate::protocol::ExecuteResult;
use crate::types::{ParamInfo, TypeInfo};

/// Result of shrinking a witness to its minimal form.
#[derive(Debug, Clone)]
pub struct ShrinkResult {
    /// The (possibly reduced) inputs.
    pub inputs: Vec<Value>,
    /// Total execute calls made during shrinking.
    pub attempts: usize,
    /// Whether any parameter was actually shrunk smaller.
    pub shrunk: bool,
}

/// Shrink a witness to the simplest inputs that still produce the same branch path.
///
/// Uses a QuickCheck-style strategy: try shrinking one parameter at a time,
/// accepting the first candidate that preserves the target branch path.
/// Repeats until no progress or budget exhausted.
///
/// `execute_fn` is called for each trial — it should run the function with
/// the given inputs and return the `ExecuteResult`. Errors from `execute_fn`
/// are treated as "candidate rejected" (the trial is skipped, not fatal).
pub fn shrink_witness(
    inputs: &[Value],
    param_infos: &[ParamInfo],
    target_path_hash: u64,
    max_attempts: usize,
    mut execute_fn: impl FnMut(&[Value]) -> Result<ExecuteResult, Box<dyn std::error::Error>>,
) -> ShrinkResult {
    let original = inputs.to_vec();
    let mut current = original.clone();
    let mut attempts = 0;
    let mut progress = true;

    while progress && attempts < max_attempts {
        progress = false;
        for i in 0..param_infos.len().min(current.len()) {
            let candidates = shrink_candidates(&current[i], &param_infos[i].typ);
            for candidate in candidates {
                if attempts >= max_attempts {
                    return ShrinkResult {
                        shrunk: current != original,
                        inputs: current,
                        attempts,
                    };
                }
                let mut trial = current.clone();
                trial[i] = candidate;
                attempts += 1;
                match execute_fn(&trial) {
                    Ok(result) if hash_branch_path(&result.branch_path) == target_path_hash => {
                        current = trial;
                        progress = true;
                        break;
                    }
                    _ => {} // Candidate rejected or execution error — skip
                }
            }
            if attempts >= max_attempts {
                break;
            }
        }
    }

    ShrinkResult {
        shrunk: current != original,
        inputs: current,
        attempts,
    }
}

// Boundary values used as shrink targets for numeric types.
const SHRINK_INT_ZERO: i64 = 0;
const SHRINK_INT_ONE: i64 = 1;
const SHRINK_INT_NEG_ONE: i64 = -1;

const SHRINK_FLOAT_ZERO: f64 = 0.0;
const SHRINK_FLOAT_ONE: f64 = 1.0;
const SHRINK_FLOAT_NEG_ONE: f64 = -1.0;

/// Produce simpler variants of `value` that still conform to `type_info`.
///
/// Never includes the original value. Returns an empty vec for types that
/// cannot be meaningfully shrunk (Complex, Opaque, Unknown) or values that
/// are already minimal.
pub fn shrink_candidates(value: &Value, type_info: &TypeInfo) -> Vec<Value> {
    let mut candidates = match type_info {
        TypeInfo::Int => shrink_int(value),
        TypeInfo::Float => shrink_float(value),
        TypeInfo::Str => shrink_string(value),
        TypeInfo::Bool => shrink_bool(value),
        TypeInfo::Array { element } => shrink_array(value, element),
        TypeInfo::Object { fields } => shrink_object(value, fields),
        TypeInfo::Nullable { inner } => shrink_nullable(value, inner),
        TypeInfo::Union { variants } => shrink_union(value, variants),
        TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => Vec::new(),
    };

    // Remove duplicates and the original value.
    candidates.retain(|c| c != value);
    dedup_values(&mut candidates);
    candidates
}

/// Deduplicate a vec of Values, preserving order (keeps first occurrence).
fn dedup_values(values: &mut Vec<Value>) {
    let mut seen = Vec::with_capacity(values.len());
    values.retain(|v| {
        if seen.contains(v) {
            false
        } else {
            seen.push(v.clone());
            true
        }
    });
}

fn shrink_int(value: &Value) -> Vec<Value> {
    let n = match value.as_i64() {
        Some(n) => n,
        None => return vec![json!(SHRINK_INT_ZERO)],
    };

    let mut out = Vec::with_capacity(4);

    // Halve toward zero.
    if n != 0 {
        out.push(json!(n / 2));
    }

    out.push(json!(SHRINK_INT_ZERO));
    out.push(json!(SHRINK_INT_ONE));
    out.push(json!(SHRINK_INT_NEG_ONE));

    out
}

fn shrink_float(value: &Value) -> Vec<Value> {
    let n = match value.as_f64() {
        Some(n) => n,
        None => return vec![json!(SHRINK_FLOAT_ZERO)],
    };

    let mut out = Vec::with_capacity(4);

    // Halve toward zero.
    if n != 0.0 {
        out.push(json!(n / 2.0));
    }

    out.push(json!(SHRINK_FLOAT_ZERO));
    out.push(json!(SHRINK_FLOAT_ONE));
    out.push(json!(SHRINK_FLOAT_NEG_ONE));

    out
}

fn shrink_string(value: &Value) -> Vec<Value> {
    let s = match value.as_str() {
        Some(s) => s,
        None => return vec![json!("")],
    };

    let mut out = Vec::with_capacity(4);

    // Remove last character.
    if !s.is_empty() {
        let without_last: String = s.chars().take(s.chars().count() - 1).collect();
        out.push(json!(without_last));
    }

    // Remove first character.
    if s.chars().count() > 1 {
        let without_first: String = s.chars().skip(1).collect();
        out.push(json!(without_first));
    }

    // Empty string.
    out.push(json!(""));

    // Single first character.
    if let Some(ch) = s.chars().next()
        && s.chars().count() > 1
    {
        out.push(json!(ch.to_string()));
    }

    out
}

fn shrink_bool(value: &Value) -> Vec<Value> {
    match value.as_bool() {
        Some(true) => vec![json!(false)],
        _ => Vec::new(),
    }
}

fn shrink_array(value: &Value, element: &TypeInfo) -> Vec<Value> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return vec![json!([])],
    };

    if arr.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(3 + arr.len());

    // Remove last element.
    if arr.len() > 1 {
        out.push(Value::Array(arr[..arr.len() - 1].to_vec()));
    }

    // Remove first element.
    if arr.len() > 1 {
        out.push(Value::Array(arr[1..].to_vec()));
    }

    // Empty array.
    out.push(json!([]));

    // Shrink individual elements in place.
    for (i, elem) in arr.iter().enumerate() {
        for shrunk in shrink_candidates(elem, element) {
            let mut new_arr = arr.clone();
            new_arr[i] = shrunk;
            out.push(Value::Array(new_arr));
        }
    }

    out
}

fn shrink_object(value: &Value, fields: &[(String, TypeInfo)]) -> Vec<Value> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut out = Vec::with_capacity(fields.len());

    // Remove each field one at a time.
    for (field_name, _) in fields {
        if obj.contains_key(field_name) {
            let mut shrunk = obj.clone();
            shrunk.remove(field_name);
            out.push(Value::Object(shrunk));
        }
    }

    // Shrink individual field values in place.
    for (field_name, field_type) in fields {
        if let Some(field_val) = obj.get(field_name) {
            for shrunk_val in shrink_candidates(field_val, field_type) {
                let mut new_obj = obj.clone();
                new_obj.insert(field_name.clone(), shrunk_val);
                out.push(Value::Object(new_obj));
            }
        }
    }

    out
}

fn shrink_nullable(value: &Value, inner: &TypeInfo) -> Vec<Value> {
    let mut out = Vec::with_capacity(4);

    // Always try null.
    out.push(Value::Null);

    // If not already null, also shrink the inner value.
    if !value.is_null() {
        out.extend(shrink_candidates(value, inner));
    }

    out
}

fn shrink_union(value: &Value, variants: &[TypeInfo]) -> Vec<Value> {
    let mut out = Vec::new();

    // Try shrinking against each variant — collect all candidates.
    for variant in variants {
        out.extend(shrink_candidates(value, variant));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Int
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_int_positive() {
        let candidates = shrink_candidates(&json!(10), &TypeInfo::Int);
        assert!(candidates.contains(&json!(5))); // halve
        assert!(candidates.contains(&json!(0)));
        assert!(candidates.contains(&json!(1)));
        assert!(candidates.contains(&json!(-1)));
        assert!(!candidates.contains(&json!(10))); // never original
    }

    #[test]
    fn shrink_int_negative() {
        let candidates = shrink_candidates(&json!(-8), &TypeInfo::Int);
        assert!(candidates.contains(&json!(-4))); // halve toward zero
        assert!(candidates.contains(&json!(0)));
    }

    #[test]
    fn shrink_int_zero_already_minimal() {
        let candidates = shrink_candidates(&json!(0), &TypeInfo::Int);
        assert!(!candidates.contains(&json!(0)));
        // Still offers 1 and -1 as alternatives.
        assert!(candidates.contains(&json!(1)));
        assert!(candidates.contains(&json!(-1)));
    }

    #[test]
    fn shrink_int_one_no_duplicate() {
        let candidates = shrink_candidates(&json!(1), &TypeInfo::Int);
        assert!(!candidates.contains(&json!(1)));
        let count = candidates.iter().filter(|c| **c == json!(0)).count();
        assert_eq!(count, 1, "no duplicate zeros");
    }

    // -----------------------------------------------------------------------
    // Float
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_float_positive() {
        let candidates = shrink_candidates(&json!(4.0), &TypeInfo::Float);
        assert!(candidates.contains(&json!(2.0)));
        assert!(candidates.contains(&json!(0.0)));
        assert!(!candidates.contains(&json!(4.0)));
    }

    #[test]
    fn shrink_float_zero_already_minimal() {
        let candidates = shrink_candidates(&json!(0.0), &TypeInfo::Float);
        assert!(!candidates.contains(&json!(0.0)));
    }

    // -----------------------------------------------------------------------
    // String
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_string_multi_char() {
        let candidates = shrink_candidates(&json!("hello"), &TypeInfo::Str);
        assert!(candidates.contains(&json!("hell"))); // drop last
        assert!(candidates.contains(&json!("ello"))); // drop first
        assert!(candidates.contains(&json!(""))); // empty
        assert!(candidates.contains(&json!("h"))); // first char
        assert!(!candidates.contains(&json!("hello")));
    }

    #[test]
    fn shrink_string_single_char() {
        let candidates = shrink_candidates(&json!("x"), &TypeInfo::Str);
        assert!(candidates.contains(&json!("")));
        // Removing last char of "x" gives "" which is already in the list.
        assert!(!candidates.contains(&json!("x")));
    }

    #[test]
    fn shrink_string_empty_already_minimal() {
        let candidates = shrink_candidates(&json!(""), &TypeInfo::Str);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Bool
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_bool_true() {
        let candidates = shrink_candidates(&json!(true), &TypeInfo::Bool);
        assert_eq!(candidates, vec![json!(false)]);
    }

    #[test]
    fn shrink_bool_false_already_minimal() {
        let candidates = shrink_candidates(&json!(false), &TypeInfo::Bool);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Array
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_array_multiple_elements() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        let candidates = shrink_candidates(&json!([1, 2, 3]), &typ);
        assert!(candidates.contains(&json!([1, 2]))); // drop last
        assert!(candidates.contains(&json!([2, 3]))); // drop first
        assert!(candidates.contains(&json!([]))); // empty
        assert!(!candidates.contains(&json!([1, 2, 3])));
    }

    #[test]
    fn shrink_array_single_element() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        let candidates = shrink_candidates(&json!([5]), &typ);
        assert!(candidates.contains(&json!([]))); // empty
        // Also contains element-shrunk variants like [0], [1], [-1].
        assert!(candidates.contains(&json!([0])));
    }

    #[test]
    fn shrink_array_empty_already_minimal() {
        let typ = TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        };
        let candidates = shrink_candidates(&json!([]), &typ);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Object
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_object_removes_fields() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("a".into(), TypeInfo::Int),
                ("b".into(), TypeInfo::Str),
            ],
        };
        let val = json!({"a": 10, "b": "hi"});
        let candidates = shrink_candidates(&val, &typ);

        // Should have field-removal candidates.
        assert!(candidates.contains(&json!({"b": "hi"}))); // removed "a"
        assert!(candidates.contains(&json!({"a": 10}))); // removed "b"

        // Should also have field-value-shrunk candidates.
        assert!(candidates.contains(&json!({"a": 5, "b": "hi"}))); // shrunk "a"
        assert!(candidates.contains(&json!({"a": 10, "b": "h"}))); // shrunk "b"

        assert!(!candidates.contains(&val));
    }

    #[test]
    fn shrink_object_empty_fields() {
        let typ = TypeInfo::Object { fields: vec![] };
        let candidates = shrink_candidates(&json!({}), &typ);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Nullable
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_nullable_non_null() {
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        };
        let candidates = shrink_candidates(&json!(10), &typ);
        assert!(candidates.contains(&json!(null)));
        // Also has inner-type shrinks.
        assert!(candidates.contains(&json!(5)));
        assert!(candidates.contains(&json!(0)));
    }

    #[test]
    fn shrink_nullable_already_null() {
        let typ = TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        };
        let candidates = shrink_candidates(&json!(null), &typ);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Union
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_union_collects_from_variants() {
        let typ = TypeInfo::Union {
            variants: vec![TypeInfo::Int, TypeInfo::Str],
        };
        let candidates = shrink_candidates(&json!(10), &typ);
        // Int shrinks.
        assert!(candidates.contains(&json!(5)));
        assert!(candidates.contains(&json!(0)));
        // Str shrinks produce nothing useful for a non-string value — that's fine.
    }

    // -----------------------------------------------------------------------
    // Complex / Opaque / Unknown
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_complex_returns_empty() {
        let typ = TypeInfo::Complex {
            kind: crate::types::ComplexKind::Date,
            metadata: Default::default(),
            inner: None,
        };
        let candidates = shrink_candidates(&json!("2026-01-01"), &typ);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_opaque_returns_empty() {
        let typ = TypeInfo::Opaque {
            label: "net.Socket".into(),
        };
        let candidates = shrink_candidates(&json!(null), &typ);
        assert!(candidates.is_empty());
    }

    #[test]
    fn shrink_unknown_returns_empty() {
        let candidates = shrink_candidates(&json!(42), &TypeInfo::Unknown);
        assert!(candidates.is_empty());
    }

    // -----------------------------------------------------------------------
    // Invariants
    // -----------------------------------------------------------------------

    #[test]
    fn candidates_never_contain_original() {
        let cases: Vec<(Value, TypeInfo)> = vec![
            (json!(0), TypeInfo::Int),
            (json!(1), TypeInfo::Int),
            (json!(-1), TypeInfo::Int),
            (json!(42), TypeInfo::Int),
            (json!(0.0), TypeInfo::Float),
            (json!(3.14), TypeInfo::Float),
            (json!(""), TypeInfo::Str),
            (json!("a"), TypeInfo::Str),
            (json!("hello"), TypeInfo::Str),
            (json!(true), TypeInfo::Bool),
            (json!(false), TypeInfo::Bool),
            (json!(null), TypeInfo::Nullable { inner: Box::new(TypeInfo::Int) }),
            (json!([]), TypeInfo::Array { element: Box::new(TypeInfo::Int) }),
            (json!([1, 2]), TypeInfo::Array { element: Box::new(TypeInfo::Int) }),
        ];

        for (val, typ) in &cases {
            let candidates = shrink_candidates(val, typ);
            assert!(
                !candidates.contains(val),
                "candidates for {:?} should not contain original",
                val
            );
        }
    }

    #[test]
    fn no_duplicate_candidates() {
        let cases: Vec<(Value, TypeInfo)> = vec![
            (json!(10), TypeInfo::Int),
            (json!("hello"), TypeInfo::Str),
            (json!([1, 2, 3]), TypeInfo::Array { element: Box::new(TypeInfo::Int) }),
        ];

        for (val, typ) in &cases {
            let candidates = shrink_candidates(val, typ);
            for (i, a) in candidates.iter().enumerate() {
                for (j, b) in candidates.iter().enumerate() {
                    if i != j {
                        assert_ne!(a, b, "duplicate candidate in shrink of {:?}", val);
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Property-based tests
    // -------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;
        use serde_json::json;

        proptest! {
            #[test]
            fn shrink_never_contains_original_int(n in any::<i64>()) {
                let val = json!(n);
                let candidates = shrink_candidates(&val, &TypeInfo::Int);
                prop_assert!(
                    !candidates.contains(&val),
                    "candidates for {val:?} should not contain original"
                );
            }

            #[test]
            fn shrink_never_contains_original_float(
                n in (-1000i32..1000).prop_map(|n| f64::from(n))
            ) {
                let val = json!(n);
                let candidates = shrink_candidates(&val, &TypeInfo::Float);
                prop_assert!(
                    !candidates.contains(&val),
                    "candidates for {val:?} should not contain original"
                );
            }

            #[test]
            fn shrink_never_contains_original_str(s in ".{0,20}") {
                let val = json!(s);
                let candidates = shrink_candidates(&val, &TypeInfo::Str);
                prop_assert!(
                    !candidates.contains(&val),
                    "candidates for {val:?} should not contain original"
                );
            }

            #[test]
            fn shrink_int_candidates_are_ints(n in any::<i64>()) {
                let val = json!(n);
                for c in shrink_candidates(&val, &TypeInfo::Int) {
                    prop_assert!(
                        c.is_i64() || c.is_u64(),
                        "shrink candidate {c:?} is not an int"
                    );
                }
            }

            #[test]
            fn shrink_float_candidates_are_floats(
                n in (-1000i32..1000).prop_map(|n| f64::from(n))
            ) {
                let val = json!(n);
                for c in shrink_candidates(&val, &TypeInfo::Float) {
                    prop_assert!(
                        c.is_f64() || c.is_i64() || c.is_u64(),
                        "shrink candidate {c:?} is not a float"
                    );
                }
            }

            #[test]
            fn shrink_str_candidates_are_strings(s in ".{0,20}") {
                let val = json!(s);
                for c in shrink_candidates(&val, &TypeInfo::Str) {
                    prop_assert!(c.is_string(),
                        "shrink candidate {c:?} is not a string");
                }
            }

            #[test]
            fn shrink_int_abs_leq_or_boundary(n in -1_000_000i64..1_000_000i64) {
                let val = json!(n);
                let abs_n = n.unsigned_abs();
                for c in shrink_candidates(&val, &TypeInfo::Int) {
                    let c_n = c.as_i64().unwrap();
                    let is_boundary = c_n == 0 || c_n == 1 || c_n == -1;
                    prop_assert!(
                        c_n.unsigned_abs() <= abs_n || is_boundary,
                        "shrink candidate {c_n} has |c| > |{n}| and is not a boundary"
                    );
                }
            }

            #[test]
            fn shrink_str_len_leq_original(s in ".{0,30}") {
                let val = json!(s);
                let orig_len = s.chars().count();
                for c in shrink_candidates(&val, &TypeInfo::Str) {
                    let c_str = c.as_str().unwrap();
                    prop_assert!(
                        c_str.chars().count() <= orig_len,
                        "shrink candidate {:?} longer than original {:?}",
                        c_str, s
                    );
                }
            }

            #[test]
            fn shrink_array_len_leq_original(len in 0..6usize) {
                let arr: Vec<Value> = (0..len).map(|i| json!(i as i64)).collect();
                let val = Value::Array(arr);
                let typ = TypeInfo::Array { element: Box::new(TypeInfo::Int) };

                for c in shrink_candidates(&val, &typ) {
                    let c_arr = c.as_array().unwrap();
                    prop_assert!(
                        c_arr.len() <= len,
                        "shrink candidate array len {} > original len {}",
                        c_arr.len(), len
                    );
                }
            }

            #[test]
            fn shrink_no_duplicates_int(n in any::<i64>()) {
                let candidates = shrink_candidates(&json!(n), &TypeInfo::Int);
                for (i, a) in candidates.iter().enumerate() {
                    for (j, b) in candidates.iter().enumerate() {
                        if i != j {
                            prop_assert!(a != b, "duplicate candidates for {}", n);
                        }
                    }
                }
            }
        }

        #[test]
        fn shrink_zero_int_minimal() {
            let candidates = shrink_candidates(&json!(0), &TypeInfo::Int);
            assert!(!candidates.contains(&json!(0)));
            for c in &candidates {
                let n = c.as_i64().unwrap();
                assert!(n.abs() <= 1, "candidate {n} is not minimal");
            }
        }

        #[test]
        fn shrink_empty_string_minimal() {
            let candidates = shrink_candidates(&json!(""), &TypeInfo::Str);
            assert!(candidates.is_empty());
        }

        #[test]
        fn shrink_false_minimal() {
            let candidates = shrink_candidates(&json!(false), &TypeInfo::Bool);
            assert!(candidates.is_empty());
        }

        #[test]
        fn shrink_null_nullable_minimal() {
            let typ = TypeInfo::Nullable {
                inner: Box::new(TypeInfo::Int),
            };
            let candidates = shrink_candidates(&json!(null), &typ);
            assert!(candidates.is_empty());
        }

        #[test]
        fn shrink_empty_array_minimal() {
            let typ = TypeInfo::Array {
                element: Box::new(TypeInfo::Int),
            };
            let candidates = shrink_candidates(&json!([]), &typ);
            assert!(candidates.is_empty());
        }
    }

    // -------------------------------------------------------------------
    // shrink_witness tests
    // -------------------------------------------------------------------

    mod witness_tests {
        use super::*;
        use crate::execution_record::{BranchDecision, SymConstraint};
        use crate::protocol::{ExecuteResult, PerformanceMetrics};
        use serde_json::json;

        fn empty_perf() -> PerformanceMetrics {
            PerformanceMetrics {
                wall_time_ms: 0.0,
                cpu_time_us: 0,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            }
        }

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
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                performance: empty_perf(),
            }
        }

        fn branch_taken() -> Vec<BranchDecision> {
            vec![BranchDecision {
                branch_id: 1,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
            }]
        }

        fn branch_not_taken() -> Vec<BranchDecision> {
            vec![BranchDecision {
                branch_id: 1,
                line: 5,
                taken: false,
                constraint: SymConstraint::default(),
            }]
        }

        #[test]
        fn shrink_int_toward_one() {
            let target_path = branch_taken();
            let target_hash = hash_branch_path(&target_path);

            let params = vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }];

            let result = shrink_witness(
                &[json!(100)],
                &params,
                target_hash,
                50,
                |inputs| {
                    let x = inputs[0].as_i64().unwrap_or(0);
                    if x > 0 {
                        Ok(make_result(branch_taken()))
                    } else {
                        Ok(make_result(branch_not_taken()))
                    }
                },
            );

            assert!(result.shrunk);
            assert_eq!(result.inputs[0], json!(1));
            assert!(result.attempts <= 50);
        }

        #[test]
        fn shrink_multi_param() {
            let target_path = branch_taken();
            let target_hash = hash_branch_path(&target_path);

            let params = vec![
                ParamInfo {
                    name: "x".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                },
                ParamInfo {
                    name: "s".into(),
                    typ: TypeInfo::Str,
                    type_name: None,
                },
            ];

            let result = shrink_witness(
                &[json!(100), json!("hello world")],
                &params,
                target_hash,
                100,
                |inputs| {
                    let x = inputs[0].as_i64().unwrap_or(0);
                    let s = inputs[1].as_str().unwrap_or("");
                    if x > 0 && !s.is_empty() {
                        Ok(make_result(branch_taken()))
                    } else {
                        Ok(make_result(branch_not_taken()))
                    }
                },
            );

            assert!(result.shrunk);
            assert_eq!(result.inputs[0], json!(1));
            // String should be shrunk to a single character
            let s = result.inputs[1].as_str().unwrap();
            assert!(s.len() <= 2, "expected short string, got {:?}", s);
            assert!(!s.is_empty());
        }

        #[test]
        fn shrink_already_minimal() {
            let target_path = branch_taken();
            let target_hash = hash_branch_path(&target_path);

            let params = vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }];

            let result = shrink_witness(
                &[json!(0)],
                &params,
                target_hash,
                20,
                |_inputs| Ok(make_result(branch_not_taken())),
            );

            assert!(!result.shrunk);
            assert_eq!(result.inputs, vec![json!(0)]);
        }

        #[test]
        fn shrink_respects_budget() {
            let target_path = branch_taken();
            let target_hash = hash_branch_path(&target_path);

            let params = vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }];

            let result = shrink_witness(
                &[json!(1000)],
                &params,
                target_hash,
                3,
                |_inputs| Ok(make_result(branch_not_taken())),
            );

            assert_eq!(result.attempts, 3);
        }

        #[test]
        fn shrink_handles_execute_errors() {
            let target_path = branch_taken();
            let target_hash = hash_branch_path(&target_path);

            let params = vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }];

            let result = shrink_witness(
                &[json!(100)],
                &params,
                target_hash,
                10,
                |_inputs| -> Result<ExecuteResult, Box<dyn std::error::Error>> {
                    Err("frontend crashed".into())
                },
            );

            assert!(!result.shrunk);
            assert!(result.attempts > 0);
        }
    }

    mod witness_prop_tests {
        use super::*;
        use crate::execution_record::{BranchDecision, SymConstraint};
        use crate::protocol::{ExecuteResult, PerformanceMetrics};
        use proptest::prelude::*;

        fn empty_perf() -> PerformanceMetrics {
            PerformanceMetrics {
                wall_time_ms: 0.0,
                cpu_time_us: 0,
                heap_used_bytes: 0,
                heap_allocated_bytes: 0,
            }
        }

        fn make_result(taken: bool) -> ExecuteResult {
            ExecuteResult {
                return_value: None,
                thrown_error: None,
                branch_path: vec![BranchDecision {
                    branch_id: 1,
                    line: 5,
                    taken,
                    constraint: SymConstraint::default(),
                }],
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                performance: empty_perf(),
            }
        }

        proptest! {
            #[test]
            fn shrunk_witness_attempts_bounded(
                max_attempts in 1..30usize,
                start_val in 1..1000i64,
            ) {
                let target = make_result(true);
                let target_hash = hash_branch_path(&target.branch_path);

                let params = vec![ParamInfo {
                    name: "x".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }];

                let result = shrink_witness(
                    &[json!(start_val)],
                    &params,
                    target_hash,
                    max_attempts,
                    |_| Ok(make_result(false)),
                );

                prop_assert!(
                    result.attempts <= max_attempts,
                    "attempts {} > budget {}",
                    result.attempts,
                    max_attempts
                );
            }

            #[test]
            fn shrunk_witness_preserves_path(start_val in 2..500i64) {
                let target = make_result(true);
                let target_hash = hash_branch_path(&target.branch_path);

                let params = vec![ParamInfo {
                    name: "x".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }];

                let result = shrink_witness(
                    &[json!(start_val)],
                    &params,
                    target_hash,
                    50,
                    |inputs| {
                        let x = inputs[0].as_i64().unwrap_or(0);
                        Ok(make_result(x > 0))
                    },
                );

                if result.shrunk {
                    let x = result.inputs[0].as_i64().unwrap_or(0);
                    let final_result = make_result(x > 0);
                    prop_assert_eq!(
                        hash_branch_path(&final_result.branch_path),
                        target_hash,
                        "shrunk witness does not preserve branch path"
                    );
                }
            }
        }
    }
}
