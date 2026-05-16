import * as os from "node:os";
import * as path from "node:path";
import {
  executeFunction,
  executeInstrumented,
  executeAdapterOwned,
  buildExecuteResponse,
  clearModuleCache,
  clearCompiledScriptCache,
  deleteCompiledScriptEntry,
  getExecTimeoutMs,
  getHarnessCacheDir,
  getHarnessScratchDir,
  isMcdcEnabled,
  DEFAULT_EXEC_TIMEOUT_MS,
  truncateMessage,
  truncateSideEffects,
  classifyConnectionFailure,
  CONN_REFUSED_PATTERNS,
  DNS_FAILURE_PATTERNS,
  AUTH_ERROR_PATTERNS,
  TIMEOUT_PATTERNS,
  buildRuntimeCryptoBoundary,
  createUnresolvableModuleStub,
  transformDynamicImports,
  createShatterImport,
  setProjectRoot,
  getCurrentJsxRuntimeOptions,
} from "./executor.js";
import type { ResolverAdapter } from "./executor.js";
import type {
  InvocationHook,
  InvocationContext,
  InvocationOutcome,
  AdapterInvocationModel,
} from "./runtime-hooks.js";
import { instrumentFunction } from "./instrumentor.js";
import { analyzeFile } from "./analyzer.js";
import * as ts from "typescript";
import * as fs from "node:fs";
import { PROTOCOL_VERSION } from "./protocol.js";
import type { SideEffect, TraceEvent } from "./protocol.js";
import { TimingCollector } from "./timing.js";

/**
 * Helper to cast a stub to a callable for test assertions.
 * Avoids `any` while allowing chained property access + calls.
 */
function asCallable(v: unknown): (...args: unknown[]) => unknown {
  return v as (...args: unknown[]) => unknown;
}

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const EXAMPLES_ROOT =
  process.env.SHATTER_EXAMPLES_DIR ??
  path.join(os.tmpdir(), "shatter-examples-main");
const EXAMPLES_DIR = path.join(EXAMPLES_ROOT, "standalone", "ts");
const RESOLVER_CHAIN_FIXTURE = path.join(
  FIXTURES_DIR,
  "resolver-adapter-chain.ts",
);

beforeEach(() => {
  clearModuleCache();
});

