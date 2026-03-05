//! Concolic execution loop: worklist-driven path exploration with Z3 solving.
//!
//! The orchestrator drives the concolic testing cycle:
//! 1. Seed the worklist with initial inputs
//! 2. Execute a function concretely via a frontend subprocess
//! 3. Collect branch constraints from the execution trace
//! 4. Negate each branch constraint and solve with Z3 for new inputs
//! 5. Add solved inputs to the worklist; repeat until done
//!
//! Inputs from Z3 are prioritized over fuzzed or seed inputs so that the
//! solver-guided exploration is always tried first.

use std::collections::{BinaryHeap, HashSet};
use std::hash::{Hash, Hasher};

use crate::coverage_metrics::DiscoveryMethod;
use crate::execution_record::SymConstraint;
use crate::frontend::{Frontend, FrontendError};
use crate::protocol::{Command, ExecuteResult, ResponseResult};
use crate::solver::{self, ConcreteValue, SolveResult};
use crate::sym_expr::SymExpr;
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
    /// Z3 solver timeout in milliseconds per query. None means no limit.
    pub solver_timeout_ms: Option<u64>,
}

/// Default maximum total executions before stopping exploration.
pub const DEFAULT_MAX_EXECUTIONS: usize = 500;

impl Default for ExploreConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            max_executions: DEFAULT_MAX_EXECUTIONS,
            plateau_threshold: 20,
            mocks: vec![],
            solver_timeout_ms: None,
        }
    }
}

/// How an input was generated — determines worklist priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputSource {
    /// Least priority: initial seed values.
    Seed = 0,
    /// Medium priority: fuzzed from concrete values of unknown constraints.
    Fuzzed = 1,
    /// High priority: Z3-solved inputs targeting a specific branch.
    Z3Solved = 2,
    /// Highest priority: user-provided candidate inputs from `.shatter/` config.
    UserProvided = 3,
}

/// An entry in the exploration worklist.
#[derive(Debug, Clone)]
pub struct WorklistEntry {
    /// Input values to pass to the function.
    pub inputs: Vec<serde_json::Value>,
    /// How these inputs were generated.
    pub source: InputSource,
}

impl Eq for WorklistEntry {}

impl PartialEq for WorklistEntry {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl PartialOrd for WorklistEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorklistEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.source.cmp(&other.source)
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
    /// Why the exploration loop stopped.
    pub termination_reason: TerminationReason,
    /// Raw execution results paired with inputs for pipeline composability.
    pub raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)>,
    /// Per-branch discovery attribution with method (Z3, Random, UserProvided).
    pub discoveries: Vec<(u32, DiscoveryMethod)>,
}

