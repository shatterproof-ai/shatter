/**
 * TypeScript function analyzer using the TypeScript Compiler API.
 *
 * Given a file path and optional function name, extracts parameter types,
 * return type, and source location for exported functions.
 */

import * as ts from "typescript";
import * as path from "node:path";
import { refineIntegerParams } from "./integer-heuristic.js";
import type {
  FunctionAnalysis,
  ParamInfo,
  TypeInfo,
  BranchInfo,
  BranchType,
  SymExpr,
  BinOpKind,
  UnOpKind,
  ExternalDependency,
  DependencyKind,
  LiteralValue,
  StaticOpacityReason,
  MediumOpacityReason,
  InductionVar,
  LoopInfo,
  BoundOp,
} from "./protocol.js";
import type { TimingCollector } from "./timing.js";

function hasExportModifier(node: ts.Node): boolean {
  const modifiers = ts.canHaveModifiers(node) ? ts.getModifiers(node) : undefined;
  return modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword) ?? false;
}

/** Default compiler options used when no tsconfig.json is available. */
const DEFAULT_COMPILER_OPTIONS: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.Node16,
  strict: true,
  noEmit: true,
  allowJs: true,
  jsx: ts.JsxEmit.ReactJSX,
};

/**
 * Load compiler options from tsconfig.json if a project root is provided,
 * falling back to hardcoded defaults on any error or when no root is given.
 */
function loadCompilerOptions(absoluteFilePath: string, projectRoot?: string): ts.CompilerOptions {
  if (!projectRoot) {
    return DEFAULT_COMPILER_OPTIONS;
  }

  const tsconfigPath = path.join(projectRoot, "tsconfig.json");
  const readResult = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
  if (readResult.error) {
    return DEFAULT_COMPILER_OPTIONS;
  }

  const parsed = ts.parseJsonConfigFileContent(
    readResult.config,
    ts.sys,
    projectRoot,
  );

  if (parsed.errors.length > 0) {
    return DEFAULT_COMPILER_OPTIONS;
  }

  // Preserve critical defaults that shatter needs
  return {
    ...parsed.options,
    noEmit: true,
    allowJs: true,
  };
}

/**
 * Analyze functions in a TypeScript file.
 *
 * If `functionName` is provided, only that function is returned.
 * Otherwise, all top-level exported functions are returned.
 */
export function analyzeFile(
  filePath: string,
  functionName?: string | null,
  projectRoot?: string | null,
  timing?: TimingCollector,
): FunctionAnalysis[] {
  const absolutePath = path.resolve(filePath);
  const compilerOptions = loadCompilerOptions(absolutePath, projectRoot ?? undefined);
  const program = timing
    ? timing.sync("analyze.ast", () => ts.createProgram([absolutePath], compilerOptions))
    : ts.createProgram([absolutePath], compilerOptions);

  const sourceFile = program.getSourceFile(absolutePath);
  if (!sourceFile) {
    return [];
  }

  const checker = program.getTypeChecker();
  const results: FunctionAnalysis[] = [];

  // Collect CommonJS-exported names so we can mark them as exported
  const commonJsExportedNames = collectCommonJsExports(sourceFile);

  const walk = (): void => {
    ts.forEachChild(sourceFile, (node) => {
      if (ts.isFunctionDeclaration(node)) {
        if (node.name) {
          const name = node.name.text;
          if (functionName != null && name !== functionName) {
            return;
          }
          const exported = hasExportModifier(node) || commonJsExportedNames.has(name);
          const analysis = analyzeFunctionDeclaration(node, checker, sourceFile, exported);
          if (analysis) {
            results.push(analysis);
          }
        } else if (hasExportModifier(node)) {
          // Unnamed default export: export default function(...) {}
          const syntheticName = "<default>";
          if (functionName != null && syntheticName !== functionName) {
            return;
          }
          const analysis = analyzeFunctionDeclarationUnnamed(node, syntheticName, checker, sourceFile);
          if (analysis) {
            results.push(analysis);
          }
        }
        return;
      }

      if (ts.isVariableStatement(node)) {
        const exported = hasExportModifier(node);
        for (const decl of node.declarationList.declarations) {
          if (!ts.isIdentifier(decl.name)) continue;
          const name = decl.name.text;
          if (functionName != null && name !== functionName) continue;

          if (decl.initializer && ts.isArrowFunction(decl.initializer)) {
            const analysis = analyzeArrowFunction(name, decl.initializer, checker, sourceFile, exported);
            if (analysis) {
              results.push(analysis);
            }
          } else if (decl.initializer && ts.isFunctionExpression(decl.initializer)) {
            const analysis = analyzeFunctionExpression(name, decl.initializer, checker, sourceFile, exported);
            if (analysis) {
              results.push(analysis);
            }
          }
        }
        return;
      }

      // CommonJS: exports.foo = function(...) {}
      if (ts.isExpressionStatement(node) && ts.isBinaryExpression(node.expression)) {
        const bin = node.expression;
        if (bin.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
            ts.isPropertyAccessExpression(bin.left) &&
            ts.isIdentifier(bin.left.expression) &&
            bin.left.expression.text === "exports") {
          const name = bin.left.name.text;
          if (functionName != null && name !== functionName) return;
          if (ts.isFunctionExpression(bin.right)) {
            const analysis = analyzeFunctionExpression(name, bin.right, checker, sourceFile, true);
            if (analysis) results.push(analysis);
          } else if (ts.isArrowFunction(bin.right)) {
            const analysis = analyzeArrowFunction(name, bin.right, checker, sourceFile, true);
            if (analysis) results.push(analysis);
          }
        }
      }
    });
  };

  if (timing) {
    timing.sync("analyze.walk", walk);
  } else {
    walk();
  }

  // Follow barrel re-exports when direct scanning found no functions and no
  // specific function was requested (whole-file analysis mode).
  if (results.length === 0 && functionName == null) {
    const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
    if (moduleSymbol) {
      const exports = checker.getExportsOfModule(moduleSymbol);
      // Group exported symbols by their declaring source file.
      const byFile = new Map<string, string[]>();

      for (const exp of exports) {
        const resolved =
          exp.flags & ts.SymbolFlags.Alias
            ? checker.getAliasedSymbol(exp)
            : exp;

        const decls = resolved.getDeclarations();
        if (!decls || decls.length === 0) continue;

        const declSourceFile = decls[0]!.getSourceFile();
        const declPath = declSourceFile.fileName;

        // Skip external package re-exports.
        if (declPath.includes("node_modules")) continue;
        // Skip self-references (avoid re-analyzing the barrel itself).
        if (path.resolve(declPath) === absolutePath) continue;

        const resolvedDeclPath = path.resolve(declPath);
        let names = byFile.get(resolvedDeclPath);
        if (!names) {
          names = [];
          byFile.set(resolvedDeclPath, names);
        }
        names.push(exp.name);
      }

      for (const [realPath, names] of byFile) {
        for (const name of names) {
          const subResults = analyzeFile(realPath, name, projectRoot, timing);
          for (const fn of subResults) {
            fn.source_file = realPath;
            results.push(fn);
          }
        }
      }
    }
  }

  return results;
}

