//! Value-level mutation strategies.
//!
//! # Tier: Value
//!
//! Value strategies operate on a **single `Value`** given its [`TypeInfo`],
//! producing a mutated variant. They are the atomic building blocks of the
//! strategy system — composable units that can be:
//!
//! - Used directly inside vector-level strategies like [`FuzzerStrategy`]
//! - Lifted to vector-level via [`ValueToVectorAdapter`]
//! - Combined (e.g., a future GA strategy could swap value-level mutators)
//!
//! This complements the vector-level [`InputStrategy`] trait in [`strategy`],
//! which operates on entire `Vec<Value>` input vectors.
//!
//! [`FuzzerStrategy`]: crate::strategy::FuzzerStrategy
//! [`InputStrategy`]: crate::strategy::InputStrategy
//! [`strategy`]: crate::strategy

use std::collections::VecDeque;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::Value;

use crate::input_gen::mutate_value;
use crate::protocol::ExecuteResult;
use crate::strategy::{InputStrategy, StrategyContext};
use crate::types::{ParamInfo, TypeInfo};

// ---------------------------------------------------------------------------
// ValueStrategy trait
// ---------------------------------------------------------------------------

/// A composable value-level mutation strategy.
///
/// # Tier: Value
///
/// Operates on a single [`Value`] given its [`TypeInfo`], producing a mutated
/// variant. Value strategies are the atomic building blocks of the input
/// generation system. They can be lifted to vector-level via
/// [`ValueToVectorAdapter`] or composed inside vector-level strategies.
///
/// Unlike [`InputStrategy`] (vector-level), value strategies do not manage
/// feedback, exhaustion, or scoring — those concerns belong to the vector
/// tier and the [`MetaStrategy`].
///
/// [`InputStrategy`]: crate::strategy::InputStrategy
/// [`MetaStrategy`]: crate::strategy::MetaStrategy
pub trait ValueStrategy: Send {
    /// Mutate a single value according to its type.
    ///
    /// Implementations should preserve the type contract: the returned value
    /// must be valid for the given `type_info`. For example, mutating an `Int`
    /// must return a JSON number, not a string.
    fn mutate(&mut self, value: &Value, type_info: &TypeInfo, rng: &mut StdRng) -> Value;

    /// Human-readable name for this value strategy.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// TypeAwareMutator — wraps input_gen::mutate_value()
// ---------------------------------------------------------------------------

/// Default value-level mutator that delegates to [`mutate_value`].
///
/// # Tier: Value
///
/// This wraps the existing type-aware mutation logic (int bit-flips, string
/// char mutations, array element mutations, etc.) into the [`ValueStrategy`]
/// trait. It is the canonical value-level mutator and the one used by
/// [`FuzzerStrategy`] internally.
///
/// [`mutate_value`]: crate::input_gen::mutate_value
/// [`FuzzerStrategy`]: crate::strategy::FuzzerStrategy
pub struct TypeAwareMutator {
    /// Dictionary of string fragments for string mutation (e.g., domain-specific tokens).
    dictionary: Vec<String>,
}

impl TypeAwareMutator {
    /// Create a new type-aware mutator with an optional string dictionary.
    pub fn new(dictionary: Vec<String>) -> Self {
        Self { dictionary }
    }
}

impl ValueStrategy for TypeAwareMutator {
    fn mutate(&mut self, value: &Value, type_info: &TypeInfo, rng: &mut StdRng) -> Value {
        let dict_refs: Vec<&str> = self.dictionary.iter().map(|s| s.as_str()).collect();
        mutate_value(value, type_info, &dict_refs, rng)
    }

    fn name(&self) -> &str {
        "type_aware_mutator"
    }
}

// ---------------------------------------------------------------------------
// ValueToVectorAdapter — lifts ValueStrategy into InputStrategy
// ---------------------------------------------------------------------------

/// Adapter that lifts a [`ValueStrategy`] into a vector-level [`InputStrategy`].
///
/// Applies the value strategy to each parameter position independently, with
/// a configurable per-parameter mutation rate. Requires a pool of base inputs
/// (fed via [`feedback`]) to mutate from.
///
/// # Tier: Vector (wrapping a Value strategy)
///
/// This is the bridge between the two tiers: it takes an atomic value-level
/// mutator and applies it across the full parameter vector to produce
/// complete input candidates for the exploration loop.
///
/// [`feedback`]: InputStrategy::feedback
pub struct ValueToVectorAdapter {
    value_strategy: Box<dyn ValueStrategy>,
    mutation_rate: f64,
    /// Interesting inputs collected via feedback, used as mutation bases.
    base_inputs: Vec<Vec<Value>>,
    /// Pre-generated mutations waiting to be yielded.
    pending: VecDeque<Vec<Value>>,
    rng: StdRng,
}

impl ValueToVectorAdapter {
    /// Create a new adapter.
    ///
    /// - `value_strategy`: the value-level mutator to apply per-parameter.
    /// - `mutation_rate`: probability (0.0–1.0) of mutating each parameter position.
    /// - `seed`: RNG seed for reproducibility (`None` for system entropy).
    pub fn new(
        value_strategy: Box<dyn ValueStrategy>,
        mutation_rate: f64,
        seed: Option<u64>,
    ) -> Self {
        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_os_rng(),
        };
        Self {
            value_strategy,
            mutation_rate,
            base_inputs: Vec::new(),
            pending: VecDeque::new(),
            rng,
        }
    }

