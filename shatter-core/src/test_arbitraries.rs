//! Proptest strategies for generating arbitrary protocol and domain types.
//!
//! Shared across test modules to avoid duplicating complex recursive generators.
//! All strategies use bounded depth/size to keep generation tractable.

#![allow(dead_code)]

use proptest::prelude::*;
use serde_json::json;

use crate::auto_mock::{IoCategory, MockParam, ValueSource};
use crate::behavior::{Behavior, BehaviorMap};
use crate::crypto_registry::{CryptoDirection, OutputSemantics, ParamRole};
use crate::equivalence::{BranchPath, BranchStep, Precondition};
use crate::execution_record::{
    BranchDecision, ErrorInfo, ExternalCall, ScopeEvent, SideEffect, SymConstraint, TraceEvent,
    TruncationInfo,
};
use crate::invariants::{
    ClassifiedInvariant, ComparisonOp, Invariant, InvariantKind, InvariantTarget,
};
use crate::protocol::{
    BoundOp, BranchInfo, BranchType, Command, ConnectionFailure, CryptoBoundary,
    DepDetectionKind, DependencyKind, DiscoveredDependency, ErrorCode, ExecuteResult,
    ExternalDependency, FunctionAnalysis, GeneratorKind, InductionVar, LiteralValue, LoopInfo,
    MockBehavior, MockConfig, PerformanceMetrics, Request, Response, ResponseResult,
    RuntimeCryptoBoundary, RuntimeCryptoBoundaryKind, TimingPhaseSummary, TimingSummary,
    PROTOCOL_VERSION,
};
use crate::protocol::{SetupContextEntry, SetupContextStack, SetupLevel};
use crate::spec::{ConcreteExample, FunctionSpec, Postcondition, Provenance, SpecClass};
use crate::sym_expr::{BinOpKind, ConstValue, SymExpr, UnOpKind};
use crate::triage::{BranchPrediction, TriageDisableReason, TriageVerdict};
use crate::types::{ComplexKind, MediumOpacityReason, ParamInfo, StaticOpacityReason, TypeInfo};

// ---------------------------------------------------------------------------
// Leaf strategies
// ---------------------------------------------------------------------------

/// Short identifier-like strings for names, paths, symbols.
fn arb_ident() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,12}".prop_map(|s| s)
}

/// Short arbitrary strings for messages, hints, etc.
fn arb_short_string() -> impl Strategy<Value = String> {
    ".{0,30}"
}

/// Small set of representative JSON values to avoid unbounded generation.
pub fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(json!(null)),
        Just(json!(42)),
        Just(json!(-1)),
        Just(json!(3.14)),
        Just(json!("hello")),
        Just(json!("")),
        Just(json!(true)),
        Just(json!(false)),
        Just(json!([1, 2, 3])),
        Just(json!({"key": "value"})),
    ]
}

/// JSON values excluding null — for use in `Option<Value>` fields where
/// `Some(null)` and `None` are indistinguishable after a JSON round-trip.
fn arb_json_value_non_null() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(json!(42)),
        Just(json!(-1)),
        Just(json!(3.14)),
        Just(json!("hello")),
        Just(json!("")),
        Just(json!(true)),
        Just(json!(false)),
        Just(json!([1, 2, 3])),
        Just(json!({"key": "value"})),
    ]
}

// ---------------------------------------------------------------------------
// Enum strategies (flat, non-recursive)
// ---------------------------------------------------------------------------

pub fn arb_bin_op_kind() -> impl Strategy<Value = BinOpKind> {
    prop_oneof![
        Just(BinOpKind::Eq),
        Just(BinOpKind::Ne),
        Just(BinOpKind::Lt),
        Just(BinOpKind::Le),
        Just(BinOpKind::Gt),
        Just(BinOpKind::Ge),
        Just(BinOpKind::Add),
        Just(BinOpKind::Sub),
        Just(BinOpKind::Mul),
        Just(BinOpKind::Div),
        Just(BinOpKind::Mod),
        Just(BinOpKind::And),
        Just(BinOpKind::Or),
        Just(BinOpKind::BitwiseAnd),
        Just(BinOpKind::BitwiseOr),
        Just(BinOpKind::BitwiseXor),
        Just(BinOpKind::Shl),
        Just(BinOpKind::Shr),
        Just(BinOpKind::BitClear),
        Just(BinOpKind::In),
        Just(BinOpKind::InstanceOf),
    ]
}

pub fn arb_un_op_kind() -> impl Strategy<Value = UnOpKind> {
    prop_oneof![
        Just(UnOpKind::Not),
        Just(UnOpKind::Neg),
        Just(UnOpKind::BitwiseNot),
        Just(UnOpKind::TypeOf),
    ]
}

pub fn arb_complex_kind() -> impl Strategy<Value = ComplexKind> {
    prop_oneof![
        Just(ComplexKind::Date),
        Just(ComplexKind::DateTime),
        Just(ComplexKind::Time),
        Just(ComplexKind::Duration),
        Just(ComplexKind::RegExp),
        Just(ComplexKind::Char),
        Just(ComplexKind::Symbol),
        Just(ComplexKind::BigInt),
        Just(ComplexKind::BigDecimal),
        Just(ComplexKind::Complex),
        Just(ComplexKind::Rational),
        Just(ComplexKind::Range),
        Just(ComplexKind::Buffer),
        Just(ComplexKind::BitSet),
        Just(ComplexKind::Error),
        Just(ComplexKind::Option),
        Just(ComplexKind::Result),
        Just(ComplexKind::Closure),
        Just(ComplexKind::Iterator),
        Just(ComplexKind::Url),
        Just(ComplexKind::IpAddress),
        Just(ComplexKind::Uuid),
        Just(ComplexKind::Path),
        Just(ComplexKind::Money),
        Just(ComplexKind::SemVer),
        Just(ComplexKind::Email),
        Just(ComplexKind::MimeType),
        Just(ComplexKind::Color),
        Just(ComplexKind::GeoPoint),
        Just(ComplexKind::Locale),
        Just(ComplexKind::Rune),
        Just(ComplexKind::GoByte),
    ]
}

