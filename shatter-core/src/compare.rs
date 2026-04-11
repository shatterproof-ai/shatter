//! Cross-language behavioral comparison via input/output spec diff.
//!
//! Unlike [`spec_diff`](crate::spec_diff), which matches equivalence classes by
//! branch path (language-specific), this module compares functions purely by
//! observable behavior: same inputs → same outputs?
//!
//! The main entry point is [`compare_specs`]. Two formatters are provided:
//! [`format_compare_text`] for human-readable markdown and
//! [`format_compare_json`] for machine-readable JSON.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::canonical_json::canonicalize_json;
use crate::spec::FunctionSpec;

// ── Types ────────────────────────────────────────────────────────────────

/// Normalized output for cross-language comparison.
///
/// Error types are flattened to just the message because different languages
/// use different error type names (e.g., TS preserves constructor names like
/// `TypeError`, Go flattens to `runtime_error`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NormalizedOutput {
    /// The function returned a value.
    Returns { value: serde_json::Value },
    /// The function threw/panicked.
    Throws { message: String },
    /// The function returned void/undefined/null.
    Void,
}

impl fmt::Display for NormalizedOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Returns { value } => write!(f, "{}", format_value_short(value)),
            Self::Throws { message } => write!(f, "throws: {message}"),
            Self::Void => write!(f, "void"),
        }
    }
}

/// A behavior where both specs produce the same output for the same input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchingBehavior {
    /// The concrete input arguments.
    pub inputs: Vec<serde_json::Value>,
    /// The shared output.
    pub output: NormalizedOutput,
}

/// A behavior where both specs have the same input but different outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DivergentBehavior {
    /// The concrete input arguments.
    pub inputs: Vec<serde_json::Value>,
    /// Output from spec A.
    pub output_a: NormalizedOutput,
    /// Output from spec B.
    pub output_b: NormalizedOutput,
}

/// A behavior present in only one spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueBehavior {
    /// The concrete input arguments.
    pub inputs: Vec<serde_json::Value>,
    /// The output observed.
    pub output: NormalizedOutput,
}

/// Result of comparing two function specs by input/output behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareResult {
    /// Name of function A.
    pub function_a: String,
    /// Name of function B.
    pub function_b: String,
    /// Behaviors where both produce the same output.
    pub matching: Vec<MatchingBehavior>,
    /// Behaviors where both have the same input but different output.
    pub divergent: Vec<DivergentBehavior>,
    /// Behaviors present only in spec A.
    pub only_in_a: Vec<UniqueBehavior>,
    /// Behaviors present only in spec B.
    pub only_in_b: Vec<UniqueBehavior>,
}

impl CompareResult {
    /// Whether any divergent behaviors were found.
    pub fn has_divergences(&self) -> bool {
        !self.divergent.is_empty()
    }

    /// Similarity percentage: matching / (matching + divergent) * 100.
    ///
    /// Returns `None` if there are no shared inputs (matching + divergent == 0).
    pub fn similarity_percent(&self) -> Option<f64> {
        let shared = self.matching.len() + self.divergent.len();
        if shared == 0 {
            return None;
        }
        Some(self.matching.len() as f64 / shared as f64 * 100.0)
    }
}

// ── Normalization ────────────────────────────────────────────────────────

/// Normalize a JSON value for cross-language comparison.
///
/// Sorts object keys recursively and canonicalizes number representations.
/// Returns a new `Value` that compares equal regardless of original key order.
pub fn normalize_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut sorted: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), normalize_value(&map[key]));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_value).collect())
        }
        // Numbers, strings, bools, null pass through — canonical_json handles
        // number formatting when we hash, and serde_json::Value equality for
        // numbers is already well-defined (same numeric value).
        other => other.clone(),
    }
}

/// Produce a canonical string key for an input vector, suitable for HashMap lookup.
fn canonical_inputs_key(inputs: &[serde_json::Value]) -> String {
    let arr = serde_json::Value::Array(inputs.to_vec());
    canonicalize_json(&arr)
}

/// Normalize a `ConcreteExample` into a `NormalizedOutput`.
///
/// Error types are deliberately discarded — only the message is kept — because
/// different languages report different error type names for the same error.
fn normalize_example_output(
    return_value: &Option<serde_json::Value>,
    thrown_error: &Option<crate::execution_record::ErrorInfo>,
) -> NormalizedOutput {
    if let Some(err) = thrown_error {
        return NormalizedOutput::Throws {
            message: err.message.clone(),
        };
    }
    match return_value {
        Some(v) if v.is_null() => NormalizedOutput::Void,
        Some(v) => NormalizedOutput::Returns {
            value: normalize_value(v),
        },
        None => NormalizedOutput::Void,
    }
}

