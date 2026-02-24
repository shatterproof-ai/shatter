/**
 * Command handlers for the Shatter protocol.
 *
 * Each handler receives a typed request and returns a typed response.
 * Currently, handshake and shutdown are fully implemented. The analyze,
 * instrument, and execute commands return stub responses (these will be
 * implemented in subsequent issues).
 */

import {
  PROTOCOL_VERSION,
  FRONTEND_LANGUAGE,
  type Request,
  type Response,
  type ErrorResponse,
} from "./protocol.js";

/** Supported capabilities for this frontend. */
const SUPPORTED_CAPABILITIES = ["analyze", "execute", "instrument"];

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

    case "analyze":
      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "analyze",
          functions: [],
        },
        shutdown: false,
      };

    case "instrument":
      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "instrument",
          instrumented: false,
          output_file: null,
        },
        shutdown: false,
      };

    case "execute":
      return {
        response: {
          protocol_version: PROTOCOL_VERSION,
          id: request.id,
          status: "execute",
          return_value: null,
          thrown_error: null,
          branch_path: [],
          lines_executed: [],
          calls_to_external: [],
          path_constraints: [],
          side_effects: [],
          performance: {
            wall_time_ms: 0,
            cpu_time_us: 0,
            heap_used_bytes: 0,
            heap_allocated_bytes: 0,
          },
        },
        shutdown: false,
      };

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

  const validCommands = ["handshake", "analyze", "instrument", "execute", "shutdown"];
  if (!validCommands.includes(obj["command"] as string)) {
    return {
      error: errorResponse(id, "invalid_request", `Unknown command: ${String(obj["command"])}`),
    };
  }

  return { request: parsed as Request };
}
