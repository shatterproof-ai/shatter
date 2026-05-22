/**
 * Tests exercising the adapter fixture corpus.
 *
 * Each fixture category validates that the analysis pipeline correctly
 * identifies adapter needs and that fixtures are well-formed for later
 * adapter integration testing.
 */

import * as path from "node:path";

import * as ts from "typescript";

import { analyzeFile, loadProjectCompilerOptions } from "./analyzer.js";
import { executeFunction, executeAdapterOwned } from "./executor.js";
import { REACT_HOOK_ADAPTER_ID } from "./react-hook-recognizer.js";
import { resolveRuntimeHooks } from "./runtime-hooks.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");

// ---------------------------------------------------------------------------
// Hooks fixtures
// ---------------------------------------------------------------------------

describe("adapter-hooks fixture", () => {
  const fixture = path.join(FIXTURES_DIR, "adapter-hooks.tsx");

  it("analyzes all exported functions", () => {
    const results = analyzeFile(fixture);
    const names = results.map((fn) => fn.name);
    expect(names).toContain("useToggle");
    expect(names).toContain("useGreeting");
    expect(names).toContain("useDebounced");
    expect(names).toContain("plainHelper");
  });

  it("attaches react-hook adapter hints to hook functions", () => {
    const results = analyzeFile(fixture);
    const hookFns = results.filter(
      (fn) => fn.adapter_hints?.some((h) => h.adapter.id === REACT_HOOK_ADAPTER_ID),
    );
    const hookNames = hookFns.map((fn) => fn.name);

    expect(hookNames).toContain("useToggle");
    expect(hookNames).toContain("useGreeting");
    expect(hookNames).toContain("useDebounced");
    expect(hookNames).not.toContain("plainHelper");
  });

  it("assigns high confidence to functions calling builtin hooks", () => {
    const results = analyzeFile(fixture);
    for (const fn of results) {
      if (fn.name === "plainHelper") continue;
      const hookHint = fn.adapter_hints?.find(
        (h) => h.adapter.id === REACT_HOOK_ADAPTER_ID,
      );
      expect(hookHint).toBeDefined();
      expect(hookHint!.confidence).toBe("high");
    }
  });

  it("detects branches in hook functions", () => {
    const toggle = analyzeFile(fixture, "useToggle");
    expect(toggle).toHaveLength(1);
    expect(toggle[0]!.branches.length).toBeGreaterThanOrEqual(1);
  });

  it("executes plainHelper without adapter intervention", async () => {
    const result = await executeFunction(fixture, "plainHelper", [5]);
    expect(result.return_value).toBe(10);
    expect(result.thrown_error).toBeNull();
  });

  it("assigns invocation_model to high-confidence hook functions", () => {
    const results = analyzeFile(fixture);
    for (const fn of results) {
      if (fn.name === "plainHelper") {
        expect(fn.invocation_model).toBeUndefined();
        continue;
      }
      // useToggle, useGreeting, useDebounced all call builtin hooks
      expect(fn.invocation_model).toBeDefined();
      expect(fn.invocation_model!.kind).toBe("adapter");
      if (fn.invocation_model!.kind === "adapter") {
        expect(fn.invocation_model!.adapter_id).toBe(REACT_HOOK_ADAPTER_ID);
        expect(fn.invocation_model!.scenario_schema).toEqual({
          kind: "hook_callable_return",
        });
      }
    }
  });

  it("executes useToggle through adapter hook", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      { adapters: [{ id: REACT_HOOK_ADAPTER_ID, apply: "required" }] },
      { phase: "execute", entry_file: fixture },
    );
    expect(runtimeHooks.invocation_hooks).toHaveLength(1);
    const hook = runtimeHooks.invocation_hooks[0]!;

    const result = await executeAdapterOwned({
      hook,
      invocationModel: {
        kind: "adapter",
        adapter_id: REACT_HOOK_ADAPTER_ID,
        scenario_schema: { kind: "hook_callable_return" },
      },
      fileForExec: fixture,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.thrown_error).toBeNull();
    const rv = result.return_value as Record<string, unknown>;
    expect(rv.state).toBe("on");
  });

  it("executes useGreeting through adapter hook", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      { adapters: [{ id: REACT_HOOK_ADAPTER_ID, apply: "required" }] },
      { phase: "execute", entry_file: fixture },
    );
    const hook = runtimeHooks.invocation_hooks[0]!;

    const result = await executeAdapterOwned({
      hook,
      invocationModel: {
        kind: "adapter",
        adapter_id: REACT_HOOK_ADAPTER_ID,
        scenario_schema: { kind: "hook_callable_return" },
      },
      fileForExec: fixture,
      functionName: "useGreeting",
      inputs: ["World"],
    });

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Hello, World!");
  });
});

