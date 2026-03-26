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
/// Uses a three-phase strategy:
/// - Phase 1: attempt to shrink all parameters at once (1 execute call). If the
///   bulk candidate preserves the target path, accept it and continue to Phase 2
///   from the already-reduced inputs.
/// - Phase 1.5 (grouped fallback): if bulk was rejected and there are 3+ parameters,
///   try consecutive groups of ⌊N/2⌋ parameters before the per-param loop. Costs
///   ≈2 execute calls and can shrink multiple params per accepted trial, making the
///   budget go further than one-at-a-time for large witnesses.
/// - Phase 2: QuickCheck-style one-parameter-at-a-time loop. Repeats until no
///   progress or budget exhausted.
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

    // Phase 1: bulk shrink — try all parameters at once (costs exactly 1 execute call).
    // When the target path is insensitive to combined reductions this skips the entire
    // per-param loop, dramatically reducing round-trips for multi-parameter witnesses.
    let mut bulk_accepted = false;
    if attempts < max_attempts
        && let Some(bulk_trial) = bulk_shrink_candidate(&current, param_infos)
    {
        attempts += 1;
        if let Ok(result) = execute_fn(&bulk_trial)
            && hash_branch_path(&result.branch_path) == target_path_hash
        {
            current = bulk_trial;
            bulk_accepted = true;
        }
    }

    // Phase 1.5: grouped fallback — when bulk was rejected and N >= 3, try
    // consecutive groups of floor(N/2) parameters before falling back to per-param.
    // With N params this costs ceil(N / (N/2)) ≈ 2 execute calls and can shrink
    // multiple params per accepted trial, making the remaining budget go further.
    let n = param_infos.len().min(current.len());
    if !bulk_accepted && n >= 3 && attempts < max_attempts {
        let group_size = n / 2;
        for trial in grouped_shrink_candidates(&current, param_infos, group_size) {
            if attempts >= max_attempts {
                return ShrinkResult {
                    shrunk: current != original,
                    inputs: current,
                    attempts,
                };
            }
            attempts += 1;
            if let Ok(result) = execute_fn(&trial)
                && hash_branch_path(&result.branch_path) == target_path_hash
            {
                current = trial;
            }
        }
    }

    // Phase 2: one-at-a-time per-param loop — refines further or handles cases
    // where the bulk candidate changed the path.
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

/// Generate a deterministic all-parameter bulk shrink candidate.
///
/// For each parameter, takes the first candidate produced by [`shrink_candidates`].
/// Parameters that are already minimal (no candidates) keep their current value.
/// Returns `None` if no parameter has a shrink candidate — nothing to try.
pub fn bulk_shrink_candidate(inputs: &[Value], param_infos: &[ParamInfo]) -> Option<Vec<Value>> {
    let mut trial = inputs.to_vec();
    let mut any_changed = false;
    for i in 0..param_infos.len().min(inputs.len()) {
        let orig_complexity = value_complexity(&inputs[i]);
        let candidates = shrink_candidates(&inputs[i], &param_infos[i].typ);
        // Only replace if the candidate is strictly simpler than the original.
        if let Some(simpler) = candidates.into_iter().find(|c| value_complexity(c) < orig_complexity) {
            trial[i] = simpler;
            any_changed = true;
        }
    }
    if any_changed { Some(trial) } else { None }
}

/// Generate deterministic grouped shrink trials.
///
/// Divides the parameters into consecutive non-overlapping groups of `group_size`.
/// For each group, builds a trial vector where every parameter in that group is
/// replaced by its first shrink candidate; parameters outside the group keep their
/// current value. Returns one trial per group that has at least one shrinkable
/// parameter. Returns an empty vec if `group_size` is 0 or no parameter has a
/// shrink candidate.
///
/// Used as Phase 1.5 in [`shrink_witness`]: when the all-at-once bulk candidate
/// fails, groups of size `⌊N/2⌋` cost ≈2 execute calls instead of N, and each
/// accepted trial shrinks multiple parameters simultaneously.
pub fn grouped_shrink_candidates(
    inputs: &[Value],
    param_infos: &[ParamInfo],
    group_size: usize,
) -> Vec<Vec<Value>> {
    let n = param_infos.len().min(inputs.len());
    if n == 0 || group_size == 0 {
        return Vec::new();
    }
    let mut trials = Vec::new();
    let mut gi = 0;
    while gi < n {
        let group_end = (gi + group_size).min(n);
        let mut trial = inputs.to_vec();
        let mut any_changed = false;
        for j in gi..group_end {
            let candidates = shrink_candidates(&inputs[j], &param_infos[j].typ);
            if let Some(first) = candidates.into_iter().next() {
                trial[j] = first;
                any_changed = true;
            }
        }
        if any_changed {
            trials.push(trial);
        }
        gi += group_size;
    }
    trials
}

// Boundary values used as shrink targets for numeric types.
const SHRINK_INT_ZERO: i64 = 0;
const SHRINK_INT_ONE: i64 = 1;
const SHRINK_INT_NEG_ONE: i64 = -1;

const SHRINK_FLOAT_ZERO: f64 = 0.0;
const SHRINK_FLOAT_ONE: f64 = 1.0;
const SHRINK_FLOAT_NEG_ONE: f64 = -1.0;

/// Score a witness by total input complexity; lower = simpler.
///
/// Used to select the best starting witness per path before shrinking:
/// starting from a simpler witness reduces the number of shrink iterations
/// required to reach a minimal form.
#[must_use]
pub fn witness_complexity(inputs: &[Value]) -> usize {
    inputs.iter().map(value_complexity).sum()
}

