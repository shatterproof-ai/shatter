//! Behavior maps, call graphs, and compositional testing logic.
//!
//! When the concolic engine tests function A that calls function B, behavior maps
//! let us reuse prior knowledge about B. A [`BehaviorMap`] records B's observed
//! input→output mappings so that when testing A, B is mocked using its known
//! behaviors. [`CallGraph`] orders functions for testing (leaves first), and
//! [`BehaviorCoverage`] tracks which of B's behaviors A actually exercises.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::execution_record::{BranchDecision, ErrorInfo, ExecutionRecord, SideEffect};
use crate::protocol::{MockBehavior, MockConfig};

// ---------------------------------------------------------------------------
// BehaviorMap
// ---------------------------------------------------------------------------

/// A single observed behavior: specific inputs produced specific outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Behavior {
    pub id: u32,
    pub input_args: Vec<serde_json::Value>,
    pub return_value: Option<serde_json::Value>,
    pub thrown_error: Option<ErrorInfo>,
    pub branch_path: Vec<BranchDecision>,
    pub side_effects: Vec<SideEffect>,
}

/// All observed behaviors for a function, built from execution records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorMap {
    pub function_id: String,
    pub behaviors: Vec<Behavior>,
}

impl BehaviorMap {
    /// Build a behavior map from execution records, deduplicating by `input_hash`.
    pub fn from_records(function_id: impl Into<String>, records: &[ExecutionRecord]) -> Self {
        let mut seen_hashes = HashSet::new();
        let mut behaviors = Vec::new();
        let mut next_id: u32 = 0;

        for record in records {
            if !seen_hashes.insert(record.input_hash) {
                continue;
            }
            behaviors.push(Behavior {
                id: next_id,
                input_args: record.parameters.clone(),
                return_value: record.return_value.clone(),
                thrown_error: record.thrown_error.clone(),
                branch_path: record.branch_path.clone(),
                side_effects: record.side_effects.clone(),
            });
            next_id += 1;
        }

        Self {
            function_id: function_id.into(),
            behaviors,
        }
    }

    /// Convert this behavior map into a [`MockConfig`] for use when testing callers.
    ///
    /// Each behavior's return value becomes an entry in `return_values`. Behaviors
    /// that threw an error (and have no return value) are skipped.
    pub fn to_mock_config(&self) -> MockConfig {
        let return_values: Vec<serde_json::Value> = self
            .behaviors
            .iter()
            .filter_map(|b| b.return_value.clone())
            .collect();

        MockConfig {
            symbol: self.function_id.clone(),
            return_values,
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }
    }
}

// ---------------------------------------------------------------------------
// CallGraph
// ---------------------------------------------------------------------------

/// Error type for call graph operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CallGraphError {
    /// A cycle was detected during topological sort.
    #[error("cycle detected involving: {0}")]
    Cycle(String),
}

/// Dependency graph built from [`FunctionAnalysis`] results.
///
/// Used to compute test ordering so leaf functions are tested first.
pub struct CallGraph {
    /// function_id → set of function_ids it calls.
    edges: HashMap<String, HashSet<String>>,
}

impl CallGraph {
    /// Build a call graph from function analyses.
    ///
    /// Matches each function's `ExternalDependency.symbol` against the set of
    /// known function names to build edges.
    pub fn from_analyses(analyses: &[crate::protocol::FunctionAnalysis]) -> Self {
        let known_names: HashSet<&str> = analyses.iter().map(|a| a.name.as_str()).collect();
        let mut edges = HashMap::new();

        for analysis in analyses {
            let callees: HashSet<String> = analysis
                .dependencies
                .iter()
                .filter(|dep| known_names.contains(dep.symbol.as_str()))
                .map(|dep| dep.symbol.clone())
                .collect();
            edges.insert(analysis.name.clone(), callees);
        }

        Self { edges }
    }