// ---------------------------------------------------------------------------
// Browser globals fixtures
// ---------------------------------------------------------------------------

describe("adapter-browser-globals fixture", () => {
  const fixture = path.join(FIXTURES_DIR, "adapter-browser-globals.ts");

  it("analyzes all exported functions", () => {
    const results = analyzeFile(fixture);
    const names = results.map((fn) => fn.name);
    expect(names).toContain("getViewportWidth");
    expect(names).toContain("getTitle");
    expect(names).toContain("readSetting");
    expect(names).toContain("setBodyClass");
  });

  it("detects branches in each function", () => {
    const results = analyzeFile(fixture);
    for (const fn of results) {
      expect(fn.branches.length).toBeGreaterThanOrEqual(1);
    }
  });

  it("execution fails without browser globals adapter", async () => {
    const result = await executeFunction(fixture, "getViewportWidth", []);
    expect(result.thrown_error).not.toBeNull();
  });

  it("readSetting has correct parameter types", () => {
    const results = analyzeFile(fixture, "readSetting");
    expect(results).toHaveLength(1);
    expect(results[0]!.params).toHaveLength(2);
    expect(results[0]!.params[0]!.type.kind).toBe("str");
    expect(results[0]!.params[1]!.type.kind).toBe("str");
  });
});

// ---------------------------------------------------------------------------
// import.meta.env fixtures
// ---------------------------------------------------------------------------

describe("adapter-import-meta-env fixture", () => {
  const fixture = path.join(FIXTURES_DIR, "adapter-import-meta-env.ts");

  it("analyzes all exported functions", () => {
    const results = analyzeFile(fixture);
    const names = results.map((fn) => fn.name);
    expect(names).toContain("getApiBase");
    expect(names).toContain("isProduction");
    expect(names).toContain("getFeatureFlag");
  });

  it("detects branches in env-reading functions", () => {
    const results = analyzeFile(fixture);
    for (const fn of results) {
      expect(fn.branches.length).toBeGreaterThanOrEqual(1);
    }
  });

  it("falls through to defaults when import.meta.env is absent", async () => {
    const result = await executeFunction(fixture, "getApiBase", [
      "https://default.example.com",
    ]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("https://default.example.com");
  });

  it("isProduction returns false without env adapter", async () => {
    const result = await executeFunction(fixture, "isProduction", []);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(false);
  });

  it("getFeatureFlag returns false without env adapter", async () => {
    const result = await executeFunction(fixture, "getFeatureFlag", ["DARK_MODE"]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(false);
  });

  it("getApiBase returns env value when adapter provides it", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{
          id: "ts/runtime/import-meta-env",
          apply: "required",
          options: { env: { VITE_API_BASE: "https://adapter.example.com" } },
        }],
      },
      { phase: "execute", entry_file: fixture },
    );
    const result = await executeFunction(
      fixture,
      "getApiBase",
      ["https://fallback.example.com"],
      undefined,
      true,
      undefined,
      runtimeHooks.sandbox_providers,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("https://adapter.example.com");
  });

  it("isProduction returns true when adapter sets MODE=production", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{
          id: "ts/runtime/import-meta-env",
          apply: "required",
          options: { env: { MODE: "production" } },
        }],
      },
      { phase: "execute", entry_file: fixture },
    );
    const result = await executeFunction(
      fixture,
      "isProduction",
      [],
      undefined,
      true,
      undefined,
      runtimeHooks.sandbox_providers,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(true);
  });

  it("getFeatureFlag returns true when adapter sets the flag", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{
          id: "ts/runtime/import-meta-env",
          apply: "required",
          options: { env: { VITE_FLAG_DARK_MODE: "true" } },
        }],
      },
      { phase: "execute", entry_file: fixture },
    );
    const result = await executeFunction(
      fixture,
      "getFeatureFlag",
      ["DARK_MODE"],
      undefined,
      true,
      undefined,
      runtimeHooks.sandbox_providers,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// tsconfig-paths fixtures
// ---------------------------------------------------------------------------

describe("adapter-tsconfig-paths fixture", () => {
  const fixtureDir = path.join(FIXTURES_DIR, "adapter-tsconfig-paths");
  const entryFile = path.join(fixtureDir, "src", "entry.ts");
  const mathFile = path.join(fixtureDir, "src", "lib", "math.ts");

  it("analyzes helper modules independently", () => {
    const results = analyzeFile(mathFile, "clamp");
    expect(results).toHaveLength(1);
    expect(results[0]!.params).toHaveLength(3);
    expect(results[0]!.branches.length).toBeGreaterThanOrEqual(2);
  });

  it("returns stub results without tsconfig-paths adapter", async () => {
    // Without the adapter, unresolved aliases get stub modules —
    // the function runs but produces wrong results (not an error).
    const result = await executeFunction(entryFile, "formatValue", [42, "test"]);
    expect(result.thrown_error).toBeNull();
    // With stubs, clamp/capitalize are undefined → result differs from expected
    expect(result.return_value).not.toBe("Test: positive 42");
  });

  it("execution succeeds with tsconfig-paths adapter", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "formatValue",
      [42, "test"],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Test: positive 42");
  });

  it("resolves wildcard alias (@lib/*)", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "formatValue",
      [-50, "score"],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Score: negative -50");
  });

  it("resolves exact alias (@utils)", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "formatValue",
      [0, "value"],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Value: zero");
  });
});

