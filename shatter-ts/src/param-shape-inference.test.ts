import * as path from "node:path";
import * as ts from "typescript";
import fc from "fast-check";
import { analyzeFile } from "./analyzer.js";
import {
  collectParamShapes,
  refineParamShapesFromBody,
} from "./param-shape-inference.js";
import type { ParamInfo, TypeInfo } from "./protocol.js";

const fixtureFile = path.join(__dirname, "__fixtures__", "typed-input-shapes.ts");

/** Compile `src` to a function body node for unit testing. */
function bodyOf(src: string): { body: ts.Block; paramNames: Set<string> } {
  const sf = ts.createSourceFile("test.ts", src, ts.ScriptTarget.Latest, true);
  const fn = sf.statements.find(ts.isFunctionDeclaration);
  if (!fn || !fn.body) throw new Error("test source must declare a function with a body");
  const paramNames = new Set(
    fn.parameters
      .map((p) => (ts.isIdentifier(p.name) ? p.name.text : null))
      .filter((n): n is string => n !== null),
  );
  return { body: fn.body, paramNames };
}

/** Build a minimal ParamInfo[] for refineParamShapesFromBody. */
function paramsOf(...names: string[]): ParamInfo[] {
  return names.map((name) => ({ name, type: { kind: "unknown" } as TypeInfo }));
}

describe("param-shape-inference: collectParamShapes", () => {
  it("marks param as array on .filter method", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { return items.filter(x => x > 0); }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("items")?.isArray).toBe(true);
  });

  it("marks nested property as array on .filter at path", () => {
    const { body, paramNames } = bodyOf(
      "function f(data) { return data.rows.filter(r => r); }",
    );
    const trie = collectParamShapes(body, paramNames);
    const data = trie.get("data");
    expect(data).toBeDefined();
    expect(data!.isArray).toBe(false);
    expect(data!.children.get("rows")?.isArray).toBe(true);
  });

  it("marks param as array on for-of", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { for (const x of items) { x; } }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("items")?.isArray).toBe(true);
  });

  it("marks param as array on array spread", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { return [...items]; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("items")?.isArray).toBe(true);
  });

  it("marks param as array on array destructuring", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { const [a, b] = items; return a; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("items")?.isArray).toBe(true);
  });

  it("marks param as array on element access", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { return items[0]; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("items")?.isArray).toBe(true);
  });

  it("marks nested property as array on element access at path", () => {
    const { body, paramNames } = bodyOf(
      "function f(data) { return data.rows[0]; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("data")?.children.get("rows")?.isArray).toBe(true);
  });

  it("registers plain property access as object field with no array signal", () => {
    const { body, paramNames } = bodyOf(
      "function f(obj) { return obj.label; }",
    );
    const trie = collectParamShapes(body, paramNames);
    const obj = trie.get("obj");
    expect(obj).toBeDefined();
    expect(obj!.children.has("label")).toBe(true);
    expect(obj!.children.get("label")?.isArray).toBe(false);
  });

  it("ignores chains rooted at non-parameter identifiers", () => {
    const { body, paramNames } = bodyOf(
      "function f(_x) { const local = {}; return local.foo; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.has("local")).toBe(false);
  });

  it("ignores accesses inside nested function bodies (param shadowing safety)", () => {
    const { body, paramNames } = bodyOf(
      "function f(items) { return items.filter((items) => items.foo); }",
    );
    const trie = collectParamShapes(body, paramNames);
    // Outer `items.filter` fires; inner arrow's `items.foo` is skipped.
    const items = trie.get("items");
    expect(items?.isArray).toBe(true);
    expect(items?.children.has("foo")).toBe(false);
  });

  it(".length alone marks as array (ambiguous but high-leverage)", () => {
    const { body, paramNames } = bodyOf(
      "function f(value) { return value.length; }",
    );
    const trie = collectParamShapes(body, paramNames);
    expect(trie.get("value")?.isArray).toBe(true);
  });
});

