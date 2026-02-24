/**
 * Source code instrumentor using the TypeScript Compiler API.
 *
 * Rewrites a target function to insert __record(lineNumber) calls at each
 * statement, enabling line-level execution tracing. This is the v1
 * instrumentation — no symbolic constraints, just line coverage tracking.
 */

import ts from "typescript";

/** Result of instrumenting a source file. */
export interface InstrumentResult {
  /** The full instrumented source code. */
  instrumentedSource: string;
  /** The name of the recording function injected into the code. */
  recordFunctionName: string;
}

/**
 * The name of the recording function inserted into instrumented code.
 * Callers must define this function before executing instrumented code.
 */
export const RECORD_FUNCTION = "__shatter_record";

/**
 * Instrument a TypeScript source file, inserting line-recording calls into
 * the specified function.
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

  const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed });
  const transformed = ts.transform(sourceFile, [
    createInstrumentationTransformer(functionName),
  ]);
  const result = printer.printFile(transformed.transformed[0] as ts.SourceFile);
  transformed.dispose();

  return {
    instrumentedSource: result,
    recordFunctionName: RECORD_FUNCTION,
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
 * Create a TypeScript transformer that instruments a specific function
 * with __shatter_record() calls at each statement.
 */
function createInstrumentationTransformer(
  targetFunctionName: string,
): ts.TransformerFactory<ts.SourceFile> {
  return (context) => {
    return (sourceFile) => {
      const visitor = (node: ts.Node): ts.Node => {
        if (ts.isFunctionDeclaration(node) && node.name?.text === targetFunctionName && node.body) {
          const newBody = instrumentBlock(node.body, sourceFile, context.factory);
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
              if (ts.isArrowFunction(decl.initializer) && ts.isBlock(decl.initializer.body)) {
                const newBody = instrumentBlock(decl.initializer.body, sourceFile, context.factory);
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
                const newBody = instrumentBlock(decl.initializer.body, sourceFile, context.factory);
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
  sourceFile: ts.SourceFile,
  factory: ts.NodeFactory,
): ts.Block {
  const newStatements: ts.Statement[] = [];

  for (const stmt of block.statements) {
    const line = sourceFile.getLineAndCharacterOfPosition(stmt.getStart(sourceFile)).line + 1;
    newStatements.push(createRecordCall(factory, line));
    newStatements.push(instrumentStatement(stmt, sourceFile, factory));
  }

  return factory.updateBlock(block, newStatements);
}

/**
 * Recursively instrument a statement, handling branch constructs (if/else,
 * switch, for, while, do-while) by instrumenting their sub-blocks.
 */
function instrumentStatement(
  stmt: ts.Statement,
  sourceFile: ts.SourceFile,
  factory: ts.NodeFactory,
): ts.Statement {
  if (ts.isIfStatement(stmt)) {
    const thenBranch = ensureBlock(stmt.thenStatement, factory);
    const instrumentedThen = instrumentBlock(thenBranch, sourceFile, factory);

    let instrumentedElse: ts.Statement | undefined;
    if (stmt.elseStatement) {
      if (ts.isIfStatement(stmt.elseStatement)) {
        // else-if: wrap in a block with a __record call for the else-if line,
        // then recursively instrument the nested if-statement.
        const elseIfLine = sourceFile.getLineAndCharacterOfPosition(
          stmt.elseStatement.getStart(sourceFile),
        ).line + 1;
        const nestedIf = instrumentStatement(stmt.elseStatement, sourceFile, factory);
        instrumentedElse = factory.createBlock(
          [createRecordCall(factory, elseIfLine), nestedIf as ts.Statement],
          true,
        );
      } else {
        const elseBlock = ensureBlock(stmt.elseStatement, factory);
        instrumentedElse = instrumentBlock(elseBlock, sourceFile, factory);
      }
    }

    return factory.updateIfStatement(stmt, stmt.expression, instrumentedThen, instrumentedElse);
  }

  if (ts.isSwitchStatement(stmt)) {
    const newClauses = stmt.caseBlock.clauses.map((clause) => {
      const newStmts: ts.Statement[] = [];
      for (const clauseStmt of clause.statements) {
        const line = sourceFile.getLineAndCharacterOfPosition(clauseStmt.getStart(sourceFile)).line + 1;
        newStmts.push(createRecordCall(factory, line));
        newStmts.push(instrumentStatement(clauseStmt, sourceFile, factory));
      }

      if (ts.isCaseClause(clause)) {
        return factory.updateCaseClause(clause, clause.expression, newStmts);
      }
      return factory.updateDefaultClause(clause, newStmts);
    });

    const newCaseBlock = factory.updateCaseBlock(stmt.caseBlock, newClauses);
    return factory.updateSwitchStatement(stmt, stmt.expression, newCaseBlock);
  }

  if (ts.isForStatement(stmt)) {
    const body = ensureBlock(stmt.statement, factory);
    const instrumentedBody = instrumentBlock(body, sourceFile, factory);
    return factory.updateForStatement(
      stmt,
      stmt.initializer,
      stmt.condition,
      stmt.incrementor,
      instrumentedBody,
    );
  }

  if (ts.isForInStatement(stmt)) {
    const body = ensureBlock(stmt.statement, factory);
    const instrumentedBody = instrumentBlock(body, sourceFile, factory);
    return factory.updateForInStatement(stmt, stmt.initializer, stmt.expression, instrumentedBody);
  }

  if (ts.isForOfStatement(stmt)) {
    const body = ensureBlock(stmt.statement, factory);
    const instrumentedBody = instrumentBlock(body, sourceFile, factory);
    return factory.updateForOfStatement(
      stmt,
      stmt.awaitModifier,
      stmt.initializer,
      stmt.expression,
      instrumentedBody,
    );
  }

  if (ts.isWhileStatement(stmt)) {
    const body = ensureBlock(stmt.statement, factory);
    const instrumentedBody = instrumentBlock(body, sourceFile, factory);
    return factory.updateWhileStatement(stmt, stmt.expression, instrumentedBody);
  }

  if (ts.isDoStatement(stmt)) {
    const body = ensureBlock(stmt.statement, factory);
    const instrumentedBody = instrumentBlock(body, sourceFile, factory);
    return factory.updateDoStatement(stmt, instrumentedBody, stmt.expression);
  }

  if (ts.isTryStatement(stmt)) {
    const tryBlock = instrumentBlock(stmt.tryBlock, sourceFile, factory);

    let catchClause: ts.CatchClause | undefined;
    if (stmt.catchClause) {
      const catchBlock = instrumentBlock(stmt.catchClause.block, sourceFile, factory);
      catchClause = factory.updateCatchClause(
        stmt.catchClause,
        stmt.catchClause.variableDeclaration,
        catchBlock,
      );
    }

    let finallyBlock: ts.Block | undefined;
    if (stmt.finallyBlock) {
      finallyBlock = instrumentBlock(stmt.finallyBlock, sourceFile, factory);
    }

    return factory.updateTryStatement(stmt, tryBlock, catchClause, finallyBlock);
  }

  return stmt;
}

/**
 * Wrap a single statement in a block if it isn't already one.
 * This normalizes shorthand bodies like `if (x) return 1;` into
 * `if (x) { return 1; }` so we can insert recording calls.
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
