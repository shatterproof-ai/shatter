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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::auto_mock;
use crate::behavior::{BehaviorCoverage, BehaviorMap, CallGraph, CallGraphError, TestOrderEntry};
use crate::cache::{BehaviorMapCache, StoredInputsCache};
use crate::execution_record::ExecutionRecord;
use crate::explorer::{self, ExploreConfig, ExploreError, IsolationMode, ObservationOutput};
use crate::fingerprint::FunctionSignature;
use crate::frontend::{Frontend, FrontendConfig, FrontendError};
use crate::interesting_pool::{self, InterestingPool};
use crate::mock_gen::mock_config_from_behavior_map;
use crate::pipeline::{self, AnalyzeOutput};
use crate::protocol::{BranchInfo, BranchType, ExecuteResult, FunctionAnalysis, MockConfig};
use crate::setup_manager::SetupManager;
use crate::status_export::{
    StatusArtifactLink, StatusExportInput, StatusFileInput, StatusFileStatus, StatusReportValidity,
    StatusRollupInput, StatusTargetInput, StatusTargetOutcome, StatusTargetValidityImpact,
    StatusValidityReason,
};
use crate::types::TypeInfo;

const TOTAL_SCAN_TIMEOUT_REASON: &str = "timed out (total scan budget exceeded)";

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
    /// Use the Z3-backed concolic orchestrator instead of random exploration.
    pub concolic: bool,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Map from qualified function ID to source file path (needed for instrumentation).
    pub file_map: HashMap<String, String>,
    /// Number of parallel frontend subprocesses (default: 1).
    pub parallelism: usize,
    /// Per-function timeout. If a function takes longer, it is skipped.
    /// Default: 30 seconds.
    pub timeout_per_fn: Duration,
    /// Per-function build/prepare timeout. Bounds the `Prepare` command (where
    /// the frontend compiles the launcher / harness) separately from the
    /// concolic exploration budget — a cold Go build cache otherwise eats most
    /// of `timeout_per_fn` and surfaces as a misleading "timed out" on a tiny
    /// function (str-v5qe, str-6sie).
    pub build_timeout: Duration,
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
    /// are skipped with reason "timed out (total scan budget exceeded)".
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
    /// Configuration for the optional genetic algorithm follow-up phase.
    pub genetic_config: crate::config::GeneticConfig,
    /// When `Some`, explore each function in fixed-size iteration batches
    /// using round-robin scheduling within each layer. Functions are explored
    /// one at a time; non-exhausted functions are re-enqueued for another
    /// batch. When `None` (default), existing parallel execution is used.
    ///
    /// This is an internal tuning parameter — not exposed via CLI.
    pub batch_size: Option<u32>,
    /// Optional disk cache for persisting per-function scheduler state
    /// across runs. When `Some` and batched mode is active
    /// (`batch_size` is `Some`), `run_layer_batched` loads advisory
    /// state on entry and stores it when a function's batch loop
    /// finishes. Scheduler state is advisory and reconstructible — a
    /// cache miss (missing, corrupt, or wrong-schema entry) degrades
    /// silently and the scheduler runs from scratch.
    pub scheduler_state_cache: Option<Arc<crate::cache::SchedulerStateCache>>,
    /// Optional disk cache for persisting function input vectors across
    /// runs. Keyed by [`crate::fingerprint::FunctionSignature`] so stored
    /// inputs survive body edits that drop the behavior map cache
    /// (str-bo4z.3). When `Some`, every successful scan of a function also
    /// writes the exercised input vectors to this cache so a later run can
    /// replay them as seeds (str-bo4z.4).
    pub stored_inputs_cache: Option<Arc<crate::cache::StoredInputsCache>>,
    /// Active coverage mode for this scan. Determines the on-disk namespace
    /// for persisted scheduler state so branch-mode and MC/DC-mode runs
    /// maintain independent cooldown and attempt histories (str-bo4z.7).
    pub coverage_mode: crate::interesting_pool::CoverageMode,
    /// When `false`, suppress all project-local artifact writes for this
    /// scan run: per-function `<scan_root>/functions/*.json`,
    /// `summary.json`, and `manifest.json`. Other persistence remains gated
    /// by its own knobs (`cache`, `pool_path`, `resume_path`,
    /// `stored_inputs_cache`, `scheduler_state_cache`). Default `true`
    /// preserves existing library behavior; the CLI sets `false` when the
    /// caller passes explicit external `-o` outputs together with
    /// `--no-cache --no-seeds` so Shatter behaves as a clean external audit
    /// tool (str-1wcl).
    pub write_artifacts: bool,
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
    /// Qualified ID of the explored function.
    ///
    /// This is scan-internal identity-bearing text, not necessarily the
    /// human-facing display name. Report builders split it into
    /// `qualified_id` and `display_name` fields for wire output.
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

fn analyze_exploration(
    exploration: &ObservationOutput,
    analysis: &FunctionAnalysis,
    fingerprint: Option<String>,
) -> AnalyzeOutput {
    let mut analyze_out = pipeline::analyze(exploration, analysis);
    analyze_out.behavior_map.fingerprint = fingerprint;
    analyze_out
}

/// Result of a full scan across multiple functions.
#[derive(Debug)]
pub struct ScanResult {
    /// Per-function results in test order.
    pub function_results: Vec<FunctionResult>,
    /// Qualified function IDs in the order they were tested.
    pub test_order: Vec<String>,
    /// Functions that were skipped before exploration (e.g. unexecutable parameter types).
    pub skipped_functions: Vec<SkippedFunction>,
    /// Sampling context (populated when --core-sample is active).
    pub sampling: Option<SamplingContext>,
    /// Source-file snapshots from the run-start manifest (str-jeen.60/63).
    /// Used by `generate_report_from_scan` to build `SourceSetSummary` from
    /// the discovered source set rather than from completed function rows.
    /// Empty when the scan ran without a manifest (e.g. synthetic test runs).
    pub source_files: Vec<crate::run_manifest::SourceFileSnapshot>,
}

/// Errors that can occur during a scan.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("exploration error: {0}")]
    Explore(#[from] ExploreError),
    #[error("concolic exploration error: {0}")]
    Concolic(#[from] crate::orchestrator::ExploreError),
    #[error("call graph cycle detected: {0}")]
    Cycle(#[from] CallGraphError),
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error(
        "frontend transport retry budget exhausted after {attempts} attempt(s): {message}"
    )]
    FrontendRetryExhausted { attempts: usize, message: String },
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
        /// Which phase timed out: `"build"` for `Prepare`, `"execution"`
        /// for the concolic exploration loop.  Surfaced in the report so
        /// the user can distinguish `--build-timeout` from
        /// `--timeout-per-fn` (str-7v73).
        phase: &'static str,
    },
    /// The whole-scan wall-clock budget expired while this function was
    /// queued or running in the active layer.
    TotalTimeout { function_name: String },
    /// Exploration encountered an error.
    Error {
        function_name: String,
        error: String,
    },
    /// Frontend declined the target as not-supported (e.g. Axum middleware
    /// signature). Mapped to `SkipCategory::Unsupported` in the scan report
    /// rather than `SkipCategory::Error`. (str-31j.4)
    Unsupported {
        function_name: String,
        reason: String,
    },
}

/// Status for a live scan progress update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProgressStatus {
    Started,
    Completed,
    Skipped,
    Failed,
}

impl ScanProgressStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// A live progress update emitted during scan execution.
#[derive(Debug, Clone)]
pub struct ScanProgressUpdate {
    /// Qualified function ID for the in-flight scan target.
    pub function_name: String,
    pub current: usize,
    pub total: usize,
    pub elapsed: Duration,
    pub status: ScanProgressStatus,
}

pub type ProgressHandler = Arc<dyn Fn(ScanProgressUpdate) + Send + Sync>;

/// Persist the input vectors from a behavior map to the signature-keyed
/// [`StoredInputsCache`] (str-bo4z.3).
///
/// No-op when `cache` is `None` so callers can unconditionally invoke this
/// alongside [`BehaviorMapCache::store`]. Failures to write are logged at
/// `warn` and swallowed — stored inputs are advisory, not load-bearing, so
/// a disk write failure must not fail the scan.
fn persist_stored_inputs(
    cache: Option<&StoredInputsCache>,
    analysis: &FunctionAnalysis,
    behavior_map: &BehaviorMap,
) {
    let Some(cache) = cache else { return };
    let signature = FunctionSignature::from_analysis(analysis);
    let inputs: Vec<Vec<serde_json::Value>> = behavior_map
        .behaviors
        .iter()
        .map(|b| b.input_args.clone())
        .collect();
    if let Err(e) = cache.store(&behavior_map.function_id, &signature, &inputs) {
        log::warn!(
            "failed to persist stored inputs for {}: {e}",
            behavior_map.function_id
        );
    }
}

fn emit_progress(
    progress_handler: Option<&ProgressHandler>,
    function_name: &str,
    current: usize,
    total: usize,
    elapsed: Duration,
    status: ScanProgressStatus,
) {
    if let Some(handler) = progress_handler {
        handler(ScanProgressUpdate {
            function_name: function_name.to_string(),
            current,
            total,
            elapsed,
            status,
        });
    }
}

/// Whether a skip is benign (expected), an unsupported target type, or an
/// actual error.
///
/// `Unsupported` is split out from `Expected` (str-jeen.46) so the scan
/// report can count attempted, skipped, and unsupported targets separately.
/// Examples: a function whose parameter list cannot be expressed in the
/// executability model is `Unsupported`; a checkpoint-resume skip or a
/// cache hit that we deliberately bypass is `Expected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipCategory {
    /// Benign: cache hits, checkpoint resumes, intentional bypasses.
    Expected,
    /// Target's shape (parameter types, invocation model, etc.) is not
    /// supported by the analyzer or executor. Discovered but never
    /// attempted.
    Unsupported,
    /// Problematic: timeouts, exploration errors, crashes. The function was
    /// attempted but failed.
    Error,
}

/// Summary of a function that was skipped during a scan.
#[derive(Debug)]
pub struct SkippedFunction {
    /// Qualified ID of the function that was skipped.
    pub function_name: String,
    /// Reason the function was skipped.
    pub reason: String,
    /// Whether this skip is expected or an error.
    pub category: SkipCategory,
}

/// Root directory for per-function scan artifacts.
fn scan_artifact_root(project_root: Option<&str>, scan_id: &str) -> PathBuf {
    scan_root(project_root, scan_id).join("functions")
}

/// Root directory for the entire scan (parent of `functions/`).
fn scan_root(project_root: Option<&str>, scan_id: &str) -> PathBuf {
    let root = project_root
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    crate::harness_storage::HarnessStorage::resolve_artifact_root(&root)
        .join("scan-results")
        .join(scan_id)
}

fn sanitize_artifact_component(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn scan_artifact_path(root: &Path, current: usize, function_name: &str) -> PathBuf {
    root.join(format!(
        "{:05}_{}.json",
        current,
        sanitize_artifact_component(function_name)
    ))
}

fn write_scan_artifact_json(
    root: &Path,
    current: usize,
    function_name: &str,
    value: &serde_json::Value,
) {
    let path = scan_artifact_path(root, current, function_name);
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        log::warn!("failed to create scan artifact dir: {e}");
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(value) else {
        log::warn!("failed to serialize scan artifact for {function_name}");
        return;
    };
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, json) {
        log::warn!("failed to write scan artifact temp file for {function_name}: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        log::warn!("failed to finalize scan artifact for {function_name}: {e}");
        return;
    }
    log::info!(
        "Wrote scan artifact for {} -> {}",
        function_name,
        path.display()
    );
}

fn write_completed_scan_artifact(
    artifact_root: Option<&PathBuf>,
    current: usize,
    total: usize,
    file_path: &str,
    result: &FunctionResult,
) {
    let Some(root) = artifact_root else {
        return;
    };
    let function_report = crate::report::build_function_report(result, file_path);
    let value = serde_json::json!({
        "version": 1,
        "status": "completed",
        "current": current,
        "total": total,
        "function": function_report,
    });
    write_scan_artifact_json(root, current, &result.function_name, &value);
}

fn write_skipped_scan_artifact(
    artifact_root: Option<&PathBuf>,
    current: usize,
    total: usize,
    function_name: &str,
    reason: &str,
    category: SkipCategory,
) {
    let Some(root) = artifact_root else {
        return;
    };
    let value = serde_json::json!({
        "version": 1,
        "status": "skipped",
        "current": current,
        "total": total,
        "function_name": function_name,
        "reason": reason,
        "category": match category {
            SkipCategory::Expected => "expected",
            SkipCategory::Unsupported => "unsupported",
            SkipCategory::Error => "error",
        },
    });
    write_scan_artifact_json(root, current, function_name, &value);
}

fn write_failed_scan_artifact(
    artifact_root: Option<&PathBuf>,
    current: usize,
    total: usize,
    function_name: &str,
    reason: &str,
) {
    let Some(root) = artifact_root else {
        return;
    };
    let value = serde_json::json!({
        "version": 1,
        "status": "failed",
        "current": current,
        "total": total,
        "function_name": function_name,
        "reason": reason,
    });
    write_scan_artifact_json(root, current, function_name, &value);
}

// ---------------------------------------------------------------------------
// Scan summary artifact
// ---------------------------------------------------------------------------

/// Status of the overall scan run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRunStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
    /// The source set changed during the run (paths added, removed, or
    /// modified between manifest capture and end-of-run validation).
    /// The summary's `source_diff` lists which paths drifted (str-jeen.3).
    StaleSourceSet,
}

/// Per-function entry in the scan summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummaryEntry {
    pub function_name: String,
    pub status: String,
    /// 1-based index in exploration order.
    pub index: usize,
    /// Relative path to the per-function artifact file (if written).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
    /// Reason for skip or failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Top-level scan summary artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub version: u32,
    pub scan_id: String,
    pub status: ScanRunStatus,
    pub total_functions: usize,
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub elapsed_secs: f64,
    pub functions: Vec<ScanSummaryEntry>,
    /// End-of-run source-set drift. `Some` when the scan finalizer ran
    /// the run-manifest validation step; `None` when validation was
    /// skipped (e.g. interrupted scan, no manifest captured). See
    /// `crate::run_manifest` (str-jeen.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_diff: Option<crate::run_manifest::ManifestDiff>,
}

const SCAN_SUMMARY_VERSION: u32 = 1;
const SCAN_SUMMARY_FILENAME: &str = "summary.json";

/// Write the scan summary to `<scan_root>/summary.json` using atomic rename.
fn write_scan_summary(scan_root: &Path, summary: &ScanSummary) {
    let path = scan_root.join(SCAN_SUMMARY_FILENAME);
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("failed to create scan summary dir: {e}");
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(summary) else {
        log::warn!("failed to serialize scan summary");
        return;
    };
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        log::warn!("failed to write scan summary temp file: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        log::warn!("failed to finalize scan summary: {e}");
        return;
    }
    log::debug!("Updated scan summary -> {}", path.display());
}

fn write_scan_status(
    scan_root: &Path,
    summary: &ScanSummary,
    manifest: &crate::run_manifest::RunManifest,
    files: &[StatusFileInput],
    targets: &[StatusTargetInput],
) {
    let manifest_path = scan_root.join(crate::run_manifest::RUN_MANIFEST_FILENAME);
    let summary_path = scan_root.join(SCAN_SUMMARY_FILENAME);
    let artifacts = [StatusArtifactLink {
        kind: "scan_summary",
        path: &summary_path,
    }];
    if let Err(e) = crate::status_export::write_run_status_json(
        scan_root,
        &StatusExportInput {
            command: "scan",
            manifest,
            manifest_path: &manifest_path,
            artifacts: &artifacts,
            files,
            targets,
            rollups: status_rollup_input_from_scan_summary(summary),
        },
    ) {
        log::warn!("failed to write scan status export: {e}");
    }
}

fn status_rollup_input_from_scan_summary(summary: &ScanSummary) -> StatusRollupInput {
    let mut validity_reasons = Vec::new();
    let report_validity = match summary.status {
        ScanRunStatus::Running | ScanRunStatus::Completed => StatusReportValidity::High,
        ScanRunStatus::Failed | ScanRunStatus::Interrupted => StatusReportValidity::Low,
        ScanRunStatus::StaleSourceSet => {
            if let Some(diff) = summary.source_diff.as_ref() {
                if !diff.added.is_empty() {
                    validity_reasons.push(StatusValidityReason {
                        code: "stale_source_set_added".to_string(),
                        detail: format!("{} source path(s) added after manifest capture", diff.added.len()),
                        recommended_action:
                            "Re-run on a quiesced source tree so the manifest snapshot reflects the explored set."
                                .to_string(),
                    });
                }
                if !diff.removed.is_empty() {
                    validity_reasons.push(StatusValidityReason {
                        code: "stale_source_set_removed".to_string(),
                        detail: format!(
                            "{} source path(s) removed after manifest capture",
                            diff.removed.len()
                        ),
                        recommended_action:
                            "Re-run on a quiesced source tree; removed files invalidate per-file buckets."
                                .to_string(),
                    });
                }
                if !diff.changed.is_empty() {
                    validity_reasons.push(StatusValidityReason {
                        code: "stale_source_set_changed".to_string(),
                        detail: format!(
                            "{} source path(s) changed content during run",
                            diff.changed.len()
                        ),
                        recommended_action:
                            "Re-run on a quiesced source tree; mid-run edits make line buckets unreliable."
                                .to_string(),
                    });
                }
            }
            StatusReportValidity::StaleSourceSet
        }
    };

    StatusRollupInput {
        report_validity: Some(report_validity),
        validity_reasons,
        line_weighted_failure_impact: None,
        gate_decisions: None,
    }
}

#[derive(Debug, Default)]
struct StatusFileCounts {
    discovered: u64,
    attempted: u64,
    completed: u64,
    failed: u64,
    unsupported: u64,
}

fn status_file_inputs_from_scan_summary(
    summary: &ScanSummary,
    file_map: &HashMap<String, String>,
) -> Vec<StatusFileInput> {
    let mut by_path: BTreeMap<String, StatusFileCounts> = BTreeMap::new();
    for entry in &summary.functions {
        let Some(path) = file_map.get(&entry.function_name) else {
            continue;
        };
        let counts = by_path.entry(path.clone()).or_default();
        counts.discovered += 1;
        match entry.status.as_str() {
            "completed" => {
                counts.attempted += 1;
                counts.completed += 1;
            }
            "failed" => {
                counts.attempted += 1;
                counts.failed += 1;
            }
            "skipped" if entry.reason.as_deref().is_some_and(is_unsupported_reason) => {
                counts.unsupported += 1;
            }
            "skipped" => {}
            _ => {}
        }
    }

    by_path
        .into_iter()
        .map(|(path, counts)| StatusFileInput {
            path,
            discovered_targets: counts.discovered,
            attempted_targets: counts.attempted,
            completed_targets: counts.completed,
            failed_targets: counts.failed,
            unsupported_targets: counts.unsupported,
            status: status_file_status_from_counts(&counts),
        })
        .collect()
}

fn is_unsupported_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("unsupported") || lower.contains("unexecutable")
}

fn status_file_status_from_counts(counts: &StatusFileCounts) -> StatusFileStatus {
    if counts.discovered == 0 {
        StatusFileStatus::NoTarget
    } else if counts.completed == counts.discovered {
        StatusFileStatus::Completed
    } else if counts.completed > 0 {
        StatusFileStatus::Partial
    } else if counts.failed > 0 {
        StatusFileStatus::Failed
    } else if counts.unsupported > 0 {
        StatusFileStatus::Unsupported
    } else {
        StatusFileStatus::Skipped
    }
}

fn status_target_inputs_from_scan_summary(
    scan_root: &Path,
    summary: &ScanSummary,
    file_map: &HashMap<String, String>,
    analyses: &[FunctionAnalysis],
) -> Vec<StatusTargetInput> {
    let spans_by_name: HashMap<&str, &FunctionAnalysis> = analyses
        .iter()
        .map(|analysis| (analysis.name.as_str(), analysis))
        .collect();

    summary
        .functions
        .iter()
        .map(|entry| {
            let analysis = spans_by_name.get(entry.function_name.as_str()).copied();
            let source_file = file_map
                .get(&entry.function_name)
                .cloned()
                .or_else(|| analysis.and_then(|analysis| analysis.source_file.clone()))
                .unwrap_or_default();
            let (outcome, validity_impact) = status_target_outcome(entry);
            StatusTargetInput {
                target_id: entry.function_name.clone(),
                name: entry.function_name.clone(),
                source_file,
                start_line: analysis.map_or(0, |analysis| analysis.start_line),
                end_line: analysis.map_or(0, |analysis| analysis.end_line),
                outcome,
                artifact_path: entry
                    .artifact
                    .as_ref()
                    .map(|artifact| scan_root.join(artifact)),
                failure_reason: entry.reason.clone(),
                unavailable_reason: entry
                    .artifact
                    .is_none()
                    .then(|| target_unavailable_reason(entry)),
                validity_impact,
            }
        })
        .collect()
}

fn status_target_outcome(
    entry: &ScanSummaryEntry,
) -> (StatusTargetOutcome, StatusTargetValidityImpact) {
    match entry.status.as_str() {
        "completed" => (
            StatusTargetOutcome::Completed,
            StatusTargetValidityImpact::Contributes,
        ),
        "failed" if entry.reason.as_deref().is_some_and(is_timeout_reason) => (
            StatusTargetOutcome::TimedOut,
            StatusTargetValidityImpact::Degrades,
        ),
        "failed" => (
            StatusTargetOutcome::Failed,
            StatusTargetValidityImpact::Degrades,
        ),
        "skipped" if entry.reason.as_deref().is_some_and(is_unsupported_reason) => (
            StatusTargetOutcome::Unsupported,
            StatusTargetValidityImpact::Excluded,
        ),
        "skipped"
            if entry
                .reason
                .as_deref()
                .is_some_and(is_unavailable_frontend_reason) =>
        {
            (
                StatusTargetOutcome::UnavailableFrontend,
                StatusTargetValidityImpact::Degrades,
            )
        }
        "skipped" => (
            StatusTargetOutcome::Skipped,
            StatusTargetValidityImpact::Excluded,
        ),
        _ => (
            StatusTargetOutcome::Failed,
            StatusTargetValidityImpact::Degrades,
        ),
    }
}

fn is_timeout_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
}

fn is_unavailable_frontend_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("frontend") || lower.contains("preflight")
}

fn target_unavailable_reason(entry: &ScanSummaryEntry) -> String {
    entry
        .reason
        .clone()
        .unwrap_or_else(|| "target artifact unavailable".to_string())
}

/// Create an initial summary with status `Running` and no function entries.
fn new_scan_summary(scan_id: &str, total_functions: usize) -> ScanSummary {
    ScanSummary {
        version: SCAN_SUMMARY_VERSION,
        scan_id: scan_id.to_string(),
        status: ScanRunStatus::Running,
        total_functions,
        completed: 0,
        failed: 0,
        skipped: 0,
        elapsed_secs: 0.0,
        functions: Vec::new(),
        source_diff: None,
    }
}

/// Record a completed function in the summary.
fn summary_record_completed(
    summary: &mut ScanSummary,
    function_name: &str,
    index: usize,
    elapsed: Duration,
) {
    summary.completed += 1;
    summary.elapsed_secs = elapsed.as_secs_f64();
    let artifact_filename = format!(
        "{:05}_{}.json",
        index,
        sanitize_artifact_component(function_name)
    );
    summary.functions.push(ScanSummaryEntry {
        function_name: function_name.to_string(),
        status: "completed".to_string(),
        index,
        artifact: Some(format!("functions/{artifact_filename}")),
        reason: None,
    });
}

/// Record a skipped function in the summary.
fn summary_record_skipped(
    summary: &mut ScanSummary,
    function_name: &str,
    index: usize,
    reason: &str,
    category: SkipCategory,
    elapsed: Duration,
) {
    summary.elapsed_secs = elapsed.as_secs_f64();
    match category {
        SkipCategory::Expected | SkipCategory::Unsupported | SkipCategory::Error => {
            summary.skipped += 1
        }
    }
    let artifact_filename = format!(
        "{:05}_{}.json",
        index,
        sanitize_artifact_component(function_name)
    );
    summary.functions.push(ScanSummaryEntry {
        function_name: function_name.to_string(),
        status: "skipped".to_string(),
        index,
        artifact: Some(format!("functions/{artifact_filename}")),
        reason: Some(reason.to_string()),
    });
}

/// Record a failed function in the summary.
fn summary_record_failed(
    summary: &mut ScanSummary,
    function_name: &str,
    index: usize,
    reason: &str,
    elapsed: Duration,
) {
    summary.failed += 1;
    summary.elapsed_secs = elapsed.as_secs_f64();
    let artifact_filename = format!(
        "{:05}_{}.json",
        index,
        sanitize_artifact_component(function_name)
    );
    summary.functions.push(ScanSummaryEntry {
        function_name: function_name.to_string(),
        status: "failed".to_string(),
        index,
        artifact: Some(format!("functions/{artifact_filename}")),
        reason: Some(reason.to_string()),
    });
}

/// Finalize the summary status based on outcomes.
fn summary_finalize(summary: &mut ScanSummary, elapsed: Duration) {
    summary.elapsed_secs = elapsed.as_secs_f64();
    if summary.status == ScanRunStatus::Interrupted {
        return;
    }
    summary.status = if summary.failed > 0 && summary.completed == 0 {
        ScanRunStatus::Failed
    } else {
        ScanRunStatus::Completed
    };
}

/// Finalize the summary and run end-of-run source-set validation.
///
/// `current_paths` is the deduplicated set of source paths the scan
/// actually used — typically the values of `ScanConfig::file_map`. When
/// the diff against `manifest` shows any drift, the summary status is
/// promoted to [`ScanRunStatus::StaleSourceSet`] so a long run that
/// silently raced concurrent edits is no longer reported as a clean
/// completion (str-jeen.3).
fn summary_finalize_with_manifest_check(
    summary: &mut ScanSummary,
    elapsed: Duration,
    manifest: &crate::run_manifest::RunManifest,
    current_paths: &[String],
) {
    summary_finalize(summary, elapsed);
    let diff = crate::run_manifest::diff_against(manifest, current_paths);
    if diff.is_stale() {
        log::warn!(
            "scan {} ended with stale source set: +{} added, -{} removed, ~{} changed",
            summary.scan_id,
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len(),
        );
        summary.status = ScanRunStatus::StaleSourceSet;
    }
    summary.source_diff = Some(diff);
}

