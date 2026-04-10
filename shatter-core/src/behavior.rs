//! Behavior maps, call graphs, and compositional testing logic.
//!
//! When the concolic engine tests function A that calls function B, behavior maps
//! let us reuse prior knowledge about B. A [`BehaviorMap`] records B's observed
//! input→output mappings so that when testing A, B is mocked using its known
//! behaviors. [`CallGraph`] orders functions for testing (leaves first), and
//! [`BehaviorCoverage`] tracks which of B's behaviors A actually exercises.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::execution_record::{
    BranchDecision, ErrorInfo, ExternalCall, ExecutionRecord, SideEffect,
};
use crate::orchestrator::hash_branch_path;
use crate::protocol::{ExecuteResult, MockBehavior, MockConfig};

// ---------------------------------------------------------------------------
// DependencyTrace
// ---------------------------------------------------------------------------

/// A single call to an external dependency, captured with ordering information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TracedCall {
    /// Name of the called function/symbol.
    pub function_name: String,
    /// Arguments passed to the call.
    pub arguments: Vec<serde_json::Value>,
    /// Return value from the call.
    pub return_value: serde_json::Value,
    /// Zero-based index indicating the order of this call relative to all
    /// dependency interactions (calls and side effects combined).
    pub call_index: u32,
}

/// Categorization of a side effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectKind {
    ConsoleOutput,
    FileWrite,
    NetworkRequest,
    EnvironmentRead,
    GlobalMutation,
    ThrownError,
    GlobalStateChange,
}

/// A side effect observed during execution, captured with ordering information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TracedSideEffect {
    /// What category of side effect this is.
    pub kind: SideEffectKind,
    /// Human-readable description of the side effect.
    pub description: String,
    /// Zero-based index indicating the order of this side effect relative to
    /// all dependency interactions (calls and side effects combined).
    pub call_index: u32,
}

/// Full dependency interaction trace for a single execution.
///
/// Captures every external call and side effect in order, enabling
/// compositional reasoning about how a function interacts with its
/// dependencies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyTrace {
    /// External calls made during execution, in order.
    pub external_calls: Vec<TracedCall>,
    /// Side effects observed during execution, in order.
    pub side_effects: Vec<TracedSideEffect>,
    /// Total number of dependency interactions (calls + side effects),
    /// useful for verifying ordering completeness.
    pub call_ordering: u32,
}

/// Classify a [`SideEffect`] into a [`SideEffectKind`].
fn side_effect_kind(effect: &SideEffect) -> SideEffectKind {
    match effect {
        SideEffect::ConsoleOutput { .. } => SideEffectKind::ConsoleOutput,
        SideEffect::FileWrite { .. } => SideEffectKind::FileWrite,
        SideEffect::NetworkRequest { .. } => SideEffectKind::NetworkRequest,
        SideEffect::EnvironmentRead { .. } => SideEffectKind::EnvironmentRead,
        SideEffect::GlobalMutation { .. } => SideEffectKind::GlobalMutation,
        SideEffect::ThrownError { .. } => SideEffectKind::ThrownError,
        SideEffect::GlobalStateChange { .. } => SideEffectKind::GlobalStateChange,
    }
}

/// Produce a human-readable description of a [`SideEffect`].
fn side_effect_description(effect: &SideEffect) -> String {
    match effect {
        SideEffect::ConsoleOutput { level, message } => {
            format!("console.{level}: {message}")
        }
        SideEffect::FileWrite { path, .. } => {
            format!("file write: {path}")
        }
        SideEffect::NetworkRequest { method, url, .. } => {
            format!("{method} {url}")
        }
        SideEffect::EnvironmentRead { variable, value } => {
            let val = value.as_deref().unwrap_or("null");
            format!("env read: {variable}={val}")
        }
        SideEffect::GlobalMutation { name } => {
            format!("mutated global: {name}")
        }
        SideEffect::ThrownError {
            error_type,
            message,
            ..
        } => {
            format!("{error_type}: {message}")
        }
        SideEffect::GlobalStateChange {
            variable,
            before,
            after,
        } => {
            format!("{variable}: {before} -> {after}")
        }
    }
}

/// Build [`TracedCall`]s from a slice of [`ExternalCall`]s, starting at the
/// given call index offset.
fn build_traced_calls(calls: &[ExternalCall], start_index: u32) -> Vec<TracedCall> {
    calls
        .iter()
        .enumerate()
        .map(|(i, call)| TracedCall {
            function_name: call.symbol.clone(),
            arguments: call.args.clone(),
            return_value: call.return_value.clone(),
            call_index: start_index + i as u32,
        })
        .collect()
}

