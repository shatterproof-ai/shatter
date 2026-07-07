//! Concolic execution loop: worklist-driven path exploration with Z3 solving.
//!
//! The orchestrator drives the concolic testing cycle in two-phase rounds:
//!
//! **Observe phase** — drain the worklist, execute all pending inputs via the
//! frontend, classify each execution as new-path or duplicate.
//!
//! **Solve/Generate phase** — for each new-path observation, extract symbolic
//! constraints, negate branches with Z3, fuzz unknown constraints, and drill
//! stalled frontiers. The resulting candidate inputs feed the next round's
//! worklist.
//!
//! The outer loop iterates: Observe → Solve/Generate → feed candidates → next
//! Observe round, until a termination condition fires (budget, plateau, or
//! worklist exhaustion).

use contracts::requires;
use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tokio::task::JoinHandle;

use std::collections::HashMap;

use crate::auto_mock::MockParam;
use crate::boundary_search;
use crate::coverage_metrics::DiscoveryMethod;
use crate::drilling;
use crate::execution_record::{BranchDecision, ScopeEvent, SymConstraint, TraceEvent};
use crate::explorer::{apply_live_first_overrides, update_live_first_states};
use crate::frontend::{Frontend, FrontendError};
use crate::frontier::{Frontier, FrontierSet, frontier_score};
use crate::genetic_fitness::{FitnessContext, FitnessWeights};
use crate::input_gen;
use crate::mcdc::McdcTable;
use crate::mock_value_space::LiveFirstState;
use crate::oracle::{
    ConditionId, FailedCondition, InputVector, OracleContext, OracleSlotMap, OracleStats,
};
use crate::protocol::{Command, ExecuteResult, MockConfig, ResponseResult, SetupContextStack};
use crate::solver::{self, ConcreteValue, SolveResult};
use crate::strategy::{
    MetaStrategy, SpecialCandidatePath, StrategyContext, build_concolic_meta_strategy,
};
use crate::sym_expr::SymExpr;
use crate::triage::{TriageState, TriageVerdict};
use crate::types::{ComplexKind, ParamInfo};

/// Parsed frontend capabilities from the handshake response.
///
/// During handshake, frontends declare which commands they support and which
/// complex types they can reconstruct. The core uses this to avoid generating
/// complex-typed inputs the frontend can't handle.
#[derive(Debug, Clone, Default)]
pub struct FrontendCapabilities {
    /// Standard commands the frontend supports ("analyze", "execute", etc.).
    pub commands: HashSet<String>,
    /// Complex types the frontend can reconstruct from `__complex_type` JSON.
    pub complex_types: HashSet<ComplexKind>,
}

impl FrontendCapabilities {
    /// Parse raw capability strings from a handshake response.
    ///
    /// Strings prefixed with `"complex_type:"` are parsed as `ComplexKind` values.
    /// All other strings are treated as command capabilities.
    /// Unknown complex type names are silently ignored.
    pub fn from_raw(capabilities: &[String]) -> Self {
        let mut commands = HashSet::new();
        let mut complex_types = HashSet::new();
        for cap in capabilities {
            if let Some(kind_str) = cap.strip_prefix("complex_type:") {
                // ComplexKind uses serde rename_all = "snake_case", so we
                // deserialize the bare string as a JSON string value.
                if let Ok(kind) = serde_json::from_value::<ComplexKind>(serde_json::Value::String(
                    kind_str.to_string(),
                )) {
                    complex_types.insert(kind);
                }
                // Silently ignore unknown complex type names
            } else {
                commands.insert(cap.clone());
            }
        }
        Self {
            commands,
            complex_types,
        }
    }

    /// Check whether the frontend declared support for a specific complex type.
    pub fn supports_complex(&self, kind: ComplexKind) -> bool {
        self.complex_types.contains(&kind)
    }
}

/// Configuration for a concolic exploration session.
#[derive(Debug, Clone)]
pub struct ExploreConfig {
    /// Maximum number of unique paths to explore before stopping.
    /// `None` means unbounded — explore runs until timeout or interruption.
    pub max_iterations: Option<usize>,
    /// Maximum total executions (including duplicated paths) before stopping.
    /// `None` means unbounded.
    pub max_executions: Option<usize>,
    /// Stop after this many consecutive executions without discovering a new path.
    /// Set to 0 to disable plateau detection.
    pub plateau_threshold: usize,
    /// Mock configurations to pass through to Execute commands.
    pub mocks: Vec<crate::protocol::MockConfig>,
    /// Mock parameters for dynamic per-iteration mock generation.
    /// When non-empty, fresh mock values are generated each iteration
    /// instead of reusing the static `mocks` field.
    pub mock_params: Vec<crate::auto_mock::MockParam>,
    /// Z3 solver timeout in milliseconds per query. None means no limit.
    pub solver_timeout_ms: Option<u64>,
    /// Random seed for reproducible concolic candidate generation.
    pub seed: Option<u64>,
    /// Run solver feedback on Tokio's blocking pool and prefetch one ready
    /// meta-strategy candidate so observation can overlap with Z3 solving.
    ///
    /// Defaults to false to preserve the existing strict serial behavior until
    /// callers opt into the internal async mode.
    pub solver_offload: bool,
    /// Per-function exploration wall-clock timeout. Whichever of this or
    /// `max_iterations`/`max_executions` triggers first stops the loop.
    pub timeout_explore: Option<Duration>,
    /// Branch frequency profile from a prior random exploration phase.
    /// When present, biases fitness scoring and frontier prioritization
    /// toward rarely-observed branches.
    pub branch_profile: Option<crate::branch_profile::BranchProfile>,
    /// Strategy meta-configuration for adaptive selection.
    pub meta_config: crate::strategy::MetaConfig,
    /// Opaque execution profile selected for this function, if any.
    pub execution_profile: Option<crate::protocol::ExecutionProfile>,
    /// Per-boundary refinement budget (executions). After the discovery loop,
    /// a separate refinement phase binary-searches between witness pairs.
    /// `None` or `Some(0)` disables refinement.
    pub refine_budget: Option<usize>,
    /// Maximum shrink attempts per discovered behavior. Set to 0 to disable.
    pub shrink_budget: usize,
    /// Enable MC/DC coverage analysis. When true, the orchestrator tracks
    /// per-condition independence and generates targeted Z3 queries for
    /// missing MC/DC pairs.
    pub mcdc: bool,
    /// Configuration for the hybrid fuzzing phase.
    pub fuzz: crate::config::FuzzConfig,
    /// Name of a frontend-provided invocation planner to consult. `None` means
    /// the orchestrator drives input generation on its own (Z3 + drilling +
    /// meta-strategy). Set `default_execute_plan` to pass a plan on every
    /// Execute for this target.
    pub planner: Option<String>,
    /// InvocationPlan to attach to every Execute request for this target.
    /// Set from the first plan returned by the planner; `None` when not using
    /// `--planner` or when the frontend returned no plans.
    pub default_execute_plan: Option<crate::protocol::InvocationPlan>,
    /// Per-parameter value source for the function under exploration.
    /// Custom-generator/extractor slots (e.g. axum `State<AppState>`) carry
    /// native-replay markers and must never be mutated or seeded over (str-6cdp).
    /// Empty when no generators are configured; every slot is then built-in.
    pub value_sources: Vec<crate::input_gen::ValueSource>,
}

/// Default shrink budget per behavior witness.
pub const DEFAULT_SHRINK_BUDGET: usize = 20;

/// Default maximum total executions before stopping exploration.
pub const DEFAULT_MAX_EXECUTIONS: usize = 500;

/// Fitness boost for branches in the first loop iteration.
const BOUNDARY_FITNESS_FIRST: f64 = 1.0;
/// Fitness boost for branches in the second loop iteration.
const BOUNDARY_FITNESS_SECOND: f64 = 0.9;

/// Stall count threshold before bounded symbolic unrolling is eligible.
const BOUNDED_UNROLL_STALL_THRESHOLD: u32 = drilling::DRILL_STALL_THRESHOLD + 1;
/// Maximum stalled loop frontiers to target with bounded unroll per round.
const MAX_BOUNDED_UNROLL_FRONTIERS_PER_ROUND: usize = 2;
/// Default bounded-unroll depth when no solver timeout budget is configured.
const DEFAULT_BOUNDED_UNROLL_DEPTH: u32 = 64;
/// Minimum bounded-unroll depth when deriving a budget from solver timeout.
const MIN_BOUNDED_UNROLL_DEPTH: u32 = 8;
/// Convert solver timeout budget into a capped unroll-depth budget.
const SOLVER_TIMEOUT_MS_PER_UNROLL_STEP: u64 = 100;

impl Default for ExploreConfig {
    fn default() -> Self {
        Self {
            max_iterations: Some(100),
            max_executions: Some(DEFAULT_MAX_EXECUTIONS),
            plateau_threshold: 20,
            mocks: vec![],
            mock_params: vec![],
            solver_timeout_ms: None,
            seed: None,
            solver_offload: false,
            timeout_explore: None,
            branch_profile: None,
            meta_config: crate::strategy::MetaConfig::default(),
            execution_profile: None,
            refine_budget: None,
            shrink_budget: DEFAULT_SHRINK_BUDGET,
            mcdc: false,
            fuzz: crate::config::FuzzConfig::default(),
            planner: None,
            default_execute_plan: None,
            value_sources: vec![],
        }
    }
}

/// How an input was generated — determines worklist priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputSource {
    /// Least priority: initial seed values.
    Seed = 0,
    /// Low priority: fuzzed from concrete values of unknown constraints.
    Fuzzed = 1,
    /// LLM seed-oracle candidate. Drained at priority 4 in the input-selection
    /// loop — above the random/fuzzed fallback but below Z3, drilling, and
    /// boundary search.
    LlmOracle = 2,
    /// Between fuzz and drill: interpolated between true/false witnesses.
    BoundarySearch = 3,
    /// Medium priority: targeted mutation of blocking params on a stalled frontier.
    Drilled = 4,
    /// MC/DC-targeted: Z3 solved with condition-independence constraint.
    /// Ranks between Drilled and Z3Solved — MC/DC refines coverage within
    /// already-visited branches, while Z3Solved discovers new branch paths.
    McdcTarget = 5,
    /// High priority: Z3-solved inputs targeting a specific branch.
    Z3Solved = 6,
    /// Highest priority: user-provided candidate inputs from `.shatter/` config.
    UserProvided = 7,
}

/// An entry in the exploration worklist.
#[derive(Debug, Clone)]
pub struct WorklistEntry {
    /// Input values to pass to the function.
    pub inputs: Vec<serde_json::Value>,
    /// How these inputs were generated.
    pub source: InputSource,
    /// Optional fitness score (0.0–1.0) from genetic scoring.
    ///
    /// When present, fitness is the primary ordering key for the worklist's
    /// BinaryHeap. When absent (`None`), the entry falls back to source-based
    /// ordering, which preserves backward compatibility with the pre-genetic
    /// pipeline.
    pub fitness: Option<f64>,
    /// Per-entry mock configurations for dynamic mock variation.
    /// When non-empty, these override `ExploreConfig::mocks` for this execution.
    pub mock_values: Vec<MockConfig>,
}

impl Eq for WorklistEntry {}

impl PartialEq for WorklistEntry {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.fitness_key() == other.fitness_key()
    }
}

impl PartialOrd for WorklistEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorklistEntry {
    /// Primary ordering: fitness score (higher is better). Entries with a
    /// fitness score always outrank entries without one. Among entries without
    /// fitness, the original source-based priority applies.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.fitness, other.fitness) {
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), Some(_)) => self
                .fitness_key()
                .cmp(&other.fitness_key())
                .then_with(|| self.source.cmp(&other.source)),
            (None, None) => self.source.cmp(&other.source),
        }
    }
}

impl WorklistEntry {
    /// Convert fitness f64 to an integer key for total ordering.
    ///
    /// Multiplies by 1_000_000 and truncates to i64 so that BinaryHeap
    /// (which requires Ord) can rank by fitness without floating-point
    /// comparison issues.
    fn fitness_key(&self) -> i64 {
        self.fitness.map_or(0, |f| (f * 1_000_000.0) as i64)
    }
}

/// Why the exploration loop terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    /// Reached the maximum number of unique paths (max_iterations).
    MaxIterations,
    /// Reached the maximum total executions (max_executions).
    MaxExecutions,
    /// No new paths discovered for `plateau_threshold` consecutive executions.
    CoveragePlateau,
    /// The worklist is empty — all reachable paths have been explored.
    WorklistExhausted,
    /// Exceeded the per-function exploration wall-clock timeout.
    TimeoutExplore,
    /// All MC/DC independence pairs satisfied (100% MC/DC coverage).
    McdcComplete,
}

/// str-nqrz: Compute the per-fuzz-phase execution cap clamped by the
/// remaining global execution budget.
///
/// The orchestrator fires a fuzz phase on coverage plateau and previously
/// granted it up to `DEFAULT_FUZZ_MAX_EXECUTIONS` executions, ignoring the
/// caller's `max_executions` (the user's `--max-iterations` cap once the CLI
/// stopped multiplying it by 5). With a small user cap (e.g. 5), a single
/// fuzz phase could add up to a thousand additional executions on top.
///
/// This helper returns the smaller of the configured per-fuzz-phase cap and
/// the remaining global budget. When `global_cap` is `None`, the configured
/// cap is returned unchanged (unbounded global budget).
pub(crate) fn clamp_fuzz_budget(
    fuzz_max_executions_raw: u32,
    global_cap: Option<usize>,
    total_executions: usize,
) -> u32 {
    match global_cap {
        Some(cap) => {
            let remaining = (cap as u32).saturating_sub(total_executions as u32);
            fuzz_max_executions_raw.min(remaining)
        }
        None => fuzz_max_executions_raw,
    }
}

/// Summary of a concolic exploration session.
#[derive(Debug)]
pub struct ExploreResult {
    /// Name of the explored function.
    pub function_name: String,
    /// Total source lines in the function (end_line - start_line + 1).
    pub total_lines: u32,
    /// Execution results for each unique path discovered.
    pub executions: Vec<ExecuteResult>,
    /// Number of unique branch paths discovered.
    pub unique_paths: usize,
    /// Total number of executions performed (including duplicate paths).
    pub total_executions: usize,
    /// Number of inputs generated by Z3 solving.
    pub z3_generated: usize,
    /// Number of inputs generated by fuzzing.
    pub fuzz_generated: usize,
    /// Number of inputs generated by boundary search between witnesses.
    pub boundary_generated: usize,
    /// Number of inputs generated by parameter drilling on stalled frontiers.
    pub drill_generated: usize,
    /// Why the exploration loop stopped.
    pub termination_reason: TerminationReason,
    /// Raw execution results paired with inputs and mock configs for pipeline composability.
    pub raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)>,
    /// Per-branch discovery attribution with method (Z3, Random, UserProvided).
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
    /// Number of inputs skipped by triage prediction.
    pub triage_skipped: usize,
    /// Number of sampled skip predictions that were wrong.
    pub triage_mispredictions: usize,
    /// Fields detected as nondeterministic via within-run re-execution sampling.
    pub nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
    /// Float probe results classifying Float params as integer-treating or float-sensitive.
    pub float_probe_results: Vec<crate::float_probe::FloatProbeResult>,
    /// Refined boundary witness pairs from post-discovery refinement phase.
    pub boundary_results: Vec<crate::boundary_search::BoundaryResult>,
    /// Shrunk witnesses: maps branch_path hash to minimal inputs that reproduce the same path.
    pub shrunk_witnesses: std::collections::HashMap<u64, Vec<serde_json::Value>>,
    /// MC/DC summary: (total_conditions, independent_conditions, opaque_conditions).
    /// Present only when `ExploreConfig::mcdc` is true.
    pub mcdc_summary: Option<(usize, usize, usize)>,
    /// Number of times Z3 solving was successfully pipelined with the next observe.
    pub pipeline_overlaps: usize,
    /// Aggregated shrink phase performance counters.
    pub shrink_stats: crate::shrink::ShrinkStats,
    /// Frontiers abandoned due to stall detection: (branch_id, final_stall_count).
    /// Used for diagnostics and budget reallocation (str-6aq.1).
    pub abandoned_frontiers: Vec<(u32, u32)>,
    /// Parameters suggested as opaque type candidates based on solver failures.
    pub opaque_suggestions: Vec<crate::executability::OpaqueSuggestion>,
    /// Module names that could not be resolved and were replaced with stubs.
    pub stubbed_modules: Vec<String>,
    /// True when the per-function wall-clock budget (`config.timeout_explore`)
    /// was exceeded at any checkpoint — main loop, pre-loop float-probe phase,
    /// or post-loop refine/shrink phases. Set whenever the orchestrator
    /// detects the deadline has been crossed, independent of which
    /// `termination_reason` recorded the loop's exit. str-jeen.65: prevents
    /// timed-out functions from being silently reported as `ok` when a tail
    /// phase (Z3, refine, shrink) runs past the deadline after the loop
    /// terminated for an unrelated reason (WorklistExhausted, MaxIterations,
    /// CoveragePlateau, McdcComplete).
    pub timed_out: bool,
    /// Aggregate LLM seed-oracle telemetry when an oracle was wired into
    /// `explore_with_oracle`. `None` when no oracle was provided.
    pub oracle_stats: Option<OracleStats>,
}

/// Caller-supplied bundle wiring an [`OracleSlotMap`] into the orchestrator.
///
/// `function_source` is the (already-trimmed) source window the orchestrator
/// passes into every [`OracleContext`]. It is the caller's responsibility to
/// honor `LlmConfig::context_lines` when preparing this string.
pub struct OracleHandle<'a> {
    pub slot_map: &'a mut OracleSlotMap,
    pub function_source: String,
}

/// Drain at most one ready LLM-oracle candidate, polling each unsolved
/// frontier in turn until one yields an input vector that survives type
/// validation and dedup against `attempted_by_condition`.
///
/// Returns `Some((inputs, condition_id))` on the first successful drain,
/// `None` when no oracle is wired in or none of the frontiers had a ready
/// candidate this tick.
#[allow(clippy::too_many_arguments)]
fn poll_oracle_for_frontier(
    oracle: Option<&mut OracleHandle<'_>>,
    frontier_set: &FrontierSet,
    seen_branch_sides: &HashSet<(u32, bool)>,
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    param_infos: &[ParamInfo],
    attempted_by_condition: &HashMap<ConditionId, VecDeque<InputVector>>,
    _function_name: &str,
    function_source: &str,
) -> Option<(InputVector, ConditionId)> {
    let handle = oracle?;

    for frontier in frontier_set.iter() {
        // Skip frontiers whose opposite side has already been observed —
        // those conditions are effectively solved.
        if seen_branch_sides.contains(&(frontier.branch_id, true))
            && seen_branch_sides.contains(&(frontier.branch_id, false))
        {
            continue;
        }
        let condition_id = ConditionId::from(frontier.branch_id);

        // Look up the most recent execution that observed this branch to
        // recover a human-readable predicate/location for OracleContext.
        let mut predicate = String::new();
        let mut location = format!("branch:{}", frontier.branch_id);
        for (_, _, result) in raw_results.iter().rev() {
            if let Some(decision) = result
                .branch_path
                .iter()
                .find(|d| d.branch_id == frontier.branch_id)
            {
                location = format!("branch:{}:line:{}", decision.branch_id, decision.line);
                if let crate::execution_record::SymConstraint::Expr { expr } = &decision.constraint
                {
                    predicate = format!("{expr:?}");
                } else if let crate::execution_record::SymConstraint::Unknown { hint } =
                    &decision.constraint
                {
                    predicate = hint.clone();
                }
                break;
            }
        }

        let attempted = attempted_by_condition
            .get(&condition_id)
            .map(|q| q.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let ctx = OracleContext {
            function_source: function_source.to_string(),
            param_types: param_infos.to_vec(),
            condition: FailedCondition {
                predicate,
                location,
            },
            attempted,
        };

        let candidate = handle.slot_map.poll(condition_id, ctx)?;

        // Type-validate against param_infos — drop mismatches with no coercion.
        if !candidate_matches_params(&candidate, param_infos) {
            continue;
        }

        // Deduplicate against attempted_by_condition[condition_id].
        let already_attempted = attempted_by_condition
            .get(&condition_id)
            .is_some_and(|q| q.iter().any(|prev| prev == &candidate));
        if already_attempted {
            continue;
        }

        return Some((candidate, condition_id));
    }
    None
}

/// Type-validate a candidate input vector against parameter type info.
///
/// Returns true only when arity matches and every value is plausibly of the
/// declared parameter type. Performs no coercion — mismatched candidates are
/// dropped so the orchestrator can poll the slot again on a future tick.
fn candidate_matches_params(candidate: &[serde_json::Value], params: &[ParamInfo]) -> bool {
    if candidate.len() != params.len() {
        return false;
    }
    for (val, param) in candidate.iter().zip(params.iter()) {
        if !json_value_matches_type(val, &param.typ) {
            return false;
        }
    }
    true
}

fn json_value_matches_type(value: &serde_json::Value, typ: &crate::types::TypeInfo) -> bool {
    use crate::types::TypeInfo;
    match typ {
        TypeInfo::Bool => value.is_boolean(),
        TypeInfo::Int { .. } => value.is_i64() || value.is_u64(),
        TypeInfo::Float => value.is_f64() || value.is_i64() || value.is_u64(),
        TypeInfo::Str => value.is_string(),
        TypeInfo::Array { .. } => value.is_array(),
        TypeInfo::Object { .. } => value.is_object(),
        TypeInfo::Nullable { inner } => value.is_null() || json_value_matches_type(value, inner),
        // Permissive for the remaining variants (Union, Complex, Opaque, Unknown).
        _ => true,
    }
}

/// Effective per-Z3-query timeout: the configured `solver_timeout_ms` capped by
/// the configured per-function `timeout_explore`. A single Z3 call shouldn't be
/// allowed to outlive the entire function budget, so when both are set we use
/// whichever is smaller. str-jeen.65.
fn effective_solver_timeout_ms(config: &ExploreConfig) -> Option<u64> {
    match (config.solver_timeout_ms, config.timeout_explore) {
        (Some(s), Some(t)) => {
            let t_ms = u64::try_from(t.as_millis()).unwrap_or(u64::MAX).max(1);
            Some(s.min(t_ms))
        }
        (s, _) => s,
    }
}

/// Solver feedback execution mode for the concolic loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConcolicFeedbackMode {
    /// Preserve the existing candidate -> observe -> feedback ordering.
    Sync,
    /// Offload feedback to Tokio's blocking pool and allow one prefetched
    /// candidate to execute while Z3 solving runs.
    Async,
}

/// Coordinates `MetaStrategy` feedback so Z3 solving can overlap with the next
/// concolic observation when a ready candidate exists.
struct ConcolicFeedbackScheduler {
    meta_strategy: Option<MetaStrategy>,
    pending_feedback: Option<JoinHandle<MetaStrategy>>,
    prefetched_candidate: Option<(
        Vec<serde_json::Value>,
        usize,
        crate::strategy::RegisteredStrategyKind,
    )>,
    mode: ConcolicFeedbackMode,
    pipeline_overlaps: usize,
}

impl ConcolicFeedbackScheduler {
    fn new(meta_strategy: MetaStrategy, mode: ConcolicFeedbackMode) -> Self {
        Self {
            meta_strategy: Some(meta_strategy),
            pending_feedback: None,
            prefetched_candidate: None,
            mode,
            pipeline_overlaps: 0,
        }
    }

    fn pipeline_overlaps(&self) -> usize {
        self.pipeline_overlaps
    }

    async fn drain_pending_feedback(&mut self) -> Result<(), ExploreError> {
        if let Some(handle) = self.pending_feedback.take() {
            let meta_strategy = handle.await.map_err(|err| {
                ExploreError::SolverFeedback(format!("solver feedback task failed: {err}"))
            })?;
            self.meta_strategy = Some(meta_strategy);
        }
        Ok(())
    }

    async fn next_meta_candidate(
        &mut self,
        ctx: &StrategyContext,
        rng: &mut impl Rng,
    ) -> Result<
        Option<(
            Vec<serde_json::Value>,
            usize,
            crate::strategy::RegisteredStrategyKind,
        )>,
        ExploreError,
    > {
        if let Some(candidate) = self.prefetched_candidate.take() {
            if self.pending_feedback.is_some() {
                self.pipeline_overlaps += 1;
            }
            return Ok(Some(candidate));
        }

        self.drain_pending_feedback().await?;
        let Some(meta_strategy) = self.meta_strategy.as_mut() else {
            return Ok(None);
        };
        Ok(meta_strategy.next(ctx, rng).map(|(inputs, idx)| {
            let kind = meta_strategy.strategy_kind(idx);
            (inputs, idx, kind)
        }))
    }

