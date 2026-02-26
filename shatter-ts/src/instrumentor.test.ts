/* eslint-disable @typescript-eslint/no-non-null-assertion */
import ts from "typescript";
import { instrumentFunction, buildSymExpr, RECORD_FUNCTION, BRANCH_FUNCTION } from "./instrumentor";
import type { SymExpr, BranchDecision, SymConstraint } from "./protocol";

/** Transpile TypeScript to JavaScript so it can be executed with new Function(). */
function transpileToJs(tsSource: string): string {
  const result = ts.transpileModule(tsSource, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.None,
      removeComments: false,
    },
  });
  return result.outputText;
}

/** Execute instrumented code and return the recorded line numbers. */
function executeAndRecord(instrumentedSource: string, functionName: string, args: unknown[]): number[] {
  const recorded: number[] = [];
  const jsSource = transpileToJs(instrumentedSource);
  const fn = new Function(
    RECORD_FUNCTION,
    BRANCH_FUNCTION,
    `${jsSource}\nreturn ${functionName}(${args.map((a) => JSON.stringify(a)).join(", ")});`,
  );
  fn(
    (line: number) => recorded.push(line),
    (_id: number, _line: number, cond: boolean, _sym: unknown) => cond,
  );
  return recorded;
}

/** Execute instrumented code and return both lines and branch decisions. */
function executeAndCollect(
  instrumentedSource: string,
  functionName: string,
  args: unknown[],
): { lines: number[]; branches: BranchDecision[]; returnValue: unknown } {
  const lines: number[] = [];
  const branches: BranchDecision[] = [];
  const jsSource = transpileToJs(instrumentedSource);
  const fn = new Function(
    RECORD_FUNCTION,
    BRANCH_FUNCTION,
    `${jsSource}\nreturn ${functionName}(${args.map((a) => JSON.stringify(a)).join(", ")});`,
  );
  const returnValue = fn(
    (line: number) => lines.push(line),
    (branchId: number, line: number, cond: boolean, symExpr: SymExpr) => {
      const constraint: SymConstraint = symExpr.kind !== "unknown"
        ? { kind: "expr", expr: symExpr }
        : { kind: "unknown", hint: "unsupported expression" };
      branches.push({ branch_id: branchId, line, taken: cond, constraint });
      return cond;
    },
  );
  return { lines, branches, returnValue };
}

