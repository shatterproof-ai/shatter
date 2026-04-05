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
import { createRequire } from "node:module";

import { reconstructValue } from "./reconstruct.js";
import { serializeReplacer } from "./serialize.js";
import type {
  ExecuteResponse,
  ErrorInfo,
  ErrorCategory,
  PerformanceMetrics,
  BranchDecision,
  SymConstraint,
  SymExpr,
  SideEffect,
  TruncationInfo,
  TraceEvent,
  ScopeEvent,
  DiscoveredDependency,
  ConnectionFailure,
  ConnectionFailureKind,
  ConditionOutcome,
  RuntimeCryptoBoundary,
} from "./protocol.js";
import { RECORD_FUNCTION, BRANCH_FUNCTION, SCOPE_EVENT_FUNCTION, MOCK_REGISTRY, MOCK_CALL_FUNCTION, MCDC_RECORD_FUNCTION, MCDC_BRANCH_FUNCTION, CRYPTO_BOUNDARY_FUNCTION, KNOWN_CRYPTO_PARAM_ROLES } from "./instrumentor.js";
import type { MockConfig, ExternalCall } from "./protocol.js";
import { REACT_MODULE_NAMES, getReactShim } from "./react-shim.js";
import logger from "./logger.js";
import type { TimingCollector } from "./timing.js";

export const DEFAULT_EXEC_TIMEOUT_MS = 15_000;

/**
 * Return true when MC/DC mode is enabled (SHATTER_MCDC=1).
 * Follows the same pattern as getExecTimeoutMs() for SHATTER_EXEC_TIMEOUT.
 */
export function isMcdcEnabled(): boolean {
  return process.env["SHATTER_MCDC"] === "1";
}

/** Default number of head console lines to keep before truncation. */
export const CAPTURE_HEAD_LINES = 50;
/** Default number of tail console lines to keep after truncation. */
export const CAPTURE_TAIL_LINES = 20;
/** Maximum total bytes for captured console output before truncation. */
export const CAPTURE_MAX_BYTES = 6144;
/** Maximum bytes for a single side-effect message. */
export const MESSAGE_MAX_BYTES = 4096;

/**
 * Read SHATTER_EXEC_TIMEOUT env var (seconds) and return milliseconds.
 * Default: 15s. Ignores non-positive or non-numeric values.
 */
export function getExecTimeoutMs(): number {
  const raw = process.env["SHATTER_EXEC_TIMEOUT"];
  if (raw !== undefined) {
    const secs = parseFloat(raw);
    if (Number.isFinite(secs) && secs > 0) {
      return secs * 1000;
    }
  }
  return DEFAULT_EXEC_TIMEOUT_MS;
}

/**
 * Read SHATTER_HARNESS_CACHE env var.
 * Returns undefined if unset or empty.
 */
export function getHarnessCacheDir(): string | undefined {
  const raw = process.env["SHATTER_HARNESS_CACHE"];
  return raw && raw.length > 0 ? raw : undefined;
}

/**
 * Read SHATTER_HARNESS_SCRATCH env var.
 * Returns undefined if unset or empty.
 */
export function getHarnessScratchDir(): string | undefined {
  const raw = process.env["SHATTER_HARNESS_SCRATCH"];
  return raw && raw.length > 0 ? raw : undefined;
}

/** Module names that spawn subprocesses — require() calls to these are flagged. */
const SUBPROCESS_MODULES = new Set([
  "child_process", "node:child_process",
]);

/** Symbols within child_process that spawn subprocesses. */
const SUBPROCESS_SYMBOLS = new Set([
  "exec", "execSync", "execFile", "execFileSync",
  "spawn", "spawnSync", "fork",
]);

/**
 * Callable function target type for Proxy-based stubs.
 * Must be a function so the Proxy supports apply/construct traps.
 */
type CallableTarget = (() => void) & Record<string, unknown>;

export interface ResolverContext {
  module_id: string;
  importer_file?: string;
  require: NodeRequire;
}

export type ResolverDecision =
  | { kind: "continue" }
  | { kind: "rewrite"; module_id: string }
  | { kind: "resolved"; value: unknown }
  | { kind: "stub"; module_id?: string };

export interface ResolverAdapter {
  id: string;
  resolveModule(context: ResolverContext): ResolverDecision | null | undefined;
}

/**
 * Check whether an error is a MODULE_NOT_FOUND error for the requested module
 * (not a transitive dependency failure). Uses duck-typing instead of
 * `instanceof` because errors that cross a VM context boundary lose their
 * prototype chain (the VM's `Error` is a different constructor).
 */
function isModuleNotFoundError(err: unknown, requestedModule: string): boolean {
  if (typeof err !== "object" || err === null) return false;
  const errObj = err as Record<string, unknown>;
  const code = typeof errObj["code"] === "string" ? errObj["code"] : undefined;
  const message = typeof errObj["message"] === "string" ? errObj["message"] : String(err);
  const hasCode = code === "MODULE_NOT_FOUND";
  const hasMessage = message.startsWith("Cannot find module");
  if (!hasCode && !hasMessage) return false;
  // Ensure the error is for the direct require, not a transitive dep
  return message.includes(requestedModule);
}

/**
 * Create a recursive Proxy that silently absorbs all property access,
 * function calls, and constructor calls. Used as a fallback when a
 * module cannot be resolved at runtime.
 *
 * - Property access returns another recursive Proxy
 * - Function calls return another recursive Proxy (chainable)
 * - Constructor calls (new) return another recursive Proxy
 * - Iteration yields nothing (spread/for-of return empty)
 * - Primitive coercion returns "" or 0 (not "undefined")
 * - `.then` returns undefined to prevent thenable coercion
 * - `.__esModule` returns true for ESM interop
 */
