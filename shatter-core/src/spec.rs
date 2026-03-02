//! Behavior specification output for explored functions.
//!
//! A [`FunctionSpec`] captures the complete behavioral specification of a function:
//! equivalence classes grouped by branch path, preconditions, postconditions,
//! concrete examples, and provenance (proven vs observed). The spec can be
//! rendered as human-readable markdown or machine-readable JSON.

use serde::{Deserialize, Serialize};

use crate::equivalence::{BranchPath, EquivalenceClass, Precondition};
use crate::execution_record::{ErrorInfo, SideEffect};
use crate::explorer::ExplorationResult;
use crate::invariants::ClassifiedInvariant;

/// Whether a property was proven by constraint solving or merely observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Confirmed by Z3 constraint solving — holds for all inputs on this path.
    Proven,
    /// Observed across all sampled inputs but not formally proven.
    Observed,
}

/// A postcondition describing what a function does on a given path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Postcondition {
    /// The function returns a value matching this pattern.
    Returns {
        /// A representative return value (from the canonical example).
        value: serde_json::Value,
    },
    /// The function throws an error.
    Throws {
        /// Error type and message.
        error: ErrorInfo,
    },
    /// The function returns void / undefined / null.
    ReturnsVoid,
}

/// A concrete input/output example for an equivalence class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcreteExample {
    /// Input arguments.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error info, if the function threw.
    pub thrown_error: Option<ErrorInfo>,
}

/// One behavioral equivalence class within a function specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecClass {
    /// Human-readable label for this class (e.g., "returns positive result").
    pub label: String,
    /// The branch path identifying this class.
    pub branch_path: BranchPath,
    /// Preconditions: constraints on inputs that lead to this path.
    pub preconditions: Vec<Precondition>,
    /// Postcondition: what the function does on this path.
    pub postcondition: Postcondition,
    /// Side effects observed on this path.
    pub side_effects: Vec<SideEffect>,
    /// At least one concrete example.
    pub examples: Vec<ConcreteExample>,
    /// How many executions fell into this class.
    pub sample_count: usize,
    /// Whether preconditions are proven or observed.
    pub precondition_provenance: Provenance,
    /// Whether postconditions are proven or observed.
    pub postcondition_provenance: Provenance,
    /// Detected invariants for this equivalence class (empty if invariant detection is disabled).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<ClassifiedInvariant>,
}

/// Per-file spec bundle: all function specs from a single source file.
///
/// Used by `--output` to write a single JSON file containing specs for every
/// explored function in a source file, keyed by file path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSpecBundle {
    /// Source file path (e.g., "src/math.ts").
    pub file: String,
    /// Specs for each explored function in this file.
    pub functions: Vec<FunctionSpec>,
}

/// Complete behavioral specification of a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionSpec {
    /// Fully qualified function name.
    pub function_name: String,
    /// Source file and location (e.g., "src/math.ts:10").
    pub location: Option<String>,
    /// Equivalence classes describing distinct behaviors.
    pub classes: Vec<SpecClass>,
    /// Total iterations used during exploration.
    pub iterations: u32,
    /// Line coverage achieved.
    pub lines_covered: usize,
    /// Total lines in the function.
    pub total_lines: u32,
    /// Function-wide invariants (across all classes). Empty if invariant detection is disabled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<ClassifiedInvariant>,
    /// SHA-256 fingerprint of the function's source, params, and branches.
    /// Used for staleness detection across runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// Build a [`FunctionSpec`] from an exploration result and its equivalence classes.
///
/// Equivalence classes are produced by [`crate::equivalence::group_into_classes`].
/// The provenance is always `Observed` since the random explorer does not use Z3.
pub fn build_spec(
    result: &ExplorationResult,
    eq_classes: &[EquivalenceClass],
    location: Option<String>,
    fingerprint: Option<String>,
) -> FunctionSpec {
    let classes = eq_classes
        .iter()
        .enumerate()
        .map(|(i, ec)| build_spec_class(i, ec))
        .collect();

    FunctionSpec {
        function_name: result.function_name.clone(),
        location,
        classes,
        iterations: result.iterations,
        lines_covered: result.lines_covered,
        total_lines: result.total_lines,
        invariants: vec![],
        fingerprint,
    }
}