/// Build [`TracedSideEffect`]s from a slice of [`SideEffect`]s, starting at
/// the given call index offset.
fn build_traced_side_effects(
    effects: &[SideEffect],
    start_index: u32,
) -> Vec<TracedSideEffect> {
    effects
        .iter()
        .enumerate()
        .map(|(i, effect)| TracedSideEffect {
            kind: side_effect_kind(effect),
            description: side_effect_description(effect),
            call_index: start_index + i as u32,
        })
        .collect()
}

/// Build a [`DependencyTrace`] from an [`ExecuteResult`].
///
/// External calls are indexed first, then side effects.
pub fn build_dependency_trace(result: &ExecuteResult) -> DependencyTrace {
    let external_calls = build_traced_calls(&result.calls_to_external, 0);
    let se_start = external_calls.len() as u32;
    let side_effects = build_traced_side_effects(&result.side_effects, se_start);
    let call_ordering = se_start + side_effects.len() as u32;

    DependencyTrace {
        external_calls,
        side_effects,
        call_ordering,
    }
}

/// Build a [`DependencyTrace`] from an [`ExecutionRecord`].
///
/// External calls are indexed first, then side effects.
pub fn build_dependency_trace_from_record(record: &ExecutionRecord) -> DependencyTrace {
    let external_calls = build_traced_calls(&record.calls_to_external, 0);
    let se_start = external_calls.len() as u32;
    let side_effects = build_traced_side_effects(&record.side_effects, se_start);
    let call_ordering = se_start + side_effects.len() as u32;

    DependencyTrace {
        external_calls,
        side_effects,
        call_ordering,
    }
}

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
    /// Full dependency interaction trace, present when the execution involved
    /// external calls or side effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_trace: Option<DependencyTrace>,
    /// Mock configurations active during this execution.
    /// Records which mock values produced this behavior, enabling downstream
    /// consumers (export, spec) to reproduce the execution context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mock_values: Vec<MockConfig>,
}

/// All observed behaviors for a function, built from execution records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorMap {
    pub function_id: String,
    pub behaviors: Vec<Behavior>,
    /// SHA-256 fingerprint of the function's source, params, and branches.
    /// Used for staleness detection: if the fingerprint matches a cached value,
    /// the function is unchanged and can be skipped during re-exploration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Fields identified as nondeterministic during exploration.
    /// Populated from the nondeterminism detection report when available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
}

impl BehaviorMap {
    /// Attach a fingerprint to this map for staleness detection.
    ///
    /// Callers should set this to the current
    /// [`compute_function_fingerprint`](crate::fingerprint::compute_function_fingerprint)
    /// output before persisting the map, so that
    /// [`BehaviorMapCache::is_fresh`](crate::cache::BehaviorMapCache::is_fresh)
    /// can detect when the underlying function body has changed.
    pub fn set_fingerprint(&mut self, fingerprint: impl Into<String>) {
        self.fingerprint = Some(fingerprint.into());
    }

