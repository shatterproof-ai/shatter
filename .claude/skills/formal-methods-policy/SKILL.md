---
name: formal-methods-policy
description: Shatter's formal methods and property-based testing policy. Use when writing or reviewing tests, adding new public functions, deciding whether to use PBT / contracts / fuzzing, or picking an invariant to assert. Covers proptest (Rust), fast-check (TS), rapid (Go), native fuzzing, and the contracts crate policy.
user-invocable: true
---

# Formal Methods & Verification Policy

Four complementary tools, each with a distinct role. **PBT is the workhorse**; the others fill gaps PBT can't reach.

| Tool | Role | When to use |
|---|---|---|
| **Property-based testing** (proptest / fast-check / rapid) | Invariant discovery, regression prevention | Any non-trivial public function with invariants |
| **Native fuzzing** (Go `testing.F`, `cargo-fuzz`) | Crash resistance at parsing boundaries | Code that deserializes untrusted input |
| **Contracts** (`contracts` crate, `#[requires]`/`#[ensures]`) | Runtime assertions at trust boundaries | Only where Rust's type system can't express the invariant — very high bar |
| **Kani model checking** (deferred — P4) | Exhaustive verification | Highest-stakes properties only (solver correctness). Not yet in use. |

## Property-Based Testing (primary strategy)

PBT is not optional decoration. When adding or modifying a public function, add property tests that cover its core invariants. Prioritize:

1. **Trust boundaries first** — serialization roundtrips, FFI bridges (Z3 solver), protocol parsing. Highest bug density.
2. **Semantic invariants over structural checks** — "solved values match ParamInfo types" is valuable; "Request serializes to JSON" is table stakes. Both are needed, but semantic properties catch real bugs.
3. **Full pipeline properties** — test compose functions, not just units. `arbitrary constraints → solve → overlay → type-check output` catches integration bugs.
4. **State machine properties** — for stateful components (orchestrator worklist, coverage tracking), test ordering, deduplication, convergence, and budget exhaustion.
5. **Mutation contracts** — input generation (mutate, crossover, shrink) must preserve type contracts and vector length.

**Shared generators**: reuse `test_arbitraries.rs` (Rust), `arbSymExpr`/`arbTypeInfo` (TS), `genTypeInfo` (Go). Don't reinvent type generators per test file. Depth-bound recursive types (SymExpr, TypeInfo) to avoid explosion. Filter NaN from floats (breaks PartialEq roundtrips).

**When NOT to use PBT**: simple getters/setters, thin wrappers, tests where specific examples are clearer.

**Coverage target**: every module that handles untrusted input, crosses an FFI boundary, or maintains state should have PBT coverage of its core invariants — not just serialization.

## Native Fuzzing

- **Go**: `testing.F` in `*_fuzz_test.go` — byte-level mutation for crash/panic discovery at parsing boundaries. Seed corpus from existing test fixtures.
- **Rust**: `cargo-fuzz` for deserialization boundaries.
- Add a fuzz target for any code that deserializes untrusted input (protocol messages, subprocess JSON).

## Contracts (`contracts` crate)

Very high bar. Use `#[requires]`/`#[ensures]` only where **all three** hold:

1. **Trust boundary** — data from outside Rust's type system (Z3 FFI, deserialized JSON, cross-collection indices).
2. **Type gap** — invariant cannot be encoded in the function signature. Prefer `Option`, `Result`, `NonZeroU32`, enums, newtypes.
3. **Silent corruption** — violation doesn't produce an immediate obvious error (panic, Z3 sort mismatch) but propagates as wrong results downstream.

**Practical rule**: if you can't point to a real or plausible bug the contract would catch that proptest wouldn't, skip it.

For the authoritative list of qualifying sites and detailed justifications, see `shatter-core/CLAUDE.md` under "Contracts Policy". The qualifying sites as of this writing: `solve_for_new_path()` (negate_index bounds + solved value types), `overlay_solved_values()` (cross-collection alignment), `to_z3_expr()` (Param index validity), and protocol validation wrappers (post-serde semantic checks).

## Anti-Patterns

- Contracts that restate type signatures — use the type system instead
- Proptest for trivial getters/setters — specific examples are clearer
- PBT that only tests serialization roundtrips without semantic invariants — roundtrips are table stakes, not the goal
- Duplicating generators across test files — use shared generators

## Per-Language Quick Reference

- **Rust** (proptest): `shatter-core/src/test_arbitraries.rs` holds shared generators. Use `proptest!` blocks with depth-bounded recursive strategies.
- **TypeScript** (fast-check): `shatter-ts/src/property.test.ts`. Use `fc.letrec` for recursive types. Shared generators: `arbSymExpr`, `arbTypeInfo`, `arbSideEffect`.
- **Go** (rapid + `testing.F`): `shatter-go/protocol/property_test.go` and `instrument/property_test.go` for semantic properties; `*_fuzz_test.go` for byte-level fuzzing.

## Where to go deeper

- `shatter-core/CLAUDE.md` — per-module PBT priorities, authoritative contracts qualifying site list
- `shatter-ts/CLAUDE.md` — SymExpr builder parity as a PBT target
- `shatter-go/CLAUDE.md` — rapid + native fuzzing combo
- Root `CLAUDE.md` — test tiers, E2E gate, completion checklist
