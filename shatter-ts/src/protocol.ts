/**
 * Protocol types for communication between the Rust core and TypeScript frontend.
 *
 * All messages are newline-delimited JSON over stdin/stdout. The core sends
 * Request messages and receives Response messages back. Every message includes
 * a protocol version for compatibility checking.
 */

// Command, ResponseStatus, ErrorCode (+ ALL_ERROR_CODES), SetupLevel,
// GeneratorKind, and BranchType — plus the PROTOCOL_VERSION constant — are
// generated from `protocol/registry.yaml` by `scripts/protocol-codegen.py`
// (str-1hlk.7). They live in `./generated/protocol-enums.ts` and are
// re-exported here so existing callers continue importing from
// `./protocol`. Do not redefine these unions in this file — the generated
// module is the only source of truth.
export {
  PROTOCOL_VERSION,
  ALL_COMMANDS,
  ALL_RESPONSE_STATUSES,
  ALL_ERROR_CODES,
  ALL_SETUP_LEVELS,
  ALL_GENERATOR_KINDS,
  ALL_BRANCH_TYPES,
} from "./generated/protocol-enums.js";
export type {
  Command,
  ResponseStatus,
  ErrorCode,
  SetupLevel,
  GeneratorKind,
  BranchType,
} from "./generated/protocol-enums.js";

import type {
  Command,
  SetupLevel,
  GeneratorKind,
  ResponseStatus,
  ErrorCode,
  BranchType,
} from "./generated/protocol-enums.js";

export const FRONTEND_LANGUAGE = "typescript";

// ---------------------------------------------------------------------------
// Request: Core → Frontend
// ---------------------------------------------------------------------------

export type SetupMode = "per_function" | "per_execution";

export interface SetupContextEntry {
  level: SetupLevel;
  context: unknown;
}

export interface SetupContextStack {
  contexts: SetupContextEntry[];
}

export type ExecutionAdapterApply =
  | "required"
  | "auto"
  | "suggest"
  | "disabled";

export interface ExecutionAdapter {
  id: string;
  apply?: ExecutionAdapterApply;
  options?: unknown;
}

export interface AdapterRelation {
  adapter_id: string;
  reason?: string;
}

export interface AdapterHint {
  adapter: ExecutionAdapter;
  confidence?: "low" | "medium" | "high";
  reasons?: string[];
  requirements?: AdapterRelation[];
  conflicts?: AdapterRelation[];
}

export interface ExecutionProfile {
  adapters: ExecutionAdapter[];
}

export interface MockConfig {
  symbol: string;
  return_values: unknown[];
  should_track_calls: boolean;
  default_behavior:
    | "return_generated"
    | "repeat_last"
    | "throw_error"
    | "passthrough";
}

interface BaseRequest {
  protocol_version: string;
  id: number;
  command: Command;
}

export interface HandshakeRequest extends BaseRequest {
  command: "handshake";
  capabilities: string[];
}

export interface AnalyzeRequest extends BaseRequest {
  command: "analyze";
  file: string;
  function?: string | null;
  project_root?: string | null;
  execution_profile?: ExecutionProfile | null;
}

export interface InstrumentRequest extends BaseRequest {
  command: "instrument";
  file: string;
  function: string;
  mocks: MockConfig[];
  project_root?: string | null;
  execution_profile?: ExecutionProfile | null;
}

export interface PrepareRequest extends BaseRequest {
  command: "prepare";
  file: string;
  function: string;
  mocks: MockConfig[];
  project_root?: string | null;
  execution_profile?: ExecutionProfile | null;
}

export interface ExecuteRequest extends BaseRequest {
  command: "execute";
  function: string;
  inputs: unknown[];
  mocks: MockConfig[];
  setup_context?: SetupContextStack | null;
  /** Opaque handle from a prior prepare command. When set, reuses cached artifacts. */
  prepare_id?: string | null;
  /** When false, skip side-effect capture (console/process interception) for lower overhead. Defaults to true. */
  capture?: boolean;
  execution_profile?: ExecutionProfile | null;
}

export interface SetupRequest extends BaseRequest {
  command: "setup";
  file: string;
  scope: string;
  level: SetupLevel;
  parent_context?: SetupContextStack | null;
  project_root?: string | null;
  execution_profile?: ExecutionProfile | null;
}

export interface TeardownRequest extends BaseRequest {
  command: "teardown";
  scope: string;
  level: SetupLevel;
}

export interface GenerateRequest extends BaseRequest {
  command: "generate";
  file: string;
  name: string;
  kind: GeneratorKind;
  recipe?: unknown;
  project_root?: string | null;
}

