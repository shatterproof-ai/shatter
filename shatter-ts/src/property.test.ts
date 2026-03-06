/**
 * Property-based tests using fast-check for protocol types and SymExpr.
 *
 * Validates serialization round-trips and structural invariants that
 * hand-written fixtures might miss.
 */
import { describe, it, expect } from "vitest";
import fc from "fast-check";
import type {
  BinOpKind,
  UnOpKind,
  SymExpr,
  SymExprConst,
  TypeInfo,
  ComplexKind,
  BranchType,
  BranchDecision,
  SymConstraint,
  ErrorCode,
  SideEffect,
  PerformanceMetrics,
  TraceEvent,
  ScopeEvent,
  HandshakeRequest,
  AnalyzeRequest,
  ExecuteRequest,
  ShutdownRequest,
  Request,
  HandshakeResponse,
  AnalyzeResponse,
  ErrorResponse,
  Response,
  FunctionAnalysis,
  ParamInfo,
  BranchInfo,
  ErrorInfo,
} from "./protocol.js";
import { PROTOCOL_VERSION } from "./protocol.js";

// ---------------------------------------------------------------------------
// Arbitraries — leaf types
// ---------------------------------------------------------------------------

const arbIdent = fc.stringMatching(/^[a-zA-Z_][a-zA-Z0-9_]{0,12}$/);
const arbShortString = fc.string({ maxLength: 30 });

const arbBinOpKind: fc.Arbitrary<BinOpKind> = fc.constantFrom(
  "eq", "ne", "lt", "le", "gt", "ge",
  "add", "sub", "mul", "div", "mod",
  "and", "or",
  "bitwise_and", "bitwise_or", "bitwise_xor",
  "in", "instance_of",
);

const arbUnOpKind: fc.Arbitrary<UnOpKind> = fc.constantFrom(
  "not", "neg", "bitwise_not", "typeof",
);

const arbComplexKind: fc.Arbitrary<ComplexKind> = fc.constantFrom(
  "date", "date_time", "time", "duration",
  "reg_exp", "char", "symbol",
  "big_int", "big_decimal", "complex", "rational", "range",
  "buffer", "bit_set",
  "error", "option", "result",
  "closure", "iterator",
  "url", "ip_address",
  "uuid", "path",
  "money", "sem_ver", "email", "mime_type", "color", "geo_point", "locale",
  "rune", "go_byte",
);

const arbBranchType: fc.Arbitrary<BranchType> = fc.constantFrom(
  "if", "else_if", "switch", "ternary",
  "logical_and", "logical_or", "while", "for",
);

const arbErrorCode: fc.Arbitrary<ErrorCode> = fc.constantFrom(
  "file_not_found", "function_not_found", "parse_error",
  "instrumentation_failed", "execution_timeout", "execution_crash",
  "version_mismatch", "invalid_request", "internal_error",
);

// ---------------------------------------------------------------------------
// Arbitraries — recursive types
// ---------------------------------------------------------------------------

const arbSymExprConst: fc.Arbitrary<SymExprConst> = fc.oneof(
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("int" as const), value: fc.integer({ min: -10000, max: 10000 }) }),
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("float" as const), value: fc.integer({ min: -1000, max: 1000 }) }),
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("str" as const), value: arbShortString }),
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("bool" as const), value: fc.boolean() }),
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("null" as const) }),
  fc.record({ kind: fc.constant("const" as const), type: fc.constant("undefined" as const) }),
);

const arbSymExpr: fc.Arbitrary<SymExpr> = fc.letrec<{ expr: SymExpr }>(tie => ({
  expr: fc.oneof(
    { depthIdentifier: "symexpr", maxDepth: 3 },
    fc.record({
      kind: fc.constant("param" as const),
      name: arbIdent,
      path: fc.array(arbIdent, { maxLength: 3 }),
    }),
    arbSymExprConst,
    fc.record({ kind: fc.constant("unknown" as const) }),
    fc.record({
      kind: fc.constant("bin_op" as const),
      op: arbBinOpKind,
      left: tie("expr"),
      right: tie("expr"),
    }),
    fc.record({
      kind: fc.constant("un_op" as const),
      op: arbUnOpKind,
      operand: tie("expr"),
    }),
  ),
})).expr;

const arbTypeInfoLeaf: fc.Arbitrary<TypeInfo> = fc.oneof(
  fc.record({ kind: fc.constant("int" as const) }),
  fc.record({ kind: fc.constant("float" as const) }),
  fc.record({ kind: fc.constant("str" as const) }),
  fc.record({ kind: fc.constant("bool" as const) }),
  fc.record({ kind: fc.constant("unknown" as const) }),
  fc.record({ kind: fc.constant("opaque" as const), label: arbIdent }),
);