function analyzeFunctionDeclaration(
  node: ts.FunctionDeclaration,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  exported: boolean,
): FunctionAnalysis | null {
  if (!node.name) return null;

  const name = node.name.text;
  const rawParams = node.parameters.map((p) => analyzeParameter(p, checker, sourceFile));
  const params = refineIntegerParams(rawParams, node.body, sourceFile);
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const { branches, loops } = node.body
    ? extractBranches(node.body, sourceFile, paramNames)
    : { branches: [], loops: [] };
  const dependencies = node.body ? extractDependencies(node.body, checker, sourceFile, paramNames) : [];
  const literals = extractLiterals(node, sourceFile, checker);

  return {
    name,
    exported,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
    ...(literals.length > 0 ? { literals } : {}),
    ...(loops.length > 0 ? { loops } : {}),
  };
}

function analyzeArrowFunction(
  name: string,
  node: ts.ArrowFunction,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  exported: boolean,
): FunctionAnalysis {
  const rawParams = node.parameters.map((p) => analyzeParameter(p, checker, sourceFile));
  const params = refineIntegerParams(rawParams, ts.isBlock(node.body) ? node.body : undefined, sourceFile);
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const body = ts.isBlock(node.body) ? node.body : null;
  const { branches, loops } = body
    ? extractBranches(body, sourceFile, paramNames)
    : { branches: [], loops: [] };
  const dependencies = body ? extractDependencies(body, checker, sourceFile, paramNames) : [];
  const literals = extractLiterals(node, sourceFile, checker);

  return {
    name,
    exported,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
    ...(literals.length > 0 ? { literals } : {}),
    ...(loops.length > 0 ? { loops } : {}),
  };
}

function analyzeFunctionDeclarationUnnamed(
  node: ts.FunctionDeclaration,
  syntheticName: string,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
): FunctionAnalysis {
  const rawParams = node.parameters.map((p) => analyzeParameter(p, checker, sourceFile));
  const params = refineIntegerParams(rawParams, node.body, sourceFile);
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const { branches, loops } = node.body
    ? extractBranches(node.body, sourceFile, paramNames)
    : { branches: [], loops: [] };
  const dependencies = node.body ? extractDependencies(node.body, checker, sourceFile, paramNames) : [];
  const literals = extractLiterals(node, sourceFile, checker);

  return {
    name: syntheticName,
    exported: true,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
    ...(literals.length > 0 ? { literals } : {}),
    ...(loops.length > 0 ? { loops } : {}),
  };
}

function analyzeFunctionExpression(
  name: string,
  node: ts.FunctionExpression,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  exported: boolean,
): FunctionAnalysis {
  const rawParams = node.parameters.map((p) => analyzeParameter(p, checker, sourceFile));
  const params = refineIntegerParams(rawParams, node.body, sourceFile);
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const body = node.body;
  const { branches, loops } = extractBranches(body, sourceFile, paramNames);
  const dependencies = extractDependencies(body, checker, sourceFile, paramNames);
  const literals = extractLiterals(node, sourceFile, checker);

  return {
    name,
    exported,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
    ...(literals.length > 0 ? { literals } : {}),
    ...(loops.length > 0 ? { loops } : {}),
  };
}

function collectCommonJsExports(sourceFile: ts.SourceFile): Set<string> {
  const names = new Set<string>();
  ts.forEachChild(sourceFile, (node) => {
    if (!ts.isExpressionStatement(node)) return;
    if (!ts.isBinaryExpression(node.expression)) return;
    const bin = node.expression;
    if (bin.operatorToken.kind !== ts.SyntaxKind.EqualsToken) return;

    if (ts.isPropertyAccessExpression(bin.left) &&
        ts.isIdentifier(bin.left.expression) &&
        bin.left.expression.text === "module" &&
        bin.left.name.text === "exports" &&
        ts.isObjectLiteralExpression(bin.right)) {
      for (const prop of bin.right.properties) {
        if (ts.isShorthandPropertyAssignment(prop)) {
          names.add(prop.name.text);
        } else if (ts.isPropertyAssignment(prop) && ts.isIdentifier(prop.name)) {
          names.add(prop.name.text);
        }
      }
    }
  });
  return names;
}

function analyzeParameter(
  param: ts.ParameterDeclaration,
  checker: ts.TypeChecker,
  sourceFile?: ts.SourceFile,
): ParamInfo {
  const name = ts.isIdentifier(param.name) ? param.name.text : param.name.getText();
  const symbol = checker.getSymbolAtLocation(param.name);
  const paramType = symbol
    ? checker.getTypeOfSymbolAtLocation(symbol, param)
    : checker.getTypeAtLocation(param);

  let typ = convertType(paramType, checker, sourceFile ?? null);

  // If the parameter has a ? token and the type isn't already nullable, wrap it
  if (param.questionToken && typ.kind !== "nullable") {
    typ = { kind: "nullable", inner: typ };
  }

  return { name, type: typ };
}

/**
 * Convert a TypeScript compiler type to our protocol TypeInfo.
 *
 * Pass `sourceFile` to enable static analysis heuristics for user-defined
 * types (abstract classes, interfaces with no implementors, etc.).
 */
