/**
 * Source code instrumentor using the TypeScript Compiler API.
 *
 * Rewrites a target function to insert:
 * - __shatter_record(line) calls at each statement for line-level coverage
 * - __shatter_branch(id, line, condition, symExpr) wrappers around branch
 *   conditions for symbolic constraint capture
 *
 * The __shatter_branch() call evaluates the original condition, records a
 * BranchDecision with both the concrete result and the static symbolic
 * expression, and returns the boolean so control flow is preserved.
 */

import ts from "typescript";
import type { SymExpr, BinOpKind, UnOpKind, MockConfig } from "./protocol.js";
import type { TimingCollector } from "./timing.js";

/** Result of instrumenting a source file. */
export interface InstrumentResult {
  /** The full instrumented source code. */
  instrumentedSource: string;
  /** The name of the line-recording function injected into the code. */
  recordFunctionName: string;
  /** The name of the branch-recording function injected into the code. */
  branchFunctionName: string;
  /** Total number of branch points instrumented. */
  branchCount: number;
  /** Number of unique executable statement lines instrumented with __shatter_record(). */
  instrumentableLineCount: number;
}

/**
 * The name of the mock call recording function inserted into instrumented code.
 * Signature: __shatter_mock_call(module, symbol, args, returnValue) → void
 */
export const MOCK_CALL_FUNCTION = "__shatter_mock_call";

/**
 * The name of the global mock registry object.
 * Maps "module:symbol" to mock functions.
 */
export const MOCK_REGISTRY = "__shatter_mocks";

/**
 * The name of the line-recording function inserted into instrumented code.
 * Callers must define this function before executing instrumented code.
 */
export const RECORD_FUNCTION = "__shatter_record";

/**
 * The name of the branch-recording function inserted into instrumented code.
 * Signature: __shatter_branch(branchId, line, conditionResult, symExpr) → boolean
 */
export const BRANCH_FUNCTION = "__shatter_branch";

/**
 * The name of the scope-event recording function inserted into instrumented code.
 * Signature: __shatter_scope_event(scopeId, kind) → void
 */
export const SCOPE_EVENT_FUNCTION = "__shatter_scope_event";

/**
 * The name of the MC/DC condition-recording function inserted into instrumented code.
 * Signature: __shatter_mcdc_record(branchId, symExprs, operator, thunks)
 *   → { decision: boolean; conditions: ConditionOutcome[] }
 */
export const MCDC_RECORD_FUNCTION = "__shatter_mcdc_record";

/**
 * The name of the MC/DC branch-recording function inserted into instrumented code.
 * Signature: __shatter_branch_mcdc(branchId, line, decision, symExpr, conditions) → boolean
 */
export const MCDC_BRANCH_FUNCTION = "__shatter_branch_mcdc";

/**
 * The name of the crypto boundary recording function inserted into instrumented code.
 * Injected before calls to known encrypt/decrypt functions so the executor can
 * capture key, IV, and algorithm values for boundary splitting.
 *
 * Signature: __shatter_crypto_boundary(boundaryId, kind, functionName, ...args) → void
 * - boundaryId: stable string ID for this boundary, e.g. "cb-0"
 * - kind: "encrypt" | "decrypt"
 * - functionName: the function name, e.g. "createDecipheriv"
 * - ...args: the original runtime arguments to the crypto function (key, iv, etc.)
 */
export const CRYPTO_BOUNDARY_FUNCTION = "__shatter_crypto_boundary";

/**
 * Parameter role mappings for known Node.js crypto functions.
 *
 * Maps function name → { argIndex: role }. The executor uses this to extract
 * algorithm, key, IV, and ciphertext index from the captured runtime arguments.
 *
 * Role meanings:
 * - "algorithm": string name of the cipher, e.g. "aes-256-cbc"
 * - "key": encryption/decryption key (Buffer or string)
 * - "iv": initialization vector (Buffer, string, or null)
 * - "data": the data argument — for decrypt functions this is the ciphertext
 */
export const KNOWN_CRYPTO_PARAM_ROLES: Record<string, Record<number, "algorithm" | "key" | "iv" | "data">> = {
  // node:crypto — stream ciphers
  createDecipheriv: { 0: "algorithm", 1: "key", 2: "iv" },
  createCipheriv: { 0: "algorithm", 1: "key", 2: "iv" },
  // node:crypto — RSA / asymmetric
  privateDecrypt: { 1: "data" },
  publicDecrypt: { 1: "data" },
  privateEncrypt: { 1: "data" },
  publicEncrypt: { 1: "data" },
};

/**
 * Set of function names that represent known decrypt operations.
 * Used during instrumentation to decide whether to inject a crypto boundary call.
 */
const KNOWN_DECRYPT_FUNCTION_NAMES = new Set<string>([
  "createDecipheriv",
  "privateDecrypt",
  "publicDecrypt",
]);

/**
 * Set of function names that represent known encrypt operations.
 */
const KNOWN_ENCRYPT_FUNCTION_NAMES = new Set<string>([
  "createCipheriv",
  "privateEncrypt",
  "publicEncrypt",
]);

/** Mutable state threaded through the instrumentation pass. */
interface InstrumentationContext {
  sourceFile: ts.SourceFile;
  factory: ts.NodeFactory;
  paramNames: Set<string>;
  nextBranchId: number;
  nextLoopId: number;
  nextCallSiteId: number;
  /** Counter for crypto boundary IDs — incremented each time a boundary is injected. */
  nextCryptoBoundaryId: number;
  /** Maps local variable names to their symbolic expressions derived from parameters. */
  dataFlowMap: Map<string, SymExpr>;
  /** Unique source lines where __shatter_record() calls were inserted. */
  instrumentableLines: Set<number>;
}

/**
 * Instrument a TypeScript source file, inserting line-recording and
 * branch-recording calls into the specified function.
 *
 * @param source - The original TypeScript source text.
 * @param functionName - The name of the function to instrument.
 * @param fileName - The file name used for parsing (affects diagnostics only).
 * @param mocks - Optional mock configurations for import rewriting.
 * @returns The instrumented source, or an error message.
 */
export function instrumentFunction(
  source: string,
  functionName: string,
  fileName = "input.ts",
  mocks: MockConfig[] = [],
  timing?: TimingCollector,
): InstrumentResult | { error: string } {
  const scriptKind = fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sourceFile = timing
    ? timing.sync("instrument.parse", () => ts.createSourceFile(
      fileName,
      source,
      ts.ScriptTarget.Latest,
      true,
      scriptKind,
    ))
    : ts.createSourceFile(
      fileName,
      source,
      ts.ScriptTarget.Latest,
      true,
      scriptKind,
    );

  const targetFunction = findFunction(sourceFile, functionName);
  if (targetFunction === undefined) {
    return { error: `Function '${functionName}' not found` };
  }

  const finalizeInstrumentation = (): InstrumentResult => {
    const paramNames = extractParamNames(targetFunction, sourceFile);
    const dataFlowMap = buildDataFlowMap(targetFunction, sourceFile, paramNames);

    // Shared mutable branch counter — captured by the transformer closure.
    const branchState = { nextBranchId: 0 };
    const instrumentableLines = new Set<number>();

    // Build mock lookup for import rewriting
    const mocksBySymbol = buildMockLookup(mocks);

    const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });
    const transformed = ts.transform(sourceFile, [
      createInstrumentationTransformer(functionName, paramNames, branchState, dataFlowMap, mocksBySymbol, instrumentableLines),
    ]);
    const result = printer.printFile(transformed.transformed[0] as ts.SourceFile);
    transformed.dispose();

    return {
      instrumentedSource: result,
      recordFunctionName: RECORD_FUNCTION,
      branchFunctionName: BRANCH_FUNCTION,
      branchCount: branchState.nextBranchId,
      instrumentableLineCount: instrumentableLines.size,
    };
  };

  return timing
    ? timing.sync("instrument.transform", finalizeInstrumentation)
    : finalizeInstrumentation();
}

/**
 * Find a function declaration or variable-declared arrow/function expression
 * by name in the top-level statements of a source file.
 */
function findFunction(
  sourceFile: ts.SourceFile,
  name: string,
): ts.FunctionDeclaration | ts.VariableStatement | undefined {
  for (const statement of sourceFile.statements) {
    if (
      ts.isFunctionDeclaration(statement) &&
      statement.name?.text === name
    ) {
      return statement;
    }

    if (ts.isVariableStatement(statement)) {
      for (const decl of statement.declarationList.declarations) {
        if (
          ts.isIdentifier(decl.name) &&
          decl.name.text === name &&
          decl.initializer &&
          (ts.isArrowFunction(decl.initializer) ||
            ts.isFunctionExpression(decl.initializer))
        ) {
          return statement;
        }
      }
    }
  }
  return undefined;
}

/**
 * Extract parameter names from a function declaration or arrow function.
 */