describe("instrumentFunction", () => {
  it("returns error when function is not found", () => {
    const result = instrumentFunction("const x = 1;", "missing");
    expect(result).toEqual({ error: "Function 'missing' not found" });
  });

  it("instruments linear code recording each statement line", () => {
    const source = `function add(a: number, b: number): number {
  const sum = a + b;
  return sum;
}`;
    const result = instrumentFunction(source, "add");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(RECORD_FUNCTION);
    const lines = executeAndRecord(result.instrumentedSource, "add", [1, 2]);
    expect(lines).toEqual([2, 3]);
  });

  it("instruments if/else recording the taken branch", () => {
    const source = `function classify(x: number): string {
  if (x > 0) {
    return "positive";
  } else {
    return "non-positive";
  }
}`;
    const result = instrumentFunction(source, "classify");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const posLines = executeAndRecord(result.instrumentedSource, "classify", [5]);
    expect(posLines).toEqual([2, 3]);

    const negLines = executeAndRecord(result.instrumentedSource, "classify", [-1]);
    expect(negLines).toEqual([2, 5]);
  });

  it("instruments while loops recording loop body on each iteration", () => {
    const source = `function countdown(n: number): number {
  let count = 0;
  while (n > 0) {
    count++;
    n--;
  }
  return count;
}`;
    const result = instrumentFunction(source, "countdown");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const lines = executeAndRecord(result.instrumentedSource, "countdown", [3]);
    // line 2: let count = 0
    // line 3: while (n > 0)
    // line 4: count++ (x3)
    // line 5: n-- (x3)
    // line 7: return count
    expect(lines).toEqual([2, 3, 4, 5, 4, 5, 4, 5, 7]);
  });

  it("instruments for loops recording body on each iteration", () => {
    const source = `function sumTo(n: number): number {
  let total = 0;
  for (let i = 1; i <= n; i++) {
    total += i;
  }
  return total;
}`;
    const result = instrumentFunction(source, "sumTo");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const lines = executeAndRecord(result.instrumentedSource, "sumTo", [2]);
    // line 2: let total = 0
    // line 3: for loop
    // line 4: total += i (x2)
    // line 6: return total
    expect(lines).toEqual([2, 3, 4, 4, 6]);
  });

  it("instruments switch statements recording the matched case", () => {
    const source = `function describe(x: number): string {
  switch (x) {
    case 1:
      return "one";
    case 2:
      return "two";
    default:
      return "other";
  }
}`;
    const result = instrumentFunction(source, "describe");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const oneLines = executeAndRecord(result.instrumentedSource, "describe", [1]);
    expect(oneLines).toEqual([2, 4]);

    const defaultLines = executeAndRecord(result.instrumentedSource, "describe", [99]);
    expect(defaultLines).toEqual([2, 8]);
  });

  it("instruments arrow function assigned to const", () => {
    const source = `const double = (x: number): number => {
  const result = x * 2;
  return result;
};`;
    const result = instrumentFunction(source, "double");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(RECORD_FUNCTION);
    const lines = executeAndRecord(result.instrumentedSource, "double", [5]);
    expect(lines).toEqual([2, 3]);
  });

  it("instruments nested if/else-if chains", () => {
    const source = `function grade(score: number): string {
  if (score >= 90) {
    return "A";
  } else if (score >= 80) {
    return "B";
  } else {
    return "C";
  }
}`;
    const result = instrumentFunction(source, "grade");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const aLines = executeAndRecord(result.instrumentedSource, "grade", [95]);
    expect(aLines).toEqual([2, 3]);

    const bLines = executeAndRecord(result.instrumentedSource, "grade", [85]);
    expect(bLines).toEqual([2, 4, 5]);

    const cLines = executeAndRecord(result.instrumentedSource, "grade", [50]);
    expect(cLines).toEqual([2, 4, 7]);
  });

  it("only instruments the target function, not others", () => {
    const source = `function helper(): number {
  return 42;
}

function target(x: number): number {
  return helper() + x;
}`;
    const result = instrumentFunction(source, "target");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    // helper should not contain record calls
    const helperMatch = result.instrumentedSource.match(
      /function helper\(\)[^{]*\{([^}]*)}/,
    );
    expect(helperMatch?.[1]).not.toContain(RECORD_FUNCTION);
  });

  it("provides the record function name in the result", () => {
    const source = `function f(): void {}`;
    const result = instrumentFunction(source, "f");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.recordFunctionName).toBe(RECORD_FUNCTION);
  });

  it("provides the branch function name in the result", () => {
    const source = `function f(): void {}`;
    const result = instrumentFunction(source, "f");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.branchFunctionName).toBe(BRANCH_FUNCTION);
  });

  it("reports the number of branch points instrumented", () => {
    const source = `function multi(x: number): string {
  if (x > 0) {
    if (x > 100) {
      return "large";
    }
    return "positive";
  }
  return "non-positive";
}`;
    const result = instrumentFunction(source, "multi");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.branchCount).toBe(2);
  });
});