export function convertType(
  type: ts.Type,
  checker: ts.TypeChecker,
  sourceFile?: ts.SourceFile | null,
): TypeInfo {
  // Handle union types first (before flag checks, since unions have compound flags)
  if (type.isUnion()) {
    return convertUnionType(type, checker, sourceFile);
  }

  const flags = type.getFlags();

  if (flags & ts.TypeFlags.String || flags & ts.TypeFlags.StringLiteral) {
    return { kind: "str" };
  }

  if (flags & ts.TypeFlags.Number || flags & ts.TypeFlags.NumberLiteral) {
    return { kind: "float" };
  }

  if (flags & ts.TypeFlags.BigInt || flags & ts.TypeFlags.BigIntLiteral) {
    return { kind: "complex", complex_kind: "big_int" };
  }

  if (flags & ts.TypeFlags.Boolean || flags & ts.TypeFlags.BooleanLiteral) {
    return { kind: "bool" };
  }

  if (flags & ts.TypeFlags.Void || flags & ts.TypeFlags.Undefined) {
    return { kind: "unknown" };
  }

  if (flags & ts.TypeFlags.Null) {
    return { kind: "unknown" };
  }

  // Check for array types
  if (checker.isArrayType(type)) {
    const typeArgs = (type as ts.TypeReference).typeArguments;
    const element = typeArgs?.[0]
      ? convertType(typeArgs[0], checker, sourceFile)
      : { kind: "unknown" as const };
    return { kind: "array", element };
  }

  // Check for enum types
  if (flags & ts.TypeFlags.Enum || flags & ts.TypeFlags.EnumLiteral) {
    return { kind: "str" };
  }

  // Check for well-known complex types by symbol name
  if (flags & ts.TypeFlags.Object) {
    // Check for Node.js opaque resource types first
    const opaqueLabel = isOpaqueType(type, checker);
    if (opaqueLabel) {
      return { kind: "opaque", label: opaqueLabel };
    }

    const symbol = type.getSymbol();
    const name = symbol?.getName();
    if (name) {
      const complexKind = complexKindFromSymbol(name);
      if (complexKind) {
        return { kind: "complex", complex_kind: complexKind };
      }
    }

    // Static analysis heuristics for user-defined types
    if (sourceFile) {
      const staticResult = detectStaticOpacity(type as ts.ObjectType, checker, sourceFile);
      if (staticResult) {
        return { kind: "opaque", label: staticResult.label, static_opacity: staticResult.reason };
      }
    }

    // Medium-confidence opaque detection: infra npm packages, closeable types, native handles
    const mediumResult = detectMediumOpacity(type as ts.ObjectType, checker, sourceFile ?? null);
    if (mediumResult) {
      return { kind: "opaque", label: mediumResult.label, medium_opacity: mediumResult.reason };
    }

    // Generic object types (interfaces, type literals, classes)
    return convertObjectType(type as ts.ObjectType, checker, sourceFile);
  }

  // ESSymbol / UniqueESSymbol
  if (flags & ts.TypeFlags.ESSymbol || flags & ts.TypeFlags.UniqueESSymbol) {
    return { kind: "complex", complex_kind: "symbol" };
  }

  return { kind: "unknown" };
}

function convertUnionType(
  type: ts.UnionType,
  checker: ts.TypeChecker,
  sourceFile?: ts.SourceFile | null,
): TypeInfo {
  const variants = type.types;

  // Check for nullable pattern: T | null or T | undefined
  const nullishVariants = variants.filter(
    (v) => v.getFlags() & (ts.TypeFlags.Null | ts.TypeFlags.Undefined),
  );
  const nonNullVariants = variants.filter(
    (v) => !(v.getFlags() & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)),
  );

  if (nullishVariants.length > 0 && nonNullVariants.length > 0) {
    const inner =
      nonNullVariants.length === 1
        ? convertType(nonNullVariants[0]!, checker, sourceFile)
        : {
            kind: "union" as const,
            variants: nonNullVariants.map((v) => convertType(v, checker, sourceFile)),
          };
    return { kind: "nullable", inner };
  }

  // Check for boolean (TypeScript represents boolean as true | false union)
  const allBooleanLiterals = variants.every(
    (v) => v.getFlags() & ts.TypeFlags.BooleanLiteral,
  );
  if (allBooleanLiterals && variants.length === 2) {
    return { kind: "bool" };
  }

  // Regular union
  const converted = variants.map((v) => convertType(v, checker, sourceFile));
  return { kind: "union", variants: converted };
}

// ---------------------------------------------------------------------------
// Branch extraction
// ---------------------------------------------------------------------------

/** Mutable state for assigning sequential branch and loop IDs. */
interface BranchContext {
  sourceFile: ts.SourceFile;
  paramNames: Set<string>;
  nextId: number;
  /** Counter incremented for every loop statement (for, while, do-while, for-of, for-in). */
  nextLoopId: number;
}

/**
 * Walk a function body and extract all branch points and loop induction info.
 */
function extractBranches(
  body: ts.Block,
  sourceFile: ts.SourceFile,
  paramNames: Set<string>,
): { branches: BranchInfo[]; loops: LoopInfo[] } {
  const ctx: BranchContext = { sourceFile, paramNames, nextId: 0, nextLoopId: 0 };
  const branches: BranchInfo[] = [];
  const loops: LoopInfo[] = [];
  walkForBranches(body, ctx, branches, loops, false);
  return { branches, loops };
}

function walkForBranches(
  node: ts.Node,
  ctx: BranchContext,
  branches: BranchInfo[],
  loops: LoopInfo[],
  isElseIf: boolean,
): void {
  if (ts.isIfStatement(node)) {
    const branchType: BranchType = isElseIf ? "else_if" : "if";
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
    const conditionText = node.expression.getText(ctx.sourceFile);
    const symCondition = buildSymExpr(node.expression, ctx.paramNames);
    branches.push({
      id: ctx.nextId++,
      line,
      condition_text: conditionText,
      condition: isSymExprMeaningful(symCondition) ? symCondition : null,
      branch_type: branchType,
    });

    // Recurse into then/else blocks
    walkForBranches(node.thenStatement, ctx, branches, loops, false);
    if (node.elseStatement) {
      if (ts.isIfStatement(node.elseStatement)) {
        walkForBranches(node.elseStatement, ctx, branches, loops, true);
      } else {
        walkForBranches(node.elseStatement, ctx, branches, loops, false);
      }
    }
    return;
  }

  if (ts.isSwitchStatement(node)) {
    for (const clause of node.caseBlock.clauses) {
      if (ts.isCaseClause(clause)) {
        const line = ctx.sourceFile.getLineAndCharacterOfPosition(clause.getStart(ctx.sourceFile)).line + 1;
        const conditionText = `${node.expression.getText(ctx.sourceFile)} === ${clause.expression.getText(ctx.sourceFile)}`;
        branches.push({
          id: ctx.nextId++,
          line,
          condition_text: conditionText,
          condition: null,
          branch_type: "switch",
        });
      }
      for (const stmt of clause.statements) {
        walkForBranches(stmt, ctx, branches, loops, false);
      }
    }
    return;
  }

  if (ts.isConditionalExpression(node)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
    const conditionText = node.condition.getText(ctx.sourceFile);
    const symCondition = buildSymExpr(node.condition, ctx.paramNames);
    branches.push({
      id: ctx.nextId++,
      line,
      condition_text: conditionText,
      condition: isSymExprMeaningful(symCondition) ? symCondition : null,
      branch_type: "ternary",
    });
    // Recurse into branches of the ternary
    walkForBranches(node.whenTrue, ctx, branches, loops, false);
    walkForBranches(node.whenFalse, ctx, branches, loops, false);
    return;
  }

  if (ts.isBinaryExpression(node)) {
    if (
      node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken ||
      node.operatorToken.kind === ts.SyntaxKind.BarBarToken
    ) {
      const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
      const conditionText = node.getText(ctx.sourceFile);
      const branchType: BranchType =
        node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken
          ? "logical_and"
          : "logical_or";
      const symCondition = buildSymExpr(node, ctx.paramNames);
      branches.push({
        id: ctx.nextId++,
        line,
        condition_text: conditionText,
        condition: isSymExprMeaningful(symCondition) ? symCondition : null,
        branch_type: branchType,
      });
      // Don't recurse into sub-expressions of this binary to avoid double-counting
      // nested && / || as separate branches — the top-level one captures it.
      // But do recurse into non-logical children.
      return;
    }
  }

  if (ts.isWhileStatement(node)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
    const conditionText = node.expression.getText(ctx.sourceFile);
    const symCondition = buildSymExpr(node.expression, ctx.paramNames);
    branches.push({
      id: ctx.nextId++,
      line,
      condition_text: conditionText,
      condition: isSymExprMeaningful(symCondition) ? symCondition : null,
      branch_type: "while",
    });
    // Increment loop counter for non-for loop statements
    ctx.nextLoopId++;
    walkForBranches(node.statement, ctx, branches, loops, false);
    return;
  }

  if (ts.isDoStatement(node)) {
    const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
    const conditionText = node.expression.getText(ctx.sourceFile);
    const symCondition = buildSymExpr(node.expression, ctx.paramNames);
    branches.push({
      id: ctx.nextId++,
      line,
      condition_text: conditionText,
      condition: isSymExprMeaningful(symCondition) ? symCondition : null,
      branch_type: "while",
    });
    ctx.nextLoopId++;
    walkForBranches(node.statement, ctx, branches, loops, false);
    return;
  }

  if (ts.isForOfStatement(node) || ts.isForInStatement(node)) {
    ctx.nextLoopId++;
    walkForBranches(node.statement, ctx, branches, loops, false);
    return;
  }

  if (ts.isForStatement(node)) {
    const loopId = ctx.nextLoopId++;
    if (node.condition) {
      const line = ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1;
      const conditionText = node.condition.getText(ctx.sourceFile);
      const symCondition = buildSymExpr(node.condition, ctx.paramNames);
      branches.push({
        id: ctx.nextId++,
        line,
        condition_text: conditionText,
        condition: isSymExprMeaningful(symCondition) ? symCondition : null,
        branch_type: "for",
      });

      // Attempt to detect induction variable for loop analysis
      const inductionVar = analyzeForLoopInductionVar(node, ctx.paramNames, ctx.sourceFile);
      if (inductionVar !== null) {
        loops.push({
          loop_id: loopId,
          line: ctx.sourceFile.getLineAndCharacterOfPosition(node.getStart(ctx.sourceFile)).line + 1,
          induction_var: inductionVar,
        });
      }
    }
    walkForBranches(node.statement, ctx, branches, loops, false);
    return;
  }

  // Recurse into child nodes
  ts.forEachChild(node, (child) => walkForBranches(child, ctx, branches, loops, false));
}

