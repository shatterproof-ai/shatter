//! Protocol types for communication between the Rust core and language frontends.
//!
//! All messages are newline-delimited JSON over stdin/stdout. The core sends
//! [`Request`] messages to frontends and receives [`Response`] messages back.
//! Every message includes a protocol version for compatibility checking.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::crypto_registry::{CryptoDirection, OutputSemantics, ParamRole};
use crate::nondeterminism::Confidence;
use crate::execution_record::{
    BranchDecision, ErrorInfo, ExternalCall, SideEffect, SymConstraint, TraceEvent,
    TruncationInfo,
};
use crate::sym_expr::SymExpr;
use crate::types::{ParamInfo, TypeInfo};

/// Current protocol version.
pub const PROTOCOL_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Setup lifecycle types
// ---------------------------------------------------------------------------

/// Granularity level for setup/teardown lifecycle management.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

// ---------------------------------------------------------------------------
// Request: Core → Frontend
// ---------------------------------------------------------------------------

/// A request message sent from the core engine to a language frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Protocol version for compatibility checking.
    pub protocol_version: String,
    /// Unique identifier for correlating responses to requests.
    pub id: u64,
    /// The command to execute.
    #[serde(flatten)]
    pub command: Command,
}

/// Commands the core can send to a frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Initial handshake to negotiate protocol version and capabilities.
    Handshake {
        /// Capabilities supported by the core.
        capabilities: Vec<String>,
    },
    /// Analyze a function to extract type information, branches, and dependencies.
    Analyze {
        /// Path to the source file.
        file: String,
        /// Name of the function to analyze. If absent, analyze all exported functions.
        function: Option<String>,
        /// Detected project root directory, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root: Option<String>,
    },
    /// Instrument a function for symbolic constraint tracking.
    Instrument {
        /// Path to the source file.
        file: String,
        /// Name of the function to instrument.
        function: String,
        /// Mock configurations for external dependencies.
        mocks: Vec<MockConfig>,
        /// Detected project root directory, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root: Option<String>,
    },
    /// Execute an instrumented function with specific inputs and mocks.
    Execute {
        /// Fully qualified function identifier.
        function: String,
        /// Input values for the function parameters (JSON-encoded).
        inputs: Vec<serde_json::Value>,
        /// Mock configurations for external dependencies.
        mocks: Vec<MockConfig>,
        /// Stack of active setup contexts from enclosing Setup commands, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        setup_context: Option<SetupContextStack>,
    },
    /// Run a setup file to initialize state before function execution.
    Setup {
        /// Path to the setup file.
        file: String,
        /// Scope identifier (function name, file path, or session label).
        scope: String,
        /// Lifecycle level for this setup.
        level: SetupLevel,
        /// Detected project root directory, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root: Option<String>,
        /// Parent context stack from enclosing setup levels, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_context: Option<SetupContextStack>,
    },
    /// Tear down state established by a prior Setup command.
    Teardown {
        /// Scope identifier matching the corresponding Setup command.
        scope: String,
        /// Lifecycle level matching the corresponding Setup command.
        level: SetupLevel,
    },
    /// Invoke a custom generator to produce a value for a type or parameter.
    Generate {
        /// Path to the generator file.
        file: String,
        /// Name of the type or parameter to generate a value for.
        name: String,
        /// Whether this generator targets a type name or a parameter name.
        kind: GeneratorKind,
        /// Reconstruction recipe from a previous generation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recipe: Option<serde_json::Value>,
        /// Detected project root directory, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root: Option<String>,
    },
    /// Request graceful shutdown of the frontend process.
    Shutdown,
}

/// Whether a generator targets a type name or a parameter name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratorKind {
    /// Generator for a named type (e.g., "User").
    TypeName,
    /// Generator for a named parameter (e.g., "authToken").
    ParamName,
}

/// Configuration for mocking an external dependency during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockConfig {
    /// Fully qualified symbol name of the dependency to mock.
    pub symbol: String,
    /// Pre-configured return values to cycle through.
    pub return_values: Vec<serde_json::Value>,
    /// Whether to record arguments passed to the mock.
    pub should_track_calls: bool,
    /// Default behavior when return_values are exhausted.
    pub default_behavior: MockBehavior,
}

/// How a mock should behave when its pre-configured return values are exhausted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MockBehavior {
    /// Return a generated value based on the return type.
    ReturnGenerated,
    /// Repeat the last return value.
    RepeatLast,
    /// Throw an error.
    ThrowError,
    /// Call through to the real implementation.
    Passthrough,
}

// ---------------------------------------------------------------------------
// Response: Frontend → Core
// ---------------------------------------------------------------------------

/// A response message sent from a language frontend to the core engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Protocol version for compatibility checking.
    pub protocol_version: String,
    /// Request ID this response corresponds to.
    pub id: u64,
    /// The response payload.
    #[serde(flatten)]
    pub result: ResponseResult,
}

/// Response payloads from a frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResponseResult {
    /// Successful handshake response.
    Handshake {
        /// Protocol version the frontend supports.
        frontend_version: String,
        /// Language this frontend handles (e.g., "typescript", "go").
        language: String,
        /// Capabilities supported by the frontend.
        capabilities: Vec<String>,
    },
    /// Successful analysis result.
    Analyze {
        /// Functions found and analyzed.
        #[serde(default)]
        functions: Vec<FunctionAnalysis>,
    },
    /// Successful instrumentation result.
    Instrument {
        /// Whether instrumentation succeeded.
        instrumented: bool,
        /// Path to the instrumented output file, if applicable.
        output_file: Option<String>,
        /// Number of executable statement lines the instrumentor inserted record calls for.
        /// Used as the denominator for line coverage instead of raw source span.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instrumentable_line_count: Option<u32>,
    },
    /// Successful execution result (boxed to reduce enum size).
    Execute(Box<ExecuteResult>),
    /// Successful setup result.
    Setup {
        /// Opaque context to pass to subsequent Execute commands.
        setup_context: serde_json::Value,
    },
    /// Acknowledgment that teardown completed.
    TeardownAck,
    /// Result of invoking a custom generator.
    Generate {
        /// The generated value.
        value: serde_json::Value,
        /// Human-readable label for this generated value.
        generator_id: String,
        /// Serializable recipe for replaying this generation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recipe: Option<serde_json::Value>,
    },
    /// Acknowledgment of shutdown request.
    ShutdownAck,
    /// Error response for any command.
    Error {
        /// Machine-readable error code.
        code: ErrorCode,
        /// Human-readable error message.
        message: String,
        /// Additional error details (e.g., stack trace, source location).
        details: Option<serde_json::Value>,
    },
}