    /// Topological sort returning leaf functions first.
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn test_order(&self) -> Result<Vec<String>, CallGraphError> {
        // Kahn's algorithm
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for (node, callees) in &self.edges {
            in_degree.entry(node.as_str()).or_insert(0);
            for callee in callees {
                *in_degree.entry(callee.as_str()).or_insert(0) += 1;
            }
        }

        // Note: in_degree counts how many functions *call* a node.
        // We want leaves first, so we want nodes with in_degree 0 in a
        // *reverse* dependency sense. Actually for test ordering we want
        // functions with no *outgoing* edges (callees) first — i.e., leaves.
        // Let's use a proper topological sort on the reversed graph.

        // Reverse the graph: if A calls B, reversed has B → A.
        let mut rev_edges: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut out_degree: HashMap<&str, usize> = HashMap::new();
        for (node, callees) in &self.edges {
            out_degree.entry(node.as_str()).or_insert(0);
            rev_edges.entry(node.as_str()).or_default();
            for callee in callees {
                rev_edges.entry(callee.as_str()).or_default().push(node.as_str());
                *out_degree.entry(node.as_str()).or_insert(0) += 1;
            }
        }

        // Ensure all callee nodes appear in out_degree even if they have no entry in edges.
        for callees in self.edges.values() {
            for callee in callees {
                out_degree.entry(callee.as_str()).or_insert(0);
            }
        }

        let mut queue: std::collections::VecDeque<&str> = out_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&node, _)| node)
            .collect();

        // Sort the initial queue for deterministic output.
        let mut sorted_queue: Vec<&str> = queue.drain(..).collect();
        sorted_queue.sort();
        queue.extend(sorted_queue);

        let mut result = Vec::new();

        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());
            if let Some(dependents) = rev_edges.get(node) {
                let mut next: Vec<&str> = Vec::new();
                for &dep in dependents {
                    if let Some(deg) = out_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            next.push(dep);
                        }
                    }
                }
                next.sort();
                queue.extend(next);
            }
        }

        let total_nodes = out_degree.len();
        if result.len() != total_nodes {
            // Find a node still with nonzero out_degree to report in the error.
            let stuck = out_degree
                .iter()
                .find(|&(_, &deg)| deg > 0)
                .map(|(&n, _)| n.to_string())
                .unwrap_or_default();
            return Err(CallGraphError::Cycle(stuck));
        }

        Ok(result)
    }

    /// Direct callees of a function.
    pub fn callees(&self, function_id: &str) -> HashSet<String> {
        self.edges
            .get(function_id)
            .cloned()
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// BehaviorCoverage
// ---------------------------------------------------------------------------

/// Tracks which of a callee's behaviors were exercised when testing a caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorCoverage {
    pub caller: String,
    pub callee: String,
    /// Indices into the callee's `BehaviorMap.behaviors` that were matched.
    pub exercised_behavior_ids: Vec<u32>,
    pub total_behaviors: u32,
}

impl BehaviorCoverage {
    /// Determine which of the callee's behaviors were exercised by the caller.
    ///
    /// Matches [`ExternalCall`] return values from the caller's records against
    /// the callee's behavior return values.
    pub fn compute(
        caller_id: &str,
        caller_records: &[ExecutionRecord],
        callee_map: &BehaviorMap,
    ) -> Self {
        let mut exercised: HashSet<u32> = HashSet::new();

        // Collect all external call return values targeting the callee.
        let call_returns: Vec<&serde_json::Value> = caller_records
            .iter()
            .flat_map(|r| &r.calls_to_external)
            .filter(|call| call.symbol == callee_map.function_id)
            .map(|call| &call.return_value)
            .collect();

        for behavior in &callee_map.behaviors {
            if let Some(ref ret) = behavior.return_value
                && call_returns.contains(&ret)
            {
                exercised.insert(behavior.id);
            }
        }

        let mut exercised_ids: Vec<u32> = exercised.into_iter().collect();
        exercised_ids.sort();

        Self {
            caller: caller_id.to_string(),
            callee: callee_map.function_id.clone(),
            exercised_behavior_ids: exercised_ids,
            total_behaviors: callee_map.behaviors.len() as u32,
        }
    }
}

// ---------------------------------------------------------------------------
// CompositeResult
// ---------------------------------------------------------------------------

