//! Daikon-style invariant detection over execution record specimens.
//!
//! Given a set of [`ExecutionRecord`] specimens (typically from a single behavior
//! cluster), checks invariant templates against all specimens and returns those
//! that hold universally. Uses `rayon` for parallel checking across both clusters
//! and candidate invariants.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::execution_record::ExecutionRecord;

/// Whether an invariant applies to inputs or outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantTarget {
    Input,
    Output,
}

/// A detected invariant that holds across all specimens in a cluster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invariant {
    /// Human-readable description of the invariant.
    pub description: String,
    /// What the invariant applies to.
    pub target: InvariantTarget,
    /// The specific template that matched.
    pub kind: InvariantKind,
}

/// The kind of invariant template that was detected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvariantKind {
    /// A numeric comparison holds for a parameter: e.g., x > 0.
    NumericComparison {
        path: JsonPath,
        op: ComparisonOp,
        value: f64,
    },
    /// A parameter always equals a constant value.
    NumericConstant {
        path: JsonPath,
        value: f64,
    },
    /// A value is never null/absent.
    NotNull {
        path: JsonPath,
    },
    /// A value is always null.
    IsNull {
        path: JsonPath,
    },
    /// A string is never empty.
    StringNonEmpty {
        path: JsonPath,
    },
    /// A string always has a specific length.
    StringLength {
        path: JsonPath,
        op: ComparisonOp,
        value: usize,
    },
    /// An output field equals an input field (output.field == input[param_index].field).
    OutputEqualsInput {
        output_path: JsonPath,
        param_index: usize,
        input_path: JsonPath,
    },
    /// A boolean is always true.
    AlwaysTrue {
        path: JsonPath,
    },
    /// A boolean is always false.
    AlwaysFalse {
        path: JsonPath,
    },
}

/// Comparison operators for numeric invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    Gt,
    Ge,
    Lt,
    Le,
}

impl std::fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparisonOp::Gt => write!(f, ">"),
            ComparisonOp::Ge => write!(f, ">="),
            ComparisonOp::Lt => write!(f, "<"),
            ComparisonOp::Le => write!(f, "<="),
        }
    }
}

/// A path into a JSON value, e.g., `["order", "items", "length"]`.
pub type JsonPath = Vec<String>;

/// Resolve a path into a JSON value, returning `None` if any segment is missing.
fn resolve_path<'a>(value: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(segment.as_str())?;
            }
            serde_json::Value::Array(arr) => {
                if segment == "length" {
                    // Handled specially by callers — arrays don't store their length as a field.
                    return None;
                }
                let idx: usize = segment.parse().ok()?;
                current = arr.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Get the numeric value at a path, treating JSON integers and floats uniformly.
fn get_numeric(value: &serde_json::Value, path: &[String]) -> Option<f64> {
    let resolved = resolve_path(value, path)?;
    resolved.as_f64()
}

/// Get the string value at a path.
fn get_string<'a>(value: &'a serde_json::Value, path: &[String]) -> Option<&'a str> {
    let resolved = resolve_path(value, path)?;
    resolved.as_str()
}

/// Get the boolean value at a path.
fn get_bool(value: &serde_json::Value, path: &[String]) -> Option<bool> {
    let resolved = resolve_path(value, path)?;
    resolved.as_bool()
}

/// Check if a value at a path is null.
fn is_null(value: &serde_json::Value, path: &[String]) -> bool {
    match resolve_path(value, path) {
        Some(v) => v.is_null(),
        None => true,
    }
}

/// Format a JSON path as a dotted string.
fn format_path(path: &[String]) -> String {
    path.join(".")
}

// ---------------------------------------------------------------------------
// Candidate generation
// ---------------------------------------------------------------------------

/// A candidate invariant to check against specimens.
struct Candidate {
    invariant: Invariant,
    check: Box<dyn Fn(&ExecutionRecord) -> bool + Send + Sync>,
}

/// Extract all leaf paths from a JSON value for candidate generation.
fn extract_paths(value: &serde_json::Value, prefix: &[String], out: &mut Vec<(JsonPath, serde_json::Value)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let mut path = prefix.to_vec();
                path.push(key.clone());
                extract_paths(val, &path, out);
            }
        }
        serde_json::Value::Array(arr) => {
            // Record the array length as a virtual path
            let mut len_path = prefix.to_vec();
            len_path.push("length".to_string());
            out.push((len_path, serde_json::Value::from(arr.len())));
            // Also extract paths from array elements (using first element as representative)
            if let Some(first) = arr.first() {
                let mut idx_path = prefix.to_vec();
                idx_path.push("0".to_string());
                extract_paths(first, &idx_path, out);
            }
        }
        _ => {
            out.push((prefix.to_vec(), value.clone()));
        }
    }
}