export interface ShutdownRequest extends BaseRequest {
  command: "shutdown";
}

export type Request =
  | HandshakeRequest
  | AnalyzeRequest
  | InstrumentRequest
  | PrepareRequest
  | ExecuteRequest
  | SetupRequest
  | TeardownRequest
  | GenerateRequest
  | ShutdownRequest;

// ---------------------------------------------------------------------------
// Response: Frontend → Core
// ---------------------------------------------------------------------------

interface BaseResponse {
  protocol_version: string;
  id: number;
  status: ResponseStatus;
  timing?: TimingSummary;
}

export interface HandshakeResponse extends BaseResponse {
  status: "handshake";
  frontend_version: string;
  language: string;
  capabilities: string[];
}

export interface AnalyzeResponse extends BaseResponse {
  status: "analyze";
  functions: FunctionAnalysis[];
}

export interface InstrumentResponse extends BaseResponse {
  status: "instrument";
  instrumented: boolean;
  output_file: string | null;
  instrumentable_line_count?: number;
}

export interface PrepareResponse extends BaseResponse {
  status: "prepare";
  prepare_id: string;
}

export interface ExecuteResponse extends BaseResponse {
  status: "execute";
  return_value: unknown;
  thrown_error: ErrorInfo | null;
  branch_path: BranchDecision[];
  lines_executed: number[];
  calls_to_external: ExternalCall[];
  path_constraints: SymConstraint[];
  side_effects: SideEffect[];
  performance: PerformanceMetrics;
  capture_truncation?: TruncationInfo;
  scope_events?: TraceEvent[];
  loop_body_states?: LoopBodyState[];
  discovered_dependencies?: DiscoveredDependency[];
  connection_failures?: ConnectionFailure[];
  /** Cryptographic boundaries intercepted at runtime.
   *  When non-empty, the function called a known encrypt or decrypt API;
   *  the core engine can use this for boundary splitting. */
  runtime_crypto_boundaries?: RuntimeCryptoBoundary[];
  /** Runtime-detected adapter hints for failures that suggest a missing adapter. */
  adapter_hints?: AdapterHint[];
  /** Standardized invocation outcome (str-hy9b.A1/A5). Always emitted by this frontend
   *  for execute responses. Status is one of completed | runtime_failed | timed_out
   *  on the executor's primary success/failure paths; build_failed and unsupported
   *  arrive on `error` responses. */
  outcome?: InvocationOutcome;
}

export interface SetupResponse extends BaseResponse {
  status: "setup";
  setup_context: unknown;
}

export interface TeardownAckResponse extends BaseResponse {
  status: "teardown_ack";
}

export interface GenerateResponse extends BaseResponse {
  status: "generate";
  value: unknown;
  generator_id: string;
  recipe?: unknown;
}

export interface ShutdownAckResponse extends BaseResponse {
  status: "shutdown_ack";
}

export interface ErrorResponse extends BaseResponse {
  status: "error";
  code: ErrorCode;
  message: string;
  details?: unknown;
}

export type Response =
  | HandshakeResponse
  | AnalyzeResponse
  | InstrumentResponse
  | PrepareResponse
  | ExecuteResponse
  | SetupResponse
  | TeardownAckResponse
  | GenerateResponse
  | ShutdownAckResponse
  | ErrorResponse;

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/** A literal constant extracted from source code for use as a candidate test input. */
export type LiteralValue =
  | { type: "int"; value: number }
  | { type: "float"; value: number }
  | { type: "str"; value: string }
  | { type: "bool"; value: boolean }
  | { type: "regex"; pattern: string };

export interface CryptoBoundary {
  symbol: string;
  source_module: string;
  direction: "encrypt" | "decrypt" | "both";
  output?:
    | "ciphertext"
    | "plaintext"
    | "key"
    | "hash"
    | "signature"
    | "verified";
  confidence?: "low" | "medium" | "high";
  param_roles: Record<
    string,
    "key" | "data" | "iv" | "nonce" | "tag" | "aad" | "algorithm"
  >;
  call_sites: number[];
  input_entropy?: number;
  output_entropy?: number;
}

/**
 * A cryptographic boundary detected at execution time.
 *
 * The instrumented frontend injects `__shatter_crypto_boundary()` calls when it
 * encounters known encrypt/decrypt API calls. Those calls populate this structure
 * with key, IV, and algorithm values captured from the actual runtime arguments.
 *
 * The core engine uses these records to apply boundary splitting: constraints on
 * the decrypted plaintext are solved, then the plaintext is re-encrypted to
 * produce a valid ciphertext test input.
 */
