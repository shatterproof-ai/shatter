//! Consumes frontend-produced `InvocationPlan`s, turning planner output into
//! concrete argument vectors that seed the explorer / orchestrator input pool.
//!
//! The shared entry point `fetch_planner_seeds` is invoked from the CLI
//! `--planner` path before either `explorer::explore_function` or
//! `orchestrator::explore` runs. Both paths consume the seeds identically,
//! preserving the single-source-of-truth rule for parallel explorer/
//! orchestrator code paths (see project-wide "parallel paths" contract).
//!
//! Scope: this pass materializes `Literal` and `Zero` `ValuePlanKind`s only.
//! `Random`, `Symbolic`, and `RuntimeValue` plan entries yield no seed for
//! the current target and fall through to the normal random / concolic input
//! generation (or, for `RuntimeValue`, to the producing frontend's own
//! runtime-value resolution at execute time). Callers that need to surface
//! planner ordering should pass plans in priority order; seeds are produced
//! in the order the frontend returned them.

use serde_json::Value;

use crate::frontend::{Frontend, FrontendError};
use crate::protocol::{
    Command as ProtoCommand, InvocationPlan, InvocationRequirement, ResponseResult,
    UnsatisfiedRequirement, ValuePlan, ValuePlanKind,
};
use crate::types::{ParamInfo, TypeInfo};

const CONSTRUCTOR_FILE_SEED_PREFIX: &str = "shatter-ctor-file-";
const CONSTRUCTOR_DIR_SEED_PREFIX: &str = "shatter-ctor-dir-";

/// Execute inputs plus scratch resources that must stay alive for the
/// frontend call using those inputs.
pub struct PlannedExecuteInputs {
    inputs: Vec<Value>,
    _scratch: Vec<ConstructorPathScratch>,
}

impl PlannedExecuteInputs {
    /// Borrow the concrete input vector.
    #[must_use]
    pub fn inputs(&self) -> &[Value] {
        &self.inputs
    }

    /// Split the concrete inputs from their scratch owners.
    #[must_use]
    pub fn into_parts(self) -> (Vec<Value>, Vec<ConstructorPathScratch>) {
        (self.inputs, self._scratch)
    }
}

pub enum ConstructorPathScratch {
    File { _file: tempfile::NamedTempFile },
    Dir { _dir: tempfile::TempDir },
}

