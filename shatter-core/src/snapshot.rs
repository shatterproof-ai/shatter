//! Snapshot export and diff: serialize behavior maps to a stable JSON format
//! and compare snapshots to detect regressions.
//!
//! A [`Snapshot`] captures the observed behaviors of one or more functions at a
//! point in time. The [`diff`] function compares two snapshots and reports which
//! behaviors match, which are new, and which have regressed.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::behavior::{Behavior, BehaviorMap};

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// A single behavior entry in a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotBehavior {
    /// Behavior identifier (e.g. "b0", "b1").
    pub id: String,
    /// Exemplar input arguments that trigger this behavior.
    pub exemplar_input: Vec<serde_json::Value>,
    /// Expected return value, if the function returns normally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<serde_json::Value>,
    /// Expected error, if the function throws.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<SnapshotError>,
}

/// Error information stored in a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotError {
    pub error_type: String,
    pub message: String,
}

/// A snapshot of a single function's behaviors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionSnapshot {
    /// Fully qualified function name.
    pub function_id: String,
    /// Observed behaviors.
    pub behaviors: Vec<SnapshotBehavior>,
}

/// A complete snapshot file containing one or more function snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// ISO 8601 timestamp of when this snapshot was created.
    pub created_at: String,
    /// Function snapshots.
    pub functions: Vec<FunctionSnapshot>,
}

// ---------------------------------------------------------------------------
// Snapshot construction
// ---------------------------------------------------------------------------

impl Snapshot {
    /// Current schema version.
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a snapshot from a single behavior map.
    pub fn from_behavior_map(behavior_map: &BehaviorMap, created_at: impl Into<String>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            created_at: created_at.into(),
            functions: vec![FunctionSnapshot::from_behavior_map(behavior_map)],
        }
    }

    /// Create a snapshot from multiple behavior maps.
    pub fn from_behavior_maps(
        maps: &[BehaviorMap],
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            created_at: created_at.into(),
            functions: maps.iter().map(FunctionSnapshot::from_behavior_map).collect(),
        }
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Write snapshot to a file.
    pub fn write_to_file(&self, path: &Path) -> Result<(), SnapshotError2> {
        let json = self.to_json().map_err(SnapshotError2::Serialize)?;
        std::fs::write(path, json).map_err(SnapshotError2::Io)
    }

    /// Read snapshot from a file.
    pub fn read_from_file(path: &Path) -> Result<Self, SnapshotError2> {
        let contents = std::fs::read_to_string(path).map_err(SnapshotError2::Io)?;
        Self::from_json(&contents).map_err(SnapshotError2::Deserialize)
    }
}

impl FunctionSnapshot {
    /// Convert a behavior map into a function snapshot.
    pub fn from_behavior_map(map: &BehaviorMap) -> Self {
        Self {
            function_id: map.function_id.clone(),
            behaviors: map
                .behaviors
                .iter()
                .map(SnapshotBehavior::from_behavior)
                .collect(),
        }
    }
}

impl SnapshotBehavior {
    /// Convert a [`Behavior`] into a snapshot behavior.
    fn from_behavior(behavior: &Behavior) -> Self {
        Self {
            id: format!("b{}", behavior.id),
            exemplar_input: behavior.input_args.clone(),
            expected_output: behavior.return_value.clone(),
            expected_error: behavior.thrown_error.as_ref().map(|e| SnapshotError {
                error_type: e.error_type.clone(),
                message: e.message.clone(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot errors
// ---------------------------------------------------------------------------

/// Errors from snapshot I/O operations.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError2 {
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(serde_json::Error),
    #[error("deserialization error: {0}")]
    Deserialize(serde_json::Error),
}

// ---------------------------------------------------------------------------
// Diff types
// ---------------------------------------------------------------------------

/// The outcome of comparing a single behavior between two snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BehaviorDiffEntry {
    /// The behavior exists in both snapshots with matching output/error.
    Match {
        behavior_id: String,
        function_id: String,
    },
    /// The behavior exists in both snapshots but the output/error changed.
    Regression {
        behavior_id: String,
        function_id: String,
        previous: BehaviorOutcome,
        current: BehaviorOutcome,
    },
    /// The behavior exists only in the new snapshot.
    New {
        behavior_id: String,
        function_id: String,
    },
    /// The behavior exists only in the old snapshot (was removed).
    Removed {
        behavior_id: String,
        function_id: String,
    },
}

/// Summarized outcome of a behavior (return value or error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SnapshotError>,
}

/// Result of diffing two snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub entries: Vec<BehaviorDiffEntry>,
}

impl SnapshotDiff {
    /// Number of matching behaviors.
    pub fn matches(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, BehaviorDiffEntry::Match { .. }))
            .count()
    }

