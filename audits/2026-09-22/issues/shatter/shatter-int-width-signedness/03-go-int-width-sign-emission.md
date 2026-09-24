---
slug: go-int-width-sign-emission
kind: new
title: "Go analyzer: emit int_width/int_signed for every integer kind and retire the go_uint/go_byte complex kinds; int8/int16/int32 params currently get out-of-range values"
priority: P2
type: bug
labels: [go-frontend, protocol, parity, input-generation, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [core-int-range-i128]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go analyzer: emit int_width/int_signed for every integer kind; retire go_uint/go_byte

Step 3 of epic `int-width-signedness-epic`.

## Problem

The protocol's `kind: "int"` already carries `int_width` and `int_signed`
(`protocol/schemas/type-info.schema.json:18-25`), and the Rust frontend fills them in. The Go
analyzer does not:

- Signed kinds map to a bare `{"kind":"int"}`. This covers `int8`, `int16`, `int32`/`rune`, `int64`
  and `int` (`shatter-go/protocol/analyzer.go:1487-1489`, `:1504-1506`, `:1559-1561`). The core
  therefore treats an `int8` parameter as full `i64` and generates values like `i64::MAX` and
  `-1000`, which `json.Unmarshal` rejects before the target runs.
- Unsigned kinds use two Go-specific complex kinds instead of the protocol field. `uint`, `uint16`,
  `uint32`, `uint64` and `uintptr` become `go_uint` (`analyzer.go:1499-1502`, `:1562-1565`), and
  `uint8`/`byte` becomes `go_byte` (`:1490-1496`, `:1555-1557`). Each has its own generator and
  mutator in the core (`shatter-core/src/input_gen.rs:997`, `:2992`), outside the `int_range` path
  that the solver's range assertions use (`solver.rs:170-179`).

So signedness and width are described two ways, depending on the frontend. Go's `uint16` and
`uint32` get `u64`-range values, and fixes to one path (str-ddxe, `int-unsigned64-clamp`) do not
reach the other.

## Acceptance criteria

- [ ] Before any change, record the current behaviour on main with a Go fixture that has `int8`,
  `int16`, `uint16`, `uint32`, `uint64` and `byte` params: paste the explore output showing
  out-of-range or unmarshal-rejected inputs.
- [ ] Both Go mapping sites (`basicTypeInfo` and the name-based switch) emit
  `{"kind":"int","int_width":W,"int_signed":S}` for every Go integer kind. `int`, `uint` and
  `uintptr` use the target's word size (64 on supported platforms; state the assumption).
- [ ] The core accepts `go_uint`/`go_byte` as deprecated aliases, mapping them to
  `Int { 64, false }` / `Int { 8, false }`, with a test and a SPEC §8 changelog row marking them
  deprecated. The aliases exist only so that an older installed Go frontend still works with a
  newer core. Before closing, file a follow-up issue "Remove go_uint/go_byte aliases" (P3,
  go-frontend), blocked on the first continuous release that ships this change, and put its id in
  the close reason.
- [ ] `protocol/parity-matrix.yaml` gains an "integer width and signedness" capability row: Rust
  and Go emit it; TS is marked n/a with the reason. A conformance case per frontend asserts the
  emitted TypeInfo for representative params. `task parity` and `task conformance` pass.
- [ ] Known-answer E2E in `shatter-core/tests/e2e_concolic_go.rs`, run with `task e2e-go` because
  cases are `#[ignore]`d, with the summary line showing `0 ignored` pasted. The fixture covers
  `int8`/`uint16`/`uint64`/`byte`: zero unmarshal failures, and branches at each type's min and max
  are found. The test fails on main.
- [ ] `shatter-go/CLAUDE.md` is updated for the protocol-visible change.

## Out of scope

TS integer typing. Go `complex64`/`complex128`. Classifying unmarshal failures as input rejections
(str-4yc9w / `rust-input-deserialize-classification`).

## Size

M

## References

str-cfsa (closed, introduced `go_uint`), str-ddxe (closed, protocol fields), `core-int-range-i128`
(prerequisite: `go_uint` already emits `u64::MAX`, so moving Go onto the int path before the core
can represent it would regress Go's boundary coverage).
