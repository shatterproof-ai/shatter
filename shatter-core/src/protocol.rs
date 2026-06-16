//! Protocol types for communication between the Rust core and language frontends.
//!
//! All messages are newline-delimited JSON over stdin/stdout. The core sends
//! [`Request`] messages to frontends and receives [`Response`] messages back.
//! Every message includes a protocol version for compatibility checking.

use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::crypto_registry::{CryptoDirection, OutputSemantics, ParamRole};
use crate::execution_record::{
    BranchDecision, ErrorInfo, ExternalCall, SideEffect, SymConstraint, TraceEvent, TruncationInfo,
};
use crate::nondeterminism::Confidence;
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

/// Ordered adapter descriptors that customize how a target should be executed.
///
/// The core treats this profile opaquely. Language frontends may interpret
/// adapter ids and options, but the wire shape remains generic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProfile {
    /// Ordered adapter descriptors to apply for this target.
    pub adapters: Vec<ExecutionAdapter>,
}

/// Opaque descriptor for one execution adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionAdapter {
    /// Namespaced adapter identifier, for example `ts/react-hooks`.
    pub id: String,
    /// Policy for whether this adapter is required, auto-applied, suggested, or disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<ExecutionAdapterApply>,
    /// Adapter-local opaque options payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// Generic relation between execution adapters used by hinting and policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRelation {
    /// Related adapter identifier.
    pub adapter_id: String,
    /// Optional human-readable explanation for the relation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Recognizer-generated hint that an adapter may be relevant for a target.
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

/// Application policy for one execution adapter descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAdapterApply {
    Required,
    Auto,
    Suggest,
    Disabled,
}

// ---------------------------------------------------------------------------
// Planner wire types (str-hy9b.E1 / str-zbyp)
//
// These mirror the Go-side structs defined in shatter-go/protocol/invocation_plan.go.
// The JSON shape is the contract: each kind field is a free-standing discriminator
// on an otherwise-flat struct, matching Go's `Kind FooKind \`json:"kind"\`` pattern.
// Do NOT reshape these into serde-tagged sum enums — that would change the wire
// format and break cross-frontend compatibility with Go's round-trip tests.
// ---------------------------------------------------------------------------

/// Constraint kind on a parameter value produced by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueRequirementKind {
    /// Any value is acceptable.
    Any,
    /// A non-zero value is required.
    NonZero,
    /// A positive numeric value is required.
    Positive,
    /// The exact literal carried in the companion `literal` field is required.
    Specific,
}

/// Constraint on a single parameter for an invocation plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueRequirement {
    /// Zero-based parameter index.
    pub param_index: u32,
    /// Declared parameter name (may be empty for unnamed parameters).
    pub param_name: String,
    /// Language-specific type string, e.g. "int" or "*Counter".
    pub type_name: String,
    /// Value constraint classification.
    pub kind: ValueRequirementKind,
    /// Required literal value when `kind` is `specific`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<serde_json::Value>,
}

/// Classification of a runtime-setup precondition required before invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRequirementKind {
    /// A method receiver must be constructed before invocation.
    ReceiverConstruction,
    /// Package-level initialization must have run.
    PackageInitialization,
}

/// Runtime-setup precondition for an invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRequirement {
    /// Requirement classification.
    pub kind: RuntimeRequirementKind,
    /// Type involved in the requirement (e.g. a receiver type name).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_name: String,
    /// Human-readable explanation of the requirement.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// Planner input describing one target and the constraints its invocation
/// plan must satisfy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationRequirement {
    /// Stable target identifier, e.g. "example.com/pkg:Add".
    pub target_id: String,
    /// Per-parameter value constraints in declaration order.
    #[serde(default)]
    pub value_requirements: Vec<ValueRequirement>,
    /// Optional runtime-setup preconditions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_requirements: Vec<RuntimeRequirement>,
}

/// Strategy for producing a concrete argument value in an `InvocationPlan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuePlanKind {
    /// Use the exact literal carried in the companion `literal` field.
    Literal,
    /// Use the zero value of the parameter type.
    Zero,
    /// Select a random value from the type's value space.
    Random,
    /// Track the parameter as a symbolic variable for concolic exploration.
    Symbolic,
    /// Source the argument from a frontend's runtime-value registry (e.g. Go's
    /// `context.Background()` for `context.Context`). The companion `literal`
    /// field carries the source expression as a JSON-encoded string and
    /// `type_hint` names the registered type. The Rust core does not
    /// materialize these directly; the producing frontend is responsible for
    /// realizing the value at execute time.
    RuntimeValue,
}

/// Concrete production strategy for one argument of an `InvocationPlan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValuePlan {
    /// Zero-based argument position.
    pub param_index: u32,
    /// Declared parameter name (may be empty).
    pub param_name: String,
    /// Production strategy.
    pub kind: ValuePlanKind,
    /// Concrete value when `kind` is `literal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<serde_json::Value>,
    /// Language-specific type hint for code generation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_hint: String,
}

/// Resolved plan for invoking a target once.
///
/// Primary output of the planner for a satisfiable `InvocationRequirement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationPlan {
    /// Stable target identifier.
    pub target_id: String,
    /// Receiver construction strategy. Use `"zero_value"` for zero-value
    /// receivers, `"constructor:<FuncName>"` for named constructors, or an
    /// empty string for free functions.
    pub receiver_kind: String,
    /// Ordered concrete type argument list for generic targets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generic_type_args: Vec<String>,
    /// One `ValuePlan` per parameter, in declaration order.
    #[serde(default)]
    pub argument_plans: Vec<ValuePlan>,
    /// Value plans for parameterized constructor arguments (str-9b1q).
    /// When non-empty, the constructor named in `receiver_kind` takes these
    /// as positional arguments. The inputs array sent to the wrapper prepends
    /// constructor arg values before method param values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constructor_arg_plans: Vec<ValuePlan>,
    /// Relative ordering within a plan set; lower values are tried first.
    pub priority: i32,
    /// Optional human-readable name for this plan.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// Why a planner requirement could not be satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsatisfiedRequirementKind {
    /// No constructor is available for the required receiver type.
    NoConstructor,
    /// Receiver type is an interface and cannot be directly instantiated.
    InterfaceReceiver,
    /// A generic type parameter has no concrete instantiation available.
    GenericUnconstrained,
    /// Package depends on cgo, which blocks overlay-based compilation.
    CgoDependency,
    /// Parameter type is too complex for the planner to synthesize.
    ComplexType,
    /// Target requires receiver construction that the planner cannot satisfy.
    RequiresConstruction,
}

