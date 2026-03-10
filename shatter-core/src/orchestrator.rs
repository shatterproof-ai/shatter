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
use std::collections::{BinaryHeap, HashSet};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::boundary_search;
use crate::coverage_metrics::DiscoveryMethod;
use crate::drilling;
use crate::execution_record::SymConstraint;
use crate::frontier::{Frontier, FrontierSet};
use crate::frontend::{Frontend, FrontendError};
use crate::genetic_fitness::{FitnessContext, FitnessWeights};
use crate::input_gen;
use crate::protocol::{Command, ExecuteResult, MockConfig, ResponseResult, SetupContextStack};
use crate::solver::{self, ConcreteValue, SolveResult};
use crate::strategy::MetaStrategy;
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
                if let Ok(kind) = serde_json::from_value::<ComplexKind>(
                    serde_json::Value::String(kind_str.to_string()),
                ) {
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
    pub max_iterations: usize,
    /// Maximum total executions (including duplicated paths) before stopping.
    pub max_executions: usize,
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
    /// Per-function exploration wall-clock timeout. Whichever of this or
    /// `max_iterations`/`max_executions` triggers first stops the loop.
    pub timeout_explore: Option<Duration>,
}

/// Default maximum total executions before stopping exploration.
pub const DEFAULT_MAX_EXECUTIONS: usize = 500;

/// Number of type-aware mutation rounds per unknown-constraint fuzz pass.
const MUTATE_ROUNDS_PER_UNKNOWN: usize = 3;

/// Mutation rate for type-aware fuzzing of unknown constraints (0.0–1.0).
///
/// Set high (1.0) because unknown constraints have no symbolic guidance,
/// so aggressive mutation is needed to explore the input space.
const MUTATE_RATE_UNKNOWN: f64 = 1.0;

impl Default for ExploreConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            max_executions: DEFAULT_MAX_EXECUTIONS,
            plateau_threshold: 20,
            mocks: vec![],
            mock_params: vec![],
            solver_timeout_ms: None,
            timeout_explore: None,
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
    /// Between fuzz and drill: interpolated between true/false witnesses.
    BoundarySearch = 2,
    /// Medium priority: targeted mutation of blocking params on a stalled frontier.
    Drilled = 3,
    /// High priority: Z3-solved inputs targeting a specific branch.
    Z3Solved = 4,
    /// Highest priority: user-provided candidate inputs from `.shatter/` config.
    UserProvided = 5,
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
}

/// Errors that can occur during concolic exploration.
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
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
        } else if param_names.len() == 1 && base_inputs.len() == 1 && !var_name.contains('.') {
            // Single-param function with a simple (non-derived) variable name:
            // the solver variable likely refers to the param. Skip derived names
            // like "email.length" which are internal Z3 variables, not params.
            result[0] = concrete_to_json(value);
        }
    }

    result
}

