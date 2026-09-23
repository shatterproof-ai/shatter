---
slug: float-constant-rational-conversion
kind: new
title: "Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7"
priority: P3
type: bug
labels: [solver, z3, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7

## Problem

Float literals in path constraints are converted to Z3 reals by scaling by 1e6, rounding and casting to `i64`. Rust's float-to-int `as` saturates, so any constant above about 9.2e12 becomes `i64::MAX / 1e6`. Any constant with magnitude below 5e-7 becomes 0, and everything in between loses precision beyond six decimal places. Branches on large thresholds (timestamps in ms or ns, byte counts) or tiny epsilons are then solved against the wrong constant, and the solver can report SAT or UNSAT incorrectly.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/solver.rs:514-517`, inside `to_z3_expr`'s `SymExpr::Const` arm:
  ```rust
  ConstValue::Float(v) => {
      let scaled = (*v * 1_000_000.0).round() as i64;
      Ok(Z3Ast::Real(Real::from_rational(scaled, 1_000_000)))
  }
  ```
- The 2026-09-04 audit noted the x1e6 scaling but not these edge cases. No existing issue covers them.

## Acceptance criteria

- [ ] Float constants become an exact Z3 rational: numerator and denominator built from the f64 mantissa and exponent (as big integers or a decimal string if needed), or `Real::from_real_str` with the shortest round-trip representation. NaN and infinities are rejected explicitly with `SolverError::Unsupported`, not silently mapped.
- [ ] Proptests with extreme magnitudes (1e15, 1e-9, subnormals, negative values) check that the Z3 value equals the f64 exactly. For example: assert `x == c` and check that the model value round-trips to the same f64.
- [ ] A regression test in which a constraint `x > 1e13` (and `x < 1e-9`, `x > 0`) produces a model satisfying the original f64 comparison. At close, show it failing on current `main` and passing after the fix.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass (solver change).

## Suggested approach

Use `f64::integer_decode` (or an equivalent via `to_bits`) to get mantissa, exponent and sign. Build the rational as `mantissa * 2^exp`, or `mantissa / 2^-exp`, with Z3 integer terms, or format it as an exact decimal string for `from_real_str`. Add the tests.

## Out of scope

- The Int/Real sort split (z3-mixed-int-real-sort-split).
- Model-to-f64 extraction precision in `extract_concrete_values` (`num as f64 / den as f64`). Mention it in the close note if it proves to be a problem.

## Priority

P3

## Type

bug

## Dependencies

- Blocked by: none.
- Related: z3-mixed-int-real-sort-split (same file, and both edit `to_z3_expr`, so coordinate if both are in flight).

## References

Audit 2026-09-22 finding core-19 (verified, P3). Source draft: `drafts/shatter-code/24-float-constant-rational-conversion.md`.
