/**
 * Command handlers for the Shatter protocol.
 *
 * Each handler receives a typed request and returns a typed response.
 * The instrument handler stores instrumented source in memory so the
 * execute handler can use it for branch-recording execution.
 *
 * Startup staging: the frontend reaches handshake-ready state before any
 * heavy module loads. After the handshake response is sent, all heavy modules
 * begin loading concurrently in the background so they are warm by the time
 * analyze/instrument/execute requests arrive.
 *
 * Heavy modules and their approximate cold-load costs:
 *   analyzer      ~233ms  (TypeScript compiler via analyzer.ts static import)
 *   executor      ~206ms  (TypeScript compiler + VM sandbox)
 *   instrumentor  ~185ms  (TypeScript compiler + AST transforms)
 *   setup-loader  ~186ms  (TypeScript compiler + module sandbox)
 *   wasm-generator  ~8ms  (Extism WASM runtime — not preloaded, only for .wasm files)
 */

import * as fs from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";
import {
  PROTOCOL_VERSION,
  FRONTEND_LANGUAGE,
  type Request,
  type Response,
  type ErrorResponse,
  type SetupLevel,
} from "./protocol.js";
import { TimingCollector } from "./timing.js";
import { InstrumentationWorker } from "./instrumentation-worker.js";
// Type-only imports for lazy-loaded modules — erased at compile time, no runtime cost.
import type { SetupModule } from "./setup-loader.js";

/** Supported capabilities for this frontend. */
const SUPPORTED_CAPABILITIES = [
  "analyze", "execute", "instrument", "prepare", "setup", "generate",
  "complex_type:date", "complex_type:date_time", "complex_type:duration",
  "complex_type:reg_exp", "complex_type:url", "complex_type:big_int",
  "complex_type:buffer", "complex_type:error", "complex_type:symbol",
];

// ---------------------------------------------------------------------------
// Lazy module loaders with background preloading
//
// Each heavy module is loaded on first use and cached. The handshake handler
// triggers background preloading of all heavy modules so they are warm by the
// time the first analyze/instrument/execute request arrives.
//
// Each module tracks two pieces of state:
//   _xxxPromise — the in-flight import(); set immediately so concurrent callers
//                 share the same load operation (no duplicate loads).
//   _xxx        — the resolved module; set when the promise resolves so guards
//                 like `if (_executor)` and getLoadedModuleNames() still work.
// ---------------------------------------------------------------------------

type ExecutorMod = typeof import('./executor.js');
type SetupLoaderMod = typeof import('./setup-loader.js');
type WasmGeneratorMod = typeof import('./wasm-generator.js');

let _executorPromise: Promise<ExecutorMod> | null = null;
let _executor: ExecutorMod | null = null;
let _setupLoaderPromise: Promise<SetupLoaderMod> | null = null;
let _setupLoader: SetupLoaderMod | null = null;
let _wasmGeneratorPromise: Promise<WasmGeneratorMod> | null = null;
let _wasmGenerator: WasmGeneratorMod | null = null;

function getExecutor(): Promise<ExecutorMod> {
  if (!_executorPromise) _executorPromise = import('./executor.js').then(m => (_executor = m));
  return _executorPromise;
}

function getSetupLoader(): Promise<SetupLoaderMod> {
  if (!_setupLoaderPromise) _setupLoaderPromise = import('./setup-loader.js').then(m => (_setupLoader = m));
  return _setupLoaderPromise;
}

function getWasmGenerator(): Promise<WasmGeneratorMod> {
  if (!_wasmGeneratorPromise) _wasmGeneratorPromise = import('./wasm-generator.js').then(m => (_wasmGenerator = m));
  return _wasmGeneratorPromise;
}

/**
 * Worker thread for CPU-bound analyze and instrument operations.
 * Created on handshake; the worker eagerly imports analyzer and instrumentor.
 */
