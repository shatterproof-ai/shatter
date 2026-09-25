# Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 → x=1.5)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | solver,z3,concolic,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-6ayh, str-r6fr |
| source findings | core-01 |

<!-- body -->
## Problem

When a float param is compared against both float and int constants (`if x > 0.5 { if x < 1 {...} }`), the Z3 translation creates `Real x` and `Int x` as unrelated constants. Model extraction then lets one overwrite the other, so the solver returns models that violate the path condition and the branch is never reached by either engine.

## Current code facts / evidence

- `shatter-core/src/solver.rs:500-503` `to_z3_expr` uses the per-comparison hint sort whenever it is Int/Real-compatible with the declared sort.
- `shatter-core/src/solver.rs:73-121` `VarTable` keeps separate `ints`/`reals` maps and calls `Int::new_const(name)` / `Real::new_const(name)` under the same name.
- `shatter-core/src/solver.rs:951-981` `extract_concrete_values` inserts ints then reals into one HashMap (Real overwrites).
- `shatter-core/src/solver.rs:172-181` `assert_int_param_ranges` binds only the Int twin (u8 range not enforced on the Real).
- Direct `solve_for_new_path` with x:Float, [x>0.5 (float), NOT(x<1 (int))], negate idx 1 → `Sat({x: Float(1.5)})` 3/3 runs. n:u8, [n<1000.5, n>300.5] → `Sat({n: Float(0.0)})`.
- E2E: Go `func Classify(x float64) string` with nested x>0.5 / x<1 — `shatter explore mix.go --concolic --max-iterations 40 --clean` → 2 paths, 4/5 lines, `return "low"` never reached. The Go frontend emits `{op:gt, const float 0.5}` and `{op:lt, const int 1}`.

## Acceptance criteria

- Each variable gets exactly one Z3 sort chosen from `param_sorts` before translation; mixed-sort comparisons coerce at use (`Real::from_int` / `to_int`).
- `extract_concrete_values` asserts (debug) that no name appears in both sort tables.
- Solver unit tests for the two repros above return models satisfying the constraints; proptest generates one param compared under mixed sorts.
- Known-answer E2E (Go and TS) shaped like Classify reaches all three returns under `--concolic`.

## Suggested approach

Resolve sort per variable up front in VarTable; coerce constants rather than declaring a twin.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: core-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-6ayh, str-r6fr
