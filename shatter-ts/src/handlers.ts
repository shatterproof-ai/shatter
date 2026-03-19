/**
 * Command handlers for the Shatter protocol.
 *
 * Each handler receives a typed request and returns a typed response.
 * The instrument handler stores instrumented source in memory so the
 * execute handler can use it for branch-recording execution.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import {
  PROTOCOL_VERSION,
  FRONTEND_LANGUAGE,
  type Request,
  type Response,
  type ErrorResponse,
  type SetupLevel,
} from "./protocol.js";
import { analyzeFile } from "./analyzer.js";
import { instrumentFunction } from "./instrumentor.js";
import {
  executeFunction,
  executeInstrumented,
  buildExecuteResponse,
  setProjectRoot,
  clearModuleCache,
} from "./executor.js";
import {
  loadSetupModule,
  runSetup,
  runTeardown,
  loadGeneratorModule,
  runGenerator,
  type SetupModule,
} from "./setup-loader.js";
import {
  loadWasmPlugin,
  runWasmGenerator,
  clearWasmCache,
} from "./wasm-generator.js";
import { TimingCollector } from "./timing.js";

/** Supported capabilities for this frontend. */
const SUPPORTED_CAPABILITIES = [
  "analyze", "execute", "instrument", "setup", "generate",
  "complex_type:date", "complex_type:date_time", "complex_type:duration",
  "complex_type:reg_exp", "complex_type:url", "complex_type:big_int",
  "complex_type:buffer", "complex_type:error", "complex_type:symbol",
];

/** Track the last analyzed file so execute can resolve function references. */
let lastAnalyzedFile: string | null = null;

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
      return {
        response: finalizeResponse({
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "handshake",
          frontend_version: PROTOCOL_VERSION,
          language: FRONTEND_LANGUAGE,
          capabilities: SUPPORTED_CAPABILITIES,
        }, timingEnabled ? new TimingCollector() : undefined),
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
      setProjectRoot(request.project_root);
      const functions = timing
        ? timing.sync("analyze.total", () =>
          analyzeFile(request.file, request.function, request.project_root, timing))
        : analyzeFile(request.file, request.function, request.project_root);

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

      const source = fs.readFileSync(request.file, "utf-8");
      const result = timing
        ? timing.sync("instrument.total", () =>
          instrumentFunction(source, request.function, request.file, request.mocks ?? [], timing))
        : instrumentFunction(source, request.function, request.file, request.mocks ?? []);

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
        // Check if we have instrumented source for this function
        const funcName = funcRef.includes(":") ? funcRef.split(":").pop()! : funcRef;
        const instrumentKey = `${fileForExec}:${funcName}`;
        const instrumentedSource = instrumentedSources.get(instrumentKey);

        const capture = request.capture ?? true;
        let rawResult;
        if (instrumentedSource) {
          rawResult = timing
            ? await timing.async("execute.total", () =>
              executeInstrumented(instrumentedSource, funcName, request.inputs, request.mocks ?? [], fileForExec, timing, capture))
            : await executeInstrumented(instrumentedSource, funcName, request.inputs, request.mocks ?? [], fileForExec, undefined, capture);
        } else {
          rawResult = timing
            ? await timing.async("execute.total", () => executeFunction(fileForExec, funcRef, request.inputs, timing, capture))
            : await executeFunction(fileForExec, funcRef, request.inputs, undefined, capture);
        }

        return {
          response: finalizeResponse(buildExecuteResponse(request.id, PROTOCOL_VERSION, rawResult, timing), timing),
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

        if (timing) {
          await timing.async("teardown.run", () => runTeardown(stored.module, request.scope, stored.context));
        } else {
          await runTeardown(stored.module, request.scope, stored.context);
        }
        setupContexts.delete(ctxKey);
        instrumentedSources.clear();
        clearModuleCache();

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
      await clearWasmCache();
      instrumentedSources.clear();
      clearModuleCache();
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

  const validCommands = ["handshake", "analyze", "instrument", "execute", "setup", "teardown", "generate", "shutdown"];
  if (!validCommands.includes(obj["command"] as string)) {
    return {
      error: errorResponse(id, "invalid_request", `Unknown command: ${String(obj["command"])}`),
    };
  }

  return { request: parsed as Request };
}

/**
 * Clear stored instrumented sources and setup state. Useful for testing.
 */
export function clearInstrumentedSources(): void {
  instrumentedSources.clear();
  loadedSetupModules.clear();
  setupContexts.clear();
}

/** Number of cached instrumented sources. Exposed for testing. */
export function instrumentedSourcesSize(): number {
  return instrumentedSources.size;
}

/** Number of cached setup contexts. Exposed for testing. */
export function setupContextsSize(): number {
  return setupContexts.size;
}