pub fn arb_branch_type() -> impl Strategy<Value = BranchType> {
    prop_oneof![
        Just(BranchType::If),
        Just(BranchType::ElseIf),
        Just(BranchType::Switch),
        Just(BranchType::Ternary),
        Just(BranchType::LogicalAnd),
        Just(BranchType::LogicalOr),
        Just(BranchType::While),
        Just(BranchType::For),
        Just(BranchType::Select),
    ]
}

pub fn arb_error_code() -> impl Strategy<Value = ErrorCode> {
    prop_oneof![
        Just(ErrorCode::FileNotFound),
        Just(ErrorCode::FunctionNotFound),
        Just(ErrorCode::ParseError),
        Just(ErrorCode::InstrumentationFailed),
        Just(ErrorCode::ExecutionTimeout),
        Just(ErrorCode::ExecutionCrash),
        Just(ErrorCode::VersionMismatch),
        Just(ErrorCode::InvalidRequest),
        Just(ErrorCode::CompilationError),
        Just(ErrorCode::InternalError),
        Just(ErrorCode::NotSupported),
    ]
}

pub fn arb_setup_level() -> impl Strategy<Value = SetupLevel> {
    prop_oneof![
        Just(SetupLevel::Session),
        Just(SetupLevel::File),
        Just(SetupLevel::Function),
        Just(SetupLevel::Execution),
    ]
}

pub fn arb_setup_context_entry() -> impl Strategy<Value = SetupContextEntry> {
    (arb_setup_level(), arb_json_value())
        .prop_map(|(level, context)| SetupContextEntry { level, context })
}

pub fn arb_setup_context_stack() -> impl Strategy<Value = SetupContextStack> {
    prop::collection::vec(arb_setup_context_entry(), 0..=3)
        .prop_map(|contexts| SetupContextStack { contexts })
}

pub fn arb_mock_behavior() -> impl Strategy<Value = MockBehavior> {
    prop_oneof![
        Just(MockBehavior::ReturnGenerated),
        Just(MockBehavior::RepeatLast),
        Just(MockBehavior::ThrowError),
        Just(MockBehavior::Passthrough),
    ]
}

pub fn arb_generator_kind() -> impl Strategy<Value = GeneratorKind> {
    prop_oneof![
        Just(GeneratorKind::TypeName),
        Just(GeneratorKind::ParamName),
    ]
}

pub fn arb_dependency_kind() -> impl Strategy<Value = DependencyKind> {
    prop_oneof![
        Just(DependencyKind::FunctionCall),
        Just(DependencyKind::MethodCall),
        Just(DependencyKind::PropertyAccess),
        Just(DependencyKind::ModuleImport),
    ]
}

pub fn arb_crypto_direction() -> impl Strategy<Value = CryptoDirection> {
    prop_oneof![
        Just(CryptoDirection::Encrypt),
        Just(CryptoDirection::Decrypt),
        Just(CryptoDirection::Both),
    ]
}

pub fn arb_output_semantics() -> impl Strategy<Value = OutputSemantics> {
    prop_oneof![
        Just(OutputSemantics::Ciphertext),
        Just(OutputSemantics::Plaintext),
        Just(OutputSemantics::Key),
        Just(OutputSemantics::Hash),
        Just(OutputSemantics::Signature),
        Just(OutputSemantics::Verified),
    ]
}

pub fn arb_param_role() -> impl Strategy<Value = ParamRole> {
    prop_oneof![
        Just(ParamRole::Key),
        Just(ParamRole::Data),
        Just(ParamRole::Iv),
        Just(ParamRole::Nonce),
        Just(ParamRole::Tag),
        Just(ParamRole::Aad),
        Just(ParamRole::Algorithm),
    ]
}

// ---------------------------------------------------------------------------
// Recursive strategies (depth-bounded)
// ---------------------------------------------------------------------------

/// Arbitrary ConstValue with bounded Complex nesting.
pub fn arb_const_value(depth: u32) -> BoxedStrategy<ConstValue> {
    let leaf = prop_oneof![
        // Use integer-valued f64 to avoid JSON round-trip precision loss.
        (-1_000_000i64..1_000_000i64).prop_map(ConstValue::Int),
        (-1000i32..1000i32).prop_map(|n| ConstValue::Float(f64::from(n))),
        arb_short_string().prop_map(ConstValue::Str),
        any::<bool>().prop_map(ConstValue::Bool),
        Just(ConstValue::Null),
        Just(ConstValue::Undefined),
    ];
    if depth == 0 {
        leaf.boxed()
    } else {
        prop_oneof![
            9 => leaf,
            1 => (arb_complex_kind(), arb_const_value(depth - 1))
                .prop_map(|(kind, repr)| ConstValue::Complex {
                    kind,
                    repr: Box::new(repr),
                }),
        ]
        .boxed()
    }
}