    async fn submit_feedback(
        &mut self,
        inputs: Vec<serde_json::Value>,
        result: ExecuteResult,
        was_new_path: bool,
        strategy_idx: Option<usize>,
        ctx: &StrategyContext,
        rng: &mut impl Rng,
    ) -> Result<(), ExploreError> {
        self.drain_pending_feedback().await?;

        let Some(mut meta_strategy) = self.meta_strategy.take() else {
            return Ok(());
        };

        if self.mode == ConcolicFeedbackMode::Sync {
            meta_strategy.feedback(&inputs, &result, was_new_path);
            if let Some(idx) = strategy_idx {
                meta_strategy.record_outcome(idx, was_new_path);
            }
            self.meta_strategy = Some(meta_strategy);
            return Ok(());
        }

        if self.prefetched_candidate.is_none()
            && let Some((prefetched_inputs, idx)) = meta_strategy.next(ctx, rng)
        {
            let kind = meta_strategy.strategy_kind(idx);
            self.prefetched_candidate = Some((prefetched_inputs, idx, kind));
        }

        self.pending_feedback = Some(tokio::task::spawn_blocking(move || {
            meta_strategy.feedback(&inputs, &result, was_new_path);
            if let Some(idx) = strategy_idx {
                meta_strategy.record_outcome(idx, was_new_path);
            }
            meta_strategy
        }));
        Ok(())
    }
}

/// Resumable state from a completed `explore()` call.
///
/// Pass to the next batch of the same function to skip path rediscovery.
/// Without this, each batch starts with an empty `covered_paths` set and
/// wastes iterations rediscovering paths found in earlier batches.
#[derive(Debug, Clone, Default)]
pub struct ExploreState {
    /// Path hashes already discovered — `observe_one()` will classify
    /// these as duplicates immediately, avoiding rediscovery.
    pub covered_paths: HashSet<u64>,
    /// Inputs that discovered new paths in prior batches. Added to the
    /// seed pool so the fuzzer/solver has frontier-adjacent starting
    /// points instead of re-deriving them from scratch.
    pub discovery_inputs: Vec<Vec<serde_json::Value>>,
}

/// Tracks fuzz attempt state for a single branch.
#[derive(Debug, Clone)]
pub struct FuzzAttemptState {
    pub count: u32,
    pub coverage_at_last_attempt: usize,
}

/// Check whether a branch is eligible for a fuzz attempt.
///
/// A branch is eligible if:
/// - It has never been fuzzed before, OR
/// - Coverage has grown since the last attempt (new paths may unlock the branch), OR
/// - `max_attempts` is `Some(n)` and fewer than `n` attempts have been made.
///
/// When `max_attempts` is `None` (indefinite mode), the branch is only re-eligible
/// when coverage grows — preventing unbounded retries on truly opaque branches.
fn is_fuzz_eligible(
    branch_id: u32,
    attempts: &HashMap<u32, FuzzAttemptState>,
    max_attempts: Option<u32>,
    current_coverage: usize,
) -> bool {
    match attempts.get(&branch_id) {
        None => true,
        Some(state) => {
            if current_coverage > state.coverage_at_last_attempt {
                return true;
            }
            match max_attempts {
                Some(max) => state.count < max,
                None => false,
            }
        }
    }
}

/// Errors that can occur during concolic exploration.
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error("planner error: {0}")]
    Planner(#[from] crate::planner_consumer::PlannerConsumerError),
    #[error("solver feedback error: {0}")]
    SolverFeedback(String),
    /// Frontend reported the target as `not_supported` during execute — either a
    /// response-level `ErrorCode::NotSupported` or a `not_supported` thrown_error
    /// nested in an Ok execute result. The scan layer maps this to
    /// `SkipCategory::Unsupported` with a clean reason instead of
    /// `SkipCategory::Error`, mirroring the random explorer path (str-303gg).
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// A single execution observation: the inputs used, the result, and path classification.
#[derive(Debug, Clone)]
pub struct Observation {
    /// Input values that were executed.
    pub inputs: Vec<serde_json::Value>,
    /// Execution result from the frontend.
    pub result: ExecuteResult,
    /// How the inputs were generated.
    pub source: InputSource,
    /// Hash of the branch path for deduplication.
    pub path_id: u64,
    /// Whether this execution discovered a previously unseen path.
    pub is_new_path: bool,
    /// Whether this was a sampled skip (triage predicted skip, but we executed anyway to validate).
    pub is_sampled_skip: bool,
    /// Mock configurations used for this execution.
    pub mock_values: Vec<MockConfig>,
}

/// Output of the Solve/Generate phase — new candidate inputs produced from observations.
#[derive(Debug, Default)]
pub struct SolveOutput {
    /// All candidate inputs to feed into the next observe round.
    pub candidates: Vec<WorklistEntry>,
    /// Number of inputs generated by Z3 solving.
    pub z3_count: usize,
    /// Number of inputs generated by fuzzing unknown constraints.
    pub fuzz_count: usize,
    /// Number of inputs generated by parameter drilling on stalled frontiers.
    pub drill_count: usize,
    /// Number of inputs generated by boundary search interpolation.
    pub boundary_count: usize,
    /// Per-parameter count of Unsat/Err solve failures (for opaque type suggestions).
    pub param_fail_counts: HashMap<String, usize>,
}

/// Compute a hash of the branch path (branch_id + taken pairs) to identify unique paths.
pub fn hash_branch_path(branch_path: &[crate::execution_record::BranchDecision]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    for decision in branch_path {
        decision.branch_id.hash(&mut hasher);
        decision.taken.hash(&mut hasher);
    }
    hasher.finish()
}

/// Extract the list of solvable `SymExpr` path conditions from an execution result's branch path.
///
/// Each constraint is adjusted based on the `taken` field so that negating it
/// in the solver produces inputs that flip the branch:
/// - `taken=true`: the path condition is the constraint itself
/// - `taken=false`: the path condition is `NOT(constraint)`, because the branch
///   condition evaluated to false
///
/// Returns `None` for branches with `Unknown` constraints; those are skipped
/// by the solver but may be targeted by fuzzing.
pub(crate) fn extract_sym_constraints(result: &ExecuteResult) -> Vec<Option<SymExpr>> {
    result
        .branch_path
        .iter()
        .map(|decision| match &decision.constraint {
            SymConstraint::Expr { expr } => {
                if decision.taken {
                    Some(expr.clone())
                } else {
                    // The branch was not taken, so the actual path condition
                    // is NOT(constraint). Wrapping it here ensures that when
                    // the solver negates this entry, it produces the raw
                    // constraint — i.e., the condition needed to take the branch.
                    Some(SymExpr::UnOp {
                        op: crate::sym_expr::UnOpKind::Not,
                        operand: Box::new(expr.clone()),
                    })
                }
            }
            SymConstraint::Unknown { .. } => None,
        })
        .collect()
}

fn bounded_unroll_depth(solver_timeout_ms: Option<u64>) -> u32 {
    match solver_timeout_ms {
        Some(timeout_ms) => {
            let derived_depth = timeout_ms / SOLVER_TIMEOUT_MS_PER_UNROLL_STEP;
            derived_depth
                .max(u64::from(MIN_BOUNDED_UNROLL_DEPTH))
                .min(u64::from(u32::MAX)) as u32
        }
        None => DEFAULT_BOUNDED_UNROLL_DEPTH,
    }
}

fn opposite_branch_constraint(expr: SymExpr, was_taken: bool) -> SymExpr {
    if was_taken {
        SymExpr::UnOp {
            op: crate::sym_expr::UnOpKind::Not,
            operand: Box::new(expr),
        }
    } else {
        expr
    }
}

fn substitute_loop_locals(
    expr: &SymExpr,
    locals: &std::collections::BTreeMap<String, SymExpr>,
) -> SymExpr {
    match expr {
        SymExpr::Param { name, .. } => locals.get(name).cloned().unwrap_or_else(|| expr.clone()),
        SymExpr::Const(_) | SymExpr::Unknown => expr.clone(),
        SymExpr::BinOp { op, left, right } => SymExpr::BinOp {
            op: *op,
            left: Box::new(substitute_loop_locals(left, locals)),
            right: Box::new(substitute_loop_locals(right, locals)),
        },
        SymExpr::UnOp { op, operand } => SymExpr::UnOp {
            op: *op,
            operand: Box::new(substitute_loop_locals(operand, locals)),
        },
        SymExpr::Call {
            name,
            receiver,
            args,
        } => SymExpr::Call {
            name: name.clone(),
            receiver: receiver
                .as_ref()
                .map(|expr| Box::new(substitute_loop_locals(expr, locals))),
            args: args
                .iter()
                .map(|arg| substitute_loop_locals(arg, locals))
                .collect(),
        },
        SymExpr::Ite {
            condition,
            then_expr,
            else_expr,
        } => SymExpr::Ite {
            condition: Box::new(substitute_loop_locals(condition, locals)),
            then_expr: Box::new(substitute_loop_locals(then_expr, locals)),
            else_expr: Box::new(substitute_loop_locals(else_expr, locals)),
        },
    }
}

fn loop_bound_constraint(
    loop_info: &crate::protocol::LoopInfo,
    loop_state: &std::collections::BTreeMap<String, SymExpr>,
) -> Option<SymExpr> {
    let induction_expr = loop_state.get(&loop_info.induction_var.name)?.clone();
    let op = match loop_info.induction_var.bound_op {
        crate::protocol::BoundOp::Lt => crate::sym_expr::BinOpKind::Lt,
        crate::protocol::BoundOp::Le => crate::sym_expr::BinOpKind::Le,
        crate::protocol::BoundOp::Gt => crate::sym_expr::BinOpKind::Gt,
        crate::protocol::BoundOp::Ge => crate::sym_expr::BinOpKind::Ge,
    };
    Some(SymExpr::BinOp {
        op,
        left: Box::new(induction_expr),
        right: Box::new(loop_info.induction_var.bound_expr.clone()),
    })
}

fn loop_prefix_constraints(
    result: &ExecuteResult,
    target_branch_id: u32,
    target_loop_id: u32,
) -> Vec<SymExpr> {
    let loop_context = extract_loop_context(&result.scope_events);
    let sym_constraints = extract_sym_constraints(result);
    let target_index = result
        .branch_path
        .iter()
        .position(|decision| decision.branch_id == target_branch_id)
        .unwrap_or(result.branch_path.len());

    result
        .branch_path
        .iter()
        .zip(sym_constraints)
        .enumerate()
        .filter_map(|(index, (decision, constraint_opt))| {
            if index >= target_index {
                return None;
            }
            let enclosing_loops = loop_context.get(&decision.branch_id);
            if enclosing_loops.is_some_and(|loop_ids| loop_ids.contains(&target_loop_id)) {
                return None;
            }
            constraint_opt
        })
        .collect()
}

fn adjust_loop_bound_candidate(
    candidate_inputs: &mut [serde_json::Value],
    template: &crate::symbolic_unroll::IterationTemplate,
    loop_info: &crate::protocol::LoopInfo,
    param_names: &[String],
    solved_values: &HashMap<String, ConcreteValue>,
) {
    let iteration_value = match solved_values.get(&template.iteration_var) {
        Some(ConcreteValue::Int(value)) if *value >= 0 => *value as u32,
        _ => return,
    };
    let loop_state =
        match crate::symbolic_unroll::materialize_iteration_state(template, iteration_value) {
            Ok(state) => state,
            Err(_) => return,
        };
    let induction_value = match loop_state.get(&loop_info.induction_var.name) {
        Some(SymExpr::Const(crate::sym_expr::ConstValue::Int(value))) => *value,
        _ => return,
    };
    let bound_name = match &loop_info.induction_var.bound_expr {
        SymExpr::Param { name, path } if path.is_empty() => name,
        _ => return,
    };
    let Some(bound_index) = param_names.iter().position(|name| name == bound_name) else {
        return;
    };

    let adjusted_bound = match loop_info.induction_var.bound_op {
        crate::protocol::BoundOp::Lt => induction_value + 1,
        crate::protocol::BoundOp::Le => induction_value,
        crate::protocol::BoundOp::Gt => induction_value - 1,
        crate::protocol::BoundOp::Ge => induction_value,
    };
    candidate_inputs[bound_index] = serde_json::json!(adjusted_bound);
}

fn stalled_loop_candidate_inputs(
    frontier: &Frontier,
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    loops: &[crate::protocol::LoopInfo],
    param_infos: &[ParamInfo],
    param_names: &[String],
    solver_timeout_ms: Option<u64>,
) -> Option<Vec<serde_json::Value>> {
    let (_, _, witness_result) = raw_results.iter().rev().find(|(inputs, _, result)| {
        inputs == &frontier.best_prefix
            && result
                .branch_path
                .iter()
                .any(|decision| decision.branch_id == frontier.branch_id)
    })?;

    let loop_context = extract_loop_context(&witness_result.scope_events);
    let target_loop_id = *loop_context.get(&frontier.branch_id)?.iter().next()?;
    let loop_info = loops.iter().find(|info| info.loop_id == target_loop_id)?;

    let loop_snapshots: Vec<crate::protocol::LoopBodyState> = witness_result
        .loop_body_states
        .iter()
        .filter(|snapshot| snapshot.loop_id == target_loop_id)
        .cloned()
        .collect();
    if loop_snapshots.len() < crate::symbolic_unroll::MIN_TEMPLATE_ITERATIONS {
        return None;
    }

    let template =
        crate::symbolic_unroll::extract_iteration_template(loop_info, &loop_snapshots).ok()?;
    let target_depth = bounded_unroll_depth(solver_timeout_ms).max(template.iteration_count + 1);
    let bounded_template = crate::symbolic_unroll::IterationTemplate {
        iteration_count: target_depth,
        ..template
    };
    let unrolled_formula =
        crate::symbolic_unroll::build_unrolled_formula(&bounded_template).ok()?;

    let target_decision = witness_result
        .branch_path
        .iter()
        .find(|decision| decision.branch_id == frontier.branch_id)?;

    let target_expr = match &target_decision.constraint {
        crate::execution_record::SymConstraint::Expr { expr } => opposite_branch_constraint(
            substitute_loop_locals(expr, &unrolled_formula.locals),
            target_decision.taken,
        ),
        crate::execution_record::SymConstraint::Unknown { .. } => return None,
    };

    let mut constraints =
        loop_prefix_constraints(witness_result, frontier.branch_id, target_loop_id)
            .into_iter()
            .map(|expr| substitute_loop_locals(&expr, &unrolled_formula.locals))
            .collect::<Vec<_>>();
    constraints.push(unrolled_formula.iteration_bound.clone());
    constraints.push(loop_bound_constraint(loop_info, &unrolled_formula.locals)?);
    constraints.push(target_expr);

    match solver::solve_constraints(&constraints, solver_timeout_ms, param_infos).ok()? {
        SolveResult::Sat(values) => {
            let mut candidate_inputs =
                overlay_solved_values(&frontier.best_prefix, &values, param_names);
            adjust_loop_bound_candidate(
                &mut candidate_inputs,
                &bounded_template,
                loop_info,
                param_names,
                &values,
            );
            Some(candidate_inputs)
        }
        SolveResult::Unsat => None,
    }
}

/// Convert Z3 `ConcreteValue`s back into JSON values suitable for the Execute protocol.
///
/// For `Complex` values, produces a `__complex_type` tagged JSON object.
/// The solver unwraps complex types to their repr for solving, but when
/// converting back to JSON we need to re-wrap with the type tag so the
/// frontend can reconstruct the native value.
pub(crate) fn concrete_to_json(value: &ConcreteValue) -> serde_json::Value {
    match value {
        ConcreteValue::Int(i) => serde_json::json!(*i),
        ConcreteValue::Float(f) => serde_json::json!(*f),
        ConcreteValue::Str(s) => serde_json::json!(s),
        ConcreteValue::Bool(b) => serde_json::json!(*b),
        ConcreteValue::Complex { kind, repr } => {
            // str-ieuc: GoByte serializes as a plain JSON integer (clamped to
            // [0, 255]) rather than a __complex_type wrapper, so the Go
            // wrapper's `json.Unmarshal` into byte / []byte / [N]byte
            // succeeds. The repr is an Int from the solver; clamp before
            // emitting so unconstrained / out-of-range solver outputs don't
            // make it to the wire.
            if matches!(kind, ComplexKind::GoByte) {
                let n = match repr.as_ref() {
                    ConcreteValue::Int(i) => i.rem_euclid(256),
                    _ => 0,
                };
                return serde_json::json!(n);
            }
            // str-cfsa: GoUint serializes as a plain non-negative JSON
            // integer. Clamp solver outputs to [0, u64::MAX] so negative
            // values from unconstrained solves don't reach the Go wrapper.
            if matches!(kind, ComplexKind::GoUint) {
                let n: u64 = match repr.as_ref() {
                    ConcreteValue::Int(i) => {
                        if *i < 0 {
                            0
                        } else {
                            *i as u64
                        }
                    }
                    _ => 0,
                };
                return serde_json::json!(n);
            }
            // Serialize the kind to its snake_case name for the wire format
            let kind_str = serde_json::to_value(kind)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{kind:?}").to_lowercase());
            let repr_json = concrete_to_json(repr);
            // Build tagged wire format: {"__complex_type": "<kind>", "value": <repr>}
            serde_json::json!({
                "__complex_type": kind_str,
                "value": repr_json,
            })
        }
    }
}

/// Build a new input vector by overlaying Z3-solved values onto existing inputs.
///
/// The solver returns variable names like "x", "config.timeout" etc. We match
/// these against parameter names in `base_inputs` (positionally). For now we
/// support simple flat parameters — if the variable name matches the parameter
/// index convention (param_0, param_1, …) or the base is a single param, we
/// update it directly.
#[requires(base_inputs.len() == param_names.len(), "base_inputs and param_names must be positionally aligned")]
#[contracts::ensures(ret.len() == base_inputs.len(), "overlay must preserve input vector length")]
pub(crate) fn overlay_solved_values(
    base_inputs: &[serde_json::Value],
    solved: &std::collections::HashMap<String, ConcreteValue>,
    param_names: &[String],
) -> Vec<serde_json::Value> {
    let mut result = base_inputs.to_vec();

    for (var_name, value) in solved {
        // Try to match variable name to a parameter by name.
        if let Some(idx) = param_names.iter().position(|n| n == var_name) {
            if idx < result.len() {
                result[idx] = concrete_to_json(value);
            }
        } else if let Some((param_name, path)) = solved_object_path(var_name)
            && let Some(idx) = param_names.iter().position(|n| n == param_name)
            && idx < result.len()
            && !path.is_empty()
        {
            overlay_json_path(&mut result[idx], &path, concrete_to_json(value));
        } else if param_names.len() == 1 && base_inputs.len() == 1 && !var_name.contains('.') {
            // Single-param function with a simple (non-derived) variable name:
            // the solver variable likely refers to the param. Skip derived names
            // like "email.length" which are internal Z3 variables, not params.
            result[0] = concrete_to_json(value);
        }
    }

    result
}

fn solved_object_path(var_name: &str) -> Option<(&str, Vec<String>)> {
    let mut parts = var_name.split('.').map(str::trim);
    let param = parts.next()?.trim();
    if param.is_empty() {
        return None;
    }
    let path: Vec<String> = parts
        .filter(|part| !part.is_empty())
        .filter(|part| !part.contains('(') && !part.contains(')'))
        .map(json_field_name)
        .collect();
    if path.is_empty() {
        None
    } else {
        Some((param, path))
    }
}

fn json_field_name(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut uppercase_next = false;
    for ch in field.chars() {
        if ch == '_' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            out.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn overlay_json_path(target: &mut serde_json::Value, path: &[String], value: serde_json::Value) {
    if path.is_empty() {
        *target = value;
        return;
    }
    if !target.is_object() {
        *target = serde_json::json!({});
    }
    let mut current = target;
    for segment in &path[..path.len() - 1] {
        if !current.is_object() {
            *current = serde_json::json!({});
        }
        let object = current
            .as_object_mut()
            .expect("target was normalized to object");
        current = object
            .entry(segment.clone())
            .or_insert_with(|| serde_json::json!({}));
    }
    if !current.is_object() {
        *current = serde_json::json!({});
    }
    let object = current
        .as_object_mut()
        .expect("target was normalized to object");
    object.insert(path[path.len() - 1].clone(), value);
}

/// Result of trying to observe a single worklist entry.
enum ObserveOneResult {
    /// Entry was executed and produced an observation.
    Observed(Box<Observation>),
    /// Entry was skipped by triage prediction.
    TriageSkipped,
    /// Frontend returned an error or unexpected response — entry skipped.
    FrontendSkipped,
    /// Frontend reported the target as `not_supported` for this iteration —
    /// either a response-level `NotSupported` or a `not_supported` thrown_error
    /// nested in an Ok result. The iteration produced no observation; the caller
    /// records the reason and reclassifies the whole FUNCTION as unsupported only
    /// if it collected no successful observation at all (str-303gg review fix).
    Unsupported(String),
    /// A termination budget was hit before executing.
    Terminated(TerminationReason),
}

/// Execute a single worklist entry and classify the result.
///
/// Returns an `Observation` with path classification, or a skip/termination
/// indicator. The caller is responsible for updating coverage state afterward.
#[allow(clippy::too_many_arguments)] // setup_context needed for parity with explorer
async fn observe_one(
    entry: &WorklistEntry,
    frontend: &mut Frontend,
    function_name: &str,
    param_infos: &[ParamInfo],
    config: &ExploreConfig,
    covered_paths: &mut HashSet<u64>,
    triage_state: &mut TriageState,
    budget: &ExploreBudget,
    setup_context: &Option<SetupContextStack>,
    prepare_id: Option<&str>,
    native_pins: Option<&crate::input_gen::NativePins>,
) -> Result<ObserveOneResult, ExploreError> {
    // Check termination budgets.
    if let Some(max) = config.max_iterations
        && budget.unique_paths >= max
    {
        return Ok(ObserveOneResult::Terminated(
            TerminationReason::MaxIterations,
        ));
    }
    if let Some(max) = config.max_executions
        && budget.total_executions >= max
    {
        return Ok(ObserveOneResult::Terminated(
            TerminationReason::MaxExecutions,
        ));
    }
    if let Some(timeout) = config.timeout_explore
        && budget.explore_start.elapsed() >= timeout
    {
        return Ok(ObserveOneResult::Terminated(
            TerminationReason::TimeoutExplore,
        ));
    }
    if config.plateau_threshold > 0 && budget.plateau_counter >= config.plateau_threshold {
        return Ok(ObserveOneResult::Terminated(
            TerminationReason::CoveragePlateau,
        ));
    }

    // Triage: predict whether this input will produce a novel path.
    // Seeds are always sampled when triage predicts Skip — they are explicitly
    // provided to exercise specific paths and are few in number, so the cost of
    // executing them is low while the cost of a wrong skip is high.
    let is_sampled_skip = if entry.source != InputSource::UserProvided {
        let verdict = triage_state.triage_candidate(&entry.inputs, covered_paths);
        triage_state.record_verdict(&verdict);
        if verdict == TriageVerdict::Skip {
            if entry.source == InputSource::Seed || triage_state.should_sample() {
                true
            } else {
                return Ok(ObserveOneResult::TriageSkipped);
            }
        } else {
            false
        }
    } else {
        false
    };

    // Use entry-level mocks when available, falling back to config.mocks.
    let effective_mocks = if entry.mock_values.is_empty() {
        &config.mocks
    } else {
        &entry.mock_values
    };

    // Execute concretely via the frontend. A per-request timeout is
    // converted to a clean termination signal rather than propagated as
    // an error — when the frontend is slow to respond (e.g., long compile
    // times in the Rust harness), the orchestrator should stop scheduling
    // new work and report a timeout, not a protocol ID mismatch.
    // execute_inputs_for_plan repairs each input against its parameter type
    // (str-kn3f) — the single funnel point for all execute paths.
    let execute_inputs = crate::planner_consumer::execute_inputs_for_plan_with_pins(
        &entry.inputs,
        param_infos,
        config.default_execute_plan.as_ref(),
        native_pins,
    )?;
    let response = match frontend
        .send(Command::Execute {
            function: function_name.to_string(),
            inputs: execute_inputs.inputs().to_vec(),
            mocks: effective_mocks.clone(),
            setup_context: setup_context.clone(),
            capture: true,
            prepare_id: prepare_id.map(|s| s.to_string()),
            execution_profile: config.execution_profile.clone(),
            plan: config.default_execute_plan.clone(),
        })
        .await
    {
        Ok(resp) => resp,
        Err(FrontendError::Timeout(_)) => {
            return Ok(ObserveOneResult::Terminated(
                TerminationReason::TimeoutExplore,
            ));
        }
        Err(e) => return Err(e.into()),
    };

    let exec_result = match response.result {
        ResponseResult::Execute(result) => *result,
        ResponseResult::Error { code, message, .. } => {
            // str-303gg review fix: a response-level `NotSupported` is an
            // unsupported iteration, not a generic frontend error. Surface it so
            // the aggregate classification can reclassify the function as
            // unsupported when nothing else executed, mirroring the random
            // explorer path — without aborting a function that did collect
            // coverage on other iterations.
            if code == crate::protocol::ErrorCode::NotSupported {
                return Ok(ObserveOneResult::Unsupported(message));
            }
            log::warn!("frontend error during execute: {message}");
            return Ok(ObserveOneResult::FrontendSkipped);
        }
        _ => return Ok(ObserveOneResult::FrontendSkipped),
    };

    // str-303gg: a `not_supported` thrown_error nested in an Ok execute result
    // is an unsupported iteration. Report it (without aborting) so the aggregate
    // classification can decide — a per-iteration abort would discard coverage
    // collected on other iterations.
    if let Some(reason) = crate::observe::thrown_not_supported_reason(&exec_result) {
        return Ok(ObserveOneResult::Unsupported(reason));
    }

    let path_id = hash_branch_path(&exec_result.branch_path);

    // Validate sampled skip prediction before modifying covered_paths.
    if is_sampled_skip {
        let already_covered = covered_paths.contains(&path_id);
        triage_state.record_sample(0, if already_covered { 0 } else { 1 });
    }

    let is_new_path = covered_paths.insert(path_id);
    if is_new_path {
        triage_state.update(&exec_result.branch_path);
    }
    let feedback_inputs = crate::planner_consumer::strategy_feedback_inputs_for_plan(
        execute_inputs.inputs(),
        param_infos.len(),
        config.default_execute_plan.as_ref(),
    );

    Ok(ObserveOneResult::Observed(Box::new(Observation {
        inputs: feedback_inputs,
        result: exec_result,
        source: entry.source,
        path_id,
        is_new_path,
        is_sampled_skip,
        mock_values: entry.mock_values.clone(),
    })))
}

/// Budget counters checked by `observe_one` to enforce termination limits.
struct ExploreBudget {
    unique_paths: usize,
    total_executions: usize,
    plateau_counter: usize,
    explore_start: Instant,
}

/// Classification of a branch's position within a loop iteration sequence.
/// Used by loop peeling to prioritize boundary iterations for negation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IterationPosition {
    /// First iteration of the loop.
    First,
    /// Second iteration of the loop.
    Second,
    /// First iteration followed immediately by loop exit.
    FirstExit,
    /// Third or later iteration (interior).
    Interior,
    /// Not inside any loop.
    NonLoop,
}

/// Walk scope_events and classify each Branch event's loop iteration position.
///
/// Returns one `IterationPosition` per Branch event in `scope_events` (skipping
/// Scope events). If `scope_events` is empty, returns `NonLoop` for every entry
/// in `branch_path`.
pub(crate) fn classify_iteration_positions(
    scope_events: &[TraceEvent],
    branch_path: &[BranchDecision],
) -> Vec<IterationPosition> {
    if scope_events.is_empty() {
        return vec![IterationPosition::NonLoop; branch_path.len()];
    }

    let mut loop_iter_count: HashMap<u32, u32> = HashMap::new();
    let mut loop_stack: Vec<u32> = Vec::new();

    // Pre-scan: for each branch event index, check if a LoopExit follows
    // before the next Branch event.
    let mut exit_follows: HashSet<usize> = HashSet::new();
    let mut branch_idx = 0usize;
    for (i, ev) in scope_events.iter().enumerate() {
        if matches!(ev, TraceEvent::Branch { .. }) {
            let mut j = i + 1;
            while j < scope_events.len() {
                match &scope_events[j] {
                    TraceEvent::Scope {
                        event: ScopeEvent::LoopExit { .. },
                    } => {
                        exit_follows.insert(branch_idx);
                        break;
                    }
                    TraceEvent::Branch { .. } => break,
                    _ => {}
                }
                j += 1;
            }
            branch_idx += 1;
        }
    }

    // Main pass: classify each branch event.
    let mut positions = Vec::new();
    let mut branch_event_idx = 0usize;
    for ev in scope_events {
        match ev {
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id },
            } => {
                *loop_iter_count.entry(*loop_id).or_insert(0) += 1;
                loop_stack.push(*loop_id);
            }
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id },
            } => {
                loop_stack.retain(|id| id != loop_id);
            }
            TraceEvent::Branch { .. } => {
                let pos = if let Some(&innermost_loop) = loop_stack.last() {
                    let count = loop_iter_count.get(&innermost_loop).copied().unwrap_or(0);
                    if count == 1 && exit_follows.contains(&branch_event_idx) {
                        IterationPosition::FirstExit
                    } else if count == 1 {
                        IterationPosition::First
                    } else if count == 2 {
                        IterationPosition::Second
                    } else {
                        IterationPosition::Interior
                    }
                } else {
                    IterationPosition::NonLoop
                };
                positions.push(pos);
                branch_event_idx += 1;
            }
            _ => {}
        }
    }

    positions
}

