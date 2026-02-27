//! Scan orchestrator: multi-function exploration in dependency order.
//!
//! When function A calls function B, testing B first lets us record its
//! behavior map and use it as a high-fidelity mock when testing A.
//! The scan orchestrator builds a [`CallGraph`], computes a test order
//! (leaves first), and drives [`explore_function`] for each function
//! with appropriate mocks.
//!
//! The [`parallel_scan`] function extends this with multi-process parallelism:
//! it spawns N frontend subprocesses as a worker pool and assigns functions
//! to workers in dependency order (layer by layer). Functions within the same
//! topological layer are explored concurrently. Per-function timeouts prevent
//! a single slow function from stalling the entire scan.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::behavior::{BehaviorCoverage, BehaviorMap, CallGraph, CallGraphError, TestOrderEntry};
use crate::cache::BehaviorMapCache;
use crate::execution_record::ExecutionRecord;
use crate::explorer::{self, ExploreConfig, ExploreError, ExplorationResult};
use crate::frontend::{Frontend, FrontendConfig, FrontendError};
use crate::protocol::{ExecuteResult, FunctionAnalysis, MockConfig};

/// Configuration for a scan run.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Maximum number of iterations (execute calls) per function.
    pub max_iterations_per_function: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Map from function name to source file path (needed for instrumentation).
    pub file_map: HashMap<String, String>,
    /// Number of parallel frontend subprocesses (default: 1).
    pub parallelism: usize,
    /// Per-function timeout. If a function takes longer, it is skipped.
    /// Default: 30 seconds.
    pub timeout_per_fn: Duration,
    /// Optional disk cache for persisting behavior maps across runs.
    /// When set, behavior maps are stored after exploration and loaded
    /// before exploration to skip re-exploring unchanged functions.
    pub cache: Option<Arc<BehaviorMapCache>>,
}

/// Result of exploring a single function during a scan.
#[derive(Debug)]
pub struct FunctionResult {
    /// Name of the explored function.
    pub function_name: String,
    /// The exploration result (paths, coverage, etc.).
    pub exploration: ExplorationResult,
    /// Behavior map built from execution results.
    pub behavior_map: BehaviorMap,
    /// Coverage of callee behaviors exercised by this function.
    pub behavior_coverage: Vec<BehaviorCoverage>,
    /// Names of functions that were mocked during exploration.
    pub mocks_used: Vec<String>,
}

/// Result of a full scan across multiple functions.
#[derive(Debug)]
pub struct ScanResult {
    /// Per-function results in test order.
    pub function_results: Vec<FunctionResult>,
    /// The order in which functions were tested.
    pub test_order: Vec<String>,
}