/// Bundles a function's behavior map with coverage information for its callees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeResult {
    pub function_id: String,
    pub behavior_map: BehaviorMap,
    pub behavior_coverage: Vec<BehaviorCoverage>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::execution_record::ExternalCall;
    use crate::protocol::{DependencyKind, ExternalDependency, FunctionAnalysis};
    use crate::types::TypeInfo;

    /// Helper: build a minimal execution record with the given parameters and return value.
    fn make_record(
        function_id: &str,
        input_hash: u64,
        params: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
    ) -> ExecutionRecord {
        ExecutionRecord {
            function_id: function_id.to_string(),
            input_hash,
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

    /// Helper: build a minimal FunctionAnalysis.
    fn make_analysis(name: &str, deps: Vec<&str>) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            params: vec![],
            branches: vec![],
            dependencies: deps
                .into_iter()
                .map(|d| ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: d.to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                })
                .collect(),
            return_type: TypeInfo::Unknown,
            start_line: 0,
            end_line: 0,
        }
    }

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    #[test]
    fn behavior_map_from_single_record() {
        let record = make_record("foo", 1, vec![json!(42)], Some(json!(true)));
        let map = BehaviorMap::from_records("foo", &[record]);

        assert_eq!(map.function_id, "foo");
        assert_eq!(map.behaviors.len(), 1);
        assert_eq!(map.behaviors[0].id, 0);
        assert_eq!(map.behaviors[0].input_args, vec![json!(42)]);
        assert_eq!(map.behaviors[0].return_value, Some(json!(true)));
    }

    #[test]
    fn behavior_map_from_multiple_records_deduplicates() {
        let records = vec![
            make_record("foo", 1, vec![json!(1)], Some(json!("a"))),
            make_record("foo", 1, vec![json!(1)], Some(json!("a"))), // duplicate hash
            make_record("foo", 2, vec![json!(2)], Some(json!("b"))),
        ];
        let map = BehaviorMap::from_records("foo", &records);

        assert_eq!(map.behaviors.len(), 2);
        assert_eq!(map.behaviors[0].id, 0);
        assert_eq!(map.behaviors[1].id, 1);
    }

    #[test]
    fn behavior_map_to_mock_config() {
        let records = vec![
            make_record("calc", 1, vec![json!("gold")], Some(json!(0.2))),
            make_record("calc", 2, vec![json!("silver")], Some(json!(0.1))),
            make_record("calc", 3, vec![json!("bronze")], None), // no return value (error case)
        ];
        let map = BehaviorMap::from_records("calc", &records);
        let mock = map.to_mock_config();

        assert_eq!(mock.symbol, "calc");
        assert_eq!(mock.return_values, vec![json!(0.2), json!(0.1)]);
        assert!(mock.should_track_calls);
        assert_eq!(mock.default_behavior, MockBehavior::RepeatLast);
    }

    #[test]
    fn call_graph_from_analyses_builds_edges() {
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);

        assert!(graph.callees("a").contains("b"));
        assert!(graph.callees("b").is_empty());
    }

    #[test]
    fn call_graph_test_order_leaf_first() {
        let analyses = vec![
            make_analysis("calculateTotal", vec!["getDiscount"]),
            make_analysis("getDiscount", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("no cycle");

        let pos_discount = order.iter().position(|x| x == "getDiscount").unwrap();
        let pos_total = order.iter().position(|x| x == "calculateTotal").unwrap();
        assert!(
            pos_discount < pos_total,
            "getDiscount should come before calculateTotal, got: {order:?}"
        );
    }

    #[test]
    fn call_graph_test_order_detects_cycle() {
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec!["a"]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let result = graph.test_order();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CallGraphError::Cycle(_)));
    }

    #[test]
    fn behavior_coverage_matches_exercised_behaviors() {
        // callee has 3 behaviors
        let callee_records = vec![
            make_record("getDiscount", 1, vec![json!("gold")], Some(json!(0.2))),
            make_record("getDiscount", 2, vec![json!("silver")], Some(json!(0.1))),
            make_record("getDiscount", 3, vec![json!("default")], Some(json!(0.0))),
        ];
        let callee_map = BehaviorMap::from_records("getDiscount", &callee_records);

        // caller exercises behaviors 0 and 2 (gold and default)
        let mut caller_record = make_record("calculateTotal", 10, vec![json!(100)], Some(json!(80.0)));
        caller_record.calls_to_external = vec![
            ExternalCall {
                symbol: "getDiscount".to_string(),
                args: vec![json!("gold")],
                return_value: json!(0.2),
            },
            ExternalCall {
                symbol: "getDiscount".to_string(),
                args: vec![json!("default")],
                return_value: json!(0.0),
            },
        ];

        let coverage = BehaviorCoverage::compute("calculateTotal", &[caller_record], &callee_map);

        assert_eq!(coverage.caller, "calculateTotal");
        assert_eq!(coverage.callee, "getDiscount");
        assert_eq!(coverage.exercised_behavior_ids, vec![0, 2]);
        assert_eq!(coverage.total_behaviors, 3);
    }

    #[test]
    fn behavior_coverage_no_match() {
        let callee_records = vec![
            make_record("helper", 1, vec![json!(1)], Some(json!("x"))),
        ];
        let callee_map = BehaviorMap::from_records("helper", &callee_records);

        // caller doesn't call helper at all
        let caller_record = make_record("main", 10, vec![], Some(json!(null)));

        let coverage = BehaviorCoverage::compute("main", &[caller_record], &callee_map);

        assert_eq!(coverage.exercised_behavior_ids, Vec::<u32>::new());
        assert_eq!(coverage.total_behaviors, 1);
    }

    #[test]
    fn composite_result_round_trips() {
        let result = CompositeResult {
            function_id: "calculateTotal".to_string(),
            behavior_map: BehaviorMap {
                function_id: "calculateTotal".to_string(),
                behaviors: vec![Behavior {
                    id: 0,
                    input_args: vec![json!(100)],
                    return_value: Some(json!(80.0)),
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                }],
            },
            behavior_coverage: vec![BehaviorCoverage {
                caller: "calculateTotal".to_string(),
                callee: "getDiscount".to_string(),
                exercised_behavior_ids: vec![0, 1],
                total_behaviors: 3,
            }],
        };

        round_trip(&result);
    }

    #[test]
    fn two_function_chain_integration() {
        // Step 1: Create execution records for getDiscount
        let discount_records = vec![
            make_record("getDiscount", 100, vec![json!("gold")], Some(json!(0.2))),
            make_record("getDiscount", 101, vec![json!("silver")], Some(json!(0.1))),
            make_record("getDiscount", 102, vec![json!("default")], Some(json!(0.0))),
        ];

        // Step 2: Build BehaviorMap
        let discount_map = BehaviorMap::from_records("getDiscount", &discount_records);
        assert_eq!(discount_map.behaviors.len(), 3);

        // Step 3: Build CallGraph
        let analyses = vec![
            make_analysis("calculateTotal", vec!["getDiscount"]),
            make_analysis("getDiscount", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);

        // Step 4: Verify test order
        let order = graph.test_order().expect("no cycle");
        assert_eq!(order, vec!["getDiscount", "calculateTotal"]);

        // Step 5: Convert BehaviorMap to MockConfig
        let mock = discount_map.to_mock_config();
        assert_eq!(mock.symbol, "getDiscount");
        assert_eq!(mock.return_values.len(), 3);

        // Step 6: Create execution records for calculateTotal using the mock
        let mut total_record_1 = make_record(
            "calculateTotal",
            200,
            vec![json!(100), json!("gold")],
            Some(json!(80.0)),
        );
        total_record_1.calls_to_external = vec![ExternalCall {
            symbol: "getDiscount".to_string(),
            args: vec![json!("gold")],
            return_value: json!(0.2),
        }];

        let mut total_record_2 = make_record(
            "calculateTotal",
            201,
            vec![json!(50), json!("silver")],
            Some(json!(45.0)),
        );
        total_record_2.calls_to_external = vec![ExternalCall {
            symbol: "getDiscount".to_string(),
            args: vec![json!("silver")],
            return_value: json!(0.1),
        }];

        let caller_records = vec![total_record_1, total_record_2];

        // Step 7: Compute BehaviorCoverage
        let coverage = BehaviorCoverage::compute("calculateTotal", &caller_records, &discount_map);
        assert_eq!(coverage.exercised_behavior_ids, vec![0, 1]); // gold and silver
        assert_eq!(coverage.total_behaviors, 3);

        // Step 8: Build CompositeResult
        let total_map = BehaviorMap::from_records("calculateTotal", &caller_records);
        let composite = CompositeResult {
            function_id: "calculateTotal".to_string(),
            behavior_map: total_map,
            behavior_coverage: vec![coverage],
        };

        assert_eq!(composite.behavior_coverage.len(), 1);
        assert_eq!(composite.behavior_coverage[0].exercised_behavior_ids, vec![0, 1]);

        // Verify round-trip
        round_trip(&composite);
    }

}
