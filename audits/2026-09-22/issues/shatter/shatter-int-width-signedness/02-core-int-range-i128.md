---
slug: core-int-range-i128
kind: new
title: "Core: represent full u64 and i64 integer ranges exactly (i128 internally) for generation, mutation, shrinking, boundaries and Z3 model extraction"
priority: P3
type: feature
labels: [input-generation, solver, protocol, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [int-unsigned64-clamp]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core: represent full u64 and i64 integer ranges exactly (i128 internally)

Step 2 of epic `int-width-signedness-epic`.

## Problem

After `int-unsigned64-clamp`, unsigned 64-bit parameters are clamped to `[0, i64::MAX]`. That removes
negative inputs, but the upper half of `u64`/`usize`, including the `u64::MAX` and `u64::MAX - 1`
boundary values, is never generated or solved for. The reason is that the core represents integer
ranges and values as `i64`:

- `shatter-core/src/types.rs:315` and `:332`: `int_range(...) -> Option<(i64, i64)>`.
- `shatter-core/src/sym_expr.rs:68` and `shatter-core/src/solver.rs:37`: `ConstValue::Int(i64)` /
  `Int(i64)`.
- `shatter-core/src/solver.rs:170-179`: range assertions built with `Int::from_i64`.
- `shatter-core/src/input_gen.rs:460-490`: `generate_int` / `generate_int_in_range` work in `i64`.

Go avoids this today only by routing unsigned types through a separate `go_uint` complex kind whose
generator emits raw `u64` JSON numbers (`input_gen.rs:997-1010`). `go-int-width-sign-emission` wants
to retire that kind, which is only safe once the core itself can represent the full `u64` range.

## Acceptance criteria

- [ ] **Supported range (the boundary of this issue):** every (width, signed) pair up to 64 bits gets
  its exact range: `u64`/`usize` = `[0, u64::MAX]`, `i64`/`isize` = `[i64::MIN, i64::MAX]`. 128-bit
  types are clamped to their 64-bit counterparts (`u128` → `[0, u64::MAX]`, `i128` → the `i64` range),
  and the doc comment says so. `int_range` returns `Option<(i128, i128)>` (i128 is the internal
  carrier). Unspecified width or signedness keeps today's behaviour (full `i64`).
- [ ] Generation, mutation, shrinking and boundary seeding produce values within the declared range,
  including both endpoints (`u64::MAX`, `i64::MIN`). Every value is emitted as an exact JSON integer
  (`serde_json` `u64` or `i64`). No wire-format change is made: values beyond 64 bits are not
  generated, and SPEC plus the protocol docs record that 128-bit ranges are clamped until a
  string encoding is designed in a separate issue.
- [ ] Z3: range assertions use arbitrary-precision constructors (e.g. `Int::from_str` or
  `from_u64`), and model extraction returns `i128` without saturating or truncating. A test asserts a
  model value above `i64::MAX` survives extraction for a `u64` param.
- [ ] A proptest over `(width, signed)` covers every generator, mutator and shrinker entry point: the
  output is always within range, and each endpoint is reachable. A known-answer E2E on a Rust fixture
  `fn f(n: u64) -> bool { n == u64::MAX }` finds the true branch. It fails on main, and both runs are
  pasted.
- [ ] The E2E also covers `fn g(n: i64) -> bool { n == i64::MIN }` (finds the true branch), and a
  `u128` param never receives a value above `u64::MAX` (asserting the clamp).
- [ ] `task e2e` (all three suites) and `task affected` pass, with `Gates selected` recorded.

## Suggested approach

Introduce a small `IntRange { min: i128, max: i128 }` type and route every `int_range` consumer
through it. Keep `ConstValue::Int` as `i64` for literals mined from source unless a consumer needs
more, and widen only the parameter-value paths first.

## Out of scope

Bit-vector (wrapping and overflow) semantics. TS `bigint`. Values beyond 64 bits and any string
wire encoding for them (follow-up issue if a target needs them).

## Size

M–L
