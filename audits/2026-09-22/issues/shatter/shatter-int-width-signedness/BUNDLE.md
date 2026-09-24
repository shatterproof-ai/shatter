# Bundle: shatter-int-width-signedness (repo shatter), revision 2

New epic created 2026-09-24 at the maintainer's request. All paths are relative to the shatter repository root (github: shatterproof-ai/shatter, main at 16794cef). Child 4 (rust-input-deserialize-classification) lives in bucket shatter-frontend-rust and is included for context.

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


---

---
slug: int-unsigned64-clamp
kind: new
title: "Unsigned 64/128-bit ints (usize, u64, u128) still get negative inputs: int_range() returns None for widths beyond i64; clamp them to [0, i64::MAX]"
priority: P2
type: bug
labels: [rust-frontend, input-generation, solver, audit]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Unsigned 64/128-bit ints (usize, u64, u128) still get negative inputs: clamp them to [0, i64::MAX]

Step 1 of epic `int-width-signedness-epic`. This is the minimal core fix, on the existing i64 data path. Full u64/i64 ranges are step 2 (`core-int-range-i128`).

## Problem

Closed str-ddxe added `int_width` / `int_signed` to `TypeInfo::Int` and in-range generation plus Z3 range assertions for sized ints. But the shared helper every consumer uses, `shatter_core::types::int_range`, deliberately returns `None` for any width whose bounds do not fit in `i64`, which includes `u64`, `u128` and `usize` (and `i128`/`isize`). Random generation, mutation, shrinking, the boundary/candidate path and the solver all treat `None` as "unconstrained full i64". So a `usize` parameter still gets negative values, including `i64::MIN`. The Rust harness rejects them at deserialization, and the report shows the rejections as `throws runtime_error` rows, i.e. as target behavior.

This is a known, documented limitation of the str-ddxe fix (its own doc comment says 64-bit ranges "stay unconstrained"), not a regression. str-ddxe's u8 E2E gate never exercised it. `usize` is the most common Rust integer parameter type, so the gap is large in practice.

## Evidence

Re-verified on shatter `main` at commit 16794cef (line numbers refer to that commit):

- `shatter-core/src/types.rs:310-345`: `TypeInfo::int_range` / `fn int_range(width, signed)` return `Some` only for 8/16/32-bit widths; the arm `// 64-bit and 128-bit ranges exceed (or fill) i64; leave unconstrained.` returns `None`.
- Consumers that fall back to full i64 on `None`: `shatter-core/src/input_gen.rs:128` (`generate_int`), `:225`, `:1955` (`mutate_int`), `:3576` (`shrink_int`), `:4020`, and `shatter-core/src/solver.rs:170-174` (Z3 range assertions).
- The analyzer maps `usize` correctly: `shatter-rust/src/analyzer.rs:791` `"usize" => Some((64, false))`, `analyzer.rs:1290` `"usize" => int_type(64, false)`.
- Reproduction (audit goals run, finding goals-15). Fixture: `standalone/rust/18_accept_language.rs` in the shatter-examples repo at snapshot `49984f4b974bf937e7e6a98e26a7bc205ddee8e2` (the checkout `scripts/examples_checkout.py` produces), function `fn parse_language_preference(part: &str, order: usize) -> Option<LanguagePreference>` at line 41. An audit run (transcript not committed; the build used was not recorded, so treat the numbers as illustrative, and the first acceptance criterion produces the pinned reproduction) showed 16 paths, 12 of them `throws runtime_error: input 1 deserialization failed: invalid value: integer `-998`, expected usize` with values -998, -44, -1, -644, -838, -9223372036854775808, -690, -926, -301, -945, -16, -905. The exact `shatter` / `shatter-rust` build used was not recorded; an earlier verifier run reported 23 rows / 19 negative with a release build.

## Acceptance criteria

