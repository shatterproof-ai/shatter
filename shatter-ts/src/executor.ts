/**
 * Function executor for the Shatter TypeScript frontend.
 *
 * Compiles TypeScript source to JavaScript, executes the target function
 * with provided inputs in a sandboxed context, and returns the result
 * with performance metrics and (for instrumented code) branch decisions.
 */

import * as ts from "typescript";
import * as fs from "node:fs";
import * as vm from "node:vm";
import * as path from "node:path";
import { reconstructValue } from "./reconstruct.js";
import type {
  ExecuteResponse,
  ErrorInfo,
  PerformanceMetrics,
  BranchDecision,
  SymConstraint,
  SymExpr,
  SideEffect,
} from "./protocol.js";
import { RECORD_FUNCTION, BRANCH_FUNCTION, MOCK_REGISTRY, MOCK_CALL_FUNCTION } from "./instrumentor.js";
import type { MockConfig, ExternalCall } from "./protocol.js";

/** Cache of compiled modules to avoid re-transpiling on every execute call. */
const compiledModuleCache = new Map<string, Record<string, unknown>>();

/**
 * Proxy console used in VM sandboxes. Delegates all calls to `consoleTarget`,
 * which can be swapped at execution time to capture output as side effects.
 */
let consoleTarget: Console = console;

const consoleProxy = new Proxy(console, {
  get(_target, prop, receiver) {
    const value = Reflect.get(consoleTarget, prop, receiver);
    if (typeof value === "function") {
      return value.bind(consoleTarget);
    }
    return value;
  },
});

/**
 * Transpile a TypeScript file to JavaScript and return the exports object.
 *
 * Results are cached by absolute file path.
 */
function loadModule(filePath: string): Record<string, unknown> {
  const absolutePath = path.resolve(filePath);
  const cached = compiledModuleCache.get(absolutePath);
  if (cached) return cached;

  const source = fs.readFileSync(absolutePath, "utf-8");
  const result = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
      strict: true,
    },
    fileName: absolutePath,
  });

  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require,
    console: consoleProxy,
    process,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    __filename: absolutePath,
    __dirname: path.dirname(absolutePath),
  });

  vm.runInContext(result.outputText, sandbox, { filename: absolutePath });

  // After CommonJS execution, module.exports may have been reassigned
  const finalExports = (sandbox as Record<string, unknown>)["module"] as { exports: Record<string, unknown> };
  const resolvedExports = finalExports.exports;

  compiledModuleCache.set(absolutePath, resolvedExports);
  return resolvedExports;
}

/**
 * Look up a function from the file path and function name.
 *
 * The function name may be a simple name like "classifyNumber" which is
 * looked up directly on the module exports. If it contains a colon
 * (file:function format from the analyze phase), the file portion is
 * stripped and only the function name is used.
 */
function resolveFunction(
  filePath: string,
  functionRef: string,
): (...args: unknown[]) => unknown {
  // Strip file prefix if present (e.g. "examples/foo.ts:myFunc" → "myFunc")
  const funcName = functionRef.includes(":")
    ? functionRef.split(":").pop()!
    : functionRef;

  const moduleExports = loadModule(filePath);
  const fn = moduleExports[funcName];

  if (typeof fn !== "function") {
    throw new Error(
      `Function "${funcName}" not found in exports of ${filePath}. ` +
      `Available exports: ${Object.keys(moduleExports).join(", ")}`,
    );
  }

  return fn as (...args: unknown[]) => unknown;
}

/**
 * Run garbage collection if --expose-gc is enabled.
 * This gives more accurate heap measurements by clearing unreachable objects.
 */
function tryGc(): void {
  if (typeof globalThis.gc === "function") {
    globalThis.gc();
  }
}

/** Result of measuring a function execution. */
interface MeasuredExecution {
  returnValue: unknown;
  thrownError: ErrorInfo | null;
  performance: PerformanceMetrics;
}

/**
 * Execute a callback with full performance instrumentation.
 *
 * Measures wall time via process.hrtime.bigint(), CPU time via process.cpuUsage(),
 * and heap delta via process.memoryUsage(). Optionally runs GC before measurement
 * if --expose-gc is enabled.
 */
