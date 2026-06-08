//! Exploration engine for discovering execution paths via random input generation.
//!
//! Drives the concolic execution loop: analyze a function's type signature,
//! generate random inputs, execute them via a language frontend, and track
//! unique execution paths. This module implements the random exploration phase
//! (no symbolic solving).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand::rngs::StdRng;
use tracing::Instrument;

use crate::auto_mock::MockParam;
use crate::coverage_metrics::DiscoveryMethod;
use crate::frontend::{Frontend, FrontendConfig, FrontendError};
use crate::input_gen::{
    PrefetchedValues, ValueSource, generate_inputs_with_custom, generate_mock_values,
    overlay_custom_inputs, prefetch_custom_values,
};
use crate::mock_value_space::{LiveCallOutcome, LiveFirstState, classify_connection_failure};
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::SetupLevel;
use crate::protocol::{
    Command as ProtoCommand, ExecuteResult, ExecutionProfile, FunctionAnalysis, MockConfig,
    ResponseResult, SetupContextEntry, SetupContextStack,
};
use crate::setup_manager::SetupManager;
use crate::strategy::{SpecialCandidatePath, StrategyContext, build_random_explorer_meta_strategy};

/// Iteration count bucket boundaries for scope-aware path hashing.
///
/// Each threshold defines the upper bound of a bucket. Counts above
/// the last threshold all land in the final bucket.
/// Default: `[0, 1, 2, 5]` → buckets: 0, 1, 2, 3–5, 6+
#[derive(Debug, Clone)]
pub struct LoopBuckets(Vec<u32>);

impl LoopBuckets {
    /// Construct from sorted, deduplicated boundary values.
    /// Panics if boundaries are not sorted or contain duplicates.
    pub fn from_boundaries(mut boundaries: Vec<u32>) -> Self {
        boundaries.sort_unstable();
        boundaries.dedup();
        Self(boundaries)
    }

    /// Disable bucketing entirely — iteration counts are ignored (current behavior).
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// Returns `true` when bucketing is disabled.
    pub fn is_disabled(&self) -> bool {
        self.0.is_empty()
    }

    /// Map an iteration count to a bucket index (0-based).
    /// Bucket *i* covers counts in `(boundaries[i-1], boundaries[i]]`
    /// (with bucket 0 covering `[0, boundaries[0]]`).
    /// Counts above the last boundary land in the final bucket.
    pub fn bucket(&self, count: u32) -> u32 {
        for (i, &threshold) in self.0.iter().enumerate() {
            if count <= threshold {
                return i as u32;
            }
        }
        self.0.len() as u32
    }
}

impl Default for LoopBuckets {
    fn default() -> Self {
        Self(vec![0, 1, 2, 5])
    }
}

/// Execution isolation level for function exploration.
///
/// Controls whether function invocations share a process or are isolated from each other.
/// The default (`None`) assumes side-effect-safe, stateless functions and offers the
/// best throughput by sharing a single frontend process across all executions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IsolationMode {
    /// No isolation (default). All executions share a single frontend process.
    /// Assumes functions are side-effect-safe and stateless.
    #[default]
    None,
    /// Each function invocation gets a fresh execution context (new process or sandbox).
    Function,
    /// Functions run sequentially (no parallelism), sharing a single process.
    Serial,
}

/// Configuration for an exploration run.
#[derive(Debug, Clone)]
pub struct ExploreConfig {
    /// Path to the source file being explored (needed for instrumentation).
    pub file: String,
    /// Maximum number of iterations (execute calls) per function.
    /// `None` means unbounded — explore runs until timeout or interruption.
    pub max_iterations: Option<u32>,
    /// Number of observer frontend subprocesses to use for random exploration.
    ///
    /// `1` preserves the serial path. Values above 1 require
    /// `observer_frontend_config` so the explorer can spawn independent
    /// frontend subprocesses; the public CLI/config knob is intentionally
    /// deferred to str-frc.6.
    pub observer_pool: usize,
    /// Frontend spawn template for observer subprocesses when
    /// `observer_pool > 1`.
    pub observer_frontend_config: Option<FrontendConfig>,
    /// Override the bounded candidate queue capacity used between the
    /// candidate generator and the observer pool. `None` keeps the auto-derived
    /// default (`observer_pool * 4`, capped by `max_iterations`). Values are
    /// clamped to at least `1`. Has no effect when `observer_pool <= 1`.
    pub candidate_queue_capacity: Option<usize>,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Mock configurations to pass to Execute commands.
    pub mocks: Vec<MockConfig>,
    /// Mock parameters for dynamic per-iteration mock generation.
    /// When non-empty, fresh mock values are generated each iteration
    /// instead of reusing the static `mocks` field.
    pub mock_params: Vec<MockParam>,
    /// Path to the setup file, if configured.
    pub setup_file: Option<String>,
    /// When to run setup relative to executions.
    pub setup_level: SetupLevel,
    /// Where each parameter's value should come from.
    pub value_sources: Vec<ValueSource>,
    /// Frontend capabilities (used to gate setup/generate commands).
    pub capabilities: FrontendCapabilities,
    /// User-provided candidate inputs from --inputs or .shatter/ config.
    /// Executed first (highest priority), with no budget cap.
    pub user_seeds: Vec<Vec<serde_json::Value>>,
    /// Resolved candidate inputs (from --inputs or .shatter/ config).
    /// Priority above pool seeds, below literal seeds.
    pub candidate_inputs: Vec<Vec<serde_json::Value>>,
    /// Pre-computed pool seed candidates (from interesting input pool).
    /// Injected after literal candidates but before random generation.
    pub pool_seeds: Vec<Vec<serde_json::Value>>,
    /// Detected project root directory, passed to frontend commands.
    pub project_root: Option<String>,
    /// Opaque execution profile selected for this function, if any.
    pub execution_profile: Option<ExecutionProfile>,
    /// Iteration count bucket boundaries for loop-aware path hashing.
    pub loop_buckets: LoopBuckets,
    /// Per-function exploration wall-clock timeout. Whichever of this or
    /// `max_iterations` triggers first stops the loop.
    pub timeout_explore: Option<Duration>,
    /// Strategy meta-configuration for adaptive selection.
    pub meta_config: crate::strategy::MetaConfig,
    /// Maximum shrink attempts per discovered behavior. Set to 0 to disable.
    pub shrink_budget: usize,
    /// Execution isolation level. Defaults to `IsolationMode::None` (stateless/shared process).
    pub isolation: IsolationMode,
    /// When true, frontends are expected to capture rich side-effect data
    /// (console output, file writes, network requests, environment reads,
    /// global state changes, etc.) per execution. Defaults to false for
    /// throughput — capture adds overhead on every execute call.
    pub capture_side_effects: bool,
    /// Shared budget surplus for dynamic reallocation within a scan layer.
    /// When `Some`, the explorer can claim additional iterations from the
    /// surplus when its initial budget is exhausted but it's still finding
    /// new paths. When `None` (default), the initial `max_iterations` is a
    /// hard cap.
    pub budget_surplus: Option<std::sync::Arc<crate::scan_orchestrator::BudgetSurplus>>,
    /// Policy for claiming surplus budget. Only used when `budget_surplus` is
    /// `Some`. Controls the minimum hit rate and maximum claim fraction.
    pub claim_policy: crate::scan_orchestrator::ClaimPolicy,
    /// Name of a frontend-provided invocation planner to consult. `None` means
    /// the random explorer selects inputs on its own. Set `default_execute_plan`
    /// to pass a plan on every Execute for this target.
    pub planner: Option<String>,
    /// InvocationPlan to attach to every Execute request for this target.
    /// Set from the first plan returned by the planner; `None` when not using
    /// `--planner` or when the frontend returned no plans.
    pub default_execute_plan: Option<crate::protocol::InvocationPlan>,
    /// When `Some`, the explorer skips its own `Prepare` lifecycle and uses
    /// this `prepare_id` on every Execute. Set by the scan orchestrator after
    /// it has already issued a Prepare under the build timeout, so the
    /// per-function exploration timeout doesn't absorb cold build cost.
    pub prepare_id_override: Option<String>,
}

/// Default interval between periodic progress summaries.
pub const PROGRESS_SUMMARY_INTERVAL_SECS: u64 = 15;

/// Snapshot of exploration progress emitted periodically during the explore loop.
#[derive(Debug, Clone)]
pub struct ExploreProgressSnapshot {
    /// Name of the function being explored.
    pub function_name: String,
    /// Wall-clock time since exploration started.
    pub elapsed: Duration,
    /// Number of iterations (executions) completed so far.
    pub iterations: u32,
    /// Number of unique execution paths discovered.
    pub paths_found: usize,
    /// Total branches reported by static analysis (if known).
    pub total_branches: Option<usize>,
    /// Number of distinct branches covered so far (unique branch IDs with
    /// recorded discoveries). `None` when the explorer does not track per-branch
    /// discovery attribution.
    pub branches_covered: Option<usize>,
    /// MC/DC summary when condition coverage tracking is enabled:
    /// `(total_conditions, independent_conditions, opaque_conditions)`.
    pub mcdc_summary: Option<(usize, usize, usize)>,
    /// Iterations elapsed since the last newly-discovered branch. Non-zero
    /// values mean the function is spinning without finding new coverage —
    /// surfaces the "continuing without new discoveries" signal.
    pub iters_since_new_discovery: u32,
}

/// Callback type for receiving periodic exploration progress summaries.
pub type ProgressCallback = dyn Fn(&ExploreProgressSnapshot) + Send + Sync;

/// Bundle of metadata the explorer / orchestrator loops need in order to emit
/// enriched progress snapshots. Wrapping the callback and the
/// explorer-external hints (e.g. `total_branches` from static analysis) keeps
/// the `explore` / `explore_function` signatures stable when future snapshot
/// fields are added.
#[derive(Copy, Clone)]
pub struct ProgressHints<'a> {
    /// Where to send snapshots. Invoked on the periodic cadence defined by
    /// [`PROGRESS_SUMMARY_INTERVAL_SECS`].
    pub callback: &'a ProgressCallback,
    /// Total branches reported by static analysis, if known. Surfaces as the
    /// denominator in "N/M branches" output.
    pub total_branches: Option<usize>,
}

/// Summary of a single function execution during exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// The input values sent to the function.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error message, if the function threw.
    pub thrown_error: Option<String>,
    /// Lines executed during this call.
    pub lines_executed: Vec<u32>,
    /// Whether this execution discovered a new unique path.
    pub is_new_path: bool,
    /// Inferred error intent (validation vs runtime), if an error was thrown.
    pub error_intent: Option<ErrorIntentLabel>,
}

/// Canonical pipeline output for a single function's observation phase.
///
/// Captures everything produced by either random or concolic exploration:
/// discovered paths, line coverage, raw execution results, and per-branch
/// discovery attribution. Used as the input to the Analyze pipeline stage.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ObservationOutput {
    /// Name of the explored function.
    pub function_name: String,
    /// Total iterations attempted.
    pub iterations: u32,
    /// Number of unique execution paths discovered.
    pub unique_paths: usize,
    /// Number of unique source lines covered across all executions.
    pub lines_covered: usize,
    /// Total source lines in the function (end_line - start_line + 1).
    pub total_lines: u32,
    /// Summary of each execution that discovered a new path.
    pub new_path_executions: Vec<ExecutionSummary>,
    /// Raw execution results paired with their inputs and mock configs,
    /// for building BehaviorMaps that track which mock values produced which outcomes.
    pub raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)>,
    /// Per-branch discovery attribution: which branch_id was first found by which method.
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
    /// Number of branch-guided follow-up inputs generated by concolic strategies.
    #[serde(default)]
    pub solver_guided_inputs: usize,
    /// Fields detected as nondeterministic via within-run re-execution sampling.
    #[serde(default)]
    pub nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
    /// Float probe results classifying Float params as integer-treating or float-sensitive.
    #[serde(default)]
    pub float_probe_results: Vec<crate::float_probe::FloatProbeResult>,
    /// Refined boundary witness pairs from post-discovery refinement phase.
    #[serde(default)]
    pub boundary_results: Vec<crate::boundary_search::BoundaryResult>,
    /// Shrunk witnesses: maps branch_path hash to minimal inputs that reproduce the same path.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub shrunk_witnesses: std::collections::HashMap<u64, Vec<serde_json::Value>>,
    /// MC/DC summary: (total_conditions, independent_conditions, opaque_conditions).
    /// Present only when the concolic orchestrator was run with `mcdc: true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcdc_summary: Option<(usize, usize, usize)>,
    /// Aggregated shrink phase performance counters.
    #[serde(default)]
    pub shrink_stats: crate::shrink::ShrinkStats,
    /// Frontiers abandoned due to stall detection: (branch_id, final_stall_count).
    /// Always empty for the random explorer (which doesn't use frontiers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abandoned_frontiers: Vec<(u32, u32)>,
    /// Parameters suggested as opaque type candidates based on exploration results.
    /// Populated when parameters have unknown types or caused repeated solver failures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opaque_suggestions: Vec<crate::executability::OpaqueSuggestion>,
    /// Module names that could not be resolved at runtime and were replaced
    /// with recursive Proxy stubs.  When non-empty the function was only
    /// **partially analyzed** — coverage may be limited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stubbed_modules: Vec<String>,
    /// True when exploration ended because the per-function wall-clock
    /// timeout (`config.timeout_explore`) tripped before the loop reached a
    /// natural termination (max_iterations, exhausted worklist, plateau).
    /// Added by str-gz8j so the CLI explore command can surface the
    /// function as `OutcomeStatus::TimedOut` instead of silently labelling
    /// it `Completed` — a successful Result with `timed_out=true` means
    /// "exploration ran out of time mid-flight," not "explored everything."
    /// `#[serde(default)]` keeps legacy artifacts loadable as `false`.
    #[serde(default)]
    pub timed_out: bool,
    /// Aggregate LLM seed-oracle telemetry. `None` when no oracle was active
    /// during this function's exploration (str-qnp0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_stats: Option<crate::oracle::OracleStats>,
}

/// Type alias for pipeline composability. `ObserveResult` is the output of
/// the Observe stage (random exploration).
pub type ObserveResult = ObservationOutput;

/// Errors that can occur during exploration.
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error("unexpected response from frontend: {0}")]
    UnexpectedResponse(String),
    /// Frontend reported the target as `not_supported` during execute. The
    /// scan layer maps this to `SkipCategory::Unsupported` with a clean
    /// reason instead of `SkipCategory::Error`. (str-31j.4)
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Compute a hash representing the "path signature" of an execution.
///
/// When `scope_events` is non-empty, uses scope-aware collapsing: repeated
/// invocations of the same loop or call scope are collapsed to the sorted set
/// of unique `(branch_id, taken)` pairs. The `buckets` parameter controls
/// whether iteration counts also contribute to the hash — when enabled,
/// different iteration count buckets produce different hashes even if the
/// branch profile is identical.
///
/// Falls back to legacy sequential hashing when `scope_events` is empty.
pub(crate) fn path_hash(result: &crate::protocol::ExecuteResult, buckets: &LoopBuckets) -> u64 {
    if !result.scope_events.is_empty() {
        return scope_aware_hash(&result.scope_events, buckets);
    }
    legacy_path_hash(result)
}

/// Legacy path hash: hashes branch decisions sequentially, falling back to
/// lines_executed, error info, or return value.
fn legacy_path_hash(result: &crate::protocol::ExecuteResult) -> u64 {
    let mut hasher = DefaultHasher::new();
    if !result.branch_path.is_empty() {
        for decision in &result.branch_path {
            decision.branch_id.hash(&mut hasher);
            decision.taken.hash(&mut hasher);
        }
    } else if !result.lines_executed.is_empty() {
        result.lines_executed.hash(&mut hasher);
    } else if let Some(ref err) = result.thrown_error {
        "error".hash(&mut hasher);
        err.error_type.hash(&mut hasher);
        err.message.hash(&mut hasher);
    } else {
        "ok".hash(&mut hasher);
        let ret_str = serde_json::to_string(&result.return_value).unwrap_or_default();
        ret_str.hash(&mut hasher);
    }
    hasher.finish()
}

