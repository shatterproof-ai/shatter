//! Daikon-style invariant detection over execution record specimens.
//!
//! Given a set of [`ExecutionRecord`] specimens (typically from a single behavior
//! cluster), checks invariant templates against all specimens and returns those
//! that hold universally. Uses `rayon` for parallel checking across both clusters
//! and candidate invariants.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    NumericConstant { path: JsonPath, value: f64 },
    /// A value is never null/absent.
    NotNull { path: JsonPath },
    /// A value is always null.
    IsNull { path: JsonPath },
    /// A string is never empty.
    StringNonEmpty { path: JsonPath },
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
    AlwaysTrue { path: JsonPath },
    /// A boolean is always false.
    AlwaysFalse { path: JsonPath },
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
fn resolve_path<'a>(
    value: &'a serde_json::Value,
    path: &[String],
) -> Option<&'a serde_json::Value> {
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
fn extract_paths(
    value: &serde_json::Value,
    prefix: &[String],
    out: &mut Vec<(JsonPath, serde_json::Value)>,
) {
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
fn generate_candidates(specimens: &[ExecutionRecord], target: InvariantTarget) -> Vec<Candidate> {
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
                            match (
                                resolve_path(&out_val, &op),
                                in_val.and_then(|v| resolve_path(v, &ip)),
                            ) {
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
pub fn detect_invariants(specimens: &[ExecutionRecord], target: InvariantTarget) -> Vec<Invariant> {
    if specimens.is_empty() {
        return vec![];
    }

    let candidates = generate_candidates(specimens, target);

    let mut invariants: Vec<Invariant> = candidates
        .into_par_iter()
        .filter(|candidate| {
            // Skip the placeholder constant-detection candidate (handled below)
            if matches!(
                candidate.invariant.kind,
                InvariantKind::NumericConstant { .. }
            ) {
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
            InvariantKind::NumericComparison {
                path,
                op: ComparisonOp::Gt,
                value,
            } if *value == 0.0 => Some(path.clone()),
            _ => None,
        })
        .collect();

    invariants.retain(|inv| match &inv.kind {
        InvariantKind::NumericComparison {
            path,
            op: ComparisonOp::Ge,
            value,
        } if *value == 0.0 => !gt_zero_paths.contains(path),
        _ => true,
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
// Classified invariants: invariants annotated with target and human-readable label.
// ---------------------------------------------------------------------------

/// An invariant with its target (input/output) and a human-readable description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedInvariant {
    /// The underlying invariant.
    pub invariant: Invariant,
    /// Whether this invariant applies to inputs or outputs.
    pub target: InvariantTarget,
    /// Human-readable description (e.g. "param[0] > 0").
    pub label: String,
    /// Confidence score (0.0–1.0): fraction of specimens satisfying this invariant.
    pub confidence: f64,
    /// Number of specimens that satisfied this invariant.
    pub satisfied_count: usize,
    /// Total number of specimens checked.
    pub total_count: usize,
}

/// Detect invariants and return them with target classification and labels.
pub fn detect_classified_invariants(
    specimens: &[ExecutionRecord],
    target: InvariantTarget,
) -> Vec<ClassifiedInvariant> {
    let total = specimens.len();
    detect_invariants(specimens, target)
        .into_iter()
        .map(|inv| {
            let label = format_invariant_label(&inv, target);
            ClassifiedInvariant {
                invariant: inv,
                target,
                label,
                confidence: 1.0, // detect_invariants only returns invariants that hold for all specimens
                satisfied_count: total,
                total_count: total,
            }
        })
        .collect()
}

/// Format a human-readable label for an invariant.
fn format_invariant_label(inv: &Invariant, target: InvariantTarget) -> String {
    let prefix = match target {
        InvariantTarget::Input => "input",
        InvariantTarget::Output => "output",
    };

    let format_path = |path: &JsonPath| -> String {
        if path.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}.{}", path.join("."))
        }
    };

    match &inv.kind {
        InvariantKind::NumericComparison { path, op, value } => {
            let op_str = match op {
                ComparisonOp::Gt => ">",
                ComparisonOp::Ge => ">=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Le => "<=",
            };
            format!("{} {op_str} {value}", format_path(path))
        }
        InvariantKind::NumericConstant { path, value } => {
            format!("{} == {value}", format_path(path))
        }
        InvariantKind::NotNull { path } => {
            format!("{} is non-null", format_path(path))
        }
        InvariantKind::IsNull { path } => {
            format!("{} is null", format_path(path))
        }
        InvariantKind::StringNonEmpty { path } => {
            format!("{} is non-empty string", format_path(path))
        }
        InvariantKind::StringLength { path, op, value } => {
            let op_str = match op {
                ComparisonOp::Gt => ">",
                ComparisonOp::Ge => ">=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Le => "<=",
            };
            format!("{}.length {op_str} {value}", format_path(path))
        }
        InvariantKind::OutputEqualsInput {
            output_path,
            param_index,
            input_path,
        } => {
            let out = if output_path.is_empty() {
                "output".to_string()
            } else {
                format!("output.{}", output_path.join("."))
            };
            let inp = if input_path.is_empty() {
                format!("input[{param_index}]")
            } else {
                format!("input[{param_index}].{}", input_path.join("."))
            };
            format!("{out} == {inp}")
        }
        InvariantKind::AlwaysTrue { path } => {
            format!("{} is always true", format_path(path))
        }
        InvariantKind::AlwaysFalse { path } => {
            format!("{} is always false", format_path(path))
        }
    }
}

/// Convert raw exploration results into ExecutionRecords.
pub fn records_from_raw_results(
    function_id: &str,
    raw_results: &[(
        Vec<serde_json::Value>,
        Vec<crate::protocol::MockConfig>,
        crate::protocol::ExecuteResult,
    )],
) -> Vec<ExecutionRecord> {
    use std::hash::{Hash, Hasher};
    raw_results
        .iter()
        .map(|(inputs, _mocks, result)| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            let input_str = serde_json::to_string(inputs).unwrap_or_default();
            input_str.hash(&mut hasher);
            let input_hash = hasher.finish();

            ExecutionRecord {
                function_id: function_id.to_string(),
                input_hash,
                parameters: inputs.clone(),
                branch_path: result.branch_path.clone(),
                scope_events: result.scope_events.clone(),
                lines_executed: result.lines_executed.clone(),
                calls_to_external: result.calls_to_external.clone(),
                path_constraints: result.path_constraints.clone(),
                return_value: result.return_value.clone(),
                thrown_error: result.thrown_error.clone(),
                side_effects: result.side_effects.clone(),
                wall_time_ms: result.performance.wall_time_ms,
                cpu_time_us: result.performance.cpu_time_us,
                heap_used_bytes: result.performance.heap_used_bytes,
                heap_allocated_bytes: result.performance.heap_allocated_bytes,
                timestamp: String::new(),
                engine_version: String::new(),
            }
        })
        .collect()
}

/// Wire schema supported by path predicates.
pub const PATH_PREDICATE_SCHEMA_VERSION: u32 = 1;
/// Scalar comparison semantics supported by this schema.
pub const PATH_PREDICATE_SEMANTICS_VERSION: &str = "json-scalars-v1";

/// A path-scoped, context-bound input predicate. Evidence and lifecycle do not
/// contribute to its identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPredicateRecord {
    pub schema_version: u32,
    pub predicate_id: String,
    pub target: PathPredicateTarget,
    pub scope: PathPredicateScope,
    pub expression: PathExpression,
    pub semantics_version: String,
    pub lifecycle: PredicateLifecycle,
    #[serde(default)]
    pub evidence: PredicateEvidence,
}

/// The function and source version to which a predicate belongs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPredicateTarget {
    pub qualified_function: String,
    pub frontend: String,
    pub source_fingerprint: String,
}

/// The exact ordered branch sequence and caller-supplied execution context.
/// Each pair projects a `BranchDecision` to `(branch_id, taken)`, retaining
/// repeated loop decisions while omitting source lines and constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPredicateScope {
    pub path_prefix: Vec<(u32, bool)>,
    pub observation_point: ObservationPoint,
    pub context_fingerprint: String,
}

