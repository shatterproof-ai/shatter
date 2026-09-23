# Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | solver,z3,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | core-19 |

<!-- body -->
## Problem

Float literals in path constraints are scaled by 1e6 and cast to i64, losing precision and saturating.

## Current code facts / evidence

- `shatter-core/src/solver.rs:514-516`: `let scaled = (*v * 1_000_000.0).round() as i64; Real::from_rational(scaled, 1_000_000)`.

## Acceptance criteria

- Exact rational built from the f64 mantissa/exponent (or `Real::from_real_str` with shortest repr).
- Proptests with extreme magnitudes (1e15, 1e-9, subnormals) round-trip within exactness.

## Suggested approach

Replace the conversion; add tests.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: core-19 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
