//! Pipeline orchestrator: composes the four stages (Observe → Analyze → Solve → Specify)
//! with proper data flow, supports partial pipelines and iterative refinement.
//!
//! Stage 1 (Observe) is async and requires a live frontend subprocess.
//! Stages 2–4 are pure synchronous functions composed from [`crate::pipeline`].
//!
//! The primary entry point is [`run_pipeline`] for the full async pipeline.
//! For offline workflows that start from a pre-existing observe output,
//! use the sync convenience functions [`run_analyze_solve_specify`] or
//! [`run_analyze_solve`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::explorer;
use crate::frontend::{Frontend, FrontendError};
use crate::orchestrator;
use crate::pipeline::{
    self, AnalyzeOutput, ObserveStageOutput, SolveOutcome, SpecifyStageOutput, StageIoError,
    StageSolveOutput,
};
use crate::protocol::{Command as ProtoCommand, FunctionAnalysis};

/// Errors that can occur during pipeline orchestration.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Frontend communication or lifecycle error (Stage 1).
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),

    /// Exploration error during the Observe stage (random explorer).
    #[error("observe stage failed: {0}")]
    Explore(#[from] explorer::ExploreError),

    /// Exploration error during the Observe stage (concolic orchestrator).
    #[error("concolic observe failed: {0}")]
    ConcolicExplore(#[from] orchestrator::ExploreError),

    /// Function not found in static analysis results.
    #[error("function '{0}' not found in analysis")]
    FunctionNotFound(String),

    /// Unexpected response from the frontend.
    #[error("unexpected frontend response: {0}")]
    UnexpectedResponse(String),

    /// Stage I/O error (reading/writing intermediate JSON).
    #[error(transparent)]
    StageIo(#[from] StageIoError),
}

/// Which stages of the pipeline to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageSet {
    /// Observe only.
    Observe,
    /// Observe + Analyze.
    ObserveAnalyze,
    /// Observe + Analyze + Solve.
    ObserveAnalyzeSolve,
    /// Full pipeline: Observe + Analyze + Solve + Specify.
    Full,
    /// Stages 2–4 only, given pre-existing observe output.
    AnalyzeSolveSpecify,
    /// Stages 2–3 only, given pre-existing observe output.
    AnalyzeSolve,
}

impl StageSet {
    /// Whether this stage set includes the Observe stage.
    pub fn includes_observe(self) -> bool {
        matches!(
            self,
            Self::Observe | Self::ObserveAnalyze | Self::ObserveAnalyzeSolve | Self::Full
        )
    }

    /// Whether this stage set includes the Analyze stage.
    pub fn includes_analyze(self) -> bool {
        !matches!(self, Self::Observe)
    }

    /// Whether this stage set includes the Solve stage.
    pub fn includes_solve(self) -> bool {
        matches!(
            self,
            Self::ObserveAnalyzeSolve | Self::Full | Self::AnalyzeSolveSpecify | Self::AnalyzeSolve
        )
    }

    /// Whether this stage set includes the Specify stage.
    pub fn includes_specify(self) -> bool {
        matches!(self, Self::Full | Self::AnalyzeSolveSpecify)
    }
}

/// Configuration for a pipeline orchestration run.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Which stages to run. Defaults to [`StageSet::Full`].
    pub stages: StageSet,

    /// Solver timeout for the Solve stage (milliseconds). `None` uses Z3 default.
    pub solver_timeout_ms: Option<u64>,

    /// Whether to detect Daikon-style invariants in the Specify stage.
    pub detect_invariants: bool,

    /// Number of iteration rounds (observe → solve → re-observe with solved inputs).
    /// 1 = single pass (no iteration). 0 is treated as 1.
    pub iteration_rounds: u32,

    /// Optional directory to persist intermediate stage outputs as JSON.
    /// When `None`, data flows in-memory only.
    pub persist_dir: Option<PathBuf>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            stages: StageSet::Full,
            solver_timeout_ms: None,
            detect_invariants: false,
            iteration_rounds: 1,
            persist_dir: None,
        }
    }
}

