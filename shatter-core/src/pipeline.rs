//! Staged pipeline: composable Observe → Analyze → Solve → Specify stages.
//!
//! Each stage is a pure function with well-defined input/output types.
//! - **Observe**: execute the function with various inputs, collect traces.
//! - **Analyze**: group into equivalence classes, build behavior map, compute coverage.
//! - **Solve**: for uncovered branches, use Z3 to find triggering inputs.
//! - **Specify**: generate behavioral specifications from full-coverage data.

use crate::behavior::BehaviorMap;
use crate::coverage_metrics::CoverageMetrics;
use crate::equivalence::{self, EquivalenceClass};
use crate::execution_record::{ExecutionRecord, SymConstraint};
use crate::explorer::ObservationOutput;
use crate::orchestrator::{extract_sym_constraints, overlay_solved_values};
use crate::protocol::{ExecuteResult, FunctionAnalysis};
use crate::solver::{self, SolveResult};
use crate::spec::FunctionSpec;
use crate::sym_expr::SymExpr;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

/// Output of the Analyze stage.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeOutput {
    /// Equivalence classes grouping executions by branch path.
    pub eq_classes: Vec<EquivalenceClass>,
    /// Behavior map built from execution records.
    pub behavior_map: BehaviorMap,
    /// Branch coverage metrics with per-method attribution.
    pub coverage_metrics: CoverageMetrics,
}

/// Bundled output of the Observe stage, suitable for serialization to disk.
/// Contains everything the Analyze stage needs as input.
#[derive(Debug, Serialize, Deserialize)]
pub struct ObserveStageOutput {
    /// Raw observation data from executing the function.
    pub observation: ObservationOutput,
    /// Static analysis results (function signature, branches, etc.).
    pub analysis: FunctionAnalysis,
    /// Source file path, for display and downstream stages.
    pub file: String,
}

/// Bundled output of the Analyze stage, suitable for serialization to disk.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeStageOutput {
    /// Analysis results (eq classes, behavior map, coverage).
    pub analyze: AnalyzeOutput,
    /// Optional behavioral specification.
    pub spec: Option<FunctionSpec>,
    /// Function name, carried forward for provenance.
    pub function_name: String,
    /// Source file path, carried forward for provenance.
    pub file: String,
}

/// Bundled output of the Solve stage, suitable for serialization to disk.
#[derive(Debug, Serialize, Deserialize)]
pub struct SolveStageOutput {
    /// Solve results for each uncovered branch.
    pub solve: StageSolveOutput,
    /// Function name, carried forward for provenance.
    pub function_name: String,
    /// Source file path, carried forward for provenance.
    pub file: String,
}

/// Result of the Solve stage — solved inputs for uncovered branches.
#[derive(Debug, Serialize, Deserialize)]
pub struct StageSolveOutput {
    /// Per-branch solve results.
    pub solved_branches: Vec<SolvedBranch>,
    /// Aggregate metrics for the solve pass.
    pub metrics: SolveMetrics,
}

/// A single branch solve attempt result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolvedBranch {
    /// Branch identifier from static analysis.
    pub branch_id: u32,
    /// Source line of the branch.
    pub line: u32,
    /// Whether we were trying to reach the true or false direction.
    pub target_taken: bool,
    /// Outcome of the solve attempt.
    pub outcome: SolveOutcome,
}

/// Outcome of attempting to solve for a single uncovered branch direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SolveOutcome {
    /// Z3 found satisfying inputs that should trigger this branch direction.
    Sat { inputs: Vec<serde_json::Value> },
    /// The constraint path is unsatisfiable — no inputs can reach this direction.
    Unsat,
    /// Branch has only opaque/unknown constraints — not solvable by Z3.
    Opaque { hint: String },
    /// Branch was never reached by any execution — no constraints available to solve.
    Unreachable,
    /// Solver error (timeout, unsupported expression, etc.).
    Error { message: String },
}

/// Aggregate metrics for the Solve stage.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SolveMetrics {
    /// Total number of uncovered branch directions targeted.
    pub total_uncovered: usize,
    /// Branches for which Z3 found satisfying inputs.
    pub sat_count: usize,
    /// Branches with unsatisfiable constraints.
    pub unsat_count: usize,
    /// Branches with opaque/unknown constraints.
    pub opaque_count: usize,
    /// Branches never reached by any execution.
    pub unreachable_count: usize,
    /// Branches where the solver returned an error.
    pub error_count: usize,
}

/// Bundled output of the Specify stage, suitable for serialization to disk.
///
/// Contains the final behavioral specification enriched with solve-stage
/// provenance, coverage completeness accounting, and test suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecifyStageOutput {
    /// Complete behavioral specification, enriched with solve provenance.
    pub spec: FunctionSpec,
    /// Coverage completeness after integrating solve results.
    pub coverage_completeness: CoverageCompleteness,
    /// Test suggestions derived from the spec and solve results.
    pub test_suggestions: Vec<TestSuggestion>,
    /// Function name, carried forward for provenance.
    pub function_name: String,
    /// Source file path, carried forward for provenance.
    pub file: String,
}

/// Coverage completeness summary after integrating all pipeline stages.
///
/// Unlike [`CoverageMetrics`] (which tracks *how* branches were discovered),
/// this tracks the final accounting of every branch direction: observed,
/// proven satisfiable, proven unsatisfiable, opaque, or unreachable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageCompleteness {
    /// Total branch directions in the function (2 × branch count).
    pub total_branch_directions: usize,
    /// Branch directions observed during execution (observe stage).
    pub observed: usize,
    /// Branch directions proven satisfiable by Z3 (solve stage).
    pub proven_sat: usize,
    /// Branch directions proven unsatisfiable (solve stage).
    pub proven_unsat: usize,
    /// Branch directions with opaque/unknown constraints.
    pub opaque: usize,
    /// Branch directions never reached by any execution.
    pub unreachable: usize,
    /// Branch directions where the solver returned an error.
    pub solver_errors: usize,
    /// Percentage of directions fully accounted for:
    /// (observed + proven_sat + proven_unsat) / total × 100.
    pub completeness_pct: f64,
}

/// A test suggestion derived from the behavioral specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestSuggestion {
    /// Human-readable description of what this test covers.
    pub description: String,
    /// Input arguments for the test.
    pub inputs: Vec<serde_json::Value>,
    /// Expected return value (if the behavior returns normally).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_return: Option<serde_json::Value>,
    /// Expected error message (if the behavior throws).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    /// How this test suggestion was derived.
    pub source: TestSuggestionSource,
}

/// Origin of a test suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSuggestionSource {
    /// Derived from an observed execution (canonical example from eq class).
    Observed,
    /// Derived from a Z3-solved input (not yet executed).
    Solved,
}

/// Error type for stage I/O operations.
#[derive(Debug)]
pub enum StageIoError {
    /// Filesystem I/O error.
    Io(std::io::Error),
    /// JSON serialization or deserialization error.
    Json(serde_json::Error),
}

impl std::fmt::Display for StageIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "stage I/O error: {e}"),
            Self::Json(e) => write!(f, "stage JSON error: {e}"),
        }
    }
}

impl std::error::Error for StageIoError {}

impl From<std::io::Error> for StageIoError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for StageIoError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Read an [`ObserveStageOutput`] from a JSON file on disk.
pub fn read_observe_stage(path: &Path) -> Result<ObserveStageOutput, StageIoError> {
    let data = std::fs::read_to_string(path)?;
    let output = serde_json::from_str(&data)?;
    Ok(output)
}

/// Write an [`ObserveStageOutput`] to a JSON file on disk.
pub fn write_observe_stage(output: &ObserveStageOutput, path: &Path) -> Result<(), StageIoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(output)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Read an [`AnalyzeStageOutput`] from a JSON file on disk.
pub fn read_analyze_stage(path: &Path) -> Result<AnalyzeStageOutput, StageIoError> {
    let data = std::fs::read_to_string(path)?;
    let output = serde_json::from_str(&data)?;
    Ok(output)
}

/// Write an [`AnalyzeStageOutput`] to a JSON file on disk.
pub fn write_analyze_stage(output: &AnalyzeStageOutput, path: &Path) -> Result<(), StageIoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(output)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Read a [`SolveStageOutput`] from a JSON file on disk.
pub fn read_solve_stage(path: &Path) -> Result<SolveStageOutput, StageIoError> {
    let data = std::fs::read_to_string(path)?;
    let output = serde_json::from_str(&data)?;
    Ok(output)
}