let _worker: InstrumentationWorker | null = null;

/** Path override for the worker script, used in tests. */
let _workerPath: string | undefined;

/** Set a custom worker script path. Exposed for testing. */
export function setWorkerPath(workerPath: string): void {
  _workerPath = workerPath;
}

function getWorker(): InstrumentationWorker {
  if (!_worker) {
    _worker = new InstrumentationWorker(_workerPath);
  }
  return _worker;
}

/**
 * Start background loading of heavy modules concurrently.
 *
 * Called after the handshake response is sent. The worker thread is started
 * to eagerly load analyzer and instrumentor. Executor and setup-loader
 * continue to lazy-load on the main thread.
 */
function preloadHeavyModules(): void {
  getWorker(); // starts worker thread, which eagerly imports analyzer + instrumentor
  void getExecutor();
  void getSetupLoader();
  // wasm-generator (~8ms) is only needed for .wasm generator files; skip preload.
}

// ---------------------------------------------------------------------------
// Handler state
// ---------------------------------------------------------------------------

/** Track the last analyzed file so execute can resolve function references. */
let lastAnalyzedFile: string | null = null;

/**
 * Project root from the last analyze request.
 * Passed to executor.setProjectRoot() on first execute (deferred to avoid
 * loading executor during analyze-only sessions).
 */
let lastProjectRoot: string | undefined;

/**
 * Stored instrumented sources, keyed by "file:function".
 * Set by the instrument handler, consumed by the execute handler.
 */
const instrumentedSources = new Map<string, string>();

/**
 * Loaded setup modules, keyed by file path.
 * Cached so teardown can use the same module instance as setup.
 */
const loadedSetupModules = new Map<string, SetupModule>();

/**
 * Setup contexts keyed by "level:scope" (e.g. "function:myFunc", "session:main").
 * Separate keys per level ensure that session-, file-, function-, and
 * execution-level contexts coexist without collision.
 */
const setupContexts = new Map<string, { module: SetupModule; context: unknown }>();

/** Build a composite cache key for the setup context map. */
function setupContextKey(level: SetupLevel, scope: string): string {
  return `${level}:${scope}`;
}

/**
 * Maps prepare_id → instrument cache key ("resolvedFile:functionName").
 * Set by the prepare handler; used by execute to look up pre-warmed scripts.
 * Cleared on teardown.
 */
const preparedKeys = new Map<string, string>();

/**
 * Maps instrument cache key → current prepare_id for stale detection.
 * When a new prepare arrives for the same target with a different prepare_id,
 * the old entry is invalidated.
 */
const preparedTargets = new Map<string, string>();

/**
 * Compute a deterministic 16-char hex prepare_id from file, function, and mocks.
 * Matches the algorithm used by the Go and Rust frontends.
 */
function computePrepareId(file: string, funcName: string, mocks: Array<{ symbol: string }>): string {
  const h = crypto.createHash("sha256");
  h.update(`${file}\x00${funcName}\x00`);
  const symbols = mocks.map(m => m.symbol).sort();
  for (const s of symbols) {
    h.update(`${s}\x00`);
  }
  return h.digest("hex").slice(0, 16);
}

let timingEnabled = false;

function wantsTimingFromHandshake(request: Request): boolean {
  return request.command === "handshake" && request.capabilities.includes("timing");
}

function maybeTimingCollector(): TimingCollector | undefined {
  return timingEnabled ? new TimingCollector() : undefined;
}

function finalizeResponse<T extends Response>(response: T, timing?: TimingCollector): T {
  if (!timing) {
    return response;
  }

  const finalized = timing.sync("serialize.response", () => response);
  const summary = timing.toSummary();
  if (summary) {
    finalized.timing = summary;
  }
  return finalized;
}

/**
 * Dispatch a parsed request to the appropriate handler.
 *
 * Returns a Response and a shutdown flag. The handler is async because
 * WASM generator loading requires awaiting plugin creation.
 */
