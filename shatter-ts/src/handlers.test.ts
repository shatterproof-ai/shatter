import * as path from "node:path";
import { handleRequest, parseRequest } from "./handlers.js";
import { PROTOCOL_VERSION, type Request, type Response } from "./protocol.js";

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
    const result = parseRequest('{"command":"handshake","protocol_version":"0.1.0"}');
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
    const result = parseRequest('{"id":1,"protocol_version":"0.1.0"}');
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
    }
  });

  it("rejects unknown command", () => {
    const result = parseRequest('{"id":1,"protocol_version":"0.1.0","command":"bogus"}');
    expect("error" in result).toBe(true);
    if ("error" in result) {
      expect(result.error.code).toBe("invalid_request");
      expect(result.error.message).toContain("bogus");
    }
  });

  it("accepts valid handshake request", () => {
    const result = parseRequest(
      '{"id":1,"protocol_version":"0.1.0","command":"handshake","capabilities":["analyze"]}'
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("handshake");
      expect(result.request.id).toBe(1);
    }
  });

  it("accepts valid shutdown request", () => {
    const result = parseRequest(
      '{"id":5,"protocol_version":"0.1.0","command":"shutdown"}'
    );
    expect("request" in result).toBe(true);
    if ("request" in result) {
      expect(result.request.command).toBe("shutdown");
    }
  });
});

describe("handleRequest", () => {
  function makeRequest(overrides: Partial<Request> & { command: Request["command"] }): Request {
    return {
      protocol_version: PROTOCOL_VERSION,
      id: 1,
      ...overrides,
    } as Request;
  }

  describe("handshake", () => {
    it("responds with frontend version and capabilities", () => {
      const { response, shutdown } = handleRequest(
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
    it("returns error for incompatible protocol version", () => {
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "handshake", capabilities: [], protocol_version: "1.0.0" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("version_mismatch");
      }
    });

    it("accepts matching major.minor with different patch", () => {
      const { response } = handleRequest(
        makeRequest({ command: "handshake", capabilities: [], protocol_version: "0.1.99" })
      );
      expect(response.status).toBe("handshake");
    });
  });

  describe("analyze", () => {
    it("returns file_not_found error for missing file", () => {
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "analyze", file: "nonexistent.ts" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("file_not_found");
      }
    });

    it("returns function_not_found error for missing function in existing file", () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile, function: "nonexistent" })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
      if (response.status === "error") {
        expect(response.code).toBe("function_not_found");
      }
    });

    it("returns function analysis for existing file and function", () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response, shutdown } = handleRequest(
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

    it("returns all functions when no function name specified", () => {
      const fixtureFile = require("path").join(__dirname, "__fixtures__", "primitives.ts");
      const { response } = handleRequest(
        makeRequest({ command: "analyze", file: fixtureFile })
      );
      expect(response.status).toBe("analyze");
      if (response.status === "analyze") {
        expect(response.functions.length).toBeGreaterThan(1);
      }
    });
  });

  describe("instrument", () => {
    it("returns not-instrumented stub", () => {
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "instrument", file: "test.ts", function: "foo", mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("instrument");
      if (response.status === "instrument") {
        expect(response.instrumented).toBe(false);
        expect(response.output_file).toBeNull();
      }
    });
  });

  describe("execute", () => {
    it("returns error when function cannot be resolved", () => {
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "execute", function: "foo", inputs: [], mocks: [] })
      );
      expect(shutdown).toBe(false);
      expect(response.status).toBe("error");
    });

    it("executes a real function after analyze", () => {
      const exampleFile = path.resolve(__dirname, "../../examples/typescript/src/01-arithmetic.ts");

      // First analyze so the handler knows the file
      handleRequest(
        makeRequest({
          command: "analyze",
          file: exampleFile,
          function: "classifyNumber",
        })
      );

      const { response, shutdown } = handleRequest(
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
    it("returns shutdown_ack and signals shutdown", () => {
      const { response, shutdown } = handleRequest(
        makeRequest({ command: "shutdown" })
      );
      expect(shutdown).toBe(true);
      expect(response.status).toBe("shutdown_ack");
      expect(response.id).toBe(1);
    });
  });

  describe("response format conformance", () => {
    it("all responses include protocol_version and id", () => {
      const commands: Request["command"][] = ["handshake", "analyze", "instrument", "execute", "shutdown"];

      for (const command of commands) {
        const request = makeRequest(
          command === "handshake" ? { command, capabilities: [] } :
          command === "analyze" ? { command, file: "t.ts" } :
          command === "instrument" ? { command, file: "t.ts", function: "f", mocks: [] } :
          command === "execute" ? { command, function: "f", inputs: [], mocks: [] } :
          { command }
        );
        const { response } = handleRequest(request);
        expect(response.protocol_version).toBe(PROTOCOL_VERSION);
        expect(response.id).toBe(1);
      }
    });
  });
});

describe("protocol round-trip", () => {
  it("handshake response deserializes from noop-frontend format", () => {
    const json = '{"protocol_version":"0.1.0","id":1,"status":"handshake","frontend_version":"0.1.0","language":"typescript","capabilities":["analyze","execute","instrument"]}';
    const response: Response = JSON.parse(json) as Response;
    expect(response.status).toBe("handshake");
    expect(response.id).toBe(1);
    if (response.status === "handshake") {
      expect(response.language).toBe("typescript");
    }
  });

  it("handshake response serializes to valid JSON", () => {
    const { response } = handleRequest({
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

  it("error response matches protocol schema", () => {
    const { response } = handleRequest({
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

  it("shutdown_ack response matches protocol schema", () => {
    const { response } = handleRequest({
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
});
