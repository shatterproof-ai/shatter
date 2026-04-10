import {
  detectRuntimeHints,
  ADAPTER_ID_REACT_HOOKS,
  ADAPTER_ID_TSCONFIG_PATHS,
  ADAPTER_ID_BROWSER_GLOBALS,
  ADAPTER_ID_IMPORT_META_ENV,
} from "./runtime-hints.js";
import type { ErrorInfo } from "./protocol.js";

function makeError(error_type: string, message: string): ErrorInfo {
  return { error_type, message, stack: null, error_category: "runtime" };
}

describe("detectRuntimeHints", () => {
  // -----------------------------------------------------------------------
  // React invalid hook call
  // -----------------------------------------------------------------------

  describe("React invalid hook call", () => {
    it("detects 'Invalid hook call' message", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Invalid hook call. Hooks can only be called inside of the body of a function component."),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_REACT_HOOKS);
      expect(hints[0]!.confidence).toBe("high");
    });

    it("detects 'Hooks can only be called inside' variant", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Hooks can only be called inside of the body of a function component. https://reactjs.org/link/invalid-hook-call"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_REACT_HOOKS);
    });

    it("detects case-insensitive hook error", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "hooks must be called inside a component"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_REACT_HOOKS);
    });

    it("does not match unrelated Error messages", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot read property 'length' of undefined"),
      );
      expect(hints).toHaveLength(0);
    });
  });

  // -----------------------------------------------------------------------
  // Browser globals
  // -----------------------------------------------------------------------

  describe("browser globals", () => {
    it("detects window is not defined", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "window is not defined"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_BROWSER_GLOBALS);
      expect(hints[0]!.confidence).toBe("high");
      expect(hints[0]!.reasons![0]).toContain("window");
    });

    it("detects document is not defined", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "document is not defined"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_BROWSER_GLOBALS);
    });

    it("detects navigator is not defined", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "navigator is not defined"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_BROWSER_GLOBALS);
    });

    it("detects localStorage is not defined", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "localStorage is not defined"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_BROWSER_GLOBALS);
    });

    it("only matches ReferenceError, not TypeError", () => {
      const hints = detectRuntimeHints(
        makeError("TypeError", "window is not defined"),
      );
      expect(hints).toHaveLength(0);
    });

    it("does not match non-browser globals", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "myCustomGlobal is not defined"),
      );
      expect(hints).toHaveLength(0);
    });
  });

  // -----------------------------------------------------------------------
  // import.meta.env
  // -----------------------------------------------------------------------

  describe("import.meta.env", () => {
    it("detects import.meta.env in error message", () => {
      const hints = detectRuntimeHints(
        makeError("TypeError", "Cannot read properties of undefined (reading 'env')"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_IMPORT_META_ENV);
      expect(hints[0]!.confidence).toBe("high");
    });

    it("detects import.meta.env literal reference", () => {
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "import.meta.env is not defined"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_IMPORT_META_ENV);
    });

    it("detects __shatter_import_meta.env polyfill reference", () => {
      const hints = detectRuntimeHints(
        makeError("TypeError", "Cannot read properties of undefined (reading 'VITE_API_URL') at __shatter_import_meta.env"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_IMPORT_META_ENV);
    });
  });

  // -----------------------------------------------------------------------
  // Unresolved tsconfig path aliases
  // -----------------------------------------------------------------------

  describe("tsconfig path aliases", () => {
    it("detects @/components path alias", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module '@/components/Button'"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_TSCONFIG_PATHS);
      expect(hints[0]!.confidence).toBe("medium");
    });

    it("detects ~/utils path alias", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module '~/utils/format'"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_TSCONFIG_PATHS);
    });

    it("detects src/ path alias", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module 'src/services/api'"),
      );
      expect(hints).toHaveLength(1);
      expect(hints[0]!.adapter.id).toBe(ADAPTER_ID_TSCONFIG_PATHS);
    });

    it("does not match relative module paths", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module './foo'"),
      );
      expect(hints).toHaveLength(0);
    });

    it("does not match npm package names without path", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module 'lodash'"),
      );
      expect(hints).toHaveLength(0);
    });

    it("does not match scoped npm packages", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Cannot find module '@types/node'"),
      );
      // @types/node starts with @ but not @/ — should not match
      expect(hints).toHaveLength(0);
    });
  });

  // -----------------------------------------------------------------------
  // No hints for unrecognized errors
  // -----------------------------------------------------------------------

  describe("unrecognized errors", () => {
    it("returns empty array for generic TypeError", () => {
      const hints = detectRuntimeHints(
        makeError("TypeError", "Cannot read properties of null (reading 'length')"),
      );
      expect(hints).toHaveLength(0);
    });

    it("returns empty array for generic Error", () => {
      const hints = detectRuntimeHints(
        makeError("Error", "Something went wrong"),
      );
      expect(hints).toHaveLength(0);
    });
  });

  // -----------------------------------------------------------------------
  // Multiple hints from one error
  // -----------------------------------------------------------------------

  describe("multiple hints", () => {
    it("can produce multiple hints from a compound error", () => {
      // An error message that matches both import.meta.env and contains import.meta.env
      const hints = detectRuntimeHints(
        makeError("ReferenceError", "window is not defined while accessing import.meta.env"),
      );
      // Should get both browser-globals and import-meta-env hints
      expect(hints.length).toBeGreaterThanOrEqual(2);
      const adapterIds = hints.map(h => h.adapter.id);
      expect(adapterIds).toContain(ADAPTER_ID_BROWSER_GLOBALS);
      expect(adapterIds).toContain(ADAPTER_ID_IMPORT_META_ENV);
    });
  });
});
