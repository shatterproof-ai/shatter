# Shatter v2: Automatic Exploratory Testing via Concolic Execution

> **Status: historical roadmap — not current state.** This is the original v2
> architecture plan. It captures the design vision and rationale; it describes
> planned and in-progress work, and includes features that were later reshaped or
> never built (e.g. some CLI subcommands, the Java frontend milestone, and a
> project layout that predates the current crate set). It is **not** an
> authoritative description of what Shatter does today.
>
> For current, implemented behavior consult:
> - **[SPEC.md](SPEC.md)** — the authoritative living specification of current
>   behavior. When PLAN.md and SPEC.md disagree, SPEC.md reflects reality.
> - **[docs/INDEX.md](docs/INDEX.md)** — documentation map: which document covers what.
> - **[PROTOCOL.md](PROTOCOL.md)** and `protocol/registry.yaml` — the current wire protocol.
>
> Read this document for the "why" behind the design, not the "what" of the
> implementation.

## The Core Problem with v1

The current system uses **coverage-guided random fuzzing**: generate random typed values, run the function, observe which lines executed, then try to hybridize/breed inputs to find new paths. This fails for non-trivial functions because:

- A function with 3 parameters of type `number` has a 3-dimensional space of ~2^192 possible inputs
- The chance of randomly guessing `x === 42` is effectively zero
- Hybridization between two wrong answers doesn't find the right answer
- No information flows backwards from branch conditions to input generation

**The fix is concolic execution**: run the function concretely but also track symbolic constraints on inputs, then use an SMT solver to find inputs that satisfy path conditions for uncovered branches.

---

## Architecture Overview

The core engine is written in **Rust**. Language-specific frontends (TypeScript, Go, etc.) are separate binaries that communicate with the core via a JSON protocol over stdin/stdout. Each frontend handles analysis, instrumentation, and execution for its language; the core handles orchestration, constraint solving, invariant detection, and reporting.

```
┌─────────────────────────────────────────────────────────────┐
│                 shatter CLI (Rust binary)                     │
│            clap-based CLI, text/JSON artifacts out            │
├─────────────────────────────────────────────────────────────┤
│                    Rust Core Engine                           │
│                                                               │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐ │
│  │   Concolic    │ │  Z3 Solver   │ │   Spec Generator     │ │
│  │ Orchestrator  │ │  (C FFI)     │ │   & Report Writer    │ │
│  └──────┬───────┘ └──────────────┘ └──────────────────────┘ │
│         │                                                     │
│  ┌──────┴───────┐ ┌──────────────┐ ┌──────────────────────┐ │
│  │  Worklist &   │ │  Invariant   │ │   Behavior           │ │
│  │  Path Manager │ │  Detector    │ │   Clustering         │ │
│  │               │ │  (rayon)     │ │                      │ │
│  └──────────────┘ └──────────────┘ └──────────────────────┘ │
│         │                                                     │
│  ┌──────┴───────┐ ┌──────────────┐ ┌──────────────────────┐ │
│  │  Frontend     │ │    Mock      │ │   Execution Record   │ │
│  │  Manager      │ │   Registry   │ │   Store              │ │
│  │ (subprocess)  │ │              │ │                      │ │
│  └──────────────┘ └──────────────┘ └──────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│              JSON-over-stdio Protocol                         │
├──────────┬──────────────┬───────────────┬───────────────────┤
│ shatter- │  shatter-go  │  shatter-java │  shatter-rust-    │
│ ts       │  (Go binary) │  (JVM)        │  frontend         │
│ (Node.js │              │               │  (Rust binary)    │
│  process)│              │               │                   │
└──────────┴──────────────┴───────────────┴───────────────────┘
```

**Why Rust for the core:**
- 10-20x more memory-efficient than Node.js (~50 MB vs ~500-800 MB for 100K execution records)
- Native Z3 integration via C FFI — no WASM overhead
- `rayon` for trivial data parallelism with shared memory (no serialization between threads)
- Rust enums model symbolic expressions with zero-cost exhaustive pattern matching
- Single static binary <10 MB, instant startup (~2ms)
- SWC (written in Rust) can parse/transform TypeScript at native speed as a library

---

## Phase 1: Concolic Engine (Rust Core)

### 1.1 Core Data Types

Rust's enum system maps directly to the symbolic expression tree:

```rust
/// A symbolic expression representing a constraint on function inputs.
#[derive(Debug, Clone, PartialEq)]
pub enum SymExpr {
    /// Reference to a function parameter, with optional field path.
    /// e.g., param "config", path ["timeout"] → config.timeout
    Param { name: String, path: Vec<String> },

    /// A literal constant value.
    Const(ConstValue),

    /// Binary operation: left op right
    BinOp { op: BinOpKind, left: Box<SymExpr>, right: Box<SymExpr> },

    /// Unary operation: op operand
    UnOp { op: UnOpKind, operand: Box<SymExpr> },

    /// Method/function call with symbolic arguments
    Call { name: String, receiver: Option<Box<SymExpr>>, args: Vec<SymExpr> },

    /// Could not be tracked symbolically — fall back to fuzzing
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Eq, Ne, Lt, Le, Gt, Ge,
    Add, Sub, Mul, Div, Mod,
    And, Or,
    BitwiseAnd, BitwiseOr, BitwiseXor,
    In, InstanceOf,
}
```

Exhaustive `match` ensures every variant is handled — the compiler catches missing cases.

### 1.2 Symbolic Instrumentation

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

Each language frontend is responsible for its own instrumentation. For TypeScript, the frontend uses the TypeScript Compiler API's AST visitor (already used in v1's `transform.ts`) extended to:

1. **Identify branch conditions**: `if`, `switch`, `?:`, `&&`, `||`, `while`, `for`
2. **Decompose conditions into symbolic expressions**: Walk the condition AST and identify which sub-expressions reference function parameters (directly or through assignments)
3. **Emit symbolic constraint constructors**: Generate code that builds a symbolic representation of the constraint at runtime

The frontend sends the symbolic constraints back to the Rust core as JSON. The core deserializes them into `SymExpr` values and feeds them to the Z3 solver.

**Key technique — taint tracking through assignments:**

When a function does `const y = x + 1; if (y > 10)`, we need to know that the constraint on `y` is really a constraint on `x`. The frontend instrumentor performs a **lightweight data flow analysis** at compile time:

1. Build a map of which local variables are derived from parameters
2. For simple assignments (`const y = x + 1`), record the symbolic expression
3. For complex flows (loops, closures, callbacks), fall back to `Unknown`

This doesn't need to be perfect — any branch where we can extract a constraint is a win over random guessing. Branches with `Unknown` constraints fall back to the v1 fuzzing approach.

### 1.3 Z3 Integration via C FFI