    /// Number of regressions (changed behaviors).
    pub fn regressions(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, BehaviorDiffEntry::Regression { .. }))
            .count()
    }

    /// Number of new behaviors (only in current).
    pub fn new_behaviors(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, BehaviorDiffEntry::New { .. }))
            .count()
    }

    /// Number of removed behaviors (only in previous).
    pub fn removed_behaviors(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, BehaviorDiffEntry::Removed { .. }))
            .count()
    }

    /// Whether there are any regressions.
    pub fn has_regressions(&self) -> bool {
        self.regressions() > 0
    }

    /// Whether there are any removed behaviors.
    pub fn has_removals(&self) -> bool {
        self.removed_behaviors() > 0
    }

    /// Format a human-readable diff report.
    pub fn format_report(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "Snapshot diff: {} match, {} regression(s), {} new, {} removed\n\n",
            self.matches(),
            self.regressions(),
            self.new_behaviors(),
            self.removed_behaviors(),
        ));

        for entry in &self.entries {
            match entry {
                BehaviorDiffEntry::Match {
                    behavior_id,
                    function_id,
                } => {
                    out.push_str(&format!("  [MATCH]      {function_id}:{behavior_id}\n"));
                }
                BehaviorDiffEntry::Regression {
                    behavior_id,
                    function_id,
                    previous,
                    current,
                } => {
                    out.push_str(&format!(
                        "  [REGRESSION] {function_id}:{behavior_id}\n"
                    ));
                    out.push_str(&format!(
                        "               previous: {}\n",
                        format_outcome(previous)
                    ));
                    out.push_str(&format!(
                        "               current:  {}\n",
                        format_outcome(current)
                    ));
                }
                BehaviorDiffEntry::New {
                    behavior_id,
                    function_id,
                } => {
                    out.push_str(&format!("  [NEW]        {function_id}:{behavior_id}\n"));
                }
                BehaviorDiffEntry::Removed {
                    behavior_id,
                    function_id,
                } => {
                    out.push_str(&format!("  [REMOVED]    {function_id}:{behavior_id}\n"));
                }
            }
        }

        out
    }
}

