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
import type { SymExpr, BinOpKind, UnOpKind } from "./protocol.js";

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
}

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

/** Mutable state threaded through the instrumentation pass. */
interface InstrumentationContext {
  sourceFile: ts.SourceFile;
  factory: ts.NodeFactory;
  paramNames: Set<string>;
  nextBranchId: number;
}

/**
 * Instrument a TypeScript source file, inserting line-recording and
 * branch-recording calls into the specified function.
 *
 * @param source - The original TypeScript source text.
 * @param functionName - The name of the function to instrument.
 * @param fileName - The file name used for parsing (affects diagnostics only).
 * @returns The instrumented source, or an error message.
 */
export function instrumentFunction(
  source: string,
  functionName: string,
  fileName = "input.ts",
): InstrumentResult | { error: string } {
  const sourceFile = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );

  const targetFunction = findFunction(sourceFile, functionName);
  if (targetFunction === undefined) {
    return { error: `Function '${functionName}' not found` };
  }

  const paramNames = extractParamNames(targetFunction, sourceFile);

  // Shared mutable branch counter — captured by the transformer closure.
  const branchState = { nextBranchId: 0 };

  const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });
  const transformed = ts.transform(sourceFile, [
    createInstrumentationTransformer(functionName, paramNames, branchState),
  ]);
  const result = printer.printFile(transformed.transformed[0] as ts.SourceFile);
  transformed.dispose();

  return {
    instrumentedSource: result,
    recordFunctionName: RECORD_FUNCTION,
    branchFunctionName: BRANCH_FUNCTION,
    branchCount: branchState.nextBranchId,
  };
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

/**
 * Create a TypeScript transformer that instruments a specific function
 * with __shatter_record() and __shatter_branch() calls.
 */
function createInstrumentationTransformer(
  targetFunctionName: string,
  paramNames: Set<string>,
  branchState: { nextBranchId: number },
): ts.TransformerFactory<ts.SourceFile> {
  return (context) => {
    return (sourceFile) => {
      const ctx: InstrumentationContext = {
        sourceFile,
        factory: context.factory,
        paramNames,
        nextBranchId: 0,
      };

      const visitor = (node: ts.Node): ts.Node => {
        if (ts.isFunctionDeclaration(node) && node.name?.text === targetFunctionName && node.body) {
          ctx.nextBranchId = 0;
          const newBody = instrumentBlock(node.body, ctx);
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
                const newBody = instrumentBlock(decl.initializer.body, ctx);
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
                const newBody = instrumentBlock(decl.initializer.body, ctx);
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

      return ts.visitNode(sourceFile, visitor) as ts.SourceFile;
    };
  };
}

/**
 * Instrument a block by prepending a __shatter_record() call before each
 * statement in the block, and recursively instrumenting branch bodies.
 */
function instrumentBlock(
  block: ts.Block,
  ctx: InstrumentationContext,
): ts.Block {
  const newStatements: ts.Statement[] = [];

  for (const stmt of block.statements) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.getStart(ctx.sourceFile)).line + 1;
    newStatements.push(createRecordCall(ctx.factory, line));
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
    const instrumentedBody = instrumentBlock(body, ctx);
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
    const instrumentedBody = instrumentBlock(body, ctx);
    return ctx.factory.updateForInStatement(stmt, stmt.initializer, stmt.expression, instrumentedBody);
  }

  if (ts.isForOfStatement(stmt)) {
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = instrumentBlock(body, ctx);
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
    const instrumentedBody = instrumentBlock(body, ctx);
    return ctx.factory.updateWhileStatement(stmt, wrappedCondition, instrumentedBody);
  }

  if (ts.isDoStatement(stmt)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(stmt.expression.getStart(ctx.sourceFile)).line + 1;
    const wrappedCondition = wrapBranchCondition(stmt.expression, line, ctx);
    const body = ensureBlock(stmt.statement, ctx.factory);
    const instrumentedBody = instrumentBlock(body, ctx);
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

  return stmt;
}

/**
 * Wrap a branch condition expression with a __shatter_branch() call.
 *
 * Transforms `condition` into:
 *   __shatter_branch(branchId, line, condition, symExprLiteral)
 *
 * The __shatter_branch function evaluates the condition (passed as a boolean),
 * records the branch decision, and returns the boolean result.
 */
function wrapBranchCondition(
  condition: ts.Expression,
  line: number,
  ctx: InstrumentationContext,
): ts.Expression {
  const branchId = ctx.nextBranchId++;
  const symExpr = buildSymExpr(condition, ctx.paramNames);
  const symExprLiteral = valueToAstLiteral(symExpr, ctx.factory);

  return ctx.factory.createCallExpression(
    ctx.factory.createIdentifier(BRANCH_FUNCTION),
    undefined,
    [
      ctx.factory.createNumericLiteral(branchId),
      ctx.factory.createNumericLiteral(line),
      condition,
      symExprLiteral,
    ],
  );
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
): SymExpr {
  if (ts.isParenthesizedExpression(expr)) {
    return buildSymExpr(expr.expression, paramNames);
  }

  if (ts.isIdentifier(expr)) {
    if (paramNames.has(expr.text)) {
      return { kind: "param", name: expr.text, path: [] };
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
      const left = buildSymExpr(expr.left, paramNames);
      const right = buildSymExpr(expr.right, paramNames);
      return { kind: "bin_op", op, left, right };
    }
    return { kind: "unknown" };
  }

  if (ts.isPrefixUnaryExpression(expr)) {
    const op = unaryTokenToOp(expr.operator);
    if (op) {
      const operand = buildSymExpr(expr.operand, paramNames);
      return { kind: "un_op", op, operand };
    }
    return { kind: "unknown" };
  }

  if (ts.isTypeOfExpression(expr)) {
    const operand = buildSymExpr(expr.expression, paramNames);
    return { kind: "un_op", op: "typeof" as UnOpKind, operand };
  }

  if (ts.isCallExpression(expr)) {
    if (ts.isPropertyAccessExpression(expr.expression)) {
      const methodName = expr.expression.name.text;
      const receiver = buildSymExpr(expr.expression.expression, paramNames);
      const args = expr.arguments.map((a) => buildSymExpr(a, paramNames));
      return { kind: "call", name: methodName, receiver, args };
    }
    if (ts.isIdentifier(expr.expression)) {
      const args = expr.arguments.map((a) => buildSymExpr(a, paramNames));
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
// Helpers
// ---------------------------------------------------------------------------

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