/// The point at which an input predicate is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationPoint {
    Entry,
}

/// A comparison between two paths into function inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathExpression {
    Compare {
        op: PathCompareOp,
        left: InputPath,
        right: InputPath,
    },
}

/// An exact numeric comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathCompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A parameter and zero or more tagged field or array segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPath {
    pub kind: InputPathKind,
    pub parameter: u32,
    pub path: Vec<PathSegment>,
}

/// The sole supported operand source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPathKind {
    InputPath,
}

/// A typed segment in an input path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathSegment {
    Field { value: String },
    Index { value: u64 },
}

/// The current evidence lifecycle; evaluation never changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredicateLifecycle {
    Candidate,
    Frozen,
    Refuted,
    Stale,
}

/// Read-only evidence metadata. `eligible` must equal `holds + violated`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredicateEvidence {
    pub eligible: u32,
    pub holds: u32,
    pub violated: u32,
    pub not_applicable: u32,
    pub supporting_witnesses: Vec<String>,
    pub refuting_witnesses: Vec<String>,
}

/// Maximum retained witness references in each evidence category.
pub const MAX_PATH_PREDICATE_WITNESSES: usize = 16;

/// One already-parsed execution observation for the input-only evaluator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathObservation {
    pub inputs: Vec<serde_json::Value>,
    pub branch_path: Vec<(u32, bool)>,
    pub observation_point: ObservationPoint,
    pub context_fingerprint: Option<String>,
    pub outcome: PathOutcome,
}

/// Outcome is retained even though the return value is not compared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathOutcome {
    Return { value: serde_json::Value },
    Thrown,
    Unavailable,
}

/// Exact predicate evaluation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PredicateEvaluation {
    Holds,
    Violated,
    NotApplicable { reason: NotApplicableReason },
}

/// Why an observation cannot support or refute a predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotApplicableReason {
    MissingValue,
    ExplicitNull,
    WrongType,
    ThrownOutcome,
    ScopeMismatch,
    UnsupportedOperation,
    UnsupportedSchema,
    ContextUnavailable,
}