/**
 * Check if a SymExpr is meaningful (not just "unknown").
 */
function isSymExprMeaningful(expr: SymExpr): boolean {
  return expr.kind !== "unknown";
}

/**
 * Attempt to detect an induction variable in a canonical for loop.
 *
 * Recognizes the pattern: `for (let i = INIT; i OP BOUND; i STEP)` where
 * OP is one of <, <=, >, >= and STEP is ++, --, +=, -=, or `i = i ± STEP`.
 * Returns null for non-canonical loops (missing condition, float inits,
 * body that reassigns the induction variable, etc.).
 */
function analyzeForLoopInductionVar(
  node: ts.ForStatement,
  paramNames: Set<string>,
  sourceFile: ts.SourceFile,
): InductionVar | null {
  // 1. Require a variable declaration list with exactly one declaration
  if (!node.initializer || !ts.isVariableDeclarationList(node.initializer)) {
    return null;
  }
  const decls = node.initializer.declarations;
  if (decls.length !== 1) {
    return null;
  }
  const decl = decls[0]!;
  if (!decl.initializer) {
    return null;
  }
  if (!ts.isIdentifier(decl.name)) {
    return null;
  }
  const varName = decl.name.text;

  // 2. Reject float induction variables: check for decimal point in init literal
  if (ts.isNumericLiteral(decl.initializer) && decl.initializer.text.includes(".")) {
    return null;
  }

  // 3. Require a binary condition with <, <=, >, >= where one side is the induction var
  if (!node.condition || !ts.isBinaryExpression(node.condition)) {
    return null;
  }
  const cond = node.condition;
  let boundOp: BoundOp;
  let boundExprNode: ts.Expression;

  const tokenKind = cond.operatorToken.kind;
  if (tokenKind === ts.SyntaxKind.LessThanToken) {
    boundOp = "lt";
  } else if (tokenKind === ts.SyntaxKind.LessThanEqualsToken) {
    boundOp = "le";
  } else if (tokenKind === ts.SyntaxKind.GreaterThanToken) {
    boundOp = "gt";
  } else if (tokenKind === ts.SyntaxKind.GreaterThanEqualsToken) {
    boundOp = "ge";
  } else {
    return null;
  }

  // One side must be the induction variable identifier
  const leftIsVar = ts.isIdentifier(cond.left) && cond.left.text === varName;
  const rightIsVar = ts.isIdentifier(cond.right) && cond.right.text === varName;
  if (leftIsVar) {
    boundExprNode = cond.right;
  } else if (rightIsVar) {
    // Flip the operator direction
    boundExprNode = cond.left;
    if (boundOp === "lt") boundOp = "gt";
    else if (boundOp === "le") boundOp = "ge";
    else if (boundOp === "gt") boundOp = "lt";
    else if (boundOp === "ge") boundOp = "le";
  } else {
    return null;
  }

  // 4. Require an incrementor; determine step expression
  if (!node.incrementor) {
    return null;
  }
  let stepExpr: SymExpr | null = null;
  const inc = node.incrementor;

  if (ts.isPostfixUnaryExpression(inc) || ts.isPrefixUnaryExpression(inc)) {
    const operand = inc.operand;
    if (!ts.isIdentifier(operand) || operand.text !== varName) {
      return null;
    }
    if (inc.operator === ts.SyntaxKind.PlusPlusToken) {
      stepExpr = { kind: "const", type: "int", value: 1 };
    } else if (inc.operator === ts.SyntaxKind.MinusMinusToken) {
      stepExpr = { kind: "const", type: "int", value: -1 };
    } else {
      return null;
    }
  } else if (ts.isBinaryExpression(inc)) {
    const incOp = inc.operatorToken.kind;
    const incLeftIsVar = ts.isIdentifier(inc.left) && inc.left.text === varName;

    if (incOp === ts.SyntaxKind.PlusEqualsToken && incLeftIsVar) {
      // i += STEP
      stepExpr = buildSymExpr(inc.right, paramNames);
    } else if (incOp === ts.SyntaxKind.MinusEqualsToken && incLeftIsVar) {
      // i -= STEP
      const inner = buildSymExpr(inc.right, paramNames);
      stepExpr = { kind: "un_op", op: "neg", operand: inner };
    } else if (incOp === ts.SyntaxKind.EqualsToken && incLeftIsVar) {
      // i = i ± STEP
      if (!ts.isBinaryExpression(inc.right)) {
        return null;
      }
      const rhs = inc.right;
      const rhsOp = rhs.operatorToken.kind;
      const rhsLeftIsVar = ts.isIdentifier(rhs.left) && rhs.left.text === varName;
      const rhsRightIsVar = ts.isIdentifier(rhs.right) && rhs.right.text === varName;

      if (rhsOp === ts.SyntaxKind.PlusToken && rhsLeftIsVar) {
        // i = i + STEP
        stepExpr = buildSymExpr(rhs.right, paramNames);
      } else if (rhsOp === ts.SyntaxKind.PlusToken && rhsRightIsVar) {
        // i = STEP + i
        stepExpr = buildSymExpr(rhs.left, paramNames);
      } else if (rhsOp === ts.SyntaxKind.MinusToken && rhsLeftIsVar) {
        // i = i - STEP
        const inner = buildSymExpr(rhs.right, paramNames);
        stepExpr = { kind: "un_op", op: "neg", operand: inner };
      } else {
        return null;
      }
    } else {
      return null;
    }
  } else {
    return null;
  }

  if (stepExpr === null) {
    return null;
  }

  // 5. Conservative safety check: verify the induction variable is NOT reassigned in the body
  if (inductionVarIsModifiedInBody(node.statement, varName, sourceFile)) {
    return null;
  }

  // 6. Build init_expr and bound_expr from AST
  const initExpr = buildSymExpr(decl.initializer, paramNames);
  const boundExpr = buildSymExpr(boundExprNode, paramNames);

  return {
    name: varName,
    init_expr: initExpr,
    step_expr: stepExpr,
    bound_expr: boundExpr,
    bound_op: boundOp,
  };
}

