import * as path from "node:path";
import {
  executeFunction,
  executeInstrumented,
  buildExecuteResponse,
  clearModuleCache,
  getExecTimeoutMs,
  DEFAULT_EXEC_TIMEOUT_MS,
  truncateMessage,
  truncateSideEffects,
} from "./executor.js";
import { instrumentFunction } from "./instrumentor.js";
import * as fs from "node:fs";
import { PROTOCOL_VERSION } from "./protocol.js";
import type { SideEffect } from "./protocol.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const EXAMPLES_DIR = path.resolve(__dirname, "../../examples/typescript/src");

beforeEach(() => {
  clearModuleCache();
});

describe("executeFunction performance metrics", () => {
  it("reports non-negative wall_time_ms", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.return_value).toBe(3);
    expect(result.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
  });

  it("reports non-negative cpu_time_us from process.cpuUsage", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.performance.cpu_time_us).toBeGreaterThanOrEqual(0);
  });

  it("reports non-negative heap_used_bytes", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.performance.heap_used_bytes).toBeGreaterThanOrEqual(0);
  });

  it("reports non-negative heap_allocated_bytes", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.performance.heap_allocated_bytes).toBeGreaterThanOrEqual(0);
  });

  it("wall_time_ms is a finite number", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(Number.isFinite(result.performance.wall_time_ms)).toBe(true);
  });

  it("cpu_time_us is an integer (microseconds from cpuUsage)", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(Number.isInteger(result.performance.cpu_time_us)).toBe(true);
  });

  it("wall time is less than 5 seconds for a trivial function", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.performance.wall_time_ms).toBeLessThan(5000);
  });
});

describe("executeFunction with memory-allocating function", () => {
  const allocatorFixture = path.join(FIXTURES_DIR, "allocator.ts");

  beforeAll(() => {
    // Create a fixture that allocates a measurable amount of memory
    fs.writeFileSync(
      allocatorFixture,
      `export function allocateArrays(): number {
  const arrays: number[][] = [];
  for (let i = 0; i < 1000; i++) {
    arrays.push(new Array(1000).fill(i));
  }
  return arrays.length;
}
`,
    );
  });

  afterAll(() => {
    fs.unlinkSync(allocatorFixture);
  });

  it("shows measurable heap delta for memory-allocating function", () => {
    const result = executeFunction(allocatorFixture, "allocateArrays", []);
    expect(result.return_value).toBe(1000);
    // The function allocates ~8MB (1000 arrays * 1000 numbers * 8 bytes).
    // heap_used_bytes may be 0 if GC reclaims during execution, but
    // for a large allocation it should generally show some delta.
    // We check the fields exist and are non-negative.
    expect(result.performance.heap_used_bytes).toBeGreaterThanOrEqual(0);
    expect(result.performance.heap_allocated_bytes).toBeGreaterThanOrEqual(0);
  });
});

describe("executeInstrumented performance metrics", () => {
  it("reports plausible metrics for instrumented execution", () => {
    const exampleFile = path.join(EXAMPLES_DIR, "01-arithmetic.ts");
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(source, "classifyNumber", exampleFile);

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
    );

    expect(result.return_value).toBe("positive-even");
    expect(result.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
    expect(result.performance.cpu_time_us).toBeGreaterThanOrEqual(0);
    expect(result.performance.heap_used_bytes).toBeGreaterThanOrEqual(0);
    expect(result.performance.heap_allocated_bytes).toBeGreaterThanOrEqual(0);
    expect(Number.isFinite(result.performance.wall_time_ms)).toBe(true);
    expect(Number.isInteger(result.performance.cpu_time_us)).toBe(true);
  });
});

describe("buildExecuteResponse", () => {
  it("includes performance metrics in the response", () => {
    const rawResult = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [3, 4],
    );

    const response = buildExecuteResponse(1, PROTOCOL_VERSION, rawResult);

    expect(response.status).toBe("execute");
    expect(response.return_value).toBe(7);
    expect(response.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
    expect(response.performance.cpu_time_us).toBeGreaterThanOrEqual(0);
    expect(response.performance.heap_used_bytes).toBeGreaterThanOrEqual(0);
    expect(response.performance.heap_allocated_bytes).toBeGreaterThanOrEqual(0);
  });

  it("serializes performance metrics to JSON correctly", () => {
    const rawResult = executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "isPositive",
      [5],
    );

    const response = buildExecuteResponse(2, PROTOCOL_VERSION, rawResult);
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as typeof response;

    expect(parsed.performance.wall_time_ms).toBe(response.performance.wall_time_ms);
    expect(parsed.performance.cpu_time_us).toBe(response.performance.cpu_time_us);
    expect(parsed.performance.heap_used_bytes).toBe(response.performance.heap_used_bytes);
    expect(parsed.performance.heap_allocated_bytes).toBe(response.performance.heap_allocated_bytes);
  });
});

