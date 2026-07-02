/**
 * React hook recognizer — detects functions that are React hooks and emits
 * adapter hints for the core engine.
 *
 * Detection signals (strongest → weakest):
 * 1. Import signal: function body calls a known React hook imported from a
 *    React package (e.g. useState from "react").
 * 2. Hook call signal: function body calls a useXxx function that itself
 *    uses React hooks (custom hook pattern).
 * 3. Name signal: function name matches the useXxx convention. This is a
 *    supporting signal only — it never triggers a hint on its own.
 */

import * as ts from "typescript";
import type { AdapterHint, FunctionAnalysis } from "./protocol.js";
import { REACT_MODULE_NAMES } from "./react-shim.js";

/** Adapter ID for React hook hints. */
export const REACT_HOOK_ADAPTER_ID = "react-hook";

/** Built-in React hooks that serve as strong import signals. */
export const BUILTIN_REACT_HOOKS = new Set([
  "useState",
  "useEffect",
  "useReducer",
  "useCallback",
  "useMemo",
  "useRef",
  "useContext",
  "useLayoutEffect",
  "useId",
  "useImperativeHandle",
  "useDebugValue",
  "useDeferredValue",
  "useTransition",
  "useSyncExternalStore",
  "useInsertionEffect",
]);

/** Extended set of React module specifiers that can provide hooks. */
const REACT_HOOK_MODULES = new Set([
  ...REACT_MODULE_NAMES,
  "react-dom",
  "react-dom/client",
]);

/**
 * Returns true if `name` follows the React hook naming convention:
 * starts with "use" followed by an uppercase letter.
 */
export function isHookName(name: string): boolean {
  return name.length > 3 && name.startsWith("use") && name[3]! >= "A" && name[3]! <= "Z";
}

/**
 * Returns true if `fileName` uses a JSX-capable extension (`.tsx` / `.jsx`).
 *
 * With the React 17+ automatic JSX runtime a component needs no `react`
 * import, so a JSX-bearing body in a `.tsx`/`.jsx` file is treated as a
 * component even absent any React import (str-cd4ur). The decision is
 * extension + JSX-in-body only; tsconfig `jsx` settings are intentionally
 * not consulted.
 */
export function isJsxFileName(fileName: string): boolean {
  return fileName.endsWith(".tsx") || fileName.endsWith(".jsx");
}

/**
 * Returns true if `name` looks like a React function component:
 * starts with an uppercase ASCII letter. Component names are PascalCase
 * by React convention; lowercase names are reserved for host elements
 * and ordinary helpers.
 */
export function isComponentName(name: string): boolean {
  if (name.length === 0) return false;
  const c = name[0]!;
  return c >= "A" && c <= "Z";
}

/**
 * Walk a function body and return true if it contains any JSX syntax.
 */
function bodyContainsJsx(body: ts.Node): boolean {
  let found = false;
  function visit(node: ts.Node): void {
    if (found) return;
    if (
      ts.isJsxElement(node) ||
      ts.isJsxSelfClosingElement(node) ||
      ts.isJsxFragment(node)
    ) {
      found = true;
      return;
    }
    ts.forEachChild(node, visit);
  }
  visit(body);
  return found;
}

/** Collected import-level information about React hooks in a source file. */
interface ReactImportContext {
  /** Names imported from React modules that are builtin hooks. */
  importedBuiltinHooks: Set<string>;
  /** All names imported from React modules (hooks + non-hooks). */
  allReactImports: Set<string>;
  /** Whether the file imports from any React module at all. */
  hasReactImport: boolean;
  /**
   * `useXxx`-named identifiers imported from ANY module (react or third-party
   * hook packages like `@mantine/hooks`, store hooks, etc.). A call to one of
   * these is treated as a hook-usage signal even without a React import
   * (str-cd4ur). Builtin-hook detection is unchanged — this only widens the
   * custom-hook signal.
   */
  hookImports: Set<string>;
}

/**
 * Scan top-level import declarations for React hook imports.
 */