/// Failure to deserialize or validate a predicate record.
#[derive(Debug, thiserror::Error)]
pub enum PredicateParseError {
    #[error("invalid path predicate JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid path predicate: {0}")]
    Validation(#[from] PredicateValidationError),
}

/// A structural or identity error in a predicate record.
#[derive(Debug, thiserror::Error)]
pub enum PredicateValidationError {
    #[error("unsupported schema or scalar semantics version")]
    UnsupportedVersion,
    #[error("required predicate field is empty")]
    EmptyField,
    #[error("evidence counts are inconsistent")]
    InvalidEvidence,
    #[error("predicate ID does not match canonical content")]
    InvalidId,
    #[error("cannot serialize predicate identity: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// A rejected evidence update leaves the record unchanged.
#[derive(Debug, thiserror::Error)]
pub enum PredicateEvidenceError {
    #[error("invalid path predicate: {0}")]
    InvalidRecord(#[from] PredicateValidationError),
    #[error("witness reference must not be empty")]
    EmptyWitness,
    #[error("evidence counter overflow")]
    Overflow,
}

/// Parse a complete version-1 record, rejecting malformed tags and identity.
pub fn parse_path_predicate(raw: &str) -> Result<PathPredicateRecord, PredicateParseError> {
    let record: PathPredicateRecord = serde_json::from_str(raw)?;
    validate_path_predicate(&record)?;
    Ok(record)
}

/// Validate a constructed or deserialized record before evaluation.
pub fn validate_path_predicate(
    predicate: &PathPredicateRecord,
) -> Result<(), PredicateValidationError> {
    if predicate.schema_version != PATH_PREDICATE_SCHEMA_VERSION
        || predicate.semantics_version != PATH_PREDICATE_SEMANTICS_VERSION
    {
        return Err(PredicateValidationError::UnsupportedVersion);
    }
    let target = &predicate.target;
    if target.qualified_function.is_empty()
        || target.frontend.is_empty()
        || target.source_fingerprint.is_empty()
        || predicate.scope.context_fingerprint.is_empty()
    {
        return Err(PredicateValidationError::EmptyField);
    }
    let evidence = &predicate.evidence;
    if evidence.holds.checked_add(evidence.violated) != Some(evidence.eligible) {
        return Err(PredicateValidationError::InvalidEvidence);
    }
    if !valid_witnesses(&evidence.supporting_witnesses, evidence.holds)
        || !valid_witnesses(&evidence.refuting_witnesses, evidence.violated)
    {
        return Err(PredicateValidationError::InvalidEvidence);
    }
    if predicate.predicate_id != canonical_path_predicate_id(predicate)? {
        return Err(PredicateValidationError::InvalidId);
    }
    Ok(())
}

fn valid_witnesses(witnesses: &[String], observations: u32) -> bool {
    witnesses.len() <= MAX_PATH_PREDICATE_WITNESSES
        && witnesses.len() <= observations as usize
        && witnesses.iter().all(|witness| !witness.is_empty())
        && witnesses
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == witnesses.len()
}

/// Evaluate one observation and atomically update its evidence counters.
/// References are opaque, deduplicated, and retained in first-seen order.
pub fn record_path_predicate_observation(
    record: &mut PathPredicateRecord,
    observation: &PathObservation,
    witness: Option<&str>,
) -> Result<PredicateEvaluation, PredicateEvidenceError> {
    validate_path_predicate(record)?;
    if witness == Some("") {
        return Err(PredicateEvidenceError::EmptyWitness);
    }
    let result = evaluate_path_predicate(record, observation);
    let mut evidence = record.evidence.clone();
    match result {
        PredicateEvaluation::Holds => {
            evidence.holds = evidence
                .holds
                .checked_add(1)
                .ok_or(PredicateEvidenceError::Overflow)?;
            evidence.eligible = evidence
                .eligible
                .checked_add(1)
                .ok_or(PredicateEvidenceError::Overflow)?;
            if let Some(reference) = witness {
                retain_witness(&mut evidence.supporting_witnesses, reference);
            }
        }
        PredicateEvaluation::Violated => {
            evidence.violated = evidence
                .violated
                .checked_add(1)
                .ok_or(PredicateEvidenceError::Overflow)?;
            evidence.eligible = evidence
                .eligible
                .checked_add(1)
                .ok_or(PredicateEvidenceError::Overflow)?;
            if let Some(reference) = witness {
                retain_witness(&mut evidence.refuting_witnesses, reference);
            }
        }
        PredicateEvaluation::NotApplicable { .. } => {
            evidence.not_applicable = evidence
                .not_applicable
                .checked_add(1)
                .ok_or(PredicateEvidenceError::Overflow)?;
        }
    }
    record.evidence = evidence;
    Ok(result)
}

fn retain_witness(witnesses: &mut Vec<String>, reference: &str) {
    if witnesses.len() < MAX_PATH_PREDICATE_WITNESSES
        && !witnesses.iter().any(|existing| existing == reference)
    {
        witnesses.push(reference.to_owned());
    }
}

/// SHA-256 of compact JSON after recursively sorting object keys and removing
/// identity-excluded fields. Arrays, including branch decisions, retain order.
pub fn canonical_path_predicate_id(
    predicate: &PathPredicateRecord,
) -> Result<String, PredicateValidationError> {
    let mut value = serde_json::to_value(predicate)?;
    if let serde_json::Value::Object(fields) = &mut value {
        fields.remove("predicate_id");
        fields.remove("evidence");
        fields.remove("lifecycle");
    }
    let canonical = sort_json_objects(value);
    let bytes = serde_json::to_vec(&canonical)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn sort_json_objects(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(fields) => {
            let sorted: std::collections::BTreeMap<_, _> = fields
                .into_iter()
                .map(|(key, value)| (key, sort_json_objects(value)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_objects).collect())
        }
        other => other,
    }
}

/// Evaluate a scalar input predicate with strict schema, context, path, and
/// outcome precedence. Lexical integer and float JSON categories are distinct;
/// mixed-category operands are inapplicable under `json-scalars-v1`. The caller
/// supplies the context binding; target and lifecycle are record metadata.
/// No evidence or lifecycle fields are mutated.
pub fn evaluate_path_predicate(
    predicate: &PathPredicateRecord,
    observation: &PathObservation,
) -> PredicateEvaluation {
    use NotApplicableReason as Reason;
    let inapplicable = |reason| PredicateEvaluation::NotApplicable { reason };
    if validate_path_predicate(predicate).is_err() {
        return inapplicable(Reason::UnsupportedSchema);
    }
    if predicate.scope.observation_point != observation.observation_point {
        return inapplicable(Reason::ScopeMismatch);
    }
    if observation.context_fingerprint.as_deref()
        != Some(predicate.scope.context_fingerprint.as_str())
    {
        return inapplicable(Reason::ContextUnavailable);
    }
    if predicate.scope.path_prefix != observation.branch_path {
        return inapplicable(Reason::ScopeMismatch);
    }
    match observation.outcome {
        PathOutcome::Thrown => return inapplicable(Reason::ThrownOutcome),
        PathOutcome::Unavailable => return inapplicable(Reason::MissingValue),
        PathOutcome::Return { .. } => {}
    }
    let PathExpression::Compare { op, left, right } = &predicate.expression;
    let left = match resolve_input_path(&observation.inputs, left).and_then(scalar_number) {
        Ok(value) => value,
        Err(reason) => return inapplicable(reason),
    };
    let right = match resolve_input_path(&observation.inputs, right).and_then(scalar_number) {
        Ok(value) => value,
        Err(reason) => return inapplicable(reason),
    };
    let ordering = match (left, right) {
        (ScalarNumber::Integer(a), ScalarNumber::Integer(b)) => a.partial_cmp(&b),
        (ScalarNumber::Float(a), ScalarNumber::Float(b)) => a.partial_cmp(&b),
        _ => return inapplicable(Reason::UnsupportedOperation),
    };
    let Some(ordering) = ordering else {
        return inapplicable(Reason::UnsupportedSchema);
    };
    let holds = match op {
        PathCompareOp::Eq => ordering.is_eq(),
        PathCompareOp::Ne => !ordering.is_eq(),
        PathCompareOp::Lt => ordering.is_lt(),
        PathCompareOp::Le => !ordering.is_gt(),
        PathCompareOp::Gt => ordering.is_gt(),
        PathCompareOp::Ge => !ordering.is_lt(),
    };
    if holds {
        PredicateEvaluation::Holds
    } else {
        PredicateEvaluation::Violated
    }
}

fn resolve_input_path<'a>(
    inputs: &'a [serde_json::Value],
    path: &InputPath,
) -> Result<&'a serde_json::Value, NotApplicableReason> {
    use NotApplicableReason as Reason;
    let mut value = inputs
        .get(path.parameter as usize)
        .ok_or(Reason::MissingValue)?;
    for segment in &path.path {
        value = match (segment, value) {
            (PathSegment::Field { value: key }, serde_json::Value::Object(fields)) => {
                fields.get(key).ok_or(Reason::MissingValue)?
            }
            (PathSegment::Index { value: index }, serde_json::Value::Array(items)) => {
                let index = usize::try_from(*index).map_err(|_| Reason::MissingValue)?;
                items.get(index).ok_or(Reason::MissingValue)?
            }
            (_, serde_json::Value::Null) => return Err(Reason::ExplicitNull),
            _ => return Err(Reason::WrongType),
        };
    }
    Ok(value)
}

enum ScalarNumber {
    Integer(i64),
    Float(f64),
}

fn scalar_number(value: &serde_json::Value) -> Result<ScalarNumber, NotApplicableReason> {
    use NotApplicableReason as Reason;
    match value {
        serde_json::Value::Null => Err(Reason::ExplicitNull),
        serde_json::Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Ok(ScalarNumber::Integer(integer))
            } else if number.as_u64().is_some() {
                Err(Reason::UnsupportedSchema)
            } else if let Some(float) = number.as_f64().filter(|value| value.is_finite()) {
                Ok(ScalarNumber::Float(float))
            } else {
                Err(Reason::UnsupportedSchema)
            }
        }
        _ => Err(Reason::WrongType),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::execution_record::ExecutionRecord;

