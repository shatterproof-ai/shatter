//! Behavior clustering: group execution records by branch path.
//!
//! Two executions with the same sequence of branch decisions (same branch IDs,
//! same taken/not-taken outcomes) belong to the same cluster. Each cluster
//! tracks statistics: specimen count, input value ranges, and output value ranges.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::execution_record::{BranchDecision, ExecutionRecord};

/// A compact representation of a branch path used as a clustering key.
///
/// Each entry is `(branch_id, taken)` — the sequence of branch decisions
/// in execution order.
pub type BranchPathKey = Vec<(u32, bool)>;

/// Extract the clustering key from a sequence of branch decisions.
fn branch_path_key(decisions: &[BranchDecision]) -> BranchPathKey {
    decisions
        .iter()
        .map(|d| (d.branch_id, d.taken))
        .collect()
}

/// Observed range for a JSON value dimension (min and max as JSON values).
///
/// For numeric values, min/max reflect the numeric range. For non-numeric
/// values, min/max are the first and last values seen (lexicographic on
/// the JSON serialization).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueRange {
    pub min: serde_json::Value,
    pub max: serde_json::Value,
    pub distinct_count: usize,
}

/// Statistics for a single behavior cluster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterStats {
    /// Number of execution records in this cluster.
    pub specimen_count: usize,
    /// Range of each input parameter across all specimens.
    /// Index corresponds to the parameter position.
    pub input_ranges: Vec<ValueRange>,
    /// Range of the output (return value) across all specimens.
    /// `None` if all specimens threw errors (no return values).
    pub output_range: Option<ValueRange>,
}

/// A group of execution records that share the same branch path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorCluster {
    /// The branch path shared by all records in this cluster.
    pub branch_path: BranchPathKey,
    /// Human-readable label, e.g. "TF" for a 2-branch path (taken, not-taken).
    pub label: String,
    /// Statistics computed from the records in this cluster.
    pub stats: ClusterStats,
    /// Indices into the original records slice for the specimens in this cluster.
    pub record_indices: Vec<usize>,
}

/// Result of clustering a set of execution records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusteringResult {
    pub function_id: String,
    pub total_records: usize,
    pub clusters: Vec<BehaviorCluster>,
}

/// Compare two JSON values for ordering.
///
/// Numeric values are compared numerically. All other values are compared
/// by their JSON serialization (which gives a stable, deterministic order).
fn json_cmp(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    match (a.as_f64(), b.as_f64()) {
        (Some(fa), Some(fb)) => fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal),
        _ => {
            let sa = serde_json::to_string(a).unwrap_or_default();
            let sb = serde_json::to_string(b).unwrap_or_default();
            sa.cmp(&sb)
        }
    }
}

/// Compute the value range for a set of JSON values.
fn compute_value_range(values: &[&serde_json::Value]) -> Option<ValueRange> {
    if values.is_empty() {
        return None;
    }

    let mut sorted: Vec<&serde_json::Value> = values.to_vec();
    sorted.sort_by(|a, b| json_cmp(a, b));

    // Count distinct values
    let mut distinct = 1;
    for window in sorted.windows(2) {
        if window[0] != window[1] {
            distinct += 1;
        }
    }

    Some(ValueRange {
        min: sorted[0].clone(),
        max: sorted[sorted.len() - 1].clone(),
        distinct_count: distinct,
    })
}

/// Build a label from a branch path key.
///
/// Each decision is "T" (taken) or "F" (not-taken), concatenated in order.
/// For example, `[(0, true), (1, false)]` becomes `"TF"`.
fn path_label(key: &BranchPathKey) -> String {
    key.iter()
        .map(|(_, taken)| if *taken { 'T' } else { 'F' })
        .collect()
}

/// Compute cluster statistics from a set of execution records.
fn compute_stats(records: &[&ExecutionRecord]) -> ClusterStats {
    let specimen_count = records.len();

    // Determine the maximum number of parameters across all records.
    let max_params = records.iter().map(|r| r.parameters.len()).max().unwrap_or(0);

    let mut input_ranges = Vec::with_capacity(max_params);
    for i in 0..max_params {
        let values: Vec<&serde_json::Value> = records
            .iter()
            .filter_map(|r| r.parameters.get(i))
            .collect();
        if let Some(range) = compute_value_range(&values) {
            input_ranges.push(range);
        }
    }

    let return_values: Vec<&serde_json::Value> = records
        .iter()
        .filter_map(|r| r.return_value.as_ref())
        .collect();
    let output_range = compute_value_range(&return_values);

    ClusterStats {
        specimen_count,
        input_ranges,
        output_range,
    }
}

