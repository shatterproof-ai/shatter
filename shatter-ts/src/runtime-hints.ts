/**
 * Runtime hint signals — maps characteristic execution failures into
 * adapter suggestions so the core engine can retry with appropriate adapters.
 *
 * This is frontend-local: the mapping stays in shatter-ts and produces
 * AdapterHint objects that travel on the execute response. Non-adapter
 * failures continue to surface as normal thrown_error values.
 */

import type { AdapterHint, ErrorInfo } from "./protocol.js";

// ---------------------------------------------------------------------------
// Adapter IDs — stable identifiers referenced by the core engine
// ---------------------------------------------------------------------------

export const ADAPTER_ID_REACT_HOOKS = "react-hook";
export const ADAPTER_ID_TSCONFIG_PATHS = "ts/module-resolution/tsconfig-paths";
export const ADAPTER_ID_BROWSER_GLOBALS = "ts/runtime/browser-globals";
export const ADAPTER_ID_IMPORT_META_ENV = "ts/runtime/import-meta-env";

// ---------------------------------------------------------------------------
// Error pattern matchers
// ---------------------------------------------------------------------------

/** Known browser globals that do not exist in Node.js. */
const BROWSER_GLOBALS = [
  "window",
  "document",
  "navigator",
  "localStorage",
  "sessionStorage",
  "location",
  "history",
  "fetch",
  "XMLHttpRequest",
  "WebSocket",
  "requestAnimationFrame",
  "cancelAnimationFrame",
  "getComputedStyle",
  "matchMedia",
  "IntersectionObserver",
  "MutationObserver",
  "ResizeObserver",
  "CustomEvent",
  "DOMParser",
] as const;

/**
 * React "Invalid hook call" error pattern.
 * React throws this when hooks are called outside a React component render cycle.
 */
const INVALID_HOOK_CALL_PATTERN =
  /Invalid hook call|Hooks can only be called inside .* the body of a function component|hooks? (?:can only|must) be called/i;

/**
 * import.meta.env access patterns.
 * Vite and similar bundlers inject import.meta.env at build time; accessing
 * it in a raw Node.js VM yields errors.
 */
const IMPORT_META_ENV_PATTERN =
  /import\.meta\.env|Cannot read propert(?:y|ies) of undefined \(reading 'env'\)|__shatter_import_meta\.env/;

/**
 * Module not found patterns that suggest tsconfig path aliases are in use.
 * Matches bare specifiers (not relative or absolute paths) that fail to resolve.
 */
const MODULE_NOT_FOUND_PATTERN =
  /Cannot find module '([^']+)'|Module not found.*'([^']+)'/;

/**
 * Detect a browser global ReferenceError.
 * Only matches when the error type is ReferenceError and the message names
 * a known browser global.
 */
function matchBrowserGlobal(errorType: string, message: string): string | null {
  if (errorType !== "ReferenceError") return null;
  for (const global of BROWSER_GLOBALS) {
    if (message.includes(`${global} is not defined`)) return global;
  }
  return null;
}

/**
 * Check whether a module specifier looks like a tsconfig path alias
 * (bare specifier, not relative or absolute, not a node_modules package
 * with a scope prefix like @scope/pkg).
 */
function looksLikePathAlias(moduleSpec: string): boolean {
  if (moduleSpec.startsWith(".") || moduleSpec.startsWith("/")) return false;
  // Common path alias patterns: @/foo, ~/foo, src/foo, lib/foo
  if (moduleSpec.startsWith("@/") || moduleSpec.startsWith("~/")) return true;
  // Multi-segment bare specifiers that don't look like npm packages
  if (moduleSpec.includes("/") && !moduleSpec.startsWith("@")) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Inspect a runtime error and return adapter hints for any recognized
 * failure patterns. Returns an empty array for unrecognized errors.
 *
 * The caller should attach these hints to the execute response alongside
 * the original thrown_error — the error itself is never suppressed.
 */
export function detectRuntimeHints(error: ErrorInfo): AdapterHint[] {
  const hints: AdapterHint[] = [];
  const { error_type, message } = error;

  // 1. React invalid hook call
  if (INVALID_HOOK_CALL_PATTERN.test(message)) {
    hints.push({
      adapter: { id: ADAPTER_ID_REACT_HOOKS },
      confidence: "high",
      reasons: ["Runtime error matches React invalid hook call pattern"],
    });
  }

  // 2. Browser global ReferenceError
  const browserGlobal = matchBrowserGlobal(error_type, message);
  if (browserGlobal) {
    hints.push({
      adapter: { id: ADAPTER_ID_BROWSER_GLOBALS },
      confidence: "high",
      reasons: [`ReferenceError: ${browserGlobal} is not defined`],
    });
  }

  // 3. import.meta.env access
  if (IMPORT_META_ENV_PATTERN.test(message)) {
    hints.push({
      adapter: { id: ADAPTER_ID_IMPORT_META_ENV },
      confidence: "high",
      reasons: ["Runtime error indicates import.meta.env access without bundler polyfill"],
    });
  }

  // 4. Unresolved tsconfig path aliases
  const moduleMatch = MODULE_NOT_FOUND_PATTERN.exec(message);
  if (moduleMatch) {
    const moduleSpec = moduleMatch[1] ?? moduleMatch[2] ?? "";
    if (looksLikePathAlias(moduleSpec)) {
      hints.push({
        adapter: { id: ADAPTER_ID_TSCONFIG_PATHS },
        confidence: "medium",
        reasons: [`Module '${moduleSpec}' looks like a tsconfig path alias that failed to resolve`],
      });
    }
  }

  return hints;
}