function extractParamNames(
  node: ts.FunctionDeclaration | ts.VariableStatement,
  sourceFile: ts.SourceFile,
): Set<string> {
  const names = new Set<string>();

  if (ts.isFunctionDeclaration(node)) {
    for (const param of node.parameters) {
      if (ts.isIdentifier(param.name)) {
        names.add(param.name.text);
      }
    }
    return names;
  }

  // Variable statement — find the arrow/function expression
  for (const decl of node.declarationList.declarations) {
    if (decl.initializer && (ts.isArrowFunction(decl.initializer) || ts.isFunctionExpression(decl.initializer))) {
      for (const param of decl.initializer.parameters) {
        if (ts.isIdentifier(param.name)) {
          names.add(param.name.text);
        }
      }
    }
  }
  return names;
}

// ---------------------------------------------------------------------------
// Data flow analysis
// ---------------------------------------------------------------------------

/**
 * Build a map from local variable names to their symbolic expressions.
 * Scans variable declarations in the function body where the initializer
 * references parameters (directly or transitively through other locals).
 */
function buildDataFlowMap(
  node: ts.FunctionDeclaration | ts.VariableStatement,
  sourceFile: ts.SourceFile,
  paramNames: Set<string>,
): Map<string, SymExpr> {
  const body = extractFunctionBody(node);
  if (!body) {
    return new Map();
  }

  const flowMap = new Map<string, SymExpr>();

  // Create a combined lookup: params + already-resolved locals
  const resolveName = (name: string): SymExpr | undefined => {
    if (paramNames.has(name)) {
      return { kind: "param", name, path: [] };
    }
    return flowMap.get(name);
  };

  visitStatementsForDataFlow(body.statements, resolveName, flowMap);
  return flowMap;
}

/**
 * Walk statements collecting variable declarations whose initializers
 * can be resolved to symbolic expressions.
 */
function visitStatementsForDataFlow(
  statements: ts.NodeArray<ts.Statement> | ReadonlyArray<ts.Statement>,
  resolveName: (name: string) => SymExpr | undefined,
  flowMap: Map<string, SymExpr>,
): void {
  const stmtArray = Array.from(statements);
  for (let i = 0; i < stmtArray.length; i++) {
    const stmt = stmtArray[i]!;
    if (ts.isVariableStatement(stmt)) {
      for (const decl of stmt.declarationList.declarations) {
        if (ts.isIdentifier(decl.name) && decl.initializer) {
          const symExpr = buildSymExprWithFlow(decl.initializer, resolveName);
          if (symExpr.kind !== "unknown") {
            flowMap.set(decl.name.text, symExpr);
          }
        } else if ((ts.isObjectBindingPattern(decl.name) || ts.isArrayBindingPattern(decl.name)) && decl.initializer) {
          const symExpr = buildSymExprWithFlow(decl.initializer, resolveName);
          if (symExpr.kind !== "unknown") {
            registerDestructuredBindings(decl.name, symExpr, flowMap);
          }
        }
      }
    }
    // Track reassignment expressions: x = expr
    if (ts.isExpressionStatement(stmt)) {
      const expr = stmt.expression;
      if (ts.isBinaryExpression(expr) && expr.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
        if (ts.isIdentifier(expr.left)) {
          const symExpr = buildSymExprWithFlow(expr.right, resolveName);
          if (symExpr.kind !== "unknown") {
            flowMap.set(expr.left.text, symExpr);
          }
        }
      }
    }
    // SSA-style merge for if/else: snapshot flowMap, visit each branch,
    // then merge divergent entries as ite(condition, then_value, else_value).
    if (ts.isIfStatement(stmt)) {
      const condSym = buildSymExprWithFlow(stmt.expression, resolveName);
      const snapshot = new Map(flowMap);

      // Visit then-branch
      visitStatementsForDataFlow(
        statementsFromBranch(stmt.thenStatement),
        resolveName,
        flowMap,
      );
      const thenMap = new Map(flowMap);

      // Restore snapshot for else-branch
      flowMap.clear();
      for (const [k, v] of snapshot) flowMap.set(k, v);

      if (stmt.elseStatement) {
        if (ts.isIfStatement(stmt.elseStatement)) {
          visitStatementsForDataFlow([stmt.elseStatement], resolveName, flowMap);
        } else {
          visitStatementsForDataFlow(
            statementsFromBranch(stmt.elseStatement),
            resolveName,
            flowMap,
          );
        }
      }
      const elseMap = new Map(flowMap);

      // Merge: produce ite for variables that diverge between branches
      mergeFlowMaps(condSym, snapshot, thenMap, elseMap, flowMap);
    }
    if (ts.isBlock(stmt)) {
      visitStatementsForDataFlow(stmt.statements, resolveName, flowMap);
    }

    // Detect closures that capture mutable variables reassigned after this point.
    // Poison those variables to {kind: 'unknown'} so the solver doesn't use stale links.
    const closures = findClosuresInNode(stmt);
    for (const closure of closures) {
      const freeVars = collectFreeIdentifiers(closure);
      for (const varName of freeVars) {
        if (!flowMap.has(varName)) continue;
        if (isConstDeclaration(varName, stmtArray)) continue;
        if (hasReassignmentInStatements(varName, stmtArray, i + 1)) {
          flowMap.set(varName, { kind: "unknown" });
        }
      }
    }
  }
}

/**
 * Merge flow maps from then/else branches using SSA phi-node semantics.
 * For variables that differ between branches, produces an ite(cond, then_val, else_val).
 * Falls back to last-writer-wins when the condition is unknown.
 */
function mergeFlowMaps(
  condSym: SymExpr,
  snapshot: Map<string, SymExpr>,
  thenMap: Map<string, SymExpr>,
  elseMap: Map<string, SymExpr>,
  flowMap: Map<string, SymExpr>,
): void {
  // If condition is unknown, the solver cannot reason about ite —
  // fall back to keeping the else-branch state (last-writer-wins behavior).
  if (condSym.kind === "unknown") {
    flowMap.clear();
    for (const [k, v] of elseMap) flowMap.set(k, v);
    // Also include variables only defined in then-branch
    for (const [k, v] of thenMap) {
      if (!flowMap.has(k)) flowMap.set(k, v);
    }
    return;
  }

  // Collect all variable names across both branches
  const allVars = new Set([...thenMap.keys(), ...elseMap.keys()]);
  flowMap.clear();

  for (const name of allVars) {
    const thenVal = thenMap.get(name);
    const elseVal = elseMap.get(name);
    const preVal = snapshot.get(name);

    // Both branches have the same value (or both undefined) — no divergence
    if (thenVal === elseVal) {
      if (thenVal) flowMap.set(name, thenVal);
      continue;
    }

    // Deep equality check for structurally identical SymExprs
    if (thenVal && elseVal && JSON.stringify(thenVal) === JSON.stringify(elseVal)) {
      flowMap.set(name, thenVal);
      continue;
    }

    // Variable only introduced in one branch (not in pre-if snapshot) —
    // conditionally defined, not reassigned. Keep whichever branch defined it.
    if (!preVal && (!thenVal || !elseVal)) {
      const val = thenVal ?? elseVal;
      if (val) flowMap.set(name, val);
      continue;
    }

    // Divergent values: produce ite
    const thenExpr = thenVal ?? preVal;
    const elseExpr = elseVal ?? preVal;
    if (thenExpr && elseExpr) {
      flowMap.set(name, {
        kind: "ite",
        condition: condSym,
        then_expr: thenExpr,
        else_expr: elseExpr,
      });
    } else if (thenExpr) {
      flowMap.set(name, thenExpr);
    } else if (elseExpr) {
      flowMap.set(name, elseExpr);
    }
  }
}

function statementsFromBranch(stmt: ts.Statement): ReadonlyArray<ts.Statement> {
  if (ts.isBlock(stmt)) {
    return stmt.statements;
  }
  return [stmt];
}

// ---------------------------------------------------------------------------
// Closure mutable-capture detection helpers
// ---------------------------------------------------------------------------

/**
 * Find all arrow functions and function expressions directly contained in a node.
 * Does NOT recurse into nested function bodies.
 */
function findClosuresInNode(node: ts.Node): ts.Node[] {
  const result: ts.Node[] = [];
  function walk(n: ts.Node): void {
    if (ts.isArrowFunction(n) || ts.isFunctionExpression(n)) {
      result.push(n);
      return; // don't recurse into the closure body
    }
    // Skip function declarations — they have their own scope
    if (ts.isFunctionDeclaration(n)) return;
    ts.forEachChild(n, walk);
  }
  ts.forEachChild(node, walk);
  return result;
}

/**
 * Collect all free identifier references in a closure node.
 * Skips the closure's own parameters, local variable declarations inside the
 * closure, property names in property access expressions, and parameters of
 * nested closures (they shadow, not capture).
 */
