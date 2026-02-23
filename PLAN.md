# Shatter v2: Automatic Exploratory Testing via Concolic Execution

## The Core Problem with v1

The current system uses **coverage-guided random fuzzing**: generate random typed values, run the function, observe which lines executed, then try to hybridize/breed inputs to find new paths. This fails for non-trivial functions because:

- A function with 3 parameters of type `number` has a 3-dimensional space of ~2^192 possible inputs
- The chance of randomly guessing `x === 42` is effectively zero
- Hybridization between two wrong answers doesn't find the right answer
- No information flows backwards from branch conditions to input generation

**The fix is concolic execution**: run the function concretely but also track symbolic constraints on inputs, then use an SMT solver to find inputs that satisfy path conditions for uncovered branches.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   shatter CLI / Agent API            │
│              (text-based artifacts in/out)            │
├─────────────────────────────────────────────────────┤
│                  Orchestrator                         │
│   ┌──────────┐ ┌──────────┐ ┌──────────────────┐   │
│   │ Language  │ │ Concolic │ │  Spec Generator  │   │
│   │ Frontend  │ │  Engine  │ │  & Report Writer │   │
│   └────┬─────┘ └────┬─────┘ └───────┬──────────┘   │
│        │             │               │               │
│   ┌────┴─────┐ ┌────┴─────┐ ┌──────┴───────┐      │
│   │  Type    │ │   Z3     │ │  Invariant   │      │
│   │ Analyzer │ │  Solver  │ │  Detector    │      │
│   └────┬─────┘ └──────────┘ └──────────────┘      │
│        │                                             │
│   ┌────┴─────┐ ┌──────────┐ ┌──────────────┐      │
│   │  Code    │ │  Mock    │ │  Execution   │      │
│   │Instrumen-│ │ Registry │ │  Sandbox     │      │
│   │  tor     │ │          │ │  (workers)   │      │
│   └──────────┘ └──────────┘ └──────────────┘      │
└─────────────────────────────────────────────────────┘
```

---

## Phase 1: Concolic Engine for TypeScript

### 1.1 Symbolic Instrumentation (the key innovation)

Instead of just inserting line counters (what v1 does), instrument **every branch condition** to capture its symbolic relationship to function parameters.

**Current v1 instrumentation:**
```typescript
// Original
if (x > 10) { doSomething(); }

// v1 instrumented (just records which line ran)
__record(42); if (x > 10) { __record(43); doSomething(); }
```

**v2 instrumentation:**
```typescript
// v2 instrumented (captures the constraint)
__record(42);
if (__branch(42, "x > 10", () => x > 10, () => ({ op: ">", left: __sym("x"), right: 10 }))) {
  __record(43); doSomething();
}
```

The `__branch` callback records:
- The branch location
- Whether it went true or false (concrete result)
- The **symbolic constraint** (parameter `x` must be `> 10`)

**Implementation approach:**

Use the TypeScript Compiler API's AST visitor (already used in `transform.ts`) but extend it to:

1. **Identify branch conditions**: `if`, `switch`, `?:`, `&&`, `||`, `while`, `for`
2. **Decompose conditions into symbolic expressions**: Walk the condition AST and identify which sub-expressions reference function parameters (directly or through assignments)
3. **Emit symbolic constraint constructors**: Generate code that builds a symbolic representation of the constraint at runtime

The symbolic representation uses a simple expression tree:

```typescript
type SymExpr =
  | { kind: "param", name: string, path: string[] }  // e.g., param "config", path ["timeout"]
  | { kind: "const", value: unknown }
  | { kind: "binop", op: string, left: SymExpr, right: SymExpr }
  | { kind: "unop", op: string, operand: SymExpr }
  | { kind: "call", name: string, args: SymExpr[] }  // e.g., str.startsWith("foo")
  | { kind: "unknown" }  // when we can't track it symbolically
