/**
 * Protocol types for communication between the Rust core and TypeScript frontend.
 *
 * All messages are newline-delimited JSON over stdin/stdout. The core sends
 * Request messages and receives Response messages back. Every message includes
 * a protocol version for compatibility checking.
 */

export const PROTOCOL_VERSION = "0.1.0";
export const FRONTEND_LANGUAGE = "typescript";

// ---------------------------------------------------------------------------
// Request: Core → Frontend
// ---------------------------------------------------------------------------

export type SetupMode = "per_function" | "per_execution";

export type SetupLevel = "session" | "file" | "function" | "execution";

export interface SetupContextEntry {
  level: SetupLevel;
  context: unknown;
}

export interface SetupContextStack {
  contexts: SetupContextEntry[];
}

export type GeneratorKind = "type_name" | "param_name";

export type Command =
  | "handshake"
  | "analyze"
  | "instrument"
  | "execute"
  | "setup"
  | "teardown"
  | "generate"
  | "shutdown";

export interface MockConfig {
  symbol: string;
  return_values: unknown[];
  should_track_calls: boolean;
  default_behavior: "return_generated" | "repeat_last" | "throw_error" | "passthrough";
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
}

export interface InstrumentRequest extends BaseRequest {
  command: "instrument";
  file: string;
  function: string;
  mocks: MockConfig[];
  project_root?: string | null;
}

export interface ExecuteRequest extends BaseRequest {
  command: "execute";
  function: string;
  inputs: unknown[];
  mocks: MockConfig[];
  setup_context?: SetupContextStack | null;
  /** When false, skip side-effect capture (console/process interception) for lower overhead. Defaults to true. */
  capture?: boolean;
}

export interface SetupRequest extends BaseRequest {
  command: "setup";
  file: string;
  scope: string;
  level: SetupLevel;
  parent_context?: SetupContextStack | null;
  project_root?: string | null;
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
  | ExecuteRequest
  | SetupRequest
  | TeardownRequest
  | GenerateRequest
  | ShutdownRequest;

// ---------------------------------------------------------------------------
// Response: Frontend → Core
// ---------------------------------------------------------------------------

export type ResponseStatus =
  | "handshake"
  | "analyze"
  | "instrument"
  | "execute"
  | "setup"
  | "teardown_ack"
  | "generate"
  | "shutdown_ack"
  | "error";

/** Canonical error codes matching protocol/registry.yaml (11 codes). */
export const ALL_ERROR_CODES = [
  "file_not_found",
  "function_not_found",
  "parse_error",
  "instrumentation_failed",
  "execution_timeout",
  "execution_crash",
  "version_mismatch",
  "invalid_request",
  "compilation_error",
  "internal_error",
  "not_supported",
] as const;

export type ErrorCode = (typeof ALL_ERROR_CODES)[number];

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
  discovered_dependencies?: DiscoveredDependency[];
  connection_failures?: ConnectionFailure[];
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
  output?: "ciphertext" | "plaintext" | "key" | "hash" | "signature" | "verified";
  confidence?: "low" | "medium" | "high";
  param_roles: Record<string, "key" | "data" | "iv" | "nonce" | "tag" | "aad" | "algorithm">;
  call_sites: number[];
  input_entropy?: number;
  output_entropy?: number;
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
}

export interface ParamInfo {
  name: string;
  type: TypeInfo;
  type_name?: string;
}

/** Well-known complex types beyond primitives and structural types. */
export type ComplexKind =
  | "date" | "date_time" | "time" | "duration"
  | "reg_exp" | "char" | "symbol"
  | "big_int" | "big_decimal" | "complex" | "rational" | "range"
  | "buffer" | "bit_set"
  | "error" | "option" | "result"
  | "closure" | "iterator"
  | "url" | "ip_address"
  | "uuid"
  | "path"
  | "money" | "sem_ver" | "email" | "mime_type" | "color" | "geo_point" | "locale"
  | "rune" | "go_byte";

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
  | { kind: "complex"; complex_kind: ComplexKind; metadata?: Record<string, unknown>; inner?: TypeInfo }
  | { kind: "opaque"; label: string; static_opacity?: StaticOpacityReason; medium_opacity?: MediumOpacityReason };

export interface BranchInfo {
  id: number;
  line: number;
  condition_text: string;
  condition: SymExpr | null;
  branch_type: BranchType;
}

export type BranchType =
  | "if"
  | "else_if"
  | "switch"
  | "ternary"
  | "logical_and"
  | "logical_or"
  | "while"
  | "for";

export type SymExpr =
  | { kind: "param"; name: string; path: string[] }
  | SymExprConst
  | { kind: "bin_op"; op: BinOpKind; left: SymExpr; right: SymExpr }
  | { kind: "un_op"; op: UnOpKind; operand: SymExpr }
  | { kind: "call"; name: string; receiver: SymExpr | null; args: SymExpr[] }
  | { kind: "unknown" };

/** Const variant of SymExpr. Flattened to match Rust serde serialization. */
export type SymExprConst =
  | { kind: "const"; type: "int"; value: number }
  | { kind: "const"; type: "float"; value: number }
  | { kind: "const"; type: "str"; value: string }
  | { kind: "const"; type: "bool"; value: boolean }
  | { kind: "const"; type: "null" }
  | { kind: "const"; type: "undefined" }
  | { kind: "const"; type: "complex"; value: { kind: ComplexKind; repr: SymExprConst } };

export type BinOpKind =
  | "eq" | "ne" | "lt" | "le" | "gt" | "ge"
  | "add" | "sub" | "mul" | "div" | "mod"
  | "and" | "or"
  | "bitwise_and" | "bitwise_or" | "bitwise_xor"
  | "in" | "instance_of";

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

export type DepDetectionKind = "unmocked_import" | "subprocess_spawn";

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
  | { kind: "thrown_error"; error_type: string; message: string; stack: string | null }
  | { kind: "global_state_change"; variable: string; before: unknown; after: unknown };

export type ErrorCategory = "validation" | "runtime" | "infrastructure" | "unknown";

export interface ErrorInfo {
  error_type: string;
  message: string;
  stack: string | null;
  error_category?: ErrorCategory;
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
