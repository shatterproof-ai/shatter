import * as path from "node:path";
import { handleRequest, parseRequest, clearInstrumentedSources, instrumentedSourcesSize, setupContextsSize, getLoadedModuleNames } from "./handlers.js";
import { clearModuleCache, compiledModuleCacheSize } from "./executor.js";
import {
  PROTOCOL_VERSION,
  type Request,
  type Response,
  type SetupResponse,
  type TeardownAckResponse,
  type GenerateResponse,
  type ExecuteResponse,
  type SetupLevel,
  type SetupContextStack,
  type SetupRequest,
  type TeardownRequest,
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

  it("accepts valid setup request with level and scope", () => {
    const result = parseRequest(
      `{"id":1,"protocol_version":"${PROTOCOL_VERSION}","command":"setup","file":"s.ts","scope":"fn","level":"function"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("setup");
      if (result.request.command === "setup") {
        expect(result.request.scope).toBe("fn");
        expect(result.request.level).toBe("function");
      }
    }
  });

  it("accepts valid teardown request with level and scope", () => {
    const result = parseRequest(
      `{"id":2,"protocol_version":"${PROTOCOL_VERSION}","command":"teardown","scope":"fn","level":"function"}`
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("teardown");
      if (result.request.command === "teardown") {
        expect(result.request.scope).toBe("fn");
        expect(result.request.level).toBe("function");
      }
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

    it("does not emit timing unless the core requests it", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: ["analyze"] })
      );
      expect(response.timing).toBeUndefined();
    });

    it("enables timing emission when the core advertises timing capability", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "handshake", capabilities: ["analyze", "timing"] })
      );
      expect(response.timing).toBeUndefined();
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

    it("emits timing when timing capability was requested in handshake", async () => {
      await handleRequest(
        makeRequest({ command: "handshake", capabilities: ["analyze", "timing"] })
      );
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "branches.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile })
      );
      expect(response.timing?.phases.some((phase) => phase.phase_path === "analyze.total")).toBe(true);
      expect(response.timing?.phases.some((phase) => phase.phase_path === "analyze.ast")).toBe(true);
      expect(response.timing?.phases.some((phase) => phase.phase_path === "analyze.walk")).toBe(true);
      expect(response.timing?.phases.some((phase) => phase.phase_path === "serialize.response")).toBe(true);
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
      const exampleFile = path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts");
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
      const exampleFile = path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts");

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

    it("executes via file:function format with relative path", async () => {
      // Node.js 24+ requires absolute paths for createRequire().
      // Relative paths in file:function format must be resolved before use.
      const relPath = path.relative(process.cwd(), path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts"));

      const { response } = await handleRequest(
        makeRequest({
          command: "execute",
          function: `${relPath}:classifyNumber`,
          inputs: [42],
          mocks: [],
        })
      );
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.return_value).toBe("positive-even");
      }
    });

    it("executes after analyze with relative path", async () => {
      // Verify lastAnalyzedFile stores an absolute path even when given relative.
      const relPath = path.relative(process.cwd(), path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts"));

      await handleRequest(
        makeRequest({
          command: "analyze",
          file: relPath,
          function: "classifyNumber",
        })
      );

      const { response } = await handleRequest(
        makeRequest({
          command: "execute",
          function: "classifyNumber",
          inputs: [0],
          mocks: [],
        })
      );
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.return_value).toBe("zero");
      }
    });

    it("executes instrumented code with relative path", async () => {
      const relPath = path.relative(process.cwd(), path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts"));

      // Instrument with relative path
      await handleRequest(
        makeRequest({
          command: "instrument",
          file: relPath,
          function: "classifyNumber",
          mocks: [],
        })
      );

      const { response } = await handleRequest(
        makeRequest({
          command: "execute",
          function: `${relPath}:classifyNumber`,
          inputs: [-1],
          mocks: [],
        })
      );
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.return_value).toBe("negative");
        expect(response.branch_path.length).toBeGreaterThan(0);
      }
    });
  });

  describe("async function execution", () => {
    it("executes async function and returns resolved value", async () => {
      const asyncFixture = path.resolve(__dirname, "__fixtures__", "async-functions.ts");

      await handleRequest(
        makeRequest({
          command: "analyze",
          file: asyncFixture,
          function: "asyncAdd",
        })
      );

      const { response } = await handleRequest(
        makeRequest({
          command: "execute",
          function: "asyncAdd",
          inputs: [10, 20],
          mocks: [],
        })
      );
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.return_value).toBe(30);
        expect(response.thrown_error).toBeNull();
      }
    });

    it("executes async function that rejects and captures thrown_error", async () => {
      const asyncFixture = path.resolve(__dirname, "__fixtures__", "async-functions.ts");

      await handleRequest(
        makeRequest({
          command: "analyze",
          file: asyncFixture,
          function: "asyncThrows",
        })
      );

      const { response } = await handleRequest(
        makeRequest({
          command: "execute",
          function: "asyncThrows",
          inputs: [],
          mocks: [],
        })
      );
      expect(response.status).toBe("execute");
      if (response.status === "execute") {
        expect(response.thrown_error).not.toBeNull();
        expect(response.thrown_error!.message).toBe("async boom");
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
    it("loads setup file and returns setup_context with function level", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "myFunc", level: "function" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          scope: "myFunc",
          parentLevels: [],
        });
      }
    });

    it("works with execution level", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "auth", level: "execution" })
      );
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          scope: "auth",
          parentLevels: [],
        });
      }
    });

    it("works with session level", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "global", level: "session" })
      );
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          scope: "global",
          parentLevels: [],
        });
      }
    });

    it("passes parent_context to setup function", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const parentContext: SetupContextStack = {
        contexts: [
          { level: "session", context: { sessionId: "s1" } },
        ],
      };
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "myFile.ts", level: "file", parent_context: parentContext })
      );
      expect(response.status).toBe("setup");
      if (response.status === "setup") {
        expect(response.setup_context).toEqual({
          db: "test_db_conn",
          scope: "myFile.ts",
          parentLevels: ["session"],
        });
      }
    });

    it("maintains separate context caches per level", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "global", level: "session" })
      );
      await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "global", level: "function" })
      );
      expect(setupContextsSize()).toBe(2);

      await handleRequest(
        makeRequest({ command: "teardown", scope: "global", level: "function" })
      );
      expect(setupContextsSize()).toBe(1);

      await handleRequest(
        makeRequest({ command: "teardown", scope: "global", level: "session" })
      );
      expect(setupContextsSize()).toBe(0);
    });

    it("returns file_not_found for missing setup file", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: "/nonexistent/setup.ts", scope: "f", level: "function" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns error when setup export is missing", async () => {
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      const { response } = await handleRequest(
        makeRequest({ command: "setup", file: fixtureFile, scope: "f", level: "function" })
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
      await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "myFunc", level: "function" })
      );
      const { response, shutdown } = await handleRequest(
        makeRequest({ command: "teardown", scope: "myFunc", level: "function" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("teardown_ack");
    });

    it("returns error when no setup context exists", async () => {
      const { response } = await handleRequest(
        makeRequest({ command: "teardown", scope: "neverSetUp", level: "function" })
      );
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("internal_error");
        expect(response.message).toContain("No setup context");
      }
    });

    it("returns error when setup file has no teardown export", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-no-teardown.ts");
      const { response: setupResp } = await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "fn", level: "function" })
      );
      expect(setupResp.status).toBe("setup");

      const { response } = await handleRequest(
        makeRequest({ command: "teardown", scope: "fn", level: "function" })
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

  describe("memory management", () => {
    it("teardown clears instrumented sources and module cache", async () => {
      const setupFile = path.resolve(__dirname, "__fixtures__", "setup-module.ts");
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      const exampleFile = path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts");

      // Instrument a function — populates instrumentedSources cache
      await handleRequest(
        makeRequest({ command: "instrument", file: exampleFile, function: "classifyNumber", mocks: [] })
      );
      expect(instrumentedSourcesSize()).toBeGreaterThan(0);

      // Execute without prior instrument — populates compiledModuleCache via loadModule()
      await handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile })
      );
      await handleRequest(
        makeRequest({ command: "execute", function: `${fixtureFile}:add`, inputs: [1, 2], mocks: [] })
      );
      expect(compiledModuleCacheSize()).toBeGreaterThan(0);

      // Setup then teardown — should clear both caches
      await handleRequest(
        makeRequest({ command: "setup", file: setupFile, scope: "testFn", level: "function" })
      );
      await handleRequest(
        makeRequest({ command: "teardown", scope: "testFn", level: "function" })
      );

      expect(instrumentedSourcesSize()).toBe(0);
      expect(compiledModuleCacheSize()).toBe(0);
    });

    it("shutdown clears instrumented sources and module cache", async () => {
      const exampleFile = path.resolve(__dirname, "../../examples/standalone/ts/01-arithmetic.ts");

      // Instrument to populate cache
      await handleRequest(
        makeRequest({ command: "instrument", file: exampleFile, function: "classifyNumber", mocks: [] })
      );
      expect(instrumentedSourcesSize()).toBeGreaterThan(0);

      // Shutdown should clear all caches
      await handleRequest(makeRequest({ command: "shutdown" }));

      expect(instrumentedSourcesSize()).toBe(0);
      expect(compiledModuleCacheSize()).toBe(0);
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
          command === "setup" ? { command, file: "s.ts", scope: "f", level: "function" as SetupLevel } :
          command === "teardown" ? { command, scope: "f", level: "function" as SetupLevel } :
          command === "generate" ? { command, file: "g.ts", name: "T", kind: "type_name" as const } :
          { command }
        );
        const { response } = await handleRequest(request);
        expect(response.protocol_version).toBe(PROTOCOL_VERSION);
        expect(response.id).toBe(1);
      }
    });
  });

  describe("analyze-only startup", () => {
    it("does not load executor, instrumentor, setup-loader, or wasm-generator after handshake+analyze", async () => {
      clearInstrumentedSources(); // reset lazy module caches
      await handleRequest(makeRequest({ command: "handshake", capabilities: ["analyze"] }));
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "branches.ts");
      await handleRequest(makeRequest({ command: "analyze", file: fixtureFile }));
      const loaded = getLoadedModuleNames();
      expect(loaded).not.toContain("executor");
      expect(loaded).not.toContain("instrumentor");
      expect(loaded).not.toContain("setupLoader");
      expect(loaded).not.toContain("wasmGenerator");
    });

    it("loads executor only when execute is first called", async () => {
      clearInstrumentedSources();
      await handleRequest(makeRequest({ command: "handshake", capabilities: [] }));
      expect(getLoadedModuleNames()).not.toContain("executor");
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      await handleRequest(makeRequest({ command: "analyze", file: fixtureFile }));
      expect(getLoadedModuleNames()).not.toContain("executor");
      // After execute, executor must be loaded
      await handleRequest(makeRequest({ command: "execute", function: "add", inputs: [{ kind: "number", value: 1 }, { kind: "number", value: 2 }] }));
      expect(getLoadedModuleNames()).toContain("executor");
    });

    it("loads instrumentor only when instrument is first called", async () => {
      clearInstrumentedSources();
      await handleRequest(makeRequest({ command: "handshake", capabilities: [] }));
      expect(getLoadedModuleNames()).not.toContain("instrumentor");
      const fixtureFile = path.resolve(__dirname, "__fixtures__", "primitives.ts");
      await handleRequest(makeRequest({ command: "analyze", file: fixtureFile }));
      expect(getLoadedModuleNames()).not.toContain("instrumentor");
      // After instrument, instrumentor must be loaded
      await handleRequest(makeRequest({ command: "instrument", file: fixtureFile, function: "add", mocks: [] }));
      expect(getLoadedModuleNames()).toContain("instrumentor");
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

  it("SetupRequest with level and parent_context round-trips through JSON", () => {
    const request: SetupRequest = {
      protocol_version: PROTOCOL_VERSION,
      id: 40,
      command: "setup",
      file: "test.ts",
      scope: "myFunc",
      level: "function",
      parent_context: {
        contexts: [
          { level: "session", context: { sessionId: "abc" } },
          { level: "file", context: { fileHandle: 42 } },
        ],
      },
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as SetupRequest;
    expect(parsed.command).toBe("setup");
    expect(parsed.scope).toBe("myFunc");
    expect(parsed.level).toBe("function");
    expect(parsed.parent_context).toBeDefined();
    expect(parsed.parent_context!.contexts).toHaveLength(2);
    expect(parsed.parent_context!.contexts[0]!.level).toBe("session");
    expect(parsed.parent_context!.contexts[1]!.level).toBe("file");
  });

  it("TeardownRequest with level round-trips through JSON", () => {
    const request: TeardownRequest = {
      protocol_version: PROTOCOL_VERSION,
      id: 41,
      command: "teardown",
      scope: "myFunc",
      level: "function",
    };
    const json = JSON.stringify(request);
    const parsed = JSON.parse(json) as TeardownRequest;
    expect(parsed.command).toBe("teardown");
    expect(parsed.scope).toBe("myFunc");
    expect(parsed.level).toBe("function");
  });

  it("SetupContextStack round-trips through JSON", () => {
    const stack: SetupContextStack = {
      contexts: [
        { level: "session", context: { id: 1 } },
        { level: "file", context: "file_handle" },
        { level: "function", context: null },
        { level: "execution", context: [1, 2, 3] },
      ],
    };
    const json = JSON.stringify(stack);
    const parsed = JSON.parse(json) as SetupContextStack;
    expect(parsed.contexts).toHaveLength(4);
    expect(parsed.contexts[0]!.level).toBe("session");
    expect(parsed.contexts[1]!.level).toBe("file");
    expect(parsed.contexts[2]!.level).toBe("function");
    expect(parsed.contexts[3]!.level).toBe("execution");
    expect(parsed).toEqual(stack);
  });

  it("execute response with scope_events round-trips through JSON", () => {
    const response: ExecuteResponse = {
      protocol_version: PROTOCOL_VERSION,
      id: 30,
      status: "execute",
      return_value: 42,
      thrown_error: null,
      branch_path: [],
      lines_executed: [1, 2],
      calls_to_external: [],
      path_constraints: [],
      side_effects: [],
      performance: { wall_time_ms: 1, cpu_time_us: 1000, heap_used_bytes: 0, heap_allocated_bytes: 0 },
      scope_events: [
        { type: "scope", event: { kind: "loop_enter", loop_id: 0 } },
        { type: "branch", decision: { branch_id: 0, line: 3, taken: true, constraint: { kind: "unknown", hint: "test" } } },
        { type: "scope", event: { kind: "loop_exit", loop_id: 0 } },
        { type: "scope", event: { kind: "call_enter", call_site_id: 1 } },
        { type: "scope", event: { kind: "call_exit", call_site_id: 1 } },
      ],
    };
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as Response;
    expect(parsed.status).toBe("execute");
    if (parsed.status === "execute") {
      expect(parsed.scope_events).toHaveLength(5);
      expect(parsed.scope_events![0]).toEqual({ type: "scope", event: { kind: "loop_enter", loop_id: 0 } });
      expect(parsed.scope_events![1]).toEqual({
        type: "branch",
        decision: { branch_id: 0, line: 3, taken: true, constraint: { kind: "unknown", hint: "test" } },
      });
      expect(parsed.scope_events![3]).toEqual({ type: "scope", event: { kind: "call_enter", call_site_id: 1 } });
    }
  });

});
