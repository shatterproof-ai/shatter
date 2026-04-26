//! Protocol types for the Shatter Rust frontend.
//!
//! These types match the JSON wire format defined in `shatter-core/src/protocol.rs`.
//! The protocol uses newline-delimited JSON (NDJSON) over stdin/stdout between
//! the core engine and this frontend.
//!
//! Like the Go frontend, we use flat structs with optional fields rather than
//! tagged enums — simpler for a standalone frontend that only needs to parse
//! requests and emit responses.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Well-known complex types beyond primitives and structural types.
/// Matches `ComplexKind` in shatter-core/src/types.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexKind {
    Date,
    DateTime,
    Time,
    Duration,
    RegExp,
    Char,
    Symbol,
    BigInt,
    BigDecimal,
    Complex,
    Rational,
    Range,
    Buffer,
    BitSet,
    Error,
    Option,
    Result,
    Closure,
    Iterator,
    Url,
    IpAddress,
    Uuid,
    Path,
    Money,
    SemVer,
    Email,
    MimeType,
    Color,
    GeoPoint,
    Locale,
    Rune,
    GoByte,
}

/// Describes the type of a value, as reported by a language frontend.
/// Matches `TypeInfo` in shatter-core/src/types.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeInfo {
    Int,
    Float,
    Str,
    Bool,
    Array {
        element: Box<TypeInfo>,
    },
    Object {
        fields: Vec<(String, TypeInfo)>,
    },
    Union {
        variants: Vec<TypeInfo>,
    },
    Nullable {
        inner: Box<TypeInfo>,
    },
    Complex {
        #[serde(rename = "complex_kind")]
        kind: ComplexKind,
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner: Option<Box<TypeInfo>>,
    },
    Opaque {
        label: String,
    },
    Unknown,
}

/// Binary operation kind for symbolic expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOpKind {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    In,
    InstanceOf,
}

/// Unary operation kind for symbolic expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOpKind {
    Not,
    Neg,
    BitwiseNot,
    #[serde(rename = "typeof")]
    TypeOf,
}

/// Constant value in a symbolic expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

/// Symbolic expression tree for branch conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SymExpr {
    Param {
        name: String,
        #[serde(default)]
        path: Vec<String>,
    },
    Const(ConstValue),
    BinOp {
        op: BinOpKind,
        left: Box<SymExpr>,
        right: Box<SymExpr>,
    },
    UnOp {
        op: UnOpKind,
        operand: Box<SymExpr>,
    },
    Call {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receiver: Option<Box<SymExpr>>,
        #[serde(default)]
        args: Vec<SymExpr>,
    },
    Ite {
        condition: Box<SymExpr>,
        then_expr: Box<SymExpr>,
        else_expr: Box<SymExpr>,
    },
    Unknown,
}

/// Branch type in control flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchType {
    If,
    ElseIf,
    Switch,
    Ternary,
    LogicalAnd,
    LogicalOr,
    While,
    For,
}

/// Outcome of an individual condition within a compound decision.
/// Embedded in `BranchDecision::conditions` when MC/DC mode is enabled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionOutcome {
    /// Index within the parent decision's condition list (source order).
    pub condition_index: u32,
    /// Concrete truth value. None if masked by short-circuit.
    pub value: Option<bool>,
    /// Whether short-circuit evaluation prevented observation.
    #[serde(default)]
    pub masked: bool,
    /// Symbolic constraint for this individual condition.
    pub constraint: serde_json::Value,
}

/// A branch point in a function's control flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub id: u32,
    pub line: u32,
    pub condition_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<SymExpr>,
    pub branch_type: BranchType,
}

/// Parameter info for a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: TypeInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

/// Kind of external dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    FunctionCall,
    MethodCall,
    PropertyAccess,
    ModuleImport,
}

/// An external dependency detected in a function body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalDependency {
    pub kind: DependencyKind,
    pub symbol: String,
    pub source_module: String,
    pub return_type: TypeInfo,
    pub param_types: Vec<TypeInfo>,
    pub call_sites: Vec<u32>,
}

/// A literal constant extracted from source code for use as a candidate test input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiteralValue {
    Int { value: i64 },
    Float { value: f64 },
    Str { value: String },
    Bool { value: bool },
    Regex { pattern: String },
}

/// A detected cryptographic API boundary within a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoBoundary {
    pub symbol: String,
    pub source_module: String,
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub param_roles: HashMap<String, String>,
    pub call_sites: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_entropy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_entropy: Option<f64>,
}