/// Planning failure for one target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnsatisfiedRequirement {
    /// Failure classification.
    pub kind: UnsatisfiedRequirementKind,
    /// Target for which planning failed.
    pub target_id: String,
    /// Human-readable explanation of the failure.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
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
        /// Opaque execution profile selected for this target, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<ExecutionProfile>,
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
        /// Opaque execution profile selected for this target, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<ExecutionProfile>,
    },
    /// Pre-build harness artifacts for a function so repeated execute calls skip compilation.
    ///
    /// The frontend compiles the instrumented harness once and returns an opaque
    /// `prepare_id`. Subsequent Execute commands can pass this ID to skip the
    /// compile phase, reducing repeated-execute overhead for concolic exploration.
    Prepare {
        /// Path to the source file.
        file: String,
        /// Name of the function to prepare.
        function: String,
        /// Mock configurations (must match the mocks used in subsequent execute calls).
        mocks: Vec<MockConfig>,
        /// Detected project root directory, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_root: Option<String>,
        /// Opaque execution profile selected for this target, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<ExecutionProfile>,
        /// Optional invocation plan; when present, the prepare_id is keyed on the
        /// plan's receiver_kind so plan-aware callers can pre-build a launcher
        /// for a specific receiver strategy (str-oegu).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<InvocationPlan>,
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
        /// When false, the frontend skips side-effect capture (console/process interception)
        /// for lower per-execute overhead. Defaults to true. Non-capture outputs
        /// (branch_path, lines_executed, return_value, thrown_error) remain correct.
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        capture: bool,
        /// Opaque handle from a prior Prepare command. When present, the frontend
        /// skips the compile phase and runs the pre-built artifact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prepare_id: Option<String>,
        /// Opaque execution profile selected for this target, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<ExecutionProfile>,
        /// Optional InvocationPlan from `get_invocation_plan`. When present,
        /// the frontend uses the plan's `receiver_kind` to construct the
        /// receiver (for method targets) before invoking the function with
        /// `inputs`. When absent, the frontend takes its legacy free-function
        /// path with an empty receiver_kind. Only the Go frontend currently
        /// consumes this field (str-hy9b.H5); TypeScript and Rust frontends
        /// ignore it. See `protocol/parity-matrix.yaml::invocation_plan` and
        /// the `ts-rust-execute-plan-not-implemented` divergence entry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<InvocationPlan>,
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
        /// Opaque execution profile selected for this target, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<ExecutionProfile>,
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
    /// Ask the frontend to produce `InvocationPlan`s for the supplied targets.
    ///
    /// The frontend returns a `ResponseResult::InvocationPlan` with one plan
    /// per satisfiable requirement, plus an `unsatisfied_requirements` list
    /// enumerating targets that could not be planned. Frontends that do not
    /// support the `invocation_plan` capability reply with a `not_supported`
    /// error.
    GetInvocationPlan {
        /// Targets to plan for, one requirement each.
        ///
        /// Wire-named `invocation_requirements` for parity with the Go
        /// Request struct in `shatter-go/protocol/types.go`. The Rust-side
        /// identifier stays short (`requirements`) for ergonomic pattern
        /// matching; serde rename bridges the two names.
        #[serde(rename = "invocation_requirements")]
        requirements: Vec<InvocationRequirement>,
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
    /// Optional timing summary for this frontend command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<TimingSummary>,
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
    /// Successful harness preparation result.
    Prepare {
        /// Opaque handle to pass to subsequent Execute commands to skip compilation.
        prepare_id: String,
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
    /// Successful planner result for a `GetInvocationPlan` request.
    InvocationPlan {
        /// Resolved plans, one per satisfiable requirement. Order is arbitrary;
        /// consumers that need a specific ordering should sort by
        /// `InvocationPlan::priority` or `target_id`.
        ///
        /// Wire-named `invocation_plans` for parity with the Go Response
        /// struct in `shatter-go/protocol/types.go`.
        #[serde(default, rename = "invocation_plans")]
        plans: Vec<InvocationPlan>,
        /// Requirements the planner could not satisfy, each annotated with a
        /// reason. Empty when every supplied requirement produced a plan.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unsatisfied_requirements: Vec<UnsatisfiedRequirement>,
    },
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

/// Deserialize an `i64` that also accepts JSON floats, truncating via `as i64`.
///
/// JavaScript's `Number.isInteger()` returns true for values like
/// `Number.MAX_VALUE` that have no fractional part but exceed i64 range.
/// Frontends may tag these as `"int"` with a float JSON value.
fn deserialize_i64_lenient<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    struct I64LenientVisitor;

    impl<'de> de::Visitor<'de> for I64LenientVisitor {
        type Value = i64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or float")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(de::Error::custom)
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
            Ok(v as i64)
        }
    }

    deserializer.deserialize_any(I64LenientVisitor)
}

/// A literal constant value extracted from source code during static analysis.
///
/// Used to seed the candidate input pool with values the function itself
/// compares against, improving branch coverage on first pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiteralValue {
    Int {
        #[serde(deserialize_with = "deserialize_i64_lenient")]
        value: i64,
    },
    Float {
        value: f64,
    },
    Str {
        value: String,
    },
    Bool {
        value: bool,
    },
    /// Regex pattern string (source text, no delimiters or flags).
    Regex {
        pattern: String,
    },
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
    /// Canonical counted loops with induction variable metadata.
    /// Only populated for loops where the induction variable is integer-typed
    /// and unmodified in the loop body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loops: Vec<LoopInfo>,
    /// When set, indicates that this function was discovered via a re-export
    /// and actually lives in a different source file than the one analyzed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    /// Recognizer-generated hints that describe relevant execution adapters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_hints: Vec<AdapterHint>,
    /// Generic invocation metadata for targets that are not meaningfully called
    /// as a plain exported function with the analyzed parameter list.
    #[serde(default, skip_serializing_if = "InvocationModel::is_direct")]
    pub invocation_model: InvocationModel,
}

/// Describes how a discovered target should be invoked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvocationModel {
    /// The target can be called directly with `FunctionAnalysis.params`.
    #[default]
    Direct,
    /// The target requires adapter-owned invocation and may expose a synthetic
    /// callable surface or opaque scenario schema instead of the raw export.
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
    fn is_direct(model: &InvocationModel) -> bool {
        matches!(model, InvocationModel::Direct)
    }
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

/// Comparison operator for induction variable bound checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundOp {
    Lt,
    Le,
    Gt,
    Ge,
}

/// Metadata about a loop induction variable (e.g., `i` in `for (i = 0; i < n; i++)`).
///
/// Only populated for canonical counted loops where the induction variable has
/// integer type and is not modified inside the loop body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InductionVar {
    /// Variable name (e.g., "i").
    pub name: String,
    /// Initial value expression (e.g., Const(Int(0))).
    pub init_expr: SymExpr,
    /// Step expression per iteration (e.g., Const(Int(1)) for i++).
    pub step_expr: SymExpr,
    /// Bound expression (e.g., Param("n") for i < n).
    pub bound_expr: SymExpr,
    /// Comparison operator in the loop condition.
    pub bound_op: BoundOp,
}

/// A canonical counted loop detected during static analysis.
///
/// The `loop_id` matches the scope event `LoopEnter`/`LoopExit` IDs emitted
/// during instrumented execution, enabling the core engine to correlate static
/// analysis with runtime traces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopInfo {
    /// Matches the loop_id in ScopeEvent::LoopEnter/LoopExit.
    pub loop_id: u32,
    /// Source line of the loop statement.
    pub line: u32,
    /// Induction variable metadata.
    pub induction_var: InductionVar,
}

/// A per-iteration symbolic state snapshot for a supported loop body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopBodyState {
    /// Matches the loop_id in ScopeEvent::LoopEnter/LoopExit and LoopInfo.
    pub loop_id: u32,
    /// Zero-based iteration index in execution order.
    pub iteration: u32,
    /// Symbolic expressions for tracked identifier locals at this iteration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locals: BTreeMap<String, SymExpr>,
}

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
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
    /// A `require()` call failed with MODULE_NOT_FOUND and was replaced
    /// with a recursive Proxy stub. Results from this execution are
    /// partially analyzed.
    StubbedImport,
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

/// Whether a runtime crypto boundary is encrypting or decrypting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCryptoBoundaryKind {
    Encrypt,
    Decrypt,
}