export async function handleRequest(request: Request): Promise<{ response: Response; shutdown: boolean }> {
  if (!isVersionCompatible(request.protocol_version)) {
    return {
      response: errorResponse(request.id, "version_mismatch",
        `Unsupported protocol version: ${request.protocol_version} (expected ${PROTOCOL_VERSION})`),
      shutdown: false,
    };
  }

  switch (request.command) {
    case "handshake":
      timingEnabled = wantsTimingFromHandshake(request);
      // Kick off background loading of heavy modules so they are warm by the
      // time the first analyze/instrument/execute request arrives. The handshake
      // response is returned immediately — preloads run concurrently.
      preloadHeavyModules();
      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "handshake",
          frontend_version: PROTOCOL_VERSION,
          language: FRONTEND_LANGUAGE,
          capabilities: SUPPORTED_CAPABILITIES,
        },
        shutdown: false,
      };

    case "analyze": {
      const timing = maybeTimingCollector();
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      lastAnalyzedFile = path.resolve(request.file);
      // Cache project_root for use by execute (deferred to avoid loading executor here).
      lastProjectRoot = request.project_root ?? undefined;
      const worker = getWorker();
      const analyzeResult = await worker.analyze(
        request.file,
        request.function ?? null,
        request.project_root ?? null,
      );
      if (timing && analyzeResult.timingPhases) {
        timing.mergePhases(analyzeResult.timingPhases);
      }
      const functions = analyzeResult.functions;

      if (request.function != null && functions.length === 0) {
        return {
          response: errorResponse(
            request.id,
            "function_not_found",
            `Function not found: ${request.function} in ${request.file}`,
          ),
          shutdown: false,
        };
      }

      return {
        response: finalizeResponse({
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "analyze",
          functions,
        }, timing),
        shutdown: false,
      };
    }

    case "instrument": {
      const timing = maybeTimingCollector();
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      const worker = getWorker();
      const source = fs.readFileSync(request.file, "utf-8");
      const instrumentResult = await worker.instrument(
        source,
        request.function,
        request.file,
        request.mocks ?? [],
      );
      if (timing && instrumentResult.timingPhases) {
        timing.mergePhases(instrumentResult.timingPhases);
      }
      const result = instrumentResult.result;

      if ("error" in result) {
        return {
          response: errorResponse(request.id, "instrumentation_failed", result.error),
          shutdown: false,
        };
      }

      // Store the instrumented source for use by the execute handler.
      // Use resolved path so keys match execute's resolved fileForExec.
      const key = `${path.resolve(request.file)}:${request.function}`;
      instrumentedSources.set(key, result.instrumentedSource);
      // Invalidate any cached compiled script for this key — the source may have changed.
      // Executor may not be loaded yet during instrument-only flows; that's fine —
      // the cache entry won't exist either.
      if (_executor) _executor.deleteCompiledScriptEntry(key);
      lastAnalyzedFile = path.resolve(request.file);

      return {
        response: finalizeResponse({
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "instrument",
          instrumented: true,
          output_file: null,
          instrumentable_line_count: result.instrumentableLineCount,
        }, timing),
        shutdown: false,
      };
    }

    case "prepare": {
      const timing = maybeTimingCollector();
      const resolvedFile = path.resolve(request.file);
      if (!fs.existsSync(resolvedFile)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      const instrumentKey = `${resolvedFile}:${request.function}`;
      const instrumentedSource = instrumentedSources.get(instrumentKey);
      if (!instrumentedSource) {
        return {
          response: errorResponse(
            request.id,
            "instrumentation_failed",
            `No instrumented source for ${request.function} in ${request.file}. Call instrument first.`,
          ),
          shutdown: false,
        };
      }

      const prepareId = computePrepareId(resolvedFile, request.function, request.mocks);

      // Invalidate stale prepared target if the same key was prepared with different inputs.
      const oldPrepareId = preparedTargets.get(instrumentKey);
      if (oldPrepareId !== undefined && oldPrepareId !== prepareId) {
        preparedKeys.delete(oldPrepareId);
        if (_executor) _executor.deleteCompiledScriptEntry(instrumentKey);
        preparedTargets.delete(instrumentKey);
      }

      // Idempotent: if already prepared, return the same id.
      if (!preparedKeys.has(prepareId)) {
        const executor = await getExecutor();
        executor.warmCompiledScriptCache(instrumentedSource, instrumentKey);
        preparedKeys.set(prepareId, instrumentKey);
        preparedTargets.set(instrumentKey, prepareId);
      }

      return {
        response: finalizeResponse({
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "prepare",
          prepare_id: prepareId,
        } as const, timing),
        shutdown: false,
      };
    }

    case "execute": {
      const timing = maybeTimingCollector();
      const funcRef = request.function;
      const fileForExec = resolveFileForExecute(funcRef);

      if (!fileForExec) {
        return {
          response: errorResponse(
            request.id,
            "function_not_found",
            `Cannot resolve file for function: ${funcRef}. Use file:function format.`,
          ),
          shutdown: false,
        };
      }

      try {
        const executor = await getExecutor();
        // Apply project root that was cached from the last analyze request.
        if (lastProjectRoot !== undefined) {
          executor.setProjectRoot(lastProjectRoot);
        }

        // Check if we have instrumented source for this function.
        // When prepare_id is set, look up the instrument key from the prepare cache.
        const funcName = funcRef.includes(":") ? funcRef.split(":").pop()! : funcRef;
        let instrumentKey = `${fileForExec}:${funcName}`;
        if (request.prepare_id) {
          const preparedKey = preparedKeys.get(request.prepare_id);
          if (preparedKey) {
            instrumentKey = preparedKey;
          } else {
            // Stale or invalidated prepare_id — fall through to default instrumentKey.
          }
        }
        const instrumentedSource = instrumentedSources.get(instrumentKey);

        const capture = request.capture ?? true;
        let rawResult;
        if (instrumentedSource) {
          rawResult = timing
            ? await timing.async("execute.total", () =>
              executor.executeInstrumented(instrumentedSource, funcName, request.inputs, request.mocks ?? [], fileForExec, timing, capture, instrumentKey))
            : await executor.executeInstrumented(instrumentedSource, funcName, request.inputs, request.mocks ?? [], fileForExec, undefined, capture, instrumentKey);
        } else {
          rawResult = timing
            ? await timing.async("execute.total", () => executor.executeFunction(fileForExec, funcRef, request.inputs, timing, capture))
            : await executor.executeFunction(fileForExec, funcRef, request.inputs, undefined, capture);
        }

        return {
          response: finalizeResponse(executor.buildExecuteResponse(request.id, PROTOCOL_VERSION, rawResult, timing), timing),
          shutdown: false,
        };
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        return {
          response: errorResponse(request.id, "internal_error", `Execute failed: ${msg}`),
          shutdown: false,
        };
      }
    }

    case "setup": {
      const timing = maybeTimingCollector();
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      try {
        const { loadSetupModule, runSetup } = await getSetupLoader();
        let setupModule = loadedSetupModules.get(request.file);
        if (!setupModule) {
          setupModule = timing
            ? timing.sync("setup.module_load", () => loadSetupModule(request.file))
            : loadSetupModule(request.file);
          loadedSetupModules.set(request.file, setupModule);
        }

        const setupContext = timing
          ? await timing.async("setup.run", () => runSetup(
            setupModule!, request.scope, request.level, request.parent_context,
          ))
          : await runSetup(setupModule, request.scope, request.level, request.parent_context);
        const ctxKey = setupContextKey(request.level, request.scope);
        setupContexts.set(ctxKey, { module: setupModule, context: setupContext });

        return {
          response: finalizeResponse({
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "setup",
            setup_context: setupContext,
          }, timing),
          shutdown: false,
        };
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        return {
          response: errorResponse(request.id, "internal_error", `Setup failed: ${msg}`),
          shutdown: false,
        };
      }
    }

    case "teardown": {
      const timing = maybeTimingCollector();
      try {
        const ctxKey = setupContextKey(request.level, request.scope);
        const stored = setupContexts.get(ctxKey);
        if (!stored) {
          return {
            response: errorResponse(
              request.id,
              "internal_error",
              `No setup context found for ${request.level}:${request.scope}. Call setup first.`,
            ),
            shutdown: false,
          };
        }

        const { runTeardown } = await getSetupLoader();
        if (timing) {
          await timing.async("teardown.run", () => runTeardown(stored.module, request.scope, stored.context));
        } else {
          await runTeardown(stored.module, request.scope, stored.context);
        }
        setupContexts.delete(ctxKey);
        instrumentedSources.clear();
        preparedKeys.clear();
        preparedTargets.clear();
        // Only clear executor caches if executor was loaded this session.
        if (_executor) {
          _executor.clearCompiledScriptCache();
          _executor.clearModuleCache();
        }

        return {
          response: finalizeResponse({
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "teardown_ack",
          }, timing),
          shutdown: false,
        };
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        return {
          response: errorResponse(request.id, "internal_error", `Teardown failed: ${msg}`),
          shutdown: false,
        };
      }
    }

    case "generate": {
      const timing = maybeTimingCollector();
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      try {
        if (request.file.endsWith(".wasm")) {
          const { loadWasmPlugin, runWasmGenerator } = await getWasmGenerator();
          const plugin = timing
            ? await timing.async("generate.module_load", () => loadWasmPlugin(request.file))
            : await loadWasmPlugin(request.file);
          const result = timing
            ? await timing.async("generate.run", () => runWasmGenerator(plugin, request.name, request.recipe))
            : await runWasmGenerator(plugin, request.name, request.recipe);

          return {
            response: finalizeResponse({
              protocol_version: PROTOCOL_VERSION,
              id: request.id,
              status: "generate",
              value: result.value,
              generator_id: result.id,
              ...(result.recipe !== undefined ? { recipe: result.recipe } : {}),
            }, timing),
            shutdown: false,
          };
        }

        const { loadGeneratorModule, runGenerator } = await getSetupLoader();
        const generatorModule = timing
          ? timing.sync("generate.module_load", () => loadGeneratorModule(request.file))
          : loadGeneratorModule(request.file);
        const value = timing
          ? timing.sync("generate.run", () => runGenerator(generatorModule, request.name, request.kind))
          : runGenerator(generatorModule, request.name, request.kind);

        return {
          response: finalizeResponse({
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "generate",
            value,
            generator_id: "generated",
          }, timing),
          shutdown: false,
        };
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        return {
          response: errorResponse(request.id, "internal_error", `Generate failed: ${msg}`),
          shutdown: false,
        };
      }
    }

    case "shutdown":
      // Only invoke cleanup on modules that were actually loaded this session.
      if (_wasmGenerator) await _wasmGenerator.clearWasmCache();
      instrumentedSources.clear();
      preparedKeys.clear();
      preparedTargets.clear();
      if (_executor) {
        _executor.clearCompiledScriptCache();
        _executor.clearModuleCache();
      }
      if (_worker) {
        await _worker.terminate();
        _worker = null;
      }
      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "shutdown_ack",
        },
        shutdown: true,
      };
  }
}

