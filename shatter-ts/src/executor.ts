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
  AdapterHint,
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
  LoopBodyState,
  LoopInfo,
  InvocationOutcome,
} from "./protocol.js";
import { detectRuntimeHints } from "./runtime-hints.js";
import { WEB_GLOBALS, DEFAULT_IMPORT_META_ENV } from "./web-globals.js";
import type {
  SandboxProvider,
  InvocationHook,
  InvocationContext,
  AdapterInvocationModel,
} from "./runtime-hooks.js";
import {
  RECORD_FUNCTION,
  BRANCH_FUNCTION,
  SCOPE_EVENT_FUNCTION,
  MOCK_REGISTRY,
  MOCK_CALL_FUNCTION,
  MCDC_RECORD_FUNCTION,
  MCDC_BRANCH_FUNCTION,
  CRYPTO_BOUNDARY_FUNCTION,
  KNOWN_CRYPTO_PARAM_ROLES,
  buildSymExprWithFlow,
} from "./instrumentor.js";
import type { MockConfig, ExternalCall } from "./protocol.js";
import { REACT_MODULE_NAMES, getReactShim } from "./react-shim.js";
import {
  DEFAULT_JSX_RUNTIME_OPTIONS,
  loadJsxRuntimeOptions,
  type JsxRuntimeOptions,
} from "./analyzer.js";
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
const SUBPROCESS_MODULES = new Set(["child_process", "node:child_process"]);

