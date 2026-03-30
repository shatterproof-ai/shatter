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
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
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

/// Shared budget surplus within a topological layer.
///
/// Functions that terminate early (worklist exhausted, coverage plateau, full
/// branch coverage) donate their unused execution budget here. Functions still
/// discovering new paths can claim from the surplus when their initial budget
/// runs out.
///
/// Each layer gets a fresh `BudgetSurplus` — budget from layer N does not carry
/// over to layer N+1.
#[derive(Debug)]
pub struct BudgetSurplus {
    /// Remaining surplus executions available for claiming.
    available: AtomicU32,
}

impl Default for BudgetSurplus {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetSurplus {
    /// Create a new empty surplus (used at the start of each layer).
    pub fn new() -> Self {
        Self {
            available: AtomicU32::new(0),
        }
    }

    /// Donate unused budget to the shared surplus.
    pub fn donate(&self, amount: u32) {
        if amount > 0 {
            self.available.fetch_add(amount, Ordering::Release);
        }
    }

    /// Try to claim up to `requested` executions from the surplus.
    ///
    /// Returns the number actually claimed (may be less than requested if the
    /// surplus is partially depleted, or 0 if less than `min_claim` is
    /// available).
    pub fn try_claim(&self, requested: u32, min_claim: u32) -> u32 {
        let mut current = self.available.load(Ordering::Acquire);
        loop {
            if current < min_claim {
                return 0;
            }
            let to_claim = current.min(requested);
            match self.available.compare_exchange_weak(
                current,
                current - to_claim,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return to_claim,
                Err(updated) => current = updated,
            }
        }
    }

    /// Current surplus available (for diagnostics/testing).
    pub fn available(&self) -> u32 {
        self.available.load(Ordering::Acquire)
    }
}

/// Policy governing when a function may claim surplus budget.
#[derive(Debug, Clone)]
pub struct ClaimPolicy {
    /// Minimum hit rate (new paths / last N executions) to qualify for claiming.
    pub min_hit_rate: f64,
    /// Window size for measuring recent hit rate.
    pub window: u32,
    /// Maximum fraction of total surplus a single function can claim at once.
    pub max_claim_fraction: f64,
}

impl Default for ClaimPolicy {
    fn default() -> Self {
        Self {
            min_hit_rate: 0.1,
            window: 10,
            max_claim_fraction: 0.5,
        }
    }
}

impl ClaimPolicy {
    /// Determine whether a function should be allowed to claim surplus budget,
    /// based on its recent exploration productivity.
    ///
    /// `recent_new_paths` is the number of new paths discovered in the last
    /// `window` executions.
    pub fn should_claim(&self, recent_new_paths: u32) -> bool {
        if self.window == 0 {
            return false;
        }
        let hit_rate = recent_new_paths as f64 / self.window as f64;
        hit_rate >= self.min_hit_rate
    }