const arbTypeInfo: fc.Arbitrary<TypeInfo> = fc.letrec<{ ti: TypeInfo }>(tie => ({
  ti: fc.oneof(
    { depthIdentifier: "typeinfo", maxDepth: 2 },
    arbTypeInfoLeaf,
    fc.record({ kind: fc.constant("array" as const), element: tie("ti") }),
    fc.record({
      kind: fc.constant("nullable" as const),
      inner: tie("ti"),
    }),
    fc.record({
      kind: fc.constant("union" as const),
      variants: fc.array(tie("ti"), { minLength: 2, maxLength: 4 }),
    }),
  ),
})).ti;

// ---------------------------------------------------------------------------
// Arbitraries — protocol records
// ---------------------------------------------------------------------------

const arbSymConstraint: fc.Arbitrary<SymConstraint> = fc.oneof(
  fc.record({ kind: fc.constant("expr" as const), expr: arbSymExpr }),
  fc.record({ kind: fc.constant("unknown" as const), hint: arbShortString }),
);

const arbBranchDecision: fc.Arbitrary<BranchDecision> = fc.record({
  branch_id: fc.nat(100),
  line: fc.integer({ min: 1, max: 500 }),
  taken: fc.boolean(),
  constraint: arbSymConstraint,
});

const arbScopeEvent: fc.Arbitrary<ScopeEvent> = fc.oneof(
  fc.record({ kind: fc.constant("loop_enter" as const), loop_id: fc.nat(20) }),
  fc.record({ kind: fc.constant("loop_exit" as const), loop_id: fc.nat(20) }),
  fc.record({ kind: fc.constant("call_enter" as const), call_site_id: fc.nat(20) }),
  fc.record({ kind: fc.constant("call_exit" as const), call_site_id: fc.nat(20) }),
);

const arbTraceEvent: fc.Arbitrary<TraceEvent> = fc.oneof(
  fc.record({ type: fc.constant("branch" as const), decision: arbBranchDecision }),
  fc.record({ type: fc.constant("scope" as const), event: arbScopeEvent }),
);

const arbPerformanceMetrics: fc.Arbitrary<PerformanceMetrics> = fc.record({
  wall_time_ms: fc.nat(10000),
  cpu_time_us: fc.nat(10000000),
  heap_used_bytes: fc.nat(100000000),
  heap_allocated_bytes: fc.nat(100000000),
});

const arbErrorInfo: fc.Arbitrary<ErrorInfo> = fc.record({
  error_type: arbIdent,
  message: arbShortString,
  stack: fc.option(arbShortString, { nil: null }),
});

const arbSideEffect: fc.Arbitrary<SideEffect> = fc.oneof(
  fc.record({ kind: fc.constant("console_output" as const), level: arbIdent, message: arbShortString }),
  fc.record({ kind: fc.constant("global_mutation" as const), name: arbIdent }),
  fc.record({
    kind: fc.constant("thrown_error" as const),
    error_type: arbIdent,
    message: arbShortString,
    stack: fc.option(arbShortString, { nil: null }),
  }),
);

const arbParamInfo: fc.Arbitrary<ParamInfo> = fc.record({
  name: arbIdent,
  type: arbTypeInfo,
});

const arbBranchInfo: fc.Arbitrary<BranchInfo> = fc.record({
  id: fc.nat(100),
  line: fc.integer({ min: 1, max: 500 }),
  condition_text: arbShortString,
  condition: fc.option(arbSymExpr, { nil: null }),
  branch_type: arbBranchType,
});

const arbFunctionAnalysis: fc.Arbitrary<FunctionAnalysis> = fc.record({
  name: arbIdent,
  exported: fc.boolean(),
  params: fc.array(arbParamInfo, { maxLength: 4 }),
  branches: fc.array(arbBranchInfo, { maxLength: 4 }),
  dependencies: fc.constant([]),
  return_type: arbTypeInfo,
  start_line: fc.integer({ min: 1, max: 500 }),
  end_line: fc.integer({ min: 1, max: 500 }),
});

// ---------------------------------------------------------------------------
// Arbitraries — full protocol messages
// ---------------------------------------------------------------------------

const arbRequest: fc.Arbitrary<Request> = fc.oneof(
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    command: fc.constant("handshake" as const),
    capabilities: fc.array(arbIdent, { maxLength: 3 }),
  }) as fc.Arbitrary<HandshakeRequest>,
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    command: fc.constant("analyze" as const),
    file: arbIdent,
  }) as fc.Arbitrary<AnalyzeRequest>,
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    command: fc.constant("execute" as const),
    function: arbIdent,
    inputs: fc.array(fc.oneof(fc.integer(), fc.constant("hello"), fc.boolean()), { maxLength: 4 }),
    mocks: fc.constant([]),
  }) as fc.Arbitrary<ExecuteRequest>,
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    command: fc.constant("shutdown" as const),
  }) as fc.Arbitrary<ShutdownRequest>,
);