/**
 * Resolve the file path for an execute request.
 *
 * The function reference can be in "file:function" format or just a function name.
 * If just a name, falls back to the last analyzed file.
 */
function resolveFileForExecute(funcRef: string): string | null {
  if (funcRef.includes(":")) {
    // file:function format — extract the file part
    const lastColon = funcRef.lastIndexOf(":");
    const file = funcRef.substring(0, lastColon);
    if (fs.existsSync(file)) {
      return path.resolve(file);
    }
  }

  // Fall back to last analyzed file (already resolved on store)
  return lastAnalyzedFile;
}

function isVersionCompatible(version: string): boolean {
  // For now, require exact match on major.minor
  const [reqMajor, reqMinor] = version.split(".").map(Number);
  const [ourMajor, ourMinor] = PROTOCOL_VERSION.split(".").map(Number);
  return reqMajor === ourMajor && reqMinor === ourMinor;
}

function errorResponse(id: number, code: ErrorResponse["code"], message: string): ErrorResponse {
  return {
    protocol_version: PROTOCOL_VERSION,
    id,
    status: "error",
    code,
    message,
  };
}

/**
 * Parse a JSON string into a Request, returning an error response if invalid.
 */
export function parseRequest(line: string): { request: Request } | { error: ErrorResponse } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    return {
      error: errorResponse(0, "invalid_request", `Invalid JSON: ${line.slice(0, 100)}`),
    };
  }

  if (typeof parsed !== "object" || parsed === null) {
    return {
      error: errorResponse(0, "invalid_request", "Request must be a JSON object"),
    };
  }

  const obj = parsed as Record<string, unknown>;

  if (typeof obj["id"] !== "number") {
    return {
      error: errorResponse(0, "invalid_request", "Request must have a numeric 'id' field"),
    };
  }

  const id = obj["id"] as number;

  if (typeof obj["protocol_version"] !== "string") {
    return {
      error: errorResponse(id, "invalid_request", "Request must have a 'protocol_version' string field"),
    };
  }

  if (typeof obj["command"] !== "string") {
    return {
      error: errorResponse(id, "invalid_request", "Request must have a 'command' string field"),
    };
  }

  const validCommands = ["handshake", "analyze", "instrument", "prepare", "execute", "setup", "teardown", "generate", "shutdown"];
  if (!validCommands.includes(obj["command"] as string)) {
    return {
      error: errorResponse(id, "invalid_request", `Unknown command: ${String(obj["command"])}`),
    };
  }

  return { request: parsed as Request };
}