export function createUnresolvableModuleStub(_moduleName: string): Record<string, unknown> {
  const handler: ProxyHandler<CallableTarget> = {
    get(_target: CallableTarget, prop: string | symbol): unknown {
      if (prop === Symbol.toPrimitive) return (hint: string) => hint === "number" ? 0 : "";
      if (prop === Symbol.iterator) return function* () {};
      if (prop === Symbol.hasInstance) return () => true;
      if (prop === "then") return undefined;
      if (prop === "__esModule") return true;
      if (prop === "default") return createProxy();
      return createProxy();
    },
    has(): boolean {
      return true;
    },
    set(): boolean {
      return true;
    },
    deleteProperty(): boolean {
      return true;
    },
    ownKeys(target: CallableTarget): string[] {
      // Must include non-configurable own keys from the target (prototype)
      // to satisfy the Proxy invariant, but mark them non-enumerable so
      // Object.keys() / spread return empty results.
      return Object.getOwnPropertyNames(target);
    },
    getOwnPropertyDescriptor(target: CallableTarget, prop: string | symbol): PropertyDescriptor | undefined {
      // For keys inherited from the function target (name, length, prototype),
      // return the real descriptor so Proxy invariants are satisfied.
      const real = Object.getOwnPropertyDescriptor(target, prop);
      if (real) return real;
      return { configurable: true, enumerable: true, value: createProxy() };
    },
    apply(): Record<string, unknown> {
      return createProxy();
    },
    construct(): Record<string, unknown> {
      return createProxy();
    },
  };

  function createProxy(): Record<string, unknown> {
    const target = Object.assign(function callableTarget() {}, {}) as CallableTarget;
    return new Proxy(target, handler) as unknown as Record<string, unknown>;
  }

  return createProxy();
}

const VALIDATION_ERROR_PATTERNS = /Validation|Invalid|BadRequest|Forbidden|Unauthorized|NotFound/i;
const RUNTIME_ERROR_TYPES = new Set(["TypeError", "ReferenceError", "SyntaxError", "RangeError", "URIError"]);

/**
 * Classify an error into a structured category using language-level signals.
 * - validation: custom error subclasses or names suggesting deliberate input rejection
 * - runtime: JS built-in error types indicating accidental failures
 * - infrastructure: timeouts and system-level failures
 */
export function classifyError(errorType: string, message: string): ErrorCategory {
  if (/timed?\s*out/i.test(message) || errorType === "ERR_SCRIPT_EXECUTION_TIMEOUT") {
    return "infrastructure";
  }
  if (VALIDATION_ERROR_PATTERNS.test(errorType) || VALIDATION_ERROR_PATTERNS.test(message)) {
    return "validation";
  }
  if (RUNTIME_ERROR_TYPES.has(errorType)) {
    return "runtime";
  }
  return "unknown";
}

// ---------------------------------------------------------------------------
// Connection failure detection — patterns mirror mock_value_space.rs constants
// ---------------------------------------------------------------------------

/** Patterns indicating a refused TCP connection. */
export const CONN_REFUSED_PATTERNS = ["ECONNREFUSED", "connection refused", "Connection refused"];

/** Patterns indicating a DNS resolution failure. */
export const DNS_FAILURE_PATTERNS = [
  "ENOTFOUND", "EAI_AGAIN", "dns resolution", "DNS resolution", "getaddrinfo", "no such host",
];

/** Patterns indicating an authentication/authorization failure. */
export const AUTH_ERROR_PATTERNS = [
  "EAUTH", "authentication failed", "unauthorized", "403 Forbidden", "401 Unauthorized",
  "invalid credentials",
];

/** Patterns indicating a timeout. */
export const TIMEOUT_PATTERNS = [
  "ETIMEDOUT", "ESOCKETTIMEDOUT", "ETIME", "timed out", "timeout", "deadline exceeded",
];

/**
 * Classify an error message as a connection failure kind, if it matches
 * any known infrastructure failure pattern. Returns null for application errors.
 */
export function classifyConnectionFailure(message: string): ConnectionFailureKind | null {
  for (const pattern of CONN_REFUSED_PATTERNS) {
    if (message.includes(pattern)) return "connection_refused";
  }
  for (const pattern of DNS_FAILURE_PATTERNS) {
    if (message.includes(pattern)) return "dns_failure";
  }
  for (const pattern of AUTH_ERROR_PATTERNS) {
    if (message.includes(pattern)) return "auth_error";
  }
  for (const pattern of TIMEOUT_PATTERNS) {
    if (message.includes(pattern)) return "timeout";
  }
  return null;
}

// ---------------------------------------------------------------------------
// ESM dynamic-import serialization
//
// TypeScript's CJS transpile preserves `import()` expressions. When the same
// ESM-only package is also `require()`'d (from a transpiled static import),
// Node can reject the sync access with "Cannot require() ES Module …
// because it is not yet fully loaded" because the async `import()` already
// started loading the module.  Replacing `import(` with `__shatter_import(`
// in the transpiled output makes all module loading go through synchronous
// `require()`, eliminating the race.
// ---------------------------------------------------------------------------

/**
 * Replace dynamic `import()` expressions in transpiled JS with a synchronous
 * `require()`-based helper, preventing races between async ESM loading and
 * synchronous `require()` on the same module.
 */