const arbResponse: fc.Arbitrary<Response> = fc.oneof(
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    status: fc.constant("handshake" as const),
    frontend_version: fc.constant(PROTOCOL_VERSION),
    language: fc.constant("typescript"),
    capabilities: fc.array(arbIdent, { maxLength: 3 }),
  }) as fc.Arbitrary<HandshakeResponse>,
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    status: fc.constant("analyze" as const),
    functions: fc.array(arbFunctionAnalysis, { maxLength: 3 }),
  }) as fc.Arbitrary<AnalyzeResponse>,
  fc.record({
    protocol_version: fc.constant(PROTOCOL_VERSION),
    id: fc.nat(1000),
    status: fc.constant("error" as const),
    code: arbErrorCode,
    message: arbShortString,
  }) as fc.Arbitrary<ErrorResponse>,
);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("property: protocol message round-trips", () => {
  it("Request survives JSON round-trip", () => {
    fc.assert(
      fc.property(arbRequest, (req) => {
        const json = JSON.stringify(req);
        const decoded = JSON.parse(json) as Request;
        expect(decoded).toEqual(req);
      }),
    );
  });

  it("Response survives JSON round-trip", () => {
    fc.assert(
      fc.property(arbResponse, (resp) => {
        const json = JSON.stringify(resp);
        const decoded = JSON.parse(json) as Response;
        expect(decoded).toEqual(resp);
      }),
    );
  });
});

describe("property: SymExpr structural validity", () => {
  const VALID_KINDS = new Set(["param", "const", "bin_op", "un_op", "call", "unknown"]);

  it("every generated SymExpr has a valid kind tag", () => {
    fc.assert(
      fc.property(arbSymExpr, (expr) => {
        expect(VALID_KINDS.has(expr.kind)).toBe(true);
      }),
    );
  });

  it("bin_op always has left, right, and op", () => {
    fc.assert(
      fc.property(arbSymExpr, (expr) => {
        if (expr.kind === "bin_op") {
          expect(expr.left).toBeDefined();
          expect(expr.right).toBeDefined();
          expect(expr.op).toBeDefined();
        }
      }),
    );
  });

  it("un_op always has operand and op", () => {
    fc.assert(
      fc.property(arbSymExpr, (expr) => {
        if (expr.kind === "un_op") {
          expect(expr.operand).toBeDefined();
          expect(expr.op).toBeDefined();
        }
      }),
    );
  });

  it("param always has name and path", () => {
    fc.assert(
      fc.property(arbSymExpr, (expr) => {
        if (expr.kind === "param") {
          expect(typeof expr.name).toBe("string");
          expect(Array.isArray(expr.path)).toBe(true);
        }
      }),
    );
  });

  it("SymExpr round-trips through JSON", () => {
    fc.assert(
      fc.property(arbSymExpr, (expr) => {
        const json = JSON.stringify(expr);
        const decoded = JSON.parse(json) as SymExpr;
        expect(decoded).toEqual(expr);
      }),
    );
  });
});

describe("property: TypeInfo structural validity", () => {
  const VALID_KINDS = new Set([
    "int", "float", "str", "bool", "unknown", "opaque",
    "array", "object", "union", "nullable", "complex",
  ]);

  it("every generated TypeInfo has a valid kind tag", () => {
    fc.assert(
      fc.property(arbTypeInfo, (ti) => {
        expect(VALID_KINDS.has(ti.kind)).toBe(true);
      }),
    );
  });

  it("TypeInfo round-trips through JSON", () => {
    fc.assert(
      fc.property(arbTypeInfo, (ti) => {
        const json = JSON.stringify(ti);
        const decoded = JSON.parse(json) as TypeInfo;
        expect(decoded).toEqual(ti);
      }),
    );
  });
});

describe("property: BranchDecision round-trips", () => {
  it("BranchDecision survives JSON round-trip", () => {
    fc.assert(
      fc.property(arbBranchDecision, (bd) => {
        const json = JSON.stringify(bd);
        const decoded = JSON.parse(json) as BranchDecision;
        expect(decoded).toEqual(bd);
      }),
    );
  });
});

describe("property: TraceEvent round-trips", () => {
  it("TraceEvent survives JSON round-trip", () => {
    fc.assert(
      fc.property(arbTraceEvent, (te) => {
        const json = JSON.stringify(te);
        const decoded = JSON.parse(json) as TraceEvent;
        expect(decoded).toEqual(te);
      }),
    );
  });
});
