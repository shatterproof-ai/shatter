import * as path from "node:path";
import { analyzeFile } from "./analyzer.js";
import type { TypeInfo } from "./protocol.js";

const fixtures = path.join(__dirname, "__fixtures__");

describe("analyzeFile", () => {
  describe("primitive types", () => {
    it("extracts number params and return type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("add");
      expect(fn.params).toHaveLength(2);
      expect(fn.params[0]!.name).toBe("a");
      expect(fn.params[0]!.typ).toEqual({ kind: "float" });
      expect(fn.params[1]!.name).toBe("b");
      expect(fn.params[1]!.typ).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts string params and return type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "greet");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.name).toBe("name");
      expect(fn.params[0]!.typ).toEqual({ kind: "str" });
      expect(fn.return_type).toEqual({ kind: "str" });
    });

    it("extracts boolean return type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "isPositive");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "bool" });
    });

    it("extracts bigint as int type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "identity");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({ kind: "int" });
      expect(fn.return_type).toEqual({ kind: "int" });
    });
  });

  describe("array types", () => {
    it("extracts number array parameter", () => {
      const results = analyzeFile(path.join(fixtures, "arrays.ts"), "sum");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({ kind: "array", element: { kind: "float" } });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts nested array types", () => {
      const results = analyzeFile(path.join(fixtures, "arrays.ts"), "flatten");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "array",
        element: { kind: "array", element: { kind: "str" } },
      });
      expect(fn.return_type).toEqual({ kind: "array", element: { kind: "str" } });
    });
  });

  describe("object types", () => {
    it("extracts interface-typed parameter as object", () => {
      const results = analyzeFile(path.join(fixtures, "objects.ts"), "distance");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      const expectedPoint: TypeInfo = {
        kind: "object",
        fields: [["x", { kind: "float" }], ["y", { kind: "float" }]],
      };
      expect(fn.params[0]!.typ).toEqual(expectedPoint);
      expect(fn.params[1]!.typ).toEqual(expectedPoint);
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts interface return type as object", () => {
      const results = analyzeFile(path.join(fixtures, "objects.ts"), "makePoint");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.return_type).toEqual({
        kind: "object",
        fields: [["x", { kind: "float" }], ["y", { kind: "float" }]],
      });
    });

    it("extracts inline object type", () => {
      const results = analyzeFile(path.join(fixtures, "objects.ts"), "getLabel");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "object",
        fields: [["name", { kind: "str" }], ["count", { kind: "float" }]],
      });
    });
  });

  describe("union and nullable types", () => {
    it("extracts string | number union", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "format");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "union",
        variants: [{ kind: "str" }, { kind: "float" }],
      });
    });

    it("extracts nullable type (T | null)", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "nullable");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "nullable",
        inner: { kind: "float" },
      });
    });

    it("extracts optional parameter as nullable", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "optional");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "nullable",
        inner: { kind: "float" },
      });
    });

    it("extracts T | undefined as nullable", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "undefinable");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "nullable",
        inner: { kind: "str" },
      });
    });

    it("extracts complex nullable union (string | number | null)", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "complex");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.typ).toEqual({
        kind: "nullable",
        inner: { kind: "union", variants: [{ kind: "str" }, { kind: "float" }] },
      });
    });
  });

  describe("arrow functions", () => {
    it("extracts arrow function parameter and return types", () => {
      const results = analyzeFile(path.join(fixtures, "arrows.ts"), "double");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("double");
      expect(fn.params[0]!.typ).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts multi-param arrow function", () => {
      const results = analyzeFile(path.join(fixtures, "arrows.ts"), "concat");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params).toHaveLength(2);
      expect(fn.params[0]!.typ).toEqual({ kind: "str" });
      expect(fn.params[1]!.typ).toEqual({ kind: "str" });
    });
  });

  describe("source location", () => {
    it("reports correct start and end lines", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.start_line).toBe(1);
      expect(fn.end_line).toBe(3);
    });

    it("reports correct lines for later functions", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "greet");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.start_line).toBe(5);
      expect(fn.end_line).toBe(7);
    });
  });

  describe("file-level analysis", () => {
    it("returns all functions when no function name specified", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"));
      expect(results).toHaveLength(4);
      const names = results.map((f) => f.name);
      expect(names).toEqual(["add", "greet", "isPositive", "identity"]);
    });

    it("returns empty array for nonexistent file", () => {
      const results = analyzeFile(path.join(fixtures, "nonexistent.ts"));
      expect(results).toEqual([]);
    });

    it("returns empty array for nonexistent function", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "nonexistent");
      expect(results).toEqual([]);
    });
  });

  describe("branches and dependencies are empty stubs", () => {
    it("returns empty branches and dependencies", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      const fn = results[0]!;
      expect(fn.branches).toEqual([]);
      expect(fn.dependencies).toEqual([]);
    });
  });
});