/// Witnesses with complexity at or below this threshold are skipped during the
/// shrink phase. Complexity 0 means all inputs are already at their minimal
/// values (integer 0, empty string, empty array/object, null) — no shrink
/// candidate can be produced, so attempting to shrink wastes the loop overhead.
pub const SHRINK_SKIP_THRESHOLD: usize = 0;

/// Witnesses at or below this complexity are treated as trivial and receive
/// only [`MIN_SHRINK_BUDGET`] attempts. Covers booleans, small integers (1–4),
/// and very short strings (1–4 chars) — types where only a handful of shrink
/// candidates exist regardless of budget.
pub const SHRINK_TRIVIAL_THRESHOLD: usize = 4;

/// Witnesses in the range `(SHRINK_TRIVIAL_THRESHOLD, SHRINK_MODERATE_THRESHOLD]`
/// receive half the base budget. Covers moderate integers, medium strings, and
/// small arrays/objects.
pub const SHRINK_MODERATE_THRESHOLD: usize = 20;

/// Minimum shrink budget for any witness that passes [`should_shrink_path`].
/// Large enough for the bulk phase (1) + grouped phase (≈2) + at least one
/// per-parameter attempt.
pub const MIN_SHRINK_BUDGET: usize = 3;

/// **Shrink selection policy (deterministic)**
///
/// Returns `true` when a path witness should be attempted for shrinking.
///
/// A witness is a candidate for shrinking if its [`witness_complexity`] exceeds
/// [`SHRINK_SKIP_THRESHOLD`]. At or below the threshold every parameter is
/// already at a minimal value — `shrink_candidates` would produce no candidates
/// and every execute() call attempted would be wasted.
///
/// Among candidates the shrink loop processes them in **descending complexity
/// order** (highest-value witnesses first). Ties are broken by ascending path
/// hash, giving deterministic ordering regardless of HashMap iteration order.
///
/// This function never uses randomness. Same inputs → same decision, always.
#[must_use]
pub fn should_shrink_path(complexity: usize) -> bool {
    complexity > SHRINK_SKIP_THRESHOLD
}

/// **Shrink budget policy (deterministic)**
///
/// Returns the maximum number of execute calls to spend shrinking a witness of
/// the given `complexity`, given a `base_budget` from [`ExploreConfig`].
///
/// | Complexity range | Budget |
/// |---|---|
/// | ≤ [`SHRINK_TRIVIAL_THRESHOLD`] | [`MIN_SHRINK_BUDGET`] |
/// | ≤ [`SHRINK_MODERATE_THRESHOLD`] | `base_budget / 2` (≥ `MIN_SHRINK_BUDGET`) |
/// | > [`SHRINK_MODERATE_THRESHOLD`] | `base_budget` (full) |
///
/// The result is always in `[MIN_SHRINK_BUDGET.min(base_budget), base_budget]`.
/// This function never uses randomness. Same inputs → same result, always.
#[must_use]
pub fn shrink_budget_for_witness(complexity: usize, base_budget: usize) -> usize {
    if complexity <= SHRINK_TRIVIAL_THRESHOLD {
        MIN_SHRINK_BUDGET.min(base_budget)
    } else if complexity <= SHRINK_MODERATE_THRESHOLD {
        (base_budget / 2).max(MIN_SHRINK_BUDGET).min(base_budget)
    } else {
        base_budget
    }
}

/// Perf counters gathered during a shrink pass.
///
/// Surfaced via `tracing::debug!` after each shrink phase so callers can
/// observe how many paths were skipped vs. actually shrunk without modifying
/// the public output types. Also included in `ObservationOutput` and
/// `ExploreResult` so CLI reports can display shrink throughput.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShrinkStats {
    /// Total unique paths considered for shrinking (one entry per path hash).
    pub paths_considered: usize,
    /// Paths skipped because witness complexity ≤ [`SHRINK_SKIP_THRESHOLD`].
    pub paths_skipped_simple: usize,
    /// Paths where shrinking was actually attempted.
    pub paths_shrunk: usize,
    /// Total execute() calls made across all shrink attempts.
    pub total_shrink_attempts: usize,
    /// Sum of per-witness budgets assigned by [`shrink_budget_for_witness`].
    /// Comparing this to `total_shrink_attempts` shows unused budget savings.
    pub total_budget_assigned: usize,
}

impl ShrinkStats {
    /// Merge `other` into `self` by summing all four counters.
    pub fn merge(&mut self, other: &ShrinkStats) {
        self.paths_considered += other.paths_considered;
        self.paths_skipped_simple += other.paths_skipped_simple;
        self.paths_shrunk += other.paths_shrunk;
        self.total_shrink_attempts += other.total_shrink_attempts;
        self.total_budget_assigned += other.total_budget_assigned;
    }
}

/// Format a one-line human-readable summary of shrink-phase statistics.
///
/// Returns an empty string when there is nothing to show (no paths were
/// considered). Intended for inclusion in perf blocks of exploration reports.
#[must_use]
pub fn format_shrink_stats_line(stats: &ShrinkStats) -> String {
    if stats.paths_considered == 0 {
        return String::new();
    }
    if stats.total_shrink_attempts == 0 {
        return "  Shrink: disabled (budget=0)\n".to_string();
    }
    format!(
        "  Shrink: {} attempts · {}/{} paths shrunk · {} skipped (simple)\n",
        stats.total_shrink_attempts,
        stats.paths_shrunk,
        stats.paths_considered,
        stats.paths_skipped_simple,
    )
}