function measureExecution(fn: () => unknown): MeasuredExecution {
  tryGc();

  const startMem = process.memoryUsage();
  const startCpu = process.cpuUsage();
  const startTime = process.hrtime.bigint();

  let returnValue: unknown = null;
  let thrownError: ErrorInfo | null = null;

  try {
    returnValue = fn();
  } catch (e: unknown) {
    const err = e as { constructor?: { name?: string }; message?: string; stack?: string };
    thrownError = {
      error_type: err.constructor?.name ?? "Error",
      message: String(err.message ?? e),
      stack: err.stack ?? null,
    };
  }

  const endTime = process.hrtime.bigint();
  const endCpu = process.cpuUsage(startCpu);
  const endMem = process.memoryUsage();

  const wallTimeNs = endTime - startTime;
  const wallTimeMs = Number(wallTimeNs) / 1_000_000;
  const cpuTimeUs = endCpu.user + endCpu.system;

  return {
    returnValue,
    thrownError,
    performance: {
      wall_time_ms: wallTimeMs,
      cpu_time_us: cpuTimeUs,
      heap_used_bytes: Math.max(0, endMem.heapUsed - startMem.heapUsed),
      heap_allocated_bytes: Math.max(0, endMem.heapTotal - startMem.heapTotal),
    },
  };
}

/** Execution result before being wrapped in a protocol response. */
interface RawExecuteResult {
  return_value: unknown;
  thrown_error: ErrorInfo | null;
  performance: PerformanceMetrics;
  branch_path: BranchDecision[];
  path_constraints: SymConstraint[];
  lines_executed: number[];
  side_effects: SideEffect[];
  calls_to_external: ExternalCall[];
}

/**
 * Create an intercepting console that records all output as side effects.
 */
function createCapturingConsole(sideEffects: SideEffect[]): Console {
  const makeLogger = (level: string) => (...args: unknown[]): void => {
    const message = args.map((a) =>
      typeof a === "string" ? a : JSON.stringify(a) ?? String(a)
    ).join(" ");
    sideEffects.push({ kind: "console_output", level, message });
  };

  return {
    log: makeLogger("log"),
    warn: makeLogger("warn"),
    error: makeLogger("error"),
    info: makeLogger("info"),
    debug: makeLogger("debug"),
    dir: console.dir,
    time: console.time,
    timeEnd: console.timeEnd,
    timeLog: console.timeLog,
    trace: console.trace,
    assert: console.assert,
    clear: console.clear,
    count: console.count,
    countReset: console.countReset,
    group: console.group,
    groupCollapsed: console.groupCollapsed,
    groupEnd: console.groupEnd,
    table: console.table,
    dirxml: console.dirxml,
    profile: console.profile,
    profileEnd: console.profileEnd,
    timeStamp: console.timeStamp,
    Console: console.Console,
  } as unknown as Console;
}

/**
 * Execute a function with the given inputs and capture the result.
 */
export function executeFunction(
  filePath: string,
  functionRef: string,
  inputs: unknown[],
): RawExecuteResult {
  const fn = resolveFunction(filePath, functionRef);

  const sideEffects: SideEffect[] = [];
  const previousTarget = consoleTarget;
  consoleTarget = createCapturingConsole(sideEffects);

  let metrics: MeasuredExecution;
  try {
    const reconstructedInputs = inputs.map(reconstructValue);
    metrics = measureExecution(() => fn(...reconstructedInputs));
  } finally {
    consoleTarget = previousTarget;
  }

  if (metrics.thrownError) {
    sideEffects.push({
      kind: "thrown_error",
      error_type: metrics.thrownError.error_type,
      message: metrics.thrownError.message,
      stack: metrics.thrownError.stack,
    });
  }

  return {
    return_value: metrics.returnValue ?? null,
    thrown_error: metrics.thrownError,
    side_effects: sideEffects,
    branch_path: [],
    path_constraints: [],
    lines_executed: [],
    performance: metrics.performance,
    calls_to_external: [],
  };
}

/**
 * Execute instrumented TypeScript source code with branch-recording callbacks.
 *
 * The instrumented source must contain __shatter_record() and __shatter_branch()
 * calls inserted by the instrumentor. This function defines those callbacks,
 * executes the code, and collects the branch decisions.
 */