/**
 * Returns true if the given variable name is modified anywhere inside `body`.
 *
 * Checks for: `varName = ...`, `varName += ...`, `varName -= ...`,
 * `varName++`, `++varName`, `varName--`, `--varName`.
 */
function inductionVarIsModifiedInBody(
  body: ts.Statement,
  varName: string,
  _sourceFile: ts.SourceFile,
): boolean {
  let found = false;

  function walk(node: ts.Node): void {
    if (found) return;

    if (ts.isBinaryExpression(node)) {
      const op = node.operatorToken.kind;
      const isAssign =
        op === ts.SyntaxKind.EqualsToken ||
        op === ts.SyntaxKind.PlusEqualsToken ||
        op === ts.SyntaxKind.MinusEqualsToken ||
        op === ts.SyntaxKind.AsteriskEqualsToken ||
        op === ts.SyntaxKind.SlashEqualsToken;
      if (isAssign && ts.isIdentifier(node.left) && node.left.text === varName) {
        found = true;
        return;
      }
    }

    if (ts.isPostfixUnaryExpression(node) || ts.isPrefixUnaryExpression(node)) {
      const op = node.operator;
      if (
        (op === ts.SyntaxKind.PlusPlusToken || op === ts.SyntaxKind.MinusMinusToken) &&
        ts.isIdentifier(node.operand) &&
        node.operand.text === varName
      ) {
        found = true;
        return;
      }
    }

    ts.forEachChild(node, walk);
  }

  walk(body);
  return found;
}

// ---------------------------------------------------------------------------
// External dependency detection
// ---------------------------------------------------------------------------

/** Accumulator for grouping call sites by symbol. */
interface DependencyAccumulator {
  kind: DependencyKind;
  symbol: string;
  sourceModule: string;
  returnType: TypeInfo;
  paramTypes: TypeInfo[];
  callSites: number[];
}

/**
 * Walk a function body and detect calls to external (imported/other-file) functions.
 */
function extractDependencies(
  body: ts.Block,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  paramNames: Set<string>,
): ExternalDependency[] {
  const accumulators = new Map<string, DependencyAccumulator>();

  walkForDependencies(body, checker, sourceFile, paramNames, accumulators);

  return Array.from(accumulators.values()).map((acc) => ({
    kind: acc.kind,
    symbol: acc.symbol,
    source_module: acc.sourceModule,
    return_type: acc.returnType,
    param_types: acc.paramTypes,
    call_sites: acc.callSites,
  }));
}

function walkForDependencies(
  node: ts.Node,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  paramNames: Set<string>,
  accumulators: Map<string, DependencyAccumulator>,
): void {
  if (ts.isCallExpression(node)) {
    const callee = node.expression;
    const line = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;

    let symbol: ts.Symbol | undefined;
    let depKind: DependencyKind = "function_call";
    let symbolName: string | undefined;

    if (ts.isIdentifier(callee)) {
      symbol = checker.getSymbolAtLocation(callee);
      symbolName = callee.text;
    } else if (ts.isPropertyAccessExpression(callee)) {
      symbol = checker.getSymbolAtLocation(callee.name);
      symbolName = callee.getText(sourceFile);
      depKind = "method_call";
    }

    if (symbol && symbolName) {
      const resolvedSymbol = resolveAliasedSymbol(symbol, checker);
      const declSourceFile = getDeclarationSourceFile(resolvedSymbol);

      if (declSourceFile && declSourceFile !== sourceFile.fileName) {
        const sourceModule = formatSourceModule(declSourceFile, sourceFile.fileName);
        const key = `${symbolName}::${sourceModule}`;

        const existing = accumulators.get(key);
        if (existing) {
          existing.callSites.push(line);
        } else {
          const sig = checker.getResolvedSignature(node);
          const returnType = sig
            ? convertType(checker.getReturnTypeOfSignature(sig), checker)
            : { kind: "unknown" as const };
          const paramTypesArr: TypeInfo[] = sig
            ? sig.getParameters().map((p) => convertType(checker.getTypeOfSymbolAtLocation(p, node), checker))
            : [];

          accumulators.set(key, {
            kind: depKind,
            symbol: symbolName,
            sourceModule,
            returnType,
            paramTypes: paramTypesArr,
            callSites: [line],
          });
        }
      }
    }
  }

  ts.forEachChild(node, (child) =>
    walkForDependencies(child, checker, sourceFile, paramNames, accumulators),
  );
}

/**
 * Follow alias chains (import aliases) to the original symbol.
 */
function resolveAliasedSymbol(symbol: ts.Symbol, checker: ts.TypeChecker): ts.Symbol {
  if (symbol.flags & ts.SymbolFlags.Alias) {
    return checker.getAliasedSymbol(symbol);
  }
  return symbol;
}

/**
 * Get the file name of a symbol's declaration, or undefined if unknown.
 */
function getDeclarationSourceFile(symbol: ts.Symbol): string | undefined {
  const declarations = symbol.getDeclarations();
  if (declarations && declarations.length > 0) {
    return declarations[0]!.getSourceFile().fileName;
  }
  return undefined;
}

/**
 * Format a source module path relative to the current file, producing a
 * human-readable module specifier.
 */
