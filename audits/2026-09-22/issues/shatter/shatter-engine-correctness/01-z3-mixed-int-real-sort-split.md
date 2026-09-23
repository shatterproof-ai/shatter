---
slug: z3-mixed-int-real-sort-split
kind: new
title: "Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 gives x=1.5)"
priority: P1
type: bug
labels: [solver, z3, concolic, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 gives x=1.5)

## Problem

A float param can be compared against both float and int constants, as in `if x > 0.5 { if x < 1 {...} }`. In that case the Z3 translation creates `Real x` and `Int x` as two unrelated constants with the same name. Model extraction then lets one overwrite the other. The solver returns "SAT" models that violate the path condition, so neither engine reaches the branch. Integer range bounds (such as u8) are asserted only on the Int twin, so the Real twin can leave the declared range.

This is a wrong-answer bug in the core solver. It affects any frontend that emits mixed-sort comparisons on one numeric param. The Go frontend does this: `{op:gt, const float 0.5}` next to `{op:lt, const int 1}`.

## Evidence

Line numbers were re-checked against `56c86168` (audit branch `audit-2026-09-22`):

- `shatter-core/src/solver.rs:500-503` (in `to_z3_expr`, which starts at :491): the declared param sort overrides the hint only when `!sorts_compatible(declared, hint_sort)`. For any numeric declared sort, the comparison's Int/Real hint wins.
- `shatter-core/src/solver.rs:74-121`: `VarTable` keeps separate `ints` and `reals` maps. `get_or_create_int` (:95-100) calls `Int::new_const(name)` and `get_or_create_real` (:102-107) calls `Real::new_const(name)` under the same name.
- `shatter-core/src/solver.rs:1023-1066`: `extract_concrete_values` inserts ints and then reals into one `HashMap`, so the Real value silently overwrites the Int value.
- `shatter-core/src/solver.rs:172-180`: `assert_int_param_ranges` binds only `vars.get_or_create_int(&p.name)`. The Real twin is unconstrained.
- Direct call observed during the audit: `solve_for_new_path` with x:Float and constraints [x>0.5 (float const), NOT(x<1 (int const))], negating index 1, returned `Ok(Sat({"x": Float(1.5)}))` in 3/3 runs. The wanted range was 0.5<x<1.
- n:u8 with [n<1000.5, n>300.5], negating index 1, returned `Sat({"n": Float(0.0)})`.
- End to end with a Go `func Classify(x float64) string` that has nested `x > 0.5` / `x < 1`: `shatter explore mix.go --concolic --max-iterations 40 --clean` reported 23 iterations, 2 paths, `worklist_exhausted`, and 4/5 lines. `return "low"` was never reached, and no tried input was in (0.5, 1). The random/hybrid engine, which also runs the Z3Solver strategy, missed it at 100 iterations too.
- Root cause of the gap: the solver proptests (str-r6fr) never generate one param under mixed sorts, and the E2E known-answer fixtures use only int or string params.

## Acceptance criteria

- [ ] Each variable gets exactly one Z3 sort, chosen from `param_sorts` before translation. Mixed-sort comparisons coerce at the use site (`Real::from_int` / `to_int`, or by coercing the constant) and never declare a second constant.
- [ ] `extract_concrete_values` has a debug assertion that no name appears in more than one sort table.
- [ ] `assert_int_param_ranges` constrains the single chosen variable, whatever its sort.
- [ ] Solver unit tests for both repros above return models that satisfy the constraints. At close, show that they fail on current `main` and pass after the fix, with test names and the before/after output in the close note.
- [ ] A proptest generates one param compared under mixed Int/Real constants and checks that every SAT model satisfies all asserted constraints.
- [ ] A known-answer E2E fixture shaped like `Classify` (nested x>0.5 / x<1 on a float param) exists for Go (`examples/go/`, exercised by `shatter-core/tests/e2e_concolic_go.rs`) and for TS (`shatter-core/tests/e2e_concolic.rs`). Under `--concolic` it reaches all three returns. Record the `cargo test --test e2e_concolic_go` and `cargo test --test e2e_concolic` output in the close note.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Resolve the sort once per variable in `VarTable`, from `param_sorts` with a fallback to the first-seen hint. `get_or_create_int`/`get_or_create_real` then return a coerced view of the single constant instead of declaring a twin. Coercing constants to the variable's sort is simpler than coercing variables. Check the Int-variable, Real-constant case (`n < 1000.5`) carefully: coerce the variable to Real, or use ceil/floor on the constant, rather than truncating.

## Out of scope

- The float-constant scaling bug (`(v*1e6).round() as i64`). It is tracked separately as float-constant-rational-conversion.
- Concolic early termination in general. It is filed separately as concolic-early-termination (bucket shatter-concolic-and-engine-design), which may list this issue as a contributing cause.

## Priority

P1: the solver returns wrong answers and the engine reports false exhaustion.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-6ayh, str-r6fr (solver proptests), concolic-early-termination.

## References

Audit 2026-09-22 finding core-01 (verified, P1). Source draft: `drafts/shatter-code/11-z3-int-real-sort-split.md`.
