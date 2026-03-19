//! Staged pipeline: composable Observe → Analyze → Solve → Specify stages.
//!
//! Each stage is a pure function with well-defined input/output types.
//! The Analyze stage groups raw exploration results into equivalence classes,
//! builds a behavior map, and computes coverage metrics.

use crate::behavior::BehaviorMap;
use crate::coverage_metrics::CoverageMetrics;
use crate::equivalence::{self, EquivalenceClass};
use crate::execution_record::{ExecutionRecord, SymConstraint};
use crate::explorer::ObservationOutput;
use crate::protocol::{ExecuteResult, FunctionAnalysis};
use crate::spec::FunctionSpec;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    Ok(()
    )
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
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
            float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None,
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
        };

        let output: ObservationOutput = concolic.into();
        assert_eq!(output.function_name, "test");
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.total_lines, 10);
        assert_eq!(output.discoveries.len(), 1);
    }
}