/// Build a [`FunctionSpec`] with invariant detection enabled.
///
/// Runs Daikon-style invariant detection on the raw execution results, producing
/// classified invariants at both the per-class and function-wide levels.
pub fn build_spec_with_invariants(
    result: &ExplorationResult,
    eq_classes: &[EquivalenceClass],
    location: Option<String>,
    fingerprint: Option<String>,
) -> FunctionSpec {
    use crate::invariants::{
        detect_classified_invariants, records_from_raw_results, InvariantTarget,
    };

    // Convert raw results to execution records for invariant detection
    let all_records = records_from_raw_results(&result.function_name, &result.raw_results);

    // Function-wide invariants (across all executions)
    let mut function_invariants =
        detect_classified_invariants(&all_records, InvariantTarget::Input);
    function_invariants.extend(detect_classified_invariants(
        &all_records,
        InvariantTarget::Output,
    ));

    // Per-class invariants: group records by equivalence class branch path
    let classes: Vec<SpecClass> = eq_classes
        .iter()
        .enumerate()
        .map(|(i, ec)| {
            let mut spec_class = build_spec_class(i, ec);

            // Collect records belonging to this class by matching inputs
            let class_records: Vec<_> = all_records
                .iter()
                .filter(|r| ec.all_inputs.contains(&r.parameters))
                .cloned()
                .collect();

            if class_records.len() >= 2 {
                let mut class_invs =
                    detect_classified_invariants(&class_records, InvariantTarget::Input);
                class_invs.extend(detect_classified_invariants(
                    &class_records,
                    InvariantTarget::Output,
                ));
                spec_class.invariants = class_invs;
            }

            spec_class
        })
        .collect();

    FunctionSpec {
        function_name: result.function_name.clone(),
        location,
        classes,
        iterations: result.iterations,
        lines_covered: result.lines_covered,
        total_lines: result.total_lines,
        invariants: function_invariants,
        fingerprint,
    }
}

/// Build a single [`SpecClass`] from an equivalence class.
fn build_spec_class(index: usize, ec: &EquivalenceClass) -> SpecClass {
    let postcondition = if let Some(ref err_msg) = ec.canonical_thrown_error {
        let (error_type, message) = match err_msg.split_once(": ") {
            Some((t, m)) => (t.to_string(), m.to_string()),
            None => ("Error".to_string(), err_msg.clone()),
        };
        Postcondition::Throws {
            error: ErrorInfo {
                error_type,
                message,
                stack: None,
            },
        }
    } else {
        match &ec.canonical_return_value {
            Some(v) if !v.is_null() => Postcondition::Returns { value: v.clone() },
            _ => Postcondition::ReturnsVoid,
        }
    };

    let label = format_class_label(index, &postcondition);

    let canonical_example = ConcreteExample {
        inputs: ec.canonical_example.clone(),
        return_value: ec.canonical_return_value.clone(),
        thrown_error: ec.canonical_thrown_error.as_ref().map(|msg| {
            let (error_type, message) = match msg.split_once(": ") {
                Some((t, m)) => (t.to_string(), m.to_string()),
                None => ("Error".to_string(), msg.clone()),
            };
            ErrorInfo {
                error_type,
                message,
                stack: None,
            }
        }),
    };

    SpecClass {
        label,
        branch_path: ec.branch_path.clone(),
        preconditions: ec.common_preconditions.clone(),
        postcondition,
        side_effects: vec![],
        examples: vec![canonical_example],
        sample_count: ec.all_inputs.len(),
        precondition_provenance: Provenance::Observed,
        postcondition_provenance: Provenance::Observed,
        invariants: vec![],
    }
}

/// Generate a human-readable label for a spec class.
fn format_class_label(index: usize, postcondition: &Postcondition) -> String {
    let outcome = match postcondition {
        Postcondition::Returns { value } => format!("returns {}", format_value_short(value)),
        Postcondition::Throws { error } => {
            format!("throws {}: {}", error.error_type, error.message)
        }
        Postcondition::ReturnsVoid => "returns void".to_string(),
    };
    format!("Class {} — {}", index + 1, outcome)
}

