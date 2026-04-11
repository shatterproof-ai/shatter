//! Recursive function testing via iterative deepening with behavior map bootstrapping.
//!
//! Self-recursive functions are tested by:
//! 1. Probing with small/boundary values to discover base-case behaviors
//! 2. Mocking self-calls using known behaviors and deepening iteratively
//! 3. Stopping when coverage stabilizes or max depth is reached

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::behavior::BehaviorMap;
use crate::execution_record::ExecutionRecord;
use crate::frontend::{Frontend, FrontendError};
use crate::protocol::{
    Command as ProtoCommand, FunctionAnalysis, MockBehavior, MockConfig, ResponseResult,
};
use crate::types::TypeInfo;

/// Default maximum deepening depth for recursive function exploration.
pub const DEFAULT_MAX_DEPTH: u32 = 10;

/// Configuration for recursive function exploration.
#[derive(Debug, Clone)]
pub struct RecursiveConfig {
    /// Maximum deepening iterations before stopping.
    pub max_depth: u32,
    /// Maximum probe executions for base-case discovery.
    pub max_probes: usize,
}

impl Default for RecursiveConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_probes: 20,
        }
    }
}

/// Result of exploring a self-recursive function.
#[derive(Debug)]
pub struct RecursiveResult {
    pub function_name: String,
    pub behavior_map: BehaviorMap,
    /// Depth at which coverage stabilized (or max_depth if it didn't).
    pub depth_reached: u32,
    /// Number of base-case behaviors found via probing.
    pub base_case_count: usize,
    /// Total probe executions performed.
    pub probes_executed: usize,
}

