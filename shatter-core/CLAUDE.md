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

## Property-Based Testing Policy (proptest)

Every non-trivial public function should have proptest coverage. Prioritize:

1. **Trust boundaries first**: serialization roundtrips, FFI bridges (Z3 solver), protocol parsing. These have the highest bug density.
2. **Semantic invariants over structural checks**: "solved values match ParamInfo types" is valuable; "Request serializes to JSON" is table stakes. Both are needed, but semantic properties catch real bugs.
3. **Full pipeline properties**: test compose functions, not just units. `arbitrary constraints → solve → overlay → type-check output` catches integration bugs that per-function tests miss.
4. **State machine properties**: for stateful components (orchestrator worklist, coverage tracking), test ordering, deduplication, convergence, and budget exhaustion.
5. **Mutation contracts**: all input generation (mutate, crossover, shrink) must preserve type contracts and vector length.

Shared generators live in `test_arbitraries.rs` — reuse them. Depth-bound recursive types (SymExpr, TypeInfo) to avoid explosion. Filter NaN from floats (breaks PartialEq roundtrips).

**When NOT to use proptest**: simple getters/setters, thin wrappers, tests where specific examples are clearer.

**Target**: every module with a `proptest!` block should have properties covering its core invariants, not just serialization.

## Contracts Policy (`contracts` crate)

Contracts (`#[requires]`, `#[ensures]`) are active only in debug/test builds. Use them ONLY where Rust's type system cannot express the invariant:

**Where contracts ARE warranted:**
- FFI/trust boundaries (Z3 solver bridge, protocol deserialization)
- Length-preservation invariants across transformations (e.g. overlay_solved_values)
- Index validity that depends on runtime relationships between parameters

**Where contracts are NOT warranted:**
- Type-encoded guarantees (Option, Result, NonEmpty wrappers)
- Single-call-site internal helpers
- Simple transformations where proptest is more valuable
- Postconditions that restate the return type