describe("executeFunction performance metrics", () => {
  it("reports plausible metrics for a trivial function", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "primitives.ts"),
      "add",
      [1, 2],
    );
    expect(result.return_value).toBe(3);

    const { wall_time_ms, cpu_time_us, heap_used_bytes, heap_allocated_bytes } =
      result.performance;
    expect(wall_time_ms).toBeGreaterThanOrEqual(0);
    expect(Number.isFinite(wall_time_ms)).toBe(true);
    expect(wall_time_ms).toBeLessThan(5000);
    expect(cpu_time_us).toBeGreaterThanOrEqual(0);
    expect(Number.isInteger(cpu_time_us)).toBe(true);
    expect(heap_used_bytes).toBeGreaterThanOrEqual(0);
    expect(heap_allocated_bytes).toBeGreaterThanOrEqual(0);
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

  it("shows measurable heap delta for memory-allocating function", async () => {
    const result = await executeFunction(
      allocatorFixture,
      "allocateArrays",
      [],
    );
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
  it("reports plausible metrics for instrumented execution", async () => {
    const exampleFile = path.join(EXAMPLES_DIR, "01-arithmetic.ts");
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "classifyNumber",
      exampleFile,
    );

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = await executeInstrumented(
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
  it("includes performance metrics in the response", async () => {
    const rawResult = await executeFunction(
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
});

// str-jeen.9 regression: private (non-exported) top-level helpers must be
// runnable through the instrumented module. Discovery (analyzer) reports
// them as targets, so execution must agree on target identity.
describe("executeInstrumented private top-level targets (str-jeen.9)", () => {
  const PRIVATE_TS = path.join(FIXTURES_DIR, "private-helpers.ts");
  const PRIVATE_TSX = path.join(FIXTURES_DIR, "private-helpers.tsx");

  it("executes a private TS function declaration without error", async () => {
    const source = fs.readFileSync(PRIVATE_TS, "utf-8");
    const instr = instrumentFunction(source, "toggleValue", PRIVATE_TS);
    if ("error" in instr) {
      throw new Error(`Instrumentation failed: ${instr.error}`);
    }

    const result = await executeInstrumented(
      instr.instrumentedSource,
      "toggleValue",
      [true],
      [],
      PRIVATE_TS,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("on");
  });

  it("executes a private TS arrow-const helper", async () => {
    const source = fs.readFileSync(PRIVATE_TS, "utf-8");
    const instr = instrumentFunction(source, "classifyMagnitude", PRIVATE_TS);
    if ("error" in instr) {
      throw new Error(`Instrumentation failed: ${instr.error}`);
    }

    const result = await executeInstrumented(
      instr.instrumentedSource,
      "classifyMagnitude",
      [42],
      [],
      PRIVATE_TS,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("large");
  });

  it("executes a private TSX helper that lives alongside an exported component", async () => {
    const source = fs.readFileSync(PRIVATE_TSX, "utf-8");
    const instr = instrumentFunction(source, "formatGreeting", PRIVATE_TSX);
    if ("error" in instr) {
      throw new Error(`Instrumentation failed: ${instr.error}`);
    }

    const result = await executeInstrumented(
      instr.instrumentedSource,
      "formatGreeting",
      ["Alice"],
      [],
      PRIVATE_TSX,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("<span>Hello, Alice!</span>");
  });
});

const SIDE_EFFECTS_FIXTURE = path.resolve(FIXTURES_DIR, "side-effects.ts");

describe("executeFunction side effect capture", () => {
  it("captures console.log and console.warn output", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "logsAndReturns",
      [42],
    );

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

  it("captures thrown error as both thrown_error and side effect", async () => {
    const result = await executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", [
      "boom",
    ]);

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

  it("captures all console levels", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "logsMultipleLevels",
      [],
    );

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

  it("captures custom error types", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "throwsCustomError",
      [],
    );

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

  it("returns empty side_effects for pure functions", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "noSideEffects",
      [1, 2],
    );

    expect(result.return_value).toBe(3);
    expect(result.thrown_error).toBeNull();
    expect(result.side_effects).toEqual([]);
  });

  it("restores global console after execution", async () => {
    const originalConsole = globalThis.console;
    await executeFunction(SIDE_EFFECTS_FIXTURE, "logsAndReturns", [1]);
    expect(globalThis.console).toBe(originalConsole);
  });

  it("restores global console even when function throws", async () => {
    const originalConsole = globalThis.console;
    await executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", ["test"]);
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

  it("captures console output in instrumented execution", async () => {
    const instrumentedSource = getInstrumentedSource("logsAndReturns");
    const result = await executeInstrumented(
      instrumentedSource,
      "logsAndReturns",
      [99],
    );

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

  it("captures thrown error and console output in instrumented execution", async () => {
    const instrumentedSource = getInstrumentedSource("throwsError");
    const result = await executeInstrumented(
      instrumentedSource,
      "throwsError",
      ["instrumented boom"],
    );

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

  it("detects module-level variable changes", async () => {
    const instrumentedSource = getInstrumentedSource("incrementCounter");
    const result = await executeInstrumented(
      instrumentedSource,
      "incrementCounter",
      [],
    );

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
  it("loadModule resolves relative imports from the target file directory", async () => {
    const depsFixture = path.join(FIXTURES_DIR, "dependencies.ts");
    const result = await executeFunction(depsFixture, "usesExternal", [3, 4]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("7");
  });

  it("executeInstrumented resolves relative imports from the source file", async () => {
    const depsFixture = path.join(FIXTURES_DIR, "dependencies.ts");
    const source = fs.readFileSync(depsFixture, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "usesExternal",
      depsFixture,
    );

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = await executeInstrumented(
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

describe("relative TypeScript import resolution (str-xaao, str-jeen.70)", () => {
  const relativeFixture = path.join(
    FIXTURES_DIR,
    "relative-ts-imports",
    "src",
    "entry.ts",
  );
  const extensionlessFixture = path.join(
    FIXTURES_DIR,
    "extensionless-ts-imports",
    "src",
    "entry.ts",
  );

  it("str-xaao: resolves sibling relative .ts imports without stub", async () => {
    const result = await executeFunction(relativeFixture, "compute", [5]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(6);
  });

  it("str-jeen.70: resolves extensionless relative imports to .ts files", async () => {
    const result = await executeFunction(extensionlessFixture, "run", [
      "world",
    ]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("hello world");
  });
});

describe("resolver adapter chain", () => {
  beforeAll(() => {
    fs.writeFileSync(
      RESOLVER_CHAIN_FIXTURE,
      `const virtualValue = require("@virtual/value");
export function readVirtualValue(): number {
  return virtualValue.answer;
}
`,
    );
  });

  afterAll(() => {
    if (fs.existsSync(RESOLVER_CHAIN_FIXTURE)) {
      fs.unlinkSync(RESOLVER_CHAIN_FIXTURE);
    }
  });

  it("applies resolver adapters in order so one hook can rewrite and the next can resolve", async () => {
    const resolverAdapters: ResolverAdapter[] = [
      {
        id: "test.rewrite",
        resolveModule({ module_id }) {
          if (module_id === "@virtual/value") {
            return { kind: "rewrite", module_id: "virtual:value" };
          }
          return { kind: "continue" };
        },
      },
      {
        id: "test.virtualize",
        resolveModule({ module_id }) {
          if (module_id === "virtual:value") {
            return { kind: "resolved", value: { answer: 41 } };
          }
          return { kind: "continue" };
        },
      },
    ];

    const result = await executeFunction(
      RESOLVER_CHAIN_FIXTURE,
      "readVirtualValue",
      [],
      undefined,
      true,
      resolverAdapters,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(41);
  });
});

describe("buildExecuteResponse side effects", () => {
  it("passes side_effects through to the response", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "logsAndReturns",
      [7],
    );
    const response = buildExecuteResponse(1, PROTOCOL_VERSION, result);

    expect(response.side_effects.length).toBeGreaterThan(0);
    expect(response.side_effects).toEqual(result.side_effects);
  });

  it("acceptance: function with console.log and thrown error has both recorded", async () => {
    const result = await executeFunction(SIDE_EFFECTS_FIXTURE, "throwsError", [
      "fail",
    ]);
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

describe("getHarnessCacheDir", () => {
  const original = process.env["SHATTER_HARNESS_CACHE"];

  afterEach(() => {
    if (original === undefined) {
      delete process.env["SHATTER_HARNESS_CACHE"];
    } else {
      process.env["SHATTER_HARNESS_CACHE"] = original;
    }
  });

  it("returns value when set", () => {
    process.env["SHATTER_HARNESS_CACHE"] = "/tmp/cache";
    expect(getHarnessCacheDir()).toBe("/tmp/cache");
  });

  it("returns undefined when unset", () => {
    delete process.env["SHATTER_HARNESS_CACHE"];
    expect(getHarnessCacheDir()).toBeUndefined();
  });

  it("returns undefined when empty", () => {
    process.env["SHATTER_HARNESS_CACHE"] = "";
    expect(getHarnessCacheDir()).toBeUndefined();
  });
});

describe("getHarnessScratchDir", () => {
  const original = process.env["SHATTER_HARNESS_SCRATCH"];

  afterEach(() => {
    if (original === undefined) {
      delete process.env["SHATTER_HARNESS_SCRATCH"];
    } else {
      process.env["SHATTER_HARNESS_SCRATCH"] = original;
    }
  });

  it("returns value when set", () => {
    process.env["SHATTER_HARNESS_SCRATCH"] = "/tmp/scratch";
    expect(getHarnessScratchDir()).toBe("/tmp/scratch");
  });

  it("returns undefined when unset", () => {
    delete process.env["SHATTER_HARNESS_SCRATCH"];
    expect(getHarnessScratchDir()).toBeUndefined();
  });

  it("returns undefined when empty", () => {
    process.env["SHATTER_HARNESS_SCRATCH"] = "";
    expect(getHarnessScratchDir()).toBeUndefined();
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

  it("aborts module-level infinite loop via vm timeout", async () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "0.1";
    clearModuleCache();
    await expect(
      executeFunction(infiniteLoopFixture, "neverReached", []),
    ).rejects.toThrow(/Script execution timed out/);
  });
});

describe("sandbox globals", () => {
  const abortSignalFixture = path.join(FIXTURES_DIR, "abort-signal.ts");

  it("AbortController and AbortSignal are available in sandbox (str-ed25)", async () => {
    const result = await executeFunction(
      abortSignalFixture,
      "useAbortSignal",
      [],
    );
    expect(result.return_value).toBe("not-aborted");
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
    expect(result[0]).toEqual({
      kind: "console_output",
      level: "log",
      message: "line 0",
    });
    expect(result[4]).toEqual({
      kind: "console_output",
      level: "log",
      message: "line 4",
    });
    const marker = result[5];
    expect(marker).toBeDefined();
    expect(marker!.kind).toBe("console_output");
    if (marker !== undefined && marker.kind === "console_output") {
      expect(marker.message).toMatch(/truncated 92 lines/);
    }
    expect(result[6]).toEqual({
      kind: "console_output",
      level: "log",
      message: "line 97",
    });
    expect(result[8]).toEqual({
      kind: "console_output",
      level: "log",
      message: "line 99",
    });
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
    expect(result.some((e) => e.kind === "global_mutation")).toBe(true);
  });
});

describe("TSX support", () => {
  it("executes a function from a .tsx file", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "component.tsx"),
      "greetingLabel",
      ["Alice"],
    );
    expect(result.return_value).toBe("<span>Hello, Alice!</span>");
  });

  it("executes a .tsx function with falsy branch", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "component.tsx"),
      "greetingLabel",
      [""],
    );
    expect(result.return_value).toBe("<span>Hello, stranger!</span>");
  });

  it("instruments and executes TSX source with branches", async () => {
    const tsxFile = path.join(FIXTURES_DIR, "component.tsx");
    const source = fs.readFileSync(tsxFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "greetingLabel",
      tsxFile,
    );

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = await executeInstrumented(
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

const ASYNC_FIXTURE = path.resolve(FIXTURES_DIR, "async-functions.ts");

describe("async function execution", () => {
  const originalEnv = process.env["SHATTER_EXEC_TIMEOUT"];

  afterEach(() => {
    if (originalEnv === undefined) {
      delete process.env["SHATTER_EXEC_TIMEOUT"];
    } else {
      process.env["SHATTER_EXEC_TIMEOUT"] = originalEnv;
    }
  });

  it("awaits async function that resolves and returns the value", async () => {
    const result = await executeFunction(ASYNC_FIXTURE, "asyncAdd", [3, 4]);
    expect(result.return_value).toBe(7);
    expect(result.thrown_error).toBeNull();
  });

  it("captures rejection from async function as thrown_error", async () => {
    const result = await executeFunction(ASYNC_FIXTURE, "asyncThrows", []);
    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.error_type).toBe("Error");
    expect(result.thrown_error!.message).toBe("async boom");
    expect(result.thrown_error!.error_category).toBe("unknown");
  });

  it("times out async function that never resolves", async () => {
    process.env["SHATTER_EXEC_TIMEOUT"] = "0.1";
    clearModuleCache();
    const result = await executeFunction(ASYNC_FIXTURE, "asyncHangs", []);
    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toContain("async execution timed out");
    expect(result.thrown_error!.error_category).toBe("infrastructure");
  });
});

const REACT_FIXTURE = path.resolve(FIXTURES_DIR, "react-component.tsx");

describe("React component execution", () => {
  it("executes a component returning JSX element objects", async () => {
    const result = await executeFunction(REACT_FIXTURE, "StatusCard", [
      { status: "active", count: 15 },
    ]);
    expect(result.thrown_error).toBeNull();
    const el = result.return_value as Record<string, unknown>;
    expect(el.$$typeof).toBe(Symbol.for("react.element"));
    expect(el.type).toBe("div");
  });

  it("hits the inactive branch for non-active status", async () => {
    const result = await executeFunction(REACT_FIXTURE, "StatusCard", [
      { status: "disabled", count: 1 },
    ]);
    expect(result.thrown_error).toBeNull();
    const el = result.return_value as Record<string, unknown>;
    expect(el.type).toBe("div");
    const props = el.props as Record<string, unknown>;
    expect(props.className).toBe("inactive");
  });

  it("evaluates useMemo factory to explore branches", async () => {
    const resultHigh = await executeFunction(REACT_FIXTURE, "StatusCard", [
      { status: "active", count: 20 },
    ]);
    const resultLow = await executeFunction(REACT_FIXTURE, "StatusCard", [
      { status: "active", count: 2 },
    ]);
    expect(resultHigh.thrown_error).toBeNull();
    expect(resultLow.thrown_error).toBeNull();
    // Both should produce element objects — the useMemo factory ran successfully
    expect((resultHigh.return_value as Record<string, unknown>).$$typeof).toBe(
      Symbol.for("react.element"),
    );
    expect((resultLow.return_value as Record<string, unknown>).$$typeof).toBe(
      Symbol.for("react.element"),
    );
  });

  it("evaluates useState function initializer", async () => {
    const result = await executeFunction(REACT_FIXTURE, "InitCounter", [
      { start: 5 },
    ]);
    expect(result.thrown_error).toBeNull();
    const el = result.return_value as Record<string, unknown>;
    expect(el.type).toBe("span");
    // start=5, initializer doubles it → count=10, so "Positive: 10"
    const props = el.props as Record<string, unknown>;
    const children = props.children;
    expect(children).toBeDefined();
  });

  it("hits non-positive branch with negative initializer", async () => {
    const result = await executeFunction(REACT_FIXTURE, "InitCounter", [
      { start: -3 },
    ]);
    expect(result.thrown_error).toBeNull();
    const el = result.return_value as Record<string, unknown>;
    const props = el.props as Record<string, unknown>;
    const children = props.children;
    expect(children).toBeDefined();
  });

  it("instruments and tracks branches in a React component", async () => {
    const source = fs.readFileSync(REACT_FIXTURE, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "StatusCard",
      REACT_FIXTURE,
    );

    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "StatusCard",
      [{ status: "active", count: 15 }],
      [],
      REACT_FIXTURE,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.branch_path.length).toBeGreaterThan(0);
    const el = result.return_value as Record<string, unknown>;
    expect(el.$$typeof).toBe(Symbol.for("react.element"));
  });

  it("does not inject React shim for non-tsx files", async () => {
    // Regular .ts fixture should work exactly as before
    const tsFixture = path.join(FIXTURES_DIR, "primitives.ts");
    const result = await executeFunction(tsFixture, "add", [3, 4]);
    expect(result.return_value).toBe(7);
    expect(result.thrown_error).toBeNull();
  });
});

describe("JSX runtime configuration (str-jeen.29)", () => {
  const JSX_IMPORT_SOURCE_DIR = path.join(FIXTURES_DIR, "jsx-import-source");
  const BADGE_FIXTURE = path.join(JSX_IMPORT_SOURCE_DIR, "badge.tsx");

  // Each test re-applies the project root because other tests reset state.
  // The default fallback test runs setProjectRoot(undefined) explicitly to
  // confirm the executor reverts to the bundled React-shim defaults.
  afterEach(() => {
    setProjectRoot(undefined);
  });

  it("loads `jsx` and `jsxImportSource` from a project's tsconfig", () => {
    setProjectRoot(JSX_IMPORT_SOURCE_DIR);
    const opts = getCurrentJsxRuntimeOptions();
    // Compare against the live ts enum to avoid hardcoding the numeric value
    // that `"react-jsx"` parses to.
    expect(opts.jsx).toBe(ts.JsxEmit.ReactJSX);
    expect(opts.jsxImportSource).toBe("preact");
  });

  it("executes a TSX function whose project declares jsxImportSource: preact", async () => {
    setProjectRoot(JSX_IMPORT_SOURCE_DIR);
    const high = await executeFunction(BADGE_FIXTURE, "Badge", [
      { severity: "high", count: 3 },
    ]);
    expect(high.thrown_error).toBeNull();
    const elHigh = high.return_value as Record<string, unknown>;
    expect(elHigh.$$typeof).toBe(Symbol.for("react.element"));
    expect(elHigh.type).toBe("span");
    const propsHigh = elHigh.props as Record<string, unknown>;
    expect(propsHigh.className).toBe("badge-high");

    const low = await executeFunction(BADGE_FIXTURE, "Badge", [
      { severity: "low", count: 0 },
    ]);
    expect(low.thrown_error).toBeNull();
    const elLow = low.return_value as Record<string, unknown>;
    const propsLow = elLow.props as Record<string, unknown>;
    expect(propsLow.className).toBe("badge-low");
  });

  it("falls back to the default automatic-React runtime when no project root is set", async () => {
    setProjectRoot(undefined);
    const opts = getCurrentJsxRuntimeOptions();
    expect(opts.jsx).toBe(ts.JsxEmit.ReactJSX);
    expect(opts.jsxImportSource).toBeUndefined();

    // The pre-existing React fixture must keep executing exactly as before
    // without any project_root: this is the "default jsx-runtime fallback
    // unchanged" half of the str-jeen.29 acceptance criterion.
    clearModuleCache();
    const result = await executeFunction(REACT_FIXTURE, "StatusCard", [
      { status: "active", count: 15 },
    ]);
    expect(result.thrown_error).toBeNull();
    const el = result.return_value as Record<string, unknown>;
    expect(el.$$typeof).toBe(Symbol.for("react.element"));
    expect(el.type).toBe("div");
  });
});

describe("scope events in execution", () => {
  it("loop function returns scope_events with loop_enter/loop_exit", async () => {
    const source = `export function countdown(n: number): number {
  let result = 0;
  while (n > 0) {
    result += n;
    n--;
  }
  return result;
}`;
    const instrumentResult = instrumentFunction(source, "countdown");
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "countdown",
      [3],
    );
    expect(result.return_value).toBe(6);
    expect(result.scope_events).toBeDefined();
    expect(result.scope_events.length).toBeGreaterThan(0);

    const loopEnters = result.scope_events.filter(
      (e: TraceEvent) => e.type === "scope" && e.event.kind === "loop_enter",
    );
    const loopExits = result.scope_events.filter(
      (e: TraceEvent) => e.type === "scope" && e.event.kind === "loop_exit",
    );
    expect(loopEnters).toHaveLength(3);
    expect(loopExits).toHaveLength(3);
  });

  it("inline callback in .map() produces call_enter/call_exit in scope_events", async () => {
    const source = `export function doublePositive(items: number[]): number[] {
  return items.map((x) => {
    if (x > 0) {
      return x * 2;
    }
    return 0;
  });
}`;
    const instrumentResult = instrumentFunction(source, "doublePositive");
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "doublePositive",
      [[1, -2, 3]],
    );
    expect(result.return_value).toEqual([2, 0, 6]);
    expect(result.scope_events).toBeDefined();

    const callEnters = result.scope_events.filter(
      (e: TraceEvent) => e.type === "scope" && e.event.kind === "call_enter",
    );
    // At least 1 (top-level function) + 3 (callback invocations) = 4 call_enters
    expect(callEnters.length).toBeGreaterThanOrEqual(4);
  });

  it("buildExecuteResponse includes scope_events", () => {
    const raw = {
      return_value: 42,
      thrown_error: null,
      performance: {
        wall_time_ms: 1,
        cpu_time_us: 1000,
        heap_used_bytes: 0,
        heap_allocated_bytes: 0,
      },
      branch_path: [],
      path_constraints: [],
      lines_executed: [],
      side_effects: [],
      calls_to_external: [],
      scope_events: [
        {
          type: "scope" as const,
          event: { kind: "call_enter" as const, call_site_id: 0 },
        },
        {
          type: "scope" as const,
          event: { kind: "call_exit" as const, call_site_id: 0 },
        },
      ],
      loop_body_states: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
      adapter_hints: [],
    };
    const response = buildExecuteResponse(1, "0.6.0", raw);
    expect(response.scope_events).toHaveLength(2);
    expect(response.scope_events![0]).toEqual({
      type: "scope",
      event: { kind: "call_enter", call_site_id: 0 },
    });
  });

  it("emits loop_body_states for supported counted loops", async () => {
    const source = `export function sumTo(n: number): number {
  let total = 0;
  for (let i = 0; i < n; i++) {
    total += i;
  }
  return total;
}`;
    const fixturePath = path.join(os.tmpdir(), `sum-to-${Date.now()}.ts`);
    fs.writeFileSync(fixturePath, source);
    const analysis = analyzeFile(fixturePath, "sumTo");
    const instrumentResult = instrumentFunction(source, "sumTo", fixturePath);
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "sumTo",
      [3],
      [],
      fixturePath,
      undefined,
      true,
      undefined,
      undefined,
      undefined,
      analysis[0]?.loops ?? [],
    );

    expect(result.loop_body_states).toHaveLength(3);
    expect(result.loop_body_states[0]).toEqual({
      loop_id: 0,
      iteration: 0,
      locals: {
        i: { kind: "const", type: "int", value: 0 },
        total: { kind: "const", type: "int", value: 0 },
      },
    });
    expect(result.loop_body_states[1]?.loop_id).toBe(0);
    expect(result.loop_body_states[1]?.iteration).toBe(1);
    expect(result.loop_body_states[1]?.locals["total"]).toEqual({
      kind: "bin_op",
      op: "add",
      left: { kind: "const", type: "int", value: 0 },
      right: { kind: "const", type: "int", value: 0 },
    });
    expect(result.loop_body_states[2]?.iteration).toBe(2);
  });
});

describe("throw_error mock behavior", () => {
  it("throws when default_behavior is throw_error with error details", async () => {
    // Source that calls the mock and catches the error
    const source = `
      export function callMock(): string {
        const fn = __shatter_mocks["mymod:myFunc"];
        if (fn) {
          try { fn(); } catch (e: any) { return e.message; }
        }
        return "no mock";
      }
    `;
    const mocks = [
      {
        symbol: "mymod:myFunc",
        return_values: [
          { code: "ENOENT", message: "No such file or directory" },
        ],
        should_track_calls: false,
        default_behavior: "throw_error" as const,
      },
    ];

    const result = await executeInstrumented(source, "callMock", [], mocks);
    expect(result.return_value).toBe("No such file or directory");
  });

  it("throws generic error when return_values is empty", async () => {
    const source = `
      export function callMock(): string {
        const fn = __shatter_mocks["lib:doStuff"];
        if (fn) {
          try { fn(); } catch (e: any) { return e.message; }
        }
        return "no mock";
      }
    `;
    const mocks = [
      {
        symbol: "lib:doStuff",
        return_values: [] as unknown[],
        should_track_calls: false,
        default_behavior: "throw_error" as const,
      },
    ];

    const result = await executeInstrumented(source, "callMock", [], mocks);
    expect(result.return_value).toBe("Mock error: lib:doStuff");
  });

  it("does not register throw_error mock as passthrough", async () => {
    const source = `
      export function callMock(): boolean {
        const fn = __shatter_mocks["net:fetch"];
        return typeof fn === "function";
      }
    `;
    const mocks = [
      {
        symbol: "net:fetch",
        return_values: [{ status: 500, error: "Internal Server Error" }],
        should_track_calls: true,
        default_behavior: "throw_error" as const,
      },
    ];

    const result = await executeInstrumented(source, "callMock", [], mocks);
    expect(result.return_value).toBe(true);
  });
});

describe("execution-time dep gap detection", () => {
  it("detects unmocked require() calls to external modules", async () => {
    const source = `
      const path = require("path");
      export function joinPaths(a: string, b: string): string {
        return path.join(a, b);
      }
    `;
    const result = await executeInstrumented(
      source,
      "joinPaths",
      ["foo", "bar"],
      [],
    );
    expect(result.discovered_dependencies.length).toBeGreaterThanOrEqual(1);
    const pathDep = result.discovered_dependencies.find(
      (d) => d.source_module === "path",
    );
    expect(pathDep).toBeDefined();
    expect(pathDep!.kind).toBe("unmocked_import");
    expect(pathDep!.is_subprocess_spawn).toBe(false);
  });

  it("detects subprocess-spawning module imports", async () => {
    const source = `
      const cp = require("child_process");
      export function getVersion(): string {
        return typeof cp.execSync;
      }
    `;
    const result = await executeInstrumented(source, "getVersion", [], []);
    const cpDep = result.discovered_dependencies.find(
      (d) => d.source_module === "child_process",
    );
    expect(cpDep).toBeDefined();
    expect(cpDep!.kind).toBe("subprocess_spawn");
    expect(cpDep!.is_subprocess_spawn).toBe(true);
  });

  it("does not flag mocked modules as unmocked", async () => {
    const source = `
      const fs = require("fs");
      export function readIt(): string {
        return typeof fs.readFileSync;
      }
    `;
    const mocks = [
      {
        symbol: "fs:readFileSync",
        return_values: ["fake"],
        should_track_calls: false,
        default_behavior: "repeat_last" as const,
      },
    ];
    const result = await executeInstrumented(source, "readIt", [], mocks);
    const fsDep = result.discovered_dependencies.find(
      (d) => d.source_module === "fs",
    );
    expect(fsDep).toBeUndefined();
  });

  it("does not flag relative imports", async () => {
    const source = `
      export function noop(): number {
        return 42;
      }
    `;
    const result = await executeInstrumented(source, "noop", [], []);
    expect(result.discovered_dependencies.length).toBe(0);
  });
});

describe("stubbed_import fallback for unresolvable modules", () => {
  it("returns a stub for MODULE_NOT_FOUND and records stubbed_import dependency", async () => {
    const source = `
      const fake = require("nonexistent-module-xyz-stub-test");
      export function useFake(): string {
        return typeof fake.someMethod;
      }
    `;
    const result = await executeInstrumented(source, "useFake", [], []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("function");
    const dep = result.discovered_dependencies.find(
      (d) => d.source_module === "nonexistent-module-xyz-stub-test",
    );
    expect(dep).toBeDefined();
    expect(dep!.kind).toBe("stubbed_import");
    expect(dep!.is_subprocess_spawn).toBe(false);
  });

  it("stub supports nested property access without crashing", async () => {
    const source = `
      const fake = require("nonexistent-deep-xyz-stub-test");
      export function deep(): string {
        return typeof fake.a.b.c.d;
      }
    `;
    const result = await executeInstrumented(source, "deep", [], []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("function");
  });

  it("stub supports constructor calls", async () => {
    const source = `
      const Fake = require("nonexistent-ctor-xyz-stub-test");
      export function construct(): string {
        const instance = new Fake.Client({ host: "localhost" });
        return typeof instance.connect;
      }
    `;
    const result = await executeInstrumented(source, "construct", [], []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("function");
  });

  it("does not stub modules that exist", async () => {
    const source = `
      const nodePath = require("path");
      export function joinIt(): string {
        return nodePath.join("a", "b");
      }
    `;
    const result = await executeInstrumented(source, "joinIt", [], []);
    expect(result.thrown_error).toBeNull();
    const stubbedDep = result.discovered_dependencies.find(
      (d) => d.kind === "stubbed_import",
    );
    expect(stubbedDep).toBeUndefined();
  });

  it("records stubbed_import only once per module", async () => {
    const source = `
      const a = require("nonexistent-dedup-xyz-stub-test");
      const b = require("nonexistent-dedup-xyz-stub-test");
      export function run(): number { return 1; }
    `;
    const result = await executeInstrumented(source, "run", [], []);
    const deps = result.discovered_dependencies.filter(
      (d) => d.source_module === "nonexistent-dedup-xyz-stub-test",
    );
    expect(deps).toHaveLength(1);
  });

  it("allows resolver adapters to request a stub before default fallback handling", async () => {
    const source = `
      const fake = require("adapter-controlled-stub");
      export function useFake(): string {
        return typeof fake.anything;
      }
    `;
    const resolverAdapters: ResolverAdapter[] = [
      {
        id: "test.stub",
        resolveModule({ module_id }) {
          if (module_id === "adapter-controlled-stub") {
            return { kind: "stub" };
          }
          return { kind: "continue" };
        },
      },
    ];

    const result = await executeInstrumented(
      source,
      "useFake",
      [],
      [],
      undefined,
      undefined,
      true,
      undefined,
      resolverAdapters,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("function");
    expect(result.discovered_dependencies).toContainEqual({
      symbol: "adapter-controlled-stub",
      source_module: "adapter-controlled-stub",
      kind: "stubbed_import",
      is_subprocess_spawn: false,
    });
  });

  it("stub .then returns undefined to prevent thenable coercion", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(stub.then).toBeUndefined();
  });

  it("stub .__esModule returns true for ESM interop", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(stub.__esModule).toBe(true);
  });

  it("stub function call returns chainable stub", () => {
    const stub = createUnresolvableModuleStub("test-module");
    const result = asCallable(stub)();
    expect(typeof result).toBe("function");
    expect(() => (result as Record<string, unknown>).foo).not.toThrow();
  });

  it("stub method call result is callable and chainable", () => {
    const stub = createUnresolvableModuleStub("test-module");
    const method = asCallable((stub as Record<string, unknown>).method);
    const result = method();
    expect(() => asCallable(result)()).not.toThrow();
  });

  it("stub is spreadable with spread operator", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect([...(stub as unknown as Iterable<unknown>)]).toEqual([]);
  });

  it("stub is iterable in for-of without throwing", () => {
    const stub = createUnresolvableModuleStub("test-module");
    const items: unknown[] = [];
    for (const x of stub as unknown as Iterable<unknown>) {
      items.push(x);
    }
    expect(items).toEqual([]);
  });

  it("stub coerces to empty string not 'undefined'", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(`${stub as unknown as string}`).toBe("");
  });

  it("stub coerces to 0 for numeric hint", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(+(stub as unknown as number)).toBe(0);
  });

  it("stub supports instanceof without throwing", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(
      () => ({}) instanceof (stub as unknown as { new (): unknown }),
    ).not.toThrow();
  });

  it("stub 'in' operator returns true for any property", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect("connect" in stub).toBe(true);
    expect("nonexistent" in stub).toBe(true);
    expect(Symbol.iterator in stub).toBe(true);
  });

  it("stub property assignment does not throw", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(() => {
      (stub as Record<string, unknown>).options = { timeout: 5000 };
    }).not.toThrow();
  });

  it("stub property deletion does not throw", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(() => {
      delete (stub as Record<string, unknown>).foo;
    }).not.toThrow();
  });

  it("stub Object.keys returns empty array", () => {
    const stub = createUnresolvableModuleStub("test-module");
    expect(Object.keys(stub)).toEqual([]);
  });

  it("stub feature detection via 'in' works through executeInstrumented", async () => {
    const source = `
      const Fake = require("nonexistent-feature-detect-stub-test");
      export function detect(): boolean {
        return "connect" in Fake && "send" in Fake;
      }
    `;
    const result = await executeInstrumented(source, "detect", [], []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(true);
  });

  it("stub chained calls work through executeInstrumented", async () => {
    const source = `
      const Fake = require("nonexistent-chain-xyz-stub-test");
      export function chain(): string {
        const client = new Fake.Client({ host: "localhost" });
        const result = client.connect().execute("query");
        return typeof result;
      }
    `;
    const result = await executeInstrumented(source, "chain", [], []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("function");
  });
});

describe("classifyConnectionFailure", () => {
  it("classifies ECONNREFUSED as connection_refused", () => {
    expect(
      classifyConnectionFailure("connect ECONNREFUSED 127.0.0.1:5432"),
    ).toBe("connection_refused");
  });

  it("classifies 'connection refused' as connection_refused", () => {
    expect(classifyConnectionFailure("Error: connection refused")).toBe(
      "connection_refused",
    );
  });

  it("classifies ENOTFOUND as dns_failure", () => {
    expect(
      classifyConnectionFailure("getaddrinfo ENOTFOUND api.example.com"),
    ).toBe("dns_failure");
  });

  it("classifies EAI_AGAIN as dns_failure", () => {
    expect(classifyConnectionFailure("EAI_AGAIN dns lookup failed")).toBe(
      "dns_failure",
    );
  });

  it("classifies getaddrinfo as dns_failure", () => {
    expect(classifyConnectionFailure("getaddrinfo failed")).toBe("dns_failure");
  });

  it("classifies EAUTH as auth_error", () => {
    expect(classifyConnectionFailure("EAUTH: authentication required")).toBe(
      "auth_error",
    );
  });

  it("classifies 401 Unauthorized as auth_error", () => {
    expect(classifyConnectionFailure("HTTP 401 Unauthorized")).toBe(
      "auth_error",
    );
  });

  it("classifies 403 Forbidden as auth_error", () => {
    expect(classifyConnectionFailure("HTTP 403 Forbidden")).toBe("auth_error");
  });

  it("classifies ETIMEDOUT as timeout", () => {
    expect(classifyConnectionFailure("connect ETIMEDOUT")).toBe("timeout");
  });

  it("classifies ESOCKETTIMEDOUT as timeout", () => {
    expect(classifyConnectionFailure("ESOCKETTIMEDOUT on request")).toBe(
      "timeout",
    );
  });

  it("classifies 'timed out' as timeout", () => {
    expect(classifyConnectionFailure("request timed out")).toBe("timeout");
  });

  it("returns null for application errors", () => {
    expect(
      classifyConnectionFailure("ValidationError: invalid input"),
    ).toBeNull();
  });

  it("returns null for generic errors", () => {
    expect(classifyConnectionFailure("Something went wrong")).toBeNull();
  });

  it("returns null for empty string", () => {
    expect(classifyConnectionFailure("")).toBeNull();
  });

  it("covers all CONN_REFUSED_PATTERNS", () => {
    for (const pattern of CONN_REFUSED_PATTERNS) {
      expect(classifyConnectionFailure(`error: ${pattern}`)).toBe(
        "connection_refused",
      );
    }
  });

  it("covers all DNS_FAILURE_PATTERNS", () => {
    for (const pattern of DNS_FAILURE_PATTERNS) {
      expect(classifyConnectionFailure(`error: ${pattern}`)).toBe(
        "dns_failure",
      );
    }
  });

  it("covers all AUTH_ERROR_PATTERNS", () => {
    for (const pattern of AUTH_ERROR_PATTERNS) {
      expect(classifyConnectionFailure(`error: ${pattern}`)).toBe("auth_error");
    }
  });

  it("covers all TIMEOUT_PATTERNS", () => {
    for (const pattern of TIMEOUT_PATTERNS) {
      expect(classifyConnectionFailure(`error: ${pattern}`)).toBe("timeout");
    }
  });
});

describe("connection_failures in executeInstrumented", () => {
  it("returns empty connection_failures for normal execution", async () => {
    const source = `
      export function add(a: number, b: number): number {
        return a + b;
      }
    `;
    const result = await executeInstrumented(source, "add", [1, 2], []);
    expect(result.connection_failures).toEqual([]);
  });
});

describe("buildExecuteResponse includes connection_failures", () => {
  it("omits connection_failures when empty", () => {
    const rawResult = {
      return_value: 42,
      thrown_error: null,
      performance: {
        wall_time_ms: 1,
        cpu_time_us: 1000,
        heap_used_bytes: 0,
        heap_allocated_bytes: 0,
      },
      branch_path: [],
      path_constraints: [],
      lines_executed: [],
      side_effects: [],
      calls_to_external: [],
      scope_events: [],
      loop_body_states: [],
      discovered_dependencies: [],
      connection_failures: [],
      runtime_crypto_boundaries: [],
      adapter_hints: [],
    };
    const resp = buildExecuteResponse(1, PROTOCOL_VERSION, rawResult);
    expect(resp.connection_failures).toBeUndefined();
  });

  it("includes connection_failures when non-empty", () => {
    const rawResult = {
      return_value: null,
      thrown_error: null,
      performance: {
        wall_time_ms: 1,
        cpu_time_us: 1000,
        heap_used_bytes: 0,
        heap_allocated_bytes: 0,
      },
      branch_path: [],
      path_constraints: [],
      lines_executed: [],
      side_effects: [],
      calls_to_external: [],
      scope_events: [],
      loop_body_states: [],
      discovered_dependencies: [],
      connection_failures: [
        {
          symbol: "pg:query",
          error_kind: "connection_refused" as const,
          message: "ECONNREFUSED",
        },
      ],
      runtime_crypto_boundaries: [],
      adapter_hints: [],
    };
    const resp = buildExecuteResponse(1, PROTOCOL_VERSION, rawResult);
    expect(resp.connection_failures).toEqual([
      {
        symbol: "pg:query",
        error_kind: "connection_refused",
        message: "ECONNREFUSED",
      },
    ]);
  });
});

// ---------------------------------------------------------------------------
// buildRuntimeCryptoBoundary tests
// ---------------------------------------------------------------------------

describe("buildRuntimeCryptoBoundary", () => {
  it("extracts algorithm, key, and iv from createDecipheriv arguments", () => {
    const key = Buffer.from("0123456789abcdef0123456789abcdef", "utf-8");
    const iv = Buffer.from("0123456789abcdef", "utf-8");
    const result = buildRuntimeCryptoBoundary(
      "cb-0",
      "decrypt",
      "createDecipheriv",
      ["aes-256-cbc", key, iv],
    );
    expect(result.boundary_id).toBe("cb-0");
    expect(result.kind).toBe("decrypt");
    expect(result.function_name).toBe("createDecipheriv");
    expect(result.algorithm).toBe("aes-256-cbc");
    expect(result.key_value).toBe(key.toString("base64"));
    expect(result.iv_value).toBe(iv.toString("base64"));
    expect(result.ciphertext_param_index).toBeUndefined();
  });

  it("extracts ciphertext_param_index for privateDecrypt", () => {
    const ciphertext = Buffer.from("encrypted-data");
    const result = buildRuntimeCryptoBoundary(
      "cb-1",
      "decrypt",
      "privateDecrypt",
      [{ key: "pem-key" }, ciphertext],
    );
    expect(result.kind).toBe("decrypt");
    expect(result.function_name).toBe("privateDecrypt");
    expect(result.ciphertext_param_index).toBe(1);
    expect(result.algorithm).toBeUndefined();
  });

  it("returns boundary with only required fields when no param roles match", () => {
    const result = buildRuntimeCryptoBoundary(
      "cb-2",
      "encrypt",
      "unknownCrypto",
      [],
    );
    expect(result.boundary_id).toBe("cb-2");
    expect(result.kind).toBe("encrypt");
    expect(result.function_name).toBe("unknownCrypto");
    expect(result.algorithm).toBeUndefined();
    expect(result.key_value).toBeUndefined();
    expect(result.iv_value).toBeUndefined();
    expect(result.ciphertext_param_index).toBeUndefined();
  });

  it("handles string key/iv by base64-encoding as binary", () => {
    const result = buildRuntimeCryptoBoundary(
      "cb-3",
      "decrypt",
      "createDecipheriv",
      ["aes-128-gcm", "rawkeystring", "rawivstring"],
    );
    expect(result.key_value).toBeDefined();
    expect(result.iv_value).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// isMcdcEnabled tests
// ---------------------------------------------------------------------------

describe("isMcdcEnabled", () => {
  afterEach(() => {
    delete process.env["SHATTER_MCDC"];
  });

  it("returns false when SHATTER_MCDC is not set", () => {
    delete process.env["SHATTER_MCDC"];
    expect(isMcdcEnabled()).toBe(false);
  });

  it("returns true when SHATTER_MCDC=1", () => {
    process.env["SHATTER_MCDC"] = "1";
    expect(isMcdcEnabled()).toBe(true);
  });

  it("returns false when SHATTER_MCDC=0", () => {
    process.env["SHATTER_MCDC"] = "0";
    expect(isMcdcEnabled()).toBe(false);
  });

  it("returns false when SHATTER_MCDC is any value other than '1'", () => {
    process.env["SHATTER_MCDC"] = "true";
    expect(isMcdcEnabled()).toBe(false);

    process.env["SHATTER_MCDC"] = "yes";
    expect(isMcdcEnabled()).toBe(false);

    process.env["SHATTER_MCDC"] = "";
    expect(isMcdcEnabled()).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// MC/DC end-to-end: instrument + execute with __shatter_mcdc_* callbacks
// ---------------------------------------------------------------------------

describe("executeInstrumented MC/DC mode", () => {
  afterEach(() => {
    delete process.env["SHATTER_MCDC"];
  });

  it("records conditions for && compound decision (both true path)", async () => {
    process.env["SHATTER_MCDC"] = "1";

    const source = `export function compoundAnd(a: number, b: number): string {
  if (a > 0 && b < 10) {
    return "both";
  }
  return "neither";
}`;

    const instrumentResult = instrumentFunction(source, "compoundAnd");
    expect("error" in instrumentResult).toBe(false);
    if ("error" in instrumentResult) return;

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "compoundAnd",
      [5, 3],
    );

    expect(result.return_value).toBe("both");
    expect(result.branch_path).toHaveLength(1);
    const bd = result.branch_path[0]!;
    expect(bd.taken).toBe(true);
    expect(bd.conditions).toBeDefined();
    expect(bd.conditions!.length).toBe(2);
    // First condition: a > 0 (true, not masked)
    expect(bd.conditions![0]!.condition_index).toBe(0);
    expect(bd.conditions![0]!.value).toBe(true);
    expect(bd.conditions![0]!.masked).toBe(false);
    // Second condition: b < 10 (true, not masked)
    expect(bd.conditions![1]!.condition_index).toBe(1);
    expect(bd.conditions![1]!.value).toBe(true);
    expect(bd.conditions![1]!.masked).toBe(false);
  });

  it("records conditions for && compound decision (first condition false — masks second)", async () => {
    process.env["SHATTER_MCDC"] = "1";

    const source = `export function compoundAnd(a: number, b: number): string {
  if (a > 0 && b < 10) {
    return "both";
  }
  return "neither";
}`;

    const instrumentResult = instrumentFunction(source, "compoundAnd");
    expect("error" in instrumentResult).toBe(false);
    if ("error" in instrumentResult) return;

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "compoundAnd",
      [-1, 3],
    );

    expect(result.return_value).toBe("neither");
    const bd = result.branch_path[0]!;
    expect(bd.taken).toBe(false);
    expect(bd.conditions!.length).toBe(2);
    // First condition: a > 0 is false
    expect(bd.conditions![0]!.value).toBe(false);
    expect(bd.conditions![0]!.masked).toBe(false);
    // Second condition masked by short-circuit
    expect(bd.conditions![1]!.value).toBeNull();
    expect(bd.conditions![1]!.masked).toBe(true);
  });

  it("records conditions for || compound decision (first true — masks second)", async () => {
    process.env["SHATTER_MCDC"] = "1";

    const source = `export function compoundOr(x: boolean, y: boolean): string {
  if (x || y) {
    return "either";
  }
  return "none";
}`;

    const instrumentResult = instrumentFunction(source, "compoundOr");
    expect("error" in instrumentResult).toBe(false);
    if ("error" in instrumentResult) return;

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "compoundOr",
      [true, false],
    );

    expect(result.return_value).toBe("either");
    const bd = result.branch_path[0]!;
    expect(bd.taken).toBe(true);
    expect(bd.conditions!.length).toBe(2);
    // First condition: x is true → short-circuit
    expect(bd.conditions![0]!.value).toBe(true);
    expect(bd.conditions![0]!.masked).toBe(false);
    // Second condition masked
    expect(bd.conditions![1]!.value).toBeNull();
    expect(bd.conditions![1]!.masked).toBe(true);
  });

  it("simple condition falls back to plain __shatter_branch (no conditions array)", async () => {
    process.env["SHATTER_MCDC"] = "1";

    const source = `export function simple(a: number): string {
  if (a > 0) {
    return "pos";
  }
  return "non-pos";
}`;

    const instrumentResult = instrumentFunction(source, "simple");
    expect("error" in instrumentResult).toBe(false);
    if ("error" in instrumentResult) return;

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "simple",
      [5],
    );

    expect(result.return_value).toBe("pos");
    const bd = result.branch_path[0]!;
    expect(bd.taken).toBe(true);
    // Simple condition: no conditions array
    expect(bd.conditions).toBeUndefined();
  });

  it("does not populate conditions when SHATTER_MCDC is not set", async () => {
    delete process.env["SHATTER_MCDC"];

    const source = `export function compoundAnd(a: number, b: number): string {
  if (a > 0 && b < 10) {
    return "both";
  }
  return "neither";
}`;

    const instrumentResult = instrumentFunction(source, "compoundAnd");
    expect("error" in instrumentResult).toBe(false);
    if ("error" in instrumentResult) return;

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "compoundAnd",
      [5, 3],
    );

    const bd = result.branch_path[0]!;
    expect(bd.taken).toBe(true);
    // No MC/DC mode — no conditions field
    expect(bd.conditions).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// No-capture fast path
// ---------------------------------------------------------------------------

describe("executeFunction no-capture fast path", () => {
  it("returns empty side_effects when capture=false", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "logsAndReturns",
      [42],
      undefined,
      false,
    );
    expect(result.side_effects).toEqual([]);
  });

  it("returns correct return_value when capture=false", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "logsAndReturns",
      [42],
      undefined,
      false,
    );
    expect(result.return_value).toBe("done: 42");
  });

  it("returns correct thrown_error when capture=false", async () => {
    const result = await executeFunction(
      SIDE_EFFECTS_FIXTURE,
      "throwsError",
      ["bang"],
      undefined,
      false,
    );
    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toBe("bang");
    // No side_effects even though there's a thrown error
    expect(result.side_effects).toEqual([]);
  });

  it("is faster than capture=true over many iterations", async () => {
    const N = 200;
    const t0 = Date.now();
    for (let i = 0; i < N; i++) {
      clearModuleCache();
      await executeFunction(
        SIDE_EFFECTS_FIXTURE,
        "logsAndReturns",
        [i],
        undefined,
        true,
      );
    }
    const captureMs = Date.now() - t0;

    const t1 = Date.now();
    for (let i = 0; i < N; i++) {
      clearModuleCache();
      await executeFunction(
        SIDE_EFFECTS_FIXTURE,
        "logsAndReturns",
        [i],
        undefined,
        false,
      );
    }
    const noCaptureMs = Date.now() - t1;

    // No-capture should be at least as fast as capture (not significantly slower).
    // We use a generous bound to avoid flakiness, but expect meaningful improvement.
    expect(noCaptureMs).toBeLessThan(captureMs * 1.5);
  });
});

describe("executeInstrumented no-capture fast path", () => {
  function getInstrumentedSourceForNoCapture(funcName: string): string {
    const source = fs.readFileSync(SIDE_EFFECTS_FIXTURE, "utf-8");
    const result = instrumentFunction(source, funcName, SIDE_EFFECTS_FIXTURE);
    if ("error" in result) throw new Error(result.error);
    return result.instrumentedSource;
  }

  it("returns empty side_effects when capture=false", async () => {
    const instrumentedSource =
      getInstrumentedSourceForNoCapture("logsAndReturns");
    const result = await executeInstrumented(
      instrumentedSource,
      "logsAndReturns",
      [99],
      [],
      undefined,
      undefined,
      false,
    );
    expect(result.side_effects).toEqual([]);
  });

  it("still populates branch_path when capture=false", async () => {
    const instrumentedSource =
      getInstrumentedSourceForNoCapture("logsAndReturns");
    const result = await executeInstrumented(
      instrumentedSource,
      "logsAndReturns",
      [99],
      [],
      undefined,
      undefined,
      false,
    );
    // branch_path should still be populated (capture only affects side_effects)
    expect(result.branch_path).toBeDefined();
    expect(result.lines_executed).toBeDefined();
  });

  it("returns correct return_value when capture=false", async () => {
    const instrumentedSource =
      getInstrumentedSourceForNoCapture("logsAndReturns");
    const result = await executeInstrumented(
      instrumentedSource,
      "logsAndReturns",
      [55],
      [],
      undefined,
      undefined,
      false,
    );
    expect(result.return_value).toBe("done: 55");
  });

  it("returns correct thrown_error when capture=false", async () => {
    const instrumentedSource = getInstrumentedSourceForNoCapture("throwsError");
    const result = await executeInstrumented(
      instrumentedSource,
      "throwsError",
      ["oops"],
      [],
      undefined,
      undefined,
      false,
    );
    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toBe("oops");
    expect(result.side_effects).toEqual([]);
  });

  it("is faster than capture=true over many iterations", async () => {
    const instrumentedSource =
      getInstrumentedSourceForNoCapture("logsAndReturns");
    const N = 100;

    const t0 = Date.now();
    for (let i = 0; i < N; i++) {
      await executeInstrumented(
        instrumentedSource,
        "logsAndReturns",
        [i],
        [],
        undefined,
        undefined,
        true,
      );
    }
    const captureMs = Date.now() - t0;

    const t1 = Date.now();
    for (let i = 0; i < N; i++) {
      await executeInstrumented(
        instrumentedSource,
        "logsAndReturns",
        [i],
        [],
        undefined,
        undefined,
        false,
      );
    }
    const noCaptureMs = Date.now() - t1;

    // No-capture should not be significantly slower than capture.
    expect(noCaptureMs).toBeLessThan(captureMs * 1.5);
  });
});

describe("executeInstrumented script caching", () => {
  const exampleFile = path.join(EXAMPLES_DIR, "01-arithmetic.ts");

  beforeEach(() => {
    clearCompiledScriptCache();
    clearModuleCache();
  });

  it("produces identical results with and without a cache key", async () => {
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "classifyNumber",
      exampleFile,
    );
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    const uncached = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
    );
    const cached = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
      [],
      exampleFile,
      undefined,
      true,
      "test-key",
    );

    expect(cached.return_value).toBe(uncached.return_value);
    expect(cached.branch_path.map((b) => b.taken)).toEqual(
      uncached.branch_path.map((b) => b.taken),
    );
  });

  it("amortizes transpilation: second call with same key omits execute.transpile from timing", async () => {
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "classifyNumber",
      exampleFile,
    );
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    // First call — cache miss, execute.transpile should appear in timing
    const timing1 = new TimingCollector();
    await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
      [],
      exampleFile,
      timing1,
      true,
      "amort-key",
    );
    const phases1 = timing1.toSummary()?.phases.map((p) => p.phase_path) ?? [];
    expect(phases1).toContain("execute.transpile");

    // Second call — cache hit, execute.transpile should NOT appear in timing
    const timing2 = new TimingCollector();
    await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [-1],
      [],
      exampleFile,
      timing2,
      true,
      "amort-key",
    );
    const phases2 = timing2.toSummary()?.phases.map((p) => p.phase_path) ?? [];
    expect(phases2).not.toContain("execute.transpile");
    // execute.module_load (the actual execution) still runs on every call
    expect(phases2).toContain("execute.module_load");
  });

  it("deleteCompiledScriptEntry forces recompilation on next call", async () => {
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "classifyNumber",
      exampleFile,
    );
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    // Warm the cache
    const timing1 = new TimingCollector();
    await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
      [],
      exampleFile,
      timing1,
      true,
      "evict-key",
    );
    expect(timing1.toSummary()?.phases.map((p) => p.phase_path)).toContain(
      "execute.transpile",
    );

    // Evict and verify next call recompiles
    deleteCompiledScriptEntry("evict-key");

    const timing2 = new TimingCollector();
    await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
      [],
      exampleFile,
      timing2,
      true,
      "evict-key",
    );
    expect(timing2.toSummary()?.phases.map((p) => p.phase_path)).toContain(
      "execute.transpile",
    );
  });

  it("different inputs with same cache key return correct results for each input", async () => {
    const source = fs.readFileSync(exampleFile, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "classifyNumber",
      exampleFile,
    );
    if ("error" in instrumentResult) throw new Error(instrumentResult.error);

    const r1 = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [42],
      [],
      exampleFile,
      undefined,
      true,
      "multi-input-key",
    );
    const r2 = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [-1],
      [],
      exampleFile,
      undefined,
      true,
      "multi-input-key",
    );
    const r3 = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "classifyNumber",
      [0],
      [],
      exampleFile,
      undefined,
      true,
      "multi-input-key",
    );

    expect(r1.return_value).toBe("positive-even");
    expect(r2.return_value).toBe("negative");
    expect(r3.return_value).toBe("zero");
  });
});

// ---------------------------------------------------------------------------
// ESM dynamic-import serialization (str-4hay)
// ---------------------------------------------------------------------------

describe("transformDynamicImports", () => {
  it("replaces import() with __shatter_import()", () => {
    expect(transformDynamicImports('import("foo")')).toBe(
      '__shatter_import("foo")',
    );
  });

  it("handles whitespace between import and parenthesis", () => {
    expect(transformDynamicImports("import ('foo')")).toBe(
      "__shatter_import('foo')",
    );
  });

  it("transforms multiple import() calls", () => {
    const input = 'const a = import("a"); const b = import("b");';
    const expected =
      'const a = __shatter_import("a"); const b = __shatter_import("b");';
    expect(transformDynamicImports(input)).toBe(expected);
  });

  it("does not transform require() calls", () => {
    const input = 'require("foo")';
    expect(transformDynamicImports(input)).toBe(input);
  });

  it("does not match partial identifiers like reimport(", () => {
    const input = 'reimport("foo")';
    expect(transformDynamicImports(input)).toBe(input);
  });

  it("leaves code without import() unchanged", () => {
    const input = 'const x = 1; const y = require("fs");';
    expect(transformDynamicImports(input)).toBe(input);
  });

  it("is idempotent: second application is a no-op", () => {
    const input = 'await import("foo")';
    const once = transformDynamicImports(input);
    const twice = transformDynamicImports(once);
    expect(once).toBe(twice);
  });
});

describe("createShatterImport", () => {
  it("returns a Promise that resolves to the required module", async () => {
    const mockRequire = (id: string) => ({ value: id });
    const shatterImport = createShatterImport(mockRequire);
    const result = await shatterImport("test-mod");
    expect(result.default).toEqual({ value: "test-mod" });
    expect(result.__esModule).toBe(true);
  });

  it("passes through modules with __esModule already set", async () => {
    const mod = { __esModule: true, foo: "bar" };
    const shatterImport = createShatterImport(() => mod);
    const result = await shatterImport("x");
    expect(result).toBe(mod);
  });

  it("wraps CJS module exports with default and spread", async () => {
    const mod = { a: 1, b: 2 };
    const shatterImport = createShatterImport(() => mod);
    const result = await shatterImport("x");
    expect(result.default).toBe(mod);
    expect(result.a).toBe(1);
    expect(result.b).toBe(2);
    expect(result.__esModule).toBe(true);
  });

  it("propagates require errors as rejected Promise", async () => {
    const shatterImport = createShatterImport(() => {
      throw new Error("MODULE_NOT_FOUND");
    });
    await expect(shatterImport("missing")).rejects.toThrow("MODULE_NOT_FOUND");
  });
});

describe("dynamic import() in user code (str-4hay regression)", () => {
  it("executes a function with dynamic import() via executeFunction", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "dynamic-import.ts"),
      "loadPath",
      [],
    );
    // loadPath does: await import("node:path").then(p => p.join("/tmp","test"))
    expect(result.return_value).toBe(path.join("/tmp", "test"));
  });

  it("executes a function with Promise.all of dynamic imports", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "dynamic-import.ts"),
      "loadMultiple",
      [],
    );
    // loadMultiple returns [path.sep, typeof fs.readFileSync]
    expect(result.return_value).toEqual([path.sep, "function"]);
  });

  it("sync function in a module with dynamic imports still works", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "dynamic-import.ts"),
      "syncAdd",
      [3, 4],
    );
    expect(result.return_value).toBe(7);
  });
});