/// Errors during recursive exploration.
#[derive(Debug, thiserror::Error)]
pub enum RecursiveError {
    #[error("frontend error: {0}")]
    Frontend(#[from] FrontendError),
    #[error("unexpected response from frontend: {0}")]
    UnexpectedResponse(String),
    #[error("no base cases found after probing")]
    NoBaseCases,
}

/// Generate canonical probe values for a type.
///
/// Returns small/boundary values likely to trigger base cases in recursive
/// functions (e.g., 0, 1, -1 for numbers; "" for strings; [] for arrays).
fn probe_values(typ: &TypeInfo) -> Vec<Value> {
    match typ {
        TypeInfo::Int => vec![json!(0), json!(1), json!(-1), json!(2)],
        TypeInfo::Float => vec![json!(0.0), json!(1.0), json!(-1.0), json!(0.5)],
        TypeInfo::Str => vec![json!(""), json!("a")],
        TypeInfo::Bool => vec![json!(false), json!(true)],
        TypeInfo::Array { .. } => vec![json!([]), json!([1])],
        TypeInfo::Object { fields } => {
            // Single probe: object with minimal field values
            let mut obj = serde_json::Map::new();
            for (name, field_typ) in fields {
                let vals = probe_values(field_typ);
                if let Some(v) = vals.into_iter().next() {
                    obj.insert(name.clone(), v);
                }
            }
            vec![Value::Object(obj)]
        }
        TypeInfo::Union { variants } => {
            // Probe values from each variant
            variants.iter().flat_map(probe_values).collect()
        }
        TypeInfo::Nullable { inner } => {
            let mut vals = vec![Value::Null];
            vals.extend(probe_values(inner));
            vals
        }
        // Complex types use same probes as Unknown for now
        TypeInfo::Complex { .. } => vec![json!(0), json!(1), json!(-1), json!(""), json!(null)],
        // Opaque types cannot be constructed; use same fallback probes as Unknown
        TypeInfo::Opaque { .. } => vec![json!(0), json!(1), json!(-1), json!(""), json!(null)],
        TypeInfo::Unknown => vec![json!(0), json!(1), json!(-1), json!(""), json!(null)],
    }
}

/// Generate combinatorial probe inputs for a function, capped at `max_probes`.
///
/// For single-parameter functions, returns probe values directly.
/// For multi-parameter functions, takes the cartesian product, capped.
fn generate_probe_inputs(params: &[crate::types::ParamInfo], max_probes: usize) -> Vec<Vec<Value>> {
    if params.is_empty() {
        return vec![vec![]];
    }

    let per_param: Vec<Vec<Value>> = params.iter().map(|p| probe_values(&p.typ)).collect();

    // Cartesian product with budget cap
    let mut results: Vec<Vec<Value>> = vec![vec![]];
    for param_vals in &per_param {
        let mut next = Vec::new();
        for existing in &results {
            for val in param_vals {
                if next.len() >= max_probes {
                    return next;
                }
                let mut combo = existing.clone();
                combo.push(val.clone());
                next.push(combo);
            }
        }
        results = next;
    }

    results
}

/// Probe a self-recursive function with small values to find base cases.
///
/// Executes the function with no mocks. Executions that don't call the function
/// itself are base cases. Returns execution records for base-case executions.
async fn probe_for_base_cases(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &RecursiveConfig,
) -> Result<(Vec<ExecutionRecord>, usize), RecursiveError> {
    let probe_inputs = generate_probe_inputs(&analysis.params, config.max_probes);
    let mut base_cases = Vec::new();
    let mut probes_executed = 0;

    for inputs in probe_inputs {
        probes_executed += 1;

        let response = frontend
            .send(ProtoCommand::Execute {
                function: analysis.name.clone(),
                inputs: inputs.clone(),
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
            })
            .await?;

        let exec_result = match response.result {
            ResponseResult::Execute(result) => result,
            ResponseResult::Error { code, message, .. } => {
                return Err(RecursiveError::UnexpectedResponse(format!(
                    "execute error ({code:?}): {message}"
                )));
            }
            other => {
                return Err(RecursiveError::UnexpectedResponse(format!("{other:?}")));
            }
        };

        // A base case: the function didn't call itself
        let calls_self = exec_result
            .calls_to_external
            .iter()
            .any(|c| c.symbol == analysis.name);

        if !calls_self {
            let input_hash = hash_inputs(&inputs);
            base_cases.push(make_exec_record(&analysis.name, input_hash, inputs, &exec_result));
        }
    }

    Ok((base_cases, probes_executed))
}

/// Explore a self-recursive function by iterative deepening.
///
/// 1. Probe with small values to find base cases (depth 0).
/// 2. For each subsequent depth, mock self-calls using the BehaviorMap from
///    all previous depths and execute with random inputs.
/// 3. Stop when no new behaviors are discovered or max_depth is reached.
pub async fn explore_recursive(
    frontend: &mut Frontend,
    analysis: &FunctionAnalysis,
    config: &RecursiveConfig,
) -> Result<RecursiveResult, RecursiveError> {
    // Step 1: Probe for base cases
    let (base_records, probes_executed) =
        probe_for_base_cases(frontend, analysis, config).await?;
    let base_case_count = base_records.len();

    if base_records.is_empty() {
        return Err(RecursiveError::NoBaseCases);
    }

    let mut all_records = base_records;
    let mut depth: u32 = 0;

    // Step 2: Iterative deepening
    for d in 1..=config.max_depth {
        depth = d;
        let current_map = BehaviorMap::from_records(&analysis.name, &all_records);
        let mock = MockConfig {
            symbol: analysis.name.clone(),
            return_values: current_map
                .behaviors
                .iter()
                .filter_map(|b| b.return_value.clone())
                .collect(),
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        };

        // Generate inputs that are slightly larger than the probe values
        let depth_inputs = generate_depth_inputs(&analysis.params, d);
        let behavior_count_before = current_map.behaviors.len();
        let mut found_new = false;

        let seen_hashes: HashSet<u64> = all_records.iter().map(|r| r.input_hash).collect();

        for inputs in depth_inputs {
            let input_hash = hash_inputs(&inputs);
            if seen_hashes.contains(&input_hash) {
                continue;
            }

            let response = frontend
                .send(ProtoCommand::Execute {
                    function: analysis.name.clone(),
                    inputs: inputs.clone(),
                    mocks: vec![mock.clone()],
                    setup_context: None,
                    capture: true,
                    prepare_id: None,
                    execution_profile: None,
                })
                .await?;

            let exec_result = match response.result {
                ResponseResult::Execute(result) => result,
                ResponseResult::Error { .. } => continue,
                _ => continue,
            };

            all_records.push(make_exec_record(&analysis.name, input_hash, inputs, &exec_result));
        }

        let updated_map = BehaviorMap::from_records(&analysis.name, &all_records);
        if updated_map.behaviors.len() > behavior_count_before {
            found_new = true;
        }

        if !found_new {
            break;
        }
    }

    let behavior_map = BehaviorMap::from_records(&analysis.name, &all_records);

    Ok(RecursiveResult {
        function_name: analysis.name.clone(),
        behavior_map,
        depth_reached: depth,
        base_case_count,
        probes_executed,
    })
}

/// Generate inputs for a given deepening depth.
///
/// At depth d, generates values around d (e.g., d, d+1, d-1 for integers)
/// to explore one level deeper than the previous iteration.
fn generate_depth_inputs(params: &[crate::types::ParamInfo], depth: u32) -> Vec<Vec<Value>> {
    if params.is_empty() {
        return vec![vec![]];
    }

    let d = depth as i64;
    let per_param: Vec<Vec<Value>> = params
        .iter()
        .map(|p| match &p.typ {
            TypeInfo::Int => vec![json!(d), json!(d + 1), json!(d + 2)],
            TypeInfo::Float => vec![json!(d as f64), json!((d + 1) as f64)],
            TypeInfo::Str => {
                vec![json!("a".repeat(depth as usize))]
            }
            TypeInfo::Array { element } => {
                // Array of length `depth` with minimal elements
                let elem = match element.as_ref() {
                    TypeInfo::Int => json!(1),
                    TypeInfo::Str => json!("a"),
                    TypeInfo::Bool => json!(true),
                    _ => json!(1),
                };
                vec![Value::Array(vec![elem; depth as usize])]
            }
            _ => probe_values(&p.typ),
        })
        .collect();

    // Cartesian product, capped at 20
    let mut results: Vec<Vec<Value>> = vec![vec![]];
    for param_vals in &per_param {
        let mut next = Vec::new();
        for existing in &results {
            for val in param_vals {
                if next.len() >= 20 {
                    return next;
                }
                let mut combo = existing.clone();
                combo.push(val.clone());
                next.push(combo);
            }
        }
        results = next;
    }

    results
}

/// Result of exploring a mutually-recursive function group.
#[derive(Debug)]
pub struct MutualRecursiveResult {
    /// Function IDs in the group.
    pub function_ids: Vec<String>,
    /// BehaviorMap for each function in the group.
    pub behavior_maps: HashMap<String, BehaviorMap>,
    /// Depth at which coverage stabilized.
    pub depth_reached: u32,
}

/// Explore a mutually-recursive function group by iterative deepening.
///
/// All functions in the group are bootstrapped simultaneously:
/// 1. Probe each function for base cases (executions that don't call any
///    other function in the group).
/// 2. At each depth, mock cross-calls within the group using BehaviorMaps
///    from previous depths.
/// 3. Stop when no new behaviors are found across any function or max_depth.
pub async fn explore_mutual_group(
    frontend: &mut Frontend,
    analyses: &HashMap<String, FunctionAnalysis>,
    group: &[String],
    config: &RecursiveConfig,
) -> Result<MutualRecursiveResult, RecursiveError> {
    let group_set: HashSet<&str> = group.iter().map(|s| s.as_str()).collect();

    // Per-function records
    let mut all_records: HashMap<String, Vec<ExecutionRecord>> = HashMap::new();
    for id in group {
        all_records.insert(id.clone(), Vec::new());
    }

    // Step 1: Probe each function for base cases
    for func_id in group {
        let analysis = analyses
            .get(func_id)
            .ok_or_else(|| RecursiveError::UnexpectedResponse(format!("missing analysis for {func_id}")))?;

        let probe_inputs = generate_probe_inputs(&analysis.params, config.max_probes);

        for inputs in probe_inputs {
            let response = frontend
                .send(ProtoCommand::Execute {
                    function: func_id.clone(),
                    inputs: inputs.clone(),
                    mocks: vec![],
                    setup_context: None,
                    capture: true,
                    prepare_id: None,
                    execution_profile: None,
                })
                .await?;

            let exec_result = match response.result {
                ResponseResult::Execute(result) => result,
                ResponseResult::Error { .. } => continue,
                _ => continue,
            };

            // Base case: doesn't call any function in the group
            let calls_group = exec_result
                .calls_to_external
                .iter()
                .any(|c| group_set.contains(c.symbol.as_str()));

            if !calls_group {
                let input_hash = hash_inputs(&inputs);
                let records = all_records.get_mut(func_id).expect("func in map");
                records.push(make_exec_record(func_id, input_hash, inputs, &exec_result));
            }
        }
    }

    // Check that at least one function has base cases
    let total_base = all_records.values().map(|r| r.len()).sum::<usize>();
    if total_base == 0 {
        return Err(RecursiveError::NoBaseCases);
    }

    // Step 2: Iterative deepening
    let mut depth: u32 = 0;
    for d in 1..=config.max_depth {
        depth = d;

        // Build mocks for each function in the group from current records
        let mut mocks: Vec<MockConfig> = Vec::new();
        for func_id in group {
            let records = &all_records[func_id];
            if records.is_empty() {
                continue;
            }
            let bmap = BehaviorMap::from_records(func_id, records);
            mocks.push(MockConfig {
                symbol: func_id.clone(),
                return_values: bmap
                    .behaviors
                    .iter()
                    .filter_map(|b| b.return_value.clone())
                    .collect(),
                should_track_calls: true,
                default_behavior: MockBehavior::RepeatLast,
            });
        }

        let total_before: usize = all_records.values().map(|r| r.len()).sum();

        // Explore each function at this depth
        for func_id in group {
            let analysis = analyses.get(func_id).expect("func in analyses");
            let depth_inputs = generate_depth_inputs(&analysis.params, d);

            let seen_hashes: HashSet<u64> = all_records[func_id].iter().map(|r| r.input_hash).collect();

            for inputs in depth_inputs {
                let input_hash = hash_inputs(&inputs);
                if seen_hashes.contains(&input_hash) {
                    continue;
                }

                let response = frontend
                    .send(ProtoCommand::Execute {
                        function: func_id.clone(),
                        inputs: inputs.clone(),
                        mocks: mocks.clone(),
                        setup_context: None,
                        capture: true,
                        prepare_id: None,
                        execution_profile: None,
                    })
                    .await?;

                let exec_result = match response.result {
                    ResponseResult::Execute(result) => result,
                    ResponseResult::Error { .. } => continue,
                    _ => continue,
                };

                let records = all_records.get_mut(func_id).expect("func in map");
                records.push(make_exec_record(func_id, input_hash, inputs, &exec_result));
            }
        }

        let total_after: usize = all_records.values().map(|r| r.len()).sum();
        if total_after == total_before {
            break; // Coverage stabilized
        }
    }

    let mut behavior_maps = HashMap::new();
    for (func_id, records) in &all_records {
        behavior_maps.insert(func_id.clone(), BehaviorMap::from_records(func_id, records));
    }

    Ok(MutualRecursiveResult {
        function_ids: group.to_vec(),
        behavior_maps,
        depth_reached: depth,
    })
}

/// Build an ExecutionRecord from an ExecuteResult.
fn make_exec_record(
    function_id: &str,
    input_hash: u64,
    inputs: Vec<Value>,
    result: &crate::protocol::ExecuteResult,
) -> ExecutionRecord {
    ExecutionRecord {
        function_id: function_id.to_string(),
        input_hash,
        parameters: inputs,
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

/// Hash a set of input values for deduplication.
fn hash_inputs(inputs: &[Value]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for input in inputs {
        input.to_string().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ParamInfo;

    #[test]
    fn probe_values_int_includes_boundaries() {
        let vals = probe_values(&TypeInfo::Int);
        assert!(vals.contains(&json!(0)));
        assert!(vals.contains(&json!(1)));
        assert!(vals.contains(&json!(-1)));
    }

    #[test]
    fn probe_values_str_includes_empty() {
        let vals = probe_values(&TypeInfo::Str);
        assert!(vals.contains(&json!("")));
    }

    #[test]
    fn probe_values_array_includes_empty() {
        let vals = probe_values(&TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        });
        assert!(vals.contains(&json!([])));
    }

    #[test]
    fn probe_values_nullable_includes_null() {
        let vals = probe_values(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Int),
        });
        assert!(vals.contains(&Value::Null));
        // Also includes inner type probes
        assert!(vals.contains(&json!(0)));
    }

    #[test]
    fn generate_probe_inputs_single_param() {
        let params = vec![ParamInfo {
            name: "n".into(),
            typ: TypeInfo::Int,
            type_name: None,
        }];
        let inputs = generate_probe_inputs(&params, 20);
        // Should have probe values for Int: 0, 1, -1, 2
        assert_eq!(inputs.len(), 4);
        assert_eq!(inputs[0], vec![json!(0)]);
        assert_eq!(inputs[1], vec![json!(1)]);
    }

    #[test]
    fn generate_probe_inputs_multi_param_capped() {
        let params = vec![
            ParamInfo {
                name: "a".into(),
                typ: TypeInfo::Int,
                type_name: None,
            },
            ParamInfo {
                name: "b".into(),
                typ: TypeInfo::Int,
                type_name: None,
            },
            ParamInfo {
                name: "c".into(),
                typ: TypeInfo::Int,
                type_name: None,
            },
        ];
        // 4^3 = 64 combinations, should be capped at 20
        let inputs = generate_probe_inputs(&params, 20);
        assert!(inputs.len() <= 20);
    }

    #[test]
    fn generate_probe_inputs_empty_params() {
        let inputs = generate_probe_inputs(&[], 20);
        assert_eq!(inputs, vec![Vec::<Value>::new()]);
    }

    #[test]
    fn generate_depth_inputs_grows_with_depth() {
        let params = vec![ParamInfo {
            name: "n".into(),
            typ: TypeInfo::Int,
            type_name: None,
        }];
        let d1 = generate_depth_inputs(&params, 1);
        let d3 = generate_depth_inputs(&params, 3);
        // Depth 1 should include value 1, depth 3 should include value 3
        assert!(d1.iter().any(|v| v.contains(&json!(1))));
        assert!(d3.iter().any(|v| v.contains(&json!(3))));
    }

    #[test]
    fn generate_depth_inputs_array_grows() {
        let params = vec![ParamInfo {
            name: "arr".into(),
            typ: TypeInfo::Array {
                element: Box::new(TypeInfo::Int),
            },
            type_name: None,
        }];
        let d2 = generate_depth_inputs(&params, 2);
        // Should contain an array of length 2
        assert!(d2.iter().any(|v| {
            v.first()
                .and_then(|a| a.as_array())
                .is_some_and(|a| a.len() == 2)
        }));
    }

    #[test]
    fn hash_inputs_deterministic() {
        let inputs = vec![json!(1), json!("hello")];
        let h1 = hash_inputs(&inputs);
        let h2 = hash_inputs(&inputs);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_inputs_distinct() {
        let a = vec![json!(1)];
        let b = vec![json!(2)];
        assert_ne!(hash_inputs(&a), hash_inputs(&b));
    }

}