export interface RuntimeCryptoBoundary {
  /** Stable identifier for this boundary within the execution trace, e.g. `"cb-0"`. */
  boundary_id: string;
  /** Whether this is an encrypt or decrypt boundary. */
  kind: "encrypt" | "decrypt";
  /** Function name as it appears in the source, e.g. `"createDecipheriv"`. */
  function_name: string;
  /** Algorithm string captured at runtime, e.g. `"aes-256-cbc"`. */
  algorithm?: string;
  /** Index of the argument holding the ciphertext. Absent when ciphertext is
   *  passed to a separate method call (e.g. `decipher.update(ciphertext)`). */
  ciphertext_param_index?: number;
  /** Base64-encoded key bytes captured at runtime. */
  key_value?: string;
  /** Base64-encoded IV bytes captured at runtime. */
  iv_value?: string;
}

export type BoundOp = "lt" | "le" | "gt" | "ge";

export interface InductionVar {
  name: string;
  init_expr: SymExpr;
  step_expr: SymExpr;
  bound_expr: SymExpr;
  bound_op: BoundOp;
}

export interface LoopInfo {
  loop_id: number;
  line: number;
  induction_var: InductionVar;
}

export interface LoopBodyState {
  loop_id: number;
  iteration: number;
  locals: Record<string, SymExpr>;
}

export interface FunctionAnalysis {
  name: string;
  exported?: boolean;
  params: ParamInfo[];
  branches: BranchInfo[];
  dependencies: ExternalDependency[];
  return_type: TypeInfo;
  start_line: number;
  end_line: number;
  literals?: LiteralValue[];
  crypto_boundaries?: CryptoBoundary[];
  loops?: LoopInfo[];
  source_file?: string;
  adapter_hints?: AdapterHint[];
  invocation_model?: InvocationModel;
}

export type InvocationModel =
  | { kind: "direct" }
  | {
      kind: "adapter";
      adapter_id: string;
      synthetic_params?: ParamInfo[];
      scenario_schema?: unknown;
    };

export interface ParamInfo {
  name: string;
  type: TypeInfo;
  type_name?: string;
}

/** Well-known complex types beyond primitives and structural types. */
export type ComplexKind =
  | "date"
  | "date_time"
  | "time"
  | "duration"
  | "reg_exp"
  | "char"
  | "symbol"
  | "big_int"
  | "big_decimal"
  | "complex"
  | "rational"
  | "range"
  | "buffer"
  | "bit_set"
  | "error"
  | "option"
  | "result"
  | "closure"
  | "iterator"
  | "url"
  | "ip_address"
  | "uuid"
  | "path"
  | "money"
  | "sem_ver"
  | "email"
  | "mime_type"
  | "color"
  | "geo_point"
  | "locale"
  | "rune"
  | "go_byte";

/** Reason a type was detected as opaque via static analysis. */
export type StaticOpacityReason =
  | "no_constructor"
  | "transitively_opaque"
  | "abstract_type"
  | "no_implementors";

/** Reason a type was detected as potentially opaque via medium-confidence static analysis. */
export type MediumOpacityReason =
  | "infrastructure_package"
  | "closeable_interface"
  | "native_handle_field";

export type TypeInfo =
  | { kind: "int" }
  | { kind: "float" }
  | { kind: "str" }
  | { kind: "bool" }
  | { kind: "unknown" }
  | { kind: "array"; element: TypeInfo }
  | { kind: "object"; fields: [string, TypeInfo][] }
  | { kind: "union"; variants: TypeInfo[] }
  | { kind: "nullable"; inner: TypeInfo }
  | {
      kind: "complex";
      complex_kind: ComplexKind;
      metadata?: Record<string, unknown>;
      inner?: TypeInfo;
    }
  | {
      kind: "opaque";
      label: string;
      static_opacity?: StaticOpacityReason;
      medium_opacity?: MediumOpacityReason;
    };

export interface BranchInfo {
  id: number;
  line: number;
  condition_text: string;
  condition: SymExpr | null;
  branch_type: BranchType;
}

export type SymExpr =
  | { kind: "param"; name: string; path: string[] }
  | SymExprConst
  | { kind: "bin_op"; op: BinOpKind; left: SymExpr; right: SymExpr }
  | { kind: "un_op"; op: UnOpKind; operand: SymExpr }
  | { kind: "call"; name: string; receiver: SymExpr | null; args: SymExpr[] }
  | { kind: "ite"; condition: SymExpr; then_expr: SymExpr; else_expr: SymExpr }
  | { kind: "unknown" };