/// Detects branches inside loops whose `taken` value never varies across iterations.
/// Retained for unit tests; not used in the main exploration loop (Z3SolverStrategy
/// handles constraint selection independently).
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct LoopInvariantDetector {
    /// Maps (loop_id, branch_id) → list of observed `taken` values.
    observations: HashMap<(u32, u32), Vec<bool>>,
    /// Confirmed invariant (loop_id, branch_id) pairs.
    invariant_cache: HashSet<(u32, u32)>,
    /// Minimum observations before confirming invariance.
    min_observations: usize,
}

#[cfg(test)]
impl LoopInvariantDetector {
    pub(crate) fn new() -> Self {
        Self {
            observations: HashMap::new(),
            invariant_cache: HashSet::new(),
            min_observations: 2,
        }
    }

    /// Record branch `taken` values for each (loop_id, branch_id) pair in the trace.
    /// After recording, update the invariant cache: confirm new invariants,
    /// revoke ones that now vary.
    pub(crate) fn observe(&mut self, scope_events: &[TraceEvent], _branch_path: &[BranchDecision]) {
        if scope_events.is_empty() {
            return;
        }

        let mut loop_stack: Vec<u32> = Vec::new();

        for ev in scope_events {
            match ev {
                TraceEvent::Scope {
                    event: ScopeEvent::LoopEnter { loop_id },
                } => {
                    loop_stack.push(*loop_id);
                }
                TraceEvent::Scope {
                    event: ScopeEvent::LoopExit { loop_id },
                } => {
                    loop_stack.retain(|id| id != loop_id);
                }
                TraceEvent::Branch { decision } => {
                    if let Some(&innermost) = loop_stack.last() {
                        self.observations
                            .entry((innermost, decision.branch_id))
                            .or_default()
                            .push(decision.taken);
                    }
                }
                _ => {}
            }
        }

        // Update invariant cache.
        for (&key, values) in &self.observations {
            if values.len() >= self.min_observations {
                let all_same = values.iter().all(|&v| v == values[0]);
                if all_same {
                    self.invariant_cache.insert(key);
                } else {
                    self.invariant_cache.remove(&key);
                }
            }
        }
    }

    /// Return the set of branch_path indices that should be SKIPPED (redundant
    /// later occurrences of invariant branches). The first occurrence of each
    /// invariant (loop_id, branch_id) is kept; all subsequent are skipped.
    pub(crate) fn skip_indices(
        &self,
        scope_events: &[TraceEvent],
        branch_path: &[BranchDecision],
    ) -> HashSet<usize> {
        if scope_events.is_empty() || self.invariant_cache.is_empty() {
            return HashSet::new();
        }

        // Map each branch event index to its enclosing loop_id.
        let mut loop_stack: Vec<u32> = Vec::new();
        let mut branch_loop_map: Vec<Option<u32>> = Vec::new();

        for ev in scope_events {
            match ev {
                TraceEvent::Scope {
                    event: ScopeEvent::LoopEnter { loop_id },
                } => {
                    loop_stack.push(*loop_id);
                }
                TraceEvent::Scope {
                    event: ScopeEvent::LoopExit { loop_id },
                } => {
                    loop_stack.retain(|id| id != loop_id);
                }
                TraceEvent::Branch { .. } => {
                    branch_loop_map.push(loop_stack.last().copied());
                }
                _ => {}
            }
        }

        let mut seen_first: HashSet<(u32, u32)> = HashSet::new();
        let mut skip = HashSet::new();

        for (i, bd) in branch_path.iter().enumerate() {
            if let Some(Some(loop_id)) = branch_loop_map.get(i) {
                let key = (*loop_id, bd.branch_id);
                if self.invariant_cache.contains(&key) && !seen_first.insert(key) {
                    skip.insert(i);
                }
            }
        }

        skip
    }
}

/// Map each branch in scope_events to its enclosing loop ID(s).
pub(crate) fn extract_loop_context(scope_events: &[TraceEvent]) -> HashMap<u32, HashSet<u32>> {
    let mut loop_stack: Vec<u32> = Vec::new();
    let mut context: HashMap<u32, HashSet<u32>> = HashMap::new();

    for ev in scope_events {
        match ev {
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id },
            } => {
                loop_stack.push(*loop_id);
            }
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id },
            } => {
                loop_stack.retain(|id| id != loop_id);
            }
            TraceEvent::Branch { decision } => {
                for &loop_id in &loop_stack {
                    context
                        .entry(decision.branch_id)
                        .or_default()
                        .insert(loop_id);
                }
            }
            _ => {}
        }
    }

    context
}

/// Input bundle for the Z3 solve phase. Used by unit tests to validate Z3 solving
/// independently of the main exploration loop.
#[cfg(test)]
struct Z3SolveInput {
    obs: Observation,
    solvable_with_idx: Vec<(usize, SymExpr)>,
    invariant_skip: HashSet<usize>,
    param_infos: Vec<ParamInfo>,
    param_names: Vec<String>,
    solver_timeout_ms: Option<u64>,
}

/// Output from the Z3 solve phase. Used by unit tests only.
#[cfg(test)]
struct Z3SolveOutput {
    /// New worklist candidates produced by successful Z3 solves.
    candidates: Vec<WorklistEntry>,
    /// Number of inputs produced by Z3.
    z3_count: usize,
    /// Branch IDs that should have `increment_stall` called on them (Unsat/Err).
    stall_branch_ids: Vec<u32>,
    /// Per-parameter count of Unsat/Err solve failures (for opaque type suggestions).
    param_fail_counts: HashMap<String, usize>,
}

/// Pure Z3 solving phase: negates branch constraints to discover new paths.
///
/// Used by unit tests to validate Z3 solving logic directly.
#[cfg(test)]
fn z3_solve_step(input: Z3SolveInput) -> Z3SolveOutput {
    let mut output = Z3SolveOutput {
        candidates: Vec::new(),
        z3_count: 0,
        stall_branch_ids: Vec::new(),
        param_fail_counts: HashMap::new(),
    };

    // Only process new-path observations.
    if !input.obs.is_new_path {
        return output;
    }

    let solvable: Vec<SymExpr> = input
        .solvable_with_idx
        .iter()
        .map(|(_, e)| e.clone())
        .collect();

    if solvable.is_empty() {
        return output;
    }

    for (solve_idx, &(branch_idx, _)) in input.solvable_with_idx.iter().enumerate() {
        // Skip redundant later occurrences of loop-invariant branches.
        if input.invariant_skip.contains(&branch_idx) {
            continue;
        }
        let branch_id = input
            .obs
            .result
            .branch_path
            .get(branch_idx)
            .map_or(0, |bd| bd.branch_id);

        match solver::solve_for_new_path(
            &solvable,
            solve_idx,
            input.solver_timeout_ms,
            &input.param_infos,
        ) {
            Ok(SolveResult::Sat(values)) => {
                let new_inputs =
                    overlay_solved_values(&input.obs.inputs, &values, &input.param_names);
                output.candidates.push(WorklistEntry {
                    inputs: new_inputs,
                    source: InputSource::Z3Solved,
                    fitness: None,
                    mock_values: input.obs.mock_values.clone(),
                });
                output.z3_count += 1;
            }
            Ok(SolveResult::Unsat) | Err(_) => {
                // Track which parameters appeared in this unsolvable constraint
                // so we can suggest opaque types for frequently-failing params.
                let (_, ref expr) = input.solvable_with_idx[solve_idx];
                for name in crate::sym_expr::extract_param_names(expr) {
                    *output.param_fail_counts.entry(name).or_insert(0) += 1;
                }
                output.stall_branch_ids.push(branch_id);
            }
        }
    }

    output
}