/// Scope-aware path hash that collapses repeated loop/call scopes.
///
/// Parses the flat trace into a tree of nested scopes, then for each scope_id
/// that appears multiple times at the same nesting level, collapses all
/// repetitions to a single sorted set of unique `(branch_id, taken)` pairs.
/// When `buckets` is non-empty, each distinct profile is also paired with
/// its iteration-count bucket so that different iteration counts produce
/// different hashes.
fn scope_aware_hash(events: &[crate::execution_record::TraceEvent], buckets: &LoopBuckets) -> u64 {
    use crate::execution_record::{ScopeEvent, TraceEvent};
    use std::collections::BTreeSet;

    /// Discriminant tag for scope enter/exit matching.
    #[derive(Clone, Copy, PartialEq)]
    enum ScopeKind {
        Loop(u32),
        Call(u32),
    }

    /// A sorted branch profile from one loop/call iteration.
    type BranchProfile = Vec<(u32, bool)>;

    /// A profile paired with its optional iteration-count bucket.
    type ProfileWithBucket = (BranchProfile, Option<u32>);

    /// Collapsed representation of a scope's content for hashing.
    #[derive(Hash)]
    enum CollapsedItem {
        /// A branch decision outside any scope at this level.
        Branch { branch_id: u32, taken: bool },
        /// A collapsed scope: set of distinct per-iteration branch profiles,
        /// each paired with its iteration-count bucket (or `None` when
        /// bucketing is disabled, preserving backward-compat hashing).
        Scope {
            scope_tag: u64,
            profile_buckets: Vec<ProfileWithBucket>,
        },
    }

    fn is_matching_exit(event: &TraceEvent, kind: ScopeKind) -> bool {
        match (kind, event) {
            (
                ScopeKind::Loop(id),
                TraceEvent::Scope {
                    event: ScopeEvent::LoopExit { loop_id },
                },
            ) => *loop_id == id,
            (
                ScopeKind::Call(id),
                TraceEvent::Scope {
                    event: ScopeEvent::CallExit { call_site_id },
                },
            ) => *call_site_id == id,
            _ => false,
        }
    }

    /// Convert accumulated per-scope profile maps into `CollapsedItem::Scope`
    /// entries appended to `items`. Called at both the normal end of `collapse()`
    /// and the early return on parent-scope exit — without this, nested scopes
    /// would be invisible to their parent's branch profile.
    fn emit_scope_items(
        items: &mut Vec<CollapsedItem>,
        loop_profiles: &HashMap<u32, HashMap<BranchProfile, u32>>,
        call_profiles: &HashMap<u32, HashMap<BranchProfile, u32>>,
        buckets: &LoopBuckets,
    ) {
        let mut scope_ids: Vec<(u64, Vec<ProfileWithBucket>)> = Vec::new();
        for (id, profiles) in loop_profiles {
            let tag = (*id as u64) | (1u64 << 32);
            let mut sorted: Vec<ProfileWithBucket> = profiles
                .iter()
                .map(|(profile, &count)| {
                    let bucket = if buckets.is_disabled() {
                        None
                    } else {
                        Some(buckets.bucket(count))
                    };
                    (profile.clone(), bucket)
                })
                .collect();
            sorted.sort();
            scope_ids.push((tag, sorted));
        }
        for (id, profiles) in call_profiles {
            let tag = (*id as u64) | (2u64 << 32);
            let mut sorted: Vec<ProfileWithBucket> = profiles
                .iter()
                .map(|(profile, &count)| {
                    let bucket = if buckets.is_disabled() {
                        None
                    } else {
                        Some(buckets.bucket(count))
                    };
                    (profile.clone(), bucket)
                })
                .collect();
            sorted.sort();
            scope_ids.push((tag, sorted));
        }
        scope_ids.sort_by_key(|(tag, _)| *tag);
        for (scope_tag, profile_buckets) in scope_ids {
            items.push(CollapsedItem::Scope {
                scope_tag,
                profile_buckets,
            });
        }
    }

    fn collapse(events: &[TraceEvent], buckets: &LoopBuckets) -> (Vec<CollapsedItem>, usize) {
        let mut items: Vec<CollapsedItem> = Vec::new();
        let mut loop_profiles: HashMap<u32, HashMap<BranchProfile, u32>> = HashMap::new();
        let mut call_profiles: HashMap<u32, HashMap<BranchProfile, u32>> = HashMap::new();

        let mut i = 0;
        while i < events.len() {
            match &events[i] {
                TraceEvent::Branch { decision } => {
                    items.push(CollapsedItem::Branch {
                        branch_id: decision.branch_id,
                        taken: decision.taken,
                    });
                    i += 1;
                }
                TraceEvent::Scope { event } => {
                    let (kind, is_enter) = match event {
                        ScopeEvent::LoopEnter { loop_id } => (ScopeKind::Loop(*loop_id), true),
                        ScopeEvent::LoopExit { loop_id } => (ScopeKind::Loop(*loop_id), false),
                        ScopeEvent::CallEnter { call_site_id } => {
                            (ScopeKind::Call(*call_site_id), true)
                        }
                        ScopeEvent::CallExit { call_site_id } => {
                            (ScopeKind::Call(*call_site_id), false)
                        }
                    };

                    if !is_enter {
                        // Exit without matching enter — we've hit the boundary
                        // of our parent scope. Emit accumulated scope items
                        // before returning so nested scopes are visible to
                        // the parent's branch profile.
                        emit_scope_items(&mut items, &loop_profiles, &call_profiles, buckets);
                        return (items, i);
                    }

                    // Recursively collapse the scope body
                    let body_start = i + 1;
                    let (child_items, consumed) = collapse(&events[body_start..], buckets);
                    let body_end = body_start + consumed;

                    // Build this iteration's branch profile (sorted for determinism)
                    let mut profile = BTreeSet::new();
                    for item in &child_items {
                        match item {
                            CollapsedItem::Branch { branch_id, taken } => {
                                profile.insert((*branch_id, *taken));
                            }
                            CollapsedItem::Scope {
                                scope_tag,
                                profile_buckets,
                            } => {
                                let mut h = DefaultHasher::new();
                                scope_tag.hash(&mut h);
                                profile_buckets.hash(&mut h);
                                let synthetic_id = (h.finish() & 0x7FFF_FFFF) as u32 | 0x8000_0000;
                                profile.insert((synthetic_id, true));
                            }
                        }
                    }
                    let profile_vec: Vec<(u32, bool)> = profile.into_iter().collect();

                    // Increment iteration count for this (scope_id, profile) pair
                    match kind {
                        ScopeKind::Loop(id) => {
                            *loop_profiles
                                .entry(id)
                                .or_default()
                                .entry(profile_vec)
                                .or_insert(0) += 1;
                        }
                        ScopeKind::Call(id) => {
                            *call_profiles
                                .entry(id)
                                .or_default()
                                .entry(profile_vec)
                                .or_insert(0) += 1;
                        }
                    }

                    // Skip past the exit event if present
                    i = body_end;
                    if i < events.len() && is_matching_exit(&events[i], kind) {
                        i += 1;
                    }
                }
            }
        }

        emit_scope_items(&mut items, &loop_profiles, &call_profiles, buckets);
        (items, events.len())
    }

    let (items, _) = collapse(events, buckets);
    let mut hasher = DefaultHasher::new();
    for item in &items {
        item.hash(&mut hasher);
    }
    hasher.finish()
}

/// Confidence-scored label for error intent classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorIntentLabel {
    /// "likely_validation", "likely_runtime", or "unknown"
    pub label: String,
    /// Confidence score 0.0–1.0
    pub confidence: f64,
}

/// Classify whether a thrown error is a deliberate validation rejection or an
/// accidental runtime failure by examining the branch path immediately before the error.
///
/// Pattern: a guard branch taken=true followed immediately by the error (no intervening
/// branches) suggests intentional input rejection. Errors without a preceding guard
/// or deep in the branch path are more likely accidental.
pub fn classify_error_intent(result: &crate::protocol::ExecuteResult) -> Option<ErrorIntentLabel> {
    let error = result.thrown_error.as_ref()?;

    // If the frontend already classified this as infrastructure, trust it
    if error.error_category.as_deref() == Some("infrastructure") {
        return Some(ErrorIntentLabel {
            label: "likely_runtime".into(),
            confidence: 0.9,
        });
    }

    let branch_path = &result.branch_path;
    if branch_path.is_empty() {
        // No branch data — can't infer intent
        return Some(ErrorIntentLabel {
            label: "unknown".into(),
            confidence: 0.0,
        });
    }

    // Check the last branch before the error
    let last_branch = branch_path.last().unwrap();

    // Guard pattern: last branch taken=true, suggesting an if-guard that led to a throw.
    // Shallow depth (few branches) + guard = likely validation.
    if last_branch.taken && branch_path.len() <= 3 {
        return Some(ErrorIntentLabel {
            label: "likely_validation".into(),
            confidence: 0.7,
        });
    }

    // Deep branch path with error = likely accidental runtime failure
    if branch_path.len() > 5 {
        return Some(ErrorIntentLabel {
            label: "likely_runtime".into(),
            confidence: 0.6,
        });
    }

    // Frontend-level classification can help disambiguate
    if error.error_category.as_deref() == Some("validation") {
        return Some(ErrorIntentLabel {
            label: "likely_validation".into(),
            confidence: 0.8,
        });
    }
    if error.error_category.as_deref() == Some("runtime") {
        return Some(ErrorIntentLabel {
            label: "likely_runtime".into(),
            confidence: 0.7,
        });
    }

    Some(ErrorIntentLabel {
        label: "unknown".into(),
        confidence: 0.3,
    })
}

/// Check whether the frontend declared support for a specific command.
pub(crate) fn frontend_supports(caps: &FrontendCapabilities, command: &str) -> bool {
    caps.commands.contains(command)
}

/// Transition per-dep `LiveFirstState` based on connection failures reported
/// in an `ExecuteResult`. Deps with connection failures transition to
/// `Unavailable`; deps that were called successfully (present in
/// `calls_to_external` but absent from `connection_failures`) transition
/// toward `Available`.
pub(crate) fn update_live_first_states(
    result: &ExecuteResult,
    states: &mut HashMap<String, LiveFirstState>,
) {
    let failed_symbols: std::collections::HashSet<&str> = result
        .connection_failures
        .iter()
        .map(|cf| cf.symbol.as_str())
        .collect();

    // Transition failed deps.
    for cf in &result.connection_failures {
        let kind = classify_connection_failure(&cf.message)
            .unwrap_or(crate::mock_value_space::ConnectionFailureKind::Other);
        let state = states.entry(cf.symbol.clone()).or_default();
        let new_state = state.transition(&LiveCallOutcome::ConnectionFailure { kind });
        if *state != new_state {
            log::info!(
                "External dep '{}' unavailable — switching to autonomous mocking",
                cf.symbol
            );
            *state = new_state;
        }
    }

    // Transition successful deps (called but not failed).
    for call in &result.calls_to_external {
        if !failed_symbols.contains(call.symbol.as_str()) {
            let state = states.entry(call.symbol.clone()).or_default();
            let new_state = state.transition(&LiveCallOutcome::Success);
            *state = new_state;
        }
    }
}

/// Override mock behavior for deps whose `LiveFirstState` is `Unavailable`.
///
/// Passthrough mocks are switched to `ReturnGenerated` so the frontend
/// produces autonomous values instead of attempting a live call.
pub(crate) fn apply_live_first_overrides(
    states: &HashMap<String, LiveFirstState>,
    mocks: &mut [MockConfig],
) {
    use crate::protocol::MockBehavior;

    for mock in mocks.iter_mut() {
        if let Some(state) = states.get(&mock.symbol)
            && !state.should_try_live()
            && mock.default_behavior == MockBehavior::Passthrough
        {
            mock.default_behavior = MockBehavior::ReturnGenerated;
        }
    }
}

/// Send a Setup command to the frontend and return a `SetupContextStack`
/// containing the returned context at the given level.
pub(crate) async fn send_setup(
    frontend: &mut Frontend,
    setup_file: &str,
    scope: &str,
    level: SetupLevel,
    project_root: Option<String>,
    execution_profile: Option<ExecutionProfile>,
) -> Result<Option<SetupContextStack>, ExploreError> {
    let response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file.to_string(),
            scope: scope.to_string(),
            level,
            project_root,
            parent_context: None,
            execution_profile,
        })
        .await?;
    match response.result {
        ResponseResult::Setup { setup_context } => Ok(Some(SetupContextStack {
            contexts: vec![SetupContextEntry {
                level,
                context: setup_context,
            }],
        })),
        ResponseResult::Error { message, .. } => {
            log::warn!("setup error for {scope}: {message}");
            Ok(None)
        }
        other => Err(ExploreError::UnexpectedResponse(format!(
            "expected Setup response, got {other:?}"
        ))),
    }
}

/// Send a Teardown command to the frontend.
pub(crate) async fn send_teardown(
    frontend: &mut Frontend,
    scope: &str,
    level: SetupLevel,
) -> Result<(), ExploreError> {
    let response = frontend
        .send(ProtoCommand::Teardown {
            scope: scope.to_string(),
            level,
        })
        .await?;
    match response.result {
        ResponseResult::TeardownAck => Ok(()),
        ResponseResult::Error { message, .. } => {
            log::warn!("teardown error for {scope}: {message}");
            Ok(())
        }
        other => Err(ExploreError::UnexpectedResponse(format!(
            "expected TeardownAck response, got {other:?}"
        ))),
    }
}