// ── Core comparison ──────────────────────────────────────────────────────

/// Compare two function specs by input/output behavior (ignoring branch paths).
///
/// Extracts all concrete examples from both specs, normalizes values, and
/// performs a set comparison on (canonical_inputs → output) pairs.
pub fn compare_specs(a: &FunctionSpec, b: &FunctionSpec) -> CompareResult {
    let map_a = build_behavior_map(a);
    let map_b = build_behavior_map(b);

    let mut matching = Vec::new();
    let mut divergent = Vec::new();
    let mut only_in_a = Vec::new();

    for (key, (inputs, output_a)) in &map_a {
        if let Some((_, output_b)) = map_b.get(key) {
            if output_a == output_b {
                matching.push(MatchingBehavior {
                    inputs: inputs.clone(),
                    output: output_a.clone(),
                });
            } else {
                divergent.push(DivergentBehavior {
                    inputs: inputs.clone(),
                    output_a: output_a.clone(),
                    output_b: output_b.clone(),
                });
            }
        } else {
            only_in_a.push(UniqueBehavior {
                inputs: inputs.clone(),
                output: output_a.clone(),
            });
        }
    }

    let only_in_b: Vec<UniqueBehavior> = map_b
        .iter()
        .filter(|(key, _)| !map_a.contains_key(key.as_str()))
        .map(|(_, (inputs, output))| UniqueBehavior {
            inputs: inputs.clone(),
            output: output.clone(),
        })
        .collect();

    CompareResult {
        function_a: a.function_name.clone(),
        function_b: b.function_name.clone(),
        matching,
        divergent,
        only_in_a,
        only_in_b,
    }
}

/// Build a map from canonical input key → (original inputs, normalized output).
///
/// If multiple examples have the same canonical inputs, the first one wins.
fn build_behavior_map(
    spec: &FunctionSpec,
) -> HashMap<String, (Vec<serde_json::Value>, NormalizedOutput)> {
    let mut map: HashMap<String, (Vec<serde_json::Value>, NormalizedOutput)> = HashMap::new();
    for class in &spec.classes {
        for example in &class.examples {
            let key = canonical_inputs_key(&example.inputs);
            map.entry(key).or_insert_with(|| {
                let output = normalize_example_output(&example.return_value, &example.thrown_error);
                (example.inputs.clone(), output)
            });
        }
    }
    map
}

// ── Formatting ───────────────────────────────────────────────────────────

/// Truncate a JSON value to a short display string.
fn format_value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    const MAX_LEN: usize = 60;
    if s.len() > MAX_LEN {
        format!("{}...", &s[..MAX_LEN])
    } else {
        s
    }
}

/// Format input arguments for display.
fn format_inputs_short(inputs: &[serde_json::Value]) -> String {
    let parts: Vec<String> = inputs.iter().map(format_value_short).collect();
    format!("({})", parts.join(", "))
}