export function transformDynamicImports(jsCode: string): string {
  return jsCode.replace(/\bimport\s*\(/g, "__shatter_import(");
}

/**
 * Build the `__shatter_import` helper injected into VM sandboxes.
 *
 * Returns a function with the same signature as `import()`:
 *   (specifier: string) => Promise<namespace>
 *
 * Under the hood it calls `require()` synchronously and wraps the result
 * in ESM-namespace interop (matching TypeScript's `__importStar` behaviour).
 */
export function createShatterImport(
  requireFn: (id: string) => unknown,
): (spec: string) => Promise<Record<string, unknown>> {
  return (spec: string): Promise<Record<string, unknown>> =>
    Promise.resolve().then(() => {
      const m = requireFn(spec);
      if (m != null && typeof m === "object" && (m as Record<string, unknown>).__esModule) {
        return m as Record<string, unknown>;
      }
      const ns: Record<string, unknown> = { __esModule: true, default: m };
      if (m != null && typeof m === "object") {
        Object.assign(ns, m as Record<string, unknown>);
      }
      return ns;
    });
}

/**
 * Check whether an error is the Node.js ESM "not yet fully loaded" race.
 * Used as a safety-net catch alongside MODULE_NOT_FOUND handling.
 */
function isEsmLoadingError(err: unknown): boolean {
  if (typeof err !== "object" || err === null) return false;
  const msg = (err as Record<string, unknown>)["message"];
  if (typeof msg !== "string") return false;
  return msg.includes("not yet fully loaded") || msg.includes("Cannot require() ES Module");
}

/** Cache of compiled modules to avoid re-transpiling on every execute call. */
const compiledModuleCache = new Map<string, Record<string, unknown>>();

/**
 * Cache of pre-compiled vm.Script objects for instrumented sources.
 * Keyed by instrument key ("resolvedFilePath:functionName").
 * Avoids re-transpiling and re-compiling JS on every instrumented execute call.
 * Invalidated per-entry on re-instrumentation, and cleared on teardown.
 */
const compiledScriptCache = new Map<string, vm.Script>();

/** Clear all cached compiled scripts (called on teardown). */
export function clearCompiledScriptCache(): void {
  compiledScriptCache.clear();
}

/** Remove a single entry from the compiled script cache (called on re-instrumentation). */
export function deleteCompiledScriptEntry(key: string): void {
  compiledScriptCache.delete(key);
}

/**
 * Pre-warm the compiled script cache for the given instrumented source.
 * If the key is already cached, this is a no-op. Called by the prepare handler.
 */
export function warmCompiledScriptCache(instrumentedSource: string, cacheKey: string): void {
  if (compiledScriptCache.has(cacheKey)) return;
  const jsResult = ts.transpileModule(instrumentedSource, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
      strict: true,
      jsx: ts.JsxEmit.ReactJSX,
    },
    fileName: cacheKey,
  });
  const compiled = new vm.Script(transformDynamicImports(jsResult.outputText), { filename: cacheKey });
  compiledScriptCache.set(cacheKey, compiled);
}

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
 * No-op console used when capture is disabled. Silences all console output from
 * user functions to prevent stdout pollution (stdout is the protocol channel).
 * Created once at module load — zero allocation per execute call.
 */
const NOOP_CONSOLE = new Proxy({} as Console, {
  get: () => () => undefined,
});

/**
 * No-op process stub used in VM sandboxes when capture is disabled.
 * Prevents user code from writing to stdout (the protocol channel) via process.stdout.
 */
const NOOP_PROCESS = new Proxy({} as NodeJS.Process, {
  get: () => () => undefined,
});

/**
 * Set the project root for module resolution. When set, NODE_PATH includes
 * the project's node_modules directory so createRequire() resolves packages.
 */
export function setProjectRoot(projectRoot: string | null | undefined): void {
  if (projectRoot) {
    const nodeModules = path.join(projectRoot, "node_modules");
    const existing = process.env["NODE_PATH"] ?? "";
    if (!existing.split(path.delimiter).includes(nodeModules)) {
      process.env["NODE_PATH"] = existing ? `${nodeModules}${path.delimiter}${existing}` : nodeModules;
      // Force Node to re-read NODE_PATH for future require calls
      require("module").Module._initPaths();
    }
  }
}

function getDefaultResolverAdapters(filePath: string | undefined): ResolverAdapter[] {
  if (!filePath || !filePath.endsWith(".tsx")) return [];
  return [{
    id: "ts/react-shim",
    resolveModule({ module_id }) {
      if (REACT_MODULE_NAMES.has(module_id)) {
        return { kind: "resolved", value: getReactShim(module_id) };
      }
      return { kind: "continue" };
    },
  }];
}

function resolveModuleWithAdapters(
  originalRequire: NodeRequire,
  modulePath: string,
  importerFile: string | undefined,
  resolverAdapters: ResolverAdapter[],
): { moduleId: string; value: unknown; stubbed: boolean } {
  let currentModuleId = modulePath;
  for (const adapter of resolverAdapters) {
    const decision = adapter.resolveModule({
      module_id: currentModuleId,
      importer_file: importerFile,
      require: originalRequire,
    });
    if (decision == null || decision.kind === "continue") {
      continue;
    }
    if (decision.kind === "rewrite") {
      currentModuleId = decision.module_id;
      continue;
    }
    if (decision.kind === "resolved") {
      return { moduleId: currentModuleId, value: decision.value, stubbed: false };
    }
    const stubModuleId = decision.module_id ?? currentModuleId;
    return {
      moduleId: stubModuleId,
      value: createUnresolvableModuleStub(stubModuleId),
      stubbed: true,
    };
  }

  return {
    moduleId: currentModuleId,
    value: originalRequire(currentModuleId),
    stubbed: false,
  };
}

export function createAdapterAwareRequire(
  originalRequire: NodeRequire,
  importerFile: string | undefined,
  resolverAdapters: ResolverAdapter[],
  onModuleResolved?: (moduleId: string, stubbed: boolean) => void,
): NodeRequire {
  const wrapped = ((modulePath: string) => {
    try {
      const resolved = resolveModuleWithAdapters(
        originalRequire,
        modulePath,
        importerFile,
        resolverAdapters,
      );
      if (resolved.stubbed) {
        logger.warn("module %s could not be resolved; returning stub", resolved.moduleId);
      }
      onModuleResolved?.(resolved.moduleId, resolved.stubbed);
      return resolved.value;
    } catch (err: unknown) {
      if (isModuleNotFoundError(err, modulePath)) {
        logger.warn("module %s could not be resolved; returning stub", modulePath);
        onModuleResolved?.(modulePath, true);
        return createUnresolvableModuleStub(modulePath);
      }
      if (isEsmLoadingError(err)) {
        logger.warn("ESM loading race for %s; returning stub", modulePath);
        onModuleResolved?.(modulePath, true);
        return createUnresolvableModuleStub(modulePath);
      }
      throw err;
    }
  }) as NodeRequire;

  wrapped.resolve = originalRequire.resolve;
  wrapped.cache = originalRequire.cache;
  wrapped.extensions = originalRequire.extensions;
  wrapped.main = originalRequire.main;

  return wrapped;
}