/// Comparison operator for induction variable bound checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundOp {
    Lt,
    Le,
    Gt,
    Ge,
}

/// Metadata about a loop induction variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InductionVar {
    pub name: String,
    pub init_expr: SymExpr,
    pub step_expr: SymExpr,
    pub bound_expr: SymExpr,
    pub bound_op: BoundOp,
}

/// A canonical counted loop detected during static analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopInfo {
    pub loop_id: u32,
    pub line: u32,
    pub induction_var: InductionVar,
}

// ---------------------------------------------------------------------------
// Adapter framework types
// ---------------------------------------------------------------------------

/// Confidence level for recognizer-generated hints.
/// Ordered low-to-high so that [`Ord`] gives natural comparison.
/// Matches `nondeterminism::Confidence` in shatter-core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

fn default_confidence() -> Confidence {
    Confidence::High
}

/// Application policy for one execution adapter descriptor.
/// Matches `ExecutionAdapterApply` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAdapterApply {
    Required,
    Auto,
    Suggest,
    Disabled,
}

/// Opaque descriptor for one execution adapter.
/// Matches `ExecutionAdapter` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionAdapter {
    /// Namespaced adapter identifier, for example `rust/async-tokio`.
    pub id: String,
    /// Policy for whether this adapter is required, auto-applied, suggested, or disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<ExecutionAdapterApply>,
    /// Adapter-local opaque options payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// Ordered adapter descriptors that customize how a target should be executed.
/// Matches `ExecutionProfile` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProfile {
    /// Ordered adapter descriptors to apply for this target.
    pub adapters: Vec<ExecutionAdapter>,
}

/// Generic relation between execution adapters used by hinting and policy.
/// Matches `AdapterRelation` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRelation {
    /// Related adapter identifier.
    pub adapter_id: String,
    /// Optional human-readable explanation for the relation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Recognizer-generated hint that an adapter may be relevant for a target.
/// Matches `AdapterHint` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterHint {
    /// Adapter descriptor suggested by the frontend.
    pub adapter: ExecutionAdapter,
    /// Frontend confidence in the hint.
    #[serde(default = "default_confidence")]
    pub confidence: Confidence,
    /// Human-readable evidence explaining why the hint matched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    /// Adapters that should also be present for this hint to make sense.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<AdapterRelation>,
    /// Adapters that conflict with this hint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<AdapterRelation>,
}

/// Describes how a discovered target should be invoked.
/// Matches `InvocationModel` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvocationModel {
    /// The target can be called directly with `FunctionAnalysis.params`.
    #[default]
    Direct,
    /// The target requires adapter-owned invocation.
    Adapter {
        /// Adapter responsible for invoking the target.
        adapter_id: String,
        /// Synthetic parameters accepted by the adapter-owned surface.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        synthetic_params: Vec<ParamInfo>,
        /// Opaque schema or shape descriptor for multi-step scenarios.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scenario_schema: Option<serde_json::Value>,
    },
}

impl InvocationModel {
    /// Returns true when the model is `Direct` (used for `skip_serializing_if`).
    pub fn is_direct(model: &InvocationModel) -> bool {
        matches!(model, InvocationModel::Direct)
    }
}

/// Analysis result for a single function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    pub name: String,
    #[serde(default)]
    pub exported: bool,
    pub params: Vec<ParamInfo>,
    pub branches: Vec<BranchInfo>,
    pub dependencies: Vec<ExternalDependency>,
    pub return_type: TypeInfo,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub literals: Vec<LiteralValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crypto_boundaries: Vec<CryptoBoundary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loops: Vec<LoopInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    /// Whether the function signature is `async fn`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_async: bool,
    /// Recognizer-generated hints that describe relevant execution adapters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_hints: Vec<AdapterHint>,
    /// How this target should be invoked (direct call or adapter-owned).
    #[serde(default, skip_serializing_if = "InvocationModel::is_direct")]
    pub invocation_model: InvocationModel,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// Current protocol version.
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Frontend version.
pub const FRONTEND_VERSION: &str = "0.1.0";

/// Language identifier for this frontend.
pub const FRONTEND_LANGUAGE: &str = "rust";

/// Status describing the result of a single invocation attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Completed,
    CompletedWithFindings,
    Unsupported,
    BuildFailed,
    RuntimeFailed,
    TimedOut,
    SkippedByPolicy,
}

/// Reusable protocol contract for one invocation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationOutcome {
    pub status: OutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thrown_error: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<serde_json::Value>,
}

