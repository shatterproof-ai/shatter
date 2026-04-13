/**
 * Tests for the browser DOM adapter — sandbox construction, storage stubs,
 * factory registration, and integration with existing browser-globals fixtures.
 */

import * as path from "node:path";
import fc from "fast-check";

import {
  createBrowserGlobalsSandbox,
  createStorageStub,
  createBrowserDomFactory,
} from "./browser-dom-adapter.js";
import { BROWSER_GLOBALS_ADAPTER_ID } from "./browser-globals-recognizer.js";
import { executeFunction } from "./executor.js";
import { resolveRuntimeHooks } from "./runtime-hooks.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const BROWSER_FIXTURE = path.join(FIXTURES_DIR, "adapter-browser-globals.ts");

// ---------------------------------------------------------------------------
// Unit: sandbox construction
// ---------------------------------------------------------------------------

describe("createBrowserGlobalsSandbox", () => {
  it("returns all expected top-level globals", () => {
    const globals = createBrowserGlobalsSandbox();
    const expectedKeys = [
      "window", "document", "navigator", "location", "history",
      "localStorage", "sessionStorage", "matchMedia",
      "requestAnimationFrame", "cancelAnimationFrame",
      "ResizeObserver", "IntersectionObserver", "MutationObserver",
      "XMLHttpRequest", "alert", "confirm", "prompt",
    ];
    for (const key of expectedKeys) {
      expect(globals[key]).toBeDefined();
    }
  });

  it("window has default dimensions", () => {
    const globals = createBrowserGlobalsSandbox();
    const win = globals.window as Record<string, unknown>;
    expect(win.innerWidth).toBe(1024);
    expect(win.innerHeight).toBe(768);
  });

  it("window is self-referential", () => {
    const globals = createBrowserGlobalsSandbox();
    const win = globals.window as Record<string, unknown>;
    expect(win.window).toBe(win);
    expect(win.self).toBe(win);
  });

  it("window options override defaults", () => {
    const globals = createBrowserGlobalsSandbox({
      window: { innerWidth: 500, innerHeight: 400 },
    });
    const win = globals.window as Record<string, unknown>;
    expect(win.innerWidth).toBe(500);
    expect(win.innerHeight).toBe(400);
  });

  it("document options override defaults", () => {
    const globals = createBrowserGlobalsSandbox({
      document: { title: "Custom Title" },
    });
    const doc = globals.document as Record<string, unknown>;
    expect(doc.title).toBe("Custom Title");
  });

  it("cross-references are wired correctly", () => {
    const globals = createBrowserGlobalsSandbox();
    const win = globals.window as Record<string, unknown>;
    expect(win.document).toBe(globals.document);
    expect(win.navigator).toBe(globals.navigator);
    expect(win.location).toBe(globals.location);
    expect(win.history).toBe(globals.history);
    expect(win.localStorage).toBe(globals.localStorage);
    expect(win.sessionStorage).toBe(globals.sessionStorage);
  });

  it("matchMedia returns a stub with matches: false", () => {
    const globals = createBrowserGlobalsSandbox();
    const matchMedia = globals.matchMedia as (q: string) => Record<string, unknown>;
    const result = matchMedia("(max-width: 768px)");
    expect(result.matches).toBe(false);
    expect(result.media).toBe("(max-width: 768px)");
  });

  it("observer constructors return stubs with expected methods", () => {
    const globals = createBrowserGlobalsSandbox();
    for (const name of ["ResizeObserver", "IntersectionObserver", "MutationObserver"]) {
      const Ctor = globals[name] as () => Record<string, unknown>;
      const instance = Ctor();
      expect(typeof instance.observe).toBe("function");
      expect(typeof instance.unobserve).toBe("function");
      expect(typeof instance.disconnect).toBe("function");
    }
  });

  it("dialog stubs return expected values", () => {
    const globals = createBrowserGlobalsSandbox();
    expect((globals.alert as () => void)()).toBeUndefined();
    expect((globals.confirm as () => boolean)()).toBe(true);
    expect((globals.prompt as () => string)()).toBe("");
  });
});

// ---------------------------------------------------------------------------
// Unit: storage stub
// ---------------------------------------------------------------------------

describe("createStorageStub", () => {
  it("starts empty when no seed provided", () => {
    const storage = createStorageStub();
    expect(storage.length).toBe(0);
    expect(storage.getItem("anything")).toBeNull();
  });

  it("pre-populates from seed", () => {
    const storage = createStorageStub({ theme: "dark", lang: "en" });
    expect(storage.length).toBe(2);
    expect(storage.getItem("theme")).toBe("dark");
    expect(storage.getItem("lang")).toBe("en");
  });

  it("setItem / getItem roundtrip", () => {
    const storage = createStorageStub();
    storage.setItem("key", "value");
    expect(storage.getItem("key")).toBe("value");
    expect(storage.length).toBe(1);
  });

  it("removeItem deletes entry", () => {
    const storage = createStorageStub({ a: "1" });
    storage.removeItem("a");
    expect(storage.getItem("a")).toBeNull();
    expect(storage.length).toBe(0);
  });

  it("clear removes all entries", () => {
    const storage = createStorageStub({ a: "1", b: "2" });
    storage.clear();
    expect(storage.length).toBe(0);
  });

  it("key() returns key at index", () => {
    const storage = createStorageStub({ alpha: "1", beta: "2" });
    const keys = [storage.key(0), storage.key(1)];
    expect(keys).toContain("alpha");
    expect(keys).toContain("beta");
    expect(storage.key(2)).toBeNull();
  });

  it("setItem coerces value to string", () => {
    const storage = createStorageStub();
    storage.setItem("num", 42 as unknown as string);
    expect(storage.getItem("num")).toBe("42");
  });
});

