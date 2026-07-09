import * as path from "node:path";
import * as ts from "typescript";
import { analyzeFile, convertTypeWithNode } from "./analyzer.js";
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

    it("treats optional callback fields as nullable(unknown) and required string fields normally", () => {
      // str-49k: functions with callback-typed options should not produce only TypeError paths.
      // The optional `transform` field must become nullable(unknown) so input_gen can omit it;
      // the required `prefix` field must keep its concrete str type.
      const results = analyzeFile(
        path.join(fixtures, "callback-options.ts"),
        "process",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      // param 0: input — plain string
      expect(fn.params[0]!.type).toEqual({ kind: "str" });
      // param 1: options object — transform optional callable → nullable(unknown), prefix → str
      expect(fn.params[1]!.type).toEqual({
        kind: "object",
        fields: [
          ["transform", { kind: "nullable", inner: { kind: "unknown" } }],
          ["prefix", { kind: "str" }],
        ],
      });
    });

    it("keeps pure function parameters as unknown (regression guard for early-return path)", () => {
      // The early return in convertObjectType for pure callable types must stay intact.
      const results = analyzeFile(
        path.join(fixtures, "callback-options.ts"),
        "applyFn",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({ kind: "unknown" });
      expect(fn.params[1]!.type).toEqual({ kind: "float" });
    });
  });

  describe("structural type recovery (str-yb7q)", () => {
    // These types previously degraded to {kind: "unknown"} or empty objects,
    // causing the core input generator to produce primitives or `{}` and the
    // function under test to fail with `*.filter is not a function` etc.

    it("treats a fixed tuple as an array of the union of element types", () => {
      const results = analyzeFile(path.join(fixtures, "typed-shapes.ts"), "sumPair");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      // Both tuple slots are number; convertType de-dupes via single-element shortcut.
      expect(fn.params[0]!.type).toEqual({
        kind: "array",
        element: { kind: "float" },
      });
    });

    it("treats a readonly tuple with mixed element types as an array union", () => {
      const results = analyzeFile(path.join(fixtures, "typed-shapes.ts"), "labelTuple");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "array",
        element: {
          kind: "union",
          variants: [{ kind: "str" }, { kind: "float" }],
        },
      });
    });

    it("synthesizes a sample field for Record<string, T> index signatures", () => {
      const results = analyzeFile(
        path.join(fixtures, "typed-shapes.ts"),
        "countRowsByKey",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "object",
        fields: [
          [
            "key",
            {
              kind: "array",
              element: {
                kind: "object",
                fields: [["id", { kind: "float" }]],
              },
            },
          ],
        ],
      });
    });

    it("treats ArrayLike<T> (numeric index signature) as array<T>", () => {
      const results = analyzeFile(
        path.join(fixtures, "typed-shapes.ts"),
        "arrayLikeLength",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "array",
        element: { kind: "float" },
      });
    });

    it("follows the constraint of a generic type parameter", () => {
      const results = analyzeFile(
        path.join(fixtures, "typed-shapes.ts"),
        "constrainedGeneric",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "array",
        element: { kind: "float" },
      });
    });

    it("preserves nested array fields (regression guard)", () => {
      const results = analyzeFile(path.join(fixtures, "typed-shapes.ts"), "nestedRows");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "object",
        fields: [
          [
            "rows",
            {
              kind: "array",
              element: {
                kind: "object",
                fields: [["id", { kind: "float" }]],
              },
            },
          ],
        ],
      });
    });

    it("preserves nested array-of-arrays (regression guard)", () => {
      const results = analyzeFile(
        path.join(fixtures, "typed-shapes.ts"),
        "nestedArrays",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "array",
        element: { kind: "array", element: { kind: "float" } },
      });
    });
  });

  describe("array element fidelity (str-9cqde)", () => {
    // Change 1 acceptance: a props field typed `Widget[]` where Widget is
    // declared in another file must resolve to array<object> with the element's
    // fields, not degrade to unknown.
    it("resolves a cross-file array element interface", () => {
      const results = analyzeFile(
        path.join(fixtures, "cross-file-array/props.ts"),
        "renderWidgets",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "object",
        fields: [
          [
            "items",
            {
              kind: "array",
              element: {
                kind: "object",
                fields: [
                  ["id", { kind: "float" }],
                  ["label", { kind: "str" }],
                ],
              },
            },
          ],
          ["title", { kind: "str" }],
        ],
      });
    });

    // Change 2 (fallback hardening): when the SAME array type instance recurs
    // (mutually recursive interfaces), the re-encountered array field must keep
    // its `array` kind with a degraded element, NOT collapse to bare unknown —
    // otherwise the core may realize a non-array value and target code doing
    // `.map`/`.find` on the field crashes.
    it("keeps array kind when a recursive array field trips the cycle guard", () => {
      const results = analyzeFile(
        path.join(fixtures, "recursive-array-types.ts"),
        "renderWorkspaces",
      );
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "object",
        fields: [
          [
            "workspaces",
            {
              kind: "array",
              element: {
                kind: "object",
                fields: [
                  ["id", { kind: "float" }],
                  [
                    "members",
                    {
                      kind: "array",
                      element: {
                        kind: "object",
                        fields: [
                          ["name", { kind: "str" }],
                          // Workspace[] recurs here — the cycle guard fires but
                          // the field stays array<unknown>, never bare unknown.
                          [
                            "workspaces",
                            { kind: "array", element: { kind: "unknown" } },
                          ],
                        ],
                      },
                    },
                  ],
                ],
              },
            },
          ],
          ["title", { kind: "str" }],
        ],
      });
    });

    // Real-world regression: the bundled frontend runs without lib.d.ts, so
    // `checker.isArrayType` returns false and every `T[]` collapses to an empty
    // object — the exact failure that made kapow's FitChoiceChips generate
    // `choices: {}` and crash with `choices.map is not a function`. We reproduce
    // the lib-less environment with a compiler host that resolves no lib files,
    // then assert array-ness is recovered from the declared type node.
    describe("recovers arrays when lib.d.ts is unavailable", () => {
      // Convert the sole parameter's type of `entry` in `src`, compiled with a
      // host that provides NO standard library (noLib + a host that only knows
      // the virtual source file). This makes `checker.isArrayType` false for
      // `T[]`, exactly like the extracted esbuild bundle.
      function convertEntryParamNoLib(src: string): TypeInfo {
        const fileName = "/virtual/entry.tsx";
        const sf = ts.createSourceFile(
          fileName,
          src,
          ts.ScriptTarget.ES2022,
          true,
          ts.ScriptKind.TSX,
        );
        const host: ts.CompilerHost = {
          getSourceFile: (fn) => (fn === fileName ? sf : undefined),
          writeFile: () => {},
          getDefaultLibFileName: () => "lib.d.ts",
          getCurrentDirectory: () => "/virtual",
          getCanonicalFileName: (f) => f,
          useCaseSensitiveFileNames: () => true,
          getNewLine: () => "\n",
          fileExists: (fn) => fn === fileName,
          readFile: () => undefined,
        };
        const program = ts.createProgram(
          [fileName],
          { noLib: true, target: ts.ScriptTarget.ES2022 },
          host,
        );
        const checker = program.getTypeChecker();
        let result: TypeInfo | undefined;
        const visit = (node: ts.Node): void => {
          if (ts.isFunctionDeclaration(node) && node.name?.text === "entry") {
            const param = node.parameters[0]!;
            const type = checker.getTypeAtLocation(param);
            // Mirror analyzeParameter: recover from the declared node so both
            // the direct-array-param and props-field paths are exercised.
            result = convertTypeWithNode(type, param.type, checker, sf, new Set());
          }
          ts.forEachChild(node, visit);
        };
        visit(sf);
        if (!result) throw new Error("entry not found");
        return result;
      }

      it("sanity: without recovery lib is truly absent (isArrayType is false)", () => {
        // Guard the test's own premise: if a future TS/host change made the lib
        // resolvable here, isArrayType would pass and this suite would no longer
        // exercise the degraded path — so assert the degradation precondition.
        const fileName = "/virtual/entry.tsx";
        const sf = ts.createSourceFile(
          fileName,
          "interface P { xs: string[]; }\nexport function entry(p: P): void {}\n",
          ts.ScriptTarget.ES2022,
          true,
          ts.ScriptKind.TSX,
        );
        const host: ts.CompilerHost = {
          getSourceFile: (fn) => (fn === fileName ? sf : undefined),
          writeFile: () => {},
          getDefaultLibFileName: () => "lib.d.ts",
          getCurrentDirectory: () => "/virtual",
          getCanonicalFileName: (f) => f,
          useCaseSensitiveFileNames: () => true,
          getNewLine: () => "\n",
          fileExists: (fn) => fn === fileName,
          readFile: () => undefined,
        };
        const program = ts.createProgram(
          [fileName],
          { noLib: true, target: ts.ScriptTarget.ES2022 },
          host,
        );
        const checker = program.getTypeChecker();
        let isArr = true;
        const visit = (node: ts.Node): void => {
          if (ts.isInterfaceDeclaration(node) && node.name.text === "P") {
            const t = checker.getTypeAtLocation(node);
            const sym = t.getProperties().find((s) => s.name === "xs")!;
            isArr = checker.isArrayType(checker.getTypeOfSymbol(sym));
          }
          ts.forEachChild(node, visit);
        };
        visit(sf);
        expect(isArr).toBe(false);
      });

      it("recovers a primitive array props field", () => {
        const t = convertEntryParamNoLib(
          "interface P { choices: string[]; title: string; }\n" +
            "export function entry(p: P): void {}\n",
        );
        expect(t).toEqual({
          kind: "object",
          fields: [
            ["choices", { kind: "array", element: { kind: "str" } }],
            ["title", { kind: "str" }],
          ],
        });
      });

      it("recovers a cross-file-style interface array element", () => {
        const t = convertEntryParamNoLib(
          "interface Widget { id: number; label: string; }\n" +
            "interface P { widgets: Widget[]; }\n" +
            "export function entry(p: P): void {}\n",
        );
        expect(t).toEqual({
          kind: "object",
          fields: [
            [
              "widgets",
              {
                kind: "array",
                element: {
                  kind: "object",
                  fields: [
                    ["id", { kind: "float" }],
                    ["label", { kind: "str" }],
                  ],
                },
              },
            ],
          ],
        });
      });

      it("recovers nested, readonly, and Array<T> generic forms", () => {
        const t = convertEntryParamNoLib(
          "interface P {\n" +
            "  nested: number[][];\n" +
            "  ro: readonly string[];\n" +
            "  gen: Array<boolean>;\n" +
            "  roGen: ReadonlyArray<string>;\n" +
            "}\n" +
            "export function entry(p: P): void {}\n",
        );
        expect(t).toEqual({
          kind: "object",
          fields: [
            [
              "nested",
              {
                kind: "array",
                element: { kind: "array", element: { kind: "float" } },
              },
            ],
            ["ro", { kind: "array", element: { kind: "str" } }],
            ["gen", { kind: "array", element: { kind: "bool" } }],
            ["roGen", { kind: "array", element: { kind: "str" } }],
          ],
        });
      });

      it("recovers a direct array parameter (not just props fields)", () => {
        const t = convertEntryParamNoLib(
          "interface Widget { id: number; }\n" +
            "export function entry(items: Widget[]): void {}\n",
        );
        expect(t).toEqual({
          kind: "array",
          element: { kind: "object", fields: [["id", { kind: "float" }]] },
        });
      });
    });

    // Review finding (Merge-with-fixes): the syntactic recovery in
    // convertTypeWithNode must fire ONLY on the degraded shapes a lib-LESS
    // checker produces for an array (bare `unknown` / empty-`fields` object).
    // With lib.d.ts present it must stay inert; a looser "not already array"
    // guard would misfire in two ways proven below.
    describe("recovery stays inert when lib.d.ts is present (str-9cqde review)", () => {
      // Compile `src` WITH the real standard library — lib.d.ts resolves from
      // disk while the virtual entry file is served in-memory — then return the
      // TypeInfo of `entry`'s sole parameter via the production convertTypeWithNode.
      function convertEntryParamWithLib(src: string): TypeInfo {
        const fileName = "/virtual/entry.ts";
        const sf = ts.createSourceFile(
          fileName,
          src,
          ts.ScriptTarget.ES2022,
          true,
        );
        // strictNullChecks so an optional `items?: T[]` carries `| undefined`
        // in its *type* while the declared type *node* stays a bare `T[]` — the
        // exact combination that produces `nullable{array}` from convertType and
        // would trip a loose recovery guard into stripping the nullable wrapper.
        const options: ts.CompilerOptions = {
          target: ts.ScriptTarget.ES2022,
          strictNullChecks: true,
        };
        const host = ts.createCompilerHost(options, true);
        const origGetSourceFile = host.getSourceFile.bind(host);
        host.getSourceFile = (fn, v, onError, shouldCreate) =>
          fn === fileName ? sf : origGetSourceFile(fn, v, onError, shouldCreate);
        const origFileExists = host.fileExists.bind(host);
        host.fileExists = (fn) => fn === fileName || origFileExists(fn);
        const program = ts.createProgram([fileName], options, host);
        const checker = program.getTypeChecker();
        let result: TypeInfo | undefined;
        const visit = (node: ts.Node): void => {
          if (ts.isFunctionDeclaration(node) && node.name?.text === "entry") {
            const param = node.parameters[0]!;
            const type = checker.getTypeAtLocation(param);
            result = convertTypeWithNode(type, param.type, checker, sf, new Set());
          }
          ts.forEachChild(node, visit);
        };
        visit(sf);
        if (!result) throw new Error("entry not found");
        return result;
      }

      it("sanity: lib IS resolvable here (isArrayType true for T[])", () => {
        // Guard the premise: if lib stopped resolving, isArrayType would be
        // false and the no-misfire assertions below would become vacuous.
        const fileName = "/virtual/entry.ts";
        const sf = ts.createSourceFile(
          fileName,
          "export function entry(xs: string[]): void {}\n",
          ts.ScriptTarget.ES2022,
          true,
        );
        const options: ts.CompilerOptions = { target: ts.ScriptTarget.ES2022 };
        const host = ts.createCompilerHost(options, true);
        const orig = host.getSourceFile.bind(host);
        host.getSourceFile = (fn, v, e, s) =>
          fn === fileName ? sf : orig(fn, v, e, s);
        const origExists = host.fileExists.bind(host);
        host.fileExists = (fn) => fn === fileName || origExists(fn);
        const program = ts.createProgram([fileName], options, host);
        const checker = program.getTypeChecker();
        let isArr = false;
        const visit = (node: ts.Node): void => {
          if (ts.isFunctionDeclaration(node) && node.name?.text === "entry") {
            isArr = checker.isArrayType(
              checker.getTypeAtLocation(node.parameters[0]!),
            );
          }
          ts.forEachChild(node, visit);
        };
        visit(sf);
        expect(isArr).toBe(true);
      });

      // Misfire (a): an OPTIONAL array param converts to `nullable{array}`.
      // The loose guard would strip the nullable wrapper (recovery returns a
      // bare `array`), a double conversion masked only because analyzeParameter
      // re-wraps on the `?` token. The degraded-shape gate leaves it untouched.
      it("keeps an optional array parameter nullable (no wrapper stripping)", () => {
        const t = convertEntryParamWithLib(
          "interface Widget { id: number; }\n" +
            "export function entry(items?: Widget[]): void {}\n",
        );
        expect(t).toEqual({
          kind: "nullable",
          inner: {
            kind: "array",
            element: { kind: "object", fields: [["id", { kind: "float" }]] },
          },
        });
      });

      // Misfire (b): a user type literally named `Array` (shadowing the global
      // in a namespace, so no declaration merging) resolves to a real object.
      // `arrayElementTypeNode` keys off the identifier text `Array`, so the
      // loose guard would override the object to a bogus `array` kind. The
      // degraded-shape gate returns the object unchanged.
      it("does not override a user type named Array<T> to array kind", () => {
        const t = convertEntryParamWithLib(
          "namespace Shadow {\n" +
            "  export interface Array<T> { widgets: T }\n" +
            "  export function entry(x: Array<number>): void {}\n" +
            "}\n",
        );
        expect(t).toEqual({
          kind: "object",
          fields: [["widgets", { kind: "float" }]],
        });
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

  describe("enum and literal-union value domains (str-knf0v)", () => {
    it("emits enum_values for a literal-union alias parameter", () => {
      const results = analyzeFile(path.join(fixtures, "enum-values.ts"), "pickMode");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "union",
        variants: [{ kind: "str" }],
        enum_values: ["fast", "slow", "off"],
      });
    });

    it("emits enum_values for a string enum parameter", () => {
      const results = analyzeFile(path.join(fixtures, "enum-values.ts"), "classify");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "union",
        variants: [{ kind: "str" }],
        enum_values: ["RED", "GREEN", "BLUE"],
      });
    });

    it("emits forward numeric member values for a numeric enum parameter", () => {
      const results = analyzeFile(path.join(fixtures, "enum-values.ts"), "rank");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "union",
        variants: [{ kind: "float" }],
        enum_values: [1, 2, 3],
      });
    });

    it("emits enum_values for a single-member numeric enum parameter", () => {
      const results = analyzeFile(path.join(fixtures, "enum-values.ts"), "solo");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toEqual({
        kind: "union",
        variants: [{ kind: "float" }],
        enum_values: [7],
      });
    });

    it("does not emit enum_values for a widened primitive union", () => {
      const results = analyzeFile(path.join(fixtures, "unions.ts"), "format");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      const typ = fn.params[0]!.type;
      expect(typ).toEqual({
        kind: "union",
        variants: [{ kind: "str" }, { kind: "float" }],
      });
      expect(typ.kind === "union" && typ.enum_values).toBeUndefined();
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

  describe("literal extraction", () => {
    it("extracts string literals from if conditions and return values", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "classifyPriority");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("express");
      expect(strs).toContain("economy");
      expect(strs).toContain("standard");
    });

    it("extracts numeric literals from switch cases", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "gradeScore");
      const fn = results[0]!;
      const ints = (fn.literals ?? [])
        .filter((l): l is { type: "int"; value: number } => l.type === "int")
        .map((l) => l.value);
      expect(ints).toContain(90);
      expect(ints).toContain(70);
      expect(ints).toContain(50);
    });

    it("extracts regex patterns", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "validateZip");
      const fn = results[0]!;
      const regexes = (fn.literals ?? []).filter(
        (l): l is { type: "regex"; pattern: string } => l.type === "regex",
      );
      expect(regexes.length).toBe(1);
      expect(regexes[0]!.pattern).toBe("^\\d{5}$");
    });

    it("extracts default parameter values", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "greetWithDefault");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("World");
    });

    it("includes file-level consts even when function body has no literals", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "noLiterals");
      const fn = results[0]!;
      // noLiterals has no body literals, but file-level consts and enums are included
      const lits = fn.literals ?? [];
      expect(lits.length).toBeGreaterThan(0);
      const ints = lits
        .filter((l): l is { type: "int"; value: number } => l.type === "int")
        .map((l) => l.value);
      expect(ints).toContain(3); // MAX_RETRIES
    });

    it("extracts literals from arrow functions", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "classifyArrow");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("admin");
      expect(strs).toContain("privileged");
      expect(strs).toContain("normal");
    });

    it("deduplicates repeated literals", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "withDuplicates");
      const fn = results[0]!;
      const okCount = (fn.literals ?? []).filter(
        (l) => l.type === "str" && (l as { type: "str"; value: string }).value === "ok",
      ).length;
      expect(okCount).toBe(1);
    });

    it("extracts file-level const values", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "useFileConsts");
      const fn = results[0]!;
      const ints = (fn.literals ?? [])
        .filter((l): l is { type: "int"; value: number } => l.type === "int")
        .map((l) => l.value);
      expect(ints).toContain(3); // MAX_RETRIES
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("v1"); // PREFIX
      const floats = (fn.literals ?? [])
        .filter((l): l is { type: "float"; value: number } => l.type === "float")
        .map((l) => l.value);
      expect(floats).toContain(0.75); // THRESHOLD
    });

    it("extracts enum member values", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "useFileConsts");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("red");
      expect(strs).toContain("green");
      expect(strs).toContain("blue");
    });

    it("extracts property access keys", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "checkStatus");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("status");
    });

    it("extracts bracket-access string keys", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "lookupBracket");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("priority");
      expect(strs).toContain("weight");
    });

    it("extracts union type literal members from parameters", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "goDirection");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("north");
      expect(strs).toContain("south");
      expect(strs).toContain("east");
    });

    it("classifies unsafe-integer union members as float, not int (str-flqp)", () => {
      const results = analyzeFile(path.join(fixtures, "literals.ts"), "clampToFinite");
      const fn = results[0]!;
      const lits = fn.literals ?? [];
      // 1e308 exceeds Number.MAX_SAFE_INTEGER and must be tagged as "float"
      const floats = lits.filter(
        (l): l is { type: "float"; value: number } => l.type === "float",
      );
      const floatValues = floats.map((l) => l.value);
      expect(floatValues).toContain(1e308);
      expect(floatValues).toContain(3.14);
      // 42 is a safe integer, should be tagged as "int"
      const intValues = lits
        .filter((l): l is { type: "int"; value: number } => l.type === "int")
        .map((l) => l.value);
      expect(intValues).toContain(42);
      // 1e308 must NOT be tagged as int
      expect(intValues).not.toContain(1e308);
    });

    // str-jeen.82: extractLiterals previously walked every file-level const
    // initializer (except function-valued consts), which swept strings out of
    // unrelated object-literal method bodies (e.g. an exported API object's
    // route strings) into every peer function's literal set. The relevance
    // rule is now: only harvest from file-level consts whose declared name is
    // referenced inside the function body / parameter defaults.
    it("does not leak strings from unrelated module-level object literals (str-jeen.82)", () => {
      const results = analyzeFile(path.join(fixtures, "literal-leak.ts"), "tagsQueryKey");
      const fn = results[0]!;
      const strs = (fn.literals ?? [])
        .filter((l): l is { type: "str"; value: string } => l.type === "str")
        .map((l) => l.value);
      expect(strs).toContain("tags");
      // Strings from the unrelated pickpackitApi object's method bodies
      // must not appear in tagsQueryKey's literals.
      expect(strs).not.toContain("/api/workspaces");
      expect(strs).not.toContain("POST");
      expect(strs).not.toContain("DELETE");
      expect(strs).not.toContain("PATCH");
      expect(strs).not.toContain("stringify");
    });

    it("still harvests referenced file-level consts (str-jeen.82)", () => {
      const results = analyzeFile(path.join(fixtures, "literal-leak.ts"), "usesRetries");
      const fn = results[0]!;
      const ints = (fn.literals ?? [])
        .filter((l): l is { type: "int"; value: number } => l.type === "int")
        .map((l) => l.value);
      expect(ints).toContain(3); // MAX_RETRIES is referenced
    });
  });

  describe("function expression patterns", () => {
    it("detects FunctionExpression in variable declaration", () => {
      const results = analyzeFile(path.join(fixtures, "function-patterns.ts"), "square");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("square");
      expect(fn.exported).toBe(true);
      expect(fn.params[0]!.type).toEqual({ kind: "float" });
      expect(fn.return_type).toEqual({ kind: "float" });
    });

    it("detects named default export function", () => {
      const results = analyzeFile(path.join(fixtures, "function-patterns.ts"), "defaultGreet");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("defaultGreet");
      expect(fn.exported).toBe(true);
      expect(fn.params[0]!.type).toEqual({ kind: "str" });
    });

    it("detects unnamed default export function as \'<default>\'", () => {
      const results = analyzeFile(path.join(fixtures, "unnamed-default.ts"));
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("<default>");
      expect(fn.exported).toBe(true);
      expect(fn.params[0]!.type).toEqual({ kind: "float" });
    });

    it("returns all functions including FunctionExpression and default exports", () => {
      const results = analyzeFile(path.join(fixtures, "function-patterns.ts"));
      const names = results.map((f) => f.name);
      expect(names).toContain("square");
      expect(names).toContain("defaultGreet");
    });
  });

  describe("CommonJS patterns", () => {
    it("detects functions referenced in module.exports object", () => {
      const results = analyzeFile(path.join(fixtures, "commonjs-patterns.js"));
      const names = results.map((f) => f.name);
      expect(names).toContain("helperA");
      expect(names).toContain("helperB");
    });

    it("detects exports.name = function pattern", () => {
      const results = analyzeFile(path.join(fixtures, "commonjs-patterns.js"));
      const names = results.map((f) => f.name);
      expect(names).toContain("standalone");
    });
  });

  describe("TSX support", () => {
    it("parses .tsx files and extracts functions", () => {
      const results = analyzeFile(path.join(fixtures, "component.tsx"));
      const names = results.map((f) => f.name);
      expect(names).toContain("greetingLabel");
      expect(names).toContain("statusBadge");
    });

    it("extracts parameters from TSX functions", () => {
      const results = analyzeFile(path.join(fixtures, "component.tsx"), "greetingLabel");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params).toHaveLength(1);
      expect(fn.params[0]!.name).toBe("name");
      expect(fn.params[0]!.type).toEqual({ kind: "str" });
    });

    it("detects branches inside TSX functions", () => {
      const results = analyzeFile(path.join(fixtures, "component.tsx"), "greetingLabel");
      expect(results).toHaveLength(1);
      expect(results[0]!.branches.length).toBeGreaterThan(0);
    });

    it("analyzes a single function by name in TSX", () => {
      const results = analyzeFile(path.join(fixtures, "component.tsx"), "statusBadge");
      expect(results).toHaveLength(1);
      expect(results[0]!.name).toBe("statusBadge");
    });
  });

  describe("static opacity heuristics", () => {
    const fixturePath = path.join(fixtures, "static-opaque-types.ts");

    it("abstract class is detected as opaque with abstract_type reason", () => {
      const results = analyzeFile(fixturePath, "handleAbstract");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toMatchObject({ kind: "opaque", static_opacity: "abstract_type" });
    });

    it("private constructor is detected as opaque with abstract_type reason", () => {
      const results = analyzeFile(fixturePath, "handleSingleton");
      expect(results).toHaveLength(1);
      expect(results[0]!.params[0]!.type).toMatchObject({ kind: "opaque", static_opacity: "abstract_type" });
    });

    it("method-only interface with no implementors is detected as opaque with no_implementors reason", () => {
      const results = analyzeFile(fixturePath, "handleSource");
      expect(results).toHaveLength(1);
      expect(results[0]!.params[0]!.type).toMatchObject({ kind: "opaque", static_opacity: "no_implementors" });
    });

    it("class whose constructor requires opaque arg is detected as transitively_opaque", () => {
      const results = analyzeFile(fixturePath, "handleWrapper");
      expect(results).toHaveLength(1);
      expect(results[0]!.params[0]!.type).toMatchObject({ kind: "opaque", static_opacity: "transitively_opaque" });
    });
  });

  describe("medium-confidence opacity heuristics", () => {
    const fixturePath = path.join(fixtures, "medium-opaque-types.ts");

    it("class with close() method is detected as opaque with closeable_interface reason", () => {
      const results = analyzeFile(fixturePath, "handleResource");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type).toMatchObject({ kind: "opaque", medium_opacity: "closeable_interface" });
      expect((fn.params[0]!.type as { static_opacity?: string }).static_opacity).toBeUndefined();
    });

    it("class with fd field is detected as opaque with native_handle_field reason", () => {
      const results = analyzeFile(fixturePath, "handleFd");
      expect(results).toHaveLength(1);
      expect(results[0]!.params[0]!.type).toMatchObject({ kind: "opaque", medium_opacity: "native_handle_field" });
    });

    it("class with handle field is detected as opaque with native_handle_field reason", () => {
      const results = analyzeFile(fixturePath, "handleOs");
      expect(results).toHaveLength(1);
      expect(results[0]!.params[0]!.type).toMatchObject({ kind: "opaque", medium_opacity: "native_handle_field" });
    });

    it("plain data class without close or handle fields is NOT detected as opaque", () => {
      const results = analyzeFile(fixturePath, "handleSafe");
      expect(results).toHaveLength(1);
      const paramType = results[0]!.params[0]!.type;
      expect(paramType.kind).not.toBe("opaque");
    });
  });

  describe("loop induction variable analysis", () => {
    it("detects canonical for loop: for (let i = 0; i < n; i++)", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoopCanonical");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.loops).toBeDefined();
      expect(fn.loops).toHaveLength(1);
      const loop = fn.loops![0]!;
      expect(loop.loop_id).toBe(0);
      expect(loop.induction_var.name).toBe("i");
      expect(loop.induction_var.init_expr).toEqual({ kind: "const", type: "int", value: 0 });
      expect(loop.induction_var.step_expr).toEqual({ kind: "const", type: "int", value: 1 });
      expect(loop.induction_var.bound_expr).toEqual({ kind: "param", name: "n", path: [] });
      expect(loop.induction_var.bound_op).toBe("lt");
    });

    it("detects for loop with step 2: for (let i = 0; i < n; i += 2)", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoopStepTwo");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.loops).toBeDefined();
      expect(fn.loops).toHaveLength(1);
      const loop = fn.loops![0]!;
      expect(loop.induction_var.name).toBe("i");
      expect(loop.induction_var.init_expr).toEqual({ kind: "const", type: "int", value: 0 });
      expect(loop.induction_var.step_expr).toEqual({ kind: "const", type: "int", value: 2 });
      expect(loop.induction_var.bound_op).toBe("lt");
    });

    it("does NOT detect loop when body modifies induction variable", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoopBodyModifiesI");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      // loops field should be absent or empty
      expect(!fn.loops || fn.loops.length === 0).toBe(true);
    });

    it("does NOT detect loop when condition is missing", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoopNoCondition");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(!fn.loops || fn.loops.length === 0).toBe(true);
    });

    it("does NOT detect loop when init is a float literal", () => {
      const results = analyzeFile(path.join(fixtures, "branches.ts"), "forLoopFloatInit");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(!fn.loops || fn.loops.length === 0).toBe(true);
    });
  });

  describe("barrel re-exports", () => {
    it("discovers functions from export * re-exports", () => {
      const results = analyzeFile(path.join(fixtures, "barrel-index.ts"));
      const names = results.map((r) => r.name).sort();
      expect(names).toContain("barrelAdd");
      expect(names).toContain("barrelGreet");
    });

    it("sets source_file to the actual declaration file", () => {
      const results = analyzeFile(path.join(fixtures, "barrel-index.ts"));
      const barrelAdd = results.find((r) => r.name === "barrelAdd");
      expect(barrelAdd).toBeDefined();
      expect(barrelAdd!.source_file).toBe(
        path.resolve(path.join(fixtures, "barrel-source.ts")),
      );
    });

    it("discovers named re-exports with rename", () => {
      const results = analyzeFile(path.join(fixtures, "barrel-index.ts"));
      const names = results.map((r) => r.name);
      // renamedAdd re-exports barrelAdd under a new name; the analyzer
      // resolves to the original declaration so the name is barrelAdd
      expect(names).toContain("barrelAdd");
    });

    it("does NOT set source_file on direct analysis", () => {
      const results = analyzeFile(path.join(fixtures, "barrel-source.ts"));
      expect(results.length).toBeGreaterThan(0);
      for (const fn of results) {
        expect(fn.source_file).toBeUndefined();
      }
    });

    it("does NOT follow re-exports when file has own functions", () => {
      // primitives.ts has its own functions — re-export following should not trigger
      const results = analyzeFile(path.join(fixtures, "primitives.ts"));
      expect(results.length).toBeGreaterThan(0);
      for (const fn of results) {
        expect(fn.source_file).toBeUndefined();
      }
    });
  });

  describe("recursive types", () => {
    it("handles self-referential types without stack overflow", () => {
      const results = analyzeFile(path.join(fixtures, "recursive-types.ts"), "traverseTree");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.name).toBe("traverseTree");
      expect(fn.params).toHaveLength(1);
      // The root param should be an object — recursive fields should bottom out
      // at {kind: "unknown"} rather than causing infinite recursion
      const rootType = fn.params[0]!.type;
      expect(rootType.kind).toBe("object");
    });

    it("handles mutually recursive types without stack overflow", () => {
      const results = analyzeFile(path.join(fixtures, "recursive-types.ts"), "processOdd");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type.kind).toBe("object");
    });

    it("handles recursive generic types without stack overflow", () => {
      const results = analyzeFile(path.join(fixtures, "recursive-types.ts"), "readDeep");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.params[0]!.type.kind).toBe("object");
    });

    it("handles recursive union types (JsonValue) without stack overflow", () => {
      const results = analyzeFile(path.join(fixtures, "recursive-types.ts"), "parseJson");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      // JsonValue is a union — it should resolve without infinite recursion
      const inputType = fn.params[0]!.type;
      expect(["union", "nullable", "unknown"]).toContain(inputType.kind);
    });
  });

  describe("React hook adapter hints", () => {
    it("emits high confidence react-hook hint for useCounter", () => {
      const results = analyzeFile(path.join(fixtures, "react-hooks.tsx"), "useCounter");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.adapter_hints).toBeDefined();
      expect(fn.adapter_hints).toHaveLength(1);
      expect(fn.adapter_hints![0]!.adapter.id).toBe("react-hook");
      expect(fn.adapter_hints![0]!.confidence).toBe("high");
      expect(fn.adapter_hints![0]!.reasons!.length).toBeGreaterThan(0);
    });

    it("emits high confidence react-hook hint for useFormattedName", () => {
      const results = analyzeFile(path.join(fixtures, "react-hooks.tsx"), "useFormattedName");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.adapter_hints).toBeDefined();
      expect(fn.adapter_hints).toHaveLength(1);
      expect(fn.adapter_hints![0]!.confidence).toBe("high");
    });

    it("emits react-hook hint for StatusCard (calls hooks)", () => {
      const results = analyzeFile(path.join(fixtures, "react-hooks.tsx"), "StatusCard");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      // StatusCard calls useState — should get a hint even without useXxx name
      expect(fn.adapter_hints).toBeDefined();
      expect(fn.adapter_hints).toHaveLength(1);
      expect(fn.adapter_hints![0]!.confidence).toBe("high");
    });

    it("routes a function component using context and a custom hook through the React adapter", () => {
      const results = analyzeFile(path.join(fixtures, "react-hooks.tsx"), "ContextPanel");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.adapter_hints).toBeDefined();
      expect(fn.adapter_hints).toHaveLength(1);
      expect(fn.adapter_hints![0]!.adapter.id).toBe("react-hook");
      expect(fn.adapter_hints![0]!.confidence).toBe("high");
      expect(fn.adapter_hints![0]!.reasons).toEqual(
        expect.arrayContaining([
          "Calls useContext imported from 'react'",
          "Calls custom hook useAccentLabel",
        ]),
      );
      expect(fn.invocation_model).toEqual({
        kind: "adapter",
        adapter_id: "react-hook",
        scenario_schema: { kind: "hook_callable_return" },
      });
    });

    it("does not emit hint for useFormatting (name only, no hook calls)", () => {
      const results = analyzeFile(path.join(fixtures, "react-hooks.tsx"), "useFormatting");
      expect(results).toHaveLength(1);
      const fn = results[0]!;
      expect(fn.adapter_hints).toBeUndefined();
    });

    it("does not emit hints for non-React .ts files", () => {
      const results = analyzeFile(path.join(fixtures, "primitives.ts"), "add");
      expect(results).toHaveLength(1);
      expect(results[0]!.adapter_hints).toBeUndefined();
    });
  });
});
