/**
 * Browser globals recognizer — detects functions that reference browser-specific
 * globals and emits adapter hints for the core engine.
 *
 * Detection is purely static (AST-level). The recognizer walks function bodies
 * and collects references to well-known browser globals such as `window`,
 * `document`, `navigator`, `localStorage`, etc.  A hint is emitted only when
 * concrete global usage is found — directory names or framework branding are
 * never considered.
 */

import * as ts from "typescript";
import type { AdapterHint, FunctionAnalysis } from "./protocol.js";

/** Adapter ID for browser-global hints. */
export const BROWSER_GLOBALS_ADAPTER_ID = "browser-globals";

/**
 * Well-known browser globals grouped by category.
 * Each entry maps a global identifier to a human-readable category label
 * used in hint reasons.
 */
const BROWSER_GLOBALS: ReadonlyMap<string, string> = new Map([
  // DOM / Window
  ["window", "DOM"],
  ["document", "DOM"],
  ["navigator", "Navigator API"],
  ["location", "Location API"],
  ["history", "History API"],

  // Storage
  ["localStorage", "Web Storage"],
  ["sessionStorage", "Web Storage"],

  // Observers
  ["ResizeObserver", "Observer API"],
  ["IntersectionObserver", "Observer API"],
  ["MutationObserver", "Observer API"],

  // Media / Layout
  ["matchMedia", "Media Queries"],
  ["requestAnimationFrame", "Animation API"],
  ["cancelAnimationFrame", "Animation API"],

  // Network
  ["XMLHttpRequest", "XMLHttpRequest"],

  // Dialog
  ["alert", "Browser Dialog"],
  ["confirm", "Browser Dialog"],
  ["prompt", "Browser Dialog"],
]);

/**
 * Globals that are ambiguous because they also exist in Node.js or other
 * runtimes.  These are still detected but yield lower confidence on their own.
 */
const AMBIGUOUS_GLOBALS = new Set(["fetch"]);

/**
 * Find the function body AST node for a given FunctionAnalysis by matching
 * on start/end line range.
 */
function findFunctionBody(
  sourceFile: ts.SourceFile,
  analysis: FunctionAnalysis,
): ts.Node | undefined {
  let result: ts.Node | undefined;

  function visit(node: ts.Node): void {
    const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
    const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

    if (startLine === analysis.start_line && endLine === analysis.end_line) {
      if (ts.isFunctionDeclaration(node) && node.body) {
        result = node.body;
      } else if (ts.isArrowFunction(node)) {
        result = ts.isBlock(node.body) ? node.body : node.body;
      } else if (ts.isFunctionExpression(node) && node.body) {
        result = node.body;
      }
    }

    if (!result) {
      if (ts.isVariableDeclaration(node) && node.initializer) {
        const init = node.initializer;
        if (ts.isArrowFunction(init) || ts.isFunctionExpression(init)) {
          const initStart = sourceFile.getLineAndCharacterOfPosition(init.getStart(sourceFile)).line + 1;
          const initEnd = sourceFile.getLineAndCharacterOfPosition(init.getEnd()).line + 1;
          if (initStart === analysis.start_line && initEnd === analysis.end_line) {
            if (ts.isArrowFunction(init)) {
              result = ts.isBlock(init.body) ? init.body : init.body;
            } else {
              result = init.body;
            }
          }
        }
      }
    }

    if (!result) {
      ts.forEachChild(node, visit);
    }
  }

  ts.forEachChild(sourceFile, visit);
  return result;
}

/** Result of scanning a function body for browser global references. */
interface BrowserGlobalScan {
  /** Distinct browser globals found, preserving discovery order. */
  globals: string[];
  /** Whether any ambiguous global (e.g. fetch) was found. */
  hasAmbiguous: boolean;
}

/**
 * Walk a function body and collect all identifier references that match
 * known browser globals.
 *
 * Only free-standing identifiers and property accesses on `window` are
 * considered (e.g. `document.querySelector(...)` or `window.localStorage`).
 * Local declarations that shadow a global name are intentionally NOT filtered
 * because this is a static heuristic — shadowing a browser global name is
 * itself a signal that the code expects a browser environment.
 */
function scanForBrowserGlobals(body: ts.Node): BrowserGlobalScan {
  const seen = new Set<string>();
  const globals: string[] = [];
  let hasAmbiguous = false;

  function visit(node: ts.Node): void {
    if (ts.isIdentifier(node)) {
      const name = node.text;

      // Skip identifiers that are property names in property accesses
      // (we only want free references, not `obj.window`).
      const parent = node.parent;
      if (parent && ts.isPropertyAccessExpression(parent) && parent.name === node) {
        // Exception: `window.X` where X is a browser global — count X too
        if (
          ts.isIdentifier(parent.expression) &&
          parent.expression.text === "window" &&
          (BROWSER_GLOBALS.has(name) || AMBIGUOUS_GLOBALS.has(name))
        ) {
          if (!seen.has(name)) {
            seen.add(name);
            globals.push(name);
            if (AMBIGUOUS_GLOBALS.has(name)) hasAmbiguous = true;
          }
        }
        // For `window` itself when accessed as `window.something`, count `window`
        // (handled below when we see `parent.expression` as the `window` identifier)
        return;
      }

      // Skip type-only positions (type annotations, type arguments)
      if (parent && (ts.isTypeReferenceNode(parent) || ts.isTypeQueryNode(parent))) {
        return;
      }

      if (BROWSER_GLOBALS.has(name) || AMBIGUOUS_GLOBALS.has(name)) {
        if (!seen.has(name)) {
          seen.add(name);
          globals.push(name);
          if (AMBIGUOUS_GLOBALS.has(name)) hasAmbiguous = true;
        }
      }
    }

    ts.forEachChild(node, visit);
  }

  visit(body);
  return { globals, hasAmbiguous };
}

/**
 * Recognize browser global usage among analyzed functions and return adapter hints.
 *
 * Returns an array parallel to `functions` — each entry is either an
 * `AdapterHint` if browser globals were detected, or `undefined`.
 */
export function recognizeBrowserGlobals(
  sourceFile: ts.SourceFile,
  functions: readonly FunctionAnalysis[],
): (AdapterHint | undefined)[] {
  return functions.map((fn) => {
    const body = findFunctionBody(sourceFile, fn);
    if (!body) return undefined;

    const { globals, hasAmbiguous } = scanForBrowserGlobals(body);
    if (globals.length === 0) return undefined;

    const reasons = globals.map((name) => {
      const category = BROWSER_GLOBALS.get(name) ?? "Browser API";
      return `References ${name} (${category})`;
    });

    // Confidence: high if any non-ambiguous global is present,
    // medium if only ambiguous globals (e.g. fetch alone).
    const hasNonAmbiguous = globals.some((g) => !AMBIGUOUS_GLOBALS.has(g));
    const confidence = hasNonAmbiguous ? "high" : (hasAmbiguous ? "medium" : "high");

    return {
      adapter: { id: BROWSER_GLOBALS_ADAPTER_ID },
      confidence,
      reasons,
    } satisfies AdapterHint;
  });
}