- [ ] A minimal fixture is committed with this issue's branch (not only in the external examples repo): e.g. `fn pick(order: usize) -> u8 { if order > 3 { 1 } else { 0 } }` plus `u64` and `u128` variants. Before changing code, run it with a build of main (record `git rev-parse HEAD` and the `shatter --version` / `shatter-rust` binary path used) and paste the output showing negative inputs.
- [ ] Unsigned 64- and 128-bit ints get a lower bound of 0 everywhere `int_range` is consulted: generation, mutation, shrinking, boundary/candidate seeding and the Z3 range assertion. Either extend the helper's return type (e.g. separate optional min/max) or return `(0, i64::MAX)` for unsigned widths ≥ 64, and document the choice in the helper's doc comment. Signed 64/128-bit stay full range.
- [ ] Proptest over `TypeInfo::Int { int_signed: Some(false), int_width }` for widths 8/16/32/64/128 and the `usize` mapping: every generator, mutator and shrinker entry point, and a solver model under the range assertion, produce values ≥ 0 and within width. The test must fail on main for width 64 (paste the failing output).
- [ ] E2E known-answer test in `shatter-core/tests/e2e_concolic_rust.rs` using the committed fixture asserts zero `deserialization failed` rows for the usize/u64 params. Show it failing on main and passing on the branch with `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust <name> -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste both `test result:` lines.
- [ ] `shatter-llm`'s parser width check (`shatter-llm-parse-validation`) consumes the corrected helper; post a comment on that issue when this lands.

## Suggested approach

Change `int_range` to return `(0, i64::MAX)` for unsigned widths ≥ 64 (values above `i64::MAX` cannot be represented in the current `Value::Number` i64 paths anyway), then audit the five `input_gen.rs` call sites and `solver.rs:170-174` for assumptions that `Some` implies width ≤ 32. Check the TS/Go frontends' use of the same helper (Go `uint64`) per the parity rule.

## Out of scope

- Classifying harness input-deserialization failures as tool errors instead of target throws (`rust-input-deserialize-classification`).
- Values above `i64::MAX` for `u64`/`u128` (representation change): `core-int-range-i128`.
- Go's separate `go_uint`/`go_byte` complex kinds: `go-int-width-sign-emission`.

## Size

S

## References

- Finding goals-15 (audit 2026-09-22). Old draft: `drafts/shatter-code/82-rust-usize-negative-inputs.md`.
- Related: str-ddxe (closed; introduced the helper and the 64-bit exclusion; note `rust-usize-reopen-note`), str-qwua7.14 (closed; walkthrough param-type disagreement), str-4yc9w (open, Go counterpart of the misclassification), str-cfsa (closed, earlier Go counterpart).


---

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


---

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


---

---
slug: rust-input-deserialize-classification
kind: new
title: "Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors"
priority: P2
type: bug
labels: [rust-frontend, reporting, audit]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors

## Problem

When the Rust harness cannot deserialize an input into the parameter's type (`input N deserialization failed: invalid value: integer `-998`, expected usize`), the target function never runs. Explore/scan output nevertheless records the row as `throws runtime_error: ...`, which reads as a behavior of the target. Any input the generator gets wrong (see `int-unsigned64-clamp`, but also any future type mismatch) becomes a fake finding, and the walkthrough error regex does not match "deserialization failed", so no gate notices.

## Evidence

- Audit transcript `audits/2026-09-22/goals-runs/rust-walk.md` lines 159-171 (untracked in the audit worktree): 12 of 16 `parse_language_preference` rows are `throws runtime_error: input 1 deserialization failed: ...`.
- Go has the same class of problem tracked as str-4yc9w (open); str-cfsa (closed) was an earlier Go counterpart.

## Acceptance criteria

- [ ] Locate where the Rust harness produces the `input N deserialization failed` error (generated harness code in `shatter-rust/src/executor.rs` / `shatter-rust-runtime`) and give it a distinct `thrown_error.error_type` (e.g. `input_error`) or a distinct execute-result outcome, consistent with whatever str-4yc9w chooses for Go. Name the sites in the close note.
- [ ] The core and report layers treat that outcome as a tool/input error: it is not counted as a target behavior/finding in explore and scan output, and it is counted in the run's error summary.
- [ ] Test: a harness-level unit test feeding a negative integer to a `usize` param asserts the new classification, and a report-level test asserts the row is not rendered as `throws`. Show them failing on main.
- [ ] If the classification is protocol-visible (new `error_type` value), update `protocol/parity-matrix.yaml` and `shatter-rust/CLAUDE.md`, and run `task parity` + `task conformance`.

## Out of scope

- Fixing the generator bug that currently produces most of these rows (`int-unsigned64-clamp`).
- The Go-side change (str-4yc9w), except for agreeing on the shared classification.

## Size

S

## References

- Split from `int-unsigned64-clamp` after the Codex cross-check of the 2026-09-22 audit (finding goals-15).
- Related: str-4yc9w (open, Go), str-cfsa (closed, Go).


---