/**
 * Transpile a TypeScript file to JavaScript and return the exports object.
 *
 * Results are cached by absolute file path.
 */
function loadModule(filePath: string, resolverAdapters?: ResolverAdapter[]): Record<string, unknown> {
  const absolutePath = path.resolve(filePath);
  const activeResolverAdapters = resolverAdapters ?? getDefaultResolverAdapters(absolutePath);
  const useCache = resolverAdapters === undefined;
  const cached = useCache ? compiledModuleCache.get(absolutePath) : undefined;
  if (cached) return cached;

  const source = fs.readFileSync(absolutePath, "utf-8");
  const result = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
      strict: true,
      jsx: ts.JsxEmit.ReactJSX,
    },
    fileName: absolutePath,
  });

  const targetRequire = createAdapterAwareRequire(
    createRequire(absolutePath),
    absolutePath,
    activeResolverAdapters,
  );
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require: targetRequire,
    console: consoleProxy,
    process,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    AbortController,
    AbortSignal,
    __filename: absolutePath,
    __dirname: path.dirname(absolutePath),
    __shatter_import: createShatterImport(targetRequire),
  });

  vm.runInContext(transformDynamicImports(result.outputText), sandbox, { filename: absolutePath, timeout: getExecTimeoutMs() });

  // After CommonJS execution, module.exports may have been reassigned
  const finalExports = (sandbox as Record<string, unknown>)["module"] as { exports: Record<string, unknown> };
  const resolvedExports = finalExports.exports;

  if (useCache) {
    compiledModuleCache.set(absolutePath, resolvedExports);
  }
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
  resolverAdapters?: ResolverAdapter[],
): (...args: unknown[]) => unknown {
  // Strip file prefix if present (e.g. "examples/foo.ts:myFunc" → "myFunc")
  const funcName = functionRef.includes(":")
    ? functionRef.split(":").pop()!
    : functionRef;

  const moduleExports = loadModule(filePath, resolverAdapters);
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
async function measureExecution(fn: () => unknown, timing?: TimingCollector): Promise<MeasuredExecution> {
  tryGc();

  const startMem = process.memoryUsage();
  const startCpu = process.cpuUsage();
  const startTime = process.hrtime.bigint();

  let returnValue: unknown = null;
  let thrownError: ErrorInfo | null = null;

  try {
    const syncResult = timing
      ? timing.sync("execute.invoke_function", fn)
      : fn();
    // If the function returned a Promise (async function), await it with timeout
    if (syncResult != null && typeof (syncResult as PromiseLike<unknown>).then === 'function') {
      const timeoutMs = getExecTimeoutMs();
      const awaitResult = () => Promise.race([
        syncResult as Promise<unknown>,
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("async execution timed out")), timeoutMs)
        ),
      ]);
      returnValue = timing
        ? await timing.async("execute.await_result", awaitResult)
        : await awaitResult();
    } else {
      returnValue = syncResult;
    }
  } catch (e: unknown) {
    const err = e as { constructor?: { name?: string }; message?: string; stack?: string };
    const errorType = err.constructor?.name ?? "Error";
    const errorMessage = String(err.message ?? e);
    thrownError = {
      error_type: errorType,
      message: errorMessage,
      stack: err.stack ?? null,
      error_category: classifyError(errorType, errorMessage),
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
  scope_events: TraceEvent[];
  discovered_dependencies: DiscoveredDependency[];
  connection_failures: ConnectionFailure[];
  runtime_crypto_boundaries: RuntimeCryptoBoundary[];
}

/**
 * Truncate a message string to fit within maxBytes.
 * If truncated, appends a suffix indicating the message was cut.
 */
export function truncateMessage(msg: string, maxBytes: number = MESSAGE_MAX_BYTES): string {
  const bytes = Buffer.byteLength(msg, "utf-8");
  if (bytes <= maxBytes) return msg;
  const suffix = "…[truncated]";
  const suffixBytes = Buffer.byteLength(suffix, "utf-8");
  const target = maxBytes - suffixBytes;
  let end = Math.min(msg.length, target);
  while (Buffer.byteLength(msg.slice(0, end), "utf-8") > target && end > 0) {
    end--;
  }
  return msg.slice(0, end) + suffix;
}

/**
 * Convert a raw Buffer/Uint8Array/string value to a base64 string.
 * Returns undefined if the value cannot be sensibly converted.
 */
function toBase64(value: unknown): string | undefined {
  if (Buffer.isBuffer(value)) {
    return value.toString("base64");
  }
  if (value instanceof Uint8Array) {
    return Buffer.from(value).toString("base64");
  }
  if (typeof value === "string") {
    return Buffer.from(value, "binary").toString("base64");
  }
  return undefined;
}

/**
 * Build a RuntimeCryptoBoundary from the runtime arguments of a crypto call.
 *
 * Uses `KNOWN_CRYPTO_PARAM_ROLES` from the instrumentor to interpret which
 * argument is the key, IV, algorithm, or ciphertext.
 */
export function buildRuntimeCryptoBoundary(
  boundaryId: string,
  kind: "encrypt" | "decrypt",
  functionName: string,
  args: unknown[],
): RuntimeCryptoBoundary {
  const paramRoles = KNOWN_CRYPTO_PARAM_ROLES[functionName];
  let algorithm: string | undefined;
  let keyValue: string | undefined;
  let ivValue: string | undefined;
  let ciphertextParamIndex: number | undefined;

  if (paramRoles !== undefined) {
    for (const [indexStr, role] of Object.entries(paramRoles)) {
      const index = parseInt(indexStr, 10);
      const value = args[index];
      if (role === "algorithm" && typeof value === "string") {
        algorithm = value;
      } else if (role === "key") {
        keyValue = toBase64(value);
      } else if (role === "iv") {
        ivValue = toBase64(value);
      } else if (role === "data" && kind === "decrypt") {
        ciphertextParamIndex = index;
      }
    }
  }

  return {
    boundary_id: boundaryId,
    kind,
    function_name: functionName,
    ...(algorithm !== undefined && { algorithm }),
    ...(ciphertextParamIndex !== undefined && { ciphertext_param_index: ciphertextParamIndex }),
    ...(keyValue !== undefined && { key_value: keyValue }),
    ...(ivValue !== undefined && { iv_value: ivValue }),
  };
}

/**
 * Truncate console_output side effects if they exceed head+tail line limits.
 * Returns the (possibly truncated) effects array and truncation metadata.
 */
export function truncateSideEffects(
  effects: SideEffect[],
  headLines: number = CAPTURE_HEAD_LINES,
  tailLines: number = CAPTURE_TAIL_LINES,
): { effects: SideEffect[]; truncation?: TruncationInfo } {
  const consoleIndices: number[] = [];
  for (let i = 0; i < effects.length; i++) {
    const effect = effects[i];
    if (effect !== undefined && effect.kind === "console_output") {
      consoleIndices.push(i);
    }
  }

  const consoleCount = consoleIndices.length;
  if (consoleCount <= headLines + tailLines) {
    return { effects };
  }

  let originalBytes = 0;
  for (const idx of consoleIndices) {
    const e = effects[idx];
    if (e !== undefined && e.kind === "console_output") {
      originalBytes += Buffer.byteLength(e.message, "utf-8");
    }
  }

  const keepHead = new Set(consoleIndices.slice(0, headLines));
  const keepTail = new Set(consoleIndices.slice(-tailLines));
  const truncatedCount = consoleCount - headLines - tailLines;

  let keptBytes = 0;
  for (const idx of [...keepHead, ...keepTail]) {
    const e = effects[idx];
    if (e !== undefined && e.kind === "console_output") {
      keptBytes += Buffer.byteLength(e.message, "utf-8");
    }
  }
  const truncatedBytes = originalBytes - keptBytes;

  const result: SideEffect[] = [];
  let markerInserted = false;
  for (let i = 0; i < effects.length; i++) {
    const effect = effects[i];
    if (effect === undefined) continue;
    if (effect.kind !== "console_output") {
      result.push(effect);
    } else if (keepHead.has(i)) {
      result.push(effect);
    } else if (keepTail.has(i)) {
      result.push(effect);
    } else if (!markerInserted) {
      result.push({
        kind: "console_output",
        level: "info",
        message: `[…truncated ${truncatedCount} lines / ${truncatedBytes} bytes…]`,
      });
      markerInserted = true;
    }
  }

  return {
    effects: result,
    truncation: {
      was_truncated: true,
      original_lines: consoleCount,
      original_bytes: originalBytes,
    },
  };
}

/**
 * Create an intercepting console that records all output as side effects.
 *
 * Captures: log, warn, error, info, debug (level mapped 1:1),
 * dir/table/dirxml (→ "log"), trace (→ "debug"), count/countReset (→ "log"),
 * time/timeEnd/timeLog (→ "log"). Non-output methods (group, clear, assert,
 * profile, timeStamp) delegate to the real console.
 */
function createCapturingConsole(sideEffects: SideEffect[]): Console {
  const makeLogger = (level: string) => (...args: unknown[]): void => {
    const message = truncateMessage(args.map((a) =>
      typeof a === "string" ? a : JSON.stringify(a, serializeReplacer) ?? String(a)
    ).join(" "));
    sideEffects.push({ kind: "console_output", level, message });
  };

  const logFn = makeLogger("log");
  const debugFn = makeLogger("debug");

  // Counters for console.count / console.countReset
  const counters = new Map<string, number>();

  // Timers for console.time / console.timeEnd / console.timeLog
  const timers = new Map<string, number>();

  return {
    log: logFn,
    warn: makeLogger("warn"),
    error: makeLogger("error"),
    info: makeLogger("info"),
    debug: debugFn,
    dir: (...args: unknown[]) => logFn(...args),
    table: (...args: unknown[]) => logFn(...args),
    dirxml: (...args: unknown[]) => logFn(...args),
    trace: (...args: unknown[]) => {
      debugFn("Trace:", ...args);
    },
    count: (label = "default") => {
      const n = (counters.get(label) ?? 0) + 1;
      counters.set(label, n);
      logFn(`${label}: ${n}`);
    },
    countReset: (label = "default") => {
      counters.set(label, 0);
    },
    time: (label = "default") => {
      timers.set(label, performance.now());
    },
    timeEnd: (label = "default") => {
      const start = timers.get(label);
      if (start !== undefined) {
        logFn(`${label}: ${(performance.now() - start).toFixed(3)}ms`);
        timers.delete(label);
      }
    },
    timeLog: (label = "default", ...args: unknown[]) => {
      const start = timers.get(label);
      if (start !== undefined) {
        logFn(`${label}: ${(performance.now() - start).toFixed(3)}ms`, ...args);
      }
    },
    assert: console.assert,
    clear: console.clear,
    group: console.group,
    groupCollapsed: console.groupCollapsed,
    groupEnd: console.groupEnd,
    profile: console.profile,
    profileEnd: console.profileEnd,
    timeStamp: console.timeStamp,
    Console: console.Console,
  } as unknown as Console;
}

/**
 * Create a process-like object that intercepts stdout/stderr writes
 * and records them as side effects.
 */
function createCapturingProcess(sideEffects: SideEffect[]): typeof process {
  const makeStreamWriter = (level: string) => {
    const originalStream = level === "stdout" ? process.stdout : process.stderr;
    return new Proxy(originalStream, {
      get(target, prop) {
        if (prop === "write") {
          return (chunk: string | Uint8Array, ...rest: unknown[]): boolean => {
            const text = typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk);
            const trimmed = text.replace(/\n$/, "");
            if (trimmed.length > 0) {
              const message = truncateMessage(trimmed);
              sideEffects.push({ kind: "console_output", level, message });
            }
            return true;
          };
        }
        const val = Reflect.get(target, prop, target);
        return typeof val === "function" ? val.bind(target) : val;
      },
    });
  };

  return new Proxy(process, {
    get(target, prop) {
      if (prop === "stdout") return makeStreamWriter("stdout");
      if (prop === "stderr") return makeStreamWriter("stderr");
      const val = Reflect.get(target, prop, target);
      return typeof val === "function" ? val.bind(target) : val;
    },
  });
}

