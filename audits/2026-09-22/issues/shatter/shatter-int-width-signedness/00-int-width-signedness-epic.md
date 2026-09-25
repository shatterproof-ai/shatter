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
exactly the four drafts that name it through `parent_slug`: `int-unsigned64-clamp`,
`core-int-range-i128`, `core-go-alias-int-range` and `go-int-width-sign-emission`. The deferred
cleanup `go-uint-alias-removal` is a follow-up, not a child: it is parented to the audit epic.

All paths are relative to the shatter repository root (github.com/shatterproof-ai/shatter). Line
numbers were verified at commit 16794cef.

## Tracker status (checked 2026-09-24)

These drafts are new. `bd search` for `int_range`, `int_width`, `int8`, `i128`, `go_uint` and
`boundary_dict` finds nothing. `unsigned` and `usize` find only the two closed predecessors below.
The audit parent epic is created in the same filing batch as these drafts.

| id | status | relation |
|---|---|---|
| str-ddxe | closed (P2 bug) | Added `int_width`/`int_signed` and `int_range`, but left 64/128-bit unconstrained. This epic finishes that work. |
| str-cfsa | closed (P1 bug) | Introduced Go's `go_uint` complex kind. Made a core-side alias by `core-go-alias-int-range`; no longer emitted after `go-int-width-sign-emission`; deleted by `go-uint-alias-removal`. |
| str-ieuc | closed (P1 bug) | Introduced Go's `go_byte` value coercion. Same path as `go_uint`. |
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

1. `int-unsigned64-clamp` (P2, shatter-core): on the existing i64 path, clamp unsigned 64/128-bit
   to `[0, i64::MAX]` and make boundary seeding respect `int_range`. This fixes negative inputs
   now.
2. `core-int-range-i128` (P2, shatter-core, blocked by 1): carry parameter ranges and values as
   `i128` so that every width up to 64 bits gets its exact range, including `u64::MAX`.
3. `core-go-alias-int-range` (P2, shatter-core, blocked by 2): the core treats incoming
   `go_uint`/`go_byte` TypeInfo as `Int { 64, false }`/`Int { 8, false }` on the `int_range` path.
   No Go change. Proof: a golden compatibility test on today's `shatter-go` analyze and handshake
   payloads, plus a bounds property test. It is blocked by 2 because `go_uint` already generates
   `u64::MAX`, and routing it onto the int path before that is representable would lose the
   boundary.
4. `go-int-width-sign-emission` (P2, shatter-go, blocked by 3): the Go analyzer emits
   `int_width`/`int_signed` for every integer kind; the planner, handler, handshake, registry,
   parity matrix and conformance cases adapt. Proof: the repo-local Go fixture and bounds checker,
   an E2E test, and default and `--concolic` reruns. It is blocked by 3 so that the core already
   accepts both wire forms when the Go output changes.

Follow-up, not a child: `go-uint-alias-removal` (P3, blocked by 4, parented to the audit epic)
removes the aliases from the core once the compatibility window has passed. It is a deferred
cleanup with its own release-based trigger, so it is kept out of this epic and cannot hold the
epic open.

## Related, not a child

`rust-input-deserialize-classification` (bucket shatter-frontend-rust) makes inputs that a harness
cannot deserialize be recorded as input rejections, not as target `throws`. It is standalone
defence in depth under the audit epic. This epic neither waits for it nor depends on its
reporting. Every proof here asserts on the generated input values themselves.

## Done when

- All four children are closed, each with its own proof: `int-unsigned64-clamp`,
  `core-int-range-i128`, `core-go-alias-int-range` and `go-int-width-sign-emission`.
- On the branch that closes `go-int-width-sign-emission`, the bounds checkers from children 1 and
  4 report `out of bounds: 0` over the committed fixtures `examples/rust/int-width` and
  `examples/go/int-width`, with both the default and the `--concolic` explorer. Paste the four
  summary lines in the epic close reason.
- `go-uint-alias-removal` is not required; the epic closes without it.

## Out of scope

- Wrapping and overflow arithmetic semantics (Z3 bit-vectors).
- Values outside the 64-bit ranges: full `u128`/`i128`, and the wire string encoding they would
  need. `core-int-range-i128` records that decision.
- Integer `ConstValue` literals above `i64::MAX` in path constraints.
- Z3 range assertions for integers nested inside objects or arrays. Today only top-level params are
  asserted (`solver.rs:172-180`).

## Superseded material

- The audit's older `audits/2026-09-22/drafts/` tree is superseded by `audits/2026-09-22/issues/`.
  Each `drafts/*/INDEX.md` opens with a SUPERSEDED banner, and nothing under `drafts/` has been or
  will be filed.
- `drafts/shatter-code/82-rust-usize-negative-inputs.md` (the original negative-`usize` finding,
  goals-15) is replaced by `int-unsigned64-clamp`. Do not file it.