/// Arbitrary SymExpr with bounded tree depth.
pub fn arb_sym_expr(depth: u32) -> BoxedStrategy<SymExpr> {
    let leaf = prop_oneof![
        (arb_ident(), prop::collection::vec(arb_ident(), 0..=3))
            .prop_map(|(name, path)| SymExpr::Param { name, path }),
        arb_const_value(1).prop_map(SymExpr::Const),
        Just(SymExpr::Unknown),
    ];
    if depth == 0 {
        leaf.boxed()
    } else {
        prop_oneof![
            5 => leaf,
            2 => (arb_bin_op_kind(), arb_sym_expr(depth - 1), arb_sym_expr(depth - 1))
                .prop_map(|(op, left, right)| SymExpr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }),
            2 => (arb_un_op_kind(), arb_sym_expr(depth - 1))
                .prop_map(|(op, operand)| SymExpr::UnOp {
                    op,
                    operand: Box::new(operand),
                }),
            1 => (
                arb_ident(),
                proptest::option::of(arb_sym_expr(depth - 1)),
                prop::collection::vec(arb_sym_expr(depth - 1), 0..=2),
            )
                .prop_map(|(name, receiver, args)| SymExpr::Call {
                    name,
                    receiver: receiver.map(Box::new),
                    args,
                }),
            1 => (arb_sym_expr(depth - 1), arb_sym_expr(depth - 1), arb_sym_expr(depth - 1))
                .prop_map(|(condition, then_expr, else_expr)| SymExpr::Ite {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }),
        ]
        .boxed()
    }
}

/// Arbitrary TypeInfo with bounded structural depth.
pub fn arb_type_info(depth: u32) -> BoxedStrategy<TypeInfo> {
    let leaf = prop_oneof![
        Just(TypeInfo::Int),
        Just(TypeInfo::Float),
        Just(TypeInfo::Str),
        Just(TypeInfo::Bool),
        Just(TypeInfo::Unknown),
        (
            arb_ident(),
            prop_oneof![
                Just(None),
                Just(Some(StaticOpacityReason::NoConstructor)),
                Just(Some(StaticOpacityReason::TransitivelyOpaque)),
                Just(Some(StaticOpacityReason::AbstractType)),
                Just(Some(StaticOpacityReason::NoImplementors)),
            ],
            prop_oneof![
                Just(None::<MediumOpacityReason>),
                Just(Some(MediumOpacityReason::InfrastructurePackage)),
                Just(Some(MediumOpacityReason::CloseableInterface)),
                Just(Some(MediumOpacityReason::NativeHandleField)),
            ],
        )
            .prop_map(|(label, static_opacity, medium_opacity)| TypeInfo::Opaque {
                label,
                static_opacity,
                medium_opacity,
            }),
    ];
    if depth == 0 {
        leaf.boxed()
    } else {
        prop_oneof![
            6 => leaf,
            1 => arb_type_info(depth - 1)
                .prop_map(|element| TypeInfo::Array { element: Box::new(element) }),
            1 => prop::collection::vec(
                    (arb_ident(), arb_type_info(depth - 1)),
                    0..=4,
                )
                .prop_map(|fields| TypeInfo::Object { fields }),
            1 => prop::collection::vec(arb_type_info(depth - 1), 2..=4)
                .prop_map(|variants| TypeInfo::Union { variants }),
            1 => arb_type_info(depth - 1)
                .prop_map(|inner| TypeInfo::Nullable { inner: Box::new(inner) }),
        ]
        .boxed()
    }
}

pub fn arb_param_info() -> impl Strategy<Value = ParamInfo> {
    (arb_ident(), arb_type_info(2)).prop_map(|(name, typ)| ParamInfo {
        name,
        typ,
        type_name: None,
    })
}

// ---------------------------------------------------------------------------
// Orchestrator strategies
// ---------------------------------------------------------------------------

pub fn arb_input_source() -> impl Strategy<Value = crate::orchestrator::InputSource> {
    use crate::orchestrator::InputSource;
    prop_oneof![
        Just(InputSource::Seed),
        Just(InputSource::Fuzzed),
        Just(InputSource::Drilled),
        Just(InputSource::McdcTarget),
        Just(InputSource::Z3Solved),
        Just(InputSource::UserProvided),
    ]
}

// ---------------------------------------------------------------------------
// Execution record strategies
// ---------------------------------------------------------------------------

pub fn arb_sym_constraint() -> impl Strategy<Value = SymConstraint> {
    prop_oneof![
        arb_sym_expr(2).prop_map(|expr| SymConstraint::Expr { expr }),
        arb_short_string().prop_map(|hint| SymConstraint::Unknown { hint }),
    ]
}

pub fn arb_branch_decision() -> impl Strategy<Value = BranchDecision> {
    (0..100u32, 1..500u32, any::<bool>(), arb_sym_constraint()).prop_map(
        |(branch_id, line, taken, constraint)| BranchDecision {
            branch_id,
            line,
            taken,
            constraint,
            conditions: None,
        },
    )
}

pub fn arb_scope_event() -> impl Strategy<Value = ScopeEvent> {
    prop_oneof![
        (0..20u32).prop_map(|id| ScopeEvent::LoopEnter { loop_id: id }),
        (0..20u32).prop_map(|id| ScopeEvent::LoopExit { loop_id: id }),
        (0..20u32).prop_map(|id| ScopeEvent::CallEnter { call_site_id: id }),
        (0..20u32).prop_map(|id| ScopeEvent::CallExit { call_site_id: id }),
    ]
}

