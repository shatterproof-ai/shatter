//! Coverage-guided boundary search for unknown/opaque constraints.
//!
//! When Z3 cannot solve a branch constraint (Unknown or timeout), and we have
//! concrete inputs that took both sides of the branch, interpolate between
//! those witnesses using binary-search-style narrowing to find the decision
//! boundary. More effective than blind mutation because the search has
//! directional signal from observed branch outcomes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::{ExecuteResult, MockConfig};
use crate::types::{ParamInfo, TypeInfo};

/// Maximum interpolation candidates generated per branch per exploration round.
pub const MAX_BOUNDARY_STEPS: usize = 4;

/// Maximum branches to attempt boundary search per execution round.
pub const MAX_BOUNDARY_BRANCHES_PER_ROUND: usize = 3;

/// Stop interpolating floats when the interval is narrower than this.
pub const FLOAT_CONVERGENCE_EPSILON: f64 = 1e-9;

/// Find inputs that took opposite sides of a specific branch.
///
/// Scans `raw_results` for the most recent true-witness and false-witness
/// for `branch_id`. Returns `None` if both sides haven't been observed.
pub fn find_witness_pair(
    raw_results: &[(Vec<Value>, Vec<MockConfig>, ExecuteResult)],
    branch_id: u32,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let mut true_witness: Option<&Vec<Value>> = None;
    let mut false_witness: Option<&Vec<Value>> = None;

    // Iterate in reverse to prefer recent witnesses (more likely near the boundary).
    for (inputs, _mocks, result) in raw_results.iter().rev() {
        for decision in &result.branch_path {
            if decision.branch_id == branch_id {
                if decision.taken && true_witness.is_none() {
                    true_witness = Some(inputs);
                } else if !decision.taken && false_witness.is_none() {
                    false_witness = Some(inputs);
                }
                break;
            }
        }
        if true_witness.is_some() && false_witness.is_some() {
            break;
        }
    }

    match (true_witness, false_witness) {
        (Some(tw), Some(fw)) => Some((tw.clone(), fw.clone())),
        _ => None,
    }
}

/// Generate interpolated input candidates between two witness input vectors.
///
/// For each parameter position, interpolates between the two witnesses'
/// values based on their `TypeInfo`. Parameters with identical values in
/// both witnesses are left unchanged. Uses round-robin across differing
/// parameters to stay within `max_steps` budget.
pub fn interpolate_inputs(
    true_witness: &[Value],
    false_witness: &[Value],
    param_infos: &[ParamInfo],
    max_steps: usize,
) -> Vec<Vec<Value>> {
    if max_steps == 0 {
        return Vec::new();
    }

    let len = true_witness.len().min(false_witness.len()).min(param_infos.len());

    // Collect per-parameter midpoint sequences for differing values.
    let mut per_param_midpoints: Vec<(usize, Vec<Value>)> = Vec::new();
    for i in 0..len {
        if true_witness[i] == false_witness[i] {
            continue;
        }
        let midpoints = interpolate_value(
            &true_witness[i],
            &false_witness[i],
            &param_infos[i].typ,
            max_steps,
        );
        if !midpoints.is_empty() {
            per_param_midpoints.push((i, midpoints));
        }
    }

    if per_param_midpoints.is_empty() {
        return Vec::new();
    }

    // Round-robin across differing parameters to produce up to max_steps candidates.
    let mut candidates: Vec<Vec<Value>> = Vec::new();
    let mut step = 0;
    'outer: loop {
        let mut produced_any = false;
        for (param_idx, midpoints) in &per_param_midpoints {
            if step >= midpoints.len() {
                continue;
            }
            produced_any = true;

            // Start from the midpoint of the two witnesses, varying only this parameter.
            let mut candidate = true_witness[..len].to_vec();
            candidate[*param_idx] = midpoints[step].clone();
            candidates.push(candidate);

            if candidates.len() >= max_steps {
                break 'outer;
            }
        }
        if !produced_any {
            break;
        }
        step += 1;
    }

    candidates
}

