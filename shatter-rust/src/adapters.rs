//! Adapter registry, recognizers, and invocation strategy for Rust targets.
//!
//! Mirrors the TS adapter substrate (shatter-ts/src/runtime-hooks.ts) but
//! tailored for Rust-specific patterns: async functions (Tokio runtime) and
//! framework handlers (Axum, etc.).
//!
//! Ordinary synchronous Rust exports use the existing direct-call path.

use crate::executor::ExecuteResult;
use crate::protocol::{
    AdapterHint, Confidence, ExecutionAdapter, ExecutionAdapterApply, FunctionAnalysis,
    InvocationModel,
};

// ---------------------------------------------------------------------------
// Adapter ID constants
// ---------------------------------------------------------------------------

/// Adapter for async functions that require a Tokio runtime to `.await`.
pub const ADAPTER_ID_ASYNC_TOKIO: &str = "rust/async-tokio";

/// Placeholder adapter for Axum framework handlers.
pub const ADAPTER_ID_AXUM_HANDLER: &str = "rust/framework/axum-handler";

/// Adapter IDs that this frontend can execute via the adapter-owned path.
const SUPPORTED_ADAPTERS: &[&str] = &[ADAPTER_ID_ASYNC_TOKIO];

// ---------------------------------------------------------------------------
// Recognizer trait and implementations
// ---------------------------------------------------------------------------

/// A recognizer inspects a function analysis and returns an adapter hint
/// if it detects a pattern requiring adapter-owned invocation.
pub trait AdapterRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis) -> Option<AdapterHint>;
}

/// Detects `async fn` signatures that require a Tokio runtime.
pub struct AsyncFunctionRecognizer;

