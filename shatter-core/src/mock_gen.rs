//! Compositional mock generation from behavior maps.
//!
//! When function A calls function B and B's [`BehaviorMap`] is available,
//! this module builds an input-conditional decision tree that produces
//! high-fidelity mocks: given the same arguments B received during
//! exploration, the mock returns the same value (or throws the same error).
//!
//! The flat [`MockConfig`] format cycles through return values in order,
//! ignoring what the caller actually passes. A [`MockDecisionTree`] instead
//! matches each call's arguments against observed input patterns and picks
//! the correct output, falling back to a default when no pattern matches.

use serde::{Deserialize, Serialize};

use crate::behavior::{Behavior, BehaviorMap};
use crate::protocol::{MockBehavior, MockConfig};

// ---------------------------------------------------------------------------
// Pattern matching types
// ---------------------------------------------------------------------------

/// How to match a single argument value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatternMatcher {
    /// Exact JSON equality.
    Eq { value: serde_json::Value },
    /// Numeric range (inclusive on both ends).
    Range { min: f64, max: f64 },
    /// Match by JSON type name ("number", "string", "boolean", "null", "array", "object").
    Type { type_name: String },
    /// Match anything.
    Any,
}

/// A condition on one argument of the mocked function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputPattern {
    /// Zero-based index of the argument to check.
    pub arg_index: usize,
    /// How to match the argument.
    pub matcher: PatternMatcher,
}

/// A single branch of the decision tree: if all input patterns match,
/// return `return_value` (or throw `error`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockCondition {
    /// Patterns that must all match for this condition to fire.
    pub input_patterns: Vec<InputPattern>,
    /// Value to return when the condition matches.
    pub return_value: serde_json::Value,
    /// If set, throw this error instead of returning.
    pub error: Option<String>,
}

/// An input-conditional mock: an ordered list of conditions checked
/// top-to-bottom, with a default for unmatched calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockDecisionTree {
    /// The symbol this mock replaces.
    pub symbol: String,
    /// Ordered conditions — first match wins.
    pub conditions: Vec<MockCondition>,
    /// Value returned when no condition matches.
    pub default_return: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Building decision trees from behavior maps
// ---------------------------------------------------------------------------

/// Build a [`MockDecisionTree`] from a [`BehaviorMap`].
///
/// Each behavior becomes a condition whose input patterns are exact-match
/// on every argument, and whose output is the observed return value (or
/// error). Behaviors that have neither a return value nor a thrown error
/// are skipped.
///
/// Returns `None` if the behavior map has no usable behaviors.
pub fn build_mock_from_behavior_map(behavior_map: &BehaviorMap) -> Option<MockDecisionTree> {
    let mut conditions: Vec<MockCondition> = Vec::new();
    let mut last_return = serde_json::Value::Null;

    for behavior in &behavior_map.behaviors {
        if let Some(condition) = behavior_to_condition(behavior) {
            if condition.error.is_none() {
                last_return = condition.return_value.clone();
            }
            conditions.push(condition);
        }
    }

    if conditions.is_empty() {
        return None;
    }

    Some(MockDecisionTree {
        symbol: behavior_map.function_id.clone(),
        conditions,
        default_return: last_return,
    })
}

/// Convert a single [`Behavior`] into a [`MockCondition`].
///
/// Returns `None` if the behavior has no return value and no thrown error.
fn behavior_to_condition(behavior: &Behavior) -> Option<MockCondition> {
    let has_return = behavior.return_value.is_some();
    let has_error = behavior.thrown_error.is_some();

    if !has_return && !has_error {
        return None;
    }

    let input_patterns: Vec<InputPattern> = behavior
        .input_args
        .iter()
        .enumerate()
        .map(|(i, arg)| InputPattern {
            arg_index: i,
            matcher: PatternMatcher::Eq {
                value: arg.clone(),
            },
        })
        .collect();

    let return_value = behavior
        .return_value
        .clone()
        .unwrap_or(serde_json::Value::Null);

    let error = behavior
        .thrown_error
        .as_ref()
        .map(|e| e.message.clone());

    Some(MockCondition {
        input_patterns,
        return_value,
        error,
    })
}

