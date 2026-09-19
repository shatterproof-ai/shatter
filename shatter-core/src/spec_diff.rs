//! Specification-level diffing: compare two [`FunctionSpec`]s to detect
//! behavioral regressions, added/removed equivalence classes, and changed
//! pre/postconditions.
//!
//! The main entry point is [`diff_specs`], which produces a [`SpecDiff`].
//! Two formatters are provided: [`format_spec_diff_text`] for human-readable
//! output and [`format_spec_diff_json`] for machine-readable JSON.

use serde::{Deserialize, Serialize};

use crate::equivalence::Precondition;
use crate::nondeterminism::NondeterministicField;
use crate::spec::{ConcreteExample, FunctionSpec, Postcondition, SpecClass};

/// The result of diffing two function specifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecDiff {
    /// Function name being compared.
    pub function_name: String,
    /// Equivalence classes present in the new spec but not the old.
    pub added_classes: Vec<SpecClass>,
    /// Equivalence classes present in the old spec but not the new.
    pub removed_classes: Vec<SpecClass>,
    /// Postconditions that changed between matching classes.
    pub changed_postconditions: Vec<PostconditionChange>,
    /// Preconditions that changed between matching classes.
    pub changed_preconditions: Vec<PreconditionChange>,
    /// Invariant properties that held in the old spec but not the new.
    pub lost_properties: Vec<String>,
    /// Matched classes whose postconditions could not be compared because no
    /// comparable canonical example was available on both sides (str-nfg4y).
    ///
    /// This is not a regression: it means the diff has no basis for a
    /// verdict on that class, not that the class was checked and found
    /// unchanged. A genuine regression that happens to also shift which
    /// input the explorer sampled as canonical will be reported here rather
    /// than in `changed_postconditions` — this guard cannot distinguish
    /// "different canonical input, same behavior" from "different canonical
    /// input, and behavior also changed". See str-mfmmr for the analogous
    /// gap in `lost_properties`'s aggregate throw/return check, which this
    /// guard does not cover.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comparison_notes: Vec<ComparisonNote>,
}

/// A note recorded when a matched class's postcondition could not be
/// compared for lack of a comparable canonical example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonNote {
    /// Label of the class the note applies to.
    pub class_label: String,
    /// Why the comparison could not be made.
    pub reason: ComparisonNoteReason,
}

/// Typed reason a [`ComparisonNote`] was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonNoteReason {
    /// One or both classes had no canonical example, or their canonical
    /// examples' input vectors were not structurally equal, so comparing
    /// their postconditions would compare unrelated inputs.
    MissingComparableWitness,
}

/// A postcondition that changed between two versions of a spec class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostconditionChange {
    /// Label of the class where the change occurred.
    pub class_label: String,
    /// The old postcondition.
    pub old: Postcondition,
    /// The new postcondition.
    pub new: Postcondition,
}

/// A precondition change between two versions of a spec class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreconditionChange {
    /// Label of the class where the change occurred.
    pub class_label: String,
    /// Preconditions that were removed (no longer hold).
    pub removed: Vec<Precondition>,
    /// Preconditions that were added (newly derived).
    pub added: Vec<Precondition>,
}

impl SpecDiff {
    /// Whether the diff has nothing to report: no detected changes and no
    /// comparison notes. This does not mean the two specs were proven
    /// behaviorally equivalent — sampled examples are not exhaustive, and a
    /// note-free result still only reflects what was comparable.
    pub fn is_empty(&self) -> bool {
        self.added_classes.is_empty()
            && self.removed_classes.is_empty()
            && self.changed_postconditions.is_empty()
            && self.changed_preconditions.is_empty()
            && self.lost_properties.is_empty()
            && self.comparison_notes.is_empty()
    }

    /// Whether the diff contains regressions (removed classes, changed
    /// postconditions, or lost properties). Comparison notes are excluded:
    /// they report insufficient evidence, not a detected regression.
    pub fn has_regressions(&self) -> bool {
        !self.removed_classes.is_empty()
            || !self.changed_postconditions.is_empty()
            || !self.lost_properties.is_empty()
    }
}