fn format_outcome(outcome: &BehaviorOutcome) -> String {
    if let Some(ref err) = outcome.error {
        format!("throws {}: {}", err.error_type, err.message)
    } else {
        match &outcome.return_value {
            Some(v) => {
                let s = v.to_string();
                if s.len() > 60 {
                    format!("returns {}...", &s[..57])
                } else {
                    format!("returns {s}")
                }
            }
            None => "returns (void)".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Diff logic
// ---------------------------------------------------------------------------

/// Compare two snapshots and produce a diff.
///
/// Behaviors are matched by function ID and behavior ID. For behaviors with the
/// same ID, the output/error is compared. Behaviors present only in one snapshot
/// are reported as new or removed.
pub fn diff(previous: &Snapshot, current: &Snapshot) -> SnapshotDiff {
    let mut entries = Vec::new();

    // Index previous snapshot by (function_id, behavior_id)
    let mut prev_map: HashMap<(&str, &str), &SnapshotBehavior> = HashMap::new();
    let mut prev_functions: HashMap<&str, &FunctionSnapshot> = HashMap::new();
    for func in &previous.functions {
        prev_functions.insert(&func.function_id, func);
        for behavior in &func.behaviors {
            prev_map.insert((&func.function_id, &behavior.id), behavior);
        }
    }

    // Walk current snapshot
    let mut seen_keys: std::collections::HashSet<(&str, &str)> =
        std::collections::HashSet::new();

    for func in &current.functions {
        for behavior in &func.behaviors {
            let key = (func.function_id.as_str(), behavior.id.as_str());
            seen_keys.insert(key);

            match prev_map.get(&key) {
                Some(prev_behavior) => {
                    let prev_outcome = BehaviorOutcome {
                        return_value: prev_behavior.expected_output.clone(),
                        error: prev_behavior.expected_error.clone(),
                    };
                    let curr_outcome = BehaviorOutcome {
                        return_value: behavior.expected_output.clone(),
                        error: behavior.expected_error.clone(),
                    };

                    if prev_outcome == curr_outcome {
                        entries.push(BehaviorDiffEntry::Match {
                            behavior_id: behavior.id.clone(),
                            function_id: func.function_id.clone(),
                        });
                    } else {
                        entries.push(BehaviorDiffEntry::Regression {
                            behavior_id: behavior.id.clone(),
                            function_id: func.function_id.clone(),
                            previous: prev_outcome,
                            current: curr_outcome,
                        });
                    }
                }
                None => {
                    entries.push(BehaviorDiffEntry::New {
                        behavior_id: behavior.id.clone(),
                        function_id: func.function_id.clone(),
                    });
                }
            }
        }
    }

    // Find removed behaviors (in previous but not in current)
    for func in &previous.functions {
        for behavior in &func.behaviors {
            let key = (func.function_id.as_str(), behavior.id.as_str());
            if !seen_keys.contains(&key) {
                entries.push(BehaviorDiffEntry::Removed {
                    behavior_id: behavior.id.clone(),
                    function_id: func.function_id.clone(),
                });
            }
        }
    }

    SnapshotDiff { entries }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::behavior::{Behavior, BehaviorMap};
    use crate::execution_record::ErrorInfo;

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
            nondeterministic_fields: vec![],
        }
    }

    // --- Snapshot construction tests ---

    #[test]
    fn snapshot_from_single_behavior_map() {
        let map = make_behavior_map(
            "add",
            vec![
                make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None),
                make_behavior(1, vec![json!(-1), json!(1)], Some(json!(0)), None),
            ],
        );

        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T10:00:00Z");

        assert_eq!(snapshot.version, Snapshot::CURRENT_VERSION);
        assert_eq!(snapshot.created_at, "2026-02-26T10:00:00Z");
        assert_eq!(snapshot.functions.len(), 1);
        assert_eq!(snapshot.functions[0].function_id, "add");
        assert_eq!(snapshot.functions[0].behaviors.len(), 2);
        assert_eq!(snapshot.functions[0].behaviors[0].id, "b0");
        assert_eq!(
            snapshot.functions[0].behaviors[0].exemplar_input,
            vec![json!(1), json!(2)]
        );
        assert_eq!(
            snapshot.functions[0].behaviors[0].expected_output,
            Some(json!(3))
        );
        assert!(snapshot.functions[0].behaviors[0].expected_error.is_none());
    }

    #[test]
    fn snapshot_from_multiple_behavior_maps() {
        let maps = vec![
            make_behavior_map(
                "add",
                vec![make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None)],
            ),
            make_behavior_map(
                "sub",
                vec![make_behavior(0, vec![json!(5), json!(3)], Some(json!(2)), None)],
            ),
        ];

        let snapshot = Snapshot::from_behavior_maps(&maps, "2026-02-26T10:00:00Z");
        assert_eq!(snapshot.functions.len(), 2);
        assert_eq!(snapshot.functions[0].function_id, "add");
        assert_eq!(snapshot.functions[1].function_id, "sub");
    }

    #[test]
    fn snapshot_includes_error_behaviors() {
        let map = make_behavior_map(
            "divide",
            vec![make_behavior(
                0,
                vec![json!(1), json!(0)],
                None,
                Some(ErrorInfo {
                    error_type: "Error".to_string(),
                    message: "division by zero".to_string(),
                    stack: None, error_category: None }),
            )],
        );

        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T10:00:00Z");
        let behavior = &snapshot.functions[0].behaviors[0];
        assert!(behavior.expected_output.is_none());
        assert_eq!(
            behavior.expected_error,
            Some(SnapshotError {
                error_type: "Error".to_string(),
                message: "division by zero".to_string(),
            })
        );
    }

    // --- Round-trip serialization tests ---

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let map = make_behavior_map(
            "classify",
            vec![
                make_behavior(0, vec![json!(5)], Some(json!("positive")), None),
                make_behavior(
                    1,
                    vec![json!(-1)],
                    None,
                    Some(ErrorInfo {
                        error_type: "RangeError".to_string(),
                        message: "negative input".to_string(),
                        stack: None, error_category: None }),
                ),
            ],
        );

        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T10:00:00Z");
        round_trip(&snapshot);
    }

    #[test]
    fn snapshot_json_is_pretty_and_stable() {
        let map = make_behavior_map(
            "add",
            vec![make_behavior(0, vec![json!(1), json!(2)], Some(json!(3)), None)],
        );

        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T10:00:00Z");
        let json = snapshot.to_json().expect("serialize");

        // Should be pretty-printed (contains newlines and indentation)
        assert!(json.contains('\n'));
        assert!(json.contains("  "));

        // Should deserialize back
        let deserialized = Snapshot::from_json(&json).expect("deserialize");
        assert_eq!(snapshot, deserialized);
    }

    #[test]
    fn snapshot_file_round_trip() {
        let map = make_behavior_map(
            "greet",
            vec![make_behavior(
                0,
                vec![json!("world")],
                Some(json!("hello world")),
                None,
            )],
        );
        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T10:00:00Z");

        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("test-snapshot.json");

        snapshot.write_to_file(&path).expect("write");
        let loaded = Snapshot::read_from_file(&path).expect("read");
        assert_eq!(snapshot, loaded);
    }

    // --- Diff tests ---

    fn make_snapshot(functions: Vec<FunctionSnapshot>) -> Snapshot {
        Snapshot {
            version: Snapshot::CURRENT_VERSION,
            created_at: "2026-02-26T10:00:00Z".to_string(),
            functions,
        }
    }

    fn make_func_snapshot(
        function_id: &str,
        behaviors: Vec<SnapshotBehavior>,
    ) -> FunctionSnapshot {
        FunctionSnapshot {
            function_id: function_id.to_string(),
            behaviors,
        }
    }

    fn make_snap_behavior(
        id: &str,
        input: Vec<serde_json::Value>,
        output: Option<serde_json::Value>,
        error: Option<SnapshotError>,
    ) -> SnapshotBehavior {
        SnapshotBehavior {
            id: id.to_string(),
            exemplar_input: input,
            expected_output: output,
            expected_error: error,
        }
    }

    #[test]
    fn diff_identical_snapshots_all_match() {
        let snapshot = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![
                make_snap_behavior("b0", vec![json!(1), json!(2)], Some(json!(3)), None),
                make_snap_behavior("b1", vec![json!(0), json!(0)], Some(json!(0)), None),
            ],
        )]);

        let result = diff(&snapshot, &snapshot);

        assert_eq!(result.matches(), 2);
        assert_eq!(result.regressions(), 0);
        assert_eq!(result.new_behaviors(), 0);
        assert_eq!(result.removed_behaviors(), 0);
        assert!(!result.has_regressions());
        assert!(!result.has_removals());
    }

    #[test]
    fn diff_detects_regression_changed_return_value() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "classify",
            vec![make_snap_behavior(
                "b0",
                vec![json!(5)],
                Some(json!("positive")),
                None,
            )],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "classify",
            vec![make_snap_behavior(
                "b0",
                vec![json!(5)],
                Some(json!("positive-odd")),
                None,
            )],
        )]);

        let result = diff(&previous, &current);

        assert_eq!(result.matches(), 0);
        assert_eq!(result.regressions(), 1);
        assert!(result.has_regressions());

        match &result.entries[0] {
            BehaviorDiffEntry::Regression {
                behavior_id,
                function_id,
                previous: prev,
                current: curr,
            } => {
                assert_eq!(behavior_id, "b0");
                assert_eq!(function_id, "classify");
                assert_eq!(prev.return_value, Some(json!("positive")));
                assert_eq!(curr.return_value, Some(json!("positive-odd")));
            }
            other => panic!("expected Regression, got: {other:?}"),
        }
    }

    #[test]
    fn diff_detects_regression_return_became_error() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "process",
            vec![make_snap_behavior(
                "b0",
                vec![json!(null)],
                Some(json!("ok")),
                None,
            )],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "process",
            vec![make_snap_behavior(
                "b0",
                vec![json!(null)],
                None,
                Some(SnapshotError {
                    error_type: "TypeError".to_string(),
                    message: "null input".to_string(),
                }),
            )],
        )]);

        let result = diff(&previous, &current);
        assert_eq!(result.regressions(), 1);
        assert!(result.has_regressions());
    }

    #[test]
    fn diff_detects_new_behaviors() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![make_snap_behavior(
                "b0",
                vec![json!(1), json!(2)],
                Some(json!(3)),
                None,
            )],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![
                make_snap_behavior("b0", vec![json!(1), json!(2)], Some(json!(3)), None),
                make_snap_behavior("b1", vec![json!(-1), json!(-2)], Some(json!(-3)), None),
            ],
        )]);

        let result = diff(&previous, &current);

        assert_eq!(result.matches(), 1);
        assert_eq!(result.new_behaviors(), 1);
        assert_eq!(result.regressions(), 0);
    }

    #[test]
    fn diff_detects_removed_behaviors() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "calc",
            vec![
                make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None),
                make_snap_behavior("b1", vec![json!(0)], Some(json!(0)), None),
            ],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "calc",
            vec![make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None)],
        )]);

        let result = diff(&previous, &current);

        assert_eq!(result.matches(), 1);
        assert_eq!(result.removed_behaviors(), 1);
        assert!(result.has_removals());
    }

    #[test]
    fn diff_detects_new_function() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None)],
        )]);
        let current = make_snapshot(vec![
            make_func_snapshot(
                "add",
                vec![make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None)],
            ),
            make_func_snapshot(
                "sub",
                vec![make_snap_behavior("b0", vec![json!(5), json!(3)], Some(json!(2)), None)],
            ),
        ]);

        let result = diff(&previous, &current);
        assert_eq!(result.matches(), 1);
        assert_eq!(result.new_behaviors(), 1);
    }

    #[test]
    fn diff_detects_removed_function() {
        let previous = make_snapshot(vec![
            make_func_snapshot(
                "add",
                vec![make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None)],
            ),
            make_func_snapshot(
                "sub",
                vec![make_snap_behavior("b0", vec![json!(5)], Some(json!(2)), None)],
            ),
        ]);
        let current = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![make_snap_behavior("b0", vec![json!(1)], Some(json!(1)), None)],
        )]);

        let result = diff(&previous, &current);
        assert_eq!(result.matches(), 1);
        assert_eq!(result.removed_behaviors(), 1);
    }

    #[test]
    fn diff_mixed_scenario() {
        // Previous: add has b0 (match), b1 (will regress); sub has b0 (will be removed)
        let previous = make_snapshot(vec![
            make_func_snapshot(
                "add",
                vec![
                    make_snap_behavior("b0", vec![json!(1), json!(2)], Some(json!(3)), None),
                    make_snap_behavior("b1", vec![json!(0), json!(0)], Some(json!(0)), None),
                ],
            ),
            make_func_snapshot(
                "sub",
                vec![make_snap_behavior("b0", vec![json!(5), json!(3)], Some(json!(2)), None)],
            ),
        ]);

        // Current: add has b0 (match), b1 (regressed), b2 (new); mul is new function
        let current = make_snapshot(vec![
            make_func_snapshot(
                "add",
                vec![
                    make_snap_behavior("b0", vec![json!(1), json!(2)], Some(json!(3)), None),
                    make_snap_behavior("b1", vec![json!(0), json!(0)], Some(json!(1)), None), // regressed
                    make_snap_behavior("b2", vec![json!(-1), json!(-2)], Some(json!(-3)), None), // new
                ],
            ),
            make_func_snapshot(
                "mul",
                vec![make_snap_behavior("b0", vec![json!(2), json!(3)], Some(json!(6)), None)],
            ),
        ]);

        let result = diff(&previous, &current);

        assert_eq!(result.matches(), 1); // add:b0
        assert_eq!(result.regressions(), 1); // add:b1
        assert_eq!(result.new_behaviors(), 2); // add:b2 + mul:b0
        assert_eq!(result.removed_behaviors(), 1); // sub:b0
        assert!(result.has_regressions());
        assert!(result.has_removals());
    }

    #[test]
    fn diff_empty_snapshots() {
        let empty = make_snapshot(vec![]);
        let result = diff(&empty, &empty);

        assert_eq!(result.matches(), 0);
        assert_eq!(result.regressions(), 0);
        assert_eq!(result.new_behaviors(), 0);
        assert_eq!(result.removed_behaviors(), 0);
    }

    #[test]
    fn diff_report_format() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![
                make_snap_behavior("b0", vec![json!(1)], Some(json!(2)), None),
                make_snap_behavior("b1", vec![json!(0)], Some(json!(0)), None),
            ],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "add",
            vec![
                make_snap_behavior("b0", vec![json!(1)], Some(json!(2)), None),
                make_snap_behavior("b1", vec![json!(0)], Some(json!(99)), None),
                make_snap_behavior("b2", vec![json!(-1)], Some(json!(-1)), None),
            ],
        )]);

        let result = diff(&previous, &current);
        let report = result.format_report();

        assert!(report.contains("[MATCH]"));
        assert!(report.contains("[REGRESSION]"));
        assert!(report.contains("[NEW]"));
        assert!(report.contains("previous: returns 0"));
        assert!(report.contains("current:  returns 99"));
    }

    #[test]
    fn diff_report_shows_error_outcomes() {
        let previous = make_snapshot(vec![make_func_snapshot(
            "risky",
            vec![make_snap_behavior(
                "b0",
                vec![json!(null)],
                None,
                Some(SnapshotError {
                    error_type: "TypeError".to_string(),
                    message: "null input".to_string(),
                }),
            )],
        )]);
        let current = make_snapshot(vec![make_func_snapshot(
            "risky",
            vec![make_snap_behavior(
                "b0",
                vec![json!(null)],
                Some(json!("handled")),
                None,
            )],
        )]);

        let result = diff(&previous, &current);
        let report = result.format_report();

        assert!(report.contains("throws TypeError: null input"));
        assert!(report.contains("returns \"handled\""));
    }

    #[test]
    fn snapshot_diff_round_trips() {
        let result = SnapshotDiff {
            entries: vec![
                BehaviorDiffEntry::Match {
                    behavior_id: "b0".to_string(),
                    function_id: "add".to_string(),
                },
                BehaviorDiffEntry::Regression {
                    behavior_id: "b1".to_string(),
                    function_id: "add".to_string(),
                    previous: BehaviorOutcome {
                        return_value: Some(json!(0)),
                        error: None,
                    },
                    current: BehaviorOutcome {
                        return_value: Some(json!(1)),
                        error: None,
                    },
                },
                BehaviorDiffEntry::New {
                    behavior_id: "b2".to_string(),
                    function_id: "add".to_string(),
                },
                BehaviorDiffEntry::Removed {
                    behavior_id: "b0".to_string(),
                    function_id: "sub".to_string(),
                },
            ],
        };
        round_trip(&result);
    }

    #[test]
    fn snapshot_from_behavior_map_end_to_end() {
        // Build behavior map -> snapshot -> JSON -> snapshot -> diff with self
        let map = make_behavior_map(
            "calculate",
            vec![
                make_behavior(0, vec![json!(100), json!("gold")], Some(json!(80.0)), None),
                make_behavior(
                    1,
                    vec![json!(0), json!("invalid")],
                    None,
                    Some(ErrorInfo {
                        error_type: "ValidationError".to_string(),
                        message: "invalid tier".to_string(),
                        stack: None, error_category: None }),
                ),
            ],
        );

        let snapshot = Snapshot::from_behavior_map(&map, "2026-02-26T12:00:00Z");
        let json = snapshot.to_json().expect("serialize");
        let loaded = Snapshot::from_json(&json).expect("deserialize");

        let result = diff(&snapshot, &loaded);
        assert_eq!(result.matches(), 2);
        assert_eq!(result.regressions(), 0);
    }
}