/// Input required for the Observe stage.
///
/// Bundles a live frontend subprocess and function metadata so the caller
/// (typically the CLI) can prepare these externally.
pub struct ObserveInput<'a> {
    /// A spawned, ready-to-use frontend subprocess.
    pub frontend: &'a mut Frontend,

    /// Source file path.
    pub file: String,

    /// Function name to explore.
    pub function_name: String,

    /// Static analysis of the function (from frontend Analyze command).
    pub analysis: FunctionAnalysis,

    /// Explorer configuration (random exploration).
    pub explore_config: explorer::ExploreConfig,

    /// Whether to use concolic (orchestrator) instead of random exploration.
    pub use_concolic: bool,

    /// Concolic-specific config (only used when `use_concolic` is true).
    pub concolic_config: Option<orchestrator::ExploreConfig>,

    /// Prepared harness ID (from a prior `prepare` command), if available.
    pub prepare_id: Option<String>,

    /// Project root path for frontend commands.
    pub project_root: Option<String>,

    /// Additional solved inputs to include as seeds (from iteration or external source).
    pub extra_seeds: Vec<Vec<serde_json::Value>>,
}

/// Collected outputs from a pipeline run.
///
/// Each field is `Option` because partial pipelines skip later stages.
/// When `iteration_rounds > 1`, the fields hold the *final* iteration's output.
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineResult {
    /// Observe stage output (always present when observe runs).
    pub observe: Option<ObserveStageOutput>,
    /// Analyze stage output.
    pub analyze: Option<AnalyzeOutput>,
    /// Solve stage output.
    pub solve: Option<StageSolveOutput>,
    /// Specify stage output (terminal stage).
    pub specify: Option<SpecifyStageOutput>,
    /// Number of iteration rounds actually completed.
    pub iterations_completed: u32,
    /// Per-iteration summaries (for diagnostics when iterating).
    pub iteration_summaries: Vec<IterationSummary>,
}

/// Summary of a single pipeline iteration round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationSummary {
    /// Which round (1-indexed).
    pub round: u32,
    /// Unique paths discovered in this round's observe stage.
    pub unique_paths: usize,
    /// Branches solved (Sat outcomes) in this round.
    pub branches_solved: usize,
    /// Sat inputs generated that will seed the next round.
    pub new_seeds: usize,
}

// ---------------------------------------------------------------------------
// Sync convenience functions (Stages 2–4, no frontend needed)
// ---------------------------------------------------------------------------

/// Run Analyze + Solve + Specify from a pre-existing observe output.
///
/// This is the primary offline API: no frontend, no async runtime needed.
/// All three stages are pure functions composed in sequence.
pub fn run_analyze_solve_specify(
    observe: &ObserveStageOutput,
    solver_timeout_ms: Option<u64>,
    detect_invariants: bool,
) -> (AnalyzeOutput, StageSolveOutput, SpecifyStageOutput) {
    let analyze_out = pipeline::analyze(&observe.observation, &observe.analysis);
    let solve_out = pipeline::solve(observe, solver_timeout_ms);
    let specify_out = pipeline::specify(observe, &analyze_out, &solve_out, detect_invariants);
    (analyze_out, solve_out, specify_out)
}

/// Run Analyze + Solve from a pre-existing observe output.
///
/// Useful when you want coverage analysis and constraint solving
/// but don't need a full behavioral specification.
pub fn run_analyze_solve(
    observe: &ObserveStageOutput,
    solver_timeout_ms: Option<u64>,
) -> (AnalyzeOutput, StageSolveOutput) {
    let analyze_out = pipeline::analyze(&observe.observation, &observe.analysis);
    let solve_out = pipeline::solve(observe, solver_timeout_ms);
    (analyze_out, solve_out)
}

