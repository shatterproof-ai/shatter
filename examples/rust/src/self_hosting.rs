//! Self-hosting test targets: functions extracted from shatter-core.
//!
//! These are standalone copies of real shatter-core algorithms, used to validate
//! that shatter can explore its own code. Each function is self-contained with
//! any needed types inlined.

/// classify_float — 3 branches: total==0 → 0 (inconclusive),
/// ratio>=threshold → 1 (integer-treating), ratio<threshold → 2 (float-sensitive).
///
/// Extracted from `shatter-core::float_probe::classify`. Classifies a float
/// parameter based on probe agreement ratio. The division-by-zero guard and
/// threshold comparison are the key branch points the solver should discover.
pub fn classify_float(agreements: usize, total: usize, threshold: f64) -> i32 {
    if total == 0 {
        return 0; // Inconclusive
    }
    let ratio = agreements as f64 / total as f64;
    if ratio >= threshold {
        1 // IntegerTreating
    } else {
        2 // FloatSensitive
    }
}

/// coverage_percentages — 2 branches: total_branches==0 → all zeros,
/// total_branches>0 → computed percentages.
///
/// Extracted from `shatter-core::coverage_metrics::CoverageMetrics::percentages`.
/// Flattened from a method on a struct to a standalone function taking the
/// four counters and total as separate parameters. Returns (z3_pct, random_pct,
/// user_pct, uncovered_pct) as a tuple.
pub fn coverage_percentages(
    total_branches: usize,
    z3_solved: usize,
    random_found: usize,
    user_provided: usize,
    uncovered: usize,
) -> (f64, f64, f64, f64) {
    if total_branches == 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let total = total_branches as f64;
    (
        z3_solved as f64 / total * 100.0,
        random_found as f64 / total * 100.0,
        user_provided as f64 / total * 100.0,
        uncovered as f64 / total * 100.0,
    )
}

/// symexpr_ratio — 2 branches: (symexpr_count + unknown_count)==0 → 0.0,
/// otherwise → symexpr_count / total.
///
/// Extracted from `shatter-core::coverage_metrics::CoverageMetrics::symexpr_ratio`.
/// Reports what fraction of branch conditions the frontend could express
/// symbolically vs opaque unknowns.
pub fn symexpr_ratio(symexpr_count: usize, unknown_count: usize) -> f64 {
    let total = symexpr_count + unknown_count;
    if total == 0 {
        return 0.0;
    }
    symexpr_count as f64 / total as f64
}

/// executions_agree — 5 branches across two conditions:
/// path_a != path_b → false,
/// both errors with same type → true,
/// both errors with different type → false (via string comparison),
/// both ok with same value → true,
/// one error one ok → false.
///
/// Simplified from `shatter-core::float_probe::executions_agree`. Uses primitive
/// types instead of ExecuteResult structs: path hashes as u64, error presence as
/// bool, and return values as i64.
pub fn executions_agree(
    path_a: u64,
    path_b: u64,
    a_has_error: bool,
    b_has_error: bool,
    a_value: i64,
    b_value: i64,
) -> bool {
    if path_a != path_b {
        return false;
    }
    match (a_has_error, b_has_error) {
        (true, true) => a_value == b_value,
        (false, false) => a_value == b_value,
        _ => false,
    }
}