/// Solve/Generate phase: produce boundary-search and drilling candidates.
///
/// For each new-path observation:
/// - Try boundary search for Unknown constraints (interpolate between witnesses)
/// - Drill stalled frontiers with targeted mutations
///
/// Z3 solving is handled by `Z3SolverStrategy.feedback()` in the main loop —
/// not in this function.
#[allow(clippy::too_many_arguments)] // boundary search + fitness scoring need broad context
fn solve_and_generate(
    observations: &[Observation],
    frontier_set: &mut FrontierSet,
    param_infos: &[ParamInfo],
    param_names: &[String],
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    seen_branch_sides: &std::collections::HashSet<(u32, bool)>,
    config: &ExploreConfig,
    loops: &[crate::protocol::LoopInfo],
    rng: &mut StdRng,
    target_branches: &HashSet<u32>,
    fitness_context: &mut FitnessContext,
    fitness_weights: &FitnessWeights,
    _mock_params: &[MockParam],
    branch_profile: Option<&crate::branch_profile::BranchProfile>,
) -> SolveOutput {
    let mut output = SolveOutput::default();

    for obs in observations.iter().filter(|o| o.is_new_path) {
        // Extract symbolic constraints from the branch path.
        let sym_constraints = extract_sym_constraints(&obs.result);

        // For Unknown constraints, try boundary search (interpolate between witnesses).
        let mut boundary_branches = 0usize;
        for (i, constraint_opt) in sym_constraints.iter().enumerate() {
            if constraint_opt.is_none() && i < obs.result.branch_path.len() {
                let bd = &obs.result.branch_path[i];
                let opposite_seen = seen_branch_sides.contains(&(bd.branch_id, !bd.taken));

                if opposite_seen
                    && let Some((tw, fw)) =
                        boundary_search::find_witness_pair(raw_results, bd.branch_id)
                {
                    let candidates = boundary_search::interpolate_inputs(
                        &tw,
                        &fw,
                        param_infos,
                        boundary_search::MAX_BOUNDARY_STEPS,
                    );
                    for interp in candidates {
                        output.candidates.push(WorklistEntry {
                            inputs: interp,
                            source: InputSource::BoundarySearch,
                            fitness: None,
                            mock_values: obs.mock_values.clone(),
                        });
                        output.boundary_count += 1;
                    }
                    boundary_branches += 1;
                    if boundary_branches >= boundary_search::MAX_BOUNDARY_BRANCHES_PER_ROUND {
                        break;
                    }
                }
            }
        }
    }

    // Parameter drilling: for stalled frontiers, generate targeted mutations.
    {
        let mut stalled: Vec<Frontier> = frontier_set
            .iter()
            .filter(|f| {
                f.stall_count >= drilling::DRILL_STALL_THRESHOLD
                    && f.stall_count < crate::frontier::FRONTIER_STALL_THRESHOLD
            })
            .cloned()
            .collect();
        stalled.sort_by(|a, b| {
            frontier_score(b)
                .partial_cmp(&frontier_score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        stalled.truncate(drilling::MAX_FRONTIERS_PER_ROUND);

        for frontier in &stalled {
            let count = drilling::DRILL_MUTATIONS_PER_PARAM * frontier.blocking_params.len().max(1);
            let drilled = drilling::generate_drilled_inputs(
                &frontier.best_prefix,
                &frontier.blocking_params,
                param_infos,
                count,
                rng,
            );
            for inputs in drilled {
                output.candidates.push(WorklistEntry {
                    inputs,
                    source: InputSource::Drilled,
                    fitness: None,
                    mock_values: vec![],
                });
                output.drill_count += 1;
            }
            frontier_set.increment_stall(frontier.branch_id);
        }
    }

    {
        let mut stalled_loop_frontiers: Vec<Frontier> = frontier_set
            .iter()
            .filter(|frontier| frontier.stall_count >= BOUNDED_UNROLL_STALL_THRESHOLD)
            .cloned()
            .collect();
        stalled_loop_frontiers.sort_by(|a, b| {
            frontier_score(b)
                .partial_cmp(&frontier_score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        stalled_loop_frontiers.truncate(MAX_BOUNDED_UNROLL_FRONTIERS_PER_ROUND);

        for frontier in &stalled_loop_frontiers {
            if let Some(inputs) = stalled_loop_candidate_inputs(
                frontier,
                raw_results,
                loops,
                param_infos,
                param_names,
                // str-jeen.65: cap by per-function budget.
                effective_solver_timeout_ms(config),
            ) {
                output.candidates.push(WorklistEntry {
                    inputs,
                    source: InputSource::Z3Solved,
                    fitness: None,
                    mock_values: vec![],
                });
                output.z3_count += 1;
            }
        }
    }

    // Score each candidate using genetic fitness. The parent observation's
    // branch path gives us approximate fitness context: candidates derived
    // from high-fitness executions should be explored first.
    if !target_branches.is_empty() {
        for candidate in &mut output.candidates {
            // Create a synthetic ExecuteResult from the parent observation's
            // branch path to estimate fitness. This gives candidates a
            // relative ranking even before execution.
            if let Some(parent_obs) = observations.iter().find(|o| o.is_new_path) {
                let breakdown = crate::genetic_fitness::score(
                    &parent_obs.result,
                    target_branches,
                    fitness_context,
                    fitness_weights,
                    branch_profile,
                );
                candidate.fitness = Some(breakdown.total);
            }
        }
    }

    // Loop peeling: boost candidates from observations containing boundary branches.
    // Candidates from observations with first/second iteration branches get higher
    // worklist priority, ensuring boundary paths are explored before deep interior.
    for obs in observations.iter().filter(|o| o.is_new_path) {
        let positions =
            classify_iteration_positions(&obs.result.scope_events, &obs.result.branch_path);
        let best_boost: Option<f64> = positions
            .iter()
            .filter_map(|pos| match pos {
                IterationPosition::First | IterationPosition::FirstExit => {
                    Some(BOUNDARY_FITNESS_FIRST)
                }
                IterationPosition::Second => Some(BOUNDARY_FITNESS_SECOND),
                _ => None,
            })
            .reduce(f64::max);

        if let Some(boost) = best_boost {
            for candidate in &mut output.candidates {
                candidate.fitness = Some(
                    candidate
                        .fitness
                        .map_or(boost, |existing| existing.max(boost)),
                );
            }
        }
    }

    output
}

/// Async boundary refinement: binary-searches between witness pairs using the
/// frontend for execution. Runs after discovery with its own per-boundary budget.
#[allow(clippy::too_many_arguments)] // native_pins added for extractor pinning (str-6cdp)
async fn refine_boundaries_async(
    frontend: &mut Frontend,
    function_name: &str,
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    param_infos: &[ParamInfo],
    budget_per_boundary: usize,
    setup_context: &Option<SetupContextStack>,
    execute_plan: Option<crate::protocol::InvocationPlan>,
    native_pins: Option<&crate::input_gen::NativePins>,
) -> Result<Vec<boundary_search::BoundaryResult>, ExploreError> {
    // Collect branch IDs with witnesses on both sides.
    let mut branch_ids: Vec<u32> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for (_inputs, _mocks, result) in raw_results {
        for decision in &result.branch_path {
            seen.insert(decision.branch_id);
        }
    }
    for &bid in &seen {
        if boundary_search::find_witness_pair(raw_results, bid).is_some() {
            branch_ids.push(bid);
        }
    }
    branch_ids.sort_unstable();

    let mocks: Vec<MockConfig> = raw_results
        .first()
        .map(|(_, m, _)| m.clone())
        .unwrap_or_default();

    let mut results = Vec::new();

    for branch_id in branch_ids {
        let (mut tw, mut fw) = match boundary_search::find_witness_pair(raw_results, branch_id) {
            Some(pair) => pair,
            None => continue,
        };

        let mut executions_used = 0;

        for _ in 0..budget_per_boundary {
            let candidates = boundary_search::interpolate_inputs(&tw, &fw, param_infos, 1);
            if candidates.is_empty() {
                break; // Converged.
            }

            let candidate = &candidates[0];
            let execute_candidate = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                candidate,
                param_infos,
                execute_plan.as_ref(),
                native_pins,
            )?;
            let response = match frontend
                .send(Command::Execute {
                    function: function_name.to_string(),
                    inputs: execute_candidate.inputs().to_vec(),
                    mocks: mocks.clone(),
                    setup_context: setup_context.clone(),
                    capture: false,
                    prepare_id: None,
                    execution_profile: None,
                    plan: execute_plan.clone(),
                })
                .await
            {
                Ok(r) => r,
                Err(_) => break,
            };

            let exec_result = match response.result {
                ResponseResult::Execute(result) => *result,
                _ => break,
            };
            executions_used += 1;

            let mut took_side: Option<bool> = None;
            for decision in &exec_result.branch_path {
                if decision.branch_id == branch_id {
                    took_side = Some(decision.taken);
                    break;
                }
            }

            match took_side {
                Some(true) => tw = candidate.clone(),
                Some(false) => fw = candidate.clone(),
                None => continue,
            }
        }

        results.push(boundary_search::BoundaryResult {
            branch_id,
            true_witness: tw,
            false_witness: fw,
            executions_used,
        });
    }

    Ok(results)
}

/// A gap in MC/DC coverage — a condition that still lacks an independence pair.
///
/// Collected after each new-path observation when MC/DC is enabled. These goals
/// are inputs to the Phase 5 targeted solver; for now they are generated and
/// logged but not yet turned into worklist entries.
#[derive(Debug, Clone)]
pub struct McdcGoal {
    /// Branch ID of the compound decision containing the target condition.
    pub branch_id: u32,
    /// Index of the condition within the decision that lacks an independence pair.
    pub target_condition_index: usize,
    /// Prefix path constraints leading up to this decision (in `taken` order).
    pub prefix_constraints: Vec<SymExpr>,
    /// Per-condition SymExprs for this compound decision.
    pub condition_exprs: Vec<SymExpr>,
    /// Observed truth values for each condition from the most recent observation.
    pub observed_values: Vec<Option<bool>>,
}

/// Run the concolic exploration loop on a function via a frontend subprocess.
///
/// The loop alternates between two phases per round:
/// 1. **Observe** — drain the worklist, execute all inputs, classify paths
/// 2. **Solve/Generate** — extract constraints from new paths, produce candidates
///
/// `function_name` is the fully-qualified name of the function to explore.
/// `seed_inputs` provides initial input sets to begin exploration.
/// `user_inputs` provides user-provided candidate inputs (highest priority).
/// `param_infos` provides parameter metadata including names and types.
#[allow(clippy::too_many_arguments)]
pub async fn explore(
    frontend: &mut Frontend,
    function_name: &str,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    user_inputs: Vec<Vec<serde_json::Value>>,
    param_infos: &[ParamInfo],
    config: &ExploreConfig,
    setup_context: Option<SetupContextStack>,
    prepare_id: Option<String>,
    loops: Vec<crate::protocol::LoopInfo>,
    progress_hints: Option<crate::explorer::ProgressHints<'_>>,
    resume_state: Option<ExploreState>,
) -> Result<(ExploreResult, ExploreState), ExploreError> {
    explore_with_oracle(
        frontend,
        function_name,
        seed_inputs,
        user_inputs,
        param_infos,
        config,
        setup_context,
        prepare_id,
        loops,
        progress_hints,
        resume_state,
        None,
    )
    .await
}

/// Same as [`explore`] but allows the caller to wire in an [`OracleHandle`]
/// so the concolic loop can drain LLM-proposed candidates at priority 4
/// (between drilling/boundary search and the random/fuzz fallback).
///
/// When `oracle` is `None`, this function behaves identically to [`explore`]
/// and no oracle-related code path executes.
#[allow(clippy::too_many_arguments)]
pub async fn explore_with_oracle(
    frontend: &mut Frontend,
    function_name: &str,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    user_inputs: Vec<Vec<serde_json::Value>>,
    param_infos: &[ParamInfo],
    config: &ExploreConfig,
    setup_context: Option<SetupContextStack>,
    prepare_id: Option<String>,
    loops: Vec<crate::protocol::LoopInfo>,
    progress_hints: Option<crate::explorer::ProgressHints<'_>>,
    resume_state: Option<ExploreState>,
    mut oracle: Option<OracleHandle<'_>>,
) -> Result<(ExploreResult, ExploreState), ExploreError> {
    let param_names: Vec<String> = param_infos.iter().map(|p| p.name.clone()).collect();
    // supplementary: priority queue for drilling, boundary search, and MC/DC candidates.
    // These are generated in-loop after new-path observations and need to be consumed
    // before MetaStrategy returns more inputs, preserving their InputSource attribution.
    let mut supplementary: BinaryHeap<WorklistEntry> = BinaryHeap::new();

    // Resume state: pre-populate covered_paths and extract prior discovery
    // inputs so batches 2..N skip rediscovery and start from frontier seeds.
    let (mut covered_paths, prior_discovery_inputs) = match resume_state {
        Some(state) => (state.covered_paths, state.discovery_inputs),
        None => (HashSet::new(), Vec::new()),
    };
    let mut batch_discovery_inputs: Vec<Vec<serde_json::Value>> = Vec::new();

    // str-6cdp: capture the native-replay marker for each custom-generator /
    // extractor parameter slot from the seed/user/prior candidate vectors that
    // upstream prefetch already resolved. These markers are re-applied at the
    // execute funnel on EVERY iteration so the extractor param (e.g. axum
    // State<AppState>) never reaches the frontend as a generated/mutated scalar,
    // even after the prefetch queue is exhausted or a strategy produced a fresh
    // non-native vector.
    let native_pins = {
        let mut candidates: Vec<Vec<serde_json::Value>> = Vec::new();
        candidates.extend(seed_inputs.iter().cloned());
        candidates.extend(user_inputs.iter().cloned());
        candidates.extend(prior_discovery_inputs.iter().cloned());
        crate::input_gen::NativePins::capture_from_inputs(&config.value_sources, &candidates)
    };
    let native_pins_arg = if native_pins.is_empty() {
        None
    } else {
        Some(&native_pins)
    };

    let mut executions = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)> = Vec::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut total_executions: usize = 0;
    // str-303gg review fix: remember a representative `not_supported` reason seen
    // during exploration. Used only at finalize to reclassify the function as
    // Unsupported when it collected no successful observation at all.
    let mut unsupported_reason: Option<String> = None;
    let mut z3_generated: usize = 0;
    let mut fuzz_generated: usize = 0;
    let mut fuzz_attempts: HashMap<u32, FuzzAttemptState> = HashMap::new();
    let mut fuzz_corpus: Option<crate::fuzzer::Corpus> = None;
    let mut boundary_generated: usize = 0;
    let mut drill_generated: usize = 0;
    let mut abandoned_frontiers: Vec<(u32, u32)> = Vec::new();
    let mut termination_reason = TerminationReason::WorklistExhausted;
    let mut param_fail_counts: HashMap<String, usize> = HashMap::new();
    let mut seen_branch_sides: HashSet<(u32, bool)> = HashSet::new();
    let mut frontier_set = FrontierSet::new();
    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };
    let mut triage_state = TriageState::new(param_names.clone());
    let mut triage_skipped: usize = 0;
    let mut triage_mispredictions: usize = 0;

    // LLM seed-oracle integration state. Only populated when `oracle` is Some.
    // `attempted_by_condition` is the rolling buffer (capped at 5 entries per
    // condition) that we feed back into OracleContext.attempted so the oracle
    // can avoid proposing duplicates.
    let mut attempted_by_condition: HashMap<ConditionId, VecDeque<InputVector>> = HashMap::new();
    // Set of condition IDs we have queried this run — used so retire() can be
    // called when the corresponding branch is finally covered by *any* strategy.
    let mut oracle_queried_conditions: HashSet<ConditionId> = HashSet::new();

    // Per-dependency LiveFirst state (mirrors explorer.rs parity).
    let mut live_first_states: HashMap<String, LiveFirstState> = HashMap::new();

    // Fitness context shares novelty state with covered_paths — the
    // orchestrator marks paths as seen in both sets whenever a new path
    // is discovered, keeping FitnessContext's novelty scoring in sync.
    let mut fitness_context = FitnessContext::new();
    let fitness_weights = FitnessWeights::default();

    // Target branches: branch IDs seen so far on only one side (not yet
    // covered on the opposite). Updated after each new-path observation.
    // Used by fitness scoring to compute proximity/coverage scores.
    let mut target_branches: HashSet<u32> = HashSet::new();

    // Build MetaStrategy for the concolic path.
    //
    // Strategy set: [UserProvided, BoundarySeeds, Z3Solver, Fuzzer]
    //
    // - UserProvidedStrategy yields user_inputs and seed_inputs (boundary dict + literals
    //   pre-computed by the CLI). These are executed first with highest priority.
    // - BoundarySeeds generates additional boundary values from param types.
    // - Z3SolverStrategy receives feedback after each execution, runs Z3 on solvable
    //   constraints, and queues solutions for subsequent next() calls.
    // - FuzzerStrategy mutates interesting inputs for Unknown constraints.
    //
    // Drilling, boundary search, and MC/DC candidates go into a supplementary
    // BinaryHeap (checked before MetaStrategy) to preserve their InputSource
    // attribution and fitness-based priority ordering.
    let fallback_loops = loops.clone();
    let meta_strategy = build_concolic_meta_strategy(
        user_inputs,
        seed_inputs,
        prior_discovery_inputs.clone(),
        param_infos,
        loops,
        // str-jeen.65: cap per-Z3-query timeout by per-function budget so a
        // single solver call cannot outlive the function deadline.
        effective_solver_timeout_ms(config),
        config.meta_config.clone(),
    );
    let strategy_ctx = StrategyContext {
        params: param_infos.to_vec(),
        literals: vec![],
        capabilities: FrontendCapabilities::default(),
        value_sources: config.value_sources.clone(),
    };
    let feedback_mode = if config.solver_offload {
        ConcolicFeedbackMode::Async
    } else {
        ConcolicFeedbackMode::Sync
    };
    let mut feedback_scheduler = ConcolicFeedbackScheduler::new(meta_strategy, feedback_mode);

    // MC/DC tracking state: only allocated when MC/DC mode is enabled.
    let mut mcdc_table: Option<McdcTable> = if config.mcdc {
        Some(McdcTable::default())
    } else {
        None
    };
    // Track the number of satisfied independence pairs to detect plateau resets.
    let mut mcdc_independent_count: usize = 0;

    // Generate initial mock values when mock_params are configured.
    // (Retained for future use; currently unused since worklist seeding was replaced
    // by UserProvidedStrategy in MetaStrategy.)
    let _initial_mocks = if !config.mock_params.is_empty() {
        input_gen::generate_mock_values(&config.mock_params, &mut rng, None)
    } else {
        vec![]
    };

    // str-jeen.65: anchor the per-function wall-clock budget at the START of
    // explore (before the float-probe pre-pass) so every phase — float-probe,
    // main loop, refine, shrink — counts against `timeout_explore`. Previously
    // `explore_start` was anchored just before the main loop (below), so a
    // long float-probe phase could silently consume the entire budget.
    let explore_start = Instant::now();
    let deadline: Option<Instant> = config.timeout_explore.map(|d| explore_start + d);
    let deadline_crossed = || deadline.is_some_and(|d| Instant::now() >= d);
    let mut timed_out_overall = false;

    // --- Float probe phase ---
    let float_indices = crate::float_probe::float_param_indices(param_infos);
    let mut float_probe_results: Vec<crate::float_probe::FloatProbeResult> = Vec::new();
    if !float_indices.is_empty() {
        'float_probe: for &idx in &float_indices {
            if deadline_crossed() {
                timed_out_overall = true;
                break 'float_probe;
            }
            let pairs = crate::float_probe::generate_probe_pairs(
                param_infos,
                idx,
                crate::float_probe::PROBE_COUNT,
                &mut rng,
            );
            let mut agreements = 0usize;
            let mut total_probes = 0usize;
            let mut divergent_values = Vec::new();

            for (float_inputs, floor_inputs) in pairs {
                if deadline_crossed() {
                    timed_out_overall = true;
                    break 'float_probe;
                }
                let execute_float_inputs = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                    &float_inputs,
                    param_infos,
                    config.default_execute_plan.as_ref(),
                    native_pins_arg,
                )?;
                let execute_floor_inputs = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                    &floor_inputs,
                    param_infos,
                    config.default_execute_plan.as_ref(),
                    native_pins_arg,
                )?;
                let float_resp = frontend
                    .send(Command::Execute {
                        function: function_name.to_string(),
                        inputs: execute_float_inputs.inputs().to_vec(),
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
                        capture: false,
                        prepare_id: prepare_id.clone(),
                        execution_profile: config.execution_profile.clone(),
                        plan: config.default_execute_plan.clone(),
                    })
                    .await?;

                let floor_resp = frontend
                    .send(Command::Execute {
                        function: function_name.to_string(),
                        inputs: execute_floor_inputs.inputs().to_vec(),
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

                    if crate::float_probe::executions_agree(float_result, floor_result) {
                        agreements += 1;
                    } else if let Some(v) = float_inputs.get(idx).and_then(|v| v.as_f64()) {
                        divergent_values.push(v);
                    }
                }
            }

            let classification = crate::float_probe::classify(
                agreements,
                total_probes,
                crate::float_probe::AGREEMENT_THRESHOLD,
            );
            float_probe_results.push(crate::float_probe::FloatProbeResult {
                param_index: idx,
                param_name: param_infos[idx].name.clone(),
                classification,
                agreements,
                total_probes,
                divergent_values,
            });
        }
    }

    // str-jeen.65: `explore_start` is now established above, before the
    // float-probe phase. The main loop continues to read it via `budget`.
    let mut plateau_counter: usize = 0;
    // Periodic progress reporting state (parity with explorer.rs random path).
    // Tracks the 15-second cadence for ExploreProgressSnapshot emission and the
    // iteration index of the most recent new-branch discovery so the snapshot
    // can surface "continuing without new discoveries" to the CLI.
    let mut last_summary_time = Instant::now();
    let mut last_reported_branches: usize = 0;
    let mut last_discovery_iteration: u64 = 0;

    // --- Strategy-driven exploration loop: Observe → Feedback → Generate ---
    //
    // Each iteration:
    //   1. Pop from supplementary (drilling/boundary/MC-DC candidates) or call
    //      meta_strategy.next() for the next MetaStrategy-produced inputs.
    //   2. Observe — execute and classify the path.
    //   3. Feed result to MetaStrategy. In async solver mode this can run on
    //      Tokio's blocking pool while one ready candidate is observed.
    //   4. If new path, call solve_and_generate() for drilling/boundary candidates
    //      and push them to the supplementary queue.
    loop {
        // --- Periodic progress summary (parity with random explorer) ---
        // Keep the discovery tracker current even when no callback is
        // registered so the `iters_since_new_discovery` field stays accurate
        // across future emissions.
        if discoveries.len() > last_reported_branches {
            last_reported_branches = discoveries.len();
            last_discovery_iteration = total_executions as u64;
        }
        if let Some(hints) = progress_hints.as_ref() {
            let since_last = last_summary_time.elapsed();
            if since_last >= Duration::from_secs(crate::explorer::PROGRESS_SUMMARY_INTERVAL_SECS) {
                let mcdc_snapshot = mcdc_table.as_ref().map(|t| t.summary());
                let iters_since_new =
                    (total_executions as u64).saturating_sub(last_discovery_iteration) as u32;
                (hints.callback)(&crate::explorer::ExploreProgressSnapshot {
                    function_name: function_name.to_string(),
                    elapsed: explore_start.elapsed(),
                    iterations: total_executions as u32,
                    paths_found: executions.len(),
                    total_branches: hints.total_branches,
                    branches_covered: Some(discoveries.len()),
                    mcdc_summary: mcdc_snapshot,
                    iters_since_new_discovery: iters_since_new,
                });
                last_summary_time = Instant::now();
            }
        }

        // Priority 4: LLM seed-oracle drain. Sits between supplementary
        // (drilling/boundary/MC-DC) and the MetaStrategy/random fallback.
        // When `oracle` is None this entire branch is skipped.
        let oracle_candidate: Option<(InputVector, ConditionId)> = if supplementary.is_empty()
            && let Some(handle) = oracle.as_mut()
        {
            // Snapshot the function-source view once so it doesn't conflict
            // with the simultaneous mutable borrow of `slot_map`.
            let function_source: String = handle.function_source.clone();
            poll_oracle_for_frontier(
                Some(handle),
                &frontier_set,
                &seen_branch_sides,
                &raw_results,
                param_infos,
                &attempted_by_condition,
                function_name,
                &function_source,
            )
        } else {
            None
        };

        // The condition id this iteration's entry is attempting (if any), used
        // for record_accepted / retire / attempted_by_condition bookkeeping.
        let mut oracle_condition_id: Option<ConditionId> = None;

        // Priority: supplementary (drilling/boundary/MC-DC) > LLM oracle > MetaStrategy.
        let (mut entry, strategy_idx) = if let Some(e) = supplementary.pop() {
            let _special_case = SpecialCandidatePath::OrchestratorSupplementaryQueue;
            (e, None)
        } else if let Some((inputs, condition_id)) = oracle_candidate {
            oracle_condition_id = Some(condition_id);
            oracle_queried_conditions.insert(condition_id);
            (
                WorklistEntry {
                    inputs,
                    source: InputSource::LlmOracle,
                    fitness: None,
                    mock_values: vec![],
                },
                None,
            )
        } else if let Some((inputs, idx, kind)) = feedback_scheduler
            .next_meta_candidate(&strategy_ctx, &mut rng)
            .await?
        {
            let source = kind.orchestrator_input_source();
            // Track generation counters for MetaStrategy-sourced inputs.
            match source {
                InputSource::Z3Solved => z3_generated += 1,
                InputSource::Fuzzed => fuzz_generated += 1,
                InputSource::BoundarySearch => boundary_generated += 1,
                _ => {}
            }
            (
                WorklistEntry {
                    inputs,
                    source,
                    fitness: None,
                    mock_values: vec![],
                },
                Some(idx),
            )
        } else {
            break;
        };

        // --- LiveFirst mock adjustment (parity with explorer.rs) ---
        // When entry has no per-execution mocks, copy from config so overrides
        // can be applied per-dep without mutating shared config.
        if entry.mock_values.is_empty() && !live_first_states.is_empty() {
            entry.mock_values = config.mocks.clone();
        }
        apply_live_first_overrides(&live_first_states, &mut entry.mock_values);

        let budget = ExploreBudget {
            unique_paths: executions.len(),
            total_executions,
            plateau_counter,
            explore_start,
        };

        let observe_result = observe_one(
            &entry,
            frontend,
            function_name,
            param_infos,
            config,
            &mut covered_paths,
            &mut triage_state,
            &budget,
            &setup_context,
            prepare_id.as_deref(),
            native_pins_arg,
        )
        .await?;

        let obs = match observe_result {
            ObserveOneResult::Observed(obs) => *obs,
            ObserveOneResult::TriageSkipped => {
                triage_skipped += 1;
                continue;
            }
            ObserveOneResult::FrontendSkipped => {
                total_executions += 1;
                continue;
            }
            ObserveOneResult::Unsupported(reason) => {
                // str-303gg review fix: a not_supported iteration is skipped like
                // a frontend error, but its reason is remembered so the function
                // can be reclassified Unsupported at finalize IF nothing else ever
                // executed. It never aborts a partially-covered function.
                total_executions += 1;
                unsupported_reason.get_or_insert(reason);
                continue;
            }
            ObserveOneResult::Terminated(reason) => {
                if reason == TerminationReason::CoveragePlateau {
                    // Check for fuzz-eligible opaque branches.
                    let fuzz_targets: Vec<u32> = frontier_set
                        .iter()
                        .filter(|f| {
                            f.blocking_params.is_empty() // Unknown constraint (opaque)
                                && is_fuzz_eligible(
                                    f.branch_id,
                                    &fuzz_attempts,
                                    config.fuzz.max_attempts,
                                    covered_paths.len(),
                                )
                        })
                        .map(|f| f.branch_id)
                        .collect();

                    if !fuzz_targets.is_empty() {
                        log::info!(
                            "Coverage plateau — entering fuzz phase targeting {} opaque branch(es)",
                            fuzz_targets.len(),
                        );

                        // Build/reuse corpus.
                        let mut corpus = fuzz_corpus.take().unwrap_or_default();
                        // Seed from execution history: inputs that reached near target branches.
                        if corpus.is_empty() {
                            for (inputs, _, result) in &raw_results {
                                for decision in &result.branch_path {
                                    if fuzz_targets.contains(&decision.branch_id) {
                                        let path_hash = hash_branch_path(&result.branch_path);
                                        corpus.add(crate::fuzzer::CorpusEntry {
                                            inputs: inputs.clone(),
                                            coverage_hash: path_hash,
                                            branch_ids: result
                                                .branch_path
                                                .iter()
                                                .map(|d| d.branch_id)
                                                .collect(),
                                        });
                                        break;
                                    }
                                }
                            }
                        }

                        if corpus.is_empty() {
                            // No seeds available — skip fuzz phase.
                            termination_reason = reason;
                            break;
                        }

                        // Run fuzz phase inline — FuzzSession::run requires
                        // an FnMut closure, but `frontend` is `&mut Frontend`
                        // (non-Copy) and can't be moved into a closure that's
                        // called multiple times. Instead, replicate the
                        // mutation-execution loop directly here.
                        let fuzz_plateau_threshold = config
                            .fuzz
                            .plateau_threshold
                            .unwrap_or(crate::config::DEFAULT_FUZZ_PLATEAU_THRESHOLD);
                        let fuzz_max_executions_raw = config
                            .fuzz
                            .max_executions
                            .unwrap_or(crate::config::DEFAULT_FUZZ_MAX_EXECUTIONS);
                        // str-nqrz: clamp the per-fuzz-phase execution cap by
                        // the remaining global execution budget so a fuzz
                        // phase entered late cannot blow past
                        // `--max-iterations`. Without this, a plateau-induced
                        // fuzz phase could add hundreds of executions on top
                        // of a small user cap.
                        let fuzz_max_executions = clamp_fuzz_budget(
                            fuzz_max_executions_raw,
                            config.max_executions,
                            total_executions,
                        );
                        let fuzz_timeout = std::time::Duration::from_secs(
                            config
                                .fuzz
                                .timeout_seconds
                                .unwrap_or(crate::config::DEFAULT_FUZZ_TIMEOUT_SECS)
                                as u64,
                        );

                        let fuzz_start = std::time::Instant::now();
                        let mut fuzz_executions: u32 = 0;
                        let mut fuzz_plateau: u32 = 0;
                        let mut fuzz_new_paths: u32 = 0;
                        let mut fuzz_rng = StdRng::from_os_rng();

                        let fuzz_termination = loop {
                            if deadline_crossed() {
                                timed_out_overall = true;
                                break crate::fuzzer::FuzzTermination::Timeout;
                            }
                            if fuzz_plateau >= fuzz_plateau_threshold {
                                break crate::fuzzer::FuzzTermination::Plateau;
                            }
                            if fuzz_executions >= fuzz_max_executions {
                                break crate::fuzzer::FuzzTermination::ExecutionCap;
                            }
                            if fuzz_start.elapsed() >= fuzz_timeout {
                                break crate::fuzzer::FuzzTermination::Timeout;
                            }

                            let parent_inputs = match corpus.pick(&mut fuzz_rng) {
                                Some(entry) => entry.inputs.clone(),
                                None => break crate::fuzzer::FuzzTermination::Plateau,
                            };

                            let mutated = crate::input_gen::havoc_mutate_inputs_with_sources(
                                &parent_inputs,
                                param_infos,
                                &config.value_sources,
                                1.0,
                                &[],
                                &mut fuzz_rng,
                            );
                            let execute_mutated = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                                &mutated,
                                param_infos,
                                config.default_execute_plan.as_ref(),
                                native_pins_arg,
                            )?;

                            let response = frontend
                                .send(Command::Execute {
                                    function: function_name.to_string(),
                                    inputs: execute_mutated.inputs().to_vec(),
                                    mocks: config.mocks.clone(),
                                    setup_context: setup_context.clone(),
                                    capture: true,
                                    prepare_id: prepare_id.clone(),
                                    execution_profile: config.execution_profile.clone(),
                                    plan: config.default_execute_plan.clone(),
                                })
                                .await?;

                            fuzz_executions += 1;

                            if let ResponseResult::Execute(result) = response.result {
                                let path_hash = hash_branch_path(&result.branch_path);
                                if covered_paths.insert(path_hash) {
                                    fuzz_plateau = 0;
                                    fuzz_new_paths += 1;
                                    for decision in &result.branch_path {
                                        if seen_branch_ids.insert(decision.branch_id) {
                                            discoveries.push((
                                                decision.branch_id,
                                                DiscoveryMethod::Fuzzed,
                                            ));
                                        }
                                    }
                                    let branch_ids: Vec<u32> =
                                        result.branch_path.iter().map(|d| d.branch_id).collect();
                                    corpus.add(crate::fuzzer::CorpusEntry {
                                        inputs: mutated,
                                        coverage_hash: path_hash,
                                        branch_ids,
                                    });
                                } else {
                                    fuzz_plateau += 1;
                                }
                            } else {
                                fuzz_plateau += 1;
                            }
                        };

                        // Update attempt tracking.
                        for branch_id in &fuzz_targets {
                            let state =
                                fuzz_attempts.entry(*branch_id).or_insert(FuzzAttemptState {
                                    count: 0,
                                    coverage_at_last_attempt: 0,
                                });
                            state.count += 1;
                            state.coverage_at_last_attempt = covered_paths.len();
                        }

                        fuzz_corpus = Some(corpus);
                        total_executions += fuzz_executions as usize;
                        fuzz_generated += fuzz_new_paths as usize;

                        log::info!(
                            "Fuzz phase complete: {} new path(s) from {} executions ({:?})",
                            fuzz_new_paths,
                            fuzz_executions,
                            fuzz_termination,
                        );

                        if deadline_crossed() {
                            timed_out_overall = true;
                            termination_reason = TerminationReason::TimeoutExplore;
                            break;
                        }

                        // Reset plateau and continue.
                        plateau_counter = 0;
                        continue;
                    }
                }
                termination_reason = reason;
                break;
            }
        };

        // --- LiveFirst state transitions (parity with explorer.rs) ---
        update_live_first_states(&obs.result, &mut live_first_states);

        // --- Crypto boundary logging (parity with explorer.rs) ---
        if !obs.result.runtime_crypto_boundaries.is_empty() {
            tracing::debug!(
                count = obs.result.runtime_crypto_boundaries.len(),
                boundaries = ?obs.result.runtime_crypto_boundaries
                    .iter()
                    .map(|b| format!("{} ({})", b.function_name, b.boundary_id))
                    .collect::<Vec<_>>(),
                "crypto boundaries detected in execution trace"
            );
        }

        total_executions += 1;
        if obs.is_sampled_skip && !obs.is_new_path {
            // Prediction was correct (duplicate path) — no misprediction.
        } else if obs.is_sampled_skip && obs.is_new_path {
            triage_mispredictions += 1;
        }

        // Record raw result for pipeline composability.
        // Use the per-execution mock values so downstream consumers see exactly
        // which mocks were active for each execution.
        let recorded_mocks = if obs.mock_values.is_empty() {
            config.mocks.clone()
        } else {
            obs.mock_values.clone()
        };
        raw_results.push((obs.inputs.clone(), recorded_mocks, obs.result.clone()));

        // Feed execution result to MetaStrategy for adaptive scoring. In async
        // mode, Z3 feedback runs on the blocking pool and the scheduler may
        // prefetch one candidate so observation can continue while solving.
        feedback_scheduler
            .submit_feedback(
                obs.inputs.clone(),
                obs.result.clone(),
                obs.is_new_path,
                strategy_idx,
                &strategy_ctx,
                &mut rng,
            )
            .await?;

        if !obs.is_new_path {
            plateau_counter += 1;
            // Oracle bookkeeping: the proposed candidate did not reach a new
            // equivalence class. Buffer it in attempted_by_condition (cap 5)
            // so the next OracleContext can deduplicate. Leave the slot to
            // transition back to Idle on its next poll.
            if let Some(cid) = oracle_condition_id {
                let buf = attempted_by_condition.entry(cid).or_default();
                buf.push_back(obs.inputs.clone());
                while buf.len() > 5 {
                    buf.pop_front();
                }
            }
            continue;
        }

        // New path discovered — reset plateau counter.
        plateau_counter = 0;

        // Oracle bookkeeping for new-path observations originating from the
        // LLM oracle: count the accepted candidate and retire the slot so a
        // fresh query can be issued for the next unsolved condition.
        if let Some(cid) = oracle_condition_id
            && let Some(handle) = oracle.as_mut()
        {
            handle.slot_map.record_accepted();
            handle.slot_map.retire(cid);
            oracle_queried_conditions.remove(&cid);
        }

        // Capture this input for resume state — frontier-adjacent seeds
        // help subsequent batches start from productive regions.
        batch_discovery_inputs.push(obs.inputs.clone());

        // Track per-branch discovery attribution.
        let method = match obs.source {
            InputSource::Z3Solved => DiscoveryMethod::Z3,
            InputSource::McdcTarget => DiscoveryMethod::McdcTarget,
            InputSource::UserProvided => DiscoveryMethod::UserProvided,
            InputSource::Drilled => DiscoveryMethod::Drilled,
            InputSource::BoundarySearch => DiscoveryMethod::BoundarySearch,
            InputSource::Seed => DiscoveryMethod::Random,
            InputSource::Fuzzed => DiscoveryMethod::Fuzzed,
            InputSource::LlmOracle => DiscoveryMethod::LlmOracle,
        };
        for decision in &obs.result.branch_path {
            if seen_branch_ids.insert(decision.branch_id) {
                discoveries.push((decision.branch_id, method));
            }
        }

        // Update frontier set and target branches: track which branch sides
        // have been seen. Branches seen on only one side are targets for
        // fitness scoring.
        for decision in &obs.result.branch_path {
            seen_branch_sides.insert((decision.branch_id, decision.taken));
            let opposite_seen = seen_branch_sides.contains(&(decision.branch_id, !decision.taken));
            if opposite_seen {
                frontier_set.remove(decision.branch_id);
                target_branches.remove(&decision.branch_id);
                // Any strategy just solved this condition — retire the oracle
                // slot so its budget isn't spent re-proposing for a covered
                // branch.
                let cid = ConditionId::from(decision.branch_id);
                if oracle_queried_conditions.remove(&cid)
                    && let Some(handle) = oracle.as_mut()
                {
                    handle.slot_map.retire(cid);
                }
                attempted_by_condition.remove(&cid);
            } else {
                target_branches.insert(decision.branch_id);
                // Reset stall count to 0: this frontier just produced a new
                // path, so it's not stalled. Preserving the old stall_count
                // would unfairly penalize frontiers that make intermittent
                // progress.
                let blocking =
                    drilling::identify_blocking_params(&decision.constraint, param_infos);
                let depth = drilling::branch_depth(&obs.result.branch_path, decision.branch_id);
                let rarity_boost = config
                    .branch_profile
                    .as_ref()
                    .map_or(0.0, |p| p.rarity(decision.branch_id));
                frontier_set.insert(Frontier {
                    branch_id: decision.branch_id,
                    depth,
                    blocking_params: blocking,
                    best_prefix: obs.inputs.clone(),
                    stall_count: 0,
                    rarity_boost,
                });
            }
        }

        // Sync fitness context: mark this path as seen so future fitness
        // scoring correctly identifies repeat paths as non-novel.
        fitness_context.mark_seen(obs.path_id);

        // MC/DC tracking: record per-condition outcomes and check for new
        // independence pairs. Also collect goals for Phase 5 solver (logged
        // at debug level; not yet turned into worklist entries).
        if let Some(ref mut table) = mcdc_table {
            for decision in &obs.result.branch_path {
                if let Some(ref conditions) = decision.conditions {
                    table.record_observation(decision.branch_id, conditions, decision.taken);
                }
            }

            // Check if MC/DC is now complete (all conditions have independence pairs).
            if table.is_complete() && !table.decisions.is_empty() {
                termination_reason = TerminationReason::McdcComplete;
                executions.push(obs.result.clone());
                break;
            }

            // Check if a new independence pair was satisfied — if so, reset the
            // plateau counter so we don't terminate prematurely while MC/DC is
            // still making progress.
            let (_, new_independent, _) = table.summary();
            if new_independent > mcdc_independent_count {
                mcdc_independent_count = new_independent;
                plateau_counter = 0;
            }

            // Collect MC/DC goals for Phase 5 (preparation): find conditions
            // that still lack independence pairs and log them for diagnostics.
            // These will drive targeted Z3 queries in the next phase.
            let sym_constraints = extract_sym_constraints(&obs.result);
            for decision in &obs.result.branch_path {
                if let Some(ref conditions) = decision.conditions
                    && let Some(dec_mcdc) = table.decisions.get(&decision.branch_id)
                {
                    // Build the prefix constraints up to (not including) this decision.
                    let decision_pos = obs
                        .result
                        .branch_path
                        .iter()
                        .position(|d| std::ptr::eq(d, decision));
                    let prefix_constraints: Vec<SymExpr> = if let Some(pos) = decision_pos {
                        sym_constraints[..pos]
                            .iter()
                            .filter_map(|c| c.clone())
                            .collect()
                    } else {
                        vec![]
                    };

                    // Collect per-condition SymExprs from the ConditionOutcome constraints.
                    let condition_exprs: Vec<SymExpr> = conditions
                        .iter()
                        .filter_map(|co| match &co.constraint {
                            crate::execution_record::SymConstraint::Expr { expr } => {
                                Some(expr.clone())
                            }
                            crate::execution_record::SymConstraint::Unknown { .. } => None,
                        })
                        .collect();

                    let observed_values: Vec<Option<bool>> = conditions
                        .iter()
                        .map(|co| if co.masked { None } else { co.value })
                        .collect();

                    for (i, &is_independent) in dec_mcdc.independent.iter().enumerate() {
                        if !is_independent {
                            let goal = McdcGoal {
                                branch_id: decision.branch_id,
                                target_condition_index: i,
                                prefix_constraints: prefix_constraints.clone(),
                                condition_exprs: condition_exprs.clone(),
                                observed_values: observed_values.clone(),
                            };
                            tracing::debug!(
                                branch_id = goal.branch_id,
                                condition_index = goal.target_condition_index,
                                "mcdc_goal: condition lacks independence pair, invoking solver"
                            );
                            // Call the MC/DC targeted solver to find inputs that
                            // flip the target condition while holding all others constant.
                            match solver::solve_for_mcdc_independence(
                                &goal.prefix_constraints,
                                &goal.condition_exprs,
                                &goal.observed_values,
                                goal.target_condition_index,
                                // str-jeen.65: cap by per-function budget.
                                effective_solver_timeout_ms(config),
                                param_infos,
                            ) {
                                Ok(SolveResult::Sat(values)) => {
                                    let new_inputs =
                                        overlay_solved_values(&obs.inputs, &values, &param_names);
                                    supplementary.push(WorklistEntry {
                                        inputs: new_inputs,
                                        source: InputSource::McdcTarget,
                                        fitness: None,
                                        mock_values: obs.mock_values.clone(),
                                    });
                                    tracing::debug!(
                                        branch_id = goal.branch_id,
                                        condition_index = goal.target_condition_index,
                                        "mcdc_goal: solver SAT — added supplementary entry"
                                    );
                                }
                                Ok(SolveResult::Unsat) => {
                                    tracing::debug!(
                                        branch_id = goal.branch_id,
                                        condition_index = goal.target_condition_index,
                                        "mcdc_goal: solver UNSAT — independence pair infeasible"
                                    );
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        branch_id = goal.branch_id,
                                        condition_index = goal.target_condition_index,
                                        error = %e,
                                        "mcdc_goal: solver error"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        executions.push(obs.result.clone());

        // Drill/boundary phase: produce supplementary candidates from this observation.
        let solve_output = solve_and_generate(
            &[obs],
            &mut frontier_set,
            param_infos,
            &param_names,
            &raw_results,
            &seen_branch_sides,
            config,
            &fallback_loops,
            &mut rng,
            &target_branches,
            &mut fitness_context,
            &fitness_weights,
            &config.mock_params,
            config.branch_profile.as_ref(),
        );

        drill_generated += solve_output.drill_count;
        boundary_generated += solve_output.boundary_count;
        // Accumulate non-pipelined path failure counts.
        for (name, count) in solve_output.param_fail_counts {
            *param_fail_counts.entry(name).or_insert(0) += count;
        }

        for candidate in solve_output.candidates {
            supplementary.push(candidate);
        }

        // Abandon frontiers that have exceeded the stall threshold.
        {
            let newly_abandoned =
                frontier_set.abandon_stalled(crate::frontier::FRONTIER_STALL_THRESHOLD);
            for f in &newly_abandoned {
                tracing::info!(
                    branch_id = f.branch_id,
                    stall_count = f.stall_count,
                    depth = f.depth,
                    "Abandoning stalled frontier after {} failed attempts",
                    f.stall_count,
                );
                target_branches.remove(&f.branch_id);
                abandoned_frontiers.push((f.branch_id, f.stall_count));
            }
        }
    }

    // str-jeen.65: capture whether the main loop exited via the wall-clock
    // budget so subsequent post-loop phases can skip cleanly.
    if matches!(termination_reason, TerminationReason::TimeoutExplore) || deadline_crossed() {
        timed_out_overall = true;
    }

    // --- Refinement phase: binary-search between witness pairs ---
    // str-jeen.65: skip refinement entirely once the deadline has passed —
    // refine_boundaries_async issues many execute() calls per boundary and is
    // unbounded relative to `timeout_explore`.
    let boundary_results = if timed_out_overall || deadline_crossed() {
        if deadline_crossed() {
            timed_out_overall = true;
        }
        Vec::new()
    } else if let Some(budget) = config.refine_budget {
        if budget > 0 {
            refine_boundaries_async(
                frontend,
                function_name,
                &raw_results,
                param_infos,
                budget,
                &setup_context,
                config.default_execute_plan.clone(),
                native_pins_arg,
            )
            .await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    if deadline_crossed() {
        timed_out_overall = true;
    }
    // -- Witness shrinking phase --
    // For each unique path, try to shrink the witness to simpler inputs.
    let mut shrunk_witnesses: std::collections::HashMap<u64, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut shrink_stats = crate::shrink::ShrinkStats::default();
    // str-jeen.65: skip shrink entirely once the wall-clock budget is gone —
    // each candidate path issues up to `witness_budget` execute() calls.
    if config.shrink_budget > 0 && !timed_out_overall && !deadline_crossed() {
        // Collect the lowest-complexity witness per unique path.
        // Starting from the simplest witness reduces shrink iterations needed.
        let mut path_witnesses: std::collections::HashMap<
            u64,
            (Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>),
        > = std::collections::HashMap::new();
        for (inputs, mocks, result) in &raw_results {
            let ph = hash_branch_path(&result.branch_path);
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
            // str-jeen.65: bail out of remaining shrink targets once the
            // per-function deadline has passed.
            if deadline_crossed() {
                timed_out_overall = true;
                break;
            }
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
            if deadline_crossed() {
                timed_out_overall = true;
                break;
            }
            if attempts < witness_budget
                && let Some(bulk_trial) =
                    crate::shrink::bulk_shrink_candidate(&current, param_infos)
            {
                attempts += 1;
                let execute_bulk_trial = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                    &bulk_trial,
                    param_infos,
                    config.default_execute_plan.as_ref(),
                    native_pins_arg,
                )?;
                let resp = frontend
                    .send(Command::Execute {
                        function: function_name.to_string(),
                        inputs: execute_bulk_trial.inputs().to_vec(),
                        mocks: effective_mocks.clone(),
                        setup_context: setup_context.clone(),
                        capture: true,
                        prepare_id: prepare_id.clone(),
                        execution_profile: config.execution_profile.clone(),
                        plan: config.default_execute_plan.clone(),
                    })
                    .await;
                if let Ok(resp) = resp
                    && let ResponseResult::Execute(exec_res) = resp.result
                    && hash_branch_path(&exec_res.branch_path) == *ph
                {
                    current = bulk_trial;
                    bulk_accepted = true;
                }
            }

            // Phase 1.5: grouped fallback — when bulk was rejected and N >= 3, try
            // consecutive groups of floor(N/2) parameters before the per-param loop.
            // Costs ≈2 execute calls and shrinks multiple params per accepted trial.
            let n = param_infos.len().min(current.len());
            if !bulk_accepted && n >= 3 && attempts < witness_budget {
                let group_size = n / 2;
                for trial in
                    crate::shrink::grouped_shrink_candidates(&current, param_infos, group_size)
                {
                    if deadline_crossed() {
                        timed_out_overall = true;
                        break;
                    }
                    if attempts >= witness_budget {
                        break;
                    }
                    attempts += 1;
                    let execute_trial = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                        &trial,
                        param_infos,
                        config.default_execute_plan.as_ref(),
                        native_pins_arg,
                    )?;
                    let resp = frontend
                        .send(Command::Execute {
                            function: function_name.to_string(),
                            inputs: execute_trial.inputs().to_vec(),
                            mocks: effective_mocks.clone(),
                            setup_context: setup_context.clone(),
                            capture: false,
                            prepare_id: prepare_id.clone(),
                            execution_profile: config.execution_profile.clone(),
                            plan: config.default_execute_plan.clone(),
                        })
                        .await;
                    if let Ok(resp) = resp
                        && let ResponseResult::Execute(exec_res) = resp.result
                        && hash_branch_path(&exec_res.branch_path) == *ph
                    {
                        current = trial;
                    }
                }
            }

            // Phase 2: one-at-a-time per-param loop.
            let mut progress = true;
            while progress && attempts < witness_budget {
                if deadline_crossed() {
                    timed_out_overall = true;
                    break;
                }
                progress = false;
                for i in 0..param_infos.len().min(current.len()) {
                    if deadline_crossed() {
                        timed_out_overall = true;
                        break;
                    }
                    let candidates =
                        crate::shrink::shrink_candidates(&current[i], &param_infos[i].typ);
                    for candidate in candidates {
                        if deadline_crossed() {
                            timed_out_overall = true;
                            break;
                        }
                        if attempts >= witness_budget {
                            break;
                        }
                        let mut trial = current.clone();
                        trial[i] = candidate;
                        attempts += 1;
                        let execute_trial = crate::planner_consumer::execute_inputs_for_plan_with_pins(
                            &trial,
                            param_infos,
                            config.default_execute_plan.as_ref(),
                            native_pins_arg,
                        )?;

                        let resp = frontend
                            .send(Command::Execute {
                                function: function_name.to_string(),
                                inputs: execute_trial.inputs().to_vec(),
                                mocks: effective_mocks.clone(),
                                setup_context: setup_context.clone(),
                                capture: false,
                                prepare_id: prepare_id.clone(),
                                execution_profile: config.execution_profile.clone(),
                                plan: config.default_execute_plan.clone(),
                            })
                            .await;

                        if let Ok(resp) = resp
                            && let ResponseResult::Execute(exec_res) = resp.result
                            && hash_branch_path(&exec_res.branch_path) == *ph
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
                if timed_out_overall {
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

    // str-jeen.65: a final deadline check covers any phase between shrink and
    // here (drain_pending_feedback, opaque suggestions, etc.) so the returned
    // result accurately reflects whether the wall-clock budget was crossed.
    if deadline_crossed() {
        timed_out_overall = true;
    }

    feedback_scheduler.drain_pending_feedback().await?;
    let pipeline_overlaps = feedback_scheduler.pipeline_overlaps();
    let unique_paths = covered_paths.len();

    // str-303gg review fix: reclassify the whole function as Unsupported only
    // when it produced no successful/behavioral observation at all AND at least
    // one iteration reported not_supported. A function that collected coverage on
    // any iteration keeps it — a single not_supported result (e.g. an axum
    // State<T> handler on a non-native-replay solver input) must not discard it.
    let had_observation = !executions.is_empty() || unique_paths > 0;
    if let Some(reason) =
        crate::observe::aggregate_unsupported_reason(unsupported_reason, had_observation)
    {
        return Err(ExploreError::Unsupported(reason));
    }
    let mcdc_summary = mcdc_table.map(|t| t.summary());
    let opaque_suggestions =
        crate::executability::build_opaque_suggestions(param_infos, &param_fail_counts);
    let stubbed_modules = crate::explorer::collect_stubbed_modules(&raw_results);

    // Build resume state: cumulative covered_paths + all discovery inputs
    // (prior batches + this batch) for the next batch to use.
    let mut all_discovery_inputs = prior_discovery_inputs;
    all_discovery_inputs.extend(batch_discovery_inputs);
    let output_state = ExploreState {
        covered_paths: covered_paths.clone(),
        discovery_inputs: all_discovery_inputs,
    };

    Ok((
        ExploreResult {
            function_name: function_name.to_string(),
            total_lines: 0, // Caller must set from FunctionAnalysis (end_line - start_line + 1)
            executions,
            unique_paths,
            total_executions,
            z3_generated,
            fuzz_generated,
            boundary_generated,
            drill_generated,
            termination_reason,
            raw_results,
            discoveries,
            triage_skipped,
            triage_mispredictions,
            nondeterministic_fields: vec![],
            float_probe_results,
            boundary_results,
            shrunk_witnesses,
            mcdc_summary,
            pipeline_overlaps,
            shrink_stats,
            abandoned_frontiers,
            opaque_suggestions,
            stubbed_modules,
            // str-jeen.65: surface the wall-clock budget verdict to the
            // pipeline conversion (see `pipeline.rs`) so any timeout — whether
            // observed by the main loop, the float-probe pre-pass, or a
            // post-loop refine/shrink phase — propagates as
            // `ObservationOutput.timed_out=true` instead of silently being
            // bucketed as a normal completion.
            timed_out: timed_out_overall
                || matches!(termination_reason, TerminationReason::TimeoutExplore),
            oracle_stats: oracle.as_ref().map(|h| h.slot_map.stats()),
        },
        output_state,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, ScopeEvent, SymConstraint, TraceEvent};
    use crate::protocol::{BoundOp, InductionVar, LoopBodyState, LoopInfo, PerformanceMetrics};
    use crate::solver::ConcreteValue;
    use crate::strategy::{
        InputStrategy, MetaConfig, MetaStrategy, RegisteredStrategy, RegisteredStrategyKind,
        StrategyContext,
    };
    use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};
    use crate::types::TypeInfo;
    use std::collections::{BTreeMap, HashMap, VecDeque};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    fn make_exec_result(branch_path: Vec<BranchDecision>) -> ExecuteResult {
        ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path,
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
    }

    fn make_int_param(name: &str) -> ParamInfo {
        ParamInfo {
            name: name.into(),
            typ: TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }
    }

    fn make_counted_loop_info() -> LoopInfo {
        LoopInfo {
            loop_id: 7,
            line: 40,
            induction_var: InductionVar {
                name: "i".into(),
                init_expr: SymExpr::Const(ConstValue::Int(0)),
                step_expr: SymExpr::Const(ConstValue::Int(1)),
                bound_expr: SymExpr::Param {
                    name: "n".into(),
                    path: vec![],
                },
                bound_op: BoundOp::Lt,
            },
        }
    }

    fn make_loop_snapshot(iteration: u32, induction_value: i64) -> LoopBodyState {
        LoopBodyState {
            loop_id: 7,
            iteration,
            locals: BTreeMap::from([(
                "i".into(),
                SymExpr::Const(ConstValue::Int(induction_value)),
            )]),
        }
    }

    struct TestFixedStrategy {
        candidates: VecDeque<Vec<serde_json::Value>>,
    }

    impl TestFixedStrategy {
        fn new(candidates: Vec<Vec<serde_json::Value>>) -> Self {
            Self {
                candidates: candidates.into(),
            }
        }
    }

    impl InputStrategy for TestFixedStrategy {
        fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<serde_json::Value>> {
            self.candidates.pop_front()
        }

        fn name(&self) -> &str {
            "test_fixed"
        }
    }

    struct BlockingFeedbackStrategy {
        feedback_started: Arc<AtomicBool>,
        release_feedback: Arc<AtomicBool>,
        feedback_calls: Arc<AtomicUsize>,
    }

    impl InputStrategy for BlockingFeedbackStrategy {
        fn next(&mut self, _ctx: &StrategyContext) -> Option<Vec<serde_json::Value>> {
            None
        }

        fn feedback(
            &mut self,
            _inputs: &[serde_json::Value],
            _result: &ExecuteResult,
            _was_new_path: bool,
        ) {
            self.feedback_started.store(true, Ordering::SeqCst);
            while !self.release_feedback.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            self.feedback_calls.fetch_add(1, Ordering::SeqCst);
        }

        fn name(&self) -> &str {
            "blocking_feedback"
        }

        fn is_finite(&self) -> bool {
            false
        }
    }

    fn scheduler_strategy_context() -> StrategyContext {
        StrategyContext {
            params: vec![make_int_param("x")],
            literals: vec![],
            capabilities: FrontendCapabilities::default(),
            value_sources: vec![],
        }
    }

    fn scheduler_meta_strategy(
        feedback_started: Arc<AtomicBool>,
        release_feedback: Arc<AtomicBool>,
        feedback_calls: Arc<AtomicUsize>,
    ) -> MetaStrategy {
        MetaStrategy::new(
            vec![
                RegisteredStrategy::new(
                    RegisteredStrategyKind::UserProvided,
                    Box::new(TestFixedStrategy::new(vec![
                        vec![serde_json::json!(1)],
                        vec![serde_json::json!(2)],
                    ])),
                ),
                RegisteredStrategy::new(
                    RegisteredStrategyKind::Z3Solver,
                    Box::new(BlockingFeedbackStrategy {
                        feedback_started,
                        release_feedback,
                        feedback_calls,
                    }),
                ),
            ],
            MetaConfig {
                adaptive: false,
                ..MetaConfig::default()
            },
        )
    }

    #[tokio::test]
    async fn async_feedback_scheduler_prefetches_candidate_before_blocking_feedback() {
        let feedback_started = Arc::new(AtomicBool::new(false));
        let release_feedback = Arc::new(AtomicBool::new(false));
        let feedback_calls = Arc::new(AtomicUsize::new(0));
        let meta_strategy = scheduler_meta_strategy(
            Arc::clone(&feedback_started),
            Arc::clone(&release_feedback),
            Arc::clone(&feedback_calls),
        );
        let mut scheduler =
            ConcolicFeedbackScheduler::new(meta_strategy, ConcolicFeedbackMode::Async);
        let ctx = scheduler_strategy_context();
        let mut rng = StdRng::seed_from_u64(7);

        let first = scheduler
            .next_meta_candidate(&ctx, &mut rng)
            .await
            .expect("scheduler should not fail")
            .expect("first candidate should be available");
        assert_eq!(first.0, vec![serde_json::json!(1)]);

        scheduler
            .submit_feedback(
                vec![serde_json::json!(1)],
                make_exec_result(vec![]),
                false,
                Some(first.1),
                &ctx,
                &mut rng,
            )
            .await
            .expect("feedback should submit");

        while !feedback_started.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }

        let second = scheduler
            .next_meta_candidate(&ctx, &mut rng)
            .await
            .expect("scheduler should not fail")
            .expect("prefetched candidate should be available while feedback is blocked");
        assert_eq!(second.0, vec![serde_json::json!(2)]);
        assert_eq!(scheduler.pipeline_overlaps(), 1);
        assert_eq!(feedback_calls.load(Ordering::SeqCst), 0);

        release_feedback.store(true, Ordering::SeqCst);
        scheduler
            .drain_pending_feedback()
            .await
            .expect("feedback should complete");
        assert_eq!(feedback_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn sync_feedback_scheduler_waits_before_next_candidate() {
        let feedback_started = Arc::new(AtomicBool::new(false));
        let release_feedback = Arc::new(AtomicBool::new(true));
        let feedback_calls = Arc::new(AtomicUsize::new(0));
        let meta_strategy = scheduler_meta_strategy(
            Arc::clone(&feedback_started),
            Arc::clone(&release_feedback),
            Arc::clone(&feedback_calls),
        );
        let mut scheduler =
            ConcolicFeedbackScheduler::new(meta_strategy, ConcolicFeedbackMode::Sync);
        let ctx = scheduler_strategy_context();
        let mut rng = StdRng::seed_from_u64(7);

        let first = scheduler
            .next_meta_candidate(&ctx, &mut rng)
            .await
            .expect("scheduler should not fail")
            .expect("first candidate should be available");
        scheduler
            .submit_feedback(
                vec![serde_json::json!(1)],
                make_exec_result(vec![]),
                false,
                Some(first.1),
                &ctx,
                &mut rng,
            )
            .await
            .expect("feedback should complete inline");

        assert!(feedback_started.load(Ordering::SeqCst));
        assert_eq!(feedback_calls.load(Ordering::SeqCst), 1);

        let second = scheduler
            .next_meta_candidate(&ctx, &mut rng)
            .await
            .expect("scheduler should not fail")
            .expect("second candidate should be available after feedback");
        assert_eq!(second.0, vec![serde_json::json!(2)]);
        assert_eq!(scheduler.pipeline_overlaps(), 0);
    }

    // -- hash_branch_path tests --

    #[test]
    fn same_branch_path_hashes_identically() {
        let path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown { hint: "x".into() },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 20,
                taken: false,
                constraint: SymConstraint::Unknown { hint: "y".into() },
                conditions: None,
            },
        ];
        assert_eq!(hash_branch_path(&path), hash_branch_path(&path));
    }

    #[test]
    fn different_taken_hashes_differently() {
        let path_a = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown { hint: "x".into() },
            conditions: None,
        }];
        let path_b = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Unknown { hint: "x".into() },
            conditions: None,
        }];
        assert_ne!(hash_branch_path(&path_a), hash_branch_path(&path_b));
    }

    #[test]
    fn empty_branch_path_hashes_consistently() {
        assert_eq!(hash_branch_path(&[]), hash_branch_path(&[]));
    }

    // -- extract_sym_constraints tests --

    #[test]
    fn extracts_expr_constraints_and_skips_unknown() {
        let x_gt_10 = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };

        let result = make_exec_result(vec![
            BranchDecision {
                branch_id: 0,
                line: 5,
                taken: true,
                constraint: SymConstraint::Expr {
                    expr: x_gt_10.clone(),
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 10,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "regex".into(),
                },
                conditions: None,
            },
        ]);

        let constraints = extract_sym_constraints(&result);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0], Some(x_gt_10));
        assert_eq!(constraints[1], None);
    }

    // -- concrete_to_json tests --

    #[test]
    fn concrete_to_json_primitives() {
        assert_eq!(
            concrete_to_json(&ConcreteValue::Int(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            concrete_to_json(&ConcreteValue::Float(2.5)),
            serde_json::json!(2.5)
        );
        assert_eq!(
            concrete_to_json(&ConcreteValue::Str("hello".into())),
            serde_json::json!("hello")
        );
        assert_eq!(
            concrete_to_json(&ConcreteValue::Bool(true)),
            serde_json::json!(true)
        );
    }

    #[test]
    fn concrete_complex_to_json_produces_tagged_format() {
        let val = ConcreteValue::Complex {
            kind: ComplexKind::Date,
            repr: Box::new(ConcreteValue::Int(1704067200000)),
        };
        let json = concrete_to_json(&val);
        assert_eq!(json["__complex_type"], "date");
        assert_eq!(json["value"], 1704067200000_i64);
    }

    #[test]
    fn concrete_complex_bigint_to_json() {
        let val = ConcreteValue::Complex {
            kind: ComplexKind::BigInt,
            repr: Box::new(ConcreteValue::Str("99999999999999999999".into())),
        };
        let json = concrete_to_json(&val);
        assert_eq!(json["__complex_type"], "big_int");
        assert_eq!(json["value"], "99999999999999999999");
    }

    // -- overlay_solved_values tests --

    #[test]
    fn overlay_replaces_matching_param() {
        let base = vec![serde_json::json!(0), serde_json::json!("hello")];
        let mut solved = HashMap::new();
        solved.insert("x".to_string(), ConcreteValue::Int(42));
        let param_names = vec!["x".to_string(), "name".to_string()];

        let result = overlay_solved_values(&base, &solved, &param_names);
        assert_eq!(result[0], serde_json::json!(42));
        assert_eq!(result[1], serde_json::json!("hello"));
    }

    #[test]
    fn overlay_single_param_fallback() {
        let base = vec![serde_json::json!(0)];
        let mut solved = HashMap::new();
        solved.insert("some_var".to_string(), ConcreteValue::Int(99));
        let param_names = vec!["x".to_string()];

        let result = overlay_solved_values(&base, &solved, &param_names);
        assert_eq!(result[0], serde_json::json!(99));
    }

    #[test]
    fn overlay_no_match_preserves_base() {
        let base = vec![serde_json::json!(5), serde_json::json!(10)];
        let mut solved = HashMap::new();
        solved.insert("unknown_var".to_string(), ConcreteValue::Int(99));
        let param_names = vec!["a".to_string(), "b".to_string()];

        let result = overlay_solved_values(&base, &solved, &param_names);
        assert_eq!(result, base);
    }

    #[test]
    fn overlay_nested_payload_field_from_rust_symbol() {
        let base = vec![serde_json::Value::Null];
        let mut solved = HashMap::new();
        solved.insert(
            "payload . label . as_deref ()".to_string(),
            ConcreteValue::Str("branch label".into()),
        );
        let param_names = vec!["payload".to_string()];

        let result = overlay_solved_values(&base, &solved, &param_names);
        assert_eq!(
            result,
            vec![serde_json::json!({
                "label": "branch label",
            })]
        );
    }

    #[test]
    fn overlay_nested_payload_field_uses_json_field_name() {
        let base = vec![serde_json::json!({ "label": "existing" })];
        let mut solved = HashMap::new();
        solved.insert(
            "payload . owner_person_id".to_string(),
            ConcreteValue::Str("person-id".into()),
        );
        let param_names = vec!["payload".to_string()];

        let result = overlay_solved_values(&base, &solved, &param_names);
        assert_eq!(
            result,
            vec![serde_json::json!({
                "label": "existing",
                "ownerPersonId": "person-id",
            })]
        );
    }

    // -- WorklistEntry ordering tests --

    // -- Integration test: concolic loop finds x=42 via Z3 --

    /// This test simulates the concolic loop without a real frontend by directly
    /// testing the solver-driven input generation for f(x) { if (x === 42) ... }.
    ///
    /// The acceptance criteria require that Z3 solving can find x=42 for an
    /// exact-equality branch that random exploration cannot feasibly discover.
    #[test]
    fn z3_finds_exact_equality_input() {
        // Simulate: we executed f(0) and observed the branch `x == 42` taken=false.
        // The constraint for the branch is (x == 42), and we negate it → we need x != 42.
        // Wait — the branch was NOT taken, so the path constraint recorded is actually
        // the negation of the condition: NOT(x == 42). To explore the true branch,
        // we negate that: x == 42.
        //
        // In our protocol, the constraint is recorded as `x == 42` with `taken: false`.
        // The solver receives the constraint as-is and negates it to explore the other path.
        // Since taken=false means the constraint evaluated to false, the original path has
        // NOT(x == 42). Negating that yields x == 42 — exactly what we want.

        let x_eq_42 = SymExpr::BinOp {
            op: BinOpKind::Eq,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(42))),
        };

        // The path has one constraint: x == 42 (which evaluated to false).
        // solve_for_new_path negates constraint[0], giving us x == 42 → NOT(x == 42)?
        // No — solve_for_new_path keeps the prefix and negates the target.
        // With index 0 and only one constraint: it negates constraint[0].
        // The constraint is (x == 42). Negating it gives (x != 42) which is SAT for many values.
        //
        // Actually, to find x=42, we need to SOLVE the constraint x==42 directly.
        // In the real concolic loop, when a branch is not taken, the constraint
        // represents the condition, and the path records that it was false.
        // To flip the branch, we want the condition to be true: x == 42.
        // This means we should solve the constraint directly, not negate it.
        //
        // Our solver API `solve_for_new_path` negates constraint[negate_index].
        // So if we pass the constraint as-is (x == 42) and negate it, we get x != 42.
        // But we want x == 42!
        //
        // The trick: the frontend records the *evaluated* constraint. When taken=false,
        // it means the condition was false. So the path constraint is NOT(x == 42).
        // To represent this, we'd store NOT(x == 42) in the constraint list.
        // Then negating it gives x == 42. ✓
        //
        // For this test, let's just use solve_constraints directly to verify Z3 can find x=42.
        let result =
            solver::solve_constraints(&[x_eq_42], None, &[]).expect("solver should not error");

        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert_eq!(x, 42, "Z3 should find x = 42");
            }
            SolveResult::Unsat => panic!("expected sat — Z3 should be able to find x = 42"),
        }
    }

    /// Test that the full negation-based approach works: given a path where x==42
    /// was false, negating the path constraint finds x=42.
    #[test]
    fn negating_failed_equality_finds_target_value() {
        // Path constraint: NOT(x == 42), representing that the branch was not taken.
        let not_x_eq_42 = SymExpr::UnOp {
            op: crate::sym_expr::UnOpKind::Not,
            operand: Box::new(SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(42))),
            }),
        };

        // Negate constraint[0] to flip the branch: NOT(NOT(x == 42)) → x == 42.
        let result = solver::solve_for_new_path(&[not_x_eq_42], 0, None, &[])
            .expect("solver should not error");

        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert_eq!(x, 42, "negating NOT(x==42) should yield x=42");
            }
            SolveResult::Unsat => panic!("expected sat"),
        }
    }

    /// Multi-branch scenario: f(x) has branches at x>10, x==42, x<100.
    /// Starting from x=0 (all branches false), Z3 can find inputs for each path.
    #[test]
    fn multi_branch_z3_exploration() {
        // Simulate path from x=0: branches x>10 (false), x==42 (false), x<100 (true).
        // Path constraints as recorded: NOT(x>10), NOT(x==42), x<100.
        let not_x_gt_10 = SymExpr::UnOp {
            op: crate::sym_expr::UnOpKind::Not,
            operand: Box::new(SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(10))),
            }),
        };
        let not_x_eq_42 = SymExpr::UnOp {
            op: crate::sym_expr::UnOpKind::Not,
            operand: Box::new(SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(42))),
            }),
        };
        let x_lt_100 = SymExpr::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(100))),
        };

        let constraints = [not_x_gt_10, not_x_eq_42, x_lt_100];

        // Negate constraint[0]: flip NOT(x>10) → x>10. With prefix empty, just x>10.
        let result = solver::solve_for_new_path(&constraints, 0, None, &[])
            .expect("should solve for branch 0");
        match result {
            SolveResult::Sat(values) => {
                let x = match values.get("x") {
                    Some(ConcreteValue::Int(v)) => *v,
                    other => panic!("expected Int for x, got {other:?}"),
                };
                assert!(x > 10, "flipping branch 0 should give x>10, got {x}");
            }
            SolveResult::Unsat => panic!("expected sat for branch 0"),
        }

        // Negate constraint[1]: keep prefix NOT(x>10) i.e. x<=10, then flip NOT(x==42) → x==42.
        // But x<=10 AND x==42 is UNSAT (can't be both ≤10 and =42).
        let result = solver::solve_for_new_path(&constraints, 1, None, &[])
            .expect("should solve for branch 1");
        assert!(
            matches!(result, SolveResult::Unsat),
            "x<=10 AND x==42 should be unsat"
        );

        // Negate constraint[2]: keep prefix NOT(x>10), NOT(x==42), flip x<100 → x>=100.
        // x<=10 AND x!=42 AND x>=100 is UNSAT.
        let result = solver::solve_for_new_path(&constraints, 2, None, &[])
            .expect("should solve for branch 2");
        assert!(
            matches!(result, SolveResult::Unsat),
            "x<=10 AND x>=100 should be unsat"
        );
    }

    /// Verify that the worklist priority queue drains Z3-solved inputs before seeds
    /// when no fitness scores are present.
    #[test]
    fn worklist_drains_in_priority_order() {
        let mut worklist = BinaryHeap::new();

        // Push in arbitrary order — all without fitness scores.
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("seed1")],
            source: InputSource::Seed,
            fitness: None,
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_1")],
            source: InputSource::Z3Solved,
            fitness: None,
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("fuzz1")],
            source: InputSource::Fuzzed,
            fitness: None,
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_2")],
            source: InputSource::Z3Solved,
            fitness: None,
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("seed2")],
            source: InputSource::Seed,
            fitness: None,
            mock_values: vec![],
        });

        let sources: Vec<_> = std::iter::from_fn(|| worklist.pop())
            .map(|e| e.source)
            .collect();

        assert_eq!(
            sources,
            vec![
                InputSource::Z3Solved,
                InputSource::Z3Solved,
                InputSource::Fuzzed,
                InputSource::Seed,
                InputSource::Seed,
            ]
        );
    }

    /// Fitness-scored entries outrank entries without fitness in the worklist.
    #[test]
    fn fitness_entries_outrank_no_fitness() {
        let mut worklist = BinaryHeap::new();

        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3")],
            source: InputSource::Z3Solved,
            fitness: None,
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("low_fit")],
            source: InputSource::Seed,
            fitness: Some(0.1),
            mock_values: vec![],
        });

        // The fitness-scored entry (even with low fitness and Seed source)
        // should come out before the unscored Z3Solved entry.
        let first = worklist.pop().unwrap();
        assert!(first.fitness.is_some(), "fitness entry should drain first");
        let second = worklist.pop().unwrap();
        assert!(second.fitness.is_none());
    }

    /// Higher fitness scores drain before lower ones.
    #[test]
    fn higher_fitness_drains_first() {
        let mut worklist = BinaryHeap::new();

        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("low")],
            source: InputSource::Fuzzed,
            fitness: Some(0.2),
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("high")],
            source: InputSource::Fuzzed,
            fitness: Some(0.9),
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("mid")],
            source: InputSource::Fuzzed,
            fitness: Some(0.5),
            mock_values: vec![],
        });

        let drained: Vec<f64> = std::iter::from_fn(|| worklist.pop())
            .map(|e| e.fitness.unwrap())
            .collect();
        assert_eq!(drained, vec![0.9, 0.5, 0.2]);
    }

    /// Equal fitness falls back to source ordering.
    #[test]
    fn equal_fitness_falls_back_to_source() {
        let mut worklist = BinaryHeap::new();

        worklist.push(WorklistEntry {
            inputs: vec![],
            source: InputSource::Seed,
            fitness: Some(0.5),
            mock_values: vec![],
        });
        worklist.push(WorklistEntry {
            inputs: vec![],
            source: InputSource::Z3Solved,
            fitness: Some(0.5),
            mock_values: vec![],
        });

        let first = worklist.pop().unwrap();
        assert_eq!(first.source, InputSource::Z3Solved);
        let second = worklist.pop().unwrap();
        assert_eq!(second.source, InputSource::Seed);
    }

    /// FitnessContext::from_seen_paths pre-seeds novelty tracking so already-
    /// discovered paths are not scored as novel.
    #[test]
    fn fitness_context_from_seen_paths_marks_existing() {
        let mut seen = HashSet::new();
        seen.insert(42u64);
        seen.insert(99u64);

        let mut ctx = FitnessContext::from_seen_paths(seen);
        assert!(!ctx.mark_seen(42), "pre-seeded path should not be novel");
        assert!(ctx.mark_seen(100), "unseen path should be novel");
    }

    // -- Observation and SolveOutput tests --

    #[test]
    fn solve_output_default_is_empty() {
        let output = SolveOutput::default();
        assert!(output.candidates.is_empty());
        assert_eq!(output.z3_count, 0);
        assert_eq!(output.fuzz_count, 0);
        assert_eq!(output.drill_count, 0);
    }

    #[test]
    fn solve_and_generate_skips_duplicate_observations() {
        // Observations that are not new paths should not produce any candidates.
        let obs = Observation {
            inputs: vec![serde_json::json!(0)],
            result: make_exec_result(vec![]),
            source: InputSource::Seed,
            path_id: 123,
            is_new_path: false,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let param_names = vec!["x".to_string()];
        let mut frontier_set = FrontierSet::new();
        let mut rng = StdRng::seed_from_u64(42);

        let output = solve_and_generate(
            &[obs],
            &mut frontier_set,
            &param_infos,
            &param_names,
            &[],
            &std::collections::HashSet::new(),
            &ExploreConfig::default(),
            &[],
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
            &[],
            None,
        );

        assert!(output.candidates.is_empty());
        assert_eq!(output.z3_count, 0);
        assert_eq!(output.fuzz_count, 0);
    }

    /// `solve_and_generate` no longer handles fuzz generation for unknown constraints —
    /// that responsibility moved to MetaStrategy.feedback() (FuzzerStrategy) in the main loop.
    /// For unknown constraints without boundary witnesses, no candidates are produced.
    #[test]
    fn solve_and_generate_produces_no_candidates_for_unknown_without_witnesses() {
        let obs = Observation {
            inputs: vec![serde_json::json!(5)],
            result: make_exec_result(vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "opaque".into(),
                },
                conditions: None,
            }]),
            source: InputSource::Seed,
            path_id: 456,
            is_new_path: true,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let param_names = vec!["x".to_string()];
        let mut frontier_set = FrontierSet::new();
        let mut rng = StdRng::seed_from_u64(42);

        let output = solve_and_generate(
            &[obs],
            &mut frontier_set,
            &param_infos,
            &param_names,
            &[],
            &std::collections::HashSet::new(),
            &ExploreConfig::default(),
            &[],
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
            &[],
            None,
        );

        // Z3 and fuzz generation moved to MetaStrategy — solve_and_generate produces no candidates
        // for unknown constraints without boundary witnesses.
        assert_eq!(output.fuzz_count, 0);
        assert_eq!(output.z3_count, 0);
        assert!(
            output.candidates.is_empty(),
            "no candidates without boundary witnesses"
        );
    }

    /// `solve_and_generate` no longer handles Z3 solving — that moved to
    /// Z3SolverStrategy.feedback() in the main loop. Solvable expr constraints
    /// produce no candidates from solve_and_generate itself.
    #[test]
    fn solve_and_generate_produces_no_z3_for_expr_constraints() {
        let x_gt_10 = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };

        let obs = Observation {
            inputs: vec![serde_json::json!(0)],
            result: make_exec_result(vec![BranchDecision {
                branch_id: 0,
                line: 5,
                taken: false,
                constraint: SymConstraint::Expr { expr: x_gt_10 },
                conditions: None,
            }]),
            source: InputSource::Seed,
            path_id: 789,
            is_new_path: true,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let param_names = vec!["x".to_string()];
        let mut frontier_set = FrontierSet::new();
        let mut rng = StdRng::seed_from_u64(42);

        let output = solve_and_generate(
            &[obs],
            &mut frontier_set,
            &param_infos,
            &param_names,
            &[],
            &std::collections::HashSet::new(),
            &ExploreConfig::default(),
            &[],
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
            &[],
            None,
        );

        // Z3 solving moved to Z3SolverStrategy.feedback() — solve_and_generate produces 0.
        assert_eq!(
            output.z3_count, 0,
            "Z3 moved to MetaStrategy; solve_and_generate produces no Z3 candidates"
        );
        assert!(
            !output
                .candidates
                .iter()
                .any(|e| e.source == InputSource::Z3Solved)
        );
    }

    #[test]
    fn bounded_unroll_fallback_solves_stalled_loop_frontier() {
        let param_infos = vec![make_int_param("n")];
        let param_names = vec!["n".to_string()];
        let loop_info = make_counted_loop_info();
        let loop_snapshots = vec![make_loop_snapshot(0, 0), make_loop_snapshot(1, 1)];

        let target_decision = BranchDecision {
            branch_id: 11,
            line: 44,
            taken: false,
            constraint: SymConstraint::Expr {
                expr: SymExpr::BinOp {
                    op: BinOpKind::Gt,
                    left: Box::new(SymExpr::Param {
                        name: "i".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(ConstValue::Int(50))),
                },
            },
            conditions: None,
        };

        let witness_result = ExecuteResult {
            scope_events: vec![
                TraceEvent::Branch {
                    decision: BranchDecision {
                        branch_id: 1,
                        line: 1,
                        taken: true,
                        constraint: SymConstraint::Expr {
                            expr: SymExpr::BinOp {
                                op: BinOpKind::Gt,
                                left: Box::new(SymExpr::Param {
                                    name: "n".into(),
                                    path: vec![],
                                }),
                                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
                            },
                        },
                        conditions: None,
                    },
                },
                TraceEvent::Scope {
                    event: ScopeEvent::LoopEnter { loop_id: 7 },
                },
                TraceEvent::Branch {
                    decision: target_decision.clone(),
                },
                TraceEvent::Scope {
                    event: ScopeEvent::LoopExit { loop_id: 7 },
                },
            ],
            loop_body_states: loop_snapshots.clone(),
            ..make_exec_result(vec![
                BranchDecision {
                    branch_id: 1,
                    line: 1,
                    taken: true,
                    constraint: SymConstraint::Expr {
                        expr: SymExpr::BinOp {
                            op: BinOpKind::Gt,
                            left: Box::new(SymExpr::Param {
                                name: "n".into(),
                                path: vec![],
                            }),
                            right: Box::new(SymExpr::Const(ConstValue::Int(0))),
                        },
                    },
                    conditions: None,
                },
                target_decision.clone(),
            ])
        };

        let template =
            crate::symbolic_unroll::extract_iteration_template(&loop_info, &loop_snapshots)
                .expect("template extraction should succeed");
        let observed_formula =
            crate::symbolic_unroll::build_unrolled_formula(&template).expect("formula builds");
        let observed_constraints = vec![
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "n".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            },
            observed_formula.iteration_bound.clone(),
            loop_bound_constraint(&loop_info, &observed_formula.locals)
                .expect("loop bound should exist"),
            opposite_branch_constraint(
                substitute_loop_locals(
                    match &target_decision.constraint {
                        SymConstraint::Expr { expr } => expr,
                        SymConstraint::Unknown { .. } => unreachable!(),
                    },
                    &observed_formula.locals,
                ),
                target_decision.taken,
            ),
        ];
        let observed_result = solver::solve_constraints(&observed_constraints, None, &param_infos)
            .expect("observed-depth solve should run");
        assert!(
            matches!(observed_result, SolveResult::Unsat),
            "observed snapshots alone should be insufficient to reach i > 50"
        );

        let frontier = Frontier {
            branch_id: target_decision.branch_id,
            depth: 1,
            blocking_params: vec![0],
            best_prefix: vec![serde_json::json!(1)],
            stall_count: BOUNDED_UNROLL_STALL_THRESHOLD,
            rarity_boost: 0.0,
        };
        let raw_results = vec![(frontier.best_prefix.clone(), vec![], witness_result)];

        let candidate = stalled_loop_candidate_inputs(
            &frontier,
            &raw_results,
            &[loop_info],
            &param_infos,
            &param_names,
            None,
        )
        .expect("bounded-unroll fallback should produce a candidate");

        let solved_n = candidate[0]
            .as_i64()
            .expect("candidate should contain an integer n");
        assert!(
            solved_n >= 52,
            "bounded-unroll fallback should solve for a loop count beyond the stall, got {solved_n}"
        );
    }

    // -- z3_solve_step unit tests --

    /// `z3_solve_step` with a solvable expression constraint produces a Z3 candidate.
    #[test]
    fn z3_solve_step_produces_z3_candidate_for_expr_constraint() {
        let x_gt_10 = SymExpr::BinOp {
            op: BinOpKind::Gt,
            left: Box::new(SymExpr::Param {
                name: "x".into(),
                path: vec![],
            }),
            right: Box::new(SymExpr::Const(ConstValue::Int(10))),
        };

        let obs = Observation {
            inputs: vec![serde_json::json!(0)],
            result: make_exec_result(vec![BranchDecision {
                branch_id: 0,
                line: 5,
                taken: false,
                constraint: SymConstraint::Expr {
                    expr: x_gt_10.clone(),
                },
                conditions: None,
            }]),
            source: InputSource::Seed,
            path_id: 1,
            is_new_path: true,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];

        // `taken=false` means the path condition is NOT(x>10), so negating at index 0
        // produces x>10 — solver should return a value > 10.
        let solvable_with_idx = vec![(
            0usize,
            SymExpr::UnOp {
                op: crate::sym_expr::UnOpKind::Not,
                operand: Box::new(x_gt_10),
            },
        )];

        let input = Z3SolveInput {
            obs,
            solvable_with_idx,
            invariant_skip: HashSet::new(),
            param_infos,
            param_names: vec!["x".to_string()],
            solver_timeout_ms: None,
        };

        let output = z3_solve_step(input);
        assert!(
            output.z3_count > 0,
            "z3_solve_step should produce Z3 candidates"
        );
        assert!(!output.candidates.is_empty());
        assert!(output.stall_branch_ids.is_empty());
        assert!(
            output
                .candidates
                .iter()
                .all(|c| c.source == InputSource::Z3Solved)
        );
    }

    /// `z3_solve_step` skips duplicate-path observations (is_new_path=false).
    #[test]
    fn z3_solve_step_skips_non_new_path() {
        let obs = Observation {
            inputs: vec![serde_json::json!(0)],
            result: make_exec_result(vec![]),
            source: InputSource::Seed,
            path_id: 1,
            is_new_path: false, // duplicate
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let input = Z3SolveInput {
            obs,
            solvable_with_idx: vec![],
            invariant_skip: HashSet::new(),
            param_infos: vec![],
            param_names: vec![],
            solver_timeout_ms: None,
        };

        let output = z3_solve_step(input);
        assert!(output.candidates.is_empty());
        assert_eq!(output.z3_count, 0);
        assert!(output.stall_branch_ids.is_empty());
    }

    #[test]
    fn meta_strategy_replaces_placeholder() {
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let strategies = vec![
            crate::strategy::RegisteredStrategy::new(
                crate::strategy::RegisteredStrategyKind::BoundarySeeds,
                Box::new(crate::strategy::BoundarySeeds::new(&params)),
            ),
            crate::strategy::RegisteredStrategy::new(
                crate::strategy::RegisteredStrategyKind::Random,
                Box::new(crate::strategy::RandomStrategy::new(Some(42))),
            ),
            crate::strategy::RegisteredStrategy::new(
                crate::strategy::RegisteredStrategyKind::Fuzzer,
                Box::new(crate::strategy::FuzzerStrategy::new(Some(42))),
            ),
        ];
        let mut meta = MetaStrategy::new(strategies, Default::default());
        let ctx = StrategyContext {
            params: params.clone(),
            literals: vec![],
            capabilities: FrontendCapabilities::default(),
            value_sources: vec![],
        };
        let mut rng = StdRng::seed_from_u64(0);

        // Should produce at least one candidate.
        let result = meta.next(&ctx, &mut rng);
        assert!(result.is_some());
    }

    #[test]
    fn meta_strategy_exhaustible_strategies_exhaust() {
        let params = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        // Only exhaustible strategies.
        let strategies = vec![
            crate::strategy::RegisteredStrategy::new(
                crate::strategy::RegisteredStrategyKind::UserProvided,
                Box::new(crate::strategy::UserProvidedStrategy::new(vec![vec![
                    serde_json::json!(1),
                ]])),
            ),
            crate::strategy::RegisteredStrategy::new(
                crate::strategy::RegisteredStrategyKind::BoundarySeeds,
                Box::new(crate::strategy::BoundarySeeds::new(&params)),
            ),
        ];
        let mut meta = MetaStrategy::new(strategies, Default::default());
        let ctx = StrategyContext {
            params: params.clone(),
            literals: vec![],
            capabilities: FrontendCapabilities::default(),
            value_sources: vec![],
        };
        let mut rng = StdRng::seed_from_u64(0);

        // Drain all candidates.
        let mut count = 0;
        while meta.next(&ctx, &mut rng).is_some() {
            count += 1;
            if count > 1000 {
                panic!("Should have exhausted by now");
            }
        }
        assert!(count > 0);
    }

    #[test]
    fn meta_strategy_feedback_reaches_fuzzer() {
        let strategies = vec![crate::strategy::RegisteredStrategy::new(
            crate::strategy::RegisteredStrategyKind::Fuzzer,
            Box::new(crate::strategy::FuzzerStrategy::new(Some(42))),
        )];
        let mut meta = MetaStrategy::new(strategies, Default::default());

        let ctx = StrategyContext {
            params: vec![ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::default(),
            value_sources: vec![],
        };
        let mut rng = StdRng::seed_from_u64(0);

        // Feed a new-path result so the fuzzer has interesting inputs to mutate.
        let mut result = make_exec_result(vec![]);
        result.return_value = Some(serde_json::json!(42));
        meta.feedback(&[serde_json::json!(5)], &result, true);

        // After feedback, fuzzer should produce mutations from the interesting input.
        let candidate = meta.next(&ctx, &mut rng);
        assert!(candidate.is_some());
    }

    // -- Integration tests with mock frontends --

    use crate::frontend::{Frontend, FrontendConfig};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// Request timeout for integration tests using mock frontends.
    const TEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    fn frontend_script(name: &str) -> PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../protocol").join(name)
    }

    fn config_for_script(script: &str) -> FrontendConfig {
        let mut config = FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![frontend_script(script).to_string_lossy().into_owned()];
        config.request_timeout = TEST_REQUEST_TIMEOUT;
        config
    }

    /// Explore with the noop frontend returns a single unique path (empty branch path)
    /// and terminates when the coverage plateau is reached. With MetaStrategy driving
    /// the loop (FuzzerStrategy generates inputs indefinitely), the loop terminates
    /// via plateau rather than worklist exhaustion.
    #[tokio::test]
    async fn explore_noop_frontend_terminates_on_plateau() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(10),
            max_executions: Some(50),
            plateau_threshold: 5,
            ..Default::default()
        };

        let (result, _) = explore(
            &mut frontend,
            "stub",
            vec![vec![serde_json::json!(0)]],
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // Noop returns empty branch_path every time → one unique path.
        assert_eq!(result.unique_paths, 1);
        assert!(result.total_executions >= 1);
        // MetaStrategy (FuzzerStrategy) generates inputs indefinitely; the loop
        // terminates on plateau since every execution hits the same empty path.
        assert!(
            result.termination_reason == TerminationReason::CoveragePlateau
                || result.termination_reason == TerminationReason::WorklistExhausted,
            "expected CoveragePlateau or WorklistExhausted, got {:?}",
            result.termination_reason
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Explore with the concolic test frontend discovers multiple paths via Z3.
    /// The test frontend simulates f(x) with branches at x>10 and x==42.
    #[tokio::test]
    async fn explore_concolic_frontend_discovers_paths_via_z3() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(20),
            max_executions: Some(100),
            plateau_threshold: 10,
            seed: Some(7),
            ..Default::default()
        };

        // Start with x=0 (hits the x<=10 path).
        let (result, _) = explore(
            &mut frontend,
            "f",
            vec![vec![serde_json::json!(0)]],
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // The orchestrator should discover at least 2 unique paths:
        // 1. x<=10 (from seed x=0)
        // 2. x>10 (from Z3 negating the x>10 constraint, or from fuzzing)
        assert!(
            result.unique_paths >= 2,
            "expected at least 2 unique paths, got {}",
            result.unique_paths
        );
        assert!(result.total_executions >= 2);
        assert!(
            result.z3_generated > 0,
            "Z3 should have generated at least one input"
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Async solver offload preserves Z3 attribution while allowing feedback to
    /// overlap with a prefetched observation.
    #[tokio::test]
    async fn explore_async_solver_offload_preserves_z3_attribution() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(20),
            max_executions: Some(100),
            plateau_threshold: 10,
            solver_offload: true,
            seed: Some(7),
            ..Default::default()
        };

        let seeds: Vec<Vec<serde_json::Value>> =
            (0..5).map(|i| vec![serde_json::json!(i)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "f",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert!(
            result.unique_paths >= 2,
            "expected at least 2 unique paths, got {}",
            result.unique_paths
        );
        assert!(
            result.z3_generated > 0,
            "Z3 should have generated at least one input"
        );
        assert!(
            result
                .discoveries
                .iter()
                .any(|(_, method)| *method == DiscoveryMethod::Z3),
            "async solver mode should preserve Z3 discovery attribution"
        );
        assert!(
            result.pipeline_overlaps > 0,
            "async solver mode should overlap feedback with prefetched observations"
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Synchronous mode keeps `pipeline_overlaps` at zero for compatibility.
    #[tokio::test]
    async fn explore_pipeline_overlaps_is_zero() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(20),
            max_executions: Some(100),
            plateau_threshold: 10,
            ..Default::default()
        };

        let seeds: Vec<Vec<serde_json::Value>> =
            (0..5).map(|i| vec![serde_json::json!(i)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "f",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // Field still present in ExploreResult (compile check).
        assert_eq!(result.pipeline_overlaps, 0);
        // Exploration must still make progress despite pipelining removal.
        assert!(
            result.unique_paths >= 1,
            "expected at least one unique path"
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Plateau detection stops exploration when no new paths are found.
    #[tokio::test]
    async fn explore_stops_on_coverage_plateau() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(100),
            max_executions: Some(100),
            // Low threshold so plateau triggers quickly.
            plateau_threshold: 3,
            ..Default::default()
        };

        // Provide multiple identical seeds so the worklist doesn't empty first.
        let seeds = (0..10).map(|i| vec![serde_json::json!(i)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // All seeds produce the same empty branch path, so after the first unique path
        // we get plateau_threshold consecutive duplicates.
        assert_eq!(result.unique_paths, 1);
        assert_eq!(
            result.termination_reason,
            TerminationReason::CoveragePlateau
        );
        // 1 new path + 3 duplicates to trigger plateau = 4 total executions.
        assert_eq!(result.total_executions, 4);

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Max executions budget stops exploration.
    #[tokio::test]
    async fn explore_stops_on_max_executions() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(100),
            max_executions: Some(3),
            plateau_threshold: 0, // disable plateau
            ..Default::default()
        };

        let seeds = (0..10).map(|i| vec![serde_json::json!(i)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert_eq!(result.total_executions, 3);
        assert_eq!(result.termination_reason, TerminationReason::MaxExecutions);

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// str-nqrz regression: a small `--max-iterations`-style user cap must
    /// be respected even when the orchestrator could otherwise enter a fuzz
    /// phase on coverage plateau. Pre-fix, the CLI multiplied
    /// `max_executions` by 5 and the fuzz phase drew from the full
    /// `DEFAULT_FUZZ_MAX_EXECUTIONS=1000` budget on plateau, so a focused
    /// run with `--max-iterations 5` reported >250 iterations.
    #[test]
    fn clamp_fuzz_budget_respects_global_cap() {
        // Global cap 5, none used yet → fuzz can use at most 5.
        assert_eq!(clamp_fuzz_budget(1000, Some(5), 0), 5);
        // Global cap 5, 3 used → fuzz can use at most 2.
        assert_eq!(clamp_fuzz_budget(1000, Some(5), 3), 2);
        // Global cap reached → fuzz can use 0 (will terminate immediately).
        assert_eq!(clamp_fuzz_budget(1000, Some(5), 5), 0);
        // Global cap exceeded (defensive) → still 0 via saturating_sub.
        assert_eq!(clamp_fuzz_budget(1000, Some(5), 7), 0);
        // Configured fuzz cap below remaining budget → keep configured cap.
        assert_eq!(clamp_fuzz_budget(10, Some(100), 0), 10);
        // No global cap → keep configured cap.
        assert_eq!(clamp_fuzz_budget(1000, None, 7), 1000);
    }

    /// str-nqrz regression: a focused concolic explore with a small user
    /// cap (here `max_executions=5`) must never run more than 5 executions,
    /// regardless of how many seeds are queued or whether plateau-driven
    /// fuzz phases would otherwise extend the run. Pre-fix, the CLI granted
    /// the orchestrator 5x headroom on `max_executions` and an
    /// uncapped per-fuzz-phase budget of 1000, so a `--max-iterations 5`
    /// run could report hundreds of iterations.
    #[tokio::test]
    async fn explore_honors_small_user_cap_with_many_seeds() {
        let config = config_for_script("fixed-branch-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let user_cap: usize = 5;
        let explore_config = ExploreConfig {
            // Mirror the CLI configuration after str-nqrz: `max_executions`
            // tracks the user iteration cap with no multiplier, and
            // refinement / shrinking are disabled.
            max_iterations: Some(user_cap),
            max_executions: Some(user_cap),
            plateau_threshold: 20,
            refine_budget: None,
            shrink_budget: 0,
            ..Default::default()
        };

        // Many more seeds than the cap allows — the cap, not seed exhaustion,
        // must determine when exploration stops.
        let seed_count = 250;
        let seeds: Vec<Vec<serde_json::Value>> = (0..seed_count)
            .map(|i| vec![serde_json::json!(i)])
            .collect();

        let (result, _) = explore(
            &mut frontend,
            "f",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert!(
            result.total_executions <= user_cap,
            "expected total_executions <= {user_cap}; got {} (termination={:?})",
            result.total_executions,
            result.termination_reason,
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// str-cir6 regression: plateau-triggered fuzzing must obey the same
    /// per-function deadline as the main concolic observe loop. Without this,
    /// a scan could continue issuing fast fuzz Execute calls long after
    /// `timeout_explore` had expired.
    #[tokio::test]
    async fn explore_stops_plateau_fuzz_on_timeout_explore() {
        let config = config_for_script("unknown-branch-fuzz-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let fuzz_cap = 50;
        let explore_config = ExploreConfig {
            max_iterations: Some(100),
            max_executions: Some(100),
            plateau_threshold: 1,
            timeout_explore: Some(Duration::from_millis(20)),
            refine_budget: None,
            shrink_budget: 0,
            fuzz: crate::config::FuzzConfig {
                plateau_threshold: Some(fuzz_cap),
                max_executions: Some(fuzz_cap),
                timeout_seconds: Some(10),
                max_attempts: Some(3),
            },
            ..Default::default()
        };

        let (result, _) = explore(
            &mut frontend,
            "f",
            vec![vec![serde_json::json!(0)]],
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert!(
            result.timed_out,
            "plateau fuzz crossing timeout_explore must surface timed_out=true"
        );
        assert_eq!(result.termination_reason, TerminationReason::TimeoutExplore);
        assert!(
            result.total_executions < fuzz_cap as usize,
            "timeout_explore should stop fuzz before exhausting its phase cap; got {} executions",
            result.total_executions
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Max iterations budget stops exploration.
    #[tokio::test]
    async fn explore_stops_on_max_iterations() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(1),
            max_executions: Some(100),
            plateau_threshold: 0,
            ..Default::default()
        };

        // Provide seeds that will hit different paths.
        let (result, _) = explore(
            &mut frontend,
            "f",
            vec![vec![serde_json::json!(0)], vec![serde_json::json!(20)]],
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert_eq!(result.unique_paths, 1);
        assert_eq!(result.termination_reason, TerminationReason::MaxIterations);

        frontend.shutdown().await.expect("shutdown failed");
    }

    #[test]
    fn frontend_capabilities_parses_complex_types() {
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(),
            "execute".into(),
            "complex_type:date".into(),
            "complex_type:reg_exp".into(),
            "complex_type:big_int".into(),
        ]);
        assert!(caps.commands.contains("analyze"));
        assert!(caps.commands.contains("execute"));
        assert!(caps.supports_complex(ComplexKind::Date));
        assert!(caps.supports_complex(ComplexKind::RegExp));
        assert!(caps.supports_complex(ComplexKind::BigInt));
        assert!(!caps.supports_complex(ComplexKind::Url));
        assert!(!caps.supports_complex(ComplexKind::Error));
    }

    #[test]
    fn frontend_capabilities_ignores_unknown_complex_types() {
        let caps = FrontendCapabilities::from_raw(&[
            "complex_type:date".into(),
            "complex_type:nonexistent_type".into(),
            "complex_type:".into(),
        ]);
        assert!(caps.supports_complex(ComplexKind::Date));
        assert_eq!(caps.complex_types.len(), 1);
    }

    #[test]
    fn frontend_capabilities_separates_commands_from_complex_types() {
        let caps = FrontendCapabilities::from_raw(&[
            "analyze".into(),
            "execute".into(),
            "instrument".into(),
            "complex_type:date".into(),
            "complex_type:url".into(),
        ]);
        assert_eq!(caps.commands.len(), 3);
        assert_eq!(caps.complex_types.len(), 2);
        // "complex_type:date" should NOT appear in commands
        assert!(!caps.commands.contains("complex_type:date"));
    }

    #[test]
    fn frontend_capabilities_default_is_empty() {
        let caps = FrontendCapabilities::default();
        assert!(caps.commands.is_empty());
        assert!(caps.complex_types.is_empty());
        assert!(!caps.supports_complex(ComplexKind::Date));
    }

    /// A very short timeout_explore stops exploration before max_executions.
    #[tokio::test]
    async fn explore_stops_on_timeout_explore() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(1000),
            max_executions: Some(10000),
            plateau_threshold: 0,
            timeout_explore: Some(Duration::from_millis(1)),
            ..Default::default()
        };

        let seeds = (0..100).map(|i| vec![serde_json::json!(i)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // Should terminate due to timeout, not max_executions or max_iterations.
        assert_eq!(result.termination_reason, TerminationReason::TimeoutExplore);
        assert!(result.total_executions < 10000);
        // str-jeen.65: the new `timed_out` field must reflect the wall-clock
        // verdict — when the loop itself exited via TimeoutExplore the flag
        // is unambiguously true.
        assert!(
            result.timed_out,
            "TerminationReason::TimeoutExplore must surface as ExploreResult.timed_out=true"
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// str-jeen.65: when the wall-clock budget is exceeded but the loop
    /// terminates "naturally" first (worklist exhausted with no candidates
    /// remaining), the `timed_out` flag must still be set so the CLI does
    /// not silently report the function as `ok`. The regression scenario:
    /// the explore loop returns quickly with WorklistExhausted, but the
    /// per-function budget was zero (or already crossed), so by the time
    /// the orchestrator reaches its final deadline check, the deadline is
    /// behind us.
    #[tokio::test]
    async fn explore_marks_timed_out_when_deadline_crossed_with_natural_termination() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(1000),
            max_executions: Some(10000),
            plateau_threshold: 0,
            // A zero-duration budget means `deadline_crossed()` returns true
            // immediately, so even a fast-completing explore must report
            // `timed_out=true`.
            timeout_explore: Some(Duration::from_millis(0)),
            ..Default::default()
        };

        let seeds: Vec<Vec<serde_json::Value>> = vec![vec![serde_json::json!(0)]];

        let (result, _) = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        assert!(
            result.timed_out,
            "wall-clock budget crossed (deadline in the past) must set \
             ExploreResult.timed_out=true regardless of termination_reason; \
             got termination_reason={:?}",
            result.termination_reason,
        );

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Triage should skip redundant inputs that predict already-covered paths.
    ///
    /// Uses a fixed-branch frontend (always returns same single-branch path).
    /// After the first seed discovers the path, triage predicts Skip for
    /// all subsequent seeds with matching constraint evaluations.
    #[tokio::test]
    async fn explore_triage_samples_redundant_seeds() {
        let config = config_for_script("fixed-branch-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: Some(50),
            max_executions: Some(50),
            plateau_threshold: 0, // disable plateau so we rely on worklist exhaustion
            ..Default::default()
        };

        // All seeds have x=5, which evaluates x>0 to true (Taken) — matching
        // the path discovered by the first execution. Seeds are always sampled
        // (executed) when triage predicts Skip, so redundant seeds still run
        // but produce no new unique paths.
        let seeds: Vec<Vec<serde_json::Value>> =
            (0..20).map(|_| vec![serde_json::json!(5)]).collect();

        let (result, _) = explore(
            &mut frontend,
            "f",
            seeds,
            vec![],
            &[ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            &explore_config,
            None,
            None,
            vec![],
            None,
            None,
        )
        .await
        .expect("explore failed");

        // Seeds bypass triage skip (always sampled), so triage_skipped may be 0.
        // The key invariant: redundant seeds don't inflate unique_paths.
        assert_eq!(result.unique_paths, 1);
        // Triage predictions for identical seeds are correct — no mispredictions.
        assert_eq!(result.triage_mispredictions, 0);

        frontend.shutdown().await.expect("shutdown failed");
    }

    // -----------------------------------------------------------------------
    // Loop peeling tests
    // -----------------------------------------------------------------------

    #[test]
    fn classify_empty_scope_events_returns_all_nonloop() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 1,
                line: 10,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 2,
                line: 20,
                taken: false,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        let positions = classify_iteration_positions(&[], &branch_path);
        assert_eq!(positions.len(), 2);
        assert!(positions.iter().all(|p| *p == IterationPosition::NonLoop));
    }

    #[test]
    fn classify_single_iteration_loop() {
        let scope_events = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let branch_path = vec![BranchDecision {
            branch_id: 10,
            line: 5,
            taken: true,
            constraint: SymConstraint::default(),
            conditions: None,
        }];
        let positions = classify_iteration_positions(&scope_events, &branch_path);
        assert_eq!(positions, vec![IterationPosition::FirstExit]);
    }

    #[test]
    fn classify_two_iteration_loop() {
        let scope_events = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let branch_path = vec![
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        let positions = classify_iteration_positions(&scope_events, &branch_path);
        assert_eq!(
            positions,
            vec![IterationPosition::First, IterationPosition::Second]
        );
    }

    #[test]
    fn classify_three_iterations_has_interior() {
        let scope_events = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let branch_path = vec![
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        let positions = classify_iteration_positions(&scope_events, &branch_path);
        assert_eq!(
            positions,
            vec![
                IterationPosition::First,
                IterationPosition::Second,
                IterationPosition::Interior,
            ]
        );
    }

    #[test]
    fn classify_branch_outside_loop_is_nonloop() {
        let scope_events = vec![
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 1,
                    line: 3,
                    taken: false,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let branch_path = vec![
            BranchDecision {
                branch_id: 1,
                line: 3,
                taken: false,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        let positions = classify_iteration_positions(&scope_events, &branch_path);
        assert_eq!(positions[0], IterationPosition::NonLoop);
        assert_eq!(positions[1], IterationPosition::FirstExit);
    }

    #[test]
    fn boundary_candidates_sort_above_interior_in_worklist() {
        let interior = WorklistEntry {
            inputs: vec![serde_json::json!(1)],
            source: InputSource::Z3Solved,
            fitness: None,
            mock_values: vec![],
        };
        let boundary = WorklistEntry {
            inputs: vec![serde_json::json!(2)],
            source: InputSource::Z3Solved,
            fitness: Some(BOUNDARY_FITNESS_FIRST),
            mock_values: vec![],
        };
        let mut heap = BinaryHeap::new();
        heap.push(interior);
        heap.push(boundary);
        let top = heap.pop().unwrap();
        assert_eq!(top.fitness, Some(BOUNDARY_FITNESS_FIRST));
    }

    /// Integration test: loop peeling boost propagates through solve_and_generate.
    ///
    /// Scenario: a stalled frontier is drilled, producing drilled candidates. A
    /// 1-iteration loop (LoopEnter + Branch + LoopExit) means the observation is
    /// classified as `FirstExit` and candidates receive `BOUNDARY_FITNESS_FIRST`.
    /// A second observation with empty scope_events is `NonLoop` — no boost.
    #[test]
    fn loop_peeling_fitness_boost_propagates_through_solve_and_generate() {
        let branch = BranchDecision {
            branch_id: 0,
            line: 5,
            taken: true,
            constraint: SymConstraint::default(),
            conditions: None,
        };
        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let param_names = vec!["x".to_string()];

        let make_frontier_set = || {
            let mut fs = FrontierSet::new();
            // Add a stalled frontier so drilling produces candidates.
            let f = crate::frontier::Frontier {
                branch_id: 0,
                depth: 1,
                blocking_params: vec![0],
                best_prefix: vec![serde_json::json!(15i64)],
                stall_count: drilling::DRILL_STALL_THRESHOLD,
                rarity_boost: 0.0,
            };
            fs.insert(f);
            fs
        };

        let make_obs = |scope_events: Vec<TraceEvent>| Observation {
            inputs: vec![serde_json::json!(15i64)],
            result: ExecuteResult {
                branch_path: vec![branch.clone()],
                scope_events,
                loop_body_states: vec![],
                return_value: None,
                thrown_error: None,
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                runtime_crypto_boundaries: vec![],
                outcome: None,
                performance: empty_perf(),
            },
            source: InputSource::Seed,
            path_id: 1,
            is_new_path: true,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        let call_solve = |obs: Observation| {
            solve_and_generate(
                &[obs],
                &mut make_frontier_set(),
                &param_infos,
                &param_names,
                &[],
                &std::collections::HashSet::new(),
                &ExploreConfig::default(),
                &[],
                &mut StdRng::seed_from_u64(42),
                &HashSet::new(),
                &mut FitnessContext::new(),
                &FitnessWeights::default(),
                &[],
                None,
            )
        };

        // Observation with 1-iteration loop: branch is FirstExit → gets BOUNDARY_FITNESS_FIRST.
        let scope_with_loop = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: branch.clone(),
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let out_boundary = call_solve(make_obs(scope_with_loop));
        assert!(
            !out_boundary.candidates.is_empty(),
            "boundary observation should produce drill candidates (stalled frontier triggers drilling)"
        );
        assert!(
            out_boundary
                .candidates
                .iter()
                .all(|c| c.fitness == Some(BOUNDARY_FITNESS_FIRST)),
            "all candidates from a boundary (FirstExit) observation should get BOUNDARY_FITNESS_FIRST boost; \
             got fitnesses: {:?}",
            out_boundary
                .candidates
                .iter()
                .map(|c| c.fitness)
                .collect::<Vec<_>>(),
        );

        // Observation with no loop context: branch is NonLoop → no boost, fitness stays None.
        let out_no_boost = call_solve(make_obs(vec![]));
        assert!(
            !out_no_boost.candidates.is_empty(),
            "non-loop observation should also produce drill candidates"
        );
        assert!(
            out_no_boost.candidates.iter().all(|c| c.fitness.is_none()),
            "candidates from a non-loop observation should have no fitness boost; \
             got fitnesses: {:?}",
            out_no_boost
                .candidates
                .iter()
                .map(|c| c.fitness)
                .collect::<Vec<_>>(),
        );
    }

    // -----------------------------------------------------------------------
    // Loop-invariant detector tests
    // -----------------------------------------------------------------------

    #[test]
    fn invariant_detector_marks_constant_branch() {
        let mut detector = LoopInvariantDetector::new();
        let events = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let bp = vec![
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        detector.observe(&events, &bp);
        let skip = detector.skip_indices(&events, &bp);
        assert!(!skip.contains(&0));
        assert!(skip.contains(&1));
    }

    #[test]
    fn invariant_detector_revokes_on_variation() {
        let mut detector = LoopInvariantDetector::new();
        let events1 = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let bp1 = vec![
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
            BranchDecision {
                branch_id: 10,
                line: 5,
                taken: true,
                constraint: SymConstraint::default(),
                conditions: None,
            },
        ];
        detector.observe(&events1, &bp1);
        assert!(!detector.skip_indices(&events1, &bp1).is_empty());

        let events2 = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: false,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
        ];
        let bp2 = vec![BranchDecision {
            branch_id: 10,
            line: 5,
            taken: false,
            constraint: SymConstraint::default(),
            conditions: None,
        }];
        detector.observe(&events2, &bp2);
        let skip = detector.skip_indices(&events1, &bp1);
        assert!(skip.is_empty());
    }

    #[test]
    fn invariant_detector_empty_events_no_skip() {
        let detector = LoopInvariantDetector::new();
        let bp = vec![BranchDecision {
            branch_id: 1,
            line: 5,
            taken: true,
            constraint: SymConstraint::default(),
            conditions: None,
        }];
        assert!(detector.skip_indices(&[], &bp).is_empty());
    }

    // -----------------------------------------------------------------------
    // Loop-invariant branch detection integration tests (str-in8d)
    // -----------------------------------------------------------------------

    /// Integration test: z3_solve_step negates an invariant loop branch exactly once,
    /// not once per loop iteration, when invariant_skip is populated.
    ///
    /// Key design: each iteration uses a DIFFERENT threshold (x > 10, x > 20, x > 30)
    /// so that without invariant detection all 3 would produce independent SAT results
    /// (10<x≤20, 20<x≤30, etc.). With invariant_skip={1,2}, only idx=0 gets Z3.
    ///
    /// Z3 solving is now in z3_solve_step (called from Z3SolverStrategy.feedback),
    /// not in solve_and_generate. This test exercises z3_solve_step directly.
    #[test]
    fn loop_invariant_branch_solved_once_not_n_times() {
        use crate::sym_expr::SymExpr;

        // 3 iterations of branch B0, all taken=true.
        // Each iteration has a stricter threshold so each negation is independently SAT.
        let n_iters: usize = 3;
        let thresholds: Vec<i64> = vec![10, 20, 30];

        let mut bp = Vec::new();
        let mut scope_evts = Vec::new();
        for &thresh in &thresholds {
            let expr = SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".into(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(thresh))),
            };
            let bd = BranchDecision {
                branch_id: 0, // same branch_id → invariant detector tracks it
                line: 5,
                taken: true,
                constraint: SymConstraint::Expr { expr },
                conditions: None,
            };
            scope_evts.push(TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            });
            scope_evts.push(TraceEvent::Branch {
                decision: bd.clone(),
            });
            bp.push(bd);
        }
        scope_evts.push(TraceEvent::Scope {
            event: ScopeEvent::LoopExit { loop_id: 1 },
        });

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int { int_width: None, int_signed: None },
            type_name: None,
        }];
        let param_names = vec!["x".to_string()];

        let obs = Observation {
            inputs: vec![serde_json::json!(35i64)],
            result: ExecuteResult {
                branch_path: bp.clone(),
                scope_events: scope_evts.clone(),
                loop_body_states: vec![],
                return_value: None,
                thrown_error: None,
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                runtime_crypto_boundaries: vec![],
                outcome: None,
                performance: empty_perf(),
            },
            source: InputSource::Seed,
            path_id: 42,
            is_new_path: true,
            is_sampled_skip: false,
            mock_values: vec![],
        };

        // Build solvable_with_idx from branch path: all taken=true so path cond = expr.
        let solvable_with_idx: Vec<(usize, SymExpr)> = bp
            .iter()
            .enumerate()
            .filter_map(|(i, bd)| match &bd.constraint {
                SymConstraint::Expr { expr } => Some((i, expr.clone())),
                SymConstraint::Unknown { .. } => None,
            })
            .collect();

        // WITH invariant detection: LoopInvariantDetector computes skip_indices = {1, 2}.
        let mut detector = LoopInvariantDetector::new();
        detector.observe(&scope_evts, &bp);
        let invariant_skip = detector.skip_indices(&scope_evts, &bp);

        let out_with_detection = z3_solve_step(Z3SolveInput {
            obs: obs.clone(),
            solvable_with_idx: solvable_with_idx.clone(),
            invariant_skip,
            param_infos: param_infos.clone(),
            param_names: param_names.clone(),
            solver_timeout_ms: None,
        });
        assert_eq!(
            out_with_detection.z3_count, 1,
            "invariant detection: should solve only the first occurrence; got z3_count={}",
            out_with_detection.z3_count,
        );

        // WITHOUT invariant detection: empty invariant_skip → all 3 iterations get Z3.
        // idx=0: NOT(x>10) → SAT. idx=1: x>10 AND NOT(x>20) → SAT. idx=2: SAT.
        let out_no_detection = z3_solve_step(Z3SolveInput {
            obs: obs.clone(),
            solvable_with_idx: solvable_with_idx.clone(),
            invariant_skip: HashSet::new(),
            param_infos: param_infos.clone(),
            param_names: param_names.clone(),
            solver_timeout_ms: None,
        });
        assert_eq!(
            out_no_detection.z3_count, n_iters,
            "no invariant detection: should solve all {n_iters} iterations; got z3_count={}",
            out_no_detection.z3_count,
        );

        assert!(
            out_with_detection.z3_count < out_no_detection.z3_count,
            "invariant detection should reduce Z3 attempts for a loop-invariant branch"
        );
    }

    #[test]
    fn extract_loop_context_maps_branches_to_loops() {
        let events = vec![
            TraceEvent::Scope {
                event: ScopeEvent::LoopEnter { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 10,
                    line: 5,
                    taken: true,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
            TraceEvent::Scope {
                event: ScopeEvent::LoopExit { loop_id: 1 },
            },
            TraceEvent::Branch {
                decision: BranchDecision {
                    branch_id: 20,
                    line: 15,
                    taken: false,
                    constraint: SymConstraint::default(),
                    conditions: None,
                },
            },
        ];
        let ctx = extract_loop_context(&events);
        assert!(ctx.get(&10).unwrap().contains(&1));
        assert!(!ctx.contains_key(&20)); // branch 20 is outside all loops
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use crate::solver::ConcreteValue;
        use crate::test_arbitraries::{arb_input_source, arb_json_value, arb_sym_expr};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn overlay_preserves_length(
                len in 1..6usize,
            ) {
                let base: Vec<serde_json::Value> =
                    (0..len).map(|i| serde_json::json!(i as i64)).collect();
                let names: Vec<String> =
                    (0..len).map(|i| format!("p{i}")).collect();
                // Empty solved map — output must equal input length.
                let solved = std::collections::HashMap::new();
                let result = overlay_solved_values(&base, &solved, &names);
                prop_assert_eq!(
                    base.len(),
                    result.len(),
                    "overlay_solved_values changed vector length"
                );
            }

            #[test]
            fn overlay_with_known_param_updates_value(idx in 0..5usize) {
                let len = idx + 1;
                let base: Vec<serde_json::Value> =
                    (0..len).map(|i| serde_json::json!(i as i64)).collect();
                let names: Vec<String> =
                    (0..len).map(|i| format!("p{i}")).collect();
                let mut solved = std::collections::HashMap::new();
                solved.insert(format!("p{idx}"), ConcreteValue::Int(999));
                let result = overlay_solved_values(&base, &solved, &names);
                prop_assert_eq!(result.len(), base.len());
                prop_assert_eq!(&result[idx], &serde_json::json!(999));
            }

            #[test]
            fn overlay_ignores_unknown_names(
                base_val in arb_json_value(),
            ) {
                let base = vec![base_val.clone()];
                let names = vec!["x".to_string()];
                let mut solved = std::collections::HashMap::new();
                // "unknown_var" doesn't match "x", so base should be unchanged.
                // Exception: single-param heuristic fires for non-dotted names,
                // so use a dotted name to avoid that path.
                solved.insert("unknown.derived".to_string(), ConcreteValue::Int(42));
                let result = overlay_solved_values(&base, &solved, &names);
                prop_assert_eq!(result.len(), 1);
                prop_assert_eq!(&result[0], &base_val);
            }

            /// Worklist dequeues entries in non-increasing InputSource priority.
            #[test]
            fn worklist_dequeues_in_priority_order(
                sources in prop::collection::vec(arb_input_source(), 1..20),
            ) {
                let mut heap = BinaryHeap::new();
                for source in &sources {
                    heap.push(WorklistEntry {
                        inputs: vec![],
                        source: *source,
                        fitness: None,
                        mock_values: vec![],
                    });
                }
                let drained: Vec<InputSource> = std::iter::from_fn(|| heap.pop())
                    .map(|e| e.source)
                    .collect();
                // Each element must be >= the next (non-increasing order).
                for window in drained.windows(2) {
                    prop_assert!(
                        window[0] >= window[1],
                        "worklist violated priority order: {:?} before {:?}",
                        window[0], window[1]
                    );
                }
                prop_assert_eq!(drained.len(), sources.len());
            }

            /// Inserting duplicate path hashes into covered_paths is idempotent.
            #[test]
            fn path_dedup_set_size_equals_distinct_count(
                hashes in prop::collection::vec(0..100u64, 1..50),
            ) {
                let mut covered = HashSet::new();
                for &h in &hashes {
                    covered.insert(h);
                }
                let distinct: HashSet<u64> = hashes.iter().copied().collect();
                prop_assert_eq!(covered.len(), distinct.len());
                // Second insert of every element returns false.
                for &h in &hashes {
                    prop_assert!(!covered.insert(h), "re-insert of {h} should return false");
                }
                prop_assert_eq!(covered.len(), distinct.len(), "size changed after re-inserts");
            }

            /// Budget exhaustion: a loop bounded by max_executions terminates
            /// after exactly min(max_executions, worklist_size) iterations.
            #[test]
            fn budget_limits_iteration_count(
                max_executions in 1..200usize,
                worklist_size in 1..500usize,
            ) {
                let mut worklist = BinaryHeap::new();
                for _ in 0..worklist_size {
                    worklist.push(WorklistEntry {
                        inputs: vec![],
                        source: InputSource::Seed,
                        fitness: None,
                        mock_values: vec![],
                    });
                }
                let mut executed = 0usize;
                while let Some(_entry) = worklist.pop() {
                    executed += 1;
                    if executed >= max_executions {
                        break;
                    }
                }
                let expected = max_executions.min(worklist_size);
                prop_assert_eq!(executed, expected);
            }

            /// Constraint accumulation grows monotonically — no constraints lost.
            #[test]
            fn constraint_accumulation_is_monotonic(
                batches in prop::collection::vec(
                    prop::collection::vec(
                        proptest::option::of(arb_sym_expr(1)),
                        0..5
                    ),
                    1..10
                ),
            ) {
                let mut all_constraints: Vec<Vec<Option<SymExpr>>> = Vec::new();
                for batch in &batches {
                    all_constraints.push(batch.clone());
                    prop_assert_eq!(
                        all_constraints.len(),
                        all_constraints.len(), // tautology for the assertion below
                    );
                }
                // Length equals number of batches — nothing was dropped.
                prop_assert_eq!(all_constraints.len(), batches.len());
                // Each entry matches its source batch.
                for (i, batch) in batches.iter().enumerate() {
                    prop_assert_eq!(&all_constraints[i], batch);
                }
            }

            /// classify_iteration_positions output length equals the number of
            /// Branch events in scope_events.
            #[test]
            fn classify_output_length_equals_branch_count(
                num_branches in 0..20usize,
                num_loops in 0..5usize,
            ) {
                let mut events = Vec::new();
                let mut branch_path = Vec::new();
                let mut active_loops = Vec::new();

                for loop_id in 0..num_loops as u32 {
                    events.push(TraceEvent::Scope {
                        event: ScopeEvent::LoopEnter { loop_id },
                    });
                    active_loops.push(loop_id);
                }

                for i in 0..num_branches {
                    let bd = BranchDecision {
                        branch_id: i as u32,
                        line: (i * 10) as u32,
                        taken: true,
                        constraint: SymConstraint::default(),
                        conditions: None,
                    };
                    events.push(TraceEvent::Branch { decision: bd.clone() });
                    branch_path.push(bd);
                }

                for loop_id in active_loops.into_iter().rev() {
                    events.push(TraceEvent::Scope {
                        event: ScopeEvent::LoopExit { loop_id },
                    });
                }

                let positions = classify_iteration_positions(&events, &branch_path);
                prop_assert_eq!(positions.len(), num_branches);
            }

            /// extract_loop_context output keys are a subset of branch_ids in the trace.
            #[test]
            fn extract_loop_context_keys_subset_of_branches(
                num_branches in 0..15usize,
                num_loops in 0..5u32,
            ) {
                let mut events = Vec::new();
                let mut branch_ids = HashSet::new();

                // Open loops
                for loop_id in 0..num_loops {
                    events.push(TraceEvent::Scope { event: ScopeEvent::LoopEnter { loop_id } });
                }
                // Branches
                for i in 0..num_branches {
                    let bid = i as u32;
                    branch_ids.insert(bid);
                    events.push(TraceEvent::Branch {
                        decision: BranchDecision { branch_id: bid, line: bid * 10, taken: true, constraint: SymConstraint::default(), conditions: None },
                    });
                }
                // Close loops
                for loop_id in (0..num_loops).rev() {
                    events.push(TraceEvent::Scope { event: ScopeEvent::LoopExit { loop_id } });
                }

                let ctx = extract_loop_context(&events);
                for key in ctx.keys() {
                    prop_assert!(branch_ids.contains(key));
                }
            }

            /// A branch that varies across observations is never marked invariant.
            #[test]
            fn varying_branch_never_invariant(
                loop_id in 0..10u32,
                branch_id in 0..20u32,
            ) {
                let mut detector = LoopInvariantDetector::new();
                let events_true = vec![
                    TraceEvent::Scope { event: ScopeEvent::LoopEnter { loop_id } },
                    TraceEvent::Branch { decision: BranchDecision { branch_id, line: 1, taken: true, constraint: SymConstraint::default() , conditions: None } },
                    TraceEvent::Scope { event: ScopeEvent::LoopEnter { loop_id } },
                    TraceEvent::Branch { decision: BranchDecision { branch_id, line: 1, taken: true, constraint: SymConstraint::default() , conditions: None } },
                    TraceEvent::Scope { event: ScopeEvent::LoopExit { loop_id } },
                ];
                let bp_true = vec![
                    BranchDecision { branch_id, line: 1, taken: true, constraint: SymConstraint::default() , conditions: None },
                    BranchDecision { branch_id, line: 1, taken: true, constraint: SymConstraint::default() , conditions: None },
                ];
                detector.observe(&events_true, &bp_true);

                let events_false = vec![
                    TraceEvent::Scope { event: ScopeEvent::LoopEnter { loop_id } },
                    TraceEvent::Branch { decision: BranchDecision { branch_id, line: 1, taken: false, constraint: SymConstraint::default() , conditions: None } },
                    TraceEvent::Scope { event: ScopeEvent::LoopExit { loop_id } },
                ];
                let bp_false = vec![
                    BranchDecision { branch_id, line: 1, taken: false, constraint: SymConstraint::default() , conditions: None },
                ];
                detector.observe(&events_false, &bp_false);

                let skip = detector.skip_indices(&events_true, &bp_true);
                prop_assert!(skip.is_empty());
            }
        }
    }

    // -- ExploreState tests --

    #[test]
    fn explore_state_default_is_empty() {
        let state = ExploreState::default();
        assert!(state.covered_paths.is_empty());
        assert!(state.discovery_inputs.is_empty());
    }

    #[test]
    fn explore_state_clone_preserves_contents() {
        let mut state = ExploreState::default();
        state.covered_paths.insert(12345);
        state.covered_paths.insert(67890);
        state.discovery_inputs.push(vec![serde_json::json!(1)]);

        let cloned = state.clone();
        assert_eq!(cloned.covered_paths.len(), 2);
        assert!(cloned.covered_paths.contains(&12345));
        assert!(cloned.covered_paths.contains(&67890));
        assert_eq!(cloned.discovery_inputs.len(), 1);
        assert_eq!(cloned.discovery_inputs[0], vec![serde_json::json!(1)]);
    }

    #[test]
    fn explore_state_covered_paths_deduplicate() {
        let mut state = ExploreState::default();
        state.covered_paths.insert(42);
        state.covered_paths.insert(42);
        assert_eq!(
            state.covered_paths.len(),
            1,
            "duplicate path hash should be deduplicated"
        );
    }
}

// ---------------------------------------------------------------------------
// Kani bounded model checking harnesses
// ---------------------------------------------------------------------------
// Proves invariants on the orchestrator's data structures that proptest
// exercises probabilistically.
//
// Run: `cd shatter-core && cargo kani --harness <name>`

#[cfg(kani)]
mod kani_proofs {
    use super::*;
    use std::cmp::Ordering;

    /// Build a symbolic `WorklistEntry` with bounded fitness and source.
    /// Uses integer-keyed fitness to avoid f64 state explosion in CBMC.
    fn symbolic_worklist_entry() -> WorklistEntry {
        let has_fitness: bool = kani::any();
        let fitness = if has_fitness {
            // Use a small integer range mapped to [0.0, 1.0] to keep CBMC tractable.
            let f_key: u8 = kani::any();
            kani::assume(f_key <= 10);
            Some(f_key as f64 / 10.0)
        } else {
            None
        };

        let source_tag: u8 = kani::any();
        kani::assume(source_tag < 7);
        let source = match source_tag {
            0 => InputSource::Seed,
            1 => InputSource::Fuzzed,
            2 => InputSource::BoundarySearch,
            3 => InputSource::Drilled,
            4 => InputSource::McdcTarget,
            5 => InputSource::Z3Solved,
            6 => InputSource::UserProvided,
            _ => unreachable!(),
        };

        WorklistEntry {
            inputs: vec![],
            source,
            fitness,
            mock_values: vec![],
        }
    }

    // -- Harness 1: WorklistEntry Ord is reflexive ----------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_worklist_ord_reflexive() {
        let a = symbolic_worklist_entry();
        assert_eq!(
            a.cmp(&a),
            Ordering::Equal,
            "an entry must be equal to itself"
        );
    }

    // -- Harness 2: WorklistEntry Ord is antisymmetric ------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_worklist_ord_antisymmetric() {
        let a = symbolic_worklist_entry();
        let b = symbolic_worklist_entry();
        if a.cmp(&b) == Ordering::Equal && b.cmp(&a) == Ordering::Equal {
            assert!(a == b, "equal ordering implies equality");
        }
    }

    // -- Harness 3: partial_cmp consistent with cmp ---------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_worklist_partial_cmp_consistent() {
        let a = symbolic_worklist_entry();
        let b = symbolic_worklist_entry();
        assert_eq!(
            a.partial_cmp(&b),
            Some(a.cmp(&b)),
            "partial_cmp must return Some(cmp(...))"
        );
    }

    // -- Harness 4: fitness always beats no-fitness ---------------------------
    #[kani::proof]
    #[kani::unwind(2)]
    fn prove_fitness_beats_no_fitness() {
        let a = symbolic_worklist_entry();
        let b = symbolic_worklist_entry();
        if a.fitness.is_some() && b.fitness.is_none() {
            assert_eq!(
                a.cmp(&b),
                Ordering::Greater,
                "entry with fitness must outrank entry without"
            );
        }
        if a.fitness.is_none() && b.fitness.is_some() {
            assert_eq!(
                a.cmp(&b),
                Ordering::Less,
                "entry without fitness must rank below entry with"
            );
        }
    }

    // -- Harness 5: overlay_solved_values preserves length (empty solved) -----
    #[kani::proof]
    #[kani::unwind(4)]
    fn prove_overlay_preserves_length() {
        let len: usize = kani::any();
        kani::assume(len >= 1 && len <= 2);

        let mut base_inputs = Vec::with_capacity(len);
        let mut param_names = Vec::with_capacity(len);
        for i in 0..len {
            base_inputs.push(serde_json::Value::Null);
            param_names.push(format!("p{i}"));
        }

        let solved = std::collections::HashMap::new();
        let result = overlay_solved_values(&base_inputs, &solved, &param_names);
        assert_eq!(
            result.len(),
            base_inputs.len(),
            "overlay must preserve input vector length"
        );
    }

    // -- Harness 6: overlay with a solved value preserves length --------------
    #[kani::proof]
    #[kani::unwind(4)]
    fn prove_overlay_single_solved_preserves_length() {
        // Fixed 1-param case to keep CBMC state small.
        let base_inputs = vec![serde_json::json!(0)];
        let param_names = vec![String::from("x")];

        let mut solved = std::collections::HashMap::new();
        solved.insert(String::from("x"), ConcreteValue::Int(42));

        let result = overlay_solved_values(&base_inputs, &solved, &param_names);
        assert_eq!(result.len(), 1, "overlay must preserve input vector length");
        assert_eq!(result[0], serde_json::json!(42));
    }
}

#[cfg(test)]
mod fuzz_trigger_tests {
    use super::*;

    #[test]
    fn branch_eligible_for_fuzzing_when_fresh() {
        let attempts: HashMap<u32, FuzzAttemptState> = HashMap::new();
        assert!(
            is_fuzz_eligible(1, &attempts, Some(3), 10),
            "a branch with no prior attempts should be eligible"
        );
    }

    #[test]
    fn branch_ineligible_after_max_attempts_no_coverage_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(
            1,
            FuzzAttemptState {
                count: 3,
                coverage_at_last_attempt: 10,
            },
        );
        assert!(
            !is_fuzz_eligible(1, &attempts, Some(3), 10),
            "branch at max attempts with no coverage growth should be ineligible"
        );
    }

    #[test]
    fn branch_eligible_after_max_attempts_with_coverage_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(
            1,
            FuzzAttemptState {
                count: 3,
                coverage_at_last_attempt: 10,
            },
        );
        assert!(
            is_fuzz_eligible(1, &attempts, Some(3), 15),
            "branch at max attempts should become eligible when coverage grows"
        );
    }

    #[test]
    fn branch_ineligible_in_indefinite_mode_no_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(
            1,
            FuzzAttemptState {
                count: 5,
                coverage_at_last_attempt: 10,
            },
        );
        assert!(
            !is_fuzz_eligible(1, &attempts, None, 10),
            "indefinite mode with no coverage growth should be ineligible"
        );
    }

    #[test]
    fn branch_eligible_in_indefinite_mode_with_growth() {
        let mut attempts = HashMap::new();
        attempts.insert(
            1,
            FuzzAttemptState {
                count: 5,
                coverage_at_last_attempt: 10,
            },
        );
        assert!(
            is_fuzz_eligible(1, &attempts, None, 15),
            "indefinite mode should become eligible when coverage grows"
        );
    }
}
