//! Consumes frontend-produced `InvocationPlan`s, turning planner output into
//! concrete argument vectors that seed the explorer / orchestrator input pool.
//!
//! The shared entry point `fetch_planner_seeds` is invoked from the CLI
//! `--planner` path before either `explorer::explore_function` or
//! `orchestrator::explore` runs. Both paths consume the seeds identically,
//! preserving the single-source-of-truth rule for parallel explorer/
//! orchestrator code paths (see project-wide "parallel paths" contract).
//!
//! Scope: this pass materializes `Literal`, `Zero`, and `RuntimeValue`
//! `ValuePlanKind`s. `Random` and `Symbolic` entries yield no seed and fall
//! through to normal random / concolic input generation. `RuntimeValue`
//! entries (e.g. `context.Context` params) are materialized as `null` — the
//! Go wrapper bakes the runtime expression at wrapper-compile time (str-gxjs.1)
//! and ignores the JSON input slot entirely, so `null` is always safe and
//! allows sibling `Literal`/`Zero` plans on the same function to survive.
//! Callers that need to surface planner ordering should pass plans in priority
//! order; seeds are produced in the order the frontend returned them.

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
    #[error("failed to materialize constructor path seed: {0}")]
    ConstructorPathSeed(String),
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
/// directly materializable. `Literal` plans produce their stored value;
/// `Zero` plans produce the zero value for their type; `RuntimeValue` plans
/// produce `null` (the Go wrapper bakes the runtime expression at compile
/// time and ignores the JSON slot, so null is always safe there). Plans
/// containing `Random` or `Symbolic` entries are skipped — those strategies
/// are driven by the explorer's random generator or the orchestrator's Z3 path.
#[must_use]
pub fn materialize_seeds(plans: &[InvocationPlan], param_infos: &[ParamInfo]) -> Vec<Vec<Value>> {
    let mut seeds = Vec::new();
    for plan in plans {
        if let Some(seed) = materialize_seed_for_plan(plan, param_infos) {
            seeds.push(seed);
        }
    }
    seeds
}