/// Errors that can occur during concolic exploration.
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
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
fn extract_sym_constraints(result: &ExecuteResult) -> Vec<Option<SymExpr>> {
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
fn concrete_to_json(value: &ConcreteValue) -> serde_json::Value {
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
fn overlay_solved_values(
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

/// Run the concolic exploration loop on a function via a frontend subprocess.
///
/// `function_name` is the fully-qualified name of the function to explore.
/// `seed_inputs` provides initial input sets to begin exploration.
/// `user_inputs` provides user-provided candidate inputs (highest priority).
/// `param_infos` provides parameter metadata including names and types. Names are
/// used to map Z3 variables back to parameter positions; types are used to declare
/// correct Z3 sorts (preventing string params from being declared as Int).
pub async fn explore(
    frontend: &mut Frontend,
    function_name: &str,
    seed_inputs: Vec<Vec<serde_json::Value>>,
    user_inputs: Vec<Vec<serde_json::Value>>,
    param_infos: &[ParamInfo],
    config: &ExploreConfig,
) -> Result<ExploreResult, ExploreError> {
    let param_names: Vec<String> = param_infos.iter().map(|p| p.name.clone()).collect();
    let mut worklist = BinaryHeap::new();
    let mut covered_paths: HashSet<u64> = HashSet::new();
    let mut executions = Vec::new();
    let mut raw_results: Vec<(Vec<serde_json::Value>, ExecuteResult)> = Vec::new();
    let mut seen_branch_ids: HashSet<u32> = HashSet::new();
    let mut discoveries: Vec<(u32, DiscoveryMethod)> = Vec::new();
    let mut total_executions: usize = 0;
    let mut z3_generated: usize = 0;
    let mut fuzz_generated: usize = 0;
    let mut plateau_counter: usize = 0;
    let mut termination_reason = TerminationReason::WorklistExhausted;

    // Add user-provided candidates with highest priority.
    for inputs in user_inputs {
        worklist.push(WorklistEntry {
            inputs,
            source: InputSource::UserProvided,
        });
    }

    // Seed the worklist.
    for inputs in seed_inputs {
        worklist.push(WorklistEntry {
            inputs,
            source: InputSource::Seed,
        });
    }

    while let Some(entry) = worklist.pop() {
        if executions.len() >= config.max_iterations {
            termination_reason = TerminationReason::MaxIterations;
            break;
        }
        if total_executions >= config.max_executions {
            termination_reason = TerminationReason::MaxExecutions;
            break;
        }
        if config.plateau_threshold > 0 && plateau_counter >= config.plateau_threshold {
            termination_reason = TerminationReason::CoveragePlateau;
            break;
        }

        // Execute concretely via the frontend.
        let response = frontend
            .send(Command::Execute {
                function: function_name.to_string(),
                inputs: entry.inputs.clone(),
                mocks: config.mocks.clone(),
                setup_context: None,
            })
            .await?;

        total_executions += 1;

        let exec_result = match response.result {
            ResponseResult::Execute(result) => *result,
            ResponseResult::Error { message, .. } => {
                // Frontend reported an error — skip this input but continue exploring.
                log::warn!("frontend error during execute: {message}");
                continue;
            }
            _ => continue,
        };

        // Record raw result for pipeline composability.
        raw_results.push((entry.inputs.clone(), exec_result.clone()));

        let path_id = hash_branch_path(&exec_result.branch_path);

        if !covered_paths.insert(path_id) {
            // Already covered this path — skip solving.
            plateau_counter += 1;
            continue;
        }

        // New path discovered — reset plateau counter.
        plateau_counter = 0;

        // Track per-branch discovery attribution from input source.
        let method = match entry.source {
            InputSource::Z3Solved => DiscoveryMethod::Z3,
            InputSource::UserProvided => DiscoveryMethod::UserProvided,
            InputSource::Seed | InputSource::Fuzzed => DiscoveryMethod::Random,
        };
        for decision in &exec_result.branch_path {
            if seen_branch_ids.insert(decision.branch_id) {
                discoveries.push((decision.branch_id, method));
            }
        }

        // Extract symbolic constraints from the branch path.
        let sym_constraints = extract_sym_constraints(&exec_result);

        // Collect the solvable prefix for each branch negation attempt.
        let solvable: Vec<SymExpr> = sym_constraints.iter().filter_map(|c| c.clone()).collect();

        // Try to negate each branch constraint with Z3.
        // For each solvable constraint at index i, build the prefix of all prior
        // solvable constraints and negate the i-th one.
        if !solvable.is_empty() {
            for negate_idx in 0..solvable.len() {
                match solver::solve_for_new_path(&solvable, negate_idx, config.solver_timeout_ms, param_infos) {
                    Ok(SolveResult::Sat(values)) => {
                        let new_inputs =
                            overlay_solved_values(&entry.inputs, &values, &param_names);
                        worklist.push(WorklistEntry {
                            inputs: new_inputs,
                            source: InputSource::Z3Solved,
                        });
                        z3_generated += 1;
                    }
                    Ok(SolveResult::Unsat) => {
                        // This path is infeasible — nothing to do.
                    }
                    Err(_) => {
                        // Solver error (unsupported expr, etc.) — skip this branch.
                    }
                }
            }
        }

        // For Unknown constraints, generate fuzzed inputs by slightly mutating
        // the concrete values that reached the branch.
        for (i, constraint_opt) in sym_constraints.iter().enumerate() {
            if constraint_opt.is_none() && i < exec_result.branch_path.len() {
                // Simple fuzzing: try flipping booleans and perturbing numbers in the input.
                for fuzzed in fuzz_inputs(&entry.inputs) {
                    worklist.push(WorklistEntry {
                        inputs: fuzzed,
                        source: InputSource::Fuzzed,
                    });
                    fuzz_generated += 1;
                }
                // Only fuzz once per execution (avoid exponential blowup).
                break;
            }
        }

        executions.push(exec_result);
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
        termination_reason,
        raw_results,
        discoveries,
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

    /// Verify that the worklist priority queue drains Z3-solved inputs before seeds.
    #[test]
    fn worklist_drains_in_priority_order() {
        let mut worklist = BinaryHeap::new();

        // Push in arbitrary order.
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("seed1")],
            source: InputSource::Seed,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_1")],
            source: InputSource::Z3Solved,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("fuzz1")],
            source: InputSource::Fuzzed,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("z3_2")],
            source: InputSource::Z3Solved,
        });
        worklist.push(WorklistEntry {
            inputs: vec![serde_json::json!("seed2")],
            source: InputSource::Seed,
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
}
