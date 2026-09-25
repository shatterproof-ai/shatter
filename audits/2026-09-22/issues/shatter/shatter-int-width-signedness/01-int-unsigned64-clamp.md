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