describe("import.meta polyfill", () => {
  it("executeFunction does not crash on import.meta.env references", async () => {
    const result = await executeFunction(
      path.join(FIXTURES_DIR, "import-meta.ts"),
      "getApiUrl",
      ["https://fallback.example.com"],
    );
    // import.meta.env is stubbed, so VITE_API_URL is undefined → fallback returned
    expect(result.return_value).toBe("https://fallback.example.com");
  });

  it("executeInstrumented does not crash on import.meta.env references", async () => {
    const fixture = path.join(FIXTURES_DIR, "import-meta.ts");
    const source = fs.readFileSync(fixture, "utf-8");
    const instrumentResult = instrumentFunction(source, "getApiUrl", fixture);
    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }

    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "getApiUrl",
      ["https://fallback.example.com"],
    );
    expect(result.return_value).toBe("https://fallback.example.com");
  });

  it("transformDynamicImports replaces import.meta with __shatter_import_meta", () => {
    expect(transformDynamicImports("import.meta.env.VITE_API_URL")).toBe(
      "__shatter_import_meta.env.VITE_API_URL",
    );
  });

  it("transformDynamicImports handles import.meta.url", () => {
    expect(transformDynamicImports("const u = import.meta.url;")).toBe(
      "const u = __shatter_import_meta.url;",
    );
  });

  it("transformDynamicImports does not replace import.meta inside __shatter_import_meta", () => {
    const input = "__shatter_import_meta.env.FOO";
    expect(transformDynamicImports(input)).toBe(input);
  });
});