/// Errors that can occur during a scan.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("exploration error: {0}")]
    Explore(#[from] ExploreError),
    #[error("call graph cycle detected: {0}")]
    Cycle(#[from] CallGraphError),
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
}

/// Per-function outcome used internally during parallel scan.
#[derive(Debug)]
enum FunctionOutcome {
    /// Exploration succeeded.
    Success(FunctionResult),
    /// Exploration timed out.
    Timeout {
        function_name: String,
        limit: Duration,
    },
    /// Exploration encountered an error.
    Error {
        function_name: String,
        error: String,
    },
}

/// Summary of a function that was skipped during a parallel scan.
#[derive(Debug)]
pub struct SkippedFunction {
    /// Name of the function that was skipped.
    pub function_name: String,
    /// Reason the function was skipped.
    pub reason: String,
}

/// Build an [`ExecutionRecord`] from an [`ExecuteResult`] and its inputs.
fn execution_record_from_result(
    function_id: &str,
    inputs: &[serde_json::Value],
    result: &ExecuteResult,
) -> ExecutionRecord {
    let mut hasher = DefaultHasher::new();
    let input_str = serde_json::to_string(inputs).unwrap_or_default();
    input_str.hash(&mut hasher);
    let input_hash = hasher.finish();

    ExecutionRecord {
        function_id: function_id.to_string(),
        input_hash,
        parameters: inputs.to_vec(),
        branch_path: result.branch_path.clone(),
        lines_executed: result.lines_executed.clone(),
        calls_to_external: result.calls_to_external.clone(),
        path_constraints: result.path_constraints.clone(),
        return_value: result.return_value.clone(),
        thrown_error: result.thrown_error.clone(),
        side_effects: result.side_effects.clone(),
        wall_time_ms: result.performance.wall_time_ms,
        cpu_time_us: result.performance.cpu_time_us,
        heap_used_bytes: result.performance.heap_used_bytes,
        heap_allocated_bytes: result.performance.heap_allocated_bytes,
        timestamp: String::new(),
        engine_version: String::new(),
    }
}

/// Run a multi-function scan in dependency order.
///
/// Builds a call graph from the analyses, determines test order (leaves first),
/// then explores each function. Callees that have already been tested provide
/// mock configurations derived from their behavior maps.
pub async fn scan(
    frontend: &mut Frontend,
    analyses: &[FunctionAnalysis],
    config: &ScanConfig,
) -> Result<ScanResult, ScanError> {
    let call_graph = CallGraph::from_analyses(analyses);
    let order_entries = call_graph.test_order()?;

    // Flatten test order entries into function names for iteration.
    // MutualGroup entries are flattened in order (handled as a group is future work).
    let test_order: Vec<String> = order_entries
        .iter()
        .flat_map(|entry| match entry {
            TestOrderEntry::Single { function_id, .. } => vec![function_id.clone()],
            TestOrderEntry::MutualGroup { function_ids } => function_ids.clone(),
        })
        .collect();

    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    let mut behavior_maps: HashMap<String, BehaviorMap> = HashMap::new();
    let mut function_results: Vec<FunctionResult> = Vec::new();

    for func_name in &test_order {
        let analysis = match analysis_map.get(func_name.as_str()) {
            Some(a) => *a,
            None => continue,
        };

        // Try loading a cached behavior map for callees that aren't yet in memory.
        if let Some(ref cache) = config.cache {
            for dep in &analysis.dependencies {
                if !behavior_maps.contains_key(&dep.symbol)
                    && let Ok(Some(cached)) = cache.load(&dep.symbol)
                {
                    behavior_maps.insert(dep.symbol.clone(), cached);
                }
            }
        }

        // Build mocks from callees that have already been tested.
        let callees = call_graph.callees(func_name);
        let mut mocks: Vec<MockConfig> = Vec::new();
        let mut mocks_used: Vec<String> = Vec::new();

        for callee in &callees {
            if let Some(bmap) = behavior_maps.get(callee) {
                mocks.push(bmap.to_mock_config());
                mocks_used.push(callee.clone());
            }
        }
        mocks_used.sort();

        let file = config
            .file_map
            .get(func_name)
            .cloned()
            .unwrap_or_default();

        let explore_config = ExploreConfig {
            file,
            max_iterations: config.max_iterations_per_function,
            seed: config.seed,
            mocks,
        };

        let exploration = explorer::explore_function(frontend, analysis, &explore_config).await?;

        // Build ExecutionRecords from raw results for BehaviorMap construction.
        let records: Vec<ExecutionRecord> = exploration
            .raw_results
            .iter()
            .map(|(inputs, result)| execution_record_from_result(func_name, inputs, result))
            .collect();

        let behavior_map = BehaviorMap::from_records(func_name, &records);

        // Persist the behavior map to cache for reuse across runs.
        if let Some(ref cache) = config.cache {
            let _ = cache.store(&behavior_map);
        }

        // Compute behavior coverage for each callee.
        let mut behavior_coverage: Vec<BehaviorCoverage> = Vec::new();
        for callee in &callees {
            if let Some(callee_map) = behavior_maps.get(callee) {
                let coverage = BehaviorCoverage::compute(func_name, &records, callee_map);
                behavior_coverage.push(coverage);
            }
        }

        behavior_maps.insert(func_name.clone(), behavior_map.clone());

        function_results.push(FunctionResult {
            function_name: func_name.clone(),
            exploration,
            behavior_map,
            behavior_coverage,
            mocks_used,
        });
    }

    Ok(ScanResult {
        function_results,
        test_order,
    })
}

/// Result of a parallel scan across multiple functions.
#[derive(Debug)]
pub struct ParallelScanResult {
    /// Per-function results (only successful explorations).
    pub function_results: Vec<FunctionResult>,
    /// The order in which functions were tested.
    pub test_order: Vec<String>,
    /// Functions that were skipped due to timeout or error.
    pub skipped: Vec<SkippedFunction>,
    /// Number of worker subprocesses used.
    pub workers_used: usize,
}

/// A channel-based pool of frontend worker subprocesses.
///
/// Workers are checked out via `recv()` and returned via `send()` after use.
/// This ensures exclusive access without lock contention.
struct WorkerPool {
    sender: tokio::sync::mpsc::Sender<Frontend>,
    receiver: Mutex<tokio::sync::mpsc::Receiver<Frontend>>,
}

impl WorkerPool {
    /// Spawn `n` frontend subprocesses and place them in the pool.
    async fn spawn(config: &FrontendConfig, n: usize) -> Result<Self, FrontendError> {
        let (sender, receiver) = tokio::sync::mpsc::channel(n);
        for _ in 0..n {
            let frontend = Frontend::spawn(config).await?;
            sender.send(frontend).await.expect("channel just created");
        }
        Ok(Self {
            sender,
            receiver: Mutex::new(receiver),
        })
    }

    /// Check out a worker from the pool.
    async fn checkout(&self) -> Frontend {
        let mut rx = self.receiver.lock().await;
        rx.recv().await.expect("pool should not be empty")
    }

    /// Return a worker to the pool.
    async fn return_worker(&self, frontend: Frontend) {
        let _ = self.sender.send(frontend).await;
    }

    /// Shut down all workers remaining in the pool.
    async fn shutdown(self) {
        drop(self.sender);
        let mut rx = self.receiver.into_inner();
        while let Some(frontend) = rx.recv().await {
            let _ = frontend.shutdown().await;
        }
    }
}

/// Run a multi-function scan in dependency order with multi-process parallelism.
///
/// Spawns `config.parallelism` frontend subprocesses and explores functions
/// layer by layer. Within each layer, functions are explored concurrently
/// across the worker pool. Behavior maps from completed layers are fed
/// forward as mocks for subsequent layers.
///
/// Per-function timeouts are enforced: if a function exceeds `config.timeout_per_fn`,
/// its exploration is aborted and it is recorded as skipped. Errors in individual
/// functions do not abort the scan.
pub async fn parallel_scan(
    frontend_config: &FrontendConfig,
    analyses: &[FunctionAnalysis],
    config: &ScanConfig,
) -> Result<ParallelScanResult, ScanError> {
    let call_graph = CallGraph::from_analyses(analyses);
    let order_entries = call_graph.test_order()?;

    // Flatten test order into layers. Each layer contains functions whose
    // callees are all in previous layers.
    let layers = build_layers(&order_entries, &call_graph);

    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    let effective_parallelism = config.parallelism.max(1);
    let pool = Arc::new(
        WorkerPool::spawn(frontend_config, effective_parallelism)
            .await
            .map_err(ScanError::Frontend)?,
    );

    let behavior_maps: Arc<Mutex<HashMap<String, BehaviorMap>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut all_results: Vec<FunctionResult> = Vec::new();
    let mut test_order: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedFunction> = Vec::new();

    for layer in &layers {
        // Build tasks for this layer: each function paired with its mocks.
        let mut tasks = Vec::new();
        for func_name in layer {
            test_order.push(func_name.clone());

            let analysis = match analysis_map.get(func_name.as_str()) {
                Some(a) => *a,
                None => {
                    skipped.push(SkippedFunction {
                        function_name: func_name.clone(),
                        reason: "no analysis found".into(),
                    });
                    continue;
                }
            };

            // Try loading cached behavior maps for callees not yet in memory.
            if let Some(ref cache) = config.cache {
                let mut maps = behavior_maps.lock().await;
                for dep in &analysis.dependencies {
                    if !maps.contains_key(&dep.symbol)
                        && let Ok(Some(cached)) = cache.load(&dep.symbol)
                    {
                        maps.insert(dep.symbol.clone(), cached);
                    }
                }
                drop(maps);
            }

            // Build mocks from callees that have already been tested.
            let callees = call_graph.callees(func_name);
            let maps = behavior_maps.lock().await;
            let mut mocks: Vec<MockConfig> = Vec::new();
            let mut mocks_used: Vec<String> = Vec::new();
            for callee in &callees {
                if let Some(bmap) = maps.get(callee) {
                    mocks.push(bmap.to_mock_config());
                    mocks_used.push(callee.clone());
                }
            }
            mocks_used.sort();
            drop(maps);

            let file = config
                .file_map
                .get(func_name)
                .cloned()
                .unwrap_or_default();

            let explore_config = ExploreConfig {
                file,
                max_iterations: config.max_iterations_per_function,
                seed: config.seed,
                mocks,
            };

            tasks.push((func_name.clone(), analysis.clone(), explore_config, mocks_used, callees));
        }

        // Execute tasks in parallel across the worker pool.
        // Each task checks out a worker, explores, then returns the worker.
        let mut handles = Vec::new();

        for (func_name, analysis, explore_config, mocks_used, callees) in tasks {
            let pool = Arc::clone(&pool);
            let behavior_maps = Arc::clone(&behavior_maps);
            let timeout = config.timeout_per_fn;
            let cache = config.cache.clone();

            let handle = tokio::spawn(async move {
                let mut frontend = pool.checkout().await;

                let result = tokio::time::timeout(
                    timeout,
                    explore_single_function(
                        &mut frontend,
                        &func_name,
                        &analysis,
                        &explore_config,
                        &mocks_used,
                        &callees,
                        &behavior_maps,
                    ),
                )
                .await;

                // Return the worker to the pool regardless of outcome.
                pool.return_worker(frontend).await;

                match result {
                    Ok(Ok(func_result)) => {
                        // Store the behavior map for downstream functions.
                        let mut maps = behavior_maps.lock().await;
                        maps.insert(func_name.clone(), func_result.behavior_map.clone());
                        drop(maps);

                        // Persist to disk cache for reuse across runs.
                        if let Some(ref cache) = cache {
                            let _ = cache.store(&func_result.behavior_map);
                        }

                        FunctionOutcome::Success(func_result)
                    }
                    Ok(Err(e)) => FunctionOutcome::Error {
                        function_name: func_name,
                        error: e.to_string(),
                    },
                    Err(_) => FunctionOutcome::Timeout {
                        function_name: func_name,
                        limit: timeout,
                    },
                }
            });

            handles.push(handle);
        }

        // Collect results from all tasks in this layer.
        for handle in handles {
            match handle.await {
                Ok(FunctionOutcome::Success(result)) => {
                    all_results.push(result);
                }
                Ok(FunctionOutcome::Timeout { function_name, limit }) => {
                    skipped.push(SkippedFunction {
                        function_name,
                        reason: format!("timed out after {:.0}s", limit.as_secs_f64()),
                    });
                }
                Ok(FunctionOutcome::Error { function_name, error }) => {
                    skipped.push(SkippedFunction {
                        function_name,
                        reason: format!("error: {error}"),
                    });
                }
                Err(e) => {
                    // JoinError (task panicked or was cancelled)
                    skipped.push(SkippedFunction {
                        function_name: "(unknown)".into(),
                        reason: format!("task join error: {e}"),
                    });
                }
            }
        }
    }

    // Shutdown workers. All spawned tasks have completed, so this is the only
    // remaining reference to the pool.
    if let Ok(pool) = Arc::try_unwrap(pool) {
        pool.shutdown().await;
    }

    Ok(ParallelScanResult {
        function_results: all_results,
        test_order,
        skipped,
        workers_used: effective_parallelism,
    })
}

/// Build exploration layers from test order entries and call graph.
///
/// Functions are grouped into layers such that all callees of functions in
/// layer N appear in layers 0..N-1. Functions within the same layer can
/// be explored in parallel.
fn build_layers(order_entries: &[TestOrderEntry], call_graph: &CallGraph) -> Vec<Vec<String>> {
    // The test_order from the behavior::CallGraph is already topologically sorted
    // (leaves first). We group consecutive entries that share no cross-dependencies
    // into the same layer. For simplicity, we assign each entry its own layer slot
    // and then could merge, but the simplest correct approach is:
    // layer 0 = functions with no callees in the scan set
    // layer N = functions whose callees are all in layers < N
    //
    // Since test_order is already leaves-first, we can build layers by tracking
    // which functions are "done" after each layer.

    let all_functions: Vec<String> = order_entries
        .iter()
        .flat_map(|entry| match entry {
            TestOrderEntry::Single { function_id, .. } => vec![function_id.clone()],
            TestOrderEntry::MutualGroup { function_ids } => function_ids.clone(),
        })
        .collect();

    if all_functions.is_empty() {
        return Vec::new();
    }

    let mut assigned: HashMap<String, usize> = HashMap::new();
    let mut layers: Vec<Vec<String>> = Vec::new();

    for func_name in &all_functions {
        let callees = call_graph.callees(func_name);
        let max_callee_layer = callees
            .iter()
            .filter_map(|c| assigned.get(c))
            .copied()
            .max();

        let my_layer = match max_callee_layer {
            Some(l) => l + 1,
            None => 0,
        };

        while layers.len() <= my_layer {
            layers.push(Vec::new());
        }
        layers[my_layer].push(func_name.clone());
        assigned.insert(func_name.clone(), my_layer);
    }

    layers
}

/// Explore a single function and build its result.
///
/// This is the core work unit for both sequential and parallel scanning.
async fn explore_single_function(
    frontend: &mut Frontend,
    func_name: &str,
    analysis: &FunctionAnalysis,
    explore_config: &ExploreConfig,
    mocks_used: &[String],
    callees: &std::collections::HashSet<String>,
    behavior_maps: &Mutex<HashMap<String, BehaviorMap>>,
) -> Result<FunctionResult, ScanError> {
    let exploration = explorer::explore_function(frontend, analysis, explore_config).await?;

    // Build ExecutionRecords from raw results for BehaviorMap construction.
    let records: Vec<ExecutionRecord> = exploration
        .raw_results
        .iter()
        .map(|(inputs, result)| execution_record_from_result(func_name, inputs, result))
        .collect();

    let behavior_map = BehaviorMap::from_records(func_name, &records);

    // Compute behavior coverage for each callee.
    let maps = behavior_maps.lock().await;
    let mut behavior_coverage: Vec<BehaviorCoverage> = Vec::new();
    for callee in callees {
        if let Some(callee_map) = maps.get(callee) {
            let coverage = BehaviorCoverage::compute(func_name, &records, callee_map);
            behavior_coverage.push(coverage);
        }
    }
    drop(maps);

    Ok(FunctionResult {
        function_name: func_name.to_string(),
        exploration,
        behavior_map,
        behavior_coverage,
        mocks_used: mocks_used.to_vec(),
    })
}

/// Format a parallel scan result as a human-readable report.
pub fn format_parallel_scan_report(result: &ParallelScanResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested, {} skipped ({} worker(s))\n",
        result.function_results.len(),
        result.skipped.len(),
        result.workers_used,
    ));

    out.push_str("\nTest order: ");
    out.push_str(&result.test_order.join(" -> "));
    out.push('\n');

    for func_result in &result.function_results {
        out.push_str(&format!("\n-- {} --\n", func_result.function_name));
        out.push_str(&explorer::format_exploration_report_verbose(
            &func_result.exploration,
        ));

        if !func_result.mocks_used.is_empty() {
            out.push_str(&format!(
                "  Mocks used: {}\n",
                func_result.mocks_used.join(", ")
            ));
        }

        for cov in &func_result.behavior_coverage {
            let exercised = cov.exercised_behavior_ids.len();
            let total = cov.total_behaviors;
            let pct = if total > 0 {
                (exercised as f64 / total as f64 * 100.0).round()
            } else {
                0.0
            };
            out.push_str(&format!(
                "  Behavior coverage of {}: {}/{} ({pct:.0}%)\n",
                cov.callee, exercised, total
            ));
        }
    }

    if !result.skipped.is_empty() {
        out.push_str("\nSkipped functions:\n");
        for skip in &result.skipped {
            out.push_str(&format!("  {}: {}\n", skip.function_name, skip.reason));
        }
    }

    out
}

