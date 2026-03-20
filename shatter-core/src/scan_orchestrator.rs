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

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::auto_mock;
use crate::behavior::{BehaviorCoverage, BehaviorMap, CallGraph, CallGraphError, TestOrderEntry};
use crate::types::TypeInfo;
use crate::cache::BehaviorMapCache;
use crate::execution_record::ExecutionRecord;
use crate::explorer::{self, ExploreConfig, ExploreError, IsolationMode, ObservationOutput};
use crate::frontend::{Frontend, FrontendConfig, FrontendError};
use crate::interesting_pool::{self, InterestingPool};
use crate::mock_gen::mock_config_from_behavior_map;
use crate::protocol::{ExecuteResult, FunctionAnalysis, MockConfig};
use crate::setup_manager::SetupManager;

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
    /// Optional stratum filter. When set, only functions in the matching
    /// call graph layers are explored; callees outside are mocked.
    pub stratum: Option<crate::stratum::StratumSpec>,
    /// User-provided mock overrides from `.shatter/config.yaml`.
    /// Keys are dependency symbol names; values override auto-generated defaults.
    pub mock_overrides: HashMap<String, crate::auto_mock::MockOverride>,
    /// Path to checkpoint file for resume support.
    /// When `Some`, completed functions are loaded on startup and the
    /// checkpoint is updated after each layer completes.
    pub resume_path: Option<PathBuf>,
    /// Total scan wall-clock timeout. When set, the scan checks elapsed
    /// time at the start of each layer; if exceeded, remaining functions
    /// are skipped with reason "total scan timeout exceeded".
    pub timeout_total: Option<Duration>,
    /// Path to the interesting input pool file (e.g., `.shatter/seeds/pool.json`).
    /// When `Some`, interesting inputs discovered during exploration are
    /// harvested into the pool after each function completes.
    pub pool_path: Option<PathBuf>,
    /// Detected project root directory, passed to frontend commands.
    pub project_root: Option<String>,
    /// Directory from which to discover `.shatter/config.yaml` files.
    /// When set, per-function candidate inputs are loaded during scan.
    pub config_dir: Option<PathBuf>,
    /// Per-function exploration wall-clock timeout. Whichever of this or
    /// `max_iterations_per_function` triggers first stops the loop.
    pub timeout_explore: Option<Duration>,
    /// Optional setup manager for multi-level setup lifecycle.
    /// When provided, the scan orchestrator runs session setup before the scan,
    /// file setup/teardown per source file, and session teardown at the end.
    pub setup_manager: Option<SetupManager>,
    /// Scheduling policy controlling which exploration tasks may overlap.
    pub policy: crate::scheduler_policy::SchedulerPolicy,
    /// Execution isolation level for all functions in this scan.
    /// Defaults to `IsolationMode::None` (stateless/shared process).
    pub isolation: IsolationMode,
    /// When true, rich side-effect capture is enabled for all functions in
    /// this scan. Defaults to false for throughput.
    pub capture_side_effects: bool,
}

/// Context about sampling mode, for report headers.
#[derive(Debug, Clone, Default)]
pub struct SamplingContext {
    /// Total functions before sampling.
    pub total_functions: usize,
    /// Functions selected by core sample (0 if no sampling).
    pub sampled_functions: usize,
    /// Functions added via dependency closure.
    pub closure_functions: usize,
    /// Per-stratum breakdown.
    pub strata_summary: Vec<crate::core_sample::StratumInfo>,
}

/// Source of a mock used during exploration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockSource {
    /// Mock derived from a previously computed behavior map.
    CachedBehaviorMap,
    /// Auto-generated type-aware stub (no behavior map available).
    TypeAwareStub,
    /// Auto-mock for a function excluded by `--stratum` filtering.
    StratumExcluded,
}

/// A mock that was used during function exploration.
#[derive(Debug, Clone)]
pub struct MockUsage {
    /// Symbol name of the mocked dependency.
    pub name: String,
    /// How the mock was sourced.
    pub source: MockSource,
}

/// Result of exploring a single function during a scan.
#[derive(Debug)]
pub struct FunctionResult {
    /// Name of the explored function.
    pub function_name: String,
    /// The exploration result (paths, coverage, etc.).
    pub exploration: ObservationOutput,
    /// Behavior map built from execution results.
    pub behavior_map: BehaviorMap,
    /// Coverage of callee behaviors exercised by this function.
    pub behavior_coverage: Vec<BehaviorCoverage>,
    /// Mocks used during exploration, with source attribution.
    pub mocks_used: Vec<MockUsage>,
    /// Branch coverage metrics from the analyze stage.
    pub coverage_metrics: crate::coverage_metrics::CoverageMetrics,
    /// Refactoring recommendations for hard-to-mock dependencies.
    pub refactoring_recommendations: Vec<crate::mock_analysis::RefactoringRecommendation>,
}

/// Result of a full scan across multiple functions.
#[derive(Debug)]
pub struct ScanResult {
    /// Per-function results in test order.
    pub function_results: Vec<FunctionResult>,
    /// The order in which functions were tested.
    pub test_order: Vec<String>,
    /// Functions that were skipped before exploration (e.g. unexecutable parameter types).
    pub skipped_functions: Vec<SkippedFunction>,
    /// Sampling context (populated when --core-sample is active).
    pub sampling: Option<SamplingContext>,
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
    #[error("stratum error: {0}")]
    Stratum(String),
}

/// Per-function outcome used internally during parallel scan.
#[derive(Debug)]
enum FunctionOutcome {
    /// Exploration succeeded.
    Success(Box<FunctionResult>),
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

/// Whether a skip is benign (expected) or an actual error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipCategory {
    /// Benign: opaque types, cache hits, checkpoint resumes.
    Expected,
    /// Problematic: timeouts, exploration errors, crashes.
    Error,
}