/// Write a [`SolveStageOutput`] to a JSON file on disk.
pub fn write_solve_stage(output: &SolveStageOutput, path: &Path) -> Result<(), StageIoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(output)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Read a [`SpecifyStageOutput`] from a JSON file on disk.
pub fn read_specify_stage(path: &Path) -> Result<SpecifyStageOutput, StageIoError> {
    let data = std::fs::read_to_string(path)?;
    let output = serde_json::from_str(&data)?;
    Ok(output)
}

/// Write a [`SpecifyStageOutput`] to a JSON file on disk.
pub fn write_specify_stage(output: &SpecifyStageOutput, path: &Path) -> Result<(), StageIoError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(output)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Run the Solve stage: attempt Z3 constraint solving for uncovered branch directions.
///
/// For each branch in the static analysis that was only observed in one direction
/// (e.g., `taken=true` but never `taken=false`), this stage:
/// 1. Finds an execution that visited the branch in the opposite direction
/// 2. Extracts the symbolic constraint path up to that branch
/// 3. Negates the target constraint and calls Z3 to find triggering inputs
///
/// Branches never reached by any execution are reported as [`SolveOutcome::Unreachable`].
/// Branches with opaque/unknown constraints are reported as [`SolveOutcome::Opaque`].
pub fn solve(observe: &ObserveStageOutput, solver_timeout_ms: Option<u64>) -> StageSolveOutput {
    let analysis = &observe.analysis;
    let raw_results = &observe.observation.raw_results;

    // Collect observed branch directions: branch_id → set of taken values.
    let mut observed_directions: HashMap<u32, HashSet<bool>> = HashMap::new();
    for (_, _mocks, result) in raw_results {
        for decision in &result.branch_path {
            observed_directions
                .entry(decision.branch_id)
                .or_default()
                .insert(decision.taken);
        }
    }

    // For each analysis branch, identify uncovered directions.
    let mut solved_branches = Vec::new();
    let mut metrics = SolveMetrics::default();

    for branch_info in &analysis.branches {
        let directions = observed_directions.get(&branch_info.id);

        // Check which directions are missing.
        let missing: Vec<bool> = match directions {
            None => {
                // Branch never reached by any execution.
                vec![true, false]
            }
            Some(seen) => {
                let mut missing = Vec::new();
                if !seen.contains(&true) {
                    missing.push(true);
                }
                if !seen.contains(&false) {
                    missing.push(false);
                }
                missing
            }
        };

        for target_taken in missing {
            metrics.total_uncovered += 1;

            let outcome = if directions.is_none() {
                // No execution ever reached this branch.
                metrics.unreachable_count += 1;
                SolveOutcome::Unreachable
            } else {
                // Find the best execution that visited this branch in the opposite direction.
                solve_for_branch_direction(
                    branch_info.id,
                    target_taken,
                    raw_results,
                    &analysis.params,
                    &analysis.loops,
                    solver_timeout_ms,
                    &mut metrics,
                )
            };

            solved_branches.push(SolvedBranch {
                branch_id: branch_info.id,
                line: branch_info.line,
                target_taken,
                outcome,
            });
        }
    }

    StageSolveOutput {
        solved_branches,
        metrics,
    }
}

/// Attempt to solve for a single uncovered branch direction.
///
/// Finds an execution that visited the branch in the opposite direction,
/// extracts its constraint path, and calls Z3 to negate the target constraint.
fn solve_for_branch_direction(
    branch_id: u32,
    target_taken: bool,
    raw_results: &[(
        Vec<serde_json::Value>,
        Vec<crate::protocol::MockConfig>,
        ExecuteResult,
    )],
    param_infos: &[crate::types::ParamInfo],
    loops: &[crate::protocol::LoopInfo],
    solver_timeout_ms: Option<u64>,
    metrics: &mut SolveMetrics,
) -> SolveOutcome {
    // Find an execution that visited this branch in the opposite direction.
    let opposite_taken = !target_taken;
    let witness = raw_results.iter().find(|(_, _, result)| {
        result
            .branch_path
            .iter()
            .any(|d| d.branch_id == branch_id && d.taken == opposite_taken)
    });

    let (base_inputs, _mocks, witness_result) = match witness {
        Some(w) => w,
        None => {
            // Should not happen (we checked directions.is_some()), but handle gracefully.
            metrics.unreachable_count += 1;
            return SolveOutcome::Unreachable;
        }
    };

    // Find the branch's position in the witness execution's branch_path.
    let branch_idx = match witness_result
        .branch_path
        .iter()
        .position(|d| d.branch_id == branch_id)
    {
        Some(idx) => idx,
        None => {
            metrics.error_count += 1;
            return SolveOutcome::Error {
                message: format!("branch {branch_id} not found in witness path"),
            };
        }
    };

    // Check if the target branch has a solvable constraint.
    let target_decision = &witness_result.branch_path[branch_idx];
    if let SymConstraint::Unknown { hint } = &target_decision.constraint {
        metrics.opaque_count += 1;
        return SolveOutcome::Opaque { hint: hint.clone() };
    }

    // Extract symbolic constraints from the witness execution.
    let raw_constraints = extract_sym_constraints(witness_result);

    // Apply loop constraint rewriting if loops are present.
    let rewritten =
        crate::loop_analysis::rewrite_loop_constraints(&raw_constraints, loops, witness_result);
    let rewritten = crate::loop_analysis::merge_loop_states(&rewritten, loops, witness_result);

    // Build the solvable-only constraint list (filtering out None/Unknown entries).
    // We need constraints up to and including the target branch.
    let prefix_with_target = &rewritten[..=branch_idx];
    let solvable: Vec<SymExpr> = prefix_with_target
        .iter()
        .filter_map(|opt| opt.clone())
        .collect();

    if solvable.is_empty() {
        metrics.opaque_count += 1;
        return SolveOutcome::Opaque {
            hint: "no solvable constraints in path prefix".into(),
        };
    }

    // The target constraint is the last solvable entry that corresponds to our branch.
    // Count how many Some entries exist up to and including branch_idx to find the
    // solvable index of our target.
    let solvable_idx = prefix_with_target
        .iter()
        .filter(|opt| opt.is_some())
        .count()
        .saturating_sub(1);

    // Call Z3 to solve with the target constraint negated.
    match solver::solve_for_new_path(&solvable, solvable_idx, solver_timeout_ms, param_infos) {
        Ok(SolveResult::Sat(solved_values)) => {
            let param_names: Vec<String> = param_infos.iter().map(|p| p.name.clone()).collect();
            let param_types = crate::orchestrator::param_types_of(param_infos);
            let inputs =
                overlay_solved_values(base_inputs, &solved_values, &param_names, &param_types);
            metrics.sat_count += 1;
            SolveOutcome::Sat { inputs }
        }
        Ok(SolveResult::Unsat) => {
            metrics.unsat_count += 1;
            SolveOutcome::Unsat
        }
        Err(e) => {
            metrics.error_count += 1;
            SolveOutcome::Error {
                message: e.to_string(),
            }
        }
    }
}

/// Run the Analyze stage on an observation result.
///
/// Takes the raw exploration output and the static function analysis,
/// then produces equivalence classes, a behavior map, and coverage metrics.
pub fn analyze(observe: &ObservationOutput, analysis: &FunctionAnalysis) -> AnalyzeOutput {
    // Build equivalence classes from raw results.
    let eq_classes = equivalence::group_into_classes(&observe.raw_results);

    // Build execution records for BehaviorMap construction.
    let records: Vec<ExecutionRecord> = observe
        .raw_results
        .iter()
        .map(|(inputs, _mocks, result)| {
            execution_record_from_result(&observe.function_name, inputs, result)
        })
        .collect();

    let mut behavior_map = BehaviorMap::from_records(&observe.function_name, &records);
    behavior_map.nondeterministic_fields = observe.nondeterministic_fields.clone();

    // Deduplicate constraints by branch_id so each branch contributes exactly one classification.
    let unique_constraints: HashMap<u32, SymConstraint> = observe
        .raw_results
        .iter()
        .flat_map(|(_, _mocks, result)| {
            result
                .branch_path
                .iter()
                .map(|d| (d.branch_id, d.constraint.clone()))
        })
        .collect();
    let all_constraints: Vec<SymConstraint> = unique_constraints.into_values().collect();

    let observed_discoveries = observed_branch_discoveries(observe, &analysis.branches);
    let mut coverage_metrics = CoverageMetrics::from_exploration(
        analysis.branches.len(),
        &observed_discoveries,
        &all_constraints,
    );

    if let Some((total, independent, opaque)) = observe.mcdc_summary {
        coverage_metrics.mcdc_metrics = Some(
            crate::coverage_metrics::McdcMetrics::from_mcdc_summary(total, independent, opaque),
        );
    }

    AnalyzeOutput {
        eq_classes,
        behavior_map,
        coverage_metrics,
    }
}