function collectFreeIdentifiers(closureNode: ts.Node): Set<string> {
  const freeVars = new Set<string>();
  const localDecls = new Set<string>();

  // Collect the closure's own parameter names
  const closureParams = new Set<string>();
  if (ts.isArrowFunction(closureNode) || ts.isFunctionExpression(closureNode)) {
    for (const param of closureNode.parameters) {
      if (ts.isIdentifier(param.name)) {
        closureParams.add(param.name.text);
      }
    }
  }

  function walk(n: ts.Node): void {
    // Skip nested function params (they shadow)
    if (ts.isArrowFunction(n) || ts.isFunctionExpression(n) || ts.isFunctionDeclaration(n)) {
      if (n !== closureNode) return; // don't recurse into nested closures
    }

    // Track local variable declarations inside the closure
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name)) {
      localDecls.add(n.name.text);
    }

    // Skip property names in property access (x.foo — 'foo' is not a free var)
    if (ts.isPropertyAccessExpression(n)) {
      walk(n.expression);
      // Skip n.name — it's a property, not a free identifier
      return;
    }

    if (ts.isIdentifier(n)) {
      const name = n.text;
      if (!closureParams.has(name) && !localDecls.has(name)) {
        freeVars.add(name);
      }
      return;
    }

    ts.forEachChild(n, walk);
  }

  // Walk the closure body
  if (ts.isArrowFunction(closureNode) || ts.isFunctionExpression(closureNode)) {
    if (closureNode.body) {
      walk(closureNode.body);
    }
  }

  return freeVars;
}

/**
 * Check if a variable is reassigned in statements starting from startIndex.
 * Recursively checks nested blocks, if/else branches, and loops.
 */
function hasReassignmentInStatements(
  varName: string,
  statements: ReadonlyArray<ts.Statement>,
  startIndex: number,
): boolean {
  for (let j = startIndex; j < statements.length; j++) {
    if (hasReassignmentInNode(varName, statements[j]!)) return true;
  }
  return false;
}

function hasReassignmentInNode(varName: string, node: ts.Node): boolean {
  // Simple assignment: x = expr
  if (ts.isBinaryExpression(node)) {
    const assignOps = [
      ts.SyntaxKind.EqualsToken,
      ts.SyntaxKind.PlusEqualsToken,
      ts.SyntaxKind.MinusEqualsToken,
      ts.SyntaxKind.AsteriskEqualsToken,
      ts.SyntaxKind.SlashEqualsToken,
      ts.SyntaxKind.PercentEqualsToken,
      ts.SyntaxKind.AmpersandEqualsToken,
      ts.SyntaxKind.BarEqualsToken,
      ts.SyntaxKind.CaretEqualsToken,
      ts.SyntaxKind.LessThanLessThanEqualsToken,
      ts.SyntaxKind.GreaterThanGreaterThanEqualsToken,
      ts.SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken,
    ];
    if (assignOps.includes(node.operatorToken.kind) && ts.isIdentifier(node.left) && node.left.text === varName) {
      return true;
    }
  }
  // Prefix/postfix increment/decrement: ++x, x++, --x, x--
  if (ts.isPrefixUnaryExpression(node) || ts.isPostfixUnaryExpression(node)) {
    const op = node.operator;
    if ((op === ts.SyntaxKind.PlusPlusToken || op === ts.SyntaxKind.MinusMinusToken) &&
        ts.isIdentifier(node.operand) && node.operand.text === varName) {
      return true;
    }
  }
  // Don't recurse into nested function bodies — they have their own scope
  if (ts.isFunctionDeclaration(node) || ts.isArrowFunction(node) || ts.isFunctionExpression(node)) {
    return false;
  }
  let found = false;
  ts.forEachChild(node, (child) => {
    if (!found && hasReassignmentInNode(varName, child)) {
      found = true;
    }
  });
  return found;
}

/**
 * Check if a variable was declared with `const` in the given statements.
 */
function isConstDeclaration(
  varName: string,
  statements: ReadonlyArray<ts.Statement>,
): boolean {
  for (const stmt of statements) {
    if (ts.isVariableStatement(stmt)) {
      const isConst = (stmt.declarationList.flags & ts.NodeFlags.Const) !== 0;
      if (isConst) {
        for (const decl of stmt.declarationList.declarations) {
          if (ts.isIdentifier(decl.name) && decl.name.text === varName) {
            return true;
          }
          // Check destructured bindings
          if (ts.isObjectBindingPattern(decl.name) || ts.isArrayBindingPattern(decl.name)) {
            for (const el of decl.name.elements) {
              if (ts.isBindingElement(el) && ts.isIdentifier(el.name) && el.name.text === varName) {
                return true;
              }
            }
          }
        }
      }
    }
  }
  return false;
}

/**
 * Append a property segment to a param SymExpr's path.
 * Returns null for non-param expressions (can't track property access on computed values).
 */
function appendPathSegment(expr: SymExpr, segment: string): SymExpr | null {
  if (expr.kind === "param") {
    return { kind: "param", name: expr.name, path: [...expr.path, segment] };
  }
  return null;
}

/**
 * Walk a destructuring pattern and register each binding in the flow map
 * with the appropriate property path appended to the base expression.
 */
function registerDestructuredBindings(
  pattern: ts.BindingPattern,
  baseExpr: SymExpr,
  flowMap: Map<string, SymExpr>,
): void {
  if (ts.isObjectBindingPattern(pattern)) {
    for (const element of pattern.elements) {
      if (element.dotDotDotToken) {
        continue;
      }
      // Renamed binding: {a: renamed} uses propertyName="a", name="renamed"
      // Direct binding: {a} uses propertyName=undefined, name="a"
      const propName = element.propertyName
        ? (ts.isIdentifier(element.propertyName) ? element.propertyName.text : null)
        : (ts.isIdentifier(element.name) ? element.name.text : null);
      if (!propName) continue;

      const childExpr = appendPathSegment(baseExpr, propName);
      if (!childExpr) continue;

      if (ts.isIdentifier(element.name)) {
        flowMap.set(element.name.text, childExpr);
      } else if ((ts.isObjectBindingPattern(element.name) || ts.isArrayBindingPattern(element.name))) {
        registerDestructuredBindings(element.name, childExpr, flowMap);
      }
    }
  } else if (ts.isArrayBindingPattern(pattern)) {
    for (let i = 0; i < pattern.elements.length; i++) {
      const element = pattern.elements[i]!;
      if (ts.isOmittedExpression(element)) continue;
      if (element.dotDotDotToken) continue;

      const childExpr = appendPathSegment(baseExpr, String(i));
      if (!childExpr) continue;

      if (ts.isIdentifier(element.name)) {
        flowMap.set(element.name.text, childExpr);
      } else if ((ts.isObjectBindingPattern(element.name) || ts.isArrayBindingPattern(element.name))) {
        registerDestructuredBindings(element.name, childExpr, flowMap);
      }
    }
  }
}

/**
 * Build a SymExpr from an expression, resolving local variables via the
 * flow-sensitive resolveName lookup (which checks params and already-mapped locals).
 */
export function buildSymExprWithFlow(
  expr: ts.Expression,
  resolveName: (name: string) => SymExpr | undefined,
): SymExpr {
  if (ts.isParenthesizedExpression(expr)) {
    return buildSymExprWithFlow(expr.expression, resolveName);
  }

  if (ts.isIdentifier(expr)) {
    const resolved = resolveName(expr.text);
    if (resolved) {
      return resolved;
    }
    return { kind: "unknown" };
  }

  if (ts.isNumericLiteral(expr)) {
    const n = Number(expr.text);
    if (Number.isInteger(n)) {
      return { kind: "const", type: "int", value: n };
    }
    return { kind: "const", type: "float", value: n };
  }

  if (ts.isStringLiteral(expr)) {
    return { kind: "const", type: "str", value: expr.text };
  }

  if (expr.kind === ts.SyntaxKind.TrueKeyword) {
    return { kind: "const", type: "bool", value: true };
  }

  if (expr.kind === ts.SyntaxKind.FalseKeyword) {
    return { kind: "const", type: "bool", value: false };
  }

  if (expr.kind === ts.SyntaxKind.NullKeyword) {
    return { kind: "const", type: "null" };
  }

  if (ts.isBinaryExpression(expr)) {
    const op = binaryTokenToOp(expr.operatorToken.kind);
    if (op) {
      const left = buildSymExprWithFlow(expr.left, resolveName);
      const right = buildSymExprWithFlow(expr.right, resolveName);
      // Only produce a symbolic bin_op if at least one side is non-unknown
      if (left.kind !== "unknown" || right.kind !== "unknown") {
        return { kind: "bin_op", op, left, right };
      }
    }
    return { kind: "unknown" };
  }

  if (ts.isPrefixUnaryExpression(expr)) {
    const op = unaryTokenToOp(expr.operator);
    if (op) {
      const operand = buildSymExprWithFlow(expr.operand, resolveName);
      if (operand.kind !== "unknown") {
        return { kind: "un_op", op, operand };
      }
    }
    return { kind: "unknown" };
  }

  if (ts.isPropertyAccessExpression(expr)) {
    // Check if this is a property chain starting from a known name
    const chain = resolvePropertyChainWithFlow(expr, resolveName);
    if (chain) {
      return chain;
    }
    return { kind: "unknown" };
  }

  if (ts.isTypeOfExpression(expr)) {
    const operand = buildSymExprWithFlow(expr.expression, resolveName);
    if (operand.kind !== "unknown") {
      return { kind: "un_op", op: "typeof" as UnOpKind, operand };
    }
    return { kind: "unknown" };
  }

  if (ts.isCallExpression(expr)) {
    if (ts.isPropertyAccessExpression(expr.expression)) {
      const methodName = expr.expression.name.text;
      const receiver = buildSymExprWithFlow(expr.expression.expression, resolveName);
      const args = expr.arguments.map((a) => buildSymExprWithFlow(a, resolveName));
      if (receiver.kind !== "unknown" || args.some((a) => a.kind !== "unknown")) {
        return { kind: "call", name: methodName, receiver, args };
      }
      return { kind: "unknown" };
    }
    if (ts.isIdentifier(expr.expression)) {
      const args = expr.arguments.map((a) => buildSymExprWithFlow(a, resolveName));
      if (args.some((a) => a.kind !== "unknown")) {
        return { kind: "call", name: expr.expression.text, receiver: null, args };
      }
      return { kind: "unknown" };
    }
    return { kind: "unknown" };
  }

  return { kind: "unknown" };
}