/** Symbols within child_process that spawn subprocesses. */
const SUBPROCESS_SYMBOLS = new Set([
  "exec",
  "execSync",
  "execFile",
  "execFileSync",
  "spawn",
  "spawnSync",
  "fork",
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
  const message =
    typeof errObj["message"] === "string" ? errObj["message"] : String(err);
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
export function createUnresolvableModuleStub(
  _moduleName: string,
): Record<string, unknown> {
  const handler: ProxyHandler<CallableTarget> = {
    get(_target: CallableTarget, prop: string | symbol): unknown {
      if (prop === Symbol.toPrimitive)
        return (hint: string) => (hint === "number" ? 0 : "");
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
    getOwnPropertyDescriptor(
      target: CallableTarget,
      prop: string | symbol,
    ): PropertyDescriptor | undefined {
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
    const target = Object.assign(
      function callableTarget() {},
      {},
    ) as CallableTarget;
    return new Proxy(target, handler) as unknown as Record<string, unknown>;
  }

  return createProxy();
}

const VALIDATION_ERROR_PATTERNS =
  /Validation|Invalid|BadRequest|Forbidden|Unauthorized|NotFound/i;
const RUNTIME_ERROR_TYPES = new Set([
  "TypeError",
  "ReferenceError",
  "SyntaxError",
  "RangeError",
  "URIError",
]);

/**
 * Classify an error into a structured category using language-level signals.
 * - validation: custom error subclasses or names suggesting deliberate input rejection
 * - runtime: JS built-in error types indicating accidental failures
 * - infrastructure: timeouts and system-level failures
 */
export function classifyError(
  errorType: string,
  message: string,
): ErrorCategory {
  if (
    /timed?\s*out/i.test(message) ||
    errorType === "ERR_SCRIPT_EXECUTION_TIMEOUT"
  ) {
    return "infrastructure";
  }
  if (
    VALIDATION_ERROR_PATTERNS.test(errorType) ||
    VALIDATION_ERROR_PATTERNS.test(message)
  ) {
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
export const CONN_REFUSED_PATTERNS = [
  "ECONNREFUSED",
  "connection refused",
  "Connection refused",
];

/** Patterns indicating a DNS resolution failure. */
export const DNS_FAILURE_PATTERNS = [
  "ENOTFOUND",
  "EAI_AGAIN",
  "dns resolution",
  "DNS resolution",
  "getaddrinfo",
  "no such host",
];

/** Patterns indicating an authentication/authorization failure. */
export const AUTH_ERROR_PATTERNS = [
  "EAUTH",
  "authentication failed",
  "unauthorized",
  "403 Forbidden",
  "401 Unauthorized",
  "invalid credentials",
];

/** Patterns indicating a timeout. */
export const TIMEOUT_PATTERNS = [
  "ETIMEDOUT",
  "ESOCKETTIMEDOUT",
  "ETIME",
  "timed out",
  "timeout",
  "deadline exceeded",
];

/**
 * Classify an error message as a connection failure kind, if it matches
 * any known infrastructure failure pattern. Returns null for application errors.
 */
export function classifyConnectionFailure(
  message: string,
): ConnectionFailureKind | null {
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
 *
 * Also replaces `import.meta` references with `__shatter_import_meta` so
 * Vite-style code (e.g. `import.meta.env.VITE_API_URL`) can execute in the
 * VM sandbox without "Cannot use 'import.meta' outside a module" errors.
 */
export function transformDynamicImports(jsCode: string): string {
  return jsCode
    .replace(/\bimport\.meta\b/g, "__shatter_import_meta")
    .replace(/\bimport\s*\(/g, "__shatter_import(");
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
      if (
        m != null &&
        typeof m === "object" &&
        (m as Record<string, unknown>).__esModule
      ) {
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
  return (
    msg.includes("not yet fully loaded") ||
    msg.includes("Cannot require() ES Module")
  );
}

/**
 * Error thrown when TypeScript-to-JavaScript transformation fails for an
 * instrumented or directly-loaded module. Carries a `category` so handlers
 * can map it to a `compilation_error` response with a precise root-cause
 * label, instead of letting it bubble up as opaque `internal_error` /
 * `runtime_failed` (str-jeen.11).
 *
 * Categories:
 * - `transpile_failed` — `ts.transpileModule` produced fatal diagnostics
 *   (unrecoverable TS parse errors).
 * - `compile_failed` — `ts.transpileModule` succeeded but `new vm.Script(...)`
 *   threw a SyntaxError parsing the emitted JS. This typically means TS type
 *   syntax (interface refs, type-only identifiers in value position, generic
 *   parameters, JSX in a misclassified file) survived transpile and reached
 *   V8.
 */
export class TranspileError extends Error {
  readonly category: "transpile_failed" | "compile_failed";
  readonly fileName: string | undefined;
  readonly diagnostics: string[];
  constructor(args: {
    category: "transpile_failed" | "compile_failed";
    fileName?: string;
    diagnostics?: string[];
    cause?: unknown;
    message: string;
  }) {
    super(args.message);
    this.name = "TranspileError";
    this.category = args.category;
    this.fileName = args.fileName;
    this.diagnostics = args.diagnostics ?? [];
    if (args.cause !== undefined) {
      (this as { cause?: unknown }).cause = args.cause;
    }
  }
}

/**
 * Format ts.Diagnostic[] into human-readable strings, defensively flattening
 * the messageText chain.
 */
function formatTsDiagnostics(diagnostics: readonly ts.Diagnostic[]): string[] {
  return diagnostics.map((d) => {
    const msg = ts.flattenDiagnosticMessageText(d.messageText, "\n");
    if (d.file && typeof d.start === "number") {
      const { line, character } = d.file.getLineAndCharacterOfPosition(d.start);
      return `${d.file.fileName}(${line + 1},${character + 1}): ${msg}`;
    }
    return msg;
  });
}

/**
 * Run `ts.transpileModule` and surface fatal diagnostics as a `TranspileError`.
 * Then wrap the emitted JS in `new vm.Script(...)`; if V8 rejects the JS
 * (typically because TS type syntax survived), surface that as a
 * `compile_failed` `TranspileError` carrying the underlying message.
 */
function transpileAndCompile(
  instrumentedSource: string,
  fileName: string | undefined,
  vmFilename: string,
): vm.Script {
  let jsResult: ts.TranspileOutput;
  try {
    jsResult = ts.transpileModule(instrumentedSource, {
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.CommonJS,
        esModuleInterop: true,
        strict: true,
        jsx: currentJsxRuntimeOptions.jsx,
        ...(currentJsxRuntimeOptions.jsxImportSource
          ? { jsxImportSource: currentJsxRuntimeOptions.jsxImportSource }
          : {}),
      },
      ...(fileName ? { fileName } : {}),
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new TranspileError({
      category: "transpile_failed",
      fileName,
      message: `TypeScript transpile threw for ${fileName ?? "<inline>"}: ${message}`,
      cause: err,
    });
  }

  // ts.transpileModule reports fatal parse errors via `diagnostics` rather
  // than throwing. Surface them so the caller can classify cleanly.
  const diagnostics = jsResult.diagnostics ?? [];
  const fatalDiagnostics = diagnostics.filter(
    (d) => d.category === ts.DiagnosticCategory.Error,
  );
  if (fatalDiagnostics.length > 0) {
    const formatted = formatTsDiagnostics(fatalDiagnostics);
    throw new TranspileError({
      category: "transpile_failed",
      fileName,
      diagnostics: formatted,
      message: `TypeScript transpile failed for ${fileName ?? "<inline>"}: ${formatted[0]}`,
    });
  }

  try {
    return new vm.Script(transformDynamicImports(jsResult.outputText), {
      filename: vmFilename,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new TranspileError({
      category: "compile_failed",
      fileName,
      message: `JavaScript compile failed for ${fileName ?? vmFilename} after TS transpile: ${message}. ` +
        `This usually means TypeScript type syntax survived transpile (for example, JSX in a file the transpiler treated as plain TS, or an unsupported TS construct).`,
      cause: err,
    });
  }
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
 *
 * `sourceFileName` must be the actual source file path (e.g. "/path/to/foo.tsx")
 * so that ts.transpileModule can determine the correct ScriptKind (TSX vs TS).
 * Do NOT pass the cache key here — it has a ":functionName" suffix that makes
 * TypeScript fall back to ScriptKind.TS, silently skipping JSX stripping.
 */
export function warmCompiledScriptCache(
  instrumentedSource: string,
  cacheKey: string,
  sourceFileName: string,
): void {
  if (compiledScriptCache.has(cacheKey)) return;
  const compiled = transpileAndCompile(instrumentedSource, sourceFileName, cacheKey);
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
      process.env["NODE_PATH"] = existing
        ? `${nodeModules}${path.delimiter}${existing}`
        : nodeModules;
      // Force Node to re-read NODE_PATH for future require calls
      require("module").Module._initPaths();
    }
  }
  // Resolve and cache the project's JSX runtime configuration so subsequent
  // transpiles honor `jsx` / `jsxImportSource`. Falls back to the default
  // automatic-React runtime when no project root or no tsconfig is present.
  currentJsxRuntimeOptions = loadJsxRuntimeOptions(projectRoot);
}

/**
 * JSX runtime options applied to TypeScript transpiles. Updated whenever
 * `setProjectRoot` is called; defaults to the bundled React shim's
 * automatic runtime.
 */
let currentJsxRuntimeOptions: JsxRuntimeOptions = DEFAULT_JSX_RUNTIME_OPTIONS;

/** Test-only accessor. */
export function getCurrentJsxRuntimeOptions(): JsxRuntimeOptions {
  return currentJsxRuntimeOptions;
}

function getDefaultResolverAdapters(
  filePath: string | undefined,
): ResolverAdapter[] {
  if (!filePath || !/\.[cm]?tsx?$/.test(filePath)) return [];
  // When the project configures a non-default `jsxImportSource` (e.g.
  // "preact"), the automatic JSX transform emits
  // `require("<source>/jsx-runtime")` and `require("<source>/jsx-dev-runtime")`.
  // The bundled React shim returns plain element-like objects that the
  // concolic engine can introspect regardless of the declared source, so we
  // route those module ids to the shim too. This lets a project's
  // `jsxImportSource` flow through transpile while keeping execution
  // hermetic — no real React or Preact runtime needs to be installed in
  // the project under test.
  const importSource = currentJsxRuntimeOptions.jsxImportSource;
  return [
    {
      id: "ts/react-shim",
      resolveModule({ module_id }) {
        if (REACT_MODULE_NAMES.has(module_id)) {
          return { kind: "resolved", value: getReactShim(module_id) };
        }
        if (importSource && importSource !== "react") {
          if (module_id === `${importSource}/jsx-runtime`) {
            return { kind: "resolved", value: getReactShim("react/jsx-runtime") };
          }
          if (module_id === `${importSource}/jsx-dev-runtime`) {
            return { kind: "resolved", value: getReactShim("react/jsx-dev-runtime") };
          }
          if (module_id === importSource) {
            return { kind: "resolved", value: getReactShim("react") };
          }
        }
        return { kind: "continue" };
      },
    },
  ];
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
      return {
        moduleId: currentModuleId,
        value: decision.value,
        stubbed: false,
      };
    }
    const stubModuleId = decision.module_id ?? currentModuleId;
    return {
      moduleId: stubModuleId,
      value: createUnresolvableModuleStub(stubModuleId),
      stubbed: true,
    };
  }

  // If adapters rewrote the module ID to an absolute .ts/.tsx path (e.g. from
  // tsconfig-paths), Node's native require cannot load it. Use loadModule() so
  // the file is transpiled and run in a sandbox like any other TS source.
  if (path.isAbsolute(currentModuleId) && /\.[cm]?tsx?$/.test(currentModuleId)) {
    return {
      moduleId: currentModuleId,
      value: loadModule(currentModuleId, resolverAdapters),
      stubbed: false,
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
        logger.warn(
          "module %s could not be resolved; returning stub",
          resolved.moduleId,
        );
      }
      onModuleResolved?.(resolved.moduleId, resolved.stubbed);
      return resolved.value;
    } catch (err: unknown) {
      if (isModuleNotFoundError(err, modulePath)) {
        // For relative imports, try adding .ts/.tsx extensions before stubbing.
        // This handles extensionless imports (./client → ./client.ts) and
        // ordinary relative imports in TS projects where Node can't load .ts.
        if (modulePath.startsWith(".") && importerFile) {
          const importerDir = path.dirname(importerFile);
          const base = path.resolve(importerDir, modulePath);
          if (!/\.[cm]?tsx?$/.test(modulePath)) {
            for (const ext of [".ts", ".tsx", "/index.ts", "/index.tsx"]) {
              const candidate = base + ext;
              if (fs.existsSync(candidate)) {
                onModuleResolved?.(candidate, false);
                return loadModule(candidate, resolverAdapters);
              }
            }
          }
        }
        logger.warn(
          "module %s could not be resolved; returning stub",
          modulePath,
        );
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
function loadModule(
  filePath: string,
  resolverAdapters?: ResolverAdapter[],
  sandboxProviders?: SandboxProvider[],
): Record<string, unknown> {
  const absolutePath = path.resolve(filePath);
  const activeResolverAdapters =
    resolverAdapters ?? getDefaultResolverAdapters(absolutePath);
  const useCache =
    resolverAdapters === undefined && sandboxProviders === undefined;
  const cached = useCache ? compiledModuleCache.get(absolutePath) : undefined;
  if (cached) return cached;

  const source = fs.readFileSync(absolutePath, "utf-8");
  // We still call ts.transpileModule directly here (rather than reusing
  // transpileAndCompile) because we need the JS *text* — the script is
  // executed via vm.runInContext below, not via vm.Script. Wrap the call so
  // fatal TS diagnostics surface as a typed TranspileError.
  let result: ts.TranspileOutput;
  try {
    result = ts.transpileModule(source, {
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.CommonJS,
        esModuleInterop: true,
        strict: true,
        jsx: currentJsxRuntimeOptions.jsx,
        ...(currentJsxRuntimeOptions.jsxImportSource
          ? { jsxImportSource: currentJsxRuntimeOptions.jsxImportSource }
          : {}),
      },
      fileName: absolutePath,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new TranspileError({
      category: "transpile_failed",
      fileName: absolutePath,
      message: `TypeScript transpile threw for ${absolutePath}: ${message}`,
      cause: err,
    });
  }
  const fatalDiagnostics = (result.diagnostics ?? []).filter(
    (d) => d.category === ts.DiagnosticCategory.Error,
  );
  if (fatalDiagnostics.length > 0) {
    const formatted = formatTsDiagnostics(fatalDiagnostics);
    throw new TranspileError({
      category: "transpile_failed",
      fileName: absolutePath,
      diagnostics: formatted,
      message: `TypeScript transpile failed for ${absolutePath}: ${formatted[0]}`,
    });
  }

  const targetRequire = createAdapterAwareRequire(
    createRequire(absolutePath),
    absolutePath,
    activeResolverAdapters,
  );
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    ...WEB_GLOBALS,
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
    __shatter_import_meta: { url: "", env: { ...DEFAULT_IMPORT_META_ENV } },
  });

  if (sandboxProviders) {
    for (const provider of sandboxProviders) {
      provider.augmentSandbox(sandbox as Record<string, unknown>);
    }
  }

  try {
    vm.runInContext(transformDynamicImports(result.outputText), sandbox, {
      filename: absolutePath,
      timeout: getExecTimeoutMs(),
    });
  } catch (err) {
    if (err instanceof SyntaxError) {
      throw new TranspileError({
        category: "compile_failed",
        fileName: absolutePath,
        message: `JavaScript compile failed for ${absolutePath} after TS transpile: ${err.message}. ` +
          `This usually means TypeScript type syntax survived transpile (for example, JSX in a file the transpiler treated as plain TS, or an unsupported TS construct).`,
        cause: err,
      });
    }
    throw err;
  }

  // After CommonJS execution, module.exports may have been reassigned
  const finalExports = (sandbox as Record<string, unknown>)["module"] as {
    exports: Record<string, unknown>;
  };
  const resolvedExports = finalExports.exports;

  if (useCache) {
    compiledModuleCache.set(absolutePath, resolvedExports);
  }
  return resolvedExports;
}

/**
 * Load a TypeScript module and return its exports. Public wrapper around
 * the private `loadModule` for use by adapter InvocationHooks that need
 * to load the target module themselves (e.g. the react-hook adapter).
 *
 * For `.ts` / `.tsx` (and `.mts` / `.cts`) files the default resolver
 * adapters automatically inject React shims so hooks execute without a
 * real React runtime.
 */
export function loadModuleExports(
  filePath: string,
  resolverAdapters?: ResolverAdapter[],
  sandboxProviders?: SandboxProvider[],
): Record<string, unknown> {
  return loadModule(filePath, resolverAdapters, sandboxProviders);
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
  sandboxProviders?: SandboxProvider[],
): (...args: unknown[]) => unknown {
  // Strip file prefix if present (e.g. "examples/foo.ts:myFunc" → "myFunc")
  const funcName = functionRef.includes(":")
    ? functionRef.split(":").pop()!
    : functionRef;

  const moduleExports = loadModule(
    filePath,
    resolverAdapters,
    sandboxProviders,
  );
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
async function measureExecution(
  fn: () => unknown,
  timing?: TimingCollector,
): Promise<MeasuredExecution> {
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
    if (
      syncResult != null &&
      typeof (syncResult as PromiseLike<unknown>).then === "function"
    ) {
      const timeoutMs = getExecTimeoutMs();
      const awaitResult = () =>
        Promise.race([
          syncResult as Promise<unknown>,
          new Promise<never>((_, reject) =>
            setTimeout(
              () => reject(new Error("async execution timed out")),
              timeoutMs,
            ),
          ),
        ]);
      returnValue = timing
        ? await timing.async("execute.await_result", awaitResult)
        : await awaitResult();
    } else {
      returnValue = syncResult;
    }
  } catch (e: unknown) {
    const err = e as {
      constructor?: { name?: string };
      message?: string;
      stack?: string;
    };
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
  loop_body_states: LoopBodyState[];
  discovered_dependencies: DiscoveredDependency[];
  connection_failures: ConnectionFailure[];
  runtime_crypto_boundaries: RuntimeCryptoBoundary[];
  adapter_hints: AdapterHint[];
}

function extractLoopBodyStates(
  source: string,
  functionName: string,
  fileName: string,
  loops: LoopInfo[],
  scopeEvents: TraceEvent[],
): LoopBodyState[] {
  if (loops.length === 0) {
    return [];
  }

  const sourceFile = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  const targetFunction = findFunctionNode(sourceFile, functionName);
  const body = extractFunctionBodyNode(targetFunction);
  if (!body) {
    return [];
  }

  const loopIterations = countObservedLoopIterations(scopeEvents);
  if (loopIterations.size === 0) {
    return [];
  }

  const loopInfoByLine = new Map<number, LoopInfo>();
  for (const loop of loops) {
    loopInfoByLine.set(loop.line, loop);
  }

  const paramNames = extractFunctionParamNames(targetFunction);
  const flowMap = new Map<string, SymExpr>();
  const snapshots: LoopBodyState[] = [];
  const resolveName = (name: string): SymExpr | undefined => {
    if (paramNames.has(name)) {
      return { kind: "param", name, path: [] };
    }
    return flowMap.get(name);
  };

  visitStatementsForLoopSnapshots(
    body.statements,
    sourceFile,
    loopInfoByLine,
    loopIterations,
    resolveName,
    flowMap,
    snapshots,
  );
  return snapshots;
}

function findFunctionNode(
  sourceFile: ts.SourceFile,
  functionName: string,
): ts.FunctionDeclaration | ts.VariableStatement | undefined {
  for (const statement of sourceFile.statements) {
    if (
      ts.isFunctionDeclaration(statement) &&
      statement.name?.text === functionName
    ) {
      return statement;
    }
    if (!ts.isVariableStatement(statement)) {
      continue;
    }
    for (const declaration of statement.declarationList.declarations) {
      if (
        ts.isIdentifier(declaration.name) &&
        declaration.name.text === functionName &&
        declaration.initializer &&
        (ts.isArrowFunction(declaration.initializer) ||
          ts.isFunctionExpression(declaration.initializer))
      ) {
        return statement;
      }
    }
  }
  return undefined;
}

function extractFunctionBodyNode(
  node: ts.FunctionDeclaration | ts.VariableStatement | undefined,
): ts.Block | undefined {
  if (!node) {
    return undefined;
  }
  if (ts.isFunctionDeclaration(node)) {
    return node.body;
  }
  for (const declaration of node.declarationList.declarations) {
    if (!declaration.initializer) {
      continue;
    }
    if (
      ts.isArrowFunction(declaration.initializer) &&
      ts.isBlock(declaration.initializer.body)
    ) {
      return declaration.initializer.body;
    }
    if (ts.isFunctionExpression(declaration.initializer)) {
      return declaration.initializer.body;
    }
  }
  return undefined;
}

function extractFunctionParamNames(
  node: ts.FunctionDeclaration | ts.VariableStatement | undefined,
): Set<string> {
  const names = new Set<string>();
  if (!node) {
    return names;
  }

  const parameters = ts.isFunctionDeclaration(node)
    ? node.parameters
    : node.declarationList.declarations.flatMap((declaration) => {
        const initializer = declaration.initializer;
        if (
          initializer &&
          (ts.isArrowFunction(initializer) ||
            ts.isFunctionExpression(initializer))
        ) {
          return initializer.parameters;
        }
        return [];
      });

  for (const param of parameters) {
    if (ts.isIdentifier(param.name)) {
      names.add(param.name.text);
    }
  }
  return names;
}

function countObservedLoopIterations(
  scopeEvents: TraceEvent[],
): Map<number, number> {
  const counts = new Map<number, number>();
  for (const event of scopeEvents) {
    if (event.type === "scope" && event.event.kind === "loop_enter") {
      counts.set(
        event.event.loop_id,
        (counts.get(event.event.loop_id) ?? 0) + 1,
      );
    }
  }
  return counts;
}

function visitStatementsForLoopSnapshots(
  statements: ts.NodeArray<ts.Statement> | ReadonlyArray<ts.Statement>,
  sourceFile: ts.SourceFile,
  loopInfoByLine: Map<number, LoopInfo>,
  loopIterations: Map<number, number>,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
  snapshots: LoopBodyState[],
): void {
  for (const statement of statements) {
    if (ts.isVariableStatement(statement)) {
      visitVariableDeclarationListForLoopSnapshots(
        statement.declarationList,
        resolveName,
        flowMap,
      );
      continue;
    }

    if (ts.isExpressionStatement(statement)) {
      visitExpressionForLoopSnapshots(
        statement.expression,
        resolveName,
        flowMap,
      );
      continue;
    }

    if (ts.isIfStatement(statement)) {
      const condition = buildSymExprWithFlow(statement.expression, resolveName);
      const snapshot = new Map(flowMap);
      visitStatementsForLoopSnapshots(
        statementsFromBranch(statement.thenStatement),
        sourceFile,
        loopInfoByLine,
        loopIterations,
        resolveName,
        flowMap,
        snapshots,
      );
      const thenMap = new Map(flowMap);
      flowMap.clear();
      for (const [name, expr] of snapshot) {
        flowMap.set(name, expr);
      }
      if (statement.elseStatement) {
        visitStatementsForLoopSnapshots(
          statementsFromBranch(statement.elseStatement),
          sourceFile,
          loopInfoByLine,
          loopIterations,
          resolveName,
          flowMap,
          snapshots,
        );
      }
      const elseMap = new Map(flowMap);
      mergeLoopSnapshotFlowMaps(condition, snapshot, thenMap, elseMap, flowMap);
      continue;
    }

    if (ts.isForStatement(statement)) {
      const line =
        sourceFile.getLineAndCharacterOfPosition(statement.getStart(sourceFile))
          .line + 1;
      const loopInfo = loopInfoByLine.get(line);
      visitForStatementForLoopSnapshots(
        statement,
        loopInfo,
        loopIterations,
        sourceFile,
        loopInfoByLine,
        resolveName,
        flowMap,
        snapshots,
      );
      continue;
    }

    if (ts.isBlock(statement)) {
      visitStatementsForLoopSnapshots(
        statement.statements,
        sourceFile,
        loopInfoByLine,
        loopIterations,
        resolveName,
        flowMap,
        snapshots,
      );
    }
  }
}

function visitForStatementForLoopSnapshots(
  statement: ts.ForStatement,
  loopInfo: LoopInfo | undefined,
  loopIterations: Map<number, number>,
  sourceFile: ts.SourceFile,
  loopInfoByLine: Map<number, LoopInfo>,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
  snapshots: LoopBodyState[],
): void {
  visitForInitializerForLoopSnapshots(
    statement.initializer,
    resolveName,
    flowMap,
  );

  const iterationCount = loopInfo
    ? (loopIterations.get(loopInfo.loop_id) ?? 0)
    : 0;
  const trackedLocals = loopInfo
    ? collectTrackedLoopLocalNames(
        statement.statement,
        loopInfo.induction_var.name,
      )
    : [];

  for (let iteration = 0; iteration < iterationCount; iteration++) {
    if (loopInfo && trackedLocals.length > 0) {
      const locals: Record<string, SymExpr> = {};
      for (const name of trackedLocals) {
        const expr = flowMap.get(name);
        if (expr && expr.kind !== "unknown") {
          locals[name] = expr;
        }
      }
      if (Object.keys(locals).length > 0) {
        snapshots.push({
          loop_id: loopInfo.loop_id,
          iteration,
          locals,
        });
      }
    }

    visitStatementsForLoopSnapshots(
      statementsFromBranch(statement.statement),
      sourceFile,
      loopInfoByLine,
      loopIterations,
      resolveName,
      flowMap,
      snapshots,
    );
    if (statement.incrementor) {
      visitExpressionForLoopSnapshots(
        statement.incrementor,
        resolveName,
        flowMap,
      );
    }
  }
}

function visitForInitializerForLoopSnapshots(
  initializer: ts.ForInitializer | undefined,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
): void {
  if (!initializer) {
    return;
  }
  if (ts.isVariableDeclarationList(initializer)) {
    visitVariableDeclarationListForLoopSnapshots(
      initializer,
      resolveName,
      flowMap,
    );
    return;
  }
  visitExpressionForLoopSnapshots(initializer, resolveName, flowMap);
}

function visitVariableDeclarationListForLoopSnapshots(
  declarationList: ts.VariableDeclarationList,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
): void {
  for (const declaration of declarationList.declarations) {
    if (ts.isIdentifier(declaration.name) && declaration.initializer) {
      const expr = buildSymExprWithFlow(declaration.initializer, resolveName);
      if (expr.kind !== "unknown") {
        flowMap.set(declaration.name.text, expr);
      }
    }
  }
}

function visitExpressionForLoopSnapshots(
  expression: ts.Expression,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
): void {
  if (ts.isBinaryExpression(expression) && ts.isIdentifier(expression.left)) {
    const nextExpr = buildLoopSnapshotMutatedExpr(
      expression.left.text,
      expression.operatorToken.kind,
      expression.right,
      resolveName,
    );
    if (nextExpr.kind !== "unknown") {
      flowMap.set(expression.left.text, nextExpr);
    }
    return;
  }

  if (
    (ts.isPrefixUnaryExpression(expression) ||
      ts.isPostfixUnaryExpression(expression)) &&
    ts.isIdentifier(expression.operand)
  ) {
    const operator =
      expression.operator === ts.SyntaxKind.PlusPlusToken
        ? "add"
        : expression.operator === ts.SyntaxKind.MinusMinusToken
          ? "sub"
          : null;
    const current = resolveName(expression.operand.text);
    if (operator && current && current.kind !== "unknown") {
      flowMap.set(expression.operand.text, {
        kind: "bin_op",
        op: operator,
        left: current,
        right: { kind: "const", type: "int", value: 1 },
      });
    }
  }
}

function buildLoopSnapshotMutatedExpr(
  name: string,
  operatorKind: ts.SyntaxKind,
  right: ts.Expression,
  resolveName: (name: string) => SymExpr | undefined,
): SymExpr {
  if (operatorKind === ts.SyntaxKind.EqualsToken) {
    return buildSymExprWithFlow(right, resolveName);
  }

  const current = resolveName(name);
  const rightExpr = buildSymExprWithFlow(right, resolveName);
  if (!current || current.kind === "unknown" || rightExpr.kind === "unknown") {
    return { kind: "unknown" };
  }

  const op =
    operatorKind === ts.SyntaxKind.PlusEqualsToken
      ? "add"
      : operatorKind === ts.SyntaxKind.MinusEqualsToken
        ? "sub"
        : null;

  if (!op) {
    return { kind: "unknown" };
  }

  return {
    kind: "bin_op",
    op,
    left: current,
    right: rightExpr,
  };
}

function statementsFromBranch(
  statement: ts.Statement,
): ReadonlyArray<ts.Statement> {
  return ts.isBlock(statement) ? statement.statements : [statement];
}

function mergeLoopSnapshotFlowMaps(
  condition: SymExpr,
  snapshot: Map<string, SymExpr>,
  thenMap: Map<string, SymExpr>,
  elseMap: Map<string, SymExpr>,
  flowMap: Map<string, SymExpr>,
): void {
  if (condition.kind === "unknown") {
    flowMap.clear();
    for (const [name, expr] of elseMap) {
      flowMap.set(name, expr);
    }
    for (const [name, expr] of thenMap) {
      if (!flowMap.has(name)) {
        flowMap.set(name, expr);
      }
    }
    return;
  }

  const allNames = new Set([...thenMap.keys(), ...elseMap.keys()]);
  flowMap.clear();

  for (const name of allNames) {
    const thenExpr = thenMap.get(name);
    const elseExpr = elseMap.get(name);
    const previousExpr = snapshot.get(name);

    if (thenExpr === elseExpr) {
      if (thenExpr) {
        flowMap.set(name, thenExpr);
      }
      continue;
    }

    if (
      thenExpr &&
      elseExpr &&
      JSON.stringify(thenExpr) === JSON.stringify(elseExpr)
    ) {
      flowMap.set(name, thenExpr);
      continue;
    }

    const mergedThenExpr = thenExpr ?? previousExpr;
    const mergedElseExpr = elseExpr ?? previousExpr;
    if (mergedThenExpr && mergedElseExpr) {
      flowMap.set(name, {
        kind: "ite",
        condition,
        then_expr: mergedThenExpr,
        else_expr: mergedElseExpr,
      });
    } else if (mergedThenExpr) {
      flowMap.set(name, mergedThenExpr);
    } else if (mergedElseExpr) {
      flowMap.set(name, mergedElseExpr);
    }
  }
}

function collectTrackedLoopLocalNames(
  statement: ts.Statement,
  inductionVarName: string,
): string[] {
  const tracked = new Set<string>([inductionVarName]);

  function walk(node: ts.Node): void {
    if (
      ts.isFunctionDeclaration(node) ||
      ts.isFunctionExpression(node) ||
      ts.isArrowFunction(node)
    ) {
      return;
    }

    if (ts.isBinaryExpression(node) && ts.isIdentifier(node.left)) {
      tracked.add(node.left.text);
    }

    if (
      (ts.isPrefixUnaryExpression(node) || ts.isPostfixUnaryExpression(node)) &&
      ts.isIdentifier(node.operand)
    ) {
      tracked.add(node.operand.text);
    }

    ts.forEachChild(node, walk);
  }

  walk(statement);
  return [...tracked];
}

/**
 * Truncate a message string to fit within maxBytes.
 * If truncated, appends a suffix indicating the message was cut.
 */
export function truncateMessage(
  msg: string,
  maxBytes: number = MESSAGE_MAX_BYTES,
): string {
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
    ...(ciphertextParamIndex !== undefined && {
      ciphertext_param_index: ciphertextParamIndex,
    }),
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
  const makeLogger =
    (level: string) =>
    (...args: unknown[]): void => {
      const message = truncateMessage(
        args
          .map((a) =>
            typeof a === "string"
              ? a
              : (JSON.stringify(a, serializeReplacer) ?? String(a)),
          )
          .join(" "),
      );
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
            const text =
              typeof chunk === "string"
                ? chunk
                : new TextDecoder().decode(chunk);
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
  sandboxProviders?: SandboxProvider[],
): Promise<RawExecuteResult> {
  const fn = timing
    ? timing.sync("execute.module_load", () =>
        resolveFunction(
          filePath,
          functionRef,
          resolverAdapters,
          sandboxProviders,
        ),
      )
    : resolveFunction(
        filePath,
        functionRef,
        resolverAdapters,
        sandboxProviders,
      );

  const previousTarget = consoleTarget;
  let metrics: MeasuredExecution;

  if (capture) {
    const sideEffects: SideEffect[] = [];
    consoleTarget = createCapturingConsole(sideEffects);
    try {
      const reconstructedInputs = inputs.map(reconstructValue);
      metrics = await measureExecution(
        () => fn(...reconstructedInputs),
        timing,
      );
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
      loop_body_states: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
      adapter_hints: metrics.thrownError
        ? detectRuntimeHints(metrics.thrownError)
        : [],
    };
  } else {
    // No-capture fast path: skip all capture infrastructure.
    // NOOP_CONSOLE silences user code's console calls to prevent stdout pollution.
    consoleTarget = NOOP_CONSOLE;
    try {
      const reconstructedInputs = inputs.map(reconstructValue);
      metrics = await measureExecution(
        () => fn(...reconstructedInputs),
        timing,
      );
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
      loop_body_states: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
      adapter_hints: metrics.thrownError
        ? detectRuntimeHints(metrics.thrownError)
        : [],
    };
  }
}

/**
 * Execute a target through an adapter-owned invocation hook instead of
 * calling the exported symbol directly.
 *
 * Used when `FunctionAnalysis.invocation_model` reports `kind: "adapter"`.
 * The hook is responsible for mounting, scenario-driving, or otherwise
 * invoking the target with `inputs` (which conform to the synthetic
 * parameter schema declared on the model). The hook's `InvocationOutcome`
 * is funnelled into a `RawExecuteResult` so the response wire shape stays
 * identical to direct-call execution — no new ExecuteResponse fields.
 *
 * Branch decisions, line coverage, path constraints, and call traces are
 * empty: adapter-owned execution is opaque to the instrumentor for now.
 * Surface them later when an instrumented adapter mode is required.
 */
export async function executeAdapterOwned(args: {
  hook: InvocationHook;
  invocationModel: AdapterInvocationModel;
  fileForExec: string;
  functionName: string;
  inputs: unknown[];
  capture?: boolean;
  timing?: TimingCollector;
}): Promise<RawExecuteResult> {
  const capture = args.capture ?? true;
  const sideEffects: SideEffect[] = [];
  const previousTarget = consoleTarget;
  consoleTarget = capture ? createCapturingConsole(sideEffects) : NOOP_CONSOLE;

  const reconstructedInputs = args.inputs.map(reconstructValue);
  const ctx: InvocationContext = {
    fileForExec: args.fileForExec,
    functionName: args.functionName,
    invocationModel: args.invocationModel,
    inputs: reconstructedInputs,
    capture,
  };

  let returnValue: unknown = null;
  let thrownError: ErrorInfo | null = null;
  let outcomeSideEffects: SideEffect[] = [];

  tryGc();
  const startMem = process.memoryUsage();
  const startCpu = process.cpuUsage();
  const startTime = process.hrtime.bigint();

  try {
    const invokeFn = () => Promise.resolve(args.hook.invoke(ctx));
    const outcome = args.timing
      ? await args.timing.async("execute.invoke_hook", invokeFn)
      : await invokeFn();

    if (outcome.thrown_error) {
      thrownError = {
        error_type: outcome.thrown_error.error_type,
        message: outcome.thrown_error.message,
        stack: outcome.thrown_error.stack,
        error_category:
          outcome.thrown_error.error_category ??
          classifyError(
            outcome.thrown_error.error_type,
            outcome.thrown_error.message,
          ),
      };
    } else {
      returnValue = outcome.return_value ?? null;
    }
    outcomeSideEffects = outcome.side_effects ? [...outcome.side_effects] : [];
  } catch (e: unknown) {
    // The hook itself threw (as opposed to returning a structured thrownError).
    // Build an ErrorInfo the same way measureExecution does.
    const err = e as {
      constructor?: { name?: string };
      message?: string;
      stack?: string;
    };
    const errorType = err.constructor?.name ?? "Error";
    const errorMessage = String(err.message ?? e);
    thrownError = {
      error_type: errorType,
      message: errorMessage,
      stack: err.stack ?? null,
      error_category: classifyError(errorType, errorMessage),
    };
  } finally {
    consoleTarget = previousTarget;
  }

  const endTime = process.hrtime.bigint();
  const endCpu = process.cpuUsage(startCpu);
  const endMem = process.memoryUsage();
  const performance: PerformanceMetrics = {
    wall_time_ms: Number(endTime - startTime) / 1_000_000,
    cpu_time_us: endCpu.user + endCpu.system,
    heap_used_bytes: Math.max(0, endMem.heapUsed - startMem.heapUsed),
    heap_allocated_bytes: Math.max(0, endMem.heapTotal - startMem.heapTotal),
  };

  if (capture && thrownError) {
    sideEffects.push({
      kind: "thrown_error",
      error_type: thrownError.error_type,
      message: thrownError.message,
      stack: thrownError.stack,
    });
  }

  // Concatenate hook-supplied side effects after console-captured ones so the
  // ordering matches what a user reading the response sees: stdout emitted
  // during invoke() first, then structured events the adapter chose to
  // surface.
  if (outcomeSideEffects.length > 0) {
    sideEffects.push(...outcomeSideEffects);
  }

  return {
    return_value: thrownError ? null : returnValue,
    thrown_error: thrownError,
    side_effects: sideEffects,
    branch_path: [],
    path_constraints: [],
    lines_executed: [],
    performance,
    calls_to_external: [],
    scope_events: [],
    loop_body_states: [],
    discovered_dependencies: [],
    connection_failures: [],
    runtime_crypto_boundaries: [],
    adapter_hints: thrownError ? detectRuntimeHints(thrownError) : [],
  };
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
  sandboxProviders?: SandboxProvider[],
  loops: LoopInfo[] = [],
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
    const compile = () =>
      transpileAndCompile(
        instrumentedSource,
        sourceFilePath,
        sourceFilePath ?? "instrumented.js",
      );
    compiledScript = timing
      ? timing.sync("execute.transpile", compile)
      : compile();
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
  const loopBodyStates: LoopBodyState[] = [];
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
    const constraint: SymConstraint =
      symExpr.kind !== "unknown"
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
          constraint:
            sym.kind !== "unknown"
              ? { kind: "expr", expr: sym }
              : { kind: "unknown", hint: "unsupported expression" },
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
          constraint:
            sym.kind !== "unknown"
              ? { kind: "expr", expr: sym }
              : { kind: "unknown", hint: "unsupported expression" },
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
    const constraint: SymConstraint =
      symExpr.kind !== "unknown"
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
    const boundary = buildRuntimeCryptoBoundary(
      boundaryId,
      kind,
      functionName,
      args,
    );
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
          const message =
            typeof errData === "object" &&
            errData !== null &&
            "message" in errData
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
        const idx =
          mock.default_behavior === "repeat_last"
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
      const errMsg =
        thrownError instanceof Error
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
  const sandboxConsole = capture
    ? createCapturingConsole(sideEffects)
    : NOOP_CONSOLE;
  const sandboxProc = capture
    ? createCapturingProcess(sideEffects)
    : NOOP_PROCESS;
  const rawRequire = sourceFilePath
    ? createRequire(path.resolve(sourceFilePath))
    : require;
  const activeResolverAdapters =
    resolverAdapters ?? getDefaultResolverAdapters(sourceFilePath);

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
      if (
        moduleId.startsWith(".") ||
        moduleId.startsWith("/") ||
        seenDiscoveredModules.has(moduleId)
      ) {
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
    ...WEB_GLOBALS,
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
    ...(sourceFilePath
      ? { __filename: sourceFilePath, __dirname: path.dirname(sourceFilePath) }
      : {}),
    [RECORD_FUNCTION]: recordFn,
    [BRANCH_FUNCTION]: branchFn,
    [MCDC_RECORD_FUNCTION]: mcdcRecordFn,
    [MCDC_BRANCH_FUNCTION]: mcdcBranchFn,
    [SCOPE_EVENT_FUNCTION]: scopeEventFn,
    [MOCK_REGISTRY]: mockRegistry,
    [MOCK_CALL_FUNCTION]: mockCallFn,
    [CRYPTO_BOUNDARY_FUNCTION]: cryptoBoundaryFn,
    __shatter_import: createShatterImport(sandboxRequire),
    __shatter_import_meta: {
      url: sourceFilePath ?? "",
      env: { ...DEFAULT_IMPORT_META_ENV },
    },
  });

  if (sandboxProviders) {
    for (const provider of sandboxProviders) {
      provider.augmentSandbox(sandbox as Record<string, unknown>);
    }
  }

  const loadModule = (): void => {
    compiledScript.runInContext(sandbox, { timeout: getExecTimeoutMs() });
  };
  if (timing) {
    timing.sync("execute.module_load", loadModule);
  } else {
    loadModule();
  }

  // Resolve the function from the module exports
  const finalExports = (sandbox as Record<string, unknown>)["module"] as {
    exports: Record<string, unknown>;
  };

  // Snapshot module-level variables before execution (JSON strings, not deep clones)
  const exportKeys = Object.keys(finalExports.exports).filter(
    (k) => typeof finalExports.exports[k] !== "function",
  );
  const beforeSnapshot = new Map<string, string | undefined>();
  for (const key of exportKeys) {
    try {
      beforeSnapshot.set(
        key,
        JSON.stringify(finalExports.exports[key], serializeReplacer),
      );
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
        afterJson = JSON.stringify(
          finalExports.exports[key],
          serializeReplacer,
        );
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

  if (sourceFilePath && loops.length > 0) {
    const sourceText = fs.readFileSync(sourceFilePath, "utf-8");
    loopBodyStates.push(
      ...extractLoopBodyStates(
        sourceText,
        functionName,
        sourceFilePath,
        loops,
        scopeEvents,
      ),
    );
  }

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
    loop_body_states: loopBodyStates,
    discovered_dependencies: discoveredDeps,
    connection_failures: connectionFailures,
    runtime_crypto_boundaries: cryptoBoundaries,
    adapter_hints: metrics.thrownError
      ? detectRuntimeHints(metrics.thrownError)
      : [],
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
  const { effects, truncation } =
    rawResult.side_effects.length === 0
      ? { effects: [] as SideEffect[], truncation: undefined }
      : timing
        ? timing.sync("execute.trace_capture", () =>
            truncateSideEffects(rawResult.side_effects),
          )
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

  if (rawResult.loop_body_states.length > 0) {
    response.loop_body_states = rawResult.loop_body_states;
  }

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

  if (rawResult.adapter_hints.length > 0) {
    response.adapter_hints = rawResult.adapter_hints;
  }

  response.outcome = deriveOutcome(rawResult);

  return response;
}

/**
 * Derive the standardized InvocationOutcome from a raw execute result.
 *
 * Status assignment (str-hy9b.A1/A5):
 * - `completed` — function returned normally (no thrown_error).
 * - `timed_out` — thrown_error indicates an execution timeout (vm.runInContext
 *   timeout, async race timeout, or a timeout-classified infrastructure error).
 * - `runtime_failed` — any other thrown error from the user function.
 *
 * The TS executor currently does not surface `build_failed`, `unsupported`,
 * `completed_with_findings`, or `skipped_by_policy` from this path; those
 * either arrive on `error` responses (compile/parse errors, unsupported
 * targets) or are reserved for upstream consumers.
 */
function deriveOutcome(rawResult: RawExecuteResult): InvocationOutcome {
  const thrown = rawResult.thrown_error;
  if (!thrown) {
    return {
      status: "completed",
      return_value: rawResult.return_value,
    };
  }
  if (isTimeoutError(thrown)) {
    return {
      status: "timed_out",
      short_reason: thrown.message || "execution timed out",
      thrown_error: thrown,
    };
  }
  return {
    status: "runtime_failed",
    short_reason: thrown.message || `${thrown.error_type} thrown`,
    thrown_error: thrown,
  };
}

function isTimeoutError(err: ErrorInfo): boolean {
  if (err.error_type === "ERR_SCRIPT_EXECUTION_TIMEOUT") return true;
  if (err.error_category === "infrastructure") {
    return classifyConnectionFailure(err.message) === "timeout";
  }
  return false;
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
