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

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Output of the Analyze stage.
#[derive(Debug)]
pub struct AnalyzeOutput {
    /// Equivalence classes grouping executions by branch path.
    pub eq_classes: Vec<EquivalenceClass>,
    /// Behavior map built from execution records.
    pub behavior_map: BehaviorMap,
    /// Branch coverage metrics with per-method attribution.
    pub coverage_metrics: CoverageMetrics,
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
        .map(|(inputs, result)| execution_record_from_result(&observe.function_name, inputs, result))
        .collect();

    let mut behavior_map = BehaviorMap::from_records(&observe.function_name, &records);
    behavior_map.nondeterministic_fields = observe.nondeterministic_fields.clone();

    // Deduplicate constraints by branch_id so each branch contributes exactly one classification.
    let unique_constraints: HashMap<u32, SymConstraint> = observe
        .raw_results
        .iter()
        .flat_map(|(_, result)| {
            result
                .branch_path
                .iter()
                .map(|d| (d.branch_id, d.constraint.clone()))
        })
        .collect();
    let all_constraints: Vec<SymConstraint> = unique_constraints.into_values().collect();

    let coverage_metrics = CoverageMetrics::from_exploration(
        analysis.branches.len(),
        &observe.discoveries,
        &all_constraints,
    );

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
        for (_, result) in &r.raw_results {
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
            capture_truncation: None,
            performance: empty_perf(),
        };

        let observe = ObservationOutput {
            function_name: "classify".into(),
            iterations: 5,
            unique_paths: 1,
            lines_covered: 3,
            total_lines: 5,
            new_path_executions: vec![],
            raw_results: vec![(vec![json!(5)], exec_result)],
            discoveries: vec![(0, DiscoveryMethod::Random)],
            nondeterministic_fields: vec![], float_probe_results: vec![],
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
            nondeterministic_fields: vec![], float_probe_results: vec![],
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
                },
                BranchDecision {
                    branch_id: 1,
                    line: 20,
                    taken: false,
                    constraint: SymConstraint::Unknown {
                        hint: "opaque".into(),
                    },
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
            capture_truncation: None,
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
                (vec![json!(1)], make_result(json!("a"))),
                (vec![json!(2)], make_result(json!("b"))),
                (vec![json!(3)], make_result(json!("c"))),
            ],
            discoveries: vec![(0, DiscoveryMethod::Random)],
            nondeterministic_fields: vec![], float_probe_results: vec![],
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
            float_probe_results: vec![],
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
    fn observation_output_from_concolic_result() {
        let concolic = crate::orchestrator::ExploreResult {
            function_name: "test".into(),
            total_lines: 10,
            executions: vec![],
            unique_paths: 2,
            total_executions: 10,
            z3_generated: 3,
            fuzz_generated: 1,
            drill_generated: 0,
            termination_reason: crate::orchestrator::TerminationReason::WorklistExhausted,
            raw_results: vec![],
            discoveries: vec![(0, DiscoveryMethod::Random)],
            triage_skipped: 0,
            triage_mispredictions: 0,
            nondeterministic_fields: vec![], float_probe_results: vec![],
        };

        let output: ObservationOutput = concolic.into();
        assert_eq!(output.function_name, "test");
        assert_eq!(output.unique_paths, 2);
        assert_eq!(output.total_lines, 10);
        assert_eq!(output.discoveries.len(), 1);
    }
}