// ---------------------------------------------------------------------------
// Unit: factory registration
// ---------------------------------------------------------------------------

describe("createBrowserDomFactory", () => {
  it("has correct adapter id", () => {
    const factory = createBrowserDomFactory();
    expect(factory.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
  });

  it("creates sandbox providers", () => {
    const factory = createBrowserDomFactory();
    const hooks = factory.createRuntimeHooks!(
      { id: BROWSER_GLOBALS_ADAPTER_ID },
      { phase: "execute" },
    );
    expect(hooks?.sandbox_providers).toHaveLength(1);
    expect(hooks?.sandbox_providers![0]!.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
  });

  it("passes adapter options through to sandbox", () => {
    const factory = createBrowserDomFactory();
    const hooks = factory.createRuntimeHooks!(
      {
        id: BROWSER_GLOBALS_ADAPTER_ID,
        options: { window: { innerWidth: 320 } },
      },
      { phase: "execute" },
    );

    const sandbox: Record<string, unknown> = {};
    hooks!.sandbox_providers![0]!.augmentSandbox(sandbox);

    const win = sandbox.window as Record<string, unknown>;
    expect(win.innerWidth).toBe(320);
  });
});

// ---------------------------------------------------------------------------
// Integration: resolveRuntimeHooks picks up browser-globals factory
// ---------------------------------------------------------------------------

describe("runtime hook resolution", () => {
  it("resolves browser-globals adapter from default factories", () => {
    const hooks = resolveRuntimeHooks(
      { adapters: [{ id: BROWSER_GLOBALS_ADAPTER_ID, apply: "required" }] },
      { phase: "execute" },
    );
    expect(hooks.sandbox_providers).toHaveLength(1);
    expect(hooks.sandbox_providers[0]!.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
  });
});

// ---------------------------------------------------------------------------
// Integration: fixture execution through adapter
// ---------------------------------------------------------------------------

describe("browser-globals fixture execution", () => {
  function resolveProviders(options?: Record<string, unknown>) {
    const hooks = resolveRuntimeHooks(
      {
        adapters: [{
          id: BROWSER_GLOBALS_ADAPTER_ID,
          apply: "required",
          ...(options ? { options } : {}),
        }],
      },
      { phase: "execute", entry_file: BROWSER_FIXTURE },
    );
    return hooks.sandbox_providers;
  }

  it("getViewportWidth returns 'desktop' with default width (1024)", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "getViewportWidth", [],
      undefined, true, undefined, resolveProviders(),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("desktop");
  });

  it("getViewportWidth returns 'mobile' with narrow width", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "getViewportWidth", [],
      undefined, true, undefined,
      resolveProviders({ window: { innerWidth: 500 } }),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("mobile");
  });

  it("getTitle returns document.title when truthy", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "getTitle", [],
      undefined, true, undefined,
      resolveProviders({ document: { title: "My Page" } }),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("My Page");
  });

  it("getTitle returns 'Untitled' when title is empty", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "getTitle", [],
      undefined, true, undefined, resolveProviders(),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Untitled");
  });

  it("readSetting returns stored value from pre-populated localStorage", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "readSetting", ["theme", "light"],
      undefined, true, undefined,
      resolveProviders({ localStorage: { theme: "dark" } }),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("dark");
  });

  it("readSetting returns fallback when key not in localStorage", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "readSetting", ["missing", "default"],
      undefined, true, undefined, resolveProviders(),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("default");
  });

  it("setBodyClass sets className when truthy", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "setBodyClass", ["active"],
      undefined, true, undefined, resolveProviders(),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("active");
  });

  it("setBodyClass clears className when falsy", async () => {
    const result = await executeFunction(
      BROWSER_FIXTURE, "setBodyClass", [""],
      undefined, true, undefined, resolveProviders(),
    );
    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("");
  });
});

// ---------------------------------------------------------------------------
// Property: localStorage roundtrip
// ---------------------------------------------------------------------------

describe("localStorage stub property tests", () => {
  it("setItem → getItem roundtrip for arbitrary key-value pairs", () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1 }),
        fc.string(),
        (key, value) => {
          const storage = createStorageStub();
          storage.setItem(key, value);
          return storage.getItem(key) === value;
        },
      ),
      { numRuns: 200 },
    );
  });

  it("removeItem makes getItem return null", () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 1 }),
        fc.string(),
        (key, value) => {
          const storage = createStorageStub();
          storage.setItem(key, value);
          storage.removeItem(key);
          return storage.getItem(key) === null;
        },
      ),
      { numRuns: 200 },
    );
  });

  it("length tracks number of unique keys", () => {
    fc.assert(
      fc.property(
        fc.array(
          fc.tuple(fc.string({ minLength: 1 }), fc.string()),
          { minLength: 0, maxLength: 50 },
        ),
        (entries) => {
          const storage = createStorageStub();
          for (const [k, v] of entries) {
            storage.setItem(k, v);
          }
          const uniqueKeys = new Set(entries.map(([k]) => k));
          return storage.length === uniqueKeys.size;
        },
      ),
      { numRuns: 200 },
    );
  });
});