/**
 * Resolve property access chains using flow-sensitive lookup.
 */
function resolvePropertyChainWithFlow(
  expr: ts.PropertyAccessExpression,
  resolveName: (name: string) => SymExpr | undefined,
): SymExpr | null {
  const path: string[] = [];
  let current: ts.Expression = expr;

  while (ts.isPropertyAccessExpression(current)) {
    path.unshift(current.name.text);
    current = current.expression;
  }

  if (ts.isIdentifier(current)) {
    const resolved = resolveName(current.text);
    if (resolved && resolved.kind === "param") {
      return { kind: "param", name: resolved.name, path: [...resolved.path, ...path] };
    }
  }
  return null;
}

/**
 * Extract the function body from a function declaration or variable statement.
 */
function extractFunctionBody(
  node: ts.FunctionDeclaration | ts.VariableStatement,
): ts.Block | undefined {
  if (ts.isFunctionDeclaration(node) && node.body) {
    return node.body;
  }

  if (ts.isVariableStatement(node)) {
    for (const decl of node.declarationList.declarations) {
      if (decl.initializer) {
        if (ts.isArrowFunction(decl.initializer) && ts.isBlock(decl.initializer.body)) {
          return decl.initializer.body;
        }
        if (ts.isFunctionExpression(decl.initializer) && decl.initializer.body) {
          return decl.initializer.body;
        }
      }
    }
  }

  return undefined;
}

// ---------------------------------------------------------------------------
// Mock support
// ---------------------------------------------------------------------------

/** Parsed mock lookup: maps "module:symbol" to the MockConfig. */
type MockLookup = Map<string, MockConfig>;

/**
 * Build a lookup map from mock configs keyed by "module:symbol".
 * The module is extracted from the symbol field if it contains a colon,
 * otherwise the symbol is used as-is.
 */
function buildMockLookup(mocks: MockConfig[]): MockLookup {
  const lookup = new Map<string, MockConfig>();
  for (const mock of mocks) {
    lookup.set(mock.symbol, mock);
  }
  return lookup;
}

/**
 * Create a TypeScript transformer that instruments a specific function
 * with __shatter_record() and __shatter_branch() calls.
 */
function createInstrumentationTransformer(
  targetFunctionName: string,
  paramNames: Set<string>,
  branchState: { nextBranchId: number },
  dataFlowMap: Map<string, SymExpr> = new Map(),
  mockLookup: MockLookup = new Map(),
  instrumentableLines: Set<number> = new Set(),
): ts.TransformerFactory<ts.SourceFile> {
  return (context) => {
    return (sourceFile) => {
      const ctx: InstrumentationContext = {
        sourceFile,
        factory: context.factory,
        paramNames,
        nextBranchId: 0,
        nextLoopId: 0,
        nextCallSiteId: 0,
        nextCryptoBoundaryId: 0,
        dataFlowMap,
        instrumentableLines,
      };

      const visitor = (node: ts.Node): ts.Node => {
        if (ts.isFunctionDeclaration(node) && node.name?.text === targetFunctionName && node.body) {
          ctx.nextBranchId = 0;
          ctx.nextLoopId = 0;
          ctx.nextCallSiteId = 1;
          ctx.nextCryptoBoundaryId = 0;
          const instrumentedBody = instrumentBlock(node.body, ctx);
          const newBody = wrapFunctionBodyWithCallScope(instrumentedBody, 0, context.factory);
          branchState.nextBranchId = ctx.nextBranchId;
          return context.factory.updateFunctionDeclaration(
            node,
            node.modifiers,
            node.asteriskToken,
            node.name,
            node.typeParameters,
            node.parameters,
            node.type,
            newBody,
          );
        }

        if (ts.isVariableStatement(node)) {
          for (const decl of node.declarationList.declarations) {
            if (
              ts.isIdentifier(decl.name) &&
              decl.name.text === targetFunctionName &&
              decl.initializer
            ) {
              ctx.nextBranchId = 0;

              if (ts.isArrowFunction(decl.initializer) && ts.isBlock(decl.initializer.body)) {
                ctx.nextLoopId = 0;
                ctx.nextCallSiteId = 1;
                const instrumentedBody = instrumentBlock(decl.initializer.body, ctx);
                const newBody = wrapFunctionBodyWithCallScope(instrumentedBody, 0, context.factory);
                branchState.nextBranchId = ctx.nextBranchId;
                const newArrow = context.factory.updateArrowFunction(
                  decl.initializer,
                  decl.initializer.modifiers,
                  decl.initializer.typeParameters,
                  decl.initializer.parameters,
                  decl.initializer.type,
                  decl.initializer.equalsGreaterThanToken,
                  newBody,
                );
                const newDecl = context.factory.updateVariableDeclaration(
                  decl,
                  decl.name,
                  decl.exclamationToken,
                  decl.type,
                  newArrow,
                );
                const newDeclList = context.factory.updateVariableDeclarationList(
                  node.declarationList,
                  [newDecl],
                );
                return context.factory.updateVariableStatement(node, node.modifiers, newDeclList);
              }

              if (ts.isFunctionExpression(decl.initializer) && decl.initializer.body) {
                ctx.nextLoopId = 0;
                ctx.nextCallSiteId = 1;
                const instrumentedBody = instrumentBlock(decl.initializer.body, ctx);
                const newBody = wrapFunctionBodyWithCallScope(instrumentedBody, 0, context.factory);
                branchState.nextBranchId = ctx.nextBranchId;
                const newFn = context.factory.updateFunctionExpression(
                  decl.initializer,
                  decl.initializer.modifiers,
                  decl.initializer.asteriskToken,
                  decl.initializer.name,
                  decl.initializer.typeParameters,
                  decl.initializer.parameters,
                  decl.initializer.type,
                  newBody,
                );
                const newDecl = context.factory.updateVariableDeclaration(
                  decl,
                  decl.name,
                  decl.exclamationToken,
                  decl.type,
                  newFn,
                );
                const newDeclList = context.factory.updateVariableDeclarationList(
                  node.declarationList,
                  [newDecl],
                );
                return context.factory.updateVariableStatement(node, node.modifiers, newDeclList);
              }
            }
          }
        }

        return ts.visitEachChild(node, visitor, context);
      };

      let result = ts.visitNode(sourceFile, visitor) as ts.SourceFile;

      // Rewrite imports for mocked symbols (post-pass to handle multi-node expansion)
      if (mockLookup.size > 0) {
        result = rewriteImportsInSourceFile(result, mockLookup, context.factory);
      }

      return result;
    };
  };
}

// ---------------------------------------------------------------------------
// Crypto boundary injection helpers
// ---------------------------------------------------------------------------

/**
 * Information about a detected crypto function call within a statement.
 */
interface CryptoCallInfo {
  functionName: string;
  kind: "encrypt" | "decrypt";
  args: ts.NodeArray<ts.Expression>;
}

/**
 * If `expr` is a call to a known encrypt or decrypt function, return its info.
 * Returns null for any other expression.
 *
 * Matches both bare calls `createDecipheriv(...)` and member calls
 * `crypto.createDecipheriv(...)`.
 */
function findCryptoCallInExpression(expr: ts.Expression): CryptoCallInfo | null {
  if (!ts.isCallExpression(expr)) return null;

  let functionName: string | null = null;

  if (ts.isPropertyAccessExpression(expr.expression)) {
    functionName = expr.expression.name.text;
  } else if (ts.isIdentifier(expr.expression)) {
    functionName = expr.expression.text;
  }

  if (functionName === null) return null;

  if (KNOWN_DECRYPT_FUNCTION_NAMES.has(functionName)) {
    return { functionName, kind: "decrypt", args: expr.arguments };
  }
  if (KNOWN_ENCRYPT_FUNCTION_NAMES.has(functionName)) {
    return { functionName, kind: "encrypt", args: expr.arguments };
  }

  return null;
}

/**
 * Scan a statement shallowly for crypto function calls.
 *
 * Only checks the top-level expression of expression statements, the
 * initializer of the first declaration in variable statements, and the
 * return expression of return statements. Nested/chained calls are not
 * traversed — the goal is to catch the common "const x = decrypt(...)"
 * and "return decrypt(...)" patterns without expensive deep traversal.
 */