/// Explore a single function by generating random inputs and executing them.
///
/// When `setup_mgr` is provided, setup lifecycle is tracked through the
/// `SetupManager` (context caching, failure tracking, skip decisions).
/// When `None`, uses the legacy direct-send approach.
pub async fn explore_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &ExploreConfig,
    mut setup_mgr: Option<&mut SetupManager>,
    progress_hints: Option<ProgressHints<'_>>,
) -> Result<ObservationOutput, ExploreError> {
    if config.observer_pool > 1
        && let Some(observer_frontend_config) = config.observer_frontend_config.clone()
    {
        return explore_function_with_observer_pool(
            frontend,
            analysis,
            config,
            setup_mgr,
            progress_hints,
            observer_frontend_config,
        )
        .await;
    }

    let instrument_response = frontend
        .send(ProtoCommand::Instrument {
            file: config.file.clone(),
            function: analysis.name.clone(),
            mocks: config.mocks.clone(),
            project_root: config.project_root.clone(),
            execution_profile: config.execution_profile.clone(),
        })
        .instrument(tracing::info_span!("explore.instrument"))
        .await?;

    let instrumentable_line_count = match instrument_response.result {
        ResponseResult::Instrument {
            instrumented,
            instrumentable_line_count,
            ..
        } => {
            if !instrumented {
                return Err(ExploreError::UnexpectedResponse(
                    "instrumentation returned instrumented=false".to_string(),
                ));
            }
            instrumentable_line_count
        }
        ResponseResult::Error { code, message, .. } => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "instrument error ({code:?}): {message}"
            )));
        }
        other => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "expected Instrument response, got {other:?}"
            )));
        }
    };

    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    // --- Setup lifecycle ---
    let has_setup = config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_level == SetupLevel::Function;
    let per_execution_setup = has_setup && config.setup_level == SetupLevel::Execution;

    let mut setup_context: Option<SetupContextStack> = None;

    // When a SetupManager is provided, check should_skip and cache contexts.
    let skip_setup = setup_mgr
        .as_ref()
        .is_some_and(|m| m.should_skip(config.setup_level));

    if per_function_setup
        && !skip_setup
        && let Some(ref setup_file) = config.setup_file
    {
        match send_setup(
            frontend,
            setup_file,
            &analysis.name,
            config.setup_level,
            config.project_root.clone(),
            config.execution_profile.clone(),
        )
        .instrument(tracing::info_span!("setup.function"))
        .await?
        {
            Some(ctx) => {
                if let Some(ref mut mgr) = setup_mgr
                    && let Some(entry) = ctx.contexts.first()
                {
                    let _ = mgr.setup(config.setup_level, &analysis.name, entry.context.clone());
                }
                setup_context = Some(ctx);
            }
            None => {
                if let Some(ref mut mgr) = setup_mgr {
                    let _ = mgr.record_failure(
                        config.setup_level,
                        format!("setup returned no context for {}", analysis.name),
                    );
                }
            }
        }
    }

    // --- Generator prefetch ---
    let has_generators = config
        .value_sources
        .iter()
        .any(|s| matches!(s, ValueSource::CustomGenerator { .. }));
    let use_generators = has_generators && frontend_supports(&config.capabilities, "generate");

    let mut prefetched = if use_generators {
        prefetch_custom_values(
            &config.value_sources,
            frontend,
            custom_generator_prefetch_budget(&config.value_sources, config.max_iterations),
        )
        .instrument(tracing::info_span!("input_gen.prefetch"))
        .await
        .unwrap_or_else(|e| {
            log::debug!("prefetch failed, falling back to built-in: {e}");
            PrefetchedValues::new()
        })
    } else {
        PrefetchedValues::new()
    };

    // ObservationAggregator owns the per-execution merge state previously
    // inlined in this function (paths, branches, lines, discoveries,
    // raw_results, new_path_executions, iterations counter, last-discovery
    // iteration). See `observation_aggregator.rs` and
    // `docs/specs/concurrent-single-function-exploration.md` §6 for the
    // out-of-order-safe aggregation contract that str-frc.3 (observer pool)
    // will route through this same seam.
    let mut aggregator =
        crate::observation_aggregator::ObservationAggregator::new(config.loop_buckets.clone());

    // Tracked for progress reporting: number of branches observed at the last
    // periodic snapshot. The aggregator owns the iteration index at which
    // the most recent new path was aggregated, exposed via
    // `iters_since_new_discovery()`.
    let mut last_reported_branches: usize = 0;

    // --- Prepare lifecycle ---
    // When the frontend supports `prepare`, pre-build the harness once so all
    // subsequent Execute calls can skip the compile phase. When the orchestrator
    // has already issued a Prepare (under its build_timeout) it passes the id
    // via `prepare_id_override` so we don't pay the build cost against the
    // per-function exploration budget.
    let prepare_id: Option<String> = if let Some(id) = config.prepare_id_override.clone() {
        Some(id)
    } else if frontend_supports(&config.capabilities, "prepare") {
        match frontend
            .send(ProtoCommand::Prepare {
                file: config.file.clone(),
                function: analysis.name.clone(),
                mocks: config.mocks.clone(),
                project_root: config.project_root.clone(),
                execution_profile: config.execution_profile.clone(),
                plan: config.default_execute_plan.clone(),
            })
            .instrument(tracing::info_span!("explore.prepare"))
            .await
        {
            Ok(resp) => match resp.result {
                crate::protocol::ResponseResult::Prepare { prepare_id } => {
                    log::debug!("prepare succeeded: {prepare_id}");
                    Some(prepare_id)
                }
                other => {
                    log::debug!("prepare returned unexpected response: {other:?}");
                    None
                }
            },
            Err(e) => {
                log::debug!("prepare failed, falling back to per-execute build: {e}");
                None
            }
        }
    } else {
        None
    };

    // Per-dependency LiveFirst state: track whether each external dep is
    // reachable (live calls pass through) or unavailable (fall back to
    // autonomous mocks). All deps start as Untried.
    let mut live_first_states: HashMap<String, LiveFirstState> = HashMap::new();

    // --- Float probe phase ---
    // Probes consume from the iteration budget, contributing to obs_state and raw_results.
    let float_indices = crate::float_probe::float_param_indices(&analysis.params);
    let mut float_probe_results: Vec<crate::float_probe::FloatProbeResult> = Vec::new();
    let probe_budget = float_indices.len() * crate::float_probe::PROBE_COUNT * 2;
    if !float_indices.is_empty()
        && config
            .max_iterations
            .is_none_or(|m| probe_budget < m as usize)
    {
        for &idx in &float_indices {
            let pairs = crate::float_probe::generate_probe_pairs(
                &analysis.params,
                idx,
                crate::float_probe::PROBE_COUNT,
                &mut rng,
            );
            let mut agreements = 0usize;
            let mut total_probes = 0usize;
            let mut divergent_values = Vec::new();

            for (float_inputs, floor_inputs) in pairs {
                let float_resp = frontend
                    .send(ProtoCommand::Execute {
                        function: analysis.name.clone(),
                        inputs: float_inputs.clone(),
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
                        capture: false,
                        prepare_id: prepare_id.clone(),
                        execution_profile: config.execution_profile.clone(),
                        plan: config.default_execute_plan.clone(),
                    })
                    .await?;

                let floor_resp = frontend
                    .send(ProtoCommand::Execute {
                        function: analysis.name.clone(),
                        inputs: floor_inputs,
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
                        capture: false,
                        prepare_id: prepare_id.clone(),
                        execution_profile: config.execution_profile.clone(),
                        plan: config.default_execute_plan.clone(),
                    })
                    .await?;

                if let (
                    ResponseResult::Execute(float_result),
                    ResponseResult::Execute(floor_result),
                ) = (&float_resp.result, &floor_resp.result)
                {
                    total_probes += 1;

                    let fhash = path_hash(float_result, &config.loop_buckets);
                    let flhash = path_hash(floor_result, &config.loop_buckets);
                    let obs_state = aggregator.observe_state_mut();
                    obs_state.seen_paths.insert(fhash);
                    obs_state.seen_paths.insert(flhash);
                    for &line in &float_result.lines_executed {
                        obs_state.all_lines.insert(line);
                    }
                    for &line in &floor_result.lines_executed {
                        obs_state.all_lines.insert(line);
                    }

                    if crate::float_probe::executions_agree(float_result, floor_result) {
                        agreements += 1;
                    } else if let Some(v) = float_inputs.get(idx).and_then(|v| v.as_f64()) {
                        divergent_values.push(v);
                    }

                    aggregator.push_raw_result(
                        float_inputs.clone(),
                        config.mocks.clone(),
                        (**float_result).clone(),
                    );
                }
            }

            let classification = crate::float_probe::classify(
                agreements,
                total_probes,
                crate::float_probe::AGREEMENT_THRESHOLD,
            );
            float_probe_results.push(crate::float_probe::FloatProbeResult {
                param_index: idx,
                param_name: analysis.params[idx].name.clone(),
                classification,
                agreements,
                total_probes,
                divergent_values,
            });
        }
    }

    // --- MetaStrategy construction ---
    // Replaces the previous hardcoded iterator chain (user → literals → candidates → pool →
    // random). MetaStrategy handles all input generation; custom generators (which require an
    // async frontend round-trip) remain a fallback path outside the strategy.
    //
    // Strategy set for the random path:
    //   [UserProvided, Literals, PoolSeeds, BoundarySeeds, Z3Solver, Random]
    //
    // user_seeds and candidate_inputs are combined into a single UserProvidedStrategy since
    // they share the same semantics (pre-specified inputs executed with highest priority).
    // When custom generators are enabled, RandomStrategy is excluded so that MetaStrategy
    // can fall back to the async generator whenever the reactive Z3Solver has no queued
    // branch-guided input.
    let mut meta_strategy = build_random_explorer_meta_strategy(
        &analysis.params,
        &analysis.literals,
        config.user_seeds.clone(),
        config.candidate_inputs.clone(),
        config.pool_seeds.clone(),
        use_generators,
        config.meta_config.clone(),
    );
    let strategy_ctx = StrategyContext {
        params: analysis.params.clone(),
        literals: analysis.literals.clone(),
        capabilities: config.capabilities.clone(),
    };
    let explore_start = Instant::now();
    // str-gz8j: track whether the per-function timeout (timeout_explore)
    // tripped, so the returned ObservationOutput can carry that signal.
    let mut timed_out_due_to_budget = false;
    let mut last_summary_time = Instant::now();
    let mut effective_budget = config.max_iterations;
    // Track recent path discoveries for surplus claim decisions.
    // Ring buffer: true = new path, false = duplicate.
    let claim_window = config.claim_policy.window as usize;
    let mut recent_hits: Vec<bool> = Vec::with_capacity(claim_window);

    loop {
        if let Some(budget) = effective_budget
            && aggregator.iterations() >= budget
        {
            // Initial budget exhausted — try to claim surplus if still productive.
            if let Some(ref surplus) = config.budget_surplus {
                let base = config.max_iterations.unwrap_or(budget);
                let recent_new = recent_hits
                    .iter()
                    .rev()
                    .take(claim_window)
                    .filter(|&&hit| hit)
                    .count() as u32;
                if config.claim_policy.should_claim(recent_new) {
                    let chunk = (base / 4).max(1);
                    let max_claimable = config.claim_policy.max_claimable(surplus.available());
                    let requested = chunk.min(max_claimable);
                    let claimed = surplus.try_claim(requested, 1);
                    if claimed > 0 {
                        effective_budget = Some(budget + claimed);
                        log::debug!(
                            "{}: claimed {} surplus iterations (budget now {})",
                            analysis.name,
                            claimed,
                            budget + claimed
                        );
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if let Some(timeout) = config.timeout_explore
            && explore_start.elapsed() >= timeout
        {
            // str-gz8j: record that we exited because of the per-function
            // timeout, so the surfaced ObservationOutput can be downgraded
            // to OutcomeStatus::TimedOut by the CLI explore command rather
            // than masquerading as a clean Completed run.
            timed_out_due_to_budget = true;
            break;
        }

        // The aggregator's iteration counter advances inside
        // `record_post_observe()` after the Execute round-trip below; the
        // periodic progress summary reads its current snapshot pre-execute
        // (one iteration behind) and the post-execute updates surface in
        // the next emission. This matches the prior behavior where the
        // progress summary was emitted at the top of the loop with the
        // *previous* iteration's counters.
        let pre_iteration_count = aggregator.iterations();
        if aggregator.discoveries_count() > last_reported_branches {
            last_reported_branches = aggregator.discoveries_count();
        }
        if let Some(hints) = progress_hints.as_ref() {
            let since_last = last_summary_time.elapsed();
            if since_last >= Duration::from_secs(PROGRESS_SUMMARY_INTERVAL_SECS) {
                let total_branches = hints.total_branches.or(Some(analysis.branches.len()));
                (hints.callback)(&ExploreProgressSnapshot {
                    function_name: analysis.name.clone(),
                    elapsed: explore_start.elapsed(),
                    iterations: pre_iteration_count,
                    paths_found: aggregator.unique_paths_count(),
                    total_branches,
                    branches_covered: Some(aggregator.discoveries_count()),
                    mcdc_summary: None,
                    iters_since_new_discovery: aggregator.iters_since_new_discovery(),
                });
                last_summary_time = Instant::now();
            }
        }

        // --- Per-execution setup ---
        if per_execution_setup
            && !skip_setup
            && let Some(ref setup_file) = config.setup_file
        {
            match send_setup(
                frontend,
                setup_file,
                &analysis.name,
                config.setup_level,
                config.project_root.clone(),
                config.execution_profile.clone(),
            )
            .instrument(tracing::info_span!("setup.execution"))
            .await?
            {
                Some(ctx) => {
                    if let Some(ref mut mgr) = setup_mgr
                        && let Some(entry) = ctx.contexts.first()
                    {
                        let _ =
                            mgr.setup(config.setup_level, &analysis.name, entry.context.clone());
                    }
                    setup_context = Some(ctx);
                }
                None => {
                    if let Some(ref mut mgr) = setup_mgr {
                        let _ = mgr.record_failure(
                            config.setup_level,
                            format!(
                                "per-execution setup returned no context for {}",
                                analysis.name
                            ),
                        );
                    }
                }
            }
        }

        // --- Input generation ---
        // Poll MetaStrategy for the next candidate inputs. When MetaStrategy is
        // exhausted (only possible when custom generators are enabled and all finite
        // strategies have been drained), fall back to the custom generator path.
        // Returns (inputs, strategy_idx): strategy_idx is None for custom-generator fallback.
        let (inputs, strategy_idx) = {
            let _input_gen_span = tracing::info_span!("input_gen").entered();
            match meta_strategy.next(&strategy_ctx, &mut rng) {
                Some((v, idx)) => {
                    let strategy_kind = meta_strategy.strategy_kind(idx);
                    let inputs = if use_generators
                        && strategy_kind != crate::strategy::RegisteredStrategyKind::Z3Solver
                    {
                        if !custom_generator_values_available(&config.value_sources, &prefetched) {
                            break;
                        }
                        overlay_custom_inputs(
                            &analysis.params,
                            &config.value_sources,
                            v,
                            &mut prefetched,
                            &mut rng,
                            Some(&config.capabilities),
                        )
                    } else {
                        v
                    };
                    (inputs, Some(idx))
                }
                None if use_generators => {
                    // Custom generators require an async frontend round-trip; they cannot
                    // be a standard InputStrategy, so they serve as the infinite fallback
                    // when MetaStrategy (finite strategies only) is exhausted.
                    if !custom_generator_values_available(&config.value_sources, &prefetched) {
                        break;
                    }
                    let v = generate_inputs_with_custom(
                        &analysis.params,
                        &config.value_sources,
                        &mut prefetched,
                        &mut rng,
                        Some(&config.capabilities),
                    );
                    (v, None)
                }
                None => break,
            }
        };

        // --- Mock generation ---
        // When mock_params is non-empty, generate fresh mock values each iteration
        // to vary dependency behavior across exploration runs.
        let mut iteration_mocks = if !config.mock_params.is_empty() {
            generate_mock_values(&config.mock_params, &mut rng, Some(&config.capabilities))
        } else {
            config.mocks.clone()
        };

        // --- LiveFirst mock adjustment ---
        // For deps whose LiveFirstState is Unavailable, override their
        // mock behavior from Passthrough to autonomous (ReturnGenerated).
        apply_live_first_overrides(&live_first_states, &mut iteration_mocks);

        // --- Execute + classify via canonical observe primitive ---
        // observe_single mutates the aggregator's ObserveState in place;
        // record_post_observe (below) folds the rest of the per-execution
        // event (raw_results, discoveries, new_path_executions, iterations,
        // last_discovery_iteration) without re-deriving what observe_single
        // already computed.
        let obs = crate::observe::observe_single(
            frontend,
            &analysis.name,
            &inputs,
            &iteration_mocks,
            setup_context.as_ref(),
            config.execution_profile.as_ref(),
            &config.loop_buckets,
            aggregator.observe_state_mut(),
            config.capture_side_effects,
            prepare_id.as_deref(),
        )
        .instrument(tracing::info_span!("explore.execute_round_trip"))
        .await
        .map_err(|e| match e {
            crate::observe::ObserveError::Frontend(fe) => ExploreError::Frontend(fe),
            crate::observe::ObserveError::Unsupported(msg) => ExploreError::Unsupported(msg),
            crate::observe::ObserveError::UnexpectedResponse(msg)
            | crate::observe::ObserveError::InstrumentationFailed(msg) => {
                ExploreError::UnexpectedResponse(msg)
            }
        })?;

        // --- LiveFirst state transitions ---
        // Check connection_failures reported by the frontend and transition
        // per-dep states accordingly.
        update_live_first_states(&obs.exec_result, &mut live_first_states);

        // --- Crypto boundary logging ---
        // When the frontend intercepts known encrypt/decrypt calls, log them.
        // The boundaries are stored in the execution record (runtime_crypto_boundaries)
        // and will be used for boundary splitting in a future solver integration pass.
        if !obs.exec_result.runtime_crypto_boundaries.is_empty() {
            tracing::debug!(
                count = obs.exec_result.runtime_crypto_boundaries.len(),
                boundaries = ?obs.exec_result.runtime_crypto_boundaries
                    .iter()
                    .map(|b| format!("{} ({})", b.function_name, b.boundary_id))
                    .collect::<Vec<_>>(),
                "crypto boundaries detected in execution trace"
            );
        }

        // --- MetaStrategy feedback and outcome recording ---
        // Fan out the execution result to all strategies for adaptive scoring.
        // record_outcome updates the sliding window for the strategy that produced
        // these inputs, enabling adaptive reallocation.
        meta_strategy.feedback(&inputs, &obs.exec_result, obs.is_new_path);
        if let Some(idx) = strategy_idx {
            meta_strategy.record_outcome(idx, obs.is_new_path);
        }

        // --- Per-execution teardown ---
        if per_execution_setup && !skip_setup && frontend_supports(&config.capabilities, "teardown")
        {
            send_teardown(frontend, &analysis.name, config.setup_level)
                .instrument(tracing::info_span!("teardown.execution"))
                .await?;
            if let Some(ref mut mgr) = setup_mgr {
                mgr.teardown(config.setup_level, &analysis.name);
            }
        }

        // Attribute discovery to the strategy that produced the inputs.
        let discovery_method = strategy_idx
            .map(|idx| meta_strategy.strategy_kind(idx).explorer_discovery_method())
            .unwrap_or(
                SpecialCandidatePath::ExplorerCustomGeneratorFallback.explorer_discovery_method(),
            );

        // Track recent path discovery rate for surplus claim decisions.
        if config.budget_surplus.is_some() {
            if recent_hits.len() >= claim_window && claim_window > 0 {
                recent_hits.remove(0);
            }
            recent_hits.push(obs.is_new_path);
        }

        aggregator.record_post_observe(
            inputs,
            iteration_mocks,
            obs.exec_result,
            discovery_method,
            obs.is_new_path,
            &obs.new_branch_ids,
        );
    }

    // --- Per-function teardown ---
    if per_function_setup && !skip_setup && frontend_supports(&config.capabilities, "teardown") {
        send_teardown(frontend, &analysis.name, config.setup_level)
            .instrument(tracing::info_span!("teardown.function"))
            .await?;
        if let Some(ref mut mgr) = setup_mgr {
            mgr.teardown(config.setup_level, &analysis.name);
        }
    }

    let total_lines = instrumentable_line_count
        .unwrap_or_else(|| analysis.end_line.saturating_sub(analysis.start_line) + 1);
    let reconcile_start = analysis.start_line;
    let reconcile_end = analysis.end_line;
    let reconcile_instrumentable = instrumentable_line_count;

    // -- Witness shrinking phase --
    let mut shrunk_witnesses: std::collections::HashMap<u64, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut shrink_stats = crate::shrink::ShrinkStats::default();
    if should_run_shrink_pass(config, explore_start, timed_out_due_to_budget) {
        // Collect the lowest-complexity witness per unique path.
        // Starting from the simplest witness reduces shrink iterations needed.
        let mut path_witnesses: std::collections::HashMap<
            u64,
            (Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>),
        > = std::collections::HashMap::new();
        for (inputs, mocks, result) in aggregator.raw_results() {
            let ph = crate::orchestrator::hash_branch_path(&result.branch_path);
            let complexity = crate::shrink::witness_complexity(inputs);
            let entry = path_witnesses
                .entry(ph)
                .or_insert_with(|| (inputs.clone(), mocks.clone()));
            if complexity < crate::shrink::witness_complexity(&entry.0) {
                *entry = (inputs.clone(), mocks.clone());
            }
        }

        // Selection policy: skip witnesses that are already minimal (complexity ≤
        // SHRINK_SKIP_THRESHOLD), then process in descending complexity order so
        // that the highest-value witnesses are shrunk first. Tie-break by ascending
        // path hash for fully deterministic ordering independent of HashMap iteration.
        let paths_considered = path_witnesses.len();
        let mut to_shrink: Vec<(
            u64,
            Vec<serde_json::Value>,
            Vec<crate::protocol::MockConfig>,
        )> = path_witnesses
            .into_iter()
            .filter(|(_, (inputs, _))| {
                crate::shrink::should_shrink_path(crate::shrink::witness_complexity(inputs))
            })
            .map(|(ph, (inputs, mocks))| (ph, inputs, mocks))
            .collect();
        to_shrink.sort_by(|(ph_a, inputs_a, _), (ph_b, inputs_b, _)| {
            let ca = crate::shrink::witness_complexity(inputs_a);
            let cb = crate::shrink::witness_complexity(inputs_b);
            cb.cmp(&ca).then(ph_a.cmp(ph_b))
        });

        shrink_stats = crate::shrink::ShrinkStats {
            paths_considered,
            paths_skipped_simple: paths_considered - to_shrink.len(),
            ..Default::default()
        };

        for (ph, witness, witness_mocks) in &to_shrink {
            let effective_mocks = if witness_mocks.is_empty() {
                config.mocks.clone()
            } else {
                witness_mocks.clone()
            };

            let mut current = witness.clone();
            let mut attempts = 0usize;
            let witness_budget = crate::shrink::shrink_budget_for_witness(
                crate::shrink::witness_complexity(witness),
                config.shrink_budget,
            );

            // Phase 1: bulk shrink — try all parameters at once (1 execute call).
            let mut bulk_accepted = false;
            if explore_deadline_crossed(config, explore_start) {
                timed_out_due_to_budget = true;
                break;
            }
            if attempts < witness_budget
                && let Some(bulk_trial) =
                    crate::shrink::bulk_shrink_candidate(&current, &analysis.params)
            {
                attempts += 1;
                let resp = frontend
                    .send(ProtoCommand::Execute {
                        function: analysis.name.clone(),
                        inputs: bulk_trial.clone(),
                        mocks: effective_mocks.clone(),
                        setup_context: None,
                        capture: true,
                        prepare_id: prepare_id.clone(),
                        execution_profile: config.execution_profile.clone(),
                        plan: config.default_execute_plan.clone(),
                    })
                    .instrument(tracing::info_span!("shrink.execute_round_trip"))
                    .await;
                if let Ok(resp) = resp
                    && let ResponseResult::Execute(exec_res) = resp.result
                    && crate::orchestrator::hash_branch_path(&exec_res.branch_path) == *ph
                {
                    current = bulk_trial;
                    bulk_accepted = true;
                }
            }

            // Phase 1.5: grouped fallback — when bulk was rejected and N >= 3, try
            // consecutive groups of floor(N/2) parameters before the per-param loop.
            // Costs ≈2 execute calls and shrinks multiple params per accepted trial.
            let n = analysis.params.len().min(current.len());
            if !bulk_accepted && n >= 3 && attempts < witness_budget {
                let group_size = n / 2;
                for trial in
                    crate::shrink::grouped_shrink_candidates(&current, &analysis.params, group_size)
                {
                    if explore_deadline_crossed(config, explore_start) {
                        timed_out_due_to_budget = true;
                        break;
                    }
                    if attempts >= witness_budget {
                        break;
                    }
                    attempts += 1;
                    let resp = frontend
                        .send(ProtoCommand::Execute {
                            function: analysis.name.clone(),
                            inputs: trial.clone(),
                            mocks: effective_mocks.clone(),
                            setup_context: None,
                            capture: false,
                            prepare_id: prepare_id.clone(),
                            execution_profile: config.execution_profile.clone(),
                            plan: config.default_execute_plan.clone(),
                        })
                        .instrument(tracing::info_span!("shrink.execute_round_trip"))
                        .await;
                    if let Ok(resp) = resp
                        && let ResponseResult::Execute(exec_res) = resp.result
                        && crate::orchestrator::hash_branch_path(&exec_res.branch_path) == *ph
                    {
                        current = trial;
                    }
                }
            }

            // Phase 2: one-at-a-time per-param loop.
            let mut progress = true;
            while progress && attempts < witness_budget {
                progress = false;
                for i in 0..analysis.params.len().min(current.len()) {
                    if explore_deadline_crossed(config, explore_start) {
                        timed_out_due_to_budget = true;
                        break;
                    }
                    let candidates =
                        crate::shrink::shrink_candidates(&current[i], &analysis.params[i].typ);
                    for candidate in candidates {
                        if explore_deadline_crossed(config, explore_start) {
                            timed_out_due_to_budget = true;
                            break;
                        }
                        if attempts >= witness_budget {
                            break;
                        }
                        let mut trial = current.clone();
                        trial[i] = candidate;
                        attempts += 1;

                        let resp = frontend
                            .send(ProtoCommand::Execute {
                                function: analysis.name.clone(),
                                inputs: trial.clone(),
                                mocks: effective_mocks.clone(),
                                setup_context: None,
                                capture: false,
                                prepare_id: prepare_id.clone(),
                                execution_profile: config.execution_profile.clone(),
                                plan: config.default_execute_plan.clone(),
                            })
                            .instrument(tracing::info_span!("shrink.execute_round_trip"))
                            .await;

                        if let Ok(resp) = resp
                            && let ResponseResult::Execute(exec_res) = resp.result
                            && crate::orchestrator::hash_branch_path(&exec_res.branch_path) == *ph
                        {
                            current = trial;
                            progress = true;
                            break;
                        }
                    }
                    if attempts >= witness_budget {
                        break;
                    }
                }
                if timed_out_due_to_budget {
                    break;
                }
            }

            shrink_stats.paths_shrunk += 1;
            shrink_stats.total_shrink_attempts += attempts;
            shrink_stats.total_budget_assigned += witness_budget;

            if current != *witness {
                shrunk_witnesses.insert(*ph, current);
            }
        }

        tracing::debug!(
            paths_considered = shrink_stats.paths_considered,
            paths_skipped_simple = shrink_stats.paths_skipped_simple,
            paths_shrunk = shrink_stats.paths_shrunk,
            total_shrink_attempts = shrink_stats.total_shrink_attempts,
            total_budget_assigned = shrink_stats.total_budget_assigned,
            "shrink pass complete"
        );
    }

    let opaque_suggestions = crate::executability::build_opaque_suggestions(
        &analysis.params,
        &std::collections::HashMap::new(),
    );
    let stubbed_modules = collect_stubbed_modules(aggregator.raw_results());
    let mut output = aggregator.into_observation_output(
        analysis.name.clone(),
        total_lines,
        timed_out_due_to_budget,
        vec![],
        float_probe_results,
        vec![],
        shrunk_witnesses,
        None,
        shrink_stats,
        vec![],
        opaque_suggestions,
        stubbed_modules,
    );
    crate::observe::reconcile_observation_coverage(
        &mut output,
        reconcile_start,
        reconcile_end,
        reconcile_instrumentable,
    );
    Ok(output)
}

fn explore_deadline_crossed(config: &ExploreConfig, explore_start: Instant) -> bool {
    config
        .timeout_explore
        .is_some_and(|timeout| explore_start.elapsed() >= timeout)
}

fn should_run_shrink_pass(
    config: &ExploreConfig,
    explore_start: Instant,
    timed_out_due_to_budget: bool,
) -> bool {
    config.shrink_budget > 0
        && !timed_out_due_to_budget
        && !explore_deadline_crossed(config, explore_start)
}

struct ObserverJob {
    inputs: Vec<serde_json::Value>,
    mocks: Vec<MockConfig>,
    strategy_idx: Option<usize>,
}

struct CandidateQueuePolicy {
    capacity: usize,
    fingerprint_lru_capacity: usize,
    fingerprints: VecDeque<u64>,
    fingerprint_set: HashSet<u64>,
    duplicates_suppressed: u64,
}

impl CandidateQueuePolicy {
    #[cfg(test)]
    fn new(observer_pool: usize, max_iterations: Option<u32>) -> Self {
        Self::with_capacity_override(observer_pool, max_iterations, None)
    }

    fn with_capacity_override(
        observer_pool: usize,
        max_iterations: Option<u32>,
        capacity_override: Option<usize>,
    ) -> Self {
        let capacity = match capacity_override {
            Some(explicit) => explicit.max(1),
            None => default_candidate_queue_capacity(observer_pool, max_iterations),
        };
        Self {
            capacity,
            fingerprint_lru_capacity: capacity.saturating_mul(4).max(1),
            fingerprints: VecDeque::new(),
            fingerprint_set: HashSet::new(),
            duplicates_suppressed: 0,
        }
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn duplicates_suppressed(&self) -> u64 {
        self.duplicates_suppressed
    }

    fn should_enqueue(&mut self, inputs: &[serde_json::Value], mocks: &[MockConfig]) -> bool {
        let fingerprint = candidate_fingerprint(inputs, mocks);
        if self.fingerprint_set.contains(&fingerprint) {
            self.duplicates_suppressed = self.duplicates_suppressed.saturating_add(1);
            return false;
        }

        self.fingerprints.push_back(fingerprint);
        self.fingerprint_set.insert(fingerprint);
        while self.fingerprints.len() > self.fingerprint_lru_capacity {
            if let Some(evicted) = self.fingerprints.pop_front() {
                self.fingerprint_set.remove(&evicted);
            }
        }
        true
    }
}

fn default_candidate_queue_capacity(observer_pool: usize, max_iterations: Option<u32>) -> usize {
    let pool = observer_pool.max(1);
    let budget_cap = max_iterations.unwrap_or(256).max(1) as usize;
    (pool * 4).min(budget_cap).max(1)
}

fn custom_generator_prefetch_budget(sources: &[ValueSource], max_iterations: Option<u32>) -> usize {
    let slots_per_input_vector = max_custom_generator_slots_per_generator(sources);
    if slots_per_input_vector == 0 {
        return 1;
    }
    let input_vectors = max_iterations.unwrap_or(1).max(1) as usize;
    slots_per_input_vector * input_vectors
}

fn max_custom_generator_slots_per_generator(sources: &[ValueSource]) -> usize {
    let mut slots_by_generator = HashMap::<(String, String), usize>::new();
    for source in sources {
        let ValueSource::CustomGenerator {
            generator_name,
            generator_file,
            ..
        } = source
        else {
            continue;
        };
        let key = (generator_file.display().to_string(), generator_name.clone());
        *slots_by_generator.entry(key).or_default() += 1;
    }

    slots_by_generator.into_values().max().unwrap_or(0)
}

fn custom_generator_values_available(
    sources: &[ValueSource],
    prefetched: &PrefetchedValues,
) -> bool {
    let mut required = HashMap::<(String, String), usize>::new();
    for source in sources {
        let ValueSource::CustomGenerator {
            generator_name,
            generator_file,
            ..
        } = source
        else {
            continue;
        };
        let key = (generator_file.display().to_string(), generator_name.clone());
        *required.entry(key).or_default() += 1;
    }

    required
        .into_iter()
        .all(|((file, name), count)| prefetched.remaining(&file, &name) >= count)
}

fn candidate_fingerprint(inputs: &[serde_json::Value], mocks: &[MockConfig]) -> u64 {
    let payload = serde_json::json!({
        "inputs": inputs,
        "mocks": mocks,
    });
    let canonical = crate::canonical_json::canonicalize_json(&payload);
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    hasher.finish()
}

struct ObserverObservation {
    inputs: Vec<serde_json::Value>,
    mocks: Vec<MockConfig>,
    result: ExecuteResult,
    strategy_idx: Option<usize>,
}

enum ObserverMessage {
    Observed(Box<ObserverObservation>),
    Failed(ExploreError),
}

#[derive(Clone, Copy)]
struct ObserverWorkerOptions {
    per_function_setup: bool,
    per_execution_setup: bool,
    skip_setup: bool,
}

#[allow(clippy::too_many_arguments)]
async fn explore_function_with_observer_pool(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &ExploreConfig,
    setup_mgr: Option<&mut SetupManager>,
    progress_hints: Option<ProgressHints<'_>>,
    observer_frontend_config: FrontendConfig,
) -> Result<ObservationOutput, ExploreError> {
    let instrument_response = frontend
        .send(ProtoCommand::Instrument {
            file: config.file.clone(),
            function: analysis.name.clone(),
            mocks: config.mocks.clone(),
            project_root: config.project_root.clone(),
            execution_profile: config.execution_profile.clone(),
        })
        .instrument(tracing::info_span!("explore.instrument"))
        .await?;

    let instrumentable_line_count = match instrument_response.result {
        ResponseResult::Instrument {
            instrumented,
            instrumentable_line_count,
            ..
        } => {
            if !instrumented {
                return Err(ExploreError::UnexpectedResponse(
                    "instrumentation returned instrumented=false".to_string(),
                ));
            }
            instrumentable_line_count
        }
        ResponseResult::Error { code, message, .. } => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "instrument error ({code:?}): {message}"
            )));
        }
        other => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "expected Instrument response, got {other:?}"
            )));
        }
    };

    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    let has_setup = config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_level == SetupLevel::Function;
    let per_execution_setup = has_setup && config.setup_level == SetupLevel::Execution;
    let skip_setup = setup_mgr
        .as_ref()
        .is_some_and(|m| m.should_skip(config.setup_level));

    let has_generators = config
        .value_sources
        .iter()
        .any(|s| matches!(s, ValueSource::CustomGenerator { .. }));
    let use_generators = has_generators && frontend_supports(&config.capabilities, "generate");
    let mut prefetched = if use_generators {
        prefetch_custom_values(
            &config.value_sources,
            frontend,
            custom_generator_prefetch_budget(&config.value_sources, config.max_iterations),
        )
        .instrument(tracing::info_span!("input_gen.prefetch"))
        .await
        .unwrap_or_else(|e| {
            log::debug!("prefetch failed, falling back to built-in: {e}");
            PrefetchedValues::new()
        })
    } else {
        PrefetchedValues::new()
    };

    let mut aggregator =
        crate::observation_aggregator::ObservationAggregator::new(config.loop_buckets.clone());
    let mut last_reported_branches: usize = 0;
    let mut live_first_states: HashMap<String, LiveFirstState> = HashMap::new();
    let mut meta_strategy = build_random_explorer_meta_strategy(
        &analysis.params,
        &analysis.literals,
        config.user_seeds.clone(),
        config.candidate_inputs.clone(),
        config.pool_seeds.clone(),
        use_generators,
        config.meta_config.clone(),
    );
    let strategy_ctx = StrategyContext {
        params: analysis.params.clone(),
        literals: analysis.literals.clone(),
        capabilities: config.capabilities.clone(),
    };

    let observer_pool = config.observer_pool.max(1);
    let mut queue_policy = CandidateQueuePolicy::with_capacity_override(
        observer_pool,
        config.max_iterations,
        config.candidate_queue_capacity,
    );
    let (job_tx, job_rx) = tokio::sync::mpsc::channel::<ObserverJob>(queue_policy.capacity());
    let job_rx = std::sync::Arc::new(tokio::sync::Mutex::new(job_rx));
    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<ObserverMessage>();
    let mut handles = Vec::with_capacity(observer_pool);

    for _worker_id in 0..observer_pool {
        let worker_rx = std::sync::Arc::clone(&job_rx);
        let worker_tx = result_tx.clone();
        let worker_config = observer_frontend_config.clone();
        let worker_analysis = analysis.clone();
        let worker_explore_config = config.clone();
        let worker_options = ObserverWorkerOptions {
            per_function_setup,
            per_execution_setup,
            skip_setup,
        };
        handles.push(tokio::spawn(async move {
            run_observer_worker(
                worker_config,
                worker_analysis,
                worker_explore_config,
                worker_rx,
                worker_tx,
                worker_options,
            )
            .await;
        }));
    }
    drop(result_tx);

    let explore_start = Instant::now();
    let mut timed_out_due_to_budget = false;
    let mut last_summary_time = Instant::now();
    let mut in_flight = 0usize;
    let mut producer_done = false;

    while !producer_done || in_flight > 0 {
        while !producer_done && in_flight < observer_pool {
            if let Some(budget) = config.max_iterations
                && aggregator.iterations().saturating_add(in_flight as u32) >= budget
            {
                producer_done = true;
                break;
            }

            if let Some(timeout) = config.timeout_explore
                && explore_start.elapsed() >= timeout
            {
                timed_out_due_to_budget = true;
                producer_done = true;
                break;
            }

            let (inputs, strategy_idx) = {
                let _input_gen_span = tracing::info_span!("input_gen").entered();
                match meta_strategy.next(&strategy_ctx, &mut rng) {
                    Some((v, idx)) => {
                        let strategy_kind = meta_strategy.strategy_kind(idx);
                        let inputs = if use_generators
                            && strategy_kind != crate::strategy::RegisteredStrategyKind::Z3Solver
                        {
                            if !custom_generator_values_available(
                                &config.value_sources,
                                &prefetched,
                            ) {
                                producer_done = true;
                                break;
                            }
                            overlay_custom_inputs(
                                &analysis.params,
                                &config.value_sources,
                                v,
                                &mut prefetched,
                                &mut rng,
                                Some(&config.capabilities),
                            )
                        } else {
                            v
                        };
                        (inputs, Some(idx))
                    }
                    None if use_generators => {
                        if !custom_generator_values_available(&config.value_sources, &prefetched) {
                            producer_done = true;
                            break;
                        }
                        let v = generate_inputs_with_custom(
                            &analysis.params,
                            &config.value_sources,
                            &mut prefetched,
                            &mut rng,
                            Some(&config.capabilities),
                        );
                        (v, None)
                    }
                    None => {
                        producer_done = true;
                        break;
                    }
                }
            };

            let mut iteration_mocks = if !config.mock_params.is_empty() {
                generate_mock_values(&config.mock_params, &mut rng, Some(&config.capabilities))
            } else {
                config.mocks.clone()
            };
            apply_live_first_overrides(&live_first_states, &mut iteration_mocks);

            if queue_policy.should_enqueue(&inputs, &iteration_mocks) {
                if job_tx
                    .send(ObserverJob {
                        inputs,
                        mocks: iteration_mocks,
                        strategy_idx,
                    })
                    .await
                    .is_err()
                {
                    return Err(ExploreError::UnexpectedResponse(
                        "observer pool stopped accepting jobs".to_string(),
                    ));
                }
                in_flight += 1;
            } else if queue_policy.duplicates_suppressed().is_multiple_of(256) {
                log::debug!(
                    "suppressed {} duplicate candidate(s) for {}",
                    queue_policy.duplicates_suppressed(),
                    analysis.name
                );
            }
        }

        if in_flight == 0 {
            break;
        }

        let message = result_rx.recv().await.ok_or_else(|| {
            ExploreError::UnexpectedResponse("observer pool stopped before completing jobs".into())
        })?;
        in_flight -= 1;

        match message {
            ObserverMessage::Observed(observation) => {
                if aggregator.discoveries_count() > last_reported_branches {
                    last_reported_branches = aggregator.discoveries_count();
                }
                if let Some(hints) = progress_hints.as_ref() {
                    let since_last = last_summary_time.elapsed();
                    if since_last >= Duration::from_secs(PROGRESS_SUMMARY_INTERVAL_SECS) {
                        let total_branches = hints.total_branches.or(Some(analysis.branches.len()));
                        (hints.callback)(&ExploreProgressSnapshot {
                            function_name: analysis.name.clone(),
                            elapsed: explore_start.elapsed(),
                            iterations: aggregator.iterations(),
                            paths_found: aggregator.unique_paths_count(),
                            total_branches,
                            branches_covered: Some(aggregator.discoveries_count()),
                            mcdc_summary: None,
                            iters_since_new_discovery: aggregator.iters_since_new_discovery(),
                        });
                        last_summary_time = Instant::now();
                    }
                }

                update_live_first_states(&observation.result, &mut live_first_states);
                let discovery_method = observation
                    .strategy_idx
                    .map(|idx| meta_strategy.strategy_kind(idx).explorer_discovery_method())
                    .unwrap_or(
                        SpecialCandidatePath::ExplorerCustomGeneratorFallback
                            .explorer_discovery_method(),
                    );
                let event = crate::observation_aggregator::ObservationEvent {
                    inputs: observation.inputs,
                    mocks: observation.mocks,
                    result: observation.result,
                    discovery_method,
                };
                let outcome = aggregator.aggregate(event.clone());
                meta_strategy.feedback(&event.inputs, &event.result, outcome.is_new_path);
                if let Some(idx) = observation.strategy_idx {
                    meta_strategy.record_outcome(idx, outcome.is_new_path);
                }
            }
            ObserverMessage::Failed(error) => {
                drop(job_tx);
                for handle in handles {
                    let _ = handle.await;
                }
                return Err(error);
            }
        }
    }

    drop(job_tx);
    for handle in handles {
        let _ = handle.await;
    }

    let total_lines = instrumentable_line_count
        .unwrap_or_else(|| analysis.end_line.saturating_sub(analysis.start_line) + 1);
    let opaque_suggestions = crate::executability::build_opaque_suggestions(
        &analysis.params,
        &std::collections::HashMap::new(),
    );
    let stubbed_modules = collect_stubbed_modules(aggregator.raw_results());

    let mut output = aggregator.into_observation_output(
        analysis.name.clone(),
        total_lines,
        timed_out_due_to_budget,
        vec![],
        vec![],
        vec![],
        std::collections::HashMap::new(),
        None,
        crate::shrink::ShrinkStats::default(),
        vec![],
        opaque_suggestions,
        stubbed_modules,
    );
    crate::observe::reconcile_observation_coverage(
        &mut output,
        analysis.start_line,
        analysis.end_line,
        instrumentable_line_count,
    );
    Ok(output)
}