impl AdapterRecognizer for AsyncFunctionRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis) -> Option<AdapterHint> {
        if !analysis.is_async {
            return None;
        }
        Some(AdapterHint {
            adapter: ExecutionAdapter {
                id: ADAPTER_ID_ASYNC_TOKIO.to_string(),
                apply: Some(ExecutionAdapterApply::Auto),
                options: None,
            },
            confidence: Confidence::High,
            reasons: vec!["function is async".to_string()],
            requirements: vec![],
            conflicts: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Registry of adapter recognizers. Run all recognizers against a function
/// analysis to collect adapter hints.
pub struct AdapterRegistry {
    recognizers: Vec<Box<dyn AdapterRecognizer>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            recognizers: vec![Box::new(AsyncFunctionRecognizer)],
        }
    }

    /// Run all recognizers against a function analysis, collecting hints.
    pub fn recognize_all(&self, analysis: &FunctionAnalysis) -> Vec<AdapterHint> {
        self.recognizers
            .iter()
            .filter_map(|r| r.recognize(analysis))
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Invocation strategy
// ---------------------------------------------------------------------------

/// The dispatch strategy for a target function.
pub enum InvocationStrategy {
    /// Call the function directly (existing harness path).
    Direct,
    /// Use the adapter-owned execution path.
    AdapterOwned { adapter_id: String },
    /// The adapter is not supported by this frontend.
    Unsupported { adapter_id: String },
}

/// Choose invocation strategy based on the function's invocation model.
pub fn choose_invocation_strategy(model: &InvocationModel) -> InvocationStrategy {
    match model {
        InvocationModel::Direct => InvocationStrategy::Direct,
        InvocationModel::Adapter { adapter_id, .. } => {
            if SUPPORTED_ADAPTERS.contains(&adapter_id.as_str()) {
                InvocationStrategy::AdapterOwned {
                    adapter_id: adapter_id.clone(),
                }
            } else {
                InvocationStrategy::Unsupported {
                    adapter_id: adapter_id.clone(),
                }
            }
        }
    }
}

/// Derive the invocation model from adapter hints.
///
/// Picks the highest-confidence hint whose apply policy is not `Disabled`.
/// Returns `Direct` if no qualifying hints exist.
pub fn derive_invocation_model(hints: &[AdapterHint]) -> InvocationModel {
    hints
        .iter()
        .filter(|h| h.adapter.apply.as_ref() != Some(&ExecutionAdapterApply::Disabled))
        .max_by_key(|h| h.confidence)
        .map(|h| InvocationModel::Adapter {
            adapter_id: h.adapter.id.clone(),
            synthetic_params: vec![],
            scenario_schema: None,
        })
        .unwrap_or(InvocationModel::Direct)
}

// ---------------------------------------------------------------------------
// Adapter-owned execution
// ---------------------------------------------------------------------------

/// Execute a function through the adapter-owned path.
///
/// Adapter-owned calls return empty coverage data (branch_path, lines_executed,
/// path_constraints, calls_to_external) — matching the TS adapter contract.
pub fn execute_adapter_owned(
    adapter_id: &str,
    file_path: &str,
    function_name: &str,
    inputs: &[serde_json::Value],
    mocks: &[serde_json::Value],
    timeout_ms: u64,
    harness_cache: &crate::executor::HarnessCache,
    crate_cache: &crate::executor::CrateHarnessCache,
    bridge_cache: &crate::executor::CrateBridgeHarnessCache,
) -> Result<ExecuteResult, crate::executor::ExecuteError> {
    match adapter_id {
        ADAPTER_ID_ASYNC_TOKIO => {
            // Use the standard executor with async_tokio harness mode.
            // The executor's harness generator wraps the call in a Tokio runtime.
            crate::executor::execute_function(
                file_path,
                function_name,
                inputs,
                mocks,
                timeout_ms,
                Some("async_tokio"),
                harness_cache,
                crate_cache,
                bridge_cache,
            )
        }
        _ => Err(crate::executor::ExecuteError::NonExecutable(format!(
            "adapter not supported: {adapter_id}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{InvocationModel, TypeInfo};

    fn stub_analysis(is_async: bool) -> FunctionAnalysis {
        FunctionAnalysis {
            name: "test_fn".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 1,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            is_async,
            adapter_hints: vec![],
            invocation_model: InvocationModel::default(),
        }
    }

    // ── Recognizer tests ──

    #[test]
    fn async_recognizer_detects_async_fn() {
        let analysis = stub_analysis(true);
        let recognizer = AsyncFunctionRecognizer;
        let hint = recognizer.recognize(&analysis);
        assert!(hint.is_some());
        let hint = hint.unwrap();
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_TOKIO);
        assert_eq!(hint.confidence, Confidence::High);
    }

    #[test]
    fn async_recognizer_ignores_sync_fn() {
        let analysis = stub_analysis(false);
        let recognizer = AsyncFunctionRecognizer;
        assert!(recognizer.recognize(&analysis).is_none());
    }

    // ── Registry tests ──

    #[test]
    fn registry_recognizes_async_fn() {
        let registry = AdapterRegistry::new();
        let analysis = stub_analysis(true);
        let hints = registry.recognize_all(&analysis);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].adapter.id, ADAPTER_ID_ASYNC_TOKIO);
    }

    #[test]
    fn registry_returns_empty_for_sync_fn() {
        let registry = AdapterRegistry::new();
        let analysis = stub_analysis(false);
        let hints = registry.recognize_all(&analysis);
        assert!(hints.is_empty());
    }

    // ── Strategy tests ──

    #[test]
    fn direct_model_yields_direct_strategy() {
        let model = InvocationModel::Direct;
        assert!(matches!(
            choose_invocation_strategy(&model),
            InvocationStrategy::Direct
        ));
    }

    #[test]
    fn supported_adapter_yields_adapter_owned() {
        let model = InvocationModel::Adapter {
            adapter_id: ADAPTER_ID_ASYNC_TOKIO.to_string(),
            synthetic_params: vec![],
            scenario_schema: None,
        };
        assert!(matches!(
            choose_invocation_strategy(&model),
            InvocationStrategy::AdapterOwned { .. }
        ));
    }

    #[test]
    fn unknown_adapter_yields_unsupported() {
        let model = InvocationModel::Adapter {
            adapter_id: "rust/unknown-adapter".to_string(),
            synthetic_params: vec![],
            scenario_schema: None,
        };
        assert!(matches!(
            choose_invocation_strategy(&model),
            InvocationStrategy::Unsupported { .. }
        ));
    }

    // ── derive_invocation_model tests ──

    #[test]
    fn derive_picks_highest_confidence() {
        let hints = vec![
            AdapterHint {
                adapter: ExecutionAdapter {
                    id: "low-adapter".into(),
                    apply: Some(ExecutionAdapterApply::Auto),
                    options: None,
                },
                confidence: Confidence::Low,
                reasons: vec![],
                requirements: vec![],
                conflicts: vec![],
            },
            AdapterHint {
                adapter: ExecutionAdapter {
                    id: ADAPTER_ID_ASYNC_TOKIO.into(),
                    apply: Some(ExecutionAdapterApply::Auto),
                    options: None,
                },
                confidence: Confidence::High,
                reasons: vec![],
                requirements: vec![],
                conflicts: vec![],
            },
        ];
        let model = derive_invocation_model(&hints);
        match model {
            InvocationModel::Adapter { adapter_id, .. } => {
                assert_eq!(adapter_id, ADAPTER_ID_ASYNC_TOKIO);
            }
            InvocationModel::Direct => panic!("expected Adapter"),
        }
    }

    #[test]
    fn derive_skips_disabled_hints() {
        let hints = vec![AdapterHint {
            adapter: ExecutionAdapter {
                id: ADAPTER_ID_ASYNC_TOKIO.into(),
                apply: Some(ExecutionAdapterApply::Disabled),
                options: None,
            },
            confidence: Confidence::High,
            reasons: vec![],
            requirements: vec![],
            conflicts: vec![],
        }];
        assert!(matches!(
            derive_invocation_model(&hints),
            InvocationModel::Direct
        ));
    }

    #[test]
    fn derive_returns_direct_for_empty_hints() {
        assert!(matches!(
            derive_invocation_model(&[]),
            InvocationModel::Direct
        ));
    }
}