export function executeInstrumented(
  instrumentedSource: string,
  functionName: string,
  inputs: unknown[],
  mocks: MockConfig[] = [],
): RawExecuteResult {
  // Transpile instrumented TS to JS
  const jsResult = ts.transpileModule(instrumentedSource, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
      strict: true,
    },
  });

  const linesExecuted: number[] = [];
  const branchDecisions: BranchDecision[] = [];
  const sideEffects: SideEffect[] = [];
  const externalCalls: ExternalCall[] = [];

  // Define the runtime callbacks
  const recordFn = (line: number): void => {
    linesExecuted.push(line);
  };

  const branchFn = (
    branchId: number,
    line: number,
    conditionResult: boolean,
    symExpr: SymExpr,
  ): boolean => {
    const constraint: SymConstraint = symExpr.kind !== "unknown"
      ? { kind: "expr", expr: symExpr }
      : { kind: "unknown", hint: "unsupported expression" };

    branchDecisions.push({
      branch_id: branchId,
      line,
      taken: conditionResult,
      constraint,
    });

    return conditionResult;
  };

  // Build mock registry from MockConfig array
  const mockRegistry: Record<string, (...args: unknown[]) => unknown> = {};
  for (const mock of mocks) {
    if (mock.default_behavior === "passthrough") {
      continue;
    }
    let callIndex = 0;
    const returnValues = mock.return_values;
    mockRegistry[mock.symbol] = (...args: unknown[]): unknown => {
      if (returnValues.length > 0) {
        const idx = mock.default_behavior === "repeat_last"
          ? Math.min(callIndex, returnValues.length - 1)
          : callIndex % returnValues.length;
        callIndex++;
        return returnValues[idx];
      }
      return undefined;
    };
  }

  // Mock call recorder
  const mockCallFn = (
    moduleName: string,
    symbolName: string,
    args: unknown[],
    returnValue: unknown,
  ): void => {
    externalCalls.push({
      symbol: `${moduleName}:${symbolName}`,
      args: Array.isArray(args) ? args : [],
      return_value: returnValue,
    });
  };

  // Build the execution context with capturing console
  const capturingConsole = createCapturingConsole(sideEffects);
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require,
    console: capturingConsole,
    process,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    [RECORD_FUNCTION]: recordFn,
    [BRANCH_FUNCTION]: branchFn,
    [MOCK_REGISTRY]: mockRegistry,
    [MOCK_CALL_FUNCTION]: mockCallFn,
  });

  vm.runInContext(jsResult.outputText, sandbox, { filename: "instrumented.js" });

  // Resolve the function from the module exports
  const finalExports = (sandbox as Record<string, unknown>)["module"] as { exports: Record<string, unknown> };

  // Snapshot module-level variables before execution
  const exportKeys = Object.keys(finalExports.exports).filter(
    (k) => typeof finalExports.exports[k] !== "function",
  );
  const beforeSnapshot = new Map<string, unknown>();
  for (const key of exportKeys) {
    beforeSnapshot.set(key, structuredClone(finalExports.exports[key]));
  }

  const fn = finalExports.exports[functionName];

  if (typeof fn !== "function") {
    throw new Error(
      `Function "${functionName}" not found in instrumented module exports. ` +
      `Available: ${Object.keys(finalExports.exports).join(", ")}`,
    );
  }

  const reconstructedInputs = inputs.map(reconstructValue);
  const metrics = measureExecution(
    () => (fn as (...args: unknown[]) => unknown)(...reconstructedInputs),
  );

  if (metrics.thrownError) {
    sideEffects.push({
      kind: "thrown_error",
      error_type: metrics.thrownError.error_type,
      message: metrics.thrownError.message,
      stack: metrics.thrownError.stack,
    });
  }

  // Detect module-level variable changes after execution
  for (const key of exportKeys) {
    const before = beforeSnapshot.get(key);
    const after = finalExports.exports[key];
    if (JSON.stringify(before) !== JSON.stringify(after)) {
      sideEffects.push({
        kind: "global_state_change",
        variable: key,
        before,
        after,
      });
    }
  }

  // Build path_constraints: the conjunction of constraints along the taken path
  const pathConstraints = branchDecisions.map((bd) => bd.constraint);

  return {
    return_value: metrics.returnValue ?? null,
    thrown_error: metrics.thrownError,
    side_effects: sideEffects,
    branch_path: branchDecisions,
    path_constraints: pathConstraints,
    lines_executed: linesExecuted,
    performance: metrics.performance,
    calls_to_external: externalCalls,
  };
}

/**
 * Build a full ExecuteResponse from a raw execution result.
 */
export function buildExecuteResponse(
  id: number,
  protocolVersion: string,
  rawResult: RawExecuteResult,
): ExecuteResponse {
  return {
    protocol_version: protocolVersion,
    id,
    status: "execute",
    return_value: rawResult.return_value,
    thrown_error: rawResult.thrown_error,
    branch_path: rawResult.branch_path,
    lines_executed: rawResult.lines_executed,
    calls_to_external: rawResult.calls_to_external,
    path_constraints: rawResult.path_constraints,
    side_effects: rawResult.side_effects,
    performance: rawResult.performance,
  };
}

/**
 * Clear the compiled module cache. Useful for testing or when source files change.
 */
export function clearModuleCache(): void {
  compiledModuleCache.clear();
}