function formatSourceModule(declFile: string, currentFile: string): string {
  // For node_modules, extract the package name
  const nodeModulesIdx = declFile.lastIndexOf("node_modules/");
  if (nodeModulesIdx !== -1) {
    const afterNodeModules = declFile.substring(nodeModulesIdx + "node_modules/".length);
    // Handle scoped packages (@scope/name)
    if (afterNodeModules.startsWith("@")) {
      const parts = afterNodeModules.split("/");
      if (parts.length >= 2) {
        return `${parts[0]}/${parts[1]}`;
      }
    }
    const firstSlash = afterNodeModules.indexOf("/");
    return firstSlash === -1 ? afterNodeModules : afterNodeModules.substring(0, firstSlash);
  }

  // For project files, compute relative path
  const rel = path.relative(path.dirname(currentFile), declFile);
  const withoutExt = rel.replace(/\.(ts|tsx|js|jsx|d\.ts)$/, "");
  return withoutExt.startsWith(".") ? withoutExt : `./${withoutExt}`;
}

// ---------------------------------------------------------------------------
// Opaque type recognition (Node.js runtime resource types)
// ---------------------------------------------------------------------------

/** Known Node.js module → opaque type names. */
const OPAQUE_NODE_TYPES: ReadonlyMap<string, ReadonlySet<string>> = new Map([
  ["net", new Set(["Socket", "Server"])],
  ["http", new Set(["IncomingMessage", "ServerResponse", "Server"])],
  ["stream", new Set(["Readable", "Writable", "Transform", "Duplex", "PassThrough"])],
  ["child_process", new Set(["ChildProcess"])],
  ["worker_threads", new Set(["Worker"])],
  ["fs", new Set(["ReadStream", "WriteStream"])],
  ["tls", new Set(["TLSSocket", "Server"])],
  ["dgram", new Set(["Socket"])],
]);

/** Reverse lookup: type name → list of modules that define it. */
const OPAQUE_NAME_TO_MODULES: ReadonlyMap<string, readonly string[]> = (() => {
  const map = new Map<string, string[]>();
  for (const [mod, names] of OPAQUE_NODE_TYPES) {
    for (const name of names) {
      const existing = map.get(name);
      if (existing) {
        existing.push(mod);
      } else {
        map.set(name, [mod]);
      }
    }
  }
  return map;
})();

/**
 * Check if a type is a known Node.js opaque resource type.
 * Returns the "module.TypeName" label or null.
 */
function isOpaqueType(type: ts.Type, checker: ts.TypeChecker): string | null {
  const symbol = type.getSymbol();
  if (!symbol) return null;

  const name = symbol.getName();
  const candidateModules = OPAQUE_NAME_TO_MODULES.get(name);
  if (!candidateModules) return null;

  // Verify the declaration comes from @types/node
  const declarations = symbol.getDeclarations();
  if (!declarations || declarations.length === 0) return null;

  for (const decl of declarations) {
    const fileName = decl.getSourceFile().fileName;
    if (!fileName.includes("@types/node")) continue;

    // Determine the module from the file path
    for (const mod of candidateModules) {
      const modulePattern = `/${mod.replace("_", "_")}`;
      if (fileName.includes(modulePattern) || fileName.includes(`/${mod}.d.ts`)) {
        return `${mod}.${name}`;
      }
    }

    // Fallback: if it's from @types/node but we can't determine the exact module,
    // use the first candidate module
    return `${candidateModules[0]}.${name}`;
  }

  return null;
}

// ---------------------------------------------------------------------------
// Medium-confidence opaque type heuristics
// ---------------------------------------------------------------------------

/** npm package names whose types are medium-confidence opaque infrastructure resources. */
const INFRA_NPM_PACKAGES: ReadonlySet<string> = new Set([
  "pg", "redis", "ioredis", "mongoose", "typeorm", "knex",
  "sequelize", "nodemailer", "prisma", "mysql2", "mysql",
  "mssql", "cassandra-driver", "couchbase",
]);

/** npm scoped package prefixes for medium-confidence infra detection. */
const INFRA_NPM_PREFIXES: readonly string[] = ["@aws-sdk/", "@google-cloud/", "@azure/"];

/**
 * Detects medium-confidence signals that a type may be an opaque infrastructure resource.
 * Returns a reason and label if detected, or null.
 *
 * Medium-confidence signal: type is returned as kind:"opaque" with medium_opacity set,
 * but check_executability in the Rust core does NOT skip based on this alone.
 * This is advisory metadata for learning mode — see executability.rs for skip policy.
 *
 * These signals are suggestive but not definitive — a single medium-confidence signal
 * should not alone produce a high-confidence opaque suggestion.
 *
 * Heuristics (first match wins):
 * 1. Type declared in a known infrastructure npm package → "infrastructure_package"
 * 2. Type has close()/dispose()/destroy() method → "closeable_interface"
 * 3. Type has field named fd/handle/fileDescriptor → "native_handle_field"
 *
 * Heuristics 2 and 3 only apply to types declared in the current source file.
 */
function detectMediumOpacity(
  type: ts.ObjectType,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile | null,
): { reason: MediumOpacityReason; label: string } | null {
  const sym = type.getSymbol();
  if (!sym) return null;
  const decls = sym.getDeclarations() ?? [];
  if (decls.length === 0) return null;

  const typeName = sym.getName();
  const mainDecl = decls[0]!;
  const declFile = mainDecl.getSourceFile().fileName;

  // Heuristic 1: Type from a known infrastructure npm package.
  // Applies to external types (node_modules) regardless of sourceFile.
  const nodeModulesIdx = declFile.lastIndexOf("node_modules/");
  if (nodeModulesIdx !== -1) {
    const afterNodeModules = declFile.substring(nodeModulesIdx + "node_modules/".length);
    let pkgName: string;
    if (afterNodeModules.startsWith("@")) {
      const parts = afterNodeModules.split("/");
      pkgName = parts.length >= 2 ? `${parts[0]}/${parts[1]}` : afterNodeModules;
    } else {
      const slash = afterNodeModules.indexOf("/");
      pkgName = slash === -1 ? afterNodeModules : afterNodeModules.substring(0, slash);
    }
    if (INFRA_NPM_PACKAGES.has(pkgName)) {
      return { reason: "infrastructure_package", label: `${pkgName}.${typeName}` };
    }
    for (const prefix of INFRA_NPM_PREFIXES) {
      if (pkgName.startsWith(prefix) || afterNodeModules.startsWith(prefix)) {
        return { reason: "infrastructure_package", label: `${pkgName}.${typeName}` };
      }
    }
  }

  // Heuristics 2 & 3: only apply to types in the current source file
  if (!sourceFile || mainDecl.getSourceFile() !== sourceFile) return null;

  // Heuristic 2: has close()/dispose()/destroy() method
  const props = type.getProperties();
  const hasCleanupMethod = props.some((prop) => {
    const name = prop.getName();
    if (!["close", "dispose", "destroy"].includes(name)) return false;
    const propType = checker.getTypeOfSymbol(prop);
    return propType.getCallSignatures().length > 0;
  });
  if (hasCleanupMethod) {
    return { reason: "closeable_interface", label: typeName };
  }

  // Heuristic 3: has field named fd/handle/fileDescriptor
  const HANDLE_FIELD_NAMES = new Set(["fd", "_fd", "handle", "_handle", "fileDescriptor", "FileDescriptor"]);
  const hasHandleField = props.some((prop) => HANDLE_FIELD_NAMES.has(prop.getName()));
  if (hasHandleField) {
    return { reason: "native_handle_field", label: typeName };
  }

  return null;
}

