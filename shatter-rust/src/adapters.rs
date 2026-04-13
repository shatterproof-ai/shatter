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

/// Generic adapter for async functions that need some runtime to `.await`.
pub const ADAPTER_ID_ASYNC_RUNTIME: &str = "rust/async-runtime";

/// Adapter for async functions that require a Tokio runtime to `.await`.
pub const ADAPTER_ID_ASYNC_TOKIO: &str = "rust/async-tokio";

/// Placeholder adapter for Axum framework handlers.
pub const ADAPTER_ID_AXUM_HANDLER: &str = "rust/framework/axum-handler";

/// Adapter IDs that this frontend can execute via the adapter-owned path.
/// Concrete adapters are registered in follow-up issues (e.g. str-t4uo.6.3).
const SUPPORTED_ADAPTERS: &[&str] = &[];

// ---------------------------------------------------------------------------
// File-level context
// ---------------------------------------------------------------------------

/// File-level context extracted from `use` declarations and attributes.
/// Passed alongside per-function analysis to recognizers.
#[derive(Debug, Clone, Default)]
pub struct FileContext {
    /// Flattened `use` paths, e.g. `["tokio::spawn", "axum::extract::Json"]`.
    pub use_paths: Vec<String>,
    /// Whether any function in the file has `#[tokio::main]` or `#[tokio::test]`.
    pub has_tokio_macro: bool,
}

// ---------------------------------------------------------------------------
// Recognizer trait and implementations
// ---------------------------------------------------------------------------

/// A recognizer inspects a function analysis (plus file-level context) and
/// returns an adapter hint if it detects a pattern requiring adapter-owned
/// invocation.
pub trait AdapterRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis, ctx: &FileContext) -> Option<AdapterHint>;
}

// ── AsyncRuntimeRecognizer ──────────────────────────────────────────────────

/// Emits a generic `rust/async-runtime` hint at Medium confidence for any
/// `async fn`. More specific recognizers (Tokio, Axum) override this when
/// framework evidence is present.
pub struct AsyncRuntimeRecognizer;

impl AdapterRecognizer for AsyncRuntimeRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis, _ctx: &FileContext) -> Option<AdapterHint> {
        if !analysis.is_async {
            return None;
        }
        Some(AdapterHint {
            adapter: ExecutionAdapter {
                id: ADAPTER_ID_ASYNC_RUNTIME.to_string(),
                apply: Some(ExecutionAdapterApply::Auto),
                options: None,
            },
            confidence: Confidence::Medium,
            reasons: vec!["function is async".to_string()],
            requirements: vec![],
            conflicts: vec![],
        })
    }
}

// ── TokioRecognizer ─────────────────────────────────────────────────────────

/// Well-known Tokio types that appear as function parameter type names.
const TOKIO_PARAM_TYPES: &[&str] = &[
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "JoinHandle",
    "JoinSet",
    "Mutex",
    "RwLock",
    "Semaphore",
    "Notify",
    "Barrier",
    "Receiver",
    "Sender",
    "UnboundedReceiver",
    "UnboundedSender",
];

/// Emits `rust/async-tokio` at High confidence when the function is async AND
/// there is strong Tokio evidence: a `tokio::` import, a `#[tokio::main]` /
/// `#[tokio::test]` macro, or Tokio types in function parameters.
pub struct TokioRecognizer;

impl AdapterRecognizer for TokioRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis, ctx: &FileContext) -> Option<AdapterHint> {
        if !analysis.is_async {
            return None;
        }

        let mut reasons = Vec::new();

        if ctx
            .use_paths
            .iter()
            .any(|p| p.starts_with("tokio::") || p == "tokio")
        {
            reasons.push("file imports tokio".to_string());
        }

        if ctx.has_tokio_macro {
            reasons.push("file uses #[tokio::main] or #[tokio::test]".to_string());
        }

        let tokio_params: Vec<&str> = analysis
            .params
            .iter()
            .filter_map(|p| p.type_name.as_deref())
            .filter(|tn| TOKIO_PARAM_TYPES.contains(tn))
            .collect();
        if !tokio_params.is_empty() {
            reasons.push(format!(
                "params use tokio types: {}",
                tokio_params.join(", ")
            ));
        }

        if reasons.is_empty() {
            return None;
        }

        Some(AdapterHint {
            adapter: ExecutionAdapter {
                id: ADAPTER_ID_ASYNC_TOKIO.to_string(),
                apply: Some(ExecutionAdapterApply::Auto),
                options: None,
            },
            confidence: Confidence::High,
            reasons,
            requirements: vec![],
            conflicts: vec![],
        })
    }
}

// ── AxumHandlerRecognizer ───────────────────────────────────────────────────

/// Axum extractor types that appear as function parameter type names.
const AXUM_EXTRACTOR_TYPES: &[&str] = &[
    "Json",
    "Path",
    "Query",
    "State",
    "Extension",
    "Form",
    "TypedHeader",
    "ConnectInfo",
    "MatchedPath",
    "OriginalUri",
    "RawBody",
    "RawQuery",
    "Host",
    "NestedPath",
    "Multipart",
];