```

**Key technique — taint tracking through assignments:**

When a function does `const y = x + 1; if (y > 10)`, we need to know that the constraint on `y` is really a constraint on `x`. The instrumentor performs a **lightweight data flow analysis** at compile time:

1. Build a map of which local variables are derived from parameters
2. For simple assignments (`const y = x + 1`), record the symbolic expression
3. For complex flows (loops, closures, callbacks), fall back to `unknown`

This doesn't need to be perfect — any branch where we can extract a constraint is a win over random guessing. Branches with `unknown` constraints fall back to the v1 fuzzing approach.

### 1.2 Z3 Integration for Constraint Solving

Use the **`z3-solver` npm package** (official Z3 WASM bindings, actively maintained).

After a concrete execution, we have a **path condition**: the conjunction of all branch constraints along the executed path. To explore a new path, **negate one branch** and solve:

```typescript
import { init } from 'z3-solver';

async function solveForNewPath(
  pathConstraints: SymConstraint[],  // constraints from a concrete run
  targetBranch: number,               // which branch to flip
): Promise<ConcreteValues | "unsat"> {
  const { Context } = await init();
  const Z3 = Context('main');

  // Declare Z3 variables for each function parameter
  const vars = new Map<string, z3.Expr>();
  for (const param of functionParams) {
    if (param.type === 'number') vars.set(param.name, Z3.Int.const(param.name));
    if (param.type === 'string') vars.set(param.name, Z3.String.const(param.name));
    if (param.type === 'boolean') vars.set(param.name, Z3.Bool.const(param.name));
  }

  const solver = new Z3.Solver();

  // Add all constraints BEFORE the target as-is
  for (let i = 0; i < targetBranch; i++) {
    solver.add(toZ3Expr(pathConstraints[i], vars, Z3));
  }
  // NEGATE the target constraint
  solver.add(Z3.Not(toZ3Expr(pathConstraints[targetBranch], vars, Z3)));

  if (await solver.check() === 'sat') {
    const model = solver.model();
    return extractConcreteValues(model, vars);
  }
  return "unsat";
}
```

**Type mapping to Z3 theories:**

| TypeScript Type | Z3 Sort | Notes |
|----------------|---------|-------|
| `number` | `Int` or `Real` | Use `Int` when only integer ops observed; `Real` otherwise |
| `string` | `String` | Z3 has good string theory (length, contains, regex) |
| `boolean` | `Bool` | Direct mapping |
| `enum` | `Int` with range constraints | Map enum values to integers |
| `T[]` | `Array(Int, T)` | Model as Z3 arrays for length/index constraints |
| `object` | Flatten to individual field variables | `config.timeout` becomes `config_timeout: Int` |
| union types | Multiple solver passes | Try each variant |

**Handling unsupported constraints:**

When a branch condition involves operations Z3 can't model (regex matching, complex string formatting, method calls on objects), we:
1. Mark the constraint as `unknown`
2. Use the concrete value from the execution trace as a hint
3. Try fuzzing around that concrete value (v1's approach, but now targeted to a specific parameter)

### 1.3 Concolic Execution Loop

The core algorithm combines concrete execution with symbolic solving:

```
function concolically_explore(fn, maxIterations):
    worklist = [generateInitialInputs(fn)]  // from type analysis
    coveredPaths = Set()
    results = []

    while worklist is not empty AND iterations < maxIterations:
        inputs = worklist.pop()

        // CONCRETE execution with symbolic recording
        result = executeInstrumented(fn, inputs)
        pathId = hash(result.branchDecisions)

        if pathId not in coveredPaths:
            coveredPaths.add(pathId)
            results.push(result)

            // SYMBOLIC: try to flip each branch
            for i in range(result.branchDecisions.length):
                newConstraints = result.pathConstraints[:i] + [NOT result.pathConstraints[i]]
                solution = Z3.solve(newConstraints)
                if solution is SAT:
                    newInputs = solution.model()
                    worklist.push(newInputs)

            // FALLBACK: for unknown constraints, fuzz around concrete values
            for branch in result.unknownBranches:
                fuzzedInputs = fuzzAroundValues(inputs, branch.relevantParams)
                worklist.push(fuzzedInputs)

    return results
