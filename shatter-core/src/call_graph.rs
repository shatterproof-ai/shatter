//! Call graph construction from a [`FunctionRegistry`].
//!
//! Builds a directed graph where edges represent intra-project function calls.
//! Provides topological layering (for bottom-up exploration order) and cycle
//! detection via Tarjan's strongly connected components algorithm.

use std::collections::HashMap;

use crate::batch_analyze::FunctionRegistry;

/// A directed call graph over functions in a [`FunctionRegistry`].
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// All node names (qualified names) in insertion order.
    nodes: Vec<String>,
    /// Map from qualified name to node index.
    node_index: HashMap<String, usize>,
    /// Adjacency list: adj[caller_idx] = vec of callee indices.
    adj: Vec<Vec<usize>>,
    /// Reverse adjacency: rev[callee_idx] = vec of caller indices.
    rev: Vec<Vec<usize>>,
}

/// An edge in the call graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Qualified name of the caller.
    pub caller: String,
    /// Qualified name of the callee.
    pub callee: String,
}

impl CallGraph {
    /// Build a call graph from a [`FunctionRegistry`].
    ///
    /// An edge from A to B is added when A has an `ExternalDependency` whose
    /// `symbol` matches the function name portion of some entry B in the registry.
    /// When multiple entries share the same function name, the dependency's
    /// `source_module` is checked against candidate file paths to disambiguate.
    pub fn from_registry(registry: &FunctionRegistry) -> Self {
        let entries = registry.entries();

        // Build node list and index.
        let mut nodes = Vec::with_capacity(entries.len());
        let mut node_index = HashMap::with_capacity(entries.len());
        for entry in entries {
            let qn = FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
            let idx = nodes.len();
            node_index.insert(qn.clone(), idx);
            nodes.push(qn);
        }

        let mut adj = vec![Vec::new(); nodes.len()];
        let mut rev = vec![Vec::new(); nodes.len()];

        // Build a name-to-indices map for resolving dependencies by symbol name.
        let mut name_to_indices: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            name_to_indices
                .entry(&entry.name)
                .or_default()
                .push(i);
        }

        for (caller_idx, entry) in entries.iter().enumerate() {
            for dep in &entry.dependencies {
                let callee_idx = resolve_dependency(
                    &dep.symbol,
                    &dep.source_module,
                    &name_to_indices,
                    entries,
                );
                if let Some(ci) = callee_idx
                    && ci != caller_idx
                {
                    adj[caller_idx].push(ci);
                    rev[ci].push(caller_idx);
                }
            }
        }

        CallGraph {
            nodes,
            node_index,
            adj,
            rev,
        }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of directed edges.
    pub fn edge_count(&self) -> usize {
        self.adj.iter().map(|v| v.len()).sum()
    }

    /// Iterate all edges as `(caller, callee)` qualified name pairs.
    pub fn edges(&self) -> Vec<Edge> {
        let mut result = Vec::new();
        for (caller_idx, callees) in self.adj.iter().enumerate() {
            for &callee_idx in callees {
                result.push(Edge {
                    caller: self.nodes[caller_idx].clone(),
                    callee: self.nodes[callee_idx].clone(),
                });
            }
        }
        result
    }

