import * as path from "node:path";
import { handleRequest, parseRequest, clearInstrumentedSources } from "./handlers.js";
import { clearModuleCache } from "./executor.js";
import {
  PROTOCOL_VERSION,
  type Request,
  type Response,
  type SetupResponse,
  type TeardownAckResponse,
  type GenerateResponse,
} from "./protocol.js";

describe("parseRequest", () => {
  it("rejects non-JSON input", () => {
    const result = parseRequest("not json");
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
    }
  });

  it("rejects non-object JSON", () => {
    const result = parseRequest('"hello"');
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
    }
  });

  it("rejects request without id", () => {
    const result = parseRequest(`{"command":"handshake","protocol_version":"${PROTOCOL_VERSION}"}`);
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
      expect(result.error.message).toContain("id");
    }
  });

  it("rejects request without protocol_version", () => {
    const result = parseRequest('{"id":1,"command":"handshake"}');
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
    }
  });

  it("rejects request without command", () => {
    const result = parseRequest(`{"id":1,"protocol_version":"${PROTOCOL_VERSION}"}`);
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
    }
  });

  it("rejects unknown command", () => {
    const result = parseRequest(`{"id":1,"protocol_version":"${PROTOCOL_VERSION}","command":"bogus"}`);
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
      expect(result.error.message).toContain("bogus");
    }
  });

  it("accepts valid handshake request", () => {
    const result = parseRequest(
      `{"id":1,"protocol_version":"${PROTOCOL_VERSION}","command":"handshake","capabilities":["analyze"]}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("handshake");
      expect(result.request.id).toBe(1);
    }
  });

  it("accepts valid shutdown request", () => {
    const result = parseRequest(
      `{"id":5,"protocol_version":"${PROTOCOL_VERSION}","command":"shutdown"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("shutdown");
    }
  });
});