```

**Priority ordering for the worklist:**

Not all uncovered branches are equally interesting. Prioritize:
1. Branches that are reachable from already-executed code (adjacent uncovered branches)
2. Branches with solvable constraints (prefer Z3-solvable over fuzzing)
3. Error-handling branches (often reveal important behavior)
4. Branches deeper in the call stack (explore thoroughly)

### 1.4 Enhanced Type-Aware Value Generation

Reuse **fast-check** arbitraries as the baseline generator, replacing the hand-rolled generators in `generator.ts`:

```typescript
import fc from 'fast-check';

function arbitraryForType(type: ts.Type, checker: ts.TypeChecker): fc.Arbitrary<unknown> {
  if (type.flags & ts.TypeFlags.Number) return fc.integer();
  if (type.flags & ts.TypeFlags.String) return fc.string();
  if (type.flags & ts.TypeFlags.Boolean) return fc.boolean();
  if (checker.isArrayType(type)) {
    const elemType = (type as ts.TypeReference).typeArguments![0];
    return fc.array(arbitraryForType(elemType, checker));
  }
  if (type.isUnion()) {
    return fc.oneof(...type.types.map(t => arbitraryForType(t, checker)));
  }
  if (type.isClassOrInterface()) {
    const props = checker.getPropertiesOfType(type);
    const shape: Record<string, fc.Arbitrary<unknown>> = {};
    for (const prop of props) {
      shape[prop.name] = arbitraryForType(checker.getTypeOfSymbol(prop), checker);
    }
    return fc.record(shape);
  }
  // ... etc
}
```

This gives us:
- Well-tested value generation with good distribution
- Built-in shrinking for minimal reproduction
- Composability for complex types

But we **also** use Z3 solutions as seeds — when Z3 says `x = 42, y = "hello"`, we create a concrete test case from that. The best exploration strategy interleaves Z3-directed inputs with fast-check-generated random inputs.

---

## Phase 2: Mocking and Scope Containment

### 2.1 Automatic Dependency Detection

For a target function, statically analyze its AST to identify all external calls:

```typescript
interface ExternalDependency {
  kind: "function-call" | "method-call" | "property-access" | "module-import";
  symbol: string;           // fully qualified name
  sourceModule: string;     // where it's imported from
  signature: ts.Signature;  // TypeScript signature for generating return values
  callSites: number[];      // line numbers where it's called
}
```

Use the TypeScript checker's `getSymbolAtLocation` and `getTypeOfSymbol` to trace every call expression back to its declaration. If the declaration is outside the target scope (different file, different module, node_modules), it's an external dependency.

### 2.2 Mock Generation from Types

For each external dependency, automatically generate a mock based on its return type:

```typescript
function generateMock(dep: ExternalDependency): MockImplementation {
  const returnType = dep.signature.getReturnType();

  return {
    symbol: dep.symbol,
    // Generate plausible return values from the return type
    returnValues: generateValuesForType(returnType),
    // Track calls for side-effect recording
    callLog: [],
    // Default: return successfully. Can be configured to throw.
    behavior: "return-generated-value",
  };
}
```

**Instrumentation for mocking** — rewrite imports at compile time:

```typescript
// Original
import { fetchUser } from './userService';
const user = fetchUser(id);