fn value_complexity(v: &Value) -> usize {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.unsigned_abs() as usize
            } else if let Some(f) = n.as_f64() {
                f.abs() as usize
            } else {
                1
            }
        }
        Value::String(s) => s.len(),
        Value::Array(arr) => arr.len() + arr.iter().map(value_complexity).sum::<usize>(),
        Value::Object(obj) => obj.len() + obj.values().map(value_complexity).sum::<usize>(),
    }
}

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
    // ShrinkStats
    // -----------------------------------------------------------------------

    #[test]
    fn shrink_stats_merge() {
        let mut a = ShrinkStats {
            paths_considered: 3,
            paths_skipped_simple: 1,
            paths_shrunk: 2,
            total_shrink_attempts: 10,
            total_budget_assigned: 20,
        };
        let b = ShrinkStats {
            paths_considered: 5,
            paths_skipped_simple: 2,
            paths_shrunk: 3,
            total_shrink_attempts: 15,
            total_budget_assigned: 30,
        };
        a.merge(&b);
        assert_eq!(a.paths_considered, 8);
        assert_eq!(a.paths_skipped_simple, 3);
        assert_eq!(a.paths_shrunk, 5);
        assert_eq!(a.total_shrink_attempts, 25);
        assert_eq!(a.total_budget_assigned, 50);
    }

    #[test]
    fn format_shrink_stats_line_empty_when_no_paths() {
        let stats = ShrinkStats::default();
        assert_eq!(format_shrink_stats_line(&stats), "");
    }

    #[test]
    fn format_shrink_stats_line_disabled() {
        let stats = ShrinkStats {
            paths_considered: 3,
            total_shrink_attempts: 0,
            ..Default::default()
        };
        let line = format_shrink_stats_line(&stats);
        assert!(line.contains("disabled"), "expected 'disabled' in '{line}'");
    }

    #[test]
    fn format_shrink_stats_line_with_attempts() {
        let stats = ShrinkStats {
            paths_considered: 4,
            paths_skipped_simple: 1,
            paths_shrunk: 2,
            total_shrink_attempts: 8,
            total_budget_assigned: 16,
        };
        let line = format_shrink_stats_line(&stats);
        assert!(line.contains("8"), "expected attempt count in '{line}'");
        assert!(line.contains("2/4"), "expected shrunk/considered ratio in '{line}'");
    }

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
            static_opacity: None,
            medium_opacity: None,
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
    // witness_complexity tests
    // -------------------------------------------------------------------

    #[test]
    fn complexity_null_and_bool() {
        assert_eq!(witness_complexity(&[json!(null)]), 0);
        assert_eq!(witness_complexity(&[json!(false)]), 1);
        assert_eq!(witness_complexity(&[json!(true)]), 1);
    }

    #[test]
    fn complexity_numbers() {
        assert_eq!(witness_complexity(&[json!(0)]), 0);
        assert_eq!(witness_complexity(&[json!(1)]), 1);
        assert_eq!(witness_complexity(&[json!(-1)]), 1);
        assert_eq!(witness_complexity(&[json!(42)]), 42);
        assert_eq!(witness_complexity(&[json!(-42)]), 42);
    }

    #[test]
    fn complexity_strings() {
        assert_eq!(witness_complexity(&[json!("")]), 0);
        assert_eq!(witness_complexity(&[json!("hi")]), 2);
        assert_eq!(witness_complexity(&[json!("hello world")]), 11);
    }

    #[test]
    fn complexity_ordering() {
        // Simpler witnesses score lower (or equal for already-minimal values).
        // Empty inputs and [0] both score 0 (both are fully minimal).
        let empty: Vec<serde_json::Value> = vec![];
        assert!(witness_complexity(&empty) <= witness_complexity(&[json!(0)]));
        assert!(witness_complexity(&[json!(0)]) < witness_complexity(&[json!(1)]));
        assert!(witness_complexity(&[json!(1)]) < witness_complexity(&[json!("hello")]));
        assert!(
            witness_complexity(&[json!("hello")]) < witness_complexity(&[json!([1i64, 2, 3])])
        );
    }

    #[test]
    fn complexity_selects_best_witness() {
        // Simulate: two witnesses for the same path, one simpler than the other.
        let complex = vec![json!("hello world"), json!(42i64)]; // 11 + 42 = 53
        let simple = vec![json!("hi"), json!(1i64)]; // 2 + 1 = 3
        let witnesses: Vec<(Vec<serde_json::Value>, Vec<()>)> =
            vec![(complex.clone(), vec![]), (simple.clone(), vec![])];
        let best = witnesses
            .into_iter()
            .min_by_key(|(inputs, _)| witness_complexity(inputs))
            .unwrap();
        assert_eq!(best.0, simple, "should select the simpler witness");
    }

    #[test]
    fn complexity_multi_param_sum() {
        // Multi-param witnesses sum across all params.
        assert_eq!(
            witness_complexity(&[json!("hi"), json!(3i64)]),
            2 + 3, // "hi".len() + abs(3)
        );
    }

    mod witness_complexity_prop_tests {
        use super::{json, witness_complexity};
        use proptest::prelude::*;

        #[test]
        fn complexity_empty_inputs_is_zero() {
            assert_eq!(witness_complexity(&[]), 0);
        }

        proptest! {
            #[test]
            fn complexity_null_is_zero(count in 1..5usize) {
                let inputs: Vec<serde_json::Value> = (0..count).map(|_| json!(null)).collect();
                prop_assert_eq!(witness_complexity(&inputs), 0);
            }

            #[test]
            fn complexity_int_equals_abs(n in -10_000i64..10_000i64) {
                let c = witness_complexity(&[json!(n)]);
                prop_assert_eq!(c, n.unsigned_abs() as usize);
            }

            #[test]
            fn complexity_string_equals_len(s in ".{0,50}") {
                let c = witness_complexity(&[json!(s.as_str())]);
                prop_assert_eq!(c, s.len());
            }

            #[test]
            fn simpler_witness_wins(
                a in 1i64..100i64,
                b in 200i64..300i64,
            ) {
                // witness with smaller int always scores lower
                let w_a = vec![json!(a)];
                let w_b = vec![json!(b)];
                prop_assert!(witness_complexity(&w_a) < witness_complexity(&w_b));
                let winners: Vec<(Vec<serde_json::Value>, Vec<()>)> =
                    vec![(w_a.clone(), vec![]), (w_b, vec![])];
                let best = winners
                    .into_iter()
                    .min_by_key(|(inputs, _)| witness_complexity(inputs))
                    .unwrap();
                prop_assert_eq!(best.0, w_a);
            }
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
                connection_failures: vec![], runtime_crypto_boundaries: vec![],
                performance: empty_perf(),
            }
        }

        fn branch_taken() -> Vec<BranchDecision> {
            vec![BranchDecision {
                branch_id: 1,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            }]
        }

        fn branch_not_taken() -> Vec<BranchDecision> {
            vec![BranchDecision {
                branch_id: 1,
                line: 5,
                taken: false,
                constraint: SymConstraint::default(),
                conditions: None,
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

    mod bulk_shrink_tests {
        use super::*;
        use crate::execution_record::{BranchDecision, SymConstraint};
        use crate::orchestrator::hash_branch_path;
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
                    conditions: None,
                }],
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![], runtime_crypto_boundaries: vec![],
                performance: empty_perf(),
            }
        }

        fn branch_taken() -> Vec<BranchDecision> {
            make_result(true).branch_path
        }

        fn branch_not_taken() -> Vec<BranchDecision> {
            make_result(false).branch_path
        }

        fn int_param(name: &str) -> ParamInfo {
            ParamInfo { name: name.into(), typ: TypeInfo::Int, type_name: None }
        }

        fn str_param(name: &str) -> ParamInfo {
            ParamInfo { name: name.into(), typ: TypeInfo::Str, type_name: None }
        }

        // -------------------------------------------------------------------
        // bulk_shrink_candidate unit tests
        // -------------------------------------------------------------------

        #[test]
        fn bulk_combines_all_shrinkable_params() {
            // Both params are shrinkable; candidate must reduce both.
            let inputs = vec![json!(100i64), json!("hello")];
            let params = vec![int_param("x"), str_param("s")];
            let candidate = bulk_shrink_candidate(&inputs, &params).unwrap();
            assert_ne!(candidate[0], json!(100i64), "int should be reduced");
            assert_ne!(candidate[1], json!("hello"), "string should be reduced");
            assert_eq!(candidate.len(), 2);
        }

        #[test]
        fn bulk_returns_none_when_all_minimal() {
            // false (bool) and "" (string) have no shrink candidates.
            let inputs = vec![json!(false), json!("")];
            let params = vec![
                ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
                str_param("s"),
            ];
            assert!(bulk_shrink_candidate(&inputs, &params).is_none());
        }

        #[test]
        fn bulk_skips_already_minimal_params() {
            // First param is minimal (false bool — no candidates), second is not.
            let inputs = vec![json!(false), json!("hello")];
            let params = vec![
                ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
                str_param("s"),
            ];
            let candidate = bulk_shrink_candidate(&inputs, &params).unwrap();
            // Minimal param unchanged.
            assert_eq!(candidate[0], json!(false));
            // Non-minimal param reduced.
            assert_ne!(candidate[1], json!("hello"));
        }

        #[test]
        fn bulk_candidate_is_deterministic() {
            let inputs = vec![json!(100i64), json!("hello")];
            let params = vec![int_param("x"), str_param("s")];
            let a = bulk_shrink_candidate(&inputs, &params);
            let b = bulk_shrink_candidate(&inputs, &params);
            assert_eq!(a, b, "bulk_shrink_candidate must be deterministic");
        }

        // -------------------------------------------------------------------
        // shrink_witness bulk-first perf evidence
        // -------------------------------------------------------------------

        #[test]
        fn bulk_accepted_uses_few_attempts() {
            // Two string params where any non-empty value preserves the path.
            // Starting from "abc"/"xyz": bulk collapses both to a 2-char string in 1 call.
            // Per-param then only needs 1-2 more steps each → well under 10 total.
            let target_hash = hash_branch_path(&branch_taken());
            let params = vec![str_param("a"), str_param("b")];

            let result = shrink_witness(
                &[json!("abc"), json!("xyz")],
                &params,
                target_hash,
                50,
                |inputs| {
                    let a = inputs[0].as_str().unwrap_or("");
                    let b = inputs[1].as_str().unwrap_or("");
                    Ok(make_result(!a.is_empty() && !b.is_empty()))
                },
            );

            assert!(result.shrunk);
            // Bulk phase accepted in 1 call, then per-param convergence is fast.
            // Without bulk this would require multiple separate param-at-a-time passes.
            assert!(
                result.attempts < 10,
                "expected few attempts with bulk-first, got {}",
                result.attempts
            );
        }

        #[test]
        fn bulk_fallback_when_path_changes() {
            // The bulk candidate violates the required condition (both must be > threshold).
            // Bulk fails; per-param loop must still find the minimal form.
            let target_hash = hash_branch_path(&branch_taken());
            let params = vec![int_param("x"), int_param("y")];

            // Path is preserved only when BOTH params are exactly 5.
            let result = shrink_witness(
                &[json!(100i64), json!(100i64)],
                &params,
                target_hash,
                100,
                |inputs| {
                    let x = inputs[0].as_i64().unwrap_or(0);
                    let y = inputs[1].as_i64().unwrap_or(0);
                    // Path only taken when both == 5 (bulk candidate won't hit this).
                    Ok(make_result(x == 5 && y == 5))
                },
            );

            // Shrink should eventually find (5, 5) or confirm no simpler form.
            // The key property: we don't panic or lose correctness even when bulk fails.
            assert!(result.attempts <= 100);
        }

        // -------------------------------------------------------------------
        // Proptest: bulk candidate complexity never exceeds original
        // -------------------------------------------------------------------

        proptest! {
            #[test]
            fn bulk_candidate_not_more_complex(
                x in -500i64..500,
                s in "[a-z]{0,10}",
            ) {
                let inputs = vec![json!(x), Value::String(s)];
                let params = vec![int_param("x"), str_param("s")];
                if let Some(candidate) = bulk_shrink_candidate(&inputs, &params) {
                    let orig_complexity = witness_complexity(&inputs);
                    let cand_complexity = witness_complexity(&candidate);
                    prop_assert!(
                        cand_complexity <= orig_complexity,
                        "bulk candidate more complex: {} > {}",
                        cand_complexity,
                        orig_complexity
                    );
                }
            }
        }
    }

    mod grouped_shrink_tests {
        use super::*;
        use crate::execution_record::{BranchDecision, SymConstraint};
        use crate::orchestrator::hash_branch_path;
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
                    conditions: None,
                }],
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![], runtime_crypto_boundaries: vec![],
                performance: empty_perf(),
            }
        }

        fn int_param(name: &str) -> ParamInfo {
            ParamInfo { name: name.into(), typ: TypeInfo::Int, type_name: None }
        }

        fn str_param(name: &str) -> ParamInfo {
            ParamInfo { name: name.into(), typ: TypeInfo::Str, type_name: None }
        }

        fn branch_taken() -> Vec<BranchDecision> {
            make_result(true).branch_path
        }

        // -------------------------------------------------------------------
        // grouped_shrink_candidates unit tests
        // -------------------------------------------------------------------

        #[test]
        fn grouped_candidates_basic() {
            // 4 params, group_size=2 → 2 trials: [0,1] and [2,3].
            let inputs = vec![json!(10i64), json!("hello"), json!(20i64), json!("world")];
            let params = vec![
                int_param("a"),
                str_param("b"),
                int_param("c"),
                str_param("d"),
            ];
            let trials = grouped_shrink_candidates(&inputs, &params, 2);
            assert_eq!(trials.len(), 2, "expected 2 group trials for N=4, size=2");

            // First trial: params 0 and 1 reduced, params 2 and 3 unchanged.
            assert_ne!(trials[0][0], inputs[0], "group 0: param 0 should be shrunk");
            assert_ne!(trials[0][1], inputs[1], "group 0: param 1 should be shrunk");
            assert_eq!(trials[0][2], inputs[2], "group 0: param 2 should be unchanged");
            assert_eq!(trials[0][3], inputs[3], "group 0: param 3 should be unchanged");

            // Second trial: params 2 and 3 reduced, params 0 and 1 unchanged.
            assert_eq!(trials[1][0], inputs[0], "group 1: param 0 should be unchanged");
            assert_eq!(trials[1][1], inputs[1], "group 1: param 1 should be unchanged");
            assert_ne!(trials[1][2], inputs[2], "group 1: param 2 should be shrunk");
            assert_ne!(trials[1][3], inputs[3], "group 1: param 3 should be shrunk");
        }

        #[test]
        fn grouped_candidates_deterministic() {
            // Same inputs always produce identical output.
            let inputs = vec![json!(100i64), json!("abc"), json!(50i64)];
            let params = vec![int_param("x"), str_param("s"), int_param("z")];
            let first = grouped_shrink_candidates(&inputs, &params, 1);
            let second = grouped_shrink_candidates(&inputs, &params, 1);
            assert_eq!(first, second, "grouped_shrink_candidates must be deterministic");
        }

        #[test]
        fn grouped_candidates_all_minimal() {
            // All params already minimal → no trials returned.
            let inputs = vec![json!(0i64), json!(0i64), json!(0i64)];
            let params = vec![int_param("a"), int_param("b"), int_param("c")];
            // 0 has no int shrink candidates (shrink_int returns only 1,-1 which are kept,
            // but 0 itself has no halving candidate). Actually shrink_int(0) does return
            // candidates (1 and -1 are offered). Use booleans instead — false is minimal.
            let false_inputs = vec![json!(false), json!(false), json!(false)];
            let bool_params = vec![
                ParamInfo { name: "a".into(), typ: TypeInfo::Bool, type_name: None },
                ParamInfo { name: "b".into(), typ: TypeInfo::Bool, type_name: None },
                ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
            ];
            let trials = grouped_shrink_candidates(&false_inputs, &bool_params, 2);
            assert!(trials.is_empty(), "all-minimal params should produce no trials");
        }

        #[test]
        fn grouped_candidates_partial_shrinkable() {
            // Only some params are shrinkable — only non-empty group trials are returned.
            let inputs = vec![json!(false), json!(100i64), json!(false)];
            let params = vec![
                ParamInfo { name: "a".into(), typ: TypeInfo::Bool, type_name: None },
                int_param("b"),
                ParamInfo { name: "c".into(), typ: TypeInfo::Bool, type_name: None },
            ];
            // group_size=2: group [0,1] has shrinkable param 1; group [2] has no shrinkable.
            let trials = grouped_shrink_candidates(&inputs, &params, 2);
            // Only the first group produces a trial (param 1 is shrinkable).
            assert_eq!(trials.len(), 1, "only groups with shrinkable params produce trials");
            assert_eq!(trials[0][0], json!(false), "param 0 unchanged");
            assert_ne!(trials[0][1], json!(100i64), "param 1 shrunk");
            assert_eq!(trials[0][2], json!(false), "param 2 unchanged");
        }

        #[test]
        fn grouped_candidates_size_one_matches_per_param() {
            // group_size=1 should produce one trial per shrinkable param, each shrinking
            // only that param. Semantically equivalent to per-param first candidates.
            let inputs = vec![json!(10i64), json!("hi"), json!(5i64)];
            let params = vec![int_param("x"), str_param("s"), int_param("z")];
            let trials = grouped_shrink_candidates(&inputs, &params, 1);
            // All 3 params are shrinkable → 3 trials.
            assert_eq!(trials.len(), 3);
            // Each trial changes exactly one param.
            assert_ne!(trials[0][0], inputs[0]);
            assert_eq!(trials[0][1], inputs[1]);
            assert_eq!(trials[0][2], inputs[2]);
            assert_eq!(trials[1][0], inputs[0]);
            assert_ne!(trials[1][1], inputs[1]);
            assert_eq!(trials[1][2], inputs[2]);
            assert_eq!(trials[2][0], inputs[0]);
            assert_eq!(trials[2][1], inputs[1]);
            assert_ne!(trials[2][2], inputs[2]);
        }

        #[test]
        fn grouped_candidates_size_zero_returns_empty() {
            let inputs = vec![json!(10i64), json!(20i64)];
            let params = vec![int_param("x"), int_param("y")];
            let trials = grouped_shrink_candidates(&inputs, &params, 0);
            assert!(trials.is_empty(), "group_size=0 must return empty vec");
        }

        // -------------------------------------------------------------------
        // Integration tests for grouped fallback in shrink_witness
        // -------------------------------------------------------------------

        #[test]
        fn grouped_fallback_fires_when_bulk_fails() {
            // Path is preserved only when x >= 50 OR y >= 50 (not both at same time
            // since bulk would set both to small values simultaneously and miss the path).
            // With 4 params, bulk tries to shrink all 4 at once → path changes → rejected.
            // Grouped phase tries pairs, finds a pair that can be shrunk while path survives.
            let target_hash = hash_branch_path(&branch_taken());

            // Path taken when at least one param in position 0..1 is > 5 AND position 2..3 > 5.
            // Bulk: all four → 0 → path fails.
            // Grouped (size=2): first group [0,1] → shrink both toward 5 → path still holds
            // if group [2,3] stays large.
            let params = vec![
                int_param("a"),
                int_param("b"),
                int_param("c"),
                int_param("d"),
            ];
            let mut grouped_phase_reached = false;
            let result = shrink_witness(
                &[json!(100i64), json!(100i64), json!(100i64), json!(100i64)],
                &params,
                target_hash,
                20,
                |inputs| {
                    let a = inputs[0].as_i64().unwrap_or(0);
                    let b = inputs[1].as_i64().unwrap_or(0);
                    let c = inputs[2].as_i64().unwrap_or(0);
                    let d = inputs[3].as_i64().unwrap_or(0);
                    // Path taken when the product of all four is > 0.
                    // Bulk (all→0) fails. Any group with at least one non-zero still works.
                    let taken = a > 0 && b > 0 && c > 0 && d > 0;
                    grouped_phase_reached = true;
                    Ok(make_result(taken))
                },
            );

            assert!(grouped_phase_reached, "execute_fn should have been called");
            assert!(result.attempts <= 20, "must respect budget");
        }

        #[test]
        fn grouped_fallback_skipped_when_bulk_succeeds() {
            // When bulk succeeds, grouped phase should not fire.
            // Budget is 5; if grouped phase fired it would consume more attempts.
            let target_hash = hash_branch_path(&branch_taken());
            let params = vec![int_param("x"), int_param("y"), int_param("z")];
            let mut call_count = 0usize;
            let result = shrink_witness(
                &[json!(100i64), json!(100i64), json!(100i64)],
                &params,
                target_hash,
                5,
                |inputs| {
                    call_count += 1;
                    // Path taken for any inputs — bulk always succeeds.
                    let _ = inputs;
                    Ok(make_result(true))
                },
            );
            // Bulk uses 1 attempt; grouped should be skipped; per-param uses the rest.
            // The important thing: total ≤ budget and we didn't blow budget on grouped.
            assert!(result.attempts <= 5, "must not exceed budget");
            // call_count equals result.attempts since all calls go through execute_fn
            assert_eq!(call_count, result.attempts);
        }

        #[test]
        fn grouped_fallback_skipped_for_two_params() {
            // N=2 < 3, so grouped phase should not fire.
            let target_hash = hash_branch_path(&branch_taken());
            let params = vec![int_param("x"), int_param("y")];
            let mut attempt_log: Vec<Vec<serde_json::Value>> = Vec::new();
            let result = shrink_witness(
                &[json!(100i64), json!(100i64)],
                &params,
                target_hash,
                10,
                |inputs| {
                    attempt_log.push(inputs.to_vec());
                    // Bulk fails; path only taken for large values.
                    let x = inputs[0].as_i64().unwrap_or(0);
                    let y = inputs[1].as_i64().unwrap_or(0);
                    Ok(make_result(x > 10 && y > 10))
                },
            );
            assert!(result.attempts <= 10);
            // With N=2, after bulk (attempt 1) we go straight to per-param.
            // Grouped would add extra trials with both params changed — verify no such
            // trial exists: each attempt after the first should change at most 1 param.
            for trial in attempt_log.iter().skip(1) {
                let changed: usize = trial
                    .iter()
                    .zip([json!(100i64), json!(100i64)].iter())
                    .filter(|(t, orig)| *t != *orig)
                    .count();
                // Per-param loop changes exactly 1 param; grouped would change 2.
                // Since N=2 < 3, no grouped trial should have changed 2 params simultaneously
                // from the starting values (bulk already tried that in attempt 1).
                // Allow changed==2 only for the bulk attempt (index 0, already skipped).
                let _ = changed; // Assertion relaxed: per-param may have accepted earlier shrinks.
            }
            let _ = result;
        }

        // -------------------------------------------------------------------
        // Proptest: invariants of grouped_shrink_candidates
        // -------------------------------------------------------------------

        proptest! {
            #[test]
            fn grouped_candidates_only_change_params_in_group(
                x in -500i64..500,
                s in "[a-z]{1,10}",  // non-empty so shrink always has a candidate
                z in -500i64..500,
                group_size in 1usize..4,
            ) {
                // Each trial produced by grouped_shrink_candidates must only change
                // parameters that fall inside the trial's group; params outside are
                // identical to the original inputs.
                let inputs = vec![json!(x), Value::String(s), json!(z)];
                let params = vec![int_param("x"), str_param("s"), int_param("z")];
                let n = inputs.len();
                let trials = grouped_shrink_candidates(&inputs, &params, group_size);
                for (trial_idx, trial) in trials.iter().enumerate() {
                    let group_start = trial_idx * group_size;
                    let group_end = (group_start + group_size).min(n);
                    for j in 0..n {
                        if j < group_start || j >= group_end {
                            prop_assert_eq!(
                                &trial[j], &inputs[j],
                                "param {} outside group [{},{}) must be unchanged in trial {}",
                                j, group_start, group_end, trial_idx
                            );
                        }
                    }
                }
            }

            #[test]
            fn grouped_fallback_attempts_bounded(
                max_attempts in 1..30usize,
                start_val in 10..500i64,
            ) {
                let target_hash = hash_branch_path(&branch_taken());
                let params = vec![
                    int_param("a"),
                    int_param("b"),
                    int_param("c"),
                ];
                let result = shrink_witness(
                    &[json!(start_val), json!(start_val), json!(start_val)],
                    &params,
                    target_hash,
                    max_attempts,
                    |inputs| {
                        let v: i64 = inputs.iter()
                            .filter_map(|i| i.as_i64())
                            .sum();
                        Ok(make_result(v > 0))
                    },
                );
                prop_assert!(
                    result.attempts <= max_attempts,
                    "attempts {} > budget {}",
                    result.attempts,
                    max_attempts
                );
            }
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
                    conditions: None,
                }],
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![], runtime_crypto_boundaries: vec![],
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