/// Get the value from a specimen based on target (input or output).
fn target_value(record: &ExecutionRecord, target: InvariantTarget) -> serde_json::Value {
    match target {
        InvariantTarget::Input => {
            if record.parameters.len() == 1 {
                record.parameters[0].clone()
            } else {
                serde_json::Value::Array(record.parameters.clone())
            }
        }
        InvariantTarget::Output => record
            .return_value
            .clone()
            .unwrap_or(serde_json::Value::Null),
    }
}

/// Generate candidate invariants from the first specimen.
fn generate_candidates(
    specimens: &[ExecutionRecord],
    target: InvariantTarget,
) -> Vec<Candidate> {
    if specimens.is_empty() {
        return vec![];
    }

    let first = &specimens[0];
    let value = target_value(first, target);

    let mut paths_and_values = Vec::new();
    extract_paths(&value, &[], &mut paths_and_values);

    let mut candidates: Vec<Candidate> = Vec::new();

    for (path, sample_value) in &paths_and_values {
        // Numeric invariants
        if let Some(num) = sample_value.as_f64() {
            generate_numeric_candidates(&mut candidates, path.clone(), num, target);
        }

        // String invariants
        if let Some(s) = sample_value.as_str() {
            generate_string_candidates(&mut candidates, path.clone(), s, target);
        }

        // Boolean invariants
        if let Some(b) = sample_value.as_bool() {
            generate_bool_candidates(&mut candidates, path.clone(), b, target);
        }

        // Null check invariants
        if sample_value.is_null() {
            generate_null_candidates(&mut candidates, path.clone(), true, target);
        } else {
            generate_null_candidates(&mut candidates, path.clone(), false, target);
        }
    }

    // Output-input relation candidates
    if target == InvariantTarget::Output {
        generate_output_input_candidates(&mut candidates, specimens);
    }

    candidates
}

fn generate_numeric_candidates(
    candidates: &mut Vec<Candidate>,
    path: JsonPath,
    _sample: f64,
    target: InvariantTarget,
) {
    // x > 0
    {
        let p = path.clone();
        let desc = format!("{} > 0", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::NumericComparison {
                    path: p.clone(),
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                get_numeric(&val, &p).is_some_and(|n| n > 0.0)
            }),
        });
    }

    // x >= 0
    {
        let p = path.clone();
        let desc = format!("{} >= 0", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::NumericComparison {
                    path: p.clone(),
                    op: ComparisonOp::Ge,
                    value: 0.0,
                },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                get_numeric(&val, &p).is_some_and(|n| n >= 0.0)
            }),
        });
    }

    // x < 0
    {
        let p = path.clone();
        let desc = format!("{} < 0", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::NumericComparison {
                    path: p.clone(),
                    op: ComparisonOp::Lt,
                    value: 0.0,
                },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                get_numeric(&val, &p).is_some_and(|n| n < 0.0)
            }),
        });
    }

    // x == C (constant detection): collect all values, check if they're all the same
    {
        let p = path.clone();
        let desc_prefix = format_path(&p);
        candidates.push(Candidate {
            invariant: Invariant {
                description: format!("{desc_prefix} == <constant>"),
                target,
                kind: InvariantKind::NumericConstant {
                    path: p.clone(),
                    value: 0.0, // placeholder, will be set during detection
                },
            },
            check: Box::new(move |_record| {
                // This is handled specially in detect_invariants
                true
            }),
        });
    }
}

fn generate_string_candidates(
    candidates: &mut Vec<Candidate>,
    path: JsonPath,
    _sample: &str,
    target: InvariantTarget,
) {
    // s.len() > 0 (non-empty string)
    let p = path.clone();
    let desc = format!("{} is non-empty", format_path(&p));
    candidates.push(Candidate {
        invariant: Invariant {
            description: desc,
            target,
            kind: InvariantKind::StringNonEmpty { path: p.clone() },
        },
        check: Box::new(move |record| {
            let val = target_value(record, target);
            get_string(&val, &p).is_some_and(|s| !s.is_empty())
        }),
    });
}

