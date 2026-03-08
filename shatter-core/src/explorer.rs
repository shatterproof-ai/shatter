//! Exploration engine for discovering execution paths via random input generation.
//!
//! Drives the concolic execution loop: analyze a function's type signature,
//! generate random inputs, execute them via a language frontend, and track
//! unique execution paths. This module implements the random exploration phase
//! (no symbolic solving).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::config::SetupMode;
use crate::coverage_metrics::DiscoveryMethod;
use crate::frontend::{Frontend, FrontendError};
use crate::input_gen::{
    generate_inputs_with_custom, generate_random_inputs, literals_to_candidate_inputs,
    prefetch_custom_values, PrefetchedValues, ValueSource,
};
use crate::orchestrator::FrontendCapabilities;
use crate::protocol::{
    Command as ProtoCommand, ExecuteResult, FunctionAnalysis, MockConfig, ResponseResult,
    SetupContextEntry, SetupContextStack, SetupLevel,
};

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

/// Configuration for an exploration run.
#[derive(Debug, Clone)]
pub struct ExploreConfig {
    /// Path to the source file being explored (needed for instrumentation).
    pub file: String,
    /// Maximum number of iterations (execute calls) per function.
    pub max_iterations: u32,
    /// Random seed for reproducibility. If None, uses entropy.
    pub seed: Option<u64>,
    /// Mock configurations to pass to Execute commands.
    pub mocks: Vec<MockConfig>,
    /// Path to the setup file, if configured.
    pub setup_file: Option<String>,
    /// When to run setup relative to executions.
    pub setup_mode: SetupMode,
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
    /// Iteration count bucket boundaries for loop-aware path hashing.
    pub loop_buckets: LoopBuckets,
    /// Per-function exploration wall-clock timeout. Whichever of this or
    /// `max_iterations` triggers first stops the loop.
    pub timeout_explore: Option<Duration>,
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
#[derive(Debug, Serialize, Deserialize)]
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
    /// Raw execution results paired with their inputs, for building BehaviorMaps.
    pub raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)>,
    /// Per-branch discovery attribution: which branch_id was first found by which method.
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
    /// Fields detected as nondeterministic via within-run re-execution sampling.
    #[serde(default)]
    pub nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
    /// Float probe results classifying Float params as integer-treating or float-sensitive.
    #[serde(default)]
    pub float_probe_results: Vec<crate::float_probe::FloatProbeResult>,
}