// Instrumented
const { fetchUser } = __mockRegistry.getOrOriginal('./userService');
const user = __recordCall('fetchUser', [id], () => fetchUser(id));
```

The `__mockRegistry` is populated before execution with generated mocks. The `__recordCall` wrapper captures the arguments and return value regardless of whether a mock or real implementation is used.

### 2.3 Compositional Testing (the "use results to mock" pattern)

This is the most powerful idea in the requirements. When we test function `A` that calls function `B`:

1. **First, test `B` in isolation** — get a comprehensive map of `B`'s behavior:
   ```
   B_behavior = {
     { input: [1, "hello"], output: { id: 1, name: "hello" }, outcome: "completed" },
     { input: [null, ""], output: null, outcome: "error", error: "InvalidInput" },
     ...
   }
   ```

2. **When testing `A`, mock `B` using its behavior map:**
   ```typescript
   mockRegistry.register('B', (args) => {
     const match = B_behavior.find(b => deepEqual(b.input, args));
     if (match) {
       if (match.outcome === 'error') throw new Error(match.error);
       return match.output;
     }
     // No matching behavior — try the closest match or return a type-appropriate default
     return generateFromReturnType(B.returnType);
   });
   ```

3. **Track which of B's behaviors A actually triggers** — this reveals A's assumptions about B's contract.

**Dependency ordering:** Build a call graph and test bottom-up (leaf functions first, then their callers). Use `go/callgraph` for Go; for TypeScript, build a simple call graph from the checker's symbol resolution.

```
                    main()
                   /      \
              handleReq()  validateConfig()
             /    |    \
     parseBody() auth() db.query()