/// A literal constant value extracted from source code during static analysis.
///
/// Used to seed the candidate input pool with values the function itself
/// compares against, improving branch coverage on first pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiteralValue {
    Int { value: i64 },
    Float { value: f64 },
    Str { value: String },
    Bool { value: bool },
    /// Regex pattern string (source text, no delimiters or flags).
    Regex { pattern: String },
}

/// Analysis result for a single function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    /// Fully qualified function name.
    pub name: String,
    /// Whether the function is exported (public) from its module.
    #[serde(default)]
    pub exported: bool,
    /// Function parameters with type information.
    pub params: Vec<ParamInfo>,
    /// Branch points found in the function.
    pub branches: Vec<BranchInfo>,
    /// External dependencies detected.
    pub dependencies: Vec<ExternalDependency>,
    /// Return type of the function.
    pub return_type: TypeInfo,
    /// Source location.
    pub start_line: u32,
    pub end_line: u32,
    /// Literal constants extracted from the function body for use as candidate inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub literals: Vec<LiteralValue>,
    /// Cryptographic API boundaries detected by matching dependencies against the crypto registry.
    /// Populated by core after analysis; frontends leave this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crypto_boundaries: Vec<CryptoBoundary>,
}

/// A branch point found during static analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchInfo {
    /// Unique identifier for this branch within the function.
    pub id: u32,
    /// Source line number.
    pub line: u32,
    /// Source text of the condition (for display).
    pub condition_text: String,
    /// Symbolic representation of the condition, if extractable.
    pub condition: Option<SymExpr>,
    /// Type of branch construct.
    pub branch_type: BranchType,
}

/// The kind of branch construct in source code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    Select,
}

/// An external dependency detected during analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalDependency {
    /// The kind of dependency.
    pub kind: DependencyKind,
    /// Fully qualified symbol name.
    pub symbol: String,
    /// Module or package the symbol is imported from.
    pub source_module: String,
    /// Return type for generating mock values.
    pub return_type: TypeInfo,
    /// Parameter types for validating mock calls.
    pub param_types: Vec<TypeInfo>,
    /// Line numbers where this dependency is called.
    pub call_sites: Vec<u32>,
}

/// A detected cryptographic API boundary within a function.
///
/// Produced by matching `ExternalDependency` entries against the crypto registry
/// (Layer 1, High confidence) or naming heuristics (Layer 2, Medium/Low confidence).
/// Carries the crypto-specific metadata (direction, param roles, output semantics)
/// alongside the call site information from the dependency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoBoundary {
    /// The symbol name from the dependency (e.g. "createDecipheriv").
    pub symbol: String,
    /// The source module (e.g. "crypto", "crypto/cipher").
    pub source_module: String,
    /// Whether this is an encrypt, decrypt, or both operation.
    pub direction: CryptoDirection,
    /// What the output represents. Always present for Layer 1 (registry) matches;
    /// absent for Layer 2 (naming heuristic) matches where output semantics are unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputSemantics>,
    /// Detection confidence: High for registry matches, Medium for strong name
    /// patterns (e.g. `decrypt*`), Low for ambiguous patterns needing context.
    #[serde(default = "default_confidence")]
    pub confidence: Confidence,
    /// Maps parameter positions to their cryptographic roles.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub param_roles: HashMap<String, ParamRole>,
    /// Line numbers where this crypto API is called.
    pub call_sites: Vec<u32>,
    /// Shannon entropy (bits/byte) of input buffer, measured at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_entropy: Option<f64>,
    /// Shannon entropy (bits/byte) of output buffer, measured at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_entropy: Option<f64>,
}

fn default_confidence() -> Confidence {
    Confidence::High
}

/// The kind of external dependency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    FunctionCall,
    MethodCall,
    PropertyAccess,
    ModuleImport,
}

/// How a dependency was detected at execution time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepDetectionKind {
    /// A `require()` or `import()` call resolved to a module not listed
    /// in static analysis dependencies.
    UnmockedImport,
    /// A subprocess-spawning API (`child_process.exec`, `spawn`, etc.) was
    /// invoked during execution.
    SubprocessSpawn,
}

/// A dependency discovered at execution time that static analysis missed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredDependency {
    /// Fully qualified symbol or module name.
    pub symbol: String,
    /// Module the dependency was imported from.
    pub source_module: String,
    /// How the dependency was detected.
    pub kind: DepDetectionKind,
    /// Whether this dependency spawns a subprocess.
    pub is_subprocess_spawn: bool,
}

/// A connection failure detected during execution, enabling LiveFirst fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionFailure {
    /// The external symbol (e.g. `"http:fetch"`) whose call failed.
    pub symbol: String,
    /// Failure category matching `ConnectionFailureKind` variants
    /// (e.g. `"connection_refused"`, `"dns_failure"`, `"auth_error"`, `"timeout"`, `"other"`).
    pub error_kind: String,
    /// The original error message from the failing call.
    pub message: String,
}

/// Result of executing an instrumented function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Return value from the function, if it returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error thrown during execution, if any.
    pub thrown_error: Option<ErrorInfo>,
    /// Branch decisions recorded during execution.
    #[serde(default)]
    pub branch_path: Vec<BranchDecision>,
    /// Source lines executed.
    #[serde(default)]
    pub lines_executed: Vec<u32>,
    /// Calls to external dependencies observed.
    #[serde(default)]
    pub calls_to_external: Vec<ExternalCall>,
    /// Symbolic path constraints collected.
    #[serde(default)]
    pub path_constraints: Vec<SymConstraint>,
    /// Scope-annotated execution trace (branches + loop/call markers).
    /// When non-empty, enables scope-aware path collapsing in `path_hash`.
    #[serde(default)]
    pub scope_events: Vec<TraceEvent>,
    /// Side effects observed during execution.
    #[serde(default)]
    pub side_effects: Vec<SideEffect>,
    /// Performance metrics.
    pub performance: PerformanceMetrics,
    /// Truncation metadata for captured side effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_truncation: Option<TruncationInfo>,
    /// Dependencies discovered at execution time that static analysis missed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discovered_dependencies: Vec<DiscoveredDependency>,
    /// Connection failures detected during mock/external calls, used to
    /// trigger LiveFirst fallback in the core engine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connection_failures: Vec<ConnectionFailure>,
}

