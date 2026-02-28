/**
 * TypeScript function analyzer using the TypeScript Compiler API.
 *
 * Given a file path and optional function name, extracts parameter types,
 * return type, and source location for exported functions.
 */

import * as ts from "typescript";
import * as path from "node:path";
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
} from "./protocol.js";

function hasExportModifier(node: ts.Node): boolean {
  const modifiers = ts.canHaveModifiers(node) ? ts.getModifiers(node) : undefined;
  return modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword) ?? false;
}

/**
 * Analyze functions in a TypeScript file.
 *
 * If `functionName` is provided, only that function is returned.
 * Otherwise, all top-level exported functions are returned.
 */
export function analyzeFile(filePath: string, functionName?: string | null): FunctionAnalysis[] {
  const absolutePath = path.resolve(filePath);
  const program = ts.createProgram([absolutePath], {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.Node16,
    strict: true,
    noEmit: true,
  });

  const sourceFile = program.getSourceFile(absolutePath);
  if (!sourceFile) {
    return [];
  }

  const checker = program.getTypeChecker();
  const results: FunctionAnalysis[] = [];

  ts.forEachChild(sourceFile, (node) => {
    if (ts.isFunctionDeclaration(node) && node.name) {
      const name = node.name.text;
      if (functionName != null && name !== functionName) {
        return;
      }
      const exported = hasExportModifier(node);
      const analysis = analyzeFunctionDeclaration(node, checker, sourceFile, exported);
      if (analysis) {
        results.push(analysis);
      }
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
        }
      }
    }
  });

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
  const params = node.parameters.map((p) => analyzeParameter(p, checker));
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const branches = node.body ? extractBranches(node.body, sourceFile, paramNames) : [];
  const dependencies = node.body ? extractDependencies(node.body, checker, sourceFile, paramNames) : [];

  return {
    name,
    exported,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
  };
}

function analyzeArrowFunction(
  name: string,
  node: ts.ArrowFunction,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile,
  exported: boolean,
): FunctionAnalysis {
  const params = node.parameters.map((p) => analyzeParameter(p, checker));
  const paramNames = new Set(params.map((p) => p.name));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  const body = ts.isBlock(node.body) ? node.body : null;
  const branches = body ? extractBranches(body, sourceFile, paramNames) : [];
  const dependencies = body ? extractDependencies(body, checker, sourceFile, paramNames) : [];

  return {
    name,
    exported,
    params,
    branches,
    dependencies,
    return_type: returnType,
    start_line: startLine,
    end_line: endLine,
  };
}

function analyzeParameter(param: ts.ParameterDeclaration, checker: ts.TypeChecker): ParamInfo {
  const name = ts.isIdentifier(param.name) ? param.name.text : param.name.getText();
  const symbol = checker.getSymbolAtLocation(param.name);
  const paramType = symbol
    ? checker.getTypeOfSymbolAtLocation(symbol, param)
    : checker.getTypeAtLocation(param);

  let typ = convertType(paramType, checker);

  // If the parameter has a ? token and the type isn't already nullable, wrap it
  if (param.questionToken && typ.kind !== "nullable") {
    typ = { kind: "nullable", inner: typ };
  }

  return { name, type: typ };
}

/**
 * Convert a TypeScript compiler type to our protocol TypeInfo.
 */
export function convertType(type: ts.Type, checker: ts.TypeChecker): TypeInfo {
  // Handle union types first (before flag checks, since unions have compound flags)
  if (type.isUnion()) {
    return convertUnionType(type, checker);
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
      ? convertType(typeArgs[0], checker)
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
    // Generic object types (interfaces, type literals, classes)
    return convertObjectType(type as ts.ObjectType, checker);
  }

  // ESSymbol / UniqueESSymbol
  if (flags & ts.TypeFlags.ESSymbol || flags & ts.TypeFlags.UniqueESSymbol) {
    return { kind: "complex", complex_kind: "symbol" };
  }

  return { kind: "unknown" };
}

function convertUnionType(type: ts.UnionType, checker: ts.TypeChecker): TypeInfo {
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
        ? convertType(nonNullVariants[0]!, checker)
        : {
            kind: "union" as const,
            variants: nonNullVariants.map((v) => convertType(v, checker)),
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
  const converted = variants.map((v) => convertType(v, checker));
  return { kind: "union", variants: converted };
}

// ---------------------------------------------------------------------------
// Branch extraction
// ---------------------------------------------------------------------------

/** Mutable state for assigning sequential branch IDs. */
interface BranchContext {
  sourceFile: ts.SourceFile;
  paramNames: Set<string>;
  nextId: number;
}

/**
 * Walk a function body and extract all branch points.
 */
function extractBranches(
  body: ts.Block,
  sourceFile: ts.SourceFile,
  paramNames: Set<string>,
): BranchInfo[] {
  const ctx: BranchContext = { sourceFile, paramNames, nextId: 0 };
  const branches: BranchInfo[] = [];
  walkForBranches(body, ctx, branches, false);
  return branches;
}

function walkForBranches(
  node: ts.Node,
  ctx: BranchContext,
  branches: BranchInfo[],
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
    walkForBranches(node.thenStatement, ctx, branches, false);
    if (node.elseStatement) {
      if (ts.isIfStatement(node.elseStatement)) {
        walkForBranches(node.elseStatement, ctx, branches, true);
      } else {
        walkForBranches(node.elseStatement, ctx, branches, false);
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
        walkForBranches(stmt, ctx, branches, false);
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
    walkForBranches(node.whenTrue, ctx, branches, false);
    walkForBranches(node.whenFalse, ctx, branches, false);
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
    walkForBranches(node.statement, ctx, branches, false);
    return;
  }

  if (ts.isForStatement(node)) {
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
    }
    walkForBranches(node.statement, ctx, branches, false);
    return;
  }

  // Recurse into child nodes
  ts.forEachChild(node, (child) => walkForBranches(child, ctx, branches, false));
}

/**
 * Check if a SymExpr is meaningful (not just "unknown").
 */
function isSymExprMeaningful(expr: SymExpr): boolean {
  return expr.kind !== "unknown";
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

function convertObjectType(type: ts.ObjectType, checker: ts.TypeChecker): TypeInfo {
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
    const converted = convertType(propType, checker);

    // Check if the property is optional
    const isOptional = (prop.flags & ts.SymbolFlags.Optional) !== 0;
    const fieldType = isOptional && converted.kind !== "nullable"
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