```

Test order: `parseBody`, `auth`, `validateConfig` → `handleReq` → `main`

Each level uses the real behavior recorded from the level below.

### 2.4 Mock Scope Configuration

The user defines a **scope boundary** — everything inside is tested with real code, everything outside is mocked:

```yaml
# shatter.scope.yaml
scope:
  include:
    - src/services/**
    - src/models/**
  exclude:
    - src/services/external/**
  mock:
    - node_modules/**           # always mock
    - src/db/**                 # mock database layer
  passthrough:
    - lodash                    # safe pure functions, don't mock
```

---

## Phase 3: Execution Recording and Performance Measurement

### 3.1 Rich Execution Records

Each test execution produces a comprehensive record:

```typescript
interface ExecutionRecord {
  // Identity
  functionId: string;          // fully qualified function name
  inputHash: string;           // hash of serialized inputs

  // Inputs
  parameters: SerializedValue[];

  // Control flow
  branchPath: BranchDecision[];   // ordered list of branch taken/not-taken
  linesExecuted: number[];
  callsToExternal: ExternalCall[];

  // Outputs
  returnValue: SerializedValue | undefined;
  thrownError: { type: string, message: string, stack: string } | undefined;
  sideEffects: SideEffect[];      // writes to mutable state, console output, etc.

  // Performance
  wallTimeMs: number;
  cpuTimeUs: number;             // process.cpuUsage()
  heapUsedBytes: number;         // process.memoryUsage() delta
  heapAllocatedBytes: number;

  // Metadata
  timestamp: string;
  engineVersion: string;
}
```

**Performance measurement:** Wrap execution in a performance harness:

```typescript
async function measureExecution(fn: Function, args: unknown[]): Promise<PerfMetrics> {
  const cpuBefore = process.cpuUsage();
  const memBefore = process.memoryUsage();
  const startHr = process.hrtime.bigint();

  // Force GC before measurement if available
  if (global.gc) global.gc();

  const result = await fn(...args);

  const elapsed = process.hrtime.bigint() - startHr;
  const cpuAfter = process.cpuUsage(cpuBefore);
  const memAfter = process.memoryUsage();

  return {
    wallTimeMs: Number(elapsed) / 1e6,
    cpuTimeUs: cpuAfter.user + cpuAfter.system,
    heapDelta: memAfter.heapUsed - memBefore.heapUsed,
  };
}
```

Run with `--expose-gc` flag for accurate memory measurement. Execute each test case in a **fresh worker thread** (already done in v1) to isolate memory measurements.

### 3.2 Side Effect Capture

Instrument common side-effect channels:

- **Console output**: Replace `console.log/warn/error` with recording proxies
- **File system**: If `fs` is in scope, intercept read/write calls
- **Network**: Intercept `fetch`/`http` calls (these should be mocked anyway)
- **Global state**: Snapshot and diff global/module-level variables before and after
- **Thrown errors**: Capture full error type, message, and stack

---

## Phase 4: Specification Derivation and Reporting

### 4.1 Behavior Clustering

Group execution records by their **behavioral signature** (not just line coverage as in v1):

```typescript
interface BehaviorCluster {
  id: string;
  signature: string;           // human-readable label like "returns empty array when input is negative"
  pathCondition: SymConstraint[]; // the conjunction of branch constraints
  specimens: ExecutionRecord[];

  // Derived properties
  inputInvariants: Invariant[];   // e.g., "x > 0", "arr.length < 10"
  outputInvariants: Invariant[];  // e.g., "return value is non-null", "return.length === input.length"
  sideEffects: string[];          // e.g., "calls logger.warn once"
}
```

### 4.2 Daikon-Style Invariant Detection

After clustering, run **invariant detection** over each cluster's specimens. Implement the core Daikon algorithm:

1. **Define invariant templates:**
   - Numeric: `x > 0`, `x === C`, `x < y`, `x = y + C`, `x = f(y)`
   - String: `s.length > 0`, `s.startsWith(C)`, `s.includes(C)`
   - Array: `arr.length > 0`, `arr.includes(x)`, `arr is sorted`
   - Relational: `output.length === input.length`, `output ⊆ input`
   - Null: `x !== null`, `x === undefined`

2. **For each cluster, check every template** against all specimens in the cluster:
   ```typescript
   function detectInvariants(specimens: ExecutionRecord[]): Invariant[] {
     const candidates = generateCandidates(specimens[0]); // all possible invariants
     return candidates.filter(inv => specimens.every(s => inv.holds(s)));
   }
   ```

3. **Filter for interesting invariants** — drop trivially true ones, keep those that are surprising or that distinguish this cluster from others.

### 4.3 Specification Report Format

Generate a **text-based specification** that agents and humans can both read:

```markdown
# Function: calculateShipping(order: Order): ShippingResult

## Parameters
- `order.items`: Array<Item> (tested with 0-50 items)
- `order.destination`: Address
- `order.priority`: "standard" | "express" | "overnight"

## Behavior 1: Free shipping for large orders
**When:** order.items.length >= 5 AND order.subtotal() > 100.00
**Returns:** { cost: 0, method: "standard", estimatedDays: 5-7 }
**Invariant:** cost is always 0
**Performance:** 0.1ms avg, <1KB heap

## Behavior 2: Express shipping calculation
**When:** order.priority === "express"
**Returns:** { cost: 12.99-45.99, method: "express", estimatedDays: 2-3 }
**Invariant:** cost scales with order.items.length
**Calls:** rateService.getExpressRate(destination) once
**Performance:** 0.3ms avg, <2KB heap

## Behavior 3: Invalid destination error
**When:** order.destination.zipCode is malformed (not 5 digits)
**Throws:** ValidationError("Invalid zip code")
**Invariant:** always throws, never returns

## Edge Cases
- Empty items array → returns { cost: 0, method: "none" }
- Null destination → throws TypeError

## Dependencies
- rateService.getExpressRate: called only for express/overnight
- rateService.getOvernightRate: called only for overnight
- taxService.calculate: called for all non-free shipping
```

This format is:
- Parseable by agents (structured with clear headers and conditions)
- Readable by humans (natural language descriptions)
- Usable as regression tests (the conditions + expected outputs are machine-verifiable)

### 4.4 Regression Test Export

Generate executable test files from the specification:

```typescript
// Auto-generated by shatter — do not edit manually
// Regenerate with: shatter retest calculateShipping

describe('calculateShipping', () => {
  it('Behavior 1: free shipping for large orders', () => {
    const order = { items: [/* 5 items */], destination: { zip: "90210" }, priority: "standard" };
    mockRateService(); // auto-generated mocks
    const result = calculateShipping(order);
    expect(result.cost).toBe(0);
    expect(result.method).toBe("standard");
  });

  it('Behavior 3: invalid destination error', () => {
    const order = { items: [item1], destination: { zip: "abc" }, priority: "standard" };
    expect(() => calculateShipping(order)).toThrow(ValidationError);
  });
});
```

Also export as a **snapshot file** (JSON) for machine-to-machine regression:

```json
{
  "function": "calculateShipping",
  "version": "2024-01-15T10:30:00Z",
  "behaviors": [
    {
      "id": "b1-free-shipping",
      "precondition": "order.items.length >= 5 && subtotal > 100",
      "exemplar_input": { "..." : "..." },
      "expected_output": { "cost": 0, "method": "standard" },
      "invariants": ["output.cost === 0"]
    }
  ]
}
```

---

## Phase 5: Go Language Support

### 5.1 Go Frontend using `go/ssa` + `go/types`

The Go frontend follows the same architecture but uses Go's excellent static analysis libraries:

**Type analysis** — `go/types` provides:
```go
func analyzeFunction(fn *types.Func) []ParameterInfo {
    sig := fn.Type().(*types.Signature)
    params := sig.Params()
    for i := 0; i < params.Len(); i++ {
        param := params.At(i)
        // param.Name(), param.Type() — fully resolved
    }
}
```

**Control flow and branch extraction** — `go/ssa`:
```go
func extractBranches(fn *ssa.Function) []BranchInfo {
    for _, block := range fn.Blocks {
        // Each block ends with a terminating instruction
        switch term := block.Instrs[len(block.Instrs)-1].(type) {
        case *ssa.If:
            // term.Cond is the branch condition — a ssa.Value
            // Trace it back to function parameters
            constraint := traceToParams(term.Cond, fn.Params)
        case *ssa.Jump:
            // unconditional — no constraint
        }
    }
}
```

**SSA form makes symbolic analysis easier than raw AST** because:
- Every variable is assigned exactly once (no aliasing confusion)
- Phi nodes explicitly show where values merge from different paths
- Data flow is explicit in the instruction graph

**Value generation** — use `rapid` (pgregory.net/rapid):
```go
func generatorForType(t types.Type) *rapid.Generator[any] {
    switch t := t.Underlying().(type) {
    case *types.Basic:
        switch t.Kind() {
        case types.Int: return rapid.Int()
        case types.String: return rapid.String()
        case types.Bool: return rapid.Bool()
        }
    case *types.Slice:
        return rapid.SliceOf(generatorForType(t.Elem()))
    case *types.Struct:
        // Generate each field
    }
}
```

**Constraint solving** — use `aclements/go-z3` (CGo bindings to Z3), or alternatively, shell out to Z3 via subprocess to avoid CGo complexity.

### 5.2 Go Instrumentation

Go doesn't have a built-in transformation API like TypeScript, but AST rewriting is straightforward with `go/ast` + `go/printer`:

```go
// Insert branch recording before if statements
ast.Inspect(file, func(n ast.Node) bool {
    if ifStmt, ok := n.(*ast.IfStmt); ok {
        // Wrap condition: if __branch(id, cond) { ... }
        ifStmt.Cond = wrapWithBranchRecorder(ifStmt.Cond, branchID)
    }
    return true
})
```

**Go mocking** — use `uber-go/mock`'s `mockgen` approach for interfaces. For concrete types, use a **build-tag approach**: generate an instrumented version of the module with swappable function variables:

```go
// Original
func FetchUser(id int) (*User, error) { ... }