/// Emits `rust/framework/axum-handler` at High confidence when the function
/// is async, the file imports `axum::`, AND the function has axum extractor
/// types in its parameters. Requires both signals — no framework guesses from
/// naming alone.
pub struct AxumHandlerRecognizer;

impl AdapterRecognizer for AxumHandlerRecognizer {
    fn recognize(&self, analysis: &FunctionAnalysis, ctx: &FileContext) -> Option<AdapterHint> {
        if !analysis.is_async {
            return None;
        }

        let has_axum_import = ctx
            .use_paths
            .iter()
            .any(|p| p.starts_with("axum::") || p == "axum");
        if !has_axum_import {
            return None;
        }

        let extractor_params: Vec<&str> = analysis
            .params
            .iter()
            .filter_map(|p| p.type_name.as_deref())
            .filter(|tn| AXUM_EXTRACTOR_TYPES.contains(tn))
            .collect();
        if extractor_params.is_empty() {
            return None;
        }

        Some(AdapterHint {
            adapter: ExecutionAdapter {
                id: ADAPTER_ID_AXUM_HANDLER.to_string(),
                apply: Some(ExecutionAdapterApply::Auto),
                options: None,
            },
            confidence: Confidence::High,
            reasons: vec![
                "file imports axum".to_string(),
                format!(
                    "params use axum extractors: {}",
                    extractor_params.join(", ")
                ),
            ],
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
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            recognizers: vec![],
        }
    }

    /// Create a registry pre-populated with the built-in Rust recognizers.
    /// Registration order matters: Axum (last) wins ties via `max_by_key`.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(AsyncRuntimeRecognizer));
        registry.register(Box::new(TokioRecognizer));
        registry.register(Box::new(AxumHandlerRecognizer));
        registry
    }

    /// Add a recognizer to the registry.
    pub fn register(&mut self, recognizer: Box<dyn AdapterRecognizer>) {
        self.recognizers.push(recognizer);
    }

    /// Run all recognizers against a function analysis, collecting hints.
    pub fn recognize_all(
        &self,
        analysis: &FunctionAnalysis,
        ctx: &FileContext,
    ) -> Vec<AdapterHint> {
        self.recognizers
            .iter()
            .filter_map(|r| r.recognize(analysis, ctx))
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::with_builtins()
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
    use crate::protocol::{InvocationModel, ParamInfo, TypeInfo};

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

    fn param_with_type_name(name: &str, type_name: &str) -> ParamInfo {
        ParamInfo {
            name: name.to_string(),
            typ: TypeInfo::Unknown,
            type_name: Some(type_name.to_string()),
        }
    }

    /// Mock recognizer that always matches with the given adapter ID and confidence.
    struct MockRecognizer {
        adapter_id: String,
        confidence: Confidence,
    }

    impl AdapterRecognizer for MockRecognizer {
        fn recognize(&self, _analysis: &FunctionAnalysis, _ctx: &FileContext) -> Option<AdapterHint> {
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
        fn recognize(&self, _analysis: &FunctionAnalysis, _ctx: &FileContext) -> Option<AdapterHint> {
            None
        }
    }

    // ── Registry tests ──

    #[test]
    fn empty_registry_produces_no_hints() {
        let registry = AdapterRegistry::new();
        let analysis = stub_analysis();
        let hints = registry.recognize_all(&analysis, &FileContext::default());
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
        let hints = registry.recognize_all(&analysis, &FileContext::default());
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
        let hints = registry.recognize_all(&stub_analysis(), &FileContext::default());
        assert_eq!(hints.len(), 2);
    }

    // ── AsyncRuntimeRecognizer tests ──

    #[test]
    fn async_runtime_recognizer_emits_medium_for_async_fn() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let recognizer = AsyncRuntimeRecognizer;
        let hint = recognizer
            .recognize(&analysis, &FileContext::default())
            .expect("should match async fn");
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_RUNTIME);
        assert_eq!(hint.confidence, Confidence::Medium);
        assert!(hint.reasons.iter().any(|r| r.contains("async")));
    }

    #[test]
    fn async_runtime_recognizer_ignores_sync_fn() {
        let analysis = stub_analysis();
        let recognizer = AsyncRuntimeRecognizer;
        assert!(recognizer
            .recognize(&analysis, &FileContext::default())
            .is_none());
    }

    // ── TokioRecognizer tests ──

