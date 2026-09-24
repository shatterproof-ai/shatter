---
slug: int-width-signedness-epic
kind: new
title: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
priority: P2
type: epic
labels: [protocol, input-generation, solver, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Epic: integer width and signedness end-to-end

This epic is a child of the shatter audit epic "Epic: Audit 2026-09-22 findings". Its four children name it as their parent.

## Why

The protocol can already describe an integer's width and signedness. `kind: "int"` carries the
optional `int_width` (8/16/32/64/128) and `int_signed`
(`protocol/schemas/type-info.schema.json:18-25`, added by str-ddxe). The rest of the system does not
use that information consistently:

- **The core cannot represent the ranges.** Integer ranges and values are `i64` throughout
  (`shatter-core/src/types.rs:315` and `:332`, `int_range() -> Option<(i64, i64)>`;
  `SymExpr::Const(Int(i64))`; the solver's `Int(i64)`). `int_range` returns `None` for 64- and 128-bit
  widths, and every consumer reads `None` as "any i64". The consumers are generation, mutation and
  shrinking (`input_gen.rs:128, 225, 1955, 3576, 4020`) and the Z3 range assertion
  (`solver.rs:170-179`). As a result, `usize`/`u64`/`u128` parameters get negative inputs: the
  Rust harness rejects them at deserialization, and the rejections are reported as target
  `throws runtime_error`.
- **Frontends disagree on how to say it.** The Rust analyzer emits `int_width`/`int_signed`
  (`shatter-rust/src/analyzer.rs:791`, `:1290`). The Go analyzer never does. It maps unsigned types
  to separate complex kinds, `go_uint` and `go_byte` (`shatter-go/protocol/analyzer.go:1490-1502`,
  `:1555-1565`), each with its own generator and mutator in the core (`input_gen.rs:997`, `:2992`).
  It maps every signed Go integer (`int8`, `int16`, `int32`/`rune`, `int64`, `int`) to a bare
  `{"kind":"int"}` with no width, so `int8`/`int16`/`int32` parameters also get out-of-range values.
  TS has no integer types (`number` is float; `bigint` is the `big_int` complex kind).

## Children, in dependency order

1. `int-unsigned64-clamp`: clamp unsigned ≥ 64-bit to `[0, i64::MAX]` on the existing i64 path.
   This fixes the negative `usize` inputs now.
2. `core-int-range-i128`: widen core integer ranges and values to `i128` so that `u64` and `i64` are
   fully representable, including the `u64::MAX` boundary. Blocked by 1.
3. `go-int-width-sign-emission`: Go emits `int_width`/`int_signed` for every integer kind and retires
   `go_uint`/`go_byte`, with a one-release alias. A parity-matrix row and conformance tests are added.
   Blocked by 2, because `go_uint` already generates `u64::MAX`, and moving Go onto the plain int path
   before the core can represent that value would regress Go boundary coverage.
4. `rust-input-deserialize-classification` (existing draft): inputs a harness cannot deserialize
   are recorded as input rejections, not target behaviours. It stands alone and is defence in depth,
   so any future type-mapping gap costs budget instead of polluting the spec.

## Done when

All children are closed with their own proof. Then a single known-answer E2E per frontend (Rust
`usize`/`u8`/`i8`, Go `uint64`/`int8`/`byte`) shows zero out-of-range inputs and exercises the
type's min and max boundary values. The run output is pasted in the close reason.

## Out of scope

Wrapping and overflow arithmetic semantics (Z3 bit-vectors), and 128-bit values beyond `i128`. On
the wire, 128-bit values may need to travel as decimal strings. That is deferred until a target needs
full `u128` range; `core-int-range-i128` records the decision.
