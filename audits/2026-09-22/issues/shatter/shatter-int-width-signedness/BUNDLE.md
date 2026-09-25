# Bundle: shatter-int-width-signedness (repo shatter), revision 5

All paths are relative to the shatter repo root (github: shatterproof-ai/shatter), checkout at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22; code line numbers verified at 16794cef. 00 is the epic; 01-04 are its children (parent_slug); 05 is a follow-up under the audit epic; the last draft is related, not a child.

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


---

---
slug: int-unsigned64-clamp
kind: new
title: "Sized ints get out-of-range inputs: int_range() returns None for unsigned 64/128-bit, and boundary seeding ignores int_range (usize/u64/u128 get negatives, u8 gets -1)"
priority: P2
type: bug
labels: [rust-frontend, input-generation, solver, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Sized ints get out-of-range inputs: clamp unsigned 64/128-bit to [0, i64::MAX] and make boundary seeding respect int_range

Step 1 of epic `int-width-signedness-epic`. This is the minimal core fix on the existing i64 data
path. Exact `u64` ranges come in step 2, `core-int-range-i128`. All paths are relative to the
shatter repo root (github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef.

## Problem

Closed str-ddxe added `int_width`/`int_signed` and the helper `shatter_core::types::int_range`. By
design, that helper returns `None` for any width whose range does not fit in `i64`, including
`u64`, `u128` and `usize` (`types.rs:311-345`). Every consumer treats `None` as "full i64".

Separately, boundary seeding never consults the helper. `boundary_dict::get_boundary_values`
(`boundary_dict.rs:54`) returns `int_boundaries()` (`:109`) for every `TypeInfo::Int`. That list
includes `0`, `±1`, `±2`, `±3`, `i64::MAX`, `i64::MIN` and powers of two. As a result:

- `usize`/`u64`/`u128` params receive negative values, including `i64::MIN`.
- `u8` params receive `-1` and `-2` from the boundary seeds, even though `int_range` covers `u8`.

The Rust harness rejects these values at deserialization, before the target runs. That wastes
budget. The rejections are also currently reported as target `throws runtime_error` rows, but that
reporting problem is a separate, related issue (`rust-input-deserialize-classification`). This
issue is only about the values generated.

## Expected bounds after this issue

The proof asserts these exact bounds:

| TypeInfo `(int_width, int_signed)` | Rust types | Bounds every generated or solved value must satisfy | Change |
|---|---|---|---|
| `(8, false)` | `u8` | `[0, 255]` | `int_range` unchanged; boundary seeding newly filtered |
| `(8, true)` | `i8` | `[-128, 127]` | as above |
| `(16, false)` | `u16` | `[0, 65535]` | as above |
| `(16, true)` | `i16` | `[-32768, 32767]` | as above |
| `(32, false)` | `u32` | `[0, 4294967295]` | as above |
| `(32, true)` | `i32` | `[-2147483648, 2147483647]` | as above |
| `(64, false)` | `u64`, `usize` | `[0, i64::MAX]` | **new** clamp |
| `(128, false)` | `u128` | `[0, i64::MAX]` | **new** clamp |
| `(64, true)` | `i64`, `isize` | `[i64::MIN, i64::MAX]` | unchanged |
| `(128, true)` | `i128` | `[i64::MIN, i64::MAX]` | unchanged |
| width or signedness absent | Go `int` today, any unsized int | `[i64::MIN, i64::MAX]` | unchanged |

## Entry points that must honour the table

These were verified at 16794cef. Each one gets a test.

1. **Generation:** `input_gen.rs:128` `generate_random_value` calls `generate_int` (`:460`) and
   `generate_int_in_range` (`:478`).
2. **Shape check and regeneration:** `input_gen.rs:225` `value_matches_type_shape`, used by
   `coerce_value_to_type` (`:199`).
3. **Mutation:** `input_gen.rs:1955` `mutate_value` calls `mutate_int` (`:2107`) and
   `clamp_to_range` (`:2138`).
4. **Shrinking:** `input_gen.rs:3576` `shrink_candidates` calls `shrink_int` (`:3596`).
5. **Literal seeding:** `input_gen.rs:4020` in `literal_matches_type` (`:4014`).
6. **Boundary seeding:** `boundary_dict.rs:54` `get_boundary_values` and `int_boundaries` (`:109`)
   must filter by `int_range`. Its consumers are:
   - `generate_boundary_inputs` (`boundary_dict.rs:378`), called from `strategy.rs:507`,
     `scan_orchestrator.rs:3058` and `pipeline_orchestrator.rs:590`;
   - `default_candidate_values` (`input_gen.rs:3793-3798`).

   `interesting_pool.rs:748` only classifies values and needs no change.
7. **Solver range assertion:** `solver.rs:172` `assert_int_param_ranges`, called from
   `solve_for_new_path` (`:1224`), `solve_constraints` (`:1264`) and `solve_for_mcdc_independence`
   (`:1354`).

## Fixture (commit with this issue's branch)

Commit the fixture before changing code. `examples/rust` is excluded from the cargo workspace, and
the manifest only bounds the analyzer's crate-root walk, like `examples/rust/enum-color`. The
`String` guard stops the solver from pinning the integer param early, so random generation,
mutation and boundary seeding all reach it.

`examples/rust/int-width/Cargo.toml`:

```toml
# Known-answer fixture for integer width/signedness (int-width-signedness epic).
# The manifest only bounds the analyzer's crate-root walk; the crate is never built.
[package]
name = "int-width-fixture"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"
```

`examples/rust/int-width/src/lib.rs`:

```rust
// Known-answer fixture for integer width and signedness.
// The String guard keeps the solver from pinning the integer param early,
// so random generation, mutation and boundary seeding all reach it.

pub fn rank_usize(label: String, order: usize) -> u8 {
    if label.starts_with("en") { if order > 3 { 1 } else { 2 } } else { 0 }
}

pub fn rank_u64(label: String, n: u64) -> u8 {
    if label.starts_with("en") { if n > 3 { 1 } else { 2 } } else { 0 }
}

pub fn rank_u128(label: String, n: u128) -> u8 {
    if label.starts_with("en") { if n > 3 { 1 } else { 2 } } else { 0 }
}

pub fn rank_u8(label: String, n: u8) -> u8 {
    if label.starts_with("en") { if n > 3 { 1 } else { 2 } } else { 0 }
}

pub fn rank_i8(label: String, n: i8) -> u8 {
    if label.starts_with("en") { if n > 3 { 1 } else { 2 } } else { 0 }
}

pub fn rank_i64(label: String, n: i64) -> u8 {
    if label.starts_with("en") { if n > 3 { 1 } else { 2 } } else { 0 }
}
```

`scripts/check_int_width_bounds.py` is the bounds checker. It reads explore artifacts only, never
the rendered report:

```python
#!/usr/bin/env python3
"""Assert every executed integer input in int-width fixture artifacts is within its type's bounds.

Usage: check_int_width_bounds.py <shatter-artifacts dir> [--full-u64]
Bounds follow int-unsigned64-clamp; --full-u64 switches to core-int-range-i128's exact u64 range.
"""
import glob, json, sys

I64_MAX = 2**63 - 1
full_u64 = "--full-u64" in sys.argv
U64_HI = 2**64 - 1 if full_u64 else I64_MAX
BOUNDS = {  # function -> (param index, min, max)
    "rank_usize": (1, 0, U64_HI), "rank_u64": (1, 0, U64_HI), "rank_u128": (1, 0, U64_HI),
    "rank_u8": (1, 0, 255), "rank_i8": (1, -128, 127), "rank_i64": (1, -2**63, I64_MAX),
    "at_u64_max": (0, 0, U64_HI), "above_i64_max": (0, 0, U64_HI), "at_i64_min": (0, -2**63, I64_MAX),
}
bad = seen = 0
for f in sorted(glob.glob(sys.argv[1] + "/explore-results/*/*.json")):
    d = json.load(open(f))
    fn = d.get("function_name")
    if fn not in BOUNDS or "observation" not in d:
        continue
    i, lo, hi = BOUNDS[fn]
    for inputs, _mocks, _result in d["observation"]["raw_results"]:
        seen += 1
        n = inputs[i]
        if not (isinstance(n, int) and not isinstance(n, bool) and lo <= n <= hi):
            bad += 1
            print(f"OUT OF BOUNDS {fn}: {n!r}")
print(f"checked {seen} executed inputs; out of bounds: {bad}")
sys.exit(1 if bad or not seen else 0)
```

## Baseline (run on main before changing code)

Run from the repo root. This needs no external examples checkout. Copy the fixture to a temp dir,
because `shatter explore` initializes `.shatter/`, `.gitignore` and `shatter-artifacts/` beside the
crate.

```bash
git rev-parse HEAD
cargo build -p shatter-cli && cargo build --manifest-path shatter-rust/Cargo.toml
tmp=$(mktemp -d) && cp -r examples/rust/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/src/lib.rs" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
python3 scripts/check_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
rm -rf "$tmp/int-width/shatter-artifacts" "$tmp/int-width/.shatter"
target/debug/shatter explore "$tmp/int-width/src/lib.rs" --concolic --allow-host-writes \
  --max-iterations 60 --request-timeout 240
python3 scripts/check_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
```

Expected on main: the checker exits 1. On 2026-09-24 at 16794cef, the default explorer printed
`checked 360 executed inputs; out of bounds: 41`. The rows were negative values for `rank_usize`,
`rank_u64` and `rank_u128` (for example -1, -8, -946 and -9223372036854775808) and `-1`/`-2` for
`rank_u8`. The report shows them as `throws runtime_error: input 1 deserialization failed: invalid
value: integer `-N`, expected usize`. The random values differ between runs. Negatives for the three
unsigned 64/128-bit functions and `-1` for `rank_u8` appear every time.

## Acceptance criteria

- [ ] The fixture and checker above are committed. The baseline output from main is pasted, with
  `git rev-parse HEAD`, and shows the checker failing.
- [ ] `int_range` returns `(0, i64::MAX)` for `(64, false)` and `(128, false)`. Its doc comment
  states the table above and says it is temporary until `core-int-range-i128`. The other rows are
  unchanged.
- [ ] Boundary seeding filters `int_boundaries()` through `int_range`. Out-of-range entries are
  dropped, not clamped to duplicates. `get_boundary_values(Int{8,false})` contains 0, 1, 255 and no
  negatives. The `default_candidate_values` first entry stays in range.
- [ ] **Table-driven unit and property tests in shatter-core, one per entry point 1–7, over every
  table row.** For seeds `0..2000`:
  - every `generate_random_value` output is within bounds;
  - every `mutate_value` output is within bounds, for inputs drawn from the row's bounds and also
    from arbitrary `i64`;
  - every `shrink_candidates` output for an in-bounds value is within bounds;
  - `literal_matches_type` accepts each of `{i64::MIN, -1, 0, 1, 255, 256, i64::MAX}` exactly when
    it is within bounds;
  - every `get_boundary_values`/`generate_boundary_inputs` entry is within bounds;
  - for the solver, `solve_constraints` with `x < 0` on an unsigned row is Unsat, and with `x > 3`
    returns a model within bounds.

  Paste the failing run on main. The rows `(64,false)`, `(128,false)` and the boundary tests for
  `(8,false)` must fail there.
- [ ] **E2E known-answer test** `e2e_rust_int_width_inputs_in_bounds` in
  `shatter-core/tests/e2e_concolic_rust.rs`. It loads the fixture through
  `repo_examples_rust_dir().join("int-width/src/lib.rs")`, so no `SHATTER_EXAMPLES_DIR` is needed.
  For each `rank_*` function it:
  - seeds with `boundary_dict::generate_boundary_inputs(&analysis.params)`, as the CLI does;
  - runs `orchestrator::explore`;
  - asserts every value in `result.raw_results[*].0[1]` is within the table bounds;
  - asserts the `n > 3` branch is taken.

  It must not assert on error rows or report text. Run it with
  `cargo build --manifest-path shatter-rust/Cargo.toml && cargo test --test e2e_concolic_rust e2e_rust_int_width -- --include-ignored`.
  Every case in that file is `#[ignore]`d, so plain `cargo test` runs nothing. Paste the failing
  `test result:` line from main and the passing line from the branch.
- [ ] Re-run the baseline commands on the branch. Both checker runs (default and `--concolic`)
  print `out of bounds: 0`. Paste them. This covers the random explorer, which the E2E does not
  reach.
- [ ] `task e2e` and `task affected` pass, with `Gates selected` recorded. `task e2e` uses
  `python3 scripts/examples_checkout.py` for the other suites.
- [ ] `shatter-llm-parse-validation` is filed with a `blocked_by` on this issue. Post a comment
  there when this issue lands, so its width check uses the corrected helper.

## Suggested approach

Keep the `Option<(i64, i64)>` signature and return `Some((0, i64::MAX))` for unsigned widths of 64
bits or more. Values above `i64::MAX` cannot pass through the current `Value::Number` i64 paths.
Add a small `int_boundaries_in(range)` that filters the static list and appends `min`/`max`, and
`min+1`/`max-1` when they differ. Audit the entry points above for any assumption that `Some`
implies a width of 32 bits or less.

## Out of scope

- Classifying harness deserialization failures (`rust-input-deserialize-classification`, str-4yc9w).
- Values above `i64::MAX` for `u64`/`u128` (`core-int-range-i128`).
- Go's `go_uint`/`go_byte` kinds (`core-go-alias-int-range`) and Go's bare signed `int`
  (`go-int-width-sign-emission`).
- Z3 range assertions for ints nested in objects. Only top-level params are asserted today.

## Size

S–M

## References

- Finding goals-15 (audit 2026-09-22). This replaces the old unfiled draft
  `drafts/shatter-code/82-rust-usize-negative-inputs.md`; the whole `drafts/` tree is superseded
  (SUPERSEDED banners on each `drafts/*/INDEX.md`) and must not be filed.
- str-ddxe (closed): introduced `int_range` and the 64-bit exclusion. See also the note draft
  `rust-usize-reopen-note`.
- str-4yc9w (open): Go misclassification of decode failures.
- str-cfsa (closed): earlier Go unsigned fix.
- str-qwua7.14 (closed): walkthrough param-type disagreement.
- None of these duplicate this draft (see the epic's tracker table).


---

---
slug: core-int-range-i128
kind: new
title: "Core: exact integer ranges for every width up to 64 bits (u64 = [0, u64::MAX]) via an i128 IntRange carrier on the parameter-value paths"
priority: P2
type: feature
labels: [input-generation, solver, protocol, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [int-unsigned64-clamp]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core: exact integer ranges for every width up to 64 bits, via an i128 IntRange carrier

Step 2 of epic `int-width-signedness-epic`. All paths are relative to the shatter repo root
(github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef, before
`int-unsigned64-clamp`, which does not move these sites.

## Problem

After `int-unsigned64-clamp`, `u64`/`usize` params are clamped to `[0, i64::MAX]`. The upper half of
`u64`, including `u64::MAX`, is never generated or solved for, because parameter ranges and values
are `i64`:

- `types.rs:315`, `:332`: `int_range(...) -> Option<(i64, i64)>`.
- `input_gen.rs:460`, `:478`, `:2107`, `:3596`: `generate_int`, `generate_int_in_range`,
  `mutate_int` and `shrink_int` read the value with `as_i64()` and compute in `i64`.
- `boundary_dict.rs:109`: the int boundary list is `i64` only.
- `solver.rs:176-177`: `assert_int_param_ranges` builds bounds with `Int::from_i64`.
- `solver.rs:1023-1030`: `extract_concrete_values` reads the model with `val.as_i64()` and silently
  drops any value that does not fit. `ConcreteValue::Int(i64)` (`solver.rs:37`) and
  `concrete_to_json` (`orchestrator.rs:1146`) carry it onward.

On main, Z3 asked for `n > 9_223_372_036_854_775_807` on a `u64` produced a model the extraction
dropped. The run fell back to `0` and a `-1` boundary seed, and the `upper-half` branch was never
reached.

Go avoids this today only through the separate `go_uint` complex kind, whose generator emits raw
`u64` JSON (`input_gen.rs:997-1010`). `core-go-alias-int-range` routes that kind onto the plain
int path, which is safe only once this issue lands.

## Contract

- **Range carrier:** `pub struct IntRange { pub min: i128, pub max: i128 }`.
  `TypeInfo::int_range()` and `types::int_range()` return `Option<IntRange>`.
- **Promised ranges:** exact for every width of 64 bits or less.

  | (width, signed) | range |
  |---|---|
  | 8/16/32 | exact, as today |
  | `(64, false)` (`u64`, `usize`) | `[0, u64::MAX]` |
  | `(64, true)` (`i64`, `isize`) | `[i64::MIN, i64::MAX]` |
  | `(128, false)` (`u128`) | clamped to `[0, u64::MAX]` |
  | `(128, true)` (`i128`) | clamped to `[i64::MIN, i64::MAX]` |
  | width or signedness absent | `[i64::MIN, i64::MAX]`, unchanged |

  The 128-bit clamp is documented in the `int_range` doc comment and in SPEC. The clamp exists
  because no value outside the 64-bit ranges is ever generated or put on the wire. JSON numbers stay
  `serde_json` `u64` or `i64`, and there is no wire-format change.
- **Paths that widen to i128.** These are all parameter-value paths:
  - generation: `generate_int`, `generate_int_in_range`;
  - mutation: `mutate_int`, `clamp_to_range`;
  - shrinking: `shrink_int`;
  - the shape check: `value_matches_type_shape` must accept `as_u64()` values within range;
  - boundary seeding: `int_boundaries` gains `u64::MAX` and `u64::MAX - 1`, filtered by range;
  - the Z3 parameter range assertion: bounds built from the i128 decimal string, e.g.
    `Int::from_str`, or `Int::from_u64` for the upper bound;
  - Z3 model extraction for integer variables: `extract_concrete_values` parses the model value as
    i128, via `as_u64()` fallback or the decimal string;
  - `ConcreteValue::Int` becomes `i128`;
  - `concrete_to_json` emits `u64` JSON when the value is above `i64::MAX` and `i64` JSON otherwise,
    and never emits a value outside the param's `IntRange`.
- **Paths that stay i64:**
  - `ConstValue::Int(i64)` (`sym_expr.rs:68`): integer literals mined from source and used as
    constants in path constraints, including `SymExpr` → Z3 at `solver.rs:513`;
  - `LiteralValue::Int` (literal seeding). It is compared against the i128 range but stays i64.

  A source literal above `i64::MAX`, such as `u64::MAX` written in code, is therefore not a
  symbolic constant. Its branch is reached through the boundary seeds and the generator endpoints,
  not through Z3. Widening `ConstValue` is out of scope.

## Fixture (append to `examples/rust/int-width/src/lib.rs`, committed by `int-unsigned64-clamp`)

These functions use `if`, because a function that only computes a boolean expression has no branch
and is skipped by explore:

```rust
pub fn at_u64_max(n: u64) -> &'static str {
    if n == u64::MAX { "max" } else { "other" }
}

pub fn above_i64_max(n: u64) -> &'static str {
    if n > 9_223_372_036_854_775_807 { "upper-half" } else { "lower-half" }
}

pub fn at_i64_min(n: i64) -> &'static str {
    if n == i64::MIN { "min" } else { "other" }
}
```

## Baseline (run before changing code, on the tip of `int-unsigned64-clamp` or main)

```bash
git rev-parse HEAD
cargo build -p shatter-cli && cargo build --manifest-path shatter-rust/Cargo.toml
tmp=$(mktemp -d) && cp -r examples/rust/int-width "$tmp"/
for mode in "" --concolic; do
  for f in at_u64_max above_i64_max at_i64_min; do
    rm -rf "$tmp/int-width/shatter-artifacts" "$tmp/int-width/.shatter"
    target/debug/shatter explore "$tmp/int-width/src/lib.rs:$f" $mode --allow-host-writes \
      --max-iterations 60 --request-timeout 240
  done
done
```

Expected at 16794cef (recorded 2026-09-24):

| function | default explorer | `--concolic` |
|---|---|---|
| `at_u64_max` | only `"other"` (from `i64::MAX`), plus negative-input throws | not recorded; expected only `"other"` |
| `above_i64_max` | only `"lower-half"` (input `i64::MAX`) | only `"lower-half"` (input `0`), plus a `-1` boundary-seed throw |
| `at_i64_min` | reaches `"min"` | not recorded |

`at_i64_min` already reaches `"min"`, so it is a regression guard, not a failing case. After
`int-unsigned64-clamp`, the negative throws are gone but `"max"` and `"upper-half"` are still
unreached.

## Acceptance criteria

- [ ] The fixture functions above are committed. The baseline output is pasted with
  `git rev-parse HEAD`.
- [ ] `IntRange` and the i128 value paths are implemented exactly as in **Contract**. The
  `int_range` doc comment and SPEC (plus `protocol/schemas/type-info.schema.json`'s `int_width`
  description) state the ranges and the 128-bit clamp.
- [ ] **Property tests over every (width, signed) row**, extending the table tests from
  `int-unsigned64-clamp` with the new bounds. For the generator, mutator, shrinker, boundary
  seeding and `concrete_to_json`:
  - every output is within `IntRange`;
  - both endpoints are reachable: `u64::MAX` and `0` for `(64,false)`, `i64::MIN` and `i64::MAX`
    for `(64,true)`;
  - `(128,false)` never yields a value above `u64::MAX`;
  - every value serializes as a JSON integer.
- [ ] **Solver tests.** Under the range assertion, `solve_constraints` with `x > 9223372036854775807`
  on a `(64,false)` param returns a model value above `i64::MAX` that survives
  `extract_concrete_values` and `concrete_to_json`. The same constraint on `(64,true)` is Unsat.
  Both fail or misbehave on main.
- [ ] **E2E** `e2e_rust_int_full_range` in `shatter-core/tests/e2e_concolic_rust.rs`, using
  `repo_examples_rust_dir().join("int-width/src/lib.rs")`, so no external checkout is needed. It
  asserts:
  - `at_u64_max` reaches `"max"`;
  - `above_i64_max` reaches `"upper-half"`;
  - `at_i64_min` reaches `"min"`;
  - every executed input is within the new bounds.

  Run it with
  `cargo build --manifest-path shatter-rust/Cargo.toml && cargo test --test e2e_concolic_rust e2e_rust_int_full_range -- --include-ignored`.
  Paste the failing line from the pre-change tip and the passing line.
- [ ] Re-run the baseline loop on the branch. Inside the loop, after each `explore`, add
  `python3 scripts/check_int_width_bounds.py "$tmp/int-width/shatter-artifacts" --full-u64`. Paste output showing `"max"` and `"upper-half"` reached and
  `out of bounds: 0`.
- [ ] `task e2e` (all three suites) and `task affected` pass, with `Gates selected` recorded.

## Out of scope

- Bit-vector (wrapping and overflow) semantics.
- TS `bigint`.
- Widening `ConstValue::Int` or `LiteralValue::Int`.
- Values outside the 64-bit ranges and any string wire encoding for them. File a follow-up only if a
  target needs full `u128`/`i128`.
- Nested-field Z3 range assertions.

## Size

M–L

## References

- str-ddxe (closed): the original helper.
- str-cfsa (closed): `go_uint`, whose `u64::MAX` generation this issue makes available on the plain
  int path.
- No existing tracker issue covers i128 or `IntRange` (`bd search` returns nothing).


---

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


---

---
slug: go-int-width-sign-emission
kind: new
title: "Go analyzer: emit int_width/int_signed for every integer kind instead of bare int and go_uint/go_byte; int8/int16/uint16 params currently get out-of-range values"
priority: P2
type: bug
labels: [go-frontend, protocol, parity, input-generation, audit-2026-09-22]
parent_epic: "Epic: integer width and signedness end-to-end (protocol → core ranges → every frontend)"
parent_slug: int-width-signedness-epic
blocked_by: [core-go-alias-int-range]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go analyzer: emit int_width/int_signed for every integer kind

Step 4 of epic `int-width-signedness-epic`. Owner: `shatter-go` (plus the protocol registry,
parity matrix and conformance files its handshake change touches). The core side, which makes the
old `go_uint`/`go_byte` wire kinds behave as unsigned ints, is the prerequisite
`core-go-alias-int-range`. Removing the aliases from the core is the separate deferred follow-up
`go-uint-alias-removal`. All paths are relative to the shatter repo root
(github.com/shatterproof-ai/shatter). Line numbers were verified at 16794cef.

## Problem

The protocol's `kind: "int"` carries `int_width`/`int_signed`
(`protocol/schemas/type-info.schema.json:18-25`), and the Rust frontend fills them in. The Go
analyzer does not, and the Go `TypeInfo` struct (`shatter-go/protocol/types.go:329`) has no fields
for them.

**Signed kinds** (`int8`, `int16`, `int32`/`rune`, `int64`, `int`) map to a bare `{"kind":"int"}`
(`shatter-go/protocol/analyzer.go:1489`, `:1506` in `basicTypeInfo`; `:1561` in
`typeInfoFromAST`). The core treats them as full `i64`, so `int8` gets 128 and `i64::MIN`, which
`json.Unmarshal` rejects before the target runs.

**Unsigned kinds** use Go-only complex kinds instead of the protocol fields:

- `uint`, `uint16`, `uint32`, `uint64` and `uintptr` map to `go_uint` in `basicTypeInfo`
  (`:1502`); `typeInfoFromAST` maps `uint`..`uint64` (`:1565`) and has no `uintptr` case;
- `uint8`/`byte` maps to `go_byte` (`:1496`, `:1558`).

`go_uint` carries no width, so even after `core-go-alias-int-range` routes it onto
`Int { 64, false }`, a `uint16` or `uint32` param still gets values up to `u64::MAX`. Only the
analyzer knows the real width.

Go-side consumers of the current TypeInfo that must keep working:

- the planner: `shatter-go/planner/param.go:692-716` (`go_uint` → `uintFamily`; `[]byte`
  detection from a `go_byte` element, str-79nvf) and `planner/composite.go:213-244`
  (unsigned pointer and struct-field zero values);
- the handler: `protocol/handler.go:306` (declares `complex_type:go_byte`) and the direct-execute
  checks at `:2227` and `:2337`;
- `protocol/registry.yaml:454` (Go's declared complex types), `protocol/parity-matrix.yaml:458`
  (the `go_byte` row) and `protocol/conformance/golden/handshake/go.json:9`.

The core needs no change here: `core-go-alias-int-range` already treats both the old and the new
wire forms as unsigned ints, and requests from core to frontend carry no TypeInfo.

## Fixture (commit with this issue's branch)

`examples/go/int-width/go.mod`:

```
module example.com/int-width

go 1.23.0
```

`examples/go/int-width/widths.go`:

```go
// Package intwidth is the known-answer fixture for Go integer width and
// signedness. Each function branches at its parameter type's min or max.
package intwidth

func AtMaxInt8(n int8) string {
	if n == 127 {
		return "max"
	}
	return "other"
}

func AtMinInt8(n int8) string {
	if n == -128 {
		return "min"
	}
	return "other"
}

func AtMaxUint16(n uint16) string {
	if n == 65535 {
		return "max"
	}
	return "other"
}

func AtMaxUint64(n uint64) string {
	if n == 18446744073709551615 {
		return "max"
	}
	return "other"
}

func AtMaxByte(b byte) string {
	if b == 255 {
		return "max"
	}
	return "other"
}

func Label(tag string, n int16) string {
	if len(tag) > 2 {
		if n > 3 {
			return "big"
		}
		return "small"
	}
	return "none"
}
```

`scripts/check_go_int_width_bounds.py`:

```python
#!/usr/bin/env python3
"""Assert every executed integer input in the Go int-width fixture artifacts is within its type's bounds."""
import glob, json, sys

BOUNDS = {  # function -> (param index, min, max)
    "AtMaxInt8": (0, -128, 127), "AtMinInt8": (0, -128, 127),
    "AtMaxUint16": (0, 0, 65535), "AtMaxUint64": (0, 0, 2**64 - 1),
    "AtMaxByte": (0, 0, 255), "Label": (1, -32768, 32767),
}
bad = seen = 0
for f in sorted(glob.glob(sys.argv[1] + "/explore-results/*/*.json")):
    d = json.load(open(f))
    fn = d.get("function_name")
    if fn not in BOUNDS or "observation" not in d:
        continue
    i, lo, hi = BOUNDS[fn]
    for inputs, _mocks, _result in d["observation"]["raw_results"]:
        seen += 1
        n = inputs[i]
        if not (isinstance(n, int) and not isinstance(n, bool) and lo <= n <= hi):
            bad += 1
            print(f"OUT OF BOUNDS {fn}: {n!r}")
print(f"checked {seen} executed inputs; out of bounds: {bad}")
sys.exit(1 if bad or not seen else 0)
```


## Baseline (run on the branch base before changing code)

This needs no external examples checkout. Commit the fixture and checker first, then run from the
repo root:

```bash
git rev-parse HEAD
cargo build -p shatter-cli && (cd shatter-go && go build -buildvcs=false -o bin/shatter-go .)
export PATH="$PWD/shatter-go/bin:$PATH"
tmp=$(mktemp -d) && cp -r examples/go/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/widths.go" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
python3 scripts/check_go_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
```

Recorded at 16794cef on 2026-09-24, before any epic child landed: the checker exits 1 with
`checked 360 executed inputs; out of bounds: 80`. Examples:

- `AtMaxInt8`: 128, 671, -303, `i64::MIN`, `i64::MAX`;
- `AtMaxUint16`: 65536, 4294967295, 18446744073709551615, `null`;
- `AtMaxByte`: 256;
- `Label`'s `int16`: `i64::MAX`, `i64::MIN`;
- `AtMaxUint64`: `null`.

The branch base will already include `int-unsigned64-clamp`, `core-int-range-i128` and
`core-go-alias-int-range`, so the count will differ. It must still be non-zero: the signed kinds
are bare `int` and `uint16` is still a width-less `go_uint`. Paste the actual output. In the report
out-of-range values appear as `throws function_error: param n: json: cannot unmarshal number 128
into Go value of type int8`. The min/max branches are already reached, through Z3 on the literals
and the unsigned boundary seeds. That coverage must not regress.

## Acceptance criteria

- [ ] The fixture and checker are committed. The baseline output is pasted with
  `git rev-parse HEAD`.
- [ ] **Analyzer.** The Go `TypeInfo` gains `IntWidth`/`IntSigned` (`json:"int_width,omitempty"`,
  `json:"int_signed,omitempty"`). Both mapping sites (`basicTypeInfo` and `typeInfoFromAST`) emit
  `{"kind":"int","int_width":W,"int_signed":S}` for every Go integer kind, including `uintptr` in
  both. `int`, `uint` and `uintptr` use width 64, and a comment states the 64-bit-platform
  assumption. `rune` is `(32, true)`; `byte` is `(8, false)`. Neither site emits `go_uint` or
  `go_byte` any more. Analyzer unit tests cover every kind at both sites.
- [ ] **Planner and handler.** They recognize unsigned ints and `[]byte` from the new TypeInfo
  (`int_signed: false`; an array whose element is `int_width: 8, int_signed: false`) in
  `planner/param.go`, `planner/composite.go` and `handler.go:2227`, `:2337`. The existing
  str-79nvf and str-ieuc tests pass unchanged, and new cases cover the int-typed forms.
- [ ] **Handshake and registry.** The Go frontend stops declaring `complex_type:go_byte`
  (`handler.go:306`). Update `protocol/conformance/golden/handshake/go.json` and Go's entry in
  `protocol/registry.yaml` (`:454`) to match. The `go_byte` kind itself stays in the schema and in
  the registry's kind list until `go-uint-alias-removal`, so no bindings are regenerated here.
- [ ] **Parity.** `protocol/parity-matrix.yaml` gains an "integer width and signedness" row: Rust
  and Go emit it; TS is n/a (`number` is float and `bigint` is `big_int`). Update the `go_byte` row
  (`:458`) to say Go no longer emits it. Add a conformance case per emitting frontend that asserts
  the TypeInfo for the fixture's params. `task parity` and `task conformance` pass.
- [ ] **E2E** `e2e_go_int_width_inputs_in_bounds` in `shatter-core/tests/e2e_concolic_go.rs`,
  using `repo_examples_go_dir().join("int-width").join("widths.go")` and
  `spawn_go_frontend("int-width")`. For every fixture function it seeds with
  `boundary_dict::generate_boundary_inputs(&analysis.params)` and runs `orchestrator::explore`. It
  asserts:
  - every value in `raw_results` is within the Go type's bounds (the checker's table);
  - the `max`/`min` branch of each `At*` function is taken;
  - `Label` reaches `"big"`.

  It must not assert on error rows. Run it with
  `task go:build && SHATTER_GO_FRONTEND_BIN="$PWD/shatter-go/bin/shatter-go" cargo test --test e2e_concolic_go e2e_go_int_width -- --include-ignored`.
  The case is `#[ignore]`d, and no `SHATTER_EXAMPLES_DIR` is needed for a repo-local fixture.
  Paste the failing `test result:` line from the branch base and the passing line from the branch.
- [ ] Re-run the baseline on the branch, once with the default explorer and once with
  `--concolic`, from a fresh copy each time so artifacts from the first run cannot be resumed:
  ```bash
  for mode in "" "--concolic"; do
    tmp=$(mktemp -d) && cp -r examples/go/int-width "$tmp"/
    target/debug/shatter explore "$tmp/int-width/widths.go" --allow-host-writes \
      --max-iterations 60 --request-timeout 240 $mode
    python3 scripts/check_go_int_width_bounds.py "$tmp/int-width/shatter-artifacts"
    rm -rf "$tmp"
  done
  ```
  Both checker runs print `out of bounds: 0`.
- [ ] A `SPEC.md` §8 changelog row marks `go_uint`/`go_byte` deprecated: no frontend emits them,
  the core still accepts them (`core-go-alias-int-range`) so an older installed `shatter-go` keeps
  working, and removal is `go-uint-alias-removal`.
- [ ] `shatter-go/CLAUDE.md` documents the protocol-visible change. `task e2e` and
  `task affected` pass, with `Gates selected` recorded.
- [ ] The close reason links `go-uint-alias-removal` (filed with the audit drafts) and gives the
  merge commit sha that starts its compatibility window. This issue does not wait for any release.

## Out of scope

- Any core change (`core-go-alias-int-range`, done before this).
- Removing the aliases from the core, schema, bindings or registry kind list
  (`go-uint-alias-removal`).
- TS integer typing.
- Go `complex64`/`complex128`.
- Classifying unmarshal failures as input rejections (str-4yc9w).

## Size

M, at the upper end. Roughly: analyzer and `TypeInfo` (2 files plus tests), planner (2 files plus
tests), handler (1 file), registry, parity matrix, handshake golden and conformance cases, the
fixture and checker, one E2E test, and `shatter-go/CLAUDE.md`. No core source changes. If the
planner/handler adaptation proves larger than expected, split it out and keep the analyzer change
behind it, not the other way round: emitting the new TypeInfo before the planner understands it
would regress `[]byte` handling.

## References

- `core-go-alias-int-range`: prerequisite; makes the core treat both wire forms as unsigned ints.
- str-cfsa (closed): introduced `go_uint`.
- str-ieuc (closed): `go_byte` coercion.
- str-79nvf (closed): `[]byte` detection.
- str-ddxe (closed): the protocol fields.
- str-4yc9w (open): Go decode-failure classification. Related, not blocking.
- No existing tracker issue covers Go int width emission (`bd search go_uint` and `int8` return
  nothing).


---

---
slug: go-uint-alias-removal
kind: new
title: "Remove the deprecated go_uint/go_byte TypeInfo aliases after the compatibility window (one continuous release at least 30 days old)"
priority: P3
type: task
labels: [go-frontend, protocol, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-int-width-sign-emission]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove the deprecated go_uint/go_byte TypeInfo aliases

A deferred follow-up to epic `int-width-signedness-epic`. It is not a child of that epic: it is
parented to the audit epic and does not hold the integer epic open. All paths are relative to the
shatter repo root (github.com/shatterproof-ai/shatter).

## Why

`core-go-alias-int-range` makes the core treat `go_uint`/`go_byte` as unsigned `Int` on the
`int_range` path, and `go-int-width-sign-emission` moves the Go analyzer to
`{"kind":"int","int_width","int_signed"}`. The core keeps accepting `go_uint`/`go_byte` as
deprecated wire aliases so that an older installed `shatter-go` binary still works with a newer
core. After the compatibility window, the aliases are
dead code and a second way of saying the same thing, which is the exact divergence the epic
removes.

## Trigger: do not start before this holds

Releases are the `continuous-<YYYYMMDD-HHMM>-<12-char sha>` prereleases created on every push to
`main` (`.github/workflows/release.yml`, the "Generate continuous build tag" step). The window has
passed when **at least one continuous release whose commit contains the merge of
`go-int-width-sign-emission` was created 30 or more days ago**. Retention keeps the 30 most recent
releases plus 12 monthly ones (`cleanup-continuous-releases.yml`), so such a release stays
listable. Verify it and paste the output:

```bash
merge=<sha of the go-int-width-sign-emission merge commit on main>
git fetch origin --tags
gh release list --repo shatterproof-ai/shatter --limit 500 --json tagName,createdAt \
  --jq '.[] | select(.tagName | startswith("continuous-")) | "\(.createdAt) \(.tagName)"' |
while read -r created tag; do
  sha=${tag##*-}
  if git merge-base --is-ancestor "$merge" "$sha" 2>/dev/null &&
     [ "$(date -d "$created" +%s)" -le "$(date -d '30 days ago' +%s)" ]; then
    echo "window passed: $tag ($created)"
  fi
done
```

At least one `window passed:` line is required. If there are none, leave the issue open and add a
comment with the date of the oldest qualifying release.

## Acceptance criteria

- [ ] The trigger output above is pasted.
- [ ] `ComplexKind::GoUint`/`GoByte` and their generators, mutators and serializer branches are
  removed from the core: `input_gen.rs` (`generate_go_uint`, `generate_go_byte`, `mutate_go_uint`,
  `mutate_go_byte`, dispatch arms), `orchestrator.rs` (`concrete_to_json` arms), `types.rs`,
  `export.rs`, `test_arbitraries.rs`, and the alias normalization added by
  `core-go-alias-int-range`. That issue's golden compatibility test is replaced by the rejection
  test below. Regenerate the bindings (`shatter-rust/src/protocol.rs`,
  `shatter-ts/src/protocol.ts`) from the schema; never hand-edit them. Remove the entries from
  `protocol/registry.yaml` and `protocol/parity-matrix.yaml`.
- [ ] `git grep -n 'go_uint\|go_byte\|GoUint\|GoByte' -- . ':!audits' ':!.beads' ':!docs/perf'`
  returns only the SPEC §8 changelog history and tests that assert rejection. Paste the output.
- [ ] Test: a TypeInfo payload with `complex_kind: "go_uint"` or `"go_byte"` now fails to
  deserialize, or maps to `Unknown` if the codebase's unknown-kind policy says so; state which,
  with a clear error. The test lives beside the existing `types.rs` ComplexKind serde tests.
- [ ] `task e2e-go` still passes, including `e2e_go_int_width_inputs_in_bounds` from
  `go-int-width-sign-emission`, and the Go bounds checker from that issue still prints
  `out of bounds: 0`.
- [ ] A SPEC §8 changelog row records the removal. `shatter-go/CLAUDE.md` is updated. `task parity`,
  `task conformance` and `task affected` pass, with `Gates selected` recorded.

## Out of scope

Any behaviour change for integer generation. This is a pure deletion.

## Size

S

## References

- `core-go-alias-int-range`: turns `go_uint`/`go_byte` into core-side aliases of unsigned `Int`.
- `go-int-width-sign-emission`: stops the Go frontend emitting them; its merge starts the window.
- str-cfsa (closed): `go_uint`.
- str-ieuc (closed): `go_byte`.
- No existing tracker issue covers the removal.


---

---
slug: rust-input-deserialize-classification
kind: new
title: "Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors"
priority: P2
type: bug
labels: [rust-frontend, reporting, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [str-4yc9w]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust harness input-deserialization failures are reported as target `throws runtime_error` instead of tool/input errors

All paths are relative to the shatter repo root (github.com/shatterproof-ai/shatter). Line numbers
were verified at 16794cef.

## Problem

Sometimes the Rust harness cannot deserialize an input into the parameter's type, for example
`input 1 deserialization failed: invalid value: integer `-1`, expected usize`. When that happens,
the target function never runs. Explore and scan output still record the row as `throws
runtime_error: ...`, which reads as a behaviour of the target.

Any input the generator gets wrong becomes a fake finding. Today the main source is the integer
bug `int-unsigned64-clamp`, but any future type-mapping gap would do the same. The walkthrough error
regex does not match "deserialization failed", so no gate notices.

This issue stands alone. It is related to the epic `int-width-signedness-epic` but is not a child:
it does not depend on the generator fix, and the epic does not depend on it. Its proof feeds bad
inputs directly, so it still works after the generator is fixed.

## Evidence

The generated harness hard-codes `"error_type": "runtime_error"` for deserialization failures at
three groups of sites in `shatter-rust/src/executor.rs`:

- the direct-call harness: `:2471`, `:2484`, `:2498`;
- the `'shatter_arm` dispatch: `:2792`, `:2805`, `:2819`;
- the JSON-literal harness: `:5005`, `:5016`, `:5030`.

Reproduction on 16794cef (recorded 2026-09-24) uses the fixture `examples/rust/int-width` as
drafted in `int-unsigned64-clamp`. If that issue has not landed yet, commit the fixture here; it is
given inline there. The command is:

```bash
cargo build -p shatter-cli && cargo build --manifest-path shatter-rust/Cargo.toml
tmp=$(mktemp -d) && cp -r examples/rust/int-width "$tmp"/
target/debug/shatter explore "$tmp/int-width/src/lib.rs:rank_usize" --allow-host-writes \
  --max-iterations 60 --request-timeout 240
```

It printed 15 paths, 13 of them `throws `runtime_error: input 1 deserialization failed: invalid
value: integer `-N`, expected usize``. This reproduction depends on the generator bug, so after
`int-unsigned64-clamp` lands, use the seeded tests below instead.

Go has the same class of problem, tracked as str-4yc9w (open, P1, started: the launcher's decode
errors bypass outcome classification). str-cfsa (closed) was an earlier Go counterpart. `bd search
deserializ` and `input_error` find no Rust-side duplicate.

## Acceptance criteria

- [ ] All nine sites above emit a distinct classification for a failed parameter decode: either a
  `thrown_error.error_type` such as `input_error`, or a distinct execute-result outcome. It must be
  the same one str-4yc9w lands for Go. **str-4yc9w owns the choice** (it is the open P1 already
  working on Go's decode-error outcome), which is why this issue is blocked by it: implement the
  classification str-4yc9w lands, name it in the close note, and do not introduce a second one.
- [ ] The core and report layers treat that outcome as a tool or input error. It is not counted as
  a target behaviour or finding in explore and scan output, and it is counted in the run's error
  summary.
- [ ] Harness-level test in shatter-rust: build the harness for `rank_usize` from the fixture and
  execute it with the explicit input `["en", -1]`. Assert the new classification. This does not
  depend on the generator, so it keeps working after `int-unsigned64-clamp`. Show it failing on
  main.
- [ ] Report-level test: a raw result carrying the new classification is not rendered as `throws`
  and appears in the error summary. Show it failing on main.
- [ ] E2E in `shatter-core/tests/e2e_concolic_rust.rs`:
  - it uses `repo_examples_rust_dir().join("int-width/src/lib.rs")`, so no external checkout is
    needed;
  - it passes `vec![vec![json!("en"), json!(-1)]]` as the explicit seed to `orchestrator::explore`;
  - it asserts that the seed's result carries the new classification and is not a completed or
    `runtime_error` outcome.

  Run it with
  `cargo build --manifest-path shatter-rust/Cargo.toml && cargo test --test e2e_concolic_rust <name> -- --include-ignored`
  and paste the failing and passing `test result:` lines.
- [ ] If the classification is protocol-visible (a new `error_type` value or outcome), update
  `protocol/parity-matrix.yaml` and `shatter-rust/CLAUDE.md`, and run `task parity` and
  `task conformance`.

## Out of scope

- Fixing the generator (`int-unsigned64-clamp`).
- The Go-side change (str-4yc9w), beyond agreeing on the shared classification.

## Size

S

## References

- Split from `int-unsigned64-clamp` after the Codex cross-check of the 2026-09-22 audit (finding
  goals-15).
- Related: str-4yc9w (open, Go), str-cfsa (closed, Go), epic `int-width-signedness-epic` (related,
  not the parent).


---

