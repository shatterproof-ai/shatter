import ts from "typescript";
import { instrumentFunction, RECORD_FUNCTION } from "./instrumentor";

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
    `${jsSource}\nreturn ${functionName}(${args.map((a) => JSON.stringify(a)).join(", ")});`,
  );
  fn((line: number) => recorded.push(line));
  return recorded;
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
});