    /// Build a behavior map from execution records, deduplicating by `input_hash`.
    pub fn from_records(function_id: impl Into<String>, records: &[ExecutionRecord]) -> Self {
        let mut seen_hashes = HashSet::new();
        let mut behaviors = Vec::new();
        let mut next_id: u32 = 0;

        for record in records {
            if !seen_hashes.insert(record.input_hash) {
                continue;
            }
            let dependency_trace =
                if record.calls_to_external.is_empty() && record.side_effects.is_empty() {
                    None
                } else {
                    Some(build_dependency_trace_from_record(record))
                };
            behaviors.push(Behavior {
                id: next_id,
                input_args: record.parameters.clone(),
                return_value: record.return_value.clone(),
                thrown_error: record.thrown_error.clone(),
                branch_path: record.branch_path.clone(),
                side_effects: record.side_effects.clone(),
                dependency_trace,
                mock_values: vec![],
            });
            next_id += 1;
        }

        Self {
            function_id: function_id.into(),
            behaviors,
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    /// Build a behavior map from an [`ObservationOutput`](crate::explorer::ObservationOutput).
    ///
    /// Converts each [`ExecutionSummary`](crate::explorer::ExecutionSummary) that discovered
    /// a new path into a [`Behavior`] entry. When raw results are available,
    /// dependency traces are populated from the matching [`ExecuteResult`].
    pub fn from_exploration_result(
        function_id: impl Into<String>,
        result: &crate::explorer::ObservationOutput,
    ) -> Self {
        let behaviors = result
            .new_path_executions
            .iter()
            .enumerate()
            .map(|(i, exec)| {
                let thrown_error = exec.thrown_error.as_ref().map(|msg| ErrorInfo {
                    error_type: "Error".to_string(),
                    message: msg.clone(),
                    stack: None, error_category: None });
                // Try to find the matching raw result for this execution to
                // extract the dependency trace and mock values.
                let matching_raw = result
                    .raw_results
                    .iter()
                    .find(|(inputs, _mocks, _)| *inputs == exec.inputs);
                let dependency_trace = matching_raw
                    .filter(|(_, _mocks, res)| {
                        !res.calls_to_external.is_empty() || !res.side_effects.is_empty()
                    })
                    .map(|(_, _mocks, res)| build_dependency_trace(res));
                let mock_values = matching_raw
                    .map(|(_, mocks, _)| mocks.clone())
                    .unwrap_or_default();
                Behavior {
                    id: i as u32,
                    input_args: exec.inputs.clone(),
                    return_value: exec.return_value.clone(),
                    thrown_error,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace,
                    mock_values,
                }
            })
            .collect();

        Self {
            function_id: function_id.into(),
            behaviors,
            fingerprint: None,
            nondeterministic_fields: result.nondeterministic_fields.clone(),
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

    /// Merge GA-discovered behaviors into this map, deduplicating by branch path hash.
    ///
    /// Only behaviors whose `branch_path` hash is not already represented in the
    /// map are added. Returns the number of newly added behaviors.
    pub fn merge_ga_discoveries(&mut self, discoveries: &[Behavior]) -> usize {
        let mut seen: HashSet<u64> =
            self.behaviors.iter().map(|b| hash_branch_path(&b.branch_path)).collect();

        let mut next_id = self.behaviors.iter().map(|b| b.id).max().map_or(0, |m| m + 1);
        let mut added = 0usize;

        for discovery in discoveries {
            let path_hash = hash_branch_path(&discovery.branch_path);
            if seen.insert(path_hash) {
                let mut behavior = discovery.clone();
                behavior.id = next_id;
                next_id += 1;
                self.behaviors.push(behavior);
                added += 1;
            }
        }

        added
    }

    /// Extract all input argument vectors from this behavior map's behaviors.
    ///
    /// Returns one `Vec<serde_json::Value>` per behavior, suitable for use as
    /// seed inputs on subsequent exploration runs. Behaviors with empty
    /// `input_args` (e.g. void-parameter functions) are filtered out.
    pub fn extract_seed_inputs(&self) -> Vec<Vec<serde_json::Value>> {
        self.behaviors
            .iter()
            .filter(|b| !b.input_args.is_empty())
            .map(|b| b.input_args.clone())
            .collect()
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

/// An entry in the test order, representing either a single function or a
/// mutually-recursive group that must be tested together.
#[derive(Debug, Clone, PartialEq)]
pub enum TestOrderEntry {
    /// A single function (possibly self-recursive).
    Single {
        function_id: String,
        is_self_recursive: bool,
    },
    /// A group of mutually-recursive functions that must be tested together.
    MutualGroup {
        /// Function IDs in the group, sorted for determinism.
        function_ids: Vec<String>,
    },
}

/// Dependency graph built from [`FunctionAnalysis`] results.
///
/// Used to compute test ordering so leaf functions are tested first.
/// Self-recursive functions (functions that call themselves) are detected
/// and annotated rather than rejected as cycles.
pub struct CallGraph {
    /// function_id → set of function_ids it calls (excluding self-calls).
    edges: HashMap<String, HashSet<String>>,
    /// Functions that call themselves.
    self_recursive: HashSet<String>,
}

/// Internal state for Tarjan's SCC algorithm.
#[derive(Default)]
struct TarjanState<'a> {
    index_counter: u32,
    stack: Vec<&'a str>,
    on_stack: HashSet<&'a str>,
    indices: HashMap<&'a str, u32>,
    lowlinks: HashMap<&'a str, u32>,
    result: Vec<Vec<String>>,
}

impl CallGraph {
    /// Build a call graph from function analyses.
    ///
    /// Matches each function's `ExternalDependency.symbol` against the set of
    /// known function names to build edges. Self-calls are detected and stored
    /// separately rather than as regular edges.
    pub fn from_analyses(analyses: &[crate::protocol::FunctionAnalysis]) -> Self {
        let known_names: HashSet<&str> = analyses.iter().map(|a| a.name.as_str()).collect();
        let mut edges = HashMap::new();
        let mut self_recursive = HashSet::new();

        for analysis in analyses {
            let mut callees: HashSet<String> = HashSet::new();
            for dep in &analysis.dependencies {
                if !known_names.contains(dep.symbol.as_str()) {
                    continue;
                }
                if dep.symbol == analysis.name {
                    self_recursive.insert(analysis.name.clone());
                } else {
                    callees.insert(dep.symbol.clone());
                }
            }
            edges.insert(analysis.name.clone(), callees);
        }

        Self {
            edges,
            self_recursive,
        }
    }

    /// Compute strongly connected components using Tarjan's algorithm.
    ///
    /// Returns SCCs in reverse topological order (leaves first).
    fn strongly_connected_components(&self) -> Vec<Vec<String>> {
        // Collect all nodes (sorted for determinism)
        let mut all_nodes: Vec<&str> = self.edges.keys().map(|s| s.as_str()).collect();
        for callees in self.edges.values() {
            for callee in callees {
                all_nodes.push(callee.as_str());
            }
        }
        all_nodes.sort();
        all_nodes.dedup();

        let mut state = TarjanState::default();

        for &node in &all_nodes {
            if !state.indices.contains_key(node) {
                self.tarjan_visit(node, &mut state);
            }
        }

        state.result
    }

    fn tarjan_visit<'a>(&'a self, node: &'a str, state: &mut TarjanState<'a>) {
        state.indices.insert(node, state.index_counter);
        state.lowlinks.insert(node, state.index_counter);
        state.index_counter += 1;
        state.stack.push(node);
        state.on_stack.insert(node);

        // Visit successors (sorted for determinism)
        if let Some(callees) = self.edges.get(node) {
            let mut sorted_callees: Vec<&str> = callees.iter().map(|s| s.as_str()).collect();
            sorted_callees.sort();
            for callee in sorted_callees {
                if !state.indices.contains_key(callee) {
                    self.tarjan_visit(callee, state);
                    let callee_low = state.lowlinks[callee];
                    let node_low = state.lowlinks.get_mut(node).expect("node in lowlinks");
                    *node_low = (*node_low).min(callee_low);
                } else if state.on_stack.contains(callee) {
                    let callee_idx = state.indices[callee];
                    let node_low = state.lowlinks.get_mut(node).expect("node in lowlinks");
                    *node_low = (*node_low).min(callee_idx);
                }
            }
        }

        // If node is a root of an SCC
        if state.lowlinks[node] == state.indices[node] {
            let mut component = Vec::new();
            loop {
                let w = state.stack.pop().expect("stack not empty");
                state.on_stack.remove(w);
                component.push(w.to_string());
                if w == node {
                    break;
                }
            }
            component.sort(); // Deterministic ordering within SCC
            state.result.push(component);
        }
    }

    /// Topological sort returning leaf functions first.
    ///
    /// Self-recursive functions are returned as `Single` entries with
    /// `is_self_recursive: true`. Mutually-recursive function groups are
    /// returned as `MutualGroup` entries. Non-recursive functions are
    /// returned as `Single` entries.
    pub fn test_order(&self) -> Result<Vec<TestOrderEntry>, CallGraphError> {
        // Tarjan's algorithm returns SCCs in reverse topological order,
        // which is exactly what we want: leaves first.
        let sccs = self.strongly_connected_components();

        let mut result = Vec::new();
        for scc in sccs {
            if scc.len() == 1 {
                let id = &scc[0];
                result.push(TestOrderEntry::Single {
                    function_id: id.clone(),
                    is_self_recursive: self.self_recursive.contains(id),
                });
            } else {
                result.push(TestOrderEntry::MutualGroup {
                    function_ids: scc,
                });
            }
        }

        Ok(result)
    }

    /// Direct callees of a function (excluding self-calls).
    pub fn callees(&self, function_id: &str) -> HashSet<String> {
        self.edges
            .get(function_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether a function calls itself.
    pub fn is_self_recursive(&self, function_id: &str) -> bool {
        self.self_recursive.contains(function_id)
    }

    /// All functions detected as self-recursive.
    pub fn self_recursive_functions(&self) -> &HashSet<String> {
        &self.self_recursive
    }

    /// All nodes in the graph.
    pub fn nodes(&self) -> HashSet<&str> {
        let mut nodes: HashSet<&str> = self.edges.keys().map(|s| s.as_str()).collect();
        for callees in self.edges.values() {
            for callee in callees {
                nodes.insert(callee.as_str());
            }
        }
        nodes
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
    use crate::explorer::{ExecutionSummary, ObservationOutput};
    use crate::protocol::{DependencyKind, ExternalDependency, FunctionAnalysis, PerformanceMetrics};
    use crate::types::TypeInfo;

    #[test]
    fn set_fingerprint_stamps_and_replaces() {
        let mut map = BehaviorMap {
            function_id: "stamp_test".to_string(),
            behaviors: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        assert!(map.fingerprint.is_none());

        map.set_fingerprint("first");
        assert_eq!(map.fingerprint.as_deref(), Some("first"));

        map.set_fingerprint(String::from("second"));
        assert_eq!(map.fingerprint.as_deref(), Some("second"));
    }

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

    /// Helper: build a minimal FunctionAnalysis.
    fn make_analysis(name: &str, deps: Vec<&str>) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            exported: true,
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
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
    fn behavior_map_from_exploration_result() {
        let result = ObservationOutput {
            function_name: "classify".to_string(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![
                ExecutionSummary {
                    inputs: vec![json!(5)],
                    return_value: Some(json!("positive")),
                    thrown_error: None,
                    lines_executed: vec![1, 2],
                    is_new_path: true, error_intent: None },
                ExecutionSummary {
                    inputs: vec![json!(-1)],
                    return_value: None,
                    thrown_error: Some("Error: negative input".to_string()),
                    lines_executed: vec![1, 3],
                    is_new_path: true, error_intent: None },
            ],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],

        };

        let map = BehaviorMap::from_exploration_result("classify", &result);
        assert_eq!(map.function_id, "classify");
        assert_eq!(map.behaviors.len(), 2);
        assert_eq!(map.behaviors[0].id, 0);
        assert_eq!(map.behaviors[0].input_args, vec![json!(5)]);
        assert_eq!(map.behaviors[0].return_value, Some(json!("positive")));
        assert!(map.behaviors[0].thrown_error.is_none());
        assert_eq!(map.behaviors[1].id, 1);
        assert!(map.behaviors[1].thrown_error.is_some());
        assert_eq!(
            map.behaviors[1].thrown_error.as_ref().unwrap().message,
            "Error: negative input"
        );
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

    /// Helper: extract function IDs from test order entries (flattening groups).
    fn entry_ids(entries: &[TestOrderEntry]) -> Vec<String> {
        let mut ids = Vec::new();
        for entry in entries {
            match entry {
                TestOrderEntry::Single { function_id, .. } => ids.push(function_id.clone()),
                TestOrderEntry::MutualGroup { function_ids } => ids.extend(function_ids.clone()),
            }
        }
        ids
    }

    #[test]
    fn call_graph_test_order_leaf_first() {
        let analyses = vec![
            make_analysis("calculateTotal", vec!["getDiscount"]),
            make_analysis("getDiscount", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("no cycle");
        let ids = entry_ids(&order);

        let pos_discount = ids.iter().position(|x| x == "getDiscount").unwrap();
        let pos_total = ids.iter().position(|x| x == "calculateTotal").unwrap();
        assert!(
            pos_discount < pos_total,
            "getDiscount should come before calculateTotal, got: {ids:?}"
        );
        // Both should be Single, non-recursive
        assert!(matches!(&order[0], TestOrderEntry::Single { is_self_recursive: false, .. }));
        assert!(matches!(&order[1], TestOrderEntry::Single { is_self_recursive: false, .. }));
    }

    #[test]
    fn call_graph_mutual_recursion_returns_group() {
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec!["a"]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("mutual recursion should not error");

        assert_eq!(order.len(), 1);
        match &order[0] {
            TestOrderEntry::MutualGroup { function_ids } => {
                assert_eq!(function_ids.len(), 2);
                assert!(function_ids.contains(&"a".to_string()));
                assert!(function_ids.contains(&"b".to_string()));
            }
            other => panic!("expected MutualGroup, got: {other:?}"),
        }
    }

    #[test]
    fn call_graph_self_recursive_not_rejected_as_cycle() {
        let analyses = vec![
            make_analysis("factorial", vec!["factorial"]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("self-recursion should not be a cycle");

        assert_eq!(order.len(), 1);
        match &order[0] {
            TestOrderEntry::Single { function_id, is_self_recursive } => {
                assert_eq!(function_id, "factorial");
                assert!(is_self_recursive);
            }
            other => panic!("expected Single, got: {other:?}"),
        }
    }

    #[test]
    fn call_graph_self_recursive_with_dependency() {
        // factorial calls itself AND helper; helper is a leaf
        let analyses = vec![
            make_analysis("factorial", vec!["factorial", "helper"]),
            make_analysis("helper", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("no cycle");
        let ids = entry_ids(&order);

        // helper should come before factorial
        let pos_helper = ids.iter().position(|x| x == "helper").unwrap();
        let pos_factorial = ids.iter().position(|x| x == "factorial").unwrap();
        assert!(pos_helper < pos_factorial);

        // helper is non-recursive Single, factorial is self-recursive Single
        assert!(matches!(&order[0], TestOrderEntry::Single { function_id, is_self_recursive: false } if function_id == "helper"));
        assert!(matches!(&order[1], TestOrderEntry::Single { function_id, is_self_recursive: true } if function_id == "factorial"));
    }

    #[test]
    fn call_graph_mutual_recursion_with_leaf_dependency() {
        // is_even calls is_odd and helper; is_odd calls is_even; helper is a leaf
        let analyses = vec![
            make_analysis("is_even", vec!["is_odd", "helper"]),
            make_analysis("is_odd", vec!["is_even"]),
            make_analysis("helper", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);
        let order = graph.test_order().expect("no error");
        let ids = entry_ids(&order);

        // helper should come before the mutual group
        let pos_helper = ids.iter().position(|x| x == "helper").unwrap();
        let pos_even = ids.iter().position(|x| x == "is_even").unwrap();
        assert!(pos_helper < pos_even);

        // The mutual group should contain is_even and is_odd
        let group_entry = order.iter().find(|e| matches!(e, TestOrderEntry::MutualGroup { .. }));
        assert!(group_entry.is_some());
        match group_entry.unwrap() {
            TestOrderEntry::MutualGroup { function_ids } => {
                assert!(function_ids.contains(&"is_even".to_string()));
                assert!(function_ids.contains(&"is_odd".to_string()));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn call_graph_self_edges_excluded_from_callees() {
        let analyses = vec![
            make_analysis("factorial", vec!["factorial", "helper"]),
            make_analysis("helper", vec![]),
        ];
        let graph = CallGraph::from_analyses(&analyses);

        // callees() should not include self
        let callees = graph.callees("factorial");
        assert!(callees.contains("helper"));
        assert!(!callees.contains("factorial"));
        assert!(graph.is_self_recursive("factorial"));
        assert!(!graph.is_self_recursive("helper"));
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
                    dependency_trace: None,
                    mock_values: vec![],
                }],
                fingerprint: None,
                nondeterministic_fields: vec![],
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
        let ids = entry_ids(&order);
        assert_eq!(ids, vec!["getDiscount", "calculateTotal"]);

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

    // -----------------------------------------------------------------------
    // DependencyTrace tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_dependency_trace_from_execute_result() {
        let result = ExecuteResult {
            return_value: Some(json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![
                ExternalCall {
                    symbol: "getRate".to_string(),
                    args: vec![json!("express")],
                    return_value: json!(12.99),
                },
                ExternalCall {
                    symbol: "applyTax".to_string(),
                    args: vec![json!(12.99)],
                    return_value: json!(14.29),
                },
            ],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 800,
                heap_used_bytes: 512,
                heap_allocated_bytes: 1024,
            },
        };

        let trace = build_dependency_trace(&result);
        assert_eq!(trace.external_calls.len(), 2);
        assert_eq!(trace.external_calls[0].function_name, "getRate");
        assert_eq!(trace.external_calls[0].call_index, 0);
        assert_eq!(trace.external_calls[1].function_name, "applyTax");
        assert_eq!(trace.external_calls[1].call_index, 1);
        assert_eq!(trace.side_effects.len(), 0);
        assert_eq!(trace.call_ordering, 2);
    }

    #[test]
    fn dependency_trace_captures_call_ordering() {
        let result = ExecuteResult {
            return_value: Some(json!("ok")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![ExternalCall {
                symbol: "fetch".to_string(),
                args: vec![json!("https://api.example.com")],
                return_value: json!({"status": 200}),
            }],
            path_constraints: vec![],
            side_effects: vec![SideEffect::ConsoleOutput {
                level: "info".to_string(),
                message: "fetching data".to_string(),
            }],
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 5.0,
                cpu_time_us: 3000,
                heap_used_bytes: 256,
                heap_allocated_bytes: 512,
            },
        };

        let trace = build_dependency_trace(&result);
        assert_eq!(trace.external_calls.len(), 1);
        assert_eq!(trace.side_effects.len(), 1);
        assert_eq!(trace.external_calls[0].call_index, 0);
        assert_eq!(trace.side_effects[0].call_index, 1);
        assert_eq!(trace.side_effects[0].kind, SideEffectKind::ConsoleOutput);
        assert_eq!(trace.call_ordering, 2);
    }

    #[test]
    fn behavior_map_entries_include_traces_when_available() {
        let mut record = make_record("myFunc", 1, vec![json!(10)], Some(json!("ok")));
        record.calls_to_external = vec![ExternalCall {
            symbol: "helper".to_string(),
            args: vec![json!(10)],
            return_value: json!(true),
        }];

        let map = BehaviorMap::from_records("myFunc", &[record]);
        assert_eq!(map.behaviors.len(), 1);
        assert!(map.behaviors[0].dependency_trace.is_some());

        let trace = map.behaviors[0].dependency_trace.as_ref().unwrap();
        assert_eq!(trace.external_calls.len(), 1);
        assert_eq!(trace.external_calls[0].function_name, "helper");
    }

    #[test]
    fn behavior_map_no_trace_when_no_deps() {
        let record = make_record("pureFunc", 1, vec![json!(5)], Some(json!(10)));
        let map = BehaviorMap::from_records("pureFunc", &[record]);
        assert_eq!(map.behaviors.len(), 1);
        assert!(map.behaviors[0].dependency_trace.is_none());
    }

    #[test]
    fn dependency_trace_round_trips() {
        let trace = DependencyTrace {
            external_calls: vec![TracedCall {
                function_name: "getRate".to_string(),
                arguments: vec![json!("express")],
                return_value: json!(12.99),
                call_index: 0,
            }],
            side_effects: vec![TracedSideEffect {
                kind: SideEffectKind::ConsoleOutput,
                description: "console.info: log msg".to_string(),
                call_index: 1,
            }],
            call_ordering: 2,
        };
        round_trip(&trace);
    }

    #[test]
    fn empty_trace_no_external_calls() {
        let result = ExecuteResult {
            return_value: Some(json!(1)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 0.1,
                cpu_time_us: 50,
                heap_used_bytes: 64,
                heap_allocated_bytes: 128,
            },
        };

        let trace = build_dependency_trace(&result);
        assert_eq!(trace.external_calls.len(), 0);
        assert_eq!(trace.side_effects.len(), 0);
        assert_eq!(trace.call_ordering, 0);
    }

    #[test]
    fn traced_call_round_trips() {
        round_trip(&TracedCall {
            function_name: "compute".to_string(),
            arguments: vec![json!(1), json!("two")],
            return_value: json!(3),
            call_index: 0,
        });
    }

    #[test]
    fn traced_side_effect_round_trips() {
        round_trip(&TracedSideEffect {
            kind: SideEffectKind::FileWrite,
            description: "file write: /tmp/out.txt".to_string(),
            call_index: 2,
        });
    }

    #[test]
    fn all_side_effect_kinds_round_trip() {
        let kinds = vec![
            SideEffectKind::ConsoleOutput,
            SideEffectKind::FileWrite,
            SideEffectKind::NetworkRequest,
            SideEffectKind::GlobalMutation,
            SideEffectKind::ThrownError,
            SideEffectKind::GlobalStateChange,
        ];
        for kind in kinds {
            round_trip(&kind);
        }
    }

    #[test]
    fn dependency_trace_with_side_effects_only() {
        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![
                SideEffect::FileWrite {
                    path: "/tmp/out.txt".to_string(),
                    content: None,
                },
                SideEffect::NetworkRequest {
                    method: "POST".to_string(),
                    url: "https://api.example.com".to_string(),
                    body: None,
                },
            ],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 2.0,
                cpu_time_us: 1500,
                heap_used_bytes: 128,
                heap_allocated_bytes: 256,
            },
        };

        let trace = build_dependency_trace(&result);
        assert_eq!(trace.external_calls.len(), 0);
        assert_eq!(trace.side_effects.len(), 2);
        assert_eq!(trace.side_effects[0].kind, SideEffectKind::FileWrite);
        assert_eq!(trace.side_effects[0].call_index, 0);
        assert_eq!(trace.side_effects[1].kind, SideEffectKind::NetworkRequest);
        assert_eq!(trace.side_effects[1].call_index, 1);
        assert_eq!(trace.call_ordering, 2);
    }

    #[test]
    fn from_exploration_result_populates_trace_from_raw_results() {
        let raw_result = ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2],
            calls_to_external: vec![ExternalCall {
                symbol: "logger".to_string(),
                args: vec![json!("classified")],
                return_value: json!(null),
            }],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 0.5,
                cpu_time_us: 300,
                heap_used_bytes: 64,
                heap_allocated_bytes: 128,
            },
        };

        let result = ObservationOutput {
            function_name: "classify".to_string(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 2,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![json!(5)],
                return_value: Some(json!("positive")),
                thrown_error: None,
                lines_executed: vec![1, 2],
                is_new_path: true, error_intent: None }],
            raw_results: vec![(vec![json!(5)], vec![], raw_result)],
            discoveries: vec![],
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],

        };

        let map = BehaviorMap::from_exploration_result("classify", &result);
        assert_eq!(map.behaviors.len(), 1);
        assert!(map.behaviors[0].dependency_trace.is_some());
        let trace = map.behaviors[0].dependency_trace.as_ref().unwrap();
        assert_eq!(trace.external_calls.len(), 1);
        assert_eq!(trace.external_calls[0].function_name, "logger");
    }

    #[test]
    fn behavior_map_nondeterministic_fields_round_trip() {
        use crate::nondeterminism::{Confidence, NondeterministicField, NondeterminismEvidence};

        let map = BehaviorMap {
            function_id: "fn1".to_string(),
            behaviors: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![NondeterministicField {
                field_path: "return.timestamp".to_string(),
                evidence: vec![NondeterminismEvidence::ObservedWithinRun],
                confidence: Confidence::High,
            }],
        };

        round_trip(&map);
    }

    #[test]
    fn behavior_map_without_nondeterministic_fields_deserializes() {
        // Backward compatibility: JSON without nondeterministic_fields should
        // deserialize with an empty vec via serde default.
        let json = r#"{"function_id":"fn1","behaviors":[]}"#;
        let map: BehaviorMap = serde_json::from_str(json).expect("deserialize");
        assert!(map.nondeterministic_fields.is_empty());
    }

    #[test]
    fn from_exploration_result_carries_nondeterministic_fields() {
        use crate::nondeterminism::{Confidence, NondeterministicField, NondeterminismEvidence};

        let result = ObservationOutput {
            function_name: "fn1".to_string(),
            iterations: 10,
            unique_paths: 1,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![NondeterministicField {
                field_path: "return.id".to_string(),
                evidence: vec![NondeterminismEvidence::ObservedWithinRun],
                confidence: Confidence::Medium,
            }],
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
        };

        let map = BehaviorMap::from_exploration_result("fn1", &result);
        assert_eq!(map.nondeterministic_fields.len(), 1);
        assert_eq!(map.nondeterministic_fields[0].field_path, "return.id");
    }

    /// Helper: build a Behavior with the given id and branch_path.
    fn make_behavior(id: u32, branch_path: Vec<BranchDecision>) -> Behavior {
        Behavior {
            id,
            input_args: vec![json!(id)],
            return_value: Some(json!(id)),
            thrown_error: None,
            branch_path,
            side_effects: vec![],
            dependency_trace: None,
            mock_values: vec![],
        }
    }

    fn make_branch(branch_id: u32, taken: bool) -> BranchDecision {
        BranchDecision {
            branch_id,
            line: 0,
            taken,
            constraint: Default::default(),
            conditions: None,
        }
    }

    #[test]
    fn merge_ga_discoveries_dedup_by_path_hash() {
        let path_a = vec![make_branch(1, true)];
        let path_b = vec![make_branch(2, false)];
        let path_c = vec![make_branch(3, true)];

        // Existing map has 2 behaviors with paths A and B.
        let mut map = BehaviorMap {
            function_id: "test_fn".to_string(),
            behaviors: vec![make_behavior(0, path_a.clone()), make_behavior(1, path_b.clone())],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };

        // GA discoveries: path_c is new, path_a is a duplicate.
        let discoveries = vec![make_behavior(0, path_c.clone()), make_behavior(1, path_a)];

        let added = map.merge_ga_discoveries(&discoveries);

        assert_eq!(added, 1, "only the new path should be added");
        assert_eq!(map.behaviors.len(), 3, "2 original + 1 new");
        assert_eq!(map.behaviors[2].id, 2, "new behavior gets next sequential id");
        assert_eq!(map.behaviors[2].branch_path, path_c);
    }

    #[test]
    fn extract_seed_inputs_collects_all_input_args() {
        let path_a = vec![make_branch(1, true)];
        let path_b = vec![make_branch(2, false)];

        let map = BehaviorMap {
            function_id: "test".to_string(),
            behaviors: vec![
                Behavior {
                    id: 0,
                    input_args: vec![json!(1), json!("a")],
                    return_value: None,
                    thrown_error: None,
                    branch_path: path_a,
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
                Behavior {
                    id: 1,
                    input_args: vec![json!(2), json!("b")],
                    return_value: None,
                    thrown_error: None,
                    branch_path: path_b,
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        let seeds = map.extract_seed_inputs();
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0], vec![json!(1), json!("a")]);
        assert_eq!(seeds[1], vec![json!(2), json!("b")]);
    }

    #[test]
    fn extract_seed_inputs_filters_empty() {
        let map = BehaviorMap {
            function_id: "test".to_string(),
            behaviors: vec![
                Behavior {
                    id: 0,
                    input_args: vec![json!(42)],
                    return_value: None,
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
                Behavior {
                    id: 1,
                    input_args: vec![],
                    return_value: None,
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        let seeds = map.extract_seed_inputs();
        assert_eq!(seeds.len(), 1, "empty input_args should be filtered out");
        assert_eq!(seeds[0], vec![json!(42)]);
    }
}