function collectReactImports(sourceFile: ts.SourceFile): ReactImportContext {
  const importedBuiltinHooks = new Set<string>();
  const allReactImports = new Set<string>();
  const hookImports = new Set<string>();
  let hasReactImport = false;

  for (const stmt of sourceFile.statements) {
    if (!ts.isImportDeclaration(stmt)) continue;
    const moduleSpec = stmt.moduleSpecifier;
    if (!ts.isStringLiteral(moduleSpec)) continue;

    const moduleName = moduleSpec.text;
    const isReactModule = REACT_HOOK_MODULES.has(moduleName);
    if (isReactModule) {
      hasReactImport = true;
    }

    // Collect `useXxx`-named bindings from ANY module as hook-usage signals.
    const namedBindings = stmt.importClause?.namedBindings;
    if (namedBindings && ts.isNamedImports(namedBindings)) {
      for (const element of namedBindings.elements) {
        const name = element.name.text;
        if (isHookName(name)) {
          hookImports.add(name);
        }
        if (isReactModule) {
          allReactImports.add(name);
          if (BUILTIN_REACT_HOOKS.has(name)) {
            importedBuiltinHooks.add(name);
          }
        }
      }
    }

    // React-module-only bookkeeping for namespace / default imports.
    if (isReactModule) {
      if (namedBindings && ts.isNamespaceImport(namedBindings)) {
        allReactImports.add(namedBindings.name.text);
      }
      // Default import: import React from "react"
      const defaultImport = stmt.importClause?.name;
      if (defaultImport) {
        allReactImports.add(defaultImport.text);
      }
    }
  }

  return { importedBuiltinHooks, allReactImports, hasReactImport, hookImports };
}

/** Result of scanning a function body for hook call sites. */
interface HookCallScan {
  /** Builtin React hooks called directly (e.g. useState). */
  builtinHookCalls: string[];
  /** Custom hook calls (useXxx functions not in the builtin set). */
  customHookCalls: string[];
}

/**
 * Walk a function body and collect all call expressions that look like
 * React hook calls.
 */
function scanFunctionForHookCalls(
  body: ts.Node,
  importCtx: ReactImportContext,
): HookCallScan {
  const builtinHookCalls: string[] = [];
  const customHookCalls: string[] = [];

  function visit(node: ts.Node): void {
    if (ts.isCallExpression(node)) {
      const callee = node.expression;
      let calleeName: string | undefined;

      if (ts.isIdentifier(callee)) {
        calleeName = callee.text;
      } else if (ts.isPropertyAccessExpression(callee) && ts.isIdentifier(callee.name)) {
        // React.useState() style
        calleeName = callee.name.text;
      }

      if (calleeName) {
        const isImportedBuiltin =
          importCtx.importedBuiltinHooks.has(calleeName) ||
          (ts.isPropertyAccessExpression(callee) &&
            ts.isIdentifier(callee.expression) &&
            importCtx.allReactImports.has(callee.expression.text) &&
            BUILTIN_REACT_HOOKS.has(calleeName));

        if (isImportedBuiltin) {
          builtinHookCalls.push(calleeName);
        } else if (isHookName(calleeName) && !BUILTIN_REACT_HOOKS.has(calleeName)) {
          customHookCalls.push(calleeName);
        }
      }
    }
    ts.forEachChild(node, visit);
  }

  visit(body);
  return { builtinHookCalls, customHookCalls };
}

/**
 * Higher-order component wrappers whose call expressions wrap a component
 * function (`memo(Fn)`, `forwardRef(Fn)`, and the `React.`-qualified forms).
 * findFunctionBody unwraps these to the inner function (str-cd4ur).
 */
const COMPONENT_HOC_WRAPPERS = new Set(["memo", "forwardRef"]);

/**
 * Returns the callee name of a call expression if it is a bare identifier
 * (`memo(...)`) or a property access (`React.memo(...)`), else undefined.
 */
function calleeName(call: ts.CallExpression): string | undefined {
  const callee = call.expression;
  if (ts.isIdentifier(callee)) return callee.text;
  if (ts.isPropertyAccessExpression(callee) && ts.isIdentifier(callee.name)) {
    return callee.name.text;
  }
  return undefined;
}

/**
 * Unwrap an expression to the underlying arrow / function expression,
 * peeling any HOC wrappers (`memo`, `forwardRef`, possibly nested) off the
 * outside. Returns undefined if no function is reachable.
 */
function unwrapToFunction(
  expr: ts.Expression,
): ts.ArrowFunction | ts.FunctionExpression | undefined {
  if (ts.isArrowFunction(expr) || ts.isFunctionExpression(expr)) {
    return expr;
  }
  if (ts.isCallExpression(expr)) {
    const name = calleeName(expr);
    if (name && COMPONENT_HOC_WRAPPERS.has(name) && expr.arguments.length > 0) {
      return unwrapToFunction(expr.arguments[0]!);
    }
  }
  return undefined;
}

/**
 * The body node to analyze for a function-like node. For arrow functions this
 * is the concise body (block or expression); for declarations / function
 * expressions it is the block (or undefined for an overload signature).
 */
function functionBodyNode(
  fn: ts.ArrowFunction | ts.FunctionExpression | ts.FunctionDeclaration,
): ts.Node | undefined {
  return fn.body;
}