/// Format the comparison result as human-readable markdown.
pub fn format_compare_text(result: &CompareResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Cross-language comparison: {} vs {}\n\n",
        result.function_a, result.function_b
    ));

    // Summary metric
    match result.similarity_percent() {
        Some(pct) => {
            out.push_str(&format!(
                "**{} of {} shared behaviors match — {:.0}% equivalent**\n\n",
                result.matching.len(),
                result.matching.len() + result.divergent.len(),
                pct,
            ));
        }
        None => {
            out.push_str("**No shared inputs found — cannot determine equivalence.**\n\n");
        }
    }

    // Divergent behaviors (most important)
    if !result.divergent.is_empty() {
        out.push_str(&format!(
            "## Divergent behaviors ({})\n\n",
            result.divergent.len()
        ));
        for d in &result.divergent {
            out.push_str(&format!(
                "- Input {}: **{}** returns `{}`, **{}** returns `{}`\n",
                format_inputs_short(&d.inputs),
                result.function_a,
                d.output_a,
                result.function_b,
                d.output_b,
            ));
        }
        out.push('\n');
    }

    // Matching behaviors
    if !result.matching.is_empty() {
        out.push_str(&format!(
            "## Matching behaviors ({})\n\n",
            result.matching.len()
        ));
        for m in &result.matching {
            out.push_str(&format!(
                "- Input {} → `{}`\n",
                format_inputs_short(&m.inputs),
                m.output,
            ));
        }
        out.push('\n');
    }

    // Unique behaviors
    if !result.only_in_a.is_empty() {
        out.push_str(&format!(
            "## Only in {} ({} behaviors)\n\n",
            result.function_a,
            result.only_in_a.len()
        ));
        for u in &result.only_in_a {
            out.push_str(&format!(
                "- Input {} → `{}`\n",
                format_inputs_short(&u.inputs),
                u.output,
            ));
        }
        out.push('\n');
    }

    if !result.only_in_b.is_empty() {
        out.push_str(&format!(
            "## Only in {} ({} behaviors)\n\n",
            result.function_b,
            result.only_in_b.len()
        ));
        for u in &result.only_in_b {
            out.push_str(&format!(
                "- Input {} → `{}`\n",
                format_inputs_short(&u.inputs),
                u.output,
            ));
        }
        out.push('\n');
    }

    out
}

