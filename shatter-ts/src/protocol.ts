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

export type Command =
  | "handshake"
  | "analyze"
  | "instrument"
  | "execute"
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
}

export interface InstrumentRequest extends BaseRequest {
  command: "instrument";
  file: string;
  function: string;
  mocks: MockConfig[];
}

export interface ExecuteRequest extends BaseRequest {
  command: "execute";
  function: string;
  inputs: unknown[];
  mocks: MockConfig[];
}

export interface ShutdownRequest extends BaseRequest {
  command: "shutdown";
}

export type Request =
  | HandshakeRequest
  | AnalyzeRequest
  | InstrumentRequest
  | ExecuteRequest
  | ShutdownRequest;

// ---------------------------------------------------------------------------
// Response: Frontend → Core
// ---------------------------------------------------------------------------

export type ResponseStatus =
  | "handshake"
  | "analyze"
  | "instrument"
  | "execute"
  | "shutdown_ack"
  | "error";

export type ErrorCode =
  | "file_not_found"
  | "function_not_found"
  | "parse_error"
  | "instrumentation_failed"
  | "execution_timeout"
  | "execution_crash"
  | "version_mismatch"
  | "invalid_request"
  | "internal_error";

interface BaseResponse {
  protocol_version: string;
  id: number;
  status: ResponseStatus;
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
  | ShutdownAckResponse
  | ErrorResponse;

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

export interface FunctionAnalysis {
  name: string;
  params: ParamInfo[];
  branches: BranchInfo[];
  dependencies: ExternalDependency[];
  return_type: TypeInfo;
  start_line: number;
  end_line: number;
}

export interface ParamInfo {
  name: string;
  type: TypeInfo;
}

export type TypeInfo =
  | { kind: "int" }
  | { kind: "float" }
  | { kind: "str" }
  | { kind: "bool" }
  | { kind: "unknown" }
  | { kind: "array"; element: TypeInfo }
  | { kind: "object"; fields: [string, TypeInfo][] }
  | { kind: "union"; variants: TypeInfo[] }
  | { kind: "nullable"; inner: TypeInfo };

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
  | { kind: "const"; type: "undefined" };

export type BinOpKind =
  | "eq" | "ne" | "lt" | "le" | "gt" | "ge"
  | "add" | "sub" | "mul" | "div" | "mod"
  | "and" | "or"
  | "bitwise_and" | "bitwise_or" | "bitwise_xor"
  | "in" | "instance_of";

export type UnOpKind = "not" | "neg" | "bitwise_not" | "typeof";

export interface BranchDecision {
  branch_id: number;
  line: number;
  taken: boolean;
  constraint: SymConstraint;
}

export type SymConstraint =
  | { kind: "expr"; expr: SymExpr }
  | { kind: "unknown"; hint: string };

export interface ExternalCall {
  symbol: string;
  args: unknown[];
  return_value: unknown;
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
  | { kind: "thrown_error"; error_type: string; message: string; stack: string | null }
  | { kind: "global_state_change"; variable: string; before: unknown; after: unknown };

export interface ErrorInfo {
  error_type: string;
  message: string;
  stack: string | null;
}

export interface PerformanceMetrics {
  wall_time_ms: number;
  cpu_time_us: number;
  heap_used_bytes: number;
  heap_allocated_bytes: number;
}
