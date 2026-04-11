# shatter-core

Rust core engine (library crate). Contains the concolic explorer, constraint solver integration, protocol types, batch analysis, call graph, and export logic.

## Key Modules

- `explorer.rs` — Concolic exploration loop and `format_exploration_report()`
- `protocol.rs` — Protocol message types shared across frontends
- `batch_analyze.rs` — Multi-file analysis orchestration
- `call_graph.rs` — Function dependency graph
- `export.rs` — Test generation from behavior maps
- `invariants.rs` — Invariant inference from execution traces
- `spec.rs` — Behavioral specification output format
- `input_gen.rs` — Generator-aware input generation and mutation
- `coverage_metrics.rs` — Concolic coverage reporting
- `equivalence.rs` — Equivalence class clustering
- `orchestrator.rs` — Multi-round exploration orchestration
- `scan_orchestrator.rs` — Multi-file scan coordination
- `executability.rs` — Opaque type detection and executability checks

## Formal Methods & Verification

Four complementary tools, each with a distinct role. PBT is the workhorse; the others fill gaps PBT can't reach.

| Tool | Role | When to use |
|---|---|---|
| **Property-based testing** | Invariant discovery, regression prevention | Any non-trivial public function with invariants |
| **Native fuzzing** | Crash resistance at parsing boundaries | Code that deserializes untrusted input |
| **Contracts** (`contracts` crate) | Runtime assertions at trust boundaries | Only where Rust's type system can't express the invariant |
| **Kani model checking** (deferred — P4) | Exhaustive verification of critical algorithms | Highest-stakes properties only (solver correctness). Not yet in use. |

## Property-Based Testing Policy (proptest)

Every non-trivial public function should have proptest coverage. PBT is not optional decoration — it is a primary testing strategy alongside unit tests and E2E tests. Prioritize:

1. **Trust boundaries first**: serialization roundtrips, FFI bridges (Z3 solver), protocol parsing. These have the highest bug density.
2. **Semantic invariants over structural checks**: "solved values match ParamInfo types" is valuable; "Request serializes to JSON" is table stakes. Both are needed, but semantic properties catch real bugs.
3. **Full pipeline properties**: test compose functions, not just units. `arbitrary constraints → solve → overlay → type-check output` catches integration bugs that per-function tests miss.
4. **State machine properties**: for stateful components (orchestrator worklist, coverage tracking), test ordering, deduplication, convergence, and budget exhaustion.
5. **Mutation contracts**: all input generation (mutate, crossover, shrink) must preserve type contracts and vector length.

Shared generators live in `test_arbitraries.rs` — reuse them. Depth-bound recursive types (SymExpr, TypeInfo) to avoid explosion. Filter NaN from floats (breaks PartialEq roundtrips).

**When NOT to use proptest**: simple getters/setters, thin wrappers, tests where specific examples are clearer.

**Target**: every module with a `proptest!` block should have properties covering its core invariants, not just serialization.

Sub-crate CLAUDE.md files (`shatter-ts/`, `shatter-go/`) document per-component PBT priorities and the language-specific PBT tooling (`fast-check`, `rapid`).

## Native Fuzzing

- **Go**: `testing.F` in `*_fuzz_test.go` files — byte-level mutation for crash/panic discovery at parsing boundaries. Seed corpus from existing test fixtures.
- **Rust**: `cargo-fuzz` for deserialization boundaries.
- Add a fuzz target for any code that deserializes untrusted input (protocol messages, subprocess JSON).

## Contracts Policy (`contracts` crate)

Contracts (`#[requires]`, `#[ensures]`) are active only in debug/test builds. They are for invariants where **all three** of the following hold:

1. **Trust boundary**: the function receives data from outside Rust's type system — Z3 FFI, deserialized JSON from a frontend subprocess, or index/length relationships between two independently-constructed collections.
2. **Type gap**: the invariant cannot be encoded in the function signature. Prefer `Option`, `Result`, `NonZeroU32`, enums, and newtypes. If you can express it as a type, do that instead.
3. **Silent corruption**: violating the invariant doesn't produce an immediate, obvious error (panic, Z3 sort mismatch) but instead causes a wrong result that propagates downstream — wrong Z3 sort inferred, solved value with wrong type, truncated output vector.

**Practical rule**: if you can't point to a real or plausible bug the contract would catch that proptest wouldn't, skip it. Proptest generates thousands of structured inputs and exercises contracts on every call path it reaches. Contracts add value only at boundaries where the corruption is silent enough that even a proptest failure would be hard to diagnose without the contract's pinpointed assertion.

**Qualifying sites** (the full list — expand only if a new boundary meeting all three criteria is introduced):

| Function | Why it qualifies |
|----------|-----------------|
| `solve_for_new_path()` | Z3 FFI; negate_index is runtime; out-of-bounds → wrong constraint negated silently |
| `solve_for_new_path()` | Z3 FFI; solved value types vs ParamInfo; Int solution for string param → silent type mismatch |
| `overlay_solved_values()` | Cross-collection alignment; length mismatch → truncated/padded inputs sent to frontend |
| `to_z3_expr()` | Z3 FFI; Param indices are runtime; invalid index → wrong Z3 variable → silently wrong solution |
| Protocol validation wrappers | Subprocess JSON; semantic validity post-serde; empty branch_path → silent hash collision |

**What does NOT qualify:**
- Orchestrator state transitions (`worklist.push`, `covered_paths.insert`) — collection API guarantees these
- Pure functions like `infer_sort()` — wrong output is a logic bug, test with proptest
- Postconditions that restate the return type or the obvious effect of a `map`/`collect`
- Any invariant expressible as a type (prefer `NonEmpty<Vec<T>>` over `#[requires(!v.is_empty())]`)
- Anywhere proptest already covers the invariant — contracts add redundant runtime checks

## Anti-Patterns

- Contracts that restate type signatures — use the type system instead
- Proptest for trivial getters/setters — specific examples are clearer
- PBT that only tests serialization roundtrips without semantic invariants — roundtrips are table stakes, not the goal
- Duplicating generators across test files — use shared generators in `test_arbitraries.rs` / `arbSymExpr` / `genTypeInfo`
