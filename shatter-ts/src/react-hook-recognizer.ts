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
}

/**
 * Scan top-level import declarations for React hook imports.
 */
function collectReactImports(sourceFile: ts.SourceFile): ReactImportContext {
  const importedBuiltinHooks = new Set<string>();
  const allReactImports = new Set<string>();
  let hasReactImport = false;

  for (const stmt of sourceFile.statements) {
    if (!ts.isImportDeclaration(stmt)) continue;
    const moduleSpec = stmt.moduleSpecifier;
    if (!ts.isStringLiteral(moduleSpec)) continue;

    const moduleName = moduleSpec.text;
    if (!REACT_HOOK_MODULES.has(moduleName)) continue;

    hasReactImport = true;

    const namedBindings = stmt.importClause?.namedBindings;
    if (namedBindings) {
      if (ts.isNamedImports(namedBindings)) {
        for (const element of namedBindings.elements) {
          const name = element.name.text;
          allReactImports.add(name);
          if (BUILTIN_REACT_HOOKS.has(name)) {
            importedBuiltinHooks.add(name);
          }
        }
      } else if (ts.isNamespaceImport(namedBindings)) {
        allReactImports.add(namedBindings.name.text);
      }
    }

    // Default import: import React from "react"
    const defaultImport = stmt.importClause?.name;
    if (defaultImport) {
      allReactImports.add(defaultImport.text);
    }
  }

  return { importedBuiltinHooks, allReactImports, hasReactImport };
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
      // Check variable declarations wrapping arrow/function expressions
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

  // No React imports at all → no hooks possible
  if (!importCtx.hasReactImport) {
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

    // Medium signal: calls custom hooks
    for (const hookName of customHookCalls) {
      reasons.push(`Calls custom hook ${hookName}`);
    }

    const hasBuiltinCalls = builtinHookCalls.length > 0;
    const hasCustomCalls = customHookCalls.length > 0;
    const hasHookName = isHookName(fn.name);

    // Strong signal: a PascalCase function whose body contains JSX is a
    // React function component. These cannot run raw — their JSX children
    // call hooks that require a React dispatcher.
    const isJsxComponent =
      !hasBuiltinCalls &&
      !hasCustomCalls &&
      isComponentName(fn.name) &&
      bodyContainsJsx(body);

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