/**
 * Clear stored instrumented sources and setup state. Useful for testing.
 * Also resets lazy module caches so tests get a clean slate.
 */
export function clearInstrumentedSources(): void {
  instrumentedSources.clear();
  preparedKeys.clear();
  if (_executor) _executor.clearCompiledScriptCache();
  loadedSetupModules.clear();
  setupContexts.clear();
  // Worker is kept alive across clears — it's stateless (no caches to reset).
  // Only shutdown and terminateWorker() destroy it.
  _executorPromise = null;  _executor = null;
  _setupLoaderPromise = null;  _setupLoader = null;
  _wasmGeneratorPromise = null;  _wasmGenerator = null;
}

/** Terminate the worker thread. Exposed for test cleanup (afterAll). */
export async function terminateWorker(): Promise<void> {
  if (_worker) {
    await _worker.terminate();
    _worker = null;
  }
}

/** Number of cached instrumented sources. Exposed for testing. */
export function instrumentedSourcesSize(): number {
  return instrumentedSources.size;
}

/** Number of cached setup contexts. Exposed for testing. */
export function setupContextsSize(): number {
  return setupContexts.size;
}

/**
 * Names of heavy modules whose promises have resolved this session.
 * Used in tests to verify which modules have finished loading.
 */
export function getLoadedModuleNames(): string[] {
  const loaded: string[] = [];
  if (_worker) loaded.push("analyzer", "instrumentor"); // loaded eagerly in worker thread
  if (_executor) loaded.push("executor");
  if (_setupLoader) loaded.push("setupLoader");
  if (_wasmGenerator) loaded.push("wasmGenerator");
  return loaded;
}