// ---------------------------------------------------------------------------
// adapter-tsconfig-at-alias fixture (str-jeen.28)
//
// Exercises the literal `@/` alias shape declared as `paths: { "@/*": [...] }`
// — the most common shape in real-world Vite/Next-style projects — through the
// shared tsconfig-paths runtime hook. Mirrors the adapter-tsconfig-paths
// stub-vs-adapter contrast above so a regression in `@/` resolution surfaces
// as a TS-side test failure independent of the cross-frontend E2E.
// ---------------------------------------------------------------------------

describe("adapter-tsconfig-at-alias fixture", () => {
  const fixtureDir = path.join(FIXTURES_DIR, "adapter-tsconfig-at-alias");
  const entryFile = path.join(fixtureDir, "src", "entry.ts");

  it("returns stub results without tsconfig-paths adapter", async () => {
    // Without the adapter, `@/lib/sign` is a bare specifier Node cannot
    // resolve, so `classifySign` becomes the unresolvable-module stub. The
    // stub is callable but returns "" (proxy primitive coercion), so the
    // template literal yields "neg:" / "pos:" instead of the real value.
    const result = await executeFunction(entryFile, "describeNumber", [42]);
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).not.toBe("pos:positive");
  });

  it("execution succeeds with tsconfig-paths adapter (positive)", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "describeNumber",
      [42],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("pos:positive");
  });

  it("execution succeeds with tsconfig-paths adapter (negative)", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "describeNumber",
      [-7],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("neg:negative");
  });

  it("execution succeeds with tsconfig-paths adapter (zero)", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: fixtureDir,
        entry_file: entryFile,
      },
    );
    const result = await executeFunction(
      entryFile,
      "describeNumber",
      [0],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("pos:zero");
  });
});

// ---------------------------------------------------------------------------
// tsconfig project-references fixtures (str-jeen.27)
// ---------------------------------------------------------------------------

describe("adapter-tsconfig-references fixture", () => {
  const fixtureDir = path.join(FIXTURES_DIR, "adapter-tsconfig-references");
  const libProjectDir = path.join(fixtureDir, "packages", "lib");
  const uiProjectDir = path.join(fixtureDir, "packages", "ui");
  const mathFile = path.join(libProjectDir, "src", "math.ts");
  const greetingFile = path.join(uiProjectDir, "src", "greeting.tsx");

  it("merges baseUrl, paths, and jsx from referenced project configs", () => {
    const options = loadProjectCompilerOptions(fixtureDir);

    // baseUrl came from packages/lib/tsconfig.json (resolved relative to that dir)
    expect(options.baseUrl).toBeDefined();
    expect(path.resolve(options.baseUrl!)).toBe(path.resolve(libProjectDir));

    // paths from packages/lib should be merged in
    expect(options.paths).toBeDefined();
    expect(Object.keys(options.paths!)).toContain("@lib/*");
    expect(options.paths!["@lib/*"]).toEqual(["src/*"]);

    // jsx from packages/ui should be merged in (react-jsx)
    expect(options.jsx).toBe(ts.JsxEmit.ReactJSX);
  });

  it("discovers and executes a target inside a referenced project (lib/math)", async () => {
    const analyzed = analyzeFile(mathFile, "clamp", fixtureDir);
    expect(analyzed).toHaveLength(1);
    expect(analyzed[0]!.params).toHaveLength(3);
    expect(analyzed[0]!.branches.length).toBeGreaterThanOrEqual(2);

    const within = await executeFunction(mathFile, "clamp", [5, 0, 10]);
    expect(within.thrown_error).toBeNull();
    expect(within.return_value).toBe(5);

    const clampedLow = await executeFunction(mathFile, "clamp", [-3, 0, 10]);
    expect(clampedLow.return_value).toBe(0);

    const clampedHigh = await executeFunction(mathFile, "clamp", [99, 0, 10]);
    expect(clampedHigh.return_value).toBe(10);
  });

  it("analyzes a TSX target inside a referenced project (ui/greeting)", () => {
    const analyzed = analyzeFile(greetingFile, "buildGreeting", fixtureDir);
    expect(analyzed).toHaveLength(1);
    expect(analyzed[0]!.params).toHaveLength(1);
    expect(analyzed[0]!.params[0]!.type.kind).toBe("str");
    expect(analyzed[0]!.branches.length).toBeGreaterThanOrEqual(1);
  });
});