    /// Get the qualified names of all functions that `qualified_name` calls.
    pub fn callees_of(&self, qualified_name: &str) -> Vec<&str> {
        match self.node_index.get(qualified_name) {
            Some(&idx) => self.adj[idx]
                .iter()
                .map(|&ci| self.nodes[ci].as_str())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Get the qualified names of all functions that call `qualified_name`.
    pub fn callers_of(&self, qualified_name: &str) -> Vec<&str> {
        match self.node_index.get(qualified_name) {
            Some(&idx) => self.rev[idx]
                .iter()
                .map(|&ci| self.nodes[ci].as_str())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Compute topological layers (Kahn's algorithm).
    ///
    /// Layer 0 contains leaf functions (no outgoing calls to other project functions).
    /// Each subsequent layer contains functions whose callees are all in earlier layers.
    /// Functions involved in cycles are placed in the final layer.
    pub fn topological_layers(&self) -> Vec<Vec<String>> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }

        // Out-degree for Kahn's (we layer bottom-up: leaves first).
        let mut out_degree: Vec<usize> = self.adj.iter().map(|v| v.len()).collect();
        let mut queue: Vec<usize> = Vec::new();
        for (i, &deg) in out_degree.iter().enumerate() {
            if deg == 0 {
                queue.push(i);
            }
        }

        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut visited = 0;

        while !queue.is_empty() {
            let mut layer = Vec::with_capacity(queue.len());
            let mut next_queue = Vec::new();

            for node in &queue {
                layer.push(self.nodes[*node].clone());
                visited += 1;

                // For each caller of this node, decrement their out-degree.
                for &caller in &self.rev[*node] {
                    out_degree[caller] -= 1;
                    if out_degree[caller] == 0 {
                        next_queue.push(caller);
                    }
                }
            }

            layers.push(layer);
            queue = next_queue;
        }

        // Any remaining nodes are in cycles.
        if visited < n {
            let cycle_layer: Vec<String> = (0..n)
                .filter(|&i| out_degree[i] > 0)
                .map(|i| self.nodes[i].clone())
                .collect();
            layers.push(cycle_layer);
        }

        layers
    }

    /// Detect strongly connected components using Tarjan's algorithm.
    ///
    /// Returns only SCCs with more than one member (actual cycles).
    pub fn cycle_groups(&self) -> Vec<Vec<String>> {
        let n = self.nodes.len();
        let mut state = TarjanState {
            index_counter: 0,
            stack: Vec::new(),
            on_stack: vec![false; n],
            node_index: vec![None; n],
            lowlink: vec![0; n],
            sccs: Vec::new(),
        };

        for i in 0..n {
            if state.node_index[i].is_none() {
                tarjan_strongconnect(i, &self.adj, &mut state);
            }
        }

        // Convert indices to names, keep only multi-member SCCs.
        state
            .sccs
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                scc.into_iter()
                    .map(|i| self.nodes[i].clone())
                    .collect()
            })
            .collect()
    }
}

/// Resolve a dependency symbol to a node index in the registry.
///
/// If only one function matches `symbol`, return it directly.
/// If multiple match, try to disambiguate using `source_module` against file paths.
fn resolve_dependency(
    symbol: &str,
    source_module: &str,
    name_to_indices: &HashMap<&str, Vec<usize>>,
    entries: &[crate::batch_analyze::FunctionEntry],
) -> Option<usize> {
    let candidates = name_to_indices.get(symbol)?;
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }

    // Multiple candidates — try to match source_module against file paths.
    if !source_module.is_empty() {
        for &idx in candidates {
            let path_str = entries[idx].file_path.to_string_lossy();
            if path_str.contains(source_module) {
                return Some(idx);
            }
        }
    }

    // Ambiguous — return first match as fallback.
    Some(candidates[0])
}

/// Internal state for Tarjan's SCC algorithm.
struct TarjanState {
    index_counter: usize,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    node_index: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    sccs: Vec<Vec<usize>>,
}