/// Compare two function specs and produce a diff.
///
/// Classes are matched by their branch path. Classes with the same branch
/// path are compared for precondition and postcondition changes. Classes
/// that exist only in one spec are reported as added or removed.
pub fn diff_specs(old: &FunctionSpec, new: &FunctionSpec) -> SpecDiff {
    let mut added_classes = Vec::new();
    let mut removed_classes = Vec::new();
    let mut changed_postconditions = Vec::new();
    let mut changed_preconditions = Vec::new();
    let mut lost_properties = Vec::new();
    let mut comparison_notes = Vec::new();

    // Collect nondeterministic fields from both specs (union).
    let nondet_fields: Vec<&NondeterministicField> = old
        .nondeterministic_fields
        .iter()
        .chain(new.nondeterministic_fields.iter())
        .collect();

    // Index old classes by branch path for matching.
    let old_by_path: std::collections::HashMap<_, _> =
        old.classes.iter().map(|c| (&c.branch_path, c)).collect();

    let new_by_path: std::collections::HashMap<_, _> =
        new.classes.iter().map(|c| (&c.branch_path, c)).collect();

    // Find matched, added, and changed classes.
    for new_class in &new.classes {
        match old_by_path.get(&new_class.branch_path) {
            Some(old_class) => {
                // Matched by branch path. A sampled return is an example,
                // not the full return set for the path: only compare
                // postconditions when both sides recorded a canonical
                // example and their input vectors are structurally equal.
                // Otherwise the two representative outcomes may simply come
                // from different inputs — comparing them would compare
                // unrelated evidence, not detect a behavioral change.
                if canonical_examples_comparable(old_class.examples.first(), new_class.examples.first())
                {
                    if !postconditions_equal_ignoring_nondeterminism(
                        &old_class.postcondition,
                        &new_class.postcondition,
                        &nondet_fields,
                    ) {
                        changed_postconditions.push(PostconditionChange {
                            class_label: old_class.label.clone(),
                            old: old_class.postcondition.clone(),
                            new: new_class.postcondition.clone(),
                        });
                    }
                } else {
                    comparison_notes.push(ComparisonNote {
                        class_label: old_class.label.clone(),
                        reason: ComparisonNoteReason::MissingComparableWitness,
                    });
                }

                // Compare preconditions using Vec-based set difference
                // (Precondition contains serde_json::Value which doesn't impl Hash).
                let removed: Vec<_> = old_class
                    .preconditions
                    .iter()
                    .filter(|p| !new_class.preconditions.contains(p))
                    .cloned()
                    .collect();
                let added: Vec<_> = new_class
                    .preconditions
                    .iter()
                    .filter(|p| !old_class.preconditions.contains(p))
                    .cloned()
                    .collect();

                if !removed.is_empty() || !added.is_empty() {
                    changed_preconditions.push(PreconditionChange {
                        class_label: old_class.label.clone(),
                        removed,
                        added,
                    });
                }
            }
            None => {
                added_classes.push(new_class.clone());
            }
        }
    }

    // Find removed classes (in old but not in new).
    for old_class in &old.classes {
        if !new_by_path.contains_key(&old_class.branch_path) {
            removed_classes.push(old_class.clone());
        }
    }

    // Detect lost properties: invariant observations that no longer hold.
    // An invariant is "all classes throw" or "all classes return".
    let old_all_throw = !old.classes.is_empty()
        && old
            .classes
            .iter()
            .all(|c| matches!(c.postcondition, Postcondition::Throws { .. }));
    let new_all_throw = !new.classes.is_empty()
        && new
            .classes
            .iter()
            .all(|c| matches!(c.postcondition, Postcondition::Throws { .. }));

    if old_all_throw && !new_all_throw && !new.classes.is_empty() {
        lost_properties.push("all paths throw an error".to_string());
    }

    let old_all_return = !old.classes.is_empty()
        && old.classes.iter().all(|c| {
            matches!(
                c.postcondition,
                Postcondition::Returns { .. } | Postcondition::ReturnsVoid
            )
        });
    let new_all_return = !new.classes.is_empty()
        && new.classes.iter().all(|c| {
            matches!(
                c.postcondition,
                Postcondition::Returns { .. } | Postcondition::ReturnsVoid
            )
        });

    if old_all_return && !new_all_return && !new.classes.is_empty() {
        lost_properties.push("all paths return without error".to_string());
    }

    // Check if coverage dropped significantly.
    if old.total_lines > 0 && new.total_lines > 0 {
        let old_pct = old.lines_covered as f64 / old.total_lines as f64;
        let new_pct = new.lines_covered as f64 / new.total_lines as f64;
        if old_pct - new_pct > 0.1 {
            lost_properties.push(format!(
                "line coverage dropped from {:.0}% to {:.0}%",
                old_pct * 100.0,
                new_pct * 100.0,
            ));
        }
    }

    SpecDiff {
        function_name: old.function_name.clone(),
        added_classes,
        removed_classes,
        changed_postconditions,
        changed_preconditions,
        lost_properties,
        comparison_notes,
    }
}

/// Whether two classes' canonical examples provide a valid basis for
/// comparing their postconditions.
///
/// Both sides must have at least one example, and their full input vectors
/// must be structurally equal (`serde_json::Value` equality: object key
/// order is irrelevant, argument/array order and missing-vs-null are
/// significant). A later example matching across specs is never substituted
/// for the canonical one — that would let an unrelated pair of postconditions
/// borrow comparability from a different, unreported input.
fn canonical_examples_comparable(
    old_example: Option<&ConcreteExample>,
    new_example: Option<&ConcreteExample>,
) -> bool {
    match (old_example, new_example) {
        (Some(old_example), Some(new_example)) => old_example.inputs == new_example.inputs,
        _ => false,
    }
}