/// Interpolate a single JSON value between two endpoints based on type.
///
/// Returns a sequence of binary-search midpoints. For types that cannot be
/// meaningfully interpolated (Str, Bool, Complex, Opaque, Unknown), returns
/// an empty vec.
fn interpolate_value(a: &Value, b: &Value, typ: &TypeInfo, max_steps: usize) -> Vec<Value> {
    match typ {
        TypeInfo::Int => interpolate_int(a, b, max_steps),
        TypeInfo::Float => interpolate_float(a, b, max_steps),
        TypeInfo::Array { element } => interpolate_array(a, b, element, max_steps),
        TypeInfo::Object { fields } => interpolate_object(a, b, fields, max_steps),
        TypeInfo::Nullable { inner } => interpolate_nullable(a, b, inner, max_steps),
        TypeInfo::Union { variants } => interpolate_union(a, b, variants, max_steps),
        TypeInfo::Bool
        | TypeInfo::Str
        | TypeInfo::Complex { .. }
        | TypeInfo::Opaque { .. }
        | TypeInfo::Unknown => Vec::new(),
    }
}

/// Binary-search midpoints between two integer values.
fn interpolate_int(a: &Value, b: &Value, max_steps: usize) -> Vec<Value> {
    let (va, vb) = match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => (x, y),
        _ => return Vec::new(),
    };
    if va == vb {
        return Vec::new();
    }

    let mut results = Vec::new();
    let mut lo = va;
    let mut hi = vb;

    for _ in 0..max_steps {
        // Avoid overflow: use i128 for midpoint computation.
        let mid = ((lo as i128 + hi as i128) / 2) as i64;
        if mid == lo || mid == hi {
            break;
        }
        results.push(Value::from(mid));
        // Narrow toward the boundary: alternate sides.
        // The caller will re-execute and tell us which side mid falls on,
        // but for a single round we generate the full bisection sequence.
        if results.len() % 2 == 1 {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    results
}

/// Binary-search midpoints between two float values.
fn interpolate_float(a: &Value, b: &Value, max_steps: usize) -> Vec<Value> {
    let (va, vb) = match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => (x, y),
        _ => return Vec::new(),
    };
    if (va - vb).abs() < FLOAT_CONVERGENCE_EPSILON {
        return Vec::new();
    }

    let mut results = Vec::new();
    let mut lo = va;
    let mut hi = vb;

    for _ in 0..max_steps {
        let mid = (lo + hi) / 2.0;
        if (mid - lo).abs() < FLOAT_CONVERGENCE_EPSILON
            || (mid - hi).abs() < FLOAT_CONVERGENCE_EPSILON
        {
            break;
        }
        results.push(serde_json::json!(mid));
        if results.len() % 2 == 1 {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    results
}

/// Per-element interpolation on differing array elements.
fn interpolate_array(
    a: &Value,
    b: &Value,
    element_type: &TypeInfo,
    max_steps: usize,
) -> Vec<Value> {
    let (arr_a, arr_b) = match (a.as_array(), b.as_array()) {
        (Some(x), Some(y)) => (x, y),
        _ => return Vec::new(),
    };

    let len = arr_a.len().min(arr_b.len());
    let mut results = Vec::new();

    for i in 0..len {
        if arr_a[i] == arr_b[i] {
            continue;
        }
        let midpoints = interpolate_value(&arr_a[i], &arr_b[i], element_type, max_steps);
        for mid in midpoints {
            let mut arr = arr_a.clone();
            if i < arr.len() {
                arr[i] = mid;
            }
            results.push(Value::Array(arr));
            if results.len() >= max_steps {
                return results;
            }
        }
    }

    results
}

/// Per-field interpolation on differing object fields.
fn interpolate_object(
    a: &Value,
    b: &Value,
    fields: &[(String, TypeInfo)],
    max_steps: usize,
) -> Vec<Value> {
    let (obj_a, obj_b) = match (a.as_object(), b.as_object()) {
        (Some(x), Some(y)) => (x, y),
        _ => return Vec::new(),
    };

    let mut results = Vec::new();

    for (name, field_type) in fields {
        let (val_a, val_b) = match (obj_a.get(name), obj_b.get(name)) {
            (Some(x), Some(y)) if x != y => (x, y),
            _ => continue,
        };
        let midpoints = interpolate_value(val_a, val_b, field_type, max_steps);
        for mid in midpoints {
            let mut obj = obj_a.clone();
            obj.insert(name.clone(), mid);
            results.push(Value::Object(obj));
            if results.len() >= max_steps {
                return results;
            }
        }
    }

    results
}

/// Interpolate nullable values: if one is null and the other non-null,
/// return both; otherwise delegate to inner type.
fn interpolate_nullable(
    a: &Value,
    b: &Value,
    inner: &TypeInfo,
    max_steps: usize,
) -> Vec<Value> {
    match (a.is_null(), b.is_null()) {
        (true, false) => vec![b.clone()],
        (false, true) => vec![a.clone()],
        (true, true) => Vec::new(),
        (false, false) => interpolate_value(a, b, inner, max_steps),
    }
}

/// Try interpolating if both values match the same union variant.
fn interpolate_union(
    a: &Value,
    b: &Value,
    variants: &[TypeInfo],
    max_steps: usize,
) -> Vec<Value> {
    for variant in variants {
        let midpoints = interpolate_value(a, b, variant, max_steps);
        if !midpoints.is_empty() {
            return midpoints;
        }
    }
    Vec::new()
}

/// Result of boundary refinement for a single branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryResult {
    /// The branch that was refined.
    pub branch_id: u32,
    /// Closest known input that takes the true side.
    pub true_witness: Vec<Value>,
    /// Closest known input that takes the false side.
    pub false_witness: Vec<Value>,
    /// Number of executions used during refinement.
    pub executions_used: usize,
}

/// Default refinement budget per boundary.
pub const DEFAULT_REFINE_BUDGET: usize = 20;

/// Execution function type for boundary refinement.
/// Execution function type for boundary refinement. Returns `None` on error
/// (refinement stops for this boundary).
type ExecuteFn = dyn FnMut(&[Value], &[MockConfig]) -> Option<ExecuteResult>;

/// Run post-discovery boundary refinement on all branches that have witnesses
/// on both sides. For each such branch, binary-searches between the witness
/// pair to find the precise transition point, using a separate per-boundary
/// execution budget.
///
/// `execute_fn` is called with `(inputs, mocks)` and must return the execution
/// result. The mocks from the first matching raw result are used.
pub fn refine_boundaries(
    raw_results: &[(Vec<Value>, Vec<MockConfig>, ExecuteResult)],
    param_infos: &[ParamInfo],
    refine_budget_per_boundary: usize,
    execute_fn: &mut ExecuteFn,
) -> Vec<BoundaryResult> {
    if refine_budget_per_boundary == 0 {
        return Vec::new();
    }

    // Collect all branch IDs that have witnesses on both sides.
    let mut branch_ids: Vec<u32> = Vec::new();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (_inputs, _mocks, result) in raw_results {
        for decision in &result.branch_path {
            seen.insert(decision.branch_id);
        }
    }
    for &bid in &seen {
        if find_witness_pair(raw_results, bid).is_some() {
            branch_ids.push(bid);
        }
    }
    branch_ids.sort_unstable();

    let mut results = Vec::new();

    for branch_id in branch_ids {
        let (mut tw, mut fw) = match find_witness_pair(raw_results, branch_id) {
            Some(pair) => pair,
            None => continue,
        };

        // Find mock values from the first raw result (use empty if none).
        let mocks: Vec<MockConfig> = raw_results
            .first()
            .map(|(_, m, _)| m.clone())
            .unwrap_or_default();

        let mut executions_used = 0;

        for _ in 0..refine_budget_per_boundary {
            // Generate midpoint candidates between the current witness pair.
            let candidates = interpolate_inputs(&tw, &fw, param_infos, 1);
            if candidates.is_empty() {
                break; // Converged — no more midpoints possible.
            }

            let candidate = &candidates[0];
            let exec_result = match execute_fn(candidate, &mocks) {
                Some(r) => r,
                None => break,
            };
            executions_used += 1;

            // Determine which side of this branch the candidate took.
            let mut took_side: Option<bool> = None;
            for decision in &exec_result.branch_path {
                if decision.branch_id == branch_id {
                    took_side = Some(decision.taken);
                    break;
                }
            }

            match took_side {
                Some(true) => tw = candidate.clone(),
                Some(false) => fw = candidate.clone(),
                None => {
                    // Branch not encountered in this execution — skip.
                    continue;
                }
            }
        }

        results.push(BoundaryResult {
            branch_id,
            true_witness: tw,
            false_witness: fw,
            executions_used,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::PerformanceMetrics;
    use serde_json::json;

    fn make_execute_result(branch_id: u32, taken: bool) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id,
                line: 1,
                taken,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
        }
    }

    fn make_param(name: &str, typ: TypeInfo) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ,
            type_name: None,
        }
    }

    // --- find_witness_pair tests ---

    #[test]
    fn find_witness_pair_with_both_sides() {
        let raw = vec![
            (vec![json!(1)], vec![], make_execute_result(0, true)),
            (vec![json!(10)], vec![], make_execute_result(0, false)),
        ];
        let result = find_witness_pair(&raw, 0);
        assert!(result.is_some());
        let (tw, fw) = result.unwrap();
        assert_eq!(tw, vec![json!(1)]);
        assert_eq!(fw, vec![json!(10)]);
    }

    #[test]
    fn find_witness_pair_missing_side() {
        let raw = vec![
            (vec![json!(1)], vec![], make_execute_result(0, true)),
            (vec![json!(2)], vec![], make_execute_result(0, true)),
        ];
        assert!(find_witness_pair(&raw, 0).is_none());
    }

    #[test]
    fn find_witness_pair_unknown_branch() {
        let raw = vec![
            (vec![json!(1)], vec![], make_execute_result(0, true)),
            (vec![json!(10)], vec![], make_execute_result(0, false)),
        ];
        assert!(find_witness_pair(&raw, 99).is_none());
    }

    #[test]
    fn find_witness_pair_prefers_recent() {
        let raw = vec![
            (vec![json!(1)], vec![], make_execute_result(0, true)),
            (vec![json!(5)], vec![], make_execute_result(0, true)),
            (vec![json!(10)], vec![], make_execute_result(0, false)),
            (vec![json!(20)], vec![], make_execute_result(0, false)),
        ];
        let (tw, fw) = find_witness_pair(&raw, 0).unwrap();
        // Reverse iteration: most recent true=5, most recent false=20
        assert_eq!(tw, vec![json!(5)]);
        assert_eq!(fw, vec![json!(20)]);
    }

    // --- interpolate_int tests ---

    #[test]
    fn interpolate_int_binary_search() {
        let results = interpolate_int(&json!(0), &json!(100), MAX_BOUNDARY_STEPS);
        assert!(!results.is_empty());
        // First midpoint should be 50
        assert_eq!(results[0], json!(50));
    }

    #[test]
    fn interpolate_int_adjacent() {
        // Adjacent values: no midpoint possible
        let results = interpolate_int(&json!(5), &json!(6), MAX_BOUNDARY_STEPS);
        assert!(results.is_empty());
    }

    #[test]
    fn interpolate_int_equal() {
        let results = interpolate_int(&json!(5), &json!(5), MAX_BOUNDARY_STEPS);
        assert!(results.is_empty());
    }

    #[test]
    fn interpolate_int_negative_range() {
        let results = interpolate_int(&json!(-100), &json!(100), MAX_BOUNDARY_STEPS);
        assert!(!results.is_empty());
        assert_eq!(results[0], json!(0));
    }

    // --- interpolate_float tests ---

    #[test]
    fn interpolate_float_binary_search() {
        let results = interpolate_float(&json!(0.0), &json!(10.0), MAX_BOUNDARY_STEPS);
        assert!(!results.is_empty());
        assert!((results[0].as_f64().unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn interpolate_float_convergence() {
        // Values within epsilon: no interpolation
        let a = 1.0;
        let b = a + FLOAT_CONVERGENCE_EPSILON / 2.0;
        let results = interpolate_float(&json!(a), &json!(b), MAX_BOUNDARY_STEPS);
        assert!(results.is_empty());
    }

    #[test]
    fn interpolate_float_nan_skipped() {
        let results = interpolate_float(&json!(f64::NAN), &json!(5.0), MAX_BOUNDARY_STEPS);
        assert!(results.is_empty());
    }

    #[test]
    fn interpolate_float_infinity_skipped() {
        let results =
            interpolate_float(&json!(f64::INFINITY), &json!(5.0), MAX_BOUNDARY_STEPS);
        assert!(results.is_empty());
    }

    // --- interpolate_inputs tests ---

    #[test]
    fn interpolate_skips_identical_values() {
        let tw = vec![json!(5), json!("hello")];
        let fw = vec![json!(5), json!("hello")];
        let params = vec![
            make_param("x", TypeInfo::Int),
            make_param("s", TypeInfo::Str),
        ];
        let result = interpolate_inputs(&tw, &fw, &params, MAX_BOUNDARY_STEPS);
        assert!(result.is_empty());
    }

    #[test]
    fn interpolate_inputs_single_differing_param() {
        let tw = vec![json!(0), json!("hello")];
        let fw = vec![json!(100), json!("hello")];
        let params = vec![
            make_param("x", TypeInfo::Int),
            make_param("s", TypeInfo::Str),
        ];
        let result = interpolate_inputs(&tw, &fw, &params, MAX_BOUNDARY_STEPS);
        assert!(!result.is_empty());
        // All candidates should have "hello" as second param (unchanged)
        for candidate in &result {
            assert_eq!(candidate.len(), 2);
            assert_eq!(candidate[1], json!("hello"));
        }
    }

    #[test]
    fn interpolate_respects_max_steps() {
        let tw = vec![json!(0)];
        let fw = vec![json!(1000000)];
        let params = vec![make_param("x", TypeInfo::Int)];
        let result = interpolate_inputs(&tw, &fw, &params, 2);
        assert!(result.len() <= 2);
    }

    #[test]
    fn candidates_preserve_vector_length() {
        let tw = vec![json!(0), json!(1.0), json!(true)];
        let fw = vec![json!(100), json!(9.0), json!(false)];
        let params = vec![
            make_param("a", TypeInfo::Int),
            make_param("b", TypeInfo::Float),
            make_param("c", TypeInfo::Bool),
        ];
        let result = interpolate_inputs(&tw, &fw, &params, MAX_BOUNDARY_STEPS);
        for candidate in &result {
            assert_eq!(candidate.len(), 3);
        }
    }

    #[test]
    fn interpolate_zero_max_steps() {
        let tw = vec![json!(0)];
        let fw = vec![json!(100)];
        let params = vec![make_param("x", TypeInfo::Int)];
        assert!(interpolate_inputs(&tw, &fw, &params, 0).is_empty());
    }

    // --- interpolate_array tests ---

    #[test]
    fn interpolate_array_per_element() {
        let a = json!([0, 5]);
        let b = json!([100, 5]);
        let results = interpolate_array(&a, &b, &TypeInfo::Int, MAX_BOUNDARY_STEPS);
        assert!(!results.is_empty());
        // Only first element differs, second should remain 5
        for r in &results {
            let arr = r.as_array().unwrap();
            assert_eq!(arr[1], json!(5));
        }
    }

    // --- interpolate_object tests ---

    #[test]
    fn interpolate_object_per_field() {
        let a = json!({"x": 0, "y": "same"});
        let b = json!({"x": 100, "y": "same"});
        let fields = vec![
            ("x".to_string(), TypeInfo::Int),
            ("y".to_string(), TypeInfo::Str),
        ];
        let results = interpolate_object(&a, &b, &fields, MAX_BOUNDARY_STEPS);
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.get("y").unwrap(), &json!("same"));
        }
    }

    // --- interpolate_nullable tests ---

    #[test]
    fn interpolate_nullable_null_vs_value() {
        let results = interpolate_nullable(&json!(null), &json!(42), &TypeInfo::Int, 4);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], json!(42));
    }

    #[test]
    fn interpolate_nullable_both_non_null() {
        let results = interpolate_nullable(&json!(0), &json!(100), &TypeInfo::Int, 4);
        assert!(!results.is_empty());
        assert_eq!(results[0], json!(50));
    }

    // --- interpolate_union tests ---

    #[test]
    fn interpolate_union_matching_variant() {
        let variants = vec![TypeInfo::Int, TypeInfo::Str];
        let results = interpolate_union(&json!(0), &json!(100), &variants, 4);
        assert!(!results.is_empty());
    }

    #[test]
    fn interpolate_union_no_match() {
        let variants = vec![TypeInfo::Str];
        let results = interpolate_union(&json!(0), &json!(100), &variants, 4);
        assert!(results.is_empty());
    }

    // --- refine_boundaries tests ---

    /// Mock execute function: simulates `x >= threshold` branch.
    fn mock_threshold_execute(
        threshold: i64,
        branch_id: u32,
    ) -> impl FnMut(&[Value], &[MockConfig]) -> Option<ExecuteResult> {
        move |inputs: &[Value], _mocks: &[MockConfig]| {
            let x = inputs[0].as_i64().unwrap_or(0);
            let taken = x >= threshold;
            Some(make_execute_result(branch_id, taken))
        }
    }

    #[test]
    fn refine_narrows_boundary() {
        // Discovery found x=0 (false) and x=100 (true) for threshold=10.
        let raw = vec![
            (vec![json!(0)], vec![], make_execute_result(0, false)),
            (vec![json!(100)], vec![], make_execute_result(0, true)),
        ];
        let params = vec![make_param("x", TypeInfo::Int)];

        let mut exec_fn = mock_threshold_execute(10, 0);
        let results = refine_boundaries(&raw, &params, 20, &mut exec_fn);

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.branch_id, 0);

        // After refinement, witnesses should be much closer to 10 than 0 and 100.
        let tw_val = r.true_witness[0].as_i64().unwrap();
        let fw_val = r.false_witness[0].as_i64().unwrap();
        assert!(tw_val >= 10, "true witness {} should be >= 10", tw_val);
        assert!(fw_val < 10, "false witness {} should be < 10", fw_val);
        // The gap should be narrow (at most a few apart).
        assert!(
            (tw_val - fw_val).abs() <= 2,
            "witnesses {} and {} should be close to boundary",
            tw_val,
            fw_val
        );
    }

    #[test]
    fn refine_budget_enforcement() {
        let raw = vec![
            (vec![json!(0)], vec![], make_execute_result(0, false)),
            (vec![json!(1000)], vec![], make_execute_result(0, true)),
        ];
        let params = vec![make_param("x", TypeInfo::Int)];

        let mut exec_fn = mock_threshold_execute(10, 0);

        let budget = 5;
        let results = refine_boundaries(&raw, &params, budget, &mut exec_fn);

        assert_eq!(results.len(), 1);
        assert!(
            results[0].executions_used <= budget,
            "used {} executions, budget was {}",
            results[0].executions_used,
            budget
        );
    }

    #[test]
    fn refine_skips_one_sided_branches() {
        // Only true side observed — no refinement possible.
        let raw = vec![
            (vec![json!(5)], vec![], make_execute_result(0, true)),
            (vec![json!(10)], vec![], make_execute_result(0, true)),
        ];
        let params = vec![make_param("x", TypeInfo::Int)];

        let mut exec_fn = mock_threshold_execute(10, 0);
        let results = refine_boundaries(&raw, &params, 20, &mut exec_fn);

        assert!(results.is_empty());
    }

    #[test]
    fn refine_zero_budget_returns_empty() {
        let raw = vec![
            (vec![json!(0)], vec![], make_execute_result(0, false)),
            (vec![json!(100)], vec![], make_execute_result(0, true)),
        ];
        let params = vec![make_param("x", TypeInfo::Int)];

        let mut exec_fn = mock_threshold_execute(10, 0);
        let results = refine_boundaries(&raw, &params, 0, &mut exec_fn);

        assert!(results.is_empty());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    proptest! {
        #[test]
        fn interpolate_int_midpoint_in_range(a in -10000i64..10000, b in -10000i64..10000) {
            let results = interpolate_int(&json!(a), &json!(b), MAX_BOUNDARY_STEPS);
            let lo = a.min(b);
            let hi = a.max(b);
            for mid in &results {
                let v = mid.as_i64().unwrap();
                prop_assert!(v >= lo && v <= hi,
                    "midpoint {} not in range [{}, {}]", v, lo, hi);
            }
        }

        #[test]
        fn interpolate_float_midpoint_in_range(
            a in -1e100f64..1e100f64,
            b in -1e100f64..1e100f64,
        ) {
            prop_assume!(a.is_finite() && b.is_finite());
            let results = interpolate_float(&json!(a), &json!(b), MAX_BOUNDARY_STEPS);
            let lo = a.min(b);
            let hi = a.max(b);
            for mid in &results {
                if let Some(v) = mid.as_f64() {
                    prop_assert!(v >= lo && v <= hi,
                        "midpoint {} not in range [{}, {}]", v, lo, hi);
                }
            }
        }

        #[test]
        fn interpolate_preserves_vector_length(
            a_val in -1000i64..1000,
            b_val in -1000i64..1000,
            c_val in proptest::num::f64::NORMAL,
            d_val in proptest::num::f64::NORMAL,
        ) {
            let tw = vec![json!(a_val), json!(c_val)];
            let fw = vec![json!(b_val), json!(d_val)];
            let params = vec![
                ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
                ParamInfo { name: "b".into(), typ: TypeInfo::Float, type_name: None },
            ];
            let candidates = interpolate_inputs(&tw, &fw, &params, MAX_BOUNDARY_STEPS);
            for c in &candidates {
                prop_assert_eq!(c.len(), 2);
            }
        }

        #[test]
        fn interpolate_bounded_output(
            a_val in -1000i64..1000,
            b_val in -1000i64..1000,
            max_steps in 1usize..10,
        ) {
            let tw = vec![json!(a_val)];
            let fw = vec![json!(b_val)];
            let params = vec![
                ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None },
            ];
            let candidates = interpolate_inputs(&tw, &fw, &params, max_steps);
            prop_assert!(candidates.len() <= max_steps);
        }
    }
}