// Error codes matching protocol/registry.yaml (11 codes).
pub const ERR_FILE_NOT_FOUND: &str = "file_not_found";
pub const ERR_FUNCTION_NOT_FOUND: &str = "function_not_found";
pub const ERR_PARSE_ERROR: &str = "parse_error";
pub const ERR_INSTRUMENTATION_FAILED: &str = "instrumentation_failed";
pub const ERR_EXECUTION_TIMEOUT: &str = "execution_timeout";
pub const ERR_EXECUTION_CRASH: &str = "execution_crash";
pub const ERR_VERSION_MISMATCH: &str = "version_mismatch";
pub const ERR_INVALID_REQUEST: &str = "invalid_request";
pub const ERR_COMPILATION_ERROR: &str = "compilation_error";
pub const ERR_INTERNAL_ERROR: &str = "internal_error";
pub const ERR_NOT_SUPPORTED: &str = "not_supported";

/// All valid error codes for parity testing.
pub const ALL_ERROR_CODES: [&str; 11] = [
    ERR_FILE_NOT_FOUND,
    ERR_FUNCTION_NOT_FOUND,
    ERR_PARSE_ERROR,
    ERR_INSTRUMENTATION_FAILED,
    ERR_EXECUTION_TIMEOUT,
    ERR_EXECUTION_CRASH,
    ERR_VERSION_MISMATCH,
    ERR_INVALID_REQUEST,
    ERR_COMPILATION_ERROR,
    ERR_INTERNAL_ERROR,
    ERR_NOT_SUPPORTED,
];

/// Granularity level for setup/teardown lifecycle management.
/// Matches `SetupLevel` in shatter-core/src/protocol.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupLevel {
    Session,
    File,
    Function,
    Execution,
}

/// A single entry in a setup context stack, associating a lifecycle level
/// with the opaque context value returned by its Setup command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupContextEntry {
    pub level: SetupLevel,
    pub context: serde_json::Value,
}

/// Stack of active setup contexts, ordered from outermost (session) to
/// innermost (execution). Passed to Execute so frontends can restore
/// all active setup state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupContextStack {
    pub contexts: Vec<SetupContextEntry>,
}

/// A request message from the core engine to this frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub protocol_version: String,
    pub id: u64,
    pub command: String,

    // Handshake fields
    #[allow(dead_code)] // will be used when capability negotiation is implemented
    #[serde(default)]
    pub capabilities: Vec<String>,

    // Analyze/Instrument/Execute fields
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub function: Option<String>,

    // Execute fields
    #[serde(default)]
    pub inputs: Vec<serde_json::Value>,
    #[serde(default)]
    pub mocks: Vec<serde_json::Value>,
    /// Opaque handle from a prior prepare command.
    #[serde(default)]
    pub prepare_id: Option<String>,
    /// Harness execution mode for the execute command.
    /// `"bin_only"` (default) uses the standalone/crate-backed dispatch harness.
    /// `"crate_bridge"` injects a feature-gated wrapper module into the library crate
    /// and routes execution through it, enabling calls to crate-private functions.
    #[serde(default)]
    pub harness_mode: Option<String>,
    /// Stack of active setup contexts from enclosing Setup commands, if any.
    #[allow(dead_code)] // carried on Execute requests; handler will forward when execute passes context
    #[serde(default)]
    pub setup_context: Option<SetupContextStack>,

    // Adapter fields
    /// Execution profile with ordered adapter descriptors.
    #[serde(default)]
    pub execution_profile: Option<ExecutionProfile>,

    // Setup fields
    /// Lifecycle level for this setup/teardown (session, file, function, execution).
    #[serde(default)]
    pub level: Option<SetupLevel>,
    /// Scope identifier (function name, file path, or session label).
    #[serde(default)]
    pub scope: Option<String>,
    /// Parent context stack from enclosing setup levels.
    #[serde(default)]
    pub parent_context: Option<SetupContextStack>,

    // Generate fields
    /// Name of the type or parameter to generate a value for.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the generator targets a type name or a parameter name.
    #[allow(dead_code)] // parsed from protocol but not used in dispatch logic yet
    #[serde(default)]
    pub kind: Option<String>,
    /// Opaque recipe state from a prior generate call, enabling stateful generators.
    #[serde(default)]
    pub recipe: Option<serde_json::Value>,

    // Project context
    /// Detected project root directory, if any.
    #[allow(dead_code)] // will be used when full analysis is implemented
    #[serde(default)]
    pub project_root: Option<String>,
}

