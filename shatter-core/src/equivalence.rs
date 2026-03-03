//! Behavioral equivalence classes for grouping executions by branch path.
//!
//! Executions that follow the same branch sequence belong to the same
//! equivalence class, regardless of their return values. Within each class
//! the simplest concrete example is chosen as the canonical representative,
//! and common preconditions are derived from all inputs in the class.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::execution_record::BranchDecision;
use crate::protocol::ExecuteResult;

/// An ordered sequence of branch decisions representing a unique execution path.
///
/// Two executions belong to the same equivalence class if and only if they
/// have the same `BranchPath` (same branch IDs in the same order, with the
/// same taken/not-taken decisions).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchPath(pub Vec<BranchStep>);

/// A single step in a branch path: which branch was hit and which direction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchStep {
    pub branch_id: u32,
    pub taken: bool,
}

impl BranchPath {
    /// Extract a `BranchPath` from a slice of `BranchDecision`s.
    pub fn from_decisions(decisions: &[BranchDecision]) -> Self {
        Self(
            decisions
                .iter()
                .map(|d| BranchStep {
                    branch_id: d.branch_id,
                    taken: d.taken,
                })
                .collect(),
        )
    }

    /// Number of branch steps in this path.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this path has no branch steps.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A single execution's data within an equivalence class.
#[derive(Debug, Clone)]
pub struct ClassMember {
    /// The input arguments for this execution.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error message, if the function threw.
    pub thrown_error: Option<String>,
    /// Lines executed during this call.
    pub lines_executed: Vec<u32>,
}

/// A group of executions that followed the same branch path.
#[derive(Debug, Clone)]
pub struct EquivalenceClass {
    /// The branch path shared by all members.
    pub branch_path: BranchPath,
    /// The simplest input from the class, chosen as the canonical example.
    pub canonical_example: Vec<serde_json::Value>,
    /// All inputs observed in this class.
    pub all_inputs: Vec<Vec<serde_json::Value>>,
    /// Common preconditions derived from all inputs (e.g. "x > 0").
    pub common_preconditions: Vec<Precondition>,
    /// Return value from the canonical example.
    pub canonical_return_value: Option<serde_json::Value>,
    /// Error from the canonical example, if it threw.
    pub canonical_thrown_error: Option<String>,
}

/// A precondition derived from observing all inputs in a class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Precondition {
    /// All values for this parameter were positive.
    AllPositive { param_index: usize },
    /// All values for this parameter were negative.
    AllNegative { param_index: usize },
    /// All values for this parameter were zero.
    AllZero { param_index: usize },
    /// All values for this parameter were the same constant.
    AllEqual {
        param_index: usize,
        value: serde_json::Value,
    },
    /// All values for this parameter were of the same JSON type.
    SameType {
        param_index: usize,
        type_name: String,
    },
}

/// Group a set of (inputs, ExecuteResult) pairs into equivalence classes.
///
/// Executions with the same branch path are grouped together. Within each
/// group, the simplest input is chosen as canonical and common preconditions
/// are derived.
pub fn group_into_classes(
    executions: &[(Vec<serde_json::Value>, ExecuteResult)],
) -> Vec<EquivalenceClass> {
    if executions.is_empty() {
        return Vec::new();
    }

    // Group executions by branch path.
    let mut groups: HashMap<BranchPath, Vec<ClassMember>> = HashMap::new();
    // Preserve insertion order for deterministic output.
    let mut insertion_order: Vec<BranchPath> = Vec::new();

    for (inputs, result) in executions {
        let path = BranchPath::from_decisions(&result.branch_path);
        let member = ClassMember {
            inputs: inputs.clone(),
            return_value: result.return_value.clone(),
            thrown_error: result
                .thrown_error
                .as_ref()
                .map(|e| format!("{}: {}", e.error_type, e.message)),
            lines_executed: result.lines_executed.clone(),
        };

        let members = groups.entry(path.clone()).or_default();
        if members.is_empty() {
            insertion_order.push(path);
        }
        members.push(member);
    }

    // Build equivalence classes in insertion order.
    insertion_order
        .into_iter()
        .filter_map(|path| {
            let members = groups.remove(&path)?;
            Some(build_class(path, members))
        })
        .collect()
}