async fn run_observer_worker(
    observer_frontend_config: FrontendConfig,
    analysis: FunctionAnalysis,
    config: ExploreConfig,
    job_rx: std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<ObserverJob>>>,
    result_tx: tokio::sync::mpsc::UnboundedSender<ObserverMessage>,
    options: ObserverWorkerOptions,
) {
    if let Err(error) = run_observer_worker_inner(
        observer_frontend_config,
        analysis,
        config,
        job_rx,
        result_tx.clone(),
        options,
    )
    .await
    {
        let _ = result_tx.send(ObserverMessage::Failed(error));
    }
}

async fn run_observer_worker_inner(
    observer_frontend_config: FrontendConfig,
    analysis: FunctionAnalysis,
    config: ExploreConfig,
    job_rx: std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<ObserverJob>>>,
    result_tx: tokio::sync::mpsc::UnboundedSender<ObserverMessage>,
    options: ObserverWorkerOptions,
) -> Result<(), ExploreError> {
    let mut frontend = Frontend::spawn(&observer_frontend_config).await?;

    let response = frontend
        .send(ProtoCommand::Instrument {
            file: config.file.clone(),
            function: analysis.name.clone(),
            mocks: config.mocks.clone(),
            project_root: config.project_root.clone(),
            execution_profile: config.execution_profile.clone(),
        })
        .instrument(tracing::info_span!("observer.instrument"))
        .await?;
    match response.result {
        ResponseResult::Instrument { instrumented, .. } if instrumented => {}
        ResponseResult::Instrument { .. } => {
            return Err(ExploreError::UnexpectedResponse(
                "observer instrumentation returned instrumented=false".to_string(),
            ));
        }
        other => {
            return Err(ExploreError::UnexpectedResponse(format!(
                "expected observer Instrument response, got {other:?}"
            )));
        }
    }

    let mut setup_context: Option<SetupContextStack> = None;
    if options.per_function_setup
        && !options.skip_setup
        && let Some(ref setup_file) = config.setup_file
    {
        setup_context = send_setup(
            &mut frontend,
            setup_file,
            &analysis.name,
            config.setup_level,
            config.project_root.clone(),
            config.execution_profile.clone(),
        )
        .instrument(tracing::info_span!("observer.setup.function"))
        .await?;
    }

    let prepare_id: Option<String> = if let Some(id) = config.prepare_id_override.clone() {
        Some(id)
    } else if frontend_supports(&config.capabilities, "prepare") {
        match frontend
            .send(ProtoCommand::Prepare {
                file: config.file.clone(),
                function: analysis.name.clone(),
                mocks: config.mocks.clone(),
                project_root: config.project_root.clone(),
                execution_profile: config.execution_profile.clone(),
                plan: config.default_execute_plan.clone(),
            })
            .instrument(tracing::info_span!("observer.prepare"))
            .await
        {
            Ok(resp) => match resp.result {
                ResponseResult::Prepare { prepare_id } => Some(prepare_id),
                other => {
                    log::debug!("observer prepare returned unexpected response: {other:?}");
                    None
                }
            },
            Err(e) => {
                log::debug!("observer prepare failed, falling back to per-execute build: {e}");
                None
            }
        }
    } else {
        None
    };

    loop {
        let job = {
            let mut receiver = job_rx.lock().await;
            receiver.recv().await
        };
        let Some(job) = job else {
            break;
        };

        if options.per_execution_setup
            && !options.skip_setup
            && let Some(ref setup_file) = config.setup_file
        {
            setup_context = send_setup(
                &mut frontend,
                setup_file,
                &analysis.name,
                config.setup_level,
                config.project_root.clone(),
                config.execution_profile.clone(),
            )
            .instrument(tracing::info_span!("observer.setup.execution"))
            .await?;
        }

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: job.inputs.clone(),
                mocks: job.mocks.clone(),
                setup_context: setup_context.clone(),
                capture: config.capture_side_effects,
                prepare_id: prepare_id.clone(),
                execution_profile: config.execution_profile.clone(),
                plan: config.default_execute_plan.clone(),
            })
            .instrument(tracing::info_span!("observer.execute_round_trip"))
            .await?;
        let exec_result = match response.result {
            ResponseResult::Execute(result) => *result,
            ResponseResult::Error { code, message, .. } => {
                if code == crate::protocol::ErrorCode::NotSupported {
                    return Err(ExploreError::Unsupported(message));
                }
                return Err(ExploreError::UnexpectedResponse(format!(
                    "execute error ({code:?}): {message}"
                )));
            }
            other => {
                return Err(ExploreError::UnexpectedResponse(format!(
                    "expected Execute response, got {other:?}"
                )));
            }
        };

        if options.per_execution_setup
            && !options.skip_setup
            && frontend_supports(&config.capabilities, "teardown")
        {
            send_teardown(&mut frontend, &analysis.name, config.setup_level)
                .instrument(tracing::info_span!("observer.teardown.execution"))
                .await?;
            setup_context = None;
        }

        if result_tx
            .send(ObserverMessage::Observed(Box::new(ObserverObservation {
                inputs: job.inputs,
                mocks: job.mocks,
                result: exec_result,
                strategy_idx: job.strategy_idx,
            })))
            .is_err()
        {
            break;
        }
    }

    if options.per_function_setup
        && !options.skip_setup
        && frontend_supports(&config.capabilities, "teardown")
    {
        send_teardown(&mut frontend, &analysis.name, config.setup_level)
            .instrument(tracing::info_span!("observer.teardown.function"))
            .await?;
    }

    frontend.shutdown().await?;
    Ok(())
}