/// Extract satisfying inputs from solve results to use as seeds for re-observation.
pub fn extract_sat_seeds(solve: &StageSolveOutput) -> Vec<Vec<serde_json::Value>> {
    solve
        .solved_branches
        .iter()
        .filter_map(|sb| match &sb.outcome {
            SolveOutcome::Sat { inputs } => Some(inputs.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Async pipeline (includes Stage 1: Observe)
// ---------------------------------------------------------------------------

/// Run the pipeline according to the given configuration.
///
/// Stage 1 (Observe) is async; Stages 2–4 are synchronous but run within
/// the async context for uniformity.
///
/// When `config.iteration_rounds > 1`, the pipeline iterates:
/// observe → analyze → solve → extract Sat inputs → re-observe with those
/// inputs as seeds → … until rounds exhausted or no new Sat inputs.
///
/// Specify runs only on the final iteration (if included in `config.stages`).
pub async fn run_pipeline(
    input: &mut ObserveInput<'_>,
    config: &PipelineConfig,
) -> Result<PipelineResult, PipelineError> {
    let rounds = config.iteration_rounds.max(1);

    let mut observe_out: Option<ObserveStageOutput> = None;
    let mut analyze_out: Option<AnalyzeOutput> = None;
    let mut solve_out: Option<StageSolveOutput> = None;
    let mut specify_out: Option<SpecifyStageOutput> = None;
    let mut iteration_summaries = Vec::new();
    let mut extra_seeds = std::mem::take(&mut input.extra_seeds);

    for round in 1..=rounds {
        // --- Stage 1: Observe ---
        let obs = run_observe_stage(input, &extra_seeds).await?;

        if let Some(ref dir) = config.persist_dir {
            let path = dir.join(format!("observe-round-{round}.json"));
            pipeline::write_observe_stage(&obs, &path)?;
        }

        // --- Stage 2: Analyze ---
        let ana = if config.stages.includes_analyze() {
            let a = pipeline::analyze(&obs.observation, &obs.analysis);
            if let Some(ref dir) = config.persist_dir {
                let stage_output = pipeline::AnalyzeStageOutput {
                    analyze: AnalyzeOutput {
                        eq_classes: a.eq_classes.clone(),
                        behavior_map: a.behavior_map.clone(),
                        coverage_metrics: a.coverage_metrics.clone(),
                    },
                    spec: None,
                    function_name: obs.observation.function_name.clone(),
                    file: obs.file.clone(),
                };
                let path = dir.join(format!("analyze-round-{round}.json"));
                pipeline::write_analyze_stage(&stage_output, &path)?;
            }
            Some(a)
        } else {
            None
        };

        // --- Stage 3: Solve ---
        let sol = if config.stages.includes_solve() {
            let s = pipeline::solve(&obs, config.solver_timeout_ms);
            if let Some(ref dir) = config.persist_dir {
                let stage_output = pipeline::SolveStageOutput {
                    solve: StageSolveOutput {
                        solved_branches: s.solved_branches.clone(),
                        metrics: s.metrics.clone(),
                    },
                    function_name: obs.observation.function_name.clone(),
                    file: obs.file.clone(),
                };
                let path = dir.join(format!("solve-round-{round}.json"));
                pipeline::write_solve_stage(&stage_output, &path)?;
            }
            Some(s)
        } else {
            None
        };

        // Build iteration summary.
        let new_seeds_vec = sol.as_ref().map(extract_sat_seeds).unwrap_or_default();
        iteration_summaries.push(IterationSummary {
            round,
            unique_paths: obs.observation.unique_paths,
            branches_solved: sol.as_ref().map_or(0, |s| s.metrics.sat_count),
            new_seeds: new_seeds_vec.len(),
        });

        observe_out = Some(obs);
        analyze_out = ana;
        solve_out = sol;

        // Check if we should iterate: need new seeds and not the final round.
        if round < rounds && !new_seeds_vec.is_empty() {
            extra_seeds = new_seeds_vec;
        } else {
            break;
        }
    }

    // --- Stage 4: Specify (final iteration only) ---
    if config.stages.includes_specify()
        && let (Some(obs), Some(ana), Some(sol)) =
            (observe_out.as_ref(), analyze_out.as_ref(), solve_out.as_ref())
    {
        let spec = pipeline::specify(obs, ana, sol, config.detect_invariants);
        if let Some(ref dir) = config.persist_dir {
            let path = dir.join("specify.json");
            pipeline::write_specify_stage(&spec, &path)?;
        }
        specify_out = Some(spec);
    }

    Ok(PipelineResult {
        observe: observe_out,
        analyze: analyze_out,
        solve: solve_out,
        specify: specify_out,
        iterations_completed: iteration_summaries.len() as u32,
        iteration_summaries,
    })
}

/// Run the Observe stage: execute the function and collect traces.
async fn run_observe_stage(
    input: &mut ObserveInput<'_>,
    extra_seeds: &[Vec<serde_json::Value>],
) -> Result<ObserveStageOutput, PipelineError> {
    // Instrument the function.
    let project_root = input.project_root.clone();
    let instrument_resp = input
        .frontend
        .send(ProtoCommand::Instrument {
            file: input.file.clone(),
            function: input.function_name.clone(),
            mocks: vec![],
            project_root: project_root.clone(),
            execution_profile: None,
        })
        .await;

    if let Err(e) = instrument_resp {
        log::debug!("instrument failed: {e}");
    }

    let observation = if input.use_concolic {
        run_concolic_observe(input, extra_seeds).await?
    } else {
        run_random_observe(input, extra_seeds).await?
    };

    Ok(ObserveStageOutput {
        observation,
        analysis: input.analysis.clone(),
        file: input.file.clone(),
    })
}

/// Run concolic exploration (orchestrator path).
async fn run_concolic_observe(
    input: &mut ObserveInput<'_>,
    extra_seeds: &[Vec<serde_json::Value>],
) -> Result<explorer::ObservationOutput, PipelineError> {
    let concolic_config = input.concolic_config.as_ref().ok_or_else(|| {
        PipelineError::UnexpectedResponse(
            "concolic mode requested but no concolic_config provided".into(),
        )
    })?;

    let mut seed_inputs =
        crate::boundary_dict::generate_boundary_inputs(&input.analysis.params);
    seed_inputs.extend(extra_seeds.iter().cloned());

    let result = orchestrator::explore(
        input.frontend,
        &input.function_name,
        seed_inputs,
        vec![],
        &input.analysis.params,
        concolic_config,
        None,
        input.prepare_id.clone(),
        input.analysis.loops.clone(),
    )
    .await?;

    let mut obs: explorer::ObservationOutput = result.into();
    obs.total_lines = input
        .analysis
        .end_line
        .saturating_sub(input.analysis.start_line)
        + 1;
    Ok(obs)
}

/// Run random exploration (explorer path).
async fn run_random_observe(
    input: &mut ObserveInput<'_>,
    extra_seeds: &[Vec<serde_json::Value>],
) -> Result<explorer::ObservationOutput, PipelineError> {
    let mut config = input.explore_config.clone();
    config.user_seeds.extend(extra_seeds.iter().cloned());

    let obs = explorer::explore_function(
        input.frontend,
        &input.analysis,
        &config,
        None,
    )
    .await?;

    Ok(obs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage_metrics::DiscoveryMethod;
    use crate::execution_record::{BranchDecision, SymConstraint};
    use crate::explorer::ObservationOutput;
    use crate::pipeline::{SolveMetrics, SolvedBranch, TestSuggestionSource};
    use crate::protocol::{BranchInfo, BranchType, PerformanceMetrics};
    use crate::sym_expr::SymExpr;
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
                .map(|i| BranchInfo {
                    id: i as u32,
                    line: (i as u32 + 1) * 10,
                    condition_text: format!("x > {i}"),
                    condition: None,
                    branch_type: BranchType::If,
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

    fn stub_observe_output(
        name: &str,
        branch_count: usize,
    ) -> ObserveStageOutput {
        // Create an observation with one path that covers branch 0 taken=true.
        let branch_path = (0..branch_count)
            .map(|i| BranchDecision {
                branch_id: i as u32,
                line: (i as u32 + 1) * 10,
                taken: true,
                constraint: if i == 0 {
                    SymConstraint::Expr {
                        expr: SymExpr::BinOp {
                            op: crate::sym_expr::BinOpKind::Gt,
                            left: Box::new(SymExpr::Param {
                                name: "x".into(),
                                path: vec![],
                            }),
                            right: Box::new(SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
                        },
                    }
                } else {
                    SymConstraint::Unknown {
                        hint: "opaque".into(),
                    }
                },
                conditions: None,
            })
            .collect();

        let exec_result = crate::protocol::ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path,
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };

        let observation = ObservationOutput {
            function_name: name.into(),
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
        };

        let analysis = stub_analysis(name, branch_count);

        ObserveStageOutput {
            observation,
            analysis,
            file: "test.ts".into(),
        }
    }

    // -- Unit tests --

    #[test]
    fn run_analyze_solve_specify_composes_correctly() {
        let observe = stub_observe_output("classify", 2);

        let (analyze, solve, specify) =
            run_analyze_solve_specify(&observe, None, false);

        // Analyze should produce eq classes and metrics.
        assert_eq!(analyze.eq_classes.len(), 1);
        assert_eq!(analyze.coverage_metrics.total_branches, 2);

        // Solve should target uncovered branch directions.
        assert!(!solve.solved_branches.is_empty());
        assert!(solve.metrics.total_uncovered > 0);

        // Specify should produce a spec with test suggestions.
        assert!(!specify.spec.classes.is_empty());
        assert_eq!(specify.function_name, "classify");
        assert_eq!(specify.file, "test.ts");

        // Coverage completeness should be consistent.
        let cc = &specify.coverage_completeness;
        assert_eq!(cc.total_branch_directions, 4); // 2 branches × 2 directions
        assert!(cc.observed > 0);
        assert!(cc.completeness_pct >= 0.0 && cc.completeness_pct <= 100.0);
    }

    #[test]
    fn run_analyze_solve_returns_consistent_pair() {
        let observe = stub_observe_output("pair_test", 3);

        let (analyze, solve) = run_analyze_solve(&observe, None);

        assert_eq!(analyze.coverage_metrics.total_branches, 3);
        assert!(solve.metrics.total_uncovered > 0);
        // All branches should be accounted for in solve metrics.
        let total_accounted = solve.metrics.sat_count
            + solve.metrics.unsat_count
            + solve.metrics.opaque_count
            + solve.metrics.unreachable_count
            + solve.metrics.error_count;
        assert_eq!(total_accounted, solve.metrics.total_uncovered);
    }

    #[test]
    fn extract_sat_seeds_filters_correctly() {
        let solve = StageSolveOutput {
            solved_branches: vec![
                SolvedBranch {
                    branch_id: 0,
                    line: 10,
                    target_taken: false,
                    outcome: SolveOutcome::Sat {
                        inputs: vec![json!(-1)],
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
                        hint: "unknown".into(),
                    },
                },
                SolvedBranch {
                    branch_id: 3,
                    line: 40,
                    target_taken: true,
                    outcome: SolveOutcome::Sat {
                        inputs: vec![json!(100)],
                    },
                },
                SolvedBranch {
                    branch_id: 4,
                    line: 50,
                    target_taken: false,
                    outcome: SolveOutcome::Unreachable,
                },
                SolvedBranch {
                    branch_id: 5,
                    line: 60,
                    target_taken: true,
                    outcome: SolveOutcome::Error {
                        message: "timeout".into(),
                    },
                },
            ],
            metrics: SolveMetrics::default(),
        };

        let seeds = extract_sat_seeds(&solve);
        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0], vec![json!(-1)]);
        assert_eq!(seeds[1], vec![json!(100)]);
    }

    #[test]
    fn extract_sat_seeds_empty_when_no_sat() {
        let solve = StageSolveOutput {
            solved_branches: vec![
                SolvedBranch {
                    branch_id: 0,
                    line: 10,
                    target_taken: false,
                    outcome: SolveOutcome::Unsat,
                },
            ],
            metrics: SolveMetrics::default(),
        };

        let seeds = extract_sat_seeds(&solve);
        assert!(seeds.is_empty());
    }

    #[test]
    fn pipeline_config_defaults() {
        let config = PipelineConfig::default();
        assert_eq!(config.stages, StageSet::Full);
        assert_eq!(config.solver_timeout_ms, None);
        assert!(!config.detect_invariants);
        assert_eq!(config.iteration_rounds, 1);
        assert!(config.persist_dir.is_none());
    }

    #[test]
    fn stage_set_includes_methods() {
        assert!(StageSet::Full.includes_observe());
        assert!(StageSet::Full.includes_analyze());
        assert!(StageSet::Full.includes_solve());
        assert!(StageSet::Full.includes_specify());

        assert!(StageSet::Observe.includes_observe());
        assert!(!StageSet::Observe.includes_analyze());
        assert!(!StageSet::Observe.includes_solve());
        assert!(!StageSet::Observe.includes_specify());

        assert!(!StageSet::AnalyzeSolveSpecify.includes_observe());
        assert!(StageSet::AnalyzeSolveSpecify.includes_analyze());
        assert!(StageSet::AnalyzeSolveSpecify.includes_solve());
        assert!(StageSet::AnalyzeSolveSpecify.includes_specify());

        assert!(!StageSet::AnalyzeSolve.includes_observe());
        assert!(StageSet::AnalyzeSolve.includes_analyze());
        assert!(StageSet::AnalyzeSolve.includes_solve());
        assert!(!StageSet::AnalyzeSolve.includes_specify());

        assert!(StageSet::ObserveAnalyze.includes_observe());
        assert!(StageSet::ObserveAnalyze.includes_analyze());
        assert!(!StageSet::ObserveAnalyze.includes_solve());
        assert!(!StageSet::ObserveAnalyze.includes_specify());

        assert!(StageSet::ObserveAnalyzeSolve.includes_observe());
        assert!(StageSet::ObserveAnalyzeSolve.includes_analyze());
        assert!(StageSet::ObserveAnalyzeSolve.includes_solve());
        assert!(!StageSet::ObserveAnalyzeSolve.includes_specify());
    }

    #[test]
    fn pipeline_result_serialization_roundtrip() {
        let observe = stub_observe_output("roundtrip_test", 1);
        let (analyze, solve, specify) =
            run_analyze_solve_specify(&observe, None, false);

        let result = PipelineResult {
            observe: Some(observe),
            analyze: Some(analyze),
            solve: Some(solve),
            specify: Some(specify),
            iterations_completed: 1,
            iteration_summaries: vec![IterationSummary {
                round: 1,
                unique_paths: 3,
                branches_solved: 1,
                new_seeds: 1,
            }],
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let deserialized: PipelineResult =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.iterations_completed, 1);
        assert_eq!(deserialized.iteration_summaries.len(), 1);
        assert!(deserialized.observe.is_some());
        assert!(deserialized.analyze.is_some());
        assert!(deserialized.solve.is_some());
        assert!(deserialized.specify.is_some());
    }

    #[test]
    fn iteration_summary_roundtrip() {
        let summary = IterationSummary {
            round: 2,
            unique_paths: 5,
            branches_solved: 3,
            new_seeds: 2,
        };

        let json = serde_json::to_string(&summary).expect("serialize");
        let deserialized: IterationSummary =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(summary, deserialized);
    }

    #[test]
    fn full_pipeline_data_flow_integrity() {
        // Build a realistic observe output with 3 branches, only branch 0 covered in
        // both directions, branch 1 covered taken=true only, branch 2 never reached.
        let branch_path_exec1 = vec![
            BranchDecision {
                branch_id: 0,
                line: 10,
                taken: true,
                constraint: SymConstraint::Expr {
                    expr: SymExpr::BinOp {
                        op: crate::sym_expr::BinOpKind::Gt,
                        left: Box::new(SymExpr::Param {
                            name: "x".into(),
                            path: vec![],
                        }),
                        right: Box::new(SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
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

        let branch_path_exec2 = vec![BranchDecision {
            branch_id: 0,
            line: 10,
            taken: false,
            constraint: SymConstraint::Expr {
                expr: SymExpr::BinOp {
                    op: crate::sym_expr::BinOpKind::Gt,
                    left: Box::new(SymExpr::Param {
                        name: "x".into(),
                        path: vec![],
                    }),
                    right: Box::new(SymExpr::Const(crate::sym_expr::ConstValue::Int(0))),
                },
            },
            conditions: None,
        }];

        let exec1 = crate::protocol::ExecuteResult {
            return_value: Some(json!("positive")),
            thrown_error: None,
            branch_path: branch_path_exec1,
            lines_executed: vec![1, 2, 3],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };

        let exec2 = crate::protocol::ExecuteResult {
            return_value: Some(json!("negative")),
            thrown_error: None,
            branch_path: branch_path_exec2,
            lines_executed: vec![1, 4, 5],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            performance: empty_perf(),
        };

        let observation = ObservationOutput {
            function_name: "classify".into(),
            iterations: 10,
            unique_paths: 2,
            lines_covered: 5,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![
                (vec![json!(5)], vec![], exec1),
                (vec![json!(-3)], vec![], exec2),
            ],
            discoveries: vec![
                (0, DiscoveryMethod::Random),
                (0, DiscoveryMethod::Random),
            ],
            nondeterministic_fields: vec![],
            float_probe_results: vec![],
            boundary_results: vec![],
            shrunk_witnesses: std::collections::HashMap::new(),
            mcdc_summary: None,
            shrink_stats: crate::shrink::ShrinkStats::default(),
            abandoned_frontiers: vec![],
            opaque_suggestions: vec![],
            stubbed_modules: vec![],
        };

        let observe = ObserveStageOutput {
            observation,
            analysis: stub_analysis("classify", 3),
            file: "test.ts".into(),
        };

        let (analyze, solve, specify) =
            run_analyze_solve_specify(&observe, None, false);

        // Analyze: should have equivalence classes.
        assert!(!analyze.eq_classes.is_empty());
        assert_eq!(analyze.coverage_metrics.total_branches, 3);

        // Solve: should target uncovered directions.
        // Branch 0: both directions observed → 0 uncovered.
        // Branch 1: only taken=true → 1 uncovered (taken=false), but constraint is opaque.
        // Branch 2: never reached → 2 uncovered (both directions).
        assert!(solve.metrics.total_uncovered >= 3);

        // Specify: coverage completeness should be mathematically consistent.
        let cc = &specify.coverage_completeness;
        assert_eq!(cc.total_branch_directions, 6); // 3 branches × 2
        let total_accounted = cc.observed
            + cc.proven_sat
            + cc.proven_unsat
            + cc.opaque
            + cc.unreachable
            + cc.solver_errors;
        assert_eq!(total_accounted, cc.total_branch_directions);

        // Test suggestions should exist from both observed and solved sources.
        assert!(!specify.test_suggestions.is_empty());
        let has_observed = specify
            .test_suggestions
            .iter()
            .any(|ts| ts.source == TestSuggestionSource::Observed);
        assert!(has_observed, "should have observed-source test suggestions");
    }

    // -- Proptest --

    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::*;
        use proptest::prelude::*;

        fn arb_solve_outcome() -> BoxedStrategy<SolveOutcome> {
            prop_oneof![
                prop::collection::vec(arb_json_value(), 0..=3)
                    .prop_map(|inputs| SolveOutcome::Sat { inputs }),
                Just(SolveOutcome::Unsat),
                "[a-z]{1,10}".prop_map(|hint| SolveOutcome::Opaque { hint }),
                Just(SolveOutcome::Unreachable),
                "[a-z]{1,10}".prop_map(|message| SolveOutcome::Error { message }),
            ]
            .boxed()
        }

        fn arb_solved_branch() -> BoxedStrategy<SolvedBranch> {
            (0u32..100, 0u32..1000, any::<bool>(), arb_solve_outcome())
                .prop_map(|(branch_id, line, target_taken, outcome)| SolvedBranch {
                    branch_id,
                    line,
                    target_taken,
                    outcome,
                })
                .boxed()
        }

        fn arb_solve_metrics() -> BoxedStrategy<SolveMetrics> {
            (
                0usize..50,
                0usize..50,
                0usize..50,
                0usize..50,
                0usize..50,
                0usize..50,
            )
                .prop_map(
                    |(total, sat, unsat, opaque, unreachable, error)| SolveMetrics {
                        total_uncovered: total,
                        sat_count: sat,
                        unsat_count: unsat,
                        opaque_count: opaque,
                        unreachable_count: unreachable,
                        error_count: error,
                    },
                )
                .boxed()
        }

        fn arb_stage_solve_output() -> BoxedStrategy<StageSolveOutput> {
            (
                prop::collection::vec(arb_solved_branch(), 0..=10),
                arb_solve_metrics(),
            )
                .prop_map(|(solved_branches, metrics)| StageSolveOutput {
                    solved_branches,
                    metrics,
                })
                .boxed()
        }

        fn arb_iteration_summary() -> BoxedStrategy<IterationSummary> {
            (1u32..20, 0usize..100, 0usize..50, 0usize..50)
                .prop_map(
                    |(round, unique_paths, branches_solved, new_seeds)| IterationSummary {
                        round,
                        unique_paths,
                        branches_solved,
                        new_seeds,
                    },
                )
                .boxed()
        }

        proptest! {
            #[test]
            fn sat_seed_extraction_preserves_count(
                branches in prop::collection::vec(arb_solved_branch(), 0..=20)
            ) {
                let expected_count = branches
                    .iter()
                    .filter(|sb| matches!(sb.outcome, SolveOutcome::Sat { .. }))
                    .count();

                let solve = StageSolveOutput {
                    solved_branches: branches,
                    metrics: SolveMetrics::default(),
                };

                let seeds = extract_sat_seeds(&solve);
                prop_assert_eq!(seeds.len(), expected_count);
            }

            #[test]
            fn iteration_summary_json_roundtrip(
                summary in arb_iteration_summary()
            ) {
                let json = serde_json::to_string(&summary).expect("serialize");
                let deserialized: IterationSummary =
                    serde_json::from_str(&json).expect("deserialize");
                prop_assert_eq!(summary, deserialized);
            }

            #[test]
            fn stage_solve_output_json_roundtrip(
                output in arb_stage_solve_output()
            ) {
                let json = serde_json::to_string(&output).expect("serialize");
                let deserialized: StageSolveOutput =
                    serde_json::from_str(&json).expect("deserialize");
                prop_assert_eq!(
                    output.solved_branches.len(),
                    deserialized.solved_branches.len()
                );
                prop_assert_eq!(output.metrics, deserialized.metrics);
            }

            #[test]
            fn stage_set_roundtrip(
                stage_set in prop_oneof![
                    Just(StageSet::Observe),
                    Just(StageSet::ObserveAnalyze),
                    Just(StageSet::ObserveAnalyzeSolve),
                    Just(StageSet::Full),
                    Just(StageSet::AnalyzeSolveSpecify),
                    Just(StageSet::AnalyzeSolve),
                ]
            ) {
                let json = serde_json::to_string(&stage_set).expect("serialize");
                let deserialized: StageSet =
                    serde_json::from_str(&json).expect("deserialize");
                prop_assert_eq!(stage_set, deserialized);
            }
        }
    }
}