/// Format a spec diff as human-readable text.
pub fn format_spec_diff_text(diff: &SpecDiff) -> String {
    let mut out = String::new();

    out.push_str(&format!("Spec diff: {}\n\n", diff.function_name));

    if diff.is_empty() {
        out.push_str("  No changes detected.\n");
        return out;
    }

    // Summary line
    let mut parts = Vec::new();
    if !diff.added_classes.is_empty() {
        parts.push(format!("{} added", diff.added_classes.len()));
    }
    if !diff.removed_classes.is_empty() {
        parts.push(format!("{} removed", diff.removed_classes.len()));
    }
    if !diff.changed_postconditions.is_empty() {
        parts.push(format!(
            "{} postcondition(s) changed",
            diff.changed_postconditions.len()
        ));
    }
    if !diff.changed_preconditions.is_empty() {
        parts.push(format!(
            "{} precondition(s) changed",
            diff.changed_preconditions.len()
        ));
    }
    if !diff.lost_properties.is_empty() {
        parts.push(format!("{} property/ies lost", diff.lost_properties.len()));
    }
    if !diff.comparison_notes.is_empty() {
        parts.push(format!(
            "{} class(es) with insufficient comparison evidence",
            diff.comparison_notes.len()
        ));
    }
    out.push_str(&format!("  Summary: {}\n\n", parts.join(", ")));

    // Added classes
    for class in &diff.added_classes {
        out.push_str(&format!("  [ADDED]   {}\n", class.label));
    }

    // Removed classes
    for class in &diff.removed_classes {
        out.push_str(&format!("  [REMOVED] {}\n", class.label));
    }

    // Changed postconditions
    for change in &diff.changed_postconditions {
        out.push_str(&format!("  [CHANGED] {}\n", change.class_label));
        out.push_str(&format!(
            "            old: {}\n",
            format_postcondition_short(&change.old)
        ));
        out.push_str(&format!(
            "            new: {}\n",
            format_postcondition_short(&change.new)
        ));
    }

    // Changed preconditions
    for change in &diff.changed_preconditions {
        out.push_str(&format!("  [PRECOND] {}\n", change.class_label));
        for removed in &change.removed {
            out.push_str(&format!(
                "            - {}\n",
                format_precondition_short(removed)
            ));
        }
        for added in &change.added {
            out.push_str(&format!(
                "            + {}\n",
                format_precondition_short(added)
            ));
        }
    }

    // Lost properties
    for prop in &diff.lost_properties {
        out.push_str(&format!("  [LOST]    {prop}\n"));
    }

    // Comparison notes: insufficient evidence, not a confirmed change.
    for note in &diff.comparison_notes {
        out.push_str(&format!(
            "  [INCONCLUSIVE] {}: insufficient comparison evidence ({})\n",
            note.class_label,
            format_comparison_note_reason(note.reason)
        ));
    }

    out
}

fn format_comparison_note_reason(reason: ComparisonNoteReason) -> &'static str {
    match reason {
        ComparisonNoteReason::MissingComparableWitness => {
            "no comparable recorded example on both sides"
        }
    }
}

/// Format a spec diff as machine-readable JSON.
pub fn format_spec_diff_json(diff: &SpecDiff) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(diff)
}

/// Compare two postconditions, treating fields listed as nondeterministic as equal.
///
/// For `Returns` variants, if the entire return value is nondeterministic (field path
/// "return"), they're always equal. Otherwise, JSON values are compared structurally
/// with nondeterministic sub-paths ignored. `Throws` and `ReturnsVoid` use normal equality.
fn postconditions_equal_ignoring_nondeterminism(
    old: &Postcondition,
    new: &Postcondition,
    nondet_fields: &[&NondeterministicField],
) -> bool {
    match (old, new) {
        (Postcondition::Returns { value: old_val }, Postcondition::Returns { value: new_val }) => {
            // If the entire return is nondeterministic, skip comparison.
            if nondet_fields.iter().any(|f| f.field_path == "return") {
                return true;
            }

            // Collect return sub-field paths (e.g. "return.timestamp" → "timestamp").
            let return_nondet_paths: Vec<&str> = nondet_fields
                .iter()
                .filter_map(|f| f.field_path.strip_prefix("return."))
                .collect();

            if return_nondet_paths.is_empty() {
                old_val == new_val
            } else {
                json_equal_ignoring_paths(old_val, new_val, &return_nondet_paths, "")
            }
        }
        _ => old == new,
    }
}

/// Recursively compare two JSON values, skipping fields whose dot-path is in `skip_paths`.
fn json_equal_ignoring_paths(
    a: &serde_json::Value,
    b: &serde_json::Value,
    skip_paths: &[&str],
    current_path: &str,
) -> bool {
    use serde_json::Value;

    // Check if the current path should be skipped.
    if !current_path.is_empty() && skip_paths.contains(&current_path) {
        return true;
    }

    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            // Both must have the same keys (ignoring skipped ones).
            let a_keys: std::collections::HashSet<_> = a_map.keys().collect();
            let b_keys: std::collections::HashSet<_> = b_map.keys().collect();

            for key in a_keys.union(&b_keys) {
                let child_path = if current_path.is_empty() {
                    (*key).clone()
                } else {
                    format!("{current_path}.{key}")
                };

                if skip_paths.contains(&child_path.as_str()) {
                    continue;
                }

                match (a_map.get(*key), b_map.get(*key)) {
                    (Some(av), Some(bv)) => {
                        if !json_equal_ignoring_paths(av, bv, skip_paths, &child_path) {
                            return false;
                        }
                    }
                    // Key missing from one side and not skipped → not equal.
                    _ => return false,
                }
            }
            true
        }
        (Value::Array(a_arr), Value::Array(b_arr)) => {
            if a_arr.len() != b_arr.len() {
                return false;
            }
            a_arr
                .iter()
                .zip(b_arr.iter())
                .all(|(av, bv)| json_equal_ignoring_paths(av, bv, skip_paths, current_path))
        }
        _ => a == b,
    }
}

fn format_postcondition_short(post: &Postcondition) -> String {
    match post {
        Postcondition::Returns { value } => {
            let s = value.to_string();
            if s.len() > 40 {
                format!("returns {}...", &s[..37])
            } else {
                format!("returns {s}")
            }
        }
        Postcondition::Throws { error } => {
            format!("throws {}: {}", error.error_type, error.message)
        }
        Postcondition::ReturnsVoid => "returns void".to_string(),
        Postcondition::Unobserved => "no return value observed".to_string(),
    }
}