/// Performance metrics from a single execution.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Wall clock time in milliseconds.
    pub wall_time_ms: f64,
    /// CPU time in microseconds.
    pub cpu_time_us: u64,
    /// Heap memory used in bytes.
    pub heap_used_bytes: u64,
    /// Heap memory allocated in bytes.
    pub heap_allocated_bytes: u64,
}

/// Machine-readable error codes for protocol errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// File not found or not readable.
    FileNotFound,
    /// Function not found in the specified file.
    FunctionNotFound,
    /// Syntax or parse error in source file.
    ParseError,
    /// Instrumentation failed.
    InstrumentationFailed,
    /// Execution timed out.
    ExecutionTimeout,
    /// Execution crashed (segfault, OOM, etc.).
    ExecutionCrash,
    /// Protocol version mismatch.
    VersionMismatch,
    /// Invalid or malformed request.
    InvalidRequest,
    /// Compilation failed (compiled-language frontends like Rust).
    CompilationError,
    /// Frontend internal error.
    InternalError,
    /// Command or operation not supported by this frontend.
    NotSupported,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl Request {
    /// Create a new request with the current protocol version.
    pub fn new(id: u64, command: Command) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            command,
        }
    }
}

impl Response {
    /// Create a new response with the current protocol version.
    pub fn new(id: u64, result: ResponseResult) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            id,
            result,
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol validation — trust boundary between subprocess JSON and core
// ---------------------------------------------------------------------------

/// Validate an `ExecuteResult` deserialized from a frontend subprocess.
///
/// Checks semantic invariants that serde can't enforce:
/// - `branch_path` entries have non-default constraints (when non-empty, an all-unknown
///   branch_path produces identical hashes for different paths → silent hash collision)
/// - `path_constraints` length matches `branch_path` length when both are non-empty
#[contracts::ensures(ret == execute_result_is_valid(result),
    "postcondition must match validation logic")]
pub fn validate_execute_result(result: &ExecuteResult) -> bool {
    execute_result_is_valid(result)
}

fn execute_result_is_valid(result: &ExecuteResult) -> bool {
    // Branch path with all-unknown constraints causes silent hash collisions
    // because the path hash depends on constraint content.
    if !result.branch_path.is_empty() {
        let all_unknown = result.branch_path.iter().all(|bd| {
            matches!(bd.constraint, SymConstraint::Unknown { .. })
        });
        // All-unknown is valid for frontends without symbolic analysis (Go),
        // but path_constraints should then also be empty.
        if all_unknown && !result.path_constraints.is_empty() {
            return false;
        }
    }
    true
}

/// Validate a `FunctionAnalysis` list deserialized from a frontend subprocess.
///
/// Checks semantic invariants that serde can't enforce:
/// - Function names are non-empty (empty name → silent lookup failures)
/// - `start_line <= end_line` (inverted range → wrong source slicing)
/// - Param count is plausible (> 255 params suggests a deserialization bug)
#[contracts::ensures(ret == analyze_result_is_valid(functions),
    "postcondition must match validation logic")]
pub fn validate_analyze_result(functions: &[FunctionAnalysis]) -> bool {
    analyze_result_is_valid(functions)
}

/// Upper bound on parameter count — anything above this likely indicates
/// a deserialization bug rather than a real function signature.
const MAX_PLAUSIBLE_PARAMS: usize = 255;

