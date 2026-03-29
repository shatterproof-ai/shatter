/**
 * Static heuristic: infer integer type refinement from usage patterns.
 *
 * Scans a function body's AST for signals that a `number` parameter is
 * integer-intended, and refines its TypeInfo from Float to Int when
 * sufficient signals are found. Runs during the analyze phase (no execution
 * needed).
 *
 * When the behavioral float probe (str-1hnm) is also available, it
 * overrides this heuristic.
 */

import * as ts from "typescript";
import type { ParamInfo } from "./protocol.js";

/** Minimum number of distinct signal categories required to refine float -> int. */
const MIN_INTEGER_SIGNALS = 2;

/** Parameter name words that suggest integer intent. */
const INTEGER_NAME_WORDS = new Set([
  "int", "integer", "count", "index", "idx", "length", "len",
  "size", "offset", "id", "num", "depth", "width", "height",
  "port", "year", "month", "day", "step", "limit", "page",
  "rank", "level", "position", "row", "col",
]);

/** Split a camelCase or snake_case name into lowercase words. */
function splitNameWords(name: string): string[] {
  return name
    .replace(/([a-z])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .split(/[_-]+/)
    .filter((w) => w.length > 0);
}

/** Check whether a node is an identifier matching the target parameter name. */
function isParamRef(node: ts.Node, paramName: string): boolean {
  return ts.isIdentifier(node) && node.text === paramName;
}

/** Check whether a numeric literal node has an integer value. */
function isIntegerLiteral(node: ts.Node): boolean {
  return ts.isNumericLiteral(node) && Number.isInteger(Number(node.text));
}

/** Check whether an expression is `Math.<method>`. */
function isMathMethod(expr: ts.Expression, method: string): boolean {
  return (
    ts.isPropertyAccessExpression(expr) &&
    ts.isIdentifier(expr.expression) &&
    expr.expression.text === "Math" &&
    expr.name.text === method
  );
}

/** Check whether a call is `parseInt(...)`. */
function isParseIntCall(expr: ts.Expression): boolean {
  return ts.isIdentifier(expr) && expr.text === "parseInt";
}

const COMPARISON_OPS = new Set([
  ts.SyntaxKind.LessThanToken,
  ts.SyntaxKind.LessThanEqualsToken,
  ts.SyntaxKind.GreaterThanToken,
  ts.SyntaxKind.GreaterThanEqualsToken,
  ts.SyntaxKind.EqualsEqualsToken,
  ts.SyntaxKind.EqualsEqualsEqualsToken,
  ts.SyntaxKind.ExclamationEqualsToken,
  ts.SyntaxKind.ExclamationEqualsEqualsToken,
]);

const INTEGER_ARITHMETIC_OPS = new Set([
  ts.SyntaxKind.PlusEqualsToken,
  ts.SyntaxKind.MinusEqualsToken,
  ts.SyntaxKind.AsteriskEqualsToken,
  ts.SyntaxKind.PercentToken,
  ts.SyntaxKind.PercentEqualsToken,
]);

/**
 * Count distinct integer-intent signal categories for a parameter in a function body.
 *
 * Returns the number of signal categories that fired (max 5). Each category
 * fires at most once. A fractional veto (e.g., `.toFixed()` on the param)
 * suppresses all signals.
 */
export function countIntegerSignals(
  body: ts.Node,
  paramName: string,
  sourceFile: ts.SourceFile,
): number {
  let hasComparisonSignal = false;
  let hasCoercionSignal = false;
  let hasArithmeticSignal = false;
  let hasNamingSignal = false;
  let hasJSDocSignal = false;
  let hasFractionalVeto = false;

  // Signal 4: Parameter naming convention (no walk needed)
  const words = splitNameWords(paramName);
  if (words.some((w) => INTEGER_NAME_WORDS.has(w))) {
    hasNamingSignal = true;
  }

  // Signal 5: JSDoc annotation — scan source text before the function body
  const parent = body.parent;
  if (parent) {
    const fullText = sourceFile.getFullText();
    const triviaStart = parent.getFullStart();
    const triviaEnd = parent.getStart(sourceFile);
    const trivia = fullText.slice(triviaStart, triviaEnd);
    // Match @param {integer} paramName or @param {int} paramName
    const jsdocPattern = new RegExp(
      `@param\\s+\\{\\s*(?:integer|int)\\s*\\}\\s+(?:\\[?${escapeRegExp(paramName)}\\]?)`,
    );
    if (jsdocPattern.test(trivia)) {
      hasJSDocSignal = true;
    }
  }

  function walk(node: ts.Node): void {
    if (hasFractionalVeto) return;

    // Skip nested function bodies — their signals are irrelevant to outer params
    if (
      (ts.isFunctionDeclaration(node) ||
        ts.isArrowFunction(node) ||
        ts.isFunctionExpression(node)) &&
      node !== body.parent
    ) {
      return;
    }

    // Check for fractional veto: param.toFixed()
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === "toFixed" &&
      isParamRef(node.expression.expression, paramName)
    ) {
      hasFractionalVeto = true;
      return;
    }

    // Check for fractional veto: Math.round(param)
    if (
      ts.isCallExpression(node) &&
      isMathMethod(node.expression, "round") &&
      node.arguments.length > 0 &&
      isParamRef(node.arguments[0]!, paramName)
    ) {
      hasFractionalVeto = true;
      return;
    }

    // Check for fractional veto: param % 1
    if (
      ts.isBinaryExpression(node) &&
      node.operatorToken.kind === ts.SyntaxKind.PercentToken &&
      isParamRef(node.left, paramName) &&
      ts.isNumericLiteral(node.right) &&
      node.right.text === "1"
    ) {
      hasFractionalVeto = true;
      return;
    }

    // Check for fractional veto: param compared against Math.floor/trunc/ceil(param)
    // e.g. `x !== Math.floor(x)` or `Math.floor(x) !== x` — float-sensitivity check
    if (ts.isBinaryExpression(node) && COMPARISON_OPS.has(node.operatorToken.kind)) {
      const isMathRoundingCall = (n: ts.Node): boolean =>
        ts.isCallExpression(n) &&
        (isMathMethod(n.expression, "floor") ||
          isMathMethod(n.expression, "trunc") ||
          isMathMethod(n.expression, "ceil")) &&
        n.arguments.length > 0 &&
        isParamRef(n.arguments[0]!, paramName);

      if (
        (isParamRef(node.left, paramName) && isMathRoundingCall(node.right)) ||
        (isMathRoundingCall(node.left) && isParamRef(node.right, paramName))
      ) {
        hasFractionalVeto = true;
        return;
      }
    }

    // Signal 1: Integer comparison literals
    if (ts.isBinaryExpression(node) && COMPARISON_OPS.has(node.operatorToken.kind)) {
      if (
        (isParamRef(node.left, paramName) && isIntegerLiteral(node.right)) ||
        (isParamRef(node.right, paramName) && isIntegerLiteral(node.left))
      ) {
        hasComparisonSignal = true;
      }
    }

    // Signal 2: Integer coercion calls
    if (ts.isCallExpression(node)) {
      const callee = node.expression;
      const hasParamArg =
        node.arguments.length > 0 && isParamRef(node.arguments[0]!, paramName);
      if (
        hasParamArg &&
        (isMathMethod(callee, "floor") ||
          isMathMethod(callee, "trunc") ||
          isMathMethod(callee, "ceil") ||
          isParseIntCall(callee))
      ) {
        hasCoercionSignal = true;
      }
    }

    // Signal 2 (bitwise coercion): param | 0, param >>> 0
    if (ts.isBinaryExpression(node)) {
      const op = node.operatorToken.kind;
      if (
        (op === ts.SyntaxKind.BarToken ||
          op === ts.SyntaxKind.GreaterThanGreaterThanGreaterThanToken) &&
        isParamRef(node.left, paramName) &&
        ts.isNumericLiteral(node.right) &&
        node.right.text === "0"
      ) {
        hasCoercionSignal = true;
      }
    }

    // Signal 3: Integer arithmetic with integer literal
    if (ts.isBinaryExpression(node) && INTEGER_ARITHMETIC_OPS.has(node.operatorToken.kind)) {
      if (
        (isParamRef(node.left, paramName) && isIntegerLiteral(node.right)) ||
        (isParamRef(node.right, paramName) && isIntegerLiteral(node.left))
      ) {
        hasArithmeticSignal = true;
      }
    }

    ts.forEachChild(node, walk);
  }

  walk(body);

  if (hasFractionalVeto) return 0;

  let count = 0;
  if (hasComparisonSignal) count++;
  if (hasCoercionSignal) count++;
  if (hasArithmeticSignal) count++;
  if (hasNamingSignal) count++;
  if (hasJSDocSignal) count++;
  return count;
}

/**
 * Post-process analyzed params: refine float -> int when sufficient integer
 * signals exist in the function body. Returns a new array; does not mutate.
 */
export function refineIntegerParams(
  params: ParamInfo[],
  body: ts.Node | undefined,
  sourceFile: ts.SourceFile,
): ParamInfo[] {
  if (!body) return params;
  return params.map((p) => {
    if (p.type.kind !== "float") return p;
    const signals = countIntegerSignals(body, p.name, sourceFile);
    if (signals >= MIN_INTEGER_SIGNALS) {
      return { ...p, type: { kind: "int" as const } };
    }
    return p;
  });
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