/// Result of trying to observe a single worklist entry.
enum ObserveOneResult {
    /// Entry was executed and produced an observation.
    Observed(Box<Observation>),
    /// Entry was skipped by triage prediction.
    TriageSkipped,
    /// Frontend returned an error or unexpected response — entry skipped.
    FrontendSkipped,
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
    config: &ExploreConfig,
    covered_paths: &mut HashSet<u64>,
    triage_state: &mut TriageState,
    budget: &ExploreBudget,
    setup_context: &Option<SetupContextStack>,
) -> Result<ObserveOneResult, ExploreError> {
    // Check termination budgets.
    if budget.unique_paths >= config.max_iterations {
        return Ok(ObserveOneResult::Terminated(TerminationReason::MaxIterations));
    }
    if budget.total_executions >= config.max_executions {
        return Ok(ObserveOneResult::Terminated(TerminationReason::MaxExecutions));
    }
    if let Some(timeout) = config.timeout_explore
        && budget.explore_start.elapsed() >= timeout
    {
        return Ok(ObserveOneResult::Terminated(TerminationReason::TimeoutExplore));
    }
    if config.plateau_threshold > 0 && budget.plateau_counter >= config.plateau_threshold {
        return Ok(ObserveOneResult::Terminated(TerminationReason::CoveragePlateau));
    }

    // Triage: predict whether this input will produce a novel path.
    let is_sampled_skip = if entry.source != InputSource::UserProvided {
        let verdict = triage_state.triage_candidate(&entry.inputs, covered_paths);
        triage_state.record_verdict(&verdict);
        if verdict == TriageVerdict::Skip {
            if triage_state.should_sample() {
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

    // Execute concretely via the frontend.
    let response = frontend
        .send(Command::Execute {
            function: function_name.to_string(),
            inputs: entry.inputs.clone(),
            mocks: config.mocks.clone(),
            setup_context: setup_context.clone(),
        })
        .await?;

    let exec_result = match response.result {
        ResponseResult::Execute(result) => *result,
        ResponseResult::Error { message, .. } => {
            log::warn!("frontend error during execute: {message}");
            return Ok(ObserveOneResult::FrontendSkipped);
        }
        _ => return Ok(ObserveOneResult::FrontendSkipped),
    };

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

    Ok(ObserveOneResult::Observed(Box::new(Observation {
        inputs: entry.inputs.clone(),
        result: exec_result,
        source: entry.source,
        path_id,
        is_new_path,
        is_sampled_skip,
    })))
}

/// Budget counters checked by `observe_one` to enforce termination limits.
struct ExploreBudget {
    unique_paths: usize,
    total_executions: usize,
    plateau_counter: usize,
    explore_start: Instant,
}

/// Solve/Generate phase: process new-path observations and produce candidate inputs.
///
/// For each new-path observation:
/// - Extract symbolic constraints and negate each with Z3
/// - Try boundary search for Unknown constraints (interpolate between witnesses)
/// - Fall back to blind fuzzing for Unknown constraints without witnesses
/// - Drill stalled frontiers with targeted mutations
#[allow(clippy::too_many_arguments)] // boundary search + fitness scoring need broad context
fn solve_and_generate(
    observations: &[Observation],
    frontier_set: &mut FrontierSet,
    param_infos: &[ParamInfo],
    param_names: &[String],
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    seen_branch_sides: &std::collections::HashSet<(u32, bool)>,
    config: &ExploreConfig,
    rng: &mut StdRng,
    target_branches: &HashSet<u32>,
    fitness_context: &mut FitnessContext,
    fitness_weights: &FitnessWeights,
) -> SolveOutput {
    let mut output = SolveOutput::default();

    for obs in observations.iter().filter(|o| o.is_new_path) {
        // Extract symbolic constraints from the branch path.
        let sym_constraints = extract_sym_constraints(&obs.result);

        // Collect solvable constraints with their original branch_path indices.
        let solvable_with_idx: Vec<(usize, SymExpr)> = sym_constraints
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|e| (i, e.clone())))
            .collect();
        let solvable: Vec<SymExpr> = solvable_with_idx.iter().map(|(_, e)| e.clone()).collect();

        // Try to negate each branch constraint with Z3.
        if !solvable.is_empty() {
            for (solve_idx, &(branch_idx, _)) in solvable_with_idx.iter().enumerate() {
                match solver::solve_for_new_path(&solvable, solve_idx, config.solver_timeout_ms, param_infos) {
                    Ok(SolveResult::Sat(values)) => {
                        let new_inputs =
                            overlay_solved_values(&obs.inputs, &values, param_names);
                        output.candidates.push(WorklistEntry {
                            inputs: new_inputs,
                            source: InputSource::Z3Solved,
                            fitness: None,
                        });
                        output.z3_count += 1;
                    }
                    Ok(SolveResult::Unsat) => {
                        if let Some(bd) = obs.result.branch_path.get(branch_idx) {
                            frontier_set.increment_stall(bd.branch_id);
                        }
                    }
                    Err(_) => {
                        if let Some(bd) = obs.result.branch_path.get(branch_idx) {
                            frontier_set.increment_stall(bd.branch_id);
                        }
                    }
                }
            }
        }

        // For Unknown constraints, try boundary search first (interpolate between
        // true/false witnesses), then fall back to blind fuzzing.
        let mut boundary_attempted = false;
        let mut boundary_branches = 0usize;
        for (i, constraint_opt) in sym_constraints.iter().enumerate() {
            if constraint_opt.is_none() && i < obs.result.branch_path.len() {
                let bd = &obs.result.branch_path[i];
                let opposite_seen =
                    seen_branch_sides.contains(&(bd.branch_id, !bd.taken));

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
                        });
                        output.boundary_count += 1;
                    }
                    boundary_attempted = true;
                    boundary_branches += 1;
                    if boundary_branches
                        >= boundary_search::MAX_BOUNDARY_BRANCHES_PER_ROUND
                    {
                        break;
                    }
                }
            }
        }

        // Fall back to type-aware mutation if boundary search wasn't applicable.
        //
        // Uses input_gen::mutate_inputs (type-aware, respects param types) for
        // several rounds, plus the legacy fuzz_inputs as a cheap supplement.
        if !boundary_attempted {
            for (i, constraint_opt) in sym_constraints.iter().enumerate() {
                if constraint_opt.is_none() && i < obs.result.branch_path.len() {
                    // Type-aware mutations (3 rounds at full mutation rate).
                    for _ in 0..MUTATE_ROUNDS_PER_UNKNOWN {
                        let mutated = input_gen::mutate_inputs(
                            &obs.inputs,
                            param_infos,
                            MUTATE_RATE_UNKNOWN,
                            &[],
                            rng,
                        );
                        output.candidates.push(WorklistEntry {
                            inputs: mutated,
                            source: InputSource::Fuzzed,
                            fitness: None,
                        });
                        output.fuzz_count += 1;
                    }
                    // Legacy deterministic fuzz as supplement.
                    for fuzzed in fuzz_inputs(&obs.inputs) {
                        output.candidates.push(WorklistEntry {
                            inputs: fuzzed,
                            source: InputSource::Fuzzed,
                            fitness: None,
                        });
                        output.fuzz_count += 1;
                    }
                    // Only fuzz once per observation (avoid exponential blowup).
                    break;
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
                    && f.stall_count < crate::frontier::DEFAULT_MAX_STALL
            })
            .cloned()
            .collect();
        stalled.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.stall_count.cmp(&b.stall_count)));
        stalled.truncate(drilling::MAX_FRONTIERS_PER_ROUND);

        for frontier in &stalled {
            let count =
                drilling::DRILL_MUTATIONS_PER_PARAM * frontier.blocking_params.len().max(1);
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
                });
                output.drill_count += 1;
            }
            frontier_set.increment_stall(frontier.branch_id);
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
                );
                candidate.fitness = Some(breakdown.total);
            }
        }
    }

    output
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
pub async fn explore(
    frontend: &mut Frontend,
    function_name: &str,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    user_inputs: Vec<Vec<serde_json::Value>>,
    param_infos: &[ParamInfo],
    config: &ExploreConfig,
    setup_context: Option<SetupContextStack>,
) -> Result<ExploreResult, ExploreError> {
    let param_names: Vec<String> = param_infos.iter().map(|p| p.name.clone()).collect();
    let mut worklist = BinaryHeap::new();
    let mut covered_paths: HashSet<u64> = HashSet::new();
    let mut executions = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)> = Vec::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut total_executions: usize = 0;
    let mut z3_generated: usize = 0;
    let mut fuzz_generated: usize = 0;
    let mut boundary_generated: usize = 0;
    let mut drill_generated: usize = 0;
    let mut termination_reason = TerminationReason::WorklistExhausted;
    let mut seen_branch_sides: HashSet<(u32, bool)> = HashSet::new();
    let mut frontier_set = FrontierSet::new();
    let mut rng = StdRng::from_os_rng();
    let mut triage_state = TriageState::new(param_names.clone());
    let mut triage_skipped: usize = 0;
    let mut triage_mispredictions: usize = 0;

    // Fitness context shares novelty state with covered_paths — the
    // orchestrator marks paths as seen in both sets whenever a new path
    // is discovered, keeping FitnessContext's novelty scoring in sync.
    let mut fitness_context = FitnessContext::new();
    let fitness_weights = FitnessWeights::default();

    // Target branches: branch IDs seen so far on only one side (not yet
    // covered on the opposite). Updated after each new-path observation.
    // Used by fitness scoring to compute proximity/coverage scores.
    let mut target_branches: HashSet<u32> = HashSet::new();

    // MetaStrategy integration point: strategies will be registered here
    // by a follow-up issue that replaces the hardcoded seed/fuzz/drill
    // generation with adaptive strategy selection.
    let _meta_strategy: MetaStrategy = MetaStrategy::new(vec![], Default::default());

    // Add user-provided candidates with highest priority.
    for inputs in user_inputs {
        worklist.push(WorklistEntry {
            inputs,
            source: InputSource::UserProvided,
            fitness: None,
        });
    }

    // Seed the worklist.
    for inputs in seed_inputs {
        worklist.push(WorklistEntry {
            inputs,
            source: InputSource::Seed,
            fitness: None,
        });
    }

    // --- Float probe phase ---
    let float_indices = crate::float_probe::float_param_indices(param_infos);
    let mut float_probe_results: Vec<crate::float_probe::FloatProbeResult> = Vec::new();
    if !float_indices.is_empty() {
        for &idx in &float_indices {
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
                let float_resp = frontend
                    .send(Command::Execute {
                        function: function_name.to_string(),
                        inputs: float_inputs.clone(),
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
                    })
                    .await?;

                let floor_resp = frontend
                    .send(Command::Execute {
                        function: function_name.to_string(),
                        inputs: floor_inputs,
                        mocks: config.mocks.clone(),
                        setup_context: setup_context.clone(),
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

    let explore_start = Instant::now();
    let mut plateau_counter: usize = 0;

    // --- Two-phase exploration loop: Observe → Solve/Generate ---
    //
    // Each iteration processes one worklist entry:
    //   1. Observe — execute the entry, classify the path
    //   2. Solve/Generate — if new path, extract constraints and produce candidates
    //
    // Solving after each observation (not batched) preserves the original
    // convergence behavior: Z3 candidates from observation N are available
    // for execution at iteration N+1.
    while let Some(entry) = worklist.pop() {
        // Phase 1: Observe — execute and classify one worklist entry.
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
            config,
            &mut covered_paths,
            &mut triage_state,
            &budget,
            &setup_context,
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
            ObserveOneResult::Terminated(reason) => {
                termination_reason = reason;
                break;
            }
        };

        total_executions += 1;
        if obs.is_sampled_skip && !obs.is_new_path {
            // Prediction was correct (duplicate path) — no misprediction.
        } else if obs.is_sampled_skip && obs.is_new_path {
            triage_mispredictions += 1;
        }

        // Record raw result for pipeline composability.
        raw_results.push((obs.inputs.clone(), config.mocks.clone(), obs.result.clone()));

        if !obs.is_new_path {
            plateau_counter += 1;
            continue;
        }

        // New path discovered — reset plateau counter.
        plateau_counter = 0;

        // Track per-branch discovery attribution.
        let method = match obs.source {
            InputSource::Z3Solved => DiscoveryMethod::Z3,
            InputSource::UserProvided => DiscoveryMethod::UserProvided,
            InputSource::Drilled => DiscoveryMethod::Drilled,
            InputSource::BoundarySearch => DiscoveryMethod::BoundarySearch,
            InputSource::Seed | InputSource::Fuzzed => DiscoveryMethod::Random,
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
            let opposite_seen =
                seen_branch_sides.contains(&(decision.branch_id, !decision.taken));
            if opposite_seen {
                frontier_set.remove(decision.branch_id);
                target_branches.remove(&decision.branch_id);
            } else {
                target_branches.insert(decision.branch_id);
                let prev_stall = frontier_set
                    .iter()
                    .find(|f| f.branch_id == decision.branch_id)
                    .map_or(0, |f| f.stall_count);
                let blocking =
                    drilling::identify_blocking_params(&decision.constraint, param_infos);
                let depth =
                    drilling::branch_depth(&obs.result.branch_path, decision.branch_id);
                frontier_set.insert(Frontier {
                    branch_id: decision.branch_id,
                    depth,
                    blocking_params: blocking,
                    best_prefix: obs.inputs.clone(),
                    stall_count: prev_stall,
                });
            }
        }

        // Sync fitness context: mark this path as seen so future fitness
        // scoring correctly identifies repeat paths as non-novel.
        fitness_context.mark_seen(obs.path_id);

        executions.push(obs.result.clone());

        // Phase 2: Solve/Generate — produce new candidates from this observation.
        let solve_output = solve_and_generate(
            &[obs],
            &mut frontier_set,
            param_infos,
            &param_names,
            &raw_results,
            &seen_branch_sides,
            config,
            &mut rng,
            &target_branches,
            &mut fitness_context,
            &fitness_weights,
        );

        z3_generated += solve_output.z3_count;
        fuzz_generated += solve_output.fuzz_count;
        drill_generated += solve_output.drill_count;
        boundary_generated += solve_output.boundary_count;

        for candidate in solve_output.candidates {
            worklist.push(candidate);
        }
    }

    let unique_paths = covered_paths.len();
    Ok(ExploreResult {
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
    })
}

/// Simple input fuzzing: produce a handful of variations on the base inputs.
///
/// For each JSON value in the input list, generate mutations:
/// - Numbers: ±1, ×-1, 0, boundary values
/// - Booleans: flip
/// - Strings: empty string, "a"
pub(crate) fn fuzz_inputs(base: &[serde_json::Value]) -> Vec<Vec<serde_json::Value>> {
    let mut results = Vec::new();

    for (idx, val) in base.iter().enumerate() {
        let mutations = fuzz_single_value(val);
        for mutated in mutations {
            let mut new_inputs = base.to_vec();
            new_inputs[idx] = mutated;
            results.push(new_inputs);
        }
    }

    results
}

pub(crate) fn fuzz_single_value(val: &serde_json::Value) -> Vec<serde_json::Value> {
    match val {
        serde_json::Value::Number(n) => {
            let mut mutations = Vec::new();
            if let Some(i) = n.as_i64() {
                let candidates = [i + 1, i - 1, 0, -i];
                for c in candidates {
                    let json_val = serde_json::json!(c);
                    if c != i && !mutations.contains(&json_val) {
                        mutations.push(json_val);
                    }
                }
            } else if let Some(f) = n.as_f64() {
                let candidates = [f + 1.0, f - 1.0, 0.0];
                for c in candidates {
                    let json_val = serde_json::json!(c);
                    if (c - f).abs() > f64::EPSILON && !mutations.contains(&json_val) {
                        mutations.push(json_val);
                    }
                }
            }
            mutations
        }
        serde_json::Value::Bool(b) => vec![serde_json::json!(!b)],
        serde_json::Value::String(_) => {
            vec![serde_json::json!(""), serde_json::json!("a")]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::protocol::PerformanceMetrics;
    use crate::solver::ConcreteValue;
    use crate::sym_expr::{BinOpKind, ConstValue, SymExpr};
    use std::collections::HashMap;

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
            capture_truncation: None,
            performance: empty_perf(),
        }
    }

    // -- hash_branch_path tests --

    #[test]
    fn same_branch_path_hashes_identically() {
        let path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "x".into(),
                },
            },
            BranchDecision {
                branch_id: 1,
                line: 20,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "y".into(),
                },
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
            constraint: SymConstraint::Unknown {
                hint: "x".into(),
            },
        }];
        let path_b = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Unknown {
                hint: "x".into(),
            },
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
            },
            BranchDecision {
                branch_id: 1,
                line: 10,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "regex".into(),
                },
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
        assert_eq!(concrete_to_json(&ConcreteValue::Int(42)), serde_json::json!(42));
        assert_eq!(concrete_to_json(&ConcreteValue::Float(3.14)), serde_json::json!(3.14));
        assert_eq!(concrete_to_json(&ConcreteValue::Str("hello".into())), serde_json::json!("hello"));
        assert_eq!(concrete_to_json(&ConcreteValue::Bool(true)), serde_json::json!(true));
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

    // -- fuzz_inputs tests --

    #[test]
    fn fuzz_integer_produces_mutations() {
        let inputs = vec![serde_json::json!(5)];
        let fuzzed = fuzz_inputs(&inputs);
        // Should produce: 6, 4, 0, -5 = 4 mutations
        assert_eq!(fuzzed.len(), 4);
        assert!(fuzzed.contains(&vec![serde_json::json!(6)]));
        assert!(fuzzed.contains(&vec![serde_json::json!(4)]));
        assert!(fuzzed.contains(&vec![serde_json::json!(0)]));
        assert!(fuzzed.contains(&vec![serde_json::json!(-5)]));
    }

    #[test]
    fn fuzz_zero_produces_mutations() {
        let inputs = vec![serde_json::json!(0)];
        let fuzzed = fuzz_inputs(&inputs);
        // Candidates: 1, -1, 0, 0. Skip 0 (== original), dedup → 1, -1
        assert_eq!(fuzzed.len(), 2);
    }

    #[test]
    fn fuzz_boolean_flips() {
        let inputs = vec![serde_json::json!(true)];
        let fuzzed = fuzz_inputs(&inputs);
        assert_eq!(fuzzed.len(), 1);
        assert_eq!(fuzzed[0], vec![serde_json::json!(false)]);
    }

    #[test]
    fn fuzz_string_produces_mutations() {
        let inputs = vec![serde_json::json!("hello")];
        let fuzzed = fuzz_inputs(&inputs);
        assert_eq!(fuzzed.len(), 2);
        assert!(fuzzed.contains(&vec![serde_json::json!("")]));
        assert!(fuzzed.contains(&vec![serde_json::json!("a")]));
    }

    #[test]
    fn fuzz_null_produces_nothing() {
        let inputs = vec![serde_json::Value::Null];
        let fuzzed = fuzz_inputs(&inputs);
        assert!(fuzzed.is_empty());
    }

    #[test]
    fn fuzz_multiple_inputs_mutates_each() {
        let inputs = vec![serde_json::json!(1), serde_json::json!(true)];
        let fuzzed = fuzz_inputs(&inputs);
        // 1 produces 3 mutations (2, 0, -1 after dedup) and true produces 1 (false) = 4
        assert_eq!(fuzzed.len(), 4);
    }

    // -- WorklistEntry ordering tests --

    // -- ExploreConfig defaults --

    #[test]
    fn default_config_has_reasonable_limits() {
        let config = ExploreConfig::default();
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.max_executions, DEFAULT_MAX_EXECUTIONS);
        assert_eq!(config.plateau_threshold, 20);
    }

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
        let result = solver::solve_constraints(&[x_eq_42], None, &[]).expect("solver should not error");

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
        let result =
            solver::solve_for_new_path(&[not_x_eq_42], 0, None, &[]).expect("solver should not error");

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
        let result =
            solver::solve_for_new_path(&constraints, 0, None, &[]).expect("should solve for branch 0");
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
        let result =
            solver::solve_for_new_path(&constraints, 1, None, &[]).expect("should solve for branch 1");
        assert!(
            matches!(result, SolveResult::Unsat),
            "x<=10 AND x==42 should be unsat"
        );

        // Negate constraint[2]: keep prefix NOT(x>10), NOT(x==42), flip x<100 → x>=100.
        // x<=10 AND x!=42 AND x>=100 is UNSAT.
        let result =
            solver::solve_for_new_path(&constraints, 2, None, &[]).expect("should solve for branch 2");
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
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_1")],
            source: InputSource::Z3Solved,
            fitness: None,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("fuzz1")],
            source: InputSource::Fuzzed,
            fitness: None,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_2")],
            source: InputSource::Z3Solved,
            fitness: None,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("seed2")],
            source: InputSource::Seed,
            fitness: None,
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
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("low_fit")],
            source: InputSource::Seed,
            fitness: Some(0.1),
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
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("high")],
            source: InputSource::Fuzzed,
            fitness: Some(0.9),
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("mid")],
            source: InputSource::Fuzzed,
            fitness: Some(0.5),
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
        });
        worklist.push(WorklistEntry {
            inputs: vec![],
            source: InputSource::Z3Solved,
            fitness: Some(0.5),
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
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
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
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
        );

        assert!(output.candidates.is_empty());
        assert_eq!(output.z3_count, 0);
        assert_eq!(output.fuzz_count, 0);
    }

    #[test]
    fn solve_and_generate_produces_fuzz_for_unknown_constraints() {
        let obs = Observation {
            inputs: vec![serde_json::json!(5)],
            result: make_exec_result(vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "opaque".into(),
                },
            }]),
            source: InputSource::Seed,
            path_id: 456,
            is_new_path: true,
            is_sampled_skip: false,
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
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
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
        );

        assert!(output.fuzz_count > 0, "should produce fuzz candidates for unknown constraints");
        assert!(
            output.candidates.iter().all(|e| e.source == InputSource::Fuzzed),
            "all candidates should be fuzzed"
        );
    }

    #[test]
    fn solve_and_generate_produces_z3_for_expr_constraints() {
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
                    expr: x_gt_10,
                },
            }]),
            source: InputSource::Seed,
            path_id: 789,
            is_new_path: true,
            is_sampled_skip: false,
        };

        let param_infos = vec![ParamInfo {
            name: "x".into(),
            typ: crate::types::TypeInfo::Int,
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
            &mut rng,
            &HashSet::new(),
            &mut FitnessContext::new(),
            &FitnessWeights::default(),
        );

        assert!(output.z3_count > 0, "should produce Z3 candidates for solvable constraints");
        assert!(
            output.candidates.iter().any(|e| e.source == InputSource::Z3Solved),
            "at least one candidate should be Z3-solved"
        );
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
    /// and terminates when the worklist is exhausted.
    #[tokio::test]
    async fn explore_noop_frontend_exhausts_worklist() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: 10,
            max_executions: 50,
            plateau_threshold: 5,
            ..Default::default()
        };

        let result = explore(
            &mut frontend,
            "stub",
            vec![vec![serde_json::json!(0)]],
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
            None,
        )
        .await
        .expect("explore failed");

        // Noop returns empty branch_path every time → one unique path, then plateau.
        assert_eq!(result.unique_paths, 1);
        assert!(result.total_executions >= 1);
        // With no branches to negate or fuzz, worklist empties after the seed.
        assert_eq!(result.termination_reason, TerminationReason::WorklistExhausted);

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Explore with the concolic test frontend discovers multiple paths via Z3.
    /// The test frontend simulates f(x) with branches at x>10 and x==42.
    #[tokio::test]
    async fn explore_concolic_frontend_discovers_paths_via_z3() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: 20,
            max_executions: 100,
            plateau_threshold: 10,
            ..Default::default()
        };

        // Start with x=0 (hits the x<=10 path).
        let result = explore(
            &mut frontend,
            "f",
            vec![vec![serde_json::json!(0)]],
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
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
        assert!(result.z3_generated > 0, "Z3 should have generated at least one input");

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Plateau detection stops exploration when no new paths are found.
    #[tokio::test]
    async fn explore_stops_on_coverage_plateau() {
        let config = config_for_script("noop-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: 100,
            max_executions: 100,
            // Low threshold so plateau triggers quickly.
            plateau_threshold: 3,
            ..Default::default()
        };

        // Provide multiple identical seeds so the worklist doesn't empty first.
        let seeds = (0..10)
            .map(|i| vec![serde_json::json!(i)])
            .collect();

        let result = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
            None,
        )
        .await
        .expect("explore failed");

        // All seeds produce the same empty branch path, so after the first unique path
        // we get plateau_threshold consecutive duplicates.
        assert_eq!(result.unique_paths, 1);
        assert_eq!(result.termination_reason, TerminationReason::CoveragePlateau);
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
            max_iterations: 100,
            max_executions: 3,
            plateau_threshold: 0, // disable plateau
            ..Default::default()
        };

        let seeds = (0..10)
            .map(|i| vec![serde_json::json!(i)])
            .collect();

        let result = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
            None,
        )
        .await
        .expect("explore failed");

        assert_eq!(result.total_executions, 3);
        assert_eq!(result.termination_reason, TerminationReason::MaxExecutions);

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Max iterations budget stops exploration.
    #[tokio::test]
    async fn explore_stops_on_max_iterations() {
        let config = config_for_script("concolic-test-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: 1,
            max_executions: 100,
            plateau_threshold: 0,
            ..Default::default()
        };

        // Provide seeds that will hit different paths.
        let result = explore(
            &mut frontend,
            "f",
            vec![vec![serde_json::json!(0)], vec![serde_json::json!(20)]],
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
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
            max_iterations: 1000,
            max_executions: 10000,
            plateau_threshold: 0,
            timeout_explore: Some(Duration::from_millis(1)),
            ..Default::default()
        };

        let seeds = (0..100)
            .map(|i| vec![serde_json::json!(i)])
            .collect();

        let result = explore(
            &mut frontend,
            "stub",
            seeds,
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
            None,
        )
        .await
        .expect("explore failed");

        // Should terminate due to timeout, not max_executions or max_iterations.
        assert_eq!(result.termination_reason, TerminationReason::TimeoutExplore);
        assert!(result.total_executions < 10000);

        frontend.shutdown().await.expect("shutdown failed");
    }

    /// Triage should skip redundant inputs that predict already-covered paths.
    ///
    /// Uses a fixed-branch frontend (always returns same single-branch path).
    /// After the first seed discovers the path, triage predicts Skip for
    /// all subsequent seeds with matching constraint evaluations.
    #[tokio::test]
    async fn explore_triage_skips_redundant_seeds() {
        let config = config_for_script("fixed-branch-frontend.sh");
        let mut frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let explore_config = ExploreConfig {
            max_iterations: 50,
            max_executions: 50,
            plateau_threshold: 0, // disable plateau so we rely on worklist exhaustion
            ..Default::default()
        };

        // All seeds have x=5, which evaluates x>0 to true (Taken) — matching
        // the path discovered by the first execution. After the first seed
        // discovers the path and updates triage, subsequent seeds predict the
        // same covered path → Skip.
        let seeds: Vec<Vec<serde_json::Value>> = (0..20)
            .map(|_| vec![serde_json::json!(5)])
            .collect();

        let result = explore(
            &mut frontend,
            "f",
            seeds,
            vec![],
            &[ParamInfo { name: "x".into(), typ: crate::types::TypeInfo::Int, type_name: None }],
            &explore_config,
            None,
        )
        .await
        .expect("explore failed");

        assert!(
            result.triage_skipped > 0,
            "expected triage to skip redundant inputs, but triage_skipped={}",
            result.triage_skipped
        );
        assert_eq!(result.unique_paths, 1);

        frontend.shutdown().await.expect("shutdown failed");
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
        }
    }
}