/// Recursive strongconnect for Tarjan's algorithm.
fn tarjan_strongconnect(v: usize, adj: &[Vec<usize>], state: &mut TarjanState) {
    state.node_index[v] = Some(state.index_counter);
    state.lowlink[v] = state.index_counter;
    state.index_counter += 1;
    state.stack.push(v);
    state.on_stack[v] = true;

    for &w in &adj[v] {
        match state.node_index[w] {
            None => {
                tarjan_strongconnect(w, adj, state);
                state.lowlink[v] = state.lowlink[v].min(state.lowlink[w]);
            }
            Some(_) if state.on_stack[w] => {
                // w is on the stack, so it's in the current SCC.
                state.lowlink[v] = state.lowlink[v].min(state.node_index[w].unwrap_or(0));
            }
            _ => {}
        }
    }

    // If v is a root node, pop the SCC.
    if state.lowlink[v] == state.node_index[v].unwrap_or(0) {
        let mut scc = Vec::new();
        while let Some(w) = state.stack.pop() {
            state.on_stack[w] = false;
            scc.push(w);
            if w == v {
                break;
            }
        }
        state.sccs.push(scc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
    use crate::protocol::{DependencyKind, ExternalDependency};
    use crate::types::TypeInfo;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Helper: build a FunctionRegistry from a list of (file, name, deps) tuples.
    /// Each dep is a symbol name (no source_module disambiguation).
    fn make_registry(
        funcs: &[(&str, &str, Vec<&str>)],
    ) -> FunctionRegistry {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        for (file, name, deps) in funcs {
            let qn = FunctionRegistry::qualified_name(
                &PathBuf::from(file),
                name,
            );
            let idx = entries.len();
            index.insert(qn, idx);
            entries.push(FunctionEntry {
                file_path: PathBuf::from(file),
                name: name.to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Unknown,
                dependencies: deps
                    .iter()
                    .map(|s| ExternalDependency {
                        kind: DependencyKind::FunctionCall,
                        symbol: s.to_string(),
                        source_module: String::new(),
                        return_type: TypeInfo::Unknown,
                        param_types: vec![],
                        call_sites: vec![],
                    })
                    .collect(),
                branch_count: 0,
            });
        }

        FunctionRegistry::from_raw(entries, index)
    }

    /// Helper: build registry with source_module on dependencies.
    fn make_registry_with_modules(
        funcs: &[(&str, &str, Vec<(&str, &str)>)],
    ) -> FunctionRegistry {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        for (file, name, deps) in funcs {
            let qn = FunctionRegistry::qualified_name(
                &PathBuf::from(file),
                name,
            );
            let idx = entries.len();
            index.insert(qn, idx);
            entries.push(FunctionEntry {
                file_path: PathBuf::from(file),
                name: name.to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Unknown,
                dependencies: deps
                    .iter()
                    .map(|(sym, module)| ExternalDependency {
                        kind: DependencyKind::FunctionCall,
                        symbol: sym.to_string(),
                        source_module: module.to_string(),
                        return_type: TypeInfo::Unknown,
                        param_types: vec![],
                        call_sites: vec![],
                    })
                    .collect(),
                branch_count: 0,
            });
        }

        FunctionRegistry::from_raw(entries, index)
    }

    #[test]
    fn empty_registry_produces_empty_graph() {
        let registry = make_registry(&[]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert!(graph.edges().is_empty());
        assert!(graph.topological_layers().is_empty());
        assert!(graph.cycle_groups().is_empty());
    }

    #[test]
    fn single_node_no_edges() {
        let registry = make_registry(&[("src/a.ts", "foo", vec![])]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.edge_count(), 0);

        let layers = graph.topological_layers();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0], vec!["src/a.ts::foo"]);
    }

    #[test]
    fn linear_chain_produces_correct_topo_layers() {
        // A calls B, B calls C. Topo layers bottom-up: [C], [B], [A]
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B"]),
            ("src/a.ts", "B", vec!["C"]),
            ("src/a.ts", "C", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 2);

        let layers = graph.topological_layers();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["src/a.ts::C"]);
        assert_eq!(layers[1], vec!["src/a.ts::B"]);
        assert_eq!(layers[2], vec!["src/a.ts::A"]);
    }

    #[test]
    fn diamond_dependency() {
        // A → B, A → C, B → D, C → D
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B", "C"]),
            ("src/a.ts", "B", vec!["D"]),
            ("src/a.ts", "C", vec!["D"]),
            ("src/a.ts", "D", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 4);

        assert_eq!(graph.callees_of("src/a.ts::A").len(), 2);
        assert_eq!(graph.callers_of("src/a.ts::D").len(), 2);

        let layers = graph.topological_layers();
        // D first, then B and C together, then A.
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["src/a.ts::D"]);
        assert!(layers[1].contains(&"src/a.ts::B".to_string()));
        assert!(layers[1].contains(&"src/a.ts::C".to_string()));
        assert_eq!(layers[2], vec!["src/a.ts::A"]);
    }

    #[test]
    fn cycle_detection_two_nodes() {
        // A → B, B → A
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B"]),
            ("src/a.ts", "B", vec!["A"]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        let cycles = graph.cycle_groups();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 2);
        assert!(cycles[0].contains(&"src/a.ts::A".to_string()));
        assert!(cycles[0].contains(&"src/a.ts::B".to_string()));
    }

    #[test]
    fn cycle_nodes_appear_in_final_topo_layer() {
        // A → B → A, C (no deps)
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B"]),
            ("src/a.ts", "B", vec!["A"]),
            ("src/a.ts", "C", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        let layers = graph.topological_layers();
        // C is a leaf → layer 0; A and B are in a cycle → final layer
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0], vec!["src/a.ts::C"]);
        assert!(layers[1].contains(&"src/a.ts::A".to_string()));
        assert!(layers[1].contains(&"src/a.ts::B".to_string()));
    }

    #[test]
    fn cross_file_dependency_resolution() {
        let registry = make_registry(&[
            ("src/main.ts", "main", vec!["helper"]),
            ("src/utils.ts", "helper", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 1);
        let edges = graph.edges();
        assert_eq!(edges[0].caller, "src/main.ts::main");
        assert_eq!(edges[0].callee, "src/utils.ts::helper");
    }

    #[test]
    fn leaf_nodes_have_no_callees() {
        let registry = make_registry(&[
            ("src/a.ts", "leaf1", vec![]),
            ("src/a.ts", "leaf2", vec![]),
            ("src/a.ts", "caller", vec!["leaf1", "leaf2"]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert!(graph.callees_of("src/a.ts::leaf1").is_empty());
        assert!(graph.callees_of("src/a.ts::leaf2").is_empty());
        assert_eq!(graph.callees_of("src/a.ts::caller").len(), 2);
    }

    #[test]
    fn unknown_dependency_is_ignored() {
        // "nonexistent" is not in the registry → no edge created.
        let registry = make_registry(&[
            ("src/a.ts", "foo", vec!["nonexistent"]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn callers_of_unknown_node_returns_empty() {
        let registry = make_registry(&[("src/a.ts", "foo", vec![])]);
        let graph = CallGraph::from_registry(&registry);

        assert!(graph.callers_of("nonexistent").is_empty());
        assert!(graph.callees_of("nonexistent").is_empty());
    }

    #[test]
    fn multiple_sccs_detected() {
        // SCC1: A ↔ B, SCC2: C ↔ D, E is a leaf
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B"]),
            ("src/a.ts", "B", vec!["A"]),
            ("src/a.ts", "C", vec!["D"]),
            ("src/a.ts", "D", vec!["C"]),
            ("src/a.ts", "E", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        let cycles = graph.cycle_groups();
        assert_eq!(cycles.len(), 2);

        let mut cycle_sets: Vec<Vec<String>> = cycles
            .into_iter()
            .map(|mut c| {
                c.sort();
                c
            })
            .collect();
        cycle_sets.sort();

        assert_eq!(
            cycle_sets[0],
            vec!["src/a.ts::A".to_string(), "src/a.ts::B".to_string()]
        );
        assert_eq!(
            cycle_sets[1],
            vec!["src/a.ts::C".to_string(), "src/a.ts::D".to_string()]
        );
    }

    #[test]
    fn three_node_cycle() {
        // A → B → C → A
        let registry = make_registry(&[
            ("src/a.ts", "A", vec!["B"]),
            ("src/a.ts", "B", vec!["C"]),
            ("src/a.ts", "C", vec!["A"]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        let cycles = graph.cycle_groups();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
    }

    #[test]
    fn source_module_disambiguates_same_name_functions() {
        // Two files both have "helper". Caller specifies source_module to pick the right one.
        let registry = make_registry_with_modules(&[
            ("src/main.ts", "caller", vec![("helper", "utils")]),
            ("src/utils.ts", "helper", vec![]),
            ("src/other.ts", "helper", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 1);
        let edges = graph.edges();
        assert_eq!(edges[0].callee, "src/utils.ts::helper");
    }

    #[test]
    fn self_call_not_added_as_edge() {
        // A function depending on its own name should not create a self-loop.
        let registry = make_registry(&[
            ("src/a.ts", "recurse", vec!["recurse"]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn large_graph_with_mixed_structure() {
        // Chain: f0 → f1 → f2 → f3 → f4 (linear)
        // Cycle: f5 ↔ f6
        // Isolated: f7
        let registry = make_registry(&[
            ("src/a.ts", "f0", vec!["f1"]),
            ("src/a.ts", "f1", vec!["f2"]),
            ("src/a.ts", "f2", vec!["f3"]),
            ("src/a.ts", "f3", vec!["f4"]),
            ("src/a.ts", "f4", vec![]),
            ("src/a.ts", "f5", vec!["f6"]),
            ("src/a.ts", "f6", vec!["f5"]),
            ("src/a.ts", "f7", vec![]),
        ]);
        let graph = CallGraph::from_registry(&registry);

        assert_eq!(graph.node_count(), 8);
        assert_eq!(graph.edge_count(), 6);

        let layers = graph.topological_layers();
        // Leaves (f4, f7) in layer 0, then f3, f2, f1, f0 spread across layers,
        // cycle (f5, f6) in the final layer.
        let last_layer = layers.last().expect("should have layers");
        assert!(last_layer.contains(&"src/a.ts::f5".to_string()));
        assert!(last_layer.contains(&"src/a.ts::f6".to_string()));

        // f4 and f7 should be in the first layer.
        assert!(layers[0].contains(&"src/a.ts::f4".to_string()));
        assert!(layers[0].contains(&"src/a.ts::f7".to_string()));

        let cycles = graph.cycle_groups();
        assert_eq!(cycles.len(), 1);
    }
}
