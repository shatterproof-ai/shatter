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