/// Format the spec as human-readable markdown.
pub fn format_spec_markdown(spec: &FunctionSpec) -> String {
    let mut out = String::new();

    // Title
    out.push_str(&format!("# Specification: `{}`\n\n", spec.function_name));

    if let Some(ref loc) = spec.location {
        out.push_str(&format!("**Location:** `{loc}`\n\n"));
    }

    // Summary
    out.push_str(&format!(
        "**Behavioral classes:** {}  \n",
        spec.classes.len()
    ));
    out.push_str(&format!(
        "**Exploration:** {} iterations, {}/{} lines covered",
        spec.iterations, spec.lines_covered, spec.total_lines,
    ));
    if spec.total_lines > 0 {
        let pct = (spec.lines_covered as f64 / spec.total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(" ({pct:.0}%)"));
    }
    out.push_str("\n\n");

    // Function-wide invariants
    if !spec.invariants.is_empty() {
        out.push_str("**Function invariants:**\n");
        for ci in &spec.invariants {
            out.push_str(&format!(
                "- {} [{}] ({}/{})\n",
                ci.invariant.description,
                ci.confidence,
                ci.satisfied_count,
                ci.total_count,
            ));
        }
        out.push('\n');
    }

    out.push_str("---\n\n");

    // Classes
    for (i, class) in spec.classes.iter().enumerate() {
        out.push_str(&format!("## {}\n\n", class.label));

        let pre_badge = provenance_badge(class.precondition_provenance);
        let post_badge = provenance_badge(class.postcondition_provenance);

        // Preconditions
        if class.preconditions.is_empty() {
            out.push_str(&format!("**Preconditions** {pre_badge}: _(none derived)_\n\n"));
        } else {
            out.push_str(&format!("**Preconditions** {pre_badge}:\n"));
            for pre in &class.preconditions {
                out.push_str(&format!("- {}\n", format_precondition(pre)));
            }
            out.push('\n');
        }

        // Postcondition
        out.push_str(&format!(
            "**Postcondition** {post_badge}: {}\n\n",
            format_postcondition(&class.postcondition)
        ));

        // Side effects
        if !class.side_effects.is_empty() {
            out.push_str("**Side effects:**\n");
            for effect in &class.side_effects {
                out.push_str(&format!("- {}\n", format_side_effect(effect)));
            }
            out.push('\n');
        }

        // Invariants (per-class)
        if !class.invariants.is_empty() {
            out.push_str("**Invariants:**\n");
            for ci in &class.invariants {
                out.push_str(&format!(
                    "- {} [{}] ({}/{})\n",
                    ci.invariant.description,
                    ci.confidence,
                    ci.satisfied_count,
                    ci.total_count,
                ));
            }
            out.push('\n');
        }

        // Examples
        out.push_str(&format!(
            "**Example** ({} execution(s) observed):\n",
            class.sample_count
        ));
        for example in &class.examples {
            let inputs_str = example
                .inputs
                .iter()
                .map(format_value_short)
                .collect::<Vec<_>>()
                .join(", ");

            let outcome = if let Some(ref err) = example.thrown_error {
                format!("throws {}: {}", err.error_type, err.message)
            } else {
                match &example.return_value {
                    Some(v) if !v.is_null() => format!("-> {}", format_value_short(v)),
                    _ => "-> void".to_string(),
                }
            };

            out.push_str(&format!(
                "```\n{}({inputs_str}) {outcome}\n```\n",
                spec.function_name
            ));
        }

        if i + 1 < spec.classes.len() {
            out.push_str("\n---\n\n");
        }
    }

    out
}

/// Format the spec as machine-readable JSON.
///
/// Returns a `Result` because serialization can theoretically fail.
pub fn format_spec_json(spec: &FunctionSpec) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(spec)
}

/// Format a collection of per-file spec bundles as machine-readable JSON.
pub fn format_file_spec_json(bundles: &[FileSpecBundle]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(bundles)
}

/// Badge text for provenance level.
fn provenance_badge(p: Provenance) -> &'static str {
    match p {
        Provenance::Proven => "[proven]",
        Provenance::Observed => "[observed]",
    }
}

/// Format a precondition as a human-readable string.
fn format_precondition(pre: &Precondition) -> String {
    match pre {
        Precondition::AllPositive { param_index } => {
            format!("param[{param_index}] > 0")
        }
        Precondition::AllNegative { param_index } => {
            format!("param[{param_index}] < 0")
        }
        Precondition::AllZero { param_index } => {
            format!("param[{param_index}] == 0")
        }
        Precondition::AllEqual { param_index, value } => {
            format!("param[{param_index}] == {}", format_value_short(value))
        }
        Precondition::SameType {
            param_index,
            type_name,
        } => {
            format!("typeof param[{param_index}] == \"{type_name}\"")
        }
    }
}