fn observed_branch_discoveries(
    observe: &ObservationOutput,
    branches: &[crate::protocol::BranchInfo],
) -> Vec<(u32, crate::coverage_metrics::DiscoveryMethod)> {
    let mut seen: HashSet<u32> = HashSet::new();
    let mut discoveries = Vec::new();
    let mut branch_lines: HashMap<u32, u32> = HashMap::new();
    let mut ambiguous_lines: HashSet<u32> = HashSet::new();
    for branch in branches {
        if branch.line == 0 {
            continue;
        }
        if branch_lines.insert(branch.line, branch.id).is_some() {
            ambiguous_lines.insert(branch.line);
        }
    }
    for line in ambiguous_lines {
        branch_lines.remove(&line);
    }

    for (branch_id, method) in &observe.discoveries {
        if seen.insert(*branch_id) {
            discoveries.push((*branch_id, *method));
        }
    }

    for (inputs, _mocks, result) in &observe.raw_results {
        for decision in &result.branch_path {
            if seen.insert(decision.branch_id) {
                discoveries.push((
                    decision.branch_id,
                    crate::coverage_metrics::DiscoveryMethod::Random,
                ));
            }
        }

        if branch_lines.is_empty()
            || (!result.branch_path.is_empty()
                && !crate::behavior::has_replayable_native_input(inputs))
            || result.lines_executed.is_empty()
        {
            continue;
        }

        let executed_lines: HashSet<u32> = result.lines_executed.iter().copied().collect();
        for (line, branch_id) in &branch_lines {
            if executed_lines.contains(line) && seen.insert(*branch_id) {
                discoveries.push((*branch_id, crate::coverage_metrics::DiscoveryMethod::Random));
            }
        }
    }

    discoveries
}

/// Run the Specify stage: build a complete, validated specification from all
/// prior stage outputs.
///
/// This is the terminal stage of the pipeline. It:
/// 1. Builds a [`FunctionSpec`] from observation data and equivalence classes
/// 2. Optionally enriches with Daikon-style invariants
/// 3. Integrates solve results: upgrades provenance for Z3-proven branches
///    and adds solved inputs as additional concrete examples
/// 4. Computes coverage completeness accounting
/// 5. Generates test suggestions from spec classes and solved inputs
pub fn specify(
    observe: &ObserveStageOutput,
    analyze: &AnalyzeOutput,
    solve: &StageSolveOutput,
    detect_invariants: bool,
) -> SpecifyStageOutput {
    let location = Some(format!(
        "{}:{}-{}",
        observe.file, observe.analysis.start_line, observe.analysis.end_line
    ));

    let mut spec =
        crate::spec::build_spec(&observe.observation, &analyze.eq_classes, location, None);

    if detect_invariants {
        crate::spec::detect_spec_invariants(&mut spec, &observe.observation, &analyze.eq_classes);
    }

    // Enrich provenance from solve results: branches with Sat outcomes are Proven.
    let proven_branch_ids: HashSet<u32> = solve
        .solved_branches
        .iter()
        .filter(|sb| matches!(sb.outcome, SolveOutcome::Sat { .. }))
        .map(|sb| sb.branch_id)
        .collect();

    for class in &mut spec.classes {
        let all_proven = !class.branch_path.0.is_empty()
            && class
                .branch_path
                .0
                .iter()
                .all(|step| proven_branch_ids.contains(&step.branch_id));
        if all_proven {
            class.precondition_provenance = crate::spec::Provenance::Proven;
            class.postcondition_provenance = crate::spec::Provenance::Proven;
        }
    }

    // Add solved inputs as additional examples on matching classes.
    for sb in &solve.solved_branches {
        if let SolveOutcome::Sat { ref inputs } = sb.outcome {
            for class in &mut spec.classes {
                let matches_branch = class
                    .branch_path
                    .0
                    .iter()
                    .any(|step| step.branch_id == sb.branch_id);
                if matches_branch {
                    class.examples.push(crate::spec::ConcreteExample {
                        inputs: inputs.clone(),
                        return_value: None,
                        thrown_error: None,
                    });
                }
            }
        }
    }

    let coverage_completeness = compute_coverage_completeness(observe, solve);
    let test_suggestions = build_test_suggestions(&spec, solve);

    SpecifyStageOutput {
        spec,
        coverage_completeness,
        test_suggestions,
        function_name: observe.observation.function_name.clone(),
        file: observe.file.clone(),
    }
}

/// Compute coverage completeness from observed directions and solve outcomes.
fn compute_coverage_completeness(
    observe: &ObserveStageOutput,
    solve: &StageSolveOutput,
) -> CoverageCompleteness {
    let total_branch_directions = observe.analysis.branches.len() * 2;

    // Count observed directions from raw results.
    let mut observed_directions: HashSet<(u32, bool)> = HashSet::new();
    for (_, _mocks, result) in &observe.observation.raw_results {
        for decision in &result.branch_path {
            observed_directions.insert((decision.branch_id, decision.taken));
        }
    }
    let observed = observed_directions.len();

    // Count solve outcomes (only for directions NOT already observed).
    let mut proven_sat = 0usize;
    let mut proven_unsat = 0usize;
    let mut opaque = 0usize;
    let mut unreachable = 0usize;
    let mut solver_errors = 0usize;

    for sb in &solve.solved_branches {
        // Solve stage only targets unobserved directions, so no overlap with observed.
        match &sb.outcome {
            SolveOutcome::Sat { .. } => proven_sat += 1,
            SolveOutcome::Unsat => proven_unsat += 1,
            SolveOutcome::Opaque { .. } => opaque += 1,
            SolveOutcome::Unreachable => unreachable += 1,
            SolveOutcome::Error { .. } => solver_errors += 1,
        }
    }

    let accounted = observed + proven_sat + proven_unsat;
    let completeness_pct = if total_branch_directions > 0 {
        (accounted as f64 / total_branch_directions as f64) * 100.0
    } else {
        100.0
    };

    CoverageCompleteness {
        total_branch_directions,
        observed,
        proven_sat,
        proven_unsat,
        opaque,
        unreachable,
        solver_errors,
        completeness_pct,
    }
}