    fn scalar_predicate(op: PathCompareOp) -> PathPredicateRecord {
        let mut record = PathPredicateRecord {
            schema_version: PATH_PREDICATE_SCHEMA_VERSION,
            predicate_id: String::new(),
            target: PathPredicateTarget {
                qualified_function: "example.compare".into(),
                frontend: "synthetic".into(),
                source_fingerprint: "source-v1".into(),
            },
            scope: PathPredicateScope {
                path_prefix: vec![(0, true), (0, false)],
                observation_point: ObservationPoint::Entry,
                context_fingerprint: "context-v1".into(),
            },
            expression: PathExpression::Compare {
                op,
                left: InputPath {
                    kind: InputPathKind::InputPath,
                    parameter: 0,
                    path: vec![PathSegment::Field { value: "a".into() }],
                },
                right: InputPath {
                    kind: InputPathKind::InputPath,
                    parameter: 0,
                    path: vec![PathSegment::Field { value: "b".into() }],
                },
            },
            semantics_version: PATH_PREDICATE_SEMANTICS_VERSION.into(),
            lifecycle: PredicateLifecycle::Candidate,
            evidence: PredicateEvidence::default(),
        };
        record.predicate_id = canonical_path_predicate_id(&record).expect("canonical ID");
        record
    }

    fn scalar_observation(a: serde_json::Value, b: serde_json::Value) -> PathObservation {
        PathObservation {
            inputs: vec![json!({ "a": a, "b": b })],
            branch_path: vec![(0, true), (0, false)],
            observation_point: ObservationPoint::Entry,
            context_fingerprint: Some("context-v1".into()),
            outcome: PathOutcome::Return { value: json!(null) },
        }
    }