/// Format a postcondition as a human-readable string.
fn format_postcondition(post: &Postcondition) -> String {
    match post {
        Postcondition::Returns { value } => {
            format!("returns {}", format_value_short(value))
        }
        Postcondition::Throws { error } => {
            format!("throws {}: {}", error.error_type, error.message)
        }
        Postcondition::ReturnsVoid => "returns void".to_string(),
    }
}

/// Format a side effect as a human-readable string.
fn format_side_effect(effect: &SideEffect) -> String {
    match effect {
        SideEffect::ConsoleOutput { level, message } => {
            format!("console.{level}(\"{message}\")")
        }
        SideEffect::FileWrite { path, .. } => format!("writes to file: {path}"),
        SideEffect::NetworkRequest { method, url, .. } => format!("{method} {url}"),
        SideEffect::EnvironmentRead { variable, value } => {
            let val = value.as_deref().unwrap_or("null");
            format!("reads env: {variable}={val}")
        }
        SideEffect::GlobalMutation { name } => format!("mutates global: {name}"),
        SideEffect::ThrownError {
            error_type,
            message,
            ..
        } => {
            format!("throws {error_type}: {message}")
        }
        SideEffect::GlobalStateChange {
            variable,
            before,
            after,
        } => {
            format!(
                "{variable}: {} -> {}",
                format_value_short(before),
                format_value_short(after)
            )
        }
    }
}