    /// Generate mutations from one base input using the value strategy.
    fn generate_mutations(&mut self, base: &[Value], params: &[ParamInfo]) {
        let mutated: Vec<Value> = base
            .iter()
            .zip(params.iter())
            .map(|(val, param)| {
                if self.rng.random_range(0.0..1.0) < self.mutation_rate {
                    self.value_strategy.mutate(val, &param.typ, &mut self.rng)
                } else {
                    val.clone()
                }
            })
            .collect();
        self.pending.push_back(mutated);
    }
}

impl InputStrategy for ValueToVectorAdapter {
    fn next(&mut self, ctx: &StrategyContext) -> Option<Vec<Value>> {
        if let Some(candidate) = self.pending.pop_front() {
            return Some(candidate);
        }

        // Try to generate from base inputs.
        if self.base_inputs.is_empty() {
            return None;
        }

        let idx = self.rng.random_range(0..self.base_inputs.len());
        let base = self.base_inputs[idx].clone();
        self.generate_mutations(&base, &ctx.params);
        self.pending.pop_front()
    }

    fn feedback(&mut self, inputs: &[Value], _result: &ExecuteResult, was_new_path: bool) {
        if was_new_path {
            self.base_inputs.push(inputs.to_vec());
        }
    }

    fn name(&self) -> &str {
        self.value_strategy.name()
    }

    fn is_finite(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::FrontendCapabilities;
    use crate::types::ParamInfo;

    fn empty_ctx() -> StrategyContext {
        StrategyContext {
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        }
    }

    fn multi_param_ctx() -> StrategyContext {
        StrategyContext {
            params: vec![
                ParamInfo {
                    name: "n".into(),
                    typ: TypeInfo::Int { int_width: None, int_signed: None },
                    type_name: None,
                },
                ParamInfo {
                    name: "s".into(),
                    typ: TypeInfo::Str,
                    type_name: None,
                },
                ParamInfo {
                    name: "b".into(),
                    typ: TypeInfo::Bool,
                    type_name: None,
                },
            ],
            literals: vec![],
            capabilities: FrontendCapabilities::from_raw(&[]),
        }
    }

    fn make_exec_result() -> ExecuteResult {
        serde_json::from_str(
            r#"{"return_value": 0, "branch_path": [], "lines_executed": [], "path_constraints": [], "performance": {"wall_time_ms": 1.0, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0}}"#,
        )
        .expect("valid ExecuteResult JSON")
    }

    // --- TypeAwareMutator tests ---

    #[test]
    fn type_aware_mutator_name() {
        let m = TypeAwareMutator::new(vec![]);
        assert_eq!(m.name(), "type_aware_mutator");
    }

    #[test]
    fn type_aware_mutator_int_returns_number() {
        let mut m = TypeAwareMutator::new(vec![]);
        let mut rng = StdRng::seed_from_u64(42);
        let result = m.mutate(&Value::from(5), &TypeInfo::Int { int_width: None, int_signed: None }, &mut rng);
        assert!(
            result.is_number(),
            "Int mutation should produce a number, got {result:?}"
        );
    }

    #[test]
    fn type_aware_mutator_string_returns_string() {
        let mut m = TypeAwareMutator::new(vec!["test".into()]);
        let mut rng = StdRng::seed_from_u64(42);
        let result = m.mutate(&Value::from("hello"), &TypeInfo::Str, &mut rng);
        assert!(
            result.is_string(),
            "Str mutation should produce a string, got {result:?}"
        );
    }

    #[test]
    fn type_aware_mutator_bool_returns_bool() {
        let mut m = TypeAwareMutator::new(vec![]);
        let mut rng = StdRng::seed_from_u64(42);
        let result = m.mutate(&Value::from(true), &TypeInfo::Bool, &mut rng);
        assert!(
            result.is_boolean(),
            "Bool mutation should produce a boolean, got {result:?}"
        );
    }

    #[test]
    fn type_aware_mutator_unknown_returns_unchanged() {
        let mut m = TypeAwareMutator::new(vec![]);
        let mut rng = StdRng::seed_from_u64(42);
        let original = Value::from("opaque");
        let result = m.mutate(&original, &TypeInfo::Unknown, &mut rng);
        assert_eq!(result, original);
    }

    // --- ValueToVectorAdapter tests ---

    #[test]
    fn adapter_returns_none_without_feedback() {
        let mutator = TypeAwareMutator::new(vec![]);
        let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 0.3, Some(42));
        let ctx = empty_ctx();
        assert!(adapter.next(&ctx).is_none());
    }