/// A cryptographic boundary detected at execution time.
///
/// Produced when the frontend's instrumented code intercepts a call to a known
/// decrypt or encrypt function. Used by the core engine to identify parameters
/// that hold ciphertext, enabling boundary splitting: solve constraints on the
/// plaintext domain then re-encrypt to produce valid ciphertext inputs.
///
/// Frontends inject `__shatter_crypto_boundary()` calls during instrumentation;
/// those calls report back here with key, IV, and algorithm values captured at
/// runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCryptoBoundary {
    /// Stable identifier for this boundary within the execution trace, e.g. `"cb-0"`.
    pub boundary_id: String,
    /// Whether this is an encrypt or decrypt boundary.
    pub kind: RuntimeCryptoBoundaryKind,
    /// Function name as it appears in the source (e.g. `"createDecipheriv"`).
    pub function_name: String,
    /// Algorithm string captured at runtime, if the function takes one (e.g. `"aes-256-cbc"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    /// Index of the argument holding the ciphertext. `None` when the ciphertext is
    /// passed to a separate method call (e.g. `decipher.update(ciphertext)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ciphertext_param_index: Option<i32>,
    /// Base64-encoded key bytes captured at runtime, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_value: Option<String>,
    /// Base64-encoded IV bytes captured at runtime, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iv_value: Option<String>,
}

/// Reason a per-file run produced no targets to attempt.
///
/// This is the closed taxonomy used in `ExploreSummary.no_target_reason`
/// (and any future protocol message that needs to surface the same
/// classification). The value is `Some(_)` only for files that yielded
/// zero scheduled targets; files with at least one target leave the field
/// unset (`None`).
///
/// `Unclassified` is the default when no more specific reason has been
/// detected. Per-language detection (TS/Go/Rust) and frontend-agnostic
/// detection (parser-failure, policy-excluded, generated-schema) refine
/// `Unclassified` into one of the more specific variants in sibling
/// issues str-jeen.22–.25; this schema-only issue (str-jeen.21) defines
/// the enum so all producers and consumers agree on the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoTargetReason {
    /// No more specific reason classifier matched. Default for any file
    /// with zero targets until a frontend or cross-cutting classifier
    /// tags it more precisely.
    Unclassified,
    /// File contains only declarations / type annotations and no
    /// executable definitions (TS `.d.ts`, Rust trait-only modules, etc.).
    DeclarationOnly,
    /// File contains only JSX / component definitions that the analyzer
    /// does not treat as callable targets. (TS-specific.)
    JsxComponentOnly,
    /// File is a test, spec, story, or demo and is excluded from target
    /// discovery by convention.
    TestOrSpec,
    /// Go file declares only methods on a receiver type that the
    /// analyzer cannot synthesize an executable target for. (Go-specific.)
    ReceiverMethodGap,
    /// File looks like a generated artifact (codegen, schema bindings,
    /// etc.) and is skipped to avoid testing generated code.
    Generated,
    /// File is a `_test.go` (or equivalent) test file. (Go-specific.)
    TestFile,
    /// File is a Rust `#[cfg(test)]`-only module. (Rust-specific.)
    TestModule,
    /// File is a Cargo build script (`build.rs`). (Rust-specific.)
    BuildScript,
    /// File was excluded by an explicit user/config policy.
    PolicyExcluded,
    /// Parser or discovery failed; no targets could be enumerated.
    ParserFailure,
    /// File looks like a generated schema artifact (e.g. OpenAPI /
    /// protobuf bindings, GraphQL codegen output).
    GeneratedSchema,
}

impl NoTargetReason {
    /// Stable snake_case token used in JSON, markdown, and logs.
    /// Keep in sync with the `#[serde(rename_all = "snake_case")]` derive.
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        match self {
            NoTargetReason::Unclassified => "unclassified",
            NoTargetReason::DeclarationOnly => "declaration_only",
            NoTargetReason::JsxComponentOnly => "jsx_component_only",
            NoTargetReason::TestOrSpec => "test_or_spec",
            NoTargetReason::ReceiverMethodGap => "receiver_method_gap",
            NoTargetReason::Generated => "generated",
            NoTargetReason::TestFile => "test_file",
            NoTargetReason::TestModule => "test_module",
            NoTargetReason::BuildScript => "build_script",
            NoTargetReason::PolicyExcluded => "policy_excluded",
            NoTargetReason::ParserFailure => "parser_failure",
            NoTargetReason::GeneratedSchema => "generated_schema",
        }
    }
}

/// Status describing the outcome of one invocation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Completed,
    CompletedWithFindings,
    Unsupported,
    BuildFailed,
    RuntimeFailed,
    TimedOut,
    SkippedByPolicy,
    /// Environment preflight check failed for this run (str-jeen.40).
    /// Distinct from `Unsupported`: a preflight failure is an env fault
    /// outside the function under test (e.g. missing `node_modules`),
    /// not a frontend capability gap.
    PreflightFailed,
}

/// Structured outcome produced by one invocation attempt.
///
/// This type is a reusable protocol contract. Individual commands may choose
/// to flatten these fields into their existing response shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationOutcome {
    /// Machine-readable classification of the overall invocation result.
    pub status: OutcomeStatus,
    /// One human-readable sentence summarizing why the invocation reached this status.
    /// Required (non-empty) for any non-completed status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_reason: Option<String>,
    /// Return value from the invocation, if it completed normally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_value: Option<serde_json::Value>,
    /// Error thrown or returned by the invocation, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thrown_error: Option<ErrorInfo>,
    /// Side effects observed while running the invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_effects: Vec<SideEffect>,
}

/// Result of executing an instrumented function.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Per-iteration symbolic snapshots for supported loop bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loop_body_states: Vec<LoopBodyState>,
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
    /// Cryptographic boundaries intercepted at runtime.
    ///
    /// When non-empty, the function called a known encrypt or decrypt API.
    /// The core engine uses this to apply boundary splitting: solve constraints
    /// on the plaintext then re-encrypt to produce valid ciphertext inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_crypto_boundaries: Vec<RuntimeCryptoBoundary>,
    /// Structured invocation outcome.
    ///
    /// The Go frontend always emits `outcome` on Execute responses (see
    /// `outcomeFromResult` in `shatter-go/protocol/handler.go`). Adding the
    /// field here is purely additive on the Rust side — the wire shape was
    /// already established by the Go frontend; this catches up the Rust
    /// parser so callers can read `outcome.status` without going through
    /// raw JSON. TS / Rust frontends do not currently emit this field;
    /// callers must treat None as "outcome not reported by frontend".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<InvocationOutcome>,
}

/// Performance metrics from a single execution.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Wall clock time in milliseconds.
    pub wall_time_ms: f64,
    /// CPU time in microseconds.
    pub cpu_time_us: i64,
    /// Heap memory used in bytes (may be negative when GC reclaims between measurements).
    pub heap_used_bytes: i64,
    /// Heap memory allocated in bytes (may be negative when GC reclaims between measurements).
    pub heap_allocated_bytes: i64,
}

/// Optional timing summary emitted by a frontend command.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimingSummary {
    /// Aggregated phase timings for the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<TimingPhaseSummary>,
}

/// Aggregated timing metrics for one named phase.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimingPhaseSummary {
    /// Stable dotted phase path, e.g. `frontend.request.execute`.
    pub phase_path: String,
    /// Total inclusive wall time spent in this phase.
    pub total_ms: f64,
    /// Exclusive wall time excluding nested child phases.
    #[serde(default)]
    pub self_ms: f64,
    /// Number of times this phase occurred.
    #[serde(default = "default_timing_count")]
    pub count: u64,
    /// Optional phase metadata for filtering or grouping.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