/// Format a JSON value for display, truncating long values.
fn format_value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equivalence::{BranchPath, BranchStep, EquivalenceClass, Precondition};
    use crate::execution_record::ErrorInfo;
    use crate::explorer::ExplorationResult;
    use serde_json::json;

    fn make_exploration_result(
        name: &str,
        iterations: u32,
        unique_paths: usize,
    ) -> ExplorationResult {
        ExplorationResult {
            function_name: name.to_string(),
            iterations,
            unique_paths,
            lines_covered: 8,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![],
        }
    }

    fn make_eq_class(
        branch_steps: Vec<(u32, bool)>,
        canonical_inputs: Vec<serde_json::Value>,
        canonical_return: Option<serde_json::Value>,
        canonical_error: Option<String>,
        preconditions: Vec<Precondition>,
        sample_count: usize,
    ) -> EquivalenceClass {
        let branch_path = BranchPath(
            branch_steps
                .into_iter()
                .map(|(id, taken)| BranchStep {
                    branch_id: id,
                    taken,
                })
                .collect(),
        );

        let all_inputs = (0..sample_count)
            .map(|_| canonical_inputs.clone())
            .collect();

        EquivalenceClass {
            branch_path,
            canonical_example: canonical_inputs,
            all_inputs,
            common_preconditions: preconditions,
            canonical_return_value: canonical_return,
            canonical_thrown_error: canonical_error,
        }
    }

    #[test]
    fn build_spec_from_exploration_and_classes() {
        let result = make_exploration_result("classifyNumber", 50, 3);
        let classes = vec![
            make_eq_class(
                vec![(0, true)],
                vec![json!(5)],
                Some(json!("positive")),
                None,
                vec![Precondition::AllPositive { param_index: 0 }],
                10,
            ),
            make_eq_class(
                vec![(0, false), (1, true)],
                vec![json!(-3)],
                Some(json!("negative")),
                None,
                vec![Precondition::AllNegative { param_index: 0 }],
                8,
            ),
            make_eq_class(
                vec![(0, false), (1, false)],
                vec![json!(0)],
                Some(json!("zero")),
                None,
                vec![Precondition::AllZero { param_index: 0 }],
                2,
            ),
        ];

        let spec = build_spec(&result, &classes, Some("math.ts:10".to_string()), None);

        assert_eq!(spec.function_name, "classifyNumber");
        assert_eq!(spec.location.as_deref(), Some("math.ts:10"));
        assert_eq!(spec.classes.len(), 3);
        assert_eq!(spec.iterations, 50);
        assert_eq!(spec.lines_covered, 8);
        assert_eq!(spec.total_lines, 10);

        // Check first class
        assert!(spec.classes[0].label.contains("positive"));
        assert_eq!(spec.classes[0].preconditions.len(), 1);
        assert_eq!(
            spec.classes[0].postcondition,
            Postcondition::Returns {
                value: json!("positive")
            }
        );
        assert_eq!(spec.classes[0].examples.len(), 1);
        assert_eq!(spec.classes[0].examples[0].inputs, vec![json!(5)]);
        assert_eq!(spec.classes[0].sample_count, 10);
    }

    #[test]
    fn build_spec_with_error_class() {
        let result = make_exploration_result("safeDivide", 20, 2);
        let classes = vec![
            make_eq_class(
                vec![(0, false)],
                vec![json!(10), json!(2)],
                Some(json!(5)),
                None,
                vec![],
                15,
            ),
            make_eq_class(
                vec![(0, true)],
                vec![json!(1), json!(0)],
                None,
                Some("Error: division by zero".to_string()),
                vec![Precondition::AllZero { param_index: 1 }],
                5,
            ),
        ];

        let spec = build_spec(&result, &classes, None, None);

        assert_eq!(spec.classes.len(), 2);

        let error_class = &spec.classes[1];
        assert!(error_class.label.contains("throws"));
        assert!(matches!(
            &error_class.postcondition,
            Postcondition::Throws { error } if error.message == "division by zero"
        ));
        assert!(error_class.examples[0].thrown_error.is_some());
    }

    #[test]
    fn build_spec_with_void_return() {
        let result = make_exploration_result("logMessage", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!("hello")],
            None,
            None,
            vec![],
            10,
        )];

        let spec = build_spec(&result, &classes, None, None);
        assert_eq!(spec.classes[0].postcondition, Postcondition::ReturnsVoid);
        assert!(spec.classes[0].label.contains("void"));
    }

    #[test]
    fn build_spec_empty_classes() {
        let result = make_exploration_result("unused", 0, 0);
        let spec = build_spec(&result, &[], None, None);

        assert!(spec.classes.is_empty());
        assert_eq!(spec.function_name, "unused");
    }

    #[test]
    fn format_spec_markdown_contains_all_sections() {
        let result = make_exploration_result("classify", 50, 2);
        let classes = vec![
            make_eq_class(
                vec![(0, true)],
                vec![json!(5)],
                Some(json!("positive")),
                None,
                vec![Precondition::AllPositive { param_index: 0 }],
                10,
            ),
            make_eq_class(
                vec![(0, false)],
                vec![json!(-3)],
                Some(json!("negative")),
                None,
                vec![Precondition::AllNegative { param_index: 0 }],
                8,
            ),
        ];

        let spec = build_spec(&result, &classes, Some("math.ts:5".to_string()), None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("# Specification: `classify`"), "missing title");
        assert!(md.contains("**Location:** `math.ts:5`"), "missing location");
        assert!(
            md.contains("**Behavioral classes:** 2"),
            "missing class count"
        );
        assert!(md.contains("50 iterations"), "missing iterations");
        assert!(md.contains("80%"), "missing coverage pct");
        assert!(md.contains("## Class 1"), "missing class 1 heading");
        assert!(md.contains("## Class 2"), "missing class 2 heading");
        assert!(md.contains("[observed]"), "missing provenance badge");
        assert!(md.contains("param[0] > 0"), "missing precondition");
        assert!(md.contains("param[0] < 0"), "missing precondition");
        assert!(md.contains("returns \"positive\""), "missing postcondition");
        assert!(md.contains("returns \"negative\""), "missing postcondition");
        assert!(md.contains("**Example**"), "missing example section");
        assert!(md.contains("classify(5)"), "missing example call");
    }

    #[test]
    fn format_spec_markdown_error_class() {
        let result = make_exploration_result("boom", 10, 1);
        let classes = vec![make_eq_class(
            vec![(0, true)],
            vec![json!(null)],
            None,
            Some("TypeError: null input".to_string()),
            vec![],
            3,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("throws TypeError: null input"), "missing error");
        assert!(md.contains("boom(null)"), "missing example");
    }

    #[test]
    fn format_spec_json_round_trips() {
        let result = make_exploration_result("add", 20, 1);
        let classes = vec![make_eq_class(
            vec![(0, true)],
            vec![json!(1), json!(2)],
            Some(json!(3)),
            None,
            vec![Precondition::AllPositive { param_index: 0 }],
            15,
        )];

        let spec = build_spec(&result, &classes, Some("math.ts:1".to_string()), None);
        let json_str = format_spec_json(&spec).expect("json serialization");

        let deserialized: FunctionSpec =
            serde_json::from_str(&json_str).expect("json deserialization");
        assert_eq!(deserialized.function_name, "add");
        assert_eq!(deserialized.classes.len(), 1);
        assert_eq!(deserialized.classes[0].examples.len(), 1);
        assert_eq!(deserialized.classes[0].examples[0].inputs, vec![json!(1), json!(2)]);
        assert_eq!(
            deserialized.classes[0].postcondition,
            Postcondition::Returns { value: json!(3) }
        );
    }

    #[test]
    fn format_spec_json_contains_expected_fields() {
        let result = make_exploration_result("fn1", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!(42)],
            Some(json!(84)),
            None,
            vec![],
            5,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("json parse");

        assert_eq!(parsed["function_name"], "fn1");
        assert!(parsed["location"].is_null());
        assert!(parsed["classes"].is_array());
        assert_eq!(parsed["classes"][0]["sample_count"], 5);
        assert_eq!(
            parsed["classes"][0]["precondition_provenance"],
            "observed"
        );
        assert_eq!(
            parsed["classes"][0]["postcondition_provenance"],
            "observed"
        );
    }

    #[test]
    fn provenance_serialization_round_trips() {
        for p in [Provenance::Proven, Provenance::Observed] {
            let json_str = serde_json::to_string(&p).expect("serialize");
            let deserialized: Provenance =
                serde_json::from_str(&json_str).expect("deserialize");
            assert_eq!(p, deserialized);
        }
    }

    #[test]
    fn postcondition_serialization_round_trips() {
        let postconditions = vec![
            Postcondition::Returns {
                value: json!("hello"),
            },
            Postcondition::Throws {
                error: ErrorInfo {
                    error_type: "Error".to_string(),
                    message: "boom".to_string(),
                    stack: None,
                },
            },
            Postcondition::ReturnsVoid,
        ];

        for post in &postconditions {
            let json_str = serde_json::to_string(post).expect("serialize");
            let deserialized: Postcondition =
                serde_json::from_str(&json_str).expect("deserialize");
            assert_eq!(*post, deserialized);
        }
    }

    #[test]
    fn format_precondition_all_variants() {
        assert_eq!(
            format_precondition(&Precondition::AllPositive { param_index: 0 }),
            "param[0] > 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllNegative { param_index: 1 }),
            "param[1] < 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllZero { param_index: 2 }),
            "param[2] == 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllEqual {
                param_index: 0,
                value: json!("hello"),
            }),
            "param[0] == \"hello\""
        );
        assert_eq!(
            format_precondition(&Precondition::SameType {
                param_index: 0,
                type_name: "number".to_string(),
            }),
            "typeof param[0] == \"number\""
        );
    }

    #[test]
    fn format_side_effect_all_variants() {
        assert_eq!(
            format_side_effect(&SideEffect::ConsoleOutput {
                level: "info".into(),
                message: "test".into()
            }),
            "console.info(\"test\")"
        );
        assert_eq!(
            format_side_effect(&SideEffect::FileWrite {
                path: "/tmp/out".into(),
                content: None,
            }),
            "writes to file: /tmp/out"
        );
        assert_eq!(
            format_side_effect(&SideEffect::NetworkRequest {
                method: "GET".into(),
                url: "http://api.test".into(),
                body: None,
            }),
            "GET http://api.test"
        );
        assert_eq!(
            format_side_effect(&SideEffect::GlobalMutation {
                name: "counter".into()
            }),
            "mutates global: counter"
        );
    }

    #[test]
    fn format_spec_markdown_no_preconditions() {
        let result = make_exploration_result("fn1", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!(1)],
            Some(json!(2)),
            None,
            vec![],
            5,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("_(none derived)_"), "should note no preconditions");
    }

    #[test]
    fn format_spec_markdown_no_location() {
        let result = make_exploration_result("fn1", 10, 1);
        let spec = build_spec(&result, &[], None, None);
        let md = format_spec_markdown(&spec);

        assert!(!md.contains("**Location:**"), "should not have location");
    }

    #[test]
    fn full_spec_round_trip_with_all_fields() {
        let spec = FunctionSpec {
            function_name: "complexFn".to_string(),
            location: Some("src/complex.ts:42".to_string()),
            classes: vec![
                SpecClass {
                    label: "Class 1 — returns 42".to_string(),
                    branch_path: BranchPath(vec![BranchStep {
                        branch_id: 0,
                        taken: true,
                    }]),
                    preconditions: vec![Precondition::AllPositive { param_index: 0 }],
                    postcondition: Postcondition::Returns { value: json!(42) },
                    side_effects: vec![SideEffect::ConsoleOutput {
                        level: "info".into(),
                        message: "processed".into(),
                    }],
                    examples: vec![ConcreteExample {
                        inputs: vec![json!(5)],
                        return_value: Some(json!(42)),
                        thrown_error: None,
                    }],
                    sample_count: 10,
                    precondition_provenance: Provenance::Proven,
                    postcondition_provenance: Provenance::Observed,
                    invariants: vec![],
                },
                SpecClass {
                    label: "Class 2 — throws Error: bad input".to_string(),
                    branch_path: BranchPath(vec![BranchStep {
                        branch_id: 0,
                        taken: false,
                    }]),
                    preconditions: vec![],
                    postcondition: Postcondition::Throws {
                        error: ErrorInfo {
                            error_type: "Error".to_string(),
                            message: "bad input".to_string(),
                            stack: None,
                        },
                    },
                    side_effects: vec![],
                    examples: vec![ConcreteExample {
                        inputs: vec![json!(-1)],
                        return_value: None,
                        thrown_error: Some(ErrorInfo {
                            error_type: "Error".to_string(),
                            message: "bad input".to_string(),
                            stack: None,
                        }),
                    }],
                    sample_count: 5,
                    precondition_provenance: Provenance::Observed,
                    postcondition_provenance: Provenance::Observed,
                    invariants: vec![],
                },
            ],
            iterations: 100,
            lines_covered: 15,
            total_lines: 20,
            invariants: vec![],
            fingerprint: None,
        };

        let json_str = serde_json::to_string_pretty(&spec).expect("serialize");
        let deserialized: FunctionSpec =
            serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(spec, deserialized);
    }

    #[test]
    fn format_spec_markdown_includes_function_invariants() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.invariants = vec![ClassifiedInvariant {
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
        }];

        let md = format_spec_markdown(&spec);
        assert!(md.contains("Function invariants:"), "should have function invariants section");
        assert!(md.contains("x > 0"), "should contain invariant description");
    }

    #[test]
    fn format_spec_markdown_includes_class_invariants() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.classes[0].invariants = vec![ClassifiedInvariant {
            invariant: Invariant {
                description: "y >= 0".to_string(),
                target: InvariantTarget::Output,
                kind: InvariantKind::NumericComparison {
                    path: vec![],
                    op: ComparisonOp::Ge,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Output,
            label: "output >= 0".to_string(),
            confidence: 1.0,
            satisfied_count: 5,
            total_count: 5,
        }];

        let md = format_spec_markdown(&spec);
        assert!(md.contains("**Invariants:**"), "should have per-class invariants section");
        assert!(md.contains("y >= 0"), "should contain invariant description");
    }

    #[test]
    fn invariants_skipped_in_json_when_empty() {
        let spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );

        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

        assert!(parsed.get("invariants").is_none(), "empty invariants should be skipped");
        assert!(
            parsed["classes"][0].get("invariants").is_none(),
            "empty class invariants should be skipped"
        );
    }

    #[test]
    fn invariants_present_in_json_when_populated() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.invariants = vec![ClassifiedInvariant {
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
        }];

        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

        assert!(parsed["invariants"].is_array(), "invariants should be present when populated");
        assert_eq!(parsed["invariants"][0]["label"], "input.x > 0");
    }

    #[test]
    fn file_spec_bundle_round_trip() {
        let spec = build_spec(
            &make_exploration_result("add", 10, 2),
            &[],
            Some("src/math.ts:1".to_string()),
            None,
        );
        let bundle = FileSpecBundle {
            file: "src/math.ts".to_string(),
            functions: vec![spec],
        };
        let bundles = vec![bundle];
        let json = format_file_spec_json(&bundles).expect("serialize");
        let parsed: Vec<FileSpecBundle> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].file, "src/math.ts");
        assert_eq!(parsed[0].functions.len(), 1);
        assert_eq!(parsed[0].functions[0].function_name, "add");
    }
}
