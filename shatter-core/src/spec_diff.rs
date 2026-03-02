//! Specification-level diffing: compare two [`FunctionSpec`]s to detect
//! behavioral regressions, added/removed equivalence classes, and changed
//! pre/postconditions.
//!
//! The main entry point is [`diff_specs`], which produces a [`SpecDiff`].
//! Two formatters are provided: [`format_spec_diff_text`] for human-readable
//! output and [`format_spec_diff_json`] for machine-readable JSON.

use serde::{Deserialize, Serialize};

use crate::equivalence::Precondition;
use crate::spec::{FunctionSpec, Postcondition, SpecClass};

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
    /// Whether the diff is empty (specs are equivalent).
    pub fn is_empty(&self) -> bool {
        self.added_classes.is_empty()
            && self.removed_classes.is_empty()
            && self.changed_postconditions.is_empty()
            && self.changed_preconditions.is_empty()
            && self.lost_properties.is_empty()
    }

    /// Whether the diff contains regressions (removed classes, changed
    /// postconditions, or lost properties).
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

    // Index old classes by branch path for matching.
    let old_by_path: std::collections::HashMap<_, _> = old
        .classes
        .iter()
        .map(|c| (&c.branch_path, c))
        .collect();

    let new_by_path: std::collections::HashMap<_, _> = new
        .classes
        .iter()
        .map(|c| (&c.branch_path, c))
        .collect();

    // Find matched, added, and changed classes.
    for new_class in &new.classes {
        match old_by_path.get(&new_class.branch_path) {
            Some(old_class) => {
                // Matched by branch path — compare postconditions.
                if old_class.postcondition != new_class.postcondition {
                    changed_postconditions.push(PostconditionChange {
                        class_label: old_class.label.clone(),
                        old: old_class.postcondition.clone(),
                        new: new_class.postcondition.clone(),
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
    }
}

/// Format a spec diff as human-readable text.
pub fn format_spec_diff_text(diff: &SpecDiff) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Spec diff: {}\n\n",
        diff.function_name
    ));

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
        parts.push(format!(
            "{} property/ies lost",
            diff.lost_properties.len()
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
        out.push_str(&format!(
            "  [PRECOND] {}\n",
            change.class_label
        ));
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

    out
}

/// Format a spec diff as machine-readable JSON.
pub fn format_spec_diff_json(diff: &SpecDiff) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(diff)
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
        let deserialized: SpecDiff =
            serde_json::from_str(&json_str).expect("json deserialization");
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
        assert!(text.contains("[PRECOND]"), "should show precondition change");
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
            diff.lost_properties.iter().any(|p| p.contains("coverage dropped")),
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
}