/**
 * Execute a function with the given inputs and capture the result.
 */
export async function executeFunction(
  filePath: string,
  functionRef: string,
  inputs: unknown[],
  timing?: TimingCollector,
  capture = true,
  resolverAdapters?: ResolverAdapter[],
): Promise<RawExecuteResult> {
  const fn = timing
    ? timing.sync("execute.module_load", () => resolveFunction(filePath, functionRef, resolverAdapters))
    : resolveFunction(filePath, functionRef, resolverAdapters);

  const previousTarget = consoleTarget;
  let metrics: MeasuredExecution;

  if (capture) {
    const sideEffects: SideEffect[] = [];
    consoleTarget = createCapturingConsole(sideEffects);
    try {
      const reconstructedInputs = inputs.map(reconstructValue);
      metrics = await measureExecution(() => fn(...reconstructedInputs), timing);
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
      scope_events: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
    };
  } else {
    // No-capture fast path: skip all capture infrastructure.
    // NOOP_CONSOLE silences user code's console calls to prevent stdout pollution.
    consoleTarget = NOOP_CONSOLE;
    try {
      const reconstructedInputs = inputs.map(reconstructValue);
      metrics = await measureExecution(() => fn(...reconstructedInputs), timing);
    } finally {
      consoleTarget = previousTarget;
    }
    return {
      return_value: metrics.returnValue ?? null,
      thrown_error: metrics.thrownError,
      side_effects: [],
      branch_path: [],
      path_constraints: [],
      lines_executed: [],
      performance: metrics.performance,
      calls_to_external: [],
      scope_events: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
    };
  }
}