Use the **`z3` crate** (Rust bindings wrapping Z3's native C API). This gives native-speed constraint solving with no WASM overhead.

After a concrete execution, we have a **path condition**: the conjunction of all branch constraints along the executed path. To explore a new path, **negate one branch** and solve:

```rust
use z3::*;

fn solve_for_new_path(
    ctx: &Context,
    path_constraints: &[SymConstraint],
    target_branch: usize,
    params: &[ParamInfo],
) -> Option<Vec<ConcreteValue>> {
    let solver = Solver::new(ctx);

    // Declare Z3 variables for each function parameter
    let vars: HashMap<String, Dynamic> = params.iter().map(|p| {
        let var = match p.typ {
            ParamType::Int => Dynamic::from(ast::Int::new_const(ctx, p.name.as_str())),
            ParamType::Float => Dynamic::from(ast::Real::new_const(ctx, p.name.as_str())),
            ParamType::Str => Dynamic::from(ast::String::new_const(ctx, p.name.as_str())),
            ParamType::Bool => Dynamic::from(ast::Bool::new_const(ctx, p.name.as_str())),
        };
        (p.name.clone(), var)
    }).collect();

    // Add all constraints BEFORE the target as-is
    for constraint in &path_constraints[..target_branch] {
        solver.assert(&to_z3_expr(ctx, constraint, &vars));
    }
    // NEGATE the target constraint
    solver.assert(&to_z3_expr(ctx, &path_constraints[target_branch], &vars).not());

    match solver.check() {
        SatResult::Sat => {
            let model = solver.get_model().unwrap();
            Some(extract_concrete_values(&model, &vars, params))
        }
        _ => None,
    }
}
```

**Type mapping to Z3 theories:**

| Source Type | Z3 Sort | Notes |
|------------|---------|-------|
| `number` / `int` / `i32` etc. | `Int` or `Real` | Use `Int` when only integer ops observed; `Real` otherwise |
| `string` / `String` | `String` | Z3 has good string theory (length, contains, regex) |
| `boolean` / `bool` | `Bool` | Direct mapping |
| enum types | `Int` with range constraints | Map enum values to integers |
| arrays / slices | `Array(Int, T)` | Model as Z3 arrays for length/index constraints |
| objects / structs | Flatten to individual field variables | `config.timeout` becomes `config_timeout: Int` |
| union types | Multiple solver passes | Try each variant |

**Handling unsupported constraints:**

When a branch condition involves operations Z3 can't model (regex matching, complex string formatting, method calls on objects), we:
1. Mark the constraint as `Unknown`
2. Use the concrete value from the execution trace as a hint
3. Try fuzzing around that concrete value (targeted to the specific parameter)

### 1.4 Concolic Execution Loop

The core algorithm, implemented in Rust, combines concrete execution with symbolic solving:

```rust
fn concolically_explore(
    frontend: &mut FrontendProcess,
    function: &FunctionInfo,
    config: &ExploreConfig,
) -> Vec<ExecutionRecord> {
    let ctx = z3::Context::new(&z3::Config::new());
    let mut worklist: BinaryHeap<PrioritizedInput> = BinaryHeap::new();
    let mut covered_paths: HashSet<u64> = HashSet::new();
    let mut results: Vec<ExecutionRecord> = Vec::new();

    // Seed with initial inputs from type analysis
    for input in frontend.generate_initial_inputs(function) {
        worklist.push(PrioritizedInput::new(input, Priority::Seed));
    }

    while let Some(prioritized) = worklist.pop() {
        if results.len() >= config.max_iterations { break; }

        // CONCRETE execution via frontend subprocess
        let record = frontend.execute(function, &prioritized.inputs);
        let path_id = hash_branch_path(&record.branch_path);

        if covered_paths.insert(path_id) {
            // SYMBOLIC: try to flip each branch (in parallel with rayon)
            let new_inputs: Vec<_> = record.branch_path.par_iter()
                .enumerate()
                .filter_map(|(i, _)| {
                    solve_for_new_path(&ctx, &record.path_constraints, i, &function.params)
                })
                .collect();

            for inputs in new_inputs {
                worklist.push(PrioritizedInput::new(inputs, Priority::Z3Solved));
            }

            // FALLBACK: for Unknown constraints, fuzz around concrete values
            for branch in &record.unknown_branches {
                let fuzzed = fuzz_around_values(&prioritized.inputs, &branch.relevant_params);
                worklist.push(PrioritizedInput::new(fuzzed, Priority::Fuzzed));
            }

            results.push(record);
        }
    }

    results
}
```

**Priority ordering for the worklist:**

Not all uncovered branches are equally interesting. Prioritize:
1. Branches that are reachable from already-executed code (adjacent uncovered branches)
2. Branches with solvable constraints (prefer Z3-solvable over fuzzing)
3. Error-handling branches (often reveal important behavior)
4. Branches deeper in the call stack (explore thoroughly)

### 1.5 Value Generation with proptest

Use **proptest** for type-aware random value generation within the Rust core. The core generates JSON-serialized values that frontends can deserialize into their native types:

```rust
use proptest::prelude::*;

fn arbitrary_for_type(typ: &TypeInfo) -> BoxedStrategy<serde_json::Value> {
    match typ {
        TypeInfo::Int => any::<i64>().prop_map(|v| json!(v)).boxed(),
        TypeInfo::Float => any::<f64>()
            .prop_filter("finite", |v| v.is_finite())
            .prop_map(|v| json!(v)).boxed(),
        TypeInfo::Str => any::<String>().prop_map(|v| json!(v)).boxed(),
        TypeInfo::Bool => any::<bool>().prop_map(|v| json!(v)).boxed(),
        TypeInfo::Array(elem) => {
            let elem_strat = arbitrary_for_type(elem);
            prop::collection::vec(elem_strat, 0..20)
                .prop_map(|v| json!(v)).boxed()
        }
        TypeInfo::Object(fields) => {
            let field_strats: Vec<_> = fields.iter()
                .map(|(name, typ)| (name.clone(), arbitrary_for_type(typ)))
                .collect();
            // Combine field strategies into a JSON object
            combine_fields(field_strats).boxed()
        }
        TypeInfo::Union(variants) => {
            let strats: Vec<_> = variants.iter()
                .map(|v| arbitrary_for_type(v))
                .collect();
            Union::new(strats).boxed()
        }
        TypeInfo::Nullable(inner) => {
            prop_oneof![
                Just(json!(null)),
                arbitrary_for_type(inner),
            ].boxed()
        }
    }
}
```

Z3 solutions are interleaved with proptest-generated random inputs. The best exploration strategy alternates between Z3-directed inputs (targeted at specific uncovered branches) and proptest-generated diverse inputs (for broad coverage).

---

## Phase 2: Mocking and Scope Containment

### 2.1 Automatic Dependency Detection

For a target function, each language frontend statically analyzes the AST to identify all external calls and reports them to the core:

```rust
/// Reported by a language frontend for each external dependency.
#[derive(Debug, Deserialize)]
pub struct ExternalDependency {
    pub kind: DependencyKind,
    pub symbol: String,           // fully qualified name
    pub source_module: String,    // where it's imported from
    pub return_type: TypeInfo,    // for generating mock return values
    pub param_types: Vec<TypeInfo>,
    pub call_sites: Vec<usize>,   // line numbers where it's called
}

#[derive(Debug, Deserialize)]
pub enum DependencyKind {
    FunctionCall,
    MethodCall,
    PropertyAccess,
    ModuleImport,
}
```

For TypeScript, the frontend uses the checker's `getSymbolAtLocation` and `getTypeOfSymbol` to trace every call expression back to its declaration. If the declaration is outside the target scope, it's an external dependency.

### 2.2 Mock Generation from Types

The Rust core generates mock configurations based on return types. These are sent to the frontend which injects them at the instrumentation level:

```rust
fn generate_mock(dep: &ExternalDependency) -> MockConfig {
    let return_values: Vec<serde_json::Value> = (0..5)
        .map(|_| generate_value_for_type(&dep.return_type))
        .collect();

    MockConfig {
        symbol: dep.symbol.clone(),
        return_values,
        should_track_calls: true,
        default_behavior: MockBehavior::ReturnGenerated,
    }
}
```

**Instrumentation for mocking** — the frontend rewrites imports at compile time:

```typescript
// Original
import { fetchUser } from './userService';
const user = fetchUser(id);

// Instrumented by the TS frontend
const { fetchUser } = __mockRegistry.getOrOriginal('./userService');
const user = __recordCall('fetchUser', [id], () => fetchUser(id));
```

The `__mockRegistry` is populated before execution with mocks specified by the core. The `__recordCall` wrapper captures the arguments and return value regardless of whether a mock or real implementation is used.

### 2.3 Compositional Testing (the "use results to mock" pattern)

This is the most powerful idea. When we test function `A` that calls function `B`:

1. **First, test `B` in isolation** — the core builds a comprehensive behavior map:
   ```rust
   pub struct BehaviorMap {
       pub function: String,
       pub behaviors: Vec<ObservedBehavior>,
   }

   pub struct ObservedBehavior {
       pub inputs: Vec<serde_json::Value>,
       pub output: serde_json::Value,
       pub outcome: Outcome, // Completed, Error(String), Timeout
   }
   ```

2. **When testing `A`, the core instructs the frontend to mock `B` using its behavior map.** The frontend looks up the closest matching observed behavior for the given arguments.

3. **The core tracks which of B's behaviors A actually triggers** — this reveals A's assumptions about B's contract.

**Dependency ordering:** Build a call graph and test bottom-up (leaf functions first, then their callers). For Go, the frontend uses `go/callgraph`; for TypeScript, it builds a call graph from the checker's symbol resolution.

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

Each test execution produces a comprehensive record, stored efficiently in Rust:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    // Identity
    pub function_id: String,
    pub input_hash: u64,

    // Inputs
    pub parameters: Vec<serde_json::Value>,

    // Control flow
    pub branch_path: Vec<BranchDecision>,
    pub lines_executed: Vec<u32>,
    pub calls_to_external: Vec<ExternalCall>,
    pub path_constraints: Vec<SymConstraint>,

    // Outputs
    pub return_value: Option<serde_json::Value>,
    pub thrown_error: Option<ErrorInfo>,
    pub side_effects: Vec<SideEffect>,

    // Performance
    pub wall_time_ms: f64,
    pub cpu_time_us: u64,
    pub heap_used_bytes: u64,
    pub heap_allocated_bytes: u64,

    // Metadata
    pub timestamp: String,
    pub engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchDecision {
    pub branch_id: u32,
    pub line: u32,
    pub taken: bool,
    pub constraint: SymConstraint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SymConstraint {
    Expr(SymExpr),
    Unknown { hint: String },
}
```

At ~200-500 bytes per record (Rust's inline allocation, no boxing overhead for enums), the core can hold 100K+ records in ~50 MB — an order of magnitude better than Node.js.

### 3.2 Performance Measurement

Performance metrics are captured by the language frontend during execution and reported back to the core. Each frontend uses its language's native profiling APIs:

**TypeScript frontend:**
```typescript
const cpuBefore = process.cpuUsage();
const memBefore = process.memoryUsage();
const startHr = process.hrtime.bigint();
// ... execute ...
const elapsed = process.hrtime.bigint() - startHr;
```

**Go frontend:**
```go
var m runtime.MemStats
runtime.ReadMemStats(&m)
heapBefore := m.HeapAlloc
start := time.Now()
// ... execute ...
elapsed := time.Since(start)
```

Each frontend reports these metrics as part of the execution result JSON. The core stores them in the `ExecutionRecord`.

### 3.3 Side Effect Capture

Each frontend instruments common side-effect channels for its language:

- **Console output**: Replace `console.log/warn/error` (TS) or intercept `fmt.Print*` (Go) with recording proxies
- **File system**: Intercept `fs` (TS) or `os` (Go) calls
- **Network**: Intercept `fetch`/`http` calls (these should be mocked anyway)
- **Global state**: Snapshot and diff module-level variables before and after
- **Thrown errors**: Capture full error type, message, and stack

Side effects are serialized as JSON and sent back to the core as part of the execution result.

---

## Phase 4: Specification Derivation and Reporting

### 4.1 Behavior Clustering

The core groups execution records by their **behavioral signature** (not just line coverage as in v1):

```rust
pub struct BehaviorCluster {
    pub id: String,
    pub signature: String,           // e.g., "returns empty array when input is negative"
    pub path_condition: Vec<SymConstraint>,
    pub specimens: Vec<ExecutionRecord>,

    // Derived properties (computed by invariant detection)
    pub input_invariants: Vec<Invariant>,
    pub output_invariants: Vec<Invariant>,
    pub side_effect_summary: Vec<String>,
}
```

Clustering is based on the branch path (which branches were taken/not-taken), not just which lines executed. Two executions that take different branches are always in different clusters, even if they happen to execute the same lines.

### 4.2 Daikon-Style Invariant Detection

After clustering, run **invariant detection** over each cluster's specimens using `rayon` for parallelism:

```rust
use rayon::prelude::*;

fn detect_invariants_all_clusters(clusters: &mut [BehaviorCluster]) {
    clusters.par_iter_mut().for_each(|cluster| {
        cluster.input_invariants = detect_invariants(&cluster.specimens, InvariantTarget::Input);
        cluster.output_invariants = detect_invariants(&cluster.specimens, InvariantTarget::Output);
    });
}

fn detect_invariants(specimens: &[ExecutionRecord], target: InvariantTarget) -> Vec<Invariant> {
    let candidates = generate_candidates(&specimens[0], target);
    candidates.into_par_iter()
        .filter(|inv| specimens.iter().all(|s| inv.holds(s)))
        .collect()
}
```

**Invariant templates:**
- Numeric: `x > 0`, `x == C`, `x < y`, `x = y + C`
- String: `s.len() > 0`, `s.starts_with(C)`, `s.contains(C)`
- Array: `arr.len() > 0`, `arr.contains(x)`, `arr is sorted`
- Relational: `output.len() == input.len()`, `output ⊆ input`
- Null: `x != null`, `x == null`

Filter for interesting invariants — drop trivially true ones, keep those that distinguish this cluster from others.

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

Generate executable test files from the specification. The core emits language-specific test code:

**TypeScript (Jest):**
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

**Go:**
```go
// Auto-generated by shatter — do not edit manually

func TestCalculateShipping_FreeShipping(t *testing.T) {
    order := Order{Items: makeItems(5), Destination: Address{Zip: "90210"}, Priority: "standard"}
    result, err := CalculateShipping(order)
    require.NoError(t, err)
    assert.Equal(t, 0.0, result.Cost)
    assert.Equal(t, "standard", result.Method)
}
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

## Phase 5: Language Frontends

### 5.1 Frontend Protocol

All frontends communicate with the Rust core via newline-delimited JSON over stdin/stdout:

```json
// Core → Frontend: analyze a function
{"command": "analyze", "file": "main.go", "function": "CalculateShipping"}

// Frontend → Core: function metadata
{
  "params": [{"name": "order", "type": {"kind": "object", "fields": [...]}}],
  "branches": [{"id": 1, "line": 23, "condition": "order.priority == \"express\""}],
  "dependencies": [{"symbol": "rateService.getExpressRate", "return_type": {...}}]
}

// Core → Frontend: execute with specific inputs and mocks
{
  "command": "execute",
  "function": "CalculateShipping",
  "inputs": [{"items": [...], "priority": "express"}],
  "mocks": {"rateService.getExpressRate": {"return_value": 12.99}}
}

// Frontend → Core: execution record
{
  "return_value": {"cost": 12.99, "method": "express"},
  "branch_path": [{"id": 1, "taken": true, "constraint": {"kind": "binop", ...}}],
  "performance": {"wall_time_ms": 0.3, "cpu_time_us": 250, "heap_bytes": 1024}
}
```

The core spawns frontends as long-lived subprocesses. `serde_json` handles serialization at 600-800 MB/s on the core side.

### 5.2 TypeScript Frontend (Node.js process)

The TypeScript frontend uses:
- **TypeScript Compiler API** for type analysis and branch extraction
- **TypeScript Compiler API Transform** for AST rewriting / instrumentation
- **Worker threads** for sandboxed execution of instrumented code

This is largely based on v1's existing code (`transform.ts`, `generator.ts`, `worker.ts`, `supervisor.ts`), refactored to communicate via the JSON protocol instead of being the orchestrator.

### 5.3 Go Frontend (Go binary)

The Go frontend uses Go's excellent static analysis libraries:

**Type analysis** — `go/types`:
```go
func analyzeFunction(fn *types.Func) []ParamInfo {
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
        switch term := block.Instrs[len(block.Instrs)-1].(type) {
        case *ssa.If:
            // term.Cond is the branch condition — trace back to params
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

**Instrumentation** — `go/ast` + `go/printer` for AST rewriting.

**Mocking** — `uber-go/mock` patterns for interfaces. For concrete types, use a build-tag approach with swappable function variables.

**Execution** — compile and run instrumented code as a subprocess, capturing JSON results via stdout.

### 5.4 Future Frontends

The protocol is language-agnostic. Future frontends follow the same pattern:
- **Java**: Use ASM or Soot for bytecode analysis, javac plugin API for source-level analysis
- **Rust**: Use `rustc` MIR (via `rustc_interface`) or `syn` for AST analysis
- **Kotlin**: Use `kotlin-compiler-embeddable` or ANTLR

Each frontend is an independent binary — adding a new language doesn't require changing the core.

---

## Phase 6: Agent-Oriented Interface

### 6.1 CLI Design (Rust, using clap)

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
shatter diff snapshots/shipping.json current/shipping.json

# Compare against previous snapshot (regression detection)
shatter diff snapshots/shipping.json
```

Startup is instant (~2ms). The CLI auto-detects which frontend to use based on file extension.

### 6.2 Output Artifacts

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

### 6.3 Agent Integration Pattern

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

### Milestone 1: Rust Core Scaffold
- Project setup with `cargo`, `clap` for CLI, `serde` for JSON
- `SymExpr` enum and Z3 integration via `z3` crate (C FFI)
- Frontend protocol definition and subprocess manager
- Concolic execution loop (orchestrator)
- `proptest`-based value generation from `TypeInfo`

### Milestone 2: TypeScript Frontend
- Refactor v1's `transform.ts` to emit symbolic constraints (not just line counters)
- Implement the JSON protocol handler (stdin/stdout)
- Type analysis → `TypeInfo` serialization
- Branch extraction with symbolic constraint construction
- Instrumented execution via worker threads, reporting results as JSON
- Mock registry and import rewriting

### Milestone 3: End-to-End Concolic Testing
- Wire core ↔ TS frontend via protocol
- Z3-directed input generation from path constraints
- Interleave Z3 solutions with proptest random inputs
- Coverage tracking and worklist priority management
- Basic CLI: `shatter explore <file>:<function>`

### Milestone 4: Mocking and Composition
- Dependency detection (frontend reports external calls)
- Mock configuration from return types
- Scope configuration (`shatter.scope.yaml`)
- Compositional testing: bottom-up with behavior maps
- Behavior map storage and lookup

### Milestone 5: Recording and Reporting
- Rich execution records with performance metrics
- Behavior clustering (branch-path based)
- Invariant detection with rayon parallelism (Daikon-style)
- Markdown spec generation
- JSON snapshot export

### Milestone 6: Regression Testing
- Snapshot comparison (`shatter diff`)
- Executable test export (Jest format, Go test format)
- CI-friendly exit codes and output

### Milestone 7: Go Frontend
- Go binary implementing the frontend protocol
- Type analysis via `go/types`
- Branch extraction via `go/ssa`
- AST instrumentation via `go/ast` + `go/printer`
- Execution via subprocess with JSON results
- Mock generation (interface-based + function variable approach)

### Milestone 8: Additional Languages
- Java frontend (ASM/Soot for bytecode, or javac plugin API)
- Rust frontend (`rustc` MIR or `syn` for AST)
- Each frontend is independent — shared protocol ensures consistency

---

## Key Open Source Dependencies

### Rust Core

| Component | Crate | Why |
|-----------|-------|-----|
| CLI | `clap` | Best-in-class argument parsing, auto-generated help |
| Constraint solving | `z3` (crate) | C FFI wrapper for Z3, native speed |
| JSON serialization | `serde` + `serde_json` | 600-800 MB/s, zero-copy deserialization |
| Data parallelism | `rayon` | Trivial `par_iter()` for invariant detection |
| Async subprocess | `tokio` | Async I/O for frontend process management |
| Value generation | `proptest` | Mature typed generators + shrinking |
| Hashing | `ahash` or `xxhash-rust` | Fast path hashing for branch signatures |
| Terminal output | `indicatif` + `crossterm` | Progress bars, colored output |

### TypeScript Frontend

| Component | Package | Why |
|-----------|---------|-----|
| Type analysis | TypeScript Compiler API | Complete type info, proven in v1 |
| AST rewriting | TypeScript Compiler API | Transform API for instrumentation |
| Execution sandbox | `worker_threads` | Isolated execution per test case |

### Go Frontend

| Component | Package | Why |
|-----------|---------|-----|
| Type analysis | `go/types` + `go/ssa` | Standard library, production-grade |
| AST rewriting | `go/ast` + `go/printer` | Standard library |
| Call graph | `go/callgraph` | Dependency ordering for compositional testing |
| Mock generation | `uber-go/mock` patterns | Proven interface mock generation |

---

## Project Structure

```
shatter/
├── Cargo.toml                    # Rust workspace root
├── shatter-core/                 # Rust core engine (library crate)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── sym_expr.rs           # SymExpr enum, ConstValue, BinOpKind
│   │   ├── solver.rs             # Z3 integration, constraint solving
│   │   ├── orchestrator.rs       # Concolic execution loop, worklist
│   │   ├── invariants.rs         # Daikon-style invariant detection
│   │   ├── clustering.rs         # Behavior clustering
│   │   ├── reporter.rs           # Markdown spec, JSON snapshot generation
│   │   ├── frontend.rs           # Frontend subprocess protocol
│   │   ├── mock_registry.rs      # Mock configuration and behavior maps
│   │   ├── execution_record.rs   # ExecutionRecord, BranchDecision types
│   │   ├── value_gen.rs          # proptest-based value generation
│   │   └── types.rs              # TypeInfo, ParamInfo, shared types
│   └── Cargo.toml
├── shatter-cli/                  # Rust CLI binary (thin wrapper over core)
│   ├── src/
│   │   └── main.rs               # clap CLI, subcommands
│   └── Cargo.toml
├── shatter-ts/                   # TypeScript frontend (Node.js)
│   ├── src/
│   │   ├── main.ts               # JSON protocol handler (stdin/stdout)
│   │   ├── analyzer.ts           # TS Compiler API for types + branches
│   │   ├── instrumentor.ts       # AST rewriting for symbolic instrumentation
│   │   ├── executor.ts           # Worker thread execution sandbox
│   │   └── mocker.ts             # Import rewriting + mock injection
│   ├── package.json
│   └── tsconfig.json
├── shatter-go/                   # Go frontend (Go binary)
│   ├── main.go                   # JSON protocol handler
│   ├── analyzer.go               # go/ssa + go/types analysis
│   ├── instrumentor.go           # go/ast rewriting
│   ├── executor.go               # Subprocess execution
│   ├── mocker.go                 # mockgen-style stub generation
│   └── go.mod
├── PLAN.md
├── LANGUAGE-EVALUATION.md
└── shatter.scope.yaml.example
```