/// Error returned when consulting the planner fails.
#[derive(Debug, thiserror::Error)]
pub enum PlannerConsumerError {
    #[error("frontend does not advertise the `get_invocation_plan` capability")]
    CapabilityMissing,
    #[error("frontend returned an unexpected response to get_invocation_plan: {0}")]
    UnexpectedResponse(String),
    #[error(transparent)]
    Frontend(#[from] FrontendError),
}

/// Result of one planner consultation.
#[derive(Debug, Clone, Default)]
pub struct PlannerSeedBundle {
    /// Argument vectors ready for seeding. Each inner `Vec<Value>` is one
    /// candidate invocation, positionally aligned with `param_infos`.
    pub seeds: Vec<Vec<Value>>,
    /// Raw plans returned by the frontend, for tracing / future use.
    pub plans: Vec<InvocationPlan>,
    /// Requirements the planner declined; surfaced so callers can log or
    /// filter targets that will need non-planner generation.
    pub unsatisfied: Vec<UnsatisfiedRequirement>,
}

/// Consult the frontend's planner for a single target.
///
/// Returns a `PlannerSeedBundle`. Callers treat `bundle.seeds` as additional
/// entries for the existing `seed_inputs` / `candidate_inputs` pools — the
/// orchestrator's `UserProvidedStrategy` and the explorer's
/// `candidate_inputs` path both consume `Vec<Vec<Value>>` shaped the same way.
///
/// # Errors
/// Returns an error if the frontend does not support the capability, returns
/// an error status, or returns a non-`InvocationPlan` response.
pub async fn fetch_planner_seeds(
    frontend: &mut Frontend,
    target_id: &str,
    param_infos: &[ParamInfo],
) -> Result<PlannerSeedBundle, PlannerConsumerError> {
    if !frontend
        .capabilities()
        .iter()
        .any(|cap| cap == "get_invocation_plan")
    {
        return Err(PlannerConsumerError::CapabilityMissing);
    }

    let requirements = vec![InvocationRequirement {
        target_id: target_id.to_string(),
        value_requirements: vec![],
        runtime_requirements: vec![],
    }];

    let response = frontend
        .send(ProtoCommand::GetInvocationPlan { requirements })
        .await?;

    match response.result {
        ResponseResult::InvocationPlan {
            plans,
            unsatisfied_requirements,
        } => Ok(PlannerSeedBundle {
            seeds: materialize_seeds(&plans, param_infos),
            plans,
            unsatisfied: unsatisfied_requirements,
        }),
        ResponseResult::Error { code, message, .. } => {
            Err(PlannerConsumerError::UnexpectedResponse(format!(
                "error code={code:?} message={message}"
            )))
        }
        other => Err(PlannerConsumerError::UnexpectedResponse(format!(
            "expected InvocationPlan, got {other:?}"
        ))),
    }
}

/// Map planner outputs to ready-to-execute argument vectors.
///
/// One seed is produced per `InvocationPlan` whose every `ValuePlan` is
/// directly materializable (`Literal` or `Zero`). Plans containing `Random`,
/// `Symbolic`, or `RuntimeValue` entries are skipped — those strategies are
/// already covered by the explorer's random generator, the orchestrator's
/// Z3 path, or the producing frontend's runtime-value resolution at execute
/// time, respectively.
#[must_use]
pub fn materialize_seeds(plans: &[InvocationPlan], param_infos: &[ParamInfo]) -> Vec<Vec<Value>> {
    let mut seeds = Vec::new();
    for plan in plans {
        if let Some(seed) = materialize_plan(plan, param_infos) {
            seeds.push(seed);
        }
    }
    seeds
}

fn materialize_plan(plan: &InvocationPlan, param_infos: &[ParamInfo]) -> Option<Vec<Value>> {
    if plan.argument_plans.len() != param_infos.len() {
        return None;
    }
    let mut values = Vec::with_capacity(plan.argument_plans.len());
    if !plan_requires_execution_scoped_constructor_scratch(plan) {
        values.extend(materialize_stored_constructor_arg_values(plan)?);
    }
    for (value_plan, param) in plan.argument_plans.iter().zip(param_infos.iter()) {
        let v = materialize_value(value_plan, param)?;
        values.push(v);
    }
    Some(values)
}

/// Materialize constructor argument plans into stored seed inputs.
///
/// Path-like constructor strings are intentionally skipped here. These seeds
/// can be cached and replayed later, but real temp file/dir resources need an
/// execution-scoped owner. [`execute_inputs_for_plan`] handles those prefixes
/// when an Execute request is actually sent.
#[must_use]
pub fn materialize_stored_constructor_arg_values(plan: &InvocationPlan) -> Option<Vec<Value>> {
    let mut values = Vec::with_capacity(plan.constructor_arg_plans.len());
    for value_plan in &plan.constructor_arg_plans {
        values.push(materialize_stored_constructor_value(value_plan)?);
    }
    Some(values)
}

fn plan_requires_execution_scoped_constructor_scratch(plan: &InvocationPlan) -> bool {
    plan.constructor_arg_plans
        .iter()
        .any(|value_plan| constructor_path_seed_kind(value_plan).is_some())
}

/// Return the concrete input vector to send to the frontend for a method plan.
///
/// Planner-generated constructor args are encoded as an input prefix before
/// method arguments. If the caller already provided that prefixed shape, the
/// vector is returned unchanged.
#[must_use]
pub fn execute_inputs_for_plan(
    inputs: &[Value],
    method_param_count: usize,
    plan: Option<&InvocationPlan>,
) -> PlannedExecuteInputs {
    let Some(plan) = plan else {
        return PlannedExecuteInputs {
            inputs: inputs.to_vec(),
            _scratch: Vec::new(),
        };
    };
    if plan.constructor_arg_plans.is_empty() || inputs.len() != method_param_count {
        return PlannedExecuteInputs {
            inputs: inputs.to_vec(),
            _scratch: Vec::new(),
        };
    }
    let Some((mut prefixed, scratch)) = materialize_execute_constructor_arg_values(plan) else {
        return PlannedExecuteInputs {
            inputs: inputs.to_vec(),
            _scratch: Vec::new(),
        };
    };
    prefixed.extend_from_slice(inputs);
    PlannedExecuteInputs {
        inputs: prefixed,
        _scratch: scratch,
    }
}

/// Strip a constructor-arg prefix from an executed vector for strategy feedback.
#[must_use]
pub fn strategy_feedback_inputs_for_plan(
    executed_inputs: &[Value],
    method_param_count: usize,
    plan: Option<&InvocationPlan>,
) -> Vec<Value> {
    let Some(plan) = plan else {
        return executed_inputs.to_vec();
    };
    let constructor_arg_count = plan.constructor_arg_plans.len();
    if constructor_arg_count == 0
        || executed_inputs.len() != method_param_count + constructor_arg_count
    {
        return executed_inputs.to_vec();
    }
    executed_inputs[constructor_arg_count..].to_vec()
}

fn materialize_value(value_plan: &ValuePlan, param: &ParamInfo) -> Option<Value> {
    match value_plan.kind {
        ValuePlanKind::Literal => value_plan.literal.clone(),
        ValuePlanKind::Zero => Some(zero_value(&param.typ)),
        ValuePlanKind::Random | ValuePlanKind::Symbolic | ValuePlanKind::RuntimeValue => None,
    }
}

fn materialize_stored_constructor_value(value_plan: &ValuePlan) -> Option<Value> {
    match value_plan.kind {
        ValuePlanKind::Literal => value_plan.literal.clone(),
        ValuePlanKind::Zero if constructor_path_seed_kind(value_plan).is_some() => None,
        ValuePlanKind::Zero => zero_constructor_value(value_plan),
        ValuePlanKind::Random | ValuePlanKind::Symbolic | ValuePlanKind::RuntimeValue => None,
    }
}

fn materialize_execute_constructor_arg_values(
    plan: &InvocationPlan,
) -> Option<(Vec<Value>, Vec<ConstructorPathScratch>)> {
    let mut values = Vec::with_capacity(plan.constructor_arg_plans.len());
    let mut scratch = Vec::new();
    for value_plan in &plan.constructor_arg_plans {
        let (value, mut owned) = materialize_execute_constructor_value(value_plan)?;
        values.push(value);
        scratch.append(&mut owned);
    }
    Some((values, scratch))
}

fn materialize_execute_constructor_value(
    value_plan: &ValuePlan,
) -> Option<(Value, Vec<ConstructorPathScratch>)> {
    match value_plan.kind {
        ValuePlanKind::Literal => value_plan.literal.clone().map(|value| (value, Vec::new())),
        ValuePlanKind::Zero => {
            if let Some(kind) = constructor_path_seed_kind(value_plan) {
                let (path, scratch) = create_constructor_path_seed(kind)?;
                return Some((Value::from(path), vec![scratch]));
            }
            Some((zero_value_for_type_hint(&value_plan.type_hint), Vec::new()))
        }
        ValuePlanKind::Random | ValuePlanKind::Symbolic | ValuePlanKind::RuntimeValue => None,
    }
}

/// Produce a JSON zero value for a given `TypeInfo`. Conservative: primitive
/// types get their canonical zero; aggregates and complex types become
/// `null`, letting the downstream input generator refine them.
fn zero_value(typ: &TypeInfo) -> Value {
    match typ {
        TypeInfo::Int => Value::from(0),
        TypeInfo::Float => Value::from(0.0),
        TypeInfo::Str => Value::from(""),
        TypeInfo::Bool => Value::from(false),
        _ => Value::Null,
    }
}

fn zero_value_for_type_hint(type_hint: &str) -> Value {
    match type_hint.trim() {
        "string" => Value::from(""),
        "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16" | "uint32"
        | "uint64" => Value::from(0),
        "float32" | "float64" => Value::from(0.0),
        "bool" => Value::from(false),
        "[]byte" | "[]uint8" => Value::Array(vec![]),
        "time.Duration" => Value::from(0),
        other if other.ends_with(".Duration") => Value::from(0),
        _ => Value::Null,
    }
}

fn zero_constructor_value(value_plan: &ValuePlan) -> Option<Value> {
    Some(zero_value_for_type_hint(&value_plan.type_hint))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstructorPathSeedKind {
    File,
    Dir,
}

fn constructor_path_seed_kind(value_plan: &ValuePlan) -> Option<ConstructorPathSeedKind> {
    let type_hint = value_plan.type_hint.trim();
    if type_hint != "string" {
        return None;
    }
    let name = value_plan.param_name.to_ascii_lowercase();
    if name_has_path_token(&value_plan.param_name, "dir")
        || name_has_path_token(&value_plan.param_name, "directory")
    {
        return Some(ConstructorPathSeedKind::Dir);
    }
    if matches!(
        name.as_str(),
        "path" | "file" | "filename" | "filepath" | "file_path"
    ) || name.ends_with("_path")
        || name.ends_with("path")
        || name.ends_with("_file")
    {
        return Some(ConstructorPathSeedKind::File);
    }
    None
}

fn create_constructor_path_seed(
    kind: ConstructorPathSeedKind,
) -> Option<(String, ConstructorPathScratch)> {
    let (path, scratch) = match kind {
        ConstructorPathSeedKind::File => {
            let file = tempfile::Builder::new()
                .prefix(CONSTRUCTOR_FILE_SEED_PREFIX)
                .tempfile()
                .map_err(|err| {
                    log::debug!("failed to create constructor file seed: {err}");
                    err
                })
                .ok()?;
            let path = file.path().to_path_buf();
            (path, ConstructorPathScratch::File { _file: file })
        }
        ConstructorPathSeedKind::Dir => {
            let dir = tempfile::Builder::new()
                .prefix(CONSTRUCTOR_DIR_SEED_PREFIX)
                .tempdir()
                .map_err(|err| {
                    log::debug!("failed to create constructor directory seed: {err}");
                    err
                })
                .ok()?;
            let path = dir.path().to_path_buf();
            (path, ConstructorPathScratch::Dir { _dir: dir })
        }
    };
    Some((path.to_string_lossy().into_owned(), scratch))
}

fn name_has_path_token(name: &str, token: &str) -> bool {
    let lower_name = name.to_ascii_lowercase();
    if lower_name == token
        || lower_name.starts_with(&format!("{token}_"))
        || lower_name.ends_with(&format!("_{token}"))
    {
        return true;
    }
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if ch.is_ascii_uppercase() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
        } else {
            current.push(ch.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.iter().any(|word| word == token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{InvocationPlan, ValuePlan};
    use serde_json::json;

    fn int_param(name: &str) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ: TypeInfo::Int,
            type_name: Some("int".into()),
        }
    }

    fn str_param(name: &str) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ: TypeInfo::Str,
            type_name: Some("string".into()),
        }
    }

