//! Adapter registry, recognizers, and invocation strategy for Rust targets.
//!
//! Mirrors the TS adapter substrate (shatter-ts/src/runtime-hooks.ts) but
//! tailored for Rust-specific patterns: async functions (Tokio runtime) and
//! framework handlers (Axum, etc.).
//!
//! Ordinary synchronous Rust exports use the existing direct-call path.

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
/// Concrete adapters are registered in follow-up issues (e.g. str-t4uo.6.3).
const SUPPORTED_ADAPTERS: &[&str] = &[];

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
    /// Create an empty registry. Concrete recognizers are registered via
    /// `register()` in follow-up issues (e.g. str-t4uo.6.2).
    pub fn new() -> Self {
        Self {
            recognizers: vec![],
        }
    }

    /// Add a recognizer to the registry.
    pub fn register(&mut self, recognizer: Box<dyn AdapterRecognizer>) {
        self.recognizers.push(recognizer);
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
/// Currently a stub — no concrete adapters are supported yet. Concrete
/// implementations will be added in follow-up issues (e.g. str-t4uo.6.3
/// for Tokio runtime adapter).
#[allow(clippy::too_many_arguments)]
pub fn execute_adapter_owned(
    adapter_id: &str,
    _file_path: &str,
    _function_name: &str,
    _inputs: &[serde_json::Value],
    _mocks: &[serde_json::Value],
    _timeout_ms: u64,
    _harness_cache: &crate::executor::HarnessCache,
    _crate_cache: &crate::executor::CrateHarnessCache,
    _bridge_cache: &crate::executor::CrateBridgeHarnessCache,
) -> Result<crate::executor::ExecuteResult, crate::executor::ExecuteError> {
    Err(crate::executor::ExecuteError::NonExecutable(format!(
        "adapter not supported: {adapter_id}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{InvocationModel, TypeInfo};

    fn stub_analysis() -> FunctionAnalysis {
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
            is_async: false,
            adapter_hints: vec![],
            invocation_model: InvocationModel::default(),
        }
    }

    /// Mock recognizer that always matches with the given adapter ID and confidence.
    struct MockRecognizer {
        adapter_id: String,
        confidence: Confidence,
    }

    impl AdapterRecognizer for MockRecognizer {
        fn recognize(&self, _analysis: &FunctionAnalysis) -> Option<AdapterHint> {
            Some(AdapterHint {
                adapter: ExecutionAdapter {
                    id: self.adapter_id.clone(),
                    apply: Some(ExecutionAdapterApply::Auto),
                    options: None,
                },
                confidence: self.confidence,
                reasons: vec!["mock match".to_string()],
                requirements: vec![],
                conflicts: vec![],
            })
        }
    }

    /// Mock recognizer that never matches.
    struct NeverMatchRecognizer;

    impl AdapterRecognizer for NeverMatchRecognizer {
        fn recognize(&self, _analysis: &FunctionAnalysis) -> Option<AdapterHint> {
            None
        }
    }

    // ── Registry tests ──

    #[test]
    fn default_registry_is_empty() {
        let registry = AdapterRegistry::new();
        let analysis = stub_analysis();
        let hints = registry.recognize_all(&analysis);
        assert!(hints.is_empty());
    }

    #[test]
    fn registry_register_adds_recognizer() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockRecognizer {
            adapter_id: "test/mock".into(),
            confidence: Confidence::High,
        }));
        let analysis = stub_analysis();
        let hints = registry.recognize_all(&analysis);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].adapter.id, "test/mock");
    }

    #[test]
    fn registry_collects_multiple_recognizers() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockRecognizer {
            adapter_id: "test/a".into(),
            confidence: Confidence::Low,
        }));
        registry.register(Box::new(NeverMatchRecognizer));
        registry.register(Box::new(MockRecognizer {
            adapter_id: "test/b".into(),
            confidence: Confidence::High,
        }));
        let hints = registry.recognize_all(&stub_analysis());
        assert_eq!(hints.len(), 2);
    }

    // ── Recognizer trait tests (AsyncFunctionRecognizer is kept but not in default registry) ──

    #[test]
    fn async_recognizer_detects_async_fn() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let recognizer = AsyncFunctionRecognizer;
        let hint = recognizer.recognize(&analysis);
        assert!(hint.is_some());
        let hint = hint.unwrap();
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_TOKIO);
        assert_eq!(hint.confidence, Confidence::High);
    }

    #[test]
    fn async_recognizer_ignores_sync_fn() {
        let analysis = stub_analysis();
        let recognizer = AsyncFunctionRecognizer;
        assert!(recognizer.recognize(&analysis).is_none());
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
    fn all_adapters_yield_unsupported_in_substrate() {
        // No concrete adapters are in SUPPORTED_ADAPTERS yet.
        let model = InvocationModel::Adapter {
            adapter_id: ADAPTER_ID_ASYNC_TOKIO.to_string(),
            synthetic_params: vec![],
            scenario_schema: None,
        };
        assert!(matches!(
            choose_invocation_strategy(&model),
            InvocationStrategy::Unsupported { .. }
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
                    id: "high-adapter".into(),
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
                assert_eq!(adapter_id, "high-adapter");
            }
            InvocationModel::Direct => panic!("expected Adapter"),
        }
    }

    #[test]
    fn derive_skips_disabled_hints() {
        let hints = vec![AdapterHint {
            adapter: ExecutionAdapter {
                id: "disabled-adapter".into(),
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

    // ── execute_adapter_owned stub tests ──

    #[test]
    fn execute_adapter_owned_returns_unsupported() {
        use std::collections::HashMap;
        let cache = crate::executor::HarnessCache::new(HashMap::new());
        let crate_cache = crate::executor::CrateHarnessCache::new(HashMap::new());
        let bridge_cache = crate::executor::CrateBridgeHarnessCache::new(HashMap::new());
        let result = execute_adapter_owned(
            ADAPTER_ID_ASYNC_TOKIO,
            "/tmp/test.rs",
            "test_fn",
            &[],
            &[],
            5000,
            &cache,
            &crate_cache,
            &bridge_cache,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::executor::ExecuteError::NonExecutable(msg) => {
                assert!(msg.contains("not supported"));
            }
            other => panic!("expected NonExecutable, got: {other:?}"),
        }
    }
}