describe("handleRequest", () => {
  beforeEach(() => {
    clearInstrumentedSources();
    clearModuleCache();
  });

  function makeRequest(overrides: Partial<Request> & { command: Request["command"] }): Request {
    return {
      protocol_version: PROTOCOL_VERSION,
      id: 1,
      ...overrides,
    } as Request;
  }

  describe("handshake", () => {
    it("responds with frontend version and capabilities", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: ["analyze", "execute"] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("handshake");
      expect(response.id).toBe(1);
      expect(response.protocol_version).toBe(PROTOCOL_VERSION);
      if (response.status === "handshake") {
        expect(response.frontend_version).toBe(PROTOCOL_VERSION);
        expect(response.language).toBe("typescript");
        expect(response.capabilities).toContain("analyze");
        expect(response.capabilities).toContain("execute");
        expect(response.capabilities).toContain("instrument");
      }
    });
  });

  describe("version mismatch", () => {
    it("returns error for incompatible protocol version", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: [], protocol_version: "1.0.0" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("version_mismatch");
      }
    });

    it("accepts matching major.minor with different patch", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: [], protocol_version: "0.1.99" })
      );
      expect(response.status).toBe("handshake");
    });
  });

  describe("analyze", () => {
    it("returns file_not_found error for missing file", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "analyze", file: "nonexistent.ts" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns function_not_found error for missing function in existing file", async () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile, function: "nonexistent" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("function_not_found");
      }
    });

    it("returns function analysis for existing file and function", async () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile, function: "add" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("analyze");
      if (response.status === "analyze") {
        expect(response.functions).toHaveLength(1);
        expect(response.functions[0]!.name).toBe("add");
        expect(response.functions[0]!.params).toHaveLength(2);
      }
    });

    it("returns all functions when no function name specified", async () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile })
      );
      expect(response.status).toBe("analyze");
      if (response.status === "analyze") {
        expect(response.functions.length).toBeGreaterThan(1);
      }
    });
  });

  describe("instrument", () => {
    it("returns file_not_found error for missing file", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "instrument", file: "nonexistent.ts", function: "foo", mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns instrumentation_failed for missing function", async () => {
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "instrument", file: fixtureFile, function: "nonexistent", mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("instrumentation_failed");
      }
    });

    it("instruments a real function successfully", async () => {
      const exampleFile = path.resolve(__dirname, "../../examples/typescript/src/01-arithmetic.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "instrument", file: exampleFile, function: "classifyNumber", mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("instrument");
      if (response.status === "instrument") {
        expect(response.instrumented).toBe(true);
        expect(response.output_file).toBeNull();
      }
    });
  });

  describe("execute", () => {
    it("returns error when function cannot be resolved", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "execute", function: "foo", inputs: [], mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
    });

    it("executes a real function after analyze", async () => {
      const exampleFile = path.resolve(__dirname, "../../examples/typescript/src/01-arithmetic.ts");

      // First analyze so the handler knows the file
      await handleRequest(
        makeRequest({
          command: "analyze",
          file: exampleFile,
          function: "classifyNumber",
        })
      );

      const { response, shutdown } = await handleRequest(
        makeRequest({
          command: "execute",
          function: "classifyNumber",
          inputs: [-5],
          mocks: [],
        })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.return_value).toBe("negative");
        expect(response.thrown_error).toBeNull();
        expect(response.branch_path).toEqual([]);
        expect(response.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
      }
    });
  });

  describe("shutdown", () => {
    it("returns shutdown_ack and signals shutdown", async () => {
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "shutdown" })
      );
      expect(shutdown).toBe(true);
      expect(response.status).toBe("shutdown_ack");
      expect(response.id).toBe(1);
    });
  });

  describe("setup", () => {
    it("loads setup file and returns setup_context", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, function: "myFunc", mode: "per_function" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          functionName: "myFunc",
          mode: "per_function",
        });
      }
    });

    it("works with per_execution mode", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, function: "auth", mode: "per_execution" })
      );
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          functionName: "auth",
          mode: "per_execution",
        });
      }
    });

    it("returns file_not_found for missing setup file", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: "/nonexistent/setup.ts", function: "f", mode: "per_function" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns error when setup export is missing", async () => {
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: fixtureFile, function: "f", mode: "per_function" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("internal_error");
        expect(response.message).toContain("setup()");
      }
    });
  });

  describe("teardown", () => {
    it("tears down after a successful setup", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      // First do setup
      await handleRequest(
        makeRequest({ command: "setup", file: setupFile, function: "myFunc", mode: "per_function" })
      );
      // Then teardown
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "teardown", function: "myFunc" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("teardown_ack");
    });

    it("returns error when no setup context exists", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "teardown", function: "neverSetUp" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("internal_error");
        expect(response.message).toContain("No setup context");
      }
    });

    it("returns error when setup file has no teardown export", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-no-teardown.ts");
      // Setup succeeds
      const { response: setupResp } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, function: "fn", mode: "per_function" })
      );
      expect(setupResp.status).toBe("setup");

      // Teardown fails because no teardown() export
      const { response } = await handleRequest(
        makeRequest({ command: "teardown", function: "fn" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("internal_error");
        expect(response.message).toContain("teardown()");
      }
    });
  });

  describe("generate", () => {
    it("generates a value for type_name kind", async () => {
      const genFile = path.resolve(__dirname, "__fixtures__", "generator-module.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "generate", file: genFile, name: "User", kind: "type_name" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("generate");
      if (response.status === "generate") {
        expect(response.value).toEqual({ id: 1, name: "Alice", email: "alice@example.com" });
        expect(response.generator_id).toBe("generated");
      }
    });

    it("generates a value for param_name kind", async () => {
      const genFile = path.resolve(__dirname, "__fixtures__", "generator-module.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "generate", file: genFile, name: "authToken", kind: "param_name" })
      );
      expect(response.status).toBe("generate");
      if (response.status === "generate") {
        expect(response.value).toBe("tok_test_abc123");
        expect(response.generator_id).toBe("generated");
      }
    });

    it("generates a numeric value", async () => {
      const genFile = path.resolve(__dirname, "__fixtures__", "generator-module.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "generate", file: genFile, name: "count", kind: "param_name" })
      );
      expect(response.status).toBe("generate");
      if (response.status === "generate") {
        expect(response.value).toBe(42);
        expect(response.generator_id).toBe("generated");
      }
    });

    it("returns file_not_found for missing generator file", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "generate", file: "/nonexistent/gen.ts", name: "T", kind: "type_name" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns error when generator export is missing", async () => {
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "generate", file: fixtureFile, name: "NonExistent", kind: "type_name" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("internal_error");
        expect(response.message).toContain("NonExistent");
      }
    });
  });

  describe("capabilities", () => {
    it("includes setup and generate in capabilities", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: [] })
      );
      if (response.status === "handshake") {
        expect(response.capabilities).toContain("setup");
        expect(response.capabilities).toContain("generate");
      }
    });
  });

  describe("response format conformance", () => {
    it("all responses include protocol_version and id", async () => {
      const commands: Request["command"][] = [
        "handshake", "analyze", "instrument", "execute",
        "setup", "teardown", "generate", "shutdown",
      ];

      for (const command of commands) {
        const request = makeRequest(
          command === "handshake" ? { command, capabilities: [] } :
          command === "analyze" ? { command, file: "t.ts" } :
          command === "instrument" ? { command, file: "t.ts", function: "f", mocks: [] } :
          command === "execute" ? { command, function: "f", inputs: [], mocks: [] } :
          command === "setup" ? { command, file: "s.ts", function: "f", mode: "per_function" as const } :
          command === "teardown" ? { command, function: "f" } :
          command === "generate" ? { command, file: "g.ts", name: "T", kind: "type_name" as const } :
          { command }
        );
        const { response } = await handleRequest(request);
        expect(response.protocol_version).toBe(PROTOCOL_VERSION);
        expect(response.id).toBe(1);
      }
    });
  });
});