    #[test]
    fn record_path_predicate_observation_counts_each_result() {
        let mut record = scalar_predicate(PathCompareOp::Eq);
        let holding = scalar_observation(json!(4), json!(4));
        let violating = scalar_observation(json!(4), json!(5));
        let mut inapplicable = holding.clone();
        inapplicable.context_fingerprint = None;

        assert_eq!(
            record_path_predicate_observation(&mut record, &holding, Some("witness-1"))
                .expect("holding observation"),
            PredicateEvaluation::Holds
        );
        assert_eq!(
            record_path_predicate_observation(&mut record, &violating, Some("witness-2"))
                .expect("violating observation"),
            PredicateEvaluation::Violated
        );
        assert!(matches!(
            record_path_predicate_observation(&mut record, &inapplicable, Some("witness-3")),
            Ok(PredicateEvaluation::NotApplicable { .. })
        ));
        assert_eq!(record.evidence.eligible, 2);
        assert_eq!(record.evidence.holds, 1);
        assert_eq!(record.evidence.violated, 1);
        assert_eq!(record.evidence.not_applicable, 1);
        assert_eq!(record.evidence.supporting_witnesses, ["witness-1"]);
        assert_eq!(record.evidence.refuting_witnesses, ["witness-2"]);
    }

    #[test]
    fn record_path_predicate_observation_preserves_state_on_error() {
        let holding = scalar_observation(json!(1), json!(1));
        let mut record = scalar_predicate(PathCompareOp::Eq);
        let original = record.clone();
        assert!(matches!(
            record_path_predicate_observation(&mut record, &holding, Some("")),
            Err(PredicateEvidenceError::EmptyWitness)
        ));
        assert_eq!(record, original);

        record.evidence.eligible = u32::MAX;
        record.evidence.holds = u32::MAX;
        let original = record.clone();
        assert!(matches!(
            record_path_predicate_observation(&mut record, &holding, None),
            Err(PredicateEvidenceError::Overflow)
        ));
        assert_eq!(record, original);

        record.evidence.eligible = 0;
        let original = record.clone();
        assert!(matches!(
            record_path_predicate_observation(&mut record, &holding, None),
            Err(PredicateEvidenceError::InvalidRecord(_))
        ));
        assert_eq!(record, original);
    }

    #[test]
    fn record_path_predicate_observation_bounds_witnesses_and_preserves_lifecycle() {
        let holding = scalar_observation(json!(1), json!(1));
        let mut record = scalar_predicate(PathCompareOp::Eq);
        record.lifecycle = PredicateLifecycle::Frozen;
        for index in 0..(MAX_PATH_PREDICATE_WITNESSES + 2) {
            let reference = format!("witness-{index}");
            record_path_predicate_observation(&mut record, &holding, Some(&reference))
                .expect("observation");
        }
        record_path_predicate_observation(&mut record, &holding, Some("witness-0"))
            .expect("repeat");
        assert_eq!(record.evidence.eligible, 19);
        assert_eq!(
            record.evidence.supporting_witnesses.len(),
            MAX_PATH_PREDICATE_WITNESSES
        );
        assert_eq!(record.evidence.supporting_witnesses[0], "witness-0");
        assert_eq!(record.evidence.supporting_witnesses[15], "witness-15");
        assert_eq!(record.lifecycle, PredicateLifecycle::Frozen);
    }

    #[test]
    fn record_path_predicate_observation_tracks_inapplicable_without_witness() {
        let mut record = scalar_predicate(PathCompareOp::Eq);
        record.lifecycle = PredicateLifecycle::Stale;
        let mut observation = scalar_observation(json!(1), json!(1));
        observation.context_fingerprint = None;
        record_path_predicate_observation(&mut record, &observation, Some("unused"))
            .expect("inapplicable");
        assert_eq!(record.evidence.not_applicable, 1);
        assert_eq!(record.evidence.eligible, 0);
        assert!(record.evidence.supporting_witnesses.is_empty());
        assert_eq!(record.lifecycle, PredicateLifecycle::Stale);
    }

    #[test]
    fn record_path_predicate_observation_accepts_every_lifecycle() {
        let observation = scalar_observation(json!(1), json!(1));
        for lifecycle in [
            PredicateLifecycle::Candidate,
            PredicateLifecycle::Frozen,
            PredicateLifecycle::Refuted,
            PredicateLifecycle::Stale,
        ] {
            let mut record = scalar_predicate(PathCompareOp::Eq);
            record.lifecycle = lifecycle;
            assert_eq!(
                record_path_predicate_observation(&mut record, &observation, None)
                    .expect("observation"),
                PredicateEvaluation::Holds
            );
            assert_eq!(record.lifecycle, lifecycle);
        }
    }

    #[test]
    fn record_path_predicate_observation_matches_count_model() {
        use proptest::prelude::*;
        use proptest::test_runner::{Config, RngSeed, TestRunner};

        let mut runner = TestRunner::new(Config {
            cases: 256,
            rng_seed: RngSeed::Fixed(0x5a77_2027),
            ..Config::default()
        });
        runner
            .run(&proptest::collection::vec(0u8..3, 0..100), |sequence| {
                let mut record = scalar_predicate(PathCompareOp::Eq);
                let mut expected = [0u32; 3];
                for status in sequence {
                    let mut observation = match status {
                        0 => scalar_observation(json!(1), json!(1)),
                        _ => scalar_observation(json!(1), json!(2)),
                    };
                    if status == 2 {
                        observation.context_fingerprint = None;
                    }
                    record_path_predicate_observation(&mut record, &observation, None)
                        .expect("bounded sequence");
                    expected[usize::from(status)] += 1;
                }
                prop_assert_eq!(record.evidence.holds, expected[0]);
                prop_assert_eq!(record.evidence.violated, expected[1]);
                prop_assert_eq!(record.evidence.not_applicable, expected[2]);
                prop_assert_eq!(record.evidence.eligible, expected[0] + expected[1]);
                Ok(())
            })
            .expect("property holds");
    }

