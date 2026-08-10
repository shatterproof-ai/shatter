//! Float probe: detect integer-treating number parameters.
//!
//! TypeScript's `number` type covers both integers and floats. When shatter
//! explores a function like `intToRoman(n: number)`, it generates floats where
//! integers would be more useful. The float probe sends (float, floor) input
//! pairs and compares execution results to classify each Float parameter as
//! integer-treating, float-sensitive, or inconclusive.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::ExecuteResult;
use crate::types::{ParamInfo, TypeInfo};

/// Number of (float, floor) probe pairs per parameter.
pub const PROBE_COUNT: usize = 5;

/// Fraction of probes that must agree for an IntegerTreating classification.
pub const AGREEMENT_THRESHOLD: f64 = 0.8;

/// When generating biased inputs, this fraction should be integers.
pub const INTEGER_BIAS_RATIO: f64 = 0.8;

/// How a Float parameter treats its fractional component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloatClassification {
    /// The function floors/truncates the value — fractional part is ignored.
    IntegerTreating,
    /// The function's behavior depends on the fractional part.
    FloatSensitive,
    /// Not enough data to decide (e.g., all probes errored).
    Inconclusive,
}

/// Result of probing a single Float parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatProbeResult {
    /// Index of this parameter in the function's param list.
    pub param_index: usize,
    /// Name of the parameter.
    pub param_name: String,
    /// Classification based on probe results.
    pub classification: FloatClassification,
    /// Number of (float, floor) pairs that agreed.
    pub agreements: usize,
    /// Total probes attempted.
    pub total_probes: usize,
    /// Float values where behavior diverged from floor (for reporting).
    pub divergent_values: Vec<f64>,
}

/// Find indices of Float-typed parameters.
pub fn float_param_indices(params: &[ParamInfo]) -> Vec<usize> {
    params
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.typ, TypeInfo::Float))
        .map(|(i, _)| i)
        .collect()
}

/// Generate `count` probe pairs for a specific Float parameter.
///
/// Each pair consists of (float_inputs, floor_inputs) where the target param
/// gets a non-integer float in one and its floor in the other. All other
/// params are filled with neutral defaults.
pub fn generate_probe_pairs(
    params: &[ParamInfo],
    param_idx: usize,
    count: usize,
    rng: &mut impl Rng,
) -> Vec<(Vec<Value>, Vec<Value>)> {
    (0..count)
        .map(|_| {
            let float_val = generate_non_integer_float(rng);
            let floor_val = float_val.floor();

            let mut float_inputs = neutral_defaults(params);
            let mut floor_inputs = neutral_defaults(params);

            float_inputs[param_idx] = json!(float_val);
            floor_inputs[param_idx] = json!(floor_val);

            (float_inputs, floor_inputs)
        })
        .collect()
}

/// Check whether two executions agree: same branch path hash AND same return
/// value (or both throw the same error type).
pub fn executions_agree(a: &ExecuteResult, b: &ExecuteResult) -> bool {
    let path_a = hash_branch_path(a);
    let path_b = hash_branch_path(b);
    if path_a != path_b {
        return false;
    }

    match (&a.thrown_error, &b.thrown_error) {
        (Some(ea), Some(eb)) => ea.error_type == eb.error_type,
        (None, None) => a.return_value == b.return_value,
        _ => false,
    }
}

/// Classify a parameter based on probe agreement ratio.
pub fn classify(agreements: usize, total: usize, threshold: f64) -> FloatClassification {
    if total == 0 {
        return FloatClassification::Inconclusive;
    }
    let ratio = agreements as f64 / total as f64;
    if ratio >= threshold {
        FloatClassification::IntegerTreating
    } else {
        FloatClassification::FloatSensitive
    }
}