    #[test]
    fn adapter_produces_after_feedback() {
        let mutator = TypeAwareMutator::new(vec![]);
        let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 1.0, Some(42));
        let ctx = empty_ctx();
        let result = make_exec_result();

        adapter.feedback(&[Value::from(5)], &result, true);
        let output = adapter.next(&ctx);
        assert!(output.is_some(), "should produce after new-path feedback");
    }

    #[test]
    fn adapter_preserves_vector_length() {
        let mutator = TypeAwareMutator::new(vec![]);
        let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 1.0, Some(42));
        let ctx = multi_param_ctx();
        let result = make_exec_result();

        adapter.feedback(
            &[Value::from(5), Value::from("hi"), Value::from(true)],
            &result,
            true,
        );

        for _ in 0..10 {
            if let Some(output) = adapter.next(&ctx) {
                assert_eq!(output.len(), 3, "output must preserve input vector length");
            }
        }
    }

    #[test]
    fn adapter_ignores_non_new_path_feedback() {
        let mutator = TypeAwareMutator::new(vec![]);
        let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 1.0, Some(42));
        let ctx = empty_ctx();
        let result = make_exec_result();

        adapter.feedback(&[Value::from(5)], &result, false);
        assert!(
            adapter.next(&ctx).is_none(),
            "non-new-path feedback should not seed inputs"
        );
    }

    #[test]
    fn adapter_is_infinite() {
        let mutator = TypeAwareMutator::new(vec![]);
        let adapter = ValueToVectorAdapter::new(Box::new(mutator), 0.3, Some(42));
        assert!(!adapter.is_finite());
    }

    #[test]
    fn adapter_name_delegates_to_value_strategy() {
        let mutator = TypeAwareMutator::new(vec![]);
        let adapter = ValueToVectorAdapter::new(Box::new(mutator), 0.3, Some(42));
        assert_eq!(adapter.name(), "type_aware_mutator");
    }

    #[test]
    fn adapter_zero_mutation_rate_preserves_inputs() {
        let mutator = TypeAwareMutator::new(vec![]);
        let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 0.0, Some(42));
        let ctx = multi_param_ctx();
        let result = make_exec_result();
        let original = vec![Value::from(42), Value::from("hello"), Value::from(false)];

        adapter.feedback(&original, &result, true);
        let output = adapter.next(&ctx).expect("should produce output");
        assert_eq!(
            output, original,
            "0% mutation rate should preserve all values"
        );
    }

    // --- Proptest ---

    mod proptests {
        use super::*;
        use crate::test_arbitraries::{arb_json_value, arb_type_info};
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            /// TypeAwareMutator must never panic on arbitrary Value + TypeInfo.
            #[test]
            fn type_aware_mutator_never_panics(
                val in arb_json_value(),
                ti in arb_type_info(3),
            ) {
                let mut m = TypeAwareMutator::new(vec![]);
                let mut rng = StdRng::seed_from_u64(42);
                // Must not panic.
                let _ = m.mutate(&val, &ti, &mut rng);
            }

            /// ValueToVectorAdapter output length always matches param count.
            #[test]
            fn adapter_output_length_matches_params(
                param_count in 1..6usize,
                seed in any::<u64>(),
            ) {
                let params: Vec<ParamInfo> = (0..param_count)
                    .map(|i| ParamInfo {
                        name: format!("p{i}"),
                        typ: TypeInfo::Int { int_width: None, int_signed: None },
                        type_name: None,
                    })
                    .collect();
                let inputs: Vec<Value> = (0..param_count)
                    .map(|i| Value::from(i as i64))
                    .collect();
                let ctx = StrategyContext {
                    params: params.clone(),
                    literals: vec![],
                    capabilities: FrontendCapabilities::from_raw(&[]),
                };
                let result = serde_json::from_str::<ExecuteResult>(
                    r#"{"return_value": 0, "branch_path": [], "lines_executed": [], "path_constraints": [], "performance": {"wall_time_ms": 1.0, "cpu_time_us": 0, "heap_used_bytes": 0, "heap_allocated_bytes": 0}}"#,
                ).expect("valid");

                let mutator = TypeAwareMutator::new(vec![]);
                let mut adapter = ValueToVectorAdapter::new(Box::new(mutator), 0.5, Some(seed));
                adapter.feedback(&inputs, &result, true);

                for _ in 0..5 {
                    if let Some(output) = adapter.next(&ctx) {
                        prop_assert_eq!(output.len(), param_count);
                    }
                }
            }
        }
    }
}