function findCryptoCallsInStatement(stmt: ts.Statement): CryptoCallInfo[] {
  const results: CryptoCallInfo[] = [];

  if (ts.isExpressionStatement(stmt)) {
    const info = findCryptoCallInExpression(stmt.expression);
    if (info !== null) results.push(info);
  }

  if (ts.isVariableStatement(stmt)) {
    for (const decl of stmt.declarationList.declarations) {
      if (decl.initializer !== undefined) {
        const info = findCryptoCallInExpression(decl.initializer);
        if (info !== null) results.push(info);
      }
    }
  }

  if (ts.isReturnStatement(stmt) && stmt.expression !== undefined) {
    const info = findCryptoCallInExpression(stmt.expression);
    if (info !== null) results.push(info);
  }

  return results;
}

/**
 * Build a `__shatter_crypto_boundary(boundaryId, kind, functionName, ...args)` call.
 *
 * The call is injected immediately before the crypto function call so the
 * executor can capture argument values before they are consumed.
 */
function createCryptoBoundaryCall(
  info: CryptoCallInfo,
  boundaryId: string,
  factory: ts.NodeFactory,
): ts.ExpressionStatement {
  return factory.createExpressionStatement(
    factory.createCallExpression(
      factory.createIdentifier(CRYPTO_BOUNDARY_FUNCTION),
      undefined,
      [
        factory.createStringLiteral(boundaryId),
        factory.createStringLiteral(info.kind),
        factory.createStringLiteral(info.functionName),
        ...info.args,
      ],
    ),
  );
}

/**
 * Instrument a block by prepending a __shatter_record() call before each
 * statement in the block, and recursively instrumenting branch bodies.
 *
 * Also injects `__shatter_crypto_boundary()` calls before any statement that
 * contains a call to a known encrypt/decrypt function, so the executor can
 * capture key, IV, and algorithm values at runtime.
 */
function instrumentBlock(
  block: ts.Block,
  ctx: InstrumentationContext,
): ts.Block {
  const newStatements: ts.Statement[] = [];

  for (const stmt of block.statements) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.getStart(ctx.sourceFile)).line + 1;
    ctx.instrumentableLines.add(line);
    newStatements.push(createRecordCall(ctx.factory, line));

    // Inject a crypto boundary recording call before any crypto API call.
    const cryptoCalls = findCryptoCallsInStatement(stmt);
    for (const info of cryptoCalls) {
      const boundaryId = `cb-${ctx.nextCryptoBoundaryId++}`;
      newStatements.push(createCryptoBoundaryCall(info, boundaryId, ctx.factory));
    }

    newStatements.push(instrumentStatement(stmt, ctx));
  }

  return ctx.factory.updateBlock(block, newStatements);
}

/**
 * Recursively instrument a statement, handling branch constructs (if/else,
 * switch, for, while, do-while) by instrumenting their sub-blocks and
 * wrapping conditions with __shatter_branch() calls.
 */
function instrumentStatement(
  stmt: ts.Statement,
  ctx: InstrumentationContext,
): ts.Statement {
  if (ts.isIfStatement(stmt)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.getStart(ctx.sourceFile)).line + 1;
    const wrappedCondition = wrapBranchCondition(stmt.expression, line, ctx);

    const thenBranch = ensureBlock(stmt.thenStatement, ctx.factory);
    const instrumentedThen = instrumentBlock(thenBranch, ctx);

    let instrumentedElse: ts.Statement | undefined;
    if (stmt.elseStatement) {
      if (ts.isIfStatement(stmt.elseStatement)) {
        const elseIfLine = ctx.sourceFile.getLineAndCharacterOfPosition(
          stmt.elseStatement.getStart(ctx.sourceFile),
        ).line + 1;
        ctx.instrumentableLines.add(elseIfLine);
        const nestedIf = instrumentStatement(stmt.elseStatement, ctx);
        instrumentedElse = ctx.factory.createBlock(
          [createRecordCall(ctx.factory, elseIfLine), nestedIf as ts.Statement],
          true,
        );
      } else {
        const elseBlock = ensureBlock(stmt.elseStatement, ctx.factory);
        instrumentedElse = instrumentBlock(elseBlock, ctx);
      }
    }

    return ctx.factory.updateIfStatement(stmt, wrappedCondition, instrumentedThen, instrumentedElse);
  }

  if (ts.isSwitchStatement(stmt)) {
    const newClauses = stmt.caseBlock.clauses.map((clause) => {
      const newStmts: ts.Statement[] = [];
      for (const clauseStmt of clause.statements) {
        const line = ctx.sourceFile.getLineAndCharacterOfPosition(clauseStmt.getStart(ctx.sourceFile)).line + 1;
        ctx.instrumentableLines.add(line);
        newStmts.push(createRecordCall(ctx.factory, line));
        newStmts.push(instrumentStatement(clauseStmt, ctx));
      }

      if (ts.isCaseClause(clause)) {
        return ctx.factory.updateCaseClause(clause, clause.expression, newStmts);
      }
      return ctx.factory.updateDefaultClause(clause, newStmts);
    });

    const newCaseBlock = ctx.factory.updateCaseBlock(stmt.caseBlock, newClauses);
    return ctx.factory.updateSwitchStatement(stmt, stmt.expression, newCaseBlock);
  }

  if (ts.isForStatement(stmt)) {
    let condition = stmt.condition;
    if (condition) {
      const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.getStart(ctx.sourceFile)).line + 1;
      condition = wrapBranchCondition(condition, line, ctx);
    }
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = wrapLoopBody(instrumentBlock(body, ctx), ctx);
    return ctx.factory.updateForStatement(
      stmt,
      stmt.initializer,
      condition,
      stmt.incrementor,
      instrumentedBody,
    );
  }

  if (ts.isForInStatement(stmt)) {
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = wrapLoopBody(instrumentBlock(body, ctx), ctx);
    return ctx.factory.updateForInStatement(stmt, stmt.initializer, stmt.expression, instrumentedBody);
  }

  if (ts.isForOfStatement(stmt)) {
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = wrapLoopBody(instrumentBlock(body, ctx), ctx);
    return ctx.factory.updateForOfStatement(
      stmt,
      stmt.awaitModifier,
      stmt.initializer,
      stmt.expression,
      instrumentedBody,
    );
  }

  if (ts.isWhileStatement(stmt)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.getStart(ctx.sourceFile)).line + 1;
    const wrappedCondition = wrapBranchCondition(stmt.expression, line, ctx);
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = wrapLoopBody(instrumentBlock(body, ctx), ctx);
    return ctx.factory.updateWhileStatement(stmt, wrappedCondition, instrumentedBody);
  }

  if (ts.isDoStatement(stmt)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.expression.getStart(ctx.sourceFile)).line + 1;
    const wrappedCondition = wrapBranchCondition(stmt.expression, line, ctx);
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = wrapLoopBody(instrumentBlock(body, ctx), ctx);
    return ctx.factory.updateDoStatement(stmt, instrumentedBody, wrappedCondition);
  }

  if (ts.isTryStatement(stmt)) {
    const tryBlock = instrumentBlock(stmt.tryBlock, ctx);

    let catchClause: ts.CatchClause | undefined;
    if (stmt.catchClause) {
      const catchBlock = instrumentBlock(stmt.catchClause.block, ctx);
      catchClause = ctx.factory.updateCatchClause(
        stmt.catchClause,
        stmt.catchClause.variableDeclaration,
        catchBlock,
      );
    }

    let finallyBlock: ts.Block | undefined;
    if (stmt.finallyBlock) {
      finallyBlock = instrumentBlock(stmt.finallyBlock, ctx);
    }

    return ctx.factory.updateTryStatement(stmt, tryBlock, catchClause, finallyBlock);
  }

  // For expression statements, return statements, and variable declarations,
  // instrument inline callbacks in call arguments.
  if (ts.isExpressionStatement(stmt)) {
    const newExpr = instrumentExpressionCallbacks(stmt.expression, ctx);
    if (newExpr !== stmt.expression) {
      return ctx.factory.updateExpressionStatement(stmt, newExpr);
    }
  }

  if (ts.isReturnStatement(stmt) && stmt.expression) {
    const newExpr = instrumentExpressionCallbacks(stmt.expression, ctx);
    if (newExpr !== stmt.expression) {
      return ctx.factory.updateReturnStatement(stmt, newExpr);
    }
  }

  if (ts.isVariableStatement(stmt)) {
    let changed = false;
    const newDecls = stmt.declarationList.declarations.map((decl) => {
      if (decl.initializer) {
        const newInit = instrumentExpressionCallbacks(decl.initializer, ctx);
        if (newInit !== decl.initializer) {
          changed = true;
          return ctx.factory.updateVariableDeclaration(decl, decl.name, decl.exclamationToken, decl.type, newInit);
        }
      }
      return decl;
    });
    if (changed) {
      const newList = ctx.factory.updateVariableDeclarationList(stmt.declarationList, newDecls);
      return ctx.factory.updateVariableStatement(stmt, stmt.modifiers, newList);
    }
  }

  return stmt;
}