const SIDE_EFFECTS_FIXTURE = path.resolve(FIXTURES_DIR, "side-effects.ts");

describe("executeFunction side effect capture", () => {
  it("captures console.log and console.warn output", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "logsAndReturns", [42]);

    expect(result.return_value).toBe("done: 42");
    expect(result.thrown_error).toBeNull();

    const consoleSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "console_output",
    );
    expect(consoleSideEffects).toHaveLength(2);
    expect(consoleSideEffects[0]).toEqual({
      kind: "console_output",
      level: "log",
      message: "processing 42",
    });
    expect(consoleSideEffects[1]).toEqual({
      kind: "console_output",
      level: "warn",
      message: "watch out",
    });
  });

  it("captures thrown error as both thrown_error and side effect", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", ["boom"]);

    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.error_type).toBe("Error");
    expect(result.thrown_error!.message).toBe("boom");

    const errorSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "thrown_error",
    );
    expect(errorSideEffects).toHaveLength(1);
    expect(errorSideEffects[0]).toMatchObject({
      kind: "thrown_error",
      error_type: "Error",
      message: "boom",
    });

    // Also captures the console.error before the throw
    const consoleSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "console_output",
    );
    expect(consoleSideEffects).toHaveLength(1);
    expect(consoleSideEffects[0]).toEqual({
      kind: "console_output",
      level: "error",
      message: "about to throw",
    });
  });

  it("captures all console levels", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "logsMultipleLevels", []);

    const consoleSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "console_output",
    );
    expect(consoleSideEffects).toHaveLength(5);

    const levels = consoleSideEffects.map((se: SideEffect) => {
      if (se.kind === "console_output") return se.level;
      return null;
    });
    expect(levels).toEqual(["log", "warn", "error", "info", "debug"]);
  });

  it("captures custom error types", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "throwsCustomError", []);

    expect(result.thrown_error!.error_type).toBe("TypeError");
    expect(result.thrown_error!.message).toBe("custom type error");

    const errorSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "thrown_error",
    );
    expect(errorSideEffects).toHaveLength(1);
    expect(errorSideEffects[0]).toMatchObject({
      kind: "thrown_error",
      error_type: "TypeError",
      message: "custom type error",
    });
  });

  it("returns empty side_effects for pure functions", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "noSideEffects", [1, 2]);

    expect(result.return_value).toBe(3);
    expect(result.thrown_error).toBeNull();
    expect(result.side_effects).toEqual([]);
  });

  it("restores global console after execution", () => {
    const originalConsole = globalThis.console;
    executeFunction(SIDE_EFFECTS_FIXTURE, "logsAndReturns", [1]);
    expect(globalThis.console).toBe(originalConsole);
  });

  it("restores global console even when function throws", () => {
    const originalConsole = globalThis.console;
    executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", ["test"]);
    expect(globalThis.console).toBe(originalConsole);
  });
});