describe("param-shape-inference: refineParamShapesFromBody", () => {
  it("refines unknown param with array signal to array TypeInfo", () => {
    const { body } = bodyOf(
      "function f(items) { return items.filter(x => x); }",
    );
    const out = refineParamShapesFromBody(paramsOf("items"), body);
    expect(out[0]!.type).toEqual({
      kind: "array",
      element: { kind: "unknown" },
    });
  });

  it("refines unknown param with nested array field to object containing array", () => {
    const { body } = bodyOf(
      "function f(data) { return data.rows.filter(r => r); }",
    );
    const out = refineParamShapesFromBody(paramsOf("data"), body);
    expect(out[0]!.type).toEqual({
      kind: "object",
      fields: [["rows", { kind: "array", element: { kind: "unknown" } }]],
    });
  });

  it("does not refine when param is already array-typed", () => {
    const { body } = bodyOf(
      "function f(nums) { return nums.filter(x => x); }",
    );
    const existing: ParamInfo[] = [
      { name: "nums", type: { kind: "array", element: { kind: "float" } } },
    ];
    const out = refineParamShapesFromBody(existing, body);
    expect(out[0]!.type).toEqual({
      kind: "array",
      element: { kind: "float" },
    });
  });

  it("does not refine when param is already a structured object", () => {
    const { body } = bodyOf(
      "function f(obj) { return obj.label; }",
    );
    const existing: ParamInfo[] = [
      {
        name: "obj",
        type: { kind: "object", fields: [["label", { kind: "str" }]] },
      },
    ];
    const out = refineParamShapesFromBody(existing, body);
    expect(out[0]!.type).toEqual({
      kind: "object",
      fields: [["label", { kind: "str" }]],
    });
  });

  it("does not refine when param is a primitive", () => {
    const { body } = bodyOf(
      "function f(n) { return n + 1; }",
    );
    const existing: ParamInfo[] = [{ name: "n", type: { kind: "float" } }];
    const out = refineParamShapesFromBody(existing, body);
    expect(out[0]!.type).toEqual({ kind: "float" });
  });

  it("does not refine opaque param (opacity is an intentional skip signal)", () => {
    const { body } = bodyOf(
      "function f(handle) { return handle.filter(x => x); }",
    );
    const existing: ParamInfo[] = [
      { name: "handle", type: { kind: "opaque", label: "SomeHandle" } },
    ];
    const out = refineParamShapesFromBody(existing, body);
    expect(out[0]!.type).toEqual({ kind: "opaque", label: "SomeHandle" });
  });

  it("leaves param alone when body has no usage of it", () => {
    const { body } = bodyOf(
      "function f(unused) { return 42; }",
    );
    const out = refineParamShapesFromBody(paramsOf("unused"), body);
    expect(out[0]!.type).toEqual({ kind: "unknown" });
  });

  it("leaves param alone when usage is purely empty (only the identifier used as value)", () => {
    const { body } = bodyOf(
      "function f(items) { return items; }",
    );
    const out = refineParamShapesFromBody(paramsOf("items"), body);
    // The identifier `items` was never accessed structurally; the trie
    // entry doesn't exist, so the param stays unknown.
    expect(out[0]!.type).toEqual({ kind: "unknown" });
  });
});