// Instrumented (build tag: shatter)
var FetchUser = func(id int) (*User, error) { return _realFetchUser(id) }
func _realFetchUser(id int) (*User, error) { ... }
```

This lets us replace `FetchUser` with a mock at test time without interface indirection.

### 5.3 Go Execution Sandbox

Use `go test` infrastructure or compile and run as a subprocess:

```go
// Generate a test harness file
func generateTestHarness(fn FunctionInfo, inputs []ConcreteInput) string {
    return fmt.Sprintf(`
package %s_test

import (
    "testing"
    "runtime"
    target "%s"
)

func TestShatter(t *testing.T) {
    var m runtime.MemStats
    runtime.ReadMemStats(&m)
    heapBefore := m.HeapAlloc

    start := time.Now()
    result, err := target.%s(%s)
    elapsed := time.Since(start)

    runtime.ReadMemStats(&m)
    heapAfter := m.HeapAlloc

    // Report results back via stdout JSON
    fmt.Println(toJSON(result, err, elapsed, heapAfter-heapBefore))
}`, ...)
}
```

---

## Phase 6: Multi-Language Architecture

### 6.1 Language-Agnostic Core

The core orchestrator, Z3 solver interface, invariant detector, and report generator are **language-independent**. Factor them into a shared core:

```
shatter-core/          (TypeScript — runs the orchestrator)
  orchestrator.ts      — concolic loop, worklist management
  solver.ts            — Z3 interface for constraint solving
  invariants.ts        — Daikon-style invariant detection
  reporter.ts          — spec generation, regression export
  types.ts             — ExecutionRecord, BehaviorCluster, etc.