/**
 * Execute instrumented TypeScript source code with branch-recording callbacks.
 *
 * The instrumented source must contain __shatter_record() and __shatter_branch()
 * calls inserted by the instrumentor. This function defines those callbacks,
 * executes the code, and collects the branch decisions.
 */
export async function executeInstrumented(
  instrumentedSource: string,
  functionName: string,
  inputs: unknown[],
  mocks: MockConfig[] = [],
  sourceFilePath?: string,
  timing?: TimingCollector,
  capture = true,
  cacheKey?: string,
  resolverAdapters?: ResolverAdapter[],
): Promise<RawExecuteResult> {
  // Transpile instrumented TS to JS, reusing a cached vm.Script when available.
  // The instrumented source for a given function is fixed after instrumentation,
  // so we can amortize both the TypeScript transpile and the JS bytecode compile
  // across all execute calls for the same function.
  const cachedScript = cacheKey ? compiledScriptCache.get(cacheKey) : undefined;
  let compiledScript: vm.Script;
  if (cachedScript) {
    compiledScript = cachedScript;
    // execute.transpile is intentionally absent from timing on cache hits
  } else {
    const transpile = () => ts.transpileModule(instrumentedSource, {
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.CommonJS,
        esModuleInterop: true,
        strict: true,
        jsx: ts.JsxEmit.ReactJSX,
      },
      ...(sourceFilePath ? { fileName: sourceFilePath } : {}),
    });
    const jsResult = timing
      ? timing.sync("execute.transpile", transpile)
      : transpile();
    compiledScript = new vm.Script(transformDynamicImports(jsResult.outputText), { filename: sourceFilePath ?? "instrumented.js" });
    if (cacheKey) {
      compiledScriptCache.set(cacheKey, compiledScript);
    }
  }

  const linesExecuted: number[] = [];
  const branchDecisions: BranchDecision[] = [];
  const sideEffects: SideEffect[] = [];
  const externalCalls: ExternalCall[] = [];
  const connectionFailures: ConnectionFailure[] = [];
  const cryptoBoundaries: RuntimeCryptoBoundary[] = [];
  const scopeEvents: TraceEvent[] = [];
  const discoveredDeps: DiscoveredDependency[] = [];
  const seenDiscoveredModules = new Set<string>();

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

    const decision: BranchDecision = {
      branch_id: branchId,
      line,
      taken: conditionResult,
      constraint,
    };
    branchDecisions.push(decision);
    scopeEvents.push({ type: "branch", decision });

    return conditionResult;
  };

  /**
   * MC/DC condition recording function.
   *
   * Evaluates condition thunks LEFT TO RIGHT, respecting short-circuit semantics.
   * For "and": stops after the first false thunk, marking remaining as masked.
   * For "or": stops after the first true thunk, marking remaining as masked.
   * Masked conditions get value: null, masked: true.
   *
   * Returns the decision outcome and per-condition ConditionOutcome array.
   */
  const mcdcRecordFn = (
    _branchId: number,
    symExprs: SymExpr[],
    operator: "and" | "or",
    thunks: Array<() => boolean>,
  ): { decision: boolean; conditions: ConditionOutcome[] } => {
    const conditions: ConditionOutcome[] = [];
    let decision: boolean;
    let stopAfter = -1;

    if (operator === "and") {
      decision = true;
      for (let i = 0; i < thunks.length; i++) {
        if (stopAfter >= 0) {
          conditions.push({
            condition_index: i,
            value: null,
            masked: true,
            constraint: { kind: "unknown", hint: "masked by short-circuit" },
          });
          continue;
        }
        const val = thunks[i]!();
        const sym = symExprs[i] ?? ({ kind: "unknown" } as SymExpr);
        conditions.push({
          condition_index: i,
          value: val,
          masked: false,
          constraint: sym.kind !== "unknown" ? { kind: "expr", expr: sym } : { kind: "unknown", hint: "unsupported expression" },
        });
        if (!val) {
          stopAfter = i;
          decision = false;
        }
      }
    } else {
      decision = false;
      for (let i = 0; i < thunks.length; i++) {
        if (stopAfter >= 0) {
          conditions.push({
            condition_index: i,
            value: null,
            masked: true,
            constraint: { kind: "unknown", hint: "masked by short-circuit" },
          });
          continue;
        }
        const val = thunks[i]!();
        const sym = symExprs[i] ?? ({ kind: "unknown" } as SymExpr);
        conditions.push({
          condition_index: i,
          value: val,
          masked: false,
          constraint: sym.kind !== "unknown" ? { kind: "expr", expr: sym } : { kind: "unknown", hint: "unsupported expression" },
        });
        if (val) {
          stopAfter = i;
          decision = true;
        }
      }
    }

    return { decision, conditions };
  };

  /**
   * MC/DC branch recording function — like __shatter_branch but also records
   * per-condition outcomes in the BranchDecision.
   */
  const mcdcBranchFn = (
    branchId: number,
    line: number,
    decision: boolean,
    symExpr: SymExpr,
    conditions: ConditionOutcome[],
  ): boolean => {
    const constraint: SymConstraint = symExpr.kind !== "unknown"
      ? { kind: "expr", expr: symExpr }
      : { kind: "unknown", hint: "unsupported expression" };

    const bd: BranchDecision = {
      branch_id: branchId,
      line,
      taken: decision,
      constraint,
      conditions,
    };
    branchDecisions.push(bd);
    scopeEvents.push({ type: "branch", decision: bd });

    return decision;
  };

  const scopeEventFn = (scopeId: number, kind: string): void => {
    const event: ScopeEvent = kind.startsWith("loop")
      ? { kind: kind as "loop_enter" | "loop_exit", loop_id: scopeId }
      : { kind: kind as "call_enter" | "call_exit", call_site_id: scopeId };
    scopeEvents.push({ type: "scope", event });
  };

  /**
   * Runtime callback injected by the instrumentor before calls to known
   * encrypt/decrypt functions. Captures key, IV, and algorithm values so
   * the core engine can apply boundary splitting.
   *
   * Signature: (boundaryId, kind, functionName, ...args)
   * where `args` are the actual runtime arguments to the crypto function.
   */
  const cryptoBoundaryFn = (
    boundaryId: string,
    kind: "encrypt" | "decrypt",
    functionName: string,
    ...args: unknown[]
  ): void => {
    const boundary = buildRuntimeCryptoBoundary(boundaryId, kind, functionName, args);
    cryptoBoundaries.push(boundary);
  };

  // Build mock registry from MockConfig array
  const mockRegistry: Record<string, (...args: unknown[]) => unknown> = {};
  for (const mock of mocks) {
    if (mock.default_behavior === "passthrough") {
      continue;
    }
    if (mock.default_behavior === "throw_error") {
      let callIndex = 0;
      const returnValues = mock.return_values;
      mockRegistry[mock.symbol] = (): never => {
        // Use return_values as error details if available
        if (returnValues.length > 0) {
          const idx = Math.min(callIndex, returnValues.length - 1);
          callIndex++;
          const errData = returnValues[idx];
          const message = typeof errData === "object" && errData !== null && "message" in errData
            ? String((errData as Record<string, unknown>)["message"])
            : `Mock error: ${mock.symbol}`;
          throw new Error(message);
        }
        throw new Error(`Mock error: ${mock.symbol}`);
      };
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

  // Mock call recorder — also classifies connection failures from thrown errors
  const mockCallFn = (
    moduleName: string,
    symbolName: string,
    args: unknown[],
    returnValue: unknown,
    thrownError?: unknown,
  ): void => {
    const symbol = `${moduleName}:${symbolName}`;
    externalCalls.push({
      symbol,
      args: Array.isArray(args) ? args : [],
      return_value: returnValue,
    });

    if (thrownError !== undefined) {
      const errMsg = thrownError instanceof Error
        ? thrownError.message
        : String(thrownError);
      const kind = classifyConnectionFailure(errMsg);
      if (kind !== null) {
        connectionFailures.push({ symbol, error_kind: kind, message: errMsg });
      }
    }
  };

  // Build the execution context: use capturing console/process when capture is enabled,
  // otherwise use no-op stubs to prevent stdout pollution without the capture overhead.
  const sandboxConsole = capture ? createCapturingConsole(sideEffects) : NOOP_CONSOLE;
  const sandboxProc = capture ? createCapturingProcess(sideEffects) : NOOP_PROCESS;
  const rawRequire = sourceFilePath ? createRequire(path.resolve(sourceFilePath)) : require;
  const activeResolverAdapters = resolverAdapters ?? getDefaultResolverAdapters(sourceFilePath);

  // Collect mocked module prefixes for gap detection
  const mockedModulePrefixes = new Set<string>();
  for (const key of Object.keys(mockRegistry)) {
    const colonIdx = key.indexOf(":");
    if (colonIdx > 0) {
      mockedModulePrefixes.add(key.substring(0, colonIdx));
    }
  }

  // Wrap require to detect unmocked external imports, subprocess APIs,
  // and gracefully stub unresolvable modules instead of crashing.
  const sandboxRequire = createAdapterAwareRequire(
    rawRequire,
    sourceFilePath,
    activeResolverAdapters,
    (moduleId, stubbed) => {
      if (moduleId.startsWith(".") || moduleId.startsWith("/") || seenDiscoveredModules.has(moduleId)) {
        return;
      }
      seenDiscoveredModules.add(moduleId);

      if (stubbed) {
        discoveredDeps.push({
          symbol: moduleId,
          source_module: moduleId,
          kind: "stubbed_import",
          is_subprocess_spawn: false,
        });
        return;
      }

      const isSubprocessModule = SUBPROCESS_MODULES.has(moduleId);
      const isMocked = mockedModulePrefixes.has(moduleId);

      if (isSubprocessModule) {
        discoveredDeps.push({
          symbol: moduleId,
          source_module: moduleId,
          kind: "subprocess_spawn",
          is_subprocess_spawn: true,
        });
      } else if (!isMocked) {
        discoveredDeps.push({
          symbol: moduleId,
          source_module: moduleId,
          kind: "unmocked_import",
          is_subprocess_spawn: false,
        });
      }
    },
  );
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require: sandboxRequire,
    console: sandboxConsole,
    process: sandboxProc,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    AbortController,
    AbortSignal,
    ...(sourceFilePath ? { __filename: sourceFilePath, __dirname: path.dirname(sourceFilePath) } : {}),
    [RECORD_FUNCTION]: recordFn,
    [BRANCH_FUNCTION]: branchFn,
    [MCDC_RECORD_FUNCTION]: mcdcRecordFn,
    [MCDC_BRANCH_FUNCTION]: mcdcBranchFn,
    [SCOPE_EVENT_FUNCTION]: scopeEventFn,
    [MOCK_REGISTRY]: mockRegistry,
    [MOCK_CALL_FUNCTION]: mockCallFn,
    [CRYPTO_BOUNDARY_FUNCTION]: cryptoBoundaryFn,
    __shatter_import: createShatterImport(sandboxRequire),
  });

  const loadModule = (): void => {
    compiledScript.runInContext(sandbox, { timeout: getExecTimeoutMs() });
  };
  if (timing) {
    timing.sync("execute.module_load", loadModule);
  } else {
    loadModule();
  }

  // Resolve the function from the module exports
  const finalExports = (sandbox as Record<string, unknown>)["module"] as { exports: Record<string, unknown> };

  // Snapshot module-level variables before execution (JSON strings, not deep clones)
  const exportKeys = Object.keys(finalExports.exports).filter(
    (k) => typeof finalExports.exports[k] !== "function",
  );
  const beforeSnapshot = new Map<string, string | undefined>();
  for (const key of exportKeys) {
    try {
      beforeSnapshot.set(key, JSON.stringify(finalExports.exports[key], serializeReplacer));
    } catch {
      // Non-serializable (circular refs, etc.) — skip comparison for this export
      beforeSnapshot.set(key, undefined);
    }
  }

  const fn = finalExports.exports[functionName];

  if (typeof fn !== "function") {
    throw new Error(
      `Function "${functionName}" not found in instrumented module exports. ` +
      `Available: ${Object.keys(finalExports.exports).join(", ")}`,
    );
  }

  const reconstructedInputs = inputs.map(reconstructValue);
  const metrics = await measureExecution(
    () => (fn as (...args: unknown[]) => unknown)(...reconstructedInputs),
    timing,
  );

  if (metrics.thrownError) {
    if (capture) {
      sideEffects.push({
        kind: "thrown_error",
        error_type: metrics.thrownError.error_type,
        message: metrics.thrownError.message,
        stack: metrics.thrownError.stack,
      });
    }

    // Classify the thrown error as a connection failure if it matches infra patterns
    const connKind = classifyConnectionFailure(metrics.thrownError.message);
    if (connKind !== null) {
      connectionFailures.push({
        symbol: "unknown",
        error_kind: connKind,
        message: metrics.thrownError.message,
      });
    }
  }

  // Detect module-level variable changes after execution
  if (capture) {
    for (const key of exportKeys) {
      const beforeJson = beforeSnapshot.get(key);
      if (beforeJson === undefined) continue; // non-serializable — skip
      let afterJson: string | undefined;
      try {
        afterJson = JSON.stringify(finalExports.exports[key], serializeReplacer);
      } catch {
        continue; // became non-serializable
      }
      if (beforeJson !== afterJson) {
        sideEffects.push({
          kind: "global_state_change",
          variable: key,
          before: JSON.parse(beforeJson),
          after: finalExports.exports[key],
        });
      }
    }
  }

  // Build path_constraints: the conjunction of constraints along the taken path
  const pathConstraints = branchDecisions.map((bd) => bd.constraint);

  return {
    return_value: metrics.returnValue ?? null,
    thrown_error: metrics.thrownError,
    side_effects: capture ? sideEffects : [],
    branch_path: branchDecisions,
    path_constraints: pathConstraints,
    lines_executed: linesExecuted,
    performance: metrics.performance,
    calls_to_external: externalCalls,
    scope_events: scopeEvents,
    discovered_dependencies: discoveredDeps,
    connection_failures: connectionFailures,
    runtime_crypto_boundaries: cryptoBoundaries,
  };
}

