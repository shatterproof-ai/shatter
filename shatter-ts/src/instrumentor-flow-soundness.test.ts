/**
 * Soundness of branch constraints recorded by the instrumentor (str-49drv.130).
 *
 * The constraint attached to each recorded branch must be true of the inputs
 * that actually ran: evaluating it on the concrete inputs has to reproduce the
 * `taken` bit. A constraint built from a stale data-flow binding (wrong program
 * point, unkilled reassignment, loop-carried value, short-circuited parameter
 * lookup) breaks that and makes the solver negate an unrelated predicate.
 */
import ts from "typescript";
import fc from "fast-check";
import { instrumentFunction, buildSymExpr, BRANCH_FUNCTION, RECORD_FUNCTION, SCOPE_EVENT_FUNCTION } from "./instrumentor";
import type { BranchDecision, SymExpr } from "./protocol";

interface CorpusFunction {
  name: string;
  params: readonly string[];
  source: string;
  /** Build the call arguments from generated integers (default: the integers themselves). */
  toArgs?: (ints: readonly number[]) => unknown[];
}

const objectArg = (ints: readonly number[]): unknown[] => [{ x: ints[0] }, ints[1]];

const OPAQUE_HELPER = `function opaque(): number { return 42; }`;

const CORPUS: readonly CorpusFunction[] = [
  {
    name: "stale",
    params: ["a", "b"],
    source: `function stale(a: number, b: number): number {
  let x = a;
  if (x > 0) { x = x + 1; }
  x = b;
  if (x > 100) { return 1; }
  return 0;
}`,
  },
  {
    name: "staleUnknown",
    params: ["a"],
    source: `function staleUnknown(a: number): number {
  let x = a;
  x = opaque();
  if (x > 10) { return 1; }
  return 0;
}`,
  },
  {
    name: "loopCarried",
    params: ["a"],
    source: `function loopCarried(a: number): number {
  let acc = a;
  let i = 0;
  while (i < 3) {
    acc = acc + 1;
    i++;
  }
  if (acc > 10) { return 1; }
  return 0;
}`,
  },
  {
    name: "forIncr",
    params: ["a", "n"],
    source: `function forIncr(a: number, n: number): number {
  for (let i = a; i < n; i++) {
    if (i > 5) { return 1; }
  }
  return 0;
}`,
  },
  {
    name: "condMut",
    params: ["a"],
    source: `function condMut(a: number): number {
  let x = a;
  while (x-- > 0) {
    if (x > 3) { return 1; }
  }
  if (x > 3) { return 2; }
  return 0;
}`,
  },
  {
    name: "paramReassign",
    params: ["a", "b"],
    source: `function paramReassign(a: number, b: number): number {
  a = b;
  if (a > 0) { return 1; }
  return 0;
}`,
  },
  {
    name: "paramOpaque",
    params: ["a"],
    source: `function paramOpaque(a: number): number {
  a = opaque();
  if (a > 0) { return 1; }
  return 0;
}`,
  },
  {
    name: "iteMerge",
    params: ["a", "b"],
    source: `function iteMerge(a: number, b: number): number {
  let x = a;
  if (b > 0) {
    x = b;
  } else {
    x = a - 1;
  }
  if (x > 5) { return 1; }
  return 0;
}`,
  },
  {
    name: "iteOneSidedOpaque",
    params: ["a", "b"],
    source: `function iteOneSidedOpaque(a: number, b: number): number {
  let x = a;
  if (b > 0) {
    x = opaque();
  }
  if (x > 5) { return 1; }
  return 0;
}`,
  },
  {
    name: "blockShadow",
    params: ["a", "b"],
    source: `function blockShadow(a: number, b: number): number {
  let x = a;
  {
    let x = b;
    if (x > 0) { return 3; }
  }
  if (x > 0) { return 1; }
  return 0;
}`,
  },
  {
    name: "nestedMutation",
    params: ["a", "b"],
    source: `function nestedMutation(a: number, b: number): number {
  let x = a;
  let y = (x = b) + 1;
  if (x > 0) { return y; }
  return 0;
}`,
  },
  {
    name: "switchFallthrough",
    params: ["a", "b"],
    source: `function switchFallthrough(a: number, b: number): number {
  let x = a;
  switch (b) {
    case 1:
      x = b;
    case 2:
      if (x > 0) { return 1; }
      break;
    default:
      x = 0;
  }
  if (x > 3) { return 2; }
  return 0;
}`,
  },
  {
    name: "tryCatch",
    params: ["a", "b"],
    source: `function tryCatch(a: number, b: number): number {
  let x = a;
  try {
    x = b;
    if (b > 5) { throw new Error("big"); }
    x = 0;
  } catch {
    if (x > 3) { return 1; }
  }
  if (x > 1) { return 2; }
  return 0;
}`,
  },
  {
    name: "forOfLoop",
    params: ["a"],
    source: `function forOfLoop(a: number): number {
  let x = a;
  for (const v of [1, 2]) {
    if (x > v) { x = v; }
  }
  if (x > 1) { return 1; }
  return 0;
}`,
  },
  {
    name: "doWhile",
    params: ["a"],
    source: `function doWhile(a: number): number {
  let x = a;
  do {
    if (x > 2) { x = x - 2; }
    x--;
  } while (x > 0);
  if (x < -5) { return 1; }
  return 0;
}`,
  },
  {
    name: "propertyWrite",
    params: ["p", "b"],
    toArgs: objectArg,
    source: `function propertyWrite(p: { x: number }, b: number): number {
  p.x = b;
  if (p.x > 3) { return 1; }
  return 0;
}`,
  },
  {
    name: "aliasWrite",
    params: ["p", "b"],
    toArgs: objectArg,
    source: `function aliasWrite(p: { x: number }, b: number): number {
  const o = p;
  o.x = b;
  if (p.x > 3) { return 1; }
  return 0;
}`,
  },
  {
    name: "elementCompoundWrite",
    params: ["p", "b"],
    toArgs: objectArg,
    source: `function elementCompoundWrite(p: { x: number }, b: number): number {
  p["x"] = b;
  p.x += 1;
  p.x++;
  if (p.x > 3) { return 1; }
  return 0;
}`,
  },
  {
    name: "closurePropertyWrite",
    params: ["p", "b"],
    toArgs: objectArg,
    source: `function closurePropertyWrite(p: { x: number }, b: number): number {
  const o = p;
  [b].forEach((v) => { o.x = v; });
  if (p.x > 3) { return 1; }
  return 0;
}`,
  },
  {
    name: "conditionAssign",
    params: ["a", "b"],
    source: `function conditionAssign(a: number, b: number): number {
  let y = b;
  if ((y = 1) && a > y) { return 1; }
  return 0;
}`,
  },
  {
    name: "conditionAssignOpaque",
    params: ["a"],
    source: `function conditionAssignOpaque(a: number): number {
  let n = a;
  if ((n = opaque()) && n > 3) { return 1; }
  return 0;
}`,
  },
  {
    name: "closureMutation",
    params: ["a"],
    source: `function closureMutation(a: number): number {
  let x = a;
  const reset = () => { x = 0; };
  reset();
  if (x > 0) { return 1; }
  return 0;
}`,
  },
];