/// Build the deduplicated, sorted set of source paths from a `file_map`.
fn dedup_source_paths(file_map: &HashMap<String, String>) -> Vec<String> {
    let mut paths: Vec<String> = file_map.values().cloned().collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Build a `ScanSummary` retroactively from a finished [`ScanResult`].
///
/// Used by the non-parallel `scan()` path which doesn't incrementally update
/// the summary during execution.
fn build_summary_from_scan_result(
    scan_id: &str,
    result: &ScanResult,
    elapsed: Duration,
) -> ScanSummary {
    let total = result.function_results.len() + result.skipped_functions.len();
    let mut summary = new_scan_summary(scan_id, total);
    let mut index = 0usize;
    // Function results and skipped functions are in test order.
    // Interleave them by maintaining separate iterators.
    let mut skip_iter = result.skipped_functions.iter().peekable();
    for fr in &result.function_results {
        // Emit any skipped functions that appear before this result in test order.
        while let Some(sf) = skip_iter.peek() {
            if result
                .test_order
                .iter()
                .position(|n| n == &sf.function_name)
                <= result
                    .test_order
                    .iter()
                    .position(|n| n == &fr.function_name)
            {
                let sf = skip_iter.next().expect("peeked");
                index += 1;
                summary_record_skipped(
                    &mut summary,
                    &sf.function_name,
                    index,
                    &sf.reason,
                    sf.category,
                    elapsed,
                );
            } else {
                break;
            }
        }
        index += 1;
        summary_record_completed(&mut summary, &fr.function_name, index, elapsed);
    }
    // Remaining skipped functions.
    for sf in skip_iter {
        index += 1;
        summary_record_skipped(
            &mut summary,
            &sf.function_name,
            index,
            &sf.reason,
            sf.category,
            elapsed,
        );
    }
    summary_finalize(&mut summary, elapsed);
    summary
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
    caller_inputs_and_results: &[(
        Vec<serde_json::Value>,
        Vec<crate::protocol::MockConfig>,
        crate::protocol::ExecuteResult,
    )],
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
    Some(crate::fingerprint::compute_function_fingerprint(
        &source, analysis,
    ))
}

/// Compute a hash of the scan configuration fields that affect exploration
/// behavior (iterations, timeouts, parallelism, isolation). Used for soft
/// config drift detection on resume — a mismatch warns but does not
/// invalidate the checkpoint.
fn scan_config_hash(config: &ScanConfig) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"scan_config_v1:");
    hasher.update(config.max_iterations_per_function.to_le_bytes());
    hasher.update(config.timeout_per_fn.as_secs().to_le_bytes());
    if let Some(t) = config.timeout_total {
        hasher.update(t.as_secs().to_le_bytes());
    }
    hasher.update(config.parallelism.to_le_bytes());
    if let Some(t) = config.timeout_explore {
        hasher.update(t.as_secs().to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Compute scan ID from the qualified target set in a ScanConfig.
fn compute_scan_id(config: &ScanConfig) -> String {
    let targets: Vec<(&str, &str)> = config
        .file_map
        .iter()
        .map(|(qualified_id, file_path)| (qualified_id.as_str(), file_path.as_str()))
        .collect();
    crate::checkpoint::ScanCheckpoint::compute_scan_id_for_targets(&targets)
}

/// Load an existing checkpoint or create a fresh one. Checks compatibility
/// (hard: scan_id mismatch → discard) and config drift (soft: warning only).
fn load_or_create_checkpoint(
    resume_path: Option<&Path>,
    scan_id: &str,
    config_hash: &str,
) -> crate::checkpoint::ScanCheckpoint {
    let Some(path) = resume_path else {
        return crate::checkpoint::ScanCheckpoint::new_with_config(
            scan_id.to_string(),
            config_hash.to_string(),
        );
    };

    match crate::checkpoint::ScanCheckpoint::load(path) {
        Ok(Some(cp)) => {
            if let Some(reason) = cp.check_compatibility(scan_id) {
                log::info!("{reason}, starting fresh");
                return crate::checkpoint::ScanCheckpoint::new_with_config(
                    scan_id.to_string(),
                    config_hash.to_string(),
                );
            }
            if let Some(drift) = cp.check_config_drift(config_hash) {
                log::warn!("{drift}");
            }
            cp
        }
        Ok(None) => crate::checkpoint::ScanCheckpoint::new_with_config(
            scan_id.to_string(),
            config_hash.to_string(),
        ),
        Err(e) => {
            log::warn!("failed to load checkpoint: {e}, starting fresh");
            crate::checkpoint::ScanCheckpoint::new_with_config(
                scan_id.to_string(),
                config_hash.to_string(),
            )
        }
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

    // Flatten test order entries into layers for stratum filtering.
    let all_layers = build_layers(&order_entries, &call_graph);

    // Apply stratum filter: only explore functions in selected layers.
    let (filtered_layers, stratum_excluded) = if let Some(ref spec) = config.stratum {
        let max_layer = if all_layers.is_empty() {
            0
        } else {
            all_layers.len() - 1
        };
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

    // str-fuhw: key analysis lookups by the same qualified ID that
    // `behavior::CallGraph::from_analyses` uses as `function_id`. Without
    // matching key formats every test_order entry would miss the map for
    // analyses with `source_file` populated (the production path).
    let analysis_qids: Vec<String> = analyses
        .iter()
        .map(crate::behavior::node_id_for_analysis)
        .collect();
    let analysis_map: HashMap<&str, &FunctionAnalysis> = analysis_qids
        .iter()
        .zip(analyses.iter())
        .map(|(qid, a)| (qid.as_str(), a))
        .collect();

    let mut behavior_maps: HashMap<String, BehaviorMap> = HashMap::new();
    let mut function_results: Vec<FunctionResult> = Vec::new();
    let mut skipped_functions: Vec<SkippedFunction> = Vec::new();
    let mut deep_fingerprints: HashMap<String, String> = HashMap::new();

    // Load checkpoint for resume support.
    let scan_id = compute_scan_id(config);
    let cfg_hash = scan_config_hash(config);
    let mut checkpoint =
        load_or_create_checkpoint(config.resume_path.as_deref(), &scan_id, &cfg_hash);

    // Load the interesting input pool for cross-function seed sharing.
    let mut input_pool = config
        .pool_path
        .as_ref()
        .and_then(|p| interesting_pool::load_pool(p).ok().flatten())
        .unwrap_or_default();
    input_pool.epoch += 1;

    let scan_start = Instant::now();

    // str-jeen.3: capture run-start manifest for end-of-run drift check.
    let manifest_source_paths = dedup_source_paths(&config.file_map);
    let project_root_path = config.project_root.as_deref().map(Path::new);
    let scan_root_dir = scan_root(config.project_root.as_deref(), &scan_id);
    let run_manifest = crate::run_manifest::capture(
        &scan_id,
        &cfg_hash,
        &manifest_source_paths,
        project_root_path,
    );
    if config.write_artifacts {
        crate::run_manifest::write_manifest(&scan_root_dir, &run_manifest);
    }

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
        // str-fuhw: iterate over `callees` (qualified IDs from the call graph)
        // rather than `analysis.dependencies` (bare `dep.symbol`) so the
        // prefetch stores entries under the same key the mocking step
        // looks up at line 1255 below. Cache lookups continue to use the
        // qualified ID; on-disk cache layout invalidates on first run
        // after the str-fuhw upgrade (intentional).
        if let Some(ref cache) = config.cache {
            for callee in &callees {
                if !behavior_maps.contains_key(callee)
                    && let Ok(Some(cached)) = cache.load(callee)
                {
                    behavior_maps.insert(callee.clone(), cached);
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

        let file = config.file_map.get(func_name).cloned().unwrap_or_default();

        let pool_seeds = crate::input_gen::pool_to_candidate_inputs_for_callees(
            &analysis.params,
            &input_pool,
            &callees,
        );

        let config_function_inputs = load_config_function_inputs(
            analysis,
            func_name,
            &config.config_dir,
            config.max_iterations_per_function,
            config.timeout_per_fn.as_secs(),
        );
        let mut candidate_inputs = config_function_inputs.candidate_inputs;
        // Extend with cached seeds from prior exploration runs.
        if let Some(ref cache) = config.cache
            && let Ok(Some(cached_map)) = cache.load(func_name)
        {
            let cached_seeds = cached_map.extract_seed_inputs();
            if !cached_seeds.is_empty() {
                log::debug!(
                    "[scan] Loaded {} cached seed(s) for {}",
                    cached_seeds.len(),
                    func_name,
                );
                candidate_inputs.extend(cached_seeds);
            }
        }

        // str-jeen.50: consult the planner before building ExploreConfig so
        // method targets carry a `default_execute_plan` into the executor —
        // see `fetch_default_execute_plan_for_method` for the no-op cases.
        let default_execute_plan = fetch_default_execute_plan_for_method(
            frontend,
            analysis,
            &file,
            config.project_root.as_deref(),
        )
        .await;

        let explore_config = ExploreConfig {
            file,
            max_iterations: Some(config.max_iterations_per_function),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: config.seed,
            mocks,
            mock_params: vec![],
            setup_file: None,
            setup_level: crate::protocol::SetupLevel::Function,
            value_sources: config_function_inputs.value_sources,
            capabilities: crate::orchestrator::FrontendCapabilities::from_raw(
                frontend.capabilities(),
            ),
            user_seeds: vec![],
            candidate_inputs,
            pool_seeds,
            project_root: config.project_root.clone(),
            // str-0x82: derive the execution profile from the analysis's
            // invocation model so adapter targets (e.g. go/http-handler) get
            // the correct profile in execute requests.
            execution_profile: execution_profile_from_analysis(analysis),
            loop_buckets: explorer::LoopBuckets::default(),
            timeout_explore: config.timeout_explore,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: crate::orchestrator::DEFAULT_SHRINK_BUDGET,
            isolation: config.isolation,
            capture_side_effects: config.capture_side_effects,
            budget_surplus: None,
            claim_policy: ClaimPolicy::default(),
            planner: None,
            default_execute_plan,
            prepare_id_override: None,
        };

        let exploration =
            explore_with_scan_mode(frontend, analysis, config.concolic, &explore_config).await?;

        // Harvest interesting inputs into the cross-function pool.
        interesting_pool::harvest_from_exploration(
            &mut input_pool,
            &exploration.raw_results,
            &analysis.params,
            func_name,
            interesting_pool::CoverageMode::Branch,
        );

        // Genetic algorithm follow-up phase: target unsolved branches.
        let mut ga_discoveries: Vec<crate::behavior::Behavior> = Vec::new();
        if config.genetic_config.enabled {
            let targets = crate::coverage_metrics::extract_targets(analysis, &exploration);
            if !targets.is_empty() {
                log::info!(
                    "[scan] Starting GA for {} ({} unsolved target(s))",
                    func_name,
                    targets.len(),
                );
                let mut seed_inputs: Vec<Vec<serde_json::Value>> = exploration
                    .raw_results
                    .iter()
                    .map(|(inputs, _, _)| inputs.clone())
                    .collect();
                // Extend GA seeds with cached inputs from prior runs.
                if let Some(ref cache) = config.cache
                    && let Ok(Some(cached_map)) = cache.load(func_name)
                {
                    seed_inputs.extend(cached_map.extract_seed_inputs());
                }
                match crate::genetic_explorer::genetic_explore(
                    frontend,
                    func_name,
                    seed_inputs,
                    targets,
                    &analysis.params,
                    &config.genetic_config,
                )
                .await
                {
                    Ok(ga_result) => {
                        if !ga_result.discoveries.is_empty() {
                            log::info!(
                                "[scan] GA found {} new behavior(s) for {}",
                                ga_result.discoveries.len(),
                                func_name,
                            );
                        }
                        ga_discoveries = ga_result.discoveries;
                    }
                    Err(e) => {
                        log::warn!("[scan] GA error for {}: {e}", func_name);
                    }
                }
            }
        }

        // Run the Analyze stage to produce behavior map and coverage metrics.
        let mut analyze_out = analyze_exploration(&exploration, analysis, current_deep_fp.clone());

        // Merge GA discoveries into the behavior map before caching.
        if !ga_discoveries.is_empty() {
            let added = analyze_out
                .behavior_map
                .merge_ga_discoveries(&ga_discoveries);
            if added > 0 {
                log::info!(
                    "[scan] Merged {added} GA behavior(s) into behavior map for {func_name}"
                );
            }
        }

        // Persist the behavior map to cache for reuse across runs.
        if let Some(ref cache) = config.cache {
            let _ = cache.store(&analyze_out.behavior_map);
        }
        // Persist input vectors to the signature-keyed store so they
        // survive body edits that would drop the behavior map (str-bo4z.3).
        persist_stored_inputs(
            config.stored_inputs_cache.as_deref(),
            analysis,
            &analyze_out.behavior_map,
        );

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
                mock_misses
                    .iter()
                    .map(|m| &m.callee_name)
                    .collect::<HashSet<_>>()
                    .len(),
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

    let result = ScanResult {
        function_results,
        test_order,
        skipped_functions,
        sampling: None,
        source_files: run_manifest.source_files.clone(),
    };

    // Write the scan summary artifact, with end-of-run source-set
    // validation (str-jeen.3).
    let mut summary = build_summary_from_scan_result(&scan_id, &result, scan_start.elapsed());
    // build_summary_from_scan_result already calls summary_finalize
    // internally; layer the manifest diff on top.
    let diff = crate::run_manifest::diff_against(&run_manifest, &manifest_source_paths);
    if diff.is_stale() {
        log::warn!(
            "scan {} ended with stale source set: +{} added, -{} removed, ~{} changed",
            summary.scan_id,
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len(),
        );
        summary.status = ScanRunStatus::StaleSourceSet;
    }
    summary.source_diff = Some(diff);
    if config.write_artifacts {
        write_scan_summary(&scan_root_dir, &summary);
        let status_files = status_file_inputs_from_scan_summary(&summary, &config.file_map);
        let status_targets = status_target_inputs_from_scan_summary(
            &scan_root_dir,
            &summary,
            &config.file_map,
            analyses,
        );
        write_scan_status(
            &scan_root_dir,
            &summary,
            &run_manifest,
            &status_files,
            &status_targets,
        );
    }

    Ok(result)
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
    /// Source-file snapshots from the run-start manifest (str-jeen.60/63).
    /// Used by `generate_report` to build `SourceSetSummary` from the
    /// discovered source set rather than from completed function rows.
    /// Empty when the scan ran without a manifest (e.g. synthetic test runs).
    pub source_files: Vec<crate::run_manifest::SourceFileSnapshot>,
}

impl ParallelScanResult {
    /// Returns true if the scan should be considered a failure:
    /// functions were attempted but none were successfully explored.
    pub fn has_scan_failure(&self) -> bool {
        let attempted = self.function_results.len() + self.skipped.len();
        attempted > 0 && self.function_results.is_empty()
    }

    /// Bucket the scan outcome into counts the CLI summary and exit policy
    /// reason about (str-izhn).
    #[must_use]
    pub fn counts(&self) -> ScanCounts {
        let mut failed = 0;
        let mut unsupported = 0;
        let mut expected = 0;
        for s in &self.skipped {
            match s.category {
                SkipCategory::Error => failed += 1,
                SkipCategory::Unsupported => unsupported += 1,
                SkipCategory::Expected => expected += 1,
            }
        }
        ScanCounts {
            completed: self.function_results.len(),
            failed,
            unsupported,
            expected_skips: expected,
        }
    }

    /// Apply a [`ScanFailurePolicy`] to the result and return the reason the
    /// scan should exit nonzero, or `None` if the policy is satisfied.
    ///
    /// The default policy is permissive (returns `None` for partial failures)
    /// to preserve backwards-compatible exit codes; CI/workflows opt in via
    /// `--fail-on-failures` or `--failure-threshold` (str-izhn).
    #[must_use]
    pub fn evaluate_failure_policy(&self, policy: ScanFailurePolicy) -> Option<String> {
        let counts = self.counts();
        let attempted = counts.completed + counts.failed;
        if policy.fail_on_failures && counts.failed > 0 {
            return Some(format!(
                "{} of {} attempted function(s) failed (--fail-on-failures)",
                counts.failed, attempted,
            ));
        }
        if let Some(threshold) = policy.failure_threshold_percent
            && attempted > 0
        {
            let pct = (counts.failed as f64 / attempted as f64) * 100.0;
            if pct > threshold as f64 {
                return Some(format!(
                    "failure rate {:.1}% ({} of {} attempted) exceeds --failure-threshold {}%",
                    pct, counts.failed, attempted, threshold,
                ));
            }
        }
        None
    }
}

/// Counts surfaced in the scan summary and exit-policy decision (str-izhn).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanCounts {
    /// Functions that were attempted and explored successfully.
    pub completed: usize,
    /// Functions attempted but skipped with [`SkipCategory::Error`] —
    /// timeouts, exploration errors, crashes, observed target panics.
    pub failed: usize,
    /// Functions discovered but never attempted because their shape is not
    /// supported (unexecutable parameters, etc.).
    pub unsupported: usize,
    /// Benign skips: cache hits, checkpoint resumes, intentional bypasses.
    pub expected_skips: usize,
}

/// CLI policy for translating scan outcomes into process exit codes
/// (str-izhn). Default is permissive: exit 0 unless **every** attempted
/// function failed (the prior `has_scan_failure` rule still applies).
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanFailurePolicy {
    /// If true, exit nonzero when any attempted function fails.
    pub fail_on_failures: bool,
    /// If set, exit nonzero when the failure rate (failed / attempted) is
    /// strictly greater than this percentage (0..=100).
    pub failure_threshold_percent: Option<u32>,
}

impl ScanFailurePolicy {
    /// Build a policy from the single `--fail-on-failures[=PERCENT]` CLI
    /// flag (str-izhn). `None` means the flag was omitted (permissive);
    /// `Some(0)` means the flag was set without a value (fail on any
    /// failure); `Some(n)` means fail when the failure rate exceeds `n%`.
    ///
    /// The flag is fused into a single arg so the clap-derived parser stays
    /// inside its default test-thread stack budget — adding a second arg
    /// tipped the `try_parse_from` error path over 2MB on small fixtures.
    #[must_use]
    pub fn from_cli_flag(value: Option<u32>) -> Self {
        match value {
            None => Self::default(),
            Some(0) => Self {
                fail_on_failures: true,
                failure_threshold_percent: None,
            },
            Some(pct) => Self {
                fail_on_failures: false,
                failure_threshold_percent: Some(pct),
            },
        }
    }
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
            sender
                .send(fe)
                .await
                .expect("channel has capacity for prewarmed worker");
            1
        } else {
            0
        };
        for _ in already..initial {
            let frontend = Frontend::spawn(&config).await?;
            sender
                .send(frontend)
                .await
                .expect("channel has capacity for initial workers");
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
            && self
                .live_count
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

    /// Replace a poisoned/dead checked-out worker, or account for the dead slot
    /// if replacement is unnecessary or spawning fails.
    async fn replace_dead_worker_if_needed(&self, pending: usize) {
        if !self.needs_replacement(pending) {
            self.reap_dead_slot();
            return;
        }

        match Frontend::spawn(&self.config).await {
            Ok(new_fe) => self.return_or_reap_worker(new_fe, pending).await,
            Err(_) => self.reap_dead_slot(),
        }
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
        if self
            .live_count
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

/// Per-task cleanup state for a checked-out shared-pool worker.
///
/// The outer task watchdog can abort a worker future before it reaches the
/// normal "return or replace frontend" block. This lease lets the join side
/// recover pool capacity exactly once so later queued tasks don't wait forever
/// on a slot held by the aborted future.
struct WorkerTaskLease {
    pool: Arc<WorkerPool>,
    tasks_remaining: Arc<AtomicUsize>,
    checked_out: AtomicBool,
    finished: AtomicBool,
    pool_accounted: AtomicBool,
}

impl WorkerTaskLease {
    fn new(pool: Arc<WorkerPool>, tasks_remaining: Arc<AtomicUsize>) -> Self {
        Self {
            pool,
            tasks_remaining,
            checked_out: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            pool_accounted: AtomicBool::new(false),
        }
    }

    fn mark_checked_out(&self) {
        self.checked_out.store(true, Ordering::Release);
    }

    fn finish_once(&self) -> usize {
        if self.finished.swap(true, Ordering::AcqRel) {
            self.tasks_remaining.load(Ordering::Acquire)
        } else {
            self.tasks_remaining
                .fetch_sub(1, Ordering::AcqRel)
                .saturating_sub(1)
        }
    }

    async fn account_live_worker(&self, frontend: Frontend, pending: usize) {
        if !self.pool_accounted.swap(true, Ordering::AcqRel) {
            self.pool.return_or_reap_worker(frontend, pending).await;
        }
    }

    async fn account_dead_worker_if_checked_out(&self, pending: usize) {
        if self.checked_out.load(Ordering::Acquire)
            && !self.pool_accounted.swap(true, Ordering::AcqRel)
        {
            self.pool.replace_dead_worker_if_needed(pending).await;
        }
    }

    async fn recover_after_abort(&self) -> usize {
        let remaining = self.finish_once();
        self.account_dead_worker_if_checked_out(remaining).await;
        self.pool.maybe_grow(remaining);
        remaining
    }
}

/// Execute a layer of function tasks using round-robin batch scheduling.
///
/// Each function is explored one at a time for a fixed number of iterations
/// (the batch size). Non-exhausted functions are re-enqueued for another
/// batch. A single frontend process is reused across all batches.
///
/// This mode is activated when `ScanConfig::batch_size` is `Some`. It
/// provides the scheduling primitive for continuous explore runs: downstream
/// features (reranking, cooldown, persistence) hook into the batch boundary.
#[allow(clippy::too_many_arguments)]
async fn run_layer_batched(
    fe_config: Arc<FrontendConfig>,
    tasks: Vec<ExploreTask>,
    batch_size: u32,
    concolic: bool,
    timeout: Duration,
    build_timeout: Duration,
    cache: &Option<Arc<BehaviorMapCache>>,
    scheduler_state_cache: &Option<Arc<crate::cache::SchedulerStateCache>>,
    behavior_maps: &Arc<Mutex<HashMap<String, BehaviorMap>>>,
    input_pool: &Arc<Mutex<InterestingPool>>,
    genetic_config: &crate::config::GeneticConfig,
    progress_handler: Option<ProgressHandler>,
    artifact_root: Option<Arc<PathBuf>>,
    total_functions: usize,
    scan_start: Instant,
    scheduler_mode: &str,
) -> Vec<FunctionOutcome> {
    use crate::batch_scheduler::{
        BatchOutcome, BatchScheduler, CoverageCounts, WorkerBatchSummary,
    };
    use crate::cache::SchedulerState;

    let task_count = tasks.len();
    // Each function's per-batch budget is capped at batch_size; the total
    // budget comes from the original ExploreConfig.max_iterations.
    let per_function_budgets: Vec<Option<u32>> = tasks
        .iter()
        .map(|t| t.explore_config.max_iterations)
        .collect();

    let mut scheduler = BatchScheduler::with_individual_budgets(&per_function_budgets, batch_size);

    // Per-task live scheduler state accumulator. Populated from disk on
    // layer entry (advisory — load failures become fresh state) and
    // flushed to disk when a function exhausts or the layer ends.
    //
    // Each entry is initialized with the task's current `deep_fp` so any
    // flush — fresh function, evicted-then-reset function, or final
    // layer-end flush — stamps the on-disk record with a fingerprint that
    // future runs can compare against. The body-change invalidation hook
    // (str-bo4z.2) below relies on this stamp.
    let mut live_states: Vec<SchedulerState> = tasks
        .iter()
        .map(|t| SchedulerState {
            function_id: t.func_name.clone(),
            fingerprint: t.deep_fp.clone(),
            mode: Some(scheduler_mode.to_string()),
            ..SchedulerState::default()
        })
        .collect();
    let mut persisted_flags: Vec<bool> = vec![false; task_count];

    // str-b2my.14: per-task accumulator for computing batch summary deltas.
    struct BatchAccumulator {
        last_coverage: CoverageCounts,
        cumulative_unique_paths: usize,
        cumulative_behaviors: usize,
    }
    let mut accumulators: Vec<BatchAccumulator> = tasks
        .iter()
        .map(|t| BatchAccumulator {
            last_coverage: CoverageCounts {
                total_branches: t.analysis.branches.len(),
                uncovered: t.analysis.branches.len(),
                ..CoverageCounts::default()
            },
            cumulative_unique_paths: 0,
            cumulative_behaviors: 0,
        })
        .collect();

    if let Some(ssc) = scheduler_state_cache.as_ref() {
        for (idx, task) in tasks.iter().enumerate() {
            // str-bo4z.2: when we know the current function fingerprint,
            // route through `load_if_fresh` so a body change clears the
            // persisted record and the function returns to the queue as
            // effectively unexplored. Tasks without a deep_fp (legacy /
            // uninstrumented call sites) fall back to `load` so we don't
            // regress behavior that pre-dated fingerprint propagation.
            let result = match task.deep_fp.as_deref() {
                Some(fp) => ssc.load_if_fresh(&task.func_name, scheduler_mode, fp),
                None => ssc.load(&task.func_name, scheduler_mode),
            };
            match result {
                Ok(Some(prior)) => {
                    log::debug!(
                        "scheduler-state cache hit for {}: iterations_consumed={}, batches_completed={}, exhausted={}",
                        task.func_name,
                        prior.iterations_consumed,
                        prior.batches_completed,
                        prior.exhausted,
                    );
                    live_states[idx] = prior;
                    live_states[idx].function_id = task.func_name.clone();
                    // Restamp with the current task fingerprint so the
                    // next persist captures it. (When deep_fp is Some,
                    // load_if_fresh guarantees the loaded record's
                    // fingerprint already equals deep_fp; the explicit
                    // assignment keeps the invariant in one place.)
                    live_states[idx].fingerprint = task.deep_fp.clone();
                }
                Ok(None) => {
                    log::debug!(
                        "scheduler-state cache miss or stale fingerprint for {} — starting fresh",
                        task.func_name,
                    );
                }
                Err(e) => {
                    log::warn!(
                        "scheduler-state cache load error for {}: {e}",
                        task.func_name,
                    );
                }
            }
        }
    }

    let mut outcomes: Vec<Option<FunctionOutcome>> = (0..task_count).map(|_| None).collect();

    // Spawn a single frontend for the batched loop. Wrapped in Option
    // so we can replace it after timeout/death without ownership issues.
    let mut frontend: Option<Frontend> = match Frontend::spawn(&fe_config).await {
        Ok(fe) => Some(fe),
        Err(e) => {
            for (i, task) in tasks.iter().enumerate() {
                outcomes[i] = Some(FunctionOutcome::Error {
                    function_name: task.func_name.clone(),
                    error: format!("failed to spawn frontend: {e}"),
                });
            }
            return outcomes.into_iter().flatten().collect();
        }
    };

    while let Some(batch_config) = scheduler.next_batch() {
        let task = &tasks[batch_config.task_index];

        // Ensure we have a live frontend.
        let needs_respawn = match frontend.as_mut() {
            Some(fe) => !fe.is_alive(),
            None => true,
        };
        if needs_respawn {
            drop(frontend.take());
            match Frontend::spawn(&fe_config).await {
                Ok(fe) => frontend = Some(fe),
                Err(e) => {
                    scheduler.record_outcome(BatchOutcome {
                        task_index: batch_config.task_index,
                        iterations_used: 0,
                        exhausted: true,
                        rank: 0,
                        summary: None,
                    });
                    {
                        let st = &mut live_states[batch_config.task_index];
                        st.exhausted = true;
                    }
                    persist_scheduler_state_if_exhausted(
                        scheduler_state_cache,
                        &live_states[batch_config.task_index],
                        &mut persisted_flags[batch_config.task_index],
                        scheduler_mode,
                    );
                    outcomes[batch_config.task_index] = Some(FunctionOutcome::Error {
                        function_name: task.func_name.clone(),
                        error: format!("frontend respawn failed: {e}"),
                    });
                    break;
                }
            }
        }
        let fe = frontend.as_mut().expect("frontend must be live here");

        emit_progress(
            progress_handler.as_ref(),
            &task.func_name,
            task.progress_index,
            total_functions,
            scan_start.elapsed(),
            ScanProgressStatus::Started,
        );

        // Adjust the explore config's iteration cap to the batch size.
        let mut batch_explore_config = task.explore_config.clone();
        batch_explore_config.max_iterations = Some(batch_config.batch_size);

        let result = run_phased(
            fe,
            &task.func_name,
            &task.analysis,
            concolic,
            &batch_explore_config,
            &task.mocks_used,
            &task.callees,
            behavior_maps,
            task.deep_fp.clone(),
            input_pool,
            genetic_config,
            cache,
            build_timeout,
            timeout,
        )
        .await;

        // If the frontend timed out or died, mark it for replacement
        // on the next iteration.
        let timed_out = matches!(
            result,
            PhasedOutcome::BuildTimedOut(_) | PhasedOutcome::ExploreTimedOut(_)
        );
        // str-quhk: also drop tainted or frontend-error frontends to
        // prevent cascading Timeout/IdMismatch across subsequent tasks.
        let frontend_error = matches!(
            result,
            PhasedOutcome::Failed(ScanError::Explore(
                ExploreError::Frontend(_),
            )) | PhasedOutcome::Failed(ScanError::Frontend(_))
        );
        let poisoned = fe.is_tainted() || frontend_error;
        if timed_out || !fe.is_alive() || poisoned {
            drop(frontend.take());
        }

        match result {
            PhasedOutcome::Success(func_result) => {
                let iterations_used = func_result.exploration.iterations;

                if let Some(ref artifact_root) = artifact_root {
                    write_completed_scan_artifact(
                        Some(artifact_root),
                        task.progress_index,
                        total_functions,
                        &task.file_path,
                        &func_result,
                    );
                }
                emit_progress(
                    progress_handler.as_ref(),
                    &task.func_name,
                    task.progress_index,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Completed,
                );

                {
                    let mut maps = behavior_maps.lock().await;
                    maps.insert(
                        func_result.function_name.clone(),
                        func_result.behavior_map.clone(),
                    );
                }
                if let Some(c) = cache {
                    let _ = c.store(&func_result.behavior_map);
                }

                // str-bo4z.6: compute uncovered branches before moving func_result.
                let uncovered =
                    compute_uncovered_branch_strings(&task.analysis, &func_result.exploration);
                let has_uncovered = !uncovered.is_empty();
                if has_uncovered {
                    log::debug!(
                        "{}: {} uncovered branch(es) remain after batch {}",
                        task.func_name,
                        uncovered.len(),
                        batch_config.batch_number,
                    );
                }

                // A function is exhausted only if it under-consumed its batch
                // AND has no remaining uncovered branches to target. Functions
                // with uncovered targets stay alive for further scheduling.
                let exhausted = iterations_used < batch_config.batch_size && !has_uncovered;

                // str-b2my.14: build worker summary before func_result is moved.
                let batch_summary = {
                    let acc = &accumulators[batch_config.task_index];
                    let coverage_after = CoverageCounts::from(&func_result.coverage_metrics);
                    let new_classes = func_result
                        .exploration
                        .unique_paths
                        .saturating_sub(acc.cumulative_unique_paths);
                    let new_retained = func_result
                        .behavior_map
                        .behaviors
                        .len()
                        .saturating_sub(acc.cumulative_behaviors);
                    let failures = func_result
                        .exploration
                        .raw_results
                        .iter()
                        .filter(|(_, _, r)| r.thrown_error.is_some())
                        .count();
                    WorkerBatchSummary {
                        executions_run: iterations_used,
                        coverage_before: acc.last_coverage.clone(),
                        coverage_after,
                        uncovered_remaining: uncovered.len(),
                        new_classes,
                        new_retained_inputs: new_retained,
                        failures,
                    }
                };

                // Update accumulator for next batch.
                {
                    let acc = &mut accumulators[batch_config.task_index];
                    acc.last_coverage = batch_summary.coverage_after.clone();
                    acc.cumulative_unique_paths = func_result.exploration.unique_paths;
                    acc.cumulative_behaviors = func_result.behavior_map.behaviors.len();
                }

                outcomes[batch_config.task_index] =
                    Some(FunctionOutcome::Success(func_result));

                scheduler.record_outcome(BatchOutcome {
                    task_index: batch_config.task_index,
                    iterations_used,
                    exhausted,
                    rank: 0,
                    summary: Some(batch_summary),
                });
                {
                    let st = &mut live_states[batch_config.task_index];
                    st.iterations_consumed = st.iterations_consumed.saturating_add(iterations_used);
                    st.batches_completed = st.batches_completed.saturating_add(1);
                    st.exhausted = exhausted;
                    st.uncovered_branches = uncovered;
                }
                persist_scheduler_state_if_exhausted(
                    scheduler_state_cache,
                    &live_states[batch_config.task_index],
                    &mut persisted_flags[batch_config.task_index],
                    scheduler_mode,
                );
            }
            PhasedOutcome::Failed(e) => {
                let unsupported_reason = match &e {
                    ScanError::Explore(ExploreError::Unsupported(msg)) => Some(msg.clone()),
                    _ => None,
                };
                let reason = match &unsupported_reason {
                    Some(msg) => format!("unsupported: {msg}"),
                    None => format!("error: {e}"),
                };
                if let Some(ref artifact_root) = artifact_root {
                    write_failed_scan_artifact(
                        Some(artifact_root),
                        task.progress_index,
                        total_functions,
                        &task.func_name,
                        &reason,
                    );
                }
                emit_progress(
                    progress_handler.as_ref(),
                    &task.func_name,
                    task.progress_index,
                    total_functions,
                    scan_start.elapsed(),
                    if unsupported_reason.is_some() {
                        ScanProgressStatus::Skipped
                    } else {
                        ScanProgressStatus::Failed
                    },
                );

                outcomes[batch_config.task_index] = Some(match unsupported_reason {
                    Some(msg) => FunctionOutcome::Unsupported {
                        function_name: task.func_name.clone(),
                        reason: msg,
                    },
                    None => FunctionOutcome::Error {
                        function_name: task.func_name.clone(),
                        error: e.to_string(),
                    },
                });

                scheduler.record_outcome(BatchOutcome {
                    task_index: batch_config.task_index,
                    iterations_used: 0,
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
                {
                    let st = &mut live_states[batch_config.task_index];
                    st.exhausted = true;
                }
                persist_scheduler_state_if_exhausted(
                    scheduler_state_cache,
                    &live_states[batch_config.task_index],
                    &mut persisted_flags[batch_config.task_index],
                    scheduler_mode,
                );
            }
            outcome @ (PhasedOutcome::BuildTimedOut(_) | PhasedOutcome::ExploreTimedOut(_)) => {
                let (phase, d) = match outcome {
                    PhasedOutcome::BuildTimedOut(d) => ("build", d),
                    PhasedOutcome::ExploreTimedOut(d) => ("execution", d),
                    _ => unreachable!(),
                };
                let reason = phase_timeout_reason(phase, d);
                if let Some(ref artifact_root) = artifact_root {
                    write_failed_scan_artifact(
                        Some(artifact_root),
                        task.progress_index,
                        total_functions,
                        &task.func_name,
                        &reason,
                    );
                }
                emit_progress(
                    progress_handler.as_ref(),
                    &task.func_name,
                    task.progress_index,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Failed,
                );

                outcomes[batch_config.task_index] = Some(FunctionOutcome::Timeout {
                    function_name: task.func_name.clone(),
                    limit: d,
                    phase,
                });

                scheduler.record_outcome(BatchOutcome {
                    task_index: batch_config.task_index,
                    iterations_used: 0,
                    exhausted: true,
                    rank: 0,
                    summary: None,
                });
                {
                    let st = &mut live_states[batch_config.task_index];
                    st.exhausted = true;
                }
                persist_scheduler_state_if_exhausted(
                    scheduler_state_cache,
                    &live_states[batch_config.task_index],
                    &mut persisted_flags[batch_config.task_index],
                    scheduler_mode,
                );
            }
        }
    }

    // Shut down the dedicated frontend if it's still alive.
    if let Some(fe) = frontend {
        let _ = fe.shutdown().await;
    }

    // Flush remaining live scheduler states for functions with uncovered
    // branches. Fully-covered functions (empty uncovered_branches) age out
    // — their state is not persisted, so future runs treat them as done.
    // str-bo4z.6: only uncovered targets are retained durably.
    if let Some(ssc) = scheduler_state_cache.as_ref() {
        for (idx, persisted) in persisted_flags.iter().enumerate() {
            if !persisted
                && !live_states[idx].uncovered_branches.is_empty()
                && let Err(e) = ssc.store(&live_states[idx], scheduler_mode)
            {
                log::warn!(
                    "scheduler-state cache store error for {}: {e}",
                    live_states[idx].function_id,
                );
            }
        }
    }

    outcomes.into_iter().flatten().collect()
}

/// Persist scheduler state to the advisory on-disk cache if this
/// function has just hit exhaustion. Idempotent per task — once
/// `persisted` is `true`, further calls are no-ops. Store errors are
/// logged and swallowed: scheduler state is advisory.
fn persist_scheduler_state_if_exhausted(
    scheduler_state_cache: &Option<Arc<crate::cache::SchedulerStateCache>>,
    state: &crate::cache::SchedulerState,
    persisted: &mut bool,
    mode: &str,
) {
    if *persisted || !state.exhausted {
        return;
    }
    let Some(ssc) = scheduler_state_cache.as_ref() else {
        return;
    };
    match ssc.store(state, mode) {
        Ok(()) => {
            *persisted = true;
            log::debug!(
                "scheduler-state persisted for {} (iterations={}, batches={})",
                state.function_id,
                state.iterations_consumed,
                state.batches_completed,
            );
        }
        Err(e) => {
            log::warn!(
                "scheduler-state cache store error for {}: {e}",
                state.function_id,
            );
        }
    }
}

/// Compute uncovered branch identifiers as strings for scheduler state persistence.
///
/// Each entry is formatted as `"{branch_id}:{line}"` for human readability
/// and machine parseability. Only branches with [`TargetReason::Uncovered`] are
/// included — opaque-constraint branches were technically discovered, just not
/// fully solved, so they don't count as "uncovered targets" for scheduling.
fn compute_uncovered_branch_strings(
    analysis: &crate::protocol::FunctionAnalysis,
    exploration: &crate::explorer::ObservationOutput,
) -> Vec<String> {
    let targets = crate::coverage_metrics::extract_targets(analysis, exploration);
    targets
        .into_iter()
        .filter(|t| t.reason == crate::coverage_metrics::TargetReason::Uncovered)
        .map(|t| format!("{}:{}", t.branch_id, t.line))
        .collect()
}

async fn explore_with_scan_mode(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    concolic: bool,
    explore_config: &ExploreConfig,
) -> Result<crate::explorer::ObservationOutput, ScanError> {
    if !concolic {
        return Ok(
            explorer::explore_function(frontend, analysis, explore_config, None, None).await?,
        );
    }

    if let Err(e) = frontend
        .send(crate::protocol::Command::Instrument {
            file: explore_config.file.clone(),
            function: analysis.name.clone(),
            mocks: explore_config.mocks.clone(),
            project_root: explore_config.project_root.clone(),
            execution_profile: explore_config.execution_profile.clone(),
        })
        .await
    {
        log::debug!("scan concolic instrument failed for {}: {e}", analysis.name);
    }

    let capabilities = crate::orchestrator::FrontendCapabilities::from_raw(frontend.capabilities());
    let prepare_id = if explore_config.prepare_id_override.is_some() {
        explore_config.prepare_id_override.clone()
    } else if capabilities.commands.contains("prepare") {
        match frontend
            .send(crate::protocol::Command::Prepare {
                file: explore_config.file.clone(),
                function: analysis.name.clone(),
                mocks: explore_config.mocks.clone(),
                project_root: explore_config.project_root.clone(),
                execution_profile: explore_config.execution_profile.clone(),
                plan: explore_config.default_execute_plan.clone(),
            })
            .await
        {
            Ok(resp) => match resp.result {
                crate::protocol::ResponseResult::Prepare { prepare_id } => Some(prepare_id),
                other => {
                    log::debug!(
                        "scan concolic prepare returned unexpected response for {}: {other:?}",
                        analysis.name
                    );
                    None
                }
            },
            Err(e) => {
                log::debug!("scan concolic prepare failed for {}: {e}", analysis.name);
                None
            }
        }
    } else {
        None
    };

    let mut seed_inputs = crate::boundary_dict::generate_boundary_inputs(&analysis.params);
    seed_inputs.extend(explore_config.pool_seeds.clone());
    let mut user_inputs = explore_config.user_seeds.clone();
    user_inputs.extend(explore_config.candidate_inputs.clone());

    let max_iterations = explore_config.max_iterations.unwrap_or(100) as usize;
    let concolic_config = crate::orchestrator::ExploreConfig {
        max_iterations: Some(max_iterations),
        max_executions: Some(max_iterations * 5),
        plateau_threshold: 20,
        mocks: explore_config.mocks.clone(),
        mock_params: explore_config.mock_params.clone(),
        solver_timeout_ms: None,
        seed: explore_config.seed,
        solver_offload: true,
        timeout_explore: explore_config.timeout_explore,
        branch_profile: None,
        meta_config: explore_config.meta_config.clone(),
        execution_profile: explore_config.execution_profile.clone(),
        loop_convergence_window: 3,
        refine_budget: None,
        shrink_budget: explore_config.shrink_budget,
        mcdc: false,
        fuzz: crate::config::FuzzConfig::default(),
        planner: explore_config.planner.clone(),
        default_execute_plan: explore_config.default_execute_plan.clone(),
    };
    let (mut result, _state) = crate::orchestrator::explore(
        frontend,
        &analysis.name,
        seed_inputs,
        user_inputs,
        &analysis.params,
        &concolic_config,
        None,
        prepare_id,
        analysis.loops.clone(),
        None,
        None,
    )
    .await?;
    result.total_lines = analysis.end_line.saturating_sub(analysis.start_line) + 1;
    Ok(result.into())
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
    concolic: bool,
    timeout: Duration,
    build_timeout: Duration,
    cache: &Option<Arc<BehaviorMapCache>>,
    stored_inputs_cache: &Option<Arc<StoredInputsCache>>,
    behavior_maps: &Arc<Mutex<HashMap<String, BehaviorMap>>>,
    input_pool: &Arc<Mutex<InterestingPool>>,
    genetic_config: &crate::config::GeneticConfig,
    progress_handler: Option<ProgressHandler>,
    artifact_root: Option<Arc<PathBuf>>,
    total_functions: usize,
    scan_start: Instant,
) -> (Vec<FunctionOutcome>, usize) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    for ExploreTask {
        func_name,
        analysis,
        explore_config,
        file_path: _,
        mocks_used,
        callees,
        deep_fp,
        progress_index,
        known_targets: _,
    } in tasks
    {
        let semaphore = Arc::clone(&semaphore);
        let fe_config = Arc::clone(&fe_config);
        let behavior_maps = Arc::clone(behavior_maps);
        let input_pool = Arc::clone(input_pool);
        let cache = cache.clone();
        let stored_inputs_cache = stored_inputs_cache.clone();
        let genetic_config = genetic_config.clone();
        let progress_handler = progress_handler.clone();
        let artifact_root = artifact_root.clone();
        let file_path = explore_config.file.clone();

        let handle = tokio::spawn(async move {
            // Acquire a concurrency slot before spawning the frontend.
            let _permit = semaphore
                .acquire()
                .await
                .expect("semaphore is never closed");
            emit_progress(
                progress_handler.as_ref(),
                &func_name,
                progress_index,
                total_functions,
                scan_start.elapsed(),
                ScanProgressStatus::Started,
            );

            let mut frontend = match Frontend::spawn(&fe_config).await {
                Ok(fe) => fe,
                Err(e) => {
                    emit_progress(
                        progress_handler.as_ref(),
                        &func_name,
                        progress_index,
                        total_functions,
                        scan_start.elapsed(),
                        ScanProgressStatus::Failed,
                    );
                    return FunctionOutcome::Error {
                        function_name: func_name,
                        error: e.to_string(),
                    };
                }
            };

            let result = run_phased(
                &mut frontend,
                &func_name,
                &analysis,
                concolic,
                &explore_config,
                &mocks_used,
                &callees,
                &behavior_maps,
                deep_fp,
                &input_pool,
                &genetic_config,
                &cache,
                build_timeout,
                timeout,
            )
            .await;

            // Always shut down the dedicated frontend — never return to a pool.
            let _ = frontend.shutdown().await;

            match result {
                PhasedOutcome::Success(func_result) => {
                    let mut maps = behavior_maps.lock().await;
                    maps.insert(func_name.clone(), func_result.behavior_map.clone());
                    drop(maps);
                    if let Some(ref cache) = cache {
                        let _ = cache.store(&func_result.behavior_map);
                    }
                    persist_stored_inputs(
                        stored_inputs_cache.as_deref(),
                        &analysis,
                        &func_result.behavior_map,
                    );
                    write_completed_scan_artifact(
                        artifact_root.as_deref(),
                        progress_index,
                        total_functions,
                        &file_path,
                        &func_result,
                    );
                    emit_progress(
                        progress_handler.as_ref(),
                        &func_name,
                        progress_index,
                        total_functions,
                        scan_start.elapsed(),
                        ScanProgressStatus::Completed,
                    );
                    FunctionOutcome::Success(func_result)
                }
                PhasedOutcome::Failed(e) => {
                    let unsupported_reason = match &e {
                        ScanError::Explore(ExploreError::Unsupported(msg)) => Some(msg.clone()),
                        _ => None,
                    };
                    let reason = match &unsupported_reason {
                        Some(msg) => format!("unsupported: {msg}"),
                        None => format!("error: {e}"),
                    };
                    write_failed_scan_artifact(
                        artifact_root.as_deref(),
                        progress_index,
                        total_functions,
                        &func_name,
                        &reason,
                    );
                    emit_progress(
                        progress_handler.as_ref(),
                        &func_name,
                        progress_index,
                        total_functions,
                        scan_start.elapsed(),
                        if unsupported_reason.is_some() {
                            ScanProgressStatus::Skipped
                        } else {
                            ScanProgressStatus::Failed
                        },
                    );
                    match unsupported_reason {
                        Some(msg) => FunctionOutcome::Unsupported {
                            function_name: func_name,
                            reason: msg,
                        },
                        None => FunctionOutcome::Error {
                            function_name: func_name,
                            error: e.to_string(),
                        },
                    }
                }
                outcome @ (PhasedOutcome::BuildTimedOut(_) | PhasedOutcome::ExploreTimedOut(_)) => {
                    let (phase, d) = match outcome {
                        PhasedOutcome::BuildTimedOut(d) => ("build", d),
                        PhasedOutcome::ExploreTimedOut(d) => ("execution", d),
                        _ => unreachable!(),
                    };
                    let reason = phase_timeout_reason(phase, d);
                    write_failed_scan_artifact(
                        artifact_root.as_deref(),
                        progress_index,
                        total_functions,
                        &func_name,
                        &reason,
                    );
                    emit_progress(
                        progress_handler.as_ref(),
                        &func_name,
                        progress_index,
                        total_functions,
                        scan_start.elapsed(),
                        ScanProgressStatus::Failed,
                    );
                    FunctionOutcome::Timeout {
                        function_name: func_name,
                        limit: d,
                        phase,
                    }
                }
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

/// Concrete branch targets known to be uncovered at enqueue time.
///
/// Computed during discovery from static analysis (all branches on first run)
/// or by diffing against prior exploration results (continuous mode).
/// Functions are only scheduled when they have non-empty known targets,
/// preventing speculative exploration of fully-covered functions.
#[derive(Debug, Clone)]
pub(crate) struct KnownTargets {
    /// Branch IDs known to be uncovered.
    pub branch_ids: Vec<u32>,
    /// Maximum estimated nesting depth among the uncovered targets.
    /// Conservative upper bound from static analysis — refined at runtime
    /// by `branch_depth()` from execution traces.
    pub max_nesting_depth: u32,
}

/// Estimate the maximum nesting depth for a set of target branches.
///
/// Since [`BranchInfo`] doesn't carry explicit nesting depth, this uses a
/// conservative heuristic: for each target branch, count the number of
/// flow-control branches (If, While, For, Switch, Select) with strictly
/// earlier line numbers. The maximum across all targets is returned.
///
/// This is an upper bound — it counts all preceding flow-control constructs,
/// not just enclosing ones. Accurate depth requires per-branch end-line info
/// (a future frontend enhancement).
fn estimate_nesting_depth(branches: &[BranchInfo], target_ids: &[u32]) -> u32 {
    if target_ids.is_empty() {
        return 0;
    }
    let target_set: HashSet<u32> = target_ids.iter().copied().collect();

    let mut max_depth = 0u32;
    for &target_id in &target_set {
        if let Some(target) = branches.iter().find(|b| b.id == target_id) {
            let depth = branches
                .iter()
                .filter(|b| {
                    b.id != target_id
                        && b.line < target.line
                        && matches!(
                            b.branch_type,
                            BranchType::If
                                | BranchType::While
                                | BranchType::For
                                | BranchType::Switch
                                | BranchType::Select
                        )
                })
                .count() as u32;
            max_depth = max_depth.max(depth);
        }
    }
    max_depth
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
    file_path: String,
    mocks_used: Vec<MockUsage>,
    callees: std::collections::HashSet<String>,
    deep_fp: Option<String>,
    progress_index: usize,
    /// Concrete uncovered branch targets this task is scheduled to cover.
    known_targets: KnownTargets,
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
            FunctionOutcome::Timeout {
                ref function_name, ..
            }
            | FunctionOutcome::Error {
                ref function_name, ..
            }
            | FunctionOutcome::TotalTimeout {
                ref function_name, ..
            }
            | FunctionOutcome::Unsupported {
                ref function_name, ..
            } => {
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
fn merge_replica_results(
    replicas: Vec<FunctionResult>,
    analysis: &FunctionAnalysis,
) -> FunctionResult {
    use crate::explorer::ObservationOutput;

    debug_assert!(
        !replicas.is_empty(),
        "merge_replica_results: replicas must not be empty"
    );

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

    // Build a merged ObservationOutput and re-analyze it once.
    // pipeline::analyze handles input-hash deduplication inside
    // BehaviorMap::from_records and uses analysis.branches.len() for
    // accurate CoverageMetrics.total_branches.
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
        abandoned_frontiers: vec![],
        opaque_suggestions: vec![],
        stubbed_modules: merged_stubbed,
        ..Default::default()
    };

    let analyze_out = analyze_exploration(&merged_exploration, analysis, fingerprint);

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
    parallel_scan_with_progress(frontend_config, analyses, config, None).await
}

pub async fn parallel_scan_with_progress(
    frontend_config: &FrontendConfig,
    analyses: &[FunctionAnalysis],
    config: &ScanConfig,
    progress_handler: Option<ProgressHandler>,
) -> Result<ParallelScanResult, ScanError> {
    let call_graph = CallGraph::from_analyses(analyses);
    let order_entries = call_graph.test_order()?;

    // Flatten test order into layers. Each layer contains functions whose
    // callees are all in previous layers.
    let all_layers = build_layers(&order_entries, &call_graph);

    // Apply stratum filter: only explore functions in selected layers.
    let (layers, stratum_excluded) = if let Some(ref spec) = config.stratum {
        let max_layer = if all_layers.is_empty() {
            0
        } else {
            all_layers.len() - 1
        };
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

    // str-fuhw: key by qualified ID so dup-named analyses across files
    // don't collide. See identical site in `scan` for rationale.
    let analysis_qids: Vec<String> = analyses
        .iter()
        .map(crate::behavior::node_id_for_analysis)
        .collect();
    let analysis_map: HashMap<&str, &FunctionAnalysis> = analysis_qids
        .iter()
        .zip(analyses.iter())
        .map(|(qid, a)| (qid.as_str(), a))
        .collect();

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
    let scan_id = compute_scan_id(config);
    let cfg_hash = scan_config_hash(config);
    let mut checkpoint =
        load_or_create_checkpoint(config.resume_path.as_deref(), &scan_id, &cfg_hash);

    let mut all_results: Vec<FunctionResult> = Vec::new();
    let mut test_order: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedFunction> = Vec::new();

    let scan_start = Instant::now();
    let scan_deadline = total_deadline(scan_start, config.timeout_total);
    let total_functions = analyses.len();
    let mut progress_index = 0usize;
    // When `config.write_artifacts` is false, every project-local artifact
    // path is suppressed: the per-function `Option<Arc<PathBuf>>` is `None`
    // (which the per-function helpers treat as a no-op), and every
    // `write_scan_summary`/`write_manifest` call is gated on the same flag
    // (str-1wcl).
    let write_artifacts = config.write_artifacts;
    let artifact_root: Option<Arc<PathBuf>> = write_artifacts
        .then(|| Arc::new(scan_artifact_root(config.project_root.as_deref(), &scan_id)));
    let scan_root_dir = scan_root(config.project_root.as_deref(), &scan_id);
    let mut summary = new_scan_summary(&scan_id, total_functions);
    // Gating closure used by every summary write in this function so that
    // `--no-cache --no-seeds + -o <external>` runs leave nothing under
    // `<project>/shatter-artifacts/` (str-1wcl).
    let maybe_write_summary = |s: &ScanSummary| {
        if write_artifacts {
            write_scan_summary(&scan_root_dir, s);
        }
    };
    maybe_write_summary(&summary);

    // str-jeen.3: capture run-start source snapshot for end-of-run drift
    // detection. The manifest lives next to the summary so external tooling
    // can audit which source set produced the report.
    let manifest_source_paths = dedup_source_paths(&config.file_map);
    let project_root_path = config.project_root.as_deref().map(Path::new);
    let run_manifest = crate::run_manifest::capture(
        &scan_id,
        &cfg_hash,
        &manifest_source_paths,
        project_root_path,
    );
    if write_artifacts {
        crate::run_manifest::write_manifest(&scan_root_dir, &run_manifest);
    }

    for (layer_idx, layer) in layers.iter().enumerate() {
        // Check total scan timeout at layer boundary.
        if let Some(deadline) = scan_deadline
            && Instant::now() >= deadline
        {
            // Skip all functions in this and remaining layers.
            for remaining_layer in &layers[layer_idx..] {
                for func_name in remaining_layer {
                    progress_index += 1;
                    skipped.push(SkippedFunction {
                        function_name: func_name.clone(),
                        reason: TOTAL_SCAN_TIMEOUT_REASON.into(),
                        category: SkipCategory::Error,
                    });
                    write_skipped_scan_artifact(
                        artifact_root.as_deref(),
                        progress_index,
                        total_functions,
                        func_name,
                        TOTAL_SCAN_TIMEOUT_REASON,
                        SkipCategory::Error,
                    );
                    summary_record_skipped(
                        &mut summary,
                        func_name,
                        progress_index,
                        TOTAL_SCAN_TIMEOUT_REASON,
                        SkipCategory::Error,
                        scan_start.elapsed(),
                    );
                    emit_progress(
                        progress_handler.as_ref(),
                        func_name,
                        progress_index,
                        total_functions,
                        scan_start.elapsed(),
                        ScanProgressStatus::Skipped,
                    );
                }
            }
            summary.status = ScanRunStatus::Interrupted;
            maybe_write_summary(&summary);
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
            progress_index += 1;
            let current_progress = progress_index;
            test_order.push(func_name.clone());

            let analysis = match analysis_map.get(func_name.as_str()) {
                Some(a) => *a,
                None => {
                    skipped.push(SkippedFunction {
                        function_name: func_name.clone(),
                        reason: "no analysis found".into(),
                        category: SkipCategory::Error,
                    });
                    write_skipped_scan_artifact(
                        artifact_root.as_deref(),
                        current_progress,
                        total_functions,
                        func_name,
                        "no analysis found",
                        SkipCategory::Error,
                    );
                    summary_record_skipped(
                        &mut summary,
                        func_name,
                        current_progress,
                        "no analysis found",
                        SkipCategory::Error,
                        scan_start.elapsed(),
                    );
                    maybe_write_summary(&summary);
                    emit_progress(
                        progress_handler.as_ref(),
                        func_name,
                        current_progress,
                        total_functions,
                        scan_start.elapsed(),
                        ScanProgressStatus::Skipped,
                    );
                    continue;
                }
            };

            let config_function_inputs = load_config_function_inputs(
                analysis,
                func_name,
                &config.config_dir,
                config.max_iterations_per_function,
                config.timeout_per_fn.as_secs(),
            );
            if config_function_inputs.skip {
                skipped.push(SkippedFunction {
                    function_name: func_name.clone(),
                    reason: "skip=true in config".into(),
                    category: SkipCategory::Expected,
                });
                write_skipped_scan_artifact(
                    artifact_root.as_deref(),
                    current_progress,
                    total_functions,
                    func_name,
                    "skip=true in config",
                    SkipCategory::Expected,
                );
                summary_record_skipped(
                    &mut summary,
                    func_name,
                    current_progress,
                    "skip=true in config",
                    SkipCategory::Expected,
                    scan_start.elapsed(),
                );
                maybe_write_summary(&summary);
                emit_progress(
                    progress_handler.as_ref(),
                    func_name,
                    current_progress,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Skipped,
                );
                continue;
            }

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
                let mut maps = behavior_maps.lock().await;
                maps.insert(func_name.clone(), cached_map);
                drop(maps);
                layer_deep_fps.push((func_name.clone(), dfp.clone()));
                skipped.push(SkippedFunction {
                    function_name: func_name.clone(),
                    reason: "resumed from checkpoint".into(),
                    category: SkipCategory::Expected,
                });
                write_skipped_scan_artifact(
                    artifact_root.as_deref(),
                    current_progress,
                    total_functions,
                    func_name,
                    "resumed from checkpoint",
                    SkipCategory::Expected,
                );
                summary_record_skipped(
                    &mut summary,
                    func_name,
                    current_progress,
                    "resumed from checkpoint",
                    SkipCategory::Expected,
                    scan_start.elapsed(),
                );
                maybe_write_summary(&summary);
                emit_progress(
                    progress_handler.as_ref(),
                    func_name,
                    current_progress,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Skipped,
                );
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
                write_skipped_scan_artifact(
                    artifact_root.as_deref(),
                    current_progress,
                    total_functions,
                    func_name,
                    "unchanged (fingerprint match)",
                    SkipCategory::Expected,
                );
                summary_record_skipped(
                    &mut summary,
                    func_name,
                    current_progress,
                    "unchanged (fingerprint match)",
                    SkipCategory::Expected,
                    scan_start.elapsed(),
                );
                maybe_write_summary(&summary);
                emit_progress(
                    progress_handler.as_ref(),
                    func_name,
                    current_progress,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Skipped,
                );
                continue;
            }

            // Compute known uncovered targets. On first exploration all
            // branches are targets; on subsequent runs subtract already-covered
            // branches recovered from the scheduler state cache.
            let all_branch_ids: Vec<u32> = analysis.branches.iter().map(|b| b.id).collect();
            let covered_ids: HashSet<u32> = if !all_branch_ids.is_empty() {
                let all_ids: HashSet<u32> = all_branch_ids.iter().copied().collect();
                if let Some(ssc) = &config.scheduler_state_cache {
                    match ssc.load(func_name, config.coverage_mode.as_str()) {
                        Ok(Some(ref prior))
                            if prior.exhausted && prior.uncovered_branches.is_empty() =>
                        {
                            all_ids // All branches covered in prior run.
                        }
                        Ok(Some(ref prior)) if !prior.uncovered_branches.is_empty() => {
                            let still_uncovered: HashSet<u32> = prior
                                .uncovered_branches
                                .iter()
                                .filter_map(|s| s.split(':').next()?.parse().ok())
                                .collect();
                            all_ids.difference(&still_uncovered).copied().collect()
                        }
                        _ => HashSet::new(),
                    }
                } else {
                    HashSet::new()
                }
            } else {
                HashSet::new()
            };
            let uncovered_ids: Vec<u32> = all_branch_ids
                .iter()
                .filter(|id| !covered_ids.contains(id))
                .copied()
                .collect();

            // Skip functions whose branches are all covered — no speculative work.
            // Functions with no branches still get explored (they have execution
            // behavior worth recording as behavior maps).
            if uncovered_ids.is_empty() && !analysis.branches.is_empty() {
                log::debug!(
                    "{}: all {} branch(es) covered — skipping",
                    func_name,
                    analysis.branches.len(),
                );
                skipped.push(SkippedFunction {
                    function_name: func_name.clone(),
                    reason: "all branches covered".into(),
                    category: SkipCategory::Expected,
                });
                write_skipped_scan_artifact(
                    artifact_root.as_deref(),
                    current_progress,
                    total_functions,
                    func_name,
                    "all branches covered",
                    SkipCategory::Expected,
                );
                summary_record_skipped(
                    &mut summary,
                    func_name,
                    current_progress,
                    "all branches covered",
                    SkipCategory::Expected,
                    scan_start.elapsed(),
                );
                maybe_write_summary(&summary);
                emit_progress(
                    progress_handler.as_ref(),
                    func_name,
                    current_progress,
                    total_functions,
                    scan_start.elapsed(),
                    ScanProgressStatus::Skipped,
                );
                continue;
            }

            let known_targets = KnownTargets {
                max_nesting_depth: estimate_nesting_depth(&analysis.branches, &uncovered_ids),
                branch_ids: uncovered_ids,
            };
            log::debug!(
                "{}: enqueuing with {} known target(s), max nesting depth {}",
                func_name,
                known_targets.branch_ids.len(),
                known_targets.max_nesting_depth,
            );

            // Try loading cached behavior maps for callees not yet in memory.
            // str-fuhw: iterate over `callees` (qualified IDs from the
            // call graph) instead of `analysis.dependencies` (bare
            // `dep.symbol`) so prefetched entries land under the same
            // key the mocking step looks up later. Cache hits are
            // best-effort; on-disk layout invalidates on first run after
            // the str-fuhw upgrade.
            if let Some(ref cache) = config.cache {
                let mut maps = behavior_maps.lock().await;
                for callee in &callees {
                    if !maps.contains_key(callee)
                        && let Ok(Some(cached)) = cache.load(callee)
                    {
                        maps.insert(callee.clone(), cached);
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

            let file = config.file_map.get(func_name).cloned().unwrap_or_default();

            let pool_seeds = {
                let pool_guard = input_pool.lock().await;
                crate::input_gen::pool_to_candidate_inputs_for_callees(
                    &analysis.params,
                    &pool_guard,
                    &callees,
                )
            };

            let mut candidate_inputs = config_function_inputs.candidate_inputs;
            // Extend with cached seeds from prior exploration runs.
            if let Some(ref cache) = config.cache
                && let Ok(Some(cached_map)) = cache.load(func_name)
            {
                let cached_seeds = cached_map.extract_seed_inputs();
                if !cached_seeds.is_empty() {
                    log::debug!(
                        "[scan] Loaded {} cached seed(s) for {}",
                        cached_seeds.len(),
                        func_name,
                    );
                    candidate_inputs.extend(cached_seeds);
                }
            }

            let explore_config = ExploreConfig {
                file: file.clone(),
                max_iterations: Some(config.max_iterations_per_function),
                observer_pool: 1,
                observer_frontend_config: None,
                candidate_queue_capacity: None,
                seed: config.seed,
                mocks,
                mock_params: vec![],
                setup_file: None,
                setup_level: crate::protocol::SetupLevel::Function,
                value_sources: config_function_inputs.value_sources,
                capabilities: config.capabilities.clone(),
                user_seeds: vec![],
                candidate_inputs,
                pool_seeds,
                project_root: config.project_root.clone(),
                // str-0x82: derive execution profile from invocation model
                // (parallel path — mirrors serial path).
                execution_profile: execution_profile_from_analysis(analysis),
                loop_buckets: explorer::LoopBuckets::default(),
                timeout_explore: config.timeout_explore,
                meta_config: crate::strategy::MetaConfig::default(),
                shrink_budget: crate::orchestrator::DEFAULT_SHRINK_BUDGET,
                isolation: config.isolation,
                capture_side_effects: config.capture_side_effects,
                budget_surplus: Some(Arc::clone(&layer_surplus)),
                claim_policy: ClaimPolicy::default(),
                planner: None,
                default_execute_plan: None,
                prepare_id_override: None,
            };

            tasks.push(ExploreTask {
                func_name: func_name.clone(),
                analysis: analysis.clone(),
                explore_config,
                file_path: file.clone(),
                mocks_used,
                callees,
                deep_fp: current_deep_fp,
                progress_index: current_progress,
                known_targets,
            });
        }

        // Collect the speculative pre-spawn (only in-flight when pool didn't
        // exist yet). If it succeeded, pass it to the new pool.
        let prewarmed = if let Some(rx) = prespawn_rx {
            match total_deadline_remaining(scan_deadline) {
                Some(remaining) => match tokio::time::timeout(remaining, rx).await {
                    Ok(Ok(Ok(fe))) => Some(fe),
                    _ => None,
                },
                None => match rx.await {
                    Ok(Ok(fe)) => Some(fe),
                    _ => None,
                },
            }
        } else {
            None
        };

        // Execute tasks in parallel, using either the shared WorkerPool (default)
        // or per-function dedicated frontends (Function isolation mode).
        // The pool is created lazily on the first layer with work and persists
        // across subsequent layers, keeping frontend subprocesses warm.
        if !tasks.is_empty() {
            // Build a map from function name to progress index for summary updates.
            let fn_progress_index: HashMap<String, usize> = tasks
                .iter()
                .map(|t| (t.func_name.clone(), t.progress_index))
                .collect();

            // Collect outcomes from either isolation path.
            let layer_outcomes: Vec<FunctionOutcome> = if let Some(bs) = config.batch_size {
                // Batched mode: round-robin one-function-at-a-time scheduling.
                // Shut down the speculative pre-spawn — batched mode manages
                // its own single frontend.
                if let Some(fe) = prewarmed {
                    tokio::spawn(async move {
                        let _ = fe.shutdown().await;
                    });
                }
                run_layer_batched(
                    Arc::clone(&fe_config_persistent),
                    tasks,
                    bs,
                    config.concolic,
                    config.timeout_per_fn,
                    config.build_timeout,
                    &config.cache,
                    &config.scheduler_state_cache,
                    &behavior_maps,
                    &input_pool,
                    &config.genetic_config,
                    progress_handler.clone(),
                    artifact_root.as_ref().map(Arc::clone),
                    total_functions,
                    scan_start,
                    config.coverage_mode.as_str(),
                )
                .await
            } else if config.isolation == IsolationMode::Function {
                // Function mode doesn't use the shared pool — shut down
                // the speculative pre-spawn if one was created.
                if let Some(fe) = prewarmed {
                    tokio::spawn(async move {
                        let _ = fe.shutdown().await;
                    });
                }
                // Each function gets a dedicated fresh frontend.
                // No shared pool — a Semaphore caps concurrency instead.
                let (outcomes, layer_peak) = run_layer_function_mode(
                    Arc::clone(&fe_config_persistent),
                    tasks,
                    effective_parallelism,
                    config.concolic,
                    config.timeout_per_fn,
                    config.build_timeout,
                    &config.cache,
                    &config.stored_inputs_cache,
                    &behavior_maps,
                    &input_pool,
                    &config.genetic_config,
                    progress_handler.clone(),
                    artifact_root.as_ref().map(Arc::clone),
                    total_functions,
                    scan_start,
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
                        let per_replica_iters = task
                            .explore_config
                            .max_iterations
                            .map(|m| (m / wpf as u32).max(1));
                        for replica in 0..wpf {
                            let mut replica_config = task.explore_config.clone();
                            replica_config.seed =
                                derive_replica_seed(task.explore_config.seed, fn_idx, replica);
                            replica_config.max_iterations = per_replica_iters;
                            out.push(ExploreTask {
                                func_name: task.func_name.clone(),
                                analysis: task.analysis.clone(),
                                explore_config: replica_config,
                                file_path: task.file_path.clone(),
                                mocks_used: task.mocks_used.clone(),
                                callees: task.callees.clone(),
                                deep_fp: task.deep_fp.clone(),
                                progress_index: task.progress_index,
                                known_targets: task.known_targets.clone(),
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
                let write_success_artifact = config.workers_per_fn <= 1;

                // Each task checks out a worker, explores, then returns the worker.
                // Behavior map storage is deferred to after all handles join so that
                // replicas for the same function can be merged first.
                let mut handles = Vec::new();

                for ExploreTask {
                    func_name,
                    analysis,
                    explore_config,
                    file_path,
                    mocks_used,
                    callees,
                    deep_fp,
                    progress_index,
                    known_targets: _,
                } in expanded_tasks
                {
                    let pool = Arc::clone(&pool);
                    let behavior_maps = Arc::clone(&behavior_maps);
                    let input_pool = Arc::clone(&input_pool);
                    let timeout = config.timeout_per_fn;
                    let build_timeout = config.build_timeout;
                    let concolic = config.concolic;
                    let genetic_config = config.genetic_config.clone();
                    let cache = config.cache.clone();
                    let progress_handler = progress_handler.clone();
                    let artifact_root = artifact_root.clone();
                    let handle_func_name = func_name.clone();
                    let handle_progress_index = progress_index;
                    let lease = Arc::new(WorkerTaskLease::new(
                        Arc::clone(&pool),
                        Arc::clone(&tasks_remaining),
                    ));
                    let handle_lease = Arc::clone(&lease);
                    let handle = tokio::spawn(async move {
                        // str-poyv: emit `started` only after we actually
                        // acquire a worker from the pool. Emitting on spawn
                        // (before checkout) made every queued task fire
                        // `started` at the same elapsed_ms — visible as a
                        // burst of `started` events even under
                        // `--scheduler-policy serial`, where the pool size
                        // is 1 and execution is strictly sequential.
                        let mut frontend = Some(pool.checkout().await);
                        lease.mark_checked_out();
                        emit_progress(
                            progress_handler.as_ref(),
                            &func_name,
                            progress_index,
                            total_functions,
                            scan_start.elapsed(),
                            ScanProgressStatus::Started,
                        );

                        let mut frontend_transport_attempts = 1usize;
                        let result = loop {
                            let Some(fe) = frontend.as_mut() else {
                                break PhasedOutcome::Failed(ScanError::Frontend(
                                    FrontendError::SubprocessExited {
                                        binary: pool.config.command.clone(),
                                        exit_status: None,
                                        stderr_tail: String::new(),
                                    },
                                ));
                            };
                            let result = run_phased(
                                fe,
                                &func_name,
                                &analysis,
                                concolic,
                                &explore_config,
                                &mocks_used,
                                &callees,
                                &behavior_maps,
                                deep_fp.clone(),
                                &input_pool,
                                &genetic_config,
                                &cache,
                                build_timeout,
                                timeout,
                            )
                            .await;

                            let Some(retry_source) =
                                retryable_frontend_transport_failure_source(&result)
                            else {
                                break result;
                            };

                            if frontend_transport_attempts >= FRONTEND_TRANSPORT_ATTEMPT_LIMIT {
                                break PhasedOutcome::Failed(ScanError::FrontendRetryExhausted {
                                    attempts: frontend_transport_attempts,
                                    message: retry_source,
                                });
                            }

                            frontend_transport_attempts += 1;
                            drop(frontend.take());
                            match Frontend::spawn(&pool.config).await {
                                Ok(replacement) => {
                                    frontend = Some(replacement);
                                }
                                Err(error) => {
                                    break PhasedOutcome::Failed(ScanError::Frontend(error));
                                }
                            }
                        };

                        let timed_out = matches!(
                            result,
                            PhasedOutcome::BuildTimedOut(_) | PhasedOutcome::ExploreTimedOut(_)
                        );

                        // Decrement the remaining-task counter FIRST so that
                        // return_or_reap_worker sees the updated pending count when
                        // deciding whether to reap this worker.
                        let remaining = lease.finish_once();

                        // After a timeout the frontend's stdout buffer contains a
                        // stale response that would cause an ID mismatch on the next
                        // request.  Kill and respawn instead of returning to pool.
                        // Also drop tainted frontends (str-quhk): when a Prepare
                        // call times out via the frontend's own request_timeout,
                        // `run_phased` falls through to explore which fails with
                        // PhasedOutcome::Failed (not BuildTimedOut), so
                        // `timed_out` is false. Without the tainted check the
                        // poisoned frontend re-enters the pool and cascades
                        // Timeout/IdMismatch errors across subsequent functions.
                        // Similarly, an IdMismatch leaves the pipe misaligned with
                        // a stale response; returning it would cascade the error.
                        // Skip replacement when the pool is already over-provisioned
                        // relative to remaining tasks — absorb the dead slot instead.
                        let frontend_error = matches!(
                            result,
                            PhasedOutcome::Failed(ScanError::Explore(
                                ExploreError::Frontend(_),
                            )) | PhasedOutcome::Failed(ScanError::Frontend(_))
                        );
                        if let Some(mut frontend) = frontend {
                            let poisoned = frontend.is_tainted() || frontend_error;
                            if timed_out || !frontend.is_alive() || poisoned {
                                // Drop the poisoned/dead frontend (kills the child process).
                                drop(frontend);
                                lease.account_dead_worker_if_checked_out(remaining).await;
                            } else {
                                lease.account_live_worker(frontend, remaining).await;
                            }
                        } else {
                            lease.account_dead_worker_if_checked_out(remaining).await;
                        }

                        // Grow the pool if tasks are still blocked on checkout().
                        pool.maybe_grow(remaining);

                        match result {
                            PhasedOutcome::Success(func_result) => {
                                if write_success_artifact {
                                    write_completed_scan_artifact(
                                        artifact_root.as_deref(),
                                        progress_index,
                                        total_functions,
                                        &file_path,
                                        &func_result,
                                    );
                                }
                                emit_progress(
                                    progress_handler.as_ref(),
                                    &func_name,
                                    progress_index,
                                    total_functions,
                                    scan_start.elapsed(),
                                    ScanProgressStatus::Completed,
                                );
                                FunctionOutcome::Success(func_result)
                            }
                            PhasedOutcome::Failed(e) => {
                                let reason = format!("error: {e}");
                                write_failed_scan_artifact(
                                    artifact_root.as_deref(),
                                    progress_index,
                                    total_functions,
                                    &func_name,
                                    &reason,
                                );
                                emit_progress(
                                    progress_handler.as_ref(),
                                    &func_name,
                                    progress_index,
                                    total_functions,
                                    scan_start.elapsed(),
                                    ScanProgressStatus::Failed,
                                );
                                FunctionOutcome::Error {
                                    function_name: func_name,
                                    error: e.to_string(),
                                }
                            }
                            outcome @ (PhasedOutcome::BuildTimedOut(_)
                            | PhasedOutcome::ExploreTimedOut(_)) => {
                                let (phase, d) = match outcome {
                                    PhasedOutcome::BuildTimedOut(d) => ("build", d),
                                    PhasedOutcome::ExploreTimedOut(d) => ("execution", d),
                                    _ => unreachable!(),
                                };
                                let reason = phase_timeout_reason(phase, d);
                                write_failed_scan_artifact(
                                    artifact_root.as_deref(),
                                    progress_index,
                                    total_functions,
                                    &func_name,
                                    &reason,
                                );
                                emit_progress(
                                    progress_handler.as_ref(),
                                    &func_name,
                                    progress_index,
                                    total_functions,
                                    scan_start.elapsed(),
                                    ScanProgressStatus::Failed,
                                );
                                FunctionOutcome::Timeout {
                                    function_name: func_name,
                                    limit: d,
                                    phase,
                                }
                            }
                        }
                    });

                    handles.push((
                        handle_func_name,
                        handle_progress_index,
                        handle_lease,
                        handle,
                    ));
                }

                let mut raw_outcomes = Vec::with_capacity(handles.len());
                let mut pending_handles = handles;
                let task_watchdog =
                    shared_pool_task_watchdog(config.build_timeout, config.timeout_per_fn);
                while !pending_handles.is_empty() {
                    let (function_name, progress_index, lease, mut handle) =
                        pending_handles.remove(0);
                    let join_limit = total_deadline_remaining(scan_deadline)
                        .map(|remaining| remaining.min(task_watchdog))
                        .unwrap_or(task_watchdog);
                    let join_result = match tokio::time::timeout(join_limit, &mut handle).await {
                        Ok(result) => result,
                        Err(_) => {
                            handle.abort();
                            let _ = handle.await;
                            lease.recover_after_abort().await;
                            let outcome = if total_deadline_remaining(scan_deadline)
                                .is_some_and(|remaining| remaining.is_zero())
                            {
                                FunctionOutcome::TotalTimeout {
                                    function_name: function_name.clone(),
                                }
                            } else {
                                let reason = phase_timeout_reason("task", task_watchdog);
                                write_failed_scan_artifact(
                                    artifact_root.as_deref(),
                                    progress_index,
                                    total_functions,
                                    &function_name,
                                    &reason,
                                );
                                emit_progress(
                                    progress_handler.as_ref(),
                                    &function_name,
                                    progress_index,
                                    total_functions,
                                    scan_start.elapsed(),
                                    ScanProgressStatus::Failed,
                                );
                                FunctionOutcome::Timeout {
                                    function_name: function_name.clone(),
                                    limit: task_watchdog,
                                    phase: "task",
                                }
                            };
                            raw_outcomes.push(outcome);
                            continue;
                        }
                    };

                    match join_result {
                        Ok(outcome) => raw_outcomes.push(outcome),
                        Err(e) => {
                            lease.recover_after_abort().await;
                            raw_outcomes.push(FunctionOutcome::Error {
                                function_name,
                                error: format!("task join error: {e}"),
                            });
                        }
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
                            maps.insert(result.function_name.clone(), result.behavior_map.clone());
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
                // Persist input vectors to the signature-keyed store
                // (str-bo4z.3). Looks up each success's analysis from the
                // per-layer map so the signature reflects the source.
                if config.stored_inputs_cache.is_some() {
                    for outcome in &outcomes {
                        if let FunctionOutcome::Success(result) = outcome
                            && let Some(analysis) = analysis_map.get(result.function_name.as_str())
                        {
                            persist_stored_inputs(
                                config.stored_inputs_cache.as_deref(),
                                analysis,
                                &result.behavior_map,
                            );
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
                        let idx = fn_progress_index
                            .get(&result.function_name)
                            .copied()
                            .unwrap_or(0);
                        summary_record_completed(
                            &mut summary,
                            &result.function_name,
                            idx,
                            scan_start.elapsed(),
                        );
                        // Record deep FP for this function so downstream layers
                        // can incorporate it into their deep fingerprints.
                        if let Some(ref fp) = result.behavior_map.fingerprint {
                            layer_deep_fps.push((result.function_name.clone(), fp.clone()));
                        }
                        all_results.push(*result);
                    }
                    FunctionOutcome::Timeout {
                        function_name,
                        limit,
                        phase,
                    } => {
                        let idx = fn_progress_index.get(&function_name).copied().unwrap_or(0);
                        let reason = phase_timeout_reason(phase, limit);
                        summary_record_failed(
                            &mut summary,
                            &function_name,
                            idx,
                            &reason,
                            scan_start.elapsed(),
                        );
                        skipped.push(SkippedFunction {
                            function_name,
                            reason,
                            category: SkipCategory::Error,
                        });
                    }
                    FunctionOutcome::TotalTimeout { function_name } => {
                        let idx = fn_progress_index.get(&function_name).copied().unwrap_or(0);
                        write_skipped_scan_artifact(
                            artifact_root.as_deref(),
                            idx,
                            total_functions,
                            &function_name,
                            TOTAL_SCAN_TIMEOUT_REASON,
                            SkipCategory::Error,
                        );
                        summary_record_skipped(
                            &mut summary,
                            &function_name,
                            idx,
                            TOTAL_SCAN_TIMEOUT_REASON,
                            SkipCategory::Error,
                            scan_start.elapsed(),
                        );
                        emit_progress(
                            progress_handler.as_ref(),
                            &function_name,
                            idx,
                            total_functions,
                            scan_start.elapsed(),
                            ScanProgressStatus::Skipped,
                        );
                        skipped.push(SkippedFunction {
                            function_name,
                            reason: TOTAL_SCAN_TIMEOUT_REASON.into(),
                            category: SkipCategory::Error,
                        });
                        summary.status = ScanRunStatus::Interrupted;
                    }
                    FunctionOutcome::Error {
                        function_name,
                        error,
                    } => {
                        let idx = fn_progress_index.get(&function_name).copied().unwrap_or(0);
                        let reason = format!("error: {error}");
                        summary_record_failed(
                            &mut summary,
                            &function_name,
                            idx,
                            &reason,
                            scan_start.elapsed(),
                        );
                        skipped.push(SkippedFunction {
                            function_name,
                            reason,
                            category: SkipCategory::Error,
                        });
                    }
                    FunctionOutcome::Unsupported {
                        function_name,
                        reason,
                    } => {
                        let idx = fn_progress_index.get(&function_name).copied().unwrap_or(0);
                        let report_reason = format!("unsupported: {reason}");
                        summary_record_skipped(
                            &mut summary,
                            &function_name,
                            idx,
                            &report_reason,
                            SkipCategory::Unsupported,
                            scan_start.elapsed(),
                        );
                        skipped.push(SkippedFunction {
                            function_name,
                            reason: report_reason,
                            category: SkipCategory::Unsupported,
                        });
                    }
                }
            }
            // Update the summary after each layer.
            maybe_write_summary(&summary);
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

    // Finalize the scan summary with end-of-run source-set validation
    // (str-jeen.3). If files drifted during the run, the status is
    // promoted to `StaleSourceSet` rather than `Completed`.
    summary_finalize_with_manifest_check(
        &mut summary,
        scan_start.elapsed(),
        &run_manifest,
        &manifest_source_paths,
    );
    maybe_write_summary(&summary);
    if write_artifacts {
        let status_files = status_file_inputs_from_scan_summary(&summary, &config.file_map);
        let status_targets = status_target_inputs_from_scan_summary(
            &scan_root_dir,
            &summary,
            &config.file_map,
            analyses,
        );
        write_scan_status(
            &scan_root_dir,
            &summary,
            &run_manifest,
            &status_files,
            &status_targets,
        );
    }

    Ok(ParallelScanResult {
        function_results: all_results,
        test_order,
        skipped,
        workers_used: peak_workers,
        workers_reaped: total_reaped,
        sampling: None,
        source_files: run_manifest.source_files,
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

/// Detect Go-style method targets from a function-analysis name. The Go
/// frontend emits methods as receiver-decorated qualified names —
/// `(*Type).Method` for pointer receivers, `(Type).Method` for value
/// receivers (str-fuhw.1.1) — while free functions stay bare. The leading
/// `(` is the cheapest reliable signal that does not require type lookup.
fn is_method_target_name(name: &str) -> bool {
    name.starts_with('(')
}

/// Consult the frontend's invocation planner for a method target and return
/// the first `InvocationPlan` it emits, suitable for attaching to
/// [`ExploreConfig::default_execute_plan`]. Returns `None` for free
/// functions, frontends without `get_invocation_plan` capability,
/// transport failures, and method targets the planner cannot satisfy.
///
/// Without this wiring the launcher wrapper's switch on
/// `Command::Execute.plan.receiver_kind` falls into its default arm and
/// emits `"shatter: unknown receiver kind"`. Mirrors the per-target
/// planner consultation the `explore` CLI command performs before
/// dispatch (see `shatter-cli/src/commands/explore.rs`
/// `fetch_planner_extra_seeds`).
async fn fetch_default_execute_plan_for_method(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    file: &str,
    project_root: Option<&str>,
) -> Option<crate::protocol::InvocationPlan> {
    if !is_method_target_name(&analysis.name) {
        return None;
    }
    if !frontend
        .capabilities()
        .iter()
        .any(|cap| cap == "get_invocation_plan")
    {
        return None;
    }

    // Prime the analysis cache so the frontend's get_invocation_plan
    // target_id lookup resolves. Mirrors the explore CLI pattern.
    let _ = frontend
        .send(crate::protocol::Command::Analyze {
            file: file.to_string(),
            function: Some(analysis.name.clone()),
            project_root: project_root.map(str::to_string),
            execution_profile: None,
        })
        .await;

    let target_id = format!(":{}", analysis.name);
    match crate::planner_consumer::fetch_planner_seeds(frontend, &target_id, &analysis.params).await
    {
        Ok(bundle) => {
            if !bundle.unsatisfied.is_empty() {
                log::debug!(
                    "[scan] planner unsatisfied for {}: {:?}",
                    analysis.name,
                    bundle.unsatisfied,
                );
            }
            bundle
                .plans
                .into_iter()
                .find(|p| !p.receiver_kind.is_empty())
        }
        Err(e) => {
            log::debug!(
                "[scan] planner fetch failed for {}: {e}",
                analysis.name,
            );
            None
        }
    }
}

/// Build an [`ExecutionProfile`] from a [`FunctionAnalysis`]'s invocation model.
///
/// When the frontend marks a target as `InvocationModel::Adapter`, the execute
/// request must include an execution profile activating that adapter so the
/// frontend can resolve the registered hook (e.g. `go/http-handler`). Without
/// this, the execute request carries no profile and the frontend errors with
/// "execution adapter not supported" (str-0x82).
///
/// Returns `None` for `InvocationModel::Direct` targets that need no adapter.
fn execution_profile_from_analysis(
    analysis: &crate::protocol::FunctionAnalysis,
) -> Option<crate::protocol::ExecutionProfile> {
    match &analysis.invocation_model {
        crate::protocol::InvocationModel::Adapter { adapter_id, .. } => {
            Some(crate::protocol::ExecutionProfile {
                adapters: vec![crate::protocol::ExecutionAdapter {
                    id: adapter_id.clone(),
                    apply: None,
                    options: None,
                }],
            })
        }
        crate::protocol::InvocationModel::Direct => None,
    }
}

/// Format a phase-tagged timeout reason used in scan failure artifacts and
/// progress events. Centralised so the three orchestrator call sites stay
/// in sync and the message is unit-testable (str-ubp1).
fn phase_timeout_reason(phase: &str, d: Duration) -> String {
    format!("timed out during {phase} after {:.0}s", d.as_secs_f64())
}

const SHARED_POOL_TASK_CLEANUP_GRACE: Duration = Duration::from_secs(5);

fn shared_pool_task_watchdog(build_timeout: Duration, timeout_per_fn: Duration) -> Duration {
    build_timeout
        .saturating_add(timeout_per_fn)
        .saturating_add(SHARED_POOL_TASK_CLEANUP_GRACE)
}

fn total_deadline(scan_start: Instant, timeout_total: Option<Duration>) -> Option<Instant> {
    timeout_total.map(|timeout| scan_start + timeout)
}

fn total_deadline_remaining(deadline: Option<Instant>) -> Option<Duration> {
    deadline.map(|instant| instant.saturating_duration_since(Instant::now()))
}

/// Outcome of a phased Prepare + explore run.
///
/// Splitting the two phases lets the orchestrator charge build cost against
/// `build_timeout` and only charge the actual concolic exploration against
/// `timeout_per_fn`. Without this split, a cold launcher build on the first
/// scanned function eats most of the per-fn budget and surfaces as a
/// misleading "timed out after Ns" (str-v5qe, str-6sie).
enum PhasedOutcome {
    Success(Box<FunctionResult>),
    Failed(ScanError),
    /// `Prepare` (build) phase exceeded `build_timeout`.
    BuildTimedOut(Duration),
    /// Concolic exploration phase exceeded `timeout_per_fn`.
    ExploreTimedOut(Duration),
}

const FRONTEND_TRANSPORT_ATTEMPT_LIMIT: usize = 3;

fn retryable_frontend_transport_error(error: &FrontendError) -> bool {
    matches!(
        error,
        FrontendError::Write(_) | FrontendError::Read(_) | FrontendError::SubprocessExited { .. }
    )
}

fn retryable_frontend_transport_failure_source(outcome: &PhasedOutcome) -> Option<String> {
    match outcome {
        PhasedOutcome::Failed(ScanError::Frontend(error))
            if retryable_frontend_transport_error(error) =>
        {
            Some(error.to_string())
        }
        PhasedOutcome::Failed(ScanError::Explore(ExploreError::Frontend(error)))
            if retryable_frontend_transport_error(error) =>
        {
            Some(error.to_string())
        }
        _ => None,
    }
}

/// Run a function's `Prepare` phase under `build_timeout`, then run the
/// concolic exploration under `explore_timeout` with the resulting
/// `prepare_id` already attached so the explorer doesn't re-prepare.
#[allow(clippy::too_many_arguments)]
async fn run_phased(
    frontend: &mut Frontend,
    func_name: &str,
    analysis: &FunctionAnalysis,
    concolic: bool,
    explore_config: &ExploreConfig,
    mocks_used: &[MockUsage],
    callees: &std::collections::HashSet<String>,
    behavior_maps: &Mutex<HashMap<String, BehaviorMap>>,
    fingerprint: Option<String>,
    input_pool: &Mutex<InterestingPool>,
    genetic_config: &crate::config::GeneticConfig,
    cache: &Option<Arc<BehaviorMapCache>>,
    build_timeout: Duration,
    explore_timeout: Duration,
) -> PhasedOutcome {
    let mut effective = explore_config.clone();
    if effective.prepare_id_override.is_none()
        && explorer::frontend_supports(&effective.capabilities, "prepare")
    {
        let prep = tokio::time::timeout(
            build_timeout,
            frontend.send(crate::protocol::Command::Prepare {
                file: effective.file.clone(),
                function: analysis.name.clone(),
                mocks: effective.mocks.clone(),
                project_root: effective.project_root.clone(),
                execution_profile: effective.execution_profile.clone(),
                plan: effective.default_execute_plan.clone(),
            }),
        )
        .await;
        match prep {
            Err(_) => return PhasedOutcome::BuildTimedOut(build_timeout),
            Ok(Err(e)) => {
                // Frontend transport / IO error during prepare. Fall back to
                // the per-execute build path inside the explorer rather than
                // failing the function outright — keeps parity with the
                // explorer's own prepare-fallback behavior.
                log::debug!("scan prepare failed for {func_name}, falling back: {e}");
                // str-quhk: if the frontend's own request_timeout fired, the
                // frontend is now tainted and all subsequent sends will
                // return Timeout immediately. Continuing to explore would
                // produce a PhasedOutcome::Failed that the pool handler used
                // to treat as non-timeout, returning the tainted frontend to
                // the pool and cascading errors across subsequent functions.
                // Short-circuit to BuildTimedOut so the caller drops the
                // frontend cleanly.
                if frontend.is_tainted() {
                    return PhasedOutcome::BuildTimedOut(build_timeout);
                }
            }
            Ok(Ok(resp)) => match resp.result {
                crate::protocol::ResponseResult::Prepare { prepare_id } => {
                    effective.prepare_id_override = Some(prepare_id);
                }
                other => {
                    log::debug!(
                        "scan prepare unexpected response for {func_name}: {other:?}",
                    );
                }
            },
        }
    }

    let explore = tokio::time::timeout(
        explore_timeout,
        explore_single_function(
            frontend,
            func_name,
            analysis,
            concolic,
            &effective,
            mocks_used,
            callees,
            behavior_maps,
            fingerprint,
            input_pool,
            genetic_config,
            cache,
        ),
    )
    .await;
    match explore {
        Err(_) => PhasedOutcome::ExploreTimedOut(explore_timeout),
        Ok(Err(e)) => PhasedOutcome::Failed(e),
        Ok(Ok(r)) => PhasedOutcome::Success(Box::new(r)),
    }
}

/// Explore a single function and build its result.
///
/// This is the core work unit for both sequential and parallel scanning.
#[allow(clippy::too_many_arguments)]
async fn explore_single_function(
    frontend: &mut Frontend,
    func_name: &str,
    analysis: &FunctionAnalysis,
    concolic: bool,
    explore_config: &ExploreConfig,
    mocks_used: &[MockUsage],
    callees: &std::collections::HashSet<String>,
    behavior_maps: &Mutex<HashMap<String, BehaviorMap>>,
    fingerprint: Option<String>,
    input_pool: &Mutex<InterestingPool>,
    genetic_config: &crate::config::GeneticConfig,
    cache: &Option<Arc<BehaviorMapCache>>,
) -> Result<FunctionResult, ScanError> {
    // str-jeen.50: when scanning a method target, consult the frontend's
    // invocation planner so the launcher wrapper's receiver-kind switch has
    // a real `receiver_kind` to dispatch on. Without this the wrapper falls
    // into its default arm and surfaces "shatter: unknown receiver kind".
    // No-op for free functions, frontends without `get_invocation_plan`,
    // and callers that already attached a plan upstream.
    let mut effective_config = explore_config.clone();
    if effective_config.default_execute_plan.is_none()
        && let Some(plan) = fetch_default_execute_plan_for_method(
            frontend,
            analysis,
            &effective_config.file,
            effective_config.project_root.as_deref(),
        )
        .await
    {
        effective_config.default_execute_plan = Some(plan);
    }
    let explore_config = &effective_config;
    let exploration = explore_with_scan_mode(frontend, analysis, concolic, explore_config).await?;

    // Genetic algorithm follow-up phase: target unsolved branches.
    let mut ga_discoveries: Vec<crate::behavior::Behavior> = Vec::new();
    if genetic_config.enabled {
        let targets = crate::coverage_metrics::extract_targets(analysis, &exploration);
        if !targets.is_empty() {
            log::info!(
                "[scan] Starting GA for {} ({} unsolved target(s))",
                func_name,
                targets.len(),
            );
            let mut seed_inputs: Vec<Vec<serde_json::Value>> = exploration
                .raw_results
                .iter()
                .map(|(inputs, _, _)| inputs.clone())
                .collect();
            // Extend GA seeds with cached inputs from prior runs.
            if let Some(c) = cache
                && let Ok(Some(cached_map)) = c.load(func_name)
            {
                seed_inputs.extend(cached_map.extract_seed_inputs());
            }
            match crate::genetic_explorer::genetic_explore(
                frontend,
                func_name,
                seed_inputs,
                targets,
                &analysis.params,
                genetic_config,
            )
            .await
            {
                Ok(ga_result) => {
                    if !ga_result.discoveries.is_empty() {
                        log::info!(
                            "[scan] GA found {} new behavior(s) for {}",
                            ga_result.discoveries.len(),
                            func_name,
                        );
                    }
                    ga_discoveries = ga_result.discoveries;
                }
                Err(e) => {
                    log::warn!("[scan] GA error for {}: {e}", func_name);
                }
            }
        }
    }

    // Donate unused budget to the layer surplus so other functions can use it.
    if let Some(ref surplus) = explore_config.budget_surplus
        && let Some(allocated) = explore_config.max_iterations
    {
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
            interesting_pool::CoverageMode::Branch,
        );
    }

    // Run the Analyze stage to produce behavior map and coverage metrics.
    let mut analyze_out = analyze_exploration(&exploration, analysis, fingerprint);

    // Merge GA discoveries into the behavior map.
    if !ga_discoveries.is_empty() {
        let added = analyze_out
            .behavior_map
            .merge_ga_discoveries(&ga_discoveries);
        if added > 0 {
            log::info!("[scan] Merged {added} GA behavior(s) into behavior map for {func_name}");
        }
    }

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
            mock_misses
                .iter()
                .map(|m| &m.callee_name)
                .collect::<HashSet<_>>()
                .len(),
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
        parts.push(format!(
            "{} via behavior map ({})",
            cached.len(),
            cached.join(", ")
        ));
    }
    if !stubs.is_empty() {
        parts.push(format!(
            "{} via type-aware stub ({})",
            stubs.len(),
            stubs.join(", ")
        ));
    }
    if !excluded.is_empty() {
        parts.push(format!(
            "{} stratum-excluded ({})",
            excluded.len(),
            excluded.join(", ")
        ));
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
        by_callee
            .entry(miss.callee_name.as_str())
            .or_default()
            .push(miss);
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
                    while !s.is_char_boundary(end) {
                        end -= 1;
                    }
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

    let expected: Vec<_> = result
        .skipped
        .iter()
        .filter(|s| s.category == SkipCategory::Expected)
        .collect();
    let errors: Vec<_> = result
        .skipped
        .iter()
        .filter(|s| s.category == SkipCategory::Error)
        .collect();
    let unsupported_count = result
        .skipped
        .iter()
        .filter(|s| s.category == SkipCategory::Unsupported)
        .count();

    // str-izhn: summary line names every bucket so CI and Makefile wrappers
    // can grep `failed=` / `unsupported=` without parsing the report body.
    out.push_str(&format!(
        "Scan complete: {} completed, {} failed, {} unsupported, {} skipped ({} worker(s))\n",
        result.function_results.len(),
        errors.len(),
        unsupported_count,
        expected.len(),
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

    let expected: Vec<_> = result
        .skipped_functions
        .iter()
        .filter(|s| s.category == SkipCategory::Expected)
        .collect();
    let errors: Vec<_> = result
        .skipped_functions
        .iter()
        .filter(|s| s.category == SkipCategory::Error)
        .collect();

    out.push_str(&format!(
        "Scan complete: {} function(s) tested\n",
        result.function_results.len()
    ));

    for func_result in &result.function_results {
        out.push_str(&format!("\n── {} ──\n", func_result.function_name));

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

/// Append "Skipped (expected)" and "Errors" sections to a report string.
fn format_skip_sections(
    expected: &[&SkippedFunction],
    errors: &[&SkippedFunction],
    out: &mut String,
) {
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
        TypeInfo::Union { variants } => variants
            .iter()
            .map(format_type)
            .collect::<Vec<_>>()
            .join(" | "),
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
        let max_layer = if all_layers.is_empty() {
            0
        } else {
            all_layers.len() - 1
        };
        let range = crate::stratum::resolve_range(spec, max_layer)?;
        crate::stratum::filter_layers(&all_layers, &range)
    } else {
        all_layers.iter().enumerate().collect()
    };

    // Collect unique source files.
    let file_count = config.file_map.values().collect::<HashSet<_>>().len();

    let selected_function_count: usize = selected_layers.iter().map(|(_, l)| l.len()).sum();
    let total_functions = analyses.len();

    let mut out = String::new();

    out.push_str("Dry-run scan plan\n");
    out.push_str("=================\n\n");

    if config.stratum.is_some() {
        out.push_str(&format!(
            "Summary: {} of {} function(s) across {} file(s), {} of {} layer(s) selected\n",
            selected_function_count,
            total_functions,
            file_count,
            selected_layers.len(),
            total_layer_count,
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
        if config.parallelism == 1 {
            ""
        } else {
            "(parallel)"
        },
    ));

    // Estimate time: each layer runs sequentially, functions within a layer run in parallel.
    // Worst case per layer = ceil(functions / workers) * timeout_per_fn.
    let timeout_secs = config.timeout_per_fn.as_secs();
    let mut total_estimate_secs: u64 = 0;
    for (_, layer) in &selected_layers {
        let batches =
            (layer.len() as u64 + config.parallelism as u64 - 1) / config.parallelism.max(1) as u64;
        total_estimate_secs += batches * timeout_secs;
    }
    let selected_layer_count = selected_layers.len();
    out.push_str(&format!(
        "Estimated time: <={total_estimate_secs}s ({selected_layer_count} layer(s) x {timeout_secs}s timeout)\n",
    ));

    // str-fuhw: key by qualified ID so dup-named analyses across files
    // don't collide. See identical site in `scan` for rationale.
    let analysis_qids: Vec<String> = analyses
        .iter()
        .map(crate::behavior::node_id_for_analysis)
        .collect();
    let analysis_map: HashMap<&str, &FunctionAnalysis> = analysis_qids
        .iter()
        .zip(analyses.iter())
        .map(|(qid, a)| (qid.as_str(), a))
        .collect();

    // str-fuhw: scan_set tracks qualified IDs as well so dependency
    // labelling and cross-stratum mock attribution don't conflate
    // duplicate-named functions across files.
    let scan_set: HashSet<&str> = analysis_qids.iter().map(String::as_str).collect();

    // Functions in selected layers (for cross-stratum mock labelling).
    let selected_set: HashSet<&str> = selected_layers
        .iter()
        .flat_map(|(_, layer)| layer.iter().map(|s| s.as_str()))
        .collect();

    for &(layer_idx, layer) in &selected_layers {
        let parallelizable = if layer.len() > 1 {
            ", parallelizable"
        } else {
            ""
        };
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

            // str-fuhw: `func_name` is a qualified ID on production paths.
            // Show the bare name in the signature line and append the file
            // path so dry-run output stays compact while still
            // disambiguating duplicate-named functions across files.
            let (func_file, display_name) = crate::behavior::split_qualified_id(func_name);
            let location_suffix = if func_file.is_empty() {
                String::new()
            } else {
                format!("  [{func_file}]")
            };

            // Format function signature.
            let params_str: Vec<String> = analysis
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, format_type(&p.typ)))
                .collect();
            let ret_str = format_type(&analysis.return_type);
            out.push_str(&format!(
                "  {}({}) -> {}{}\n",
                display_name,
                params_str.join(", "),
                ret_str,
                location_suffix,
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

            // Show callees by bare name so the dependency line stays
            // readable; full qualified IDs already appear in each
            // function's location suffix above.
            let deps_str = if internal_deps.is_empty() {
                "none".to_string()
            } else {
                internal_deps
                    .iter()
                    .map(|d| {
                        let (_, dep_display) = crate::behavior::split_qualified_id(d);
                        if selected_set.contains(d) {
                            format!("{dep_display} (behavior-mock)")
                        } else {
                            format!("{dep_display} (outside stratum — auto-mock)")
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

#[derive(Debug, Default)]
struct ConfigFunctionInputs {
    skip: bool,
    candidate_inputs: Vec<Vec<serde_json::Value>>,
    value_sources: Vec<crate::input_gen::ValueSource>,
}

/// Load per-function scan inputs from `.shatter/config.yaml` if `config_dir` is set.
///
/// Returns empty candidate inputs and value sources on missing config or resolution errors
/// (logged as warnings).
fn load_config_function_inputs(
    analysis: &FunctionAnalysis,
    func_name: &str,
    config_dir: &Option<PathBuf>,
    max_iterations: u32,
    timeout_secs: u64,
) -> ConfigFunctionInputs {
    let Some(dir) = config_dir else {
        return ConfigFunctionInputs::default();
    };
    match crate::config::resolve_function_config_with_inputs(
        func_name,
        dir,
        None,
        Some(max_iterations),
        timeout_secs,
        &[],
    ) {
        Ok(resolved) => {
            if !resolved.candidate_inputs.is_empty() {
                log::debug!(
                    "Scan: {} candidate input(s) from config for {}",
                    resolved.candidate_inputs.len(),
                    func_name,
                );
            }
            let candidate_inputs = resolved
                .candidate_inputs
                .iter()
                .map(|input| input.args.clone())
                .collect();
            let value_sources = crate::input_gen::resolve_value_sources(
                &analysis.params,
                &resolved.param_generators,
                &resolved.generators,
            );
            ConfigFunctionInputs {
                skip: resolved.skip,
                candidate_inputs,
                value_sources,
            }
        }
        Err(e) => {
            log::warn!("Failed to resolve config inputs for {}: {}", func_name, e,);
            ConfigFunctionInputs::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DependencyKind, ExecuteResult, ExternalDependency, PerformanceMetrics};
    use crate::types::{ParamInfo, TypeInfo};

    /// Request timeout for integration tests using the noop frontend.
    const TEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    /// str-ubp1: phase-tagged reasons distinguish build vs execution timeouts
    /// so users can tell whether `--build-timeout` or `--timeout-per-fn` is
    /// the binding limit on a failure.
    #[test]
    fn phase_timeout_reason_tags_phase_and_duration() {
        assert_eq!(
            phase_timeout_reason("build", Duration::from_secs(30)),
            "timed out during build after 30s",
        );
        assert_eq!(
            phase_timeout_reason("execution", Duration::from_secs(15)),
            "timed out during execution after 15s",
        );
    }

    /// str-0agd regression: the shared-pool join watchdog is only a cleanup
    /// backstop around the explicit build and exploration phase timeouts. It
    /// must not add the old hidden 30-second grace that turned the documented
    /// default `--timeout-per-fn=30` into user-visible `task after 90s`
    /// outcomes for Kapow scan targets.
    #[test]
    fn shared_pool_task_watchdog_uses_small_cleanup_cushion() {
        let watchdog =
            shared_pool_task_watchdog(Duration::from_secs(30), Duration::from_secs(30));

        assert!(
            watchdog <= Duration::from_secs(65),
            "watchdog should not stretch default scan task timeout to {watchdog:?}"
        );
    }

    /// str-0x82: adapter-typed invocation models produce an execution profile
    /// containing the adapter id. Direct models produce `None`. This ensures
    /// scan-discovered adapter targets (e.g. go/http-handler) carry the correct
    /// profile into execute requests.
    #[test]
    fn execution_profile_from_adapter_invocation_model() {
        let mut analysis = make_analysis("handler", vec![]);
        analysis.invocation_model = crate::protocol::InvocationModel::Adapter {
            adapter_id: "go/http-handler".into(),
            synthetic_params: vec![],
            scenario_schema: None,
        };
        let profile = execution_profile_from_analysis(&analysis).expect("adapter must produce profile");
        assert_eq!(profile.adapters.len(), 1);
        assert_eq!(profile.adapters[0].id, "go/http-handler");
    }

    #[test]
    fn execution_profile_from_direct_invocation_model_is_none() {
        let analysis = make_analysis("plain_fn", vec![]);
        assert!(execution_profile_from_analysis(&analysis).is_none());
    }

    /// str-v5qe / str-6sie regression: a Go scan with a tiny per-fn timeout
    /// and a generous build_timeout must not surface as a per-fn timeout when
    /// the launcher build takes longer than `timeout_per_fn`. We model the
    /// behaviour at the ScanConfig level — the two budgets are independent
    /// and `build_timeout` is plumbed separately rather than being absorbed
    /// into `timeout_per_fn`.
    #[test]
    fn scan_config_separates_build_and_per_fn_budgets() {
        let cfg = minimal_scan_config(HashMap::new());
        // The default minimal config is identical for legacy callers, but the
        // build_timeout field is now an independent budget on ScanConfig.
        assert!(cfg.build_timeout > Duration::ZERO);
        // A reasonable invariant for the Go pipeline: build_timeout is the
        // build budget and timeout_per_fn is the explore budget. They are
        // not the same value, and one not being included in the other lets
        // a cold-cache build absorb its own cost.
        let separated = ScanConfig {
            build_timeout: Duration::from_secs(60),
            timeout_per_fn: Duration::from_secs(5),
            ..minimal_scan_config(HashMap::new())
        };
        assert_eq!(separated.build_timeout, Duration::from_secs(60));
        assert_eq!(separated.timeout_per_fn, Duration::from_secs(5));
    }

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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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

    fn minimal_scan_config(file_map: HashMap<String, String>) -> ScanConfig {
        ScanConfig {
            max_iterations_per_function: 1,
            concolic: false,
            seed: None,
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: false,
        }
    }

    #[test]
    fn scan_id_includes_qualified_function_targets() {
        let mut first_targets = HashMap::new();
        first_targets.insert(
            "src/service.go::(*Reader).Write".to_string(),
            "src/service.go".to_string(),
        );
        first_targets.insert(
            "src/service.go::(*Writer).Write".to_string(),
            "src/service.go".to_string(),
        );

        let mut second_targets = HashMap::new();
        second_targets.insert(
            "src/service.go::(*Reader).Write".to_string(),
            "src/service.go".to_string(),
        );
        second_targets.insert(
            "src/service.go::(*Buffer).Write".to_string(),
            "src/service.go".to_string(),
        );

        assert_ne!(
            compute_scan_id(&minimal_scan_config(first_targets)),
            compute_scan_id(&minimal_scan_config(second_targets)),
            "scan IDs should not collide when same-file qualified targets change",
        );
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
            loop_body_states: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
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
            loop_body_states: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
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
                        raw_results: vec![],
                        discoveries: vec![],
                        nondeterministic_fields: vec![],
                        float_probe_results: vec![],
                        boundary_results: vec![],
                        shrunk_witnesses: std::collections::HashMap::new(),
                        mcdc_summary: None,
                        shrink_stats: crate::shrink::ShrinkStats::default(),
                        abandoned_frontiers: vec![],
                        opaque_suggestions: vec![],
                        stubbed_modules: vec![],
                        ..Default::default()
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
                        raw_results: vec![],
                        discoveries: vec![],
                        nondeterministic_fields: vec![],
                        float_probe_results: vec![],
                        boundary_results: vec![],
                        shrunk_witnesses: std::collections::HashMap::new(),
                        mcdc_summary: None,
                        shrink_stats: crate::shrink::ShrinkStats::default(),
                        abandoned_frontiers: vec![],
                        opaque_suggestions: vec![],
                        stubbed_modules: vec![],
                        ..Default::default()
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
                    mocks_used: vec![MockUsage {
                        name: "leaf".into(),
                        source: MockSource::CachedBehaviorMap,
                    }],
                    coverage_metrics: Default::default(),
                    mock_misses: vec![],
                    refactoring_recommendations: vec![],
                },
            ],
            skipped_functions: vec![],
            sampling: None,
            source_files: vec![],
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
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
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
                                    ..Default::default()
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
            source_files: vec![],
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
                                    ..Default::default()
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
            source_files: vec![],
        };

        let report = format_scan_report(&result);
        assert!(
            report.contains("Skipped (expected, 1):"),
            "missing expected section: {report}"
        );
        assert!(report.contains("handleRequest: param \"socket\" → net.Socket (network handle"));
        assert!(
            report.contains("Errors (1):"),
            "missing errors section: {report}"
        );
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
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
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
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
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
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
        };
        let report = format_scan_report(&result);
        assert!(
            !report.contains("Explored"),
            "no sampling context should omit Explored header"
        );
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
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("1 completed"));
        assert!(report.contains("1 failed"));
        assert!(report.contains("0 unsupported"));
        assert!(report.contains("0 skipped"));
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
                    raw_results: vec![],
                    discoveries: vec![],
                    nondeterministic_fields: vec![],
                    float_probe_results: vec![],
                    boundary_results: vec![],
                    shrunk_witnesses: std::collections::HashMap::new(),
                    mcdc_summary: None,
                    shrink_stats: crate::shrink::ShrinkStats::default(),
                    abandoned_frontiers: vec![],
                    opaque_suggestions: vec![],
                    stubbed_modules: vec![],
                    ..Default::default()
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
            source_files: vec![],
        };

        let report = format_parallel_scan_report(&result);
        assert!(report.contains("1 completed"));
        assert!(report.contains("0 failed"));
        assert!(report.contains("0 unsupported"));
        assert!(report.contains("0 skipped"));
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("leaf".to_string(), "test.ts".to_string());
        file_map.insert("caller".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 2,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(99),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("cached_fn".to_string(), "test.ts".to_string());

        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let cache =
            Arc::new(BehaviorMapCache::new(tmp_dir.path().to_path_buf()).expect("create cache"));

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            isolation: IsolationMode::Function,
            capture_side_effects: false,
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.function_results.len(), 1);

        // Verify the behavior map was persisted to cache.
        let loaded = cache.load("cached_fn").expect("cache load should succeed");
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_a".to_string(), "test.ts".to_string());
        file_map.insert("fn_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
            assert_eq!(s.reason, "timed out (total scan budget exceeded)");
        }
    }

    #[tokio::test]
    async fn parallel_scan_emits_started_and_completed_progress() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, Mutex as StdMutex};

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
            invocation_model: crate::protocol::InvocationModel::Direct,
            source_file: None,
            adapter_hints: vec![],
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(99),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let progress = Arc::new(move |update: ScanProgressUpdate| {
            sink_events.lock().unwrap().push(update);
        }) as ProgressHandler;

        let result = parallel_scan_with_progress(&fe_config, &analyses, &config, Some(progress))
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(result.function_results.len(), 1);

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].status, ScanProgressStatus::Started);
        assert_eq!(events[1].status, ScanProgressStatus::Completed);
        assert_eq!(events[0].function_name, "solo");
        assert_eq!(events[1].function_name, "solo");
        assert_eq!(events[0].current, 1);
        assert_eq!(events[1].current, 1);
        assert_eq!(events[0].total, 1);
        assert_eq!(events[1].total, 1);
    }

    #[tokio::test]
    async fn parallel_scan_emits_skipped_and_failed_progress() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, Mutex as StdMutex};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let analysis = FunctionAnalysis {
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
            invocation_model: crate::protocol::InvocationModel::Direct,
            source_file: None,
            adapter_hints: vec![],
        };

        let mut skipped_file_map = HashMap::new();
        skipped_file_map.insert("solo".to_string(), "test.ts".to_string());
        let skipped_config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(99),
            file_map: skipped_file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let skipped_events = Arc::new(StdMutex::new(Vec::new()));
        let skipped_sink = Arc::clone(&skipped_events);
        let skipped_progress = Arc::new(move |update: ScanProgressUpdate| {
            skipped_sink.lock().unwrap().push(update);
        }) as ProgressHandler;

        let mut noop_config = FrontendConfig::new(PathBuf::from("bash"));
        noop_config.args = vec![noop_path.to_string_lossy().into_owned()];
        noop_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let skipped_result = parallel_scan_with_progress(
            &noop_config,
            std::slice::from_ref(&analysis),
            &skipped_config,
            Some(skipped_progress),
        )
        .await
        .expect("skip-only scan should succeed");

        assert!(skipped_result.function_results.is_empty());
        {
            let skipped_events = skipped_events.lock().unwrap();
            assert_eq!(skipped_events.len(), 1);
            assert_eq!(skipped_events[0].status, ScanProgressStatus::Skipped);
            assert_eq!(skipped_events[0].current, 1);
            assert_eq!(skipped_events[0].total, 1);
        }

        let mut failed_file_map = HashMap::new();
        failed_file_map.insert("solo".to_string(), "test.ts".to_string());
        let failed_config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(99),
            file_map: failed_file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let failed_events = Arc::new(StdMutex::new(Vec::new()));
        let failed_sink = Arc::clone(&failed_events);
        let failed_progress = Arc::new(move |update: ScanProgressUpdate| {
            failed_sink.lock().unwrap().push(update);
        }) as ProgressHandler;

        let mut bad_config = FrontendConfig::new(PathBuf::from("/definitely/missing-binary"));
        bad_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let failed_result = parallel_scan_with_progress(
            &bad_config,
            std::slice::from_ref(&analysis),
            &failed_config,
            Some(failed_progress),
        )
        .await
        .expect("frontend spawn failure should be reported as a scan result");

        assert!(failed_result.function_results.is_empty());
        assert_eq!(failed_result.skipped.len(), 1);
        {
            let failed_events = failed_events.lock().unwrap();
            assert_eq!(failed_events.len(), 2);
            assert_eq!(failed_events[0].status, ScanProgressStatus::Started);
            assert_eq!(failed_events[1].status, ScanProgressStatus::Failed);
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("slow_a".to_string(), "test.ts".to_string());
        file_map.insert("slow_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            // Single worker: same pool slot is reused, exposing stale-response bug.
            parallelism: 1,
            // Short per-function timeout triggers during the slow execute.
            timeout_per_fn: Duration::from_secs(3),
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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

    #[tokio::test]
    async fn parallel_scan_total_timeout_interrupts_active_layer() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};
        use std::time::Instant;

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let slow_path = manifest_dir.join("../protocol/slow-execute-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![slow_path.to_string_lossy().into_owned()];
        fe_config
            .env_vars
            .push(("SLOW_EXECUTE_SECS".to_string(), "5".to_string()));
        fe_config.request_timeout = Duration::from_secs(10);

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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("slow_a".to_string(), "test.ts".to_string());
        file_map.insert("slow_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(10),
            build_timeout: Duration::from_secs(30),
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: Some(Duration::from_millis(200)),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            parallel_scan(&fe_config, &analyses, &config),
        )
        .await
        .expect("scan should respect total timeout while a layer is active")
        .expect("parallel_scan should return skipped results");

        assert!(
            started.elapsed() < Duration::from_secs(2),
            "scan should return before the outer test timeout"
        );
        assert!(result.function_results.is_empty());
        assert_eq!(result.skipped.len(), 2);
        for skipped in &result.skipped {
            assert_eq!(skipped.reason, "timed out (total scan budget exceeded)");
        }
    }

    /// Regression test (str-quhk): when a frontend's response pipe is
    /// misaligned (e.g. from a duplicate response line), the poisoned frontend
    /// must NOT be returned to the worker pool. Before the fix, the first
    /// function to encounter an IdMismatch would return the frontend to the
    /// pool, where every subsequent checkout would also read a stale response
    /// and cascade IdMismatch errors across the entire scan.
    ///
    /// This test uses a mock frontend that injects a duplicate response line
    /// after the first execute, triggering an IdMismatch. With two functions
    /// sharing a single-worker pool, the second function must still get a
    /// clean outcome (not an IdMismatch cascade from the first function's
    /// poisoned frontend).
    #[tokio::test]
    async fn parallel_scan_drops_poisoned_frontend_instead_of_cascading() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mismatch_path = manifest_dir.join("../protocol/id-mismatch-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![mismatch_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);
        // Use a unique flag file per test run so the script only injects
        // a duplicate on the FIRST spawned process (the one that poisons
        // the pipe). Replacement workers spawned after the fix drops the
        // poisoned frontend will see the flag file and behave normally.
        let flag_file = std::env::temp_dir().join(format!(
            "shatter-id-mismatch-injected-{}",
            std::process::id()
        ));
        // Clean up from a previous run.
        let _ = std::fs::remove_file(&flag_file);
        fe_config.env_vars.push((
            "SHATTER_MISMATCH_FLAG".to_string(),
            flag_file.to_string_lossy().into_owned(),
        ));

        // Two functions in the same layer so both compete for pool workers.
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_a".to_string(), "test.ts".to_string());
        file_map.insert("fn_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            // Single worker: forces pool reuse, exposing cascade if the fix
            // doesn't drop the poisoned frontend.
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(30),
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed even when the first function hits IdMismatch");

        // The first function hits an IdMismatch (from the duplicate response)
        // and should be skipped/failed. The key assertion: the second function
        // must NOT report "response id N does not match request id M" — the
        // poisoned frontend must be dropped and replaced, so the second
        // function gets a clean frontend.
        let id_mismatch_count = result
            .skipped
            .iter()
            .filter(|s| s.reason.contains("response id") && s.reason.contains("does not match"))
            .count();

        // At most one function can hit the injected IdMismatch (the one that
        // used the poisoned frontend). The second function must NOT cascade.
        assert!(
            id_mismatch_count <= 1,
            "IdMismatch should not cascade across pool workers; \
             found {id_mismatch_count} IdMismatch skips: {:?}",
            result
                .skipped
                .iter()
                .map(|s| format!("{}: {}", s.function_name, s.reason))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn parallel_scan_retries_dead_idle_frontend_before_failing_function() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dead_once_path = manifest_dir.join("../protocol/dead-after-handshake-once-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![dead_once_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);
        let flag_file = std::env::temp_dir().join(format!(
            "shatter-dead-after-handshake-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&flag_file);
        fe_config.env_vars.push((
            "SHATTER_DEAD_AFTER_HANDSHAKE_FLAG".to_string(),
            flag_file.to_string_lossy().into_owned(),
        ));

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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_a".to_string(), "test.ts".to_string());
        file_map.insert("fn_b".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 1,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(30),
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should retry a dead idle frontend");

        assert_eq!(result.function_results.len(), 2);
        assert!(
            result.skipped.is_empty(),
            "no function should inherit the dead worker failure: {:?}",
            result.skipped
        );
    }

    #[tokio::test]
    async fn parallel_scan_retries_repeated_execute_frontend_exits() {
        use crate::frontend::FrontendConfig;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let flaky_path = manifest_dir.join("../protocol/execute-exits-twice-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![flaky_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = Duration::from_secs(10);
        let counter_file = std::env::temp_dir().join(format!(
            "shatter-execute-exits-twice-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&counter_file);
        fe_config.env_vars.push((
            "SHATTER_EXECUTE_EXIT_COUNTER".to_string(),
            counter_file.to_string_lossy().into_owned(),
        ));

        let analyses = vec![FunctionAnalysis {
            name: "fn_retry".to_string(),
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("fn_retry".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 1,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 1,
            timeout_per_fn: Duration::from_secs(30),
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should retry repeated transient frontend exits");

        assert_eq!(result.function_results.len(), 1);
        assert!(
            result.skipped.is_empty(),
            "no function should fail after two retryable frontend exits: {:?}",
            result.skipped
        );
        let attempts = std::fs::read_to_string(&counter_file)
            .expect("counter file should record execute attempts");
        assert_eq!(attempts, "3");
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
            concolic: false,
            seed: None,
            file_map,
            parallelism: 2,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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

    /// str-fuhw regression: two Go files each defining `Write` must
    /// surface as two distinct targets with their own file paths in the
    /// dry-run plan. Before the fix, scan internal maps (`file_map`,
    /// `analysis_map`) and `behavior::CallGraph::from_analyses` keyed on
    /// the bare function name; the second `Write` overwrote the first,
    /// so only one of the two ever appeared in `test_order` and the
    /// dry-run plan reported a single function with one file path.
    ///
    /// This test exercises the qualified-ID flow through:
    /// - `behavior::CallGraph::from_analyses` (must produce two nodes)
    /// - `format_dry_run_plan` (must list both `Write` entries with
    ///   distinct file location suffixes)
    /// - `analysis_map` keyed by qualified ID (must lookup both)
    /// - `scan_set` keyed by qualified ID (no collapse).
    #[test]
    fn dry_run_plan_distinguishes_duplicate_bare_names_across_go_files() {
        const FILE_A: &str = "src/pkg/a/io.go";
        const FILE_B: &str = "src/pkg/b/io.go";
        const FUNC_NAME: &str = "Write";

        // Two analyses sharing the bare name `Write`, each carrying its
        // own `source_file` so the qualified node ID becomes
        // `"<file>::Write"` and the call graph keeps them distinct.
        let mut write_a = make_analysis(FUNC_NAME, vec![]);
        write_a.source_file = Some(FILE_A.to_string());
        let mut write_b = make_analysis(FUNC_NAME, vec![]);
        write_b.source_file = Some(FILE_B.to_string());
        let analyses = vec![write_a, write_b];

        // file_map is keyed by qualified ID, mirroring the production
        // wiring in `rebuild_analyses_from_registry` (str-fuhw).
        let mut file_map = HashMap::new();
        file_map.insert(format!("{FILE_A}::{FUNC_NAME}"), FILE_A.to_string());
        file_map.insert(format!("{FILE_B}::{FUNC_NAME}"), FILE_B.to_string());

        let config = ScanConfig {
            max_iterations_per_function: 100,
            concolic: false,
            seed: None,
            file_map,
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let plan = format_dry_run_plan(&analyses, &[], &config).expect("should succeed");

        // Both functions must be listed.
        let write_count = plan.matches("Write(").count();
        assert_eq!(
            write_count, 2,
            "two Write functions in different files should appear as two \
             dry-run plan entries; got plan:\n{plan}",
        );

        // Each must carry its own file location suffix.
        assert!(
            plan.contains(&format!("[{FILE_A}]")),
            "plan should disambiguate the {FILE_A} Write via location suffix; \
             got plan:\n{plan}",
        );
        assert!(
            plan.contains(&format!("[{FILE_B}]")),
            "plan should disambiguate the {FILE_B} Write via location suffix; \
             got plan:\n{plan}",
        );

        // Plan summary must count two functions across two files.
        assert!(
            plan.contains("2 function(s) across 2 file(s)"),
            "plan summary should report 2 functions, 2 files; got plan:\n{plan}",
        );
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
            concolic: false,
            seed: None,
            file_map,
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
            reason: "param \"sock\" → net.Socket (network handle — requires live network binding)"
                .into(),
            category: SkipCategory::Expected,
        }];

        let config = ScanConfig {
            max_iterations_per_function: 100,
            concolic: false,
            seed: None,
            file_map: [("good".to_string(), "src/lib.ts".to_string())]
                .into_iter()
                .collect(),
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let plan = format_dry_run_plan(&analyses, &skipped, &config).expect("should succeed");

        assert!(plan.contains("Skipped (unexecutable)"));
        assert!(plan.contains("broken: param \"sock\" → net.Socket (network handle"));
    }

    #[test]
    fn dry_run_plan_empty_analyses() {
        let config = ScanConfig {
            max_iterations_per_function: 100,
            concolic: false,
            seed: None,
            file_map: HashMap::new(),
            parallelism: 1,
            timeout_per_fn: crate::frontend::DEFAULT_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
        use crate::batch_analyze::FunctionEntry;
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::core_sample::{self, CoreSampleConfig, SampleBudget};
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
                .map(|qn| {
                    qn.rsplit_once("::")
                        .map_or(qn.clone(), |(_, n)| n.to_string())
                })
                .collect();

        // Should have exactly 10 leaf functions.
        assert_eq!(
            stratum_names.len(),
            10,
            "stratum should select 10 leaf functions"
        );

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
        use crate::batch_analyze::FunctionEntry;
        use crate::call_graph::CallGraph as CgCallGraph;
        use crate::core_sample::{self, CoreSampleConfig, SampleBudget};
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
        use crate::batch_analyze::FunctionEntry;
        use crate::call_graph::CallGraph as CgCallGraph;
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
                .map(|qn| {
                    qn.rsplit_once("::")
                        .map_or(qn.clone(), |(_, n)| n.to_string())
                })
                .collect();

        assert_eq!(selected.len(), 1);
        assert!(selected.contains("fn_c"));
    }

    /// Verify stratum-excluded mock source is correctly assigned when
    /// scanning a middle layer whose callees are outside the selected stratum.
    #[test]
    fn stratum_excluded_mock_source_attribution() {
        use crate::batch_analyze::FunctionEntry;
        use crate::call_graph::CallGraph as CgCallGraph;
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
            .map(|qn| {
                qn.rsplit_once("::")
                    .map_or(qn.clone(), |(_, n)| n.to_string())
            })
            .collect();
        assert!(
            selected_bare.contains("fn_b"),
            "fn_b should be in selected stratum"
        );
        assert!(!selected_bare.contains("fn_a"), "fn_a should be excluded");
        assert!(!selected_bare.contains("fn_c"), "fn_c should be excluded");

        // fn_c is a callee of fn_b and excluded — should get StratumExcluded source.
        let excluded_bare: std::collections::HashSet<String> = excluded
            .iter()
            .map(|qn| {
                qn.rsplit_once("::")
                    .map_or(qn.clone(), |(_, n)| n.to_string())
            })
            .collect();
        assert!(
            excluded_bare.contains("fn_c"),
            "fn_c should be in excluded set"
        );
        assert!(
            excluded_bare.contains("fn_a"),
            "fn_a should be in excluded set"
        );

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
            MockUsage {
                name: "dep_a".into(),
                source: MockSource::CachedBehaviorMap,
            },
            MockUsage {
                name: "dep_b".into(),
                source: MockSource::TypeAwareStub,
            },
            MockUsage {
                name: "dep_c".into(),
                source: MockSource::StratumExcluded,
            },
        ];
        let formatted = format_mocks_used(&mocks);
        assert!(
            formatted.contains("behavior map"),
            "should mention behavior map"
        );
        assert!(
            formatted.contains("type-aware stub"),
            "should mention type-aware stub"
        );
        assert!(
            formatted.contains("stratum-excluded"),
            "should mention stratum-excluded"
        );
        assert!(formatted.contains("dep_a"));
        assert!(formatted.contains("dep_b"));
        assert!(formatted.contains("dep_c"));
    }

    // ── config candidate inputs ────────────────────────────────────

    fn load_test_config_candidate_inputs(
        func_name: &str,
        config_dir: &Option<PathBuf>,
        max_iterations: u32,
        timeout_secs: u64,
    ) -> Vec<Vec<serde_json::Value>> {
        let analysis = make_analysis_with_params(
            func_name,
            vec![],
            TypeInfo::Unknown,
            vec![],
        );
        load_config_function_inputs(
            &analysis,
            func_name,
            config_dir,
            max_iterations,
            timeout_secs,
        )
        .candidate_inputs
    }

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

        let result =
            load_test_config_candidate_inputs("myFunc", &Some(tmp.path().to_path_buf()), 100, 30);

        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            vec![serde_json::json!(42), serde_json::json!("hello")]
        );
        assert_eq!(result[1], vec![serde_json::json!(0), serde_json::json!("")]);
    }

    #[test]
    fn load_config_candidate_inputs_returns_empty_without_config_dir() {
        let result = load_test_config_candidate_inputs("myFunc", &None, 100, 30);
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

        let result =
            load_test_config_candidate_inputs("myFunc", &Some(tmp.path().to_path_buf()), 100, 30);

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn parallel_scan_skips_functions_marked_in_config() {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;
        use std::sync::{Arc, Mutex as StdMutex};

        let tmp = tempfile::tempdir().unwrap();
        let shatter_dir = tmp.path().join(".shatter");
        std::fs::create_dir_all(&shatter_dir).unwrap();
        std::fs::write(
            shatter_dir.join("config.yaml"),
            "functions:\n  solo:\n    skip: true\n",
        )
        .unwrap();

        let analysis = FunctionAnalysis {
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
            invocation_model: crate::protocol::InvocationModel::Direct,
            source_file: None,
            adapter_hints: vec![],
        };

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());
        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(99),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: None,
            config_dir: Some(tmp.path().to_path_buf()),
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let progress = Arc::new(move |update: ScanProgressUpdate| {
            sink_events.lock().unwrap().push(update);
        }) as ProgressHandler;

        let frontend_config = FrontendConfig::new(PathBuf::from("missing-frontend-not-used"));
        let result = parallel_scan_with_progress(
            &frontend_config,
            std::slice::from_ref(&analysis),
            &config,
            Some(progress),
        )
        .await
        .expect("config-skipped scan should not spawn a frontend");

        assert!(result.function_results.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].function_name, "solo");
        assert_eq!(result.skipped[0].reason, "skip=true in config");
        assert_eq!(result.skipped[0].category, SkipCategory::Expected);

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, ScanProgressStatus::Skipped);
        assert_eq!(events[0].function_name, "solo");
    }

    #[test]
    fn load_config_function_inputs_resolves_custom_value_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let shatter_dir = tmp.path().join(".shatter");
        let generators_dir = shatter_dir.join("generators");
        std::fs::create_dir_all(&generators_dir).unwrap();
        let generator_path = generators_dir.join("pickpackit.rs");
        std::fs::write(&generator_path, "// generator").unwrap();

        let config_yaml = "\
defaults:
  generators:
    State: generators/pickpackit.rs
  param_generators:
    current: generators/pickpackit.rs
";
        std::fs::write(shatter_dir.join("config.yaml"), config_yaml).unwrap();

        let analysis = make_analysis_with_params(
            "workspaces",
            vec![
                ParamInfo {
                    name: "state".into(),
                    typ: TypeInfo::Unknown,
                    type_name: Some("State".into()),
                },
                ParamInfo {
                    name: "current".into(),
                    typ: TypeInfo::Unknown,
                    type_name: Some("CurrentAccount".into()),
                },
            ],
            TypeInfo::Unknown,
            vec![],
        );

        let result = load_config_function_inputs(
            &analysis,
            "workspaces",
            &Some(tmp.path().to_path_buf()),
            100,
            30,
        );

        assert_eq!(result.value_sources.len(), 2);
        assert!(matches!(
            &result.value_sources[0],
            crate::input_gen::ValueSource::CustomGenerator {
                generator_name,
                param_name: None,
                generator_file,
                kind: crate::protocol::GeneratorKind::TypeName,
            } if generator_name == "State" && generator_file.as_path() == generator_path.as_path()
        ));
        assert!(matches!(
            &result.value_sources[1],
            crate::input_gen::ValueSource::CustomGenerator {
                generator_name,
                param_name: Some(param_name),
                generator_file,
                kind: crate::protocol::GeneratorKind::ParamName,
            } if generator_name == "current"
                && param_name == "current"
                && generator_file.as_path() == generator_path.as_path()
        ));
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
        let source = crate::fingerprint::extract_function_source(
            source_path,
            analysis.start_line,
            analysis.end_line,
        )
        .expect("extract source");
        let shallow = crate::fingerprint::compute_function_fingerprint(&source, analysis);
        crate::fingerprint::compute_deep_fingerprint(
            &shallow,
            &HashMap::new(),
            &std::collections::HashSet::new(),
        )
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
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        };

        // Compute the fingerprint parallel_scan will derive from the source file.
        let expected_fp = compute_expected_deep_fp(&source_file, &analysis);

        // Pre-seed the cache with a map whose fingerprint matches — this triggers
        // the is_fresh() path and skips the function without spawning a worker.
        let cache_dir = tmp_dir.path().join("cache");
        let cache = Arc::new(BehaviorMapCache::new(cache_dir).unwrap());
        cache
            .store(&make_cached_map("warm_fn", &expected_fp))
            .unwrap();

        // Use the noop frontend — it would succeed if spawned, but it must NOT be
        // spawned at all for a full-cache-hit scan.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        let mut file_map = HashMap::new();
        file_map.insert(
            "warm_fn".to_string(),
            source_file.to_string_lossy().into_owned(),
        );

        let config = ScanConfig {
            max_iterations_per_function: 3,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &[analysis], &config)
            .await
            .expect("parallel_scan should succeed");

        // Key assertion: no workers were ever spawned.
        assert_eq!(
            result.workers_used, 0,
            "warm cache should spawn zero workers"
        );
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
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        };

        let mut file_map = HashMap::new();
        file_map.insert(
            "warm_fn".to_string(),
            warm_source.to_string_lossy().into_owned(),
        );
        // stale_fn points to a nonexistent file → fingerprint is None → cache check skipped → always explored.
        file_map.insert("stale_fn".to_string(), "nonexistent.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
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
        assert!(
            result.workers_used >= 1,
            "stale function requires at least one worker"
        );
        // Pool was capped: 1 stale task with parallelism=4 → pool size 1.
        assert_eq!(
            result.workers_used, 1,
            "pool should be capped at tasks.len()"
        );
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            // High parallelism — but only 1 task exists, so pool must be capped at 1.
            parallelism: 8,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // Only 1 task → pool capped at 1, not 8.
        assert_eq!(
            result.workers_used, 1,
            "pool should be capped to tasks.len()=1, not parallelism=8"
        );
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        assert_eq!(
            result.function_results.len(),
            4,
            "all 4 functions must complete"
        );
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");

        // All tasks must complete — reaping must never cause deadlock.
        assert_eq!(
            result.function_results.len(),
            4,
            "all 4 functions must complete (no deadlock)"
        );

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

    /// If a timed-out worker is dropped and its replacement cannot be spawned,
    /// the pool must decrement live_count. Otherwise queued tasks can block
    /// forever on checkout even though no frontend process remains.
    #[tokio::test]
    async fn worker_pool_reaps_dead_slot_when_replacement_spawn_fails() {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;

        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let live_count = Arc::new(AtomicUsize::new(1));
        let idle_reaped = Arc::new(AtomicUsize::new(0));
        let config = Arc::new(FrontendConfig::new(PathBuf::from(
            "definitely-missing-shatter-frontend",
        )));

        let pool = WorkerPool {
            sender,
            receiver: Mutex::new(receiver),
            max_workers: 1,
            live_count: Arc::clone(&live_count),
            peak_size: Arc::new(AtomicUsize::new(1)),
            idle_reaped: Arc::clone(&idle_reaped),
            config,
        };

        pool.replace_dead_worker_if_needed(1).await;

        assert_eq!(
            live_count.load(Ordering::Relaxed),
            0,
            "failed replacement must release the dead worker slot",
        );
        assert_eq!(
            idle_reaped.load(Ordering::Relaxed),
            1,
            "failed replacement should be counted as a reaped dead slot",
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
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
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
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 2,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed across 3 layers");

        // All 3 functions must complete — deadlock would prevent this.
        assert_eq!(
            result.function_results.len(),
            3,
            "all 3 functions must complete"
        );
        assert!(result.skipped.is_empty(), "no functions should be skipped");

        // Each layer has 1 task → pool is capped at 1 per layer.
        // With the persistent pool the same 1 worker is reused for all 3 layers.
        assert_eq!(
            result.workers_used, 1,
            "single worker reused across all 3 layers"
        );

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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        };

        let leaf_names = ["leaf_a", "leaf_b", "leaf_c", "leaf_d"];
        let mut analyses: Vec<FunctionAnalysis> = leaf_names.iter().map(|n| make_leaf(n)).collect();

        // root depends on all 4 leaves → placed in layer 1.
        analyses.push(FunctionAnalysis {
            name: "root".to_string(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        });

        let mut file_map = HashMap::new();
        for name in leaf_names.iter().chain(["root"].iter()) {
            file_map.insert(name.to_string(), "test.ts".to_string());
        }

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed with shrinking layer transition");

        // All 5 functions (4 leaves + root) must complete — no deadlock.
        assert_eq!(
            result.function_results.len(),
            5,
            "all 5 functions must complete"
        );
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("alpha".to_string(), "test.ts".to_string());
        file_map.insert("beta".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4, // Serial policy must ignore this and use 1 worker.
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("serial policy scan should succeed");

        // All functions must be explored — serial policy doesn't skip anything.
        assert_eq!(
            result.function_results.len(),
            2,
            "both functions should complete"
        );
        assert!(result.skipped.is_empty(), "no functions should be skipped");

        // Serial enforces 1 effective worker regardless of configured parallelism.
        assert_eq!(
            result.workers_used, 1,
            "serial policy must use exactly 1 worker"
        );
    }

    /// str-poyv: `--scheduler-policy serial` must not emit overlapping
    /// `Started` events. Before the fix, all queued tasks called
    /// `emit_progress(Started)` immediately after `tokio::spawn` and
    /// before `pool.checkout().await`, so a 4-function layer scanned
    /// under Serial would emit four `Started` events in rapid
    /// succession (all at the same elapsed_ms) followed by four
    /// `Completed` events. The fix moves the emission to AFTER
    /// checkout, which serializes Started/Completed pairs.
    #[tokio::test]
    async fn serial_policy_does_not_overlap_started_events() {
        use crate::frontend::FrontendConfig;
        use crate::scheduler_policy::SchedulerPolicy;
        use crate::types::{ParamInfo, TypeInfo};
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, Mutex as StdMutex};

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;

        // Four independent leaf functions land in the same layer; under
        // Serial the pool size is 1 so they must execute strictly one
        // after another.
        let make_leaf = |name: &str| FunctionAnalysis {
            name: name.to_string(),
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
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        };
        let analyses: Vec<FunctionAnalysis> = ["alpha", "beta", "gamma", "delta"]
            .iter()
            .map(|n| make_leaf(n))
            .collect();

        let mut file_map = HashMap::new();
        for name in ["alpha", "beta", "gamma", "delta"] {
            file_map.insert(name.to_string(), "test.ts".to_string());
        }

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(7),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let progress = Arc::new(move |update: ScanProgressUpdate| {
            sink.lock().unwrap().push(update);
        }) as ProgressHandler;

        parallel_scan_with_progress(&fe_config, &analyses, &config, Some(progress))
            .await
            .expect("serial scan should succeed");

        let events = events.lock().unwrap();

        // Invariant: at any point in the stream, the number of Started
        // events without a matching terminal event must be ≤ 1. Under
        // the old behavior all four Started events fired upfront and
        // the in-flight count peaked at 4.
        let mut in_flight: i64 = 0;
        let mut peak: i64 = 0;
        for ev in events.iter() {
            match ev.status {
                ScanProgressStatus::Started => in_flight += 1,
                ScanProgressStatus::Completed
                | ScanProgressStatus::Failed
                | ScanProgressStatus::Skipped => in_flight -= 1,
            }
            peak = peak.max(in_flight);
            assert!(
                in_flight >= 0,
                "terminal event without matching Started; events={:?}",
                events
                    .iter()
                    .map(|e| (e.status.as_str(), e.function_name.as_str()))
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(
            peak,
            1,
            "serial policy must emit at most one Started in flight at a time; \
             got peak={peak}. events={:?}",
            events
                .iter()
                .map(|e| (e.status.as_str(), e.function_name.as_str()))
                .collect::<Vec<_>>(),
        );

        // Sanity: every function emitted a Started + terminal pair.
        let started_count = events
            .iter()
            .filter(|e| e.status == ScanProgressStatus::Started)
            .count();
        assert_eq!(started_count, 4, "expected 4 Started events");
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
            FunctionAnalysis {
                name: "fn_two".to_string(),
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("fn_one".to_string(), "test.ts".to_string());
        file_map.insert("fn_two".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 4,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan with Function isolation should succeed");

        // Both independent functions must be explored.
        assert_eq!(
            result.function_results.len(),
            2,
            "both functions should complete"
        );
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
            FunctionAnalysis {
                name: "caller_fn".to_string(),
                exported: true,
                params: vec![ParamInfo {
                    name: "y".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                }],
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
                adapter_hints: vec![],
                invocation_model: crate::protocol::InvocationModel::Direct,
            },
        ];

        let mut file_map = HashMap::new();
        file_map.insert("leaf_fn".to_string(), "test.ts".to_string());
        file_map.insert("caller_fn".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(42),
            file_map,
            parallelism: 2,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
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
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan with Function isolation should succeed");

        // Both functions must be explored; no errors.
        assert_eq!(
            result.function_results.len(),
            2,
            "both functions should complete"
        );
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
        assert_eq!(
            pool.live_count.load(Ordering::Relaxed),
            3,
            "should not reap when at floor"
        );
        assert_eq!(pool.idle_reaped(), 0);

        // Checkout and return with pending=1.
        // floor = min(1 + 1, 3) = 2. current=3 > floor → should reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 1).await;
        assert_eq!(
            pool.live_count.load(Ordering::Relaxed),
            2,
            "should reap one worker when above floor"
        );
        assert_eq!(pool.idle_reaped(), 1);

        // Checkout and return with pending=0.
        // floor = min(0 + 1, 3) = 1. current=2 > floor → should reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 0).await;
        assert_eq!(
            pool.live_count.load(Ordering::Relaxed),
            1,
            "should reap to floor"
        );
        assert_eq!(pool.idle_reaped(), 2);

        // Checkout and return with pending=0 again.
        // floor = 1. current=1 == floor → no reap.
        let w = pool.checkout().await;
        pool.return_or_reap_worker(w, 0).await;
        assert_eq!(
            pool.live_count.load(Ordering::Relaxed),
            1,
            "should not reap below floor"
        );
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
        let prewarmed = Frontend::spawn(&fe_config)
            .await
            .expect("spawn should succeed");

        // Create pool with prewarmed worker, needing 1 worker, max 2.
        let pool = WorkerPool::spawn_capped(Arc::clone(&fe_config), 2, 1, Some(prewarmed))
            .await
            .expect("pool should spawn");

        assert_eq!(
            pool.live_count.load(Ordering::Relaxed),
            1,
            "pool should have 1 worker from prewarmed"
        );

        // Checkout should succeed without blocking indefinitely.
        let worker = pool.checkout().await;
        pool.return_or_reap_worker(worker, 1).await;

        pool.shutdown().await;
    }

    #[tokio::test]
    async fn worker_task_lease_recovers_checked_out_worker_after_abort() {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;

        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut fe_config = FrontendConfig::new(PathBuf::from("bash"));
        fe_config.args = vec![noop_path.to_string_lossy().into_owned()];
        fe_config.request_timeout = TEST_REQUEST_TIMEOUT;
        let fe_config = Arc::new(fe_config);

        let pool = Arc::new(
            WorkerPool::spawn_capped(Arc::clone(&fe_config), 1, 2, None)
                .await
                .expect("pool should spawn"),
        );
        let tasks_remaining = Arc::new(AtomicUsize::new(2));
        let lease = Arc::new(WorkerTaskLease::new(
            Arc::clone(&pool),
            Arc::clone(&tasks_remaining),
        ));

        let checked_out = pool.checkout().await;
        lease.mark_checked_out();
        drop(checked_out);

        let waiter_pool = Arc::clone(&pool);
        let mut waiter = tokio::spawn(async move { waiter_pool.checkout().await });

        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut waiter)
                .await
                .is_err(),
            "checkout should block before the aborted lease restores capacity"
        );

        let remaining = lease.recover_after_abort().await;
        assert_eq!(remaining, 1);

        let replacement = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("checkout should be woken by abort recovery")
            .expect("checkout task should not panic");
        pool.return_or_reap_worker(replacement, 0).await;

        drop(lease);
        drop(tasks_remaining);
        if let Ok(pool) = Arc::try_unwrap(pool) {
            pool.shutdown().await;
        }
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
        raw_results: Vec<(
            Vec<serde_json::Value>,
            Vec<crate::protocol::MockConfig>,
            crate::protocol::ExecuteResult,
        )>,
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
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            ..Default::default()
        };
        let analysis = make_analysis(func_name, vec![]);
        let analyze_out = analyze_exploration(&exploration, &analysis, None);
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
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            }],
            scope_events: vec![],
            loop_body_states: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            return_value: Some(serde_json::Value::Null),
            thrown_error: None,
            side_effects: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        }
    }

    #[test]
    fn write_completed_scan_artifact_persists_function_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let result =
            make_function_result("scan_fn", vec![(vec![], vec![], make_execute_result(1))]);

        write_completed_scan_artifact(Some(&root), 2, 5, "src/scan.ts", &result);

        let path = scan_artifact_path(&root, 2, "scan_fn");
        let json = std::fs::read_to_string(path).expect("read artifact");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");

        assert_eq!(value["status"], "completed");
        assert_eq!(value["current"], 2);
        assert_eq!(value["function"]["function_name"], "scan_fn");
        assert_eq!(value["function"]["file_path"], "src/scan.ts");
    }

    #[test]
    fn write_skipped_scan_artifact_persists_reason_and_category() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();

        write_skipped_scan_artifact(
            Some(&root),
            3,
            7,
            "scan_fn",
            "resumed from checkpoint",
            SkipCategory::Expected,
        );

        let path = scan_artifact_path(&root, 3, "scan_fn");
        let json = std::fs::read_to_string(path).expect("read artifact");
        let value: serde_json::Value = serde_json::from_str(&json).expect("json");

        assert_eq!(value["status"], "skipped");
        assert_eq!(value["reason"], "resumed from checkpoint");
        assert_eq!(value["category"], "expected");
    }

    #[test]
    fn merge_replica_results_single_passthrough() {
        // A single-element merge should return an equivalent result.
        let result = make_function_result("foo", vec![(vec![], vec![], make_execute_result(1))]);
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
    fn merge_replica_outcomes_preserves_unsupported_variant() {
        // str-31j.4: when every replica of a function reports the target as
        // unsupported, the merged outcome must remain FunctionOutcome::Unsupported
        // so the scan layer can route it to SkipCategory::Unsupported.
        let analysis_owned = make_analysis("middleware_fn", vec![]);
        let analysis_map: std::collections::HashMap<&str, &FunctionAnalysis> =
            std::iter::once(("middleware_fn", &analysis_owned)).collect();

        let outcomes = vec![FunctionOutcome::Unsupported {
            function_name: "middleware_fn".into(),
            reason: "axum middleware not supported: Request, Next".into(),
        }];

        let merged = merge_replica_outcomes(outcomes, &analysis_map);
        assert_eq!(merged.len(), 1);
        match &merged[0] {
            FunctionOutcome::Unsupported {
                function_name,
                reason,
            } => {
                assert_eq!(function_name, "middleware_fn");
                assert!(reason.contains("axum middleware not supported"), "got: {reason}");
            }
            other => panic!("expected Unsupported, got: {other:?}"),
        }
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
    fn make_execute_result_with_calls(
        calls: Vec<crate::execution_record::ExternalCall>,
    ) -> crate::protocol::ExecuteResult {
        use crate::execution_record::{BranchDecision, SymConstraint};
        crate::protocol::ExecuteResult {
            branch_path: vec![BranchDecision {
                branch_id: 1,
                line: 1,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            }],
            scope_events: vec![],
            loop_body_states: vec![],
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
            outcome: None,
        }
    }

    /// Build a `BehaviorMap` with the given input_args entries as behaviors.
    fn make_behavior_map_with_inputs(
        function_id: &str,
        known_inputs: Vec<Vec<serde_json::Value>>,
    ) -> BehaviorMap {
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
        assert!(
            misses.is_empty(),
            "expected no miss when args match behavior map"
        );
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
        assert_eq!(
            misses.len(),
            1,
            "duplicate missed args should be deduplicated"
        );
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
                        opaque_suggestions: vec![],
                        stubbed_modules: vec![],
                        ..Default::default()
                    },
                    behavior_map: BehaviorMap {
                        function_id: "leaf".into(),
                        behaviors: vec![],
                        fingerprint: None,
                        nondeterministic_fields: vec![],
                    },
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
                        opaque_suggestions: vec![],
                        stubbed_modules: vec![],
                        ..Default::default()
                    },
                    behavior_map: BehaviorMap {
                        function_id: "caller".into(),
                        behaviors: vec![],
                        fingerprint: None,
                        nondeterministic_fields: vec![],
                    },
                    behavior_coverage: vec![],
                    mocks_used: vec![MockUsage {
                        name: "leaf".into(),
                        source: MockSource::CachedBehaviorMap,
                    }],
                    mock_misses: vec![miss],
                    coverage_metrics: Default::default(),
                    refactoring_recommendations: vec![],
                },
            ],
            skipped_functions: vec![],
            sampling: None,
            source_files: vec![],
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

    /// Regression test: GA discoveries must be merged into the behavior map
    /// produced by the scan pipeline. Before the fix for str-1wg, both scan
    /// paths logged discoveries but never called `merge_ga_discoveries()`,
    /// silently discarding GA-found behaviors.
    #[test]
    fn scan_ga_discoveries_are_merged_into_behavior_map() {
        use crate::behavior::{Behavior, BehaviorMap};
        use crate::execution_record::{BranchDecision, SymConstraint};

        fn make_branch(branch_id: u32, taken: bool) -> BranchDecision {
            BranchDecision {
                branch_id,
                line: branch_id * 10,
                taken,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
                conditions: None,
            }
        }

        // Simulate the behavior map that pipeline::analyze would produce
        // (containing only concolic-discovered behaviors).
        let mut behavior_map = BehaviorMap {
            function_id: "test_fn".into(),
            behaviors: vec![Behavior {
                id: 0,
                input_args: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("concolic")),
                thrown_error: None,
                branch_path: vec![make_branch(1, true)],
                side_effects: vec![],
                dependency_trace: None,
                mock_values: vec![],
            }],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };

        // Simulate GA discoveries: one new path (branch 2) and one duplicate
        // (branch 1, already in the map).
        let ga_discoveries = vec![
            Behavior {
                id: 0,
                input_args: vec![serde_json::json!(99)],
                return_value: Some(serde_json::json!("ga_new")),
                thrown_error: None,
                branch_path: vec![make_branch(2, false)],
                side_effects: vec![],
                dependency_trace: None,
                mock_values: vec![],
            },
            Behavior {
                id: 1,
                input_args: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("ga_dup")),
                thrown_error: None,
                branch_path: vec![make_branch(1, true)],
                side_effects: vec![],
                dependency_trace: None,
                mock_values: vec![],
            },
        ];

        // This is the exact pattern used in scan_orchestrator after the fix.
        let added = behavior_map.merge_ga_discoveries(&ga_discoveries);

        assert_eq!(added, 1, "only the novel GA discovery should be added");
        assert_eq!(
            behavior_map.behaviors.len(),
            2,
            "1 concolic + 1 GA discovery"
        );
        assert_eq!(
            behavior_map.behaviors[1].return_value,
            Some(serde_json::json!("ga_new")),
            "the new GA behavior should be the one with branch 2"
        );
    }

    #[test]
    fn has_scan_failure_all_skipped() {
        let result = ParallelScanResult {
            function_results: vec![],
            test_order: vec!["foo".into()],
            skipped: vec![SkippedFunction {
                function_name: "foo".into(),
                reason: "build failed".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        assert!(
            result.has_scan_failure(),
            "0 explored out of 1 attempted must be a failure"
        );
    }

    #[test]
    fn has_scan_failure_some_explored() {
        let result = ParallelScanResult {
            function_results: vec![make_function_result("bar", vec![])],
            test_order: vec!["bar".into(), "baz".into()],
            skipped: vec![SkippedFunction {
                function_name: "baz".into(),
                reason: "timeout".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        assert!(
            !result.has_scan_failure(),
            "1 explored out of 2 attempted is not a total failure"
        );
    }

    #[test]
    fn evaluate_failure_policy_default_passes_partial_failures() {
        let result = ParallelScanResult {
            function_results: vec![make_function_result("ok", vec![])],
            test_order: vec!["ok".into(), "bad".into()],
            skipped: vec![SkippedFunction {
                function_name: "bad".into(),
                reason: "timeout".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        assert!(
            result
                .evaluate_failure_policy(ScanFailurePolicy::default())
                .is_none(),
            "default permissive policy must not flag partial failures",
        );
    }

    #[test]
    fn evaluate_failure_policy_fail_on_failures_flags_any_failure() {
        let result = ParallelScanResult {
            function_results: vec![make_function_result("ok", vec![])],
            test_order: vec!["ok".into(), "bad".into()],
            skipped: vec![SkippedFunction {
                function_name: "bad".into(),
                reason: "panic".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        let reason = result.evaluate_failure_policy(ScanFailurePolicy {
            fail_on_failures: true,
            failure_threshold_percent: None,
        });
        assert!(
            reason.as_deref().is_some_and(|r| r.contains("1 of 2")),
            "fail-on-failures must name the failed/attempted counts; got: {reason:?}",
        );
    }

    #[test]
    fn evaluate_failure_policy_threshold_allows_under_limit() {
        // 1 failed out of 4 attempted = 25%, threshold 50 should pass.
        let result = ParallelScanResult {
            function_results: vec![
                make_function_result("a", vec![]),
                make_function_result("b", vec![]),
                make_function_result("c", vec![]),
            ],
            test_order: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            skipped: vec![SkippedFunction {
                function_name: "d".into(),
                reason: "timeout".into(),
                category: SkipCategory::Error,
            }],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        assert!(
            result
                .evaluate_failure_policy(ScanFailurePolicy {
                    fail_on_failures: false,
                    failure_threshold_percent: Some(50),
                })
                .is_none(),
            "25% failure rate must satisfy a 50% threshold",
        );
    }

    #[test]
    fn evaluate_failure_policy_threshold_trips_over_limit() {
        // 3 failed out of 4 attempted = 75%, threshold 50 should fail.
        let result = ParallelScanResult {
            function_results: vec![make_function_result("a", vec![])],
            test_order: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            skipped: vec![
                SkippedFunction {
                    function_name: "b".into(),
                    reason: "timeout".into(),
                    category: SkipCategory::Error,
                },
                SkippedFunction {
                    function_name: "c".into(),
                    reason: "panic".into(),
                    category: SkipCategory::Error,
                },
                SkippedFunction {
                    function_name: "d".into(),
                    reason: "error".into(),
                    category: SkipCategory::Error,
                },
            ],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        let reason = result.evaluate_failure_policy(ScanFailurePolicy {
            fail_on_failures: false,
            failure_threshold_percent: Some(50),
        });
        assert!(
            reason
                .as_deref()
                .is_some_and(|r| r.contains("75.0%") && r.contains("--failure-threshold 50")),
            "threshold breach must name the rate and limit; got: {reason:?}",
        );
    }

    #[test]
    fn counts_separate_unsupported_and_failed() {
        let result = ParallelScanResult {
            function_results: vec![make_function_result("ok", vec![])],
            test_order: vec!["ok".into(), "bad".into(), "u".into()],
            skipped: vec![
                SkippedFunction {
                    function_name: "bad".into(),
                    reason: "panic".into(),
                    category: SkipCategory::Error,
                },
                SkippedFunction {
                    function_name: "u".into(),
                    reason: "unexecutable param".into(),
                    category: SkipCategory::Unsupported,
                },
            ],
            workers_used: 1,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        let counts = result.counts();
        assert_eq!(counts.completed, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.unsupported, 1);
        assert_eq!(counts.expected_skips, 0);
    }

    #[test]
    fn has_scan_failure_nothing_attempted() {
        let result = ParallelScanResult {
            function_results: vec![],
            test_order: vec![],
            skipped: vec![],
            workers_used: 0,
            workers_reaped: 0,
            sampling: None,
            source_files: vec![],
        };
        assert!(
            !result.has_scan_failure(),
            "0 attempted means nothing to fail"
        );
    }

    // --- Scan summary tests ---

    #[test]
    fn scan_summary_new_has_running_status() {
        let summary = new_scan_summary("test-scan-id", 5);
        assert_eq!(summary.version, SCAN_SUMMARY_VERSION);
        assert_eq!(summary.scan_id, "test-scan-id");
        assert_eq!(summary.status, ScanRunStatus::Running);
        assert_eq!(summary.total_functions, 5);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
        assert!(summary.functions.is_empty());
    }

    #[test]
    fn scan_summary_record_completed_increments_count() {
        let mut summary = new_scan_summary("s1", 3);
        summary_record_completed(&mut summary, "fn_a", 1, Duration::from_secs(2));
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.functions.len(), 1);
        assert_eq!(summary.functions[0].status, "completed");
        assert_eq!(summary.functions[0].function_name, "fn_a");
        assert_eq!(summary.functions[0].index, 1);
        assert!(summary.functions[0].artifact.is_some());
        assert!(summary.functions[0].reason.is_none());
    }

    #[test]
    fn scan_summary_record_skipped_increments_count() {
        let mut summary = new_scan_summary("s1", 3);
        summary_record_skipped(
            &mut summary,
            "fn_b",
            2,
            "cache hit",
            SkipCategory::Expected,
            Duration::from_secs(1),
        );
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.functions[0].status, "skipped");
        assert_eq!(summary.functions[0].reason.as_deref(), Some("cache hit"));
    }

    #[test]
    fn scan_summary_record_failed_increments_count() {
        let mut summary = new_scan_summary("s1", 3);
        summary_record_failed(&mut summary, "fn_c", 3, "timeout", Duration::from_secs(5));
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.functions[0].status, "failed");
        assert_eq!(summary.functions[0].reason.as_deref(), Some("timeout"));
    }

    #[test]
    fn scan_summary_finalize_completed_when_has_successes() {
        let mut summary = new_scan_summary("s1", 2);
        summary_record_completed(&mut summary, "fn_a", 1, Duration::from_secs(1));
        summary_record_failed(&mut summary, "fn_b", 2, "error", Duration::from_secs(2));
        summary_finalize(&mut summary, Duration::from_secs(3));
        // Has at least one success, so status is Completed (not Failed).
        assert_eq!(summary.status, ScanRunStatus::Completed);
    }

    #[test]
    fn scan_summary_finalize_failed_when_no_successes() {
        let mut summary = new_scan_summary("s1", 2);
        summary_record_failed(&mut summary, "fn_a", 1, "err1", Duration::from_secs(1));
        summary_record_failed(&mut summary, "fn_b", 2, "err2", Duration::from_secs(2));
        summary_finalize(&mut summary, Duration::from_secs(3));
        assert_eq!(summary.status, ScanRunStatus::Failed);
    }

    #[test]
    fn write_scan_summary_produces_valid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let mut summary = new_scan_summary("test-id", 2);
        summary_record_completed(&mut summary, "fn_a", 1, Duration::from_secs(1));
        summary_record_skipped(
            &mut summary,
            "fn_b",
            2,
            "cached",
            SkipCategory::Expected,
            Duration::from_secs(2),
        );
        summary_finalize(&mut summary, Duration::from_secs(2));
        write_scan_summary(&root, &summary);

        let path = root.join("summary.json");
        assert!(path.exists(), "summary.json should exist");
        let json = std::fs::read_to_string(&path).expect("read");
        let parsed: ScanSummary = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.scan_id, "test-id");
        assert_eq!(parsed.status, ScanRunStatus::Completed);
        assert_eq!(parsed.completed, 1);
        assert_eq!(parsed.skipped, 1);
        assert_eq!(parsed.functions.len(), 2);
    }

    #[test]
    fn write_scan_summary_atomic_no_tmp_left() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let summary = new_scan_summary("test-id", 0);
        write_scan_summary(&root, &summary);

        let tmp = root.join("summary.json.tmp");
        assert!(!tmp.exists(), "temp file should not persist after rename");
    }

    #[test]
    fn scan_summary_artifact_references_match_function_artifacts() {
        let mut summary = new_scan_summary("s1", 2);
        summary_record_completed(&mut summary, "myFunc", 3, Duration::from_secs(1));

        let entry = &summary.functions[0];
        // The artifact path should match what scan_artifact_path would produce.
        let expected = format!(
            "functions/{:05}_{}.json",
            3,
            sanitize_artifact_component("myFunc")
        );
        assert_eq!(entry.artifact.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn scan_summary_roundtrip_serde() {
        let mut summary = new_scan_summary("rt-test", 3);
        summary_record_completed(&mut summary, "a", 1, Duration::from_secs(1));
        summary_record_failed(&mut summary, "b", 2, "crash", Duration::from_secs(2));
        summary_record_skipped(
            &mut summary,
            "c",
            3,
            "cached",
            SkipCategory::Expected,
            Duration::from_secs(3),
        );
        summary_finalize(&mut summary, Duration::from_secs(3));

        let json = serde_json::to_string(&summary).expect("serialize");
        let parsed: ScanSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.scan_id, "rt-test");
        assert_eq!(parsed.status, ScanRunStatus::Completed);
        assert_eq!(parsed.completed, 1);
        assert_eq!(parsed.failed, 1);
        assert_eq!(parsed.skipped, 1);
        assert_eq!(parsed.functions.len(), 3);
    }

    #[test]
    fn build_summary_from_scan_result_captures_all_functions() {
        let results = vec![
            make_function_result("fn_a", vec![(vec![], vec![], make_execute_result(1))]),
            make_function_result("fn_b", vec![(vec![], vec![], make_execute_result(2))]),
        ];
        let skipped = vec![SkippedFunction {
            function_name: "fn_c".into(),
            reason: "opaque types".into(),
            category: SkipCategory::Expected,
        }];
        let scan_result = ScanResult {
            function_results: results,
            test_order: vec!["fn_a".into(), "fn_b".into(), "fn_c".into()],
            skipped_functions: skipped,
            sampling: None,
            source_files: vec![],
        };
        let summary =
            build_summary_from_scan_result("scan-1", &scan_result, Duration::from_secs(10));
        assert_eq!(summary.status, ScanRunStatus::Completed);
        assert_eq!(summary.completed, 2);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.functions.len(), 3);
    }

    // --- SchedulerStateCache wiring (str-bo4z.5) ---

    #[test]
    fn persist_scheduler_state_if_exhausted_writes_on_exhaustion() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SchedulerStateCache::new(dir.path().to_path_buf()).unwrap());
        let cache_opt: Option<Arc<SchedulerStateCache>> = Some(Arc::clone(&cache));

        let state = SchedulerState {
            function_id: "pkg:fn_alpha".into(),
            iterations_consumed: 42,
            batches_completed: 3,
            exhausted: true,
            ..SchedulerState::default()
        };
        let mut persisted = false;

        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &state,
            &mut persisted,
            CoverageMode::Branch.as_str(),
        );
        assert!(persisted, "first call on an exhausted state must persist");

        let loaded = cache
            .load("pkg:fn_alpha", CoverageMode::Branch.as_str())
            .unwrap()
            .expect("stored state must round-trip");
        assert_eq!(loaded.iterations_consumed, 42);
        assert_eq!(loaded.batches_completed, 3);
        assert!(loaded.exhausted);
    }

    #[test]
    fn persist_scheduler_state_if_exhausted_is_idempotent() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SchedulerStateCache::new(dir.path().to_path_buf()).unwrap());
        let cache_opt: Option<Arc<SchedulerStateCache>> = Some(Arc::clone(&cache));

        let state = SchedulerState {
            function_id: "fn_once".into(),
            exhausted: true,
            ..SchedulerState::default()
        };
        let mut persisted = false;

        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &state,
            &mut persisted,
            CoverageMode::Branch.as_str(),
        );
        assert!(persisted);

        // Second call with a mutated state should not re-write the
        // cache entry — the persisted flag prevents it.
        let state2 = SchedulerState {
            function_id: "fn_once".into(),
            iterations_consumed: 999,
            exhausted: true,
            ..SchedulerState::default()
        };
        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &state2,
            &mut persisted,
            CoverageMode::Branch.as_str(),
        );

        let loaded = cache
            .load("fn_once", CoverageMode::Branch.as_str())
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded.iterations_consumed, 0,
            "idempotent helper must not overwrite persisted state"
        );
    }

    #[test]
    fn persist_scheduler_state_if_exhausted_skips_non_exhausted() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SchedulerStateCache::new(dir.path().to_path_buf()).unwrap());
        let cache_opt: Option<Arc<SchedulerStateCache>> = Some(Arc::clone(&cache));

        let state = SchedulerState {
            function_id: "fn_mid".into(),
            exhausted: false,
            ..SchedulerState::default()
        };
        let mut persisted = false;

        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &state,
            &mut persisted,
            CoverageMode::Branch.as_str(),
        );
        assert!(
            !persisted,
            "non-exhausted states are flushed at layer end, not on-outcome"
        );
        assert!(
            cache
                .load("fn_mid", CoverageMode::Branch.as_str())
                .unwrap()
                .is_none()
        );
    }

    // --- str-bo4z.2: body-change invalidation regression guards ---
    //
    // The full `run_layer_batched` requires a live frontend subprocess so
    // we can't drive it from a unit test. Instead, these tests exercise
    // the cache call sequence the load loop uses (`load_if_fresh` keyed
    // on the task's `deep_fp`) to lock in the contract.

    #[test]
    fn load_if_fresh_clears_stale_state_for_changed_function() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();
        let mode = CoverageMode::Branch.as_str();

        // Pre-populate as if a previous run had partially explored
        // `pkg:fn_changed` and recorded substantial progress under
        // fingerprint OLD.
        let prior = SchedulerState {
            function_id: "pkg:fn_changed".into(),
            fingerprint: Some("OLD".into()),
            iterations_consumed: 200,
            batches_completed: 7,
            exhausted: true,
            ..SchedulerState::default()
        };
        cache.store(&prior, mode).unwrap();

        // Same call shape the load loop uses when the task's current
        // deep_fp is "NEW".
        let loaded = cache.load_if_fresh("pkg:fn_changed", mode, "NEW").unwrap();
        assert_eq!(
            loaded, None,
            "body change must drop stale state so the function returns to the queue unexplored"
        );

        // Idempotency: a second pass observes a clean cache miss.
        let loaded_again = cache.load_if_fresh("pkg:fn_changed", mode, "NEW").unwrap();
        assert_eq!(loaded_again, None);

        // The on-disk file is gone — a plain `load` also reports a miss.
        assert_eq!(cache.load("pkg:fn_changed", mode).unwrap(), None);
    }

    #[test]
    fn load_if_fresh_preserves_state_for_unchanged_function() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();
        let mode = CoverageMode::Branch.as_str();

        let prior = SchedulerState {
            function_id: "pkg:fn_stable".into(),
            fingerprint: Some("FP".into()),
            iterations_consumed: 75,
            batches_completed: 4,
            exhausted: false,
            ..SchedulerState::default()
        };
        cache.store(&prior, mode).unwrap();

        let loaded = cache
            .load_if_fresh("pkg:fn_stable", mode, "FP")
            .unwrap()
            .expect("matching fingerprint must reload state");
        assert_eq!(loaded.iterations_consumed, 75);
        assert_eq!(loaded.batches_completed, 4);
        assert!(!loaded.exhausted);
    }

    #[test]
    fn load_if_fresh_does_not_disturb_sibling_functions() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = SchedulerStateCache::new(dir.path().to_path_buf()).unwrap();
        let mode = CoverageMode::Branch.as_str();

        let changed = SchedulerState {
            function_id: "pkg:fn_changed".into(),
            fingerprint: Some("OLD".into()),
            iterations_consumed: 50,
            ..SchedulerState::default()
        };
        let stable = SchedulerState {
            function_id: "pkg:fn_stable".into(),
            fingerprint: Some("STABLE".into()),
            iterations_consumed: 100,
            ..SchedulerState::default()
        };
        cache.store(&changed, mode).unwrap();
        cache.store(&stable, mode).unwrap();

        // Invalidate the changed function.
        let dropped = cache.load_if_fresh("pkg:fn_changed", mode, "NEW").unwrap();
        assert_eq!(dropped, None);

        // Sibling is fully recoverable under its own (matching) fingerprint.
        let kept = cache
            .load_if_fresh("pkg:fn_stable", mode, "STABLE")
            .unwrap()
            .expect("unrelated function must keep its state");
        assert_eq!(kept.iterations_consumed, 100);
    }

    #[test]
    fn persist_scheduler_state_if_exhausted_noop_when_cache_absent() {
        use crate::cache::SchedulerState;
        use crate::interesting_pool::CoverageMode;

        let cache_opt: Option<Arc<crate::cache::SchedulerStateCache>> = None;
        let state = SchedulerState {
            function_id: "fn_none".into(),
            exhausted: true,
            ..SchedulerState::default()
        };
        let mut persisted = false;

        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &state,
            &mut persisted,
            CoverageMode::Branch.as_str(),
        );
        assert!(
            !persisted,
            "cache=None must not flip persisted flag — nothing was stored"
        );
    }

    /// str-bo4z.7: branch-mode and MC/DC-mode scheduler states are independent.
    #[test]
    fn persist_scheduler_state_partitions_by_mode() {
        use crate::cache::{SchedulerState, SchedulerStateCache};
        use crate::interesting_pool::CoverageMode;

        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(SchedulerStateCache::new(dir.path().to_path_buf()).unwrap());
        let cache_opt: Option<Arc<SchedulerStateCache>> = Some(Arc::clone(&cache));

        let branch_state = SchedulerState {
            function_id: "pkg:my_func".into(),
            exhausted: true,
            iterations_consumed: 100,
            mode: Some("branch".into()),
            ..SchedulerState::default()
        };
        let mut branch_persisted = false;
        persist_scheduler_state_if_exhausted(
            &cache_opt,
            &branch_state,
            &mut branch_persisted,
            CoverageMode::Branch.as_str(),
        );
        assert!(branch_persisted);

        // MC/DC mode for the same function must be a cache miss.
        assert!(
            cache
                .load("pkg:my_func", CoverageMode::Mcdc.as_str())
                .unwrap()
                .is_none(),
            "branch-mode exhaustion must not contaminate MC/DC mode"
        );

        // Branch mode must round-trip.
        let loaded = cache
            .load("pkg:my_func", CoverageMode::Branch.as_str())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.iterations_consumed, 100);
        assert!(loaded.exhausted);
    }

    // ---- str-bo4z.6: compute_uncovered_branch_strings tests ----

    fn make_branch_info(id: u32, line: u32, condition: &str) -> crate::protocol::BranchInfo {
        crate::protocol::BranchInfo {
            id,
            line,
            condition_text: condition.to_string(),
            condition: None,
            branch_type: crate::protocol::BranchType::If,
        }
    }

    fn make_analysis_with_branches(branches: Vec<crate::protocol::BranchInfo>) -> FunctionAnalysis {
        let mut analysis = make_analysis("test_fn", vec![]);
        analysis.branches = branches;
        analysis
    }

    fn make_observation_with_discoveries(
        discoveries: Vec<(u32, crate::coverage_metrics::DiscoveryMethod)>,
    ) -> crate::explorer::ObservationOutput {
        crate::explorer::ObservationOutput {
            function_name: "test_fn".to_string(),
            iterations: 10,
            unique_paths: discoveries.len(),
            lines_covered: 0,
            total_lines: 0,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: Default::default(),
            mcdc_summary: None,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn uncovered_branches_mixed_coverage() {
        let analysis = make_analysis_with_branches(vec![
            make_branch_info(0, 5, "x > 0"),
            make_branch_info(1, 10, "x < 100"),
            make_branch_info(2, 15, "x == 42"),
        ]);
        let observation = make_observation_with_discoveries(vec![
            (0, crate::coverage_metrics::DiscoveryMethod::Random),
            (2, crate::coverage_metrics::DiscoveryMethod::Z3),
        ]);

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        assert_eq!(uncovered, vec!["1:10"]);
    }

    #[test]
    fn uncovered_branches_all_covered() {
        let analysis = make_analysis_with_branches(vec![
            make_branch_info(0, 5, "x > 0"),
            make_branch_info(1, 10, "x < 100"),
        ]);
        let observation = make_observation_with_discoveries(vec![
            (0, crate::coverage_metrics::DiscoveryMethod::Z3),
            (1, crate::coverage_metrics::DiscoveryMethod::Random),
        ]);

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        assert!(uncovered.is_empty());
    }

    #[test]
    fn uncovered_branches_no_branches() {
        let analysis = make_analysis_with_branches(vec![]);
        let observation = make_observation_with_discoveries(vec![]);

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        assert!(uncovered.is_empty());
    }

    #[test]
    fn uncovered_branches_all_uncovered() {
        let analysis = make_analysis_with_branches(vec![
            make_branch_info(0, 5, "x > 0"),
            make_branch_info(1, 10, "x < 100"),
            make_branch_info(2, 15, "x == 42"),
        ]);
        let observation = make_observation_with_discoveries(vec![]);

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        assert_eq!(uncovered, vec!["0:5", "1:10", "2:15"]);
    }

    #[test]
    fn uncovered_branches_format_is_id_colon_line() {
        let analysis = make_analysis_with_branches(vec![make_branch_info(42, 123, "foo")]);
        let observation = make_observation_with_discoveries(vec![]);

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        assert_eq!(uncovered, vec!["42:123"]);
    }

    #[test]
    fn uncovered_branches_excludes_opaque_constraint() {
        use crate::execution_record::{BranchDecision, SymConstraint};
        // Branch 0 is discovered, branch 1 has an opaque constraint (discovered
        // but Unknown), branch 2 is truly uncovered.
        let analysis = make_analysis_with_branches(vec![
            make_branch_info(0, 5, "x > 0"),
            make_branch_info(1, 10, "isValid(x)"),
            make_branch_info(2, 15, "x == 42"),
        ]);

        // Branch 1 is "discovered" (appears in discoveries) but has an opaque
        // constraint at runtime. It should NOT appear in uncovered_branches
        // because it was technically reached.
        let mut observation = make_observation_with_discoveries(vec![
            (0, crate::coverage_metrics::DiscoveryMethod::Z3),
            (1, crate::coverage_metrics::DiscoveryMethod::Random),
        ]);
        // Add a raw result with an Unknown constraint for branch 1 so
        // extract_targets classifies it as OpaqueConstraint rather than
        // Uncovered — but since it's already in discoveries, it won't be
        // in the uncovered list either way.
        observation.raw_results.push((
            vec![serde_json::json!(1)],
            vec![],
            crate::protocol::ExecuteResult {
                branch_path: vec![BranchDecision {
                    branch_id: 1,
                    line: 10,
                    taken: true,
                    constraint: SymConstraint::Unknown {
                        hint: "opaque call".to_string(),
                    },
                    conditions: None,
                }],
                return_value: None,
                thrown_error: None,
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                loop_body_states: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                runtime_crypto_boundaries: vec![],
                outcome: None,
                performance: empty_perf(),
            },
        ));

        let uncovered = compute_uncovered_branch_strings(&analysis, &observation);
        // Only branch 2 is truly uncovered; branch 1 was discovered (opaque
        // but reached). The filter on TargetReason::Uncovered excludes
        // OpaqueConstraint targets.
        assert_eq!(uncovered, vec!["2:15"]);
    }

    // ── str-jeen.3: run-manifest validation integration tests ──────────

    #[test]
    fn finalize_with_unchanged_manifest_completes_normally() {
        use std::fs::{File, write as write_file};
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("a.rs");
        File::create(&path).unwrap().write_all(b"x").unwrap();
        let _ = write_file(&path, b"x");

        let paths = vec!["a.rs".to_string()];
        let manifest = crate::run_manifest::capture("scan-1", "cfg", &paths, Some(tmp.path()));
        let mut summary = new_scan_summary("scan-1", 1);
        summary.completed = 1;
        summary_finalize_with_manifest_check(
            &mut summary,
            Duration::from_millis(10),
            &manifest,
            &paths,
        );
        assert_eq!(summary.status, ScanRunStatus::Completed);
        let diff = summary.source_diff.expect("source_diff present");
        assert!(!diff.is_stale());
    }

    #[test]
    fn finalize_with_modified_source_promotes_to_stale() {
        use std::fs::{File, write as write_file};
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("a.rs");
        File::create(&path).unwrap().write_all(b"original").unwrap();

        let paths = vec!["a.rs".to_string()];
        let manifest = crate::run_manifest::capture("scan-2", "cfg", &paths, Some(tmp.path()));

        // Simulate a concurrent edit during the run.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_file(&path, b"mutated").unwrap();

        let mut summary = new_scan_summary("scan-2", 1);
        summary.completed = 1;
        summary_finalize_with_manifest_check(
            &mut summary,
            Duration::from_millis(10),
            &manifest,
            &paths,
        );
        assert_eq!(summary.status, ScanRunStatus::StaleSourceSet);
        let diff = summary.source_diff.expect("source_diff present");
        assert_eq!(diff.changed, vec!["a.rs".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn finalize_with_removed_source_promotes_to_stale() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("a.rs");
        File::create(&path).unwrap().write_all(b"x").unwrap();

        let paths = vec!["a.rs".to_string()];
        let manifest = crate::run_manifest::capture("scan-3", "cfg", &paths, Some(tmp.path()));
        std::fs::remove_file(&path).unwrap();

        let mut summary = new_scan_summary("scan-3", 1);
        summary.completed = 1;
        summary_finalize_with_manifest_check(
            &mut summary,
            Duration::from_millis(10),
            &manifest,
            &paths,
        );
        assert_eq!(summary.status, ScanRunStatus::StaleSourceSet);
        let diff = summary.source_diff.expect("source_diff present");
        assert_eq!(diff.removed, vec!["a.rs".to_string()]);
    }

    #[test]
    fn finalize_with_added_source_promotes_to_stale() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        File::create(tmp.path().join("a.rs"))
            .unwrap()
            .write_all(b"x")
            .unwrap();
        let original_paths = vec!["a.rs".to_string()];
        let manifest =
            crate::run_manifest::capture("scan-4", "cfg", &original_paths, Some(tmp.path()));

        // A new file appears mid-run and is in the end-of-run path set.
        File::create(tmp.path().join("b.rs"))
            .unwrap()
            .write_all(b"y")
            .unwrap();
        let current_paths = vec!["a.rs".to_string(), "b.rs".to_string()];

        let mut summary = new_scan_summary("scan-4", 2);
        summary.completed = 2;
        summary_finalize_with_manifest_check(
            &mut summary,
            Duration::from_millis(10),
            &manifest,
            &current_paths,
        );
        assert_eq!(summary.status, ScanRunStatus::StaleSourceSet);
        let diff = summary.source_diff.expect("source_diff present");
        assert_eq!(diff.added, vec!["b.rs".to_string()]);
    }

    #[tokio::test]
    async fn parallel_scan_writes_run_manifest() {
        use crate::frontend::FrontendConfig;
        use std::fs::File;
        use std::io::Write;
        use std::path::{Path, PathBuf};
        use tempfile::TempDir;

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");

        // Create a tempdir with real source files at the paths the
        // file_map references.
        let tmp = TempDir::new().expect("tempdir");
        let src_path = tmp.path().join("test.ts");
        File::create(&src_path)
            .unwrap()
            .write_all(b"// stub")
            .unwrap();

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
            invocation_model: crate::protocol::InvocationModel::Direct,
            source_file: None,
            adapter_hints: vec![],
        }];

        let mut file_map = HashMap::new();
        file_map.insert("solo".to_string(), "test.ts".to_string());

        let config = ScanConfig {
            max_iterations_per_function: 2,
            concolic: false,
            seed: Some(7),
            file_map,
            parallelism: 1,
            timeout_per_fn: TEST_REQUEST_TIMEOUT,
            build_timeout: Duration::from_secs(30),
            cache: None,
            stratum: None,
            mock_overrides: HashMap::new(),
            resume_path: None,
            timeout_total: None,
            pool_path: None,
            project_root: Some(tmp.path().to_string_lossy().into_owned()),
            config_dir: None,
            timeout_explore: None,
            setup_manager: None,
            policy: crate::scheduler_policy::SchedulerPolicy::default(),
            isolation: IsolationMode::None,
            capture_side_effects: false,
            workers_per_fn: 1,
            capabilities: crate::orchestrator::FrontendCapabilities::default(),
            genetic_config: crate::config::GeneticConfig::default(),
            batch_size: None,
            scheduler_state_cache: None,
            stored_inputs_cache: None,
            coverage_mode: crate::interesting_pool::CoverageMode::Branch,
            write_artifacts: true,
        };

        let result = parallel_scan(&fe_config, &analyses, &config)
            .await
            .expect("parallel_scan should succeed");
        assert!(!result.function_results.is_empty());

        // Manifest must exist next to summary.json.
        let scan_id = compute_scan_id(&config);
        let scan_root_dir = scan_root(config.project_root.as_deref(), &scan_id);
        let manifest = crate::run_manifest::read_manifest(&scan_root_dir)
            .expect("manifest.json should be written");
        assert_eq!(manifest.scan_id, scan_id);
        assert_eq!(manifest.source_files.len(), 1);
        assert_eq!(manifest.source_files[0].path, "test.ts");
        assert_eq!(manifest.source_files[0].size, b"// stub".len() as u64);
        assert!(manifest.source_files[0].content_hash.is_some());

        // Summary should be Completed (no drift) and the source_diff
        // should be present and clean.
        let summary_path = scan_root_dir.join("summary.json");
        let summary_bytes = std::fs::read(&summary_path).expect("summary.json read");
        let summary: ScanSummary =
            serde_json::from_slice(&summary_bytes).expect("summary.json parse");
        assert_eq!(summary.status, ScanRunStatus::Completed);
        let diff = summary.source_diff.expect("source_diff written");
        assert!(!diff.is_stale());

        let status_path = scan_root_dir.join(crate::status_export::RUN_STATUS_FILENAME);
        let status_bytes = std::fs::read(status_path).expect("run-status.json read");
        let status: crate::status_export::RunStatus =
            serde_json::from_slice(&status_bytes).expect("run-status.json parse");
        assert_eq!(status.run.scan_id, scan_id);
        assert_eq!(status.command.name, "scan");
        assert_eq!(status.command.config_hash, manifest.scope_hash);
        assert_eq!(status.manifest.path, "manifest.json");
        assert_eq!(status.artifacts[0].kind, "scan_summary");
        assert_eq!(status.artifacts[0].path, "summary.json");
        assert_eq!(status.files.len(), 1);
        assert_eq!(status.files[0].path, "test.ts");
        assert_eq!(status.files[0].discovered_target_count, 1);
        assert_eq!(status.files[0].attempted_target_count, 1);
        assert_eq!(status.files[0].completed_target_count, 1);
        assert_eq!(
            status.files[0].status,
            crate::status_export::StatusFileStatus::Completed
        );
        assert_eq!(status.targets.len(), 1);
        assert_eq!(status.targets[0].target_id, "solo");
        assert_eq!(status.targets[0].name, "solo");
        assert_eq!(status.targets[0].source_file, "test.ts");
        assert_eq!(status.targets[0].start_line, 1);
        assert_eq!(status.targets[0].end_line, 5);
        assert_eq!(
            status.targets[0].outcome,
            crate::status_export::StatusTargetOutcome::Completed
        );
        assert!(status.targets[0].artifact.is_some());
        assert_eq!(status.rollups.source_denominators.selected_source_files, 1);
        assert_eq!(status.rollups.source_denominators.discovered_targets, 1);
        assert_eq!(status.rollups.source_denominators.attempted_targets, 1);
        assert_eq!(status.rollups.source_denominators.completed_targets, 1);
        assert_eq!(
            status.rollups.validity.report_validity,
            crate::status_export::StatusReportValidity::High
        );
        assert!(status.rollups.gate_decisions.is_none());

        let status_tsv_path = scan_root_dir.join(crate::status_export::RUN_STATUS_TSV_FILENAME);
        let status_tsv = std::fs::read_to_string(status_tsv_path).expect("run-status.tsv read");
        assert_eq!(
            status_tsv.lines().next(),
            Some(
                crate::status_export::RUN_STATUS_TSV_COLUMNS
                    .join("\t")
                    .as_str()
            )
        );
        assert!(status_tsv.lines().any(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            columns.get(4) == Some(&"file") && columns.get(5) == Some(&"test.ts")
        }));
        assert!(status_tsv.lines().any(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            columns.get(4) == Some(&"target") && columns.get(16) == Some(&"solo")
        }));
    }
}

#[cfg(test)]
mod proptests_uncovered {
    use super::*;
    use proptest::prelude::*;

    fn arb_branch_info() -> impl Strategy<Value = crate::protocol::BranchInfo> {
        (0_u32..100, 1_u32..500).prop_map(|(id, line)| crate::protocol::BranchInfo {
            id,
            line,
            condition_text: format!("cond_{id}"),
            condition: None,
            branch_type: crate::protocol::BranchType::If,
        })
    }

    proptest! {
        /// The output of compute_uncovered_branch_strings is the exact set of
        /// branches not in discoveries, formatted as "{id}:{line}".
        #[test]
        fn uncovered_is_complement_of_discoveries(
            branches in proptest::collection::vec(arb_branch_info(), 0..15),
            discovery_ratio in 0.0_f64..=1.0,
        ) {
            // Deduplicate branch IDs — real analyses never have duplicates.
            let mut seen = std::collections::HashSet::new();
            let branches: Vec<_> = branches
                .into_iter()
                .filter(|b| seen.insert(b.id))
                .collect();

            // Select a subset of branches as "discovered" based on ratio.
            let discovery_count =
                (branches.len() as f64 * discovery_ratio).round() as usize;
            let discoveries: Vec<(u32, crate::coverage_metrics::DiscoveryMethod)> = branches
                .iter()
                .take(discovery_count)
                .map(|b| (b.id, crate::coverage_metrics::DiscoveryMethod::Random))
                .collect();

            let discovered_ids: std::collections::HashSet<u32> =
                discoveries.iter().map(|(id, _)| *id).collect();

            let mut analysis =
                crate::protocol::FunctionAnalysis {
                    name: "prop_fn".into(),
                    exported: true,
                    params: vec![],
                    branches: branches.clone(),
                    dependencies: vec![],
                    return_type: crate::types::TypeInfo::Unknown,
                    start_line: 1,
                    end_line: 100,
                    literals: vec![],
                    crypto_boundaries: vec![],
                    loops: vec![],
                    source_file: None,
                    adapter_hints: vec![],
                    invocation_model: crate::protocol::InvocationModel::Direct,
                };
            let _ = &mut analysis; // suppress unused-mut

            let observation = crate::explorer::ObservationOutput {
                function_name: "prop_fn".into(),
                iterations: 10,
                unique_paths: 0,
                lines_covered: 0,
                total_lines: 0,
                new_path_executions: vec![],
                raw_results: vec![],
                discoveries,
                nondeterministic_fields: vec![],
                float_probe_results: vec![],
                boundary_results: vec![],
                shrunk_witnesses: Default::default(),
                mcdc_summary: None,
                shrink_stats: crate::shrink::ShrinkStats::default(),
                abandoned_frontiers: vec![],
                opaque_suggestions: vec![],
                stubbed_modules: vec![],
                            ..Default::default()
            };

            let mut result = compute_uncovered_branch_strings(&analysis, &observation);

            // Every result entry must correspond to a branch NOT in discoveries.
            // Sort both sides — the function's iteration order depends on
            // extract_targets_inner which iterates analysis.branches, but the
            // exact ordering is an implementation detail, not a contract.
            let mut expected: Vec<String> = branches
                .iter()
                .filter(|b| !discovered_ids.contains(&b.id))
                .map(|b| format!("{}:{}", b.id, b.line))
                .collect();
            result.sort();
            expected.sort();

            prop_assert_eq!(&result, &expected);

            // Every entry must match "{u32}:{u32}" format.
            for entry in &result {
                let parts: Vec<&str> = entry.split(':').collect();
                prop_assert_eq!(parts.len(), 2, "bad format: {}", entry);
                prop_assert!(parts[0].parse::<u32>().is_ok(), "bad id: {}", entry);
                prop_assert!(parts[1].parse::<u32>().is_ok(), "bad line: {}", entry);
            }
        }
    }
}

// ---- str-b2my.11: estimate_nesting_depth and KnownTargets tests ----

#[cfg(test)]
mod tests_nesting_depth {
    use super::*;

    fn branch(id: u32, line: u32, bt: BranchType) -> BranchInfo {
        BranchInfo {
            id,
            line,
            condition_text: format!("cond_{id}"),
            condition: None,
            branch_type: bt,
        }
    }

    #[test]
    fn empty_branches_yields_zero() {
        assert_eq!(estimate_nesting_depth(&[], &[]), 0);
    }

    #[test]
    fn empty_targets_yields_zero() {
        let branches = vec![branch(0, 10, BranchType::If)];
        assert_eq!(estimate_nesting_depth(&branches, &[]), 0);
    }

    #[test]
    fn single_branch_single_target_depth_zero() {
        let branches = vec![branch(0, 10, BranchType::If)];
        assert_eq!(estimate_nesting_depth(&branches, &[0]), 0);
    }

    #[test]
    fn nested_if_branches() {
        let branches = vec![
            branch(0, 10, BranchType::If),
            branch(1, 12, BranchType::If),
            branch(2, 14, BranchType::If),
        ];
        assert_eq!(estimate_nesting_depth(&branches, &[2]), 2);
        assert_eq!(estimate_nesting_depth(&branches, &[1]), 1);
        assert_eq!(estimate_nesting_depth(&branches, &[0, 1, 2]), 2);
    }

    #[test]
    fn only_flow_control_types_count() {
        let branches = vec![
            branch(0, 5, BranchType::LogicalAnd),
            branch(1, 8, BranchType::Ternary),
            branch(2, 10, BranchType::If),
            branch(3, 12, BranchType::ElseIf),
            branch(4, 15, BranchType::If),
        ];
        assert_eq!(estimate_nesting_depth(&branches, &[4]), 1);
    }

    #[test]
    fn while_for_switch_select_count_as_depth() {
        let branches = vec![
            branch(0, 10, BranchType::While),
            branch(1, 15, BranchType::For),
            branch(2, 20, BranchType::Switch),
            branch(3, 25, BranchType::Select),
            branch(4, 30, BranchType::If),
        ];
        assert_eq!(estimate_nesting_depth(&branches, &[4]), 4);
    }

    #[test]
    fn subset_targets_only_considers_given_ids() {
        let branches = vec![
            branch(0, 10, BranchType::If),
            branch(1, 20, BranchType::If),
            branch(2, 30, BranchType::If),
        ];
        assert_eq!(estimate_nesting_depth(&branches, &[1]), 1);
    }

    #[test]
    fn nonexistent_target_id_ignored() {
        let branches = vec![branch(0, 10, BranchType::If)];
        assert_eq!(estimate_nesting_depth(&branches, &[99]), 0);
    }

    #[test]
    fn known_targets_struct_construction() {
        let branches = vec![branch(0, 10, BranchType::If), branch(1, 20, BranchType::If)];
        let uncovered = vec![0, 1];
        let targets = KnownTargets {
            max_nesting_depth: estimate_nesting_depth(&branches, &uncovered),
            branch_ids: uncovered,
        };
        assert_eq!(targets.branch_ids, vec![0, 1]);
        assert_eq!(targets.max_nesting_depth, 1);
    }
}

#[cfg(test)]
mod proptests_nesting_depth {
    use super::*;
    use proptest::prelude::*;

    fn arb_branch_type() -> impl Strategy<Value = BranchType> {
        prop_oneof![
            Just(BranchType::If),
            Just(BranchType::ElseIf),
            Just(BranchType::Switch),
            Just(BranchType::Ternary),
            Just(BranchType::LogicalAnd),
            Just(BranchType::LogicalOr),
            Just(BranchType::While),
            Just(BranchType::For),
            Just(BranchType::Select),
        ]
    }

    fn arb_branch_info() -> impl Strategy<Value = BranchInfo> {
        (0_u32..100, 1_u32..500, arb_branch_type()).prop_map(|(id, line, bt)| BranchInfo {
            id,
            line,
            condition_text: format!("cond_{id}"),
            condition: None,
            branch_type: bt,
        })
    }

    proptest! {
        #[test]
        fn depth_bounded_by_flow_control_count(
            branches in proptest::collection::vec(arb_branch_info(), 0..20),
            target_ratio in 0.0_f64..=1.0,
        ) {
            let mut seen = std::collections::HashSet::new();
            let branches: Vec<_> = branches
                .into_iter()
                .filter(|b| seen.insert(b.id))
                .collect();

            let flow_control_count = branches
                .iter()
                .filter(|b| matches!(
                    b.branch_type,
                    BranchType::If | BranchType::While | BranchType::For
                    | BranchType::Switch | BranchType::Select
                ))
                .count() as u32;

            let target_count =
                (branches.len() as f64 * target_ratio).ceil() as usize;
            let target_ids: Vec<u32> = branches
                .iter()
                .take(target_count)
                .map(|b| b.id)
                .collect();

            let depth = estimate_nesting_depth(&branches, &target_ids);
            prop_assert!(
                depth <= flow_control_count,
                "depth {} exceeds flow-control count {}",
                depth,
                flow_control_count,
            );
        }

        #[test]
        fn empty_targets_always_zero(
            branches in proptest::collection::vec(arb_branch_info(), 0..20),
        ) {
            prop_assert_eq!(estimate_nesting_depth(&branches, &[]), 0);
        }
    }
}