    #[test]
    fn tokio_recognizer_high_with_import() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let ctx = FileContext {
            use_paths: vec!["tokio::spawn".into()],
            has_tokio_macro: false,
        };
        let hint = TokioRecognizer
            .recognize(&analysis, &ctx)
            .expect("should match with tokio import");
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_TOKIO);
        assert_eq!(hint.confidence, Confidence::High);
        assert!(hint.reasons.iter().any(|r| r.contains("imports tokio")));
    }

    #[test]
    fn tokio_recognizer_high_with_macro() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let ctx = FileContext {
            use_paths: vec![],
            has_tokio_macro: true,
        };
        let hint = TokioRecognizer
            .recognize(&analysis, &ctx)
            .expect("should match with tokio macro");
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_TOKIO);
        assert_eq!(hint.confidence, Confidence::High);
        assert!(hint.reasons.iter().any(|r| r.contains("tokio::main")));
    }

    #[test]
    fn tokio_recognizer_high_with_param_types() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        analysis.params = vec![param_with_type_name("stream", "TcpStream")];
        let hint = TokioRecognizer
            .recognize(&analysis, &FileContext::default())
            .expect("should match with tokio param type");
        assert_eq!(hint.adapter.id, ADAPTER_ID_ASYNC_TOKIO);
        assert_eq!(hint.confidence, Confidence::High);
        assert!(hint.reasons.iter().any(|r| r.contains("TcpStream")));
    }

    #[test]
    fn tokio_recognizer_none_without_evidence() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        assert!(TokioRecognizer
            .recognize(&analysis, &FileContext::default())
            .is_none());
    }

    #[test]
    fn tokio_recognizer_none_for_sync_fn() {
        let ctx = FileContext {
            use_paths: vec!["tokio::spawn".into()],
            has_tokio_macro: true,
        };
        assert!(TokioRecognizer
            .recognize(&stub_analysis(), &ctx)
            .is_none());
    }

    // ── AxumHandlerRecognizer tests ──

    #[test]
    fn axum_recognizer_high_with_both_signals() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        analysis.params = vec![param_with_type_name("body", "Json")];
        let ctx = FileContext {
            use_paths: vec!["axum::extract::Json".into()],
            has_tokio_macro: false,
        };
        let hint = AxumHandlerRecognizer
            .recognize(&analysis, &ctx)
            .expect("should match with axum import + extractor param");
        assert_eq!(hint.adapter.id, ADAPTER_ID_AXUM_HANDLER);
        assert_eq!(hint.confidence, Confidence::High);
        assert!(hint.reasons.iter().any(|r| r.contains("imports axum")));
        assert!(hint.reasons.iter().any(|r| r.contains("Json")));
    }

    #[test]
    fn axum_recognizer_none_without_import() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        analysis.params = vec![param_with_type_name("body", "Json")];
        assert!(AxumHandlerRecognizer
            .recognize(&analysis, &FileContext::default())
            .is_none());
    }

    #[test]
    fn axum_recognizer_none_without_extractor_params() {
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let ctx = FileContext {
            use_paths: vec!["axum::Router".into()],
            has_tokio_macro: false,
        };
        assert!(AxumHandlerRecognizer
            .recognize(&analysis, &ctx)
            .is_none());
    }

    #[test]
    fn axum_recognizer_none_for_sync_fn() {
        let analysis = stub_analysis();
        let ctx = FileContext {
            use_paths: vec!["axum::extract::Json".into()],
            has_tokio_macro: false,
        };
        assert!(AxumHandlerRecognizer
            .recognize(&analysis, &ctx)
            .is_none());
    }

    // ── with_builtins integration ──

    #[test]
    fn builtins_registry_all_three_fire_for_axum_handler() {
        let registry = AdapterRegistry::with_builtins();
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        analysis.params = vec![param_with_type_name("body", "Json")];
        let ctx = FileContext {
            use_paths: vec![
                "tokio::net::TcpListener".into(),
                "axum::extract::Json".into(),
            ],
            has_tokio_macro: false,
        };
        let hints = registry.recognize_all(&analysis, &ctx);
        // All three recognizers should fire: async-runtime, async-tokio, axum-handler.
        assert_eq!(hints.len(), 3);
        let ids: Vec<&str> = hints.iter().map(|h| h.adapter.id.as_str()).collect();
        assert!(ids.contains(&ADAPTER_ID_ASYNC_RUNTIME));
        assert!(ids.contains(&ADAPTER_ID_ASYNC_TOKIO));
        assert!(ids.contains(&ADAPTER_ID_AXUM_HANDLER));

        // derive_invocation_model picks axum (last High-confidence hint).
        let model = derive_invocation_model(&hints);
        match model {
            InvocationModel::Adapter { adapter_id, .. } => {
                assert_eq!(adapter_id, ADAPTER_ID_AXUM_HANDLER);
            }
            InvocationModel::Direct => panic!("expected Adapter"),
        }
    }

    #[test]
    fn builtins_registry_only_generic_for_plain_async() {
        let registry = AdapterRegistry::with_builtins();
        let mut analysis = stub_analysis();
        analysis.is_async = true;
        let hints = registry.recognize_all(&analysis, &FileContext::default());
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].adapter.id, ADAPTER_ID_ASYNC_RUNTIME);
        assert_eq!(hints[0].confidence, Confidence::Medium);
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