// ---------------------------------------------------------------------------
// Complex type recognition
// ---------------------------------------------------------------------------

import type { ComplexKind } from "./protocol.js";

/** Map well-known TypeScript class/interface names to ComplexKind. */
function complexKindFromSymbol(name: string): ComplexKind | null {
  switch (name) {
    case "Date": return "date";
    case "RegExp": return "reg_exp";
    case "URL": return "url";
    case "Error":
    case "TypeError":
    case "RangeError":
    case "SyntaxError":
    case "ReferenceError":
    case "URIError":
    case "EvalError":
      return "error";
    case "Buffer":
    case "Uint8Array":
    case "Int8Array":
    case "Uint16Array":
    case "Int16Array":
    case "Uint32Array":
    case "Int32Array":
    case "Float32Array":
    case "Float64Array":
      return "buffer";
    default:
      return null;
  }
}

// Object type conversion
// ---------------------------------------------------------------------------

/**
 * Detects if a TypeScript class or interface type is opaque via static analysis.
 * Returns a reason and label if detected, or null.
 *
 * Heuristics (first match wins):
 * 1. Abstract class → "abstract_type"
 * 2. All constructors are private/protected → "abstract_type"
 * 3. Interface with no concrete implementors in sourceFile → "no_implementors"
 * 4. Class with public ctor but no exported factory (createX, newX, openX) → "no_constructor"
 * 5. All public constructors require an opaque argument → "transitively_opaque"
 *
 * Only analyzes types declared in the current source file to avoid false
 * positives on types from other modules.
 */
function detectStaticOpacity(
  type: ts.ObjectType,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
): { reason: StaticOpacityReason; label: string } | null {
  const sym = type.getSymbol();
  if (!sym) return null;
  const decls = sym.getDeclarations() ?? [];
  if (decls.length === 0) return null;

  // Only analyze types declared in the current source file
  const mainDecl = decls[0]!;
  if (mainDecl.getSourceFile() !== sourceFile) return null;

  const typeName = sym.getName();
  const label = typeName;

  // 1. Abstract class
  if (
    decls.some(
      (d) =>
        ts.isClassDeclaration(d) &&
        ts.getModifiers(d)?.some((m) => m.kind === ts.SyntaxKind.AbstractKeyword),
    )
  ) {
    return { reason: "abstract_type", label };
  }

  // 2. All constructors are private/protected
  const classDecl = decls.find((d): d is ts.ClassDeclaration => ts.isClassDeclaration(d));
  if (classDecl) {
    const ctors = classDecl.members.filter(
      (m): m is ts.ConstructorDeclaration => ts.isConstructorDeclaration(m),
    );
    if (ctors.length > 0) {
      const allNonPublic = ctors.every((ctor) =>
        ts.getModifiers(ctor)?.some(
          (m) =>
            m.kind === ts.SyntaxKind.PrivateKeyword ||
            m.kind === ts.SyntaxKind.ProtectedKeyword,
        ),
      );
      if (allNonPublic) return { reason: "abstract_type", label };
    }
  }

  // 3. Interface with no concrete implementors in file.
  //    Only applies to "service" interfaces (all members are method signatures).
  //    Data/shape interfaces (with non-method properties) can be satisfied by plain
  //    object literals and are therefore constructible — do not flag them.
  const isInterface = decls.some((d) => ts.isInterfaceDeclaration(d));
  if (isInterface) {
    const props = type.getProperties();
    // If the interface has any non-method property, it is a data-shape interface
    // that can be satisfied with a plain object literal — not opaque.
    const hasNonMethodProperty = props.some((prop) => {
      const propType = checker.getTypeOfSymbol(prop);
      return propType.getCallSignatures().length === 0;
    });
    if (hasNonMethodProperty) return null;

    // getEffectiveImplementsTypeNodes is available at runtime but not in all TypeScript
    // declaration versions; use a dynamic call to avoid compilation errors. If absent,
    // skip the heuristic entirely — returning null is safe and avoids false positives.
    const getImplNodes = (ts as unknown as Record<string, unknown>)["getEffectiveImplementsTypeNodes"] as
      | ((node: ts.ClassDeclaration) => readonly ts.ExpressionWithTypeArguments[] | undefined)
      | undefined;
    if (!getImplNodes) return null;

    const hasImplementor = sourceFile.statements.some((stmt) => {
      if (!ts.isClassDeclaration(stmt)) return false;
      const implTypes = getImplNodes(stmt);
      if (!implTypes) return false;
      return implTypes.some((impl: ts.ExpressionWithTypeArguments) => {
        const implSym = checker.getSymbolAtLocation(impl.expression);
        return implSym === sym || implSym?.name === typeName;
      });
    });
    if (!hasImplementor) return { reason: "no_implementors", label };
    return null;
  }

  // 4 (transitively_opaque) only applies to classes
  if (!classDecl) return null;

  const ctors = classDecl.members.filter(
    (m): m is ts.ConstructorDeclaration => ts.isConstructorDeclaration(m),
  );

  const publicCtors = ctors.filter(
    (ctor) =>
      !ts.getModifiers(ctor)?.some(
        (m) =>
          m.kind === ts.SyntaxKind.PrivateKeyword ||
          m.kind === ts.SyntaxKind.ProtectedKeyword,
      ),
  );

  // Only classes with at least one explicit public constructor are candidates.
  // Classes with an implicit default constructor are freely constructible.
  if (ctors.length === 0) return null;
  if (publicCtors.length === 0) return null; // already caught by rule 2

  // 4. Check transitively opaque: all public ctors have ≥1 opaque param.
  //    We do NOT flag classes whose public constructors only take primitive args
  //    because those are freely constructible (e.g. class Foo { constructor(n: number) {} }).
  const allTransitive = publicCtors.every((ctor) =>
    ctor.parameters.some((param) => {
      const paramType = checker.getTypeAtLocation(param);
      // Pass null as sourceFile to avoid infinite recursion
      const converted = convertType(paramType, checker, null);
      return converted.kind === "opaque";
    }),
  );
  if (allTransitive) return { reason: "transitively_opaque", label };

  // NOTE: "no_constructor" is deliberately not produced here.
  // A class with at least one public explicit constructor and no opaque args is freely
  // constructible from outside (e.g. `new Foo(42)`). A "factory-less" heuristic
  // (flag classes with no exported create*/new* function) was evaluated but proved too
  // noisy — many plain data classes have public constructors and no factory, yet are
  // trivially synthesizable. Rule 2 already covers the only reliable case (all ctors private).
  return null;
}

