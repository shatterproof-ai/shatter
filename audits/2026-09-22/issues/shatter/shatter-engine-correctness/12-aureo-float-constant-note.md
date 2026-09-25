---
slug: aureo-float-constant-note
kind: note-to-existing
title: "Note on str-aureo: float constants above 2147.483647 wrap through a C int cast; propose raising to P1 and requiring exact-rational tests"
priority: P1
type: bug
labels: [solver, z3, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-aureo
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-aureo: float constants above 2147.483647 wrap through a C int cast; propose raising to P1 and requiring exact-rational tests

**Target:** str-aureo (open, P2, "Float constants lose precision before solving"). Add a comment and propose raising the priority to P1. No new issue.

This replaces the earlier audit draft `float-constant-rational-conversion`, which duplicated str-aureo (checked with `bd show str-aureo` on 2026-09-23). str-aureo already covers the `(*v * 1_000_000.0).round() as i64` encoding at `shatter-core/src/solver.rs:514-516`, the tiny-value collapse to 0 (its 1e-7 repro), exact binary-rational encoding, and extraction in scope. The comment adds a much larger affected range, found by the Codex cross-check, and makes the test requirements exact.

## Comment text

> **Audit 2026-09-22 (finding core-19, corrected by cross-check): the affected range is much larger than "precision beyond six decimals".**
>
> **1. Constants with magnitude above 2147.483647 are corrupted, not only rounded.** The pinned `z3` crate is 0.19.10 (`Cargo.lock`). Its `Real::from_rational(num: i64, den: i64)` passes `num as c_int` and `den as c_int` to `Z3_mk_real` (`z3-0.19.10/src/ast/real.rs:63-75`). The caller scales by 1e6 first, so the numerator overflows a 32-bit int once `|v| > 2147.483647`, and the `as` cast wraps silently. Example: `3000.0` becomes `3_000_000_000 as i32 = -1_294_967_296`, which Z3 reads as **-1294.967296**. A branch `if x > 3000.0` is therefore solved as `x > -1294.97`. Above about 9.2e12 the earlier `f64 as i64` cast also saturates, and `i64::MAX as i32` is `-1`, so every such constant becomes -0.000001. This was verified from source; the runtime was not re-run for this note.
>
> Thresholds such as 5000.0, 86400.0 or 1e6 are common in real code, so this is a wrong-answer bug in the core solver on ordinary inputs, of the same class as str-t854z (P1). **Proposed priority: P1.**
>
> **2. Test for exact rational equality, not f64 round-trip.** A shortest-round-trip decimal string does not meet this issue's contract. Binary f64 `0.1` is exactly `3602879701896397 / 36028797018963968`, while the decimal `"0.1"` is `1/10`. Both round-trip to the same f64, so a round-trip assertion would pass an inexact encoding. Required tests:
> - A unit test on the constant translation: for each value in a fixed corpus (`0.1`, `2147.483648`, `3000.0`, `-3000.0`, `1e13`, `1e-7`, `5e-324`, `f64::MAX`), the Z3 numeral equals the exact binary rational from `f64::integer_decode`, compared as big integers (numerator and denominator strings from Z3 against the expected ones). Build the numeral with `Real::from_rational_str` (present in z3 0.19.10, `real.rs:25`) or `Real::from_big_rational`, not `from_rational`.
> - A bounded proptest over finite f64 values with the same exact-equality assertion.
> - A regression test on current `main`: `x: Float`, constraint `x > 3000.0` (and `x < -3000.0`) must produce a model that satisfies the original f64 comparison. It must fail before the fix and pass after it, with both outputs quoted in the close note.
>
> **3. Model extraction must be fixed with the translation, or the new tests cannot pass.** Exact rationals for tiny or very large values have numerators and denominators beyond `i64`. `extract_concrete_values` (`solver.rs:1036-1045`) first calls `as_rational()`, which uses `Z3_get_numeral_small` and returns `None` outside `i64`. It then tries `val.to_string().parse::<f64>()`. Z3 prints such a numeral as an s-expression like `(/ 1.0 10000000.0)`, so the parse fails and the variable is left out of the model without any error. The fix must convert big rationals to the nearest f64 (for example through the numeral's decimal or rational string), or return `SolverError::Unsupported`. It must never drop the assignment. str-aureo already puts extraction in scope; this adds the concrete failure mode. Separate the tests: translation tests (point 2) check the Z3 numeral, and model round-trip tests check the extracted `ConcreteValue`.
>
> **4. NaN and infinities** go to `SolverError::Unsupported` explicitly, as the issue already requires. Add a test for each.
>
> Verify with `task affected` (record `Gates selected`) and `task e2e` (this is a solver change, and `task e2e` runs the `#[ignore]`d subprocess suites).
>
> Related: str-t854z (same function `to_z3_expr`; coordinate if both are in flight).