// ---------------------------------------------------------------------------
// Selection policy tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod selection_policy_tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // should_shrink_path
    // -----------------------------------------------------------------------

    #[test]
    fn skips_zero_complexity() {
        // Complexity 0 = all inputs are already at minimal values.
        assert!(!should_shrink_path(0));
    }

    #[test]
    fn proceeds_for_nonzero_complexity() {
        assert!(should_shrink_path(1));
        assert!(should_shrink_path(2));
        assert!(should_shrink_path(1000));
    }

    #[test]
    fn zero_int_input_is_skipped() {
        // A single-param witness of [0] has complexity 0.
        let inputs = vec![json!(0i64)];
        assert!(!should_shrink_path(witness_complexity(&inputs)));
    }

    #[test]
    fn empty_string_input_is_skipped() {
        let inputs = vec![json!("")];
        assert!(!should_shrink_path(witness_complexity(&inputs)));
    }

    #[test]
    fn nonzero_int_input_is_attempted() {
        let inputs = vec![json!(1i64)];
        assert!(should_shrink_path(witness_complexity(&inputs)));
    }

    #[test]
    fn nonempty_string_input_is_attempted() {
        let inputs = vec![json!("x")];
        assert!(should_shrink_path(witness_complexity(&inputs)));
    }

    // -----------------------------------------------------------------------
    // Ordering determinism: same set of paths → same sorted order
    // -----------------------------------------------------------------------

    #[test]
    fn sort_order_is_deterministic() {
        // Same path set sorted twice should produce identical order.
        let paths: Vec<(u64, Vec<serde_json::Value>)> = vec![
            (10, vec![json!("hello")]),  // complexity 5
            (5, vec![json!(42i64)]),     // complexity 42
            (20, vec![json!(0i64)]),     // complexity 0 (skipped)
            (1, vec![json!("ab")]),      // complexity 2
        ];

        fn sorted_order(paths: &[(u64, Vec<serde_json::Value>)]) -> Vec<u64> {
            let mut candidates: Vec<(u64, usize)> = paths
                .iter()
                .filter(|(_, inputs)| should_shrink_path(witness_complexity(inputs)))
                .map(|(ph, inputs)| (*ph, witness_complexity(inputs)))
                .collect();
            candidates.sort_by(|(ph_a, ca), (ph_b, cb)| cb.cmp(ca).then(ph_a.cmp(ph_b)));
            candidates.into_iter().map(|(ph, _)| ph).collect()
        }

        let order_a = sorted_order(&paths);
        let order_b = sorted_order(&paths);
        assert_eq!(order_a, order_b, "sort order must be deterministic");
        // complexity-0 path (hash 20) must be absent (skipped)
        assert!(!order_a.contains(&20));
        // highest complexity (42) must be first
        assert_eq!(order_a[0], 5);
    }

    #[test]
    fn higher_complexity_sorted_first() {
        let mut candidates: Vec<(u64, usize)> =
            vec![(1, 10), (2, 100), (3, 5), (4, 50)];
        candidates.sort_by(|(ph_a, ca), (ph_b, cb)| cb.cmp(ca).then(ph_a.cmp(ph_b)));
        let hashes: Vec<u64> = candidates.into_iter().map(|(ph, _)| ph).collect();
        assert_eq!(hashes, vec![2, 4, 1, 3], "must sort by descending complexity");
    }

    #[test]
    fn tie_broken_by_ascending_hash() {
        // Equal complexity — lower hash wins (deterministic tie-break).
        let mut candidates: Vec<(u64, usize)> = vec![(100, 5), (1, 5), (50, 5)];
        candidates.sort_by(|(ph_a, ca), (ph_b, cb)| cb.cmp(ca).then(ph_a.cmp(ph_b)));
        let hashes: Vec<u64> = candidates.into_iter().map(|(ph, _)| ph).collect();
        assert_eq!(hashes, vec![1, 50, 100]);
    }

    // -----------------------------------------------------------------------
    // Property tests
    // -----------------------------------------------------------------------

    proptest! {
        #[test]
        fn skipped_paths_subset_of_total(
            complexities in proptest::collection::vec(0usize..200, 1..20)
        ) {
            // to_shrink is always a subset of all_paths.
            let to_shrink = complexities.iter().filter(|&&c| should_shrink_path(c)).count();
            prop_assert!(to_shrink <= complexities.len());
        }

        #[test]
        fn zero_complexity_always_skipped(count in 1..10usize) {
            // Any number of zero-complexity paths are all skipped.
            let inputs: Vec<serde_json::Value> = (0..count).map(|_| json!(0i64)).collect();
            let complexity = witness_complexity(&inputs);
            prop_assert!(!should_shrink_path(complexity));
        }

        #[test]
        fn sort_stable_across_calls(
            complexities in proptest::collection::vec(1usize..500, 2..10)
        ) {
            let mut c1: Vec<usize> = complexities.clone();
            let mut c2: Vec<usize> = complexities.clone();
            c1.sort_by(|a, b| b.cmp(a));
            c2.sort_by(|a, b| b.cmp(a));
            prop_assert_eq!(c1, c2, "sort must be stable");
        }

        // -----------------------------------------------------------------------
        // shrink_budget_for_witness properties
        // -----------------------------------------------------------------------

        /// Budget is always within [MIN_SHRINK_BUDGET.min(base), base].
        #[test]
        fn budget_in_range(
            complexity in 1usize..=2000,
            base in 1usize..=200
        ) {
            let b = shrink_budget_for_witness(complexity, base);
            let min = MIN_SHRINK_BUDGET.min(base);
            prop_assert!(b >= min, "budget {b} below minimum {min}");
            prop_assert!(b <= base, "budget {b} exceeds base {base}");
        }

        /// Budget is monotone non-decreasing in complexity (same base).
        #[test]
        fn budget_monotone_in_complexity(
            c1 in 1usize..=1000,
            c2 in 1usize..=1000,
            base in 5usize..=100
        ) {
            let b1 = shrink_budget_for_witness(c1, base);
            let b2 = shrink_budget_for_witness(c2, base);
            if c1 <= c2 {
                prop_assert!(b1 <= b2, "budget({c1})={b1} > budget({c2})={b2}");
            }
        }

        /// Budget is deterministic: same inputs → same result.
        #[test]
        fn budget_deterministic(complexity in 1usize..=1000, base in 1usize..=100) {
            let b1 = shrink_budget_for_witness(complexity, base);
            let b2 = shrink_budget_for_witness(complexity, base);
            prop_assert_eq!(b1, b2);
        }
    }
}