fn generate_bool_candidates(
    candidates: &mut Vec<Candidate>,
    path: JsonPath,
    sample: bool,
    target: InvariantTarget,
) {
    if sample {
        let p = path.clone();
        let desc = format!("{} is always true", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::AlwaysTrue { path: p.clone() },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                get_bool(&val, &p) == Some(true)
            }),
        });
    } else {
        let p = path.clone();
        let desc = format!("{} is always false", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::AlwaysFalse { path: p.clone() },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                get_bool(&val, &p) == Some(false)
            }),
        });
    }
}

fn generate_null_candidates(
    candidates: &mut Vec<Candidate>,
    path: JsonPath,
    sample_is_null: bool,
    target: InvariantTarget,
) {
    if sample_is_null {
        let p = path.clone();
        let desc = format!("{} is always null", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::IsNull { path: p.clone() },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                is_null(&val, &p)
            }),
        });
    } else {
        let p = path.clone();
        let desc = format!("{} != null", format_path(&p));
        candidates.push(Candidate {
            invariant: Invariant {
                description: desc,
                target,
                kind: InvariantKind::NotNull { path: p.clone() },
            },
            check: Box::new(move |record| {
                let val = target_value(record, target);
                !is_null(&val, &p)
            }),
        });
    }
}

fn generate_output_input_candidates(
    candidates: &mut Vec<Candidate>,
    specimens: &[ExecutionRecord],
) {
    if specimens.is_empty() {
        return;
    }
    let first = &specimens[0];
    let output = target_value(first, InvariantTarget::Output);

    let mut output_paths = Vec::new();
    extract_paths(&output, &[], &mut output_paths);

    for (param_index, param_val) in first.parameters.iter().enumerate() {
        let mut input_paths = Vec::new();
        extract_paths(param_val, &[], &mut input_paths);

        for (out_path, out_sample) in &output_paths {
            for (in_path, in_sample) in &input_paths {
                // Only check if first specimen shows equality
                if out_sample == in_sample && !out_sample.is_null() {
                    let op = out_path.clone();
                    let ip = in_path.clone();
                    let pi = param_index;
                    let out_formatted = format_path(out_path);
                    let in_formatted = if first.parameters.len() == 1 {
                        format_path(in_path)
                    } else {
                        format!("param[{}].{}", param_index, format_path(in_path))
                    };
                    let desc = format!("output.{out_formatted} == input.{in_formatted}");

                    candidates.push(Candidate {
                        invariant: Invariant {
                            description: desc,
                            target: InvariantTarget::Output,
                            kind: InvariantKind::OutputEqualsInput {
                                output_path: op.clone(),
                                param_index: pi,
                                input_path: ip.clone(),
                            },
                        },
                        check: Box::new(move |record| {
                            let out_val = target_value(record, InvariantTarget::Output);
                            let in_val = record.parameters.get(pi);
                            match (resolve_path(&out_val, &op), in_val.and_then(|v| resolve_path(v, &ip))) {
                                (Some(a), Some(b)) => a == b,
                                _ => false,
                            }
                        }),
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect invariants that hold across all specimens for the given target.
pub fn detect_invariants(
    specimens: &[ExecutionRecord],
    target: InvariantTarget,
) -> Vec<Invariant> {
    if specimens.is_empty() {
        return vec![];
    }

    let candidates = generate_candidates(specimens, target);

    let mut invariants: Vec<Invariant> = candidates
        .into_par_iter()
        .filter(|candidate| {
            // Skip the placeholder constant-detection candidate (handled below)
            if matches!(candidate.invariant.kind, InvariantKind::NumericConstant { .. }) {
                return false;
            }
            specimens.iter().all(|s| (candidate.check)(s))
        })
        .map(|c| c.invariant)
        .collect();

    // Numeric constant detection: for each numeric path, check if all values are identical
    detect_numeric_constants(specimens, target, &mut invariants);

    // Filter out trivially true invariants
    filter_trivial(&mut invariants);

    // Sort for deterministic output
    invariants.sort_by(|a, b| a.description.cmp(&b.description));
    invariants
}

/// Check if all specimens have the same numeric value at each path.
fn detect_numeric_constants(
    specimens: &[ExecutionRecord],
    target: InvariantTarget,
    invariants: &mut Vec<Invariant>,
) {
    if specimens.is_empty() {
        return;
    }

    let first = &specimens[0];
    let value = target_value(first, target);
    let mut paths = Vec::new();
    extract_paths(&value, &[], &mut paths);

    for (path, sample) in &paths {
        if let Some(first_num) = sample.as_f64() {
            let all_same = specimens.iter().all(|s| {
                let val = target_value(s, target);
                get_numeric(&val, path) == Some(first_num)
            });
            if all_same {
                invariants.push(Invariant {
                    description: format!("{} == {first_num}", format_path(path)),
                    target,
                    kind: InvariantKind::NumericConstant {
                        path: path.clone(),
                        value: first_num,
                    },
                });
            }
        }
    }
}

/// Remove trivially true invariants.
///
/// An invariant is trivially true if it conveys no useful information. For example:
/// - `x >= 0` when `x > 0` also holds (the stronger invariant subsumes it)
/// - A constant invariant `x == C` when a comparison `x > 0` also holds and C > 0
///   (we keep both since constant equality is strictly more informative)
fn filter_trivial(invariants: &mut Vec<Invariant>) {
    // Remove `x >= 0` when `x > 0` is also present (for the same path)
    let gt_zero_paths: Vec<JsonPath> = invariants
        .iter()
        .filter_map(|inv| match &inv.kind {
            InvariantKind::NumericComparison { path, op: ComparisonOp::Gt, value }
                if *value == 0.0 =>
            {
                Some(path.clone())
            }
            _ => None,
        })
        .collect();

    invariants.retain(|inv| {
        match &inv.kind {
            InvariantKind::NumericComparison {
                path,
                op: ComparisonOp::Ge,
                value,
            } if *value == 0.0 => !gt_zero_paths.contains(path),
            _ => true,
        }
    });

    // Remove `x < 0` when values are actually >= 0 (these would have been filtered
    // already by the check, but just in case)
    // No action needed — the candidate filtering handles this.

    // Remove numeric constant when there's only one specimen (trivially true)
    // Actually, we keep it — it's still informative. The caller decides what's useful.
}

/// Detect invariants across multiple behavior clusters in parallel.
///
/// For each cluster's set of specimens, detects both input and output invariants.
/// Returns a vec of `(input_invariants, output_invariants)` tuples, one per cluster.
pub fn detect_invariants_all_clusters(
    clusters: &[Vec<ExecutionRecord>],
) -> Vec<(Vec<Invariant>, Vec<Invariant>)> {
    clusters
        .par_iter()
        .map(|specimens| {
            let input_invs = detect_invariants(specimens, InvariantTarget::Input);
            let output_invs = detect_invariants(specimens, InvariantTarget::Output);
            (input_invs, output_invs)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::execution_record::ExecutionRecord;

    /// Build a minimal execution record with the given parameters and return value.
    fn make_record(
        params: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
    ) -> ExecutionRecord {
        ExecutionRecord {
            function_id: "test_fn".to_string(),
            input_hash: 0,
            parameters: params,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            return_value,
            thrown_error: None,
            side_effects: vec![],
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
            timestamp: String::new(),
            engine_version: String::new(),
        }
    }

    #[test]
    fn detects_positive_numeric_invariant() {
        let specimens = vec![
            make_record(vec![json!({"x": 5})], Some(json!(10))),
            make_record(vec![json!({"x": 10})], Some(json!(20))),
            make_record(vec![json!({"x": 1})], Some(json!(2))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_x_gt_0 = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Gt,
                    value,
                } if path == &vec!["x".to_string()] && *value == 0.0
            )
        });
        assert!(has_x_gt_0, "should detect x > 0, got: {invariants:?}");
    }

    #[test]
    fn does_not_detect_positive_when_zero_present() {
        let specimens = vec![
            make_record(vec![json!({"x": 0})], Some(json!(0))),
            make_record(vec![json!({"x": 5})], Some(json!(10))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_x_gt_0 = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Gt,
                    value,
                } if path == &vec!["x".to_string()] && *value == 0.0
            )
        });
        assert!(!has_x_gt_0, "should not detect x > 0 when x=0 is present");
    }

    #[test]
    fn detects_not_null_invariant() {
        let specimens = vec![
            make_record(vec![json!({"name": "alice"})], Some(json!("ok"))),
            make_record(vec![json!({"name": "bob"})], Some(json!("ok"))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_not_null = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::NotNull { path } if path == &vec!["name".to_string()])
        });
        assert!(has_not_null, "should detect name != null, got: {invariants:?}");
    }

    #[test]
    fn detects_string_non_empty() {
        let specimens = vec![
            make_record(vec![json!({"s": "hello"})], None),
            make_record(vec![json!({"s": "world"})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_non_empty = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::StringNonEmpty { path } if path == &vec!["s".to_string()])
        });
        assert!(has_non_empty, "should detect s is non-empty, got: {invariants:?}");
    }

    #[test]
    fn does_not_detect_non_empty_when_empty_present() {
        let specimens = vec![
            make_record(vec![json!({"s": ""})], None),
            make_record(vec![json!({"s": "hello"})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_non_empty = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::StringNonEmpty { path } if path == &vec!["s".to_string()])
        });
        assert!(!has_non_empty, "should not detect non-empty when empty string present");
    }

    #[test]
    fn detects_output_equals_input_field() {
        let specimens = vec![
            make_record(vec![json!({"len": 3})], Some(json!({"len": 3}))),
            make_record(vec![json!({"len": 5})], Some(json!({"len": 5}))),
            make_record(vec![json!({"len": 0})], Some(json!({"len": 0}))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Output);

        let has_relation = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::OutputEqualsInput {
                    output_path,
                    param_index: 0,
                    input_path,
                } if output_path == &vec!["len".to_string()] && input_path == &vec!["len".to_string()]
            )
        });
        assert!(has_relation, "should detect output.len == input.len, got: {invariants:?}");
    }

    #[test]
    fn does_not_detect_false_output_input_relation() {
        let specimens = vec![
            make_record(vec![json!({"len": 3})], Some(json!({"len": 3}))),
            make_record(vec![json!({"len": 5})], Some(json!({"len": 7}))), // different!
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Output);

        let has_relation = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::OutputEqualsInput { .. })
        });
        assert!(!has_relation, "should not detect output.len == input.len when they differ");
    }

    #[test]
    fn detects_numeric_constant() {
        let specimens = vec![
            make_record(vec![json!(1)], Some(json!(42))),
            make_record(vec![json!(2)], Some(json!(42))),
            make_record(vec![json!(3)], Some(json!(42))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Output);

        let has_constant = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericConstant { path, value }
                if path.is_empty() && *value == 42.0
            )
        });
        assert!(has_constant, "should detect output == 42, got: {invariants:?}");
    }

    #[test]
    fn filters_ge_zero_when_gt_zero_present() {
        // When all values are > 0, both > 0 and >= 0 would match.
        // The >= 0 should be filtered as it's subsumed by > 0.
        let specimens = vec![
            make_record(vec![json!({"x": 1})], None),
            make_record(vec![json!({"x": 5})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_gt_0 = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison { path, op: ComparisonOp::Gt, value }
                if path == &vec!["x".to_string()] && *value == 0.0
            )
        });
        let has_ge_0 = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison { path, op: ComparisonOp::Ge, value }
                if path == &vec!["x".to_string()] && *value == 0.0
            )
        });

        assert!(has_gt_0, "should have x > 0");
        assert!(!has_ge_0, "should not have x >= 0 when x > 0 is present");
    }

    #[test]
    fn empty_specimens_returns_no_invariants() {
        let invariants = detect_invariants(&[], InvariantTarget::Input);
        assert!(invariants.is_empty());
    }

    #[test]
    fn detects_always_true_boolean() {
        let specimens = vec![
            make_record(vec![json!({"active": true})], None),
            make_record(vec![json!({"active": true})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_always_true = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::AlwaysTrue { path } if path == &vec!["active".to_string()])
        });
        assert!(has_always_true, "should detect active is always true, got: {invariants:?}");
    }

    #[test]
    fn detects_is_null_invariant() {
        let specimens = vec![
            make_record(vec![json!({"opt": null})], None),
            make_record(vec![json!({"opt": null})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_is_null = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::IsNull { path } if path == &vec!["opt".to_string()])
        });
        assert!(has_is_null, "should detect opt is always null, got: {invariants:?}");
    }

    #[test]
    fn combined_invariants_x_positive_and_output_len_equals_input_len() {
        // The acceptance criteria scenario: all inputs have x > 0 and output.len == input.len
        let specimens = vec![
            make_record(
                vec![json!({"x": 1, "len": 3})],
                Some(json!({"len": 3})),
            ),
            make_record(
                vec![json!({"x": 5, "len": 7})],
                Some(json!({"len": 7})),
            ),
            make_record(
                vec![json!({"x": 10, "len": 1})],
                Some(json!({"len": 1})),
            ),
        ];

        let input_invs = detect_invariants(&specimens, InvariantTarget::Input);
        let output_invs = detect_invariants(&specimens, InvariantTarget::Output);

        // Should detect x > 0
        let has_x_gt_0 = input_invs.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Gt,
                    value,
                } if path == &vec!["x".to_string()] && *value == 0.0
            )
        });
        assert!(has_x_gt_0, "should detect x > 0 in inputs, got: {input_invs:?}");

        // Should detect output.len == input.len
        let has_len_relation = output_invs.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::OutputEqualsInput {
                    output_path,
                    param_index: 0,
                    input_path,
                } if output_path == &vec!["len".to_string()] && input_path == &vec!["len".to_string()]
            )
        });
        assert!(
            has_len_relation,
            "should detect output.len == input.len, got: {output_invs:?}"
        );
    }

    #[test]
    fn parallel_cluster_detection() {
        let cluster1 = vec![
            make_record(vec![json!({"x": 1})], Some(json!(10))),
            make_record(vec![json!({"x": 2})], Some(json!(20))),
        ];
        let cluster2 = vec![
            make_record(vec![json!({"x": -1})], Some(json!(0))),
            make_record(vec![json!({"x": -5})], Some(json!(0))),
        ];

        let results = detect_invariants_all_clusters(&[cluster1, cluster2]);

        assert_eq!(results.len(), 2);

        // Cluster 1: x > 0
        let (input_invs, _) = &results[0];
        assert!(input_invs.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Gt,
                    value,
                } if path == &vec!["x".to_string()] && *value == 0.0
            )
        }));

        // Cluster 2: x < 0
        let (input_invs, _) = &results[1];
        assert!(input_invs.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Lt,
                    value,
                } if path == &vec!["x".to_string()] && *value == 0.0
            )
        }));
    }

    #[test]
    fn does_not_report_trivially_true_type_invariants() {
        // All x values are numbers — "x is a number" should NOT be reported
        // (there's no such template, but ensure we don't have spurious invariants)
        let specimens = vec![
            make_record(vec![json!({"x": -3})], None),
            make_record(vec![json!({"x": 0})], None),
            make_record(vec![json!({"x": 5})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        // x spans negative, zero, and positive — no numeric comparison should hold
        let has_numeric_comparison = invariants.iter().any(|inv| {
            matches!(&inv.kind, InvariantKind::NumericComparison { path, .. } if path == &vec!["x".to_string()])
        });
        assert!(
            !has_numeric_comparison,
            "should not report numeric comparisons when values span negative/zero/positive, got: {invariants:?}"
        );
    }

    #[test]
    fn invariant_serialization_round_trips() {
        let inv = Invariant {
            description: "x > 0".to_string(),
            target: InvariantTarget::Input,
            kind: InvariantKind::NumericComparison {
                path: vec!["x".to_string()],
                op: ComparisonOp::Gt,
                value: 0.0,
            },
        };

        let json = serde_json::to_string(&inv).expect("serialize");
        let deserialized: Invariant = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(inv, deserialized);
    }

    #[test]
    fn nested_object_paths_detected() {
        let specimens = vec![
            make_record(vec![json!({"order": {"total": 100}})], None),
            make_record(vec![json!({"order": {"total": 200}})], None),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        let has_nested = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison { path, op: ComparisonOp::Gt, value }
                if path == &vec!["order".to_string(), "total".to_string()] && *value == 0.0
            )
        });
        assert!(has_nested, "should detect order.total > 0, got: {invariants:?}");
    }

    #[test]
    fn single_parameter_without_wrapping_object() {
        // When there's a single primitive parameter, it should still work
        let specimens = vec![
            make_record(vec![json!(5)], Some(json!(10))),
            make_record(vec![json!(10)], Some(json!(20))),
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Input);

        // The root value is numeric and > 0
        let has_gt_0 = invariants.iter().any(|inv| {
            matches!(
                &inv.kind,
                InvariantKind::NumericComparison {
                    path,
                    op: ComparisonOp::Gt,
                    value,
                } if path.is_empty() && *value == 0.0
            )
        });
        assert!(has_gt_0, "should detect param > 0 for scalar input, got: {invariants:?}");
    }
}