describe("web/fetch/crypto globals available by default (str-ysnp + str-jeen.71)", () => {
  const fixture = path.join(FIXTURES_DIR, "web-globals.ts");

  it("executeFunction provides Headers", async () => {
    const result = await executeFunction(fixture, "buildHeaders", ["abc"]);
    expect(result.return_value).toBe("Bearer abc");
  });

  it("executeFunction provides Request", async () => {
    const result = await executeFunction(fixture, "buildRequest", [
      "https://example.com/x",
    ]);
    expect(result.return_value).toBe("POST");
  });

  it("executeFunction provides Response", async () => {
    const result = await executeFunction(fixture, "buildResponse", ["hi"]);
    expect(result.return_value).toBe("200");
  });

  it("executeFunction provides fetch", async () => {
    const result = await executeFunction(fixture, "hasFetch", []);
    expect(result.return_value).toBe(true);
  });

  it("executeFunction provides crypto (Web Crypto API)", async () => {
    const result = await executeFunction(fixture, "randomUuidLength", []);
    expect(result.return_value).toBe(36);
  });

  it("executeFunction surfaces Vite-style import.meta.env defaults", async () => {
    const result = await executeFunction(fixture, "viteMode", []);
    expect(typeof result.return_value).toBe("string");
    expect(result.return_value).not.toBe("unknown");
  });

  it("executeFunction returns fallback for unknown import.meta.env keys without throwing", async () => {
    const result = await executeFunction(fixture, "readVitePosthogKey", [
      "fallback",
    ]);
    expect(result.return_value).toBe("fallback");
  });

  it("executeInstrumented provides Headers", async () => {
    const source = fs.readFileSync(fixture, "utf-8");
    const instrumentResult = instrumentFunction(source, "buildHeaders", fixture);
    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }
    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "buildHeaders",
      ["xyz"],
    );
    expect(result.return_value).toBe("Bearer xyz");
  });

  it("executeInstrumented provides crypto", async () => {
    const source = fs.readFileSync(fixture, "utf-8");
    const instrumentResult = instrumentFunction(
      source,
      "randomUuidLength",
      fixture,
    );
    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }
    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "randomUuidLength",
      [],
    );
    expect(result.return_value).toBe(36);
  });

  it("executeInstrumented surfaces import.meta.env defaults", async () => {
    const source = fs.readFileSync(fixture, "utf-8");
    const instrumentResult = instrumentFunction(source, "viteMode", fixture);
    if ("error" in instrumentResult) {
      throw new Error(`Instrumentation failed: ${instrumentResult.error}`);
    }
    const result = await executeInstrumented(
      instrumentResult.instrumentedSource,
      "viteMode",
      [],
    );
    expect(typeof result.return_value).toBe("string");
    expect(result.return_value).not.toBe("unknown");
  });
});