pub fn arb_trace_event() -> impl Strategy<Value = TraceEvent> {
    prop_oneof![
        arb_branch_decision().prop_map(|decision| TraceEvent::Branch { decision }),
        arb_scope_event().prop_map(|event| TraceEvent::Scope { event }),
    ]
}

pub fn arb_error_info() -> impl Strategy<Value = ErrorInfo> {
    (
        arb_ident(),
        arb_short_string(),
        proptest::option::of(arb_short_string()),
        proptest::option::of(prop_oneof![
            Just("validation".to_string()),
            Just("runtime".to_string()),
            Just("infrastructure".to_string()),
            Just("unknown".to_string()),
        ]),
    )
        .prop_map(|(error_type, message, stack, error_category)| ErrorInfo {
            error_type,
            message,
            stack,
            error_category,
        })
}

pub fn arb_side_effect() -> impl Strategy<Value = SideEffect> {
    prop_oneof![
        (arb_ident(), arb_short_string())
            .prop_map(|(level, message)| SideEffect::ConsoleOutput { level, message }),
        (arb_ident(), proptest::option::of(arb_short_string()))
            .prop_map(|(path, content)| SideEffect::FileWrite { path, content }),
        (
            arb_ident(),
            arb_short_string(),
            proptest::option::of(arb_json_value_non_null())
        )
            .prop_map(|(method, url, body)| SideEffect::NetworkRequest {
                method,
                url,
                body
            }),
        (arb_ident(), proptest::option::of(arb_short_string()))
            .prop_map(|(variable, value)| SideEffect::EnvironmentRead { variable, value }),
        arb_ident().prop_map(|name| SideEffect::GlobalMutation { name }),
        (
            arb_ident(),
            arb_short_string(),
            proptest::option::of(arb_short_string())
        )
            .prop_map(|(error_type, message, stack)| SideEffect::ThrownError {
                error_type,
                message,
                stack,
            }),
        (arb_ident(), arb_json_value(), arb_json_value()).prop_map(|(variable, before, after)| {
            SideEffect::GlobalStateChange {
                variable,
                before,
                after,
            }
        }),
    ]
}

pub fn arb_external_call() -> impl Strategy<Value = ExternalCall> {
    (
        arb_ident(),
        prop::collection::vec(arb_json_value(), 0..=3),
        arb_json_value(),
    )
        .prop_map(|(symbol, args, return_value)| ExternalCall {
            symbol,
            args,
            return_value,
        })
}

pub fn arb_truncation_info() -> impl Strategy<Value = TruncationInfo> {
    (any::<bool>(), 0..1000u32, 0..10000u64).prop_map(
        |(was_truncated, original_lines, original_bytes)| TruncationInfo {
            was_truncated,
            original_lines,
            original_bytes,
        },
    )
}

pub fn arb_performance_metrics() -> impl Strategy<Value = PerformanceMetrics> {
    // Use integer-valued f64 for wall_time_ms to avoid JSON round-trip precision loss.
    (
        0..10000u64,
        0..10_000_000u64,
        0..100_000_000u64,
        0..100_000_000u64,
    )
        .prop_map(
            |(wall_ms, cpu_time_us, heap_used_bytes, heap_allocated_bytes)| PerformanceMetrics {
                wall_time_ms: wall_ms as f64,
                cpu_time_us,
                heap_used_bytes,
                heap_allocated_bytes,
            },
        )
}

pub fn arb_dep_detection_kind() -> impl Strategy<Value = DepDetectionKind> {
    prop_oneof![
        Just(DepDetectionKind::UnmockedImport),
        Just(DepDetectionKind::SubprocessSpawn),
        Just(DepDetectionKind::StubbedImport),
    ]
}

pub fn arb_discovered_dependency() -> impl Strategy<Value = DiscoveredDependency> {
    (
        arb_ident(),
        arb_ident(),
        arb_dep_detection_kind(),
        any::<bool>(),
    )
        .prop_map(
            |(symbol, source_module, kind, is_subprocess_spawn)| DiscoveredDependency {
                symbol,
                source_module,
                kind,
                is_subprocess_spawn,
            },
        )
}

pub fn arb_connection_failure() -> impl Strategy<Value = ConnectionFailure> {
    (
        arb_ident(),
        prop_oneof![
            Just("connection_refused".to_string()),
            Just("dns_failure".to_string()),
            Just("auth_error".to_string()),
            Just("timeout".to_string()),
            Just("other".to_string()),
        ],
        arb_short_string(),
    )
        .prop_map(|(symbol, error_kind, message)| ConnectionFailure {
            symbol,
            error_kind,
            message,
        })
}

pub fn arb_runtime_crypto_boundary_kind(
) -> impl Strategy<Value = RuntimeCryptoBoundaryKind> {
    prop_oneof![
        Just(RuntimeCryptoBoundaryKind::Encrypt),
        Just(RuntimeCryptoBoundaryKind::Decrypt),
    ]
}

pub fn arb_runtime_crypto_boundary() -> impl Strategy<Value = RuntimeCryptoBoundary> {
    (
        arb_short_string(),
        arb_runtime_crypto_boundary_kind(),
        prop_oneof![
            Just("createDecipheriv".to_string()),
            Just("privateDecrypt".to_string()),
            Just("createCipheriv".to_string()),
        ],
        proptest::option::of(prop_oneof![
            Just("aes-256-cbc".to_string()),
            Just("aes-128-gcm".to_string()),
        ]),
        proptest::option::of(-1i32..=5i32),
        proptest::option::of(arb_short_string()),
        proptest::option::of(arb_short_string()),
    )
        .prop_map(
            |(boundary_id, kind, function_name, algorithm, ciphertext_param_index, key_value, iv_value)| {
                RuntimeCryptoBoundary {
                    boundary_id,
                    kind,
                    function_name,
                    algorithm,
                    ciphertext_param_index,
                    key_value,
                    iv_value,
                }
            },
        )
}