/// Format the comparison result as JSON.
pub fn format_compare_json(result: &CompareResult) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(result)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equivalence::{BranchPath, BranchStep};
    use crate::execution_record::ErrorInfo;
    use crate::spec::{ConcreteExample, Postcondition, Provenance, SpecClass};
    use serde_json::json;

    /// Build a minimal FunctionSpec for testing.
    fn make_spec(
        name: &str,
        examples: Vec<(Vec<serde_json::Value>, ConcreteExample)>,
    ) -> FunctionSpec {
        let classes: Vec<SpecClass> = examples
            .into_iter()
            .enumerate()
            .map(|(i, (_, example))| SpecClass {
                label: format!("Class {}", i + 1),
                branch_path: BranchPath(vec![BranchStep {
                    branch_id: i as u32,
                    taken: true,
                }]),
                preconditions: Vec::new(),
                postcondition: match (&example.return_value, &example.thrown_error) {
                    (_, Some(err)) => Postcondition::Throws { error: err.clone() },
                    (Some(v), None) if v.is_null() => Postcondition::ReturnsVoid,
                    (Some(v), None) => Postcondition::Returns { value: v.clone() },
                    (None, None) => Postcondition::ReturnsVoid,
                },
                side_effects: Vec::new(),
                examples: vec![example],
                sample_count: 1,
                precondition_provenance: Provenance::Observed,
                postcondition_provenance: Provenance::Observed,
                invariants: Vec::new(),
            })
            .collect();

        FunctionSpec {
            function_name: name.to_string(),
            location: None,
            classes,
            iterations: 10,
            lines_covered: 5,
            total_lines: 10,
            invariants: Vec::new(),
            fingerprint: None,
            nondeterministic_fields: Vec::new(),
        }
    }

    fn example_returns(
        inputs: Vec<serde_json::Value>,
        value: serde_json::Value,
    ) -> (Vec<serde_json::Value>, ConcreteExample) {
        let i = inputs.clone();
        (
            i,
            ConcreteExample {
                inputs,
                return_value: Some(value),
                thrown_error: None,
            },
        )
    }

    fn example_throws(
        inputs: Vec<serde_json::Value>,
        error_type: &str,
        message: &str,
    ) -> (Vec<serde_json::Value>, ConcreteExample) {
        let i = inputs.clone();
        (
            i,
            ConcreteExample {
                inputs,
                return_value: None,
                thrown_error: Some(ErrorInfo {
                    error_type: error_type.to_string(),
                    message: message.to_string(),
                    stack: None,
                    error_category: None,
                }),
            },
        )
    }

    #[test]
    fn identical_specs_all_matching() {
        let spec_a = make_spec(
            "add_ts",
            vec![
                example_returns(vec![json!(1), json!(2)], json!(3)),
                example_returns(vec![json!(0), json!(0)], json!(0)),
            ],
        );
        let spec_b = make_spec(
            "add_go",
            vec![
                example_returns(vec![json!(1), json!(2)], json!(3)),
                example_returns(vec![json!(0), json!(0)], json!(0)),
            ],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert_eq!(result.matching.len(), 2);
        assert!(result.divergent.is_empty());
        assert!(result.only_in_a.is_empty());
        assert!(result.only_in_b.is_empty());
        assert_eq!(result.similarity_percent(), Some(100.0));
    }

    #[test]
    fn divergent_output_detected() {
        let spec_a = make_spec(
            "div_ts",
            vec![example_returns(vec![json!(10), json!(3)], json!(3))],
        );
        let spec_b = make_spec(
            "div_go",
            vec![example_returns(vec![json!(10), json!(3)], json!(3.333))],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert!(result.matching.is_empty());
        assert_eq!(result.divergent.len(), 1);
        assert_eq!(result.similarity_percent(), Some(0.0));
        assert!(result.has_divergences());
    }

    #[test]
    fn disjoint_inputs_all_unique() {
        let spec_a = make_spec("f_ts", vec![example_returns(vec![json!(1)], json!(10))]);
        let spec_b = make_spec("f_go", vec![example_returns(vec![json!(2)], json!(20))]);

        let result = compare_specs(&spec_a, &spec_b);
        assert!(result.matching.is_empty());
        assert!(result.divergent.is_empty());
        assert_eq!(result.only_in_a.len(), 1);
        assert_eq!(result.only_in_b.len(), 1);
        assert_eq!(result.similarity_percent(), None);
    }

    #[test]
    fn error_normalization_ignores_error_type() {
        // TS reports TypeError, Go reports runtime_error — same message means matching.
        let spec_a = make_spec(
            "validate_ts",
            vec![example_throws(
                vec![json!(-1)],
                "TypeError",
                "value must be positive",
            )],
        );
        let spec_b = make_spec(
            "validate_go",
            vec![example_throws(
                vec![json!(-1)],
                "runtime_error",
                "value must be positive",
            )],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert_eq!(result.matching.len(), 1);
        assert!(result.divergent.is_empty());
    }

    #[test]
    fn error_different_message_is_divergent() {
        let spec_a = make_spec(
            "validate_ts",
            vec![example_throws(
                vec![json!(-1)],
                "TypeError",
                "negative input",
            )],
        );
        let spec_b = make_spec(
            "validate_go",
            vec![example_throws(
                vec![json!(-1)],
                "runtime_error",
                "invalid value: -1",
            )],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert!(result.matching.is_empty());
        assert_eq!(result.divergent.len(), 1);
    }

    #[test]
    fn value_normalization_sorted_keys() {
        // Object key order should not affect comparison.
        let spec_a = make_spec(
            "f_ts",
            vec![example_returns(vec![json!(1)], json!({"b": 2, "a": 1}))],
        );
        let spec_b = make_spec(
            "f_go",
            vec![example_returns(vec![json!(1)], json!({"a": 1, "b": 2}))],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert_eq!(result.matching.len(), 1);
        assert!(result.divergent.is_empty());
    }

    #[test]
    fn void_returns_match() {
        let spec_a = make_spec(
            "noop_ts",
            vec![(
                vec![json!(1)],
                ConcreteExample {
                    inputs: vec![json!(1)],
                    return_value: None,
                    thrown_error: None,
                },
            )],
        );
        let spec_b = make_spec(
            "noop_go",
            vec![(
                vec![json!(1)],
                ConcreteExample {
                    inputs: vec![json!(1)],
                    return_value: Some(json!(null)),
                    thrown_error: None,
                },
            )],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert_eq!(
            result.matching.len(),
            1,
            "None and null should both normalize to Void"
        );
    }

    #[test]
    fn mixed_scenario() {
        let spec_a = make_spec(
            "calc_ts",
            vec![
                example_returns(vec![json!(2), json!(3)], json!(5)), // matching
                example_returns(vec![json!(0), json!(0)], json!(0)), // matching
                example_throws(vec![json!(-1), json!(0)], "Error", "negative"), // divergent
                example_returns(vec![json!(100)], json!(200)),       // only in A
            ],
        );
        let spec_b = make_spec(
            "calc_go",
            vec![
                example_returns(vec![json!(2), json!(3)], json!(5)),
                example_returns(vec![json!(0), json!(0)], json!(0)),
                example_returns(vec![json!(-1), json!(0)], json!(-1)), // divergent (returns instead of throws)
                example_returns(vec![json!(999)], json!(1998)),        // only in B
            ],
        );

        let result = compare_specs(&spec_a, &spec_b);
        assert_eq!(result.matching.len(), 2);
        assert_eq!(result.divergent.len(), 1);
        assert_eq!(result.only_in_a.len(), 1);
        assert_eq!(result.only_in_b.len(), 1);
        let pct = result
            .similarity_percent()
            .expect("should have shared inputs");
        assert!((pct - 66.67).abs() < 0.1, "expected ~66.67%, got {pct}"); // 2/3
    }

    #[test]
    fn format_text_includes_summary_metric() {
        let spec_a = make_spec(
            "f_ts",
            vec![
                example_returns(vec![json!(1)], json!(10)),
                example_returns(vec![json!(2)], json!(20)),
            ],
        );
        let spec_b = make_spec(
            "f_go",
            vec![
                example_returns(vec![json!(1)], json!(10)),
                example_returns(vec![json!(2)], json!(99)),
            ],
        );

        let result = compare_specs(&spec_a, &spec_b);
        let text = format_compare_text(&result);
        assert!(text.contains("1 of 2 shared behaviors match"));
        assert!(text.contains("50%"));
    }

    #[test]
    fn format_json_roundtrips() {
        let spec_a = make_spec("f_ts", vec![example_returns(vec![json!(1)], json!(2))]);
        let spec_b = make_spec("f_go", vec![example_returns(vec![json!(1)], json!(2))]);

        let result = compare_specs(&spec_a, &spec_b);
        let json_str = format_compare_json(&result).expect("serialize");
        let parsed: CompareResult = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(parsed, result);
    }

    #[test]
    fn normalize_value_sorts_nested_keys() {
        let v = json!({"z": [{"c": 3, "a": 1}], "a": {"y": 2, "x": 1}});
        let normalized = normalize_value(&v);
        // Verify keys are sorted by re-serializing
        let s = serde_json::to_string(&normalized).unwrap();
        assert!(s.starts_with("{\"a\":{\"x\":"));
    }

    #[test]
    fn normalize_value_is_idempotent() {
        let v = json!({"z": 1, "a": [{"c": 3, "b": 2}], "m": null});
        let once = normalize_value(&v);
        let twice = normalize_value(&once);
        assert_eq!(once, twice);
    }

    // ── Property-based tests ─────────────────────────────────────────────

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn arb_value() -> impl Strategy<Value = serde_json::Value> {
            prop_oneof![
                Just(json!(null)),
                any::<bool>().prop_map(|b| json!(b)),
                any::<i64>().prop_map(|n| json!(n)),
                "[a-z]{1,10}".prop_map(|s| json!(s)),
            ]
        }

        fn arb_inputs() -> impl Strategy<Value = Vec<serde_json::Value>> {
            prop::collection::vec(arb_value(), 1..4)
        }

        fn arb_example() -> impl Strategy<Value = (Vec<serde_json::Value>, ConcreteExample)> {
            (arb_inputs(), arb_value()).prop_map(|(inputs, ret)| {
                let i = inputs.clone();
                (
                    i,
                    ConcreteExample {
                        inputs,
                        return_value: Some(ret),
                        thrown_error: None,
                    },
                )
            })
        }

        fn arb_spec(name: &'static str) -> impl Strategy<Value = FunctionSpec> {
            prop::collection::vec(arb_example(), 1..6)
                .prop_map(move |examples| make_spec(name, examples))
        }

        proptest! {
            #[test]
            fn comparison_is_symmetric(
                a in arb_spec("f_a"),
                b in arb_spec("f_b"),
            ) {
                let r1 = compare_specs(&a, &b);
                let r2 = compare_specs(&b, &a);
                prop_assert_eq!(r1.matching.len(), r2.matching.len());
                prop_assert_eq!(r1.divergent.len(), r2.divergent.len());
                prop_assert_eq!(r1.only_in_a.len(), r2.only_in_b.len());
                prop_assert_eq!(r1.only_in_b.len(), r2.only_in_a.len());
            }

            #[test]
            fn normalization_is_idempotent(v in arb_value()) {
                let once = normalize_value(&v);
                let twice = normalize_value(&once);
                prop_assert_eq!(once, twice);
            }

            #[test]
            fn self_comparison_has_no_divergences(a in arb_spec("f")) {
                let result = compare_specs(&a, &a);
                prop_assert!(result.divergent.is_empty());
                prop_assert!(result.only_in_a.is_empty());
                prop_assert!(result.only_in_b.is_empty());
            }
        }
    }
}