    /// Compute the maximum number of executions this function should claim,
    /// given the current surplus.
    pub fn max_claimable(&self, surplus_available: u32) -> u32 {
        (surplus_available as f64 * self.max_claim_fraction).floor() as u32
    }
}

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
    /// Number of workers to assign per function in `IsolationMode::None`.
    ///
    /// When > 1, each function in a layer is explored by this many parallel
    /// workers simultaneously, each with a different random seed derived from
    /// the base seed. Each worker receives `max_iterations / workers_per_fn`
    /// iterations so the total budget stays constant. Results from all workers
    /// are merged after the layer completes, with duplicate inputs deduplicated
    /// by input hash. Default: 1 (one worker per function, backward compatible).
    pub workers_per_fn: usize,
    /// Frontend capabilities from handshake, used to gate prepare commands.
    pub capabilities: crate::orchestrator::FrontendCapabilities,
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

/// A record of a caller invoking a mocked callee with arguments that fall
/// outside the callee's explored behavior map domain.
///
/// When a caller passes novel inputs to a mocked callee, the mock cannot
/// return an observed (real) value — it fabricates a response. The caller's
/// exploration then proceeds on a false assumption about what the callee
/// actually returns for those inputs.
///
/// A `MockMiss` surfaces this assumption so users know which callee behaviors
/// are assumed, not observed. It does **not** trigger re-exploration in this
/// phase — detection and reporting only.
#[derive(Debug, Clone, PartialEq)]
pub struct MockMiss {
    /// Symbol name of the callee whose behavior map was missed.
    pub callee_name: String,
    /// The arguments the caller passed that were not in the callee's domain.
    pub missed_inputs: Vec<serde_json::Value>,
    /// Input hash of the caller execution that triggered this miss.
    /// Identifies which caller execution was operating on false assumptions.
    pub caller_execution_id: u64,
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
    /// Mock misses detected during exploration.
    ///
    /// Each entry records a callee call whose arguments fell outside the
    /// callee's explored behavior map. The mock returned a fabricated value
    /// rather than an observed one; these results may be unsound.
    pub mock_misses: Vec<MockMiss>,
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

/// Detect mock misses in the raw execution results of a caller.
///
/// For each execution in `raw_results`, inspects every external call to a
/// callee whose behavior map is present in `callee_maps`. A miss is recorded
/// when the call arguments do not match any `input_args` entry in the callee's
/// behavior map — meaning the mock fabricated a response for an input it had
/// never actually observed.
///
/// Misses are deduplicated per `(callee_name, missed_inputs)` pair across the
/// whole set of executions; the `caller_execution_id` stored is the input hash
/// of the first caller execution that triggered each distinct miss.
///
/// # Arguments
/// * `caller_inputs_and_results` — Slice of `(caller_inputs, _mocks, execute_result)`
///   from [`ObservationOutput::raw_results`].
/// * `callee_maps` — Behavior maps for callees, keyed by symbol name.
pub fn detect_mock_misses(
    caller_inputs_and_results: &[(Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>, crate::protocol::ExecuteResult)],
    callee_maps: &HashMap<String, BehaviorMap>,
) -> Vec<MockMiss> {
    // Track seen (callee, inputs) pairs to avoid duplicates.
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut misses: Vec<MockMiss> = Vec::new();

    for (caller_inputs, _mocks, exec_result) in caller_inputs_and_results {
        // Compute the caller's execution id (input hash) once per execution.
        let mut hasher = DefaultHasher::new();
        let input_str = serde_json::to_string(caller_inputs).unwrap_or_default();
        input_str.hash(&mut hasher);
        let caller_execution_id = hasher.finish();

        for call in &exec_result.calls_to_external {
            let callee_map = match callee_maps.get(&call.symbol) {
                Some(m) => m,
                None => continue, // no behavior map for this callee — type-aware stub or external
            };

            // Check whether the call arguments match any known behavior's input_args.
            let is_in_domain = callee_map
                .behaviors
                .iter()
                .any(|b| b.input_args == call.args);

            if !is_in_domain {
                // Deduplicate by (callee, serialised args).
                let args_key = serde_json::to_string(&call.args).unwrap_or_default();
                let dedup_key = (call.symbol.clone(), args_key);
                if seen.insert(dedup_key) {
                    misses.push(MockMiss {
                        callee_name: call.symbol.clone(),
                        missed_inputs: call.args.clone(),
                        caller_execution_id,
                    });
                }
            }
        }
    }

    misses
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

        let pool_seeds = crate::input_gen::pool_to_candidate_inputs_for_callees(&analysis.params, &input_pool, &callees);

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
            capabilities: crate::orchestrator::FrontendCapabilities::from_raw(frontend.capabilities()),
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
            budget_surplus: None,
            claim_policy: ClaimPolicy::default(),
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

        // Detect mock misses: callee calls with args outside the callee's behavior map domain.
        let callee_maps_for_misses: HashMap<String, BehaviorMap> = callees
            .iter()
            .filter_map(|c| behavior_maps.get(c).map(|m| (c.clone(), m.clone())))
            .collect();
        let mock_misses = detect_mock_misses(&exploration.raw_results, &callee_maps_for_misses);
        if !mock_misses.is_empty() {
            log::debug!(
                "{func_name}: {} mock miss(es) detected across {} callee(s)",
                mock_misses.len(),
                mock_misses.iter().map(|m| &m.callee_name).collect::<HashSet<_>>().len(),
            );
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
            mock_misses,
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
    /// Peak number of worker subprocesses alive at any point (high-water mark).
    pub workers_used: usize,
    /// Workers shut down early because queued work dropped below pool size.
    pub workers_reaped: usize,
    /// Sampling context (populated when --core-sample is active).
    pub sampling: Option<SamplingContext>,
}

/// Minimum number of idle workers to keep warm in the pool.
/// Prevents reaping the last worker, ensuring fast checkout for the next task.
const MIN_IDLE_WORKERS: usize = 1;

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
    /// Count of workers currently alive (checked out or in the channel).
    /// Decremented when a worker is reaped; incremented on growth.
    live_count: Arc<AtomicUsize>,
    /// Peak value of `live_count` ever observed. Never decrements (not affected
    /// by reaping), so `workers_used` reflects the true high-water mark.
    peak_size: Arc<AtomicUsize>,
    /// Count of workers shut down early because queued work dropped below pool size.
    idle_reaped: Arc<AtomicUsize>,
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
    ///
    /// If `prewarmed` is `Some`, the pre-spawned worker is deposited into the pool
    /// and counted toward the initial batch, reducing the number of fresh spawns.
    async fn spawn_capped(
        config: Arc<FrontendConfig>,
        max_workers: usize,
        needed: usize,
        prewarmed: Option<Frontend>,
    ) -> Result<Self, FrontendError> {
        let initial = initial_workers(max_workers, needed);
        // Channel capacity == max_workers so growth workers can always be deposited.
        let (sender, receiver) = tokio::sync::mpsc::channel(max_workers);

        // Deposit prewarmed worker first, then spawn the remaining initial workers.
        let already = if let Some(fe) = prewarmed {
            sender.send(fe).await.expect("channel has capacity for prewarmed worker");
            1
        } else {
            0
        };
        for _ in already..initial {
            let frontend = Frontend::spawn(&config).await?;
            sender.send(frontend).await.expect("channel has capacity for initial workers");
        }
        Ok(Self {
            sender,
            receiver: Mutex::new(receiver),
            max_workers,
            live_count: Arc::new(AtomicUsize::new(initial)),
            peak_size: Arc::new(AtomicUsize::new(initial)),
            idle_reaped: Arc::new(AtomicUsize::new(0)),
            config,
        })
    }

    /// Check out a worker from the pool, blocking until one is available.
    async fn checkout(&self) -> Frontend {
        let mut rx = self.receiver.lock().await;
        rx.recv().await.expect("pool should not be empty")
    }

    /// Return a worker to the pool, or reap it if the pool exceeds the idle floor.
    ///
    /// The floor is `min(pending + MIN_IDLE_WORKERS, max_workers)`, keeping one
    /// extra idle worker warm for fast checkout on the next task.  A CAS prevents
    /// two concurrent returners from both deciding to reap the same slot.
    async fn return_or_reap_worker(&self, frontend: Frontend, pending: usize) {
        let floor = (pending + MIN_IDLE_WORKERS).min(self.max_workers);
        let current = self.live_count.load(Ordering::Acquire);
        if current > floor
            && self.live_count
                .compare_exchange(current, current - 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            self.idle_reaped.fetch_add(1, Ordering::Relaxed);
            // Shutdown in a detached task so the caller is not delayed.
            tokio::spawn(async move {
                let _ = frontend.shutdown().await;
            });
            return;
        }
        let _ = self.sender.send(frontend).await;
    }

    /// Absorb a dead worker's capacity slot without spawning a replacement.
    ///
    /// Called when a crashed or timed-out worker leaves the pool over-provisioned
    /// (more alive workers than pending tasks).  Decrements `live_count` so the
    /// slot is not counted toward future replacement or growth decisions.
    fn reap_dead_slot(&self) {
        self.live_count.fetch_sub(1, Ordering::Relaxed);
        self.idle_reaped.fetch_add(1, Ordering::Relaxed);
    }

    /// True iff the pool needs a replacement for a dead (timed-out or crashed) worker.
    ///
    /// Returns false when the pool is already over-provisioned relative to `pending`
    /// tasks — the dead slot should be absorbed via `reap_dead_slot()` instead.
    fn needs_replacement(&self, pending: usize) -> bool {
        self.live_count.load(Ordering::Relaxed) <= pending.max(1)
    }

    /// Grow the pool by one worker if demand justifies it and we are below the ceiling.
    ///
    /// `tasks_remaining` is the number of tasks that have not yet completed.  If that
    /// exceeds `live_count`, tasks are blocked on `checkout()` and a new worker will
    /// reduce their wait.  The CAS ensures at most one growth per available slot even
    /// when multiple tasks return concurrently.  The actual subprocess spawn runs in a
    /// detached task so the caller is not delayed.
    fn maybe_grow(&self, tasks_remaining: usize) {
        let current = self.live_count.load(Ordering::Relaxed);
        if tasks_remaining <= current || current >= self.max_workers {
            return;
        }
        if self.live_count
            .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            return; // another concurrent task already claimed the slot
        }
        // Track peak — reaping can decrease live_count, but peak_size never shrinks.
        self.peak_size.fetch_max(current + 1, Ordering::Relaxed);
        let sender = self.sender.clone();
        let config = Arc::clone(&self.config);
        let live_count = Arc::clone(&self.live_count);
        tokio::spawn(async move {
            match Frontend::spawn(&config).await {
                Ok(fe) => {
                    let _ = sender.send(fe).await;
                }
                Err(_) => {
                    // Release the claimed slot so future growth attempts can retry.
                    live_count.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });
    }

    /// Peak number of workers alive at any point during this pool's lifetime.
    /// Unlike `live_count`, this never decrements due to reaping.
    fn peak_size(&self) -> usize {
        self.peak_size.load(Ordering::Relaxed)
    }

    /// Number of workers reaped early due to excess capacity.
    fn idle_reaped(&self) -> usize {
        self.idle_reaped.load(Ordering::Relaxed)
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
    tasks: Vec<ExploreTask>,
    max_concurrent: usize,
    timeout: Duration,
    cache: &Option<Arc<BehaviorMapCache>>,
    behavior_maps: &Arc<Mutex<HashMap<String, BehaviorMap>>>,
    input_pool: &Arc<Mutex<InterestingPool>>,
) -> (Vec<FunctionOutcome>, usize) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for ExploreTask { func_name, analysis, explore_config, mocks_used, callees, deep_fp } in tasks {
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

/// Derive a deterministic seed for a worker replica.
///
/// When a base seed is set, XORs it with a mixing of the function index and
/// replica index so that each replica explores a different region of the input
/// space. When no base seed is set, returns `None` so each replica uses fresh
/// entropy independently.
fn derive_replica_seed(base: Option<u64>, fn_idx: usize, replica: usize) -> Option<u64> {
    base.map(|s| {
        s ^ ((fn_idx as u64).wrapping_mul(0x9e3779b97f4a7c15))
            ^ (replica as u64).wrapping_mul(0x6c62272e07bb0142)
    })
}

/// Internal task descriptor for a single-function exploration slot.
///
/// Carries all per-function data needed to dispatch one worker. When
/// `workers_per_fn > 1`, a function may appear in multiple `ExploreTask`s
/// with different seeds so that parallel workers explore different paths.
struct ExploreTask {
    func_name: String,
    analysis: FunctionAnalysis,
    explore_config: ExploreConfig,
    mocks_used: Vec<MockUsage>,
    callees: std::collections::HashSet<String>,
    deep_fp: Option<String>,
}

/// Merge outcomes for replicas of the same function into one outcome per function.
///
/// When `workers_per_fn > 1`, multiple `FunctionOutcome::Success` entries may
/// exist for the same function (one per worker). This function groups them,
/// re-analyzes the merged raw results, and produces one merged `FunctionResult`
/// per function. If any replica succeeded the function is considered successful;
/// if all replicas failed, the first failure is kept.
fn merge_replica_outcomes(
    outcomes: Vec<FunctionOutcome>,
    analysis_map: &HashMap<&str, &FunctionAnalysis>,
) -> Vec<FunctionOutcome> {
    // Partition by function name, preserving insertion order for determinism.
    let mut by_name: HashMap<String, Vec<Box<FunctionResult>>> = HashMap::new();
    let mut name_order: Vec<String> = Vec::new();
    let mut errors: HashMap<String, FunctionOutcome> = HashMap::new();

    for outcome in outcomes {
        match outcome {
            FunctionOutcome::Success(result) => {
                let name = result.function_name.clone();
                if !by_name.contains_key(&name) {
                    name_order.push(name.clone());
                }
                by_name.entry(name).or_default().push(result);
            }
            FunctionOutcome::Timeout { ref function_name, .. }
            | FunctionOutcome::Error { ref function_name, .. } => {
                // Keep the first failure; success from another replica overrides it.
                let name = function_name.clone();
                errors.entry(name.clone()).or_insert(outcome);
                if !name_order.contains(&name) {
                    name_order.push(name);
                }
            }
        }
    }

    name_order
        .into_iter()
        .map(|name| {
            if let Some(replicas) = by_name.remove(&name) {
                if replicas.len() == 1 {
                    FunctionOutcome::Success(replicas.into_iter().next().unwrap())
                } else {
                    // Merge replicas.
                    let analysis = match analysis_map.get(name.as_str()) {
                        Some(a) => a,
                        None => {
                            // No analysis available: just keep the first replica.
                            return FunctionOutcome::Success(replicas.into_iter().next().unwrap());
                        }
                    };
                    let unboxed: Vec<FunctionResult> = replicas.into_iter().map(|r| *r).collect();
                    FunctionOutcome::Success(Box::new(merge_replica_results(unboxed, analysis)))
                }
            } else {
                // All replicas failed; return the stored error.
                errors.remove(&name).unwrap_or(FunctionOutcome::Error {
                    function_name: name,
                    error: "all replicas failed".into(),
                })
            }
        })
        .collect()
}

/// Merge multiple per-replica `FunctionResult`s for the same function.
///
/// Concatenates `raw_results` from all replicas, then re-runs `pipeline::analyze`
/// on the merged data to produce a single correct `BehaviorMap` and
/// `CoverageMetrics`. Duplicate inputs are deduplicated by input hash inside
/// `BehaviorMap::from_records`. The fingerprint is taken from the first replica
/// (all replicas share the same source fingerprint).
fn merge_replica_results(replicas: Vec<FunctionResult>, analysis: &FunctionAnalysis) -> FunctionResult {
    use crate::explorer::ObservationOutput;

    debug_assert!(!replicas.is_empty(), "merge_replica_results: replicas must not be empty");

    let func_name = replicas[0].function_name.clone();
    let fingerprint = replicas[0].behavior_map.fingerprint.clone();
    let total_lines = replicas[0].exploration.total_lines;
    let mocks_used = replicas[0].mocks_used.clone();
    let behavior_coverage = replicas[0].behavior_coverage.clone();
    let refactoring_recommendations = replicas[0].refactoring_recommendations.clone();
    // Collect mock misses from all replicas, deduplicating by (callee, inputs).
    let mut merged_miss_seen: HashSet<(String, String)> = HashSet::new();
    let merged_mock_misses: Vec<MockMiss> = replicas
        .iter()
        .flat_map(|r| &r.mock_misses)
        .filter(|m| {
            let key = (
                m.callee_name.clone(),
                serde_json::to_string(&m.missed_inputs).unwrap_or_default(),
            );
            merged_miss_seen.insert(key)
        })
        .cloned()
        .collect();

    // Accumulate exploration data across replicas.
    let mut merged_raw = Vec::new();
    let mut merged_discoveries = Vec::new();
    let mut merged_nondeterministic: Vec<crate::nondeterminism::NondeterministicField> = Vec::new();
    let mut merged_float_probes = Vec::new();
    let mut merged_boundary = Vec::new();
    let mut merged_shrunk: std::collections::HashMap<u64, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut merged_new_path_execs = Vec::new();
    let mut total_iterations: u32 = 0;
    let mut max_lines_covered: usize = 0;
    let mut mcdc_summary: Option<(usize, usize, usize)> = None;

    for replica in replicas {
        let exp = replica.exploration;
        merged_raw.extend(exp.raw_results);
        merged_discoveries.extend(exp.discoveries);
        merged_new_path_execs.extend(exp.new_path_executions);
        for field in exp.nondeterministic_fields {
            if !merged_nondeterministic.contains(&field) {
                merged_nondeterministic.push(field);
            }
        }
        merged_float_probes.extend(exp.float_probe_results);
        merged_boundary.extend(exp.boundary_results);
        for (k, v) in exp.shrunk_witnesses {
            // First writer wins: keep whichever replica found it.
            merged_shrunk.entry(k).or_insert(v);
        }
        total_iterations += exp.iterations;
        max_lines_covered = max_lines_covered.max(exp.lines_covered);
        if mcdc_summary.is_none() {
            mcdc_summary = exp.mcdc_summary;
        }
    }

    // Build a merged ObservationOutput and re-analyze it. pipeline::analyze
    // handles input-hash deduplication inside BehaviorMap::from_records and
    // uses analysis.branches.len() for accurate CoverageMetrics.total_branches.
    let merged_stubbed = crate::explorer::collect_stubbed_modules(&merged_raw);
    let merged_exploration = ObservationOutput {
        function_name: func_name.clone(),
        iterations: total_iterations,
        // unique_paths and lines_covered will be re-stated conservatively;
        // the definitive coverage data comes from BehaviorMap + CoverageMetrics.
        unique_paths: merged_new_path_execs.len(),
        lines_covered: max_lines_covered,
        total_lines,
        new_path_executions: merged_new_path_execs,
        raw_results: merged_raw,
        discoveries: merged_discoveries,
        nondeterministic_fields: merged_nondeterministic,
        float_probe_results: merged_float_probes,
        boundary_results: merged_boundary,
        shrunk_witnesses: merged_shrunk,
        mcdc_summary,
        shrink_stats: crate::shrink::ShrinkStats::default(),
        abandoned_frontiers: vec![], opaque_suggestions: vec![],
        stubbed_modules: merged_stubbed,
    };

    let mut analyze_out = crate::pipeline::analyze(&merged_exploration, analysis);
    analyze_out.behavior_map.fingerprint = fingerprint;

    FunctionResult {
        function_name: func_name,
        exploration: merged_exploration,
        behavior_map: analyze_out.behavior_map,
        behavior_coverage,
        mocks_used,
        mock_misses: merged_mock_misses,
        coverage_metrics: analyze_out.coverage_metrics,
        refactoring_recommendations,
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
    // Persistent pool reused across layers; track peak count and total idle reaps.
    let mut peak_workers: usize = 0;
    let mut total_reaped: usize = 0;
    // Warm frontend pool that persists across topological layers, avoiding
    // repeated spawn+handshake overhead. Created lazily on the first layer
    // with real work; grown/reaped dynamically by WorkerPool internals.
    let mut persistent_pool: Option<Arc<WorkerPool>> = None;
    let fe_config_persistent = Arc::new(frontend_config.clone());

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

        // Speculatively pre-spawn one worker while we build the task list,
        // but only when the persistent pool doesn't exist yet (subsequent
        // layers already have warm workers in the pool).
        let prespawn_rx = if persistent_pool.is_none() {
            let (prespawn_tx, rx) = tokio::sync::oneshot::channel();
            let cfg = Arc::clone(&fe_config_persistent);
            tokio::spawn(async move {
                let _ = prespawn_tx.send(Frontend::spawn(&cfg).await);
            });
            Some(rx)
        } else {
            None
        };

        // Build tasks for this layer: each function paired with its mocks.
        let mut tasks: Vec<ExploreTask> = Vec::new();
        // Track deep FPs computed in this layer (added after the layer completes).
        let mut layer_deep_fps: Vec<(String, String)> = Vec::new();
        // Per-layer budget surplus: resets at each layer boundary.
        let layer_surplus = Arc::new(BudgetSurplus::new());

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
                crate::input_gen::pool_to_candidate_inputs_for_callees(&analysis.params, &pool_guard, &callees)
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
                capabilities: config.capabilities.clone(),
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
                budget_surplus: Some(Arc::clone(&layer_surplus)),
                claim_policy: ClaimPolicy::default(),
            };

            tasks.push(ExploreTask {
                func_name: func_name.clone(),
                analysis: analysis.clone(),
                explore_config,
                mocks_used,
                callees,
                deep_fp: current_deep_fp,
            });
        }

        // Collect the speculative pre-spawn (only in-flight when pool didn't
        // exist yet). If it succeeded, pass it to the new pool.
        let prewarmed = if let Some(rx) = prespawn_rx {
            match rx.await {
                Ok(Ok(fe)) => Some(fe),
                _ => None,
            }
        } else {
            None
        };

        // Execute tasks in parallel, using either the shared WorkerPool (default)
        // or per-function dedicated frontends (Function isolation mode).
        // The pool is created lazily on the first layer with work and persists
        // across subsequent layers, keeping frontend subprocesses warm.
        if !tasks.is_empty() {

            // Collect outcomes from either isolation path.
            let layer_outcomes: Vec<FunctionOutcome> =
                if config.isolation == IsolationMode::Function {
                    // Function mode doesn't use the shared pool — shut down
                    // the speculative pre-spawn if one was created.
                    if let Some(fe) = prewarmed {
                        tokio::spawn(async move { let _ = fe.shutdown().await; });
                    }
                    // Each function gets a dedicated fresh frontend.
                    // No shared pool — a Semaphore caps concurrency instead.
                    let (outcomes, layer_peak) = run_layer_function_mode(
                        Arc::clone(&fe_config_persistent),
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
                    // When workers_per_fn > 1, expand each function into multiple tasks
                    // with different seeds so parallel workers explore different paths.
                    // The iteration budget is split evenly across replicas to keep the
                    // total budget constant.
                    let expanded_tasks: Vec<ExploreTask> = if config.workers_per_fn <= 1 {
                        tasks
                    } else {
                        let wpf = config.workers_per_fn;
                        let mut out = Vec::with_capacity(tasks.len() * wpf);
                        for (fn_idx, task) in tasks.into_iter().enumerate() {
                            let per_replica_iters =
                                (task.explore_config.max_iterations / wpf as u32).max(1);
                            for replica in 0..wpf {
                                let mut replica_config = task.explore_config.clone();
                                replica_config.seed =
                                    derive_replica_seed(task.explore_config.seed, fn_idx, replica);
                                replica_config.max_iterations = per_replica_iters;
                                out.push(ExploreTask {
                                    func_name: task.func_name.clone(),
                                    analysis: task.analysis.clone(),
                                    explore_config: replica_config,
                                    mocks_used: task.mocks_used.clone(),
                                    callees: task.callees.clone(),
                                    deep_fp: task.deep_fp.clone(),
                                });
                            }
                        }
                        out
                    };

                    // Reuse the persistent pool if it exists; otherwise create it
                    // with the speculative pre-spawn from this first layer.
                    let pool = if let Some(ref existing) = persistent_pool {
                        Arc::clone(existing)
                    } else {
                        let new_pool = Arc::new(
                            WorkerPool::spawn_capped(
                                Arc::clone(&fe_config_persistent),
                                effective_parallelism,
                                expanded_tasks.len(),
                                prewarmed,
                            )
                            .await
                            .map_err(ScanError::Frontend)?,
                        );
                        persistent_pool = Some(Arc::clone(&new_pool));
                        new_pool
                    };

                    // Each task decrements this counter after returning its worker so that
                    // `maybe_grow` can detect tasks still blocked on `checkout()`.
                    let tasks_remaining = Arc::new(AtomicUsize::new(expanded_tasks.len()));

                    // Each task checks out a worker, explores, then returns the worker.
                    // Behavior map storage is deferred to after all handles join so that
                    // replicas for the same function can be merged first.
                    let mut handles = Vec::new();

                    for ExploreTask { func_name, analysis, explore_config, mocks_used, callees, deep_fp }
                        in expanded_tasks
                    {
                        let pool = Arc::clone(&pool);
                        let behavior_maps = Arc::clone(&behavior_maps);
                        let input_pool = Arc::clone(&input_pool);
                        let timeout = config.timeout_per_fn;
                        let fe_config = Arc::clone(&fe_config_persistent);
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

                            // Decrement the remaining-task counter FIRST so that
                            // return_or_reap_worker sees the updated pending count when
                            // deciding whether to reap this worker.
                            let remaining = tasks_remaining.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);

                            // After a timeout the frontend's stdout buffer contains a
                            // stale response that would cause an ID mismatch on the next
                            // request.  Kill and respawn instead of returning to pool.
                            // Skip replacement when the pool is already over-provisioned
                            // relative to remaining tasks — absorb the dead slot instead.
                            if timed_out || !frontend.is_alive() {
                                // Drop the poisoned/dead frontend (kills the child process).
                                drop(frontend);
                                if pool.needs_replacement(remaining) {
                                    match Frontend::spawn(&fe_config).await {
                                        Ok(new_fe) => pool.return_or_reap_worker(new_fe, remaining).await,
                                        Err(_) => { /* pool shrinks — acceptable degradation */ }
                                    }
                                } else {
                                    // Over-capacity: absorb the dead slot without spawning.
                                    pool.reap_dead_slot();
                                }
                            } else {
                                pool.return_or_reap_worker(frontend, remaining).await;
                            }

                            // Grow the pool if tasks are still blocked on checkout().
                            pool.maybe_grow(remaining);

                            match result {
                                Ok(Ok(func_result)) => FunctionOutcome::Success(Box::new(func_result)),
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

                    let mut raw_outcomes = Vec::with_capacity(handles.len());
                    for handle in handles {
                        match handle.await {
                            Ok(outcome) => raw_outcomes.push(outcome),
                            Err(e) => raw_outcomes.push(FunctionOutcome::Error {
                                function_name: "(unknown)".into(),
                                error: format!("task join error: {e}"),
                            }),
                        }
                    }

                    // Merge replicas for any function explored by multiple workers, then
                    // store the final behavior maps and cache entries for all successes.
                    let outcomes = if config.workers_per_fn > 1 {
                        merge_replica_outcomes(raw_outcomes, &analysis_map)
                    } else {
                        raw_outcomes
                    };

                    // Store behavior maps for downstream layers and disk cache.
                    // Doing this after the join (rather than inside each spawn) is safe
                    // because same-layer functions have no cross-dependencies.
                    {
                        let mut maps = behavior_maps.lock().await;
                        for outcome in &outcomes {
                            if let FunctionOutcome::Success(result) = outcome {
                                maps.insert(
                                    result.function_name.clone(),
                                    result.behavior_map.clone(),
                                );
                            }
                        }
                    }
                    if let Some(ref cache) = config.cache {
                        for outcome in &outcomes {
                            if let FunctionOutcome::Success(result) = outcome {
                                let _ = cache.store(&result.behavior_map);
                            }
                        }
                    }

                    // Pool persists across layers — no per-layer shutdown.
                    // Workers will be reaped or reused as the next layer demands.
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
        } else {
            // No tasks in this layer (all cache hits). Shut down the speculative
            // pre-spawn if one was created (only on the first layer before the
            // persistent pool exists).
            if let Some(fe) = prewarmed {
                tokio::spawn(async move {
                    let _ = fe.shutdown().await;
                });
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

    // Shut down the persistent worker pool now that all layers are done.
    if let Some(pool) = persistent_pool {
        peak_workers = peak_workers.max(pool.peak_size());
        total_reaped += pool.idle_reaped();
        if let Ok(p) = Arc::try_unwrap(pool) {
            p.shutdown().await;
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
        workers_reaped: total_reaped,
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

    // Donate unused budget to the layer surplus so other functions can use it.
    if let Some(ref surplus) = explore_config.budget_surplus {
        let allocated = explore_config.max_iterations;
        let used = exploration.iterations;
        let unused = allocated.saturating_sub(used);
        if unused > 0 {
            surplus.donate(unused);
            log::debug!(
                "{func_name}: donated {unused} unused iterations to surplus (used {used}/{allocated})"
            );
        }
    }

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

    // Detect mock misses: callee calls with args outside the callee's behavior map domain.
    let callee_maps_for_misses: HashMap<String, BehaviorMap> = callees
        .iter()
        .filter_map(|c| maps.get(c).map(|m| (c.clone(), m.clone())))
        .collect();
    drop(maps);
    let mock_misses = detect_mock_misses(&exploration.raw_results, &callee_maps_for_misses);
    if !mock_misses.is_empty() {
        log::debug!(
            "{func_name}: {} mock miss(es) detected across {} callee(s)",
            mock_misses.len(),
            mock_misses.iter().map(|m| &m.callee_name).collect::<HashSet<_>>().len(),
        );
    }

    let refactoring_recommendations =
        crate::mock_analysis::generate_recommendations(&analysis.dependencies);

    Ok(FunctionResult {
        function_name: func_name.to_string(),
        exploration,
        behavior_map: analyze_out.behavior_map,
        behavior_coverage,
        mocks_used: mocks_used.to_vec(),
        mock_misses,
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

/// Format mock misses as an indented human-readable string.
///
/// Groups misses by callee name and shows the count of distinct missed input
/// sets per callee. Truncates individual input args to a short representation
/// to keep report lines readable.
fn format_mock_misses(misses: &[MockMiss]) -> String {
    if misses.is_empty() {
        return String::new();
    }

    // Group by callee name for a compact summary.
    let mut by_callee: HashMap<&str, Vec<&MockMiss>> = HashMap::new();
    for miss in misses {
        by_callee.entry(miss.callee_name.as_str()).or_default().push(miss);
    }

    let mut callee_names: Vec<&str> = by_callee.keys().copied().collect();
    callee_names.sort();

    let mut parts = Vec::new();
    for callee in callee_names {
        let callee_misses = &by_callee[callee];
        let count = callee_misses.len();
        // Show at most 3 example input tuples, truncated to 60 chars each.
        let examples: Vec<String> = callee_misses
            .iter()
            .take(3)
            .map(|m| {
                let s = serde_json::to_string(&m.missed_inputs).unwrap_or_else(|_| "?".into());
                if s.len() > 60 {
                    let mut end = 60;
                    while !s.is_char_boundary(end) { end -= 1; }
                    format!("{}…", &s[..end])
                } else {
                    s
                }
            })
            .collect();
        let example_str = examples.join(", ");
        if count <= 3 {
            parts.push(format!("{callee}: {count} miss(es) — {example_str}"));
        } else {
            parts.push(format!(
                "{callee}: {count} miss(es) — {example_str} (+{} more)",
                count - 3
            ));
        }
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

        if !func_result.mock_misses.is_empty() {
            out.push_str(&format!(
                "  Mock misses (inputs outside callee's explored domain): {}\n",
                format_mock_misses(&func_result.mock_misses)
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

        if !func_result.mock_misses.is_empty() {
            out.push_str(&format!(
                "  Mock misses (inputs outside callee's explored domain): {}\n",
                format_mock_misses(&func_result.mock_misses)
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
        TypeInfo::Opaque { label, .. } => label.clone(),
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
        &[],
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
            loops: vec![],
            source_file: None,
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
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
                        raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                        raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
                    refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![
                SkippedFunction {
                    function_name: "handleRequest".into(),
                    reason: "param \"socket\" → net.Socket (network handle — requires live network binding)".into(),
                    category: SkipCategory::Expected,
                },
                SkippedFunction {
                    function_name: "processStream".into(),
                    reason: "param \"input\" → stream.Readable (I/O stream — wraps OS file descriptor or pipe)".into(),
                    category: SkipCategory::Expected,
                },
            ],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(report.contains("1 function(s) tested"));
        assert!(report.contains("Skipped (expected, 2):"));
        assert!(report.contains("handleRequest: param \"socket\" → net.Socket (network handle"));
        assert!(report.contains("processStream: param \"input\" → stream.Readable (I/O stream"));
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                mock_misses: vec![],
                refactoring_recommendations: vec![],
            }],
            skipped_functions: vec![
                SkippedFunction {
                    function_name: "handleRequest".into(),
                    reason: "param \"socket\" → net.Socket (network handle — requires live network binding)".into(),
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
        assert!(report.contains("handleRequest: param \"socket\" → net.Socket (network handle"));
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                    nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                    nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
                    refactoring_recommendations: vec![],
            }],
            skipped: vec![SkippedFunction {
                function_name: "f2".into(),
                reason: "timed out after 30s".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 4,
            workers_reaped: 0,
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
                    raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
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
                    mock_misses: vec![],
                    refactoring_recommendations: vec![],
            }],
            skipped: vec![],
            workers_used: 1,
            workers_reaped: 0,
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            reason: "param \"sock\" → net.Socket (network handle — requires live network binding)".into(),
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
        };

        let plan = format_dry_run_plan(&analyses, &skipped, &config).expect("should succeed");

        assert!(plan.contains("Skipped (unexecutable)"));
        assert!(plan.contains("broken: param \"sock\" → net.Socket (network handle"));
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.function_results.len(), 4, "all 4 functions must complete");
        // With 4 tasks and max=4 the pool starts at initial_workers(4,4)=1 and must
        // grow as tasks complete.  After all 4 tasks run, live_count should be > 1.
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

    /// When a layer has more workers than remaining tasks, idle workers must be
    /// reaped rather than kept alive until layer shutdown.
    ///
    /// This is the perf-evidence test for idle reaping: `workers_reaped` must be
    /// > 0 (reaping actually fired), all tasks must complete (no deadlock), and
    /// `workers_used` must still reflect the true peak (not the post-reap count).
    #[tokio::test]
    async fn parallel_scan_idle_workers_reaped_on_shrinking_workload() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        // Four independent functions in one layer. The pool starts at
        // initial_workers(4,4)=1 and grows as tasks complete. As completion
        // accelerates (noop is near-instant), live_count will exceed remaining
        // tasks, triggering idle reaping.
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
            loops: vec![],
            source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // All tasks must complete — reaping must never cause deadlock.
        assert_eq!(result.function_results.len(), 4, "all 4 functions must complete (no deadlock)");

        // Reaping must have fired: with 4 tasks completing near-simultaneously,
        // live_count will exceed remaining tasks at some point.
        assert!(
            result.workers_reaped > 0,
            "idle reaping should have fired (workers_reaped={})",
            result.workers_reaped,
        );

        // workers_used tracks the peak (not post-reap live_count) so existing
        // growth assertions still hold.
        assert!(
            result.workers_used >= 1,
            "workers_used must reflect the true peak (got {})",
            result.workers_used,
        );
    }

    // ── Persistent pool cross-layer tests ────────────────────────────

    /// A 3-function chain forces 3 topological layers (A → B → C).
    /// The persistent pool must survive all layer transitions — if it were
    /// accidentally shut down between layers, checkout() would deadlock.
    #[tokio::test]
    async fn parallel_scan_pool_persists_across_layers() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let make_fn = |name: &str, deps: Vec<&str>| FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
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
            loops: vec![],
            source_file: None,
        };

        // leaf_a → mid → root: three distinct topological layers.
        let analyses = vec![
            make_fn("leaf_a", vec![]),
            make_fn("mid", vec!["leaf_a"]),
            make_fn("root", vec!["mid"]),
        ];

        let mut file_map = HashMap::new();
        for name in ["leaf_a", "mid", "root"] {
            file_map.insert(name.to_string(), "test.ts".to_string());
        }

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
            isolation: IsolationMode::None,
            capture_side_effects: false,
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed across 3 layers");

        // All 3 functions must complete — deadlock would prevent this.
        assert_eq!(result.function_results.len(), 3, "all 3 functions must complete");
        assert!(result.skipped.is_empty(), "no functions should be skipped");

        // Each layer has 1 task → pool is capped at 1 per layer.
        // With the persistent pool the same 1 worker is reused for all 3 layers.
        assert_eq!(result.workers_used, 1, "single worker reused across all 3 layers");

        // Dependency order: leaf_a < mid < root.
        let order = &result.test_order;
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("leaf_a") < pos("mid"), "leaf_a must come before mid");
        assert!(pos("mid") < pos("root"), "mid must come before root");
    }

    /// Four independent leaf functions followed by one root that depends on all
    /// four forces the pool to grow during layer 0 (4 tasks, parallelism 4) and
    /// then shrink for layer 1 (1 task). The persistent pool must handle the
    /// size transition correctly: reapers should drain the excess workers rather
    /// than crashing or deadlocking.
    #[tokio::test]
    async fn parallel_scan_pool_reuses_workers_across_shrinking_layers() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let make_leaf = |name: &str| FunctionAnalysis {
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
            loops: vec![],
            source_file: None,
        };

        let leaf_names = ["leaf_a", "leaf_b", "leaf_c", "leaf_d"];
        let mut analyses: Vec<FunctionAnalysis> =
            leaf_names.iter().map(|n| make_leaf(n)).collect();

        // root depends on all 4 leaves → placed in layer 1.
        analyses.push(FunctionAnalysis {
            name: "root".to_string(),
            exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![],
            dependencies: leaf_names
                .iter()
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
            loops: vec![],
            source_file: None,
        });

        let mut file_map = HashMap::new();
        for name in leaf_names.iter().chain(["root"].iter()) {
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed with shrinking layer transition");

        // All 5 functions (4 leaves + root) must complete — no deadlock.
        assert_eq!(result.function_results.len(), 5, "all 5 functions must complete");
        assert!(result.skipped.is_empty(), "no functions should be skipped");

        // root must come after all leaves.
        let order = &result.test_order;
        let root_pos = order.iter().position(|n| n == "root").unwrap();
        for leaf in &leaf_names {
            let leaf_pos = order.iter().position(|n| n == *leaf).unwrap();
            assert!(leaf_pos < root_pos, "{leaf} must be explored before root");
        }

        // Layer 0 had 4 tasks with parallelism 4: pool started at
        // initial_workers(4,4)=1 and grew as tasks were dispatched.
        assert!(
            result.workers_used >= 2,
            "pool should have grown to ≥2 workers during layer 0 (got {})",
            result.workers_used,
        );

        // As layer 0 tasks completed rapidly (noop), live_count exceeded
        // remaining+MIN_IDLE, triggering idle reaping. The persistent pool
        // accumulates the reap count across both layers.
        assert!(
            result.workers_reaped > 0,
            "idle workers should have been reaped during layer 0→1 transition (got {})",
            result.workers_reaped,
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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
                loops: vec![],
                source_file: None,
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
                loops: vec![],
                source_file: None,
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
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
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

    // ── idle floor and speculative pre-spawn tests ─────────────────

    /// `return_or_reap_worker` respects the MIN_IDLE_WORKERS floor: it should
    /// not reap when live_count equals the floor, and should reap when above.
    #[tokio::test]
    async fn return_or_reap_respects_idle_floor() {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;

        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;
        let fe_config = Arc::new(fe_config);

        // Create a pool with max_workers=3, needed=3.
        // initial_workers(3,3) = 1 (quarter of 3 = max(0,1) = 1).
        let pool = WorkerPool::spawn_capped(Arc::clone(&fe_config), 3, 3, None)
            .await
            .expect("pool should spawn");
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 1);

        // Manually grow to 3 by spawning workers and depositing them.
        for _ in 0..2 {
            let fe = Frontend::spawn(&fe_config).await.expect("spawn");
            let _ = pool.sender.send(fe).await;
            pool.live_count.fetch_add(1, Ordering::Relaxed);
        }
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 3);

        // Checkout a worker and return with pending=2.
        // floor = min(2 + 1, 3) = 3. current=3 == floor → no reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 2).await;
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 3, "should not reap when at floor");
        assert_eq!(pool.idle_reaped(), 0);

        // Checkout and return with pending=1.
        // floor = min(1 + 1, 3) = 2. current=3 > floor → should reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 1).await;
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 2, "should reap one worker when above floor");
        assert_eq!(pool.idle_reaped(), 1);

        // Checkout and return with pending=0.
        // floor = min(0 + 1, 3) = 1. current=2 > floor → should reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 0).await;
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 1, "should reap to floor");
        assert_eq!(pool.idle_reaped(), 2);

        // Checkout and return with pending=0 again.
        // floor = 1. current=1 == floor → no reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 0).await;
        assert_eq!(pool.live_count.load(Ordering::Relaxed), 1, "should not reap below floor");
        assert_eq!(pool.idle_reaped(), 2);

        pool.shutdown().await;
    }

    /// A speculative pre-spawn is deposited into the pool as an initial worker.
    #[tokio::test]
    async fn speculative_prespawn_is_used() {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;

        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;
        let fe_config = Arc::new(fe_config);

        // Pre-spawn a worker.
        let prewarmed = Frontend::spawn(&fe_config).await.expect("spawn should succeed");

        // Create pool with prewarmed worker, needing 1 worker, max 2.
        let pool = WorkerPool::spawn_capped(Arc::clone(&fe_config), 2, 1, Some(prewarmed))
            .await
            .expect("pool should spawn");

        assert_eq!(pool.live_count.load(Ordering::Relaxed), 1, "pool should have 1 worker from prewarmed");

        // Checkout should succeed without blocking indefinitely.
        let worker = pool.checkout().await;
        pool.return_or_reap_worker(worker, 1).await;

        pool.shutdown().await;
    }

    // ──────────────────────────────────────────────────────────────────
    // Unit tests for derive_replica_seed and merge_replica_results
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn derive_replica_seed_deterministic() {
        // Same inputs always produce the same seed.
        assert_eq!(
            derive_replica_seed(Some(42), 0, 0),
            derive_replica_seed(Some(42), 0, 0)
        );
        assert_eq!(
            derive_replica_seed(Some(42), 3, 1),
            derive_replica_seed(Some(42), 3, 1)
        );
    }

    #[test]
    fn derive_replica_seed_none_stays_none() {
        assert_eq!(derive_replica_seed(None, 0, 0), None);
        assert_eq!(derive_replica_seed(None, 5, 3), None);
    }

    #[test]
    fn derive_replica_seed_distinct_replicas() {
        // Different replicas of the same function get different seeds.
        let s0 = derive_replica_seed(Some(1), 0, 0);
        let s1 = derive_replica_seed(Some(1), 0, 1);
        let s2 = derive_replica_seed(Some(1), 0, 2);
        assert_ne!(s0, s1);
        assert_ne!(s0, s2);
        assert_ne!(s1, s2);
        // Different functions get different seeds even at replica 0.
        let sf0 = derive_replica_seed(Some(1), 0, 0);
        let sf1 = derive_replica_seed(Some(1), 1, 0);
        assert_ne!(sf0, sf1);
    }

    /// Build a minimal `FunctionResult` with the given raw results for testing merge.
    fn make_function_result(
        func_name: &str,
        raw_results: Vec<(Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>, crate::protocol::ExecuteResult)>,
    ) -> FunctionResult {
        use crate::explorer::ObservationOutput;
        let exploration = ObservationOutput {
            function_name: func_name.to_string(),
            iterations: raw_results.len() as u32,
            unique_paths: raw_results.len(),
            lines_covered: 0,
            total_lines: 0,
            new_path_executions: vec![],
            raw_results,
            discoveries: vec![],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: Default::default(),
            mcdc_summary: None,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![], opaque_suggestions: vec![], stubbed_modules: vec![],
        };
        let analysis = make_analysis(func_name, vec![]);
        let mut analyze_out = crate::pipeline::analyze(&exploration, &analysis);
        analyze_out.behavior_map.fingerprint = None;
        FunctionResult {
            function_name: func_name.to_string(),
            exploration,
            behavior_map: analyze_out.behavior_map,
            behavior_coverage: vec![],
            mocks_used: vec![],
            coverage_metrics: analyze_out.coverage_metrics,
            mock_misses: vec![],
            refactoring_recommendations: vec![],
        }
    }

    fn make_execute_result(branch_id: u32) -> ExecuteResult {
        use crate::execution_record::{BranchDecision, SymConstraint};
        ExecuteResult {
            branch_path: vec![BranchDecision {
                branch_id,
                line: 1,
                taken: true,
                constraint: SymConstraint::Unknown { hint: String::new() },
                conditions: None,
            }],
            scope_events: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            return_value: Some(serde_json::Value::Null),
            thrown_error: None,
            side_effects: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![], runtime_crypto_boundaries: vec![],
        }
    }

    #[test]
    fn merge_replica_results_single_passthrough() {
        // A single-element merge should return an equivalent result.
        let result = make_function_result("foo", vec![
            (vec![], vec![], make_execute_result(1)),
        ]);
        let analysis = make_analysis("foo", vec![]);
        let merged = merge_replica_results(vec![result], &analysis);
        assert_eq!(merged.function_name, "foo");
        assert_eq!(merged.behavior_map.behaviors.len(), 1);
    }

    #[test]
    fn merge_replica_results_deduplicates_identical_inputs() {
        // Two replicas both discover the same input: merged result should deduplicate.
        let input_a = vec![serde_json::json!(1)];
        let exec_a = make_execute_result(1);

        let r1 = make_function_result("bar", vec![(input_a.clone(), vec![], exec_a.clone())]);
        let r2 = make_function_result("bar", vec![(input_a.clone(), vec![], exec_a.clone())]);

        let analysis = make_analysis("bar", vec![]);
        let merged = merge_replica_results(vec![r1, r2], &analysis);

        assert_eq!(merged.function_name, "bar");
        // Same input hash → deduplicated to 1 behavior.
        assert_eq!(merged.behavior_map.behaviors.len(), 1);
        // iterations = sum of replicas
        assert_eq!(merged.exploration.iterations, 2);
    }

    #[test]
    fn merge_replica_results_combines_distinct_inputs() {
        let input_a = vec![serde_json::json!(1)];
        let input_b = vec![serde_json::json!(2)];

        let r1 = make_function_result("baz", vec![(input_a, vec![], make_execute_result(1))]);
        let r2 = make_function_result("baz", vec![(input_b, vec![], make_execute_result(2))]);

        let analysis = make_analysis("baz", vec![]);
        let merged = merge_replica_results(vec![r1, r2], &analysis);

        assert_eq!(merged.function_name, "baz");
        // Two distinct inputs → 2 behaviors in merged map.
        assert_eq!(merged.behavior_map.behaviors.len(), 2);
        assert_eq!(merged.exploration.iterations, 2);
    }

    // --- BudgetSurplus unit tests ---

    #[test]
    fn budget_surplus_donate_and_claim() {
        let surplus = BudgetSurplus::new();
        assert_eq!(surplus.available(), 0);

        surplus.donate(50);
        assert_eq!(surplus.available(), 50);

        let claimed = surplus.try_claim(30, 1);
        assert_eq!(claimed, 30);
        assert_eq!(surplus.available(), 20);
    }

    #[test]
    fn budget_surplus_claim_limited_by_available() {
        let surplus = BudgetSurplus::new();
        surplus.donate(10);

        let claimed = surplus.try_claim(100, 1);
        assert_eq!(claimed, 10);
        assert_eq!(surplus.available(), 0);
    }

    #[test]
    fn budget_surplus_claim_zero_when_below_min() {
        let surplus = BudgetSurplus::new();
        surplus.donate(3);

        // min_claim is 5, but only 3 available → returns 0.
        let claimed = surplus.try_claim(10, 5);
        assert_eq!(claimed, 0);
        assert_eq!(surplus.available(), 3);
    }

    #[test]
    fn budget_surplus_multiple_donations() {
        let surplus = BudgetSurplus::new();
        surplus.donate(10);
        surplus.donate(20);
        surplus.donate(30);
        assert_eq!(surplus.available(), 60);
    }

    #[test]
    fn budget_surplus_donate_zero_is_noop() {
        let surplus = BudgetSurplus::new();
        surplus.donate(0);
        assert_eq!(surplus.available(), 0);
    }

    #[test]
    fn claim_policy_rejects_stalled_function() {
        let policy = ClaimPolicy::default(); // min_hit_rate = 0.1, window = 10
        // 0 new paths in 10 executions → hit rate 0.0 < 0.1
        assert!(!policy.should_claim(0));
    }

    #[test]
    fn claim_policy_accepts_productive_function() {
        let policy = ClaimPolicy::default();
        // 2 new paths in 10 executions → hit rate 0.2 >= 0.1
        assert!(policy.should_claim(2));
    }

    #[test]
    fn claim_policy_boundary_hit_rate() {
        let policy = ClaimPolicy::default();
        // 1 new path in 10 executions → hit rate 0.1 >= 0.1 (exactly at threshold)
        assert!(policy.should_claim(1));
    }

    #[test]
    fn claim_policy_max_claimable_caps_at_fraction() {
        let policy = ClaimPolicy {
            max_claim_fraction: 0.5,
            ..ClaimPolicy::default()
        };
        assert_eq!(policy.max_claimable(100), 50);
        assert_eq!(policy.max_claimable(0), 0);
        assert_eq!(policy.max_claimable(1), 0); // floor(0.5) = 0
    }

    #[test]
    fn claim_policy_zero_window_always_rejects() {
        let policy = ClaimPolicy {
            window: 0,
            ..ClaimPolicy::default()
        };
        assert!(!policy.should_claim(5));
    }

    #[test]
    fn budget_surplus_layer_boundary_reset() {
        // Each layer creates a new BudgetSurplus — verify they're independent.
        let layer0 = BudgetSurplus::new();
        layer0.donate(100);

        let layer1 = BudgetSurplus::new();
        assert_eq!(layer1.available(), 0);
        // layer0's surplus is not visible to layer1.
        assert_eq!(layer0.available(), 100);
    }

    // --- detect_mock_misses unit tests ---

    /// Build a minimal `ExecuteResult` with the given external calls.
    fn make_execute_result_with_calls(calls: Vec<crate::execution_record::ExternalCall>) -> crate::protocol::ExecuteResult {
        use crate::execution_record::{BranchDecision, SymConstraint};
        crate::protocol::ExecuteResult {
            branch_path: vec![BranchDecision {
                branch_id: 1,
                line: 1,
                taken: true,
                constraint: SymConstraint::Unknown { hint: String::new() },
                conditions: None,
            }],
            scope_events: vec![],
            lines_executed: vec![],
            calls_to_external: calls,
            path_constraints: vec![],
            return_value: None,
            thrown_error: None,
            side_effects: vec![],
            performance: crate::protocol::PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
        }
    }

    /// Build a `BehaviorMap` with the given input_args entries as behaviors.
    fn make_behavior_map_with_inputs(function_id: &str, known_inputs: Vec<Vec<serde_json::Value>>) -> BehaviorMap {
        use crate::behavior::Behavior;
        BehaviorMap {
            function_id: function_id.to_string(),
            behaviors: known_inputs
                .into_iter()
                .enumerate()
                .map(|(i, args)| Behavior {
                    id: i as u32,
                    input_args: args,
                    return_value: Some(serde_json::json!(i)),
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                })
                .collect(),
            fingerprint: None,
            nondeterministic_fields: vec![],
        }
    }

    #[test]
    fn detect_mock_misses_empty_raw_results_produces_no_misses() {
        let callee_map = make_behavior_map_with_inputs("callee", vec![vec![serde_json::json!(1)]]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);
        let misses = detect_mock_misses(&[], &callee_maps);
        assert!(misses.is_empty());
    }

    #[test]
    fn detect_mock_misses_hit_produces_no_miss() {
        // Caller passes args that ARE in the callee's behavior map → no miss.
        let known_args = vec![serde_json::json!(42)];
        let callee_map = make_behavior_map_with_inputs("callee", vec![known_args.clone()]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

        let caller_inputs = vec![serde_json::json!(0)];
        let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: known_args,
            return_value: serde_json::json!(99),
        }]);
        let misses = detect_mock_misses(&[(caller_inputs, vec![], exec)], &callee_maps);
        assert!(misses.is_empty(), "expected no miss when args match behavior map");
    }

    #[test]
    fn detect_mock_misses_miss_is_recorded() {
        // Caller passes args NOT in the callee's behavior map → one miss.
        let callee_map = make_behavior_map_with_inputs("callee", vec![vec![serde_json::json!(1)]]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

        let missed_args = vec![serde_json::json!(999)]; // not in behavior map
        let caller_inputs = vec![serde_json::json!(0)];
        let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: missed_args.clone(),
            return_value: serde_json::json!(0),
        }]);

        let misses = detect_mock_misses(&[(caller_inputs, vec![], exec)], &callee_maps);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].callee_name, "callee");
        assert_eq!(misses[0].missed_inputs, missed_args);
    }

    #[test]
    fn detect_mock_misses_deduplicates_identical_misses_across_executions() {
        // Two caller executions both call callee with the same unknown args.
        // Should produce one miss, not two.
        let callee_map = make_behavior_map_with_inputs("callee", vec![]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

        let missed_args = vec![serde_json::json!(7)];
        let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: missed_args,
            return_value: serde_json::json!(0),
        }]);

        let raw = vec![
            (vec![serde_json::json!(1)], vec![], exec.clone()),
            (vec![serde_json::json!(2)], vec![], exec),
        ];
        let misses = detect_mock_misses(&raw, &callee_maps);
        assert_eq!(misses.len(), 1, "duplicate missed args should be deduplicated");
    }

    #[test]
    fn detect_mock_misses_distinct_missed_args_each_recorded() {
        // Two distinct sets of missed args → two miss entries.
        let callee_map = make_behavior_map_with_inputs("callee", vec![]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

        let missed_a = vec![serde_json::json!(10)];
        let missed_b = vec![serde_json::json!(20)];

        let exec_a = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: missed_a,
            return_value: serde_json::json!(0),
        }]);
        let exec_b = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: missed_b,
            return_value: serde_json::json!(0),
        }]);

        let raw = vec![
            (vec![serde_json::json!(1)], vec![], exec_a),
            (vec![serde_json::json!(2)], vec![], exec_b),
        ];
        let misses = detect_mock_misses(&raw, &callee_maps);
        assert_eq!(misses.len(), 2);
    }

    #[test]
    fn detect_mock_misses_ignores_unknown_callee() {
        // Calls to symbols not in callee_maps are silently skipped.
        let callee_maps: HashMap<String, BehaviorMap> = HashMap::new();
        let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "unknown_dep".to_string(),
            args: vec![serde_json::json!(1)],
            return_value: serde_json::json!(0),
        }]);
        let misses = detect_mock_misses(&[(vec![], vec![], exec)], &callee_maps);
        assert!(misses.is_empty());
    }

    #[test]
    fn detect_mock_misses_caller_execution_id_set() {
        // The caller_execution_id should be the input hash of the triggering execution.
        let callee_map = make_behavior_map_with_inputs("callee", vec![]);
        let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

        let caller_inputs = vec![serde_json::json!(42)];
        let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
            symbol: "callee".to_string(),
            args: vec![serde_json::json!(1)],
            return_value: serde_json::json!(0),
        }]);

        let misses = detect_mock_misses(&[(caller_inputs.clone(), vec![], exec)], &callee_maps);
        assert_eq!(misses.len(), 1);
        // Verify it matches the hash of caller_inputs.
        let mut hasher = std::hash::DefaultHasher::new();
        let input_str = serde_json::to_string(&caller_inputs).unwrap();
        use std::hash::Hash;
        input_str.hash(&mut hasher);
        use std::hash::Hasher;
        let expected_id = hasher.finish();
        assert_eq!(misses[0].caller_execution_id, expected_id);
    }

    #[test]
    fn format_mock_misses_empty_produces_empty_string() {
        assert_eq!(format_mock_misses(&[]), "");
    }

    #[test]
    fn format_mock_misses_single_miss_shows_callee_and_inputs() {
        let miss = MockMiss {
            callee_name: "myCallee".to_string(),
            missed_inputs: vec![serde_json::json!(1), serde_json::json!("hello")],
            caller_execution_id: 0,
        };
        let output = format_mock_misses(&[miss]);
        assert!(output.contains("myCallee"), "should mention callee name");
        assert!(output.contains("1 miss"), "should mention count");
    }

    #[test]
    fn format_scan_report_includes_mock_miss_line() {
        // A scan result with a mock miss should show it in the report.
        let miss = MockMiss {
            callee_name: "leaf".to_string(),
            missed_inputs: vec![serde_json::json!(99)],
            caller_execution_id: 1234,
        };
        let result = ScanResult {
            test_order: vec!["leaf".into(), "caller".into()],
            function_results: vec![
                FunctionResult {
                    function_name: "leaf".into(),
                    exploration: ObservationOutput {
                        function_name: "leaf".into(),
                        iterations: 5,
                        unique_paths: 1,
                        lines_covered: 3,
                        total_lines: 5,
                        new_path_executions: vec![],
                        raw_results: vec![],
                        discoveries: vec![],
                        nondeterministic_fields: vec![],
                        float_probe_results: vec![],
                        boundary_results: vec![],
                        shrunk_witnesses: std::collections::HashMap::new(),
                        mcdc_summary: None,
                        shrink_stats: crate::shrink::ShrinkStats::default(),
                        abandoned_frontiers: vec![],
                        opaque_suggestions: vec![], stubbed_modules: vec![],
                    },
                    behavior_map: BehaviorMap { function_id: "leaf".into(), behaviors: vec![], fingerprint: None, nondeterministic_fields: vec![] },
                    behavior_coverage: vec![],
                    mocks_used: vec![],
                    mock_misses: vec![],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
                },
                FunctionResult {
                    function_name: "caller".into(),
                    exploration: ObservationOutput {
                        function_name: "caller".into(),
                        iterations: 10,
                        unique_paths: 2,
                        lines_covered: 8,
                        total_lines: 10,
                        new_path_executions: vec![],
                        raw_results: vec![],
                        discoveries: vec![],
                        nondeterministic_fields: vec![],
                        float_probe_results: vec![],
                        boundary_results: vec![],
                        shrunk_witnesses: std::collections::HashMap::new(),
                        mcdc_summary: None,
                        shrink_stats: crate::shrink::ShrinkStats::default(),
                        abandoned_frontiers: vec![],
                        opaque_suggestions: vec![], stubbed_modules: vec![],
                    },
                    behavior_map: BehaviorMap { function_id: "caller".into(), behaviors: vec![], fingerprint: None, nondeterministic_fields: vec![] },
                    behavior_coverage: vec![],
                    mocks_used: vec![MockUsage { name: "leaf".into(), source: MockSource::CachedBehaviorMap }],
                    mock_misses: vec![miss],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
                },
            ],
            skipped_functions: vec![],
            sampling: None,
        };

        let report = format_scan_report(&result);
        assert!(
            report.contains("Mock misses"),
            "report should contain 'Mock misses' section, got:\n{report}"
        );
        assert!(
            report.contains("leaf"),
            "mock miss section should name the callee"
        );
    }

    // --- detect_mock_misses property tests ---

    mod mock_miss_props {
        use super::*;
        use proptest::prelude::*;

        /// Generate a small JSON value (scalar or short string) for use in test args.
        fn arb_json_scalar() -> impl Strategy<Value = serde_json::Value> {
            prop_oneof![
                any::<i32>().prop_map(|v| serde_json::json!(v)),
                ".{0,20}".prop_map(|s: String| serde_json::Value::String(s)),
                any::<bool>().prop_map(|v| serde_json::json!(v)),
            ]
        }

        /// Generate a small args vector (0–3 elements).
        fn arb_args() -> impl Strategy<Value = Vec<serde_json::Value>> {
            proptest::collection::vec(arb_json_scalar(), 0..4)
        }

        proptest! {
            /// Any call whose args exactly match a behavior-map entry is never a miss.
            #[test]
            fn hit_never_produces_miss(
                known_args in arb_args(),
            ) {
                let callee_map = make_behavior_map_with_inputs("callee", vec![known_args.clone()]);
                let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

                let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
                    symbol: "callee".to_string(),
                    args: known_args,
                    return_value: serde_json::json!(null),
                }]);
                let misses = detect_mock_misses(&[(vec![], vec![], exec)], &callee_maps);
                prop_assert!(misses.is_empty(), "a call matching the behavior map should never be a miss");
            }

            /// Misses never reference a callee not in `callee_maps`.
            #[test]
            fn misses_only_reference_known_callees(
                known_args in arb_args(),
                missed_args in arb_args(),
            ) {
                prop_assume!(known_args != missed_args);

                let callee_map = make_behavior_map_with_inputs("callee", vec![known_args]);
                let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

                let exec = make_execute_result_with_calls(vec![
                    crate::execution_record::ExternalCall {
                        symbol: "callee".to_string(),
                        args: missed_args,
                        return_value: serde_json::json!(null),
                    },
                    crate::execution_record::ExternalCall {
                        symbol: "unknown".to_string(),  // not in callee_maps
                        args: vec![serde_json::json!(1)],
                        return_value: serde_json::json!(null),
                    },
                ]);
                let misses = detect_mock_misses(&[(vec![], vec![], exec)], &callee_maps);
                // All misses reference only "callee", never "unknown".
                for miss in &misses {
                    prop_assert_eq!(miss.callee_name.as_str(), "callee");
                }
            }

            /// Duplicate (callee, args) pairs across executions produce only one miss entry.
            #[test]
            fn deduplication_invariant(
                missed_args in arb_args(),
                n_duplicates in 1usize..5,
            ) {
                let callee_map = make_behavior_map_with_inputs("callee", vec![]);
                let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

                let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
                    symbol: "callee".to_string(),
                    args: missed_args,
                    return_value: serde_json::json!(null),
                }]);
                let raw: Vec<_> = (0..n_duplicates)
                    .map(|i| (vec![serde_json::json!(i as i32)], vec![], exec.clone()))
                    .collect();
                let misses = detect_mock_misses(&raw, &callee_maps);
                prop_assert_eq!(misses.len(), 1, "duplicates should be collapsed to one miss");
            }

            /// Misses count never exceeds total distinct (callee, args) pairs in the raw results.
            #[test]
            fn miss_count_bounded_by_distinct_call_sites(
                call_args in proptest::collection::vec(arb_args(), 1..8),
            ) {
                // Empty behavior map → every distinct call is a miss.
                let callee_map = make_behavior_map_with_inputs("callee", vec![]);
                let callee_maps = HashMap::from([("callee".to_string(), callee_map)]);

                let raw: Vec<_> = call_args.iter().enumerate().map(|(i, args)| {
                    let exec = make_execute_result_with_calls(vec![crate::execution_record::ExternalCall {
                        symbol: "callee".to_string(),
                        args: args.clone(),
                        return_value: serde_json::json!(null),
                    }]);
                    (vec![serde_json::json!(i as i32)], vec![], exec)
                }).collect();

                let misses = detect_mock_misses(&raw, &callee_maps);
                // Miss count ≤ number of distinct arg vectors.
                let distinct: std::collections::HashSet<String> = call_args
                    .iter()
                    .map(|a| serde_json::to_string(a).unwrap())
                    .collect();
                prop_assert!(
                    misses.len() <= distinct.len(),
                    "miss count {} should not exceed distinct call count {}",
                    misses.len(), distinct.len()
                );
            }
        }
    }

    // --- BudgetSurplus property tests ---

    mod budget_surplus_props {
        use super::*;
        use proptest::prelude::*;
        use std::sync::Arc;

        proptest! {
            #[test]
            fn surplus_conservation(
                donations in proptest::collection::vec(0u32..1000, 1..20),
                claims in proptest::collection::vec((1u32..500, 1u32..10), 1..20),
            ) {
                let surplus = BudgetSurplus::new();
                let total_donated: u64 = donations.iter().map(|&d| d as u64).sum();

                for d in &donations {
                    surplus.donate(*d);
                }

                let mut total_claimed: u64 = 0;
                for (requested, min_claim) in &claims {
                    let claimed = surplus.try_claim(*requested, *min_claim);
                    total_claimed += claimed as u64;
                }

                // Total claimed never exceeds total donated.
                prop_assert!(total_claimed <= total_donated);
                // Remaining + claimed = donated.
                prop_assert_eq!(surplus.available() as u64 + total_claimed, total_donated);
            }

            #[test]
            fn concurrent_claims_never_exceed_donated(
                donated in 10u32..10000,
                num_claimers in 2usize..8,
            ) {
                let surplus = Arc::new(BudgetSurplus::new());
                surplus.donate(donated);

                let mut handles = Vec::new();
                for _ in 0..num_claimers {
                    let s = Arc::clone(&surplus);
                    handles.push(std::thread::spawn(move || {
                        let mut total = 0u64;
                        // Each thread claims in small chunks until exhausted.
                        loop {
                            let claimed = s.try_claim(10, 1);
                            if claimed == 0 {
                                break;
                            }
                            total += claimed as u64;
                        }
                        total
                    }));
                }

                let total_claimed: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
                prop_assert_eq!(total_claimed, donated as u64);
                prop_assert_eq!(surplus.available(), 0);
            }

            #[test]
            fn claim_policy_monotonic_hit_rate(
                low_hits in 0u32..5,
                high_hits in 5u32..20,
            ) {
                let policy = ClaimPolicy::default();
                // If low hits qualifies, high hits must also qualify.
                if policy.should_claim(low_hits) {
                    prop_assert!(policy.should_claim(high_hits));
                }
            }
        }
    }
}
