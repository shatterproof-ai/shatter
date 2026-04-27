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
//! `Random` and `Symbolic` plan entries yield no seed for the current target
//! and fall through to the normal random / concolic input generation. Callers
//! that need to surface planner ordering should pass plans in priority order;
//! seeds are produced in the order the frontend returned them.

use serde_json::Value;

use crate::frontend::{Frontend, FrontendError};
use crate::protocol::{
    Command as ProtoCommand, InvocationPlan, InvocationRequirement, ResponseResult,
    UnsatisfiedRequirement, ValuePlan, ValuePlanKind,
};
use crate::types::{ParamInfo, TypeInfo};

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
        ResponseResult::Error { code, message, .. } => Err(PlannerConsumerError::UnexpectedResponse(
            format!("error code={code:?} message={message}"),
        )),
        other => Err(PlannerConsumerError::UnexpectedResponse(format!(
            "expected InvocationPlan, got {other:?}"
        ))),
    }
}

/// Map planner outputs to ready-to-execute argument vectors.
///
/// One seed is produced per `InvocationPlan` whose every `ValuePlan` is
/// directly materializable (`Literal` or `Zero`). Plans containing `Random`
/// or `Symbolic` entries are skipped — those strategies are already covered
/// by the explorer's random generator and the orchestrator's Z3 path.
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
    for (value_plan, param) in plan.argument_plans.iter().zip(param_infos.iter()) {
        let v = materialize_value(value_plan, param)?;
        values.push(v);
    }
    Some(values)
}

fn materialize_value(value_plan: &ValuePlan, param: &ParamInfo) -> Option<Value> {
    match value_plan.kind {
        ValuePlanKind::Literal => value_plan.literal.clone(),
        ValuePlanKind::Zero => Some(zero_value(&param.typ)),
        ValuePlanKind::Random | ValuePlanKind::Symbolic => None,
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
            argument_plans: vec![literal_plan(0, "a", json!(7)), zero_plan(1, "b")],
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
            argument_plans: vec![zero_plan(0, "s")],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[str_param("s")]);
        assert_eq!(seeds, vec![vec![json!("")]]);
    }

    #[test]
    fn symbolic_plan_is_skipped() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            argument_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "a".into(),
                kind: ValuePlanKind::Symbolic,
                literal: None,
                type_hint: String::new(),
            }],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[plan], &[int_param("a")]);
        assert!(seeds.is_empty(), "symbolic plan should not materialize");
    }

    #[test]
    fn mismatched_arity_skips_plan() {
        let plan = InvocationPlan {
            target_id: "t".into(),
            receiver_kind: String::new(),
            argument_plans: vec![literal_plan(0, "a", json!(7))],
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
            argument_plans: vec![literal_plan(0, "a", json!(v))],
            priority: 0,
            label: String::new(),
        };
        let seeds = materialize_seeds(&[mk(1), mk(2), mk(3)], &[int_param("a")]);
        assert_eq!(
            seeds,
            vec![vec![json!(1)], vec![json!(2)], vec![json!(3)]]
        );
    }
}