fn format_precondition_short(pre: &Precondition) -> String {
    match pre {
        Precondition::AllPositive { param_index } => format!("param[{param_index}] > 0"),
        Precondition::AllNegative { param_index } => format!("param[{param_index}] < 0"),
        Precondition::AllZero { param_index } => format!("param[{param_index}] == 0"),
        Precondition::AllEqual { param_index, value } => {
            format!("param[{param_index}] == {value}")
        }
        Precondition::SameType {
            param_index,
            type_name,
        } => {
            format!("typeof param[{param_index}] == \"{type_name}\"")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equivalence::{BranchPath, BranchStep};
    use crate::execution_record::ErrorInfo;
    use crate::spec::{ConcreteExample, Provenance};
    use serde_json::json;

    fn make_spec(name: &str, classes: Vec<SpecClass>) -> FunctionSpec {
        FunctionSpec {
            function_name: name.to_string(),
            location: Some("test.ts:1".to_string()),
            classes,
            iterations: 50,
            lines_covered: 8,
            total_lines: 10,
            invariants: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    fn make_class(
        label: &str,
        branch_steps: Vec<(u32, bool)>,
        preconditions: Vec<Precondition>,
        postcondition: Postcondition,
    ) -> SpecClass {
        let branch_path = BranchPath(
            branch_steps
                .into_iter()
                .map(|(id, taken)| BranchStep {
                    branch_id: id,
                    taken,
                })
                .collect(),
        );
        SpecClass {
            label: label.to_string(),
            branch_path,
            preconditions,
            postcondition,
            side_effects: vec![],
            examples: vec![ConcreteExample {
                inputs: vec![json!(1)],
                return_value: Some(json!(1)),
                thrown_error: None,
            }],
            sample_count: 5,
            precondition_provenance: Provenance::Observed,
            postcondition_provenance: Provenance::Observed,
            invariants: vec![],
        }
    }

    #[test]
    fn identical_specs_produce_empty_diff() {
        let spec = make_spec(
            "classify",
            vec![
                make_class(
                    "Class 1 — returns positive",
                    vec![(0, true)],
                    vec![Precondition::AllPositive { param_index: 0 }],
                    Postcondition::Returns {
                        value: json!("positive"),
                    },
                ),
                make_class(
                    "Class 2 — returns negative",
                    vec![(0, false)],
                    vec![Precondition::AllNegative { param_index: 0 }],
                    Postcondition::Returns {
                        value: json!("negative"),
                    },
                ),
            ],
        );

        let diff = diff_specs(&spec, &spec);
        assert!(diff.is_empty());
        assert!(!diff.has_regressions());
    }

    #[test]
    fn added_class_detected() {
        let old = make_spec(
            "classify",
            vec![make_class(
                "Class 1 — returns positive",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );
        let new = make_spec(
            "classify",
            vec![
                make_class(
                    "Class 1 — returns positive",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("positive"),
                    },
                ),
                make_class(
                    "Class 2 — returns negative",
                    vec![(0, false)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("negative"),
                    },
                ),
            ],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.added_classes.len(), 1);
        assert_eq!(diff.added_classes[0].label, "Class 2 — returns negative");
        assert!(diff.removed_classes.is_empty());
    }

    #[test]
    fn removed_class_detected() {
        let old = make_spec(
            "classify",
            vec![
                make_class(
                    "Class 1 — returns positive",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("positive"),
                    },
                ),
                make_class(
                    "Class 2 — returns negative",
                    vec![(0, false)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("negative"),
                    },
                ),
            ],
        );
        let new = make_spec(
            "classify",
            vec![make_class(
                "Class 1 — returns positive",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert!(diff.added_classes.is_empty());
        assert_eq!(diff.removed_classes.len(), 1);
        assert_eq!(diff.removed_classes[0].label, "Class 2 — returns negative");
        assert!(diff.has_regressions());
    }

    #[test]
    fn changed_postcondition_return_value() {
        let old = make_spec(
            "compute",
            vec![make_class(
                "Class 1 — returns 42",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(42) },
            )],
        );
        let new = make_spec(
            "compute",
            vec![make_class(
                "Class 1 — returns 99",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(99) },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.changed_postconditions.len(), 1);
        assert_eq!(
            diff.changed_postconditions[0].old,
            Postcondition::Returns { value: json!(42) }
        );
        assert_eq!(
            diff.changed_postconditions[0].new,
            Postcondition::Returns { value: json!(99) }
        );
        assert!(diff.has_regressions());
    }

    #[test]
    fn changed_postcondition_error_to_return() {
        let old = make_spec(
            "process",
            vec![make_class(
                "Class 1 — throws ValidationError",
                vec![(0, true)],
                vec![],
                Postcondition::Throws {
                    error: ErrorInfo {
                        error_type: "ValidationError".to_string(),
                        message: "invalid input".to_string(),
                        stack: None,
                        error_category: None,
                    },
                },
            )],
        );
        let new = make_spec(
            "process",
            vec![make_class(
                "Class 1 — returns null",
                vec![(0, true)],
                vec![],
                Postcondition::ReturnsVoid,
            )],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.changed_postconditions.len(), 1);
        assert!(matches!(
            diff.changed_postconditions[0].old,
            Postcondition::Throws { .. }
        ));
        assert_eq!(
            diff.changed_postconditions[0].new,
            Postcondition::ReturnsVoid
        );
        assert!(diff.has_regressions());
    }

    #[test]
    fn precondition_widened() {
        let old = make_spec(
            "classify",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![
                    Precondition::AllPositive { param_index: 0 },
                    Precondition::SameType {
                        param_index: 0,
                        type_name: "number".to_string(),
                    },
                ],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );
        // New spec only requires SameType (widened — AllPositive dropped)
        let new = make_spec(
            "classify",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![Precondition::SameType {
                    param_index: 0,
                    type_name: "number".to_string(),
                }],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.changed_preconditions.len(), 1);
        assert_eq!(diff.changed_preconditions[0].removed.len(), 1);
        assert_eq!(
            diff.changed_preconditions[0].removed[0],
            Precondition::AllPositive { param_index: 0 }
        );
        assert!(diff.changed_preconditions[0].added.is_empty());
    }

    #[test]
    fn precondition_narrowed() {
        let old = make_spec(
            "classify",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![Precondition::SameType {
                    param_index: 0,
                    type_name: "number".to_string(),
                }],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );
        // New spec adds AllPositive (narrowed)
        let new = make_spec(
            "classify",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![
                    Precondition::SameType {
                        param_index: 0,
                        type_name: "number".to_string(),
                    },
                    Precondition::AllPositive { param_index: 0 },
                ],
                Postcondition::Returns {
                    value: json!("positive"),
                },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.changed_preconditions.len(), 1);
        assert!(diff.changed_preconditions[0].removed.is_empty());
        assert_eq!(diff.changed_preconditions[0].added.len(), 1);
        assert_eq!(
            diff.changed_preconditions[0].added[0],
            Precondition::AllPositive { param_index: 0 }
        );
    }

    #[test]
    fn json_output_round_trips() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );

        let diff = diff_specs(&old, &new);
        let json_str = format_spec_diff_json(&diff).expect("json serialization");
        let deserialized: SpecDiff = serde_json::from_str(&json_str).expect("json deserialization");
        assert_eq!(diff, deserialized);
    }

    #[test]
    fn format_text_produces_readable_output() {
        let old = make_spec(
            "classify",
            vec![
                make_class(
                    "Class 1 — returns positive",
                    vec![(0, true)],
                    vec![Precondition::AllPositive { param_index: 0 }],
                    Postcondition::Returns {
                        value: json!("positive"),
                    },
                ),
                make_class(
                    "Class 2 — returns zero",
                    vec![(0, false), (1, false)],
                    vec![Precondition::AllZero { param_index: 0 }],
                    Postcondition::Returns {
                        value: json!("zero"),
                    },
                ),
            ],
        );
        let new = make_spec(
            "classify",
            vec![
                make_class(
                    "Class 1 — returns positive",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("pos"),
                    },
                ),
                make_class(
                    "Class 3 — returns negative",
                    vec![(0, false), (1, true)],
                    vec![],
                    Postcondition::Returns {
                        value: json!("negative"),
                    },
                ),
            ],
        );

        let diff = diff_specs(&old, &new);
        let text = format_spec_diff_text(&diff);

        assert!(text.contains("Spec diff: classify"));
        assert!(text.contains("[ADDED]"));
        assert!(text.contains("[REMOVED]"));
        assert!(text.contains("[CHANGED]"));
        assert!(
            text.contains("returns \"positive\""),
            "should show old postcondition"
        );
        assert!(
            text.contains("returns \"pos\""),
            "should show new postcondition"
        );
        assert!(
            text.contains("[PRECOND]"),
            "should show precondition change"
        );
    }

    #[test]
    fn empty_specs_produce_empty_diff() {
        let old = make_spec("fn1", vec![]);
        let new = make_spec("fn1", vec![]);

        let diff = diff_specs(&old, &new);
        assert!(diff.is_empty());
    }

    #[test]
    fn lost_property_all_return_to_mixed() {
        let old = make_spec(
            "fn1",
            vec![
                make_class(
                    "Class 1",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns { value: json!(1) },
                ),
                make_class(
                    "Class 2",
                    vec![(0, false)],
                    vec![],
                    Postcondition::Returns { value: json!(2) },
                ),
            ],
        );
        let new = make_spec(
            "fn1",
            vec![
                make_class(
                    "Class 1",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns { value: json!(1) },
                ),
                make_class(
                    "Class 2",
                    vec![(0, false)],
                    vec![],
                    Postcondition::Throws {
                        error: ErrorInfo {
                            error_type: "Error".to_string(),
                            message: "boom".to_string(),
                            stack: None,
                            error_category: None,
                        },
                    },
                ),
            ],
        );

        let diff = diff_specs(&old, &new);
        assert!(
            diff.lost_properties
                .contains(&"all paths return without error".to_string()),
            "should detect lost all-return property, got: {:?}",
            diff.lost_properties
        );
    }

    #[test]
    fn coverage_drop_detected() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.lines_covered = 9;
        old.total_lines = 10;

        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        new.lines_covered = 5;
        new.total_lines = 10;

        let diff = diff_specs(&old, &new);
        assert!(
            diff.lost_properties
                .iter()
                .any(|p| p.contains("coverage dropped")),
            "should detect coverage drop, got: {:?}",
            diff.lost_properties
        );
    }

    #[test]
    fn format_text_empty_diff() {
        let spec = make_spec("fn1", vec![]);
        let diff = diff_specs(&spec, &spec);
        let text = format_spec_diff_text(&diff);
        assert!(text.contains("No changes detected"));
    }

    // -- Nondeterminism-aware postcondition comparison tests --

    use crate::nondeterminism::{Confidence, NondeterminismEvidence, NondeterministicField};

    #[test]
    fn nondeterministic_field_excludes_postcondition_diff() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!({"id": 1, "timestamp": 1000}),
                },
            )],
        );
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!({"id": 1, "timestamp": 2000}),
                },
            )],
        );
        // Mark "return.timestamp" as nondeterministic.
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.timestamp".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert!(
            diff.changed_postconditions.is_empty(),
            "nondeterministic field should be excluded from comparison"
        );
    }

    #[test]
    fn deterministic_field_still_reports_postcondition_diff() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!({"id": 1, "name": "alice"}),
                },
            )],
        );
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!({"id": 1, "name": "bob"}),
                },
            )],
        );
        // Only "return.timestamp" is nondeterministic — "name" is not.
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.timestamp".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert_eq!(
            diff.changed_postconditions.len(),
            1,
            "deterministic field change should still be reported"
        );
    }

    #[test]
    fn whole_return_nondeterministic_skips_comparison() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(42) },
            )],
        );
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(99) },
            )],
        );
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert!(
            diff.changed_postconditions.is_empty(),
            "whole return marked nondeterministic should skip comparison"
        );
    }

    #[test]
    fn mixed_nondeterministic_and_deterministic_changes() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    value: json!({"id": 1, "ts": 100, "status": "ok"}),
                },
            )],
        );
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns {
                    // Both ts (nondeterministic) AND status (deterministic) changed.
                    value: json!({"id": 1, "ts": 200, "status": "error"}),
                },
            )],
        );
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.ts".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert_eq!(
            diff.changed_postconditions.len(),
            1,
            "deterministic change in 'status' should still be reported"
        );
    }

    #[test]
    fn throws_postcondition_not_affected_by_return_nondeterminism() {
        let old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Throws {
                    error: ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "old".to_string(),
                        stack: None,
                        error_category: None,
                    },
                },
            )],
        );
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Throws {
                    error: ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "new".to_string(),
                        stack: None,
                        error_category: None,
                    },
                },
            )],
        );
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.timestamp".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert_eq!(
            diff.changed_postconditions.len(),
            1,
            "throws comparison should not be affected by return nondeterminism"
        );
    }

    // -- str-nfg4y: canonical-input comparability guard --

    use crate::equivalence::group_into_classes;
    use crate::execution_record::BranchDecision;
    use crate::explorer::ObservationOutput;
    use crate::protocol::{ExecuteResult, MockConfig};
    use crate::spec::build_spec;
    use crate::types::TypeInfo;

    /// Build a `FunctionSpec` for a single-branch-path pure function from
    /// `(input, output)` pairs, using the real `group_into_classes ->
    /// build_spec` pipeline — the same reproduction recipe str-nfg4y used.
    fn spec_from_observations(function_name: &str, pairs: &[(i64, i64)]) -> FunctionSpec {
        let executions: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)> = pairs
            .iter()
            .map(|(input, output)| {
                let result = ExecuteResult {
                    return_value: Some(json!(output)),
                    branch_path: vec![BranchDecision {
                        branch_id: 1,
                        line: 1,
                        taken: true,
                        constraint: Default::default(),
                        conditions: None,
                    }],
                    lines_executed: vec![1],
                    ..Default::default()
                };
                (vec![json!(input)], vec![], result)
            })
            .collect();

        let classes = group_into_classes(&executions);
        let observation = ObservationOutput {
            function_name: function_name.to_string(),
            iterations: pairs.len() as u32,
            unique_paths: 1,
            lines_covered: 1,
            total_lines: 1,
            ..Default::default()
        };

        build_spec(
            &observation,
            &classes,
            None,
            None,
            &TypeInfo::Int {
                int_width: None,
                int_signed: None,
            },
        )
    }

    fn round_trip(spec: &FunctionSpec) -> FunctionSpec {
        let json = serde_json::to_string(spec).expect("serialize spec");
        serde_json::from_str(&json).expect("deserialize spec")
    }

    #[test]
    fn population_shift_identity_function_is_not_a_regression() {
        // Baseline observes (1 -> 1), (2 -> 2); pick_simplest selects [1] as
        // canonical. Candidate observes only (2 -> 2); canonical becomes
        // [2]. The identity function's behavior did not change, but naive
        // postcondition comparison would report 1 -> 2 as a changed return.
        let old = round_trip(&spec_from_observations("identity", &[(1, 1), (2, 2)]));
        let new = round_trip(&spec_from_observations("identity", &[(2, 2)]));

        let diff = diff_specs(&old, &new);
        assert!(
            diff.changed_postconditions.is_empty(),
            "unmatched canonical inputs must not produce a changed-postcondition \
             regression, got: {:?}",
            diff.changed_postconditions
        );
        assert!(!diff.has_regressions());
        assert_eq!(diff.comparison_notes.len(), 1);
        assert_eq!(
            diff.comparison_notes[0].reason,
            ComparisonNoteReason::MissingComparableWitness
        );
    }

    #[test]
    fn population_shift_identity_function_is_not_a_regression_reversed() {
        let old = round_trip(&spec_from_observations("identity", &[(2, 2)]));
        let new = round_trip(&spec_from_observations("identity", &[(1, 1), (2, 2)]));

        let diff = diff_specs(&old, &new);
        assert!(diff.changed_postconditions.is_empty());
        assert!(!diff.has_regressions());
        assert_eq!(diff.comparison_notes.len(), 1);
    }

    #[test]
    fn same_canonical_input_changed_output_is_still_a_regression() {
        let old = round_trip(&spec_from_observations("identity", &[(1, 1)]));
        let new = round_trip(&spec_from_observations("identity", &[(1, 2)]));

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.changed_postconditions.len(), 1);
        assert!(diff.comparison_notes.is_empty());
        assert!(diff.has_regressions());
    }

    #[test]
    fn missing_example_on_either_side_generates_a_note() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples.clear();
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert!(diff.changed_postconditions.is_empty());
        assert_eq!(diff.comparison_notes.len(), 1);
    }

    #[test]
    fn missing_example_on_both_sides_generates_a_note() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples.clear();
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );
        new.classes[0].examples.clear();

        let diff = diff_specs(&old, &new);
        assert!(diff.changed_postconditions.is_empty());
        assert_eq!(diff.comparison_notes.len(), 1);
    }

    #[test]
    fn two_present_examples_with_empty_inputs_remain_comparable() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples = vec![ConcreteExample {
            inputs: vec![],
            return_value: Some(json!(1)),
            thrown_error: None,
        }];
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );
        new.classes[0].examples = vec![ConcreteExample {
            inputs: vec![],
            return_value: Some(json!(2)),
            thrown_error: None,
        }];

        let diff = diff_specs(&old, &new);
        assert_eq!(
            diff.changed_postconditions.len(),
            1,
            "two present zero-argument examples must be treated as comparable"
        );
        assert!(diff.comparison_notes.is_empty());
    }

    #[test]
    fn object_key_order_does_not_affect_comparability() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!({"a": 1, "b": 2})],
            return_value: Some(json!(1)),
            thrown_error: None,
        }];
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );
        new.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!({"b": 2, "a": 1})],
            return_value: Some(json!(2)),
            thrown_error: None,
        }];

        let diff = diff_specs(&old, &new);
        assert_eq!(
            diff.changed_postconditions.len(),
            1,
            "object key insertion order must not affect comparability"
        );
        assert!(diff.comparison_notes.is_empty());
    }

    #[test]
    fn argument_order_affects_comparability() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!(1), json!(2)],
            return_value: Some(json!(1)),
            thrown_error: None,
        }];
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );
        new.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!(2), json!(1)],
            return_value: Some(json!(2)),
            thrown_error: None,
        }];

        let diff = diff_specs(&old, &new);
        assert!(
            diff.changed_postconditions.is_empty(),
            "swapped argument order must not be treated as comparable"
        );
        assert_eq!(diff.comparison_notes.len(), 1);
    }

    #[test]
    fn note_only_diff_is_not_empty_but_has_no_regressions() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples.clear();
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert!(
            !diff.is_empty(),
            "a note must not be reported as an empty diff"
        );
        assert!(!diff.has_regressions(), "a note alone is not a regression");
    }

    #[test]
    fn note_coexists_with_independent_regressions() {
        let mut old = make_spec(
            "fn1",
            vec![
                make_class(
                    "Class 1",
                    vec![(0, true)],
                    vec![],
                    Postcondition::Returns { value: json!(1) },
                ),
                make_class(
                    "Class 2",
                    vec![(0, false)],
                    vec![],
                    Postcondition::Returns { value: json!(9) },
                ),
            ],
        );
        old.classes[0].examples.clear();
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );

        let diff = diff_specs(&old, &new);
        assert_eq!(diff.comparison_notes.len(), 1);
        assert_eq!(diff.removed_classes.len(), 1);
        assert!(
            diff.has_regressions(),
            "an independent removed-class regression must not be suppressed by a note"
        );
    }

    #[test]
    fn comparison_note_json_round_trips() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples.clear();
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );

        let diff = diff_specs(&old, &new);
        let json_str = format_spec_diff_json(&diff).expect("json serialization");
        assert!(json_str.contains("comparison_notes"));
        assert!(json_str.contains("missing_comparable_witness"));
        let deserialized: SpecDiff =
            serde_json::from_str(&json_str).expect("json deserialization");
        assert_eq!(diff, deserialized);
    }

    #[test]
    fn legacy_json_without_comparison_notes_field_deserializes_with_empty_notes() {
        let legacy_json = r#"{
            "function_name": "fn1",
            "added_classes": [],
            "removed_classes": [],
            "changed_postconditions": [],
            "changed_preconditions": [],
            "lost_properties": []
        }"#;
        let diff: SpecDiff =
            serde_json::from_str(legacy_json).expect("legacy diff must deserialize");
        assert!(diff.comparison_notes.is_empty());
        assert!(diff.is_empty());
    }

    #[test]
    fn note_only_text_reports_inconclusive_not_no_changes() {
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples.clear();
        let new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );

        let diff = diff_specs(&old, &new);
        let text = format_spec_diff_text(&diff);
        assert!(!text.contains("No changes detected"));
        assert!(text.contains("[INCONCLUSIVE]"));
        assert!(text.contains("insufficient comparison evidence"));
    }

    #[test]
    fn masks_do_not_establish_comparability_for_unequal_inputs() {
        // A whole-return nondeterminism mask must not let an uncomparable
        // pair slip through as "equal" instead of being reported as a note:
        // the guard runs before the mask-aware comparison, not instead of it.
        let mut old = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(1) },
            )],
        );
        old.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!(1)],
            return_value: Some(json!(1)),
            thrown_error: None,
        }];
        let mut new = make_spec(
            "fn1",
            vec![make_class(
                "Class 1",
                vec![(0, true)],
                vec![],
                Postcondition::Returns { value: json!(2) },
            )],
        );
        new.classes[0].examples = vec![ConcreteExample {
            inputs: vec![json!(2)],
            return_value: Some(json!(2)),
            thrown_error: None,
        }];
        new.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];

        let diff = diff_specs(&old, &new);
        assert!(diff.changed_postconditions.is_empty());
        assert_eq!(
            diff.comparison_notes.len(),
            1,
            "mismatched canonical inputs must produce a note even with a \
             whole-return mask present"
        );
    }

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::arb_json_value;
        use proptest::prelude::*;

        fn class_with_example(
            label: &str,
            postcondition: Postcondition,
            inputs: Vec<serde_json::Value>,
        ) -> SpecClass {
            let mut class = make_class(label, vec![(0, true)], vec![], postcondition);
            class.examples = vec![ConcreteExample {
                inputs,
                return_value: None,
                thrown_error: None,
            }];
            class
        }

        proptest! {
            /// Swapping which side is "old" and which is "new" never changes
            /// whether a pair of canonical examples is judged comparable.
            #[test]
            fn comparability_is_symmetric(
                old_inputs in prop::collection::vec(arb_json_value(), 0..=3),
                new_inputs in prop::collection::vec(arb_json_value(), 0..=3),
            ) {
                let old = ConcreteExample { inputs: old_inputs, return_value: None, thrown_error: None };
                let new = ConcreteExample { inputs: new_inputs, return_value: None, thrown_error: None };
                prop_assert_eq!(
                    canonical_examples_comparable(Some(&old), Some(&new)),
                    canonical_examples_comparable(Some(&new), Some(&old))
                );
            }

            /// Identical canonical input vectors are always comparable,
            /// regardless of the arbitrary JSON values involved.
            #[test]
            fn identical_inputs_are_always_comparable(
                inputs in prop::collection::vec(arb_json_value(), 0..=4),
            ) {
                let old = ConcreteExample { inputs: inputs.clone(), return_value: None, thrown_error: None };
                let new = ConcreteExample { inputs, return_value: None, thrown_error: None };
                prop_assert!(canonical_examples_comparable(Some(&old), Some(&new)));
            }

            /// A missing canonical example on either side can never be
            /// comparable, no matter what the other side's inputs are.
            #[test]
            fn missing_example_is_never_comparable(
                inputs in prop::collection::vec(arb_json_value(), 0..=4),
            ) {
                let present = ConcreteExample { inputs, return_value: None, thrown_error: None };
                prop_assert!(!canonical_examples_comparable(None, Some(&present)));
                prop_assert!(!canonical_examples_comparable(Some(&present), None));
                prop_assert!(!canonical_examples_comparable(None, None));
            }

            /// Swapping old/new preserves the resulting comparability
            /// classification (note vs. compared) through the full
            /// `diff_specs` pipeline, for arbitrary same/different postcondition
            /// values and arbitrary canonical inputs.
            #[test]
            fn swapping_old_new_preserves_comparability_classification(
                old_inputs in prop::collection::vec(arb_json_value(), 0..=3),
                new_inputs in prop::collection::vec(arb_json_value(), 0..=3),
                old_return in arb_json_value(),
                new_return in arb_json_value(),
            ) {
                let old_spec = make_spec(
                    "fn1",
                    vec![class_with_example(
                        "Class 1",
                        Postcondition::Returns { value: old_return },
                        old_inputs.clone(),
                    )],
                );
                let new_spec = make_spec(
                    "fn1",
                    vec![class_with_example(
                        "Class 1",
                        Postcondition::Returns { value: new_return },
                        new_inputs.clone(),
                    )],
                );

                let forward = diff_specs(&old_spec, &new_spec);
                let backward = diff_specs(&new_spec, &old_spec);

                let forward_is_note = !forward.comparison_notes.is_empty();
                let backward_is_note = !backward.comparison_notes.is_empty();
                prop_assert_eq!(forward_is_note, backward_is_note);

                let should_be_comparable = old_inputs == new_inputs;
                prop_assert_eq!(forward_is_note, !should_be_comparable);
            }

            /// A note-bearing diff always round-trips through JSON with its
            /// structured reason intact.
            #[test]
            fn note_bearing_diff_round_trips(
                old_inputs in prop::collection::vec(arb_json_value(), 0..=3),
                new_inputs in prop::collection::vec(arb_json_value(), 1..=3),
            ) {
                // Bias toward mismatched inputs so most cases produce a note;
                // when they happen to match, the round-trip property still holds.
                let old_spec = make_spec(
                    "fn1",
                    vec![class_with_example(
                        "Class 1",
                        Postcondition::Returns { value: json!(1) },
                        old_inputs,
                    )],
                );
                let new_spec = make_spec(
                    "fn1",
                    vec![class_with_example(
                        "Class 1",
                        Postcondition::Returns { value: json!(1) },
                        new_inputs,
                    )],
                );

                let diff = diff_specs(&old_spec, &new_spec);
                let json_str = format_spec_diff_json(&diff).expect("serialize");
                let round_tripped: SpecDiff = serde_json::from_str(&json_str).expect("deserialize");
                prop_assert_eq!(diff, round_tripped);
            }
        }
    }
}