/// Summary of a function that was skipped during a scan.
#[derive(Debug)]
pub struct SkippedFunction {
    /// Name of the function that was skipped.
    pub function_name: String,
    /// Reason the function was skipped.
    pub reason: String,
    /// Whether this skip is expected or an error.
    pub category: SkipCategory,
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
        scope_events: result.scope_events.clone(),
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

/// Compute the fingerprint for a function using its source text and analysis.
///
/// Reads the function's source from disk (using `file_map` + line range) and
/// hashes it with the analysis metadata. Returns `None` if the source cannot
/// be read (e.g., file not found or line range missing).
fn compute_fingerprint_for_function(
    func_name: &str,
    analysis: &FunctionAnalysis,
    config: &ScanConfig,
) -> Option<String> {
    let file_path = config.file_map.get(func_name)?;
    if analysis.start_line == 0 || analysis.end_line == 0 {
        return None;
    }
    let source = crate::fingerprint::extract_function_source(
        std::path::Path::new(file_path),
        analysis.start_line,
        analysis.end_line,
    )
    .ok()?;
    Some(crate::fingerprint::compute_function_fingerprint(&source, analysis))
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

    // Flatten test order entries into layers for stratum filtering.
    let all_layers = build_layers(&order_entries, &call_graph);

    // Apply stratum filter: only explore functions in selected layers.
    let (filtered_layers, stratum_excluded) = if let Some(ref spec) = config.stratum {
        let max_layer = if all_layers.is_empty() { 0 } else { all_layers.len() - 1 };
        let range = crate::stratum::resolve_range(spec, max_layer)?;
        let selected: Vec<Vec<String>> = crate::stratum::filter_layers(&all_layers, &range)
            .into_iter()
            .map(|(_, funcs)| funcs.clone())
            .collect();
        let selected_set: HashSet<String> = selected.iter().flatten().cloned().collect();
        let excluded: HashSet<String> = all_layers
            .iter()
            .flatten()
            .filter(|f| !selected_set.contains(f.as_str()))
            .cloned()
            .collect();
        (selected, excluded)
    } else {
        (all_layers, HashSet::new())
    };

    // Flatten filtered layers into function names for iteration.
    let test_order: Vec<String> = filtered_layers
        .into_iter()
        .flat_map(|layer| layer.into_iter())
        .collect();

    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    let mut behavior_maps: HashMap<String, BehaviorMap> = HashMap::new();
    let mut function_results: Vec<FunctionResult> = Vec::new();
    let mut skipped_functions: Vec<SkippedFunction> = Vec::new();
    let mut deep_fingerprints: HashMap<String, String> = HashMap::new();

    // Load checkpoint for resume support.
    let scan_id = crate::checkpoint::ScanCheckpoint::compute_scan_id(
        &config.file_map.values().map(|s| s.as_str()).collect::<Vec<_>>(),
    );
    let mut checkpoint = match &config.resume_path {
        Some(path) => {
            match crate::checkpoint::ScanCheckpoint::load(path) {
                Ok(Some(cp)) if cp.scan_id == scan_id => cp,
                Ok(Some(_)) => {
                    log::info!("checkpoint scan_id mismatch, starting fresh");
                    crate::checkpoint::ScanCheckpoint::new(scan_id)
                }
                Ok(None) => crate::checkpoint::ScanCheckpoint::new(scan_id),
                Err(e) => {
                    log::warn!("failed to load checkpoint: {e}, starting fresh");
                    crate::checkpoint::ScanCheckpoint::new(scan_id)
                }
            }
        }
        None => crate::checkpoint::ScanCheckpoint::new(scan_id),
    };

    // Load the interesting input pool for cross-function seed sharing.
    let mut input_pool = config
        .pool_path
        .as_ref()
        .and_then(|p| interesting_pool::load_pool(p).ok().flatten())
        .unwrap_or_default();
    input_pool.epoch += 1;

    for func_name in &test_order {
        let analysis = match analysis_map.get(func_name.as_str()) {
            Some(a) => *a,
            None => continue,
        };

        // Compute shallow fingerprint, then deep fingerprint incorporating callees.
        let shallow_fingerprint = compute_fingerprint_for_function(func_name, analysis, config);
        let callees = call_graph.callees(func_name);
        let current_deep_fp = shallow_fingerprint.as_ref().map(|sfp| {
            crate::fingerprint::compute_deep_fingerprint(sfp, &deep_fingerprints, &callees)
        });

        // Check resume checkpoint first (uses deep FP).
        if let (Some(cache), Some(dfp)) = (&config.cache, &current_deep_fp)
            && checkpoint.is_completed(func_name, dfp, cache)
            && let Ok(Some(cached_map)) = cache.load(func_name)
        {
            behavior_maps.insert(func_name.clone(), cached_map);
            deep_fingerprints.insert(func_name.clone(), dfp.clone());
            skipped_functions.push(SkippedFunction {
                function_name: func_name.clone(),
                reason: "resumed from checkpoint".into(),
                category: SkipCategory::Expected,
            });
            continue;
        }

        // Check cache freshness using deep fingerprint.
        if let (Some(cache), Some(dfp)) = (&config.cache, &current_deep_fp)
            && let Ok(true) = cache.is_fresh(func_name, dfp)
            && let Ok(Some(cached_map)) = cache.load(func_name)
        {
            behavior_maps.insert(func_name.clone(), cached_map);
            deep_fingerprints.insert(func_name.clone(), dfp.clone());
            skipped_functions.push(SkippedFunction {
                function_name: func_name.clone(),
                reason: "unchanged (fingerprint match)".into(),
                category: SkipCategory::Expected,
            });
            continue;
        }

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
        let mut mocks: Vec<MockConfig> = Vec::new();
        let mut mocks_used: Vec<MockUsage> = Vec::new();

        for callee in &callees {
            if let Some(bmap) = behavior_maps.get(callee) {
                mocks.push(mock_config_from_behavior_map(bmap));
                mocks_used.push(MockUsage {
                    name: callee.clone(),
                    source: MockSource::CachedBehaviorMap,
                });
            }
        }

        // Generate auto-mocks for remaining unmocked dependencies.
        let auto_mocks = crate::auto_mock::generate_auto_mocks(
            &analysis.dependencies,
            None,
            &config.mock_overrides,
            &mocks,
        );
        for am in &auto_mocks {
            let source = if stratum_excluded.contains(&am.symbol) {
                MockSource::StratumExcluded
            } else {
                MockSource::TypeAwareStub
            };
            mocks_used.push(MockUsage {
                name: am.symbol.clone(),
                source,
            });
        }
        mocks.extend(auto_mocks);
        mocks_used.sort_by(|a, b| a.name.cmp(&b.name));

        let file = config
            .file_map
            .get(func_name)
            .cloned()
            .unwrap_or_default();

        let pool_seeds = crate::input_gen::pool_to_candidate_inputs(&analysis.params, &input_pool);

        let candidate_inputs = load_config_candidate_inputs(
            func_name,
            &config.config_dir,
            config.max_iterations_per_function,
            config.timeout_per_fn.as_secs(),
        );

        let explore_config = ExploreConfig {
            file,
            max_iterations: config.max_iterations_per_function,
            seed: config.seed,
            mocks,
            mock_params: vec![],
            setup_file: None,
            setup_level: crate::protocol::SetupLevel::Function,
            value_sources: vec![],
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs,
            pool_seeds,
            project_root: config.project_root.clone(),
            loop_buckets: explorer::LoopBuckets::default(),
            timeout_explore: config.timeout_explore,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: crate::orchestrator::DEFAULT_SHRINK_BUDGET,
            isolation: config.isolation,
            capture_side_effects: config.capture_side_effects,
        };

        let exploration = explorer::explore_function(frontend, analysis, &explore_config, None).await?;

        // Harvest interesting inputs into the cross-function pool.
        interesting_pool::harvest_from_exploration(
            &mut input_pool,
            &exploration.raw_results,
            &analysis.params,
            func_name,
        );

        // Run the Analyze stage to produce behavior map and coverage metrics.
        let mut analyze_out = crate::pipeline::analyze(&exploration, analysis);
        analyze_out.behavior_map.fingerprint = current_deep_fp.clone();

        // Persist the behavior map to cache for reuse across runs.
        if let Some(ref cache) = config.cache {
            let _ = cache.store(&analyze_out.behavior_map);
        }

        // Record deep fingerprint for downstream functions.
        if let Some(ref dfp) = current_deep_fp {
            deep_fingerprints.insert(func_name.clone(), dfp.clone());
            checkpoint.mark_completed(func_name, dfp);
        }

        // Save checkpoint periodically.
        if let Some(ref path) = config.resume_path {
            let _ = checkpoint.save(path);
        }

        // Compute behavior coverage for each callee (cross-function concern).
        let records: Vec<ExecutionRecord> = exploration
            .raw_results
            .iter()
            .map(|(inputs, _mocks, result)| execution_record_from_result(func_name, inputs, result))
            .collect();
        let mut behavior_coverage: Vec<BehaviorCoverage> = Vec::new();
        for callee in &callees {
            if let Some(callee_map) = behavior_maps.get(callee) {
                let coverage = BehaviorCoverage::compute(func_name, &records, callee_map);
                behavior_coverage.push(coverage);
            }
        }

        behavior_maps.insert(func_name.clone(), analyze_out.behavior_map.clone());

        let refactoring_recommendations =
            crate::mock_analysis::generate_recommendations(&analysis.dependencies);

        function_results.push(FunctionResult {
            function_name: func_name.clone(),
            exploration,
            behavior_map: analyze_out.behavior_map,
            behavior_coverage,
            mocks_used,
            coverage_metrics: analyze_out.coverage_metrics,
            refactoring_recommendations,
        });
    }

    // Save the interesting input pool if configured.
    if let Some(ref pool_path) = config.pool_path
        && let Err(e) = interesting_pool::save_pool(&input_pool, pool_path)
    {
        log::warn!("failed to save interesting pool: {e}");
    }

    Ok(ScanResult {
        function_results,
        test_order,
        skipped_functions,
        sampling: None,
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
    /// Sampling context (populated when --core-sample is active).
    pub sampling: Option<SamplingContext>,
}

/// A channel-based pool of frontend worker subprocesses with adaptive growth.
///
/// Workers are checked out via `checkout()` and returned via `return_worker()`.
/// The pool starts with a fraction of `max_workers` and grows toward the ceiling
/// as tasks complete and more runnable work remains.
struct WorkerPool {
    sender: tokio::sync::mpsc::Sender<Frontend>,
    receiver: Mutex<tokio::sync::mpsc::Receiver<Frontend>>,
    /// Hard ceiling on spawned workers (from `--parallelism`).
    max_workers: usize,
    /// Count of workers currently alive (checked out or in the channel). Only grows.
    current_size: Arc<AtomicUsize>,
    /// Config used to spawn replacement and growth workers.
    config: Arc<FrontendConfig>,
}

/// Number of workers to create at pool startup.
///
/// Starts at ≈25 % of `max_workers` (minimum 1), capped by actual task count so
/// we never over-provision on sparse layers.  The pool grows toward `max_workers`
/// as tasks complete and more work is still queued.
fn initial_workers(max_workers: usize, needed: usize) -> usize {
    let quarter = (max_workers / 4).max(1);
    quarter.min(needed).min(max_workers)
}

impl WorkerPool {
    /// Spawn an initial batch of frontend subprocesses and place them in the pool.
    ///
    /// Starts with `initial_workers(max_workers, needed)` workers rather than the
    /// full `max_workers`, then grows on demand via [`maybe_grow`].
    async fn spawn_capped(
        config: Arc<FrontendConfig>,
        max_workers: usize,
        needed: usize,
    ) -> Result<Self, FrontendError> {
        let initial = initial_workers(max_workers, needed);
        // Channel capacity == max_workers so growth workers can always be deposited.
        let (sender, receiver) = tokio::sync::mpsc::channel(max_workers);
        for _ in 0..initial {
            let frontend = Frontend::spawn(&config).await?;
            sender.send(frontend).await.expect("channel has capacity for initial workers");
        }
        Ok(Self {
            sender,
            receiver: Mutex::new(receiver),
            max_workers,
            current_size: Arc::new(AtomicUsize::new(initial)),
            config,
        })
    }

    /// Check out a worker from the pool, blocking until one is available.
    async fn checkout(&self) -> Frontend {
        let mut rx = self.receiver.lock().await;
        rx.recv().await.expect("pool should not be empty")
    }

    /// Return a worker to the pool.
    async fn return_worker(&self, frontend: Frontend) {
        let _ = self.sender.send(frontend).await;
    }

    /// Grow the pool by one worker if demand justifies it and we are below the ceiling.
    ///
    /// `tasks_remaining` is the number of tasks that have not yet completed.  If that
    /// exceeds `current_size`, tasks are blocked on `checkout()` and a new worker will
    /// reduce their wait.  The CAS ensures at most one growth per available slot even
    /// when multiple tasks return concurrently.  The actual subprocess spawn runs in a
    /// detached task so the caller is not delayed.
    fn maybe_grow(&self, tasks_remaining: usize) {
        let current = self.current_size.load(Ordering::Relaxed);
        if tasks_remaining <= current || current >= self.max_workers {
            return;
        }
        if self.current_size
            .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return; // another concurrent task already claimed the slot
        }
        let sender = self.sender.clone();
        let config = Arc::clone(&self.config);
        let current_size = Arc::clone(&self.current_size);
        tokio::spawn(async move {
            match Frontend::spawn(&config).await {
                Ok(fe) => {
                    let _ = sender.send(fe).await;
                }
                Err(_) => {
                    // Release the claimed slot so future growth attempts can retry.
                    current_size.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });
    }

    /// Return the number of workers currently alive (in pool or checked out).
    fn current_size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }

    /// Shut down all workers remaining in the pool.
    ///
    /// Any in-flight `maybe_grow` tasks hold a sender clone, so `rx.recv()` waits
    /// for them naturally — newly spawned frontends are captured and shut down too.
    async fn shutdown(self) {
        drop(self.sender);
        let mut rx = self.receiver.into_inner();
        while let Some(frontend) = rx.recv().await {
            let _ = frontend.shutdown().await;
        }
    }
}

/// Execute a layer of function tasks in Function isolation mode.
///
/// Each function gets a dedicated fresh frontend process. Concurrency is capped
/// by `max_concurrent` (from `--parallelism`). The process is killed when the
/// function's exploration completes — it is never shared with other functions.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
async fn run_layer_function_mode(
    fe_config: Arc<FrontendConfig>,
    tasks: Vec<(
        String,
        FunctionAnalysis,
        ExploreConfig,
        Vec<MockUsage>,
        HashSet<String>,
        Option<String>,
    )>,
    max_concurrent: usize,
    timeout: Duration,
    cache: &Option<Arc<BehaviorMapCache>>,
    behavior_maps: &Arc<Mutex<HashMap<String, BehaviorMap>>>,
    input_pool: &Arc<Mutex<InterestingPool>>,
) -> (Vec<FunctionOutcome>, usize) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for (func_name, analysis, explore_config, mocks_used, callees, deep_fp) in tasks {
        let semaphore = Arc::clone(&semaphore);
        let fe_config = Arc::clone(&fe_config);
        let behavior_maps = Arc::clone(behavior_maps);
        let input_pool = Arc::clone(input_pool);
        let cache = cache.clone();

        let handle = tokio::spawn(async move {
            // Acquire a concurrency slot before spawning the frontend.
            let _permit = semaphore.acquire().await.expect("semaphore is never closed");

            let mut frontend = match Frontend::spawn(&fe_config).await {
                Ok(fe) => fe,
                Err(e) => {
                    return FunctionOutcome::Error {
                        function_name: func_name,
                        error: e.to_string(),
                    };
                }
            };

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
                    deep_fp,
                    &input_pool,
                ),
            )
            .await;

            // Always shut down the dedicated frontend — never return to a pool.
            let _ = frontend.shutdown().await;

            match result {
                Ok(Ok(func_result)) => {
                    let mut maps = behavior_maps.lock().await;
                    maps.insert(func_name.clone(), func_result.behavior_map.clone());
                    drop(maps);
                    if let Some(ref cache) = cache {
                        let _ = cache.store(&func_result.behavior_map);
                    }
                    FunctionOutcome::Success(Box::new(func_result))
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

    let peak = max_concurrent.min(handles.len());
    let mut outcomes = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(outcome) => outcomes.push(outcome),
            Err(e) => outcomes.push(FunctionOutcome::Error {
                function_name: "(unknown)".into(),
                error: format!("task join error: {e}"),
            }),
        }
    }
    (outcomes, peak)
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
    let all_layers = build_layers(&order_entries, &call_graph);

    // Apply stratum filter: only explore functions in selected layers.
    let (layers, stratum_excluded) = if let Some(ref spec) = config.stratum {
        let max_layer = if all_layers.is_empty() { 0 } else { all_layers.len() - 1 };
        let range = crate::stratum::resolve_range(spec, max_layer)?;
        let selected: Vec<Vec<String>> = crate::stratum::filter_layers(&all_layers, &range)
            .into_iter()
            .map(|(_, funcs)| funcs.clone())
            .collect();
        let selected_set: HashSet<String> = selected.iter().flatten().cloned().collect();
        let excluded: HashSet<String> = all_layers
            .iter()
            .flatten()
            .filter(|f| !selected_set.contains(f.as_str()))
            .cloned()
            .collect();
        (selected, excluded)
    } else {
        (all_layers, HashSet::new())
    };

    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    let effective_parallelism = config.policy.effective_workers(config.parallelism).max(1);
    // Workers are created lazily per-layer; track the peak count across layers.
    let mut peak_workers: usize = 0;

    let behavior_maps: Arc<Mutex<HashMap<String, BehaviorMap>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Load the interesting input pool for cross-function seed sharing.
    let input_pool: Arc<Mutex<InterestingPool>> = {
        let loaded = config
            .pool_path
            .as_ref()
            .and_then(|p| interesting_pool::load_pool(p).ok().flatten())
            .unwrap_or_default();
        Arc::new(Mutex::new(loaded))
    };
    input_pool.lock().await.epoch += 1;

    // Deep fingerprints: accumulated across layers. Since functions within a
    // layer have no cross-dependencies, callee deep FPs are always from prior
    // layers and this map is immutable during within-layer parallel execution.
    let mut deep_fingerprints: HashMap<String, String> = HashMap::new();

    // Load checkpoint for resume support.
    let scan_id = crate::checkpoint::ScanCheckpoint::compute_scan_id(
        &config.file_map.values().map(|s| s.as_str()).collect::<Vec<_>>(),
    );
    let mut checkpoint = match &config.resume_path {
        Some(path) => {
            match crate::checkpoint::ScanCheckpoint::load(path) {
                Ok(Some(cp)) if cp.scan_id == scan_id => cp,
                Ok(Some(_)) => {
                    log::info!("checkpoint scan_id mismatch, starting fresh");
                    crate::checkpoint::ScanCheckpoint::new(scan_id)
                }
                Ok(None) => crate::checkpoint::ScanCheckpoint::new(scan_id),
                Err(e) => {
                    log::warn!("failed to load checkpoint: {e}, starting fresh");
                    crate::checkpoint::ScanCheckpoint::new(scan_id)
                }
            }
        }
        None => crate::checkpoint::ScanCheckpoint::new(scan_id),
    };

    let mut all_results: Vec<FunctionResult> = Vec::new();
    let mut test_order: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedFunction> = Vec::new();

    let scan_start = Instant::now();

    for (layer_idx, layer) in layers.iter().enumerate() {
        // Check total scan timeout at layer boundary.
        if let Some(total) = config.timeout_total
            && scan_start.elapsed() >= total
        {
            // Skip all functions in this and remaining layers.
            for remaining_layer in &layers[layer_idx..] {
                for func_name in remaining_layer {
                    skipped.push(SkippedFunction {
                        function_name: func_name.clone(),
                        reason: "total scan timeout exceeded".into(),
                        category: SkipCategory::Error,
                    });
                }
            }
            break;
        }

        // Build tasks for this layer: each function paired with its mocks.
        let mut tasks = Vec::new();
        // Track deep FPs computed in this layer (added after the layer completes).
        let mut layer_deep_fps: Vec<(String, String)> = Vec::new();

        for func_name in layer {
            test_order.push(func_name.clone());

            let analysis = match analysis_map.get(func_name.as_str()) {
                Some(a) => *a,
                None => {
                    skipped.push(SkippedFunction {
                        function_name: func_name.clone(),
                        reason: "no analysis found".into(),
                        category: SkipCategory::Error,
                    });
                    continue;
                }
            };

            // Compute shallow fingerprint, then deep fingerprint incorporating callees.
            let shallow_fingerprint =
                compute_fingerprint_for_function(func_name, analysis, config);

            let callees = call_graph.callees(func_name);
            let current_deep_fp = shallow_fingerprint.as_ref().map(|sfp| {
                crate::fingerprint::compute_deep_fingerprint(sfp, &deep_fingerprints, &callees)
            });

            // Check resume checkpoint first (uses deep FP).
            if let (Some(cache), Some(dfp)) = (&config.cache, &current_deep_fp)
                && checkpoint.is_completed(func_name, dfp, cache)
                && let Ok(Some(cached_map)) = cache.load(func_name)
            {
                let mut maps = behavior_maps.lock().await;
                maps.insert(func_name.clone(), cached_map);
                drop(maps);
                layer_deep_fps.push((func_name.clone(), dfp.clone()));
                skipped.push(SkippedFunction {
                    function_name: func_name.clone(),
                    reason: "resumed from checkpoint".into(),
                    category: SkipCategory::Expected,
                });
                continue;
            }

            // Check cache freshness using deep fingerprint.
            if let (Some(cache), Some(dfp)) = (&config.cache, &current_deep_fp)
                && let Ok(true) = cache.is_fresh(func_name, dfp)
                && let Ok(Some(cached_map)) = cache.load(func_name)
            {
                let mut maps = behavior_maps.lock().await;
                maps.insert(func_name.clone(), cached_map);
                drop(maps);
                layer_deep_fps.push((func_name.clone(), dfp.clone()));
                skipped.push(SkippedFunction {
                    function_name: func_name.clone(),
                    reason: "unchanged (fingerprint match)".into(),
                    category: SkipCategory::Expected,
                });
                continue;
            }

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
            let maps = behavior_maps.lock().await;
            let mut mocks: Vec<MockConfig> = Vec::new();
            let mut mocks_used: Vec<MockUsage> = Vec::new();
            for callee in &callees {
                if let Some(bmap) = maps.get(callee) {
                    mocks.push(mock_config_from_behavior_map(bmap));
                    mocks_used.push(MockUsage {
                        name: callee.clone(),
                        source: MockSource::CachedBehaviorMap,
                    });
                }
            }
            drop(maps);

            // Generate auto-mocks for remaining unmocked dependencies.
            let auto_mocks = crate::auto_mock::generate_auto_mocks(
                &analysis.dependencies,
                None,
                &config.mock_overrides,
                &mocks,
            );
            for am in &auto_mocks {
                let source = if stratum_excluded.contains(&am.symbol) {
                    MockSource::StratumExcluded
                } else {
                    MockSource::TypeAwareStub
                };
                mocks_used.push(MockUsage {
                    name: am.symbol.clone(),
                    source,
                });
            }
            mocks.extend(auto_mocks);
            mocks_used.sort_by(|a, b| a.name.cmp(&b.name));

            let file = config
                .file_map
                .get(func_name)
                .cloned()
                .unwrap_or_default();

            let pool_seeds = {
                let pool_guard = input_pool.lock().await;
                crate::input_gen::pool_to_candidate_inputs(&analysis.params, &pool_guard)
            };

            let candidate_inputs = load_config_candidate_inputs(
                func_name,
                &config.config_dir,
                config.max_iterations_per_function,
                config.timeout_per_fn.as_secs(),
            );

            let explore_config = ExploreConfig {
                file,
                max_iterations: config.max_iterations_per_function,
                seed: config.seed,
                mocks,
                mock_params: vec![],
                setup_file: None,
                setup_level: crate::protocol::SetupLevel::Function,
                value_sources: vec![],
                capabilities: crate::orchestrator::FrontendCapabilities::default(),
                user_seeds: vec![],
                candidate_inputs,
                pool_seeds,
                project_root: config.project_root.clone(),
                loop_buckets: explorer::LoopBuckets::default(),
                timeout_explore: config.timeout_explore,
                meta_config: crate::strategy::MetaConfig::default(),
                shrink_budget: crate::orchestrator::DEFAULT_SHRINK_BUDGET,
                isolation: config.isolation,
                capture_side_effects: config.capture_side_effects,
            };

            tasks.push((func_name.clone(), analysis.clone(), explore_config, mocks_used, callees, current_deep_fp));
        }

        // Execute tasks in parallel, using either the shared WorkerPool (default)
        // or per-function dedicated frontends (Function isolation mode).
        // The pool/semaphore is created lazily — only when there is real
        // exploration work to do in this layer. Cache-hit layers skip it.
        if !tasks.is_empty() {
            let fe_config = Arc::new(frontend_config.clone());

            // Collect outcomes from either isolation path.
            let layer_outcomes: Vec<FunctionOutcome> =
                if config.isolation == IsolationMode::Function {
                    // Function mode: each function gets a dedicated fresh frontend.
                    // No shared pool — a Semaphore caps concurrency instead.
                    let (outcomes, layer_peak) = run_layer_function_mode(
                        Arc::clone(&fe_config),
                        tasks,
                        effective_parallelism,
                        config.timeout_per_fn,
                        &config.cache,
                        &behavior_maps,
                        &input_pool,
                    )
                    .await;
                    peak_workers = peak_workers.max(layer_peak);
                    outcomes
                } else {
                    // Default: shared WorkerPool — workers are reused across functions.
                    let pool = Arc::new(
                        WorkerPool::spawn_capped(
                            Arc::clone(&fe_config),
                            effective_parallelism,
                            tasks.len(),
                        )
                        .await
                        .map_err(ScanError::Frontend)?,
                    );

                    // Each task decrements this counter after returning its worker so that
                    // `maybe_grow` can detect tasks still blocked on `checkout()`.
                    let tasks_remaining = Arc::new(AtomicUsize::new(tasks.len()));

                    // Each task checks out a worker, explores, then returns the worker.
                    let mut handles = Vec::new();

                    for (func_name, analysis, explore_config, mocks_used, callees, deep_fp) in
                        tasks
                    {
                        let pool = Arc::clone(&pool);
                        let behavior_maps = Arc::clone(&behavior_maps);
                        let input_pool = Arc::clone(&input_pool);
                        let timeout = config.timeout_per_fn;
                        let cache = config.cache.clone();
                        let fe_config = Arc::clone(&fe_config);
                        let tasks_remaining = Arc::clone(&tasks_remaining);

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
                                    deep_fp.clone(),
                                    &input_pool,
                                ),
                            )
                            .await;

                            let timed_out = result.is_err();

                            // After a timeout the frontend's stdout buffer contains a
                            // stale response that would cause an ID mismatch on the next
                            // request.  Kill and respawn instead of returning to pool.
                            if timed_out || !frontend.is_alive() {
                                // Drop the poisoned/dead frontend (kills the child process).
                                drop(frontend);
                                match Frontend::spawn(&fe_config).await {
                                    Ok(new_fe) => pool.return_worker(new_fe).await,
                                    Err(_) => { /* pool shrinks — acceptable degradation */ }
                                }
                            } else {
                                pool.return_worker(frontend).await;
                            }

                            // Decrement the remaining-task counter and grow the pool if
                            // there are tasks still blocked on checkout() and room to grow.
                            let remaining =
                                tasks_remaining.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
                            pool.maybe_grow(remaining);

                            match result {
                                Ok(Ok(func_result)) => {
                                    // Store the behavior map for downstream functions.
                                    let mut maps = behavior_maps.lock().await;
                                    maps.insert(
                                        func_name.clone(),
                                        func_result.behavior_map.clone(),
                                    );
                                    drop(maps);

                                    // Persist to disk cache for reuse across runs.
                                    if let Some(ref cache) = cache {
                                        let _ = cache.store(&func_result.behavior_map);
                                    }

                                    FunctionOutcome::Success(Box::new(func_result))
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

                    let mut outcomes = Vec::with_capacity(handles.len());
                    for handle in handles {
                        match handle.await {
                            Ok(outcome) => outcomes.push(outcome),
                            Err(e) => outcomes.push(FunctionOutcome::Error {
                                function_name: "(unknown)".into(),
                                error: format!("task join error: {e}"),
                            }),
                        }
                    }

                    // Record the actual peak before shutdown — pool may have grown during the layer.
                    peak_workers = peak_workers.max(pool.current_size());

                    // Shut down this layer's workers now that all tasks are done.
                    // This frees subprocess resources before the next layer begins,
                    // and ensures zero processes remain when the layer was a cache hit.
                    if let Ok(p) = Arc::try_unwrap(pool) {
                        p.shutdown().await;
                    }
                    outcomes
                };

            // Process outcomes from whichever path ran.
            for outcome in layer_outcomes {
                match outcome {
                    FunctionOutcome::Success(result) => {
                        // Record deep FP for this function so downstream layers
                        // can incorporate it into their deep fingerprints.
                        if let Some(ref fp) = result.behavior_map.fingerprint {
                            layer_deep_fps.push((result.function_name.clone(), fp.clone()));
                        }
                        all_results.push(*result);
                    }
                    FunctionOutcome::Timeout { function_name, limit } => {
                        skipped.push(SkippedFunction {
                            function_name,
                            reason: format!("timed out after {:.0}s", limit.as_secs_f64()),
                            category: SkipCategory::Error,
                        });
                    }
                    FunctionOutcome::Error { function_name, error } => {
                        skipped.push(SkippedFunction {
                            function_name,
                            reason: format!("error: {error}"),
                            category: SkipCategory::Error,
                        });
                    }
                }
            }
        }

        // Merge this layer's deep fingerprints into the accumulated map.
        for (name, fp) in layer_deep_fps {
            checkpoint.mark_completed(&name, &fp);
            deep_fingerprints.insert(name, fp);
        }

        // Persist checkpoint after each layer completes.
        checkpoint.layer_index = layer_idx;
        if let Some(ref path) = config.resume_path {
            let _ = checkpoint.save(path);
        }
    }

    // Save the interesting input pool if configured.
    if let Some(ref pool_path) = config.pool_path {
        let pool_guard = input_pool.lock().await;
        if let Err(e) = interesting_pool::save_pool(&pool_guard, pool_path) {
            log::warn!("failed to save interesting pool: {e}");
        }
    }

    Ok(ParallelScanResult {
        function_results: all_results,
        test_order,
        skipped,
        workers_used: peak_workers,
        sampling: None,
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
#[allow(clippy::too_many_arguments)]
async fn explore_single_function(
    frontend: &mut Frontend,
    func_name: &str,
    analysis: &FunctionAnalysis,
    explore_config: &ExploreConfig,
    mocks_used: &[MockUsage],
    callees: &std::collections::HashSet<String>,
    behavior_maps: &Mutex<HashMap<String, BehaviorMap>>,
    fingerprint: Option<String>,
    input_pool: &Mutex<InterestingPool>,
) -> Result<FunctionResult, ScanError> {
    let exploration = explorer::explore_function(frontend, analysis, explore_config, None).await?;

    // Harvest interesting inputs into the cross-function pool.
    {
        let mut pool_guard = input_pool.lock().await;
        interesting_pool::harvest_from_exploration(
            &mut pool_guard,
            &exploration.raw_results,
            &analysis.params,
            func_name,
        );
    }

    // Run the Analyze stage to produce behavior map and coverage metrics.
    let mut analyze_out = crate::pipeline::analyze(&exploration, analysis);
    analyze_out.behavior_map.fingerprint = fingerprint;

    // Compute behavior coverage for each callee (cross-function concern).
    let records: Vec<ExecutionRecord> = exploration
        .raw_results
        .iter()
        .map(|(inputs, _mocks, result)| execution_record_from_result(func_name, inputs, result))
        .collect();
    let maps = behavior_maps.lock().await;
    let mut behavior_coverage: Vec<BehaviorCoverage> = Vec::new();
    for callee in callees {
        if let Some(callee_map) = maps.get(callee) {
            let coverage = BehaviorCoverage::compute(func_name, &records, callee_map);
            behavior_coverage.push(coverage);
        }
    }
    drop(maps);

    let refactoring_recommendations =
        crate::mock_analysis::generate_recommendations(&analysis.dependencies);

    Ok(FunctionResult {
        function_name: func_name.to_string(),
        exploration,
        behavior_map: analyze_out.behavior_map,
        behavior_coverage,
        mocks_used: mocks_used.to_vec(),
        coverage_metrics: analyze_out.coverage_metrics,
        refactoring_recommendations,
    })
}

/// Format mock usages as a human-readable string with source attribution.
fn format_mocks_used(mocks: &[MockUsage]) -> String {
    let cached: Vec<&str> = mocks
        .iter()
        .filter(|m| m.source == MockSource::CachedBehaviorMap)
        .map(|m| m.name.as_str())
        .collect();
    let stubs: Vec<&str> = mocks
        .iter()
        .filter(|m| m.source == MockSource::TypeAwareStub)
        .map(|m| m.name.as_str())
        .collect();
    let excluded: Vec<&str> = mocks
        .iter()
        .filter(|m| m.source == MockSource::StratumExcluded)
        .map(|m| m.name.as_str())
        .collect();

    let mut parts = Vec::new();
    if !cached.is_empty() {
        parts.push(format!("{} via behavior map ({})", cached.len(), cached.join(", ")));
    }
    if !stubs.is_empty() {
        parts.push(format!("{} via type-aware stub ({})", stubs.len(), stubs.join(", ")));
    }
    if !excluded.is_empty() {
        parts.push(format!("{} stratum-excluded ({})", excluded.len(), excluded.join(", ")));
    }
    parts.join("; ")
}

/// Format a parallel scan result as a human-readable report.
pub fn format_parallel_scan_report(result: &ParallelScanResult) -> String {
    let mut out = String::new();

    if let Some(ref ctx) = result.sampling {
        let pct = if ctx.total_functions > 0 {
            (ctx.sampled_functions as f64 / ctx.total_functions as f64 * 100.0).round() as usize
        } else {
            0
        };
        out.push_str(&format!(
            "Explored {}/{} functions ({}% core sample, {} via dependency closure)\n",
            ctx.sampled_functions + ctx.closure_functions,
            ctx.total_functions,
            pct,
            ctx.closure_functions,
        ));
    }

    let expected: Vec<_> = result.skipped.iter().filter(|s| s.category == SkipCategory::Expected).collect();
    let errors: Vec<_> = result.skipped.iter().filter(|s| s.category == SkipCategory::Error).collect();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested, {} skipped, {} error(s) ({} worker(s))\n",
        result.function_results.len(),
        expected.len(),
        errors.len(),
        result.workers_used,
    ));

    for func_result in &result.function_results {
        out.push_str(&format!("\n-- {} --\n", func_result.function_name));
        out.push_str(&explorer::format_exploration_report_verbose(
            &func_result.exploration,
        ));

        if !func_result.mocks_used.is_empty() {
            out.push_str(&format!(
                "  Mocks used: {}\n",
                format_mocks_used(&func_result.mocks_used)
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

        let recs_text =
            crate::mock_analysis::format_recommendations(&func_result.refactoring_recommendations);
        if !recs_text.is_empty() {
            out.push_str(&format!("\n{recs_text}"));
        }
    }

    format_skip_sections(&expected, &errors, &mut out);

    out
}

/// Format a scan result as a human-readable report.
pub fn format_scan_report(result: &ScanResult) -> String {
    let mut out = String::new();

    if let Some(ref ctx) = result.sampling {
        let pct = if ctx.total_functions > 0 {
            (ctx.sampled_functions as f64 / ctx.total_functions as f64 * 100.0).round() as usize
        } else {
            0
        };
        out.push_str(&format!(
            "Explored {}/{} functions ({}% core sample, {} via dependency closure)\n",
            ctx.sampled_functions + ctx.closure_functions,
            ctx.total_functions,
            pct,
            ctx.closure_functions,
        ));
    }

    let expected: Vec<_> = result.skipped_functions.iter().filter(|s| s.category == SkipCategory::Expected).collect();
    let errors: Vec<_> = result.skipped_functions.iter().filter(|s| s.category == SkipCategory::Error).collect();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested\n",
        result.function_results.len()
    ));

    for func_result in &result.function_results {
        out.push_str(&format!("\n── {} ──\n", func_result.function_name));

        out.push_str(&explorer::format_exploration_report_verbose(&func_result.exploration));

        if !func_result.mocks_used.is_empty() {
            out.push_str(&format!(
                "  Mocks used: {}\n",
                format_mocks_used(&func_result.mocks_used)
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

        let recs_text =
            crate::mock_analysis::format_recommendations(&func_result.refactoring_recommendations);
        if !recs_text.is_empty() {
            out.push_str(&format!("\n{recs_text}"));
        }
    }

    format_skip_sections(&expected, &errors, &mut out);

    out
}

/// Append "Skipped (expected)" and "Errors" sections to a report string.
fn format_skip_sections(expected: &[&SkippedFunction], errors: &[&SkippedFunction], out: &mut String) {
    if !expected.is_empty() {
        out.push_str(&format!("\nSkipped (expected, {}):\n", expected.len()));
        for skip in expected {
            out.push_str(&format!("  {}: {}\n", skip.function_name, skip.reason));
        }
    }

    if !errors.is_empty() {
        out.push_str(&format!("\nErrors ({}):\n", errors.len()));
        for skip in errors {
            out.push_str(&format!("  {}: {}\n", skip.function_name, skip.reason));
        }
    }
}

/// Format a [`TypeInfo`] as a concise human-readable string.
fn format_type(ty: &TypeInfo) -> String {
    match ty {
        TypeInfo::Int => "int".to_string(),
        TypeInfo::Float => "float".to_string(),
        TypeInfo::Str => "string".to_string(),
        TypeInfo::Bool => "bool".to_string(),
        TypeInfo::Array { element } => format!("{}[]", format_type(element)),
        TypeInfo::Nullable { inner } => format!("{}?", format_type(inner)),
        TypeInfo::Object { fields } => {
            if fields.is_empty() {
                "object".to_string()
            } else {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(name, t)| format!("{name}: {}", format_type(t)))
                    .collect();
                format!("{{{}}}", field_strs.join(", "))
            }
        }
        TypeInfo::Union { variants } => {
            variants
                .iter()
                .map(format_type)
                .collect::<Vec<_>>()
                .join(" | ")
        }
        TypeInfo::Complex { kind, .. } => format!("{kind:?}"),
        TypeInfo::Opaque { label } => label.clone(),
        TypeInfo::Unknown => "unknown".to_string(),
    }
}

/// Generate a dry-run plan showing what a scan would do without exploring.
///
/// Builds the call graph, computes test order and layers, determines mocking
/// decisions, and formats a human-readable plan. Requires only the static
/// analysis results — no frontends need to be running.
pub fn format_dry_run_plan(
    analyses: &[FunctionAnalysis],
    skipped: &[SkippedFunction],
    config: &ScanConfig,
) -> Result<String, ScanError> {
    let call_graph = CallGraph::from_analyses(analyses);
    let order_entries = call_graph.test_order()?;
    let all_layers = build_layers(&order_entries, &call_graph);
    let total_layer_count = all_layers.len();

    // Apply stratum filter if specified.
    let selected_layers: Vec<(usize, &Vec<String>)> = if let Some(ref spec) = config.stratum {
        let max_layer = if all_layers.is_empty() { 0 } else { all_layers.len() - 1 };
        let range = crate::stratum::resolve_range(spec, max_layer)?;
        crate::stratum::filter_layers(&all_layers, &range)
    } else {
        all_layers.iter().enumerate().collect()
    };

    // Collect unique source files.
    let file_count = config
        .file_map
        .values()
        .collect::<HashSet<_>>()
        .len();

    let selected_function_count: usize = selected_layers.iter().map(|(_, l)| l.len()).sum();
    let total_functions = analyses.len();

    let mut out = String::new();

    out.push_str("Dry-run scan plan\n");
    out.push_str("=================\n\n");

    if config.stratum.is_some() {
        out.push_str(&format!(
            "Summary: {} of {} function(s) across {} file(s), {} of {} layer(s) selected\n",
            selected_function_count, total_functions, file_count,
            selected_layers.len(), total_layer_count,
        ));
    } else {
        out.push_str(&format!(
            "Summary: {} function(s) across {} file(s), {} layer(s)\n",
            total_functions, file_count, total_layer_count,
        ));
    }
    out.push_str(&format!(
        "Workers: {} {}\n",
        config.parallelism,
        if config.parallelism == 1 { "" } else { "(parallel)" },
    ));

    // Estimate time: each layer runs sequentially, functions within a layer run in parallel.
    // Worst case per layer = ceil(functions / workers) * timeout_per_fn.
    let timeout_secs = config.timeout_per_fn.as_secs();
    let mut total_estimate_secs: u64 = 0;
    for (_, layer) in &selected_layers {
        let batches = (layer.len() as u64 + config.parallelism as u64 - 1)
            / config.parallelism.max(1) as u64;
        total_estimate_secs += batches * timeout_secs;
    }
    let selected_layer_count = selected_layers.len();
    out.push_str(&format!(
        "Estimated time: <={total_estimate_secs}s ({selected_layer_count} layer(s) x {timeout_secs}s timeout)\n",
    ));

    // Build analysis lookup.
    let analysis_map: HashMap<&str, &FunctionAnalysis> =
        analyses.iter().map(|a| (a.name.as_str(), a)).collect();

    // All function names in the scan set.
    let scan_set: HashSet<&str> = analyses.iter().map(|a| a.name.as_str()).collect();

    // Functions in selected layers (for cross-stratum mock labelling).
    let selected_set: HashSet<&str> = selected_layers
        .iter()
        .flat_map(|(_, layer)| layer.iter().map(|s| s.as_str()))
        .collect();

    for &(layer_idx, layer) in &selected_layers {
        let parallelizable = if layer.len() > 1 { ", parallelizable" } else { "" };
        out.push_str(&format!(
            "\nLayer {} ({} function(s){}):\n",
            layer_idx,
            layer.len(),
            parallelizable,
        ));

        for func_name in layer {
            let analysis = match analysis_map.get(func_name.as_str()) {
                Some(a) => *a,
                None => continue,
            };

            // Format function signature.
            let params_str: Vec<String> = analysis
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, format_type(&p.typ)))
                .collect();
            let ret_str = format_type(&analysis.return_type);
            out.push_str(&format!(
                "  {}({}) -> {}\n",
                func_name,
                params_str.join(", "),
                ret_str,
            ));

            // Branch count.
            let branch_count = analysis.branches.len();

            // Internal dependencies (other functions in the scan set).
            let callees = call_graph.callees(func_name);
            let internal_deps: Vec<&str> = callees
                .iter()
                .filter(|c| scan_set.contains(c.as_str()))
                .map(|c| c.as_str())
                .collect();

            let deps_str = if internal_deps.is_empty() {
                "none".to_string()
            } else {
                internal_deps
                    .iter()
                    .map(|d| {
                        if selected_set.contains(d) {
                            format!("{d} (behavior-mock)")
                        } else {
                            format!("{d} (outside stratum — auto-mock)")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "    Branches: {} | Deps: {}\n",
                branch_count, deps_str,
            ));

            // External dependencies with auto-mock classification.
            let external_deps: Vec<_> = analysis
                .dependencies
                .iter()
                .filter(|d| !scan_set.contains(d.symbol.as_str()))
                .collect();

            if !external_deps.is_empty() {
                let ext_strs: Vec<String> = external_deps
                    .iter()
                    .map(|dep| {
                        let category = auto_mock::classify_dependency(dep);
                        let label = match category {
                            auto_mock::IoCategory::FileSystem => "filesystem — auto-mock",
                            auto_mock::IoCategory::Network => "network — auto-mock",
                            auto_mock::IoCategory::Database => "database — auto-mock",
                            auto_mock::IoCategory::PureUtility => "pure utility — passthrough",
                            auto_mock::IoCategory::ExternalOther => "external — auto-mock",
                        };
                        format!("{} ({})", dep.symbol, label)
                    })
                    .collect();
                out.push_str(&format!("    External: {}\n", ext_strs.join(", ")));
            }
        }
    }

    if !skipped.is_empty() {
        out.push_str("\nSkipped (unexecutable):\n");
        for skip in skipped {
            out.push_str(&format!("  {}: {}\n", skip.function_name, skip.reason));
        }
    }

    Ok(out)
}

/// Load per-function candidate inputs from `.shatter/config.yaml` if `config_dir` is set.
/// Returns an empty vec on missing config or resolution errors (logged as warnings).
fn load_config_candidate_inputs(
    func_name: &str,
    config_dir: &Option<PathBuf>,
    max_iterations: u32,
    timeout_secs: u64,
) -> Vec<Vec<serde_json::Value>> {
    let Some(dir) = config_dir else {
        return vec![];
    };
    match crate::config::resolve_function_config_with_inputs(
        func_name,
        dir,
        None,
        max_iterations,
        timeout_secs,
    ) {
        Ok(resolved) if !resolved.candidate_inputs.is_empty() => {
            log::debug!(
                "Scan: {} candidate input(s) from config for {}",
                resolved.candidate_inputs.len(),
                func_name,
            );
            resolved
                .candidate_inputs
                .iter()
                .map(|input| input.args.clone())
                .collect()
        }
        Ok(_) => vec![],
        Err(e) => {
            log::warn!(
                "Failed to resolve config candidate inputs for {}: {}",
                func_name,
                e,
            );
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        DependencyKind, ExecuteResult, ExternalDependency, PerformanceMetrics,
    };
    use crate::types::{ParamInfo, TypeInfo};

    /// Request timeout for integration tests using the noop frontend.
    const TEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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
            literals: vec![],
            crypto_boundaries: vec![],
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
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
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
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
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
    fn format_scan_report_shows_test_order() {
        let result = ScanResult {
            test_order: vec!["leaf".into(), "caller".into()],
            function_results: vec![
                FunctionResult {
                    function_name: "leaf".into(),
                    exploration: ObservationOutput {
                        function_name: "leaf".into(),
                        iterations: 5,
                        unique_paths: 2,
                        lines_covered: 3,
                        total_lines: 5,
                        new_path_executions: vec![],
                        raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                    },
                    behavior_map: BehaviorMap {
                        function_id: "leaf".into(),
                        behaviors: vec![],
                        fingerprint: None,
                        nondeterministic_fields: vec![],
                    },
                    behavior_coverage: vec![],
                    mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
                },
                FunctionResult {
                    function_name: "caller".into(),
                    exploration: ObservationOutput {
                        function_name: "caller".into(),
                        iterations: 10,
                        unique_paths: 3,
                        lines_covered: 8,
                        total_lines: 10,
                        new_path_executions: vec![],
                        raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                    },
                    behavior_map: BehaviorMap {
                        function_id: "caller".into(),
                        behaviors: vec![],
                        fingerprint: None,
                        nondeterministic_fields: vec![],
                    },
                    behavior_coverage: vec![BehaviorCoverage {
                        caller: "caller".into(),
                        callee: "leaf".into(),
                        exercised_behavior_ids: vec![0, 1],
                        total_behaviors: 3,
                    }],
                    mocks_used: vec![MockUsage { name: "leaf".into(), source: MockSource::CachedBehaviorMap }],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
                },
            ],
            skipped_functions: vec![],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(report.contains("2 function(s) tested"));
        assert!(!report.contains("Test order"));
        assert!(report.contains("Mocks used: 1 via behavior map (leaf)"));
        assert!(report.contains("Behavior coverage of leaf: 2/3"));
    }

    #[test]
    fn format_scan_report_single_function_no_deps() {
        let result = ScanResult {
            test_order: vec!["standalone".into()],
            function_results: vec![FunctionResult {
                function_name: "standalone".into(),
                exploration: ObservationOutput {
                    function_name: "standalone".into(),
                    iterations: 10,
                    unique_paths: 1,
                    lines_covered: 5,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "standalone".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(!report.contains("Mocks used"));
        assert!(!report.contains("Behavior coverage"));
    }

    #[test]
    fn format_scan_report_includes_skipped_functions() {
        let result = ScanResult {
            test_order: vec!["good_func".into()],
            function_results: vec![FunctionResult {
                function_name: "good_func".into(),
                exploration: ObservationOutput {
                    function_name: "good_func".into(),
                    iterations: 5,
                    unique_paths: 1,
                    lines_covered: 3,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "good_func".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![
                SkippedFunction {
                    function_name: "handleRequest".into(),
                    reason: "param \"socket\" has opaque type net.Socket".into(),
                    category: SkipCategory::Expected,
                },
                SkippedFunction {
                    function_name: "processStream".into(),
                    reason: "param \"input\" has opaque type stream.Readable".into(),
                    category: SkipCategory::Expected,
                },
            ],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(report.contains("Skipped (expected, 2):"));
        assert!(report.contains("handleRequest: param \"socket\" has opaque type net.Socket"));
        assert!(report.contains("processStream: param \"input\" has opaque type stream.Readable"));
        assert!(!report.contains("Errors ("));
    }

    #[test]
    fn format_scan_report_mixed_expected_and_errors() {
        let result = ScanResult {
            test_order: vec!["good_func".into()],
            function_results: vec![FunctionResult {
                function_name: "good_func".into(),
                exploration: ObservationOutput {
                    function_name: "good_func".into(),
                    iterations: 5,
                    unique_paths: 1,
                    lines_covered: 3,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "good_func".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                coverage_metrics: Default::default(),
                refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![
                SkippedFunction {
                    function_name: "handleRequest".into(),
                    reason: "param \"socket\" has opaque type net.Socket".into(),
                    category: SkipCategory::Expected,
                },
                SkippedFunction {
                    function_name: "authenticate".into(),
                    reason: "error: unexpected response from frontend".into(),
                    category: SkipCategory::Error,
                },
            ],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(report.contains("Skipped (expected, 1):"), "missing expected section: {report}");
        assert!(report.contains("handleRequest: param \"socket\" has opaque type net.Socket"));
        assert!(report.contains("Errors (1):"), "missing errors section: {report}");
        assert!(report.contains("authenticate: error: unexpected response from frontend"));
    }

    #[test]
    fn format_scan_report_no_skipped_functions_omits_section() {
        let result = ScanResult {
            test_order: vec!["func".into()],
            function_results: vec![FunctionResult {
                function_name: "func".into(),
                exploration: ObservationOutput {
                    function_name: "func".into(),
                    iterations: 1,
                    unique_paths: 1,
                    lines_covered: 1,
                    total_lines: 1,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "func".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(!report.contains("Skipped (expected"));
        assert!(!report.contains("Errors ("));
    }

    #[test]
    fn format_scan_report_includes_sampling_context() {
        let result = ScanResult {
            test_order: vec!["func".into()],
            function_results: vec![FunctionResult {
                function_name: "func".into(),
                exploration: ObservationOutput {
                    function_name: "func".into(),
                    iterations: 1,
                    unique_paths: 1,
                    lines_covered: 1,
                    total_lines: 1,
                    new_path_executions: vec![],
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "func".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![],
            sampling: Some(SamplingContext {
                total_functions: 100,
                sampled_functions: 10,
                closure_functions: 3,
                strata_summary: vec![],
            }),
        };
        let report = format_scan_report(&result);
        assert!(
            report.contains("Explored 13/100 functions"),
            "report should show sampling context: {report}"
        );
        assert!(report.contains("10% core sample"));
    }

    #[test]
    fn format_scan_report_no_sampling_context_omits_header() {
        let result = ScanResult {
            test_order: vec!["func".into()],
            function_results: vec![FunctionResult {
                function_name: "func".into(),
                exploration: ObservationOutput {
                    function_name: "func".into(),
                    iterations: 1,
                    unique_paths: 1,
                    lines_covered: 1,
                    total_lines: 1,
                    new_path_executions: vec![],
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "func".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![],
            sampling: None,
        };
        let report = format_scan_report(&result);
        assert!(!report.contains("Explored"), "no sampling context should omit Explored header");
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
                exploration: ObservationOutput {
                    function_name: "f1".into(),
                    iterations: 5,
                    unique_paths: 1,
                    lines_covered: 3,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "f1".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped: vec![SkippedFunction {
                function_name: "f2".into(),
                reason: "timed out after 30s".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 4,
            sampling: None,
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(report.contains("0 skipped"));
        assert!(report.contains("1 error(s)"));
        assert!(report.contains("4 worker(s)"));
        assert!(!report.contains("Test order"));
        assert!(report.contains("Errors (1):"));
        assert!(report.contains("f2: timed out after 30s"));
        assert!(!report.contains("Skipped (expected"));
    }

    #[test]
    fn format_parallel_scan_report_no_skipped() {
        let result = ParallelScanResult {
            test_order: vec!["f1".into()],
            function_results: vec![FunctionResult {
                function_name: "f1".into(),
                exploration: ObservationOutput {
                    function_name: "f1".into(),
                    iterations: 10,
                    unique_paths: 2,
                    lines_covered: 5,
                    total_lines: 5,
                    new_path_executions: vec![],
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
                },
                behavior_map: BehaviorMap {
                    function_id: "f1".into(),
                    behaviors: vec![],
                    fingerprint: None,
                    nondeterministic_fields: vec![],
                },
                behavior_coverage: vec![],
                mocks_used: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
            }],
            skipped: vec![],
            workers_used: 1,
            sampling: None,
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("0 skipped"));
        assert!(report.contains("0 error(s)"));
        assert!(!report.contains("Skipped (expected"));
        assert!(!report.contains("Errors ("));
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
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

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
                literals: vec![],
                crypto_boundaries: vec![],
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
                literals: vec![],
                crypto_boundaries: vec![],
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
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // leaf and caller are in separate layers (dependency order), so each
        // layer has 1 task → pool size = min(parallelism=2, tasks=1) = 1.
        assert_eq!(result.workers_used, 1);
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
        assert!(caller_result.mocks_used.iter().any(|m| m.name == "leaf"));
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
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

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
            literals: vec![],
            crypto_boundaries: vec![],
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(99),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
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
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

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
            literals: vec![],
            crypto_boundaries: vec![],
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
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: Some(cache.clone()),
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
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

    #[tokio::test]
    async fn parallel_scan_timeout_total_zero_skips_all() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let analyses = vec![
            FunctionAnalysis {
                name: "fn_a".to_string(),
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
                literals: vec![],
                crypto_boundaries: vec![],
            },
            FunctionAnalysis {
                name: "fn_b".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "y".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_a".to_string(), "test.ts".to_string());
        file_map.insert("fn_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: Some(Duration::ZERO),
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // All functions should be skipped due to immediate timeout.
        assert!(
            result.function_results.is_empty(),
            "no functions should be explored when timeout_total is zero"
        );
        assert_eq!(result.skipped.len(), 2);
        for s in &result.skipped {
            assert_eq!(s.reason, "total scan timeout exceeded");
        }
    }

    /// Regression test: after a per-function timeout, the tainted frontend
    /// (which still has a stale response buffered in stdout) must be discarded
    /// and replaced.  Without the respawn fix, the second function would fail
    /// with an ID mismatch instead of a clean timeout.
    #[tokio::test]
    async fn parallel_scan_respawns_frontend_after_timeout() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let slow_path = manifest_dir.join("../protocol/slow-execute-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![slow_path.to_string_lossy().into_owned()];
        // Request timeout must be long enough for handshake/instrument
        // but the per-function timeout triggers during execute.
        fe_config.request_timeout = Duration::from_secs(10);

        // Two independent functions in the same layer — both will timeout.
        let analyses = vec![
            FunctionAnalysis {
                name: "slow_a".to_string(),
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
                literals: vec![],
                crypto_boundaries: vec![],
            },
            FunctionAnalysis {
                name: "slow_b".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "y".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("slow_a".to_string(), "test.ts".to_string());
        file_map.insert("slow_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            seed: Some(42),
            file_map,
            // Single worker: same pool slot is reused, exposing stale-response bug.
            parallelism: 1,
            // Short per-function timeout triggers during the slow execute.
            timeout_per_fn: Duration::from_secs(3),
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed even when all functions timeout");

        // Both functions should be skipped with a timeout reason, NOT an error.
        // Before the fix, the second function would fail with
        // "response id N does not match request id N+1".
        assert!(
            result.function_results.is_empty(),
            "no functions should succeed when execute always times out"
        );
        assert_eq!(
            result.skipped.len(),
            2,
            "both functions should be skipped, got: {:?}",
            result.skipped
        );
        for s in &result.skipped {
            assert!(
                s.reason.contains("timed out"),
                "skip reason should be timeout, not ID mismatch; got: {}",
                s.reason
            );
        }
    }

    // ── dry-run plan tests ──────────────────────────────────────────

    fn make_analysis_with_params(
        name: &str,
        params: Vec<ParamInfo>,
        return_type: TypeInfo,
        deps: Vec<ExternalDependency>,
    ) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params,
            branches: vec![],
            dependencies: deps,
            return_type,
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
        }
    }

    #[test]
    fn dry_run_plan_shows_layers_and_deps() {
        // leaf has no deps, caller depends on leaf
        let analyses = vec![
            make_analysis("leaf", vec![]),
            make_analysis("caller", vec!["leaf"]),
        ];

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "src/math.ts".to_string());
        file_map.insert("caller".to_string(), "src/app.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 100,
            seed: None,
            file_map,
            parallelism: 2,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let plan = format_dry_run_plan(&analyses, &[], &config).expect("should succeed");

        assert!(plan.contains("Dry-run scan plan"));
        assert!(plan.contains("2 function(s) across 2 file(s), 2 layer(s)"));
        assert!(plan.contains("Workers: 2"));
        assert!(plan.contains("Layer 0"));
        assert!(plan.contains("Layer 1"));
        assert!(plan.contains("leaf"));
        assert!(plan.contains("leaf (behavior-mock)"));
    }

    #[test]
    fn dry_run_plan_shows_external_deps() {
        let analyses = vec![make_analysis_with_params(
            "fetchData",
            vec![ParamInfo {
                name: "url".into(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            TypeInfo::Unknown,
            vec![ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "axios.get".into(),
                source_module: "axios".into(),
                return_type: TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![],
            }],
        )];

        let mut file_map = HashMap::new();
        file_map.insert("fetchData".to_string(), "src/api.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 100,
            seed: None,
            file_map,
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let plan = format_dry_run_plan(&analyses, &[], &config).expect("should succeed");

        assert!(plan.contains("fetchData(url: string) -> unknown"));
        assert!(plan.contains("axios.get (network"));
    }

    #[test]
    fn dry_run_plan_shows_skipped_functions() {
        let analyses = vec![make_analysis("good", vec![])];

        let skipped = vec![SkippedFunction {
            function_name: "broken".into(),
            reason: "param \"sock\" has opaque type net.Socket".into(),
            category: SkipCategory::Expected,
        }];

        let config = ScanConfig {
            max_iterations_per_function: 100,
            seed: None,
            file_map: [("good".to_string(), "src/lib.ts".to_string())]
                .into_iter()
                .collect(),
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let plan = format_dry_run_plan(&analyses, &skipped, &config).expect("should succeed");

        assert!(plan.contains("Skipped (unexecutable)"));
        assert!(plan.contains("broken: param \"sock\" has opaque type net.Socket"));
    }

    #[test]
    fn dry_run_plan_empty_analyses() {
        let config = ScanConfig {
            max_iterations_per_function: 100,
            seed: None,
            file_map: HashMap::new(),
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let plan = format_dry_run_plan(&[], &[], &config).expect("should succeed");

        assert!(plan.contains("0 function(s)"));
        assert!(plan.contains("0 layer(s)"));
        assert!(!plan.contains("Layer 0"));
    }

    // ── stratum + core-sample composability tests ──────────────────

    /// Verify that when stratum filter is applied before core-sample,
    /// the budget operates on the stratum-filtered set, not the full population.
    ///
    /// This is the reproduction test for str-bwv: previously core-sample was
    /// applied first on all functions, then stratum filtered the result,
    /// causing the budget to be computed against the wrong population.
    #[test]
    fn stratum_then_core_sample_budget_on_filtered_set() {
        use crate::core_sample::{self, CoreSampleConfig, SampleBudget};
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::batch_analyze::FunctionEntry;
        use crate::types::TypeInfo;
        use std::path::PathBuf;

        // Create 30 functions: 10 leaves (layer 0), 10 mid (layer 1), 10 top (layer 2).
        // layer 0: leaf_0..leaf_9 (no deps)
        // layer 1: mid_0..mid_9 (each calls a leaf)
        // layer 2: top_0..top_9 (each calls a mid)
        let mut entries = Vec::new();
        for i in 0..10 {
            entries.push(FunctionEntry {
                file_path: PathBuf::from(format!("/src/leaf{i}.ts")),
                name: format!("leaf_{i}"),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![],
                branch_count: 2,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            });
        }
        for i in 0..10 {
            entries.push(FunctionEntry {
                file_path: PathBuf::from(format!("/src/mid{i}.ts")),
                name: format!("mid_{i}"),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: format!("leaf_{i}"),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 3,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            });
        }
        for i in 0..10 {
            entries.push(FunctionEntry {
                file_path: PathBuf::from(format!("/src/top{i}.ts")),
                name: format!("top_{i}"),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: format!("mid_{i}"),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 5,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            });
        }

        let registry = {
                let mut index = std::collections::HashMap::new();
                for (i, e) in entries.iter().enumerate() {
                    index.insert(e.name.clone(), i);
                }
                crate::batch_analyze::FunctionRegistry::from_raw(entries.clone(), index)
            };
        let cg = CgCallGraph::from_registry(&registry);

        // Step 1: Apply stratum filter for layer 0 only (the 10 leaf functions).
        let layers = cg.topological_layers();
        let max_layer = layers.len() - 1;
        let stratum_spec = crate::stratum::parse_stratum_spec("0").unwrap();
        let range = crate::stratum::resolve_range(&stratum_spec, max_layer).unwrap();
        let stratum_names: std::collections::HashSet<String> =
            crate::stratum::filter_layers(&layers, &range)
                .into_iter()
                .flat_map(|(_, funcs)| funcs.iter().cloned())
                .map(|qn| qn.rsplit_once("::").map_or(qn.clone(), |(_, n)| n.to_string()))
                .collect();

        // Should have exactly 10 leaf functions.
        assert_eq!(stratum_names.len(), 10, "stratum should select 10 leaf functions");

        // Step 2: Filter entries to stratum set.
        let filtered_entries: Vec<FunctionEntry> = entries
            .iter()
            .filter(|e| stratum_names.contains(&e.name))
            .cloned()
            .collect();
        assert_eq!(filtered_entries.len(), 10);

        // Step 3: Apply core sample at 50% on the FILTERED set.
        let filtered_registry = {
                let mut index = std::collections::HashMap::new();
                for (i, e) in filtered_entries.iter().enumerate() {
                    index.insert(e.name.clone(), i);
                }
                crate::batch_analyze::FunctionRegistry::from_raw(filtered_entries.clone(), index)
            };
        let filtered_cg = CgCallGraph::from_registry(&filtered_registry);
        let cs_config = CoreSampleConfig {
            budget: SampleBudget::Percentage(50.0),
            seed: 42,
            scan_root: "/".to_string(),
        };
        let result = core_sample::select_core_sample(&filtered_entries, &filtered_cg, &cs_config);

        // 50% of 10 = 5 functions. With stratum-first ordering, we get ~5.
        assert!(
            result.selected.len() <= 7 && result.selected.len() >= 3,
            "core sample of 50% of 10 stratum-filtered functions should select ~5, got {}",
            result.selected.len(),
        );

        // BUG REPRODUCTION: If core-sample ran on ALL 30 first, it would select
        // ~15, then stratum would filter to only those in layer 0 — potentially
        // far fewer or a mismatch. Verify that all selected are leaf functions.
        for name in &result.selected {
            let bare = name.rsplit_once("::").map_or(name.as_str(), |(_, n)| n);
            assert!(
                bare.starts_with("leaf_"),
                "selected function should be a leaf (stratum 0), got: {name}"
            );
        }
    }

    /// Verify core-sample on full population (without stratum) still works.
    #[test]
    fn core_sample_without_stratum_uses_full_population() {
        use crate::core_sample::{self, CoreSampleConfig, SampleBudget};
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::batch_analyze::FunctionEntry;
        use crate::types::TypeInfo;
        use std::path::PathBuf;

        let entries: Vec<FunctionEntry> = (0..20)
            .map(|i| FunctionEntry {
                file_path: PathBuf::from(format!("/src/f{i}.ts")),
                name: format!("fn_{i}"),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![],
                branch_count: i % 10,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            })
            .collect();

        let registry = {
                let mut index = std::collections::HashMap::new();
                for (i, e) in entries.iter().enumerate() {
                    index.insert(e.name.clone(), i);
                }
                crate::batch_analyze::FunctionRegistry::from_raw(entries.clone(), index)
            };
        let cg = CgCallGraph::from_registry(&registry);
        let cs_config = CoreSampleConfig {
            budget: SampleBudget::Percentage(50.0),
            seed: 42,
            scan_root: "/".to_string(),
        };
        let result = core_sample::select_core_sample(&entries, &cg, &cs_config);

        // 50% of 20 = 10.
        assert!(
            result.selected.len() >= 8 && result.selected.len() <= 12,
            "50% of 20 should select ~10, got {}",
            result.selected.len(),
        );
    }

    /// Verify stratum-only filtering works (no core-sample).
    #[test]
    fn stratum_only_filters_layers_correctly() {
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::batch_analyze::FunctionEntry;
        use crate::types::TypeInfo;
        use std::path::PathBuf;

        // 3-layer chain: c (leaf) -> b -> a
        let entries = vec![
            FunctionEntry {
                file_path: PathBuf::from("/src/c.ts"),
                name: "fn_c".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![],
                branch_count: 0,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
            FunctionEntry {
                file_path: PathBuf::from("/src/b.ts"),
                name: "fn_b".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: "fn_c".to_string(),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 3,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
            FunctionEntry {
                file_path: PathBuf::from("/src/a.ts"),
                name: "fn_a".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: "fn_b".to_string(),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 5,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
        ];

        let registry = {
                let mut index = std::collections::HashMap::new();
                for (i, e) in entries.iter().enumerate() {
                    index.insert(e.name.clone(), i);
                }
                crate::batch_analyze::FunctionRegistry::from_raw(entries, index)
            };
        let cg = CgCallGraph::from_registry(&registry);
        let layers = cg.topological_layers();

        // Stratum "0" should select only the leaf (fn_c).
        let stratum_spec = crate::stratum::parse_stratum_spec("0").unwrap();
        let max_layer = layers.len() - 1;
        let range = crate::stratum::resolve_range(&stratum_spec, max_layer).unwrap();
        let selected: std::collections::HashSet<String> =
            crate::stratum::filter_layers(&layers, &range)
                .into_iter()
                .flat_map(|(_, funcs)| funcs.iter().cloned())
                .map(|qn| qn.rsplit_once("::").map_or(qn.clone(), |(_, n)| n.to_string()))
                .collect();

        assert_eq!(selected.len(), 1);
        assert!(selected.contains("fn_c"));
    }

    /// Verify stratum-excluded mock source is correctly assigned when
    /// scanning a middle layer whose callees are outside the selected stratum.
    #[test]
    fn stratum_excluded_mock_source_attribution() {
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::batch_analyze::FunctionEntry;
        use crate::types::TypeInfo;
        use std::path::PathBuf;

        // 3-layer chain: fn_a (layer 2) → fn_b (layer 1) → fn_c (layer 0)
        let entries = vec![
            FunctionEntry {
                file_path: PathBuf::from("src/c.ts"),
                name: "fn_c".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![],
                branch_count: 1,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
            FunctionEntry {
                file_path: PathBuf::from("src/b.ts"),
                name: "fn_b".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: "fn_c".to_string(),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 3,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
            FunctionEntry {
                file_path: PathBuf::from("src/a.ts"),
                name: "fn_a".to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Int,
                dependencies: vec![crate::protocol::ExternalDependency {
                    symbol: "fn_b".to_string(),
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 5,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            },
        ];

        let registry = {
            let mut index = std::collections::HashMap::new();
            for (i, e) in entries.iter().enumerate() {
                index.insert(e.name.clone(), i);
            }
            crate::batch_analyze::FunctionRegistry::from_raw(entries, index)
        };
        let cg = CgCallGraph::from_registry(&registry);
        let layers = cg.topological_layers();
        assert!(layers.len() >= 3, "expected at least 3 layers");

        // Select stratum "1" — only fn_b (middle layer).
        let spec = crate::stratum::parse_stratum_spec("1").unwrap();
        let max_layer = layers.len() - 1;
        let range = crate::stratum::resolve_range(&spec, max_layer).unwrap();

        let selected: Vec<Vec<String>> = crate::stratum::filter_layers(&layers, &range)
            .into_iter()
            .map(|(_, funcs)| funcs.clone())
            .collect();
        let selected_set: std::collections::HashSet<String> =
            selected.iter().flatten().cloned().collect();
        let excluded: std::collections::HashSet<String> = layers
            .iter()
            .flatten()
            .filter(|f| !selected_set.contains(f.as_str()))
            .cloned()
            .collect();

        // fn_b should be selected; fn_a and fn_c excluded.
        let selected_bare: std::collections::HashSet<String> = selected_set
            .iter()
            .map(|qn| qn.rsplit_once("::").map_or(qn.clone(), |(_, n)| n.to_string()))
            .collect();
        assert!(selected_bare.contains("fn_b"), "fn_b should be in selected stratum");
        assert!(!selected_bare.contains("fn_a"), "fn_a should be excluded");
        assert!(!selected_bare.contains("fn_c"), "fn_c should be excluded");

        // fn_c is a callee of fn_b and excluded — should get StratumExcluded source.
        let excluded_bare: std::collections::HashSet<String> = excluded
            .iter()
            .map(|qn| qn.rsplit_once("::").map_or(qn.clone(), |(_, n)| n.to_string()))
            .collect();
        assert!(excluded_bare.contains("fn_c"), "fn_c should be in excluded set");
        assert!(excluded_bare.contains("fn_a"), "fn_a should be in excluded set");

        // Simulate mock source attribution: fn_c is a dependency of fn_b
        // and is in the excluded set → StratumExcluded.
        let mock_source = if excluded.iter().any(|e| e.contains("fn_c")) {
            MockSource::StratumExcluded
        } else {
            MockSource::TypeAwareStub
        };
        assert_eq!(mock_source, MockSource::StratumExcluded);
    }

    /// Verify format_mocks_used includes stratum-excluded mocks.
    #[test]
    fn format_mocks_used_includes_stratum_excluded() {
        let mocks = vec![
            MockUsage { name: "dep_a".into(), source: MockSource::CachedBehaviorMap },
            MockUsage { name: "dep_b".into(), source: MockSource::TypeAwareStub },
            MockUsage { name: "dep_c".into(), source: MockSource::StratumExcluded },
        ];
        let formatted = format_mocks_used(&mocks);
        assert!(formatted.contains("behavior map"), "should mention behavior map");
        assert!(formatted.contains("type-aware stub"), "should mention type-aware stub");
        assert!(formatted.contains("stratum-excluded"), "should mention stratum-excluded");
        assert!(formatted.contains("dep_a"));
        assert!(formatted.contains("dep_b"));
        assert!(formatted.contains("dep_c"));
    }

    // ── config candidate inputs ────────────────────────────────────

    #[test]
    fn load_config_candidate_inputs_returns_args_from_config() {
        let tmp = tempfile::tempdir().unwrap();
        let shatter_dir = tmp.path().join(".shatter");
        std::fs::create_dir_all(&shatter_dir).unwrap();

        // Write a candidate inputs JSON file.
        let inputs_json = serde_json::json!([
            { "args": [42, "hello"] },
            { "args": [0, ""] }
        ]);
        let inputs_path = shatter_dir.join("my_inputs.json");
        std::fs::write(&inputs_path, serde_json::to_string(&inputs_json).unwrap()).unwrap();

        // Write a config.yaml that references the inputs file for "myFunc".
        let config_yaml = "functions:\n  myFunc:\n    inputs: my_inputs.json\n";
        let config_path = shatter_dir.join("config.yaml");
        std::fs::write(&config_path, config_yaml).unwrap();

        let result = load_config_candidate_inputs(
            "myFunc",
            &Some(tmp.path().to_path_buf()),
            100,
            30,
        );

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![serde_json::json!(42), serde_json::json!("hello")]);
        assert_eq!(result[1], vec![serde_json::json!(0), serde_json::json!("")]);
    }

    #[test]
    fn load_config_candidate_inputs_returns_empty_without_config_dir() {
        let result = load_config_candidate_inputs("myFunc", &None, 100, 30);
        assert!(result.is_empty());
    }

    #[test]
    fn load_config_candidate_inputs_returns_empty_for_unmatched_function() {
        let tmp = tempfile::tempdir().unwrap();
        let shatter_dir = tmp.path().join(".shatter");
        std::fs::create_dir_all(&shatter_dir).unwrap();

        let inputs_json = serde_json::json!([{ "args": [1] }]);
        let inputs_path = shatter_dir.join("my_inputs.json");
        std::fs::write(&inputs_path, serde_json::to_string(&inputs_json).unwrap()).unwrap();

        // Config only has inputs for "otherFunc", not "myFunc".
        let config_yaml = "functions:\n  otherFunc:\n    inputs: my_inputs.json\n";
        std::fs::write(shatter_dir.join("config.yaml"), config_yaml).unwrap();

        let result = load_config_candidate_inputs(
            "myFunc",
            &Some(tmp.path().to_path_buf()),
            100,
            30,
        );

        assert!(result.is_empty());
    }

    // ── lazy worker spawn tests ──────────────────────────────────────

    /// Helper: build a BehaviorMap with a specific fingerprint for cache seeding.
    fn make_cached_map(function_id: &str, fingerprint: &str) -> crate::behavior::BehaviorMap {
        crate::behavior::BehaviorMap {
            function_id: function_id.into(),
            behaviors: vec![],
            fingerprint: Some(fingerprint.to_string()),
            nondeterministic_fields: vec![],
        }
    }

    /// Helper: compute the deep fingerprint that parallel_scan will compute for a
    /// leaf function whose source file exists on disk at `source_path`, with the
    /// given `analysis`. Used to pre-seed the cache so the scan gets a cache hit.
    fn compute_expected_deep_fp(
        source_path: &std::path::Path,
        analysis: &FunctionAnalysis,
    ) -> String {
        let source =
            crate::fingerprint::extract_function_source(source_path, analysis.start_line, analysis.end_line)
                .expect("extract source");
        let shallow = crate::fingerprint::compute_function_fingerprint(&source, analysis);
        crate::fingerprint::compute_deep_fingerprint(&shallow, &HashMap::new(), &std::collections::HashSet::new())
    }

    /// A warm-cache scan (all functions fingerprint-matching) must spawn zero workers.
    ///
    /// This is the primary perf acceptance criterion: if every function hits the
    /// cache, no frontend subprocess is ever started.
    #[tokio::test]
    async fn parallel_scan_cache_hit_spawns_no_workers() {
        use crate::cache::BehaviorMapCache;
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        // Create a real source file so fingerprinting returns Some(fp).
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let source_file = tmp_dir.path().join("warm.ts");
        std::fs::write(&source_file, "function warm_fn(x: number) { return x; }").unwrap();

        let analysis = FunctionAnalysis {
            name: "warm_fn".to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
        };

        // Compute the fingerprint parallel_scan will derive from the source file.
        let expected_fp = compute_expected_deep_fp(&source_file, &analysis);

        // Pre-seed the cache with a map whose fingerprint matches — this triggers
        // the is_fresh() path and skips the function without spawning a worker.
        let cache_dir = tmp_dir.path().join("cache");
        let cache = Arc::new(BehaviorMapCache::new(cache_dir).unwrap());
        cache.store(&make_cached_map("warm_fn", &expected_fp)).unwrap();

        // Use the noop frontend — it would succeed if spawned, but it must NOT be
        // spawned at all for a full-cache-hit scan.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let mut file_map = HashMap::new();
        file_map.insert("warm_fn".to_string(), source_file.to_string_lossy().into_owned());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: Some(cache),
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &[analysis], &config)
            .await
            .expect("parallel_scan should succeed");

        // Key assertion: no workers were ever spawned.
        assert_eq!(result.workers_used, 0, "warm cache should spawn zero workers");
        // Function should appear in skipped (cache hit), not results.
        assert!(result.function_results.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert!(
            result.skipped[0].reason.contains("unchanged"),
            "skip reason: {}",
            result.skipped[0].reason
        );
    }

    /// A scan with one cached and one stale function must explore only the stale
    /// one, spawning workers only for that layer.
    #[tokio::test]
    async fn parallel_scan_partial_cache_explores_stale_functions() {
        use crate::cache::BehaviorMapCache;
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        // warm_fn: will be a cache hit (fingerprint pre-seeded).
        let warm_source = tmp_dir.path().join("warm.ts");
        std::fs::write(&warm_source, "function warm_fn(x: number) { return x; }").unwrap();

        let warm_analysis = FunctionAnalysis {
            name: "warm_fn".to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
        };

        let warm_fp = compute_expected_deep_fp(&warm_source, &warm_analysis);

        let cache_dir = tmp_dir.path().join("cache");
        let cache = Arc::new(BehaviorMapCache::new(cache_dir).unwrap());
        cache.store(&make_cached_map("warm_fn", &warm_fp)).unwrap();
        // stale_fn has no cache entry → will be explored.

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        // Both functions are in the same layer (independent, no deps between them).
        let stale_analysis = FunctionAnalysis {
            name: "stale_fn".to_string(),
            exported: true,
            params: vec![ParamInfo { name: "y".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
        };

        let mut file_map = HashMap::new();
        file_map.insert("warm_fn".to_string(), warm_source.to_string_lossy().into_owned());
        // stale_fn points to a nonexistent file → fingerprint is None → cache check skipped → always explored.
        file_map.insert("stale_fn".to_string(), "nonexistent.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: Some(cache),
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let analyses = vec![warm_analysis, stale_analysis];
        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // stale_fn was explored; warm_fn was a cache hit.
        assert_eq!(result.function_results.len(), 1);
        assert_eq!(result.function_results[0].function_name, "stale_fn");
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].function_name, "warm_fn");
        // At least one worker was spawned for the stale function.
        assert!(result.workers_used >= 1, "stale function requires at least one worker");
        // Pool was capped: 1 stale task with parallelism=4 → pool size 1.
        assert_eq!(result.workers_used, 1, "pool should be capped at tasks.len()");
    }

    /// A layer with a single stale function and high parallelism must spawn
    /// only one worker (capped to tasks.len()).
    #[tokio::test]
    async fn parallel_scan_pool_capped_to_task_count() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let analyses = vec![FunctionAnalysis {
            name: "solo".to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            // High parallelism — but only 1 task exists, so pool must be capped at 1.
            parallelism: 8,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // Only 1 task → pool capped at 1, not 8.
        assert_eq!(result.workers_used, 1, "pool should be capped to tasks.len()=1, not parallelism=8");
        assert_eq!(result.function_results.len(), 1);
    }

    /// When a layer has more tasks than the initial pool size the pool must grow
    /// toward the parallelism ceiling as tasks complete.  This is the perf-evidence
    /// test for adaptive growth: `workers_used` must be > 1 (grew) and ≤ parallelism.
    #[tokio::test]
    async fn parallel_scan_pool_grows_with_demand() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        // Four independent functions (no dependencies) → one layer with 4 tasks.
        let make_fn = |name: &str| FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
        };
        let analyses = vec![
            make_fn("fn_a"),
            make_fn("fn_b"),
            make_fn("fn_c"),
            make_fn("fn_d"),
        ];

        let mut file_map = HashMap::new();
        for name in ["fn_a", "fn_b", "fn_c", "fn_d"] {
            file_map.insert(name.to_string(), "test.ts".to_string());
        }

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.function_results.len(), 4, "all 4 functions must complete");
        // With 4 tasks and max=4 the pool starts at initial_workers(4,4)=1 and must
        // grow as tasks complete.  After all 4 tasks run, current_size should be > 1.
        assert!(
            result.workers_used > 1,
            "pool should have grown beyond initial 1 worker (got {})",
            result.workers_used,
        );
        assert!(
            result.workers_used <= 4,
            "pool must not exceed parallelism ceiling (got {})",
            result.workers_used,
        );
    }

    // ── SchedulerPolicy integration tests ─────────────────────────────

    /// Serial policy must explore all functions and produce the same results
    /// as LayerParallel with a single worker.  This is the conservative
    /// baseline regression test.
    #[tokio::test]
    async fn serial_policy_scan_completes_all_functions() {
        use crate::frontend::FrontendConfig;
        use crate::scheduler_policy::SchedulerPolicy;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let analyses = vec![
            FunctionAnalysis {
                name: "alpha".to_string(),
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
                literals: vec![],
                crypto_boundaries: vec![],
            },
            FunctionAnalysis {
                name: "beta".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "y".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("alpha".to_string(), "test.ts".to_string());
        file_map.insert("beta".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 4, // Serial policy must ignore this and use 1 worker.
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: SchedulerPolicy::Serial,
            isolation: IsolationMode::None,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("serial policy scan should succeed");

        // All functions must be explored — serial policy doesn't skip anything.
        assert_eq!(result.function_results.len(), 2, "both functions should complete");
        assert!(result.skipped.is_empty(), "no functions should be skipped");

        // Serial enforces 1 effective worker regardless of configured parallelism.
        assert_eq!(result.workers_used, 1, "serial policy must use exactly 1 worker");
    }

    /// LayerParallel is the default policy.
    #[test]
    fn layer_parallel_policy_is_default() {
        use crate::scheduler_policy::SchedulerPolicy;
        assert_eq!(SchedulerPolicy::default(), SchedulerPolicy::LayerParallel);
    }

    // ── IsolationMode::Function tests ────────────────────────────────

    /// Two independent functions with Function isolation must both be explored.
    /// Each gets its own dedicated frontend; both complete successfully.
    #[tokio::test]
    async fn parallel_scan_function_isolation_explores_all_functions() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let analyses = vec![
            FunctionAnalysis {
                name: "fn_one".to_string(),
                exported: true,
                params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
            FunctionAnalysis {
                name: "fn_two".to_string(),
                exported: true,
                params: vec![ParamInfo { name: "y".into(), typ: TypeInfo::Int, type_name: None }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_one".to_string(), "test.ts".to_string());
        file_map.insert("fn_two".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::Function,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan with Function isolation should succeed");

        // Both independent functions must be explored.
        assert_eq!(result.function_results.len(), 2, "both functions should complete");
        assert!(result.skipped.is_empty(), "no functions should be skipped");
    }

    /// A dependency chain (caller → leaf) with Function isolation must explore
    /// both functions in dependency order, with no errors.
    #[tokio::test]
    async fn parallel_scan_function_isolation_respects_dependency_order() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let analyses = vec![
            FunctionAnalysis {
                name: "leaf_fn".to_string(),
                exported: true,
                params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
                branches: vec![],
                dependencies: vec![],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
            FunctionAnalysis {
                name: "caller_fn".to_string(),
                exported: true,
                params: vec![ParamInfo { name: "y".into(), typ: TypeInfo::Int, type_name: None }],
                branches: vec![],
                dependencies: vec![ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: "leaf_fn".to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                return_type: TypeInfo::Unknown,
                start_line: 1,
                end_line: 5,
                literals: vec![],
                crypto_boundaries: vec![],
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("leaf_fn".to_string(), "test.ts".to_string());
        file_map.insert("caller_fn".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            seed: Some(42),
            file_map,
            parallelism: 2,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::Function,
            capture_side_effects: false,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan with Function isolation should succeed");

        // Both functions must be explored; no errors.
        assert_eq!(result.function_results.len(), 2, "both functions should complete");
        assert!(result.skipped.is_empty(), "no functions should be skipped");
        // leaf_fn must be explored before caller_fn (dependency order).
        assert_eq!(result.test_order[0], "leaf_fn");
        assert_eq!(result.test_order[1], "caller_fn");
        // caller_fn should have used leaf_fn as a mock.
        let caller_result = result
            .function_results
            .iter()
            .find(|r| r.function_name == "caller_fn")
            .expect("caller_fn should be in results");
        assert!(caller_result.mocks_used.iter().any(|m| m.name == "leaf_fn"));
    }
}
