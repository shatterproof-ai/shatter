/**
 * TypeScript function analyzer using the TypeScript Compiler API.
 *
 * Given a file path and optional function name, extracts parameter types,
 * return type, and source location for exported functions.
 */

import * as ts from "typescript";
import * as path from "node:path";
import type { FunctionAnalysis, ParamInfo, TypeInfo } from "./protocol.js";

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
      const analysis = analyzeFunctionDeclaration(node, checker, sourceFile);
      if (analysis) {
        results.push(analysis);
      }
    }

    if (ts.isVariableStatement(node)) {
      for (const decl of node.declarationList.declarations) {
        if (!ts.isIdentifier(decl.name)) continue;
        const name = decl.name.text;
        if (functionName != null && name !== functionName) continue;

        if (decl.initializer && ts.isArrowFunction(decl.initializer)) {
          const analysis = analyzeArrowFunction(name, decl.initializer, checker, sourceFile);
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
): FunctionAnalysis | null {
  if (!node.name) return null;

  const name = node.name.text;
  const params = node.parameters.map((p) => analyzeParameter(p, checker));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  return {
    name,
    params,
    branches: [],
    dependencies: [],
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
): FunctionAnalysis {
  const params = node.parameters.map((p) => analyzeParameter(p, checker));

  const sig = checker.getSignatureFromDeclaration(node);
  const returnType = sig
    ? convertType(checker.getReturnTypeOfSignature(sig), checker)
    : { kind: "unknown" as const };

  const startLine = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
  const endLine = sourceFile.getLineAndCharacterOfPosition(node.getEnd()).line + 1;

  return {
    name,
    params,
    branches: [],
    dependencies: [],
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

  return { name, typ };
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
    return { kind: "int" };
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

  // Object types (interfaces, type literals, classes)
  if (flags & ts.TypeFlags.Object) {
    return convertObjectType(type as ts.ObjectType, checker);
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