pub fn arb_execute_result() -> impl Strategy<Value = ExecuteResult> {
    (
        proptest::option::of(arb_json_value_non_null()),
        proptest::option::of(arb_error_info()),
        prop::collection::vec(arb_branch_decision(), 0..=5),
        prop::collection::vec(1..500u32, 0..=10),
        prop::collection::vec(arb_external_call(), 0..=3),
        prop::collection::vec(arb_sym_constraint(), 0..=3),
        prop::collection::vec(arb_trace_event(), 0..=8),
        prop::collection::vec(arb_side_effect(), 0..=3),
        arb_performance_metrics(),
        (
            proptest::option::of(arb_truncation_info()),
            prop::collection::vec(arb_discovered_dependency(), 0..=3),
            prop::collection::vec(arb_connection_failure(), 0..=3),
            prop::collection::vec(arb_runtime_crypto_boundary(), 0..=2),
        ),
    )
        .prop_map(
            |(
                return_value,
                thrown_error,
                branch_path,
                lines_executed,
                calls_to_external,
                path_constraints,
                scope_events,
                side_effects,
                performance,
                (
                    capture_truncation,
                    discovered_dependencies,
                    connection_failures,
                    runtime_crypto_boundaries,
                ),
            )| {
                ExecuteResult {
                    return_value,
                    thrown_error,
                    branch_path,
                    lines_executed,
                    calls_to_external,
                    path_constraints,
                    scope_events,
                    side_effects,
                    performance,
                    capture_truncation,
                    discovered_dependencies,
                    connection_failures,
                    runtime_crypto_boundaries,
                }
            },
        )
}

// ---------------------------------------------------------------------------
// Protocol message strategies
// ---------------------------------------------------------------------------

pub fn arb_mock_config() -> impl Strategy<Value = MockConfig> {
    (
        arb_ident(),
        prop::collection::vec(arb_json_value(), 0..=3),
        any::<bool>(),
        arb_mock_behavior(),
    )
        .prop_map(
            |(symbol, return_values, should_track_calls, default_behavior)| MockConfig {
                symbol,
                return_values,
                should_track_calls,
                default_behavior,
            },
        )
}

pub fn arb_literal_value() -> impl Strategy<Value = LiteralValue> {
    prop_oneof![
        (-1000i64..1000).prop_map(|value| LiteralValue::Int { value }),
        (-1000i32..1000).prop_map(|n| LiteralValue::Float {
            value: f64::from(n)
        }),
        arb_short_string().prop_map(|value| LiteralValue::Str { value }),
        any::<bool>().prop_map(|value| LiteralValue::Bool { value }),
        arb_short_string().prop_map(|pattern| LiteralValue::Regex { pattern }),
    ]
}

pub fn arb_branch_info() -> impl Strategy<Value = BranchInfo> {
    (
        0..100u32,
        1..500u32,
        arb_short_string(),
        proptest::option::of(arb_sym_expr(2)),
        arb_branch_type(),
    )
        .prop_map(
            |(id, line, condition_text, condition, branch_type)| BranchInfo {
                id,
                line,
                condition_text,
                condition,
                branch_type,
            },
        )
}

pub fn arb_confidence() -> impl Strategy<Value = crate::nondeterminism::Confidence> {
    use crate::nondeterminism::Confidence;
    prop_oneof![
        Just(Confidence::Low),
        Just(Confidence::Medium),
        Just(Confidence::High),
    ]
}

pub fn arb_crypto_boundary() -> impl Strategy<Value = CryptoBoundary> {
    (
        arb_ident(),
        arb_ident(),
        arb_crypto_direction(),
        proptest::option::of(arb_output_semantics()),
        arb_confidence(),
    )
        .prop_map(
            |(symbol, source_module, direction, output, confidence)| CryptoBoundary {
                symbol,
                source_module,
                direction,
                output,
                confidence,
                param_roles: std::collections::HashMap::new(),
                call_sites: vec![],
                input_entropy: None,
                output_entropy: None,
            },
        )
}

pub fn arb_external_dependency() -> impl Strategy<Value = ExternalDependency> {
    (
        arb_dependency_kind(),
        arb_ident(),
        arb_ident(),
        arb_type_info(1),
        prop::collection::vec(arb_type_info(1), 0..=3),
        prop::collection::vec(1..500u32, 0..=3),
    )
        .prop_map(
            |(kind, symbol, source_module, return_type, param_types, call_sites)| {
                ExternalDependency {
                    kind,
                    symbol,
                    source_module,
                    return_type,
                    param_types,
                    call_sites,
                }
            },
        )
}

pub fn arb_bound_op() -> impl Strategy<Value = BoundOp> {
    prop_oneof![
        Just(BoundOp::Lt),
        Just(BoundOp::Le),
        Just(BoundOp::Gt),
        Just(BoundOp::Ge),
    ]
}

