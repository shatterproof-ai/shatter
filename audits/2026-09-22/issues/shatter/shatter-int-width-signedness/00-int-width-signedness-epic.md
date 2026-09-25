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

This epic sits under the shatter audit epic "Epic: Audit 2026-09-22 findings". Its children are
`int-unsigned64-clamp`, `core-int-range-i128`, `go-int-width-sign-emission` and the deferred
cleanup `go-uint-alias-removal`. Each names this epic through `parent_slug`.

All paths are relative to the shatter repository root (github.com/shatterproof-ai/shatter). Line
numbers were verified at commit 16794cef.

## Tracker status (checked 2026-09-24)

These drafts are new. `bd search` for `int_range`, `int_width`, `int8`, `i128`, `go_uint` and
`boundary_dict` finds nothing. `unsigned` and `usize` find only the two closed predecessors below.
The audit parent epic is created in the same filing batch as these drafts.

| id | status | relation |
|---|---|---|
| str-ddxe | closed (P2 bug) | Added `int_width`/`int_signed` and `int_range`, but left 64/128-bit unconstrained. This epic finishes that work. |
| str-cfsa | closed (P1 bug) | Introduced Go's `go_uint` complex kind. Retired by `go-int-width-sign-emission`. |
| str-ieuc | closed (P1 bug) | Introduced Go's `go_byte` value coercion. Retired by `go-int-width-sign-emission`. |
| str-79nvf | closed (P2 task) | Planner recognizes `[]byte` from a `go_byte` element. Must keep working after `go_byte` is retired. |
| str-4yc9w | open, started (P1 bug) | Go param-decode failures are classified as completed runs. Related to reporting, not a child. |
| str-qwua7.14 | closed (P1 bug) | Rust walkthrough analyzer and harness param-type disagreement. Background only. |

## Why

The protocol can already describe an integer's width and signedness. `kind: "int"` carries the
optional `int_width` (8/16/32/64/128) and `int_signed`
(`protocol/schemas/type-info.schema.json:18-25`, added by str-ddxe). The rest of the system does not
use that information consistently:

- **The core cannot represent the ranges.** `TypeInfo::int_range() -> Option<(i64, i64)>`
  (`shatter-core/src/types.rs:315`, `:332`) returns `None` for 64- and 128-bit widths, and every
  consumer reads `None` as "any i64". So `usize`/`u64`/`u128` parameters get negative inputs.
- **Boundary seeding ignores the range entirely.** `boundary_dict::get_boundary_values`
  (`shatter-core/src/boundary_dict.rs:54`) returns the same `int_boundaries()` list (`:109`) for
  every `Int`, including `-1`, `-2` and `i64::MIN`. So even a `u8` parameter gets `-1` today.
- **Frontends disagree on how to say it.** The Rust analyzer emits `int_width`/`int_signed`
  (`shatter-rust/src/analyzer.rs:791`, `:1290`). The Go analyzer maps unsigned types to the
  complex kinds `go_uint`/`go_byte` (`shatter-go/protocol/analyzer.go:1496`, `:1502`, `:1558`,
  `:1565`). It maps every signed integer to a bare `{"kind":"int"}` (`:1489`, `:1506`, `:1561`), so
  `int8`/`int16`/`int32` get values that `json.Unmarshal` rejects. TS has no integer types.

These are the baselines on 16794cef (debug builds, default random explorer, 60 iterations). The
fixtures and commands are in the children:

- Rust: 41 of 360 executed inputs were out of bounds. Examples are `usize`/`u64`/`u128` values of
  -1, -8, -946 and `i64::MIN`, and `u8` values of -1 and -2.
- Go: 80 of 360 executed inputs were out of bounds. Examples are `int8` values of 128, 671 and
  `i64::MIN`; `uint16` values of 65536 and `u64::MAX`; `byte` 256; `int16` `i64::MAX`; and
  `null` for `uint16`/`uint64`.

## Children, in dependency order

1. `int-unsigned64-clamp` (P2): on the existing i64 path, clamp unsigned 64/128-bit to
   `[0, i64::MAX]` and make boundary seeding respect `int_range`. This fixes negative inputs now.
2. `core-int-range-i128` (P3, blocked by 1): carry parameter ranges and values as `i128` so that
   every width up to 64 bits gets its exact range, including `u64::MAX`.
3. `go-int-width-sign-emission` (P2, blocked by 2): Go emits `int_width`/`int_signed` for every
   integer kind. The core keeps `go_uint`/`go_byte` only as deprecated aliases. This step adds a
   parity-matrix row and conformance cases. It is blocked by 2 because `go_uint` already generates
   `u64::MAX`, and moving Go to the plain int path earlier would lose that boundary.
Follow-up, not a child: `go-uint-alias-removal` (P3, blocked by 3, parented to the audit epic)
removes the aliases once the compatibility window has passed. It is a deferred cleanup with its own
release-based trigger, so it is kept out of this epic and cannot hold the epic open.

## Related, not a child

`rust-input-deserialize-classification` (bucket shatter-frontend-rust) makes inputs that a harness
cannot deserialize be recorded as input rejections, not as target `throws`. It is standalone
defence in depth under the audit epic. This epic neither waits for it nor depends on its
reporting. Every proof here asserts on the generated input values themselves.

## Done when

- `int-unsigned64-clamp`, `core-int-range-i128` and `go-int-width-sign-emission` are closed, each
  with its own proof.
- On the branch that closes `go-int-width-sign-emission`, the bounds checkers from children 1 and
  3 report `out of bounds: 0` over the committed fixtures `examples/rust/int-width` and
  `examples/go/int-width`, with both the default and the `--concolic` explorer. Paste the four
  summary lines in the epic close reason.


## Out of scope

- Wrapping and overflow arithmetic semantics (Z3 bit-vectors).
- Values outside the 64-bit ranges: full `u128`/`i128`, and the wire string encoding they would
  need. `core-int-range-i128` records that decision.
- Integer `ConstValue` literals above `i64::MAX` in path constraints.
- Z3 range assertions for integers nested inside objects or arrays. Today only top-level params are
  asserted (`solver.rs:172-180`).