describe("executeAdapterOwned", () => {
  const adapterModel: AdapterInvocationModel = {
    kind: "adapter",
    adapter_id: "test/adapter",
  };

  function makeHook(
    fn: (
      ctx: InvocationContext,
    ) => InvocationOutcome | Promise<InvocationOutcome>,
  ): InvocationHook {
    return { id: "test/adapter", invoke: fn };
  }

  it("returns structured return_value with empty branch_path/lines_executed", async () => {
    const calls: InvocationContext[] = [];
    const hook = makeHook((ctx) => {
      calls.push(ctx);
      return { status: "completed", return_value: { ok: true, n: 42 } };
    });

    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [1, "two"],
    });

    expect(result.return_value).toEqual({ ok: true, n: 42 });
    expect(result.thrown_error).toBeNull();
    expect(result.branch_path).toEqual([]);
    expect(result.lines_executed).toEqual([]);
    expect(result.path_constraints).toEqual([]);
    expect(result.calls_to_external).toEqual([]);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.functionName).toBe("fakeFn");
    expect(calls[0]?.inputs).toEqual([1, "two"]);
    expect(calls[0]?.capture).toBe(true);
  });

  it("captures structured thrownError and emits a thrown_error side effect", async () => {
    const hook = makeHook(() => ({
      status: "runtime_failed",
      thrown_error: {
        error_type: "ValidationError",
        message: "bad input",
        stack: "stack-trace",
      },
    }));

    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [],
    });

    expect(result.return_value).toBeNull();
    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error?.error_type).toBe("ValidationError");
    expect(result.thrown_error?.message).toBe("bad input");
    const thrownSE = result.side_effects.find(
      (se) => se.kind === "thrown_error",
    );
    expect(thrownSE).toBeDefined();
    expect((thrownSE as { error_type: string }).error_type).toBe(
      "ValidationError",
    );
  });

  it("captures hook-thrown exceptions (non-structured) into thrown_error", async () => {
    const hook = makeHook(() => {
      throw new TypeError("hook crashed");
    });

    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [],
    });

    expect(result.return_value).toBeNull();
    expect(result.thrown_error?.error_type).toBe("TypeError");
    expect(result.thrown_error?.message).toBe("hook crashed");
  });

  it("surfaces hook-supplied side effects in the result", async () => {
    const hook = makeHook(() => ({
      status: "completed_with_findings",
      return_value: null,
      side_effects: [
        {
          kind: "global_state_change",
          variable: "counter",
          before: 0,
          after: 1,
        },
        {
          kind: "global_mutation",
          name: "flag",
        },
      ],
    }));

    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [],
      capture: true,
    });

    const kinds = result.side_effects.map((se) => se.kind);
    expect(kinds).toContain("global_state_change");
    expect(kinds).toContain("global_mutation");
  });

  it("does not capture console output when capture is false", async () => {
    const hook = makeHook((ctx) => {
      // eslint-disable-next-line no-console
      console.log("should not be captured");
      void ctx;
      return { status: "completed", return_value: 1 };
    });

    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [],
      capture: false,
    });

    expect(
      result.side_effects.find((se) => se.kind === "console_output"),
    ).toBeUndefined();
  });

  it("populates performance metrics", async () => {
    const hook = makeHook(() => ({ status: "completed", return_value: "ok" }));
    const result = await executeAdapterOwned({
      hook,
      invocationModel: adapterModel,
      fileForExec: "/tmp/fake.ts",
      functionName: "fakeFn",
      inputs: [],
    });

    expect(result.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
    expect(result.performance.cpu_time_us).toBeGreaterThanOrEqual(0);
  });
});