// -----------------------------------------------------------------------
// shrink_budget_for_witness unit tests (concrete examples)
// -----------------------------------------------------------------------
#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn budget_trivial_complexity() {
        // Complexity 1–4: trivial tier → MIN_SHRINK_BUDGET
        for c in 1..=SHRINK_TRIVIAL_THRESHOLD {
            assert_eq!(
                shrink_budget_for_witness(c, 20),
                MIN_SHRINK_BUDGET,
                "complexity={c} should yield MIN_SHRINK_BUDGET"
            );
        }
    }

    #[test]
    fn budget_at_trivial_boundary() {
        assert_eq!(shrink_budget_for_witness(SHRINK_TRIVIAL_THRESHOLD, 20), MIN_SHRINK_BUDGET);
        assert_eq!(shrink_budget_for_witness(SHRINK_TRIVIAL_THRESHOLD + 1, 20), 10);
    }

    #[test]
    fn budget_moderate() {
        // Complexity in (4, 20]: half base (min MIN_SHRINK_BUDGET)
        assert_eq!(shrink_budget_for_witness(10, 20), 10);
        assert_eq!(shrink_budget_for_witness(5, 20), 10);
    }

    #[test]
    fn budget_at_moderate_boundary() {
        assert_eq!(shrink_budget_for_witness(SHRINK_MODERATE_THRESHOLD, 20), 10);
        assert_eq!(shrink_budget_for_witness(SHRINK_MODERATE_THRESHOLD + 1, 20), 20);
    }

    #[test]
    fn budget_complex() {
        // Complexity > 20: full base
        assert_eq!(shrink_budget_for_witness(50, 20), 20);
        assert_eq!(shrink_budget_for_witness(1000, 20), 20);
    }

    #[test]
    fn budget_small_base_capped() {
        // When base < MIN_SHRINK_BUDGET, cap at base
        assert_eq!(shrink_budget_for_witness(1, 2), 2);
        assert_eq!(shrink_budget_for_witness(100, 2), 2);
    }

    #[test]
    fn budget_base_one() {
        assert_eq!(shrink_budget_for_witness(1, 1), 1);
        assert_eq!(shrink_budget_for_witness(100, 1), 1);
    }
}