describe("param-shape-inference: analyzeFile integration", () => {
  it("refines `any`-typed param to array on .filter usage (sumPositives)", () => {
    const results = analyzeFile(fixtureFile, "sumPositives");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.name).toBe("items");
    expect(results[0]!.params[0]!.type).toEqual({
      kind: "array",
      element: { kind: "unknown" },
    });
  });

  it("refines `any`-typed param to object with nested array (countRows)", () => {
    const results = analyzeFile(fixtureFile, "countRows");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({
      kind: "object",
      fields: [["rows", { kind: "array", element: { kind: "unknown" } }]],
    });
  });

  it("refines on for-of usage (joinNames)", () => {
    const results = analyzeFile(fixtureFile, "joinNames");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type.kind).toBe("array");
  });

  it("refines on array spread (spreadCopy)", () => {
    const results = analyzeFile(fixtureFile, "spreadCopy");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type.kind).toBe("array");
  });

  it("refines on array destructuring (firstTwo)", () => {
    const results = analyzeFile(fixtureFile, "firstTwo");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type.kind).toBe("array");
  });

  it("refines object with multiple array fields (processReport)", () => {
    const results = analyzeFile(fixtureFile, "processReport");
    expect(results).toHaveLength(1);
    const type = results[0]!.params[0]!.type;
    expect(type.kind).toBe("object");
    if (type.kind !== "object") throw new Error("expected object");
    const byName = new Map(type.fields);
    expect(byName.get("rows")).toEqual({
      kind: "array",
      element: { kind: "unknown" },
    });
    expect(byName.get("items")).toEqual({
      kind: "array",
      element: { kind: "unknown" },
    });
  });

  it("leaves explicitly typed array param alone (sumExplicit)", () => {
    const results = analyzeFile(fixtureFile, "sumExplicit");
    expect(results).toHaveLength(1);
    const type = results[0]!.params[0]!.type;
    expect(type.kind).toBe("array");
    if (type.kind !== "array") throw new Error("expected array");
    expect(type.element).toEqual({ kind: "float" });
  });

  it("leaves param alone when body has no usage (unused)", () => {
    const results = analyzeFile(fixtureFile, "unused");
    expect(results).toHaveLength(1);
    // `any` parameter with no body usage stays unknown.
    expect(results[0]!.params[0]!.type).toEqual({ kind: "unknown" });
  });

  it("infers plain object for property-only access (readField)", () => {
    const results = analyzeFile(fixtureFile, "readField");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type).toEqual({
      kind: "object",
      fields: [["label", { kind: "unknown" }]],
    });
  });

  it(".length-only param refines to array (mixedSignals)", () => {
    const results = analyzeFile(fixtureFile, "mixedSignals");
    expect(results).toHaveLength(1);
    expect(results[0]!.params[0]!.type.kind).toBe("array");
  });
});

describe("param-shape-inference: properties", () => {
  it("synthesised TypeInfo is well-formed and respects depth bound", () => {
    fc.assert(
      fc.property(
        // Generate a sequence of property paths (max 6 levels deep) plus an
        // array signal flag for each leaf, then build a body that exercises
        // those paths.
        fc.array(
          fc.tuple(
            fc.array(
              fc.stringMatching(/^[a-z][a-z0-9]{0,6}$/),
              { minLength: 0, maxLength: 6 },
            ),
            fc.boolean(),
          ),
          { minLength: 1, maxLength: 10 },
        ),
        (paths) => {
          const lines = paths.map(([segs, isArr]) => {
            const chain = segs.length === 0 ? "p" : "p." + segs.join(".");
            return isArr ? `${chain}.filter(x => x);` : `${chain};`;
          });
          const src = `function f(p) { ${lines.join(" ")} }`;
          const { body } = bodyOf(src);
          const out = refineParamShapesFromBody(paramsOf("p"), body);
          checkTypeInfoWellFormed(out[0]!.type, 0);
        },
      ),
      { numRuns: 100 },
    );
  });
});

function checkTypeInfoWellFormed(t: TypeInfo, depth: number): void {
  // Synthesised trees only contain {unknown, array, object} from the
  // refiner. Depth must respect MAX_INFER_DEPTH (32).
  expect(depth).toBeLessThanOrEqual(32);
  expect(["unknown", "array", "object"]).toContain(t.kind);
  if (t.kind === "array") {
    checkTypeInfoWellFormed(t.element, depth + 1);
  }
  if (t.kind === "object") {
    for (const [, f] of t.fields) {
      checkTypeInfoWellFormed(f, depth + 1);
    }
  }
}
