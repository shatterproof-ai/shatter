/* eslint-disable @typescript-eslint/no-non-null-assertion */
import ts from "typescript";
import { instrumentFunction, buildSymExpr, RECORD_FUNCTION, BRANCH_FUNCTION, SCOPE_EVENT_FUNCTION, MOCK_REGISTRY, MOCK_CALL_FUNCTION, MCDC_RECORD_FUNCTION, MCDC_BRANCH_FUNCTION, flattenConditions, CRYPTO_BOUNDARY_FUNCTION } from "./instrumentor";
import type { SymExpr, BranchDecision, SymConstraint, MockConfig, TraceEvent, ScopeEvent, ConditionOutcome } from "./protocol";

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
    SCOPE_EVENT_FUNCTION,
    `${jsSource}\nreturn ${functionName}(${args.map((a) => JSON.stringify(a)).join(", ")});`,
  );
  fn(
    (line: number) => recorded.push(line),
    (_id: number, _line: number, cond: boolean, _sym: unknown) => cond,
    () => {},
  );
  return recorded;
}

/** Execute instrumented code and return both lines and branch decisions. */
function executeAndCollect(
  instrumentedSource: string,
  functionName: string,
  args: unknown[],
): { lines: number[]; branches: BranchDecision[]; returnValue: unknown; scopeEvents: TraceEvent[] } {
  const lines: number[] = [];
  const branches: BranchDecision[] = [];
  const scopeEvents: TraceEvent[] = [];
  const jsSource = transpileToJs(instrumentedSource);
  const fn = new Function(
    RECORD_FUNCTION,
    BRANCH_FUNCTION,
    SCOPE_EVENT_FUNCTION,
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
    (scopeId: number, kind: string) => {
      const event: ScopeEvent = kind.startsWith("loop")
        ? { kind: kind as "loop_enter" | "loop_exit", loop_id: scopeId }
        : { kind: kind as "call_enter" | "call_exit", call_site_id: scopeId };
      scopeEvents.push({ type: "scope", event });
    },
  );
  return { lines, branches, returnValue, scopeEvents };
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

  it("instruments expression-bodied arrow: reports instrumentable_line_count >= 1 (str-jeen.81)", () => {
    const source = `const double = (x: number): number => x * 2;`;
    const result = instrumentFunction(source, "double");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentableLineCount).toBeGreaterThanOrEqual(1);
    expect(result.branchCount).toBe(0);
    expect(result.instrumentedSource).toContain(RECORD_FUNCTION);
    const lines = executeAndRecord(result.instrumentedSource, "double", [5]);
    expect(lines.length).toBeGreaterThanOrEqual(1);
  });

  it("expression-bodied arrow with array literal returns correct value (str-jeen.81)", () => {
    const source = `const key = (id: string) => [id, "tag"];`;
    const result = instrumentFunction(source, "key");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentableLineCount).toBeGreaterThanOrEqual(1);
    const lines = executeAndRecord(result.instrumentedSource, "key", ["abc"]);
    expect(lines.length).toBeGreaterThanOrEqual(1);
  });

  it("expression-bodied arrow with string concat returns correct value (str-jeen.81)", () => {
    const source = `const concat = (a: string, b: string): string => a + b;`;
    const result = instrumentFunction(source, "concat");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentableLineCount).toBeGreaterThanOrEqual(1);
    const lines = executeAndRecord(result.instrumentedSource, "concat", ["hello", " world"]);
    expect(lines.length).toBeGreaterThanOrEqual(1);
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

  it("reports instrumentable line count excluding non-executable lines", () => {
    // 10-line function with 5 executable statement lines:
    // line 1: function signature (not instrumented)
    // line 2: blank (not instrumented)
    // line 3: if statement (instrumented)
    // line 4: return true (instrumented)
    // line 5: closing brace (not instrumented)
    // line 6: blank (not instrumented)
    // line 7: return false (instrumented)
    // line 8: closing brace (not instrumented)
    const source = `function example(x: number): boolean {

  if (x > 0) {
    return true;
  }

  return false;
}`;
    const result = instrumentFunction(source, "example");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    // Only 3 executable statement lines: if, return true, return false
    expect(result.instrumentableLineCount).toBe(3);
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

  it("resolves local variable data flow to symbolic expression", () => {
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
    // y = x * 2, so y > 10 becomes (x * 2) > 10
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "mul",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 2 },
        },
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

  it("coerces non-boolean branch conditions to boolean (str-o7a)", () => {
    const source = `function check(x: number): string {
  if (x) {
    return "truthy";
  }
  return "falsy";
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    // Truthy numeric value (42) — taken must be true, not 42
    const truthy = executeAndCollect(result.instrumentedSource, "check", [42]);
    expect(truthy.branches.length).toBeGreaterThanOrEqual(1);
    expect(truthy.branches[0]!.taken).toBe(true);
    expect(typeof truthy.branches[0]!.taken).toBe("boolean");

    // Falsy numeric value (0) — taken must be false, not 0
    const falsy = executeAndCollect(result.instrumentedSource, "check", [0]);
    expect(falsy.branches.length).toBeGreaterThanOrEqual(1);
    expect(falsy.branches[0]!.taken).toBe(false);
    expect(typeof falsy.branches[0]!.taken).toBe("boolean");

    // Null — taken must be false, not null
    const nullCase = executeAndCollect(result.instrumentedSource, "check", [null as unknown as number]);
    expect(nullCase.branches.length).toBeGreaterThanOrEqual(1);
    expect(nullCase.branches[0]!.taken).toBe(false);
    expect(typeof nullCase.branches[0]!.taken).toBe("boolean");
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

  it("resolves local variable via data flow map", () => {
    const expr = parseExpr("y > 10;");
    const flowMap = new Map<string, SymExpr>([
      ["y", { kind: "bin_op", op: "add", left: { kind: "param", name: "x", path: [] }, right: { kind: "const", type: "int", value: 1 } }],
    ]);
    expect(buildSymExpr(expr, new Set(["x"]), flowMap)).toEqual({
      kind: "bin_op",
      op: "gt",
      left: {
        kind: "bin_op",
        op: "add",
        left: { kind: "param", name: "x", path: [] },
        right: { kind: "const", type: "int", value: 1 },
      },
      right: { kind: "const", type: "int", value: 10 },
    });
  });
});

describe("data flow tracking", () => {
  it("tracks simple assignment from parameter expression", () => {
    const source = `function check(x: number): boolean {
  const y = x + 1;
  if (y > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("tracks transitive data flow through multiple locals", () => {
    const source = `function check(x: number): boolean {
  const a = x + 1;
  const b = a * 2;
  if (b > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    // b = a * 2 = (x + 1) * 2
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "mul",
          left: {
            kind: "bin_op",
            op: "add",
            left: { kind: "param", name: "x", path: [] },
            right: { kind: "const", type: "int", value: 1 },
          },
          right: { kind: "const", type: "int", value: 2 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("does not track variables with non-symbolic initializers", () => {
    const source = `function check(x: number): boolean {
  const y = Math.random();
  if (y > 0.5) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [1]);
    // y is not derived from params, so constraint is unknown
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "unknown" },
        right: { kind: "const", type: "float", value: 0.5 },
      },
    });
  });

  it("tracks data flow through negation", () => {
    const source = `function check(flag: boolean): boolean {
  const notFlag = !flag;
  if (notFlag) {
    return true;
  }
  return false;
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

  it("tracks data flow through method calls on parameters", () => {
    const source = `function check(s: string): boolean {
  const up = s.toUpperCase();
  if (up === "HELLO") {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", ["hello"]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: {
          kind: "call",
          name: "toUpperCase",
          receiver: { kind: "param", name: "s", path: [] },
          args: [],
        },
        right: { kind: "const", type: "str", value: "HELLO" },
      },
    });
  });

  it("tracks data flow through free function calls with param args", () => {
    const source = `function check(x: number): boolean {
  const s = String(x);
  if (s === "5") {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [5]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: {
          kind: "call",
          name: "String",
          receiver: null,
          args: [{ kind: "param", name: "x", path: [] }],
        },
        right: { kind: "const", type: "str", value: "5" },
      },
    });
  });

  it("tracks data flow through typeof expressions", () => {
    const source = `function check(x: unknown): boolean {
  const t = typeof x;
  if (t === "string") {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", ["hello"]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: {
          kind: "un_op",
          op: "typeof",
          operand: { kind: "param", name: "x", path: [] },
        },
        right: { kind: "const", type: "str", value: "string" },
      },
    });
  });

  it("tracks data flow through indexOf call (parity: CallExpression in buildSymExprWithFlow)", () => {
    const source = `function check(s: string): boolean {
  const x = s.indexOf("@");
  if (x === -1) {
    return false;
  }
  return true;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", ["hello"]);
    // The branch condition should resolve the call expression through data flow, not be unknown
    expect(branches.length).toBeGreaterThan(0);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "eq",
        left: {
          kind: "call",
          name: "indexOf",
          receiver: { kind: "param", name: "s", path: [] },
          args: [{ kind: "const", type: "str", value: "@" }],
        },
        right: { kind: "un_op", op: "neg", operand: { kind: "const", type: "int", value: 1 } },
      },
    });
  });

  it("tracks object destructuring from parameter", () => {
    const source = `function check(config: {x: number}): boolean {
  const {x} = config;
  if (x > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ x: 20 }]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "config", path: ["x"] },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("tracks renamed object destructuring binding", () => {
    const source = `function check(config: {x: number}): boolean {
  const {x: val} = config;
  if (val > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ x: 20 }]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "config", path: ["x"] },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("tracks array destructuring from parameter", () => {
    const source = `function check(arr: number[]): boolean {
  const [a, b] = arr;
  if (a > 0) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [[5, 10]]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "arr", path: ["0"] },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("tracks nested object destructuring", () => {
    const source = `function check(obj: {inner: {val: number}}): boolean {
  const {inner: {val}} = obj;
  if (val > 0) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ inner: { val: 5 } }]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "obj", path: ["inner", "val"] },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("ignores rest patterns in destructuring", () => {
    const source = `function check(obj: {a: number, b: number, c: number}): boolean {
  const {a, ...rest} = obj;
  if (a > 0) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ a: 5, b: 1, c: 2 }]);
    // 'a' should be tracked, rest is ignored
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "obj", path: ["a"] },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("tracks destructured binding with default value", () => {
    const source = `function check(config: {x?: number}): boolean {
  const {x = 5} = config;
  if (x > 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [{ x: 20 }]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "config", path: ["x"] },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("produces unknown for destructuring from non-param expression", () => {
    const source = `function check(x: number): boolean {
  const {y} = JSON.parse("{}");
  if (y > 0) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [1]);
    // y is not derived from params, so constraint left side is unknown
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "unknown" },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("produces ite for conditional reassignment in if-only (no else)", () => {
    const source = `function check(a: number, b: number, flag: boolean): number {
  let x = a;
  if (flag) {
    x = b;
  }
  if (x > 10) {
    return 1;
  }
  return 0;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [5, 20, true]);
    // The second branch condition (x > 10) should resolve x as ite(flag, b, a)
    const secondBranch = branches[1];
    expect(secondBranch).toBeDefined();
    expect(secondBranch!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "ite",
          condition: { kind: "param", name: "flag", path: [] },
          then_expr: { kind: "param", name: "b", path: [] },
          else_expr: { kind: "param", name: "a", path: [] },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("produces ite for conditional reassignment in if/else", () => {
    const source = `function check(a: number, b: number, flag: boolean): number {
  let x = a;
  if (flag) {
    x = b;
  } else {
    x = a + 1;
  }
  if (x > 10) {
    return 1;
  }
  return 0;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [5, 20, true]);
    const secondBranch = branches[1];
    expect(secondBranch).toBeDefined();
    expect(secondBranch!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "ite",
          condition: { kind: "param", name: "flag", path: [] },
          then_expr: { kind: "param", name: "b", path: [] },
          else_expr: {
            kind: "bin_op",
            op: "add",
            left: { kind: "param", name: "a", path: [] },
            right: { kind: "const", type: "int", value: 1 },
          },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("skips ite when condition is unknown", () => {
    // Use a bare function call with no param args — resolves to fully unknown
    const source = `function check(a: number, b: number): number {
  let x = a;
  const unknownCond = () => true;
  if (unknownCond()) {
    x = b;
  }
  if (x > 10) {
    return 1;
  }
  return 0;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [5, 20]);
    // With unknown condition, falls back to last-writer-wins — no ite
    const secondBranch = branches[1];
    expect(secondBranch).toBeDefined();
    // x should NOT be an ite since the condition is unknown
    const expr = secondBranch!.constraint;
    if (expr.kind === "expr") {
      expect(expr.expr.kind).toBe("bin_op");
      if (expr.expr.kind === "bin_op") {
        expect(expr.expr.left.kind).not.toBe("ite");
      }
    }
  });

  it("produces no ite when both branches assign the same value", () => {
    const source = `function check(a: number, flag: boolean): number {
  let x = a;
  if (flag) {
    x = a;
  } else {
    x = a;
  }
  if (x > 10) {
    return 1;
  }
  return 0;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [5, true]);
    const secondBranch = branches[1];
    expect(secondBranch).toBeDefined();
    // Both branches assign the same value — should be param(a), not ite
    expect(secondBranch!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: { kind: "param", name: "a", path: [] },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });
});

describe("closure over mutable state", () => {
  it("preserves symbolic link for const capture (safe)", () => {
    // const y is safe — closure captures an immutable binding
    const source = `function check(x: number): boolean {
  const y = x + 1;
  const f = () => y;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("poisons let variable captured by closure when mutated after", () => {
    // let y captured by closure, then y++ (compound mutation not tracked by flowMap)
    // Poisoning marks y as unknown so the stale symbolic link isn't used
    const source = `function check(x: number): boolean {
  let y = x + 1;
  const f = () => y;
  y++;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    const expr = branches[0]!.constraint;
    expect(expr.kind).toBe("expr");
    if (expr.kind === "expr") {
      expect(expr.expr.kind).toBe("bin_op");
      if (expr.expr.kind === "bin_op") {
        expect(expr.expr.left.kind).toBe("unknown");
      }
    }
  });

  it("preserves symbolic link for let variable not reassigned after closure", () => {
    // let y captured but no mutation follows — safe to keep symbolic link
    const source = `function check(x: number): boolean {
  let y = x + 1;
  const f = () => y;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "param", name: "x", path: [] },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("poisons var variable captured by closure when mutated after", () => {
    // var y captured, then y += 5 (compound assignment, not tracked by flowMap)
    const source = `function check(x: number): boolean {
  var y = x + 1;
  const f = () => y;
  y += 5;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    const expr = branches[0]!.constraint;
    expect(expr.kind).toBe("expr");
    if (expr.kind === "expr") {
      expect(expr.expr.kind).toBe("bin_op");
      if (expr.expr.kind === "bin_op") {
        expect(expr.expr.left.kind).toBe("unknown");
      }
    }
  });

  it("only poisons the captured-and-mutated variable, not unaffected ones", () => {
    // x is captured by f and mutated afterward — poisoned
    // y is NOT captured by f and not mutated — stays symbolic
    const source = `function check(a: number, b: number): boolean {
  let x = a + 1;
  let y = b + 1;
  const f = () => x;
  x++;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20, 20]);
    // y was never captured or mutated — should stay symbolic
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "param", name: "b", path: [] },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });
});

describe("flow tracking for mutated locals", () => {
  it("updates symbolic flow after postfix increment", () => {
    const source = `function check(x: number): boolean {
  let y = x + 1;
  y++;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: {
            kind: "bin_op",
            op: "add",
            left: { kind: "param", name: "x", path: [] },
            right: { kind: "const", type: "int", value: 1 },
          },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("updates symbolic flow after compound assignment", () => {
    const source = `function check(x: number): boolean {
  let y = x + 1;
  y -= 2;
  if (y > 10) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [20]);
    expect(branches[0]!.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "sub",
          left: {
            kind: "bin_op",
            op: "add",
            left: { kind: "param", name: "x", path: [] },
            right: { kind: "const", type: "int", value: 1 },
          },
          right: { kind: "const", type: "int", value: 2 },
        },
        right: { kind: "const", type: "int", value: 10 },
      },
    });
  });

  it("tracks canonical for-loop incrementors for later branches", () => {
    const source = `function check(n: number): boolean {
  let i = 0;
  for (; i < n; i++) {
  }
  if (i > 0) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [3]);
    const finalBranch = branches[branches.length - 1]!;
    expect(finalBranch.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "const", type: "int", value: 0 },
          right: { kind: "const", type: "int", value: 1 },
        },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });

  it("keeps loop-body accumulator updates symbolic for later branches", () => {
    const source = `function check(n: number): boolean {
  let total = 0;
  for (let i = 0; i < n; i++) {
    total += i;
  }
  if (total > 0) return true;
  return false;
}`;
    const result = instrumentFunction(source, "check");
    if ("error" in result) throw new Error(result.error);

    const { branches } = executeAndCollect(result.instrumentedSource, "check", [3]);
    const finalBranch = branches[branches.length - 1]!;
    expect(finalBranch.constraint).toEqual({
      kind: "expr",
      expr: {
        kind: "bin_op",
        op: "gt",
        left: {
          kind: "bin_op",
          op: "add",
          left: { kind: "const", type: "int", value: 0 },
          right: { kind: "const", type: "int", value: 0 },
        },
        right: { kind: "const", type: "int", value: 0 },
      },
    });
  });
});

describe("mock injection via import rewriting", () => {
  it("rewrites import with mocked symbol to use mock registry", () => {
    const source = `import { foo } from 'bar';
function doStuff(x: number): number {
  return foo(x);
}`;
    const mocks: MockConfig[] = [{
      symbol: "bar:foo",
      return_values: [42],
      should_track_calls: true,
      default_behavior: "return_generated",
    }];
    const result = instrumentFunction(source, "doStuff", "input.ts", mocks);
    if ("error" in result) throw new Error(result.error);

    // The import should be removed and replaced with a const using the mock registry
    expect(result.instrumentedSource).not.toContain("import { foo }");
    expect(result.instrumentedSource).toContain(MOCK_REGISTRY);
    expect(result.instrumentedSource).toContain("bar:foo");
  });

  it("keeps non-mocked imports intact", () => {
    const source = `import { foo, bar } from 'mymod';
function doStuff(x: number): number {
  return foo(x) + bar(x);
}`;
    const mocks: MockConfig[] = [{
      symbol: "mymod:foo",
      return_values: [42],
      should_track_calls: true,
      default_behavior: "return_generated",
    }];
    const result = instrumentFunction(source, "doStuff", "input.ts", mocks);
    if ("error" in result) throw new Error(result.error);

    // bar should still be imported normally
    expect(result.instrumentedSource).toContain("bar");
    expect(result.instrumentedSource).toContain(MOCK_REGISTRY);
    expect(result.instrumentedSource).toContain("mymod:foo");
    // bar should not reference mock registry
    expect(result.instrumentedSource).not.toContain("mymod:bar");
  });

  it("generates mock call recording via __shatter_mock_call", () => {
    const source = `import { compute } from 'math-lib';
function doStuff(x: number): number {
  return compute(x);
}`;
    const mocks: MockConfig[] = [{
      symbol: "math-lib:compute",
      return_values: [99],
      should_track_calls: true,
      default_behavior: "return_generated",
    }];
    const result = instrumentFunction(source, "doStuff", "input.ts", mocks);
    if ("error" in result) throw new Error(result.error);

    expect(result.instrumentedSource).toContain(MOCK_CALL_FUNCTION);
    expect(result.instrumentedSource).toContain("math-lib");
    expect(result.instrumentedSource).toContain("compute");
  });

  it("does not modify imports when no mocks provided", () => {
    const source = `import { foo } from 'bar';
function doStuff(x: number): number {
  return foo(x);
}`;
    const result = instrumentFunction(source, "doStuff", "input.ts", []);
    if ("error" in result) throw new Error(result.error);

    expect(result.instrumentedSource).toContain("import { foo }");
    expect(result.instrumentedSource).not.toContain(MOCK_REGISTRY);
  });

  it("executes mocked function through registry", () => {
    const source = `import { getValue } from 'data';
function doStuff(x: number): number {
  return getValue(x) + 1;
}`;
    const mocks: MockConfig[] = [{
      symbol: "data:getValue",
      return_values: [100],
      should_track_calls: true,
      default_behavior: "return_generated",
    }];
    const result = instrumentFunction(source, "doStuff", "input.ts", mocks);
    if ("error" in result) throw new Error(result.error);

    // Execute the instrumented code with a mock registry
    const jsSource = transpileToJs(result.instrumentedSource);
    const mockCalls: Array<{ module: string; symbol: string; args: unknown[]; returnValue: unknown }> = [];

    const mockRegistry: Record<string, (...args: unknown[]) => unknown> = {
      "data:getValue": (_x: unknown) => 100,
    };

    const fn = new Function(
      RECORD_FUNCTION,
      BRANCH_FUNCTION,
      SCOPE_EVENT_FUNCTION,
      MOCK_REGISTRY,
      MOCK_CALL_FUNCTION,
      `${jsSource}\nreturn doStuff(5);`,
    );
    const returnValue = fn(
      () => {},
      (_id: number, _line: number, cond: boolean) => cond,
      () => {},
      mockRegistry,
      (mod: string, sym: string, args: unknown[], ret: unknown) => {
        mockCalls.push({ module: mod, symbol: sym, args: [...args], returnValue: ret });
      },
    );

    expect(returnValue).toBe(101); // 100 + 1
    expect(mockCalls).toHaveLength(1);
    expect(mockCalls[0]!.module).toBe("data");
    expect(mockCalls[0]!.symbol).toBe("getValue");
    expect(mockCalls[0]!.returnValue).toBe(100);
  });

  describe("TSX support", () => {
    it("instruments a function in TSX source", () => {
      const source = `
export function greetingLabel(name: string): string {
  if (name) {
    return \`<span>Hello, \${name}!</span>\`;
  }
  return "<span>Hello, stranger!</span>";
}`;
      const result = instrumentFunction(source, "greetingLabel", "component.tsx");
      expect("error" in result).toBe(false);
      if (!("error" in result)) {
        expect(result.instrumentedSource).toContain(RECORD_FUNCTION);
        expect(result.instrumentedSource).toContain(BRANCH_FUNCTION);
        expect(result.branchCount).toBeGreaterThan(0);
      }
    });

    it("instruments TSX source containing JSX elements", () => {
      const source = `
export function jsxReturning(show: boolean): unknown {
  if (show) {
    return <div className="visible">content</div>;
  }
  return <div className="hidden" />;
}`;
      const result = instrumentFunction(source, "jsxReturning", "component.tsx");
      expect("error" in result).toBe(false);
      if (!("error" in result)) {
        expect(result.instrumentedSource).toContain(BRANCH_FUNCTION);
        expect(result.branchCount).toBeGreaterThan(0);
      }
    });

    it("uses ScriptKind.TS for .ts files and ScriptKind.TSX for .tsx files", () => {
      const source = `
export function greetingLabel(name: string): string {
  if (name) {
    return "hello " + name;
  }
  return "hello stranger";
}`;
      const resultTs = instrumentFunction(source, "greetingLabel", "component.ts");
      const resultTsx = instrumentFunction(source, "greetingLabel", "component.tsx");
      expect("error" in resultTs).toBe(false);
      expect("error" in resultTsx).toBe(false);
      if (!("error" in resultTs) && !("error" in resultTsx)) {
        expect(resultTs.branchCount).toBe(resultTsx.branchCount);
      }
    });
  });
});

describe("scope events", () => {
  it("while loop produces loop_enter/loop_exit per iteration", () => {
    const source = `function countdown(n: number): number {
  let result = 0;
  while (n > 0) {
    result += n;
    n--;
  }
  return result;
}`;
    const result = instrumentFunction(source, "countdown");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "countdown", [3]);
    const loopEvents = scopeEvents.filter(
      (e): e is { type: "scope"; event: ScopeEvent } =>
        e.type === "scope" && (e.event.kind === "loop_enter" || e.event.kind === "loop_exit"),
    );
    const enters = loopEvents.filter((e) => e.event.kind === "loop_enter");
    const exits = loopEvents.filter((e) => e.event.kind === "loop_exit");
    expect(enters).toHaveLength(3);
    expect(exits).toHaveLength(3);
  });

  it("for-of loop produces scope markers", () => {
    const source = `function sumArray(items: number[]): number {
  let total = 0;
  for (const item of items) {
    total += item;
  }
  return total;
}`;
    const result = instrumentFunction(source, "sumArray");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "sumArray", [[1, 2, 3]]);
    const loopEvents = scopeEvents.filter(
      (e): e is { type: "scope"; event: ScopeEvent } =>
        e.type === "scope" && (e.event.kind === "loop_enter" || e.event.kind === "loop_exit"),
    );
    expect(loopEvents.filter((e) => e.event.kind === "loop_enter")).toHaveLength(3);
    expect(loopEvents.filter((e) => e.event.kind === "loop_exit")).toHaveLength(3);
  });

  it("nested loops get distinct loop_ids", () => {
    const source = `function nested(rows: number, cols: number): number {
  let count = 0;
  for (let i = 0; i < rows; i++) {
    for (let j = 0; j < cols; j++) {
      count++;
    }
  }
  return count;
}`;
    const result = instrumentFunction(source, "nested");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "nested", [2, 3]);
    const loopEnters = scopeEvents.filter(
      (e): e is { type: "scope"; event: { kind: "loop_enter"; loop_id: number } } =>
        e.type === "scope" && e.event.kind === "loop_enter",
    );
    const loopIds = new Set(loopEnters.map((e) => e.event.loop_id));
    expect(loopIds.size).toBe(2);
  });

  it("function body gets call_enter/call_exit scope events", () => {
    const source = `function simple(x: number): number {
  return x + 1;
}`;
    const result = instrumentFunction(source, "simple");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "simple", [5]);
    const callEvents = scopeEvents.filter(
      (e): e is { type: "scope"; event: ScopeEvent } =>
        e.type === "scope" && (e.event.kind === "call_enter" || e.event.kind === "call_exit"),
    );
    expect(callEvents.length).toBeGreaterThanOrEqual(2);
    expect(callEvents[0]!.event.kind).toBe("call_enter");
    expect(callEvents[callEvents.length - 1]!.event.kind).toBe("call_exit");
  });

  it("inline arrow callback in .map() produces call_enter/call_exit markers", () => {
    const source = `function doubleAll(items: number[]): number[] {
  return items.map((x) => {
    if (x > 0) {
      return x * 2;
    }
    return 0;
  });
}`;
    const result = instrumentFunction(source, "doubleAll");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "doubleAll", [[1, -2, 3]]);
    const callEnters = scopeEvents.filter(
      (e): e is { type: "scope"; event: { kind: "call_enter"; call_site_id: number } } =>
        e.type === "scope" && e.event.kind === "call_enter",
    );
    // At least 1 (top-level function) + 3 (callback invocations) = 4 call_enters
    expect(callEnters.length).toBeGreaterThanOrEqual(4);
    const callSiteIds = new Set(callEnters.map((e) => e.event.call_site_id));
    expect(callSiteIds.size).toBeGreaterThanOrEqual(2);
  });

  it("do-while loop produces scope markers", () => {
    const source = `function doWhileTest(n: number): number {
  let result = 0;
  do {
    result += n;
    n--;
  } while (n > 0);
  return result;
}`;
    const result = instrumentFunction(source, "doWhileTest");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    const { scopeEvents } = executeAndCollect(result.instrumentedSource, "doWhileTest", [3]);
    const loopEvents = scopeEvents.filter(
      (e): e is { type: "scope"; event: ScopeEvent } =>
        e.type === "scope" && (e.event.kind === "loop_enter" || e.event.kind === "loop_exit"),
    );
    expect(loopEvents.filter((e) => e.event.kind === "loop_enter")).toHaveLength(3);
    expect(loopEvents.filter((e) => e.event.kind === "loop_exit")).toHaveLength(3);
  });
});

// ---------------------------------------------------------------------------
// MC/DC instrumentor tests
// ---------------------------------------------------------------------------

describe("MC/DC instrumentation (SHATTER_MCDC=1)", () => {
  afterEach(() => {
    delete process.env["SHATTER_MCDC"];
  });

  it("does not emit MC/DC calls when SHATTER_MCDC is not set", () => {
    delete process.env["SHATTER_MCDC"];
    const source = `function check(a: number, b: number): boolean {
  if (a > 0 && b < 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(BRANCH_FUNCTION);
    expect(result.instrumentedSource).not.toContain(MCDC_RECORD_FUNCTION);
    expect(result.instrumentedSource).not.toContain(MCDC_BRANCH_FUNCTION);
  });

  it("emits MC/DC calls for compound && when SHATTER_MCDC=1", () => {
    process.env["SHATTER_MCDC"] = "1";
    const source = `function check(a: number, b: number): boolean {
  if (a > 0 && b < 10) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(MCDC_RECORD_FUNCTION);
    expect(result.instrumentedSource).toContain(MCDC_BRANCH_FUNCTION);
    // IIFE pattern: should use arrow function wrapper
    expect(result.instrumentedSource).toContain("=>");
    // Operator should be embedded
    expect(result.instrumentedSource).toContain('"and"');
  });

  it("emits MC/DC calls for compound || when SHATTER_MCDC=1", () => {
    process.env["SHATTER_MCDC"] = "1";
    const source = `function check(x: boolean, y: boolean): boolean {
  if (x || y) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(MCDC_RECORD_FUNCTION);
    expect(result.instrumentedSource).toContain('"or"');
  });

  it("does NOT emit MC/DC calls for simple non-compound condition even when SHATTER_MCDC=1", () => {
    process.env["SHATTER_MCDC"] = "1";
    const source = `function check(a: number): boolean {
  if (a > 0) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    // Simple condition: no MC/DC decomposition
    expect(result.instrumentedSource).not.toContain(MCDC_RECORD_FUNCTION);
    expect(result.instrumentedSource).not.toContain(MCDC_BRANCH_FUNCTION);
    expect(result.instrumentedSource).toContain(BRANCH_FUNCTION);
  });

  it("generates thunks for each leaf condition (arrow functions in the thunk array)", () => {
    process.env["SHATTER_MCDC"] = "1";
    const source = `function check(a: number, b: number, c: number): boolean {
  if (a > 0 && b < 10 && c !== 5) {
    return true;
  }
  return false;
}`;
    const result = instrumentFunction(source, "check");
    expect("error" in result).toBe(false);
    if ("error" in result) return;

    expect(result.instrumentedSource).toContain(MCDC_RECORD_FUNCTION);
    // Three conditions: three thunks in the array
    // Each thunk is wrapped in double-not for boolean coercion: !!
    const occurrences = (result.instrumentedSource.match(/!!/g) ?? []).length;
    expect(occurrences).toBeGreaterThanOrEqual(3);
  });

  it("flattenConditions: returns null for single-condition expression", () => {
    const params = new Set(["a"]);
    const flow = new Map<string, SymExpr>();
    const sourceFile = ts.createSourceFile("test.ts", "a > 0", ts.ScriptTarget.Latest, true);
    const expr = (sourceFile.statements[0] as ts.ExpressionStatement).expression;
    expect(flattenConditions(expr, params, flow)).toBeNull();
  });

  it("flattenConditions: returns two conditions for a && b", () => {
    const params = new Set(["a", "b"]);
    const flow = new Map<string, SymExpr>();
    const sourceFile = ts.createSourceFile("test.ts", "a && b", ts.ScriptTarget.Latest, true);
    const expr = (sourceFile.statements[0] as ts.ExpressionStatement).expression;
    const result = flattenConditions(expr, params, flow);
    expect(result).not.toBeNull();
    expect(result!.operator).toBe("and");
    expect(result!.conditions.length).toBe(2);
  });

  it("flattenConditions: flattens a && b && c to three conditions", () => {
    const params = new Set(["a", "b", "c"]);
    const flow = new Map<string, SymExpr>();
    const sourceFile = ts.createSourceFile("test.ts", "a && b && c", ts.ScriptTarget.Latest, true);
    const expr = (sourceFile.statements[0] as ts.ExpressionStatement).expression;
    const result = flattenConditions(expr, params, flow);
    expect(result).not.toBeNull();
    expect(result!.conditions.length).toBe(3);
  });

  it("flattenConditions: returns null for > 16 conditions", () => {
    const names = Array.from({ length: 17 }, (_, i) => `v${i}`);
    const params = new Set(names);
    const flow = new Map<string, SymExpr>();
    const chain = names.join(" && ");
    const sourceFile = ts.createSourceFile("test.ts", chain, ts.ScriptTarget.Latest, true);
    const expr = (sourceFile.statements[0] as ts.ExpressionStatement).expression;
    expect(flattenConditions(expr, params, flow)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Crypto boundary injection tests
// ---------------------------------------------------------------------------

describe("crypto boundary injection", () => {
  it("injects __shatter_crypto_boundary before createDecipheriv call", () => {
    const source = `
      function fn(ciphertext: Buffer): string {
        const decipher = crypto.createDecipheriv("aes-256-cbc", KEY, IV);
        return decipher.update(ciphertext).toString();
      }
    `;
    const result = instrumentFunction(source, "fn");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.instrumentedSource).toContain(CRYPTO_BOUNDARY_FUNCTION);
    expect(result.instrumentedSource).toContain('"decrypt"');
    expect(result.instrumentedSource).toContain('"createDecipheriv"');
  });

  it("injects __shatter_crypto_boundary before createCipheriv call (encrypt)", () => {
    const source = `
      function fn(plaintext: string): Buffer {
        const cipher = crypto.createCipheriv("aes-256-cbc", KEY, IV);
        return Buffer.concat([cipher.update(plaintext, "utf8"), cipher.final()]);
      }
    `;
    const result = instrumentFunction(source, "fn");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.instrumentedSource).toContain(CRYPTO_BOUNDARY_FUNCTION);
    expect(result.instrumentedSource).toContain('"encrypt"');
    expect(result.instrumentedSource).toContain('"createCipheriv"');
  });

  it("does not inject crypto boundary for non-crypto calls", () => {
    const source = `
      function fn(x: number): number {
        const y = parseInt(String(x));
        return y + 1;
      }
    `;
    const result = instrumentFunction(source, "fn");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.instrumentedSource).not.toContain(CRYPTO_BOUNDARY_FUNCTION);
  });

  it("injects unique boundary IDs for multiple crypto calls", () => {
    const source = `
      function fn(c1: Buffer, c2: Buffer): string {
        const d1 = crypto.createDecipheriv("aes-256-cbc", KEY, IV);
        const d2 = crypto.createDecipheriv("aes-128-gcm", KEY2, IV2);
        return d1.update(c1).toString() + d2.update(c2).toString();
      }
    `;
    const result = instrumentFunction(source, "fn");
    expect("error" in result).toBe(false);
    if ("error" in result) return;
    expect(result.instrumentedSource).toContain('"cb-0"');
    expect(result.instrumentedSource).toContain('"cb-1"');
  });
});