function convertObjectType(
  type: ts.ObjectType,
  checker: ts.TypeChecker,
  sourceFile?: ts.SourceFile | null,
): TypeInfo {
  // Skip callable signatures (function types)
  const callSignatures = type.getCallSignatures();
  if (callSignatures.length > 0) {
    return { kind: "unknown" };
  }

  const properties = type.getProperties();
  if (properties.length === 0) {
    return { kind: "object", fields: [] };
  }

  const fields: [string, TypeInfo][] = properties.map((prop) => {
    const propType = checker.getTypeOfSymbol(prop);
    const isOptional = (prop.flags & ts.SymbolFlags.Optional) !== 0;

    // Per-field callable check: function-typed fields cannot be generated by the
    // solver, so treat them as unknown. Optional callable fields become nullable so
    // input_gen can omit them (~30% of the time), allowing the engine to reach the
    // real transformation logic instead of only TypeError paths.
    if (propType.getCallSignatures().length > 0) {
      if (isOptional) {
        return [prop.name, { kind: "nullable" as const, inner: { kind: "unknown" as const } }];
      }
      return [prop.name, { kind: "unknown" as const }];
    }

    // Do not pass sourceFile into field types to avoid false positives on
    // fields whose types happen to match heuristic patterns out of context.
    const converted = convertType(propType, checker);
    const fieldType =
      isOptional && converted.kind !== "nullable"
        ? { kind: "nullable" as const, inner: converted }
        : converted;

    return [prop.name, fieldType];
  });

  return { kind: "object", fields };
}

// ---------------------------------------------------------------------------
// Symbolic expression builder (local copy for analyzer independence)
// ---------------------------------------------------------------------------

/**
 * Convert a TypeScript AST expression into a SymExpr matching the protocol
 * format. Expressions referencing function parameters produce symbolic nodes;
 * everything else falls back to Unknown.
 */
function buildSymExpr(
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
    const chain = resolveParamPropertyChain(expr, paramNames);
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
    const op = mapBinaryOp(expr.operatorToken.kind);
    if (op) {
      const left = buildSymExpr(expr.left, paramNames);
      const right = buildSymExpr(expr.right, paramNames);
      return { kind: "bin_op", op, left, right };
    }
    return { kind: "unknown" };
  }

  if (ts.isPrefixUnaryExpression(expr)) {
    const op = mapUnaryOp(expr.operator);
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

function resolveParamPropertyChain(
  expr: ts.PropertyAccessExpression,
  paramNames: Set<string>,
): { name: string; path: string[] } | null {
  const pathParts: string[] = [];
  let current: ts.Expression = expr;

  while (ts.isPropertyAccessExpression(current)) {
    pathParts.unshift(current.name.text);
    current = current.expression;
  }

  if (ts.isIdentifier(current) && paramNames.has(current.text)) {
    return { name: current.text, path: pathParts };
  }
  return null;
}

function mapBinaryOp(kind: ts.SyntaxKind): BinOpKind | null {
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

function mapUnaryOp(kind: ts.PrefixUnaryOperator): UnOpKind | null {
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
// Literal extraction — collect all constant values from source code
// ---------------------------------------------------------------------------

/**
 * Walk a function's body (and default parameter values), plus file-level
 * constants, enums, union type literal members, and property access keys
 * to extract candidate test inputs. Results are deduplicated.
 */
function extractLiterals(
  node: ts.FunctionDeclaration | ts.ArrowFunction | ts.FunctionExpression,
  sourceFile: ts.SourceFile,
  checker: ts.TypeChecker,
): LiteralValue[] {
  const seen = new Set<string>();
  const results: LiteralValue[] = [];

  function add(lit: LiteralValue): void {
    const key = JSON.stringify(lit);
    if (!seen.has(key)) {
      seen.add(key);
      results.push(lit);
    }
  }

  function addNumeric(num: number): void {
    if (Number.isSafeInteger(num)) {
      add({ type: "int", value: num });
    } else {
      add({ type: "float", value: num });
    }
  }

  function walk(n: ts.Node): void {
    if (ts.isStringLiteral(n)) {
      add({ type: "str", value: n.text });
    } else if (ts.isNoSubstitutionTemplateLiteral(n)) {
      add({ type: "str", value: n.text });
    } else if (ts.isNumericLiteral(n)) {
      addNumeric(Number(n.text));
    } else if (n.kind === ts.SyntaxKind.TrueKeyword) {
      add({ type: "bool", value: true });
    } else if (n.kind === ts.SyntaxKind.FalseKeyword) {
      add({ type: "bool", value: false });
    } else if (ts.isRegularExpressionLiteral(n)) {
      const text = n.text;
      const lastSlash = text.lastIndexOf("/");
      const pattern = text.slice(1, lastSlash);
      add({ type: "regex", pattern });
    } else if (ts.isPropertyAccessExpression(n)) {
      // Extract property access keys as candidate string inputs
      add({ type: "str", value: n.name.text });
    } else if (
      ts.isElementAccessExpression(n) &&
      n.argumentExpression &&
      ts.isStringLiteral(n.argumentExpression)
    ) {
      // Extract bracket-access string keys: obj["key"]
      add({ type: "str", value: n.argumentExpression.text });
    }
    ts.forEachChild(n, walk);
  }

  // Walk default parameter values
  for (const param of node.parameters) {
    if (param.initializer) {
      walk(param.initializer);
    }
  }

  // Walk function body
  if (node.body) {
    walk(node.body);
  }

  // Extract file-level const declarations
  ts.forEachChild(sourceFile, (child) => {
    if (ts.isVariableStatement(child)) {
      const isConst =
        (child.declarationList.flags & ts.NodeFlags.Const) !== 0;
      if (!isConst) return;
      for (const decl of child.declarationList.declarations) {
        if (!decl.initializer) continue;
        // Skip function-valued consts (they're not literal candidates)
        if (
          ts.isArrowFunction(decl.initializer) ||
          ts.isFunctionExpression(decl.initializer)
        )
          continue;
        walk(decl.initializer);
      }
    }

    // Extract enum member values
    if (ts.isEnumDeclaration(child)) {
      for (const member of child.members) {
        if (member.initializer) {
          if (ts.isStringLiteral(member.initializer)) {
            add({ type: "str", value: member.initializer.text });
          } else if (ts.isNumericLiteral(member.initializer)) {
            addNumeric(Number(member.initializer.text));
          }
        }
      }
    }
  });

  // Extract union type literal members from parameter types
  for (const param of node.parameters) {
    const paramType = checker.getTypeAtLocation(param);
    if (paramType.isUnion()) {
      for (const variant of paramType.types) {
        if (variant.isStringLiteral()) {
          add({ type: "str", value: variant.value });
        } else if (variant.isNumberLiteral()) {
          addNumeric(variant.value);
        }
      }
    }
  }

  return results;
}