/**
 * Recursively visit expressions, wrapping inline callback arguments
 * (arrow functions and function expressions) with call_enter/call_exit scope events.
 */
function instrumentExpressionCallbacks(
  expr: ts.Expression,
  ctx: InstrumentationContext,
): ts.Expression {
  if (ts.isCallExpression(expr)) {
    let changed = false;
    const newArgs = expr.arguments.map((arg) => {
      if (ts.isArrowFunction(arg) && ts.isBlock(arg.body)) {
        changed = true;
        return wrapCallbackWithScope(arg, ctx);
      }
      if (ts.isFunctionExpression(arg) && arg.body) {
        changed = true;
        return wrapCallbackFnExprWithScope(arg, ctx);
      }
      const newArg = instrumentExpressionCallbacks(arg, ctx);
      if (newArg !== arg) changed = true;
      return newArg;
    });
    const newCallExpr = instrumentExpressionCallbacks(expr.expression, ctx);
    if (changed || newCallExpr !== expr.expression) {
      return ctx.factory.updateCallExpression(expr, newCallExpr, expr.typeArguments, newArgs);
    }
  }

  if (ts.isPropertyAccessExpression(expr)) {
    const newExpr = instrumentExpressionCallbacks(expr.expression, ctx);
    if (newExpr !== expr.expression) {
      return ctx.factory.updatePropertyAccessExpression(expr, newExpr, expr.name);
    }
  }

  return expr;
}

/** Wrap an inline arrow function callback with call_enter/call_exit and instrument its body. */
function wrapCallbackWithScope(
  arrow: ts.ArrowFunction,
  ctx: InstrumentationContext,
): ts.ArrowFunction {
  const callSiteId = ctx.nextCallSiteId++;
  const body = arrow.body as ts.Block;
  const instrumentedBody = instrumentBlock(body, ctx);
  const wrappedBody = wrapFunctionBodyWithCallScope(instrumentedBody, callSiteId, ctx.factory);
  return ctx.factory.updateArrowFunction(
    arrow,
    arrow.modifiers,
    arrow.typeParameters,
    arrow.parameters,
    arrow.type,
    arrow.equalsGreaterThanToken,
    wrappedBody,
  );
}

/** Wrap an inline function expression callback with call_enter/call_exit and instrument its body. */
function wrapCallbackFnExprWithScope(
  fn: ts.FunctionExpression,
  ctx: InstrumentationContext,
): ts.FunctionExpression {
  const callSiteId = ctx.nextCallSiteId++;
  const instrumentedBody = instrumentBlock(fn.body, ctx);
  const wrappedBody = wrapFunctionBodyWithCallScope(instrumentedBody, callSiteId, ctx.factory);
  return ctx.factory.updateFunctionExpression(
    fn,
    fn.modifiers,
    fn.asteriskToken,
    fn.name,
    fn.typeParameters,
    fn.parameters,
    fn.type,
    wrappedBody,
  );
}

/**
 * Return true when MC/DC mode is enabled (SHATTER_MCDC=1).
 * Placed here to avoid a circular dependency with executor.ts.
 */
function isMcdcEnabled(): boolean {
  return process.env["SHATTER_MCDC"] === "1";
}

/**
 * Result of flattening a pure && or || boolean expression tree.
 */
export interface FlattenedConditions {
  /** Leaf conditions extracted left-to-right from the expression. */
  conditions: Array<{ expr: ts.Expression; symExpr: SymExpr }>;
  /** The logical operator connecting all conditions (must be uniform). */
  operator: "and" | "or";
}

/**
 * Flatten a pure && or || expression tree into a list of leaf conditions.
 *
 * Returns non-null only for compound decisions with a uniform top-level
 * operator (pure && chains or pure || chains). Returns null for:
 * - Non-binary expressions (identifiers, comparisons, calls, etc.)
 * - Mixed operators like `(A && B) || C`
 * - Single-condition decisions (no compound operator at the top level)
 * - Chains with more than 16 conditions
 *
 * Recursion into nested same-operator chains: `A && B && C` → [A, B, C].
 */
export function flattenConditions(
  expr: ts.Expression,
  paramNames: Set<string>,
  dataFlowMap: Map<string, SymExpr>,
): FlattenedConditions | null {
  // Unwrap parentheses
  if (ts.isParenthesizedExpression(expr)) {
    return flattenConditions(expr.expression, paramNames, dataFlowMap);
  }

  if (!ts.isBinaryExpression(expr)) {
    return null;
  }

  const isAnd = expr.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken;
  const isOr = expr.operatorToken.kind === ts.SyntaxKind.BarBarToken;

  if (!isAnd && !isOr) {
    return null;
  }

  const operator: "and" | "or" = isAnd ? "and" : "or";

  // Recursively collect leaves, ensuring uniform operator throughout
  const leaves: Array<{ expr: ts.Expression; symExpr: SymExpr }> = [];

  function collect(node: ts.Expression): boolean {
    // Unwrap parentheses without changing semantics
    if (ts.isParenthesizedExpression(node)) {
      return collect(node.expression);
    }

    if (ts.isBinaryExpression(node)) {
      const nodeIsAnd = node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken;
      const nodeIsOr = node.operatorToken.kind === ts.SyntaxKind.BarBarToken;

      if (nodeIsAnd && operator === "and") {
        // Same operator — recurse into both sides
        return collect(node.left) && collect(node.right);
      }
      if (nodeIsOr && operator === "or") {
        return collect(node.left) && collect(node.right);
      }
      // Different or mixed operator — treat whole sub-expression as a leaf
    }

    // Leaf condition
    const symExpr = buildSymExpr(node, paramNames, dataFlowMap);
    leaves.push({ expr: node, symExpr });
    return true;
  }

  const ok = collect(expr);
  if (!ok) return null;

  // Must be genuinely compound (at least 2 conditions) and within the cap
  if (leaves.length < 2) return null;
  if (leaves.length > 16) return null;

  return { conditions: leaves, operator };
}

/**
 * Wrap a branch condition expression with a __shatter_branch() call.
 *
 * When MC/DC mode is enabled and the condition is a compound && or || chain,
 * generates:
 *   const __mcdcN = __shatter_mcdc_record(branchId, [symExprs], "and"|"or", [() => !!(cond), ...]);
 *   __shatter_branch_mcdc(branchId, line, __mcdcN.decision, symExprFull, __mcdcN.conditions)
 *
 * Otherwise (non-MC/DC or simple condition):
 *   __shatter_branch(branchId, line, !!(condition), symExprLiteral)
 */
function wrapBranchCondition(
  condition: ts.Expression,
  line: number,
  ctx: InstrumentationContext,
): ts.Expression {
  const branchId = ctx.nextBranchId++;
  const symExpr = buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap);
  const symExprLiteral = valueToAstLiteral(symExpr, ctx.factory);

  // MC/DC path: compound && or || chain
  if (isMcdcEnabled()) {
    const flattened = flattenConditions(condition, ctx.paramNames, ctx.dataFlowMap);
    if (flattened !== null) {
      return buildMcdcBranchCall(branchId, line, condition, flattened, symExprLiteral, ctx);
    }
  }

  // Default path: simple branch recording
  return ctx.factory.createCallExpression(
    ctx.factory.createIdentifier(BRANCH_FUNCTION),
    undefined,
    [
      ctx.factory.createNumericLiteral(branchId),
      ctx.factory.createNumericLiteral(line),
      ctx.factory.createPrefixUnaryExpression(
        ts.SyntaxKind.ExclamationToken,
        ctx.factory.createPrefixUnaryExpression(
          ts.SyntaxKind.ExclamationToken,
          condition,
        ),
      ),
      symExprLiteral,
    ],
  );
}

/**
 * Build the MC/DC instrumented branch call for a compound condition.
 *
 * Generates:
 *   (() => {
 *     const __mcdcN = __shatter_mcdc_record(branchId, [symExprs], "and"|"or",
 *       [() => !!(cond0), () => !!(cond1), ...]);
 *     return __shatter_branch_mcdc(branchId, line, __mcdcN.decision, symExprFull, __mcdcN.conditions);
 *   })()
 *
 * The IIFE is used so we can declare the __mcdcN variable and still produce an expression.
 */