/// Build a single equivalence class from its members.
fn build_class(branch_path: BranchPath, members: Vec<ClassMember>) -> EquivalenceClass {
    let all_inputs: Vec<Vec<serde_json::Value>> =
        members.iter().map(|m| m.inputs.clone()).collect();

    let canonical_idx = pick_simplest(&all_inputs);
    let canonical_example = all_inputs[canonical_idx].clone();
    let canonical_return_value = members[canonical_idx].return_value.clone();
    let canonical_thrown_error = members[canonical_idx].thrown_error.clone();

    let common_preconditions = derive_preconditions(&all_inputs);

    EquivalenceClass {
        branch_path,
        canonical_example,
        all_inputs,
        common_preconditions,
        canonical_return_value,
        canonical_thrown_error,
    }
}

/// Pick the index of the "simplest" input set.
///
/// Simplicity heuristic: prefer inputs whose JSON serialization is shortest.
/// Among ties, prefer inputs with smaller absolute numeric values.
fn pick_simplest(inputs: &[Vec<serde_json::Value>]) -> usize {
    if inputs.is_empty() {
        return 0;
    }

    inputs
        .iter()
        .enumerate()
        .min_by_key(|(_, args)| {
            let json_len: usize = args.iter().map(|v| v.to_string().len()).sum();
            let abs_sum = args.iter().map(numeric_magnitude).sum::<u64>();
            (json_len, abs_sum)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Compute a magnitude score for a JSON value, used for tie-breaking simplicity.
fn numeric_magnitude(v: &serde_json::Value) -> u64 {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.unsigned_abs()
            } else if let Some(f) = n.as_f64() {
                f.abs() as u64
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Derive common preconditions from all inputs in a class.
///
/// Examines each parameter position across all input sets and checks for
/// common patterns (all positive, all negative, all zero, all equal, same type).
fn derive_preconditions(all_inputs: &[Vec<serde_json::Value>]) -> Vec<Precondition> {
    if all_inputs.is_empty() {
        return Vec::new();
    }

    let param_count = all_inputs.iter().map(|args| args.len()).min().unwrap_or(0);
    let mut preconditions = Vec::new();

    for param_idx in 0..param_count {
        let values: Vec<&serde_json::Value> =
            all_inputs.iter().map(|args| &args[param_idx]).collect();

        if values.is_empty() {
            continue;
        }

        // Check all-equal first (strongest precondition).
        if values.len() > 1 && values.windows(2).all(|w| w[0] == w[1]) {
            preconditions.push(Precondition::AllEqual {
                param_index: param_idx,
                value: values[0].clone(),
            });
            continue;
        }

        // Check numeric preconditions.
        let all_numeric = values.iter().all(|v| v.is_number());
        if all_numeric && !values.is_empty() {
            let floats: Vec<f64> = values.iter().filter_map(|v| v.as_f64()).collect();
            if floats.len() == values.len() {
                if floats.iter().all(|&f| f == 0.0) {
                    preconditions.push(Precondition::AllZero {
                        param_index: param_idx,
                    });
                    continue;
                }
                if floats.iter().all(|&f| f > 0.0) {
                    preconditions.push(Precondition::AllPositive {
                        param_index: param_idx,
                    });
                    continue;
                }
                if floats.iter().all(|&f| f < 0.0) {
                    preconditions.push(Precondition::AllNegative {
                        param_index: param_idx,
                    });
                    continue;
                }
            }
        }

        // Check same-type precondition.
        let type_names: Vec<&str> = values.iter().map(json_type_name).collect();
        if type_names.len() > 1 && type_names.windows(2).all(|w| w[0] == w[1]) {
            preconditions.push(Precondition::SameType {
                param_index: param_idx,
                type_name: type_names[0].to_string(),
            });
        }
    }

    preconditions
}

/// Return a type name for a JSON value.
fn json_type_name(v: &&serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
    use crate::protocol::PerformanceMetrics;
    use serde_json::json;

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    fn make_branch(id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id: id,
            line: id * 10,
            taken,
            constraint: SymConstraint::Unknown {
                hint: "test".into(),
            },
        }
    }

    fn make_result(
        return_value: Option<serde_json::Value>,
        branch_path: Vec<BranchDecision>,
    ) -> ExecuteResult {
        ExecuteResult {
            return_value,
            thrown_error: None,
            branch_path,
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            capture_truncation: None,
            performance: empty_perf(),
        }
    }

    fn make_error_result(
        error_type: &str,
        message: &str,
        branch_path: Vec<BranchDecision>,
    ) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: error_type.into(),
                message: message.into(),
                stack: None, error_category: None }),
            branch_path,
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            capture_truncation: None,
            performance: empty_perf(),
        }
    }

    #[test]
    fn same_branches_different_returns_same_class() {
        let path = vec![make_branch(0, true), make_branch(1, false)];
        let executions = vec![
            (vec![json!(5)], make_result(Some(json!("five")), path.clone())),
            (vec![json!(7)], make_result(Some(json!("seven")), path.clone())),
            (vec![json!(3)], make_result(Some(json!("three")), path)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1, "all should be in one class");
        assert_eq!(classes[0].all_inputs.len(), 3);
    }

    #[test]
    fn different_branches_different_classes() {
        let path_a = vec![make_branch(0, true)];
        let path_b = vec![make_branch(0, false)];
        let executions = vec![
            (vec![json!(5)], make_result(Some(json!("pos")), path_a)),
            (vec![json!(-3)], make_result(Some(json!("neg")), path_b)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 2, "should be in two classes");
    }

    #[test]
    fn canonical_example_is_simplest_input() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (
                vec![json!(999999)],
                make_result(Some(json!("big")), path.clone()),
            ),
            (vec![json!(1)], make_result(Some(json!("small")), path.clone())),
            (
                vec![json!(50000)],
                make_result(Some(json!("medium")), path),
            ),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].canonical_example, vec![json!(1)]);
    }

    #[test]
    fn empty_executions_produce_no_classes() {
        let classes = group_into_classes(&[]);
        assert!(classes.is_empty());
    }

    #[test]
    fn single_execution_produces_one_class() {
        let path = vec![make_branch(0, true)];
        let executions = vec![(vec![json!(42)], make_result(Some(json!(84)), path))];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].canonical_example, vec![json!(42)]);
        assert_eq!(classes[0].canonical_return_value, Some(json!(84)));
        assert_eq!(classes[0].all_inputs.len(), 1);
    }

    #[test]
    fn preconditions_all_positive() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (vec![json!(5)], make_result(Some(json!("a")), path.clone())),
            (vec![json!(10)], make_result(Some(json!("b")), path.clone())),
            (vec![json!(3)], make_result(Some(json!("c")), path)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(
            classes[0]
                .common_preconditions
                .contains(&Precondition::AllPositive { param_index: 0 }),
            "expected AllPositive precondition, got: {:?}",
            classes[0].common_preconditions
        );
    }

    #[test]
    fn preconditions_all_negative() {
        let path = vec![make_branch(0, false)];
        let executions = vec![
            (vec![json!(-1)], make_result(Some(json!("a")), path.clone())),
            (vec![json!(-5)], make_result(Some(json!("b")), path.clone())),
            (vec![json!(-100)], make_result(Some(json!("c")), path)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(classes[0]
            .common_preconditions
            .contains(&Precondition::AllNegative { param_index: 0 }));
    }

    #[test]
    fn preconditions_all_equal() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (vec![json!("hello")], make_result(Some(json!(1)), path.clone())),
            (vec![json!("hello")], make_result(Some(json!(2)), path.clone())),
            (vec![json!("hello")], make_result(Some(json!(3)), path)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(classes[0]
            .common_preconditions
            .contains(&Precondition::AllEqual {
                param_index: 0,
                value: json!("hello"),
            }));
    }

    #[test]
    fn preconditions_same_type() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (
                vec![json!("alpha")],
                make_result(Some(json!(1)), path.clone()),
            ),
            (
                vec![json!("beta")],
                make_result(Some(json!(2)), path.clone()),
            ),
            (vec![json!("gamma")], make_result(Some(json!(3)), path)),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(classes[0]
            .common_preconditions
            .contains(&Precondition::SameType {
                param_index: 0,
                type_name: "string".to_string(),
            }));
    }

    #[test]
    fn error_executions_grouped_by_branch_path() {
        let path = vec![make_branch(0, true), make_branch(1, true)];
        let executions = vec![
            (
                vec![json!(0)],
                make_error_result("Error", "division by zero", path.clone()),
            ),
            (
                vec![json!(null)],
                make_error_result("TypeError", "null input", path),
            ),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1, "same branch path => same class");
        assert!(classes[0].canonical_thrown_error.is_some());
    }

    #[test]
    fn branch_path_from_decisions() {
        let decisions = vec![make_branch(0, true), make_branch(1, false)];
        let path = BranchPath::from_decisions(&decisions);
        assert_eq!(path.len(), 2);
        assert!(!path.is_empty());
        assert_eq!(path.0[0].branch_id, 0);
        assert!(path.0[0].taken);
        assert_eq!(path.0[1].branch_id, 1);
        assert!(!path.0[1].taken);
    }

    #[test]
    fn branch_path_equality() {
        let p1 = BranchPath(vec![
            BranchStep { branch_id: 0, taken: true },
            BranchStep { branch_id: 1, taken: false },
        ]);
        let p2 = BranchPath(vec![
            BranchStep { branch_id: 0, taken: true },
            BranchStep { branch_id: 1, taken: false },
        ]);
        let p3 = BranchPath(vec![
            BranchStep { branch_id: 0, taken: true },
            BranchStep { branch_id: 1, taken: true },
        ]);
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn empty_branch_path_groups_together() {
        // When frontends don't report branch_path, all executions have
        // empty branch paths and end up in one class.
        let executions = vec![
            (vec![json!(1)], make_result(Some(json!("a")), vec![])),
            (vec![json!(2)], make_result(Some(json!("b")), vec![])),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(classes[0].branch_path.is_empty());
    }

    #[test]
    fn canonical_example_prefers_shorter_json() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (
                vec![json!({"name": "very long string value here", "extra": true})],
                make_result(Some(json!(1)), path.clone()),
            ),
            (
                vec![json!({"a": 1})],
                make_result(Some(json!(2)), path),
            ),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes[0].canonical_example, vec![json!({"a": 1})]);
    }

    #[test]
    fn multiple_params_preconditions() {
        let path = vec![make_branch(0, true)];
        let executions = vec![
            (
                vec![json!(5), json!(-1)],
                make_result(Some(json!("a")), path.clone()),
            ),
            (
                vec![json!(10), json!(-2)],
                make_result(Some(json!("b")), path),
            ),
        ];

        let classes = group_into_classes(&executions);
        assert_eq!(classes.len(), 1);
        assert!(classes[0]
            .common_preconditions
            .contains(&Precondition::AllPositive { param_index: 0 }));
        assert!(classes[0]
            .common_preconditions
            .contains(&Precondition::AllNegative { param_index: 1 }));
    }

    #[test]
    fn branch_path_round_trips() {
        let path = BranchPath(vec![
            BranchStep { branch_id: 0, taken: true },
            BranchStep { branch_id: 3, taken: false },
        ]);
        let json = serde_json::to_string(&path).expect("serialize");
        let deserialized: BranchPath = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(path, deserialized);
    }

    #[test]
    fn precondition_round_trips() {
        let preconditions = vec![
            Precondition::AllPositive { param_index: 0 },
            Precondition::AllNegative { param_index: 1 },
            Precondition::AllZero { param_index: 2 },
            Precondition::AllEqual {
                param_index: 0,
                value: json!("test"),
            },
            Precondition::SameType {
                param_index: 0,
                type_name: "number".to_string(),
            },
        ];
        for p in &preconditions {
            let json = serde_json::to_string(p).expect("serialize");
            let d: Precondition = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*p, d);
        }
    }
}