/// Transitional alias: existing code that references `ExplorationResult`
/// continues to compile while consumers migrate to `ObservationOutput`.
pub type ExplorationResult = ObservationOutput;

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
        Scope { scope_tag: u64, profile_buckets: Vec<ProfileWithBucket> },
    }

    fn is_matching_exit(event: &TraceEvent, kind: ScopeKind) -> bool {
        match (kind, event) {
            (ScopeKind::Loop(id), TraceEvent::Scope { event: ScopeEvent::LoopExit { loop_id } }) => *loop_id == id,
            (ScopeKind::Call(id), TraceEvent::Scope { event: ScopeEvent::CallExit { call_site_id } }) => *call_site_id == id,
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
                    let bucket = if buckets.is_disabled() { None } else { Some(buckets.bucket(count)) };
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
                    let bucket = if buckets.is_disabled() { None } else { Some(buckets.bucket(count)) };
                    (profile.clone(), bucket)
                })
                .collect();
            sorted.sort();
            scope_ids.push((tag, sorted));
        }
        scope_ids.sort_by_key(|(tag, _)| *tag);
        for (scope_tag, profile_buckets) in scope_ids {
            items.push(CollapsedItem::Scope { scope_tag, profile_buckets });
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
                        ScopeEvent::CallEnter { call_site_id } => (ScopeKind::Call(*call_site_id), true),
                        ScopeEvent::CallExit { call_site_id } => (ScopeKind::Call(*call_site_id), false),
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
                            CollapsedItem::Scope { scope_tag, profile_buckets } => {
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
                            *loop_profiles.entry(id).or_default().entry(profile_vec).or_insert(0) += 1;
                        }
                        ScopeKind::Call(id) => {
                            *call_profiles.entry(id).or_default().entry(profile_vec).or_insert(0) += 1;
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

/// Send a Setup command to the frontend and return a `SetupContextStack`
/// containing the returned context at the given level.
pub(crate) async fn send_setup(
    frontend: &mut Frontend,
    setup_file: &str,
    scope: &str,
    mode: SetupMode,
    project_root: Option<String>,
) -> Result<Option<SetupContextStack>, ExploreError> {
    let level = SetupLevel::from(mode);
    let response = frontend
        .send(ProtoCommand::Setup {
            file: setup_file.to_string(),
            scope: scope.to_string(),
            level,
            project_root,
            parent_context: None,
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
pub(crate) async fn send_teardown(frontend: &mut Frontend, scope: &str, mode: SetupMode) -> Result<(), ExploreError> {
    let level = SetupLevel::from(mode);
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
pub async fn explore_function(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &ExploreConfig,
) -> Result<ObservationOutput, ExploreError> {
    let instrument_response = frontend
        .send(ProtoCommand::Instrument {
            file: config.file.clone(),
            function: analysis.name.clone(),
            mocks: config.mocks.clone(),
            project_root: config.project_root.clone(),
        })
        .await?;

    let instrumentable_line_count = match instrument_response.result {
        ResponseResult::Instrument { instrumented, instrumentable_line_count, .. } => {
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
    let has_setup =
        config.setup_file.is_some() && frontend_supports(&config.capabilities, "setup");
    let per_function_setup = has_setup && config.setup_mode == SetupMode::PerFunction;
    let per_execution_setup = has_setup && config.setup_mode == SetupMode::PerExecution;

    let mut setup_context: Option<SetupContextStack> = None;

    if per_function_setup
        && let Some(ref setup_file) = config.setup_file
    {
        setup_context =
            send_setup(frontend, setup_file, &analysis.name, config.setup_mode, config.project_root.clone()).await?;
    }

    // --- Generator prefetch ---
    let has_generators = config
        .value_sources
        .iter()
        .any(|s| matches!(s, ValueSource::CustomGenerator { .. }));
    let use_generators = has_generators && frontend_supports(&config.capabilities, "generate");

    let mut prefetched = if use_generators {
        prefetch_custom_values(&config.value_sources, frontend, config.max_iterations as usize)
            .await
            .unwrap_or_else(|e| {
                log::debug!("prefetch failed, falling back to built-in: {e}");
                PrefetchedValues::new()
            })
    } else {
        PrefetchedValues::new()
    };

    let mut seen_paths: HashSet<u64> = HashSet::new();
    let mut all_lines: HashSet<u32> = HashSet::new();
    let mut new_path_executions: Vec<ExecutionSummary> = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();
    let mut iterations: u32 = 0;
    let mut path_counts: HashMap<u64, u32> = HashMap::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();

    // --- Float probe phase ---
    // Probes consume from the iteration budget, contributing to seen_paths and raw_results.
    let float_indices = crate::float_probe::float_param_indices(&analysis.params);
    let mut float_probe_results: Vec<crate::float_probe::FloatProbeResult> = Vec::new();
    let probe_budget = float_indices.len() * crate::float_probe::PROBE_COUNT * 2;
    if !float_indices.is_empty() && probe_budget < config.max_iterations as usize {
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
                    })
                    .await?;

                let floor_resp = frontend
                    .send(ProtoCommand::Execute {
                        function: analysis.name.clone(),
                        inputs: floor_inputs,
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
                    })
                    .await?;

                if let (
                    ResponseResult::Execute(float_result),
                    ResponseResult::Execute(floor_result),
                ) = (&float_resp.result, &floor_resp.result) {
                    total_probes += 1;

                    let fhash = path_hash(float_result, &config.loop_buckets);
                    let flhash = path_hash(floor_result, &config.loop_buckets);
                    seen_paths.insert(fhash);
                    seen_paths.insert(flhash);
                    for &line in &float_result.lines_executed {
                        all_lines.insert(line);
                    }
                    for &line in &floor_result.lines_executed {
                        all_lines.insert(line);
                    }

                    if crate::float_probe::executions_agree(float_result, floor_result) {
                        agreements += 1;
                    } else if let Some(v) = float_inputs.get(idx).and_then(|v| v.as_f64()) {
                        divergent_values.push(v);
                    }

                    raw_results.push((float_inputs.clone(), (**float_result).clone()));
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

    let float_bias = crate::float_probe::build_bias_map(&float_probe_results);
    let has_integer_treating = float_bias
        .values()
        .any(|c| *c == crate::float_probe::FloatClassification::IntegerTreating);

    // --- User-provided candidate inputs (highest priority, no budget cap) ---
    let mut user_iter = config.user_seeds.iter().cloned().peekable();

    // --- Literal-derived seed inputs ---
    // Execute extracted literals first to cover magic-value branches before random exploration.
    let literal_candidates = literals_to_candidate_inputs(&analysis.params, &analysis.literals);
    let literal_budget = literal_candidates
        .len()
        .min(config.max_iterations as usize / 2);
    let mut literal_iter = literal_candidates.into_iter().take(literal_budget).peekable();

    // --- Candidate inputs (from --inputs or .shatter/ config) ---
    // Priority above pool seeds, below literal seeds.
    let candidate_budget = config
        .candidate_inputs
        .len()
        .min(config.max_iterations as usize / 3);
    let mut candidate_iter = config.candidate_inputs.iter().take(candidate_budget).cloned().peekable();

    // --- Pool-derived seed inputs ---
    // Cross-function interesting values, injected after literals but before random generation.
    let pool_budget = config
        .pool_seeds
        .len()
        .min(config.max_iterations as usize / 4);
    let mut pool_iter = config.pool_seeds.iter().take(pool_budget).cloned().peekable();

    let explore_start = Instant::now();

    for _ in 0..config.max_iterations {
        if let Some(timeout) = config.timeout_explore
            && explore_start.elapsed() >= timeout
        {
            break;
        }

        iterations += 1;

        // --- Per-execution setup ---
        if per_execution_setup
            && let Some(ref setup_file) = config.setup_file
        {
            setup_context =
                send_setup(frontend, setup_file, &analysis.name, config.setup_mode, config.project_root.clone()).await?;
        }

        // --- Input generation ---
        // Priority: user seeds → literals → candidate inputs → pool seeds → custom generators → random.
        let inputs = if let Some(user_inputs) = user_iter.next() {
            user_inputs
        } else if let Some(lit_inputs) = literal_iter.next() {
            lit_inputs
        } else if let Some(cand_inputs) = candidate_iter.next() {
            cand_inputs
        } else if let Some(pool_inputs) = pool_iter.next() {
            pool_inputs
        } else if use_generators {
            generate_inputs_with_custom(
                &analysis.params,
                &config.value_sources,
                &mut prefetched,
                &mut rng,
                Some(&config.capabilities),
            )
        } else if has_integer_treating {
            crate::input_gen::generate_random_inputs_with_float_bias(
                &analysis.params,
                &float_bias,
                &mut rng,
                None,
            )
        } else {
            generate_random_inputs(&analysis.params, &mut rng, None)
        };

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: inputs.clone(),
                mocks: config.mocks.clone(),
                setup_context: setup_context.clone(),
            })
            .await?;

        let exec_result = match response.result {
            ResponseResult::Execute(result) => *result,
            ResponseResult::Error { code, message, .. } => {
                return Err(ExploreError::UnexpectedResponse(format!(
                    "execute error ({code:?}): {message}"
                )));
            }
            other => {
                return Err(ExploreError::UnexpectedResponse(format!("{other:?}")));
            }
        };

        // --- Per-execution teardown ---
        if per_execution_setup && frontend_supports(&config.capabilities, "teardown") {
            send_teardown(frontend, &analysis.name, config.setup_mode).await?;
        }

        for &line in &exec_result.lines_executed {
            all_lines.insert(line);
        }

        let hash = path_hash(&exec_result, &config.loop_buckets);
        *path_counts.entry(hash).or_insert(0) += 1;
        let is_new = seen_paths.insert(hash);

        // Track per-branch discovery attribution.
        for decision in &exec_result.branch_path {
            if seen_branch_ids.insert(decision.branch_id) {
                discoveries.push((decision.branch_id, DiscoveryMethod::Random));
            }
        }

        if is_new {
            let error_intent = classify_error_intent(&exec_result);
            new_path_executions.push(ExecutionSummary {
                inputs: inputs.clone(),
                return_value: exec_result.return_value.clone(),
                thrown_error: exec_result
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec_result.lines_executed.clone(),
                is_new_path: true,
                error_intent,
            });
        }

        raw_results.push((inputs, exec_result));
    }

    // --- Per-function teardown ---
    if per_function_setup && frontend_supports(&config.capabilities, "teardown") {
        send_teardown(frontend, &analysis.name, config.setup_mode).await?;
    }

    let total_lines = instrumentable_line_count
        .unwrap_or_else(|| analysis.end_line.saturating_sub(analysis.start_line) + 1);

    Ok(ObservationOutput {
        function_name: analysis.name.clone(),
        iterations,
        unique_paths: seen_paths.len(),
        lines_covered: all_lines.len(),
        total_lines,
        new_path_executions,
        raw_results,
        discoveries,
        nondeterministic_fields: vec![],
        float_probe_results,
    })
}

/// Options for formatting an exploration report.
#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub location: Option<String>,
    pub show_perf: bool,
    pub wall_time: Option<std::time::Duration>,
    pub coverage_metrics: Option<crate::coverage_metrics::CoverageMetrics>,
    pub style: crate::report_style::ReportStyle,
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
    out.push_str(&format!(" \u{2550}\u{2550}\u{2550}{reset}\n", reset = style.reset));
    out
}

pub fn format_exploration_report(result: &ObservationOutput, options: &ReportOptions) -> String {
    let s = &options.style;
    let mut out = String::new();

    // Function header with box-drawing line
    let location = options.location.as_deref().unwrap_or("");
    let header_text = if location.is_empty() {
        format!("{bold}{name}{reset}", bold = s.bold, name = result.function_name, reset = s.reset)
    } else {
        format!(
            "{bold}{name}{reset} {dim}({location}){reset}",
            bold = s.bold, name = result.function_name, dim = s.dim, reset = s.reset,
        )
    };
    let plain_len = result.function_name.len() + if location.is_empty() { 0 } else { location.len() + 3 };
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

    // Tree-style path clusters
    if !result.new_path_executions.is_empty() {
        let last_idx = result.new_path_executions.len() - 1;
        for (i, exec) in result.new_path_executions.iter().enumerate() {
            let is_last = i == last_idx;
            let branch = if is_last { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" };
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
        out.push_str(&crate::coverage_metrics::format_coverage_metrics(metrics, s));
    }
    // Float probe results
    if !result.float_probe_results.is_empty() {
        out.push_str(&format!("  {dim}Float probes:{reset}\n", dim = s.dim, reset = s.reset));
        let last_idx = result.float_probe_results.len() - 1;
        for (i, probe) in result.float_probe_results.iter().enumerate() {
            let connector = if i == last_idx { "\u{2514}\u{2500}" } else { "\u{251c}\u{2500}" };
            let label = match probe.classification {
                crate::float_probe::FloatClassification::IntegerTreating => {
                    format!("integer-treating ({}/{} agree)", probe.agreements, probe.total_probes)
                }
                crate::float_probe::FloatClassification::FloatSensitive => {
                    let divs: Vec<String> = probe.divergent_values.iter().map(|v| format!("{v}")).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SetupMode;
    use crate::execution_record::{BranchDecision, ErrorInfo, SymConstraint};
    use crate::input_gen::ValueSource;
    use crate::orchestrator::FrontendCapabilities;
    use crate::protocol::ExecuteResult;
    use crate::protocol::PerformanceMetrics;

    /// Base command capabilities for test frontends.
    const BASE_CAPABILITIES: &[&str] = &["analyze", "execute", "instrument"];

    /// Build a capability string list from base + additional capabilities.
    fn capabilities_with(extra: &[&str]) -> Vec<String> {
        BASE_CAPABILITIES.iter().chain(extra.iter()).map(|s| (*s).into()).collect()
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
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("positive-even")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_same_lines_executed_produces_same_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!(3.5)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!(99.0)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        assert_eq!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_different_lines_executed_produces_different_hash() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 3],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None, branch_path: vec![], lines_executed: vec![1, 2, 4],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        assert_ne!(path_hash(&r1, &no_buckets()), path_hash(&r2, &no_buckets()));
    }

    #[test]
    fn path_hash_distinguishes_error_from_success() {
        let ok = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None, branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        let err = ExecuteResult {
            return_value: None,
            thrown_error: Some(ErrorInfo { error_type: "Error".into(), message: "boom".into(), stack: None, error_category: None }),
            branch_path: vec![], lines_executed: vec![],
            calls_to_external: vec![], path_constraints: vec![], side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, performance: empty_perf(),
        };
        assert_ne!(path_hash(&ok, &no_buckets()), path_hash(&err, &no_buckets()));
    }

    #[test]
    fn path_hash_uses_branch_path_when_available() {
        let r1 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0, line: 10, taken: true,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            }],
            lines_executed: vec![], calls_to_external: vec![], path_constraints: vec![],
            scope_events: vec![], side_effects: vec![], capture_truncation: None, performance: empty_perf(),
        };
        let r2 = ExecuteResult {
            return_value: Some(serde_json::json!("same")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0, line: 10, taken: false,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            }],
            lines_executed: vec![], calls_to_external: vec![], path_constraints: vec![],
            scope_events: vec![], side_effects: vec![], capture_truncation: None, performance: empty_perf(),
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
                constraint: SymConstraint::Unknown { hint: String::new() },
            },
        }
    }

    fn loop_enter(id: u32) -> TraceEvent {
        TraceEvent::Scope { event: ScopeEvent::LoopEnter { loop_id: id } }
    }

    fn loop_exit(id: u32) -> TraceEvent {
        TraceEvent::Scope { event: ScopeEvent::LoopExit { loop_id: id } }
    }

    fn call_enter(id: u32) -> TraceEvent {
        TraceEvent::Scope { event: ScopeEvent::CallEnter { call_site_id: id } }
    }

    fn call_exit(id: u32) -> TraceEvent {
        TraceEvent::Scope { event: ScopeEvent::CallExit { call_site_id: id } }
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
            side_effects: vec![],
            capture_truncation: None,
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
                branch_id: 0, line: 10, taken: true,
                constraint: SymConstraint::Unknown { hint: "test".into() },
            }],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            scope_events: vec![],
            side_effects: vec![],
            capture_truncation: None,
            performance: empty_perf(),
        };
        let hash1 = path_hash(&r, &no_buckets());
        let hash2 = legacy_path_hash(&r);
        assert_eq!(hash1, hash2, "empty scope_events should use legacy_path_hash");
    }

    #[test]
    fn path_hash_simple_loop_same_branches_same_hash() {
        // 3 iterations of loop 0 with branch 0 always true
        let trace_3 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        // 5 iterations of same
        let trace_5 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
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
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        // Loop where one iteration takes branch=false (different branch set)
        let trace_mixed = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, false), loop_exit(0),
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
                loop_enter(1), branch_evt(1, true), loop_exit(1),
                loop_enter(1), branch_evt(1, true), loop_exit(1),
            loop_exit(0),
            loop_enter(0),
                loop_enter(1), branch_evt(1, true), loop_exit(1),
            loop_exit(0),
        ];
        let trace_3x1 = vec![
            loop_enter(0),
                loop_enter(1), branch_evt(1, true), loop_exit(1),
            loop_exit(0),
            loop_enter(0),
                loop_enter(1), branch_evt(1, true), loop_exit(1),
            loop_exit(0),
            loop_enter(0),
                loop_enter(1), branch_evt(1, true), loop_exit(1),
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
            loop_enter(0), branch_evt(0, true), branch_evt(1, false),
            // no loop_exit — break/return
        ];
        // Same branches, with proper exit
        let trace_normal = vec![
            loop_enter(0), branch_evt(0, true), branch_evt(1, false), loop_exit(0),
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
            call_enter(0), branch_evt(0, true), call_exit(0),
            call_enter(0), branch_evt(0, true), call_exit(0),
            call_enter(0), branch_evt(0, true), call_exit(0),
        ];
        // 1 recursive call
        let trace_1 = vec![
            call_enter(0), branch_evt(0, true), call_exit(0),
        ];
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
                loop_enter(0), branch_evt(0, true), loop_exit(0),
                loop_enter(0), branch_evt(0, true), loop_exit(0),
                branch_evt(1, false),
            call_exit(0),
            call_enter(0),
                loop_enter(0), branch_evt(0, true), loop_exit(0),
                branch_evt(1, false),
            call_exit(0),
        ];
        // Same but with different iteration counts
        let trace_b = vec![
            call_enter(0),
                loop_enter(0), branch_evt(0, true), loop_exit(0),
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
        let trace_1 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        // 3 iterations of same profile — bucket 3 (3–5) vs bucket 1 (1)
        let trace_3 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
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
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        // 4 iterations → also bucket 3 (range 3–5)
        let trace_4 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
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
        let trace_1 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        let trace_5 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
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
        let trace_1 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        // 5 iterations → bucket 1 (2-10)
        let trace_5 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        assert_ne!(
            path_hash(&exec_with_scope(trace_1), &buckets),
            path_hash(&exec_with_scope(trace_5), &buckets),
            "custom boundaries should distinguish different buckets"
        );
        // 3 iterations and 8 iterations → both bucket 1 (2-10)
        let trace_3 = vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ];
        let trace_8: Vec<_> = (0..8).flat_map(|_| vec![
            loop_enter(0), branch_evt(0, true), loop_exit(0),
        ]).collect();
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
        let trace_1 = vec![
            call_enter(0), loop_enter(0), loop_exit(0), call_exit(0),
        ];
        // 2 iterations
        let trace_2 = vec![
            call_enter(0),
            loop_enter(0), loop_exit(0),
            loop_enter(0), loop_exit(0),
            call_exit(0),
        ];
        // 5 iterations → bucket 3 (3-5)
        let trace_5: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..5 { v.push(loop_enter(0)); v.push(loop_exit(0)); }
            v.push(call_exit(0));
            v
        };
        // 10 iterations → bucket 4 (6+)
        let trace_10: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..10 { v.push(loop_enter(0)); v.push(loop_exit(0)); }
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
            unique.len(), 5,
            "loop inside call scope should produce 5 distinct hashes for 5 buckets, got {hashes:?}",
        );

        // 3 and 5 iterations both land in bucket 3 (3-5) → same hash
        let trace_3: Vec<_> = {
            let mut v = vec![call_enter(0)];
            for _ in 0..3 { v.push(loop_enter(0)); v.push(loop_exit(0)); }
            v.push(call_exit(0));
            v
        };
        assert_eq!(
            path_hash(&exec_with_scope(trace_3), &buckets), h5,
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
            function_name: "classify".into(), iterations: 10, unique_paths: 2,
            lines_covered: 5, total_lines: 10,
            new_path_executions: vec![
                ExecutionSummary {
                    inputs: vec![serde_json::json!(5)],
                    return_value: Some(serde_json::json!("positive-odd")),
                    thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true, error_intent: None },
                ExecutionSummary {
                    inputs: vec![serde_json::json!(-3)],
                    return_value: Some(serde_json::json!("negative")),
                    thrown_error: None, lines_executed: vec![1, 4, 5], is_new_path: true, error_intent: None },
            ],
            raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
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
            function_name: "safeDivide".into(), iterations: 5, unique_paths: 1,
            lines_covered: 3, total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(10)],
                return_value: Some(serde_json::json!(5)),
                thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true, error_intent: None }],
            raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions {
            location: Some("src/math.ts:10-25".into()), ..Default::default()
        });
        assert!(report.contains("safeDivide"));
        assert!(report.contains("src/math.ts:10-25"));
    }

    #[test]
    fn format_exploration_report_shows_errors() {
        let result = ObservationOutput {
            function_name: "risky".into(), iterations: 5, unique_paths: 1,
            lines_covered: 0, total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(null)],
                return_value: None,
                thrown_error: Some("TypeError: cannot read null".into()),
                lines_executed: vec![], is_new_path: true, error_intent: None }],
            raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions::default());
        assert!(report.contains("throws"));
        assert!(report.contains("TypeError"));
    }

    #[test]
    fn format_exploration_report_with_perf() {
        let result = ObservationOutput {
            function_name: "fast".into(), iterations: 10, unique_paths: 1,
            lines_covered: 0, total_lines: 0, new_path_executions: vec![], raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions {
            show_perf: true, wall_time: Some(std::time::Duration::from_millis(42)),
            ..Default::default()
        });
        assert!(report.contains("Perf:"));
        assert!(report.contains("42.0ms"));
        assert!(report.contains("10 iteration(s)"));
    }

    #[test]
    fn format_exploration_report_includes_coverage_metrics() {
        let result = ObservationOutput {
            function_name: "analyze".into(), iterations: 20, unique_paths: 3,
            lines_covered: 8, total_lines: 10, new_path_executions: vec![], raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let metrics = crate::coverage_metrics::CoverageMetrics {
            total_branches: 4, z3_solved: 2, random_found: 1, user_provided: 0,
            uncovered: 1, symexpr_count: 3, unknown_count: 1,
        };
        let report = format_exploration_report(&result, &ReportOptions {
            coverage_metrics: Some(metrics), ..Default::default()
        });
        assert!(report.contains("Branches:"));
        assert!(report.contains("Z3:"));
        assert!(report.contains("uncovered:"));
        assert!(report.contains("Symbolic:"));
    }

    #[test]
    fn format_exploration_report_with_color() {
        let result = ObservationOutput {
            function_name: "colorTest".into(), iterations: 5, unique_paths: 1,
            lines_covered: 4, total_lines: 5,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(1)],
                return_value: Some(serde_json::json!("ok")),
                thrown_error: None, lines_executed: vec![1, 2, 3, 4], is_new_path: true, error_intent: None }],
            raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let report = format_exploration_report(&result, &ReportOptions {
            style: crate::report_style::ReportStyle::ansi(), ..Default::default()
        });
        assert!(report.contains("\x1b["), "report should contain ANSI codes when style is ansi");
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
            function_name: "classify".into(), iterations: 10, unique_paths: 2,
            lines_covered: 5, total_lines: 10,
            new_path_executions: vec![ExecutionSummary {
                inputs: vec![serde_json::json!(5)],
                return_value: Some(serde_json::json!("positive-odd")),
                thrown_error: None, lines_executed: vec![1, 2, 3], is_new_path: true, error_intent: None }],
            raw_results: vec![], discoveries: vec![], nondeterministic_fields: vec![], float_probe_results: vec![],
        };
        let report = format_exploration_report_verbose(&result);
        assert!(report.contains("10 iteration(s)"));
        assert!(report.contains("2 unique path(s)"));
        assert!(report.contains("positive-odd"));
        assert!(report.contains("Discovered paths:"));
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
                constraint: SymConstraint::Unknown { hint: String::new() },
            }],
            lines_executed: vec![1, 5, 6],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None,
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
            capture_truncation: None,
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
                constraint: SymConstraint::Unknown { hint: String::new() },
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
            capture_truncation: None,
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

    fn stub_analysis() -> FunctionAnalysis {
        use crate::types::{ParamInfo, TypeInfo};
        FunctionAnalysis {
            name: "stub".into(), exported: true,
            params: vec![ParamInfo { name: "x".into(), typ: TypeInfo::Int, type_name: None }],
            branches: vec![], dependencies: vec![],
            return_type: TypeInfo::Unknown, start_line: 1, end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
        }
    }

    #[tokio::test]
    async fn explore_function_instruments_before_executing() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 3, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("should succeed with noop frontend");
        assert_eq!(result.function_name, "stub");
        assert_eq!(result.iterations, 3);
        assert_eq!(result.unique_paths, 1);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn per_function_setup_teardown_lifecycle() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["setup", "teardown"]));
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("per_function setup should succeed");
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
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerExecution,
            value_sources: vec![], capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("per_execution setup should succeed");
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
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: Some("setup.ts".into()), setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("should succeed without setup capability");
        assert_eq!(result.iterations, 2);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn generator_integration_uses_custom_values() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let caps = FrontendCapabilities::from_raw(&capabilities_with(&["generate"]));
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 2, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![ValueSource::CustomGenerator {
                generator_name: "x".into(), param_name: Some("x".into()),
                generator_file: "gen.ts".into(),
                kind: crate::protocol::GeneratorKind::ParamName,
            }],
            capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("generators should succeed");
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
            file: "test.ts".into(), max_iterations: 3, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: caps,
            user_seeds: vec![],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("no generators should succeed");
        assert_eq!(result.iterations, 3);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn user_seeds_consumed_before_literals() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let user_seed_value = vec![serde_json::json!(999)];
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 5, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: FrontendCapabilities::default(),
            user_seeds: vec![user_seed_value.clone()],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("user seeds should succeed");
        assert_eq!(result.iterations, 5);
        // The first execution should use the user-provided seed value.
        assert_eq!(result.raw_results[0].0, user_seed_value);
        frontend.shutdown().await.expect("shutdown failed");
    }

    #[tokio::test]
    async fn candidate_inputs_consumed_between_literals_and_pool() {
        let mut frontend = spawn_noop_frontend().await;
        let analysis = stub_analysis();
        let candidate_value = vec![serde_json::json!(777)];
        let pool_value = vec![serde_json::json!(888)];
        let config = ExploreConfig {
            file: "test.ts".into(), max_iterations: 10, seed: Some(42), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: FrontendCapabilities::default(),
            user_seeds: vec![],
            candidate_inputs: vec![candidate_value.clone()],
            pool_seeds: vec![pool_value.clone()],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("candidate inputs should succeed");
        // Literal-derived inputs come first (from stub_analysis literals),
        // then candidate_inputs, then pool_seeds.
        // Find the candidate value in raw_results — it should appear before pool value.
        let candidate_pos = result.raw_results.iter().position(|(inputs, _)| *inputs == candidate_value);
        let pool_pos = result.raw_results.iter().position(|(inputs, _)| *inputs == pool_value);
        assert!(candidate_pos.is_some(), "candidate input should be executed");
        assert!(pool_pos.is_some(), "pool seed should be executed");
        assert!(
            candidate_pos.unwrap() < pool_pos.unwrap(),
            "candidate inputs should be consumed before pool seeds"
        );
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
            file: "test.ts".into(), max_iterations: 1, seed: Some(99), mocks: vec![],
            setup_file: None, setup_mode: SetupMode::PerFunction,
            value_sources: vec![], capabilities: FrontendCapabilities::default(),
            user_seeds: vec![non_boundary_seed],
            candidate_inputs: vec![],
            pool_seeds: vec![],
            project_root: None,
            loop_buckets: LoopBuckets::default(),
            timeout_explore: None,
        };
        let result = explore_function(&mut frontend, &analysis, &config)
            .await.expect("should succeed with noop frontend");
        assert!(!result.raw_results.is_empty(), "raw_results should be populated");

        let mut pool = crate::interesting_pool::InterestingPool::default();
        let harvested = crate::interesting_pool::harvest_from_exploration(
            &mut pool,
            &result.raw_results,
            &analysis.params,
            &analysis.name,
        );
        assert!(harvested > 0, "non-boundary user seed should be harvested from random explorer");
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
                    }],
                    lines_executed: vec![],
                    calls_to_external: vec![],
                    path_constraints: vec![],
                    scope_events: vec![],
                    side_effects: vec![],
                    performance: perf.clone(),
                    capture_truncation: None,
                };
                let flipped = crate::protocol::ExecuteResult {
                    branch_path: vec![crate::execution_record::BranchDecision {
                        branch_id,
                        line,
                        taken: false,
                        constraint,
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
}
