/**
 * Tests exercising the adapter fixture corpus.
 *
 * Each fixture category validates that the analysis pipeline correctly
 * identifies adapter needs and that fixtures are well-formed for later
 * adapter integration testing.
 */

import * as path from "node:path";

import { analyzeFile } from "./analyzer.js";
import { executeFunction } from "./executor.js";
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