/// Generate test suggestions from spec classes and solve results.
fn build_test_suggestions(spec: &FunctionSpec, solve: &StageSolveOutput) -> Vec<TestSuggestion> {
    let mut suggestions = Vec::new();

    // One suggestion per spec class from its canonical example.
    for class in &spec.classes {
        if let Some(example) = class.examples.first() {
            let (expected_return, expected_error) = match &class.postcondition {
                crate::spec::Postcondition::Returns { value } => (Some(value.clone()), None),
                crate::spec::Postcondition::Throws { error } => (
                    None,
                    Some(format!("{}: {}", error.error_type, error.message)),
                ),
                crate::spec::Postcondition::ReturnsVoid => (None, None),
            };

            suggestions.push(TestSuggestion {
                description: class.label.clone(),
                inputs: example.inputs.clone(),
                expected_return,
                expected_error,
                source: TestSuggestionSource::Observed,
            });
        }
    }

    // One suggestion per solved branch with Sat inputs.
    for sb in &solve.solved_branches {
        if let SolveOutcome::Sat { ref inputs } = sb.outcome {
            let direction = if sb.target_taken { "true" } else { "false" };
            suggestions.push(TestSuggestion {
                description: format!(
                    "Z3-solved: branch {} line {} direction {}",
                    sb.branch_id, sb.line, direction
                ),
                inputs: inputs.clone(),
                expected_return: None,
                expected_error: None,
                source: TestSuggestionSource::Solved,
            });
        }
    }

    suggestions
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

impl From<crate::orchestrator::ExploreResult> for ObservationOutput {
    fn from(r: crate::orchestrator::ExploreResult) -> Self {
        fn executed_lines_from_result(exec: &ExecuteResult) -> Vec<u32> {
            if !exec.lines_executed.is_empty() {
                return exec.lines_executed.clone();
            }

            exec.branch_path
                .iter()
                .filter_map(|decision| (decision.line > 0).then_some(decision.line))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        }

        // Build ExecutionSummary entries from the first raw execution that
        // reached each unique path so report consumers keep the inputs that
        // discovered the path. Fall back to the path-only execution list for
        // older synthetic results that have no raw input pairing.
        let new_path_executions: Vec<crate::explorer::ExecutionSummary> =
            if r.raw_results.is_empty() {
                r.executions
                    .iter()
                    .map(|exec| crate::explorer::ExecutionSummary {
                        inputs: vec![],
                        return_value: exec.return_value.clone(),
                        thrown_error: exec
                            .thrown_error
                            .as_ref()
                            .map(|e| format!("{}: {}", e.error_type, e.message)),
                        lines_executed: executed_lines_from_result(exec),
                        is_new_path: true,
                        error_intent: crate::explorer::classify_error_intent(exec),
                    })
                    .collect()
            } else {
                let mut seen_paths = std::collections::HashSet::new();
                r.raw_results
                    .iter()
                    .filter_map(|(inputs, _mocks, exec)| {
                        let path_hash = crate::orchestrator::hash_branch_path(&exec.branch_path);
                        if !seen_paths.insert(path_hash) {
                            return None;
                        }
                        Some(crate::explorer::ExecutionSummary {
                            inputs: inputs.clone(),
                            return_value: exec.return_value.clone(),
                            thrown_error: exec
                                .thrown_error
                                .as_ref()
                                .map(|e| format!("{}: {}", e.error_type, e.message)),
                            lines_executed: executed_lines_from_result(exec),
                            is_new_path: true,
                            error_intent: crate::explorer::classify_error_intent(exec),
                        })
                    })
                    .collect()
            };

        // Compute lines covered from raw_results.
        let mut all_lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
        if r.raw_results.is_empty() {
            for result in &r.executions {
                for line in executed_lines_from_result(result) {
                    all_lines.insert(line);
                }
            }
        } else {
            for (_, _mocks, result) in &r.raw_results {
                for line in executed_lines_from_result(result) {
                    all_lines.insert(line);
                }
            }
        }

        // str-gz8j: lift the orchestrator's per-function timeout signal into
        // ObservationOutput.timed_out so the CLI explore command can route
        // the function into the TimedOut bucket instead of treating it as
        // Completed.
        //
        // str-jeen.65: trust the orchestrator's `timed_out` flag directly —
        // it captures wall-clock budget violations from any phase (main loop,
        // float-probe, refine, shrink) and folds in
        // `termination_reason == TimeoutExplore` itself. Keying solely on
        // `termination_reason` silently mis-bucketed timed-out functions as
        // `ok` whenever a tail phase (Z3, refine, shrink) overshot the
        // deadline after the loop exited via WorklistExhausted /
        // MaxIterations / CoveragePlateau / McdcComplete.
        let timed_out = r.timed_out
            || matches!(
                r.termination_reason,
                crate::orchestrator::TerminationReason::TimeoutExplore
            );
        Self {
            function_name: r.function_name,
            iterations: r.total_executions as u32,
            unique_paths: r.unique_paths,
            lines_covered: all_lines.len(),
            total_lines: r.total_lines,
            new_path_executions,
            raw_results: r.raw_results,
            discoveries: r.discoveries,
            solver_guided_inputs: r.z3_generated + r.boundary_generated + r.drill_generated,
            nondeterministic_fields: r.nondeterministic_fields,
            float_probe_results: r.float_probe_results,
            boundary_results: r.boundary_results,
            shrunk_witnesses: r.shrunk_witnesses,
            mcdc_summary: r.mcdc_summary,
            shrink_stats: r.shrink_stats,
            abandoned_frontiers: r.abandoned_frontiers,
            opaque_suggestions: r.opaque_suggestions,
            stubbed_modules: r.stubbed_modules,
            timed_out,
            oracle_stats: r.oracle_stats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage_metrics::DiscoveryMethod;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::explorer::ObservationOutput;
    use crate::protocol::PerformanceMetrics;
    use crate::types::{ParamInfo, TypeInfo};
    use serde_json::json;

    fn empty_perf() -> PerformanceMetrics {
        PerformanceMetrics {
            wall_time_ms: 0.0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
        }
    }

    fn stub_analysis(name: &str, branch_count: usize) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.into(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            branches: (0..branch_count)
                .map(|i| crate::protocol::BranchInfo {
                    id: i as u32,
                    line: (i as u32 + 1) * 10,
                    condition_text: format!("x > {i}"),
                    condition: None,
                    branch_type: crate::protocol::BranchType::If,
                })
                .collect(),
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
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
    fn analyze_produces_all_outputs() {
        let branch_path = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "test".into(),
            },
            conditions: None,
        }];
        let exec_result = ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path,
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

        let observe = ObservationOutput {
            function_name: "classify".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], vec![], exec_result)],
            discoveries: vec![(0, DiscoveryMethod::Random)],
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

        let analysis = stub_analysis("classify", 2);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.eq_classes.len(), 1);
        assert_eq!(output.behavior_map.function_id, "classify");
        assert_eq!(output.coverage_metrics.total_branches, 2);
        assert_eq!(output.coverage_metrics.random_found, 1);
        assert_eq!(output.coverage_metrics.uncovered, 1);
        assert_eq!(output.coverage_metrics.unknown_count, 1);
    }

    #[test]
    fn analyze_counts_raw_branch_paths_without_discoveries() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "first runtime branch".into(),
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 20,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "second runtime branch".into(),
                },
                conditions: None,
            },
        ];
        let exec_result = ExecuteResult {
            return_value: Some(json!("covered")),
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
        };

        let observe = ObservationOutput {
            function_name: "classify".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 0,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], vec![], exec_result)],
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

        let analysis = stub_analysis("classify", 3);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.coverage_metrics.total_branches, 3);
        assert_eq!(output.coverage_metrics.random_found, 2);
        assert_eq!(output.coverage_metrics.uncovered, 1);
        assert_eq!(output.coverage_metrics.unknown_count, 2);
    }

    #[test]
    fn analyze_counts_executed_branch_lines_without_branch_path() {
        let exec_result = ExecuteResult {
            return_value: Some(json!({"status": 500, "body": {"error": "name is required"}})),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![30, 36, 37, 38, 39],
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
        let observe = ObservationOutput {
            function_name: "create_person".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 5,
            total_lines: 45,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!({"name": "   "})], vec![], exec_result)],
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

        let mut analysis = stub_analysis("create_person", 3);
        analysis.branches[0].id = 0;
        analysis.branches[0].line = 38;
        analysis.branches[0].condition_text = "trimmed_name.is_empty()".to_string();
        analysis.branches[1].id = 1;
        analysis.branches[1].line = 45;
        analysis.branches[2].id = 2;
        analysis.branches[2].line = 52;

        let output = analyze(&observe, &analysis);

        assert_eq!(output.coverage_metrics.total_branches, 3);
        assert_eq!(
            output.coverage_metrics.random_found, 1,
            "a retained replay that executed a known branch line should count that branch"
        );
        assert_eq!(output.coverage_metrics.uncovered, 2);
    }

    #[test]
    fn analyze_does_not_infer_branch_lines_when_branch_path_exists() {
        let exec_result = ExecuteResult {
            return_value: Some(json!("covered")),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "first runtime branch".into(),
                },
                conditions: None,
            }],
            lines_executed: vec![10, 20],
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
        let observe = ObservationOutput {
            function_name: "classify".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 2,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], vec![], exec_result)],
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

        let analysis = stub_analysis("classify", 3);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.coverage_metrics.random_found, 1);
        assert_eq!(output.coverage_metrics.uncovered, 2);
    }

    #[test]
    fn analyze_infers_native_replay_branch_lines_with_partial_branch_path() {
        let exec_result = ExecuteResult {
            return_value: Some(json!({"status": 500, "body": {"error": "name is required"}})),
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "workspace access".into(),
                },
                conditions: None,
            }],
            lines_executed: vec![10, 20],
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
        let observe = ObservationOutput {
            function_name: "create_person".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 2,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(
                vec![json!({
                    "__shatter_native": true,
                    "handle": "current-account",
                    "__shatter_replay": {
                        "language": "rust",
                        "file": ".shatter/generators/current.rs",
                        "name": "CurrentAccountGen",
                        "recipe": null
                    }
                })],
                vec![],
                exec_result,
            )],
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

        let analysis = stub_analysis("create_person", 3);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.coverage_metrics.random_found, 2);
        assert_eq!(output.coverage_metrics.uncovered, 1);
    }

    #[test]
    fn analyze_skips_ambiguous_same_line_branch_inference() {
        let exec_result = ExecuteResult {
            return_value: Some(json!("covered")),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![10],
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
        let observe = ObservationOutput {
            function_name: "classify".into(),
            iterations: 1,
            unique_paths: 1,
            lines_covered: 1,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], vec![], exec_result)],
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

        let mut analysis = stub_analysis("classify", 2);
        analysis.branches[0].line = 10;
        analysis.branches[1].line = 10;

        let output = analyze(&observe, &analysis);

        assert_eq!(
            output.coverage_metrics.random_found, 0,
            "line fallback must not count multiple static branches sharing one line"
        );
        assert_eq!(output.coverage_metrics.uncovered, 2);
    }

    #[test]
    fn explore_result_conversion_recovers_lines_from_branch_decisions() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 7,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "x > 0".into(),
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 9,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "x < 10".into(),
                },
                conditions: None,
            },
        ];
        let exec_result = ExecuteResult {
            return_value: Some(json!("covered")),
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
        };
        let result = crate::orchestrator::ExploreResult {
            function_name: "classify".into(),
            total_lines: 10,
            executions: vec![exec_result],
            unique_paths: 1,
            total_executions: 1,
            z3_generated: 0,
            fuzz_generated: 0,
            boundary_generated: 0,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::MaxIterations,
            raw_results: vec![],
            discoveries: vec![],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: false,
            oracle_stats: None,
        };

        let observe: ObservationOutput = result.into();

        assert_eq!(observe.lines_covered, 2);
        assert_eq!(observe.new_path_executions[0].lines_executed, vec![7, 9]);
    }

    #[test]
    fn explore_result_conversion_recovers_raw_result_lines_from_branch_decisions() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 7,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "x > 0".into(),
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 9,
                taken: false,
                constraint: SymConstraint::Unknown {
                    hint: "x < 10".into(),
                },
                conditions: None,
            },
        ];
        let exec_result = ExecuteResult {
            return_value: Some(json!("covered")),
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
        };
        let result = crate::orchestrator::ExploreResult {
            function_name: "classify".into(),
            total_lines: 10,
            executions: vec![],
            unique_paths: 1,
            total_executions: 1,
            z3_generated: 0,
            fuzz_generated: 0,
            boundary_generated: 0,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::MaxIterations,
            raw_results: vec![(vec![json!(3)], vec![], exec_result)],
            discoveries: vec![],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: false,
            oracle_stats: None,
        };

        let observe: ObservationOutput = result.into();

        assert_eq!(observe.lines_covered, 2);
        assert_eq!(observe.new_path_executions[0].inputs, vec![json!(3)]);
        assert_eq!(observe.new_path_executions[0].lines_executed, vec![7, 9]);
    }

    #[test]
    fn analyze_empty_exploration() {
        let observe = ObservationOutput {
            function_name: "empty".into(),
            iterations: 0,
            unique_paths: 0,
            lines_covered: 0,
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

        let analysis = stub_analysis("empty", 3);
        let output = analyze(&observe, &analysis);

        assert!(output.eq_classes.is_empty());
        assert_eq!(output.behavior_map.behaviors.len(), 0);
        assert_eq!(output.coverage_metrics.total_branches, 3);
        assert_eq!(output.coverage_metrics.uncovered, 3);
    }

    /// Regression: constraint count must reflect unique branch_ids, not total observations.
    /// 3 executions × 2 branches = 6 observations, but only 2 unique constraints.
    #[test]
    fn constraint_count_deduplicates_by_branch_id() {
        let make_branch_path = || {
            vec![
                BranchDecision {
                    branch_id: 0,
                    line: 10,
                    taken: true,
                    constraint: SymConstraint::Expr {
                        expr: crate::sym_expr::SymExpr::Param {
                            name: "x".into(),
                            path: vec![],
                        },
                    },
                    conditions: None,
                },
                BranchDecision {
                    branch_id: 1,
                    line: 20,
                    taken: false,
                    constraint: SymConstraint::Unknown {
                        hint: "opaque".into(),
                    },
                    conditions: None,
                },
            ]
        };

        let make_result = |val: serde_json::Value| ExecuteResult {
            return_value: Some(val),
            thrown_error: None,
            branch_path: make_branch_path(),
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

        let observe = ObservationOutput {
            function_name: "dedup_test".into(),
            iterations: 3,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![
                (vec![json!(1)], vec![], make_result(json!("a"))),
                (vec![json!(2)], vec![], make_result(json!("b"))),
                (vec![json!(3)], vec![], make_result(json!("c"))),
            ],
            discoveries: vec![(0, DiscoveryMethod::Random)],
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

        let analysis = stub_analysis("dedup_test", 2);
        let output = analyze(&observe, &analysis);

        // Must be 2 (one per unique branch_id), not 6 (total observations).
        let constraint_total =
            output.coverage_metrics.symexpr_count + output.coverage_metrics.unknown_count;
        assert_eq!(
            constraint_total, 2,
            "constraints must equal unique branch_ids, not total observations"
        );
        assert_eq!(output.coverage_metrics.symexpr_count, 1);
        assert_eq!(output.coverage_metrics.unknown_count, 1);
    }

    #[test]
    fn analyze_carries_nondeterministic_fields() {
        use crate::nondeterminism::{Confidence, NondeterminismEvidence, NondeterministicField};

        let observe = ObservationOutput {
            function_name: "nondet_fn".into(),
            iterations: 1,
            unique_paths: 0,
            lines_covered: 0,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![NondeterministicField {
                field_path: "return.timestamp".into(),
                evidence: vec![NondeterminismEvidence::ObservedWithinRun],
                confidence: Confidence::High,
            }],
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

        let analysis = stub_analysis("nondet_fn", 0);
        let output = analyze(&observe, &analysis);

        assert_eq!(output.behavior_map.nondeterministic_fields.len(), 1);
        assert_eq!(
            output.behavior_map.nondeterministic_fields[0].field_path,
            "return.timestamp"
        );
    }

    #[test]
    fn observe_stage_output_round_trips() {
        let observe = ObservationOutput {
            function_name: "test_fn".into(),
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
        let analysis = stub_analysis("test_fn", 1);
        let stage = ObserveStageOutput {
            observation: observe,
            analysis,
            file: "test.ts".into(),
        };
        let json = serde_json::to_string(&stage).expect("serialize");
        let d: ObserveStageOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d.observation.function_name, "test_fn");
        assert_eq!(d.file, "test.ts");
        assert_eq!(d.analysis.branches.len(), 1);
    }

    #[test]
    fn analyze_output_round_trips() {
        let observe = ObservationOutput {
            function_name: "roundtrip".into(),
            iterations: 1,
            unique_paths: 0,
            lines_covered: 0,
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
        let analysis = stub_analysis("roundtrip", 2);
        let output = analyze(&observe, &analysis);

        let json = serde_json::to_string(&output).expect("serialize");
        let d: AnalyzeOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d.eq_classes.len(), output.eq_classes.len());
        assert_eq!(
            d.coverage_metrics.total_branches,
            output.coverage_metrics.total_branches
        );
        assert_eq!(d.behavior_map.function_id, output.behavior_map.function_id);
    }

    #[test]
    fn analyze_stage_output_round_trips() {
        let observe = ObservationOutput {
            function_name: "stage_rt".into(),
            iterations: 1,
            unique_paths: 0,
            lines_covered: 0,
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
        let analysis = stub_analysis("stage_rt", 1);
        let analyze_out = analyze(&observe, &analysis);
        let stage = AnalyzeStageOutput {
            analyze: analyze_out,
            spec: None,
            function_name: "stage_rt".into(),
            file: "test.ts".into(),
        };
        let json = serde_json::to_string(&stage).expect("serialize");
        let d: AnalyzeStageOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d.function_name, "stage_rt");
        assert_eq!(d.file, "test.ts");
        assert!(d.spec.is_none());
    }

    #[test]
    fn eq_classes_bounded_by_raw_results() {
        let branch_path_a = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown { hint: "t".into() },
            conditions: None,
        }];
        let branch_path_b = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Unknown { hint: "t".into() },
            conditions: None,
        }];
        let make_result = |bp: Vec<BranchDecision>| ExecuteResult {
            return_value: Some(json!(1)),
            thrown_error: None,
            branch_path: bp,
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
            performance: empty_perf(),
        };
        let observe = ObservationOutput {
            function_name: "bounded".into(),
            iterations: 3,
            unique_paths: 2,
            lines_covered: 1,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![
                (vec![json!(1)], vec![], make_result(branch_path_a.clone())),
                (vec![json!(2)], vec![], make_result(branch_path_a)),
                (vec![json!(-1)], vec![], make_result(branch_path_b)),
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
        let analysis = stub_analysis("bounded", 1);
        let output = analyze(&observe, &analysis);
        assert!(
            output.eq_classes.len() <= observe.raw_results.len(),
            "eq classes ({}) must not exceed raw results ({})",
            output.eq_classes.len(),
            observe.raw_results.len()
        );
    }

    #[test]
    fn observation_output_from_concolic_result() {
        let concolic = crate::orchestrator::ExploreResult {
            function_name: "test".into(),
            total_lines: 10,
            executions: vec![],
            unique_paths: 2,
            total_executions: 10,
            z3_generated: 3,
            fuzz_generated: 1,
            boundary_generated: 0,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::WorklistExhausted,
            raw_results: vec![],
            discoveries: vec![(0, DiscoveryMethod::Random)],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: false,
            oracle_stats: None,
        };

        let output: ObservationOutput = concolic.into();
        assert_eq!(output.function_name, "test");
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.total_lines, 10);
        assert_eq!(output.discoveries.len(), 1);
        // str-gz8j: WorklistExhausted is a normal termination, not a timeout.
        assert!(
            !output.timed_out,
            "WorklistExhausted should not flag the observation as timed_out"
        );
    }

    #[test]
    fn observation_output_from_concolic_result_preserves_raw_input_witnesses() {
        let true_path = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown { hint: "x".into() },
            conditions: None,
        }];
        let false_path = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Unknown { hint: "x".into() },
            conditions: None,
        }];
        let make_result = |branch_path: Vec<BranchDecision>, value: i64| ExecuteResult {
            return_value: Some(json!(value)),
            thrown_error: None,
            branch_path,
            lines_executed: vec![10],
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
        let concolic = crate::orchestrator::ExploreResult {
            function_name: "test".into(),
            total_lines: 10,
            executions: vec![],
            unique_paths: 2,
            total_executions: 3,
            z3_generated: 0,
            fuzz_generated: 0,
            boundary_generated: 0,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::WorklistExhausted,
            raw_results: vec![
                (
                    vec![json!({"__shatter_native": true, "handle": "state-1"})],
                    vec![],
                    make_result(true_path.clone(), 1),
                ),
                (
                    vec![json!({"__shatter_native": true, "handle": "state-2"})],
                    vec![],
                    make_result(true_path, 2),
                ),
                (
                    vec![json!({"__shatter_native": true, "handle": "state-3"})],
                    vec![],
                    make_result(false_path, 3),
                ),
            ],
            discoveries: vec![],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: false,
            oracle_stats: None,
        };

        let output: ObservationOutput = concolic.into();

        assert_eq!(output.new_path_executions.len(), 2);
        assert_eq!(
            output.new_path_executions[0].inputs,
            vec![json!({"__shatter_native": true, "handle": "state-1"})]
        );
        assert_eq!(
            output.new_path_executions[1].inputs,
            vec![json!({"__shatter_native": true, "handle": "state-3"})]
        );
    }

    /// str-gz8j: per-function timeout signal must propagate from
    /// `ExploreResult.termination_reason` into `ObservationOutput.timed_out`
    /// so the CLI explore command can downgrade the function's outcome to
    /// `OutcomeStatus::TimedOut` instead of silently labelling it
    /// `Completed`. Concolic-path side of the parallel pair.
    #[test]
    fn observation_output_from_concolic_result_marks_timed_out_on_timeout_termination() {
        let concolic = crate::orchestrator::ExploreResult {
            function_name: "slow".into(),
            total_lines: 5,
            executions: vec![],
            unique_paths: 0,
            total_executions: 3,
            z3_generated: 0,
            fuzz_generated: 0,
            boundary_generated: 0,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::TimeoutExplore,
            raw_results: vec![],
            discoveries: vec![],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: true,
            oracle_stats: None,
        };
        let output: ObservationOutput = concolic.into();
        assert!(
            output.timed_out,
            "TerminationReason::TimeoutExplore must propagate as ObservationOutput.timed_out=true"
        );
    }

    /// str-jeen.65: the regression case — a function whose main loop exited
    /// via `WorklistExhausted` (a "normal" termination) but whose overall
    /// wall-clock budget was crossed during a post-loop phase (refine /
    /// shrink). The orchestrator sets `ExploreResult.timed_out=true` in that
    /// case and the conversion must surface it as
    /// `ObservationOutput.timed_out=true` so the CLI reports the function as
    /// `timed_out` rather than `ok`.
    #[test]
    fn observation_output_marks_timed_out_when_post_loop_phase_overshot_deadline() {
        let concolic = crate::orchestrator::ExploreResult {
            function_name: "tail_overshoot".into(),
            total_lines: 5,
            executions: vec![],
            unique_paths: 1,
            total_executions: 8,
            z3_generated: 0,
            fuzz_generated: 0,
            boundary_generated: 0,
            drill_generated: 0,
            // Loop terminated "naturally" — but a post-loop phase overran the
            // budget, so the orchestrator flipped `timed_out` to true.
            termination_reason: crate::orchestrator::TerminationReason::WorklistExhausted,
            raw_results: vec![],
            discoveries: vec![],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
            timed_out: true,
            oracle_stats: None,
        };
        let output: ObservationOutput = concolic.into();
        assert!(
            output.timed_out,
            "ExploreResult.timed_out=true must propagate to ObservationOutput \
             even when termination_reason != TimeoutExplore (str-jeen.65)",
        );
    }

    // ---- Solve stage tests ----

    fn stub_observe_stage(
        name: &str,
        branch_count: usize,
        raw_results: Vec<(
            Vec<serde_json::Value>,
            Vec<crate::protocol::MockConfig>,
            ExecuteResult,
        )>,
    ) -> ObserveStageOutput {
        let observation = ObservationOutput {
            function_name: name.into(),
            iterations: raw_results.len() as u32,
            unique_paths: 0,
            lines_covered: 0,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results,
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
        ObserveStageOutput {
            observation,
            analysis: stub_analysis(name, branch_count),
            file: "test.ts".into(),
        }
    }

    #[test]
    fn solve_all_covered_returns_empty() {
        // Both directions observed for the single branch → nothing to solve.
        let branch_path_t = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param {
                        name: "x".into(),
                        path: vec![],
                    }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(
                        crate::sym_expr::ConstValue::Int(0),
                    )),
                },
            },
            conditions: None,
        }];
        let branch_path_f = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param {
                        name: "x".into(),
                        path: vec![],
                    }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(
                        crate::sym_expr::ConstValue::Int(0),
                    )),
                },
            },
            conditions: None,
        }];
        let make_result = |bp| ExecuteResult {
            return_value: Some(json!(1)),
            thrown_error: None,
            branch_path: bp,
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
            performance: empty_perf(),
        };
        let observe = stub_observe_stage(
            "all_covered",
            1,
            vec![
                (vec![json!(5)], vec![], make_result(branch_path_t)),
                (vec![json!(-1)], vec![], make_result(branch_path_f)),
            ],
        );

        let output = solve(&observe, Some(1000));
        assert!(
            output.solved_branches.is_empty(),
            "no uncovered branches to solve"
        );
        assert_eq!(output.metrics.total_uncovered, 0);
    }

    #[test]
    fn solve_unreachable_branch() {
        // Branch 0 is in analysis but no execution ever reached it.
        let observe = stub_observe_stage("unreachable", 1, vec![]);

        let output = solve(&observe, Some(1000));
        // Branch 0 has two missing directions (true and false).
        assert_eq!(output.metrics.total_uncovered, 2);
        assert_eq!(output.metrics.unreachable_count, 2);
        for sb in &output.solved_branches {
            assert_eq!(sb.outcome, SolveOutcome::Unreachable);
        }
    }

    #[test]
    fn solve_opaque_constraint() {
        // Branch observed in one direction but with Unknown constraint → Opaque.
        let branch_path = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Unknown {
                hint: "opaque call".into(),
            },
            conditions: None,
        }];
        let result = ExecuteResult {
            return_value: Some(json!(1)),
            thrown_error: None,
            branch_path,
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
            performance: empty_perf(),
        };
        let observe = stub_observe_stage("opaque", 1, vec![(vec![json!(1)], vec![], result)]);

        let output = solve(&observe, Some(1000));
        assert_eq!(output.metrics.total_uncovered, 1);
        assert_eq!(output.metrics.opaque_count, 1);
        assert_eq!(output.solved_branches.len(), 1);
        assert!(!output.solved_branches[0].target_taken);
        assert!(matches!(
            output.solved_branches[0].outcome,
            SolveOutcome::Opaque { .. }
        ));
    }

    #[test]
    fn solve_with_solvable_constraint() {
        // Branch 0: x > 0, only taken=true observed. Solve should find inputs for taken=false.
        let branch_path = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: true,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param {
                        name: "x".into(),
                        path: vec![],
                    }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(
                        crate::sym_expr::ConstValue::Int(0),
                    )),
                },
            },
            conditions: None,
        }];
        let result = ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path,
            lines_executed: vec![1, 2],
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
        let observe = stub_observe_stage("solvable", 1, vec![(vec![json!(5)], vec![], result)]);

        let output = solve(&observe, Some(5000));
        assert_eq!(output.metrics.total_uncovered, 1);
        assert_eq!(output.solved_branches.len(), 1);
        assert_eq!(output.solved_branches[0].branch_id, 0);
        assert!(!output.solved_branches[0].target_taken);
        // Z3 should find x <= 0 as satisfying.
        assert!(
            matches!(output.solved_branches[0].outcome, SolveOutcome::Sat { .. }),
            "expected Sat, got {:?}",
            output.solved_branches[0].outcome
        );
        if let SolveOutcome::Sat { inputs } = &output.solved_branches[0].outcome {
            assert_eq!(inputs.len(), 1, "single param function");
            // The solved value should be <= 0.
            let val = inputs[0].as_i64().expect("should be integer");
            assert!(val <= 0, "solved value {val} should satisfy x <= 0");
        }
    }

    #[test]
    fn solve_metrics_tally() {
        // Verify metrics counts match the number of solved_branches by outcome type.
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Expr {
                    expr: crate::sym_expr::SymExpr::BinOp {
                        op: crate::sym_expr::BinOpKind::Gt,
                        left: Box::new(crate::sym_expr::SymExpr::Param {
                            name: "x".into(),
                            path: vec![],
                        }),
                        right: Box::new(crate::sym_expr::SymExpr::Const(
                            crate::sym_expr::ConstValue::Int(0),
                        )),
                    },
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1,
                line: 20,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "opaque".into(),
                },
                conditions: None,
            },
        ];
        let result = ExecuteResult {
            return_value: Some(json!(1)),
            thrown_error: None,
            branch_path,
            lines_executed: vec![1, 2],
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
        // 3 branches in analysis: branch 0 (solvable, one direction), branch 1 (opaque, one direction),
        // branch 2 (never reached).
        let observe = stub_observe_stage("tally", 3, vec![(vec![json!(5)], vec![], result)]);

        let output = solve(&observe, Some(5000));
        let m = &output.metrics;

        // Tally check: sum of outcomes should equal total_uncovered.
        let tally =
            m.sat_count + m.unsat_count + m.opaque_count + m.unreachable_count + m.error_count;
        assert_eq!(
            tally, m.total_uncovered,
            "outcome tally ({tally}) must equal total_uncovered ({})",
            m.total_uncovered
        );

        // Branch 2 was never reached → 2 unreachable (true + false).
        assert_eq!(m.unreachable_count, 2);
        // Branch 1 opaque → 1 opaque.
        assert_eq!(m.opaque_count, 1);
    }

    #[test]
    fn solve_stage_output_round_trips() {
        let solve_out = StageSolveOutput {
            solved_branches: vec![
                SolvedBranch {
                    branch_id: 0,
                    line: 10,
                    target_taken: false,
                    outcome: SolveOutcome::Sat {
                        inputs: vec![json!(42)],
                    },
                },
                SolvedBranch {
                    branch_id: 1,
                    line: 20,
                    target_taken: true,
                    outcome: SolveOutcome::Unsat,
                },
                SolvedBranch {
                    branch_id: 2,
                    line: 30,
                    target_taken: false,
                    outcome: SolveOutcome::Opaque {
                        hint: "test".into(),
                    },
                },
                SolvedBranch {
                    branch_id: 3,
                    line: 40,
                    target_taken: true,
                    outcome: SolveOutcome::Unreachable,
                },
                SolvedBranch {
                    branch_id: 4,
                    line: 50,
                    target_taken: false,
                    outcome: SolveOutcome::Error {
                        message: "timeout".into(),
                    },
                },
            ],
            metrics: SolveMetrics {
                total_uncovered: 5,
                sat_count: 1,
                unsat_count: 1,
                opaque_count: 1,
                unreachable_count: 1,
                error_count: 1,
            },
        };
        let stage = SolveStageOutput {
            solve: solve_out,
            function_name: "round_trip".into(),
            file: "test.ts".into(),
        };
        let json = serde_json::to_string(&stage).expect("serialize");
        let d: SolveStageOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d.function_name, "round_trip");
        assert_eq!(d.file, "test.ts");
        assert_eq!(d.solve.solved_branches.len(), 5);
        assert_eq!(d.solve.metrics.total_uncovered, 5);
        assert_eq!(d.solve.metrics.sat_count, 1);
        assert_eq!(
            d.solve.solved_branches[0].outcome,
            SolveOutcome::Sat {
                inputs: vec![json!(42)]
            }
        );
        assert_eq!(d.solve.solved_branches[1].outcome, SolveOutcome::Unsat);
        assert_eq!(
            d.solve.solved_branches[3].outcome,
            SolveOutcome::Unreachable
        );
    }

    // -- Property-based tests --

    fn arb_solve_outcome() -> impl proptest::strategy::Strategy<Value = SolveOutcome> {
        use proptest::prelude::*;
        prop_oneof![
            proptest::collection::vec(
                prop_oneof![
                    Just(json!(42)),
                    Just(json!("hello")),
                    Just(json!(true)),
                    Just(json!(2.5)),
                    Just(json!(null)),
                ],
                0..5,
            )
            .prop_map(|inputs| SolveOutcome::Sat { inputs }),
            Just(SolveOutcome::Unsat),
            "[a-z ]{1,30}".prop_map(|hint| SolveOutcome::Opaque { hint }),
            Just(SolveOutcome::Unreachable),
            "[a-z ]{1,30}".prop_map(|message| SolveOutcome::Error { message }),
        ]
    }

    fn arb_solved_branch() -> impl proptest::strategy::Strategy<Value = SolvedBranch> {
        use proptest::prelude::*;
        (0..100u32, 1..500u32, any::<bool>(), arb_solve_outcome()).prop_map(
            |(branch_id, line, target_taken, outcome)| SolvedBranch {
                branch_id,
                line,
                target_taken,
                outcome,
            },
        )
    }

    fn arb_solve_metrics() -> impl proptest::strategy::Strategy<Value = SolveMetrics> {
        use proptest::prelude::*;
        (0..50usize, 0..50usize, 0..50usize, 0..50usize, 0..50usize).prop_map(
            |(sat, unsat, opaque, unreachable, error)| SolveMetrics {
                total_uncovered: sat + unsat + opaque + unreachable + error,
                sat_count: sat,
                unsat_count: unsat,
                opaque_count: opaque,
                unreachable_count: unreachable,
                error_count: error,
            },
        )
    }

    proptest::proptest! {
        /// SolveStageOutput survives a serialize → deserialize roundtrip.
        #[test]
        fn solve_stage_output_proptest_roundtrip(
            branches in proptest::collection::vec(arb_solved_branch(), 0..10),
            metrics in arb_solve_metrics(),
            name in "[a-z_]{1,20}",
            file in "[a-z/]{1,20}\\.ts",
        ) {
            let branch_count = branches.len();
            let expected_total = metrics.total_uncovered;
            let stage = SolveStageOutput {
                solve: StageSolveOutput {
                    solved_branches: branches,
                    metrics,
                },
                function_name: name.clone(),
                file: file.clone(),
            };
            let json = serde_json::to_string(&stage).expect("serialize");
            let d: SolveStageOutput = serde_json::from_str(&json).expect("deserialize");
            proptest::prop_assert_eq!(d.function_name, name);
            proptest::prop_assert_eq!(d.file, file);
            proptest::prop_assert_eq!(d.solve.solved_branches.len(), branch_count);
            proptest::prop_assert_eq!(d.solve.metrics.total_uncovered, expected_total);
        }

        /// SolveMetrics tally invariant: component counts always sum to total.
        #[test]
        fn solve_metrics_tally_invariant(metrics in arb_solve_metrics()) {
            let sum = metrics.sat_count
                + metrics.unsat_count
                + metrics.opaque_count
                + metrics.unreachable_count
                + metrics.error_count;
            proptest::prop_assert_eq!(sum, metrics.total_uncovered);
        }

        /// Each SolveOutcome variant survives a roundtrip.
        #[test]
        fn solve_outcome_roundtrip(outcome in arb_solve_outcome()) {
            let json = serde_json::to_string(&outcome).expect("serialize");
            let d: SolveOutcome = serde_json::from_str(&json).expect("deserialize");
            proptest::prop_assert_eq!(d, outcome);
        }

        /// SpecifyStageOutput survives a JSON roundtrip.
        #[test]
        fn specify_stage_output_roundtrip(
            output in crate::test_arbitraries::arb_specify_stage_output()
        ) {
            let json = serde_json::to_string(&output).expect("serialize");
            let d: SpecifyStageOutput = serde_json::from_str(&json).expect("deserialize");
            proptest::prop_assert_eq!(&d.function_name, &output.function_name);
            proptest::prop_assert_eq!(&d.file, &output.file);
            proptest::prop_assert_eq!(&d.test_suggestions, &output.test_suggestions);
            // Float field: compare with tolerance instead of exact equality.
            proptest::prop_assert!(
                (d.coverage_completeness.completeness_pct
                    - output.coverage_completeness.completeness_pct)
                    .abs()
                    < 1e-10
            );
            proptest::prop_assert_eq!(
                d.coverage_completeness.observed,
                output.coverage_completeness.observed
            );
        }

        /// CoverageCompleteness roundtrip (float field compared with tolerance).
        #[test]
        fn coverage_completeness_roundtrip(
            cc in crate::test_arbitraries::arb_coverage_completeness()
        ) {
            let json = serde_json::to_string(&cc).expect("serialize");
            let d: CoverageCompleteness = serde_json::from_str(&json).expect("deserialize");
            proptest::prop_assert_eq!(d.total_branch_directions, cc.total_branch_directions);
            proptest::prop_assert_eq!(d.observed, cc.observed);
            proptest::prop_assert_eq!(d.proven_sat, cc.proven_sat);
            proptest::prop_assert_eq!(d.proven_unsat, cc.proven_unsat);
            proptest::prop_assert_eq!(d.opaque, cc.opaque);
            proptest::prop_assert_eq!(d.unreachable, cc.unreachable);
            proptest::prop_assert_eq!(d.solver_errors, cc.solver_errors);
            proptest::prop_assert!((d.completeness_pct - cc.completeness_pct).abs() < 1e-10);
        }

        /// TestSuggestion roundtrip.
        #[test]
        fn test_suggestion_roundtrip(
            ts in crate::test_arbitraries::arb_test_suggestion()
        ) {
            let json = serde_json::to_string(&ts).expect("serialize");
            let d: TestSuggestion = serde_json::from_str(&json).expect("deserialize");
            proptest::prop_assert_eq!(d, ts);
        }
    }

    fn stub_specify_inputs(name: &str, branch_count: usize) -> (ObserveStageOutput, AnalyzeOutput) {
        let branch_path = (0..branch_count)
            .map(|i| BranchDecision {
                branch_id: i as u32,
                line: (i as u32 + 1) * 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: format!("cond_{i}"),
                },
                conditions: None,
            })
            .collect::<Vec<_>>();

        let exec_result = ExecuteResult {
            return_value: Some(json!("ok")),
            thrown_error: None,
            branch_path,
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

        let observation = ObservationOutput {
            function_name: name.into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(42)], vec![], exec_result)],
            discoveries: vec![(0, DiscoveryMethod::Random)],
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

        let analysis = stub_analysis(name, branch_count);

        let observe_stage = ObserveStageOutput {
            observation,
            analysis,
            file: "test.ts".into(),
        };

        let analyze_out = analyze(&observe_stage.observation, &observe_stage.analysis);

        (observe_stage, analyze_out)
    }

    #[test]
    fn specify_produces_complete_output() {
        let (observe_stage, analyze_out) = stub_specify_inputs("myFunc", 2);

        let solve_out = StageSolveOutput {
            solved_branches: vec![SolvedBranch {
                branch_id: 0,
                line: 10,
                target_taken: false,
                outcome: SolveOutcome::Sat {
                    inputs: vec![json!(-1)],
                },
            }],
            metrics: SolveMetrics {
                total_uncovered: 1,
                sat_count: 1,
                ..Default::default()
            },
        };

        let result = specify(&observe_stage, &analyze_out, &solve_out, false);

        assert_eq!(result.function_name, "myFunc");
        assert_eq!(result.file, "test.ts");
        assert!(!result.spec.classes.is_empty());
        assert!(!result.test_suggestions.is_empty());
        assert!(result.coverage_completeness.total_branch_directions > 0);
    }

    #[test]
    fn specify_enriches_provenance_from_solved() {
        let (observe_stage, analyze_out) = stub_specify_inputs("provenFunc", 1);

        // Solve found an input for branch 0 in the false direction.
        let solve_out = StageSolveOutput {
            solved_branches: vec![SolvedBranch {
                branch_id: 0,
                line: 10,
                target_taken: false,
                outcome: SolveOutcome::Sat {
                    inputs: vec![json!(-5)],
                },
            }],
            metrics: SolveMetrics {
                total_uncovered: 1,
                sat_count: 1,
                ..Default::default()
            },
        };

        let result = specify(&observe_stage, &analyze_out, &solve_out, false);

        // The class containing branch 0 should now be Proven.
        let class = &result.spec.classes[0];
        assert_eq!(
            class.precondition_provenance,
            crate::spec::Provenance::Proven
        );
        assert_eq!(
            class.postcondition_provenance,
            crate::spec::Provenance::Proven
        );

        // The solved input should appear as an additional example.
        assert!(class.examples.len() >= 2);
        assert_eq!(class.examples.last().unwrap().inputs, vec![json!(-5)]);
    }

    #[test]
    fn specify_coverage_completeness_calculation() {
        let (observe_stage, analyze_out) = stub_specify_inputs("coverageFunc", 3);

        // 3 branches × 2 directions = 6 total.
        // Observe saw all 3 in true direction = 3 observed.
        // Solve targets: branch 0 false (Sat), branch 1 false (Unsat), branch 2 false (Opaque).
        let solve_out = StageSolveOutput {
            solved_branches: vec![
                SolvedBranch {
                    branch_id: 0,
                    line: 10,
                    target_taken: false,
                    outcome: SolveOutcome::Sat {
                        inputs: vec![json!(0)],
                    },
                },
                SolvedBranch {
                    branch_id: 1,
                    line: 20,
                    target_taken: false,
                    outcome: SolveOutcome::Unsat,
                },
                SolvedBranch {
                    branch_id: 2,
                    line: 30,
                    target_taken: false,
                    outcome: SolveOutcome::Opaque {
                        hint: "regex".into(),
                    },
                },
            ],
            metrics: SolveMetrics {
                total_uncovered: 3,
                sat_count: 1,
                unsat_count: 1,
                opaque_count: 1,
                ..Default::default()
            },
        };

        let result = specify(&observe_stage, &analyze_out, &solve_out, false);
        let cc = &result.coverage_completeness;

        assert_eq!(cc.total_branch_directions, 6);
        assert_eq!(cc.observed, 3);
        assert_eq!(cc.proven_sat, 1);
        assert_eq!(cc.proven_unsat, 1);
        assert_eq!(cc.opaque, 1);
        // completeness = (3 + 1 + 1) / 6 ≈ 83.33%
        let expected_pct = (5.0 / 6.0) * 100.0;
        assert!((cc.completeness_pct - expected_pct).abs() < 0.01);
    }

    #[test]
    fn specify_empty_solve() {
        let (observe_stage, analyze_out) = stub_specify_inputs("noSolve", 2);

        let solve_out = StageSolveOutput {
            solved_branches: vec![],
            metrics: SolveMetrics::default(),
        };

        let result = specify(&observe_stage, &analyze_out, &solve_out, false);

        // All classes should be Observed (no Z3 enrichment).
        for class in &result.spec.classes {
            assert_eq!(
                class.precondition_provenance,
                crate::spec::Provenance::Observed
            );
        }

        // Only observed test suggestions (no solved).
        for ts in &result.test_suggestions {
            assert_eq!(ts.source, TestSuggestionSource::Observed);
        }
    }

    #[test]
    fn specify_with_invariants() {
        let (observe_stage, analyze_out) = stub_specify_inputs("invFunc", 1);

        let solve_out = StageSolveOutput {
            solved_branches: vec![],
            metrics: SolveMetrics::default(),
        };

        let result = specify(&observe_stage, &analyze_out, &solve_out, true);

        // With detect_invariants=true, the invariants detection runs.
        // With only 1 execution, function-wide invariants may or may not be detected,
        // but the pipeline should not error.
        assert_eq!(result.function_name, "invFunc");
    }
}