    #[test]
    fn path_predicate_reasons_and_precedence_are_exact() {
        use NotApplicableReason as Reason;
        let predicate = scalar_predicate(PathCompareOp::Eq);
        let mut observation = scalar_observation(json!(4), json!(4));
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::Holds
        );
        observation.inputs[0]["b"] = json!(5);
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::Violated
        );

        observation.inputs[0]
            .as_object_mut()
            .expect("object")
            .remove("a");
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::MissingValue
            }
        );
        observation.inputs[0]["a"] = json!(null);
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ExplicitNull
            }
        );
        observation.inputs[0]["a"] = json!("4");
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::WrongType
            }
        );
        observation.inputs[0]["a"] = json!(4.0);
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::UnsupportedOperation
            }
        );

        observation.outcome = PathOutcome::Thrown;
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ThrownOutcome
            }
        );
        observation.branch_path.push((1, true));
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ScopeMismatch
            }
        );
        observation.context_fingerprint = None;
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ContextUnavailable
            }
        );
        let mut invalid = predicate.clone();
        invalid.schema_version = 2;
        assert_eq!(
            evaluate_path_predicate(&invalid, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::UnsupportedSchema
            }
        );
    }

    #[test]
    fn path_predicate_identity_is_canonical_and_excludes_metadata() {
        let record = scalar_predicate(PathCompareOp::Eq);
        let json = serde_json::to_string(&record).expect("serialize");
        assert_eq!(parse_path_predicate(&json).expect("parse"), record);
        let mut identity = serde_json::to_value(&record).expect("value");
        let fields = identity.as_object_mut().expect("object");
        fields.remove("predicate_id");
        fields.remove("lifecycle");
        fields.remove("evidence");
        let bytes = serde_json::to_string(&sort_json_objects(identity)).expect("canonical JSON");
        assert_eq!(
            bytes,
            r#"{"expression":{"kind":"compare","left":{"kind":"input_path","parameter":0,"path":[{"kind":"field","value":"a"}]},"op":"eq","right":{"kind":"input_path","parameter":0,"path":[{"kind":"field","value":"b"}]}},"schema_version":1,"scope":{"context_fingerprint":"context-v1","observation_point":"entry","path_prefix":[[0,true],[0,false]]},"semantics_version":"json-scalars-v1","target":{"frontend":"synthetic","qualified_function":"example.compare","source_fingerprint":"source-v1"}}"#
        );
        assert_eq!(
            record.predicate_id,
            "9314ff78ee46e35614bd04db9fe9114813076239c77473d8b5ac16bea6efb99e"
        );
        assert_eq!(
            record.predicate_id,
            hex::encode(Sha256::digest(bytes.as_bytes()))
        );
        let mut changed = record.clone();
        changed.lifecycle = PredicateLifecycle::Frozen;
        changed.evidence.not_applicable = 5;
        assert_eq!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
        changed.target.frontend.push('x');
        assert_ne!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
        changed = record.clone();
        changed.scope.path_prefix.reverse();
        assert_ne!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
        changed = record.clone();
        changed.scope.context_fingerprint.push('x');
        assert_ne!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
        changed = record.clone();
        changed.expression = scalar_predicate(PathCompareOp::Ne).expression;
        assert_ne!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
        changed = record.clone();
        changed.semantics_version.push('x');
        assert_ne!(
            canonical_path_predicate_id(&changed).expect("ID"),
            record.predicate_id
        );
    }

    #[test]
    fn path_predicate_parsing_rejects_malformed_records() {
        let record = scalar_predicate(PathCompareOp::Eq);
        let mut value = serde_json::to_value(record).expect("value");
        value["schema_version"] = json!(2);
        assert!(parse_path_predicate(&value.to_string()).is_err());
        value["schema_version"] = json!(1);
        value["expression"]["op"] = json!("bogus");
        assert!(parse_path_predicate(&value.to_string()).is_err());
        value["expression"]["op"] = json!("eq");
        value["evidence"]["eligible"] = json!(1);
        assert!(parse_path_predicate(&value.to_string()).is_err());
        value["evidence"] = json!(null);
        assert!(parse_path_predicate(&value.to_string()).is_err());

        let mut overflow = scalar_predicate(PathCompareOp::Eq);
        overflow.evidence.holds = u32::MAX;
        overflow.evidence.violated = 1;
        overflow.evidence.eligible = u32::MAX;
        assert!(matches!(
            validate_path_predicate(&overflow),
            Err(PredicateValidationError::InvalidEvidence)
        ));
    }

    #[test]
    fn path_predicate_resolves_typed_nested_paths_and_number_boundaries() {
        use NotApplicableReason as Reason;
        let mut predicate = scalar_predicate(PathCompareOp::Lt);
        let PathExpression::Compare { left, right, .. } = &mut predicate.expression;
        left.path = vec![
            PathSegment::Field {
                value: "values".into(),
            },
            PathSegment::Index { value: 0 },
        ];
        right.path = vec![
            PathSegment::Field {
                value: "values".into(),
            },
            PathSegment::Index { value: 1 },
        ];
        predicate.predicate_id = canonical_path_predicate_id(&predicate).expect("ID");
        let mut observation = scalar_observation(json!(0), json!(0));
        observation.inputs = vec![json!({"values": [1.25, 2.5]})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::Holds
        );
        observation.inputs = vec![json!({"values": [i64::MAX - 1, i64::MAX]})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::Holds
        );
        observation.inputs = vec![json!({"values": [null, 2]})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ExplicitNull
            }
        );
        observation.inputs = vec![json!({"values": [1]})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::MissingValue
            }
        );
        observation.inputs = vec![json!({"values": {"0": 1, "1": 2}})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::WrongType
            }
        );
        observation.inputs = vec![json!({"values": [1, 2]})];
        let PathExpression::Compare { left, .. } = &mut predicate.expression;
        left.path = vec![
            PathSegment::Field {
                value: "values".into(),
            },
            PathSegment::Field { value: "0".into() },
        ];
        predicate.predicate_id = canonical_path_predicate_id(&predicate).expect("ID");
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::WrongType
            }
        );
        let PathExpression::Compare { left, .. } = &mut predicate.expression;
        left.path = vec![
            PathSegment::Field {
                value: "values".into(),
            },
            PathSegment::Index { value: 0 },
        ];
        predicate.predicate_id = canonical_path_predicate_id(&predicate).expect("ID");
        observation.inputs = vec![json!({"values": [u64::MAX, 2]})];
        observation.context_fingerprint = None;
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::ContextUnavailable
            }
        );
        observation.context_fingerprint = Some("context-v1".into());
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::NotApplicable {
                reason: Reason::UnsupportedSchema
            }
        );
        observation.inputs = vec![json!({"values": [1, 2], "unrelated": u64::MAX})];
        assert_eq!(
            evaluate_path_predicate(&predicate, &observation),
            PredicateEvaluation::Holds
        );
    }

    #[test]
    fn path_predicate_matches_reference_on_one_thousand_deterministic_cases() {
        use proptest::prelude::*;
        use proptest::test_runner::{Config, RngSeed, TestRunner};
        let config = Config {
            cases: 1_000,
            rng_seed: RngSeed::Fixed(0x5a77_2026),
            ..Config::default()
        };
        let mut runner = TestRunner::new(config);
        let strategy = (
            -1000i64..=1000,
            -1000i64..=1000,
            0u8..6,
            0u8..3,
            any::<bool>(),
        );
        runner
            .run(&strategy, |(a, b, index, category, nested)| {
                let op = match index {
                    0 => PathCompareOp::Eq,
                    1 => PathCompareOp::Ne,
                    2 => PathCompareOp::Lt,
                    3 => PathCompareOp::Le,
                    4 => PathCompareOp::Gt,
                    _ => PathCompareOp::Ge,
                };
                let expected = match index {
                    0 => a == b,
                    1 => a != b,
                    2 => a < b,
                    3 => a <= b,
                    4 => a > b,
                    _ => a >= b,
                };
                let predicate = scalar_predicate(op);
                let mut predicate = predicate;
                let (left, right) = match category {
                    0 => (json!(a), json!(b)),
                    1 => (json!(a as f64 + 0.5), json!(b as f64 + 0.5)),
                    _ => (json!(a), json!(b as f64 + 0.5)),
                };
                let mut observation = scalar_observation(left.clone(), right.clone());
                if nested {
                    let PathExpression::Compare {
                        left: lhs,
                        right: rhs,
                        ..
                    } = &mut predicate.expression;
                    lhs.path = vec![
                        PathSegment::Field {
                            value: "values".into(),
                        },
                        PathSegment::Index { value: 0 },
                    ];
                    rhs.path = vec![
                        PathSegment::Field {
                            value: "values".into(),
                        },
                        PathSegment::Index { value: 1 },
                    ];
                    predicate.predicate_id = canonical_path_predicate_id(&predicate).expect("ID");
                    observation.inputs = vec![json!({"values": [left, right]})];
                }
                let actual = evaluate_path_predicate(&predicate, &observation);
                if category == 2 {
                    prop_assert_eq!(
                        actual,
                        PredicateEvaluation::NotApplicable {
                            reason: NotApplicableReason::UnsupportedOperation
                        }
                    );
                    return Ok(());
                }
                prop_assert_eq!(
                    actual,
                    if expected {
                        PredicateEvaluation::Holds
                    } else {
                        PredicateEvaluation::Violated
                    }
                );
                Ok(())
            })
            .expect("reference evaluator property");
    }

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
            scope_events: vec![],
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
        assert!(
            has_not_null,
            "should detect name != null, got: {invariants:?}"
        );
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
        assert!(
            has_non_empty,
            "should detect s is non-empty, got: {invariants:?}"
        );
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
        assert!(
            !has_non_empty,
            "should not detect non-empty when empty string present"
        );
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
        assert!(
            has_relation,
            "should detect output.len == input.len, got: {invariants:?}"
        );
    }

    #[test]
    fn does_not_detect_false_output_input_relation() {
        let specimens = vec![
            make_record(vec![json!({"len": 3})], Some(json!({"len": 3}))),
            make_record(vec![json!({"len": 5})], Some(json!({"len": 7}))), // different!
        ];

        let invariants = detect_invariants(&specimens, InvariantTarget::Output);

        let has_relation = invariants
            .iter()
            .any(|inv| matches!(&inv.kind, InvariantKind::OutputEqualsInput { .. }));
        assert!(
            !has_relation,
            "should not detect output.len == input.len when they differ"
        );
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
        assert!(
            has_constant,
            "should detect output == 42, got: {invariants:?}"
        );
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
        assert!(
            has_always_true,
            "should detect active is always true, got: {invariants:?}"
        );
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
        assert!(
            has_is_null,
            "should detect opt is always null, got: {invariants:?}"
        );
    }

    #[test]
    fn combined_invariants_x_positive_and_output_len_equals_input_len() {
        // The acceptance criteria scenario: all inputs have x > 0 and output.len == input.len
        let specimens = vec![
            make_record(vec![json!({"x": 1, "len": 3})], Some(json!({"len": 3}))),
            make_record(vec![json!({"x": 5, "len": 7})], Some(json!({"len": 7}))),
            make_record(vec![json!({"x": 10, "len": 1})], Some(json!({"len": 1}))),
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
        assert!(
            has_x_gt_0,
            "should detect x > 0 in inputs, got: {input_invs:?}"
        );

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
        assert!(
            has_nested,
            "should detect order.total > 0, got: {invariants:?}"
        );
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
        assert!(
            has_gt_0,
            "should detect param > 0 for scalar input, got: {invariants:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Tests for ClassifiedInvariant, detect_classified_invariants, format_invariant_label
    // -----------------------------------------------------------------------

    #[test]
    fn classified_invariants_all_observed() {
        let specimens = vec![
            make_record(vec![json!({"x": 5})], Some(json!(10))),
            make_record(vec![json!({"x": 10})], Some(json!(20))),
        ];

        let classified = detect_classified_invariants(&specimens, InvariantTarget::Input);
        assert!(!classified.is_empty());

        for ci in &classified {
            assert_eq!(ci.target, InvariantTarget::Input);
            assert!((ci.confidence - 1.0).abs() < f64::EPSILON);
            assert_eq!(ci.satisfied_count, 2);
            assert_eq!(ci.total_count, 2);
            assert!(!ci.label.is_empty());
        }
    }

    #[test]
    fn classified_invariants_empty_returns_empty() {
        let classified = detect_classified_invariants(&[], InvariantTarget::Input);
        assert!(classified.is_empty());
    }

    #[test]
    fn classified_invariants_include_output() {
        let specimens = vec![
            make_record(vec![json!(1)], Some(json!(42))),
            make_record(vec![json!(2)], Some(json!(42))),
        ];

        let classified = detect_classified_invariants(&specimens, InvariantTarget::Output);
        assert!(!classified.is_empty());
        for ci in &classified {
            assert_eq!(ci.target, InvariantTarget::Output);
        }
    }

    #[test]
    fn classified_invariant_has_label() {
        let specimens = vec![
            make_record(vec![json!({"x": 5})], Some(json!(10))),
            make_record(vec![json!({"x": 10})], Some(json!(20))),
        ];

        let classified = detect_classified_invariants(&specimens, InvariantTarget::Input);
        let x_inv = classified.iter().find(|ci| ci.label.contains("input.x"));
        assert!(
            x_inv.is_some(),
            "should have label containing 'input.x', got: {:?}",
            classified.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn classified_invariant_serialization_round_trips() {
        let ci = ClassifiedInvariant {
            invariant: Invariant {
                description: "x > 0".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NumericComparison {
                    path: vec!["x".to_string()],
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Input,
            label: "input.x > 0".to_string(),
            confidence: 1.0,
            satisfied_count: 10,
            total_count: 10,
        };

        let json = serde_json::to_string(&ci).expect("serialize");
        let deserialized: ClassifiedInvariant = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ci, deserialized);
    }

    #[test]
    fn format_invariant_label_numeric_comparison() {
        let inv = Invariant {
            description: "x > 0".to_string(),
            target: InvariantTarget::Input,
            kind: InvariantKind::NumericComparison {
                path: vec!["x".to_string()],
                op: ComparisonOp::Gt,
                value: 0.0,
            },
        };
        let label = format_invariant_label(&inv, InvariantTarget::Input);
        assert_eq!(label, "input.x > 0");
    }

    #[test]
    fn format_invariant_label_output_equals_input() {
        let inv = Invariant {
            description: "output.len == input[0].len".to_string(),
            target: InvariantTarget::Output,
            kind: InvariantKind::OutputEqualsInput {
                output_path: vec!["len".to_string()],
                param_index: 0,
                input_path: vec!["len".to_string()],
            },
        };
        let label = format_invariant_label(&inv, InvariantTarget::Output);
        assert_eq!(label, "output.len == input[0].len");
    }

    #[test]
    fn format_invariant_label_root_path() {
        let inv = Invariant {
            description: "output > 0".to_string(),
            target: InvariantTarget::Output,
            kind: InvariantKind::NumericComparison {
                path: vec![],
                op: ComparisonOp::Gt,
                value: 0.0,
            },
        };
        let label = format_invariant_label(&inv, InvariantTarget::Output);
        assert_eq!(label, "output > 0");
    }

    #[test]
    fn records_from_raw_results_converts_correctly() {
        use crate::protocol::{ExecuteResult, PerformanceMetrics};

        let raw = vec![(
            vec![json!(42)],
            vec![],
            ExecuteResult {
                return_value: Some(json!("positive")),
                thrown_error: None,
                branch_path: vec![],
                lines_executed: vec![1, 2],
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
                performance: PerformanceMetrics {
                    wall_time_ms: 1.0,
                    cpu_time_us: 100,
                    heap_used_bytes: 256,
                    heap_allocated_bytes: 512,
                },
            },
        )];

        let records = records_from_raw_results("test_fn", &raw);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].function_id, "test_fn");
        assert_eq!(records[0].parameters, vec![json!(42)]);
        assert_eq!(records[0].return_value, Some(json!("positive")));
        assert_eq!(records[0].lines_executed, vec![1, 2]);
    }

    #[test]
    fn records_from_raw_results_empty_input() {
        let records = records_from_raw_results("empty_fn", &[]);
        assert!(records.is_empty());
    }
}