// ---------------------------------------------------------------------------
// adapter-react-context fixture (str-zgsk)
//
// Validates that a function component using a custom hook around
// `useContext` is classified as a React hook target and executes through
// the react-hook adapter without producing the "Invalid hook call" /
// "Cannot read properties of null (reading 'useContext')" failure mode that
// motivated str-zgsk.
// ---------------------------------------------------------------------------

describe("adapter-react-context fixture", () => {
  const fixture = path.join(FIXTURES_DIR, "adapter-react-context.tsx");

  it("analyzes all exported functions", () => {
    const results = analyzeFile(fixture);
    const names = results.map((fn) => fn.name);
    expect(names).toContain("useThemeMode");
    expect(names).toContain("ThemedLabel");
    expect(names).toContain("NamespacePanel");
  });

  it("classifies every component/hook as a react-hook adapter target", () => {
    const results = analyzeFile(fixture);
    for (const fn of results) {
      expect(fn.invocation_model).toBeDefined();
      expect(fn.invocation_model!.kind).toBe("adapter");
      if (fn.invocation_model!.kind === "adapter") {
        expect(fn.invocation_model!.adapter_id).toBe(REACT_HOOK_ADAPTER_ID);
      }
    }
  });

  it("executes useThemeMode through the adapter without crashing", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      { adapters: [{ id: REACT_HOOK_ADAPTER_ID, apply: "required" }] },
      { phase: "execute", entry_file: fixture },
    );
    const hook = runtimeHooks.invocation_hooks[0]!;
    const result = await executeAdapterOwned({
      hook,
      invocationModel: {
        kind: "adapter",
        adapter_id: REACT_HOOK_ADAPTER_ID,
        scenario_schema: { kind: "hook_callable_return" },
      },
      fileForExec: fixture,
      functionName: "useThemeMode",
      inputs: [],
    });

    expect(result.thrown_error).toBeNull();
    const rv = result.return_value as Record<string, unknown>;
    expect(rv.mode).toBe("light");
    expect(rv.accent).toBe("blue");
  });

  it("executes ThemedLabel (component calling custom hook) through the adapter", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      { adapters: [{ id: REACT_HOOK_ADAPTER_ID, apply: "required" }] },
      { phase: "execute", entry_file: fixture },
    );
    const hook = runtimeHooks.invocation_hooks[0]!;
    const result = await executeAdapterOwned({
      hook,
      invocationModel: {
        kind: "adapter",
        adapter_id: REACT_HOOK_ADAPTER_ID,
        scenario_schema: { kind: "hook_callable_return" },
      },
      fileForExec: fixture,
      functionName: "ThemedLabel",
      inputs: [{ label: "hello" }],
    });

    expect(result.thrown_error).toBeNull();
  });

  it("executes NamespacePanel (React.useContext namespace import) through the adapter", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      { adapters: [{ id: REACT_HOOK_ADAPTER_ID, apply: "required" }] },
      { phase: "execute", entry_file: fixture },
    );
    const hook = runtimeHooks.invocation_hooks[0]!;
    const result = await executeAdapterOwned({
      hook,
      invocationModel: {
        kind: "adapter",
        adapter_id: REACT_HOOK_ADAPTER_ID,
        scenario_schema: { kind: "hook_callable_return" },
      },
      fileForExec: fixture,
      functionName: "NamespacePanel",
      inputs: [{ title: "panel" }],
    });

    expect(result.thrown_error).toBeNull();
  });
});