describe("symbolic branch instrumentation", () => {
  it("wraps if condition with __shatter_branch call", () => {
    const source = `function check(x: number): boolean {
  if (x > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.instrumentedSource).toContain(BRANCH_FUNCTION);
    expect(result.branchCount).toBe(1);
  });

  it("captures branch decision for taken if-branch", () => {
    const source = `function check(x: number): boolean {
  if (x > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches, returnValue } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(returnValue).toBe(true);
    expect(branches).toHaveLength(1);
    expect(branches[0]!.branch_id).toBe(0);
    expect(branches[0]!.taken).toBe(true);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "x", path: [] },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("captures branch decision for not-taken if-branch", () => {
    const source = `function check(x: number): boolean {
  if (x > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches, returnValue } = executeAndCollect(result.instrumentedSource, "check", [5]);
    expect(returnValue).toBe(false);
    expect(branches).toHaveLength(1);
    expect(branches[0]!.taken).toBe(false);
  });

  it("captures multiple branch decisions in if/else-if chain", () => {
    const source = `function grade(score: number): string {
  if (score >= 90) {
    return "A";
  } else if (score >= 80) {
    return "B";
  } else {
    return "C";
  }
}`;
    const result = instrumentFunction(source, "grade");
    if ("error" in result) throw new Error(result.error);

    // Score 85: first branch not taken, second taken
    const { branches } = executeAndCollect(result.instrumentedSource, "grade", [85]);
    expect(branches).toHaveLength(2);
    expect(branches[0]!.taken).toBe(false);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "ge",
        left: { kind: "param", name: "score", path: [] },
        right: { kind: "const", type: "int", value: 90 },
      },
    });
    expect(branches[1]!.taken).toBe(true);
    expect(branches[1]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "ge",
        left: { kind: "param", name: "score", path: [] },
        right: { kind: "const", type: "int", value: 80 },
      },
    });
  });

  it("captures while loop branch decisions on each iteration", () => {
    const source = `function countdown(n: number): number {
  let count = 0;
  while (n > 0) {
    count++;
    n--;
  }
  return count;
}`;
    const result = instrumentFunction(source, "countdown");
    if ("error" in result) throw new Error(result.error);

    const { branches, returnValue } = executeAndCollect(result.instrumentedSource, "countdown", [2]);
    expect(returnValue).toBe(2);
    // while loop: 2 true iterations + 1 false exit
    expect(branches).toHaveLength(3);
    expect(branches[0]!.taken).toBe(true);
    expect(branches[1]!.taken).toBe(true);
    expect(branches[2]!.taken).toBe(false);
    // All branches have the same branch_id (same branch point)
    expect(branches[0]!.branch_id).toBe(branches[1]!.branch_id);
  });

  it("captures for loop branch decisions", () => {
    const source = `function sumTo(n: number): number {
  let total = 0;
  for (let i = 1; i <= n; i++) {
    total += i;
  }
  return total;
}`;
    const result = instrumentFunction(source, "sumTo");
    if ("error" in result) throw new Error(result.error);

    const { branches, returnValue } = executeAndCollect(result.instrumentedSource, "sumTo", [2]);
    expect(returnValue).toBe(3);
    // for loop with n=2: condition true twice (i=1, i=2), then false (i=3)
    expect(branches).toHaveLength(3);
    expect(branches[0]!.taken).toBe(true);
    expect(branches[1]!.taken).toBe(true);
    expect(branches[2]!.taken).toBe(false);
  });

  it("captures do-while branch decisions", () => {
    const source = `function atLeastOnce(n: number): number {
  let count = 0;
  do {
    count++;
    n--;
  } while (n > 0);
  return count;
}`;
    const result = instrumentFunction(source, "atLeastOnce");
    if ("error" in result) throw new Error(result.error);

    // n=0: loop body runs once, then condition is false
    const { branches, returnValue } = executeAndCollect(result.instrumentedSource, "atLeastOnce", [0]);
    expect(returnValue).toBe(1);
    expect(branches).toHaveLength(1);
    expect(branches[0]!.taken).toBe(false);
  });

  it("handles string comparison constraints", () => {
    const source = `function greet(name: string): string {
  if (name === "world") {
    return "Hello, World!";
  }
  return "Hello, " + name;
}`;
    const result = instrumentFunction(source, "greet");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "greet", ["world"]);
    expect(branches[0]!.taken).toBe(true);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: { kind: "param", name: "name", path: [] },
        right: { kind: "const", type: "str", value: "world" },
      },
    });
  });

  it("handles boolean literal constraints", () => {
    const source = `function check(flag: boolean): string {
  if (flag === true) {
    return "on";
  }
  return "off";
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [true]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: { kind: "param", name: "flag", path: [] },
        right: { kind: "const", type: "bool", value: true },
      },
    });
  });

  it("handles negation operator in constraints", () => {
    const source = `function check(flag: boolean): string {
  if (!flag) {
    return "off";
  }
  return "on";
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [false]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "un_op",
        op: "not",
        operand: { kind: "param", name: "flag", path: [] },
      },
    });
  });

  it("handles property access on parameters", () => {
    const source = `function check(config: { timeout: number }): boolean {
  if (config.timeout > 30) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ timeout: 60 }]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "config", path: ["timeout"] },
        right: { kind: "const", type: "int", value: 30 },
      },
    });
  });

  it("handles nested property access on parameters", () => {
    const source = `function check(config: { server: { port: number } }): boolean {
  if (config.server.port > 1024) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(
      result.instrumentedSource,
      "check",
      [{ server: { port: 8080 } }],
    );
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "config", path: ["server", "port"] },
        right: { kind: "const", type: "int", value: 1024 },
      },
    });
  });

  it("produces unknown for unsupported expressions", () => {
    const source = `function check(x: number): boolean {
  const y = x * 2;
  if (y > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    // y is a local variable, not a parameter — left side is unknown
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "unknown" },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("handles compound logical conditions", () => {
    const source = `function inRange(x: number): boolean {
  if (x > 0 && x < 100) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "inRange");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "inRange", [50]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "and",
        left: {
          kind: "bin_op",
          op: "gt",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 0 },
        },
        right: {
          kind: "bin_op",
          op: "lt",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 100 },
        },
      },
    });
  });

  it("handles arrow function with branches", () => {
    const source = `const isPositive = (x: number): boolean => {
  if (x > 0) {
    return true;
  }
  return false;
};`;
    const result = instrumentFunction(source, "isPositive");
    if ("error" in result) throw new Error(result.error);

    const { branches, returnValue } = executeAndCollect(
      result.instrumentedSource,
      "isPositive",
      [5],
    );
    expect(returnValue).toBe(true);
    expect(branches).toHaveLength(1);
    expect(branches[0]!.taken).toBe(true);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "x", path: [] },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("assigns incrementing branch IDs to distinct branch points", () => {
    const source = `function multi(x: number, y: number): string {
  if (x > 0) {
    if (y > 0) {
      return "both positive";
    }
    return "x positive";
  }
  return "neither";
}`;
    const result = instrumentFunction(source, "multi");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "multi", [5, 5]);
    expect(branches).toHaveLength(2);
    expect(branches[0]!.branch_id).toBe(0);
    expect(branches[1]!.branch_id).toBe(1);
  });
});

describe("buildSymExpr", () => {
  function parseExpr(code: string): ts.Expression {
    const sourceFile = ts.createSourceFile("test.ts", code, ts.ScriptTarget.Latest, true);
    const stmt = sourceFile.statements[0] as ts.ExpressionStatement;
    return stmt.expression;
  }

  it("builds param for known parameter identifiers", () => {
    const expr = parseExpr("x;");
    expect(buildSymExpr(expr, new Set(["x"]))).toEqual({
      kind: "param",
      name: "x",
      path: [],
    });
  });

  it("builds unknown for unknown identifiers", () => {
    const expr = parseExpr("y;");
    expect(buildSymExpr(expr, new Set(["x"]))).toEqual({ kind: "unknown" });
  });

  it("builds int const for integer literals", () => {
    const expr = parseExpr("42;");
    expect(buildSymExpr(expr, new Set())).toEqual({
      kind: "const",
      type: "int",
      value: 42,
    });
  });

  it("builds float const for decimal literals", () => {
    const expr = parseExpr("3.14;");
    expect(buildSymExpr(expr, new Set())).toEqual({
      kind: "const",
      type: "float",
      value: 3.14,
    });
  });

  it("builds str const for string literals", () => {
    const expr = parseExpr('"hello";');
    expect(buildSymExpr(expr, new Set())).toEqual({
      kind: "const",
      type: "str",
      value: "hello",
    });
  });

  it("builds bool const for true/false", () => {
    expect(buildSymExpr(parseExpr("true;"), new Set())).toEqual({
      kind: "const",
      type: "bool",
      value: true,
    });
    expect(buildSymExpr(parseExpr("false;"), new Set())).toEqual({
      kind: "const",
      type: "bool",
      value: false,
    });
  });

  it("builds null const", () => {
    expect(buildSymExpr(parseExpr("null;"), new Set())).toEqual({
      kind: "const",
      type: "null",
    });
  });

  it("builds bin_op for comparison operators", () => {
    const expr = parseExpr("x > 10;");
    expect(buildSymExpr(expr, new Set(["x"]))).toEqual({
      kind: "bin_op",
      op: "gt",
      left: { kind: "param", name: "x", path: [] },
      right: { kind: "const", type: "int", value: 10 },
    });
  });

  it("builds un_op for negation", () => {
    const expr = parseExpr("!flag;");
    expect(buildSymExpr(expr, new Set(["flag"]))).toEqual({
      kind: "un_op",
      op: "not",
      operand: { kind: "param", name: "flag", path: [] },
    });
  });

  it("builds param with path for property access", () => {
    const expr = parseExpr("obj.field;");
    expect(buildSymExpr(expr, new Set(["obj"]))).toEqual({
      kind: "param",
      name: "obj",
      path: ["field"],
    });
  });

  it("builds call for method calls", () => {
    const expr = parseExpr("arr.includes(x);");
    expect(buildSymExpr(expr, new Set(["arr", "x"]))).toEqual({
      kind: "call",
      name: "includes",
      receiver: { kind: "param", name: "arr", path: [] },
      args: [{ kind: "param", name: "x", path: [] }],
    });
  });

  it("builds call for function calls", () => {
    const expr = parseExpr("isValid(x);");
    expect(buildSymExpr(expr, new Set(["x"]))).toEqual({
      kind: "call",
      name: "isValid",
      receiver: null,
      args: [{ kind: "param", name: "x", path: [] }],
    });
  });

  it("handles parenthesized expressions", () => {
    const expr = parseExpr("(x > 10);");
    expect(buildSymExpr(expr, new Set(["x"]))).toEqual({
      kind: "bin_op",
      op: "gt",
      left: { kind: "param", name: "x", path: [] },
      right: { kind: "const", type: "int", value: 10 },
    });
  });

  it("maps all comparison operators correctly", () => {
    const ops: Array<[string, string]> = [
      ["x == 1", "eq"],
      ["x === 1", "eq"],
      ["x != 1", "ne"],
      ["x !== 1", "ne"],
      ["x < 1", "lt"],
      ["x <= 1", "le"],
      ["x > 1", "gt"],
      ["x >= 1", "ge"],
    ];
    for (const [code, expectedOp] of ops) {
      const expr = parseExpr(`${code};`);
      const result = buildSymExpr(expr, new Set(["x"]));
      expect(result).toHaveProperty("op", expectedOp);
    }
  });

  it("maps arithmetic operators correctly", () => {
    const ops: Array<[string, string]> = [
      ["x + 1", "add"],
      ["x - 1", "sub"],
      ["x * 2", "mul"],
      ["x / 2", "div"],
      ["x % 2", "mod"],
    ];
    for (const [code, expectedOp] of ops) {
      const expr = parseExpr(`${code};`);
      const result = buildSymExpr(expr, new Set(["x"]));
      expect(result).toHaveProperty("op", expectedOp);
    }
  });

  it("maps logical operators correctly", () => {
    const ops: Array<[string, string]> = [
      ["x && y", "and"],
      ["x || y", "or"],
    ];
    for (const [code, expectedOp] of ops) {
      const expr = parseExpr(`${code};`);
      const result = buildSymExpr(expr, new Set(["x", "y"]));
      expect(result).toHaveProperty("op", expectedOp);
    }
  });
});