function buildMcdcBranchCall(
  branchId: number,
  line: number,
  fullCondition: ts.Expression,
  flattened: FlattenedConditions,
  symExprFullLiteral: ts.Expression,
  ctx: InstrumentationContext,
): ts.Expression {
  const factory = ctx.factory;

  // Build array of per-condition SymExpr literals: [symExpr0, symExpr1, ...]
  const symExprArrayElems = flattened.conditions.map(({ symExpr }) =>
    valueToAstLiteral(symExpr, factory),
  );
  const symExprArray = factory.createArrayLiteralExpression(symExprArrayElems);

  // Build operator string literal: "and" or "or"
  const operatorLiteral = factory.createStringLiteral(flattened.operator);

  // Build thunk array: [() => !!(cond0), () => !!(cond1), ...]
  const thunkElems = flattened.conditions.map(({ expr: condExpr }) => {
    const doubleNot = factory.createPrefixUnaryExpression(
      ts.SyntaxKind.ExclamationToken,
      factory.createPrefixUnaryExpression(ts.SyntaxKind.ExclamationToken, condExpr),
    );
    return factory.createArrowFunction(
      undefined,
      undefined,
      [],
      undefined,
      factory.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
      doubleNot,
    );
  });
  const thunkArray = factory.createArrayLiteralExpression(thunkElems);

  // Name for the MC/DC result variable (unique per branch)
  const mcdcVarName = `__mcdc${branchId}`;

  // const __mcdcN = __shatter_mcdc_record(branchId, symExprArray, operator, thunkArray);
  const mcdcVarDecl = factory.createVariableStatement(
    undefined,
    factory.createVariableDeclarationList(
      [factory.createVariableDeclaration(
        factory.createIdentifier(mcdcVarName),
        undefined,
        undefined,
        factory.createCallExpression(
          factory.createIdentifier(MCDC_RECORD_FUNCTION),
          undefined,
          [
            factory.createNumericLiteral(branchId),
            symExprArray,
            operatorLiteral,
            thunkArray,
          ],
        ),
      )],
      ts.NodeFlags.Const,
    ),
  );

  // __shatter_branch_mcdc(branchId, line, __mcdcN.decision, symExprFull, __mcdcN.conditions)
  const mcdcBranchCall = factory.createCallExpression(
    factory.createIdentifier(MCDC_BRANCH_FUNCTION),
    undefined,
    [
      factory.createNumericLiteral(branchId),
      factory.createNumericLiteral(line),
      factory.createPropertyAccessExpression(factory.createIdentifier(mcdcVarName), "decision"),
      symExprFullLiteral,
      factory.createPropertyAccessExpression(factory.createIdentifier(mcdcVarName), "conditions"),
    ],
  );

  // Wrap in IIFE so we can declare a variable and return an expression:
  // (() => { const __mcdcN = ...; return __shatter_branch_mcdc(...); })()
  const iife = factory.createCallExpression(
    factory.createParenthesizedExpression(
      factory.createArrowFunction(
        undefined,
        undefined,
        [],
        undefined,
        factory.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
        factory.createBlock(
          [
            mcdcVarDecl,
            factory.createReturnStatement(mcdcBranchCall),
          ],
          /* multiLine */ true,
        ),
      ),
    ),
    undefined,
    [],
  );

  return iife;
}

// ---------------------------------------------------------------------------
// Symbolic expression builder
// ---------------------------------------------------------------------------

/**
 * Convert a TypeScript AST expression into a SymExpr object matching the
 * Rust serialization format. Expressions involving function parameters
 * produce symbolic nodes; everything else falls back to Unknown.
 */
export function buildSymExpr(
  expr: ts.Expression,
  paramNames: Set<string>,
  dataFlowMap: Map<string, SymExpr> = new Map(),
): SymExpr {
  if (ts.isParenthesizedExpression(expr)) {
    return buildSymExpr(expr.expression, paramNames, dataFlowMap);
  }

  if (ts.isIdentifier(expr)) {
    if (paramNames.has(expr.text)) {
      return { kind: "param", name: expr.text, path: [] };
    }
    const flowExpr = dataFlowMap.get(expr.text);
    if (flowExpr) {
      return flowExpr;
    }
    return { kind: "unknown" };
  }

  if (ts.isPropertyAccessExpression(expr)) {
    const chain = resolvePropertyChain(expr, paramNames);
    if (chain) {
      return { kind: "param", name: chain.name, path: chain.path };
    }
    return { kind: "unknown" };
  }

  if (ts.isNumericLiteral(expr)) {
    const n = Number(expr.text);
    if (Number.isInteger(n)) {
      return { kind: "const", type: "int", value: n };
    }
    return { kind: "const", type: "float", value: n };
  }

  if (ts.isStringLiteral(expr)) {
    return { kind: "const", type: "str", value: expr.text };
  }

  if (expr.kind === ts.SyntaxKind.TrueKeyword) {
    return { kind: "const", type: "bool", value: true };
  }

  if (expr.kind === ts.SyntaxKind.FalseKeyword) {
    return { kind: "const", type: "bool", value: false };
  }

  if (expr.kind === ts.SyntaxKind.NullKeyword) {
    return { kind: "const", type: "null" };
  }

  if (ts.isBinaryExpression(expr)) {
    const op = binaryTokenToOp(expr.operatorToken.kind);
    if (op) {
      const left = buildSymExpr(expr.left, paramNames, dataFlowMap);
      const right = buildSymExpr(expr.right, paramNames, dataFlowMap);
      return { kind: "bin_op", op, left, right };
    }
    return { kind: "unknown" };
  }

  if (ts.isPrefixUnaryExpression(expr)) {
    const op = unaryTokenToOp(expr.operator);
    if (op) {
      const operand = buildSymExpr(expr.operand, paramNames, dataFlowMap);
      return { kind: "un_op", op, operand };
    }
    return { kind: "unknown" };
  }

  if (ts.isTypeOfExpression(expr)) {
    const operand = buildSymExpr(expr.expression, paramNames, dataFlowMap);
    return { kind: "un_op", op: "typeof" as UnOpKind, operand };
  }

  if (ts.isCallExpression(expr)) {
    if (ts.isPropertyAccessExpression(expr.expression)) {
      const methodName = expr.expression.name.text;
      const receiver = buildSymExpr(expr.expression.expression, paramNames, dataFlowMap);
      const args = expr.arguments.map((a) => buildSymExpr(a, paramNames, dataFlowMap));
      return { kind: "call", name: methodName, receiver, args };
    }
    if (ts.isIdentifier(expr.expression)) {
      const args = expr.arguments.map((a) => buildSymExpr(a, paramNames, dataFlowMap));
      return { kind: "call", name: expr.expression.text, receiver: null, args };
    }
    return { kind: "unknown" };
  }

  return { kind: "unknown" };
}

/**
 * Resolve a chain of property accesses to a base parameter name and path.
 * Returns null if the base is not a known parameter.
 *
 * Example: `config.timeout.max` where `config` is a param
 *   → { name: "config", path: ["timeout", "max"] }
 */