describe("protocol round-trip", () => {
  it("handshake response deserializes from noop-frontend format", () => {
    const json = `{"protocol_version":"${PROTOCOL_VERSION}","id":1,"status":"handshake","frontend_version":"${PROTOCOL_VERSION}","language":"typescript","capabilities":["analyze","execute","instrument"]}`;
    const response: Response = JSON.parse(json) as Response;
    expect(response.status).toBe("handshake");
    expect(response.id).toBe(1);
    if (response.status === "handshake") {
      expect(response.language).toBe("typescript");
    }
  });

  it("handshake response serializes to valid JSON", async () => {
    const { response } = await handleRequest({
      protocol_version: PROTOCOL_VERSION,
      id: 42,
      command: "handshake",
      capabilities: ["analyze"],
    });
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.protocol_version).toBe(PROTOCOL_VERSION);
    expect(parsed.id).toBe(42);
    expect(parsed.status).toBe("handshake");
  });

  it("error response matches protocol schema", async () => {
    const { response } = await handleRequest({
      protocol_version: "9.9.9",
      id: 10,
      command: "handshake",
      capabilities: [],
    });
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("error");
    if (parsed.status === "error") {
      expect(parsed.code).toBe("version_mismatch");
      expect(typeof parsed.message).toBe("string");
    }
  });

  it("shutdown_ack response matches protocol schema", async () => {
    const { response } = await handleRequest({
      protocol_version: PROTOCOL_VERSION,
      id: 99,
      command: "shutdown",
    });
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("shutdown_ack");
    expect(parsed.id).toBe(99);
    expect(parsed.protocol_version).toBe(PROTOCOL_VERSION);
  });

  it("setup response round-trips through JSON", () => {
    const response: SetupResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 20,
      status: "setup",
      setup_context: { db_handle: "conn_42", temp_dir: "/tmp/test" },
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("setup");
    expect(parsed.id).toBe(20);
    if (parsed.status === "setup") {
      expect(parsed.setup_context).toEqual({ db_handle: "conn_42", temp_dir: "/tmp/test" });
    }
  });

  it("teardown_ack response round-trips through JSON", () => {
    const response: TeardownAckResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 21,
      status: "teardown_ack",
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("teardown_ack");
    expect(parsed.id).toBe(21);
    expect(parsed.protocol_version).toBe(PROTOCOL_VERSION);
  });

  it("generate response round-trips through JSON", () => {
    const response: GenerateResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 22,
      status: "generate",
      value: { id: 1, name: "Alice", email: "alice@example.com" },
      generator_id: "generated",
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("generate");
    expect(parsed.id).toBe(22);
    if (parsed.status === "generate") {
      expect(parsed.value).toEqual({ id: 1, name: "Alice", email: "alice@example.com" });
      expect(parsed.generator_id).toBe("generated");
    }
  });

  it("generate response with primitive value round-trips", () => {
    const response: GenerateResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 23,
      status: "generate",
      value: "tok_abc123",
      generator_id: "generated",
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("generate");
    if (parsed.status === "generate") {
      expect(parsed.value).toBe("tok_abc123");
    }
  });

  it("generate response with recipe round-trips through JSON", () => {
    const response: GenerateResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 24,
      status: "generate",
      value: { name: "Bob" },
      generator_id: "wasm-user-gen",
      recipe: { seed: 42, variant: "admin" },
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("generate");
    if (parsed.status === "generate") {
      expect(parsed.generator_id).toBe("wasm-user-gen");
      expect(parsed.recipe).toEqual({ seed: 42, variant: "admin" });
    }
  });

  it("setup request round-trips through JSON", () => {
    const request: Request = {
      protocol_version: PROTOCOL_VERSION,
      id: 30,
      command: "setup",
      file: "./setup/global.ts",
      function: "processOrder",
      mode: "per_function",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as Request;
    expect(parsed.command).toBe("setup");
    expect(parsed.id).toBe(30);
    if (parsed.command === "setup") {
      expect(parsed.file).toBe("./setup/global.ts");
      expect(parsed.function).toBe("processOrder");
      expect(parsed.mode).toBe("per_function");
    }
  });

  it("setup request with per_execution mode round-trips", () => {
    const request: Request = {
      protocol_version: PROTOCOL_VERSION,
      id: 31,
      command: "setup",
      file: "./setup/auth.ts",
      function: "authenticate",
      mode: "per_execution",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as Request;
    if (parsed.command === "setup") {
      expect(parsed.mode).toBe("per_execution");
    }
  });

  it("teardown request round-trips through JSON", () => {
    const request: Request = {
      protocol_version: PROTOCOL_VERSION,
      id: 32,
      command: "teardown",
      function: "processOrder",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as Request;
    expect(parsed.command).toBe("teardown");
    if (parsed.command === "teardown") {
      expect(parsed.function).toBe("processOrder");
    }
  });

  it("generate request with type_name round-trips through JSON", () => {
    const request: Request = {
      protocol_version: PROTOCOL_VERSION,
      id: 33,
      command: "generate",
      file: "./generators/user.ts",
      name: "User",
      kind: "type_name",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as Request;
    expect(parsed.command).toBe("generate");
    if (parsed.command === "generate") {
      expect(parsed.file).toBe("./generators/user.ts");
      expect(parsed.name).toBe("User");
      expect(parsed.kind).toBe("type_name");
    }
  });

  it("generate request with param_name round-trips through JSON", () => {
    const request: Request = {
      protocol_version: PROTOCOL_VERSION,
      id: 34,
      command: "generate",
      file: "./generators/token.ts",
      name: "authToken",
      kind: "param_name",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as Request;
    if (parsed.command === "generate") {
      expect(parsed.kind).toBe("param_name");
    }
  });
});

describe("parseRequest with new commands", () => {
  it("accepts valid setup request", () => {
    const result = parseRequest(
      `{"id":1,"protocol_version":"${PROTOCOL_VERSION}","command":"setup","file":"s.ts","function":"fn","mode":"per_function"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("setup");
    }
  });

  it("accepts valid teardown request", () => {
    const result = parseRequest(
      `{"id":2,"protocol_version":"${PROTOCOL_VERSION}","command":"teardown","function":"fn"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("teardown");
    }
  });

  it("accepts valid generate request", () => {
    const result = parseRequest(
      `{"id":3,"protocol_version":"${PROTOCOL_VERSION}","command":"generate","file":"g.ts","name":"User","kind":"type_name"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("generate");
    }
  });
});
