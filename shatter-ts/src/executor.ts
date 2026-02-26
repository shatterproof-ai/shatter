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
import type {
  ExecuteResponse,
  ErrorInfo,
  PerformanceMetrics,
  BranchDecision,
  SymConstraint,
  SymExpr,
} from "./protocol.js";
import { RECORD_FUNCTION, BRANCH_FUNCTION } from "./instrumentor.js";

/** Cache of compiled modules to avoid re-transpiling on every execute call. */
const compiledModuleCache = new Map<string, Record<string, unknown>>();

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
    console,
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

/** Execution result before being wrapped in a protocol response. */
interface RawExecuteResult {
  return_value: unknown;
  thrown_error: ErrorInfo | null;
  performance: PerformanceMetrics;
  branch_path: BranchDecision[];
  path_constraints: SymConstraint[];
  lines_executed: number[];
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

  const startMem = process.memoryUsage();
  const startTime = performance.now();

  let returnValue: unknown = null;
  let thrownError: ErrorInfo | null = null;

  try {
    returnValue = fn(...inputs);
  } catch (e: unknown) {
    const err = e as { constructor?: { name?: string }; message?: string; stack?: string };
    thrownError = {
      error_type: err.constructor?.name ?? "Error",
      message: String(err.message ?? e),
      stack: err.stack ?? null,
    };
  }

  const endTime = performance.now();
  const endMem = process.memoryUsage();

  return {
    return_value: returnValue ?? null,
    thrown_error: thrownError,
    branch_path: [],
    path_constraints: [],
    lines_executed: [],
    performance: {
      wall_time_ms: endTime - startTime,
      cpu_time_us: Math.round((endTime - startTime) * 1000),
      heap_used_bytes: Math.max(0, endMem.heapUsed - startMem.heapUsed),
      heap_allocated_bytes: Math.max(0, endMem.heapTotal - startMem.heapTotal),
    },
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

  // Build the execution context
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require,
    console,
    process,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    [RECORD_FUNCTION]: recordFn,
    [BRANCH_FUNCTION]: branchFn,
  });

  vm.runInContext(jsResult.outputText, sandbox, { filename: "instrumented.js" });

  // Resolve the function from the module exports
  const finalExports = (sandbox as Record<string, unknown>)["module"] as { exports: Record<string, unknown> };
  const fn = finalExports.exports[functionName];

  if (typeof fn !== "function") {
    throw new Error(
      `Function "${functionName}" not found in instrumented module exports. ` +
      `Available: ${Object.keys(finalExports.exports).join(", ")}`,
    );
  }

  const startMem = process.memoryUsage();
  const startTime = performance.now();

  let returnValue: unknown = null;
  let thrownError: ErrorInfo | null = null;

  try {
    returnValue = (fn as (...args: unknown[]) => unknown)(...inputs);
  } catch (e: unknown) {
    const err = e as { constructor?: { name?: string }; message?: string; stack?: string };
    thrownError = {
      error_type: err.constructor?.name ?? "Error",
      message: String(err.message ?? e),
      stack: err.stack ?? null,
    };
  }

  const endTime = performance.now();
  const endMem = process.memoryUsage();

  // Build path_constraints: the conjunction of constraints along the taken path
  const pathConstraints = branchDecisions.map((bd) => bd.constraint);

  return {
    return_value: returnValue ?? null,
    thrown_error: thrownError,
    branch_path: branchDecisions,
    path_constraints: pathConstraints,
    lines_executed: linesExecuted,
    performance: {
      wall_time_ms: endTime - startTime,
      cpu_time_us: Math.round((endTime - startTime) * 1000),
      heap_used_bytes: Math.max(0, endMem.heapUsed - startMem.heapUsed),
      heap_allocated_bytes: Math.max(0, endMem.heapTotal - startMem.heapTotal),
    },
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
    calls_to_external: [],
    path_constraints: rawResult.path_constraints,
    side_effects: [],
    performance: rawResult.performance,
  };
}

/**
 * Clear the compiled module cache. Useful for testing or when source files change.
 */
export function clearModuleCache(): void {
  compiledModuleCache.clear();
}