function instrumentCorpus(fn: CorpusFunction): string {
  const result = instrumentFunction(`${OPAQUE_HELPER}\n${fn.source}`, fn.name);
  if ("error" in result) throw new Error(result.error);
  return ts.transpileModule(result.instrumentedSource, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
  }).outputText;
}

function runAndCollectBranches(js: string, fn: CorpusFunction, args: readonly unknown[]): BranchDecision[] {
  const branches: BranchDecision[] = [];
  const run = new Function(
    RECORD_FUNCTION,
    BRANCH_FUNCTION,
    SCOPE_EVENT_FUNCTION,
    `${js}\nreturn ${fn.name}(${args.map((a) => JSON.stringify(a)).join(", ")});`,
  );
  run(
    () => {},
    (branchId: number, line: number, cond: boolean, symExpr: SymExpr) => {
      branches.push({
        branch_id: branchId,
        line,
        taken: cond,
        constraint: symExpr.kind === "unknown"
          ? { kind: "unknown", hint: "unsupported expression" }
          : { kind: "expr", expr: symExpr },
      });
      return cond;
    },
    () => {},
  );
  return branches;
}

/** Marker for sub-expressions the oracle cannot evaluate concretely. */
const UNEVALUABLE = Symbol("unevaluable");
type Concrete = unknown;

/**
 * Evaluate a SymExpr with JS semantics against concrete parameter values.
 * Returns UNEVALUABLE for unknown leaves and node kinds with no concrete model.
 */