    fn literal_plan(param_index: u32, param_name: &str, literal: Value) -> ValuePlan {
        ValuePlan {
            param_index,
            param_name: param_name.to_string(),
            kind: ValuePlanKind::Literal,
            literal: Some(literal),
            type_hint: String::new(),
        }
    }

    fn zero_plan(param_index: u32, param_name: &str) -> ValuePlan {
        ValuePlan {
            param_index,
            param_name: param_name.to_string(),
            kind: ValuePlanKind::Zero,
            literal: None,
            type_hint: String::new(),
        }
    }

    #[test]
    fn materialize_literal_and_zero_produces_seed() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "a", json!(7)), zero_plan(1, "b")],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[int_param("a"), int_param("b")]);
        assert_eq!(seeds, vec![vec![json!(7), json!(0)]]);
    }

    #[test]
    fn materialize_zero_str_returns_empty_string() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![zero_plan(0, "s")],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[str_param("s")]);
        assert_eq!(seeds, vec![vec![json!("")]]);
    }

    #[test]
    fn execute_constructor_arg_plans_prefixes_method_inputs_with_owned_dir() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:NewLoader".into(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "ns", json!("default"))],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "dir".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let planned = execute_inputs_for_plan(&[json!("default")], 1, Some(&plan));
        assert_eq!(planned.inputs().len(), 2);
        assert_eq!(planned.inputs().get(1), Some(&json!("default")));
        let Some(dir) = planned.inputs().first().and_then(Value::as_str) else {
            panic!(
                "expected a string constructor seed, got {:?}",
                planned.inputs()
            );
        };
        let dir = std::path::PathBuf::from(dir);
        assert!(
            dir.is_dir(),
            "directory-like constructor string should materialize as a usable directory",
        );
        drop(planned);
        assert!(
            !dir.exists(),
            "directory constructor scratch should be removed when planned inputs drop",
        );
    }

    #[test]
    fn execute_path_like_constructor_string_creates_unique_file_seeds() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:newJSONLRecorder".into(),
            generic_type_args: vec![],
            argument_plans: vec![],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "path".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let first = execute_inputs_for_plan(&[], 0, Some(&plan));
        let second = execute_inputs_for_plan(&[], 0, Some(&plan));
        let paths: Vec<std::path::PathBuf> = [first.inputs(), second.inputs()]
            .into_iter()
            .map(|seed| {
                std::path::PathBuf::from(
                    seed.first()
                        .and_then(Value::as_str)
                        .expect("expected a string constructor seed"),
                )
            })
            .collect();
        assert_ne!(
            paths[0], paths[1],
            "path-like constructor strings should use unique temp resources",
        );
        for path in &paths {
            assert!(
                path.is_file(),
                "path-like constructor string should materialize as a usable file: {path:?}",
            );
        }
        drop(first);
        drop(second);
        for path in &paths {
            assert!(
                !path.exists(),
                "file constructor scratch should be removed when planned inputs drop: {path:?}",
            );
        }
    }

    #[test]
    fn materialize_path_like_constructor_string_stores_method_shape_seed() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:newJSONLRecorder".into(),
            generic_type_args: vec![],
            argument_plans: vec![],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "path".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[]);
        assert_eq!(
            seeds,
            vec![Vec::<Value>::new()],
            "stored planner seeds must omit constructor temp path prefixes",
        );
    }

    #[test]
    fn materialize_path_like_constructor_string_keeps_method_seed() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:newJSONLRecorder".into(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "event", json!({"Listener": "\n"}))],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "path".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[str_param("event")]);
        assert_eq!(seeds, vec![vec![json!({"Listener": "\n"})]]);
    }

    #[test]
    fn execute_dir_path_constructor_string_creates_directory_seed() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:NewLoader".into(),
            generic_type_args: vec![],
            argument_plans: vec![],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "dirPath".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let planned = execute_inputs_for_plan(&[], 0, Some(&plan));
        let path = std::path::PathBuf::from(
            planned
                .inputs()
                .first()
                .and_then(Value::as_str)
                .expect("expected a string constructor seed"),
        );
        assert!(path.is_dir(), "dirPath should materialize as a directory");
    }

    #[test]
    fn materialize_non_path_constructor_string_stays_empty() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:NewProfile".into(),
            generic_type_args: vec![],
            argument_plans: vec![],
            constructor_arg_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "profile".into(),
                kind: ValuePlanKind::Zero,
                literal: None,
                type_hint: "string".into(),
            }],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[]);
        assert_eq!(seeds, vec![vec![json!("")]]);
    }

    #[test]
    fn symbolic_plan_is_skipped() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "a".into(),
                kind: ValuePlanKind::Symbolic,
                literal: None,
                type_hint: String::new(),
            }],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[int_param("a")]);
        assert!(seeds.is_empty(), "symbolic plan should not materialize");
    }

    #[test]
    fn runtime_value_plan_is_skipped() {
        // Mirrors the Go planner's runtimeValuePlans output: kind=runtime_value,
        // literal carries a JSON-encoded source expression, type_hint names the
        // registered Go type. The consumer must accept the wire form
        // (str-1hlk.4) but skip materialization — the Go launcher resolves the
        // value at execute time via planner.LookupRuntimeValue.
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "ctx".into(),
                kind: ValuePlanKind::RuntimeValue,
                literal: Some(json!("context.Background()")),
                type_hint: "context.Context".into(),
            }],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[int_param("ctx")]);
        assert!(
            seeds.is_empty(),
            "runtime_value plan should not materialize in core consumer",
        );
    }

    #[test]
    fn mismatched_arity_skips_plan() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "a", json!(7))],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        // Two params, one argument plan — skip.
        let seeds = materialize_seeds(&[plan], &[int_param("a"), int_param("b")]);
        assert!(seeds.is_empty());
    }

    #[test]
    fn multiple_plans_become_multiple_seeds() {
        let mk = |v: i64| InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "a", json!(v))],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[mk(1), mk(2), mk(3)], &[int_param("a")]);
        assert_eq!(seeds, vec![vec![json!(1)], vec![json!(2)], vec![json!(3)]]);
    }
}
