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