pub fn arb_induction_var() -> impl Strategy<Value = InductionVar> {
    (
        arb_ident(),
        arb_sym_expr(0),
        arb_sym_expr(0),
        arb_sym_expr(0),
        arb_bound_op(),
    )
        .prop_map(|(name, init_expr, step_expr, bound_expr, bound_op)| InductionVar {
            name,
            init_expr,
            step_expr,
            bound_expr,
            bound_op,
        })
}

pub fn arb_loop_info() -> impl Strategy<Value = LoopInfo> {
    (0..100u32, 1..500u32, arb_induction_var()).prop_map(|(loop_id, line, induction_var)| {
        LoopInfo {
            loop_id,
            line,
            induction_var,
        }
    })
}

pub fn arb_function_analysis() -> impl Strategy<Value = FunctionAnalysis> {
    (
        arb_ident(),
        any::<bool>(),
        prop::collection::vec(arb_param_info(), 0..=4),
        prop::collection::vec(arb_branch_info(), 0..=4),
        prop::collection::vec(arb_external_dependency(), 0..=2),
        arb_type_info(1),
        1..500u32,
        1..500u32,
        prop::collection::vec(arb_literal_value(), 0..=3),
    )
        .prop_map(
            |(
                name,
                exported,
                params,
                branches,
                dependencies,
                return_type,
                start_line,
                end_line,
                literals,
            )| {
                FunctionAnalysis {
                    name,
                    exported,
                    params,
                    branches,
                    dependencies,
                    return_type,
                    start_line,
                    end_line: end_line.max(start_line),
                    literals,
                    crypto_boundaries: vec![],
                    loops: vec![],
                    source_file: None,
                }
            },
        )
}

pub fn arb_function_analysis_with_loops() -> impl Strategy<Value = FunctionAnalysis> {
    (arb_function_analysis(), prop::collection::vec(arb_loop_info(), 0..=2)).prop_map(
        |(mut fa, loops)| {
            fa.loops = loops;
            fa
        },
    )
}

pub fn arb_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        prop::collection::vec(arb_ident(), 0..=3)
            .prop_map(|capabilities| Command::Handshake { capabilities }),
        (arb_ident(), proptest::option::of(arb_ident())).prop_map(|(file, function)| {
            Command::Analyze {
                file,
                function,
                project_root: None,
            }
        }),
        (
            arb_ident(),
            arb_ident(),
            prop::collection::vec(arb_mock_config(), 0..=2),
        )
            .prop_map(|(file, function, mocks)| Command::Instrument {
                file,
                function,
                mocks,
                project_root: None,
            }),
        (
            arb_ident(),
            arb_ident(),
            prop::collection::vec(arb_mock_config(), 0..=2),
        )
            .prop_map(|(file, function, mocks)| Command::Prepare {
                file,
                function,
                mocks,
                project_root: None,
            }),
        (
            arb_ident(),
            prop::collection::vec(arb_json_value(), 0..=4),
            prop::collection::vec(arb_mock_config(), 0..=2),
            proptest::option::of(arb_setup_context_stack()),
            proptest::option::of(arb_ident()),
        )
            .prop_map(
                |(function, inputs, mocks, setup_context, prepare_id)| Command::Execute {
                    function,
                    inputs,
                    mocks,
                    setup_context,
                    capture: true,
                    prepare_id,
                }
            ),
        (arb_ident(), arb_ident(), arb_setup_level()).prop_map(|(file, scope, level)| {
            Command::Setup {
                file,
                scope,
                level,
                project_root: None,
                parent_context: None,
            }
        }),
        (arb_ident(), arb_setup_level())
            .prop_map(|(scope, level)| Command::Teardown { scope, level }),
        (arb_ident(), arb_ident(), arb_generator_kind()).prop_map(|(file, name, kind)| {
            Command::Generate {
                file,
                name,
                kind,
                recipe: None,
                project_root: None,
            }
        }),
        Just(Command::Shutdown),
    ]
}

pub fn arb_response_result() -> impl Strategy<Value = ResponseResult> {
    prop_oneof![
        (
            arb_ident(),
            arb_ident(),
            prop::collection::vec(arb_ident(), 0..=3)
        )
            .prop_map(|(frontend_version, language, capabilities)| {
                ResponseResult::Handshake {
                    frontend_version,
                    language,
                    capabilities,
                }
            }),
        prop::collection::vec(arb_function_analysis(), 0..=3)
            .prop_map(|functions| ResponseResult::Analyze { functions }),
        (
            any::<bool>(),
            proptest::option::of(arb_ident()),
            proptest::option::of(1..500u32)
        )
            .prop_map(|(instrumented, output_file, instrumentable_line_count)| {
                ResponseResult::Instrument {
                    instrumented,
                    output_file,
                    instrumentable_line_count,
                }
            }),
        arb_ident().prop_map(|prepare_id| ResponseResult::Prepare { prepare_id }),
        arb_execute_result().prop_map(|er| ResponseResult::Execute(Box::new(er))),
        arb_json_value().prop_map(|ctx| ResponseResult::Setup { setup_context: ctx }),
        Just(ResponseResult::TeardownAck),
        (arb_json_value_non_null(), arb_ident()).prop_map(|(value, generator_id)| {
            ResponseResult::Generate {
                value,
                generator_id,
                recipe: None,
            }
        }),
        Just(ResponseResult::ShutdownAck),
        (arb_error_code(), arb_short_string()).prop_map(|(code, message)| {
            ResponseResult::Error {
                code,
                message,
                details: None,
            }
        }),
    ]
}