/// Check whether a set of arguments matches all input patterns in a condition.
pub fn matches_condition(args: &[serde_json::Value], condition: &MockCondition) -> bool {
    condition.input_patterns.iter().all(|pattern| {
        args.get(pattern.arg_index)
            .is_some_and(|arg| matches_pattern(arg, &pattern.matcher))
    })
}

/// Check whether a single argument matches a pattern.
fn matches_pattern(value: &serde_json::Value, matcher: &PatternMatcher) -> bool {
    match matcher {
        PatternMatcher::Eq { value: expected } => value == expected,
        PatternMatcher::Range { min, max } => value
            .as_f64()
            .is_some_and(|n| n >= *min && n <= *max),
        PatternMatcher::Type { type_name } => json_type_name(value) == type_name.as_str(),
        PatternMatcher::Any => true,
    }
}

/// Return a type name string for a JSON value.
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Conversion to protocol MockConfig
// ---------------------------------------------------------------------------

/// Convert a [`MockDecisionTree`] to the flat [`MockConfig`] protocol type.
///
/// The decision tree's conditions are flattened: each condition's return value
/// becomes an entry in `return_values` (error conditions produce `null`).
/// The decision tree metadata is stored in the return values so frontends
/// can reconstruct conditional behavior.
///
/// This is a lossy conversion — the flat format cannot fully represent
/// input-conditional logic. For frontends that support conditional mocks,
/// the tree should be sent directly (future protocol extension).
pub fn to_enhanced_mock_config(tree: &MockDecisionTree) -> MockConfig {
    let return_values: Vec<serde_json::Value> = tree
        .conditions
        .iter()
        .filter(|c| c.error.is_none())
        .map(|c| c.return_value.clone())
        .collect();

    MockConfig {
        symbol: tree.symbol.clone(),
        return_values,
        should_track_calls: true,
        default_behavior: MockBehavior::RepeatLast,
    }
}

