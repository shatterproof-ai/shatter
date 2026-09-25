---
slug: core-go-alias-int-range
kind: new
title: "Core: treat incoming go_uint/go_byte TypeInfo as Int{64,unsigned}/Int{8,unsigned} on the int_range path (no Go frontend change)"
priority: P2
type: task
labels: [input-generation, solver, protocol, go-frontend, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [core-int-range-i128]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core: treat incoming go_uint/go_byte TypeInfo as unsigned Int on the int_range path

Step 3 of epic `int-width-signedness-epic`. Owner: `shatter-core`. This issue changes no Go code
and no protocol-visible frontend output. All paths are relative to the shatter repo root
(github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef; no non-audit file
has changed between 16794cef and the audit checkout's HEAD.

## Problem

Today's `shatter-go` describes unsigned integers with two Go-only complex kinds instead of the
protocol's `int_width`/`int_signed` fields. Observed on 2026-09-24 by sending `analyze` over stdio
to the audit checkout's `shatter-go/bin/shatter-go`:

- `func AtMaxUint64(n uint64)` → `{"name":"n","type":{"kind":"complex","complex_kind":"go_uint"}}`
- `func AtMaxByte(b byte)` → `{"name":"b","type":{"kind":"complex","complex_kind":"go_byte"}}`
- `func FirstByte(b []byte)` → `{"kind":"array","element":{"kind":"complex","complex_kind":"go_byte"}}`
- `func AtMaxInt8(n int8)` → bare `{"kind":"int"}` (signed kinds; not this issue)
- the handshake declares `complex_type:go_byte` (also `protocol/conformance/golden/handshake/go.json:9`,
  `shatter-go/protocol/handler.go:306`).

In the core these kinds (`ComplexKind::GoByte`/`GoUint`, `shatter-core/src/types.rs:106`, `:113`)
have their own generators, mutators and serializer branches (`input_gen.rs:677-678`, `:979`,
`:997-1010`, `:2563-2564`, `:2976`, `:2992`; `orchestrator.rs:1151-1175`). They sit outside
`TypeInfo::int_range()`, boundary seeding (`boundary_dict.rs:54`) and the solver's range
assertion (`solver.rs:172`). Any fix to the int path (`int-unsigned64-clamp`,
`core-int-range-i128`) does not reach them, and they carry their own clamping rules
(`go_byte` wraps with `rem_euclid(256)`; `go_uint` floors negatives to 0).

After `core-int-range-i128`, `Int { int_width: 64, int_signed: false }` has the exact range
`[0, u64::MAX]`, so the plain int path can represent everything `go_uint` does. This issue routes
the two aliases onto that path in the core. The Go frontend keeps emitting them unchanged. That
lets `go-int-width-sign-emission` change the Go analyzer later without any core work, and keeps an
older installed `shatter-go` working with a newer core.

Requests from core to frontend carry no TypeInfo (`protocol/schemas/request.schema.json`: no
request references `type-info`, `param-info` or `function-analysis`; `value_requirement` has only
a `type_name` string). So normalizing on the core side cannot change what the Go planner sees.

## Contract

- A frontend TypeInfo `{"kind":"complex","complex_kind":"go_uint"}` is treated by the core as
  `Int { int_width: Some(64), int_signed: Some(false) }`; `go_byte` as
  `Int { int_width: Some(8), int_signed: Some(false) }`. This applies wherever such a TypeInfo
  appears, including as an array element (`[]byte` arrives as an array of `go_byte`) and inside
  object fields.
- The normalization happens once, where frontend TypeInfo is ingested, so no generator, mutator,
  shrinker, boundary seeder or solver call sees `ComplexKind::GoUint`/`GoByte`. Doing it in
  `TypeInfo` deserialization (for example a `#[serde(from = ...)]` shim) also covers cached
  analyses; if it is done elsewhere, cached analyses must be covered explicitly.
- Wire values for these params stay plain JSON integers, as today: `concrete_to_json` already
  emits `ConcreteValue::Int` as a bare number.
- `ComplexKind::GoUint`/`GoByte` stay in the enum so existing payloads and the
  `complex_type:go_byte` handshake capability still parse. Their generator and serializer
  branches become unreachable from frontend input; removing them is `go-uint-alias-removal`.
- `go_duration` is unchanged.

## Acceptance criteria

- [ ] **Golden compatibility test.** Capture today's `shatter-go` payloads into a core test
  fixture (for example `shatter-core/tests/fixtures/go-alias/`): the `handshake` response and the
  `analyze` responses for a `uint64` param, a `byte` param and a `[]byte` param. Capture recipe
  (protocol version `0.1.0`; `widths.go` holds `AtMaxUint64(n uint64)`, `AtMaxByte(b byte)` and a
  `[]byte` function):
  ```bash
  cd shatter-go && go build -buildvcs=false -o bin/shatter-go . && cd ..
  P='"protocol_version":"0.1.0"'
  { printf '{%s,"id":0,"command":"handshake","capabilities":[]}\n' "$P"
    for f in AtMaxUint64 AtMaxByte <BytesFn>; do
      printf '{%s,"id":1,"command":"analyze","file":"%s","function":"%s","project_root":"%s"}\n' \
        "$P" "$dir/widths.go" "$f" "$dir"
    done; } | shatter-go/bin/shatter-go
  ```
  Record the capture commit in the fixture's README or header. The test asserts that the core
  accepts the handshake, deserializes each analysis, and that the resulting param TypeInfo is
  `Int { 64, false }`, `Int { 8, false }` and `Array { Int { 8, false } }`. The test fails on the
  branch base (record the failing assertion).
- [ ] **Bounds property test** (proptest, per `/formal-methods-policy`): for params deserialized
  from the golden `go_uint` and `go_byte` TypeInfo, every value produced by generation, mutation,
  shrinking, boundary seeding and solver model extraction is an integer in `[0, u64::MAX]` and
  `[0, 255]` respectively.
- [ ] **Boundaries kept.** A unit test asserts boundary seeding for normalized `go_uint` includes
  `0` and `u64::MAX`, and for `go_byte` includes `0` and `255`. These are what reach
  `n == 18446744073709551615` and `b == 255` today through the `go_uint`/`go_byte` generators.
- [ ] Existing Go E2E still passes: `task go:build && cargo test --test e2e_concolic_go`,
  including the str-79nvf/str-ieuc byte-slice coverage (`examples/go/06-byte-slice.go`). Paste the
  `test result:` line.
- [ ] No Go source, `protocol/registry.yaml`, `protocol/parity-matrix.yaml` or handshake golden
  change. `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Any change to the Go analyzer, planner or handler (`go-int-width-sign-emission`).
- Deleting the alias kinds and their dead generator code (`go-uint-alias-removal`).
- Signed Go ints, which arrive as bare `{"kind":"int"}` (`go-int-width-sign-emission`).
- `export.rs:513`/`:589`: these key on a `__complex_type: "go_byte"` value tag, which
  `concrete_to_json` never emits for `GoByte` (it returns a plain integer), so they are unaffected.

## Size

S. One ingestion point in `shatter-core`, one golden fixture, one property test and one unit test.

## References

- `core-int-range-i128`: prerequisite. Before it, `Int { 64, false }` is clamped to
  `[0, i64::MAX]` and routing `go_uint` onto it would lose the `u64::MAX` boundary.
- str-cfsa (closed): introduced `go_uint`.
- str-ieuc (closed): `go_byte` coercion.
- str-79nvf (closed): Go planner `[]byte` detection from a `go_byte` element (Go-side; unaffected).
- No existing tracker issue covers this (see the epic's tracker table).