/**
 * Build a full ExecuteResponse from a raw execution result.
 * Applies side-effect truncation before assembling the response.
 */
export function buildExecuteResponse(
  id: number,
  protocolVersion: string,
  rawResult: RawExecuteResult,
  timing?: TimingCollector,
): ExecuteResponse {
  // Skip truncation when there are no side effects (e.g. capture-disabled runs).
  const { effects, truncation } = rawResult.side_effects.length === 0
    ? { effects: [] as SideEffect[], truncation: undefined }
    : timing
      ? timing.sync("execute.trace_capture", () => truncateSideEffects(rawResult.side_effects))
      : truncateSideEffects(rawResult.side_effects);

  const response: ExecuteResponse = {
    protocol_version: protocolVersion,
    id,
    status: "execute",
    return_value: rawResult.return_value,
    thrown_error: rawResult.thrown_error,
    branch_path: rawResult.branch_path,
    lines_executed: rawResult.lines_executed,
    calls_to_external: rawResult.calls_to_external,
    path_constraints: rawResult.path_constraints,
    side_effects: effects,
    performance: rawResult.performance,
    scope_events: rawResult.scope_events,
  };

  if (truncation) {
    response.capture_truncation = truncation;
  }

  if (rawResult.discovered_dependencies.length > 0) {
    response.discovered_dependencies = rawResult.discovered_dependencies;
  }

  if (rawResult.connection_failures.length > 0) {
    response.connection_failures = rawResult.connection_failures;
  }

  if (rawResult.runtime_crypto_boundaries.length > 0) {
    response.runtime_crypto_boundaries = rawResult.runtime_crypto_boundaries;
  }

  return response;
}

/**
 * Clear the compiled module cache. Useful for testing or when source files change.
 */
export function clearModuleCache(): void {
  compiledModuleCache.clear();
}

/** Number of cached compiled modules. Exposed for testing. */
export function compiledModuleCacheSize(): number {
  return compiledModuleCache.size;
}