/** Const variant of SymExpr. Flattened to match Rust serde serialization. */
export type SymExprConst =
  | { kind: "const"; type: "int"; value: number }
  | { kind: "const"; type: "float"; value: number }
  | { kind: "const"; type: "str"; value: string }
  | { kind: "const"; type: "bool"; value: boolean }
  | { kind: "const"; type: "null" }
  | { kind: "const"; type: "undefined" }
  | {
      kind: "const";
      type: "complex";
      value: { kind: ComplexKind; repr: SymExprConst };
    };

export type BinOpKind =
  | "eq"
  | "ne"
  | "lt"
  | "le"
  | "gt"
  | "ge"
  | "add"
  | "sub"
  | "mul"
  | "div"
  | "mod"
  | "and"
  | "or"
  | "bitwise_and"
  | "bitwise_or"
  | "bitwise_xor"
  | "in"
  | "instance_of";

export type UnOpKind = "not" | "neg" | "bitwise_not" | "typeof";

export interface ConditionOutcome {
  condition_index: number;
  value: boolean | null;
  masked?: boolean;
  constraint: SymConstraint;
}

export interface BranchDecision {
  branch_id: number;
  line: number;
  taken: boolean;
  constraint: SymConstraint;
  /** Per-condition outcomes for MC/DC. Present only in MC/DC mode for compound decisions. */
  conditions?: ConditionOutcome[];
}

export type ScopeEvent =
  | { kind: "loop_enter"; loop_id: number }
  | { kind: "loop_exit"; loop_id: number }
  | { kind: "call_enter"; call_site_id: number }
  | { kind: "call_exit"; call_site_id: number };

export type TraceEvent =
  | { type: "branch"; decision: BranchDecision }
  | { type: "scope"; event: ScopeEvent };

export type SymConstraint =
  | { kind: "expr"; expr: SymExpr }
  | { kind: "unknown"; hint: string };

export interface ExternalCall {
  symbol: string;
  args: unknown[];
  return_value: unknown;
}

export type ConnectionFailureKind =
  | "connection_refused"
  | "dns_failure"
  | "auth_error"
  | "timeout"
  | "other";

export interface ConnectionFailure {
  symbol: string;
  error_kind: ConnectionFailureKind;
  message: string;
}

export type DepDetectionKind =
  | "unmocked_import"
  | "subprocess_spawn"
  | "stubbed_import";

export interface DiscoveredDependency {
  symbol: string;
  source_module: string;
  kind: DepDetectionKind;
  is_subprocess_spawn: boolean;
}

export interface ExternalDependency {
  kind: DependencyKind;
  symbol: string;
  source_module: string;
  return_type: TypeInfo;
  param_types: TypeInfo[];
  call_sites: number[];
}

export type DependencyKind =
  | "function_call"
  | "method_call"
  | "property_access"
  | "module_import";

export type SideEffect =
  | { kind: "console_output"; level: string; message: string }
  | { kind: "file_write"; path: string; content: string }
  | { kind: "network_request"; method: string; url: string; body: unknown }
  | { kind: "environment_read"; variable: string; value: string | null }
  | { kind: "global_mutation"; name: string }
  | {
      kind: "thrown_error";
      error_type: string;
      message: string;
      stack: string | null;
    }
  | {
      kind: "global_state_change";
      variable: string;
      before: unknown;
      after: unknown;
    };

export type ErrorCategory =
  | "validation"
  | "runtime"
  | "infrastructure"
  | "unknown";

export interface ErrorInfo {
  error_type: string;
  message: string;
  stack: string | null;
  error_category?: ErrorCategory;
}

export type OutcomeStatus =
  | "completed"
  | "completed_with_findings"
  | "unsupported"
  | "build_failed"
  | "runtime_failed"
  | "timed_out"
  | "skipped_by_policy"
  // str-jeen.40: env-preflight failure (e.g. missing node_modules).
  | "preflight_failed";

/** Reusable protocol contract for one invocation result. */
export interface InvocationOutcome {
  status: OutcomeStatus;
  /** One human-readable sentence summarizing why the invocation reached this status. Required (non-empty) for any non-completed status. */
  short_reason?: string;
  return_value?: unknown;
  thrown_error?: ErrorInfo;
  side_effects?: SideEffect[];
}

export interface TruncationInfo {
  was_truncated: boolean;
  original_lines: number;
  original_bytes: number;
}

export interface PerformanceMetrics {
  wall_time_ms: number;
  cpu_time_us: number;
  heap_used_bytes: number;
  heap_allocated_bytes: number;
}

export interface TimingSummary {
  phases: TimingPhaseSummary[];
}

export interface TimingPhaseSummary {
  phase_path: string;
  total_ms: number;
  self_ms: number;
  count: number;
  attributes?: Record<string, string>;
}