fn default_timing_count() -> u64 {
    1
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
    /// Environment preflight check failed for this run (str-jeen.40).
    /// Distinct from `NotSupported`: indicates an environmental fault
    /// outside the function under test (e.g. missing `node_modules`).
    PreflightFailed,
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
            timing: None,
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
        let all_unknown = result
            .branch_path
            .iter()
            .all(|bd| matches!(bd.constraint, SymConstraint::Unknown { .. }));
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
        if let InvocationModel::Adapter {
            adapter_id,
            synthetic_params,
            ..
        } = &func.invocation_model
        {
            if adapter_id.is_empty() {
                return false;
            }
            if synthetic_params.len() > MAX_PLAUSIBLE_PARAMS {
                return false;
            }
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
                execution_profile: None,
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
                execution_profile: None,
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
                execution_profile: None,
            },
        ));
    }

    #[test]
    fn prepare_request_round_trips() {
        round_trip(&Request::new(
            5,
            Command::Prepare {
                file: "src/shipping.ts".into(),
                function: "calculateShipping".into(),
                mocks: vec![],
                project_root: None,
                execution_profile: None,
                plan: None,
            },
        ));
    }

    #[test]
    fn prepare_request_with_mocks_round_trips() {
        round_trip(&Request::new(
            6,
            Command::Prepare {
                file: "src/order.ts".into(),
                function: "processOrder".into(),
                mocks: vec![MockConfig {
                    symbol: "db.save".into(),
                    return_values: vec![serde_json::json!(true)],
                    should_track_calls: true,
                    default_behavior: MockBehavior::RepeatLast,
                }],
                project_root: Some(".".into()),
                execution_profile: None,
                plan: None,
            },
        ));
    }

    /// str-oegu AC4: plan-bearing Prepare requests survive serialization round-trip.
    #[test]
    fn prepare_request_with_plan_round_trips() {
        round_trip(&Request::new(
            9,
            Command::Prepare {
                file: "svc/svc.go".into(),
                function: "Compute".into(),
                mocks: vec![],
                project_root: None,
                execution_profile: None,
                plan: Some(InvocationPlan {
                    target_id: "example.com/svc:Compute".into(),
                    receiver_kind: "constructor:New".into(),
                    generic_type_args: vec![],
                    argument_plans: vec![ValuePlan {
                        param_index: 0,
                        param_name: "x".into(),
                        kind: ValuePlanKind::Zero,
                        literal: None,
                        type_hint: "int".into(),
                    }],
                    constructor_arg_plans: vec![],
                    priority: 1,
                    label: "constructor:New + x=0".into(),
                }),
            },
        ));
    }

    /// str-oegu AC4: plan field is omitted from JSON when None.
    #[test]
    fn prepare_plan_omitted_when_none() {
        let req = Request::new(
            10,
            Command::Prepare {
                file: "svc/svc.go".into(),
                function: "Compute".into(),
                mocks: vec![],
                project_root: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(
            !json.as_object().expect("object").contains_key("plan"),
            "plan: None should be omitted from JSON"
        );
    }

    #[test]
    fn prepare_response_round_trips() {
        round_trip(&Response::new(
            5,
            ResponseResult::Prepare {
                prepare_id: "a1b2c3d4e5f6a7b8".into(),
            },
        ));
    }

    #[test]
    fn execute_request_with_prepare_id_round_trips() {
        round_trip(&Request::new(
            7,
            Command::Execute {
                function: "calculateShipping".into(),
                inputs: vec![serde_json::json!({"weight": 2.5})],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: Some("a1b2c3d4e5f6a7b8".into()),
                execution_profile: None,
                plan: None,
            },
        ));
    }

    #[test]
    fn prepare_id_omitted_when_none_in_execute_json() {
        let req = Request::new(
            8,
            Command::Execute {
                function: "fn1".into(),
                inputs: vec![],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(
            !json.as_object().expect("object").contains_key("prepare_id"),
            "prepare_id: None should be omitted from JSON"
        );
    }

    #[test]
    fn prepare_id_present_when_set_in_execute_json() {
        let req = Request::new(
            9,
            Command::Execute {
                function: "fn1".into(),
                inputs: vec![],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: Some("deadbeef12345678".into()),
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["prepare_id"], "deadbeef12345678");
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
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
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
                                (
                                    "items".into(),
                                    TypeInfo::Array {
                                        element: Box::new(TypeInfo::Int { int_width: None, int_signed: None }),
                                    },
                                ),
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
                    loops: vec![],
                    source_file: None,
                    adapter_hints: vec![],
                    invocation_model: InvocationModel::Direct,
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
                    conditions: None,
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
                loop_body_states: vec![],
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
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                runtime_crypto_boundaries: vec![],
                outcome: None,
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
                loop_body_states: vec![],
                performance: PerformanceMetrics {
                    wall_time_ms: 0.1,
                    cpu_time_us: 80,
                    heap_used_bytes: 256,
                    heap_allocated_bytes: 256,
                },
                capture_truncation: None,
                discovered_dependencies: vec![],
                connection_failures: vec![],
                runtime_crypto_boundaries: vec![],
                outcome: None,
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
                loop_body_states: vec![],
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
                runtime_crypto_boundaries: vec![],
                outcome: None,
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
    fn runtime_crypto_boundary_round_trips() {
        round_trip(&RuntimeCryptoBoundary {
            boundary_id: "cb-0".into(),
            kind: RuntimeCryptoBoundaryKind::Decrypt,
            function_name: "createDecipheriv".into(),
            algorithm: Some("aes-256-cbc".into()),
            ciphertext_param_index: None,
            key_value: Some("dGVzdGtleQ==".into()),
            iv_value: Some("dGVzdGl2dGVzdGl2".into()),
        });
    }

    #[test]
    fn runtime_crypto_boundary_omits_optional_fields_when_empty() {
        let boundary = RuntimeCryptoBoundary {
            boundary_id: "cb-1".into(),
            kind: RuntimeCryptoBoundaryKind::Encrypt,
            function_name: "createCipheriv".into(),
            algorithm: None,
            ciphertext_param_index: None,
            key_value: None,
            iv_value: None,
        };
        let json = serde_json::to_string(&boundary).expect("serialize");
        assert!(!json.contains("algorithm"));
        assert!(!json.contains("ciphertext_param_index"));
        assert!(!json.contains("key_value"));
        assert!(!json.contains("iv_value"));
        let decoded: RuntimeCryptoBoundary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(boundary, decoded);
    }

    #[test]
    fn execute_result_without_runtime_crypto_boundaries_omits_field() {
        let result = ExecuteResult {
            return_value: None,
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            !json.contains("runtime_crypto_boundaries"),
            "empty runtime_crypto_boundaries should be omitted from JSON"
        );
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
            loop_body_states: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            !json.contains("connection_failures"),
            "empty connection_failures should be omitted from JSON"
        );
    }

    #[test]
    fn execute_result_without_loop_body_states_omits_field() {
        let result = ExecuteResult {
            return_value: Some(serde_json::json!(42)),
            thrown_error: None,
            branch_path: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            !json.contains("loop_body_states"),
            "empty loop_body_states should be omitted from JSON"
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
    const ALL_ERROR_CODES: [(ErrorCode, &str); 12] = [
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
        (ErrorCode::PreflightFailed, "preflight_failed"),
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
                ErrorCode::PreflightFailed => "preflight_failed",
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
    fn timing_summary_round_trips() {
        round_trip(&TimingSummary {
            phases: vec![TimingPhaseSummary {
                phase_path: "frontend.request.execute".into(),
                total_ms: 2.5,
                self_ms: 1.5,
                count: 3,
                attributes: BTreeMap::from([
                    ("language".into(), "typescript".into()),
                    ("command".into(), "execute".into()),
                ]),
            }],
        });
    }

    #[test]
    fn response_with_timing_round_trips() {
        let mut response = Response::new(
            42,
            ResponseResult::Analyze {
                functions: Vec::new(),
            },
        );
        response.timing = Some(TimingSummary {
            phases: vec![TimingPhaseSummary {
                phase_path: "frontend.request.analyze".into(),
                total_ms: 4.0,
                self_ms: 4.0,
                count: 1,
                attributes: BTreeMap::new(),
            }],
        });
        round_trip(&response);
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
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
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
                execution_profile: None,
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
    fn analyze_request_with_execution_profile_round_trips() {
        round_trip(&Request::new(
            12,
            Command::Analyze {
                file: "main.ts".into(),
                function: Some("useTeamSwitch".into()),
                project_root: Some("/repo".into()),
                execution_profile: Some(ExecutionProfile {
                    adapters: vec![
                        ExecutionAdapter {
                            id: "ts/module-resolution/tsconfig-paths".into(),
                            apply: Some(ExecutionAdapterApply::Auto),
                            options: None,
                        },
                        ExecutionAdapter {
                            id: "ts/react-hooks".into(),
                            apply: Some(ExecutionAdapterApply::Suggest),
                            options: Some(serde_json::json!({"mode": "callable_return"})),
                        },
                    ],
                }),
            },
        ));
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
                inputs: vec![serde_json::json!({"id": 1}), serde_json::json!("express")],
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
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
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
                execution_profile: None,
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
            assert!(
                result.return_value.is_none()
                    || result.return_value == Some(serde_json::Value::Null)
            );
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
        let resp: Response =
            serde_json::from_str(json).expect("deserialize instrument with line count");
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
                            ParamInfo {
                                name: "a".into(),
                                typ: TypeInfo::Int { int_width: None, int_signed: None },
                                type_name: None,
                            },
                            ParamInfo {
                                name: "b".into(),
                                typ: TypeInfo::Int { int_width: None, int_signed: None },
                                type_name: None,
                            },
                        ],
                        branches: vec![],
                        dependencies: vec![],
                        return_type: TypeInfo::Int { int_width: None, int_signed: None },
                        start_line: 1,
                        end_line: 3,
                        literals: vec![],
                        crypto_boundaries: vec![],
                        loops: vec![],
                        source_file: None,
                        adapter_hints: vec![],
                        invocation_model: InvocationModel::Direct,
                    },
                    FunctionAnalysis {
                        name: "divide".into(),
                        exported: true,
                        params: vec![
                            ParamInfo {
                                name: "a".into(),
                                typ: TypeInfo::Float,
                                type_name: None,
                            },
                            ParamInfo {
                                name: "b".into(),
                                typ: TypeInfo::Float,
                                type_name: None,
                            },
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
                        loops: vec![],
                        source_file: None,
                        adapter_hints: vec![],
                        invocation_model: InvocationModel::Direct,
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
                execution_profile: None,
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
                execution_profile: None,
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
                execution_profile: None,
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
                execution_profile: None,
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
                execution_profile: None,
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
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
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
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(
            !json
                .as_object()
                .expect("object")
                .contains_key("setup_context")
        );
    }

    #[test]
    fn execute_no_capture_round_trips() {
        round_trip(&Request::new(
            50,
            Command::Execute {
                function: "myFunc".into(),
                inputs: vec![serde_json::json!(1)],
                mocks: vec![],
                setup_context: None,
                capture: false,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        ));
    }

    #[test]
    fn execute_without_capture_field_defaults_to_true() {
        // Verify that JSON without the capture field deserializes with capture = true.
        let json = r#"{"protocol_version":"0.1.0","id":51,"command":"execute","function":"myFunc","inputs":[1],"mocks":[]}"#;
        let req: Request = serde_json::from_str(json).expect("deserialize");
        if let Command::Execute { capture, .. } = &req.command {
            assert!(*capture, "capture should default to true when absent");
        } else {
            panic!("expected Execute command");
        }
    }

    #[test]
    fn execute_capture_true_omits_field_in_json() {
        // capture: true is the default — should be omitted from serialized JSON.
        let req = Request::new(
            52,
            Command::Execute {
                function: "myFunc".into(),
                inputs: vec![serde_json::json!(1)],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        assert!(
            !json.as_object().expect("object").contains_key("capture"),
            "capture: true should be omitted from JSON (default)"
        );
    }

    #[test]
    fn execute_capture_false_included_in_json() {
        // capture: false is non-default — should be present in serialized JSON.
        let req = Request::new(
            53,
            Command::Execute {
                function: "myFunc".into(),
                inputs: vec![serde_json::json!(1)],
                mocks: vec![],
                setup_context: None,
                capture: false,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_value(&req).expect("serialize");
        let obj = json.as_object().expect("object");
        assert!(
            obj.contains_key("capture"),
            "capture: false should be present in JSON"
        );
        assert_eq!(obj["capture"], serde_json::json!(false));
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
                execution_profile: None,
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
        round_trip(&LiteralValue::Float { value: 2.5 });
        round_trip(&LiteralValue::Str {
            value: "express".into(),
        });
        round_trip(&LiteralValue::Bool { value: true });
        round_trip(&LiteralValue::Regex {
            pattern: "\\d+".into(),
        });
    }

    #[test]
    fn literal_value_int_accepts_float_json() {
        // Regression test for str-flqp: TS frontend emits float values
        // (e.g. Number.MAX_VALUE = 1.7976931348623157e+308) tagged as "int".
        // The deserializer must accept JSON floats in the Int variant.
        let json = r#"{"type":"int","value":1.7976931348623157e+308}"#;
        let lit: LiteralValue = serde_json::from_str(json).expect("should accept float in int");
        assert!(matches!(lit, LiteralValue::Int { .. }));

        // Also test a normal float that happens to be whole (e.g. 42.0)
        let json2 = r#"{"type":"int","value":42.0}"#;
        let lit2: LiteralValue = serde_json::from_str(json2).expect("should accept 42.0 as int");
        assert!(matches!(lit2, LiteralValue::Int { value: 42 }));
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
                LiteralValue::Int { value: 100 },
                LiteralValue::Regex {
                    pattern: "\\d{5}".into(),
                },
            ],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        });
    }

    #[test]
    fn function_analysis_without_literals_field_deserializes_as_empty() {
        let json = r#"{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}"#;
        let fa: FunctionAnalysis = serde_json::from_str(json).expect("deserialize");
        assert!(
            fa.literals.is_empty(),
            "missing field should default to empty"
        );
        assert_eq!(fa.invocation_model, InvocationModel::Direct);
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
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        };
        let json = serde_json::to_value(&fa).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("literals"),
            "empty literals should not appear in JSON"
        );
    }

    #[test]
    fn function_analysis_with_adapter_invocation_round_trips() {
        round_trip(&FunctionAnalysis {
            name: "useTeamSwitch".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 20,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Adapter {
                adapter_id: "ts/react-hooks".into(),
                synthetic_params: vec![ParamInfo {
                    name: "action".into(),
                    typ: TypeInfo::Str,
                    type_name: None,
                }],
                scenario_schema: Some(serde_json::json!({
                    "kind": "call_return",
                    "args": [{"type": "string"}]
                })),
            },
        });
    }

    #[test]
    fn direct_invocation_model_omitted_from_json() {
        let fa = FunctionAnalysis {
            name: "identity".into(),
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
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        };
        let json = serde_json::to_value(&fa).expect("serialize");
        assert!(
            !json.as_object().unwrap().contains_key("invocation_model"),
            "direct invocation model should be omitted from JSON"
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
                execution_profile: None,
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
                execution_profile: None,
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
                execution_profile: None,
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
            input_entropy: None,
            output_entropy: None,
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
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        });
    }

    #[test]
    fn function_analysis_with_adapter_hints_round_trips() {
        round_trip(&FunctionAnalysis {
            name: "render".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Unknown,
            start_line: 1,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![AdapterHint {
                adapter: ExecutionAdapter {
                    id: "ts/browser-globals".into(),
                    apply: Some(ExecutionAdapterApply::Suggest),
                    options: None,
                },
                confidence: Confidence::Medium,
                reasons: vec!["uses window".into()],
                requirements: vec![AdapterRelation {
                    adapter_id: "ts/dom-runtime".into(),
                    reason: Some("needs DOM globals".into()),
                }],
                conflicts: vec![AdapterRelation {
                    adapter_id: "ts/node-only".into(),
                    reason: Some("server-only runtime".into()),
                }],
            }],
            invocation_model: InvocationModel::Direct,
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
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
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
    fn missing_adapter_hints_default_to_empty() {
        let json = r#"{"name":"stub","params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}"#;
        let fa: FunctionAnalysis = serde_json::from_str(json).expect("deserialize");
        assert!(fa.adapter_hints.is_empty());
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
            loop_body_states: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
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
                conditions: None,
            }],
            lines_executed: vec![10],
            calls_to_external: vec![],
            path_constraints: vec![SymConstraint::Expr {
                expr: SymExpr::Const(ConstValue::Bool(true)),
            }],
            side_effects: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            performance: PerformanceMetrics::default(),
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
            outcome: None,
        };
        assert!(!validate_execute_result(&result));
    }

    #[test]
    fn validate_analyze_result_rejects_empty_adapter_id() {
        let functions = vec![FunctionAnalysis {
            name: "hook".into(),
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
            adapter_hints: vec![],
            invocation_model: InvocationModel::Adapter {
                adapter_id: String::new(),
                synthetic_params: vec![],
                scenario_schema: None,
            },
        }];

        assert!(!validate_analyze_result(&functions));
    }

    #[test]
    fn validate_analyze_result_accepts_valid_function() {
        let functions = vec![FunctionAnalysis {
            name: "foo".into(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: vec![],
            return_type: TypeInfo::Int { int_width: None, int_signed: None },
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
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
            return_type: TypeInfo::Int { int_width: None, int_signed: None },
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
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
            return_type: TypeInfo::Int { int_width: None, int_signed: None },
            start_line: 20,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        }];
        assert!(!validate_analyze_result(&functions));
    }

    #[test]
    fn validate_analyze_result_accepts_empty_list() {
        assert!(validate_analyze_result(&[]));
    }

    #[test]
    fn execute_response_with_negative_heap_bytes_deserializes() {
        // Regression test for str-x426: Go frontend sends negative heap_used_bytes
        // when GC reclaims memory between before/after measurements. Rust was using
        // u64, which rejects negative integers.
        let json = r#"{"protocol_version":"0.1.0","id":5,"status":"execute","return_value":"positive","thrown_error":null,"branch_path":[{"branch_id":1,"line":13,"taken":true}],"lines_executed":[12,13,14],"calls_to_external":[],"path_constraints":[],"side_effects":[],"performance":{"wall_time_ms":1.5,"cpu_time_us":1200,"heap_used_bytes":-24576,"heap_allocated_bytes":-8192}}"#;
        let resp: Response =
            serde_json::from_str(json).expect("should deserialize negative heap bytes");
        assert_eq!(resp.id, 5);
        if let ResponseResult::Execute(result) = &resp.result {
            assert_eq!(result.performance.heap_used_bytes, -24576);
            assert_eq!(result.performance.heap_allocated_bytes, -8192);
        } else {
            panic!("expected Execute response");
        }
    }

    #[test]
    fn outcome_status_round_trips() {
        for status in [
            OutcomeStatus::Completed,
            OutcomeStatus::CompletedWithFindings,
            OutcomeStatus::Unsupported,
            OutcomeStatus::BuildFailed,
            OutcomeStatus::RuntimeFailed,
            OutcomeStatus::TimedOut,
            OutcomeStatus::SkippedByPolicy,
            OutcomeStatus::PreflightFailed,
        ] {
            let json = serde_json::to_string(&status).expect("serialize");
            let decoded: OutcomeStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, status);
        }
    }

    // -----------------------------------------------------------------
    // Planner wire types (str-zbyp).
    //
    // Cross-language compatibility with Go is asserted via
    // `go_invocation_plan_fixture_deserializes` below — it parses a
    // byte-for-byte fixture of what shatter-go/protocol/invocation_plan.go
    // produces and fails if the field names or kind spellings drift.
    // -----------------------------------------------------------------

    #[test]
    fn value_requirement_kinds_round_trip_all_variants() {
        for kind in [
            ValueRequirementKind::Any,
            ValueRequirementKind::NonZero,
            ValueRequirementKind::Positive,
            ValueRequirementKind::Specific,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let decoded: ValueRequirementKind =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, kind);
        }
        assert_eq!(
            serde_json::to_string(&ValueRequirementKind::NonZero).unwrap(),
            "\"non_zero\""
        );
    }

    #[test]
    fn runtime_requirement_kinds_round_trip_all_variants() {
        for kind in [
            RuntimeRequirementKind::ReceiverConstruction,
            RuntimeRequirementKind::PackageInitialization,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let decoded: RuntimeRequirementKind =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, kind);
        }
        assert_eq!(
            serde_json::to_string(&RuntimeRequirementKind::ReceiverConstruction).unwrap(),
            "\"receiver_construction\""
        );
    }

    #[test]
    fn value_plan_kinds_round_trip_all_variants() {
        for kind in [
            ValuePlanKind::Literal,
            ValuePlanKind::Zero,
            ValuePlanKind::Random,
            ValuePlanKind::Symbolic,
            ValuePlanKind::RuntimeValue,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let decoded: ValuePlanKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, kind);
        }
        // Go's `ValuePlanKindRuntimeValue = "runtime_value"` must not drift to
        // "runtimeValue" or similar (str-1hlk.4).
        assert_eq!(
            serde_json::to_string(&ValuePlanKind::RuntimeValue).unwrap(),
            "\"runtime_value\""
        );
    }

    #[test]
    fn value_plan_deserializes_go_runtime_value_argument() {
        // Round-trip a ValuePlan shaped like the Go planner's
        // runtimeValuePlans output (kind=runtime_value, literal carries the
        // source expression as a JSON-encoded string, type_hint names the
        // registered type).
        let go_wire = r#"{
            "param_index": 0,
            "param_name": "ctx",
            "kind": "runtime_value",
            "literal": "context.Background()",
            "type_hint": "context.Context"
        }"#;
        let decoded: ValuePlan = serde_json::from_str(go_wire).expect("deserialize");
        assert_eq!(decoded.kind, ValuePlanKind::RuntimeValue);
        assert_eq!(decoded.param_name, "ctx");
        assert_eq!(decoded.type_hint, "context.Context");
        assert_eq!(
            decoded.literal,
            Some(serde_json::Value::String("context.Background()".into()))
        );
        // Re-serialize and ensure the kind round-trips.
        let reencoded = serde_json::to_value(&decoded).expect("serialize");
        assert_eq!(reencoded["kind"], "runtime_value");
    }

    #[test]
    fn unsatisfied_requirement_kinds_round_trip_all_variants() {
        for kind in [
            UnsatisfiedRequirementKind::NoConstructor,
            UnsatisfiedRequirementKind::InterfaceReceiver,
            UnsatisfiedRequirementKind::GenericUnconstrained,
            UnsatisfiedRequirementKind::CgoDependency,
            UnsatisfiedRequirementKind::ComplexType,
            UnsatisfiedRequirementKind::RequiresConstruction,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let decoded: UnsatisfiedRequirementKind =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, kind);
        }
        // Go's `UnsatisfiedRequirementKindCGODependency = "cgo_dependency"`
        // must not drift to "cgoDependency" or similar.
        assert_eq!(
            serde_json::to_string(&UnsatisfiedRequirementKind::CgoDependency).unwrap(),
            "\"cgo_dependency\""
        );
    }

    #[test]
    fn invocation_requirement_round_trips() {
        let requirement = InvocationRequirement {
            target_id: "example.com/pkg:Add".into(),
            value_requirements: vec![
                ValueRequirement {
                    param_index: 0,
                    param_name: "x".into(),
                    type_name: "int".into(),
                    kind: ValueRequirementKind::Any,
                    literal: None,
                },
                ValueRequirement {
                    param_index: 1,
                    param_name: "y".into(),
                    type_name: "int".into(),
                    kind: ValueRequirementKind::Specific,
                    literal: Some(serde_json::json!(42)),
                },
            ],
            runtime_requirements: vec![RuntimeRequirement {
                kind: RuntimeRequirementKind::ReceiverConstruction,
                type_name: "Counter".into(),
                detail: "needs new Counter".into(),
            }],
        };
        let json = serde_json::to_string(&requirement).expect("serialize");
        let decoded: InvocationRequirement =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, requirement);
    }

    #[test]
    fn invocation_plan_round_trips() {
        let plan = InvocationPlan {
            target_id: "example.com/pkg:Add".into(),
            receiver_kind: "constructor:NewCounter".into(),
            generic_type_args: vec!["string".into()],
            argument_plans: vec![
                ValuePlan {
                    param_index: 0,
                    param_name: "x".into(),
                    kind: ValuePlanKind::Literal,
                    literal: Some(serde_json::json!(7)),
                    type_hint: "int".into(),
                },
                ValuePlan {
                    param_index: 1,
                    param_name: "y".into(),
                    kind: ValuePlanKind::Symbolic,
                    literal: None,
                    type_hint: "int".into(),
                },
            ],
            constructor_arg_plans: vec![],
            priority: 0,
            label: "constructor_new_counter".into(),
        };
        let json = serde_json::to_string(&plan).expect("serialize");
        let decoded: InvocationPlan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, plan);
    }

    #[test]
    fn unsatisfied_requirement_round_trips() {
        let failure = UnsatisfiedRequirement {
            kind: UnsatisfiedRequirementKind::InterfaceReceiver,
            target_id: "example.com/pkg:Store.Put".into(),
            detail: "receiver is interface Store".into(),
        };
        let json = serde_json::to_string(&failure).expect("serialize");
        let decoded: UnsatisfiedRequirement =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, failure);
    }

    #[test]
    fn get_invocation_plan_request_round_trips() {
        let request = Request::new(
            42,
            Command::GetInvocationPlan {
                requirements: vec![InvocationRequirement {
                    target_id: "example.com/pkg:Add".into(),
                    value_requirements: vec![],
                    runtime_requirements: vec![],
                }],
            },
        );
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(
            json.contains("\"command\":\"get_invocation_plan\""),
            "expected snake_case command tag, got: {json}"
        );
        assert!(
            json.contains("\"invocation_requirements\""),
            "expected Go-compatible field `invocation_requirements`, got: {json}"
        );
        let decoded: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, request);
    }

    #[test]
    fn invocation_plan_response_round_trips_populated() {
        let response = Response::new(
            42,
            ResponseResult::InvocationPlan {
                plans: vec![InvocationPlan {
                    target_id: "example.com/pkg:Add".into(),
                    receiver_kind: String::new(),
                    generic_type_args: vec![],
                    argument_plans: vec![ValuePlan {
                        param_index: 0,
                        param_name: "x".into(),
                        kind: ValuePlanKind::Zero,
                        literal: None,
                        type_hint: "int".into(),
                    }],
                    constructor_arg_plans: vec![],
                    priority: 1,
                    label: "zero_args".into(),
                }],
                unsatisfied_requirements: vec![UnsatisfiedRequirement {
                    kind: UnsatisfiedRequirementKind::CgoDependency,
                    target_id: "example.com/pkg:Native".into(),
                    detail: String::new(),
                }],
            },
        );
        let json = serde_json::to_string(&response).expect("serialize");
        assert!(
            json.contains("\"status\":\"invocation_plan\""),
            "expected snake_case status tag, got: {json}"
        );
        assert!(
            json.contains("\"invocation_plans\""),
            "expected Go-compatible field `invocation_plans`, got: {json}"
        );
        let decoded: Response = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, response);
    }

    #[test]
    fn go_response_invocation_plan_fixture_deserializes() {
        // Wire-shape fixture matching shatter-go/protocol/types.go Response
        // (InvocationPlans + UnsatisfiedRequirements under status
        // "invocation_plan"). Guards against cross-language field-name drift.
        let go_json = r#"{
            "protocol_version": "1.0.0",
            "id": 3,
            "status": "invocation_plan",
            "invocation_plans": [
                {
                    "target_id": "example.com/pkg:Add",
                    "receiver_kind": "",
                    "argument_plans": [],
                    "priority": 0,
                    "label": "free_function"
                }
            ],
            "unsatisfied_requirements": [
                {
                    "kind": "no_constructor",
                    "target_id": "example.com/pkg:(*Service).Run",
                    "detail": "receiver planning deferred"
                }
            ]
        }"#;
        let response: Response = serde_json::from_str(go_json).expect("deserialize");
        match response.result {
            ResponseResult::InvocationPlan {
                plans,
                unsatisfied_requirements,
            } => {
                assert_eq!(plans.len(), 1);
                assert_eq!(plans[0].target_id, "example.com/pkg:Add");
                assert_eq!(unsatisfied_requirements.len(), 1);
                assert_eq!(
                    unsatisfied_requirements[0].kind,
                    UnsatisfiedRequirementKind::NoConstructor
                );
            }
            other => panic!("expected InvocationPlan, got: {other:?}"),
        }
    }

    #[test]
    fn invocation_plan_response_round_trips_empty() {
        let response = Response::new(
            7,
            ResponseResult::InvocationPlan {
                plans: vec![],
                unsatisfied_requirements: vec![],
            },
        );
        let json = serde_json::to_string(&response).expect("serialize");
        let decoded: Response = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, response);
    }

    /// Byte-for-byte fixture mirroring what
    /// `shatter-go/protocol/invocation_plan.go` produces when marshaled via
    /// `encoding/json`. This is the cross-language compatibility check: if Go
    /// or Rust ever drifts on a field name or kind spelling, this test fails.
    #[test]
    fn go_invocation_plan_fixture_deserializes() {
        let go_json = r#"{
            "target_id": "example.com/pkg:Add",
            "receiver_kind": "constructor:NewCounter",
            "argument_plans": [
                {
                    "param_index": 0,
                    "param_name": "x",
                    "kind": "literal",
                    "literal": 7,
                    "type_hint": "int"
                },
                {
                    "param_index": 1,
                    "param_name": "y",
                    "kind": "symbolic",
                    "type_hint": "int"
                }
            ],
            "priority": 0,
            "label": "constructor_new_counter"
        }"#;
        let plan: InvocationPlan = serde_json::from_str(go_json).expect("deserialize");
        assert_eq!(plan.target_id, "example.com/pkg:Add");
        assert_eq!(plan.receiver_kind, "constructor:NewCounter");
        assert_eq!(plan.argument_plans.len(), 2);
        assert_eq!(plan.argument_plans[0].kind, ValuePlanKind::Literal);
        assert_eq!(
            plan.argument_plans[0].literal,
            Some(serde_json::json!(7))
        );
        assert_eq!(plan.argument_plans[1].kind, ValuePlanKind::Symbolic);
        assert_eq!(plan.argument_plans[1].literal, None);
        assert_eq!(plan.label, "constructor_new_counter");
    }

    #[test]
    fn go_invocation_requirement_fixture_deserializes() {
        let go_json = r#"{
            "target_id": "example.com/pkg:Put",
            "value_requirements": [
                {
                    "param_index": 0,
                    "param_name": "key",
                    "type_name": "string",
                    "kind": "non_zero"
                }
            ],
            "runtime_requirements": [
                {
                    "kind": "package_initialization",
                    "detail": "init() must run"
                }
            ]
        }"#;
        let req: InvocationRequirement =
            serde_json::from_str(go_json).expect("deserialize");
        assert_eq!(req.target_id, "example.com/pkg:Put");
        assert_eq!(
            req.value_requirements[0].kind,
            ValueRequirementKind::NonZero
        );
        assert_eq!(
            req.runtime_requirements[0].kind,
            RuntimeRequirementKind::PackageInitialization
        );
        assert_eq!(req.runtime_requirements[0].type_name, "");
    }

    #[test]
    fn go_unsatisfied_requirement_fixture_deserializes() {
        let go_json = r#"{
            "kind": "cgo_dependency",
            "target_id": "example.com/pkg:Native",
            "detail": "package uses cgo"
        }"#;
        let failure: UnsatisfiedRequirement =
            serde_json::from_str(go_json).expect("deserialize");
        assert_eq!(
            failure.kind,
            UnsatisfiedRequirementKind::CgoDependency
        );
        assert_eq!(failure.target_id, "example.com/pkg:Native");
    }

    #[test]
    fn go_unsatisfied_requires_construction_fixture_deserializes() {
        let go_json = r#"{
            "kind": "requires_construction",
            "target_id": "example.com/pkg:(*Service).Run",
            "detail": "receiver type *Service requires construction"
        }"#;
        let failure: UnsatisfiedRequirement =
            serde_json::from_str(go_json).expect("deserialize");
        assert_eq!(
            failure.kind,
            UnsatisfiedRequirementKind::RequiresConstruction
        );
        assert_eq!(
            failure.target_id,
            "example.com/pkg:(*Service).Run"
        );
    }

    #[test]
    fn go_response_requires_construction_in_invocation_plan() {
        let go_json = r#"{
            "protocol_version": "1.0.0",
            "id": 5,
            "status": "invocation_plan",
            "invocation_plans": [],
            "unsatisfied_requirements": [
                {
                    "kind": "requires_construction",
                    "target_id": "example.com/pkg:(*Handler).Serve",
                    "detail": "receiver needs construction"
                }
            ]
        }"#;
        let response: Response = serde_json::from_str(go_json).expect("deserialize");
        match response.result {
            ResponseResult::InvocationPlan {
                unsatisfied_requirements,
                ..
            } => {
                assert_eq!(unsatisfied_requirements.len(), 1);
                assert_eq!(
                    unsatisfied_requirements[0].kind,
                    UnsatisfiedRequirementKind::RequiresConstruction
                );
            }
            other => panic!("expected InvocationPlan, got: {other:?}"),
        }
    }

    #[test]
    fn invocation_outcome_round_trips() {
        let outcome = InvocationOutcome {
            status: OutcomeStatus::CompletedWithFindings,
            short_reason: Some("completed with findings".into()),
            return_value: Some(serde_json::json!({"status": 200})),
            thrown_error: Some(ErrorInfo {
                error_type: "warning".into(),
                message: "partial support".into(),
                stack: None,
                error_category: None,
            }),
            side_effects: vec![SideEffect::ConsoleOutput {
                level: "warn".into(),
                message: "degraded execution".into(),
            }],
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let decoded: InvocationOutcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, outcome);
    }

    /// Round-trip an Execute command carrying an InvocationPlan (str-hy9b.H5).
    /// Verifies the new `plan` field serializes under the expected key, omits
    /// when None, and survives a Rust-only round-trip.
    #[test]
    fn execute_command_with_plan_round_trips() {
        let plan = InvocationPlan {
            target_id: "example.com/svc:(*Service).DoIt".into(),
            receiver_kind: "constructor:New".into(),
            generic_type_args: vec![],
            argument_plans: vec![ValuePlan {
                param_index: 0,
                param_name: "x".into(),
                kind: ValuePlanKind::Literal,
                literal: Some(serde_json::json!(7)),
                type_hint: "int".into(),
            }],
            constructor_arg_plans: vec![],
            priority: 0,
            label: "ctor_new".into(),
        };
        let request = Request::new(
            17,
            Command::Execute {
                function: "(*Service).DoIt".into(),
                inputs: vec![serde_json::json!(7)],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: Some(plan.clone()),
            },
        );
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(
            json.contains("\"plan\""),
            "expected plan field present, got: {json}"
        );
        assert!(
            json.contains("\"receiver_kind\":\"constructor:New\""),
            "expected receiver_kind in plan, got: {json}"
        );
        let decoded: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, request);
    }

    /// Round-trip an Execute command with no plan: `plan` MUST be omitted from
    /// the JSON output so the wire shape stays bit-identical to pre-H5
    /// behavior. This is the parity guarantee for the additive field.
    #[test]
    fn execute_command_without_plan_omits_field() {
        let request = Request::new(
            18,
            Command::Execute {
                function: "Add".into(),
                inputs: vec![serde_json::json!(1), serde_json::json!(2)],
                mocks: vec![],
                setup_context: None,
                capture: true,
                prepare_id: None,
                execution_profile: None,
                plan: None,
            },
        );
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(
            !json.contains("\"plan\""),
            "expected plan field omitted when None, got: {json}"
        );
        let decoded: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, request);
    }

    /// Round-trip an Execute response carrying an `outcome` field (str-hy9b.H5
    /// step 2b). The Go frontend always emits `outcome`; previously the Rust
    /// parser silently dropped it. This test locks the new Rust-side field so
    /// callers can read `outcome.status` after a round-trip.
    #[test]
    fn execute_result_with_outcome_round_trips() {
        let outcome = InvocationOutcome {
            status: OutcomeStatus::Completed,
            short_reason: None,
            return_value: Some(serde_json::json!(1)),
            thrown_error: None,
            side_effects: vec![],
        };
        let result = ExecuteResult {
            return_value: Some(serde_json::json!(1)),
            outcome: Some(outcome.clone()),
            ..Default::default()
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(
            json.contains("\"outcome\""),
            "expected outcome field present, got: {json}"
        );
        assert!(
            json.contains("\"status\":\"completed\""),
            "expected outcome.status='completed', got: {json}"
        );
        let decoded: ExecuteResult =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.outcome, Some(outcome));
    }

    /// Wire-shape fixture: a Go-emitted Execute response with `outcome`
    /// populated must round-trip cleanly into the Rust `ExecuteResult`,
    /// preserving outcome.status. Mirrors the JSON shape produced by
    /// `shatter-go/protocol/handler.go::outcomeFromResult`.
    #[test]
    fn go_execute_response_with_outcome_deserializes() {
        let go_json = r#"{
            "protocol_version": "1.0.0",
            "id": 9,
            "status": "execute",
            "return_value": 1,
            "branch_path": [],
            "lines_executed": [],
            "performance": {
                "wall_time_ms": 0.0,
                "cpu_time_us": 0,
                "heap_used_bytes": 0,
                "heap_allocated_bytes": 0
            },
            "outcome": {
                "status": "completed",
                "return_value": 1
            }
        }"#;
        let response: Response = serde_json::from_str(go_json).expect("deserialize");
        match response.result {
            ResponseResult::Execute(er) => {
                let outcome = er.outcome.expect("outcome should be parsed");
                assert_eq!(outcome.status, OutcomeStatus::Completed);
                assert_eq!(outcome.return_value, Some(serde_json::json!(1)));
            }
            other => panic!("expected Execute, got: {other:?}"),
        }
    }
}