shatter-ts/            (TypeScript frontend)
  analyzer.ts          — TS compiler API for types + branches
  instrumentor.ts      — AST rewriting for TS
  executor.ts          — worker thread execution
  mocker.ts            — import rewriting + mock registry

shatter-go/            (Go frontend — itself written in Go, communicates via JSON-over-stdio)
  analyzer.go          — go/ssa + go/types
  instrumentor.go      — go/ast rewriting
  executor.go          — subprocess execution
  mocker.go            — mockgen-style stub generation
  main.go              — JSON protocol handler
```

**Communication between core and frontends:**

The Go frontend is a separate binary. The TypeScript orchestrator communicates with it via a simple JSON protocol over stdin/stdout:

```json
// Request: analyze a function
{"command": "analyze", "file": "main.go", "function": "CalculateShipping"}

// Response: function metadata
{"params": ["..."], "branches": ["..."], "dependencies": ["..."]}

// Request: execute with specific inputs
{"command": "execute", "function": "CalculateShipping", "inputs": ["..."], "mocks": {}}

// Response: execution record
{"returnValue": "...", "branchPath": ["..."], "performance": {}}
```

This same protocol works for Java (via a JVM-based frontend) and Rust (via a Rust-based frontend) in the future.

### 6.2 Recommended Implementation Language

**Write the core orchestrator in TypeScript.** Reasons:
- Z3 WASM bindings are best in TypeScript (no CGo hassle)
- TypeScript is the first target language, so TypeScript frontend needs no IPC
- Agents and CLI tools commonly use Node.js
- The existing codebase is TypeScript

For the Go frontend, write a Go binary that handles analysis, instrumentation, and execution, communicating with the core via the JSON protocol.

---

## Phase 7: Agent-Oriented Interface

### 7.1 CLI Design

```bash
# Explore a single function
shatter explore src/services/shipping.ts:calculateShipping

# Explore all exported functions in a file
shatter explore src/services/shipping.ts

# Explore with scope config
shatter explore --scope shatter.scope.yaml src/services/

# Re-run recorded tests (regression)
shatter retest --snapshot snapshots/shipping.json

# Generate human-readable spec
shatter spec src/services/shipping.ts:calculateShipping

# Generate executable test file
shatter export-tests --framework jest src/services/shipping.ts