/// Group execution records into behavior clusters based on their branch path.
///
/// Records with identical branch paths (same sequence of `(branch_id, taken)`)
/// are placed in the same cluster. Clusters are sorted by their label for
/// deterministic output.
pub fn cluster_by_branch_path(
    function_id: impl Into<String>,
    records: &[ExecutionRecord],
) -> ClusteringResult {
    let function_id = function_id.into();

    // Group record indices by branch path key.
    let mut groups: HashMap<BranchPathKey, Vec<usize>> = HashMap::new();
    for (i, record) in records.iter().enumerate() {
        let key = branch_path_key(&record.branch_path);
        groups.entry(key).or_default().push(i);
    }

    // Build clusters, sorted by label for determinism.
    let mut clusters: Vec<BehaviorCluster> = groups
        .into_iter()
        .map(|(key, indices)| {
            let label = path_label(&key);
            let cluster_records: Vec<&ExecutionRecord> =
                indices.iter().map(|&i| &records[i]).collect();
            let stats = compute_stats(&cluster_records);
            BehaviorCluster {
                branch_path: key,
                label,
                stats,
                record_indices: indices,
            }
        })
        .collect();

    clusters.sort_by(|a, b| a.label.cmp(&b.label));

    ClusteringResult {
        function_id,
        total_records: records.len(),
        clusters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, ExecutionRecord, SymConstraint};
    use serde_json::json;

    /// Helper: build a minimal execution record with given branch path and values.
    fn make_record(
        branch_decisions: Vec<(u32, bool)>,
        params: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
    ) -> ExecutionRecord {
        let branch_path = branch_decisions
            .into_iter()
            .enumerate()
            .map(|(i, (id, taken))| BranchDecision {
                branch_id: id,
                line: (10 + i as u32) * 10,
                taken,
                constraint: SymConstraint::Unknown {
                    hint: format!("branch_{id}"),
                },
            })
            .collect();

        ExecutionRecord {
            function_id: "test_fn".to_string(),
            input_hash: 0,
            parameters: params,
            branch_path,
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
    fn empty_records_produces_empty_clusters() {
        let result = cluster_by_branch_path("f", &[]);
        assert_eq!(result.function_id, "f");
        assert_eq!(result.total_records, 0);
        assert!(result.clusters.is_empty());
    }

    #[test]
    fn single_record_produces_one_cluster() {
        let records = vec![make_record(vec![(0, true)], vec![json!(5)], Some(json!(10)))];
        let result = cluster_by_branch_path("f", &records);

        assert_eq!(result.clusters.len(), 1);
        assert_eq!(result.clusters[0].label, "T");
        assert_eq!(result.clusters[0].stats.specimen_count, 1);
        assert_eq!(result.clusters[0].record_indices, vec![0]);
    }

    #[test]
    fn two_branch_function_groups_into_four_clusters() {
        // Simulate a function with 2 branches, 10 executions covering all 4 paths.
        let records = vec![
            // TT path
            make_record(vec![(0, true), (1, true)], vec![json!(10)], Some(json!(100))),
            make_record(vec![(0, true), (1, true)], vec![json!(20)], Some(json!(200))),
            make_record(vec![(0, true), (1, true)], vec![json!(15)], Some(json!(150))),
            // TF path
            make_record(vec![(0, true), (1, false)], vec![json!(5)], Some(json!(50))),
            make_record(vec![(0, true), (1, false)], vec![json!(8)], Some(json!(80))),
            // FT path
            make_record(vec![(0, false), (1, true)], vec![json!(-1)], Some(json!(-10))),
            make_record(vec![(0, false), (1, true)], vec![json!(-5)], Some(json!(-50))),
            make_record(vec![(0, false), (1, true)], vec![json!(-3)], Some(json!(-30))),
            // FF path
            make_record(vec![(0, false), (1, false)], vec![json!(0)], Some(json!(0))),
            make_record(vec![(0, false), (1, false)], vec![json!(-100)], Some(json!(-1000))),
        ];

        let result = cluster_by_branch_path("classify", &records);

        assert_eq!(result.total_records, 10);
        assert_eq!(result.clusters.len(), 4);

        // Clusters are sorted by label: FF, FT, TF, TT
        assert_eq!(result.clusters[0].label, "FF");
        assert_eq!(result.clusters[1].label, "FT");
        assert_eq!(result.clusters[2].label, "TF");
        assert_eq!(result.clusters[3].label, "TT");
    }

    #[test]
    fn cluster_specimen_counts_are_accurate() {
        let records = vec![
            make_record(vec![(0, true), (1, true)], vec![json!(10)], Some(json!(100))),
            make_record(vec![(0, true), (1, true)], vec![json!(20)], Some(json!(200))),
            make_record(vec![(0, true), (1, true)], vec![json!(15)], Some(json!(150))),
            make_record(vec![(0, true), (1, false)], vec![json!(5)], Some(json!(50))),
            make_record(vec![(0, true), (1, false)], vec![json!(8)], Some(json!(80))),
            make_record(vec![(0, false), (1, true)], vec![json!(-1)], Some(json!(-10))),
            make_record(vec![(0, false), (1, true)], vec![json!(-5)], Some(json!(-50))),
            make_record(vec![(0, false), (1, true)], vec![json!(-3)], Some(json!(-30))),
            make_record(vec![(0, false), (1, false)], vec![json!(0)], Some(json!(0))),
            make_record(vec![(0, false), (1, false)], vec![json!(-100)], Some(json!(-1000))),
        ];

        let result = cluster_by_branch_path("classify", &records);

        // FF: 2, FT: 3, TF: 2, TT: 3
        assert_eq!(result.clusters[0].stats.specimen_count, 2); // FF
        assert_eq!(result.clusters[1].stats.specimen_count, 3); // FT
        assert_eq!(result.clusters[2].stats.specimen_count, 2); // TF
        assert_eq!(result.clusters[3].stats.specimen_count, 3); // TT
    }

    #[test]
    fn input_ranges_reflect_parameter_min_max() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!(10)], Some(json!(100))),
            make_record(vec![(0, true)], vec![json!(20)], Some(json!(200))),
            make_record(vec![(0, true)], vec![json!(5)], Some(json!(50))),
        ];

        let result = cluster_by_branch_path("f", &records);
        let cluster = &result.clusters[0];

        assert_eq!(cluster.stats.input_ranges.len(), 1);
        assert_eq!(cluster.stats.input_ranges[0].min, json!(5));
        assert_eq!(cluster.stats.input_ranges[0].max, json!(20));
        assert_eq!(cluster.stats.input_ranges[0].distinct_count, 3);
    }

    #[test]
    fn output_range_reflects_return_value_min_max() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!(1)], Some(json!(100))),
            make_record(vec![(0, true)], vec![json!(2)], Some(json!(300))),
            make_record(vec![(0, true)], vec![json!(3)], Some(json!(200))),
        ];

        let result = cluster_by_branch_path("f", &records);
        let range = result.clusters[0].stats.output_range.as_ref().expect("has output");

        assert_eq!(range.min, json!(100));
        assert_eq!(range.max, json!(300));
        assert_eq!(range.distinct_count, 3);
    }

    #[test]
    fn output_range_is_none_when_all_records_threw() {
        let mut record = make_record(vec![(0, true)], vec![json!(1)], None);
        record.thrown_error = Some(crate::execution_record::ErrorInfo {
            error_type: "Error".to_string(),
            message: "boom".to_string(),
            stack: None, error_category: None });

        let result = cluster_by_branch_path("f", &[record]);
        assert!(result.clusters[0].stats.output_range.is_none());
    }

    #[test]
    fn records_with_no_branches_cluster_together() {
        let records = vec![
            make_record(vec![], vec![json!(1)], Some(json!("a"))),
            make_record(vec![], vec![json!(2)], Some(json!("b"))),
        ];

        let result = cluster_by_branch_path("f", &records);
        assert_eq!(result.clusters.len(), 1);
        assert_eq!(result.clusters[0].label, "");
        assert_eq!(result.clusters[0].stats.specimen_count, 2);
    }

    #[test]
    fn record_indices_point_to_correct_records() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!(1)], Some(json!("a"))),
            make_record(vec![(0, false)], vec![json!(2)], Some(json!("b"))),
            make_record(vec![(0, true)], vec![json!(3)], Some(json!("c"))),
        ];

        let result = cluster_by_branch_path("f", &records);

        // F cluster should have index 1
        let f_cluster = result.clusters.iter().find(|c| c.label == "F").expect("F cluster");
        assert_eq!(f_cluster.record_indices, vec![1]);

        // T cluster should have indices 0 and 2
        let t_cluster = result.clusters.iter().find(|c| c.label == "T").expect("T cluster");
        assert!(t_cluster.record_indices.contains(&0));
        assert!(t_cluster.record_indices.contains(&2));
    }

    #[test]
    fn multiple_parameters_have_independent_ranges() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!(1), json!(100)], Some(json!("ok"))),
            make_record(vec![(0, true)], vec![json!(5), json!(50)], Some(json!("ok"))),
            make_record(vec![(0, true)], vec![json!(3), json!(200)], Some(json!("ok"))),
        ];

        let result = cluster_by_branch_path("f", &records);
        let ranges = &result.clusters[0].stats.input_ranges;

        assert_eq!(ranges.len(), 2);
        // First param: 1..5
        assert_eq!(ranges[0].min, json!(1));
        assert_eq!(ranges[0].max, json!(5));
        // Second param: 50..200
        assert_eq!(ranges[1].min, json!(50));
        assert_eq!(ranges[1].max, json!(200));
    }

    #[test]
    fn string_values_use_lexicographic_range() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!("banana")], Some(json!("ok"))),
            make_record(vec![(0, true)], vec![json!("apple")], Some(json!("ok"))),
            make_record(vec![(0, true)], vec![json!("cherry")], Some(json!("ok"))),
        ];

        let result = cluster_by_branch_path("f", &records);
        let range = &result.clusters[0].stats.input_ranges[0];

        // JSON serialization includes quotes: "apple" < "banana" < "cherry"
        assert_eq!(range.min, json!("apple"));
        assert_eq!(range.max, json!("cherry"));
    }

    #[test]
    fn clustering_result_round_trips() {
        let records = vec![
            make_record(vec![(0, true)], vec![json!(1)], Some(json!(10))),
            make_record(vec![(0, false)], vec![json!(2)], Some(json!(20))),
        ];
        let result = cluster_by_branch_path("f", &records);

        let json_str = serde_json::to_string(&result).expect("serialize");
        let deserialized: ClusteringResult =
            serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(result, deserialized);
    }
}