/// Materialize one plan into a stored seed input vector.
///
/// For plans requiring execution-scoped constructor scratch, the stored seed is
/// method-shaped and omits constructor prefixes. Callers that install a default
/// execute plan should keep this seed paired with the same plan.
#[must_use]
pub fn materialize_seed_for_plan(
    plan: &InvocationPlan,
    param_infos: &[ParamInfo],
) -> Option<Vec<Value>> {
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

/// Materialize all seeds that can safely execute under `selected_plan`.
///
/// The explorer/orchestrator currently install a single default execute plan
/// for planner-seeded execution. Seeds from other plans are safe to preserve
/// only when they share the same receiver construction shape and constructor
/// argument plans; otherwise the default plan could execute a method seed with
/// the wrong receiver setup.
#[must_use]
pub fn materialize_seeds_compatible_with_plan(
    plans: &[InvocationPlan],
    selected_plan: &InvocationPlan,
    param_infos: &[ParamInfo],
) -> Vec<Vec<Value>> {
    plans
        .iter()
        .filter(|plan| plan_compatible_with_default_execute_plan(plan, selected_plan))
        .filter_map(|plan| materialize_seed_for_plan(plan, param_infos))
        .collect()
}

fn plan_compatible_with_default_execute_plan(
    plan: &InvocationPlan,
    selected_plan: &InvocationPlan,
) -> bool {
    plan.target_id == selected_plan.target_id
        && plan.receiver_kind == selected_plan.receiver_kind
        && plan.generic_type_args == selected_plan.generic_type_args
        && plan.constructor_arg_plans == selected_plan.constructor_arg_plans
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
/// method arguments. If the caller already provided that prefixed shape, stale
/// prefixes are stripped and rematerialized so path scratch is fresh for this
/// execution.
pub fn execute_inputs_for_plan(
    inputs: &[Value],
    param_infos: &[ParamInfo],
    plan: Option<&InvocationPlan>,
) -> Result<PlannedExecuteInputs, PlannerConsumerError> {
    execute_inputs_for_plan_with_pins(inputs, param_infos, plan, None)
}

/// Like [`execute_inputs_for_plan`], but re-emits native-replay markers for
/// custom-generator/extractor parameter slots (str-6cdp).
///
/// `native_pins` is method-parameter indexed. After type repair, each pinned
/// slot is overwritten with its captured native-replay marker so the extractor
/// param (axum `State<AppState>`, `FromRequestParts`) carries its native value on
/// EVERY Execute — regardless of how the method vector was produced (fresh
/// random generation, mutation, crossover, seeding, or prefetch exhaustion).
/// This is the single funnel for all execute paths, so pinning here guarantees
/// 100% coverage in one place.
pub fn execute_inputs_for_plan_with_pins(
    inputs: &[Value],
    param_infos: &[ParamInfo],
    plan: Option<&InvocationPlan>,
    native_pins: Option<&crate::input_gen::NativePins>,
) -> Result<PlannedExecuteInputs, PlannerConsumerError> {
    let method_param_count = param_infos.len();
    // Repair a parameter-aligned input slice against its declared types so eroded
    // struct inputs (missing required fields, malformed uuids) still deserialize
    // and the function actually executes (str-kn3f). This is the single funnel
    // point for ALL execute paths — both the orchestrator's observe loop and the
    // explorer's strategy loops route through here. Purely additive on objects.
    // After repair, re-apply native-replay markers for custom-generator slots
    // (str-6cdp) so the extractor param is never a mutated/generated scalar.
    let repair = |method: &[Value]| -> Vec<Value> {
        let mut repaired: Vec<Value> = method
            .iter()
            .enumerate()
            .map(|(i, value)| match param_infos.get(i) {
                Some(param) => crate::input_gen::repair_required_fields(value, &param.typ),
                None => value.clone(),
            })
            .collect();
        if let Some(pins) = native_pins {
            pins.apply(&mut repaired);
        }
        repaired
    };

    let constructor_arg_count = plan.map_or(0, |p| p.constructor_arg_plans.len());
    if plan.is_none() || constructor_arg_count == 0 {
        // No constructor prefix: every input is a method argument.
        return Ok(PlannedExecuteInputs {
            inputs: repair(inputs),
            _scratch: Vec::new(),
        });
    }
    let plan = plan.expect("checked plan.is_some above");
    let method_inputs = if inputs.len() == method_param_count {
        inputs
    } else if inputs.len() == method_param_count + constructor_arg_count {
        &inputs[constructor_arg_count..]
    } else {
        // Arity mismatch — best-effort positional repair, no prefix surgery.
        return Ok(PlannedExecuteInputs {
            inputs: repair(inputs),
            _scratch: Vec::new(),
        });
    };
    let repaired_method = repair(method_inputs);
    let (mut prefixed, scratch) = materialize_execute_constructor_arg_values(plan)?;
    prefixed.extend_from_slice(&repaired_method);
    Ok(PlannedExecuteInputs {
        inputs: prefixed,
        _scratch: scratch,
    })
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
        // str-r2q7: RuntimeValue params (e.g. context.Context) are baked into the Go
        // launcher wrapper as direct assignments at compile time (str-gxjs.1). The
        // corresponding JSON input slot is ignored by the wrapper, so null is safe. We
        // emit null rather than dropping the entire plan so that sibling Literal/Zero
        // plans (e.g. an upstream URL hint on the same function) survive materialization.
        ValuePlanKind::RuntimeValue => Some(Value::Null),
        ValuePlanKind::Random | ValuePlanKind::Symbolic => None,
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
) -> Result<(Vec<Value>, Vec<ConstructorPathScratch>), PlannerConsumerError> {
    let mut values = Vec::with_capacity(plan.constructor_arg_plans.len());
    let mut scratch = Vec::new();
    for value_plan in &plan.constructor_arg_plans {
        let (value, mut owned) = materialize_execute_constructor_value(value_plan)?;
        values.push(value);
        scratch.append(&mut owned);
    }
    Ok((values, scratch))
}

fn materialize_execute_constructor_value(
    value_plan: &ValuePlan,
) -> Result<(Value, Vec<ConstructorPathScratch>), PlannerConsumerError> {
    match value_plan.kind {
        ValuePlanKind::Literal => value_plan
            .literal
            .clone()
            .map(|value| (value, Vec::new()))
            .ok_or_else(|| {
                PlannerConsumerError::ConstructorPathSeed(format!(
                    "constructor parameter {:?} literal is missing",
                    value_plan.param_name
                ))
            }),
        ValuePlanKind::Zero => {
            if let Some(kind) = constructor_path_seed_kind(value_plan) {
                let (path, scratch) = create_constructor_path_seed(kind)?;
                return Ok((Value::from(path), vec![scratch]));
            }
            Ok((zero_value_for_type_hint(&value_plan.type_hint), Vec::new()))
        }
        ValuePlanKind::Random | ValuePlanKind::Symbolic | ValuePlanKind::RuntimeValue => {
            Err(PlannerConsumerError::ConstructorPathSeed(format!(
                "constructor parameter {:?} is not directly materializable",
                value_plan.param_name
            )))
        }
    }
}

/// Produce a JSON zero value for a given `TypeInfo`. Conservative: primitive
/// types get their canonical zero; aggregates and complex types become
/// `null`, letting the downstream input generator refine them.
fn zero_value(typ: &TypeInfo) -> Value {
    match typ {
        TypeInfo::Int { .. } => Value::from(0),
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
) -> Result<(String, ConstructorPathScratch), PlannerConsumerError> {
    let (path, scratch) = match kind {
        ConstructorPathSeedKind::File => {
            let file = tempfile::Builder::new()
                .prefix(CONSTRUCTOR_FILE_SEED_PREFIX)
                .tempfile()
                .map_err(|err| {
                    PlannerConsumerError::ConstructorPathSeed(format!(
                        "failed to create constructor file seed: {err}"
                    ))
                })?;
            let path = file.path().to_path_buf();
            (path, ConstructorPathScratch::File { _file: file })
        }
        ConstructorPathSeedKind::Dir => {
            let dir = tempfile::Builder::new()
                .prefix(CONSTRUCTOR_DIR_SEED_PREFIX)
                .tempdir()
                .map_err(|err| {
                    PlannerConsumerError::ConstructorPathSeed(format!(
                        "failed to create constructor directory seed: {err}"
                    ))
                })?;
            let path = dir.path().to_path_buf();
            (path, ConstructorPathScratch::Dir { _dir: dir })
        }
    };
    Ok((path.to_string_lossy().into_owned(), scratch))
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
            typ: TypeInfo::Int { int_width: None, int_signed: None },
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

    /// str-6cdp: the execute funnel must re-emit the native-replay marker for a
    /// custom-generator/extractor slot even when the incoming method vector
    /// carries a NON-native scalar there (the actual failure mode: fresh random
    /// generation / prefetch exhaustion produced e.g. `1`/`null`/`"test"`).
    #[test]
    fn funnel_reapplies_native_marker_for_pinned_slot() {
        use crate::input_gen::{NativePins, ValueSource};

        let params = vec![
            // Slot 0: extractor param (custom generator / native replay).
            ParamInfo {
                name: "state".into(),
                typ: TypeInfo::Opaque {
                    label: "AppState".into(),
                    static_opacity: None,
                    medium_opacity: None,
                },
                type_name: Some("AppState".into()),
            },
            // Slot 1: ordinary built-in int param.
            int_param("id"),
        ];
        let sources = vec![
            ValueSource::CustomGenerator {
                generator_name: "AppState".into(),
                param_name: None,
                generator_file: "/gen/state.rs".into(),
                kind: crate::protocol::GeneratorKind::TypeName,
            },
            ValueSource::BuiltIn,
        ];
        let marker = json!({
            "__shatter_native": true,
            "__shatter_replay": {"file": "state.rs", "name": "AppState", "recipe": {}}
        });
        // Pins captured from a candidate vector that DID carry the marker.
        let pins = NativePins::capture_from_inputs(&sources, &[vec![marker.clone(), json!(7)]]);
        assert!(!pins.is_empty(), "pins should capture the marker from candidates");

        // The vector reaching execute has a NON-native scalar in the pinned slot.
        let eroded = vec![json!(1), json!(42)];
        let out = execute_inputs_for_plan_with_pins(&eroded, &params, None, Some(&pins))
            .expect("funnel succeeds")
            .inputs()
            .to_vec();

        assert_eq!(out[0], marker, "pinned slot must be restored to the native marker");
        assert_eq!(out[1], json!(42), "built-in slot must pass through unchanged");

        // Without pins, the funnel leaves the eroded non-native scalar in place
        // (proves the pin is what restores the marker, not repair).
        let unpinned = execute_inputs_for_plan_with_pins(&eroded, &params, None, None)
            .expect("funnel succeeds")
            .inputs()
            .to_vec();
        assert_eq!(unpinned[0], json!(1), "without pins the slot stays non-native");
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

    fn runtime_plan(param_index: u32, param_name: &str, type_hint: &str) -> ValuePlan {
        ValuePlan {
            param_index,
            param_name: param_name.to_string(),
            kind: ValuePlanKind::RuntimeValue,
            literal: Some(json!("runtime")),
            type_hint: type_hint.to_string(),
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
    fn materialize_compatible_seeds_preserves_matching_constructor_shape_only() {
        let selected = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:NewThing".into(),
            generic_type_args: vec![],
            argument_plans: vec![literal_plan(0, "event", json!("first"))],
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
        let compatible = InvocationPlan {
            argument_plans: vec![literal_plan(0, "event", json!("second"))],
            priority: 1,
            ..selected.clone()
        };
        let incompatible = InvocationPlan {
            receiver_kind: "constructor:OtherThing".into(),
            argument_plans: vec![literal_plan(0, "event", json!("wrong"))],
            priority: 2,
            ..selected.clone()
        };

        let seeds = materialize_seeds_compatible_with_plan(
            &[selected.clone(), compatible, incompatible],
            &selected,
            &[str_param("event")],
        );

        assert_eq!(seeds, vec![vec![json!("first")], vec![json!("second")]]);
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
        let planned = execute_inputs_for_plan(&[json!("default")], &[ParamInfo { name: String::new(), typ: TypeInfo::Str, type_name: None }], Some(&plan))
            .expect("constructor directory seed should materialize");
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
        let refreshed = execute_inputs_for_plan(planned.inputs(), &[ParamInfo { name: String::new(), typ: TypeInfo::Str, type_name: None }], Some(&plan))
            .expect("stale constructor prefix should rematerialize");
        assert_eq!(refreshed.inputs().len(), 2);
        assert_eq!(refreshed.inputs().get(1), Some(&json!("default")));
        let refreshed_dir = std::path::PathBuf::from(
            refreshed
                .inputs()
                .first()
                .and_then(Value::as_str)
                .expect("expected refreshed constructor seed"),
        );
        assert_ne!(
            dir, refreshed_dir,
            "stale constructor prefixes should be replaced with fresh scratch",
        );
        assert!(
            refreshed_dir.is_dir(),
            "refreshed constructor prefix should be a usable directory",
        );
        drop(refreshed);
        assert!(
            !refreshed_dir.exists(),
            "refreshed directory scratch should be removed when planned inputs drop",
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
        let first =
            execute_inputs_for_plan(&[], &[], Some(&plan)).expect("first file seed materializes");
        let second =
            execute_inputs_for_plan(&[], &[], Some(&plan)).expect("second file seed materializes");
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
        let planned = execute_inputs_for_plan(&[], &[], Some(&plan))
            .expect("constructor directory seed should materialize");
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
    fn runtime_value_plan_materializes_as_null() {
        // str-r2q7: RuntimeValue params (e.g. context.Context) used to return
        // None and drop the entire seed. Now they materialize as null — the Go
        // wrapper bakes context.Background() at wrapper-compile time (str-gxjs.1)
        // and ignores the JSON slot, so null is safe.
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
        assert_eq!(
            seeds,
            vec![vec![Value::Null]],
            "runtime_value param should materialize as null so the seed survives",
        );
    }

    #[test]
    fn runtime_value_sibling_literal_survives() {
        // str-r2q7: when a function has a RuntimeValue param (e.g. ctx) alongside
        // a Literal param (e.g. upstream URL hint), the entire seed must survive.
        // Previously, RuntimeValue => None caused the ? in materialize_seed_for_plan
        // to drop the whole seed, silently discarding the sibling Literal hint.
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            generic_type_args: vec![],
            argument_plans: vec![
                ValuePlan {
                    param_index: 0,
                    param_name: "ctx".into(),
                    kind: ValuePlanKind::RuntimeValue,
                    literal: Some(json!("context.Background()")),
                    type_hint: "context.Context".into(),
                },
                literal_plan(1, "upstream", json!("http://localhost:11434")),
            ],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };
        let seeds =
            materialize_seeds(&[plan], &[int_param("ctx"), str_param("upstream")]);
        assert_eq!(
            seeds,
            vec![vec![Value::Null, json!("http://localhost:11434")]],
            "sibling Literal hint must survive when ctx is RuntimeValue",
        );
    }

    #[test]
    fn runtime_value_placeholder_keeps_literal_method_seed() {
        let body = json!(
            r#"{"model":"claude-3-5-sonnet-20241022","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#
        );
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: "constructor:NewHandler".into(),
            generic_type_args: vec![],
            argument_plans: vec![
                runtime_plan(0, "w", "http.ResponseWriter"),
                literal_plan(1, "r", body.clone()),
            ],
            constructor_arg_plans: vec![],
            priority: 0,
            label: String::new(),
        };

        let seeds = materialize_seeds(&[plan], &[int_param("w"), str_param("r")]);

        assert_eq!(seeds, vec![vec![Value::Null, body]]);
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

    // ── Property tests ──────────────────────────────────────────────────────
    // Covers materialize_seeds invariants for all plan-kind combinations.

    #[cfg(test)]
    mod prop_tests {
        use super::*;
        use proptest::prelude::*;
        use serde_json::json;

        fn arb_value_plan(index: u32) -> impl Strategy<Value = ValuePlan> {
            prop_oneof![
                // Literal
                any::<i64>().prop_map(move |n| ValuePlan {
                    param_index: index,
                    param_name: format!("p{index}"),
                    kind: ValuePlanKind::Literal,
                    literal: Some(json!(n)),
                    type_hint: String::new(),
                }),
                // Zero
                Just(ValuePlan {
                    param_index: index,
                    param_name: format!("p{index}"),
                    kind: ValuePlanKind::Zero,
                    literal: None,
                    type_hint: String::new(),
                }),
                // RuntimeValue (str-r2q7: must materialize as null)
                Just(ValuePlan {
                    param_index: index,
                    param_name: format!("p{index}"),
                    kind: ValuePlanKind::RuntimeValue,
                    literal: Some(json!("context.Background()")),
                    type_hint: "context.Context".into(),
                }),
                // Random/Symbolic (must drop the entire plan)
                Just(ValuePlan {
                    param_index: index,
                    param_name: format!("p{index}"),
                    kind: ValuePlanKind::Random,
                    literal: None,
                    type_hint: String::new(),
                }),
            ]
        }

        proptest! {
            /// Every produced seed has exactly as many slots as param_infos.
            #[test]
            fn seed_length_equals_param_count(
                plans in prop::collection::vec(
                    (0u32..4).prop_flat_map(|n| {
                        let slots: u32 = n + 1;
                        prop::collection::vec(arb_value_plan(0), slots as usize)
                            .prop_map(move |args| InvocationPlan {
                                target_id: "t".into(),
                                receiver_kind: String::new(),
                                generic_type_args: vec![],
                                argument_plans: args,
                                constructor_arg_plans: vec![],
                                priority: 0,
                                label: String::new(),
                            })
                    }),
                    0..5,
                )
            ) {
                for plan in &plans {
                    let arity = plan.argument_plans.len();
                    let params: Vec<_> = (0..arity)
                        .map(|i| int_param(&format!("p{i}")))
                        .collect();
                    let seeds = materialize_seeds(std::slice::from_ref(plan), &params);
                    for seed in &seeds {
                        prop_assert_eq!(seed.len(), arity,
                            "seed length must equal param count");
                    }
                }
            }

            /// A plan with any Random entry never produces a seed.
            #[test]
            fn random_plan_never_materializes(extra_literal in any::<i64>()) {
                let plan = InvocationPlan {
                    target_id: "t".into(),
                    receiver_kind: String::new(),
                    generic_type_args: vec![],
                    argument_plans: vec![
                        ValuePlan {
                            param_index: 0,
                            param_name: "ctx".into(),
                            kind: ValuePlanKind::RuntimeValue,
                            literal: Some(json!("context.Background()")),
                            type_hint: "context.Context".into(),
                        },
                        ValuePlan {
                            param_index: 1,
                            param_name: "rnd".into(),
                            kind: ValuePlanKind::Random,
                            literal: None,
                            type_hint: String::new(),
                        },
                        ValuePlan {
                            param_index: 2,
                            param_name: "lit".into(),
                            kind: ValuePlanKind::Literal,
                            literal: Some(json!(extra_literal)),
                            type_hint: String::new(),
                        },
                    ],
                    constructor_arg_plans: vec![],
                    priority: 0,
                    label: String::new(),
                };
                let seeds = materialize_seeds(
                    &[plan],
                    &[int_param("ctx"), int_param("rnd"), int_param("lit")],
                );
                prop_assert!(seeds.is_empty(),
                    "plan with Random must not produce a seed");
            }

            /// RuntimeValue slots always materialize as null (never drop the plan).
            #[test]
            fn runtime_value_slot_is_always_null(literal_val in any::<i64>()) {
                let plan = InvocationPlan {
                    target_id: "t".into(),
                    receiver_kind: String::new(),
                    generic_type_args: vec![],
                    argument_plans: vec![
                        ValuePlan {
                            param_index: 0,
                            param_name: "ctx".into(),
                            kind: ValuePlanKind::RuntimeValue,
                            literal: Some(json!("context.Background()")),
                            type_hint: "context.Context".into(),
                        },
                        ValuePlan {
                            param_index: 1,
                            param_name: "v".into(),
                            kind: ValuePlanKind::Literal,
                            literal: Some(json!(literal_val)),
                            type_hint: String::new(),
                        },
                    ],
                    constructor_arg_plans: vec![],
                    priority: 0,
                    label: String::new(),
                };
                let seeds = materialize_seeds(
                    &[plan],
                    &[int_param("ctx"), int_param("v")],
                );
                prop_assert_eq!(seeds.len(), 1, "plan with RuntimeValue + Literal must produce one seed");
                prop_assert_eq!(&seeds[0][0], &Value::Null, "RuntimeValue slot must be null");
                prop_assert_eq!(&seeds[0][1], &json!(literal_val), "Literal slot must keep its value");
            }
        }
    }
}
