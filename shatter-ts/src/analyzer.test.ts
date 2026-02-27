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
      expect(fn.params[0]!.type).toEqual({ kind: "float" });
      expect(fn.params[1]!.name).toBe("b");
      expect(fn.params[1]!.type).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts string params and return type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "greet");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.name).toBe("name");
      expect(fn.params[0]!.type).toEqual({ kind: "str" });
      expect(fn.return_type).toEqual({ kind: "str" });
    });

    it("extracts boolean return type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "isPositive");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "bool" });
    });

    it("extracts bigint as complex big_int type", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "identity");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "complex", complex_kind: "big_int" });
      expect(fn.return_type).toEqual({ kind: "complex", complex_kind: "big_int" });
    });
  });

  describe("array types", () => {
    it("extracts number array parameter", () => {
      const results = analyzeFile(path.join(fixtures, "arrays.ts"), "sum");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "array", element: { kind: "float" } });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts nested array types", () => {
      const results = analyzeFile(path.join(fixtures, "arrays.ts"), "flatten");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
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
      expect(fn.params[0]!.type).toEqual(expectedPoint);
      expect(fn.params[1]!.type).toEqual(expectedPoint);
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
      expect(fn.params[0]!.type).toEqual({
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
      expect(fn.params[0]!.type).toEqual({
        kind: "union",
        variants: [{ kind: "str" }, { kind: "float" }],
      });
    });

    it("extracts nullable type (T | null)", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "nullable");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "nullable",
        inner: { kind: "float" },
      });
    });

    it("extracts optional parameter as nullable", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "optional");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "nullable",
        inner: { kind: "float" },
      });
    });

    it("extracts T | undefined as nullable", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "undefinable");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "nullable",
        inner: { kind: "str" },
      });
    });

    it("extracts complex nullable union (string | number | null)", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "complex");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
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
      expect(fn.params[0]!.type).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("extracts multi-param arrow function", () => {
      const results = analyzeFile(path.join(fixtures, "arrows.ts"), "concat");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params).toHaveLength(2);
      expect(fn.params[0]!.type).toEqual({ kind: "str" });
      expect(fn.params[1]!.type).toEqual({ kind: "str" });
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

  describe("functions without branches have empty branches", () => {
    it("returns empty branches for simple function", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      const fn = results[0]!;
      expect(fn.branches).toEqual([]);
    });
  });

  describe("branch extraction", () => {
    it("extracts a single if branch", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "simpleIf");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.id).toBe(0);
      expect(fn.branches[0]!.branch_type).toBe("if");
      expect(fn.branches[0]!.condition_text).toBe("x > 0");
      expect(fn.branches[0]!.condition).toEqual({
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "x", path: [] },
        right: { kind: "const", type: "int", value: 0 },
      });
    });

    it("extracts if/else-if as separate branches", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "ifElseIf");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(2);
      expect(fn.branches[0]!.branch_type).toBe("if");
      expect(fn.branches[0]!.condition_text).toBe("x > 0");
      expect(fn.branches[1]!.branch_type).toBe("else_if");
      expect(fn.branches[1]!.condition_text).toBe("x < 0");
    });

    it("extracts switch cases", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "switchCase");
      const fn = results[0]!;
      // Two case clauses (default is not a case clause)
      expect(fn.branches).toHaveLength(2);
      expect(fn.branches[0]!.branch_type).toBe("switch");
      expect(fn.branches[0]!.condition_text).toBe("x === 1");
      expect(fn.branches[1]!.branch_type).toBe("switch");
      expect(fn.branches[1]!.condition_text).toBe("x === 2");
    });

    it("extracts ternary expressions", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "ternary");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.branch_type).toBe("ternary");
      expect(fn.branches[0]!.condition_text).toBe("x > 0");
    });

    it("extracts logical AND short-circuit", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "logicalAnd");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.branch_type).toBe("logical_and");
      expect(fn.branches[0]!.condition_text).toContain("&&");
    });

    it("extracts logical OR short-circuit", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "logicalOr");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.branch_type).toBe("logical_or");
      expect(fn.branches[0]!.condition_text).toContain("||");
    });

    it("extracts nested if branches", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "nestedBranches");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(2);
      expect(fn.branches[0]!.condition_text).toBe("x > 0");
      expect(fn.branches[1]!.condition_text).toBe("y > 0");
    });

    it("extracts while loop branches", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "whileLoop");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.branch_type).toBe("while");
      expect(fn.branches[0]!.condition_text).toBe("i < x");
    });

    it("extracts for loop branches", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoop");
      const fn = results[0]!;
      expect(fn.branches).toHaveLength(1);
      expect(fn.branches[0]!.branch_type).toBe("for");
      expect(fn.branches[0]!.condition_text).toBe("i < x");
    });

    it("assigns sequential IDs across mixed branch types", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "mixedBranches");
      const fn = results[0]!;
      // if, 2 switch cases, ternary = 4 branches
      expect(fn.branches).toHaveLength(4);
      const ids = fn.branches.map((b) => b.id);
      expect(ids).toEqual([0, 1, 2, 3]);
    });

    it("includes symbolic condition for param-based conditions", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "simpleIf");
      const fn = results[0]!;
      expect(fn.branches[0]!.condition).not.toBeNull();
      expect(fn.branches[0]!.condition!.kind).toBe("bin_op");
    });
  });

  describe("opaque Node.js types", () => {
    it("emits opaque for net.Socket parameter", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleSocket");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "net.Socket" });
    });

    it("emits opaque for net.Server parameter", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleNetServer");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "net.Server" });
    });

    it("emits opaque for http.IncomingMessage and http.ServerResponse", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleHttp");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "http.IncomingMessage" });
      expect(fn.params[1]!.type).toEqual({ kind: "opaque", label: "http.ServerResponse" });
    });

    it("emits opaque for stream.Readable and stream.Writable", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleStreams");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "stream.Readable" });
      expect(fn.params[1]!.type).toEqual({ kind: "opaque", label: "stream.Writable" });
    });

    it("emits opaque for stream.Transform and stream.Duplex", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleTransformDuplex");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "stream.Transform" });
      expect(fn.params[1]!.type).toEqual({ kind: "opaque", label: "stream.Duplex" });
    });

    it("emits opaque for child_process.ChildProcess", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleChildProcess");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "child_process.ChildProcess" });
    });

    it("emits opaque for fs.ReadStream and fs.WriteStream", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-node-types.ts"), "handleFsStreams");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "opaque", label: "fs.ReadStream" });
      expect(fn.params[1]!.type).toEqual({ kind: "opaque", label: "fs.WriteStream" });
    });

    it("does NOT emit opaque for user-defined Socket class", () => {
      const results = analyzeFile(path.join(fixtures, "opaque-user-types.ts"), "handleUserSocket");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type.kind).not.toBe("opaque");
    });
  });

  describe("dependency detection", () => {
    it("returns empty dependencies for function without external calls", () => {
      const results = analyzeFile(path.join(fixtures, "dependencies.ts"), "noExternalDeps");
      const fn = results[0]!;
      expect(fn.dependencies).toEqual([]);
    });

    it("detects imported function calls as external dependencies", () => {
      const results = analyzeFile(path.join(fixtures, "dependencies.ts"), "usesExternal");
      const fn = results[0]!;
      expect(fn.dependencies.length).toBeGreaterThanOrEqual(2);
      const symbols = fn.dependencies.map((d) => d.symbol);
      expect(symbols).toContain("helperAdd");
      expect(symbols).toContain("helperFormat");
    });

    it("groups multiple calls to same function into one dependency", () => {
      const results = analyzeFile(path.join(fixtures, "dependencies.ts"), "usesExternalMultipleTimes");
      const fn = results[0]!;
      const helperAddDep = fn.dependencies.find((d) => d.symbol === "helperAdd");
      expect(helperAddDep).toBeDefined();
      expect(helperAddDep!.call_sites).toHaveLength(2);
    });

    it("includes source_module for external dependencies", () => {
      const results = analyzeFile(path.join(fixtures, "dependencies.ts"), "usesExternal");
      const fn = results[0]!;
      const helperAddDep = fn.dependencies.find((d) => d.symbol === "helperAdd");
      expect(helperAddDep).toBeDefined();
      expect(helperAddDep!.source_module).toContain("deps-helper");
    });

    it("includes return type and param types for dependencies", () => {
      const results = analyzeFile(path.join(fixtures, "dependencies.ts"), "usesExternal");
      const fn = results[0]!;
      const helperAddDep = fn.dependencies.find((d) => d.symbol === "helperAdd");
      expect(helperAddDep).toBeDefined();
      expect(helperAddDep!.return_type).toEqual({ kind: "float" });
      expect(helperAddDep!.param_types).toEqual([{ kind: "float" }, { kind: "float" }]);
    });

    it("returns empty dependencies for simple arithmetic function", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      const fn = results[0]!;
      expect(fn.dependencies).toEqual([]);
    });
  });
});