function evaluate(expr: SymExpr, env: ReadonlyMap<string, unknown>): Concrete | typeof UNEVALUABLE {
  switch (expr.kind) {
    case "param": {
      if (!env.has(expr.name)) return UNEVALUABLE;
      let value: unknown = env.get(expr.name);
      for (const segment of expr.path) {
        if (value === null || typeof value !== "object") return UNEVALUABLE;
        value = (value as Record<string, unknown>)[segment];
      }
      return value;
    }
    case "const":
      return "value" in expr ? expr.value : null;
    case "un_op": {
      const operand = evaluate(expr.operand, env);
      if (operand === UNEVALUABLE) return UNEVALUABLE;
      switch (expr.op) {
        case "not": return !operand;
        case "neg": return -(operand as number);
        case "bitwise_not": return ~(operand as number);
        case "typeof": return typeof operand;
        default: return UNEVALUABLE;
      }
    }
    case "bin_op": {
      const left = evaluate(expr.left, env);
      const right = evaluate(expr.right, env);
      if (left === UNEVALUABLE || right === UNEVALUABLE) return UNEVALUABLE;
      const l = left as number;
      const r = right as number;
      switch (expr.op) {
        case "eq": return l === r;
        case "ne": return l !== r;
        case "lt": return l < r;
        case "le": return l <= r;
        case "gt": return l > r;
        case "ge": return l >= r;
        case "add": return l + r;
        case "sub": return l - r;
        case "mul": return l * r;
        case "div": return l / r;
        case "mod": return l % r;
        case "and": return l && r;
        case "or": return l || r;
        case "bitwise_and": return l & r;
        case "bitwise_or": return l | r;
        case "bitwise_xor": return l ^ r;
        default: return UNEVALUABLE;
      }
    }
    case "ite": {
      const condition = evaluate(expr.condition, env);
      if (condition === UNEVALUABLE) return UNEVALUABLE;
      return evaluate(condition ? expr.then_expr : expr.else_expr, env);
    }
    default:
      return UNEVALUABLE;
  }
}

function containsUnknown(expr: SymExpr): boolean {
  return JSON.stringify(expr).includes(`"kind":"unknown"`);
}

/** Resolve the recorded constraint of a branch, or undefined when it is (partly) unknown. */
function knownConstraint(branch: BranchDecision): SymExpr | undefined {
  if (branch.constraint.kind !== "expr") return undefined;
  return containsUnknown(branch.constraint.expr) ? undefined : branch.constraint.expr;
}

function firstBranch(fnName: string, branchId: number, args: readonly number[]): BranchDecision {
  const fn = CORPUS.find((c) => c.name === fnName)!;
  const branches = runAndCollectBranches(instrumentCorpus(fn), fn, args);
  const branch = branches.find((b) => b.branch_id === branchId);
  if (!branch) throw new Error(`branch ${branchId} of ${fnName} not recorded for ${JSON.stringify(args)}`);
  return branch;
}

const PARAM_A = { kind: "param", name: "a", path: [] } as const;
const PARAM_B = { kind: "param", name: "b", path: [] } as const;