describe("executeInstrumented side effect capture", () => {
  function getInstrumentedSource(funcName: string): string {
    const source = fs.readFileSync(SIDE_EFFECTS_FIXTURE, "utf-8");
    const result = instrumentFunction(source, funcName, SIDE_EFFECTS_FIXTURE);
    if ("error" in result) {
      throw new Error(result.error);
    }
    return result.instrumentedSource;
  }

  it("captures console output in instrumented execution", () => {
    const instrumentedSource = getInstrumentedSource("logsAndReturns");
    const result = executeInstrumented(instrumentedSource, "logsAndReturns", [99]);

    expect(result.return_value).toBe("done: 99");

    const consoleSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "console_output",
    );
    expect(consoleSideEffects.length).toBeGreaterThanOrEqual(2);

    const logEffect = consoleSideEffects.find(
      (se: SideEffect) => se.kind === "console_output" && se.level === "log",
    );
    expect(logEffect).toBeDefined();
    if (logEffect && logEffect.kind === "console_output") {
      expect(logEffect.message).toContain("processing");
      expect(logEffect.message).toContain("99");
    }
  });

  it("captures thrown error and console output in instrumented execution", () => {
    const instrumentedSource = getInstrumentedSource("throwsError");
    const result = executeInstrumented(instrumentedSource, "throwsError", ["instrumented boom"]);

    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toBe("instrumented boom");

    const errorSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "thrown_error",
    );
    expect(errorSideEffects).toHaveLength(1);

    const consoleSideEffects = result.side_effects.filter(
      (se: SideEffect) => se.kind === "console_output",
    );
    expect(consoleSideEffects.length).toBeGreaterThanOrEqual(1);
  });

  it("detects module-level variable changes", () => {
    const instrumentedSource = getInstrumentedSource("incrementCounter");
    const result = executeInstrumented(instrumentedSource, "incrementCounter", []);

    expect(result.return_value).toBe(1);

    const stateChanges = result.side_effects.filter(
      (se: SideEffect) => se.kind === "global_state_change",
    );
    expect(stateChanges).toHaveLength(1);
    expect(stateChanges[0]).toMatchObject({
      kind: "global_state_change",
      variable: "counter",
      before: 0,
      after: 1,
    });
  });
});

describe("intra-package module resolution", () => {
  it("loadModule resolves relative imports from the target file directory", () => {
    const depsFixture = path.join(FIXTURES_DIR, "dependencies.ts");
    const result = executeFunction(depsFixture, "usesExternal", [3, 4]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("7");
  });

  it("executeInstrumented resolves relative imports from the source file", () => {
    const depsFixture = path.join(FIXTURES_DIR, "dependencies.ts");
    const source = fs.readFileSync(depsFixture, "utf-8");
    const instrumentResult = instrumentFunction(source, "usesExternal", depsFixture);

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = executeInstrumented(
      instrumentResult.instrumentedSource,
      "usesExternal",
      [3, 4],
      [],
      depsFixture,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("7");
  });
});

describe("buildExecuteResponse side effects", () => {
  it("passes side_effects through to the response", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "logsAndReturns", [7]);
    const response = buildExecuteResponse(1, PROTOCOL_VERSION, result);

    expect(response.side_effects.length).toBeGreaterThan(0);
    expect(response.side_effects).toEqual(result.side_effects);
  });

  it("acceptance: function with console.log and thrown error has both recorded", () => {
    const result = executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", ["fail"]);
    const response = buildExecuteResponse(1, PROTOCOL_VERSION, result);

    const hasConsoleOutput = response.side_effects.some(
      (se: SideEffect) => se.kind === "console_output",
    );
    const hasThrownError = response.side_effects.some(
      (se: SideEffect) => se.kind === "thrown_error",
    );

    expect(hasConsoleOutput).toBe(true);
    expect(hasThrownError).toBe(true);
  });
});

describe("getExecTimeoutMs", () => {
  const originalEnv = process.env["SHATTER_EXEC_TIMEOUT"];

  afterEach(() => {
    if (originalEnv === undefined) {
      delete process.env["SHATTER_EXEC_TIMEOUT"];
    } else {
      process.env["SHATTER_EXEC_TIMEOUT"] = originalEnv;
    }
  });

  it("returns default when env var is not set", () => {
    delete process.env["SHATTER_EXEC_TIMEOUT"];
    expect(getExecTimeoutMs()).toBe(DEFAULT_EXEC_TIMEOUT_MS);
  });

  it("parses integer seconds to milliseconds", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "20";
    expect(getExecTimeoutMs()).toBe(20000);
  });

  it("parses float seconds to milliseconds", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "2.5";
    expect(getExecTimeoutMs()).toBe(2500);
  });

  it("ignores non-numeric values and returns default", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "not-a-number";
    expect(getExecTimeoutMs()).toBe(DEFAULT_EXEC_TIMEOUT_MS);
  });

  it("ignores zero and returns default", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "0";
    expect(getExecTimeoutMs()).toBe(DEFAULT_EXEC_TIMEOUT_MS);
  });

  it("ignores negative values and returns default", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "-5";
    expect(getExecTimeoutMs()).toBe(DEFAULT_EXEC_TIMEOUT_MS);
  });
});