pub fn arb_request() -> impl Strategy<Value = Request> {
    (0..1000u64, arb_command()).prop_map(|(id, command)| Request {
        protocol_version: PROTOCOL_VERSION.to_string(),
        id,
        command,
    })
}

pub fn arb_response() -> impl Strategy<Value = Response> {
    (
        0..1000u64,
        arb_response_result(),
        proptest::option::of(arb_timing_summary()),
    )
        .prop_map(|(id, result, timing)| Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            timing,
            result,
        })
}

pub fn arb_timing_summary() -> impl Strategy<Value = TimingSummary> {
    prop::collection::vec(arb_timing_phase_summary(), 0..=4)
        .prop_map(|phases| TimingSummary { phases })
}

pub fn arb_timing_phase_summary() -> impl Strategy<Value = TimingPhaseSummary> {
    (
        arb_ident(),
        0u32..10_000,
        0u32..10_000,
        1u64..10,
        prop::collection::btree_map(arb_ident(), arb_ident(), 0..=3),
    )
        .prop_map(
            |(phase_path, total_ms, self_ms, count, attributes)| TimingPhaseSummary {
                phase_path,
                total_ms: total_ms as f64,
                self_ms: self_ms as f64,
                count,
                attributes,
            },
        )
}

// ---------------------------------------------------------------------------
// Spec / export / invariant strategies
// ---------------------------------------------------------------------------

pub fn arb_comparison_op() -> impl Strategy<Value = ComparisonOp> {
    prop_oneof![
        Just(ComparisonOp::Gt),
        Just(ComparisonOp::Ge),
        Just(ComparisonOp::Lt),
        Just(ComparisonOp::Le),
    ]
}

pub fn arb_invariant_target() -> impl Strategy<Value = InvariantTarget> {
    prop_oneof![Just(InvariantTarget::Input), Just(InvariantTarget::Output),]
}

fn arb_json_path() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(arb_ident(), 0..=3)
}

pub fn arb_invariant_kind() -> impl Strategy<Value = InvariantKind> {
    prop_oneof![
        (arb_json_path(), arb_comparison_op(), -1000i32..1000i32).prop_map(|(path, op, v)| {
            InvariantKind::NumericComparison {
                path,
                op,
                value: f64::from(v),
            }
        }),
        (arb_json_path(), -1000i32..1000i32).prop_map(|(path, v)| InvariantKind::NumericConstant {
            path,
            value: f64::from(v),
        }),
        arb_json_path().prop_map(|path| InvariantKind::NotNull { path }),
        arb_json_path().prop_map(|path| InvariantKind::IsNull { path }),
        arb_json_path().prop_map(|path| InvariantKind::StringNonEmpty { path }),
        (arb_json_path(), arb_comparison_op(), 0..100usize)
            .prop_map(|(path, op, value)| InvariantKind::StringLength { path, op, value }),
        (arb_json_path(), 0..5usize, arb_json_path()).prop_map(
            |(output_path, param_index, input_path)| {
                InvariantKind::OutputEqualsInput {
                    output_path,
                    param_index,
                    input_path,
                }
            }
        ),
        arb_json_path().prop_map(|path| InvariantKind::AlwaysTrue { path }),
        arb_json_path().prop_map(|path| InvariantKind::AlwaysFalse { path }),
    ]
}

pub fn arb_invariant() -> impl Strategy<Value = Invariant> {
    (
        arb_short_string(),
        arb_invariant_target(),
        arb_invariant_kind(),
    )
        .prop_map(|(description, target, kind)| Invariant {
            description,
            target,
            kind,
        })
}

pub fn arb_classified_invariant() -> impl Strategy<Value = ClassifiedInvariant> {
    (
        arb_invariant(),
        arb_invariant_target(),
        arb_short_string(),
        // Use integer-based confidence to avoid NaN/precision issues
        0..=100u32,
        1..100usize,
    )
        .prop_map(|(invariant, target, label, conf_pct, total_count)| {
            let confidence = f64::from(conf_pct) / 100.0;
            let satisfied_count =
                ((confidence * total_count as f64).round() as usize).min(total_count);
            ClassifiedInvariant {
                invariant,
                target,
                label,
                confidence,
                satisfied_count,
                total_count,
            }
        })
}

pub fn arb_branch_step() -> impl Strategy<Value = BranchStep> {
    (0..100u32, any::<bool>()).prop_map(|(branch_id, taken)| BranchStep { branch_id, taken })
}

pub fn arb_branch_path() -> impl Strategy<Value = BranchPath> {
    prop::collection::vec(arb_branch_step(), 0..=5).prop_map(BranchPath)
}

pub fn arb_precondition() -> impl Strategy<Value = Precondition> {
    prop_oneof![
        (0..5usize).prop_map(|i| Precondition::AllPositive { param_index: i }),
        (0..5usize).prop_map(|i| Precondition::AllNegative { param_index: i }),
        (0..5usize).prop_map(|i| Precondition::AllZero { param_index: i }),
        (0..5usize, arb_json_value()).prop_map(|(i, value)| Precondition::AllEqual {
            param_index: i,
            value,
        }),
        (0..5usize, arb_ident()).prop_map(|(i, type_name)| Precondition::SameType {
            param_index: i,
            type_name,
        }),
    ]
}

pub fn arb_postcondition() -> impl Strategy<Value = Postcondition> {
    prop_oneof![
        arb_json_value().prop_map(|value| Postcondition::Returns { value }),
        arb_error_info().prop_map(|error| Postcondition::Throws { error }),
        Just(Postcondition::ReturnsVoid),
    ]
}