/// A response message from this frontend to the core engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub protocol_version: String,
    pub id: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<TimingSummary>,

    // Handshake fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,

    // Setup fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_context: Option<serde_json::Value>,

    // Generate fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<serde_json::Value>,

    // Prepare fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_id: Option<String>,

    // Instrument fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumented: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumentable_line_count: Option<u32>,

    // Analyze fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<FunctionAnalysis>>,

    // Execute result fields — flattened to match the core's wire format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thrown_error: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_path: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_executed: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls_to_external: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_constraints: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side_effects: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<serde_json::Value>,

    // Error fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    // Standardized invocation outcome (str-hy9b.A1/A5). Emitted by handle_execute
    // for both success and error responses so cross-frontend consumers can rely
    // on a uniform invocation-result envelope. Status is derived from the
    // executor result: completed | runtime_failed | timed_out on the success
    // path, build_failed on compilation_error, unsupported on non_executable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<InvocationOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TimingSummary {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<TimingPhaseSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TimingPhaseSummary {
    pub phase_path: String,
    pub total_ms: f64,
    #[serde(default)]
    pub self_ms: f64,
    #[serde(default = "default_timing_count")]
    pub count: u64,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attributes: std::collections::BTreeMap<String, String>,
}

fn default_timing_count() -> u64 {
    1
}