/// Build a bias map from probe results (param_index -> classification).
pub fn build_bias_map(results: &[FloatProbeResult]) -> HashMap<usize, FloatClassification> {
    results
        .iter()
        .map(|r| (r.param_index, r.classification))
        .collect()
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Generate a float that is guaranteed not to be an integer.
fn generate_non_integer_float(rng: &mut impl Rng) -> f64 {
    let candidates = [1.5, -2.7, 99.9, 0.3, -0.5, 3.17, 42.5, -17.3, 7.1, 256.99];
    let base = candidates[rng.random_range(0..candidates.len())];
    let offset = rng.random_range(0.01..0.49);
    let val: f64 = base + offset;
    if val.fract() == 0.0 { val + 0.1 } else { val }
}

/// Generate neutral default values for all parameters.
fn neutral_defaults(params: &[ParamInfo]) -> Vec<Value> {
    params
        .iter()
        .map(|p| match &p.typ {
            TypeInfo::Int { .. } => json!(1),
            TypeInfo::Float => json!(1.0),
            TypeInfo::Str => json!("test"),
            TypeInfo::Bool => json!(true),
            _ => json!(null),
        })
        .collect()
}

fn hash_branch_path(result: &ExecuteResult) -> u64 {
    let mut hasher = DefaultHasher::new();
    for decision in &result.branch_path {
        decision.branch_id.hash(&mut hasher);
        decision.taken.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::ErrorInfo;
    use crate::protocol::PerformanceMetrics;

    fn make_param(name: &str, typ: TypeInfo) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ,
            type_name: None,
        }
    }

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    fn make_exec(return_value: Value) -> ExecuteResult {
        ExecuteResult {
            return_value: Some(return_value),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
            performance: empty_perf(),
        }
    }

    fn make_error_exec(error_type: &str) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: error_type.to_string(),
                message: "test".to_string(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
            performance: empty_perf(),
        }
    }

    #[test]
    fn float_param_indices_finds_floats() {
        let params = vec![
            make_param("name", TypeInfo::Str),
            make_param("age", TypeInfo::Float),
            make_param("active", TypeInfo::Bool),
            make_param("score", TypeInfo::Float),
        ];
        assert_eq!(float_param_indices(&params), vec![1, 3]);
    }

    #[test]
    fn float_param_indices_empty_when_no_floats() {
        let params = vec![
            make_param("name", TypeInfo::Str),
            make_param("count", TypeInfo::Int { int_width: None, int_signed: None }),
        ];
        assert!(float_param_indices(&params).is_empty());
    }

    #[test]
    fn generate_probe_pairs_correct_structure() {
        let params = vec![
            make_param("label", TypeInfo::Str),
            make_param("value", TypeInfo::Float),
        ];
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let pairs = generate_probe_pairs(&params, 1, 3, &mut rng);

        assert_eq!(pairs.len(), 3);
        for (float_inputs, floor_inputs) in &pairs {
            assert_eq!(float_inputs.len(), 2);
            assert_eq!(floor_inputs.len(), 2);
            assert_eq!(float_inputs[0], json!("test"));
            let fv = float_inputs[1].as_f64().unwrap();
            assert_ne!(fv.fract(), 0.0, "probe float should be non-integer");
            let flv = floor_inputs[1].as_f64().unwrap();
            assert_eq!(flv.fract(), 0.0, "floor should be integer");
            assert_eq!(flv, fv.floor());
        }
    }

    #[test]
    fn executions_agree_same_return() {
        assert!(executions_agree(
            &make_exec(json!("hello")),
            &make_exec(json!("hello"))
        ));
    }

    #[test]
    fn executions_disagree_different_return() {
        assert!(!executions_agree(
            &make_exec(json!("hello")),
            &make_exec(json!("world"))
        ));
    }

    #[test]
    fn executions_agree_same_error_type() {
        assert!(executions_agree(
            &make_error_exec("TypeError"),
            &make_error_exec("TypeError")
        ));
    }

    #[test]
    fn executions_disagree_different_error_type() {
        assert!(!executions_agree(
            &make_error_exec("TypeError"),
            &make_error_exec("RangeError")
        ));
    }

    #[test]
    fn executions_disagree_return_vs_error() {
        assert!(!executions_agree(
            &make_exec(json!("ok")),
            &make_error_exec("TypeError")
        ));
    }

    #[test]
    fn classify_all_agree() {
        assert_eq!(
            classify(5, 5, AGREEMENT_THRESHOLD),
            FloatClassification::IntegerTreating
        );
    }

    #[test]
    fn classify_none_agree() {
        assert_eq!(
            classify(0, 5, AGREEMENT_THRESHOLD),
            FloatClassification::FloatSensitive
        );
    }

    #[test]
    fn classify_at_threshold() {
        assert_eq!(
            classify(4, 5, AGREEMENT_THRESHOLD),
            FloatClassification::IntegerTreating
        );
    }

    #[test]
    fn classify_below_threshold() {
        assert_eq!(
            classify(3, 5, AGREEMENT_THRESHOLD),
            FloatClassification::FloatSensitive
        );
    }

    #[test]
    fn classify_zero_probes() {
        assert_eq!(
            classify(0, 0, AGREEMENT_THRESHOLD),
            FloatClassification::Inconclusive
        );
    }

    #[test]
    fn build_bias_map_from_results() {
        let results = vec![
            FloatProbeResult {
                param_index: 1,
                param_name: "x".into(),
                classification: FloatClassification::IntegerTreating,
                agreements: 5,
                total_probes: 5,
                divergent_values: vec![],
            },
            FloatProbeResult {
                param_index: 3,
                param_name: "y".into(),
                classification: FloatClassification::FloatSensitive,
                agreements: 1,
                total_probes: 5,
                divergent_values: vec![1.5, 2.5],
            },
        ];
        let map = build_bias_map(&results);
        assert_eq!(map.get(&1), Some(&FloatClassification::IntegerTreating));
        assert_eq!(map.get(&3), Some(&FloatClassification::FloatSensitive));
        assert_eq!(map.get(&0), None);
    }
}