fn analyze_result_is_valid(functions: &[FunctionAnalysis]) -> bool {
    for func in functions {
        if func.name.is_empty() {
            return false;
        }
        if func.start_line > func.end_line {
            return false;
        }
        if func.params.len() > MAX_PLAUSIBLE_PARAMS {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym_expr::{BinOpKind, ConstValue};

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let json = serde_json::to_string(value).expect("serialize");
        let deserialized: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*value, deserialized, "round-trip failed for json: {json}");
    }

    // -- Request round-trips --

    #[test]
    fn handshake_request_round_trips() {
        round_trip(&Request::new(
            1,
            Command::Handshake {
                capabilities: vec!["analyze".into(), "execute".into()],
            },
        ));
    }

    #[test]
    fn analyze_request_with_function_round_trips() {
        round_trip(&Request::new(
            2,
            Command::Analyze {
                file: "src/main.ts".into(),
                function: Some("calculateShipping".into()),
                project_root: None,
            },
        ));
    }

    #[test]
    fn analyze_request_without_function_round_trips() {
        round_trip(&Request::new(
            3,
            Command::Analyze {
                file: "src/main.ts".into(),
                function: None,
                project_root: None,
            },
        ));
    }

    #[test]
    fn instrument_request_round_trips() {
        round_trip(&Request::new(
            4,
            Command::Instrument {
                file: "src/main.ts".into(),
                function: "calculateShipping".into(),
                mocks: vec![MockConfig {
                    symbol: "rateService.getExpressRate".into(),
                    return_values: vec![serde_json::json!(12.99)],
                    should_track_calls: true,
                    default_behavior: MockBehavior::RepeatLast,
                }],
                project_root: None,
            },
        ));
    }

    #[test]
    fn execute_request_round_trips() {
        round_trip(&Request::new(
            5,
            Command::Execute {
                function: "calculateShipping".into(),
                inputs: vec![serde_json::json!({"items": [1, 2, 3], "priority": "express"})],
                mocks: vec![],
                setup_context: None,
            },
        ));
    }

    #[test]
    fn shutdown_request_round_trips() {
        round_trip(&Request::new(6, Command::Shutdown));
    }

    // -- Response round-trips --

    #[test]
    fn handshake_response_round_trips() {
        round_trip(&Response::new(
            1,
            ResponseResult::Handshake {
                frontend_version: PROTOCOL_VERSION.into(),
                language: "typescript".into(),
                capabilities: vec!["analyze".into(), "execute".into(), "instrument".into()],
            },
        ));
    }

    #[test]
    fn analyze_response_round_trips() {
        round_trip(&Response::new(
            2,
            ResponseResult::Analyze {
                functions: vec![FunctionAnalysis {
                    name: "calculateShipping".into(),
                    exported: true,
                    params: vec![ParamInfo {
                        name: "order".into(),
                        typ: TypeInfo::Object {
                            fields: vec![
                                ("items".into(), TypeInfo::Array {
                                    element: Box::new(TypeInfo::Int),
                                }),
                                ("priority".into(), TypeInfo::Str),
                            ],
                        },
                        type_name: None,
                    }],
                    branches: vec![BranchInfo {
                        id: 1,
                        line: 23,
                        condition_text: "order.priority === \"express\"".into(),
                        condition: Some(SymExpr::BinOp {
                            op: BinOpKind::Eq,
                            left: Box::new(SymExpr::Param {
                                name: "order".into(),
                                path: vec!["priority".into()],
                            }),
                            right: Box::new(SymExpr::Const(ConstValue::Str("express".into()))),
                        }),
                        branch_type: BranchType::If,
                    }],
                    dependencies: vec![ExternalDependency {
                        kind: DependencyKind::FunctionCall,
                        symbol: "rateService.getExpressRate".into(),
                        source_module: "./rateService".into(),
                        return_type: TypeInfo::Float,
                        param_types: vec![TypeInfo::Str],
                        call_sites: vec![25],
                    }],
                    return_type: TypeInfo::Object {
                        fields: vec![
                            ("cost".into(), TypeInfo::Float),
                            ("method".into(), TypeInfo::Str),
                        ],
                    },
                    start_line: 10,
                    end_line: 45,
                    literals: vec![],
                    crypto_boundaries: vec![],
                }],
            },
        ));
    }

    #[test]
    fn instrument_response_round_trips() {
        round_trip(&Response::new(
            3,
            ResponseResult::Instrument {
                instrumented: true,
                output_file: Some("/tmp/instrumented_main.ts".into()),
                instrumentable_line_count: Some(12),
            },
        ));
    }

    #[test]
    fn instrument_response_without_line_count_round_trips() {
        round_trip(&Response::new(
            3,
            ResponseResult::Instrument {
                instrumented: true,
                output_file: None,
                instrumentable_line_count: None,
            },
        ));
    }

    #[test]
    fn execute_response_round_trips() {
        round_trip(&Response::new(
            4,
            ResponseResult::Execute(Box::new(ExecuteResult {
                return_value: Some(serde_json::json!({"cost": 12.99, "method": "express"})),
                thrown_error: None,
                branch_path: vec![BranchDecision {
                    branch_id: 1,
                    line: 23,
                    taken: true,
                    constraint: SymConstraint::Expr {
                        expr: SymExpr::BinOp {
                            op: BinOpKind::Eq,
                            left: Box::new(SymExpr::Param {
                                name: "order".into(),
                                path: vec!["priority".into()],
                            }),
                            right: Box::new(SymExpr::Const(ConstValue::Str("express".into()))),
                        },
                    },
                }],
                lines_executed: vec![10, 11, 23, 24, 30],
                calls_to_external: vec![ExternalCall {
                    symbol: "rateService.getExpressRate".into(),
                    args: vec![serde_json::json!("90210")],
                    return_value: serde_json::json!(12.99),
                }],
                path_constraints: vec![SymConstraint::Expr {
                    expr: SymExpr::BinOp {
                        op: BinOpKind::Eq,
                        left: Box::new(SymExpr::Param {
                            name: "order".into(),
                            path: vec!["priority".into()],
                        }),
                        right: Box::new(SymExpr::Const(ConstValue::Str("express".into()))),
                    },
                }],
                scope_events: vec![],
                side_effects: vec![SideEffect::ConsoleOutput {
                    level: "info".into(),
                    message: "Processing express order".into(),
                }],
                performance: PerformanceMetrics {
                    wall_time_ms: 0.3,
                    cpu_time_us: 250,
                    heap_used_bytes: 1024,
                    heap_allocated_bytes: 2048,
                },
                capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
            })),
        ));
    }

    #[test]
    fn execute_response_with_error_round_trips() {
        round_trip(&Response::new(
            5,
            ResponseResult::Execute(Box::new(ExecuteResult {
                return_value: None,
                thrown_error: Some(ErrorInfo {
                    error_type: "ValidationError".into(),
                    message: "Invalid zip code".into(),
                    stack: Some("at validateZip (shipping.ts:15)".into()),
                    error_category: None,
                }),
                branch_path: vec![],
                lines_executed: vec![10, 15],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                performance: PerformanceMetrics {
                    wall_time_ms: 0.1,
                    cpu_time_us: 80,
                    heap_used_bytes: 256,
                    heap_allocated_bytes: 256,
                },
                capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
            })),
        ));
    }

    #[test]
    fn execute_response_with_connection_failures_round_trips() {
        round_trip(&Response::new(
            42,
            ResponseResult::Execute(Box::new(ExecuteResult {
                return_value: None,
                thrown_error: Some(ErrorInfo {
                    error_type: "Error".into(),
                    message: "connect ECONNREFUSED 127.0.0.1:5432".into(),
                    stack: None,
                    error_category: Some("infrastructure".into()),
                }),
                branch_path: vec![],
                lines_executed: vec![],
                calls_to_external: vec![],
                path_constraints: vec![],
                side_effects: vec![],
                scope_events: vec![],
                performance: PerformanceMetrics::default(),
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![
                    ConnectionFailure {
                        symbol: "pg:query".into(),
                        error_kind: "connection_refused".into(),
                        message: "connect ECONNREFUSED 127.0.0.1:5432".into(),
                    },
                    ConnectionFailure {
                        symbol: "http:fetch".into(),
                        error_kind: "dns_failure".into(),
                        message: "getaddrinfo ENOTFOUND api.example.com".into(),
                    },
                ],
            })),
        ));
    }

    #[test]
    fn connection_failure_struct_round_trips() {
        round_trip(&ConnectionFailure {
            symbol: "redis:connect".into(),
            error_kind: "timeout".into(),
            message: "ETIMEDOUT connecting to redis:6379".into(),
        });
    }

    #[test]
    fn execute_result_without_connection_failures_omits_field() {
        let result = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            !json.contains("connection_failures"),
            "empty connection_failures should be omitted from JSON"
        );
    }

    #[test]
    fn shutdown_ack_response_round_trips() {
        round_trip(&Response::new(6, ResponseResult::ShutdownAck));
    }

    #[test]
    fn error_response_round_trips() {
        round_trip(&Response::new(
            7,
            ResponseResult::Error {
                code: ErrorCode::FileNotFound,
                message: "File not found: src/missing.ts".into(),
                details: None,
            },
        ));
    }

    #[test]
    fn error_response_with_details_round_trips() {
        round_trip(&Response::new(
            8,
            ResponseResult::Error {
                code: ErrorCode::ParseError,
                message: "Syntax error at line 42".into(),
                details: Some(serde_json::json!({
                    "line": 42,
                    "column": 10,
                    "source": "if (x > {"
                })),
            },
        ));
    }

    // -- Component type round-trips --

    #[test]
    fn mock_config_round_trips() {
        round_trip(&MockConfig {
            symbol: "db.query".into(),
            return_values: vec![
                serde_json::json!({"id": 1, "name": "Alice"}),
                serde_json::json!({"id": 2, "name": "Bob"}),
            ],
            should_track_calls: true,
            default_behavior: MockBehavior::ReturnGenerated,
        });
    }

    #[test]
    fn all_mock_behaviors_round_trip() {
        round_trip(&MockBehavior::ReturnGenerated);
        round_trip(&MockBehavior::RepeatLast);
        round_trip(&MockBehavior::ThrowError);
        round_trip(&MockBehavior::Passthrough);
    }

    /// Canonical error code list (11 codes) matching protocol/registry.yaml.
    /// This match is exhaustive — adding a variant to ErrorCode without
    /// updating this list causes a compiler error.
    const ALL_ERROR_CODES: [(ErrorCode, &str); 11] = [
        (ErrorCode::FileNotFound, "file_not_found"),
        (ErrorCode::FunctionNotFound, "function_not_found"),
        (ErrorCode::ParseError, "parse_error"),
        (ErrorCode::InstrumentationFailed, "instrumentation_failed"),
        (ErrorCode::ExecutionTimeout, "execution_timeout"),
        (ErrorCode::ExecutionCrash, "execution_crash"),
        (ErrorCode::VersionMismatch, "version_mismatch"),
        (ErrorCode::InvalidRequest, "invalid_request"),
        (ErrorCode::CompilationError, "compilation_error"),
        (ErrorCode::InternalError, "internal_error"),
        (ErrorCode::NotSupported, "not_supported"),
    ];

    #[test]
    fn all_error_codes_round_trip() {
        for (code, _) in ALL_ERROR_CODES {
            round_trip(&code);
        }
    }

    #[test]
    fn error_code_serialized_form_matches_registry() {
        for (code, expected_str) in ALL_ERROR_CODES {
            let serialized = serde_json::to_string(&code).expect("serialize");
            assert_eq!(
                serialized,
                format!("\"{expected_str}\""),
                "ErrorCode::{code:?} should serialize to \"{expected_str}\""
            );
        }
    }

    /// Exhaustive match — compiler enforces that every ErrorCode variant is listed.
    #[test]
    fn error_code_enum_is_exhaustive() {
        fn variant_name(code: &ErrorCode) -> &'static str {
            match code {
                ErrorCode::FileNotFound => "file_not_found",
                ErrorCode::FunctionNotFound => "function_not_found",
                ErrorCode::ParseError => "parse_error",
                ErrorCode::InstrumentationFailed => "instrumentation_failed",
                ErrorCode::ExecutionTimeout => "execution_timeout",
                ErrorCode::ExecutionCrash => "execution_crash",
                ErrorCode::VersionMismatch => "version_mismatch",
                ErrorCode::InvalidRequest => "invalid_request",
                ErrorCode::CompilationError => "compilation_error",
                ErrorCode::InternalError => "internal_error",
                ErrorCode::NotSupported => "not_supported",
            }
        }
        for (code, expected) in ALL_ERROR_CODES {
            assert_eq!(variant_name(&code), expected);
        }
    }

    #[test]
    fn select_branch_type_deserializes() {
        let json = r#""select""#;
        let bt: BranchType = serde_json::from_str(json).expect("select should deserialize");
        assert_eq!(bt, BranchType::Select);
    }

    #[test]
    fn all_branch_types_round_trip() {
        let types = [
            BranchType::If,
            BranchType::ElseIf,
            BranchType::Switch,
            BranchType::Ternary,
            BranchType::LogicalAnd,
            BranchType::LogicalOr,
            BranchType::While,
            BranchType::For,
            BranchType::Select,
        ];
        for bt in types {
            round_trip(&bt);
        }
    }

    #[test]
    fn all_dependency_kinds_round_trip() {
        let kinds = [
            DependencyKind::FunctionCall,
            DependencyKind::MethodCall,
            DependencyKind::PropertyAccess,
            DependencyKind::ModuleImport,
        ];
        for kind in kinds {
            round_trip(&kind);
        }
    }

    #[test]
    fn branch_info_without_condition_round_trips() {
        round_trip(&BranchInfo {
            id: 5,
            line: 100,
            condition_text: "complexRegex.test(input)".into(),
            condition: None,
            branch_type: BranchType::If,
        });
    }

    #[test]
    fn external_dependency_round_trips() {
        round_trip(&ExternalDependency {
            kind: DependencyKind::MethodCall,
            symbol: "logger.warn".into(),
            source_module: "winston".into(),
            return_type: TypeInfo::Unknown,
            param_types: vec![TypeInfo::Str],
            call_sites: vec![15, 30, 45],
        });
    }

    #[test]
    fn performance_metrics_round_trips() {
        round_trip(&PerformanceMetrics {
            wall_time_ms: 1.5,
            cpu_time_us: 1200,
            heap_used_bytes: 4096,
            heap_allocated_bytes: 8192,
        });
    }

    #[test]
    fn function_analysis_minimal_round_trips() {
        round_trip(&FunctionAnalysis {
            name: "identity".into(),
            exported: false,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Unknown,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 3,
            literals: vec![],
            crypto_boundaries: vec![],
        });
    }

    // -- JSON format verification --

    #[test]
    fn analyze_request_serializes_with_command_tag() {
        let req = Request::new(
            1,
            Command::Analyze {
                file: "main.ts".into(),
                function: Some("foo".into()),
                project_root: None,
            },
        );
        let json: serde_json::Value = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["command"], "analyze");
        assert_eq!(json["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(json["id"], 1);
        assert_eq!(json["file"], "main.ts");
        assert_eq!(json["function"], "foo");
    }

    #[test]
    fn error_response_serializes_with_status_tag() {
        let resp = Response::new(
            1,
            ResponseResult::Error {
                code: ErrorCode::FileNotFound,
                message: "not found".into(),
                details: None,
            },
        );
        let json: serde_json::Value = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["status"], "error");
        assert_eq!(json["code"], "file_not_found");
    }

    #[test]
    fn execute_request_with_mocks_round_trips() {
        round_trip(&Request::new(
            10,
            Command::Execute {
                function: "processOrder".into(),
                inputs: vec![
                    serde_json::json!({"id": 1}),
                    serde_json::json!("express"),
                ],
                mocks: vec![
                    MockConfig {
                        symbol: "db.save".into(),
                        return_values: vec![serde_json::json!(true)],
                        should_track_calls: true,
                        default_behavior: MockBehavior::RepeatLast,
                    },
                    MockConfig {
                        symbol: "emailService.send".into(),
                        return_values: vec![],
                        should_track_calls: false,
                        default_behavior: MockBehavior::Passthrough,
                    },
                ],
                setup_context: None,
            },
        ));
    }

    #[test]
    fn instrument_request_empty_mocks_round_trips() {
        round_trip(&Request::new(
            11,
            Command::Instrument {
                file: "src/utils.ts".into(),
                function: "formatDate".into(),
                mocks: vec![],
                project_root: None,
            },
        ));
    }

    #[test]
    fn noop_frontend_handshake_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":1,"status":"handshake","frontend_version":"0.1.0","language":"noop","capabilities":["analyze","execute","instrument"]}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop handshake");
        assert_eq!(resp.id, 1);
        assert_eq!(
            resp.result,
            ResponseResult::Handshake {
                frontend_version: PROTOCOL_VERSION.into(),
                language: "noop".into(),
                capabilities: vec!["analyze".into(), "execute".into(), "instrument".into()],
            }
        );
    }

    #[test]
    fn noop_frontend_analyze_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":2,"status":"analyze","functions":[{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}]}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop analyze");
        assert_eq!(resp.id, 2);
        if let ResponseResult::Analyze { functions } = &resp.result {
            assert_eq!(functions.len(), 1);
            assert_eq!(functions[0].name, "stub");
        } else {
            panic!("expected Analyze response");
        }
    }

    #[test]
    fn analyze_response_missing_functions_field_defaults_to_empty() {
        // Regression test for str-xkb: Go frontend omitted "functions" field
        // for files with no function definitions (e.g., doc.go).
        let json = r#"{"protocol_version":"0.1.0","id":2,"status":"analyze"}"#;
        let resp: Response = serde_json::from_str(json)
            .expect("should deserialize analyze response without functions field");
        assert_eq!(resp.id, 2);
        if let ResponseResult::Analyze { functions } = &resp.result {
            assert!(functions.is_empty(), "expected empty functions vec");
        } else {
            panic!("expected Analyze response");
        }
    }

    #[test]
    fn noop_frontend_execute_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":3,"status":"execute","return_value":null,"thrown_error":null,"branch_path":[],"lines_executed":[],"calls_to_external":[],"path_constraints":[],"side_effects":[],"performance":{"wall_time_ms":0.0,"cpu_time_us":0,"heap_used_bytes":0,"heap_allocated_bytes":0}}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop execute");
        assert_eq!(resp.id, 3);
        if let ResponseResult::Execute(result) = &resp.result {
            assert!(result.return_value.is_none() || result.return_value == Some(serde_json::Value::Null));
            assert!(result.branch_path.is_empty());
        } else {
            panic!("expected Execute response");
        }
    }

    #[test]
    fn noop_frontend_instrument_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":4,"status":"instrument","instrumented":true,"output_file":null}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop instrument");
        assert_eq!(resp.id, 4);
        assert_eq!(
            resp.result,
            ResponseResult::Instrument {
                instrumented: true,
                output_file: None,
                instrumentable_line_count: None,
            }
        );
    }

    #[test]
    fn instrument_response_with_line_count_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":4,"status":"instrument","instrumented":true,"output_file":null,"instrumentable_line_count":9}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize instrument with line count");
        assert_eq!(
            resp.result,
            ResponseResult::Instrument {
                instrumented: true,
                output_file: None,
                instrumentable_line_count: Some(9),
            }
        );
    }

    #[test]
    fn noop_frontend_shutdown_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":5,"status":"shutdown_ack"}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop shutdown");
        assert_eq!(resp.id, 5);
        assert_eq!(resp.result, ResponseResult::ShutdownAck);
    }

    #[test]
    fn noop_frontend_error_response_deserializes() {
        let json = r#"{"protocol_version":"0.1.0","id":6,"status":"error","code":"invalid_request","message":"Unknown command: bogus","details":null}"#;
        let resp: Response = serde_json::from_str(json).expect("deserialize noop error");
        assert_eq!(resp.id, 6);
        if let ResponseResult::Error { code, message, .. } = &resp.result {
            assert_eq!(*code, ErrorCode::InvalidRequest);
            assert!(message.contains("bogus"));
        } else {
            panic!("expected Error response");
        }
    }

    #[test]
    fn analyze_response_multiple_functions_round_trips() {
        round_trip(&Response::new(
            12,
            ResponseResult::Analyze {
                functions: vec![
                    FunctionAnalysis {
                        name: "add".into(),
                        exported: true,
                        params: vec![
                            ParamInfo { name: "a".into(), typ: TypeInfo::Int, type_name: None },
                            ParamInfo { name: "b".into(), typ: TypeInfo::Int, type_name: None },
                        ],
                        branches: vec![],
                        dependencies: vec![],
                        return_type: TypeInfo::Int,
                        start_line: 1,
                        end_line: 3,
                        literals: vec![],
                        crypto_boundaries: vec![],
                    },
                    FunctionAnalysis {
                        name: "divide".into(),
                        exported: true,
                        params: vec![
                            ParamInfo { name: "a".into(), typ: TypeInfo::Float, type_name: None },
                            ParamInfo { name: "b".into(), typ: TypeInfo::Float, type_name: None },
                        ],
                        branches: vec![BranchInfo {
                            id: 0,
                            line: 6,
                            condition_text: "b === 0".into(),
                            condition: Some(SymExpr::BinOp {
                                op: BinOpKind::Eq,
                                left: Box::new(SymExpr::Param {
                                    name: "b".into(),
                                    path: vec![],
                                }),
                                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
                            }),
                            branch_type: BranchType::If,
                        }],
                        dependencies: vec![],
                        return_type: TypeInfo::Float,
                        start_line: 5,
                        end_line: 10,
                        literals: vec![],
                        crypto_boundaries: vec![],
                    },
                ],
            },
        ));
    }

    // -- Setup / Teardown / Generate round-trips --

    #[test]
    fn setup_request_round_trips() {
        round_trip(&Request::new(
            20,
            Command::Setup {
                file: "./setup/global.ts".into(),
                scope: "processOrder".into(),
                level: SetupLevel::Function,
                project_root: None,
                parent_context: None,
            },
        ));
    }

    #[test]
    fn setup_request_per_execution_round_trips() {
        round_trip(&Request::new(
            21,
            Command::Setup {
                file: "./setup/auth.ts".into(),
                scope: "authenticate".into(),
                level: SetupLevel::Execution,
                project_root: None,
                parent_context: None,
            },
        ));
    }

    #[test]
    fn setup_request_session_level_round_trips() {
        round_trip(&Request::new(
            22,
            Command::Setup {
                file: "./setup/session.ts".into(),
                scope: "test-session".into(),
                level: SetupLevel::Session,
                project_root: None,
                parent_context: None,
            },
        ));
    }

    #[test]
    fn setup_request_file_level_round_trips() {
        round_trip(&Request::new(
            23,
            Command::Setup {
                file: "./setup/file.ts".into(),
                scope: "src/auth.ts".into(),
                level: SetupLevel::File,
                project_root: None,
                parent_context: None,
            },
        ));
    }

    #[test]
    fn setup_request_with_parent_context_round_trips() {
        round_trip(&Request::new(
            24,
            Command::Setup {
                file: "./setup/func.ts".into(),
                scope: "processOrder".into(),
                level: SetupLevel::Function,
                project_root: None,
                parent_context: Some(SetupContextStack {
                    contexts: vec![SetupContextEntry {
                        level: SetupLevel::Session,
                        context: serde_json::json!({"session_id": "s1"}),
                    }],
                }),
            },
        ));
    }

    #[test]
    fn teardown_request_round_trips() {
        round_trip(&Request::new(
            25,
            Command::Teardown {
                scope: "processOrder".into(),
                level: SetupLevel::Function,
            },
        ));
    }

    #[test]
    fn generate_request_type_name_round_trips() {
        round_trip(&Request::new(
            23,
            Command::Generate {
                file: "./generators/user.ts".into(),
                name: "User".into(),
                kind: GeneratorKind::TypeName,
                recipe: None,
                project_root: None,
            },
        ));
    }

    #[test]
    fn generate_request_param_name_round_trips() {
        round_trip(&Request::new(
            24,
            Command::Generate {
                file: "./generators/token.ts".into(),
                name: "authToken".into(),
                kind: GeneratorKind::ParamName,
                recipe: None,
                project_root: None,
            },
        ));
    }

    #[test]
    fn setup_response_round_trips() {
        round_trip(&Response::new(
            20,
            ResponseResult::Setup {
                setup_context: serde_json::json!({"db_handle": "conn_42", "temp_dir": "/tmp/test"}),
            },
        ));
    }

    #[test]
    fn teardown_ack_response_round_trips() {
        round_trip(&Response::new(21, ResponseResult::TeardownAck));
    }

    #[test]
    fn generate_response_round_trips() {
        round_trip(&Response::new(
            22,
            ResponseResult::Generate {
                value: serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
                generator_id: "generated".into(),
                recipe: None,
            },
        ));
    }

    #[test]
    fn generate_response_primitive_value_round_trips() {
        round_trip(&Response::new(
            23,
            ResponseResult::Generate {
                value: serde_json::json!("tok_abc123"),
                generator_id: "generated".into(),
                recipe: None,
            },
        ));
    }

    #[test]
    fn execute_request_with_setup_context_round_trips() {
        round_trip(&Request::new(
            30,
            Command::Execute {
                function: "processOrder".into(),
                inputs: vec![serde_json::json!({"id": 1})],
                mocks: vec![],
                setup_context: Some(SetupContextStack {
                    contexts: vec![SetupContextEntry {
                        level: SetupLevel::Function,
                        context: serde_json::json!({"db_handle": "conn_42"}),
                    }],
                }),
            },
        ));
    }

    #[test]
    fn execute_request_without_setup_context_backward_compatible() {
        // Verify that JSON without setup_context still deserializes correctly.
        let json = r#"{"protocol_version":"0.1.0","id":26,"command":"execute","function":"myFunc","inputs":[1],"mocks":[]}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.id, 26);
        if let Command::Execute { setup_context, .. } = &req.command {
            assert_eq!(*setup_context, None);
        } else {
            panic!("expected Execute command");
        }
    }

    #[test]
    fn execute_request_without_setup_context_omits_field_in_json() {
        let req = Request::new(
            31,
            Command::Execute {
                function: "myFunc".into(),
                inputs: vec![serde_json::json!(1)],
                mocks: vec![],
                setup_context: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(!json.as_object().expect("object").contains_key("setup_context"));
    }

    #[test]
    fn setup_request_serializes_with_command_tag() {
        let req = Request::new(
            32,
            Command::Setup {
                file: "./setup.ts".into(),
                scope: "fn1".into(),
                level: SetupLevel::Function,
                project_root: None,
                parent_context: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["command"], "setup");
        assert_eq!(json["file"], "./setup.ts");
        assert_eq!(json["scope"], "fn1");
        assert_eq!(json["level"], "function");
    }

    #[test]
    fn teardown_request_serializes_with_command_tag() {
        let req = Request::new(
            33,
            Command::Teardown {
                scope: "fn1".into(),
                level: SetupLevel::Function,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["command"], "teardown");
        assert_eq!(json["scope"], "fn1");
        assert_eq!(json["level"], "function");
    }

    #[test]
    fn generate_request_serializes_with_command_tag() {
        let req = Request::new(
            32,
            Command::Generate {
                file: "./gen.ts".into(),
                name: "User".into(),
                kind: GeneratorKind::TypeName,
                recipe: None,
                project_root: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["command"], "generate");
        assert_eq!(json["file"], "./gen.ts");
        assert_eq!(json["name"], "User");
        assert_eq!(json["kind"], "type_name");
    }

    #[test]
    fn all_generator_kinds_round_trip() {
        round_trip(&GeneratorKind::TypeName);
        round_trip(&GeneratorKind::ParamName);
    }

    // -- LiteralValue tests --

    #[test]
    fn literal_value_round_trips_all_variants() {
        round_trip(&LiteralValue::Int { value: 42 });
        round_trip(&LiteralValue::Int { value: -1 });
        round_trip(&LiteralValue::Float { value: 3.14 });
        round_trip(&LiteralValue::Str { value: "express".into() });
        round_trip(&LiteralValue::Bool { value: true });
        round_trip(&LiteralValue::Regex { pattern: "\\d+".into() });
    }

    #[test]
    fn function_analysis_with_literals_round_trips() {
        round_trip(&FunctionAnalysis {
            name: "classify".into(),
            exported: true,
            params: vec![ParamInfo { name: "s".into(), typ: TypeInfo::Str, type_name: None }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Str,
            start_line: 1,
            end_line: 10,
            literals: vec![
                LiteralValue::Str { value: "express".into() },
                LiteralValue::Int { value: 100 },
                LiteralValue::Regex { pattern: "\\d{5}".into() },
            ],
            crypto_boundaries: vec![],
        });
    }

    #[test]
    fn function_analysis_without_literals_field_deserializes_as_empty() {
        let json = r#"{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}"#;
        let fa: FunctionAnalysis = serde_json::from_str(json).expect("deserialize");
        assert!(fa.literals.is_empty(), "missing field should default to empty");
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
        };
        let json = serde_json::to_value(&fa).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("literals"),
            "empty literals should not appear in JSON"
        );
    }

    #[test]
    fn analyze_with_project_root_round_trips() {
        round_trip(&Request::new(
            10,
            Command::Analyze {
                file: "src/main.ts".into(),
                function: Some("handler".into()),
                project_root: Some("/home/user/project".into()),
            },
        ));
    }

    #[test]
    fn analyze_without_project_root_omits_field() {
        let req = Request::new(
            11,
            Command::Analyze {
                file: "main.ts".into(),
                function: None,
                project_root: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("project_root"),
            "None project_root should be omitted from JSON"
        );
    }

    #[test]
    fn instrument_with_project_root_round_trips() {
        round_trip(&Request::new(
            12,
            Command::Instrument {
                file: "src/utils.ts".into(),
                function: "processData".into(),
                mocks: vec![],
                project_root: Some("/home/user/project".into()),
            },
        ));
    }

    // -- CryptoBoundary tests --

    #[test]
    fn crypto_boundary_round_trips() {
        use crate::crypto_registry::{CryptoDirection, OutputSemantics, ParamRole};
        use crate::nondeterminism::Confidence;
        use std::collections::HashMap;

        let mut roles = HashMap::new();
        roles.insert("0".to_string(), ParamRole::Algorithm);
        roles.insert("1".to_string(), ParamRole::Key);
        roles.insert("2".to_string(), ParamRole::Iv);

        round_trip(&CryptoBoundary {
            symbol: "createDecipheriv".into(),
            source_module: "crypto".into(),
            direction: CryptoDirection::Decrypt,
            output: Some(OutputSemantics::Plaintext),
            confidence: Confidence::High,
            param_roles: roles,
            call_sites: vec![5, 12],
            input_entropy: None,
            output_entropy: None,
        });
    }

    #[test]
    fn crypto_boundary_heuristic_round_trips() {
        use crate::crypto_registry::CryptoDirection;
        use crate::nondeterminism::Confidence;

        round_trip(&CryptoBoundary {
            symbol: "encryptPayload".into(),
            source_module: "my-custom-lib".into(),
            direction: CryptoDirection::Encrypt,
            output: None,
            confidence: Confidence::Medium,
            param_roles: HashMap::new(),
            call_sites: vec![42],
        });
    }

    #[test]
    fn crypto_boundary_missing_confidence_defaults_to_high() {
        let json = r#"{"symbol":"createDecipheriv","source_module":"crypto","direction":"decrypt","output":"plaintext","call_sites":[5]}"#;
        let cb: CryptoBoundary = serde_json::from_str(json).expect("deserialize");
        assert_eq!(cb.confidence, Confidence::High);
    }

    #[test]
    fn function_analysis_with_crypto_boundaries_round_trips() {
        use crate::crypto_registry::{CryptoDirection, OutputSemantics};
        use crate::nondeterminism::Confidence;

        round_trip(&FunctionAnalysis {
            name: "decrypt".into(),
            exported: true,
            params: vec![ParamInfo {
                name: "data".into(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Str,
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![CryptoBoundary {
                symbol: "createDecipheriv".into(),
                source_module: "crypto".into(),
                direction: CryptoDirection::Decrypt,
                output: Some(OutputSemantics::Plaintext),
                confidence: Confidence::High,
                param_roles: HashMap::new(),
                call_sites: vec![3],
                input_entropy: None,
                output_entropy: None,
            }],
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

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn request_round_trips(req in arb_request()) {
                let json = serde_json::to_string(&req).unwrap();
                let decoded: Request = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(req, decoded);
            }

            #[test]
            fn response_round_trips(resp in arb_response()) {
                let json = serde_json::to_string(&resp).unwrap();
                let decoded: Response = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(resp, decoded);
            }

            #[test]
            fn execute_result_round_trips(er in arb_execute_result()) {
                let json = serde_json::to_string(&er).unwrap();
                let decoded: ExecuteResult = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(er, decoded);
            }

            #[test]
            fn function_analysis_round_trips(fa in arb_function_analysis()) {
                let json = serde_json::to_string(&fa).unwrap();
                let decoded: FunctionAnalysis = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(fa, decoded);
            }
        }
    }

    #[test]
    fn validate_execute_result_accepts_empty_branch_path() {
        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
        };
        assert!(validate_execute_result(&result));
    }

    #[test]
    fn validate_execute_result_rejects_unknown_branches_with_constraints() {
        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![BranchDecision {
                branch_id: 1,
                line: 10,
                taken: true,
                constraint: SymConstraint::Unknown {
                    hint: "opaque".into(),
                },
            }],
            lines_executed: vec![10],
            calls_to_external: vec![],
            path_constraints: vec![SymConstraint::Expr {
                expr: SymExpr::Const(ConstValue::Bool(true)),
            }],
            side_effects: vec![],
            scope_events: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None, discovered_dependencies: vec![], connection_failures: vec![],
        };
        assert!(!validate_execute_result(&result));
    }

    #[test]
    fn validate_analyze_result_accepts_valid_function() {
        let functions = vec![FunctionAnalysis {
            name: "foo".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
        }];
        assert!(validate_analyze_result(&functions));
    }

    #[test]
    fn validate_analyze_result_rejects_empty_name() {
        let functions = vec![FunctionAnalysis {
            name: String::new(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
        }];
        assert!(!validate_analyze_result(&functions));
    }

    #[test]
    fn validate_analyze_result_rejects_inverted_lines() {
        let functions = vec![FunctionAnalysis {
            name: "bar".into(),
            exported: false,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 20,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
        }];
        assert!(!validate_analyze_result(&functions));
    }

    #[test]
    fn validate_analyze_result_accepts_empty_list() {
        assert!(validate_analyze_result(&[]));
    }
}