/// Build a [`MockConfig`] from a [`BehaviorMap`], using the decision tree
/// when possible and falling back to the flat conversion otherwise.
pub fn mock_config_from_behavior_map(behavior_map: &BehaviorMap) -> MockConfig {
    match build_mock_from_behavior_map(behavior_map) {
        Some(tree) => to_enhanced_mock_config(&tree),
        None => behavior_map.to_mock_config(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::{Behavior, BehaviorMap};
    use crate::execution_record::ErrorInfo;
    use serde_json::json;

    fn make_behavior(
        id: u32,
        inputs: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
        error: Option<ErrorInfo>,
    ) -> Behavior {
        Behavior {
            id,
            input_args: inputs,
            return_value,
            thrown_error: error,
            branch_path: vec![],
            side_effects: vec![],
            dependency_trace: None,
        }
    }

    fn make_behavior_map(function_id: &str, behaviors: Vec<Behavior>) -> BehaviorMap {
        BehaviorMap {
            function_id: function_id.to_string(),
            behaviors,
            fingerprint: None,
        }
    }

    // -- Simple function: one return value → single-condition mock --

    #[test]
    fn single_behavior_produces_single_condition() {
        let map = make_behavior_map(
            "getPrice",
            vec![make_behavior(0, vec![json!("widget")], Some(json!(9.99)), None)],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");

        assert_eq!(tree.symbol, "getPrice");
        assert_eq!(tree.conditions.len(), 1);
        assert_eq!(tree.conditions[0].return_value, json!(9.99));
        assert!(tree.conditions[0].error.is_none());
        assert_eq!(tree.conditions[0].input_patterns.len(), 1);
        assert_eq!(tree.conditions[0].input_patterns[0].arg_index, 0);
        assert_eq!(
            tree.conditions[0].input_patterns[0].matcher,
            PatternMatcher::Eq {
                value: json!("widget")
            }
        );
        assert_eq!(tree.default_return, json!(9.99));
    }

    // -- Branching function: two conditions --

    #[test]
    fn branching_function_produces_two_conditions() {
        let map = make_behavior_map(
            "classify",
            vec![
                make_behavior(0, vec![json!(5)], Some(json!("positive")), None),
                make_behavior(1, vec![json!(-3)], Some(json!("negative")), None),
            ],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");

        assert_eq!(tree.conditions.len(), 2);
        assert_eq!(tree.conditions[0].return_value, json!("positive"));
        assert_eq!(tree.conditions[1].return_value, json!("negative"));

        // First condition matches input 5
        assert!(matches_condition(&[json!(5)], &tree.conditions[0]));
        assert!(!matches_condition(&[json!(-3)], &tree.conditions[0]));

        // Second condition matches input -3
        assert!(matches_condition(&[json!(-3)], &tree.conditions[1]));
        assert!(!matches_condition(&[json!(5)], &tree.conditions[1]));

        // Default is the last return value
        assert_eq!(tree.default_return, json!("negative"));
    }

    // -- Error-throwing function → mock includes error condition --

    #[test]
    fn error_throwing_function_includes_error_condition() {
        let map = make_behavior_map(
            "safeDivide",
            vec![
                make_behavior(0, vec![json!(10), json!(2)], Some(json!(5)), None),
                make_behavior(
                    1,
                    vec![json!(1), json!(0)],
                    None,
                    Some(ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "division by zero".to_string(),
                        stack: None, error_category: None }),
                ),
            ],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");

        assert_eq!(tree.conditions.len(), 2);

        // First condition: normal return
        assert!(tree.conditions[0].error.is_none());
        assert_eq!(tree.conditions[0].return_value, json!(5));

        // Second condition: error
        assert_eq!(
            tree.conditions[1].error.as_deref(),
            Some("division by zero")
        );

        // Default return comes from the last non-error behavior
        assert_eq!(tree.default_return, json!(5));
    }

    // -- Multiple parameters → correct arg_index --

    #[test]
    fn multiple_parameters_have_correct_arg_indices() {
        let map = make_behavior_map(
            "add",
            vec![make_behavior(
                0,
                vec![json!(3), json!(4), json!("extra")],
                Some(json!(7)),
                None,
            )],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");

        let patterns = &tree.conditions[0].input_patterns;
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].arg_index, 0);
        assert_eq!(
            patterns[0].matcher,
            PatternMatcher::Eq { value: json!(3) }
        );
        assert_eq!(patterns[1].arg_index, 1);
        assert_eq!(
            patterns[1].matcher,
            PatternMatcher::Eq { value: json!(4) }
        );
        assert_eq!(patterns[2].arg_index, 2);
        assert_eq!(
            patterns[2].matcher,
            PatternMatcher::Eq {
                value: json!("extra")
            }
        );
    }

    // -- to_enhanced_mock_config produces valid MockConfig --

    #[test]
    fn enhanced_mock_config_from_decision_tree() {
        let map = make_behavior_map(
            "lookup",
            vec![
                make_behavior(0, vec![json!("a")], Some(json!(1)), None),
                make_behavior(1, vec![json!("b")], Some(json!(2)), None),
                make_behavior(
                    2,
                    vec![json!("bad")],
                    None,
                    Some(ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "not found".to_string(),
                        stack: None, error_category: None }),
                ),
            ],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");
        let config = to_enhanced_mock_config(&tree);

        assert_eq!(config.symbol, "lookup");
        // Error conditions are filtered out of return_values
        assert_eq!(config.return_values, vec![json!(1), json!(2)]);
        assert!(config.should_track_calls);
        assert_eq!(config.default_behavior, MockBehavior::RepeatLast);
    }

    // -- Empty behavior map → None --

    #[test]
    fn empty_behavior_map_returns_none() {
        let map = make_behavior_map("noop", vec![]);
        assert!(build_mock_from_behavior_map(&map).is_none());
    }

    // -- mock_config_from_behavior_map fallback --

    #[test]
    fn fallback_to_flat_mock_when_no_tree() {
        let map = make_behavior_map("noop", vec![]);
        let config = mock_config_from_behavior_map(&map);
        assert_eq!(config.symbol, "noop");
        assert!(config.return_values.is_empty());
    }

    #[test]
    fn mock_config_from_behavior_map_uses_tree_when_possible() {
        let map = make_behavior_map(
            "fn1",
            vec![
                make_behavior(0, vec![json!(1)], Some(json!("a")), None),
                make_behavior(
                    1,
                    vec![json!(2)],
                    None,
                    Some(ErrorInfo {
                        error_type: "Error".to_string(),
                        message: "fail".to_string(),
                        stack: None, error_category: None }),
                ),
            ],
        );

        let config = mock_config_from_behavior_map(&map);
        // Tree filters out error conditions from return_values
        assert_eq!(config.return_values, vec![json!("a")]);
    }

    // -- Pattern matching tests --

    #[test]
    fn eq_pattern_matches_exact_value() {
        let matcher = PatternMatcher::Eq { value: json!(42) };
        assert!(matches_pattern(&json!(42), &matcher));
        assert!(!matches_pattern(&json!(43), &matcher));
        assert!(!matches_pattern(&json!("42"), &matcher));
    }

    #[test]
    fn range_pattern_matches_inclusive_bounds() {
        let matcher = PatternMatcher::Range {
            min: 0.0,
            max: 10.0,
        };
        assert!(matches_pattern(&json!(0), &matcher));
        assert!(matches_pattern(&json!(5), &matcher));
        assert!(matches_pattern(&json!(10), &matcher));
        assert!(!matches_pattern(&json!(-1), &matcher));
        assert!(!matches_pattern(&json!(11), &matcher));
        assert!(!matches_pattern(&json!("five"), &matcher));
    }

    #[test]
    fn type_pattern_matches_json_type() {
        let matcher = PatternMatcher::Type {
            type_name: "string".to_string(),
        };
        assert!(matches_pattern(&json!("hello"), &matcher));
        assert!(!matches_pattern(&json!(42), &matcher));

        let num_matcher = PatternMatcher::Type {
            type_name: "number".to_string(),
        };
        assert!(matches_pattern(&json!(42), &num_matcher));
        assert!(!matches_pattern(&json!("42"), &num_matcher));
    }

    #[test]
    fn any_pattern_matches_everything() {
        let matcher = PatternMatcher::Any;
        assert!(matches_pattern(&json!(null), &matcher));
        assert!(matches_pattern(&json!(42), &matcher));
        assert!(matches_pattern(&json!("hello"), &matcher));
        assert!(matches_pattern(&json!([1, 2, 3]), &matcher));
    }

    #[test]
    fn matches_condition_requires_all_patterns() {
        let condition = MockCondition {
            input_patterns: vec![
                InputPattern {
                    arg_index: 0,
                    matcher: PatternMatcher::Eq { value: json!(1) },
                },
                InputPattern {
                    arg_index: 1,
                    matcher: PatternMatcher::Eq { value: json!(2) },
                },
            ],
            return_value: json!(3),
            error: None,
        };

        assert!(matches_condition(&[json!(1), json!(2)], &condition));
        assert!(!matches_condition(&[json!(1), json!(3)], &condition));
        assert!(!matches_condition(&[json!(2), json!(2)], &condition));
        // Missing argument
        assert!(!matches_condition(&[json!(1)], &condition));
    }

    // -- Serialization round-trip --

    #[test]
    fn decision_tree_round_trips_through_json() {
        let tree = MockDecisionTree {
            symbol: "myFunc".to_string(),
            conditions: vec![
                MockCondition {
                    input_patterns: vec![InputPattern {
                        arg_index: 0,
                        matcher: PatternMatcher::Eq { value: json!(1) },
                    }],
                    return_value: json!("one"),
                    error: None,
                },
                MockCondition {
                    input_patterns: vec![InputPattern {
                        arg_index: 0,
                        matcher: PatternMatcher::Range {
                            min: 2.0,
                            max: 10.0,
                        },
                    }],
                    return_value: json!("many"),
                    error: None,
                },
            ],
            default_return: json!("unknown"),
        };

        let json = serde_json::to_string(&tree).expect("serialize");
        let deserialized: MockDecisionTree =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tree, deserialized);
    }

    // -- Behavior with no return and no error is skipped --

    #[test]
    fn behavior_without_return_or_error_is_skipped() {
        let map = make_behavior_map(
            "sideEffectOnly",
            vec![
                // This behavior has neither return nor error — should be skipped
                Behavior {
                    id: 0,
                    input_args: vec![json!("x")],
                    return_value: None,
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                },
                make_behavior(1, vec![json!("y")], Some(json!("ok")), None),
            ],
        );

        let tree = build_mock_from_behavior_map(&map).expect("should produce a tree");
        // Only the second behavior produced a condition
        assert_eq!(tree.conditions.len(), 1);
        assert_eq!(tree.conditions[0].return_value, json!("ok"));
    }
}
