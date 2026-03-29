import * as path from "node:path";
import { analyzeFile } from "./analyzer.js";
import { countIntegerSignals, refineIntegerParams } from "./integer-heuristic.js";
import type { ParamInfo } from "./protocol.js";

const fixtures = path.join(__dirname, "__fixtures__");
const heuristicFixture = path.join(fixtures, "integer-heuristic.ts");

describe("integer heuristic via analyzeFile", () => {
  it("refines param with comparison + naming signals", () => {
    const results = analyzeFile(heuristicFixture, "factorial");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("count");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });

  it("does not refine with only naming signal (needs 2)", () => {
    const results = analyzeFile(heuristicFixture, "getAtIndex");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("index");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("does not refine when fractional veto fires (.toFixed)", () => {
    const results = analyzeFile(heuristicFixture, "formatCurrency");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("amount");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("refines param with comparison + coercion + naming signals", () => {
    const results = analyzeFile(heuristicFixture, "paginate");
    expect(results).toHaveLength(1);
    // pageIndex has naming (page + index) + comparison (< 0)
    expect(results[0]!.params[0]!.name).toBe("pageIndex");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });

  it("does not refine with only coercion signal (needs 2)", () => {
    const results = analyzeFile(heuristicFixture, "truncateValue");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("value");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("refines param with comparison + bitwise coercion signals", () => {
    const results = analyzeFile(heuristicFixture, "bitwiseCoerce");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("n");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });

  it("refines param with comparison + naming signals (countdown)", () => {
    const results = analyzeFile(heuristicFixture, "countdown");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("count");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });

  it("does not refine when fractional veto fires (% 1)", () => {
    const results = analyzeFile(heuristicFixture, "isWhole");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("does not refine when fractional veto fires (Math.round)", () => {
    const results = analyzeFile(heuristicFixture, "roundIt");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("refines param with JSDoc + comparison signals", () => {
    const results = analyzeFile(heuristicFixture, "jsdocInteger");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("n");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });

  it("does not refine params with no signals", () => {
    const results = analyzeFile(heuristicFixture, "multiply");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
    expect(results[0]!.params[1]!.type).toEqual({ kind: "float" });
  });

  it("does not refine with only comparison signal (no naming match for n)", () => {
    const results = analyzeFile(heuristicFixture, "clampValue");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("n");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });

  it("refines param with bitwise coercion + naming signals", () => {
    const results = analyzeFile(heuristicFixture, "coerceAndUse");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("num");
    expect(results[0]!.params[0]!.type).toEqual({ kind: "int" });
  });
});

describe("refineIntegerParams edge cases", () => {
  it("returns params unchanged when body is undefined", () => {
    const params: ParamInfo[] = [
      { name: "count", type: { kind: "float" } },
      { name: "label", type: { kind: "str" } },
    ];
    const result = refineIntegerParams(params, undefined, undefined as never);
    expect(result).toBe(params);
  });

  it("never changes non-float params", () => {
    const params: ParamInfo[] = [
      { name: "count", type: { kind: "str" } },
      { name: "index", type: { kind: "bool" } },
      { name: "size", type: { kind: "int" } },
      { name: "length", type: { kind: "unknown" } },
    ];
    // Even with integer-suggestive names, non-float types should not be touched
    const result = refineIntegerParams(params, undefined, undefined as never);
    expect(result).toEqual(params);
  });
});

describe("existing analyzer tests remain unaffected", () => {
  it("add(a, b) still returns float params", () => {
    const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
    expect(results[0]!.params[1]!.type).toEqual({ kind: "float" });
  });

  it("isPositive(n) still returns float param", () => {
    const results = analyzeFile(path.join(fixtures, "primitives.ts"), "isPositive");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({ kind: "float" });
  });
});