pub fn arb_provenance() -> impl Strategy<Value = Provenance> {
    prop_oneof![Just(Provenance::Proven), Just(Provenance::Observed),]
}

pub fn arb_concrete_example() -> impl Strategy<Value = ConcreteExample> {
    (
        prop::collection::vec(arb_json_value(), 0..=4),
        proptest::option::of(arb_json_value_non_null()),
        proptest::option::of(arb_error_info()),
    )
        .prop_map(|(inputs, return_value, thrown_error)| ConcreteExample {
            inputs,
            return_value,
            thrown_error,
        })
}

pub fn arb_spec_class() -> impl Strategy<Value = SpecClass> {
    (
        arb_short_string(),
        arb_branch_path(),
        prop::collection::vec(arb_precondition(), 0..=3),
        arb_postcondition(),
        prop::collection::vec(arb_side_effect(), 0..=2),
        prop::collection::vec(arb_concrete_example(), 1..=3),
        1..50usize,
        arb_provenance(),
        arb_provenance(),
        prop::collection::vec(arb_classified_invariant(), 0..=2),
    )
        .prop_map(
            |(
                label,
                branch_path,
                preconditions,
                postcondition,
                side_effects,
                examples,
                sample_count,
                pre_prov,
                post_prov,
                invariants,
            )| SpecClass {
                label,
                branch_path,
                preconditions,
                postcondition,
                side_effects,
                examples,
                sample_count,
                precondition_provenance: pre_prov,
                postcondition_provenance: post_prov,
                invariants,
            },
        )
}

pub fn arb_function_spec() -> impl Strategy<Value = FunctionSpec> {
    (
        arb_ident(),
        proptest::option::of(arb_short_string()),
        prop::collection::vec(arb_spec_class(), 0..=4),
        0..1000u32,
        0..500usize,
        0..500u32,
        prop::collection::vec(arb_classified_invariant(), 0..=2),
        proptest::option::of(arb_ident()),
    )
        .prop_map(
            |(
                function_name,
                location,
                classes,
                iterations,
                lines_covered,
                total_lines,
                invariants,
                fingerprint,
            )| FunctionSpec {
                function_name,
                location,
                classes,
                iterations,
                lines_covered,
                total_lines,
                invariants,
                fingerprint,
                nondeterministic_fields: vec![],
            },
        )
}

pub fn arb_behavior() -> impl Strategy<Value = Behavior> {
    (
        0..1000u32,
        prop::collection::vec(arb_json_value(), 0..=4),
        proptest::option::of(arb_json_value_non_null()),
        proptest::option::of(arb_error_info()),
        prop::collection::vec(arb_branch_decision(), 0..=5),
        prop::collection::vec(arb_side_effect(), 0..=2),
    )
        .prop_map(
            |(id, input_args, return_value, thrown_error, branch_path, side_effects)| Behavior {
                id,
                input_args,
                return_value,
                thrown_error,
                branch_path,
                side_effects,
                dependency_trace: None,
                mock_values: vec![],
            },
        )
}

pub fn arb_behavior_map() -> impl Strategy<Value = BehaviorMap> {
    (arb_ident(), prop::collection::vec(arb_behavior(), 0..=5)).prop_map(
        |(function_id, behaviors)| BehaviorMap {
            function_id,
            behaviors,
            fingerprint: None,
            nondeterministic_fields: vec![],
        },
    )
}

// ---------------------------------------------------------------------------
// Auto-mock strategies
// ---------------------------------------------------------------------------

pub fn arb_io_category() -> impl Strategy<Value = IoCategory> {
    prop_oneof![
        Just(IoCategory::FileSystem),
        Just(IoCategory::Network),
        Just(IoCategory::Database),
        Just(IoCategory::PureUtility),
        Just(IoCategory::ExternalOther),
    ]
}

pub fn arb_value_source() -> impl Strategy<Value = ValueSource> {
    prop_oneof![
        Just(ValueSource::AutoGenerated),
        Just(ValueSource::UserOverride),
        Just(ValueSource::BehaviorMap),
    ]
}

pub fn arb_mock_param() -> impl Strategy<Value = MockParam> {
    (
        arb_ident(),
        arb_type_info(2),
        arb_io_category(),
        1..10u32,
        arb_value_source(),
    )
        .prop_map(
            |(symbol, return_type, category, call_count_estimate, value_source)| MockParam {
                symbol,
                return_type,
                category,
                call_count_estimate,
                value_source,
            },
        )
}

// ---------------------------------------------------------------------------
// Triage strategies
// ---------------------------------------------------------------------------

pub fn arb_branch_prediction() -> impl Strategy<Value = BranchPrediction> {
    prop_oneof![
        Just(BranchPrediction::Taken),
        Just(BranchPrediction::NotTaken),
        Just(BranchPrediction::Indeterminate),
    ]
}

pub fn arb_triage_verdict() -> impl Strategy<Value = TriageVerdict> {
    prop_oneof![
        Just(TriageVerdict::Skip),
        (1..20usize, 0..10usize).prop_map(|(novel_count, first_novel_depth)| {
            TriageVerdict::Execute {
                novel_count,
                first_novel_depth,
            }
        }),
        Just(TriageVerdict::Indeterminate),
    ]
}

pub fn arb_triage_disable_reason() -> impl Strategy<Value = TriageDisableReason> {
    prop_oneof![
        Just(TriageDisableReason::LowSkipRate),
        Just(TriageDisableReason::HighMisprediction),
    ]
}