describe("flow-map-program-point", () => {
  it("stale: x > 0 uses the binding at that branch (param a), not the later x = b", () => {
    const branch = firstBranch("stale", 0, [5, 0]);
    expect(knownConstraint(branch)).toEqual({
      kind: "bin_op", op: "gt", left: PARAM_A, right: { kind: "const", type: "int", value: 0 },
    });
  });

  it("stale: x > 100 after x = b uses param b", () => {
    const branch = firstBranch("stale", 1, [5, 0]);
    expect(knownConstraint(branch)).toEqual({
      kind: "bin_op", op: "gt", left: PARAM_B, right: { kind: "const", type: "int", value: 100 },
    });
  });

  it("staleUnknown: reassignment from an opaque call kills the old binding", () => {
    expect(knownConstraint(firstBranch("staleUnknown", 0, [5]))).toBeUndefined();
  });

  it("loopCarried: loop condition over a loop-mutated counter is unknown", () => {
    expect(knownConstraint(firstBranch("loopCarried", 0, [5]))).toBeUndefined();
  });

  it("loopCarried: branch after the loop over a loop-mutated accumulator is unknown", () => {
    const fn = CORPUS.find((c) => c.name === "loopCarried")!;
    const branches = runAndCollectBranches(instrumentCorpus(fn), fn, [5]);
    const after = branches.find((b) => b.branch_id === 1)!;
    expect(knownConstraint(after)).toBeUndefined();
  });

  it("forIncr: i > 5 over a for-incremented induction variable is unknown", () => {
    expect(knownConstraint(firstBranch("forIncr", 1, [0, 10]))).toBeUndefined();
  });

  it("condMut: x > 3 over a variable mutated in the loop condition is unknown", () => {
    expect(knownConstraint(firstBranch("condMut", 1, [9]))).toBeUndefined();
    const fn = CORPUS.find((c) => c.name === "condMut")!;
    const branches = runAndCollectBranches(instrumentCorpus(fn), fn, [0]);
    const after = branches.find((b) => b.branch_id === 2)!;
    expect(knownConstraint(after)).toBeUndefined();
  });

  it("paramReassign: a reassigned parameter resolves through the flow map (param b)", () => {
    expect(knownConstraint(firstBranch("paramReassign", 0, [5, -1]))).toEqual({
      kind: "bin_op", op: "gt", left: PARAM_B, right: { kind: "const", type: "int", value: 0 },
    });
  });

  it("paramOpaque: a parameter reassigned from an opaque call is unknown", () => {
    expect(knownConstraint(firstBranch("paramOpaque", 0, [5]))).toBeUndefined();
  });

  it("oracle: every known recorded constraint evaluates to the taken bit on the concrete inputs", () => {
    const compiled = CORPUS.map((fn) => ({ fn, js: instrumentCorpus(fn) }));
    fc.assert(
      fc.property(
        fc.integer({ min: 0, max: compiled.length - 1 }),
        fc.array(fc.integer({ min: -20, max: 20 }), { minLength: 2, maxLength: 2 }),
        (index, rawArgs) => {
          const { fn, js } = compiled[index]!;
          const args = (fn.toArgs ?? ((ints) => [...ints]))(rawArgs).slice(0, fn.params.length);
          const env = new Map(fn.params.map((name, i) => [name, args[i]]));
          for (const branch of runAndCollectBranches(js, fn, args)) {
            const constraint = knownConstraint(branch);
            if (constraint === undefined) continue;
            const value = evaluate(constraint, env);
            if (value === UNEVALUABLE) continue;
            if (Boolean(value) !== branch.taken) {
              throw new Error(
                `${fn.name}(${JSON.stringify(args)}): branch ${branch.branch_id} taken=${branch.taken} ` +
                `but constraint evaluates to ${String(value)}: ${JSON.stringify(constraint)}`,
              );
            }
          }
        },
      ),
      { numRuns: 500 },
    );
  });

  it("buildSymExpr resolves a flow-bound name (and property chains on it) before the parameter set", () => {
    const identifier = fc.constantFrom("a", "b", "value", "count");
    const segment = fc.constantFrom("x", "y", "len");
    fc.assert(
      fc.property(identifier, identifier, fc.array(segment, { maxLength: 3 }), segment, (name, target, basePath, prop) => {
        const bound: SymExpr = { kind: "param", name: target, path: basePath };
        const flowMap = new Map<string, SymExpr>([[name, bound]]);
        const paramNames = new Set([name, target]);
        const parse = (text: string): ts.Expression => {
          const file = ts.createSourceFile("p.ts", `${text};`, ts.ScriptTarget.Latest, true);
          return (file.statements[0] as ts.ExpressionStatement).expression;
        };
        expect(buildSymExpr(parse(name), paramNames, flowMap)).toEqual(bound);
        expect(buildSymExpr(parse(`${name}.${prop}`), paramNames, flowMap)).toEqual({
          kind: "param", name: target, path: [...basePath, prop],
        });
        const killed = new Map<string, SymExpr>([[name, { kind: "unknown" }]]);
        expect(buildSymExpr(parse(name), paramNames, killed)).toEqual({ kind: "unknown" });
        expect(buildSymExpr(parse(`${name}.${prop}`), paramNames, killed)).toEqual({ kind: "unknown" });
      }),
    );
  });

  it("property, element and alias writes kill the written parameter", () => {
    for (const name of ["propertyWrite", "aliasWrite", "elementCompoundWrite", "closurePropertyWrite"]) {
      const fn = CORPUS.find((c) => c.name === name)!;
      const branches = runAndCollectBranches(instrumentCorpus(fn), fn, objectArg([10, 0]));
      expect([name, knownConstraint(branches[0]!)]).toEqual([name, undefined]);
    }
  });

  it("an if-condition's own assignments are applied before its constraint is recorded", () => {
    // The whole constraint is partly unknown (the `=` node), but its known
    // sub-terms must not read the binding the condition overwrote.
    const cases = [
      ["conditionAssign", [5, 9], "b"],
      ["conditionAssignOpaque", [0], "a"],
    ] as const;
    for (const [name, args, staleParam] of cases) {
      const fn = CORPUS.find((c) => c.name === name)!;
      const branches = runAndCollectBranches(instrumentCorpus(fn), fn, args);
      const recorded = JSON.stringify(branches[0]!.constraint);
      expect([name, recorded.includes(`"name":"${staleParam}"`)]).toEqual([name, false]);
    }
  });
});