# Compare against previous snapshot (regression detection)
shatter diff snapshots/shipping.json
```

### 7.2 Output Artifacts

All output is text/JSON — no GUI required:

1. **Exploration log** (streaming, for agent consumption during execution):
   ```
   [explore] calculateShipping: analyzing 3 parameters, 8 branches
   [explore] Path 1: items=[], dest={zip:"90210"}, priority="standard" → {cost:0, method:"none"} (0.1ms)
   [explore] Path 2: items=[...5], dest={zip:"90210"}, priority="standard" → {cost:0, method:"standard"} (0.2ms)
   [explore] Branch 4 (line 23): solving x.priority === "express"... found input
   [explore] Path 3: items=[...1], dest={zip:"90210"}, priority="express" → {cost:12.99} (0.3ms)
   [explore] Coverage: 8/8 branches, 45/52 lines (86.5%)
   [explore] 3 distinct behaviors found
   ```

2. **Specification markdown** (as shown in Phase 4)

3. **Snapshot JSON** (for regression)

4. **Test file** (executable Jest/Go test)

### 7.3 Agent Integration Pattern

An AI coding agent would use shatter like this:

```
Agent: "I need to understand what calculateShipping does before refactoring it."
→ runs: shatter spec src/services/shipping.ts:calculateShipping
→ reads the generated markdown spec
→ now has a comprehensive understanding of all behaviors

Agent: "I've refactored calculateShipping. Let me verify I haven't broken anything."
→ runs: shatter retest --snapshot snapshots/shipping.json
→ sees: "2/3 behaviors match. Behavior 2 (express shipping) now returns different cost."
→ investigates the regression
```

---

## Implementation Roadmap

### Milestone 1: Concolic Engine for TypeScript (core innovation)
- Symbolic constraint extraction from TS branch conditions
- Z3 integration via `z3-solver` npm
- Concolic execution loop with concrete+symbolic tracking
- Replace brute-force generator with Z3-directed + fast-check hybrid
- Basic CLI: `shatter explore <file>:<function>`

### Milestone 2: Mocking and Composition
- Static dependency analysis for TS functions
- Automatic mock generation from return types
- Scope configuration (what to mock, what to test)
- Compositional testing: bottom-up with behavior maps
- Mock registry and import rewriting

### Milestone 3: Recording and Reporting
- Rich execution records with performance metrics
- Behavior clustering (branch-path based)
- Invariant detection (Daikon-style)
- Markdown spec generation
- JSON snapshot export

### Milestone 4: Regression Testing
- Snapshot comparison (`shatter diff`)
- Executable test export (Jest format)
- CI-friendly exit codes and output

### Milestone 5: Go Language Support
- Go frontend using `go/ssa` + `go/types`
- Go AST instrumentation
- Go execution via subprocess
- JSON protocol between core and Go frontend
- Go mock generation (interface-based + function variable approach)

### Milestone 6: Additional Languages
- Java frontend (using ASM or Soot for bytecode analysis, or javac plugin API)
- Rust frontend (using `rustc` MIR or `syn` for AST analysis)
- Shared protocol ensures each frontend is independent

---

## Key Open Source Dependencies Summary

| Component | Tool | Why |
|-----------|------|-----|
| Constraint solving | `z3-solver` (npm) | Official Z3 WASM, no native deps, actively maintained |
| Value generation | `fast-check` | Mature typed generators + shrinking |
| TS type analysis | TypeScript Compiler API | Already proven in v1, complete type info |
| TS AST rewriting | TypeScript Compiler API | Transform API for instrumentation |
| Go type analysis | `go/types` + `go/ssa` | Standard library, production-grade |
| Go AST rewriting | `go/ast` + `go/printer` | Standard library |
| Go mocking | `uber-go/mock` patterns | Proven interface mock generation |
| Go value generation | `pgregory.net/rapid` | Actively maintained property-based testing |
| Invariant detection | Daikon (algorithm only) | Reimplement the core algorithm, not the tool |
| Serialization | `serialize-javascript` | Already in v1 |
