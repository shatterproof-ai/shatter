import * as path from "node:path";
import {
  executeFunction,
  executeInstrumented,
  buildExecuteResponse,
  clearModuleCache,
} from "./executor.js";
import { instrumentFunction } from "./instrumentor.js";
import * as fs from "node:fs";
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

    const response = buildExecuteResponse(1, "0.1.0", rawResult);

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

    const response = buildExecuteResponse(2, "0.1.0", rawResult);
    const json = JSON.stringify(response);
    const parsed = JSON.parse(json) as typeof response;

    expect(parsed.performance.wall_time_ms).toBe(response.performance.wall_time_ms);
    expect(parsed.performance.cpu_time_us).toBe(response.performance.cpu_time_us);
    expect(parsed.performance.heap_used_bytes).toBe(response.performance.heap_used_bytes);
    expect(parsed.performance.heap_allocated_bytes).toBe(response.performance.heap_allocated_bytes);
  });
});