/**
 * Find the function body AST node for a given FunctionAnalysis by matching
 * on start/end line range.
 *
 * Handles plain declarations, arrow/function expressions, const-assigned
 * arrow/function components, and HOC-wrapped components (`const C =
 * memo(fn)` / `forwardRef(fn)`) by unwrapping the call expression to the
 * inner function (str-cd4ur). Line matching accepts either the declaration's
 * initializer range or the inner function's own range, since analyzers may
 * report either.
 */
function findFunctionBody(
  sourceFile: ts.SourceFile,
  analysis: FunctionAnalysis,
): ts.Node | undefined {
  let result: ts.Node | undefined;

  const lineRange = (node: ts.Node): [number, number] => [
    sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
    sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1,
  ];
  const matches = (node: ts.Node): boolean => {
    const [s, e] = lineRange(node);
    return s === analysis.start_line && e === analysis.end_line;
  };

  function visit(node: ts.Node): void {
    if (matches(node)) {
      if (ts.isFunctionDeclaration(node) && node.body) {
        result = node.body;
      } else if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) {
        result = functionBodyNode(node);
      }
    }

    if (!result) {
      // Variable declarations wrapping arrow/function expressions or HOC calls.
      if (ts.isVariableDeclaration(node) && node.initializer) {
        const inner = unwrapToFunction(node.initializer);
        if (inner && (matches(node.initializer) || matches(inner))) {
          result = functionBodyNode(inner);
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

/**
 * Recognize React hooks among analyzed functions and return adapter hints.
 *
 * Returns an array parallel to `functions` — each entry is either an
 * `AdapterHint` if the function is a detected React hook, or `undefined`.
 */
export function recognizeReactHooks(
  sourceFile: ts.SourceFile,
  functions: readonly FunctionAnalysis[],
): (AdapterHint | undefined)[] {
  const importCtx = collectReactImports(sourceFile);
  const isJsxFile = isJsxFileName(sourceFile.fileName);

  // Recognition is possible when the file imports from React, imports any
  // useXxx-named hook (third-party or store hook), or is a JSX-capable file
  // (.tsx/.jsx). Absent all three, no function can be a recognized hook or
  // component and we early-return to preserve prior behavior for plain .ts.
  if (!importCtx.hasReactImport && importCtx.hookImports.size === 0 && !isJsxFile) {
    return functions.map(() => undefined);
  }

  return functions.map((fn) => {
    const body = findFunctionBody(sourceFile, fn);
    if (!body) return undefined;

    const { builtinHookCalls, customHookCalls } = scanFunctionForHookCalls(body, importCtx);
    const reasons: string[] = [];

    // Strong signal: calls builtin React hooks
    for (const hookName of builtinHookCalls) {
      reasons.push(`Calls ${hookName} imported from 'react'`);
    }

    // Medium signal: calls custom hooks. Without a React import in the file,
    // only useXxx calls that resolve to an imported hook (from any module)
    // count — a locally-defined useXxx helper is not a hook-usage signal.
    // Within a React-importing file, any useXxx call counts (legacy behavior).
    const qualifyingCustomCalls = importCtx.hasReactImport
      ? customHookCalls
      : customHookCalls.filter((name) => importCtx.hookImports.has(name));
    for (const hookName of qualifyingCustomCalls) {
      reasons.push(`Calls custom hook ${hookName}`);
    }

    const hasBuiltinCalls = builtinHookCalls.length > 0;
    const hasCustomCalls = qualifyingCustomCalls.length > 0;
    const hasHookName = isHookName(fn.name);

    // Strong signal: a PascalCase function whose body contains JSX is a
    // React function component. These cannot run raw — they (and their JSX
    // children) require a React dispatcher, so they must execute under the
    // react-hook adapter. This holds even when the component also calls a
    // custom/store hook, so it is deliberately independent of the hook-call
    // signals: a JSX component is a high-confidence, auto-appliable target
    // regardless (str-cd4ur — otherwise a JSX component that also calls a
    // store hook would be demoted to medium/suggested and never applied,
    // leaving it to crash on the raw path). A React import OR a JSX-capable
    // file extension (automatic JSX runtime) is required so we don't tag
    // JSX-shaped helpers in plain .ts files.
    const isJsxComponent =
      isComponentName(fn.name) &&
      bodyContainsJsx(body) &&
      (importCtx.hasReactImport || isJsxFile);

    if (isJsxComponent) {
      reasons.push("Returns JSX (PascalCase function component)");
    }

    // Name signal alone is not sufficient
    if (!hasBuiltinCalls && !hasCustomCalls && !isJsxComponent) {
      return undefined;
    }

    if (hasHookName) {
      reasons.push("Follows useXxx naming convention");
    }

    const confidence = hasBuiltinCalls || isJsxComponent ? "high" : "medium";

    return {
      adapter: { id: REACT_HOOK_ADAPTER_ID, apply: "auto" },
      confidence,
      reasons,
    } satisfies AdapterHint;
  });
}