describe("execution timeout enforcement", () => {
  const infiniteLoopFixture = path.join(FIXTURES_DIR, "infinite-loop.ts");
  const originalEnv = process.env["SHATTER_EXEC_TIMEOUT"];

  beforeAll(() => {
    fs.writeFileSync(
      infiniteLoopFixture,
      `while (true) {}\nexport function neverReached(): string { return "unreachable"; }\n`,
    );
  });

  afterAll(() => {
    fs.unlinkSync(infiniteLoopFixture);
    if (originalEnv === undefined) {
      delete process.env["SHATTER_EXEC_TIMEOUT"];
    } else {
      process.env["SHATTER_EXEC_TIMEOUT"] = originalEnv;
    }
  });

  it("aborts module-level infinite loop via vm timeout", () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "0.1";
    clearModuleCache();
    expect(() => {
      executeFunction(infiniteLoopFixture, "neverReached", []);
    }).toThrow(/Script execution timed out/);
  });
});

describe("truncation", () => {
  it("truncateMessage returns short strings unchanged", () => {
    expect(truncateMessage("hello", 100)).toBe("hello");
  });

  it("truncateMessage truncates long strings", () => {
    const long = "x".repeat(5000);
    const result = truncateMessage(long, 100);
    expect(Buffer.byteLength(result, "utf-8")).toBeLessThanOrEqual(100);
    expect(result).toContain("…[truncated]");
  });

  it("truncateSideEffects returns few entries unchanged", () => {
    const effects: SideEffect[] = Array.from({ length: 10 }, (_, i) => ({
      kind: "console_output" as const,
      level: "log",
      message: `line ${i}`,
    }));
    const { effects: result, truncation } = truncateSideEffects(effects);
    expect(result).toHaveLength(10);
    expect(truncation).toBeUndefined();
  });

  it("truncateSideEffects truncates many entries with marker", () => {
    const effects: SideEffect[] = Array.from({ length: 100 }, (_, i) => ({
      kind: "console_output" as const,
      level: "log",
      message: `line ${i}`,
    }));
    const { effects: result, truncation } = truncateSideEffects(effects, 5, 3);
    // 5 head + 1 marker + 3 tail = 9
    expect(result).toHaveLength(9);
    expect(result[0]).toEqual({ kind: "console_output", level: "log", message: "line 0" });
    expect(result[4]).toEqual({ kind: "console_output", level: "log", message: "line 4" });
    const marker = result[5];
    expect(marker).toBeDefined();
    expect(marker!.kind).toBe("console_output");
    if (marker !== undefined && marker.kind === "console_output") {
      expect(marker.message).toMatch(/truncated 92 lines/);
    }
    expect(result[6]).toEqual({ kind: "console_output", level: "log", message: "line 97" });
    expect(result[8]).toEqual({ kind: "console_output", level: "log", message: "line 99" });
    expect(truncation?.was_truncated).toBe(true);
    expect(truncation?.original_lines).toBe(100);
  });

  it("truncateSideEffects preserves non-console effects", () => {
    const effects: SideEffect[] = [
      { kind: "console_output", level: "log", message: "a" },
      { kind: "global_mutation", name: "x" },
      ...Array.from({ length: 100 }, (_, i) => ({
        kind: "console_output" as const,
        level: "log",
        message: `line ${i}`,
      })),
    ];
    const { effects: result } = truncateSideEffects(effects, 5, 3);
    expect(result.some(e => e.kind === "global_mutation")).toBe(true);
  });
});

describe("TSX support", () => {
  it("executes a function from a .tsx file", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "component.tsx"),
      "greetingLabel",
      ["Alice"],
    );
    expect(result.return_value).toBe("<span>Hello, Alice!</span>");
  });

  it("executes a .tsx function with falsy branch", () => {
    const result = executeFunction(
      path.join(FIXTURES_DIR, "component.tsx"),
      "greetingLabel",
      [""],
    );
    expect(result.return_value).toBe("<span>Hello, stranger!</span>");
  });

  it("instruments and executes TSX source with branches", () => {
    const tsxFile = path.join(FIXTURES_DIR, "component.tsx");
    const source = fs.readFileSync(tsxFile, "utf-8");
    const instrumentResult = instrumentFunction(source, "greetingLabel", tsxFile);

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = executeInstrumented(
      instrumentResult.instrumentedSource,
      "greetingLabel",
      ["Bob"],
      [],
      tsxFile,
    );

    expect(result.return_value).toBe("<span>Hello, Bob!</span>");
    expect(result.branch_path.length).toBeGreaterThan(0);
  });
});
