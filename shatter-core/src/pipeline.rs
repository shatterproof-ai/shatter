//! Staged pipeline: composable Observe → Analyze → Solve → Specify stages.
//!
//! Each stage is a pure function with well-defined input/output types.
//! - **Observe**: execute the function with various inputs, collect traces.
//! - **Analyze**: group into equivalence classes, build behavior map, compute coverage.
//! - **Solve**: for uncovered branches, use Z3 to find triggering inputs.
//! - Specify (future): generate behavioral specifications from full-coverage data.

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
pub fn solve(
    observe: &ObserveStageOutput,
    solver_timeout_ms: Option<u64>,
) -> StageSolveOutput {
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
    raw_results: &[(Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>, ExecuteResult)],
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
        return SolveOutcome::Opaque {
            hint: hint.clone(),
        };
    }

    // Extract symbolic constraints from the witness execution.
    let raw_constraints = extract_sym_constraints(witness_result);

    // Apply loop constraint rewriting if loops are present.
    let rewritten = crate::loop_analysis::rewrite_loop_constraints(&raw_constraints, loops, witness_result);
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
            let inputs = overlay_solved_values(base_inputs, &solved_values, &param_names);
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
        .map(|(inputs, _mocks, result)| execution_record_from_result(&observe.function_name, inputs, result))
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

    let mut coverage_metrics = CoverageMetrics::from_exploration(
        analysis.branches.len(),
        &observe.discoveries,
        &all_constraints,
    );

    if let Some((total, independent, opaque)) = observe.mcdc_summary {
        coverage_metrics.mcdc_metrics =
            Some(crate::coverage_metrics::McdcMetrics::from_mcdc_summary(total, independent, opaque));
    }

    AnalyzeOutput {
        eq_classes,
        behavior_map,
        coverage_metrics,
    }
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
        // Build ExecutionSummary entries from unique-path executions.
        let new_path_executions: Vec<crate::explorer::ExecutionSummary> = r
            .executions
            .iter()
            .map(|exec| crate::explorer::ExecutionSummary {
                inputs: vec![], // Concolic explorer doesn't pair inputs with executions vec
                return_value: exec.return_value.clone(),
                thrown_error: exec
                    .thrown_error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.error_type, e.message)),
                lines_executed: exec.lines_executed.clone(),
                is_new_path: true,
                error_intent: crate::explorer::classify_error_intent(exec),
            })
            .collect();

        // Compute lines covered from raw_results.
        let mut all_lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for (_, _mocks, result) in &r.raw_results {
            for &line in &result.lines_executed {
                all_lines.insert(line);
            }
        }

        Self {
            function_name: r.function_name,
            iterations: r.total_executions as u32,
            unique_paths: r.unique_paths,
            lines_covered: all_lines.len(),
            total_lines: r.total_lines,
            new_path_executions,
            raw_results: r.raw_results,
            discoveries: r.discoveries,
            nondeterministic_fields: r.nondeterministic_fields,
            float_probe_results: r.float_probe_results,
            boundary_results: r.boundary_results,
            shrunk_witnesses: r.shrunk_witnesses,
            mcdc_summary: r.mcdc_summary,
            shrink_stats: r.shrink_stats,
            abandoned_frontiers: r.abandoned_frontiers,
            opaque_suggestions: r.opaque_suggestions,
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
                typ: TypeInfo::Int,
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
        }
    }

    #[test]
    fn analyze_produces_all_outputs() {
        let branch_path = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown { hint: "test".into() },
                conditions: None,
            },
        ];
        let exec_result = ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path,
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
        };

        let analysis = stub_analysis("dedup_test", 2);
        let output = analyze(&observe, &analysis);

        // Must be 2 (one per unique branch_id), not 6 (total observations).
        let constraint_total =
            output.coverage_metrics.symexpr_count + output.coverage_metrics.unknown_count;
        assert_eq!(constraint_total, 2, "constraints must equal unique branch_ids, not total observations");
        assert_eq!(output.coverage_metrics.symexpr_count, 1);
        assert_eq!(output.coverage_metrics.unknown_count, 1);
    }

    #[test]
    fn analyze_carries_nondeterministic_fields() {
        use crate::nondeterminism::{Confidence, NondeterministicField, NondeterminismEvidence};

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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
        };
        let analysis = stub_analysis("roundtrip", 2);
        let output = analyze(&observe, &analysis);

        let json = serde_json::to_string(&output).expect("serialize");
        let d: AnalyzeOutput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d.eq_classes.len(), output.eq_classes.len());
        assert_eq!(d.coverage_metrics.total_branches, output.coverage_metrics.total_branches);
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            branch_id: 0, line: 10, taken: true,
            constraint: SymConstraint::Unknown { hint: "t".into() },
            conditions: None,
        }];
        let branch_path_b = vec![BranchDecision {
            branch_id: 0, line: 10, taken: false,
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            pipeline_overlaps: 0,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
        };

        let output: ObservationOutput = concolic.into();
        assert_eq!(output.function_name, "test");
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.total_lines, 10);
        assert_eq!(output.discoveries.len(), 1);
    }

    // ---- Solve stage tests ----

    fn stub_observe_stage(
        name: &str,
        branch_count: usize,
        raw_results: Vec<(Vec<serde_json::Value>, Vec<crate::protocol::MockConfig>, ExecuteResult)>,
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
            branch_id: 0, line: 10, taken: true,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param { name: "x".into(), path: vec![] }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
                },
            },
            conditions: None,
        }];
        let branch_path_f = vec![BranchDecision {
            branch_id: 0, line: 10, taken: false,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param { name: "x".into(), path: vec![] }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };
        let observe = stub_observe_stage("all_covered", 1, vec![
            (vec![json!(5)], vec![], make_result(branch_path_t)),
            (vec![json!(-1)], vec![], make_result(branch_path_f)),
        ]);

        let output = solve(&observe, Some(1000));
        assert!(output.solved_branches.is_empty(), "no uncovered branches to solve");
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
            branch_id: 0, line: 10, taken: true,
            constraint: SymConstraint::Unknown { hint: "opaque call".into() },
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };
        let observe = stub_observe_stage("opaque", 1, vec![
            (vec![json!(1)], vec![], result),
        ]);

        let output = solve(&observe, Some(1000));
        assert_eq!(output.metrics.total_uncovered, 1);
        assert_eq!(output.metrics.opaque_count, 1);
        assert_eq!(output.solved_branches.len(), 1);
        assert_eq!(output.solved_branches[0].target_taken, false);
        assert!(matches!(output.solved_branches[0].outcome, SolveOutcome::Opaque { .. }));
    }

    #[test]
    fn solve_with_solvable_constraint() {
        // Branch 0: x > 0, only taken=true observed. Solve should find inputs for taken=false.
        let branch_path = vec![BranchDecision {
            branch_id: 0, line: 10, taken: true,
            constraint: SymConstraint::Expr {
                expr: crate::sym_expr::SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(crate::sym_expr::SymExpr::Param { name: "x".into(), path: vec![] }),
                    right: Box::new(crate::sym_expr::SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };
        let observe = stub_observe_stage("solvable", 1, vec![
            (vec![json!(5)], vec![], result),
        ]);

        let output = solve(&observe, Some(5000));
        assert_eq!(output.metrics.total_uncovered, 1);
        assert_eq!(output.solved_branches.len(), 1);
        assert_eq!(output.solved_branches[0].branch_id, 0);
        assert_eq!(output.solved_branches[0].target_taken, false);
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
                branch_id: 0, line: 10, taken: true,
                constraint: SymConstraint::Expr {
                    expr: crate::sym_expr::SymExpr::BinOp {
                        op: crate::sym_expr::BinOpKind::Gt,
                        left: Box::new(crate::sym_expr::SymExpr::Param { name: "x".into(), path: vec![] }),
                        right: Box::new(crate::sym_expr::SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
                    },
                },
                conditions: None,
            },
            BranchDecision {
                branch_id: 1, line: 20, taken: true,
                constraint: SymConstraint::Unknown { hint: "opaque".into() },
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![], runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };
        // 3 branches in analysis: branch 0 (solvable, one direction), branch 1 (opaque, one direction),
        // branch 2 (never reached).
        let observe = stub_observe_stage("tally", 3, vec![
            (vec![json!(5)], vec![], result),
        ]);

        let output = solve(&observe, Some(5000));
        let m = &output.metrics;

        // Tally check: sum of outcomes should equal total_uncovered.
        let tally = m.sat_count + m.unsat_count + m.opaque_count + m.unreachable_count + m.error_count;
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
                    outcome: SolveOutcome::Sat { inputs: vec![json!(42)] },
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
                    outcome: SolveOutcome::Opaque { hint: "test".into() },
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
                    outcome: SolveOutcome::Error { message: "timeout".into() },
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
        assert_eq!(d.solve.solved_branches[0].outcome, SolveOutcome::Sat { inputs: vec![json!(42)] });
        assert_eq!(d.solve.solved_branches[1].outcome, SolveOutcome::Unsat);
        assert_eq!(d.solve.solved_branches[3].outcome, SolveOutcome::Unreachable);
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
                    Just(json!(3.14)),
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
    }
}