impl Response {
    /// Create a base response with protocol version and request ID.
    pub fn base(id: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            status: String::new(),
            timing: None,
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            prepare_id: None,
            code: None,
            message: None,
            outcome: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    #[test]
    fn typeinfo_opaque_round_trips() {
        round_trip(&TypeInfo::Opaque {
            label: "net.Socket".to_string(),
        });
    }

    #[test]
    fn typeinfo_opaque_serializes_with_correct_kind() {
        let ti = TypeInfo::Opaque {
            label: "fs.FileHandle".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&ti).expect("serialize");
        assert_eq!(json["kind"], "opaque");
        assert_eq!(json["label"], "fs.FileHandle");
    }

    #[test]
    fn typeinfo_opaque_inside_array_round_trips() {
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Opaque {
                label: "stream.Readable".to_string(),
            }),
        });
    }

    #[test]
    fn typeinfo_opaque_inside_nullable_round_trips() {
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Opaque {
                label: "channel".to_string(),
            }),
        });
    }

    #[test]
    fn typeinfo_opaque_inside_object_round_trips() {
        round_trip(&TypeInfo::Object {
            fields: vec![
                (
                    "conn".into(),
                    TypeInfo::Opaque {
                        label: "pg.Client".to_string(),
                    },
                ),
                ("name".into(), TypeInfo::Str),
            ],
        });
    }

    #[test]
    fn existing_typeinfo_variants_still_round_trip() {
        round_trip(&TypeInfo::Int);
        round_trip(&TypeInfo::Float);
        round_trip(&TypeInfo::Str);
        round_trip(&TypeInfo::Bool);
        round_trip(&TypeInfo::Unknown);
        round_trip(&TypeInfo::Array {
            element: Box::new(TypeInfo::Int),
        });
        round_trip(&TypeInfo::Nullable {
            inner: Box::new(TypeInfo::Str),
        });
        round_trip(&TypeInfo::Object {
            fields: vec![("x".into(), TypeInfo::Int)],
        });
        round_trip(&TypeInfo::Union {
            variants: vec![TypeInfo::Str, TypeInfo::Int],
        });
    }

    #[test]
    fn typeinfo_complex_still_round_trips() {
        round_trip(&TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: HashMap::new(),
            inner: None,
        });
    }

    #[test]
    fn opaque_in_function_analysis_json_deserializes() {
        // Verify TypeInfo::Opaque works when embedded in a FunctionAnalysis-shaped JSON,
        // parsed as a generic Value and then extracting the type field.
        let json = r#"{"kind": "opaque", "label": "stream.Readable"}"#;
        let param_type: TypeInfo = serde_json::from_str(json).expect("deserialize param type");
        assert_eq!(
            param_type,
            TypeInfo::Opaque {
                label: "stream.Readable".to_string(),
            }
        );

        // Nested inside an object field (simulating a return_type in analysis results)
        let nested_json = r#"{"kind": "object", "fields": [["conn", {"kind": "opaque", "label": "pg.Client"}], ["ready", {"kind": "bool"}]]}"#;
        let nested: TypeInfo = serde_json::from_str(nested_json).expect("deserialize nested");
        if let TypeInfo::Object { fields } = &nested {
            assert_eq!(fields.len(), 2);
            assert_eq!(
                fields[0].1,
                TypeInfo::Opaque {
                    label: "pg.Client".to_string(),
                }
            );
        } else {
            panic!("expected Object, got {:?}", nested);
        }
    }

    #[test]
    fn symexpr_ite_round_trips() {
        round_trip(&SymExpr::Ite {
            condition: Box::new(SymExpr::Param {
                name: "flag".into(),
                path: vec![],
            }),
            then_expr: Box::new(SymExpr::Param {
                name: "b".into(),
                path: vec![],
            }),
            else_expr: Box::new(SymExpr::Param {
                name: "a".into(),
                path: vec![],
            }),
        });
    }

    #[test]
    fn symexpr_ite_deserializes_from_json() {
        let json = r#"{"kind":"ite","condition":{"kind":"param","name":"flag","path":[]},"then_expr":{"kind":"param","name":"b","path":[]},"else_expr":{"kind":"param","name":"a","path":[]}}"#;
        let expr: SymExpr = serde_json::from_str(json).expect("deserialize ite");
        assert!(matches!(expr, SymExpr::Ite { .. }));
    }

    // -- Request deserialization tests for new commands --

    #[test]
    fn setup_request_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":20,"command":"setup","file":"./setup.rs","scope":"processOrder","level":"function"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize setup");
        assert_eq!(req.id, 20);
        assert_eq!(req.command, "setup");
        assert_eq!(req.file.as_deref(), Some("./setup.rs"));
        assert_eq!(req.scope.as_deref(), Some("processOrder"));
        assert_eq!(req.level, Some(SetupLevel::Function));
    }

    #[test]
    fn setup_request_with_parent_context_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":21,"command":"setup","file":"./setup.rs","scope":"myFunc","level":"execution","parent_context":{"contexts":[{"level":"session","context":{"db":"conn_42"}}]}}"#;
        let req: Request =
            serde_json::from_str(json).expect("deserialize setup with parent_context");
        assert_eq!(req.level, Some(SetupLevel::Execution));
        let parent = req.parent_context.expect("parent_context present");
        assert_eq!(parent.contexts.len(), 1);
        assert_eq!(parent.contexts[0].level, SetupLevel::Session);
    }

    #[test]
    fn setup_level_all_variants_round_trip() {
        round_trip(&SetupLevel::Session);
        round_trip(&SetupLevel::File);
        round_trip(&SetupLevel::Function);
        round_trip(&SetupLevel::Execution);
    }

    #[test]
    fn setup_context_entry_round_trips() {
        round_trip(&SetupContextEntry {
            level: SetupLevel::Function,
            context: serde_json::json!({"db": "conn_42"}),
        });
    }

    #[test]
    fn setup_context_stack_round_trips() {
        round_trip(&SetupContextStack {
            contexts: vec![
                SetupContextEntry {
                    level: SetupLevel::Session,
                    context: serde_json::json!({"session_id": "abc"}),
                },
                SetupContextEntry {
                    level: SetupLevel::Function,
                    context: serde_json::json!({"db": "conn_42"}),
                },
            ],
        });
    }

    #[test]
    fn teardown_request_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":22,"command":"teardown","scope":"processOrder","level":"function"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize teardown");
        assert_eq!(req.id, 22);
        assert_eq!(req.command, "teardown");
        assert_eq!(req.scope.as_deref(), Some("processOrder"));
        assert_eq!(req.level, Some(SetupLevel::Function));
    }

    #[test]
    fn generate_request_type_name_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":23,"command":"generate","file":"./gen.ts","name":"User","kind":"type_name"}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize generate type_name");
        assert_eq!(req.id, 23);
        assert_eq!(req.command, "generate");
        assert_eq!(req.file.as_deref(), Some("./gen.ts"));
        assert_eq!(req.name.as_deref(), Some("User"));
        assert_eq!(req.kind.as_deref(), Some("type_name"));
    }

    #[test]
    fn execute_request_with_setup_context_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":25,"command":"execute","function":"fn1","inputs":[1],"mocks":[],"setup_context":{"contexts":[{"level":"session","context":{"db":"conn_42"}}]}}"#;
        let req: Request =
            serde_json::from_str(json).expect("deserialize execute with setup_context");
        let ctx = req.setup_context.expect("setup_context present");
        assert_eq!(ctx.contexts.len(), 1);
        assert_eq!(ctx.contexts[0].level, SetupLevel::Session);
        assert_eq!(
            ctx.contexts[0].context,
            serde_json::json!({"db": "conn_42"})
        );
    }

    #[test]
    fn execute_request_without_setup_context_defaults_to_none() {
        let json = r#"{"protocol_version":"0.1.0","id":26,"command":"execute","function":"fn1","inputs":[],"mocks":[]}"#;
        let req: Request =
            serde_json::from_str(json).expect("deserialize execute without setup_context");
        assert_eq!(req.setup_context, None);
    }

    // -- Response round-trip tests for new statuses --

    #[test]
    fn setup_response_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 20,
            status: "setup".to_string(),
            timing: None,
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: Some(serde_json::json!({"db_handle": "conn_42"})),
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: None,
            message: None,
            prepare_id: None,
            outcome: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn generate_response_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 22,
            status: "generate".to_string(),
            timing: None,
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: Some(serde_json::json!({"id": 1, "name": "Alice"})),
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: None,
            message: None,
            prepare_id: None,
            outcome: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn error_response_still_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 99,
            status: "error".to_string(),
            timing: None,
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: Some("internal_error".to_string()),
            message: Some("something broke".to_string()),
            prepare_id: None,
            outcome: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn handshake_response_still_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 1,
            status: "handshake".to_string(),
            timing: None,
            frontend_version: Some(FRONTEND_VERSION.to_string()),
            language: Some(FRONTEND_LANGUAGE.to_string()),
            capabilities: Some(vec!["analyze".to_string()]),
            setup_context: None,
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: None,
            message: None,
            prepare_id: None,
            outcome: None,
        };
        round_trip(&resp);
    }

    // -- LiteralValue tests --

    #[test]
    fn literal_value_all_variants_round_trip() {
        round_trip(&LiteralValue::Int { value: 42 });
        round_trip(&LiteralValue::Float { value: 3.14 });
        round_trip(&LiteralValue::Str {
            value: "express".into(),
        });
        round_trip(&LiteralValue::Bool { value: true });
        round_trip(&LiteralValue::Regex {
            pattern: "\\d+".into(),
        });
    }

    #[test]
    fn function_analysis_with_literals_round_trips() {
        round_trip(&FunctionAnalysis {
            name: "classify".into(),
            exported: true,
            params: vec![ParamInfo {
                name: "s".into(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Str,
            start_line: 1,
            end_line: 10,
            literals: vec![
                LiteralValue::Str {
                    value: "express".into(),
                },
                LiteralValue::Regex {
                    pattern: "\\d{5}".into(),
                },
            ],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            is_async: false,
            adapter_hints: vec![],
            invocation_model: InvocationModel::default(),
        });
    }

    #[test]
    fn function_analysis_empty_literals_omits_field_in_json() {
        let fa = FunctionAnalysis {
            name: "stub".into(),
            exported: false,
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
        };
        let json = serde_json::to_value(&fa).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("literals"),
            "empty literals should not appear in JSON"
        );
    }

    #[test]
    fn function_analysis_without_literals_field_deserializes_as_empty() {
        let json = r#"{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}"#;
        let fa: FunctionAnalysis = serde_json::from_str(json).expect("deserialize");
        assert!(fa.literals.is_empty());
    }

    #[test]
    fn crypto_boundary_round_trips() {
        let mut roles = HashMap::new();
        roles.insert("0".to_string(), "algorithm".to_string());
        roles.insert("1".to_string(), "key".to_string());

        round_trip(&CryptoBoundary {
            symbol: "createDecipheriv".into(),
            source_module: "crypto".into(),
            direction: "decrypt".into(),
            output: Some("plaintext".into()),
            confidence: Some("high".into()),
            param_roles: roles,
            call_sites: vec![5, 12],
            input_entropy: None,
            output_entropy: None,
        });
    }

    #[test]
    fn crypto_boundary_heuristic_round_trips() {
        round_trip(&CryptoBoundary {
            symbol: "encryptPayload".into(),
            source_module: "my-custom-lib".into(),
            direction: "encrypt".into(),
            output: None,
            confidence: Some("medium".into()),
            param_roles: HashMap::new(),
            call_sites: vec![42],
            input_entropy: None,
            output_entropy: None,
        });
    }

    #[test]
    fn empty_crypto_boundaries_omitted_from_json() {
        let fa = FunctionAnalysis {
            name: "stub".into(),
            exported: false,
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
        };
        let json = serde_json::to_value(&fa).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("crypto_boundaries"),
            "empty crypto_boundaries should not appear in JSON"
        );
    }

    #[test]
    fn missing_crypto_boundaries_defaults_to_empty() {
        let json = r#"{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}"#;
        let fa: FunctionAnalysis = serde_json::from_str(json).expect("deserialize");
        assert!(fa.crypto_boundaries.is_empty());
    }

    #[test]
    fn timing_summary_round_trips() {
        let summary = TimingSummary {
            phases: vec![
                TimingPhaseSummary {
                    phase_path: "analyze.total".to_string(),
                    total_ms: 42.5,
                    self_ms: 10.0,
                    count: 1,
                    attributes: std::collections::BTreeMap::new(),
                },
                TimingPhaseSummary {
                    phase_path: "analyze.parse".to_string(),
                    total_ms: 32.5,
                    self_ms: 32.5,
                    count: 1,
                    attributes: std::collections::BTreeMap::from([
                        ("file".to_string(), "test.rs".to_string()),
                    ]),
                },
            ],
        };
        round_trip(&summary);
    }

    #[test]
    fn response_with_timing_round_trips() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 5,
            status: "analyze".to_string(),
            timing: Some(TimingSummary {
                phases: vec![TimingPhaseSummary {
                    phase_path: "analyze.total".to_string(),
                    total_ms: 15.0,
                    self_ms: 15.0,
                    count: 1,
                    attributes: std::collections::BTreeMap::new(),
                }],
            }),
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: None,
            message: None,
            prepare_id: None,
            outcome: None,
        };
        round_trip(&resp);
    }

    #[test]
    fn timing_omitted_when_none() {
        let resp = Response {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: 6,
            status: "analyze".to_string(),
            timing: None,
            frontend_version: None,
            language: None,
            capabilities: None,
            setup_context: None,
            value: None,
            generator_id: None,
            recipe: None,
            instrumented: None,
            output_file: None,
            instrumentable_line_count: None,
            functions: None,
            return_value: None,
            thrown_error: None,
            branch_path: None,
            lines_executed: None,
            calls_to_external: None,
            path_constraints: None,
            side_effects: None,
            performance: None,
            code: None,
            message: None,
            prepare_id: None,
            outcome: None,
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("timing"),
            "timing should be omitted when None"
        );
    }

    #[test]
    fn empty_timing_phases_omitted() {
        let summary = TimingSummary { phases: vec![] };
        let json = serde_json::to_value(&summary).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("phases"),
            "empty phases should be omitted"
        );
    }

    // ── Adapter framework round-trip tests ──

    #[test]
    fn confidence_round_trips() {
        round_trip(&Confidence::Low);
        round_trip(&Confidence::Medium);
        round_trip(&Confidence::High);
    }

    #[test]
    fn confidence_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_value(Confidence::High).unwrap(),
            serde_json::json!("high")
        );
        assert_eq!(
            serde_json::to_value(Confidence::Medium).unwrap(),
            serde_json::json!("medium")
        );
        assert_eq!(
            serde_json::to_value(Confidence::Low).unwrap(),
            serde_json::json!("low")
        );
    }

    #[test]
    fn execution_adapter_apply_round_trips() {
        round_trip(&ExecutionAdapterApply::Required);
        round_trip(&ExecutionAdapterApply::Auto);
        round_trip(&ExecutionAdapterApply::Suggest);
        round_trip(&ExecutionAdapterApply::Disabled);
    }

    #[test]
    fn execution_adapter_round_trips() {
        round_trip(&ExecutionAdapter {
            id: "rust/async-tokio".into(),
            apply: Some(ExecutionAdapterApply::Auto),
            options: None,
        });
        round_trip(&ExecutionAdapter {
            id: "rust/framework/axum-handler".into(),
            apply: None,
            options: Some(serde_json::json!({"port": 8080})),
        });
    }

    #[test]
    fn execution_profile_round_trips() {
        round_trip(&ExecutionProfile {
            adapters: vec![
                ExecutionAdapter {
                    id: "rust/async-tokio".into(),
                    apply: Some(ExecutionAdapterApply::Auto),
                    options: None,
                },
            ],
        });
    }

    #[test]
    fn adapter_relation_round_trips() {
        round_trip(&AdapterRelation {
            adapter_id: "rust/async-tokio".into(),
            reason: Some("requires async runtime".into()),
        });
        round_trip(&AdapterRelation {
            adapter_id: "rust/other".into(),
            reason: None,
        });
    }

    #[test]
    fn adapter_hint_round_trips() {
        round_trip(&AdapterHint {
            adapter: ExecutionAdapter {
                id: "rust/async-tokio".into(),
                apply: Some(ExecutionAdapterApply::Auto),
                options: None,
            },
            confidence: Confidence::High,
            reasons: vec!["function is async".into()],
            requirements: vec![],
            conflicts: vec![],
        });
    }

    #[test]
    fn invocation_model_direct_round_trips() {
        round_trip(&InvocationModel::Direct);
    }

    #[test]
    fn invocation_model_direct_json_shape() {
        let json = serde_json::to_value(InvocationModel::Direct).unwrap();
        assert_eq!(json["kind"], "direct");
    }

    #[test]
    fn invocation_model_adapter_round_trips() {
        round_trip(&InvocationModel::Adapter {
            adapter_id: "rust/async-tokio".into(),
            synthetic_params: vec![],
            scenario_schema: None,
        });
    }

    #[test]
    fn invocation_model_adapter_json_shape() {
        let model = InvocationModel::Adapter {
            adapter_id: "rust/async-tokio".into(),
            synthetic_params: vec![],
            scenario_schema: None,
        };
        let json = serde_json::to_value(&model).unwrap();
        assert_eq!(json["kind"], "adapter");
        assert_eq!(json["adapter_id"], "rust/async-tokio");
    }

    #[test]
    fn function_analysis_omits_direct_invocation_model() {
        let fa = FunctionAnalysis {
            name: "stub".into(),
            exported: false,
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
            invocation_model: InvocationModel::Direct,
        };
        let json = serde_json::to_value(&fa).unwrap();
        let obj = json.as_object().unwrap();
        assert!(
            !obj.contains_key("invocation_model"),
            "Direct invocation_model should be omitted from JSON"
        );
        assert!(
            !obj.contains_key("adapter_hints"),
            "empty adapter_hints should be omitted from JSON"
        );
        assert!(
            !obj.contains_key("is_async"),
            "is_async: false should be omitted from JSON"
        );
    }

    #[test]
    fn function_analysis_includes_adapter_fields_when_present() {
        let fa = FunctionAnalysis {
            name: "my_async".into(),
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
            is_async: true,
            adapter_hints: vec![AdapterHint {
                adapter: ExecutionAdapter {
                    id: "rust/async-tokio".into(),
                    apply: Some(ExecutionAdapterApply::Auto),
                    options: None,
                },
                confidence: Confidence::High,
                reasons: vec!["function is async".into()],
                requirements: vec![],
                conflicts: vec![],
            }],
            invocation_model: InvocationModel::Adapter {
                adapter_id: "rust/async-tokio".into(),
                synthetic_params: vec![],
                scenario_schema: None,
            },
        };
        let json = serde_json::to_value(&fa).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("is_async"), "is_async should be present when true");
        assert!(obj.contains_key("adapter_hints"), "adapter_hints should be present when non-empty");
        assert!(obj.contains_key("invocation_model"), "invocation_model should be present when Adapter");
        round_trip(&fa);
    }

    #[test]
    fn request_with_execution_profile_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":99,"command":"execute","function":"f","inputs":[],"mocks":[],"execution_profile":{"adapters":[{"id":"rust/async-tokio","apply":"auto"}]}}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize");
        let profile = req.execution_profile.expect("execution_profile present");
        assert_eq!(profile.adapters.len(), 1);
        assert_eq!(profile.adapters[0].id, "rust/async-tokio");
        assert_eq!(profile.adapters[0].apply, Some(ExecutionAdapterApply::Auto));
    }

    #[test]
    fn request_without_execution_profile_defaults_to_none() {
        let json = r#"{"protocol_version":"0.1.0","id":100,"command":"execute","function":"f","inputs":[],"mocks":[]}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.execution_profile, None);
    }

    #[test]
    fn outcome_status_round_trips() {
        round_trip(&OutcomeStatus::Completed);
        round_trip(&OutcomeStatus::CompletedWithFindings);
        round_trip(&OutcomeStatus::Unsupported);
        round_trip(&OutcomeStatus::BuildFailed);
        round_trip(&OutcomeStatus::RuntimeFailed);
        round_trip(&OutcomeStatus::TimedOut);
        round_trip(&OutcomeStatus::SkippedByPolicy);
    }

    #[test]
    fn invocation_outcome_round_trips() {
        round_trip(&InvocationOutcome {
            status: OutcomeStatus::CompletedWithFindings,
            short_reason: Some("completed with findings".into()),
            return_value: Some(serde_json::json!({"ok": true})),
            thrown_error: Some(serde_json::json!({
                "error_type": "warning",
                "message": "partial support",
                "stack": null
            })),
            side_effects: vec![serde_json::json!({
                "kind": "console_output",
                "level": "warn",
                "message": "degraded"
            })],
        });
    }
}