function resolvePropertyChain(
  expr: ts.PropertyAccessExpression,
  paramNames: Set<string>,
): { name: string; path: string[] } | null {
  const path: string[] = [];
  let current: ts.Expression = expr;

  while (ts.isPropertyAccessExpression(current)) {
    path.unshift(current.name.text);
    current = current.expression;
  }

  if (ts.isIdentifier(current) && paramNames.has(current.text)) {
    return { name: current.text, path };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Operator mapping
// ---------------------------------------------------------------------------

function binaryTokenToOp(kind: ts.SyntaxKind): BinOpKind | null {
  switch (kind) {
    case ts.SyntaxKind.EqualsEqualsToken:
    case ts.SyntaxKind.EqualsEqualsEqualsToken:
      return "eq";
    case ts.SyntaxKind.ExclamationEqualsToken:
    case ts.SyntaxKind.ExclamationEqualsEqualsToken:
      return "ne";
    case ts.SyntaxKind.LessThanToken:
      return "lt";
    case ts.SyntaxKind.LessThanEqualsToken:
      return "le";
    case ts.SyntaxKind.GreaterThanToken:
      return "gt";
    case ts.SyntaxKind.GreaterThanEqualsToken:
      return "ge";
    case ts.SyntaxKind.PlusToken:
      return "add";
    case ts.SyntaxKind.MinusToken:
      return "sub";
    case ts.SyntaxKind.AsteriskToken:
      return "mul";
    case ts.SyntaxKind.SlashToken:
      return "div";
    case ts.SyntaxKind.PercentToken:
      return "mod";
    case ts.SyntaxKind.AmpersandAmpersandToken:
      return "and";
    case ts.SyntaxKind.BarBarToken:
      return "or";
    case ts.SyntaxKind.AmpersandToken:
      return "bitwise_and";
    case ts.SyntaxKind.BarToken:
      return "bitwise_or";
    case ts.SyntaxKind.CaretToken:
      return "bitwise_xor";
    case ts.SyntaxKind.InKeyword:
      return "in";
    case ts.SyntaxKind.InstanceOfKeyword:
      return "instance_of";
    default:
      return null;
  }
}

function unaryTokenToOp(kind: ts.PrefixUnaryOperator): UnOpKind | null {
  switch (kind) {
    case ts.SyntaxKind.ExclamationToken:
      return "not";
    case ts.SyntaxKind.MinusToken:
      return "neg";
    case ts.SyntaxKind.TildeToken:
      return "bitwise_not";
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// AST literal builder
// ---------------------------------------------------------------------------

/**
 * Convert a plain JavaScript value into a TypeScript AST expression node.
 * Used to embed SymExpr objects as literal expressions in instrumented code.
 */
function valueToAstLiteral(value: unknown, factory: ts.NodeFactory): ts.Expression {
  if (value === null) {
    return factory.createNull();
  }
  if (value === undefined) {
    return factory.createIdentifier("undefined");
  }
  if (typeof value === "string") {
    return factory.createStringLiteral(value);
  }
  if (typeof value === "number") {
    return factory.createNumericLiteral(value);
  }
  if (typeof value === "boolean") {
    return value ? factory.createTrue() : factory.createFalse();
  }
  if (Array.isArray(value)) {
    return factory.createArrayLiteralExpression(
      value.map((v: unknown) => valueToAstLiteral(v, factory)),
    );
  }
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    return factory.createObjectLiteralExpression(
      entries.map(([k, v]) =>
        factory.createPropertyAssignment(k, valueToAstLiteral(v, factory)),
      ),
      false,
    );
  }
  return factory.createIdentifier("undefined");
}

// ---------------------------------------------------------------------------
// Import rewriting for mocks
// ---------------------------------------------------------------------------

/**
 * Rewrite all imports in a source file for mocked symbols.
 * Expands each import declaration that contains mocked symbols into
 * a (possibly smaller) import + const declarations for mock lookups.
 */
function rewriteImportsInSourceFile(
  sourceFile: ts.SourceFile,
  mockLookup: MockLookup,
  factory: ts.NodeFactory,
): ts.SourceFile {
  const newStatements: ts.Statement[] = [];
  let changed = false;

  for (const stmt of sourceFile.statements) {
    if (ts.isImportDeclaration(stmt)) {
      const result = rewriteImportForMocks(stmt, mockLookup, factory);
      if (Array.isArray(result)) {
        newStatements.push(...result as ts.Statement[]);
        changed = true;
      } else {
        newStatements.push(result as ts.Statement);
      }
    } else {
      newStatements.push(stmt);
    }
  }

  if (!changed) {
    return sourceFile;
  }

  return factory.updateSourceFile(sourceFile, newStatements);
}

/**
 * Rewrite an import declaration to use the mock registry for mocked symbols.
 *
 * For `import { foo, bar } from 'module'` where `module:foo` is mocked:
 * - `foo` becomes: `const foo = __shatter_mocks['module:foo'] || original_foo`
 * - `bar` remains unchanged (kept in the original import)
 *
 * Returns the original import if no symbols are mocked, or a mix of the
 * original import (for non-mocked symbols) plus variable statements for
 * mocked symbols.
 */
function rewriteImportForMocks(
  node: ts.ImportDeclaration,
  mockLookup: MockLookup,
  factory: ts.NodeFactory,
): ts.Node | ts.Node[] {
  const moduleSpecifier = node.moduleSpecifier;
  if (!ts.isStringLiteral(moduleSpecifier)) {
    return node;
  }
  const moduleName = moduleSpecifier.text;

  const namedBindings = node.importClause?.namedBindings;
  if (!namedBindings || !ts.isNamedImports(namedBindings)) {
    return node;
  }

  const mockedElements: ts.ImportSpecifier[] = [];
  const unmockedElements: ts.ImportSpecifier[] = [];

  for (const element of namedBindings.elements) {
    const symbolKey = `${moduleName}:${element.name.text}`;
    if (mockLookup.has(symbolKey)) {
      mockedElements.push(element);
    } else {
      unmockedElements.push(element);
    }
  }

  if (mockedElements.length === 0) {
    return node;
  }

  const result: ts.Node[] = [];

  // Keep the original import for unmocked symbols
  if (unmockedElements.length > 0) {
    const newBindings = factory.createNamedImports(unmockedElements);
    const newClause = factory.createImportClause(false, undefined, newBindings);
    result.push(
      factory.createImportDeclaration(node.modifiers, newClause, node.moduleSpecifier),
    );
  }

  // For each mocked symbol, generate a variable declaration that looks up the mock
  for (const element of mockedElements) {
    const symbolName = element.name.text;
    const mockKey = `${moduleName}:${symbolName}`;

    // const <symbol> = (() => { const _orig = <symbol>; const _mock = __shatter_mocks['module:symbol'];
    //   return _mock ? (...args) => { const _r = _mock(...args); __shatter_mock_call('module', 'symbol', args, _r); return _r; } : _orig; })()
    // Simplified: const <symbol> = __shatter_mocks['module:symbol']
    // with call recording wrapper
    const mockLookupExpr = factory.createElementAccessExpression(
      factory.createIdentifier(MOCK_REGISTRY),
      factory.createStringLiteral(mockKey),
    );

    // Create a wrapper that records mock calls:
    // __shatter_mocks['module:symbol']
    //   ? (...args) => { const r = __shatter_mocks['module:symbol'](...args); __shatter_mock_call('module', 'symbol', args, r); return r; }
    //   : undefined
    const argsParam = factory.createParameterDeclaration(
      undefined, factory.createToken(ts.SyntaxKind.DotDotDotToken),
      factory.createIdentifier("args"),
    );

    const callMock = factory.createCallExpression(
      factory.createElementAccessExpression(
        factory.createIdentifier(MOCK_REGISTRY),
        factory.createStringLiteral(mockKey),
      ),
      undefined,
      [factory.createSpreadElement(factory.createIdentifier("args"))],
    );

    const rDecl = factory.createVariableStatement(
      undefined,
      factory.createVariableDeclarationList(
        [factory.createVariableDeclaration("_r", undefined, undefined, callMock)],
        ts.NodeFlags.Const,
      ),
    );

    const recordCall = factory.createExpressionStatement(
      factory.createCallExpression(
        factory.createIdentifier(MOCK_CALL_FUNCTION),
        undefined,
        [
          factory.createStringLiteral(moduleName),
          factory.createStringLiteral(symbolName),
          factory.createIdentifier("args"),
          factory.createIdentifier("_r"),
        ],
      ),
    );

    const returnR = factory.createReturnStatement(factory.createIdentifier("_r"));

    const wrapperBody = factory.createBlock([rDecl, recordCall, returnR], true);
    const wrapperArrow = factory.createArrowFunction(
      undefined, undefined, [argsParam], undefined,
      factory.createToken(ts.SyntaxKind.EqualsGreaterThanToken),
      wrapperBody,
    );

    // Ternary: __shatter_mocks['key'] ? wrapper : undefined
    const conditional = factory.createConditionalExpression(
      mockLookupExpr,
      factory.createToken(ts.SyntaxKind.QuestionToken),
      wrapperArrow,
      factory.createToken(ts.SyntaxKind.ColonToken),
      factory.createIdentifier("undefined"),
    );

    const varDecl = factory.createVariableStatement(
      undefined,
      factory.createVariableDeclarationList(
        [factory.createVariableDeclaration(symbolName, undefined, undefined, conditional)],
        ts.NodeFlags.Const,
      ),
    );

    result.push(varDecl);
  }

  return result;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Create a `__shatter_scope_event(scopeId, kind);` expression statement.
 */
function createScopeEventCall(
  factory: ts.NodeFactory,
  scopeId: number,
  kind: string,
): ts.ExpressionStatement {
  return factory.createExpressionStatement(
    factory.createCallExpression(
      factory.createIdentifier(SCOPE_EVENT_FUNCTION),
      undefined,
      [
        factory.createNumericLiteral(scopeId),
        factory.createStringLiteral(kind),
      ],
    ),
  );
}

/**
 * Wrap an instrumented loop body with loop_enter/loop_exit scope events.
 * Events are placed inside the body so each iteration emits its own pair.
 */
function wrapLoopBody(
  instrumentedBody: ts.Block,
  ctx: InstrumentationContext,
): ts.Block {
  const loopId = ctx.nextLoopId++;
  const enter = createScopeEventCall(ctx.factory, loopId, "loop_enter");
  const exit = createScopeEventCall(ctx.factory, loopId, "loop_exit");
  return ctx.factory.updateBlock(instrumentedBody, [
    enter,
    ...instrumentedBody.statements,
    exit,
  ]);
}

/**
 * Wrap a function body in try/finally with call_enter/call_exit scope events.
 * call_enter fires at entry, call_exit fires in finally (even on throw/return).
 */
function wrapFunctionBodyWithCallScope(
  body: ts.Block,
  callSiteId: number,
  factory: ts.NodeFactory,
): ts.Block {
  const enter = createScopeEventCall(factory, callSiteId, "call_enter");
  const tryFinally = factory.createTryStatement(
    factory.createBlock([...body.statements], true),
    undefined,
    factory.createBlock([createScopeEventCall(factory, callSiteId, "call_exit")], true),
  );
  return factory.createBlock([enter, tryFinally], true);
}

/**
 * Wrap a single statement in a block if it isn't already one.
 */
function ensureBlock(stmt: ts.Statement, factory: ts.NodeFactory): ts.Block {
  if (ts.isBlock(stmt)) {
    return stmt;
  }
  return factory.createBlock([stmt], true);
}

/**
 * Create a `__shatter_record(lineNumber);` expression statement.
 */
function createRecordCall(factory: ts.NodeFactory, line: number): ts.ExpressionStatement {
  return factory.createExpressionStatement(
    factory.createCallExpression(
      factory.createIdentifier(RECORD_FUNCTION),
      undefined,
      [factory.createNumericLiteral(line)],
    ),
  );
}
