/**
 * Command handlers for the Shatter protocol.
 *
 * Each handler receives a typed request and returns a typed response.
 * The instrument handler stores instrumented source in memory so the
 * execute handler can use it for branch-recording execution.
 */

import * as fs from "node:fs";
import {
  PROTOCOL_VERSION,
  FRONTEND_LANGUAGE,
  type Request,
  type Response,
  type ErrorResponse,
} from "./protocol.js";
import { analyzeFile } from "./analyzer.js";
import { instrumentFunction } from "./instrumentor.js";
import {
  executeFunction,
  executeInstrumented,
  buildExecuteResponse,
} from "./executor.js";
import {
  loadSetupModule,
  runSetup,
  runTeardown,
  loadGeneratorModule,
  runGenerator,
  type SetupModule,
} from "./setup-loader.js";

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
 * Setup contexts from the most recent setup call, keyed by function name.
 * Stored so teardown can pass the context back to the teardown function.
 */
const setupContexts = new Map<string, { module: SetupModule; context: unknown }>();

/**
 * Dispatch a parsed request to the appropriate handler.
 *
 * Returns a Response, or null if the frontend should shut down after
 * sending the response.
 */
export function handleRequest(request: Request): { response: Response; shutdown: boolean } {
  if (!isVersionCompatible(request.protocol_version)) {
    return {
      response: errorResponse(request.id, "version_mismatch",
        `Unsupported protocol version: ${request.protocol_version} (expected ${PROTOCOL_VERSION})`),
      shutdown: false,
    };
  }

  switch (request.command) {
    case "handshake":
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
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      lastAnalyzedFile = request.file;
      const functions = analyzeFile(request.file, request.function);

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
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "analyze",
          functions,
        },
        shutdown: false,
      };
    }

    case "instrument": {
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      const source = fs.readFileSync(request.file, "utf-8");
      const result = instrumentFunction(source, request.function, request.file);

      if ("error" in result) {
        return {
          response: errorResponse(request.id, "instrumentation_failed", result.error),
          shutdown: false,
        };
      }

      // Store the instrumented source for use by the execute handler
      const key = `${request.file}:${request.function}`;
      instrumentedSources.set(key, result.instrumentedSource);
      lastAnalyzedFile = request.file;

      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "instrument",
          instrumented: true,
          output_file: null,
        },
        shutdown: false,
      };
    }

    case "execute": {
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

        let rawResult;
        if (instrumentedSource) {
          rawResult = executeInstrumented(instrumentedSource, funcName, request.inputs);
        } else {
          rawResult = executeFunction(fileForExec, funcRef, request.inputs);
        }

        return {
          response: buildExecuteResponse(request.id, PROTOCOL_VERSION, rawResult),
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
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      try {
        let setupModule = loadedSetupModules.get(request.file);
        if (!setupModule) {
          setupModule = loadSetupModule(request.file);
          loadedSetupModules.set(request.file, setupModule);
        }

        const setupContext = runSetup(setupModule, request.function, request.mode);

        setupContexts.set(request.function, { module: setupModule, context: setupContext });

        return {
          response: {
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "setup",
            setup_context: setupContext,
          },
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
      try {
        const stored = setupContexts.get(request.function);
        if (!stored) {
          return {
            response: errorResponse(
              request.id,
              "internal_error",
              `No setup context found for function: ${request.function}. Call setup first.`,
            ),
            shutdown: false,
          };
        }

        runTeardown(stored.module, request.function, stored.context);
        setupContexts.delete(request.function);

        return {
          response: {
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "teardown_ack",
          },
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
      if (!fs.existsSync(request.file)) {
        return {
          response: errorResponse(request.id, "file_not_found", `File not found: ${request.file}`),
          shutdown: false,
        };
      }

      try {
        const generatorModule = loadGeneratorModule(request.file);
        const value = runGenerator(generatorModule, request.name, request.kind);

        return {
          response: {
            protocol_version: PROTOCOL_VERSION,
            id: request.id,
            status: "generate",
            value,
          },
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
      return file;
    }
  }

  // Fall back to last analyzed file
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