/// Extract deduplicated module names with `StubbedImport` kind from raw results.
pub fn collect_stubbed_modules(
    raw_results: &[(
        Vec<serde_json::Value>,
        Vec<crate::protocol::MockConfig>,
        crate::protocol::ExecuteResult,
    )],
) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for (_, _, result) in raw_results {
        for dep in &result.discovered_dependencies {
            if dep.kind == crate::protocol::DepDetectionKind::StubbedImport {
                seen.insert(dep.source_module.clone());
            }
        }
    }
    seen.into_iter().collect()
}

/// Stats from a genetic algorithm follow-up phase, for report rendering.
#[derive(Debug, Clone)]
pub struct GeneticStats {
    /// Number of unsolved branch targets the GA attempted.
    pub targets_attempted: usize,
    /// Number of those targets the GA solved (newly covered).
    pub targets_solved: usize,
    /// Number of generations completed.
    pub generations_run: u32,
    /// Total individual executions performed.
    pub total_executions: usize,
}

/// Options for formatting an exploration report.
#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub location: Option<String>,
    pub show_perf: bool,
    pub wall_time: Option<std::time::Duration>,
    pub coverage_metrics: Option<crate::coverage_metrics::CoverageMetrics>,
    pub style: crate::report_style::ReportStyle,
    pub genetic_stats: Option<GeneticStats>,
}

/// Render the top banner for an explore session.
pub fn format_explore_header(
    function_count: usize,
    style: &crate::report_style::ReportStyle,
) -> String {
    format!(
        "\n{bold}\u{2550}\u{2550}\u{2550} Shatter Explore \u{2550}\u{2550}\u{2550}{reset}  {dim}{function_count} function(s){reset}\n\n",
        bold = style.bold,
        dim = style.dim,
        reset = style.reset,
    )
}

/// Render the bottom summary banner for an explore session.
pub fn format_explore_footer(
    total_paths: usize,
    function_count: usize,
    total_covered: usize,
    total_lines: u32,
    style: &crate::report_style::ReportStyle,
) -> String {
    let mut out = format!(
        "{bold}\u{2550}\u{2550}\u{2550} Summary: {total_paths} paths across {function_count} function(s)",
        bold = style.bold,
    );
    if total_lines > 0 {
        let pct = (total_covered as f64 / total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(
            " \u{00b7} {covered}/{total} lines ({pct})",
            covered = total_covered,
            total = total_lines,
            pct = style.color_coverage_pct(pct),
        ));
    }
    out.push_str(&format!(
        " \u{2550}\u{2550}\u{2550}{reset}\n",
        reset = style.reset
    ));
    out
}

pub fn format_exploration_report(result: &ObservationOutput, options: &ReportOptions) -> String {
    let s = &options.style;
    let mut out = String::new();

    // Function header with box-drawing line
    let location = options.location.as_deref().unwrap_or("");
    let header_text = if location.is_empty() {
        format!(
            "{bold}{name}{reset}",
            bold = s.bold,
            name = result.function_name,
            reset = s.reset
        )
    } else {
        format!(
            "{bold}{name}{reset} {dim}({location}){reset}",
            bold = s.bold,
            name = result.function_name,
            dim = s.dim,
            reset = s.reset,
        )
    };
    let plain_len = result.function_name.len()
        + if location.is_empty() {
            0
        } else {
            location.len() + 3
        };
    let pad = 50usize.saturating_sub(plain_len);
    out.push_str(&format!(
        "\u{2500}\u{2500} {header_text} {line}\n",
        line = "\u{2500}".repeat(pad),
    ));

    // Summary line: paths + coverage bar
    let mut summary_parts = vec![format!("{} paths", result.unique_paths)];
    if result.total_lines > 0 && result.lines_covered > 0 {
        let pct = (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0);
        summary_parts.push(format!(
            "{}/{} lines ({}) {} {}",
            result.lines_covered,
            result.total_lines,
            s.color_coverage_pct(pct),
            s.coverage_bar(pct),
            s.coverage_indicator(pct),
        ));
    }
    out.push_str(&format!("  {}\n", summary_parts.join(" \u{00b7} ")));

    // Stubbed-import warning
    if !result.stubbed_modules.is_empty() {
        let modules = result.stubbed_modules.join(", ");
        out.push_str(&format!(
            "  {yellow}\u{26a0} Partially analyzed:{reset} stubbed imports for {dim}{modules}{reset}\n",
            yellow = s.yellow,
            dim = s.dim,
            reset = s.reset,
        ));
    }

    // Tree-style path clusters
    if !result.new_path_executions.is_empty() {
        let last_idx = result.new_path_executions.len() - 1;
        for (i, exec) in result.new_path_executions.iter().enumerate() {
            let is_last = i == last_idx;
            let branch = if is_last {
                "\u{2514}\u{2500}"
            } else {
                "\u{251c}\u{2500}"
            };
            let continuation = if is_last { "  " } else { "\u{2502} " };

            let outcome_label = format_outcome_label_styled(exec, s);
            out.push_str(&format!(
                "  {branch} {cyan}{num}{reset}. {outcome}\n",
                cyan = s.cyan,
                num = i + 1,
                reset = s.reset,
                outcome = outcome_label,
            ));
            let inputs_str = exec
                .inputs
                .iter()
                .map(format_value_short)
                .collect::<Vec<_>>()
                .join(", ");
            let outcome_short = format_outcome_short(exec);
            out.push_str(&format!(
                "  {continuation}    {dim}{name}({inputs_str}) {outcome_short}{reset}\n",
                dim = s.dim,
                name = result.function_name,
                reset = s.reset,
            ));
        }
    }

    if let Some(ref metrics) = options.coverage_metrics {
        out.push_str(&crate::coverage_metrics::format_coverage_metrics(
            metrics, s,
        ));
    }
    // Float probe results
    if !result.float_probe_results.is_empty() {
        out.push_str(&format!(
            "  {dim}Float probes:{reset}\n",
            dim = s.dim,
            reset = s.reset
        ));
        let last_idx = result.float_probe_results.len() - 1;
        for (i, probe) in result.float_probe_results.iter().enumerate() {
            let connector = if i == last_idx {
                "\u{2514}\u{2500}"
            } else {
                "\u{251c}\u{2500}"
            };
            let label = match probe.classification {
                crate::float_probe::FloatClassification::IntegerTreating => {
                    format!(
                        "integer-treating ({}/{} agree)",
                        probe.agreements, probe.total_probes
                    )
                }
                crate::float_probe::FloatClassification::FloatSensitive => {
                    let divs: Vec<String> = probe
                        .divergent_values
                        .iter()
                        .map(|v| format!("{v}"))
                        .collect();
                    if divs.is_empty() {
                        "float-sensitive".to_string()
                    } else {
                        format!("float-sensitive \u{2014} diverges at {}", divs.join(", "))
                    }
                }
                crate::float_probe::FloatClassification::Inconclusive => "inconclusive".to_string(),
            };
            out.push_str(&format!(
                "  {connector} {dim}{name}: {label}{reset}\n",
                dim = s.dim,
                name = probe.param_name,
                reset = s.reset,
            ));
        }
    }

    if !result.abandoned_frontiers.is_empty() {
        let ids: Vec<String> = result
            .abandoned_frontiers
            .iter()
            .map(|(id, _)| id.to_string())
            .collect();
        out.push_str(&format!(
            "  {dim}Abandoned frontiers: {} (branch IDs: {}){reset}\n",
            result.abandoned_frontiers.len(),
            ids.join(", "),
            dim = s.dim,
            reset = s.reset,
        ));
    }

    if !result.opaque_suggestions.is_empty() {
        out.push_str(&format!(
            "  {yellow}Suggestions:{reset}\n",
            yellow = s.yellow,
            reset = s.reset,
        ));
        for suggestion in &result.opaque_suggestions {
            use crate::executability::OpaqueSuggestionReason;
            let type_hint = suggestion.type_name.as_deref().unwrap_or("unknown type");
            let detail = match suggestion.reason {
                OpaqueSuggestionReason::UnknownType => format!(
                    "{type_hint} \u{2014} unanalysed type; solver cannot inspect its structure"
                ),
                OpaqueSuggestionReason::FrequentSolveFailure => format!(
                    "{type_hint} \u{2014} appeared in {} unsolvable constraints",
                    suggestion.failed_solve_count
                ),
            };
            out.push_str(&format!(
                "  {yellow}\u{26a0}{reset} Consider marking '{param}' opaque: {detail}\n",
                yellow = s.yellow,
                reset = s.reset,
                param = suggestion.param_name,
            ));
        }
        let config_hint = result
            .opaque_suggestions
            .first()
            .and_then(|s| s.type_name.as_deref())
            .unwrap_or("TypeName");
        out.push_str(&format!(
            "  {dim}  Add to .shatter/config.yaml: opaque_types: [\"{config_hint}\"]{reset}\n",
            dim = s.dim,
            reset = s.reset,
        ));
    }

    if options.show_perf {
        if let Some(dur) = options.wall_time {
            out.push_str(&format!(
                "  {dim}Perf: {:.1}ms, {} iteration(s){reset}\n",
                dur.as_secs_f64() * 1000.0,
                result.iterations,
                dim = s.dim,
                reset = s.reset,
            ));
        } else {
            out.push_str(&format!(
                "  {dim}Perf: {} iteration(s){reset}\n",
                result.iterations,
                dim = s.dim,
                reset = s.reset,
            ));
        }
        let shrink_line = crate::shrink::format_shrink_stats_line(&result.shrink_stats);
        if !shrink_line.is_empty() {
            out.push_str(&shrink_line);
        }
    }
    if let Some(ref ga) = options.genetic_stats {
        out.push_str(&format!(
            "  {dim}GA: {solved}/{attempted} targets solved \u{00b7} {gens} generation(s) \u{00b7} {execs} execution(s){reset}\n",
            solved = ga.targets_solved,
            attempted = ga.targets_attempted,
            gens = ga.generations_run,
            execs = ga.total_executions,
            dim = s.dim,
            reset = s.reset,
        ));
    }
    out
}

pub fn format_exploration_report_verbose(result: &ObservationOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "  Exploration complete: {} iteration(s), {} unique path(s) discovered\n",
        result.iterations, result.unique_paths
    ));
    if result.total_lines > 0 && result.lines_covered > 0 {
        let pct = (result.lines_covered as f64 / result.total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(
            "  Line coverage: {}/{} lines ({pct:.0}%)\n",
            result.lines_covered, result.total_lines
        ));
    }
    out.push_str("\n  Discovered paths:\n");
    for (i, exec) in result.new_path_executions.iter().enumerate() {
        let inputs_str = exec
            .inputs
            .iter()
            .map(format_value_short)
            .collect::<Vec<_>>()
            .join(", ");
        let outcome = if let Some(ref err) = exec.thrown_error {
            format!("THROWS {err}")
        } else {
            match &exec.return_value {
                Some(v) if !v.is_null() => format!("-> {}", format_value_short(v)),
                _ => "-> (void)".to_string(),
            }
        };
        out.push_str(&format!("    {}: ({inputs_str}) {outcome}\n", i + 1));
    }
    out
}

fn format_outcome_label_styled(
    exec: &ExecutionSummary,
    style: &crate::report_style::ReportStyle,
) -> String {
    if let Some(ref err) = exec.thrown_error {
        let intent_suffix = match &exec.error_intent {
            Some(label) if label.label != "unknown" => format!(" [{}]", label.label),
            _ => String::new(),
        };
        format!(
            "{red}throws {err}{intent_suffix}{reset}",
            red = style.red,
            reset = style.reset,
        )
    } else {
        match &exec.return_value {
            Some(v) if !v.is_null() => format!(
                "{green}returns {val}{reset}",
                green = style.green,
                val = format_value_short(v),
                reset = style.reset,
            ),
            _ => format!(
                "{green}returns (void){reset}",
                green = style.green,
                reset = style.reset,
            ),
        }
    }
}

fn format_outcome_short(exec: &ExecutionSummary) -> String {
    if exec.thrown_error.is_some() {
        "\u{2192} Error".to_string()
    } else {
        match &exec.return_value {
            Some(v) if !v.is_null() => format!("\u{2192} {}", format_value_short(v)),
            _ => "\u{2192} (void)".to_string(),
        }
    }
}

fn format_value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s
    }
}