/// Format a scan result as a human-readable report.
pub fn format_scan_report(result: &ScanResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested\n",
        result.function_results.len()
    ));

    out.push_str("\nTest order: ");
    out.push_str(&result.test_order.join(" → "));
    out.push('\n');

    for func_result in &result.function_results {
        out.push_str(&format!("\n── {} ──\n", func_result.function_name));

        out.push_str(&explorer::format_exploration_report_verbose(&func_result.exploration));

        if !func_result.mocks_used.is_empty() {
            out.push_str(&format!(
                "  Mocks used: {}\n",
                func_result.mocks_used.join(", ")
            ));
        }

        for cov in &func_result.behavior_coverage {
            let exercised = cov.exercised_behavior_ids.len();
            let total = cov.total_behaviors;
            let pct = if total > 0 {
                (exercised as f64 / total as f64 * 100.0).round()
            } else {
                0.0
            };
            out.push_str(&format!(
                "  Behavior coverage of {}: {}/{} ({pct:.0}%)\n",
                cov.callee, exercised, total
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        DependencyKind, ExecuteResult, ExternalDependency, PerformanceMetrics,
    };
    use crate::types::TypeInfo;

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
            start_line: 1,
            end_line: 5,
        }
    }

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    #[test]
    fn execution_record_from_result_builds_correctly() {
        let exec_result = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };

        let inputs = vec![serde_json::json!(10)];
        let record = execution_record_from_result("myFunc", &inputs, &exec_result);

        assert_eq!(record.function_id, "myFunc");
        assert_eq!(record.parameters, inputs);
        assert_eq!(record.return_value, Some(serde_json::json!(42)));
        assert_eq!(record.lines_executed, vec![1, 2, 3]);
    }

    #[test]
    fn execution_record_from_result_hashes_inputs_consistently() {
        let exec_result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            performance: empty_perf(),
        };

        let inputs = vec![serde_json::json!(1), serde_json::json!("hello")];
        let r1 = execution_record_from_result("f", &inputs, &exec_result);
        let r2 = execution_record_from_result("f", &inputs, &exec_result);
        assert_eq!(r1.input_hash, r2.input_hash);

        let different_inputs = vec![serde_json::json!(2)];
        let r3 = execution_record_from_result("f", &different_inputs, &exec_result);
        assert_ne!(r1.input_hash, r3.input_hash);
    }

    #[test]
    fn scan_mutual_recursion_returns_group() {
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec!["a"]),
        ];
        let call_graph = CallGraph::from_analyses(&analyses);
        let result = call_graph.test_order().expect("mutual recursion should not error");
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0], TestOrderEntry::MutualGroup { function_ids } if function_ids.len() == 2));
    }

    #[test]
    fn format_scan_report_shows_test_order() {
        let result = ScanResult {
            test_order: vec!["leaf".into(), "caller".into()],
            function_results: vec![
                FunctionResult {
                    function_name: "leaf".into(),
                    exploration: ExplorationResult {
                        function_name: "leaf".into(),
                        iterations: 5,
                        unique_paths: 2,
                        lines_covered: 3,
                        total_lines: 5,
                        new_path_executions: vec![],
                        raw_results: vec![],
                    },
                    behavior_map: BehaviorMap {
                        function_id: "leaf".into(),
                        behaviors: vec![],
                    },
                    behavior_coverage: vec![],
                    mocks_used: vec![],
                },
                FunctionResult {
                    function_name: "caller".into(),
                    exploration: ExplorationResult {
                        function_name: "caller".into(),
                        iterations: 10,
                        unique_paths: 3,
                        lines_covered: 8,
                        total_lines: 10,
                        new_path_executions: vec![],
                        raw_results: vec![],
                    },
                    behavior_map: BehaviorMap {
                        function_id: "caller".into(),
                        behaviors: vec![],
                    },
                    behavior_coverage: vec![BehaviorCoverage {
                        caller: "caller".into(),
                        callee: "leaf".into(),
                        exercised_behavior_ids: vec![0, 1],
                        total_behaviors: 3,
                    }],
                    mocks_used: vec!["leaf".into()],
                },
            ],
        };

        let report = format_scan_report(&result);
        assert!(report.contains("2 function(s) tested"));
        assert!(report.contains("leaf → caller"));
        assert!(report.contains("Mocks used: leaf"));
        assert!(report.contains("Behavior coverage of leaf: 2/3"));
    }

    #[test]
    fn format_scan_report_single_function_no_deps() {
        let result = ScanResult {
            test_order: vec!["standalone".into()],
            function_results: vec![FunctionResult {
                function_name: "standalone".into(),
                exploration: ExplorationResult {
                    function_name: "standalone".into(),
                    iterations: 10,
                    unique_paths: 1,
                    lines_covered: 5,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![],
                },
                behavior_map: BehaviorMap {
                    function_id: "standalone".into(),
                    behaviors: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
            }],
        };

        let report = format_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(!report.contains("Mocks used"));
        assert!(!report.contains("Behavior coverage"));
    }

    // ── build_layers tests ──────────────────────────────────────────

    #[test]
    fn build_layers_empty_input() {
        let call_graph = CallGraph::from_analyses(&[]);
        let layers = build_layers(&[], &call_graph);
        assert!(layers.is_empty());
    }

    #[test]
    fn build_layers_single_function() {
        let analyses = vec![make_analysis("f", vec![])];
        let call_graph = CallGraph::from_analyses(&analyses);
        let order = call_graph.test_order().expect("should succeed");
        let layers = build_layers(&order, &call_graph);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0], vec!["f"]);
    }

    #[test]
    fn build_layers_independent_functions_in_same_layer() {
        let analyses = vec![
            make_analysis("a", vec![]),
            make_analysis("b", vec![]),
            make_analysis("c", vec![]),
        ];
        let call_graph = CallGraph::from_analyses(&analyses);
        let order = call_graph.test_order().expect("should succeed");
        let layers = build_layers(&order, &call_graph);
        // All independent functions should be in layer 0.
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].len(), 3);
    }

    #[test]
    fn build_layers_linear_chain_produces_separate_layers() {
        // a -> b -> c: layers should be [c], [b], [a]
        let analyses = vec![
            make_analysis("a", vec!["b"]),
            make_analysis("b", vec!["c"]),
            make_analysis("c", vec![]),
        ];
        let call_graph = CallGraph::from_analyses(&analyses);
        let order = call_graph.test_order().expect("should succeed");
        let layers = build_layers(&order, &call_graph);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["c"]);
        assert_eq!(layers[1], vec!["b"]);
        assert_eq!(layers[2], vec!["a"]);
    }

    #[test]
    fn build_layers_diamond_groups_siblings() {
        // a -> b, a -> c, b -> d, c -> d
        let analyses = vec![
            make_analysis("a", vec!["b", "c"]),
            make_analysis("b", vec!["d"]),
            make_analysis("c", vec!["d"]),
            make_analysis("d", vec![]),
        ];
        let call_graph = CallGraph::from_analyses(&analyses);
        let order = call_graph.test_order().expect("should succeed");
        let layers = build_layers(&order, &call_graph);
        // d in layer 0, b and c in layer 1, a in layer 2
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["d"]);
        assert!(layers[1].contains(&"b".to_string()));
        assert!(layers[1].contains(&"c".to_string()));
        assert_eq!(layers[2], vec!["a"]);
    }

    // ── format_parallel_scan_report tests ───────────────────────────

    #[test]
    fn format_parallel_scan_report_shows_workers_and_skipped() {
        let result = ParallelScanResult {
            test_order: vec!["f1".into(), "f2".into()],
            function_results: vec![FunctionResult {
                function_name: "f1".into(),
                exploration: ExplorationResult {
                    function_name: "f1".into(),
                    iterations: 5,
                    unique_paths: 1,
                    lines_covered: 3,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![],
                },
                behavior_map: BehaviorMap {
                    function_id: "f1".into(),
                    behaviors: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
            }],
            skipped: vec![SkippedFunction {
                function_name: "f2".into(),
                reason: "timed out after 30s".into(),
            }],
            workers_used: 4,
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(report.contains("1 skipped"));
        assert!(report.contains("4 worker(s)"));
        assert!(report.contains("f1 -> f2"));
        assert!(report.contains("Skipped functions:"));
        assert!(report.contains("f2: timed out after 30s"));
    }

    #[test]
    fn format_parallel_scan_report_no_skipped() {
        let result = ParallelScanResult {
            test_order: vec!["f1".into()],
            function_results: vec![FunctionResult {
                function_name: "f1".into(),
                exploration: ExplorationResult {
                    function_name: "f1".into(),
                    iterations: 10,
                    unique_paths: 2,
                    lines_covered: 5,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![],
                },
                behavior_map: BehaviorMap {
                    function_id: "f1".into(),
                    behaviors: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
            }],
            skipped: vec![],
            workers_used: 1,
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("0 skipped"));
        assert!(!report.contains("Skipped functions:"));
    }

    // ── parallel_scan integration test ──────────────────────────────

    #[tokio::test]
    async fn parallel_scan_with_noop_frontend() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);

        let analyses = vec![
            FunctionAnalysis {
                name: "leaf".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "x".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
            },
            FunctionAnalysis {
                name: "caller".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "y".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
                branches: vec![],
                dependencies: vec![ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: "leaf".to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "test.ts".to_string());
        file_map.insert("caller".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            seed: Some(42),
            file_map,
            parallelism: 2,
            timeout_per_fn: Duration::from_secs(10),
            cache: None,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.workers_used, 2);
        assert_eq!(result.function_results.len(), 2);
        assert!(result.skipped.is_empty());
        // leaf should be tested before caller (dependency order)
        assert_eq!(result.test_order[0], "leaf");
        assert_eq!(result.test_order[1], "caller");

        // The caller should have used leaf as a mock
        let caller_result = result
            .function_results
            .iter()
            .find(|r| r.function_name == "caller")
            .expect("caller should be in results");
        assert!(caller_result.mocks_used.contains(&"leaf".to_string()));
    }

    #[tokio::test]
    async fn parallel_scan_single_worker() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);

        let analyses = vec![FunctionAnalysis {
            name: "solo".to_string(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(99),
            file_map,
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(10),
            cache: None,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.workers_used, 1);
        assert_eq!(result.function_results.len(), 1);
        assert_eq!(result.function_results[0].function_name, "solo");
        assert_eq!(result.function_results[0].exploration.iterations, 2);
    }

    #[tokio::test]
    async fn parallel_scan_persists_behavior_maps_to_cache() {
        use crate::cache::BehaviorMapCache;
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);

        let analyses = vec![FunctionAnalysis {
            name: "cached_fn".to_string(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("cached_fn".to_string(), "test.ts".to_string());

        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let cache = Arc::new(
            BehaviorMapCache::new(tmp_dir.path().to_path_buf()).expect("create cache"),
        );

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(10),
            cache: Some(cache.clone()),
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.function_results.len(), 1);

        // Verify the behavior map was persisted to cache.
        let loaded = cache
            .load("cached_fn")
            .expect("cache load should succeed");
        assert!(loaded.is_some(), "behavior map should be cached on disk");
        assert_eq!(loaded.as_ref().unwrap().function_id, "cached_fn");
    }
}