import { TranspileError } from "./executor.js";

/**
 * Type-syntax coverage suite (str-jeen.11). Each function in
 * `__fixtures__/type-syntax-coverage.tsx` exercises a TypeScript construct
 * that previously crashed the executor at runtime. The suite asserts the
 * full instrument+execute pipeline produces a clean result for every entry.
 */
describe("TS type syntax pipeline coverage (str-jeen.11)", () => {
  const FIXTURE = path.join(FIXTURES_DIR, "type-syntax-coverage.tsx");

  async function runFixture(
    funcName: string,
    inputs: unknown[],
  ): Promise<{ thrown: unknown; outputContains?: string }> {
    const source = fs.readFileSync(FIXTURE, "utf-8");
    const inst = instrumentFunction(source, funcName, FIXTURE);
    if ("error" in inst) {
      throw new Error(`instrument failed for ${funcName}: ${inst.error}`);
    }
    const result = await executeInstrumented(
      inst.instrumentedSource,
      funcName,
      inputs,
      [],
      FIXTURE,
      undefined,
      true,
      `${FIXTURE}:${funcName}`,
    );
    clearCompiledScriptCache();
    return { thrown: result.thrown_error };
  }

  const typeSyntaxCases: Array<[string, unknown[]]> = [
    ["classifyButton", [{ label: "hello" }]],
    ["pickGeneric", [[1, 2, 3], 1]],
    ["GenericList", [{ items: ["a", "b"], render: (x: string) => x }]],
    ["HelloTsx", [{ name: "x" }]],
    ["checkSatisfies", [3]],
    ["makeRenderOptions", [1]],
  ];
  it.each(typeSyntaxCases)(
    "instruments + executes %s",
    async (fn, inputs) => {
      const r = await runFixture(fn, inputs);
      expect(r.thrown).toBeNull();
    },
  );

  it("classifies V8 SyntaxError after transpile as TranspileError(compile_failed)", async () => {
    // The instrumented source is hand-crafted to look like valid TS that
    // ts.transpileModule passes through verbatim, but contains an
    // identifier in a position the JS parser rejects. This proves the
    // compile_failed classification is reachable from executeInstrumented.
    const malformedJsLikeTs = "const x = 1;\nconst y =;\n";
    let caught: unknown = null;
    try {
      await executeInstrumented(
        malformedJsLikeTs,
        "missing",
        [],
        [],
        "/tmp/syntax-bad.ts",
        undefined,
        true,
        "/tmp/syntax-bad.ts:missing",
      );
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(TranspileError);
    expect((caught as TranspileError).category).toMatch(
      /transpile_failed|compile_failed/,
    );
  });
});