/// Build a [`BranchProfile`] from exploration results.
///
/// Counts how many executions each branch_id appeared in (regardless of
/// taken/not-taken), divides by total execution count to produce frequencies
/// in [0.0, 1.0]. Returns an empty profile when there are no raw results.
pub fn collect_branch_profile(
    observation: &ObservationOutput,
) -> crate::branch_profile::BranchProfile {
    use std::collections::HashSet;

    let total = observation.raw_results.len();
    if total == 0 {
        return crate::branch_profile::BranchProfile::new(HashMap::new());
    }

    let mut counts: HashMap<u32, u32> = HashMap::new();
    for (_inputs, _mocks, result) in &observation.raw_results {
        // Deduplicate branch_ids within a single execution — count presence, not repeats.
        let seen: HashSet<u32> = result.branch_path.iter().map(|bd| bd.branch_id).collect();
        for branch_id in seen {
            *counts.entry(branch_id).or_insert(0) += 1;
        }
    }

    let frequencies: HashMap<u32, f64> = counts
        .into_iter()
        .map(|(id, count)| (id, count as f64 / total as f64))
        .collect();

    crate::branch_profile::BranchProfile::new(frequencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
    use crate::input_gen::ValueSource;
    use crate::orchestrator::FrontendCapabilities;
    use crate::protocol::ExecuteResult;
    use crate::protocol::PerformanceMetrics;
    use crate::protocol::SetupLevel;
    use std::path::PathBuf;

    /// Base command capabilities for test frontends.
    const BASE_CAPABILITIES: &[&str] = &["analyze", "execute", "instrument"];

    /// Build a capability string list from base + additional capabilities.
    fn capabilities_with(extra: &[&str]) -> Vec<String> {
        BASE_CAPABILITIES
            .iter()
            .chain(extra.iter())
            .map(|s| (*s).into())
            .collect()
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
    fn path_hash_distinguishes_different_return_values() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("negative")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("positive-even")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert_ne!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_same_lines_executed_produces_same_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!(3.5)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!(99.0)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert_eq!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_different_lines_executed_produces_different_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1, 2, 4],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert_ne!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_distinguishes_error_from_success() {
        let ok = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let err = ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "Error".into(),
                message: "boom".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert_ne!(
            path_hash(&ok, &no_buckets()),
            path_hash(&err, &no_buckets())
        );
    }

    #[test]
    fn path_hash_uses_branch_path_when_available() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
                conditions: None,
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
                conditions: None,
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            capture_truncation: None,
            performance: empty_perf(),
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert_ne!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    // -- Scope-aware path_hash tests --

    use crate::execution_record::{ScopeEvent, TraceEvent};

    /// Helper: LoopBuckets::none() for backward-compat tests.
    fn no_buckets() -> LoopBuckets {
        LoopBuckets::none()
    }

    /// Helper: branch trace event.
    fn branch_evt(branch_id: u32, taken: bool) -> TraceEvent {
        TraceEvent::Branch {
            decision: BranchDecision {
                branch_id,
                line: 0,
                taken,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            },
        }
    }

    fn loop_enter(id: u32) -> TraceEvent {
        TraceEvent::Scope {
            event: ScopeEvent::LoopEnter { loop_id: id },
        }
    }

    fn loop_exit(id: u32) -> TraceEvent {
        TraceEvent::Scope {
            event: ScopeEvent::LoopExit { loop_id: id },
        }
    }

    fn call_enter(id: u32) -> TraceEvent {
        TraceEvent::Scope {
            event: ScopeEvent::CallEnter { call_site_id: id },
        }
    }

    fn call_exit(id: u32) -> TraceEvent {
        TraceEvent::Scope {
            event: ScopeEvent::CallExit { call_site_id: id },
        }
    }

    /// Helper: make an ExecuteResult with given scope_events.
    fn exec_with_scope(events: Vec<TraceEvent>) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: events,
            loop_body_states: vec![],
            side_effects: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
            performance: empty_perf(),
        }
    }

    #[test]
    fn path_hash_backward_compat_when_scope_events_empty() {
        // When scope_events is empty, should use branch_path (legacy behavior)
        let r = ExecuteResult {
            return_value: Some(serde_json::json!("result")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "test".into(),
                },
                conditions: None,
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
            performance: empty_perf(),
        };
        let hash1 = path_hash(&r, &no_buckets());
        let hash2 = legacy_path_hash(&r);
        assert_eq!(
            hash1, hash2,
            "empty scope_events should use legacy_path_hash"
        );
    }

    #[test]
    fn path_hash_simple_loop_same_branches_same_hash() {
        // 3 iterations of loop 0 with branch 0 always true
        let trace_3 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        // 5 iterations of same
        let trace_5 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &no_buckets()),
            path_hash(&exec_with_scope(trace_5), &no_buckets()),
            "same branch set across different iteration counts should produce same hash"
        );
    }

    #[test]
    fn path_hash_loop_different_branches_different_hash() {
        // Loop where all iterations take branch=true
        let trace_all_true = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        // Loop where one iteration takes branch=false (different branch set)
        let trace_mixed = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, false),
            loop_exit(0),
        ];
        assert_ne!(
            path_hash(&exec_with_scope(trace_all_true), &no_buckets()),
            path_hash(&exec_with_scope(trace_mixed), &no_buckets()),
            "different branch sets within loop should produce different hashes"
        );
    }

    #[test]
    fn path_hash_nested_loops() {
        // Outer loop 0, inner loop 1 — same branch sets across iterations
        let trace_2x2 = vec![
            loop_enter(0),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_exit(0),
            loop_enter(0),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_exit(0),
        ];
        let trace_3x1 = vec![
            loop_enter(0),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_exit(0),
            loop_enter(0),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_exit(0),
            loop_enter(0),
            loop_enter(1),
            branch_evt(1, true),
            loop_exit(1),
            loop_exit(0),
        ];
        assert_eq!(
            path_hash(&exec_with_scope(trace_2x2), &no_buckets()),
            path_hash(&exec_with_scope(trace_3x1), &no_buckets()),
            "nested loops with same branch sets should produce same hash"
        );
    }

    #[test]
    fn path_hash_early_exit_from_loop() {
        // Loop with early exit (no LoopExit marker)
        let trace_early = vec![
            loop_enter(0),
            branch_evt(0, true),
            branch_evt(1, false),
            // no loop_exit — break/return
        ];
        // Same branches, with proper exit
        let trace_normal = vec![
            loop_enter(0),
            branch_evt(0, true),
            branch_evt(1, false),
            loop_exit(0),
        ];
        assert_eq!(
            path_hash(&exec_with_scope(trace_early), &no_buckets()),
            path_hash(&exec_with_scope(trace_normal), &no_buckets()),
            "early exit from loop should produce same hash as normal exit"
        );
    }

    #[test]
    fn path_hash_recursion_collapses() {
        // 3 recursive calls with same branch
        let trace_3 = vec![
            call_enter(0),
            branch_evt(0, true),
            call_exit(0),
            call_enter(0),
            branch_evt(0, true),
            call_exit(0),
            call_enter(0),
            branch_evt(0, true),
            call_exit(0),
        ];
        // 1 recursive call
        let trace_1 = vec![call_enter(0), branch_evt(0, true), call_exit(0)];
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &no_buckets()),
            path_hash(&exec_with_scope(trace_1), &no_buckets()),
            "repeated recursive calls with same branches should collapse"
        );
    }

    #[test]
    fn path_hash_mixed_loops_and_recursion() {
        // Call scope containing a loop
        let trace_a = vec![
            call_enter(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            branch_evt(1, false),
            call_exit(0),
            call_enter(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            branch_evt(1, false),
            call_exit(0),
        ];
        // Same but with different iteration counts
        let trace_b = vec![
            call_enter(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            branch_evt(1, false),
            call_exit(0),
        ];
        assert_eq!(
            path_hash(&exec_with_scope(trace_a), &no_buckets()),
            path_hash(&exec_with_scope(trace_b), &no_buckets()),
            "mixed loops and recursion with same branch sets should collapse"
        );
    }

    #[test]
    fn path_hash_branches_outside_scopes_still_ordered() {
        // Branches outside scopes maintain order
        let trace_a = vec![branch_evt(0, true), branch_evt(1, false)];
        let trace_b = vec![branch_evt(1, false), branch_evt(0, true)];
        assert_ne!(
            path_hash(&exec_with_scope(trace_a), &no_buckets()),
            path_hash(&exec_with_scope(trace_b), &no_buckets()),
            "branches outside scopes should be order-sensitive"
        );
    }

    // -- Loop bucketing tests --

    #[test]
    fn loop_buckets_default_boundaries() {
        let b = LoopBuckets::default();
        assert_eq!(b.bucket(0), 0); // bucket 0: count=0
        assert_eq!(b.bucket(1), 1); // bucket 1: count=1
        assert_eq!(b.bucket(2), 2); // bucket 2: count=2
        assert_eq!(b.bucket(3), 3); // bucket 3: count=3 (3-5 range)
        assert_eq!(b.bucket(4), 3);
        assert_eq!(b.bucket(5), 3);
        assert_eq!(b.bucket(6), 4); // bucket 4: count=6+ (overflow)
        assert_eq!(b.bucket(100), 4);
    }

    #[test]
    fn loop_buckets_none_is_disabled() {
        let b = LoopBuckets::none();
        assert!(b.is_disabled());
    }

    #[test]
    fn path_hash_loop_bucketing_distinguishes_counts() {
        // 1 iteration of loop with branch 0 true
        let trace_1 = vec![loop_enter(0), branch_evt(0, true), loop_exit(0)];
        // 3 iterations of same profile — bucket 3 (3–5) vs bucket 1 (1)
        let trace_3 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        let buckets = LoopBuckets::default();
        assert_ne!(
            path_hash(&exec_with_scope(trace_1), &buckets),
            path_hash(&exec_with_scope(trace_3), &buckets),
            "1 iteration vs 3 iterations should produce different hashes with default buckets"
        );
    }

    #[test]
    fn path_hash_loop_bucketing_collapses_within_bucket() {
        // 3 iterations → bucket 3 (range 3–5)
        let trace_3 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        // 4 iterations → also bucket 3 (range 3–5)
        let trace_4 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        let buckets = LoopBuckets::default();
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &buckets),
            path_hash(&exec_with_scope(trace_4), &buckets),
            "3 and 4 iterations should produce same hash (both in 3-5 bucket)"
        );
    }

    #[test]
    fn path_hash_loop_bucketing_disabled() {
        // With LoopBuckets::none(), different iteration counts should still collapse
        let trace_1 = vec![loop_enter(0), branch_evt(0, true), loop_exit(0)];
        let trace_5 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        let buckets = LoopBuckets::none();
        assert_eq!(
            path_hash(&exec_with_scope(trace_1), &buckets),
            path_hash(&exec_with_scope(trace_5), &buckets),
            "disabled bucketing should collapse all iteration counts"
        );
    }

    #[test]
    fn path_hash_loop_bucketing_custom_boundaries() {
        // Custom boundaries: [1, 10] → 3 buckets: 0-1, 2-10, 11+
        let buckets = LoopBuckets::from_boundaries(vec![1, 10]);
        // 1 iteration → bucket 0 (0-1)
        let trace_1 = vec![loop_enter(0), branch_evt(0, true), loop_exit(0)];
        // 5 iterations → bucket 1 (2-10)
        let trace_5 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        assert_ne!(
            path_hash(&exec_with_scope(trace_1), &buckets),
            path_hash(&exec_with_scope(trace_5), &buckets),
            "custom boundaries should distinguish different buckets"
        );
        // 3 iterations and 8 iterations → both bucket 1 (2-10)
        let trace_3 = vec![
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
            loop_enter(0),
            branch_evt(0, true),
            loop_exit(0),
        ];
        let trace_8: Vec<_> = (0..8)
            .flat_map(|_| vec![loop_enter(0), branch_evt(0, true), loop_exit(0)])
            .collect();
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &buckets),
            path_hash(&exec_with_scope(trace_8), &buckets),
            "3 and 8 iterations should produce same hash with [1,10] boundaries"
        );
    }

    /// Regression: collapse() must emit accumulated scope items before the
    /// early return on a parent scope exit. Without this fix, a loop nested
    /// inside a call scope is invisible to the call's branch profile, so
    /// different iteration counts all hash identically.
    #[test]
    fn path_hash_nested_loop_in_call_scope_bucketing() {
        let buckets = LoopBuckets::default(); // [0, 1, 2, 5]

        // Simulate a for-of loop inside a function call scope.
        // The loop body has no branches, so only iteration count (via
        // bucketing) can distinguish different input lengths.

        // 0 loop iterations (empty string)
        let trace_0 = vec![call_enter(0), call_exit(0)];
        // 1 iteration
        let trace_1 = vec![call_enter(0), loop_enter(0), loop_exit(0), call_exit(0)];
        // 2 iterations
        let trace_2 = vec![
            call_enter(0),
            loop_enter(0),
            loop_exit(0),
            loop_enter(0),
            loop_exit(0),
            call_exit(0),
        ];
        // 5 iterations → bucket 3 (3-5)
        let trace_5: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..5 {
                v.push(loop_enter(0));
                v.push(loop_exit(0));
            }
            v.push(call_exit(0));
            v
        };
        // 10 iterations → bucket 4 (6+)
        let trace_10: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..10 {
                v.push(loop_enter(0));
                v.push(loop_exit(0));
            }
            v.push(call_exit(0));
            v
        };

        let h0 = path_hash(&exec_with_scope(trace_0), &buckets);
        let h1 = path_hash(&exec_with_scope(trace_1), &buckets);
        let h2 = path_hash(&exec_with_scope(trace_2), &buckets);
        let h5 = path_hash(&exec_with_scope(trace_5), &buckets);
        let h10 = path_hash(&exec_with_scope(trace_10), &buckets);

        // All five buckets should produce distinct hashes
        let hashes = vec![h0, h1, h2, h5, h10];
        let unique: std::collections::HashSet<u64> = hashes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            5,
            "loop inside call scope should produce 5 distinct hashes for 5 buckets, got {hashes:?}",
        );

        // 3 and 5 iterations both land in bucket 3 (3-5) → same hash
        let trace_3: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..3 {
                v.push(loop_enter(0));
                v.push(loop_exit(0));
            }
            v.push(call_exit(0));
            v
        };
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &buckets),
            h5,
            "3 and 5 iterations should produce same hash (both in bucket 3-5)"
        );
    }

    #[test]
    fn format_value_short_truncates_long_values() {
        let short = serde_json::json!("hi");
        assert_eq!(format_value_short(&short), "\"hi\"");
        let long = serde_json::json!("a]very long string that exceeds forty characters easily");
        let formatted = format_value_short(&long);
        assert!(formatted.len() <= 43);
        assert!(formatted.ends_with("..."));
    }

    #[test]
    fn format_exploration_report_shows_clustered_paths() {
        let result = ObservationOutput {
            function_name: "classify".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![
                ExecutionSummary {
                    inputs: vec![serde_json::json!(5)],
                    return_value: Some(serde_json::json!("positive-odd")),
                    thrown_error: None,
                    lines_executed: vec![1, 2, 3],
                    is_new_path: true,
                    error_intent: None,
                },
                ExecutionSummary {
                    inputs: vec![serde_json::json!(-3)],
                    return_value: Some(serde_json::json!("negative")),
                    thrown_error: None,
                    lines_executed: vec![1, 4, 5],
                    is_new_path: true,
                    error_intent: None,
                },
            ],
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
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(report.contains("classify"));
        assert!(report.contains("2 paths"));
        assert!(report.contains("50%"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("negative"));
        // Tree-style connectors
        assert!(report.contains("\u{251c}\u{2500}") || report.contains("\u{2514}\u{2500}"));
    }

    #[test]
    fn format_exploration_report_with_location() {
        let result = ObservationOutput {
            function_name: "safeDivide".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(10)],
                return_value: Some(serde_json::json!(5)),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                location: Some("src/math.ts:10-25".into()),
                ..Default::default()
            },
        );
        assert!(report.contains("safeDivide"));
        assert!(report.contains("src/math.ts:10-25"));
    }

    #[test]
    fn format_exploration_report_shows_errors() {
        let result = ObservationOutput {
            function_name: "risky".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 0,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(null)],
                return_value: None,
                thrown_error: Some("TypeError: cannot read null".into()),
                lines_executed: vec![],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(report.contains("throws"));
        assert!(report.contains("TypeError"));
    }

    #[test]
    fn format_exploration_report_with_perf() {
        let result = ObservationOutput {
            function_name: "fast".into(),
            iterations: 10,
            unique_paths: 1,
            lines_covered: 0,
            total_lines: 0,
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
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                show_perf: true,
                wall_time: Some(std::time::Duration::from_millis(42)),
                ..Default::default()
            },
        );
        assert!(report.contains("Perf:"));
        assert!(report.contains("42.0ms"));
        assert!(report.contains("10 iteration(s)"));
    }

    #[test]
    fn format_exploration_report_includes_coverage_metrics() {
        let result = ObservationOutput {
            function_name: "analyze".into(),
            iterations: 20,
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
        };
        let metrics = crate::coverage_metrics::CoverageMetrics {
            total_branches: 4,
            z3_solved: 2,
            random_found: 1,
            user_provided: 0,
            fuzz_found: 0,
            uncovered: 1,
            symexpr_count: 3,
            unknown_count: 1,
            mcdc_metrics: None,
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                coverage_metrics: Some(metrics),
                ..Default::default()
            },
        );
        assert!(report.contains("Branches:"));
        assert!(report.contains("Z3:"));
        assert!(report.contains("uncovered:"));
        assert!(report.contains("Symbolic:"));
    }

    #[test]
    fn format_exploration_report_with_color() {
        let result = ObservationOutput {
            function_name: "colorTest".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 4,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None,
                lines_executed: vec![1, 2, 3, 4],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                style: crate::report_style::ReportStyle::ansi(),
                ..Default::default()
            },
        );
        assert!(
            report.contains("\x1b["),
            "report should contain ANSI codes when style is ansi"
        );
        assert!(report.contains("\x1b[1m"), "function name should be bold");
        assert!(report.contains("\x1b[32m"), "returns should be green");
    }

    #[test]
    fn format_explore_header_and_footer() {
        let style = crate::report_style::ReportStyle::default();
        let header = format_explore_header(4, &style);
        assert!(header.contains("Shatter Explore"));
        assert!(header.contains("4 function(s)"));

        let footer = format_explore_footer(15, 4, 30, 50, &style);
        assert!(footer.contains("15 paths across 4 function(s)"));
        assert!(footer.contains("30/50"));
    }

    #[test]
    fn format_exploration_report_verbose_shows_legacy_format() {
        let result = ObservationOutput {
            function_name: "classify".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(5)],
                return_value: Some(serde_json::json!("positive-odd")),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report_verbose(&result);
        assert!(report.contains("10 iteration(s)"));
        assert!(report.contains("2 unique path(s)"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("Discovered paths:"));
    }

    #[test]
    fn format_report_shows_stubbed_modules_warning() {
        let result = ObservationOutput {
            function_name: "useFake".into(),
            iterations: 3,
            unique_paths: 1,
            lines_covered: 2,
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
            stubbed_modules: vec!["pg".into(), "redis".into()],
            ..Default::default()
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(
            report.contains("Partially analyzed"),
            "report should contain partial analysis warning"
        );
        assert!(
            report.contains("pg"),
            "report should list stubbed module 'pg'"
        );
        assert!(
            report.contains("redis"),
            "report should list stubbed module 'redis'"
        );
    }

    #[test]
    fn format_report_no_warning_without_stubbed_modules() {
        let result = ObservationOutput {
            function_name: "clean".into(),
            iterations: 3,
            unique_paths: 1,
            lines_covered: 2,
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
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(
            !report.contains("Partially analyzed"),
            "clean report should not contain partial analysis warning"
        );
    }

    #[test]
    fn collect_stubbed_modules_deduplicates_and_sorts() {
        use crate::protocol::{
            DepDetectionKind, DiscoveredDependency, ExecuteResult, PerformanceMetrics,
        };

        fn stub_dep(module: &str) -> DiscoveredDependency {
            DiscoveredDependency {
                symbol: String::new(),
                source_module: module.to_string(),
                kind: DepDetectionKind::StubbedImport,
                is_subprocess_spawn: false,
            }
        }

        let result1 = ExecuteResult {
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
            performance: PerformanceMetrics::default(),
            discovered_dependencies: vec![stub_dep("redis"), stub_dep("pg")],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let result2 = ExecuteResult {
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
            performance: PerformanceMetrics::default(),
            discovered_dependencies: vec![stub_dep("pg")], // duplicate
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let raw = vec![(vec![], vec![], result1), (vec![], vec![], result2)];
        let modules = collect_stubbed_modules(&raw);
        assert_eq!(modules, vec!["pg", "redis"]); // sorted, deduplicated
    }

    #[test]
    fn collect_stubbed_modules_ignores_non_stubbed_deps() {
        use crate::protocol::{
            DepDetectionKind, DiscoveredDependency, ExecuteResult, PerformanceMetrics,
        };
        let result = ExecuteResult {
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
            performance: PerformanceMetrics::default(),
            discovered_dependencies: vec![DiscoveredDependency {
                symbol: String::new(),
                source_module: "axios".to_string(),
                kind: DepDetectionKind::UnmockedImport,
                is_subprocess_spawn: false,
            }],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let raw = vec![(vec![], vec![], result)];
        let modules = collect_stubbed_modules(&raw);
        assert!(modules.is_empty());
    }

    #[test]
    fn classify_error_intent_guard_branch_is_validation() {
        use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
        let result = crate::protocol::ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "ValidationError".into(),
                message: "invalid input".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: vec![BranchDecision {
                branch_id: 1,
                line: 5,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            }],
            lines_executed: vec![1, 5, 6],
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
            performance: Default::default(),
        };
        let label = classify_error_intent(&result).unwrap();
        assert_eq!(label.label, "likely_validation");
        assert!(label.confidence > 0.5);
    }

    #[test]
    fn classify_error_intent_no_error_returns_none() {
        let result = crate::protocol::ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![1],
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
            performance: Default::default(),
        };
        assert!(classify_error_intent(&result).is_none());
    }

    #[test]
    fn classify_error_intent_deep_path_is_runtime() {
        use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
        let branches: Vec<BranchDecision> = (0..8)
            .map(|i| BranchDecision {
                branch_id: i,
                line: 10 + i,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: String::new(),
                },
                conditions: None,
            })
            .collect();
        let result = crate::protocol::ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo {
                error_type: "TypeError".into(),
                message: "null deref".into(),
                stack: None,
                error_category: None,
            }),
            branch_path: branches,
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
            performance: Default::default(),
        };
        let label = classify_error_intent(&result).unwrap();
        assert_eq!(label.label, "likely_runtime");
    }

    async fn spawn_noop_frontend() -> Frontend {
        use crate::frontend::FrontendConfig;
        use std::path::{Path, PathBuf};
        use std::time::Duration;
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let noop_path = manifest_dir.join("../protocol/noop-frontend.sh");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![noop_path.to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        Frontend::spawn(&config).await.expect("spawn noop frontend")
    }

    fn recording_frontend_config(log_path: &std::path::Path) -> crate::frontend::FrontendConfig {
        use crate::frontend::FrontendConfig;
        use std::path::{Path, PathBuf};
        use std::time::Duration;

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let frontend_path = manifest_dir.join("../protocol/observer-recording-frontend.sh");
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![frontend_path.to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        config.env_vars.push((
            "SHATTER_OBSERVER_LOG".to_string(),
            log_path.to_string_lossy().into_owned(),
        ));
        config.env_vars.push((
            "SHATTER_OBSERVER_EXEC_SLEEP".to_string(),
            "0.05".to_string(),
        ));
        config
    }

    async fn spawn_recording_frontend(log_path: &std::path::Path) -> Frontend {
        Frontend::spawn(&recording_frontend_config(log_path))
            .await
            .expect("spawn recording frontend")
    }

    fn observer_log_path(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::Builder::new()
            .prefix("shatter-observer-test-")
            .tempdir()
            .expect("create observer log tempdir");
        let path = dir.path().join(format!("{name}.log"));
        (dir, path)
    }

    fn stub_analysis() -> FunctionAnalysis {
        use crate::types::{ParamInfo, TypeInfo};
        FunctionAnalysis {
            name: "stub".into(),
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
        }
    }

    fn fs_dependency_analysis(symbol: &str) -> FunctionAnalysis {
        use crate::protocol::{DependencyKind, ExternalDependency};
        use crate::types::{ParamInfo, TypeInfo};

        FunctionAnalysis {
            name: "load".into(),
            exported: true,
            params: vec![ParamInfo {
                name: "path".into(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: symbol.into(),
                source_module: "os".into(),
                return_type: TypeInfo::Unknown,
                param_types: vec![TypeInfo::Str],
                call_sites: vec![2],
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
        }
    }

    fn path_feedback_frontend_config(
        mode: &str,
        log_path: &std::path::Path,
    ) -> (tempfile::TempDir, crate::frontend::FrontendConfig) {
        use crate::frontend::FrontendConfig;
        use std::path::PathBuf;
        use std::time::Duration;

        let dir = tempfile::Builder::new()
            .prefix("shatter-path-feedback-")
            .tempdir()
            .expect("create path feedback tempdir");
        let script_path = dir.path().join("frontend.py");
        std::fs::write(
            &script_path,
            r#"
import json
import os
import sys

PROTOCOL_VERSION = "0.1.0"
mode = os.environ["SHATTER_PATH_FEEDBACK_MODE"]
log_path = os.environ["SHATTER_PATH_FEEDBACK_LOG"]

def respond(payload):
    print(json.dumps(payload, separators=(",", ":")), flush=True)

def execute_response(request_id, value):
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps(value) + "\n")
    exists = isinstance(value, str) and (
        os.path.isfile(value) if mode == "file" else os.path.isdir(value)
    )
    if exists:
        return {
            "protocol_version": PROTOCOL_VERSION,
            "id": request_id,
            "status": "execute",
            "return_value": "advanced",
            "thrown_error": None,
            "branch_path": [{"branch_id": 1, "line": 3, "taken": True}],
            "lines_executed": [1, 2, 3],
            "calls_to_external": [],
            "path_constraints": [],
            "side_effects": [],
            "performance": {"wall_time_ms": 0.0, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0},
        }
    op = "open" if mode == "file" else "readdirent"
    return {
        "protocol_version": PROTOCOL_VERSION,
        "id": request_id,
        "status": "execute",
        "return_value": None,
        "thrown_error": {
            "error_type": "function_error",
            "message": f"{op} {value}: no such file or directory",
            "stack": None,
        },
        "branch_path": [{"branch_id": 0, "line": 2, "taken": False}],
        "lines_executed": [1, 2],
        "calls_to_external": [],
        "path_constraints": [],
        "side_effects": [],
        "performance": {"wall_time_ms": 0.0, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0},
    }

for line in sys.stdin:
    if not line.strip():
        continue
    request = json.loads(line)
    request_id = request["id"]
    command = request["command"]
    if command == "handshake":
        respond({
            "protocol_version": PROTOCOL_VERSION,
            "id": request_id,
            "status": "handshake",
            "frontend_version": PROTOCOL_VERSION,
            "language": "path-feedback",
            "capabilities": ["execute", "instrument"],
        })
    elif command == "instrument":
        respond({
            "protocol_version": PROTOCOL_VERSION,
            "id": request_id,
            "status": "instrument",
            "instrumented": True,
            "output_file": None,
        })
    elif command == "execute":
        inputs = request.get("inputs") or [""]
        respond(execute_response(request_id, inputs[0]))
    elif command == "shutdown":
        respond({"protocol_version": PROTOCOL_VERSION, "id": request_id, "status": "shutdown_ack"})
        break
    else:
        respond({
            "protocol_version": PROTOCOL_VERSION,
            "id": request_id,
            "status": "error",
            "code": "invalid_request",
            "message": f"Unknown command: {command}",
            "details": None,
        })
"#,
        )
        .expect("write path feedback frontend");

        let mut config = FrontendConfig::new(PathBuf::from("python3"));
        config.args = vec![script_path.to_string_lossy().into_owned()];
        config.request_timeout = Duration::from_secs(5);
        config.env_vars.push((
            "SHATTER_PATH_FEEDBACK_MODE".to_string(),
            mode.to_string(),
        ));
        config.env_vars.push((
            "SHATTER_PATH_FEEDBACK_LOG".to_string(),
            log_path.to_string_lossy().into_owned(),
        ));
        (dir, config)
    }

    fn path_feedback_config(
        target_root: &std::path::Path,
        missing_path: &str,
    ) -> ExploreConfig {
        ExploreConfig {
            file: target_root.join("fixture.go").to_string_lossy().into_owned(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![vec![serde_json::json!(missing_path)]],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: Some(target_root.to_string_lossy().into_owned()),
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        }
    }

    async fn run_path_feedback_fixture(
        mode: &str,
        symbol: &str,
    ) -> (ObservationOutput, tempfile::TempDir, std::path::PathBuf) {
        let target_root = tempfile::Builder::new()
            .prefix("shatter-target-root-")
            .tempdir()
            .expect("create target root");
        let log_path = target_root.path().join(format!("{mode}.log"));
        let (_script_dir, frontend_config) = path_feedback_frontend_config(mode, &log_path);
        let mut frontend = Frontend::spawn(&frontend_config)
            .await
            .expect("spawn path feedback frontend");
        let analysis = fs_dependency_analysis(symbol);
        let config = path_feedback_config(target_root.path(), "missing-input-path");
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("path feedback exploration should succeed");
        frontend.shutdown().await.expect("shutdown failed");
        (result, target_root, log_path)
    }

    #[tokio::test]
    async fn explore_function_seeds_temp_file_after_go_enoent_file_path() {
        let (result, target_root, _log_path) =
            run_path_feedback_fixture("file", "os.ReadFile").await;

        assert_eq!(result.iterations, 2);
        let advanced = result.raw_results.iter().find(|(inputs, _, exec)| {
            inputs.first().and_then(|input| input.as_str()) != Some("missing-input-path")
                && exec.return_value == Some(serde_json::json!("advanced"))
        });
        assert!(
            advanced.is_some(),
            "missing file feedback should schedule a real temp-file input; raw={:?}",
            result.raw_results
        );

        let generated_path = advanced
            .and_then(|(inputs, _, _)| inputs.first())
            .and_then(|input| input.as_str())
            .expect("advanced input should be a path string");
        assert!(
            !generated_path.starts_with(&target_root.path().to_string_lossy().to_string()),
            "path feedback must not create files in the target project: {generated_path}"
        );
    }

    #[tokio::test]
    async fn explore_function_seeds_temp_dir_after_go_enoent_dir_path() {
        let (result, target_root, _log_path) =
            run_path_feedback_fixture("dir", "os.ReadDir").await;

        assert_eq!(result.iterations, 2);
        let advanced = result.raw_results.iter().find(|(inputs, _, exec)| {
            inputs.first().and_then(|input| input.as_str()) != Some("missing-input-path")
                && exec.return_value == Some(serde_json::json!("advanced"))
        });
        assert!(
            advanced.is_some(),
            "missing directory feedback should schedule a real temp-directory input; raw={:?}",
            result.raw_results
        );

        let generated_path = advanced
            .and_then(|(inputs, _, _)| inputs.first())
            .and_then(|input| input.as_str())
            .expect("advanced input should be a path string");
        assert!(
            !generated_path.starts_with(&target_root.path().to_string_lossy().to_string()),
            "path feedback must not create directories in the target project: {generated_path}"
        );
    }

    #[tokio::test]
    async fn explore_function_observer_pool_uses_multiple_frontend_processes() {
        let (_log_dir, log_path) = observer_log_path("pool");

        let observer_frontend_config = recording_frontend_config(&log_path);
        let mut frontend = spawn_recording_frontend(&log_path).await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(6),
            observer_pool: 2,
            observer_frontend_config: Some(observer_frontend_config),
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };

        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("observer-pool exploration should succeed");
        frontend.shutdown().await.expect("shutdown failed");

        assert_eq!(result.iterations, 6);

        let log = std::fs::read_to_string(&log_path).expect("observer log should exist");
        let execute_pids: std::collections::BTreeSet<&str> = log
            .lines()
            .filter_map(|line| line.strip_prefix("execute:"))
            .collect();
        assert!(
            execute_pids.len() >= 2,
            "observer_pool=2 should execute candidates on at least two frontend processes; \
             pids={execute_pids:?}, log={log}"
        );
    }

    #[test]
    fn candidate_queue_capacity_uses_spec_default() {
        assert_eq!(default_candidate_queue_capacity(4, None), 16);
        assert_eq!(default_candidate_queue_capacity(4, Some(3)), 3);
        assert_eq!(default_candidate_queue_capacity(0, None), 4);
        assert_eq!(default_candidate_queue_capacity(2, Some(0)), 1);
    }

    #[test]
    fn custom_generator_prefetch_budget_scales_with_iteration_budget() {
        let generator_file = PathBuf::from("gen.rs");
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "account".to_string(),
                param_name: None,
                generator_file: generator_file.clone(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::BuiltIn,
            ValueSource::CustomGenerator {
                generator_name: "account".to_string(),
                param_name: None,
                generator_file,
                kind: crate::protocol::GeneratorKind::TypeName,
            },
        ];

        assert_eq!(custom_generator_prefetch_budget(&sources, Some(5)), 10);
        assert_eq!(custom_generator_prefetch_budget(&sources, None), 2);
        assert_eq!(custom_generator_prefetch_budget(&[], Some(5)), 1);
    }

    #[test]
    fn custom_generator_values_available_requires_all_custom_slots() {
        let generator_file = PathBuf::from("gen.rs");
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "State".into(),
                param_name: None,
                generator_file: generator_file.clone(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::CustomGenerator {
                generator_name: "current".into(),
                param_name: Some("current".into()),
                generator_file: generator_file.clone(),
                kind: crate::protocol::GeneratorKind::ParamName,
            },
        ];
        let file_key = generator_file.display().to_string();
        let mut prefetched = PrefetchedValues::new();
        prefetched.insert(
            file_key.clone(),
            "State".into(),
            vec![serde_json::json!({"__shatter_native": true})],
        );

        assert!(!custom_generator_values_available(&sources, &prefetched));

        prefetched.insert(
            file_key,
            "current".into(),
            vec![serde_json::json!({"__shatter_native": true})],
        );

        assert!(custom_generator_values_available(&sources, &prefetched));
    }

    #[test]
    fn candidate_queue_policy_honors_explicit_capacity_override() {
        // str-frc.6: explicit override wins over the auto-derived default.
        let policy = CandidateQueuePolicy::with_capacity_override(4, None, Some(7));
        assert_eq!(policy.capacity(), 7);

        // Zero is clamped to one so the bounded channel can be constructed.
        let clamped = CandidateQueuePolicy::with_capacity_override(2, Some(8), Some(0));
        assert_eq!(clamped.capacity(), 1);

        // None preserves the auto-derived capacity (default behavior).
        let auto = CandidateQueuePolicy::with_capacity_override(4, None, None);
        assert_eq!(auto.capacity(), default_candidate_queue_capacity(4, None));
    }

    #[test]
    fn candidate_queue_policy_suppresses_duplicate_fingerprints() {
        let mut policy = CandidateQueuePolicy::new(2, Some(8));
        let inputs = vec![serde_json::json!(7)];
        let mocks = Vec::new();

        assert!(policy.should_enqueue(&inputs, &mocks));
        assert!(!policy.should_enqueue(&inputs, &mocks));
        assert_eq!(policy.duplicates_suppressed(), 1);

        let other = vec![serde_json::json!(8)];
        assert!(policy.should_enqueue(&other, &mocks));
    }

    #[tokio::test]
    async fn explore_function_observer_pool_runs_function_setup_per_observer() {
        let (_log_dir, log_path) = observer_log_path("setup");

        let observer_frontend_config = recording_frontend_config(&log_path);
        let mut frontend = spawn_recording_frontend(&log_path).await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["setup", "teardown"]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(4),
            observer_pool: 2,
            observer_frontend_config: Some(observer_frontend_config),
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: Some("setup.ts".into()),
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };

        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("observer-pool exploration should succeed");
        frontend.shutdown().await.expect("shutdown failed");

        assert_eq!(result.iterations, 4);

        let log = std::fs::read_to_string(&log_path).expect("observer log should exist");
        let setup_pids: std::collections::BTreeSet<&str> = log
            .lines()
            .filter_map(|line| line.strip_prefix("setup:"))
            .collect();
        assert!(
            setup_pids.len() >= 2,
            "function setup should run once on each observer process; \
             pids={setup_pids:?}, log={log}"
        );
    }

    #[tokio::test]
    async fn explore_function_observer_pool_drains_in_flight_on_timeout() {
        let (_log_dir, log_path) = observer_log_path("timeout");

        let observer_frontend_config = recording_frontend_config(&log_path);
        let mut frontend = spawn_recording_frontend(&log_path).await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(10_000),
            observer_pool: 2,
            observer_frontend_config: Some(observer_frontend_config),
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: Some(std::time::Duration::from_millis(1)),
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };

        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("observer-pool timeout should return a partial observation");
        frontend.shutdown().await.expect("shutdown failed");

        assert!(
            result.timed_out,
            "observer-pool timeout should surface timed_out=true"
        );
        assert!(
            result.iterations < 10_000,
            "timeout should stop before max_iterations; iterations={}",
            result.iterations
        );
        assert_eq!(
            result.iterations as usize,
            result.raw_results.len(),
            "drained in-flight executions should be aggregated exactly once"
        );
    }

    #[tokio::test]
    async fn explore_function_observer_pool_suppresses_duplicate_candidates() {
        let (_log_dir, log_path) = observer_log_path("dedup");

        let duplicate = vec![serde_json::json!(7)];
        let observer_frontend_config = recording_frontend_config(&log_path);
        let mut frontend = spawn_recording_frontend(&log_path).await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(8),
            observer_pool: 2,
            observer_frontend_config: Some(observer_frontend_config),
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![
                duplicate.clone(),
                duplicate.clone(),
                duplicate.clone(),
                duplicate.clone(),
            ],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };

        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("observer-pool exploration should succeed");
        frontend.shutdown().await.expect("shutdown failed");

        let duplicate_executions = result
            .raw_results
            .iter()
            .filter(|(inputs, _, _)| *inputs == duplicate)
            .count();
        assert_eq!(
            duplicate_executions,
            1,
            "duplicate candidate fingerprints should execute at most once; raw inputs={:?}",
            result
                .raw_results
                .iter()
                .map(|(inputs, _, _)| inputs)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn explore_function_observer_pool_budget_counts_executed_not_enqueued() {
        let (_log_dir, log_path) = observer_log_path("budget");

        let duplicate = vec![serde_json::json!(11)];
        let unique = vec![serde_json::json!(12)];
        let observer_frontend_config = recording_frontend_config(&log_path);
        let mut frontend = spawn_recording_frontend(&log_path).await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(6),
            observer_pool: 2,
            observer_frontend_config: Some(observer_frontend_config),
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![duplicate.clone(), duplicate, unique.clone()],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };

        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("observer-pool exploration should succeed");
        frontend.shutdown().await.expect("shutdown failed");

        assert_eq!(result.iterations, 6);
        assert!(
            result
                .raw_results
                .iter()
                .any(|(inputs, _, _)| *inputs == unique),
            "suppressed duplicates should not consume the execution budget"
        );
    }

    #[tokio::test]
    async fn explore_function_instruments_before_executing() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(3),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("should succeed with noop frontend");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 3);
        assert_eq!(result.unique_paths, 1);
        // str-gz8j: a normal max-iterations termination must not flag the
        // observation as timed out.
        assert!(
            !result.timed_out,
            "max-iterations completion must leave timed_out=false"
        );
        frontend.shutdown().await.expect("shutdown failed");
    }

    /// str-gz8j: tripping `timeout_explore` in the random explorer path must
    /// surface `timed_out=true` on the returned ObservationOutput. Without
    /// this, the CLI explore command sees only a successful Result and
    /// labels the function as Completed even though it ran out of budget.
    #[tokio::test]
    async fn explore_function_marks_timed_out_when_per_function_timeout_trips() {
        use std::time::Duration;
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(10_000),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            // 1ms budget is below any practical iteration time on the noop
            // frontend, so the loop must exit on the timeout branch.
            timeout_explore: Some(Duration::from_millis(1)),
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("explore_function should still return Ok on per-function timeout");
        assert!(
            result.timed_out,
            "timeout_explore=1ms must surface as ObservationOutput.timed_out=true; \
             got iterations={}, unique_paths={}, timed_out={}",
            result.iterations, result.unique_paths, result.timed_out,
        );
        // Sanity: the budget should fire well before max_iterations.
        assert!(
            result.iterations < 10_000,
            "expected timeout to stop exploration before max_iterations; iterations={}",
            result.iterations,
        );
        frontend.shutdown().await.expect("shutdown failed");
    }

    /// str-cir6: once the per-function deadline has crossed, the follow-up
    /// shrink pass must not keep issuing Execute requests. Zolem's
    /// lookupRequestSchema reproduced this as a stream of quick target errors
    /// after the intended scan budget had already been consumed.
    #[test]
    fn shrink_pass_is_suppressed_after_explore_deadline() {
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(1),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: Some(Duration::from_secs(1)),
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 16,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let started_before_deadline = Instant::now() - Duration::from_secs(2);

        assert!(
            !should_run_shrink_pass(&config, started_before_deadline, false),
            "shrink pass must stop after timeout_explore has elapsed"
        );
        assert!(
            !should_run_shrink_pass(&config, Instant::now(), true),
            "shrink pass must stop when the explore loop recorded a timeout"
        );
        assert!(
            should_run_shrink_pass(&config, Instant::now(), false),
            "shrink pass may run while budget remains"
        );
    }

    #[tokio::test]
    async fn per_function_setup_teardown_lifecycle() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["setup", "teardown"]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: Some("setup.ts".into()),
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("per_function setup should succeed");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn per_execution_setup_teardown_lifecycle() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["setup", "teardown"]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: Some("setup.ts".into()),
            setup_level: SetupLevel::Execution,
            value_sources: vec![],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("per_execution setup should succeed");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 2);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn setup_skipped_when_frontend_lacks_capability() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&[]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: Some("setup.ts".into()),
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("should succeed without setup capability");
        assert_eq!(result.iterations, 2);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn generator_integration_uses_custom_values() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["generate"]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![ValueSource::CustomGenerator {
                generator_name: "x".into(),
                param_name: Some("x".into()),
                generator_file: "gen.ts".into(),
                kind: crate::protocol::GeneratorKind::ParamName,
            }],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("generators should succeed");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn fallback_to_builtin_when_no_generators_configured() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["generate"]));
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(3),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("no generators should succeed");
        assert_eq!(result.iterations, 3);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn user_seeds_consumed_before_literals() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let user_seed_value = vec![serde_json::json!(999)];
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(5),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![user_seed_value.clone()],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("user seeds should succeed");
        assert_eq!(result.iterations, 5);
        // The user-provided seed should appear in raw_results within the budget.
        // MetaStrategy uses adaptive selection so ordering is not guaranteed, but
        // UserProvidedStrategy has exactly 1 input and MUST be drained within budget.
        let found = result
            .raw_results
            .iter()
            .any(|(inputs, _, _)| *inputs == user_seed_value);
        assert!(
            found,
            "user seed {:?} should be executed within the budget; got {:?}",
            user_seed_value,
            result
                .raw_results
                .iter()
                .map(|(i, _, _)| i)
                .collect::<Vec<_>>()
        );
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn candidate_inputs_consumed_between_literals_and_pool() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let candidate_value = vec![serde_json::json!(777)];
        let pool_value = vec![serde_json::json!(888)];
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(10),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(42),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![candidate_value.clone()],
            pool_seeds: vec![pool_value.clone()],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            meta_config: crate::strategy::MetaConfig::default(),
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("candidate inputs should succeed");
        // Both candidate_inputs and pool_seeds should be executed within the budget.
        // MetaStrategy uses adaptive selection so strict ordering is not guaranteed;
        // both UserProvidedStrategy (candidate) and PoolSeedsStrategy (pool) are finite
        // and MUST each be drained within the 10-iteration budget.
        let candidate_found = result
            .raw_results
            .iter()
            .any(|(inputs, _, _)| *inputs == candidate_value);
        let pool_found = result
            .raw_results
            .iter()
            .any(|(inputs, _, _)| *inputs == pool_value);
        assert!(candidate_found, "candidate input should be executed");
        assert!(pool_found, "pool seed should be executed");
        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Random explorer raw_results are harvestable into the interesting pool.
    ///
    /// Verifies that explore_function produces raw_results containing non-boundary
    /// values (from user seeds) that harvest_from_exploration can extract. This
    /// guards against the pre-str-ttu3 gap where only the concolic path harvested.
    #[tokio::test]
    async fn random_explorer_raw_results_are_harvestable() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let non_boundary_seed = vec![serde_json::json!(42)];
        let config = ExploreConfig {
            file: "test.ts".into(),
            max_iterations: Some(2),
            observer_pool: 1,
            observer_frontend_config: None,
            candidate_queue_capacity: None,
            seed: Some(99),
            mocks: vec![],
            mock_params: vec![],
            setup_file: None,
            setup_level: SetupLevel::Function,
            value_sources: vec![],
            capabilities: FrontendCapabilities::default(),
            user_seeds: vec![non_boundary_seed],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            execution_profile: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
            // Non-adaptive (round-robin) to ensure UserProvidedStrategy is drained
            // deterministically within 2 iterations. harvest_from_exploration filters
            // paths with count > DEFAULT_RARITY_THRESHOLD (2), so we must keep
            // iterations ≤ DEFAULT_RARITY_THRESHOLD to avoid filtering all results.
            meta_config: crate::strategy::MetaConfig {
                adaptive: false,
                ..Default::default()
            },
            shrink_budget: 0,
            isolation: IsolationMode::None,
            capture_side_effects: false,
            budget_surplus: None,
            claim_policy: crate::scan_orchestrator::ClaimPolicy::default(),
            planner: None,
            default_execute_plan: None,
            prepare_id_override: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config, None, None)
            .await
            .expect("should succeed with noop frontend");
        assert!(
            !result.raw_results.is_empty(),
            "raw_results should be populated"
        );

        let mut pool = crate::interesting_pool::InterestingPool::default();
        let harvested = crate::interesting_pool::harvest_from_exploration(
            &mut pool,
            &result.raw_results,
            &analysis.params,
            &analysis.name,
            crate::interesting_pool::CoverageMode::Branch,
        );
        assert!(
            harvested > 0,
            "non-boundary user seed should be harvested from random explorer"
        );
        frontend.shutdown().await.expect("shutdown failed");
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn path_hash_is_deterministic(er in arb_execute_result()) {
                let buckets = LoopBuckets::none();
                let h1 = path_hash(&er, &buckets);
                let h2 = path_hash(&er, &buckets);
                prop_assert_eq!(h1, h2, "path_hash must be deterministic");
            }

            #[test]
            fn path_hash_sensitive_to_taken_bit(
                branch_id in 0..50u32,
                line in 1..200u32,
            ) {
                // Flipping the taken bit on a single branch should change the hash.
                let constraint = crate::execution_record::SymConstraint::Unknown {
                    hint: String::new(),
                };
                let perf = crate::protocol::PerformanceMetrics {
                    wall_time_ms: 0.0,
                    cpu_time_us: 0,
                    heap_used_bytes: 0,
                    heap_allocated_bytes: 0,
                };
                let base = crate::protocol::ExecuteResult {
                    return_value: None,
                    thrown_error: None,
                    branch_path: vec![crate::execution_record::BranchDecision {
                        branch_id,
                        line,
                        taken: true,
                        constraint: constraint.clone(),
                        conditions: None,
                    }],
                    lines_executed: vec![],
                    calls_to_external: vec![],
                    path_constraints: vec![],
                    scope_events: vec![],
            loop_body_states: vec![],
                    side_effects: vec![],
                    performance: perf.clone(),
                    capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
                    outcome: None,
                };
                let flipped = crate::protocol::ExecuteResult {
                    branch_path: vec![crate::execution_record::BranchDecision {
                        branch_id,
                        line,
                        taken: false,
                        constraint,
                        conditions: None,
                    }],
                    performance: perf,
                    ..base.clone()
                };
                let buckets = LoopBuckets::none();
                prop_assert_ne!(
                    path_hash(&base, &buckets),
                    path_hash(&flipped, &buckets),
                    "flipping taken bit should change path_hash"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // collect_branch_profile
    // -----------------------------------------------------------------------

    #[test]
    fn collect_branch_profile_empty_results() {
        let obs = ObservationOutput {
            function_name: "test".into(),
            iterations: 0,
            unique_paths: 0,
            lines_covered: 0,
            total_lines: 0,
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
        };
        let profile = collect_branch_profile(&obs);
        assert!(profile.is_empty());
    }

    #[test]
    fn collect_branch_profile_single_execution() {
        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![
                BranchDecision {
                    branch_id: 1,
                    line: 10,
                    taken: true,
                    constraint: SymConstraint::Unknown { hint: "".into() },
                    conditions: None,
                },
                BranchDecision {
                    branch_id: 2,
                    line: 20,
                    taken: false,
                    constraint: SymConstraint::Unknown { hint: "".into() },
                    conditions: None,
                },
            ],
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
        let obs = ObservationOutput {
            function_name: "test".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 2,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![(vec![], vec![], result)],
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
        };
        let profile = collect_branch_profile(&obs);
        assert_eq!(profile.len(), 2);
        // Both branches seen in 1/1 execution → frequency 1.0, rarity 0.0
        assert_eq!(profile.rarity(1), 0.0);
        assert_eq!(profile.rarity(2), 0.0);
        // Unknown branch → rarity 1.0
        assert_eq!(profile.rarity(99), 1.0);
    }

    #[test]
    fn collect_branch_profile_partial_frequency() {
        let make_result = |branch_ids: &[u32]| -> ExecuteResult {
            ExecuteResult {
                return_value: None,
                thrown_error: None,
                branch_path: branch_ids
                    .iter()
                    .map(|&id| BranchDecision {
                        branch_id: id,
                        line: id * 10,
                        taken: true,
                        constraint: SymConstraint::Unknown { hint: "".into() },
                        conditions: None,
                    })
                    .collect(),
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
            }
        };

        let obs = ObservationOutput {
            function_name: "test".into(),
            iterations: 4,
            unique_paths: 2,
            lines_covered: 3,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![
                (vec![], vec![], make_result(&[1, 2])), // exec 1: branches 1,2
                (vec![], vec![], make_result(&[1, 2])), // exec 2: branches 1,2
                (vec![], vec![], make_result(&[1, 3])), // exec 3: branches 1,3
                (vec![], vec![], make_result(&[1])),    // exec 4: branch 1 only
            ],
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
        };
        let profile = collect_branch_profile(&obs);

        // Branch 1: in 4/4 → freq 1.0, rarity 0.0
        assert_eq!(profile.rarity(1), 0.0);
        // Branch 2: in 2/4 → freq 0.5, rarity 0.5
        assert!((profile.rarity(2) - 0.5).abs() < f64::EPSILON);
        // Branch 3: in 1/4 → freq 0.25, rarity 0.75
        assert!((profile.rarity(3) - 0.75).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // LiveFirst integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn update_live_first_states_transitions_on_connection_failure() {
        use crate::execution_record::ExternalCall;
        use crate::protocol::ConnectionFailure;

        let mut states: HashMap<String, LiveFirstState> = HashMap::new();

        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![ExternalCall {
                symbol: "db.query".into(),
                args: vec![],
                return_value: serde_json::json!(null),
            }],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            performance: empty_perf(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![ConnectionFailure {
                symbol: "db.query".into(),
                error_kind: "connection_refused".into(),
                message: "connect ECONNREFUSED 127.0.0.1:5432".into(),
            }],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };

        update_live_first_states(&result, &mut states);

        assert_eq!(
            states.get("db.query"),
            Some(&LiveFirstState::Unavailable),
            "dep with connection failure should transition to Unavailable"
        );
    }

    #[test]
    fn update_live_first_states_transitions_on_success() {
        use crate::execution_record::ExternalCall;

        let mut states: HashMap<String, LiveFirstState> = HashMap::new();

        let result = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![ExternalCall {
                symbol: "api.fetch".into(),
                args: vec![],
                return_value: serde_json::json!({"status": 200}),
            }],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            performance: empty_perf(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };

        update_live_first_states(&result, &mut states);

        assert_eq!(
            states.get("api.fetch"),
            Some(&LiveFirstState::Available),
            "dep with successful call should transition to Available"
        );
    }

    #[test]
    fn update_live_first_states_unavailable_is_terminal() {
        use crate::execution_record::ExternalCall;

        let mut states: HashMap<String, LiveFirstState> = HashMap::new();
        states.insert("db.query".into(), LiveFirstState::Unavailable);

        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![ExternalCall {
                symbol: "db.query".into(),
                args: vec![],
                return_value: serde_json::json!(null),
            }],
            path_constraints: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            side_effects: vec![],
            performance: empty_perf(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };

        update_live_first_states(&result, &mut states);

        assert_eq!(
            states.get("db.query"),
            Some(&LiveFirstState::Unavailable),
            "Unavailable is terminal — success should not revive it"
        );
    }

    #[test]
    fn apply_live_first_overrides_switches_passthrough_to_generated() {
        use crate::protocol::MockBehavior;

        let mut states: HashMap<String, LiveFirstState> = HashMap::new();
        states.insert("db.query".into(), LiveFirstState::Unavailable);
        states.insert("api.fetch".into(), LiveFirstState::Available);

        let mut mocks = vec![
            MockConfig {
                symbol: "db.query".into(),
                return_values: vec![],
                should_track_calls: true,
                default_behavior: MockBehavior::Passthrough,
            },
            MockConfig {
                symbol: "api.fetch".into(),
                return_values: vec![],
                should_track_calls: true,
                default_behavior: MockBehavior::Passthrough,
            },
        ];

        apply_live_first_overrides(&states, &mut mocks);

        assert_eq!(
            mocks[0].default_behavior,
            MockBehavior::ReturnGenerated,
            "Unavailable dep should switch from Passthrough to ReturnGenerated"
        );
        assert_eq!(
            mocks[1].default_behavior,
            MockBehavior::Passthrough,
            "Available dep should remain Passthrough"
        );
    }

    #[test]
    fn apply_live_first_overrides_preserves_non_passthrough() {
        use crate::protocol::MockBehavior;

        let mut states: HashMap<String, LiveFirstState> = HashMap::new();
        states.insert("db.query".into(), LiveFirstState::Unavailable);

        let mut mocks = vec![MockConfig {
            symbol: "db.query".into(),
            return_values: vec![serde_json::json!(42)],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        apply_live_first_overrides(&states, &mut mocks);

        assert_eq!(
            mocks[0].default_behavior,
            MockBehavior::RepeatLast,
            "Non-Passthrough behavior should not be overridden"
        );
    }

    #[test]
    fn format_exploration_report_shows_ga_stats_when_present() {
        let result = ObservationOutput {
            function_name: "target".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None,
                lines_executed: vec![1, 2],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                genetic_stats: Some(GeneticStats {
                    targets_attempted: 5,
                    targets_solved: 3,
                    generations_run: 42,
                    total_executions: 2100,
                }),
                ..Default::default()
            },
        );
        assert!(
            report.contains("GA:"),
            "report should contain GA section header"
        );
        assert!(
            report.contains("3/5 targets solved"),
            "report should show solved/attempted"
        );
        assert!(
            report.contains("42 generation(s)"),
            "report should show generation count"
        );
        assert!(
            report.contains("2100 execution(s)"),
            "report should show execution count"
        );
    }

    #[test]
    fn format_exploration_report_omits_ga_section_when_none() {
        let result = ObservationOutput {
            function_name: "plain".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None,
                lines_executed: vec![1, 2, 3],
                is_new_path: true,
                error_intent: None,
            }],
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
        };
        let report = format_exploration_report(
            &result,
            &ReportOptions {
                genetic_stats: None,
                ..Default::default()
            },
        );
        assert!(
            !report.contains("GA:"),
            "report should not contain GA section when stats are None"
        );
    }

    #[test]
    fn progress_hints_callback_receives_snapshot_with_new_fields() {
        use std::sync::{Arc, Mutex};

        let captured: Arc<Mutex<Vec<ExploreProgressSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        let cb: Box<ProgressCallback> = Box::new(move |snap: &ExploreProgressSnapshot| {
            sink.lock().unwrap().push(snap.clone());
        });
        let hints = ProgressHints {
            callback: cb.as_ref(),
            total_branches: Some(12),
        };

        (hints.callback)(&ExploreProgressSnapshot {
            function_name: "classifyNumber".into(),
            elapsed: Duration::from_secs(15),
            iterations: 847,
            paths_found: 5,
            total_branches: hints.total_branches,
            branches_covered: Some(8),
            mcdc_summary: Some((7, 3, 0)),
            iters_since_new_discovery: 12,
        });

        let snaps = captured.lock().unwrap();
        assert_eq!(snaps.len(), 1);
        let snap = &snaps[0];
        assert_eq!(snap.function_name, "classifyNumber");
        assert_eq!(snap.total_branches, Some(12));
        assert_eq!(snap.branches_covered, Some(8));
        assert_eq!(snap.mcdc_summary, Some((7, 3, 0)));
        assert_eq!(snap.iters_since_new_discovery, 12);
        let covered = snap.branches_covered.unwrap();
        let total = snap.total_branches.unwrap();
        assert!(
            covered <= total,
            "branches_covered ({covered}) must not exceed total_branches ({total})"
        );
    }
}
