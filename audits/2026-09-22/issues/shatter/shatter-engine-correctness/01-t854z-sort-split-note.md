---
slug: t854z-sort-split-note
kind: note-to-existing
title: "Note on str-t854z: Go Classify E2E repro, u8 range-bound twin, and executable E2E verification requirements"
priority: P1
type: bug
labels: [solver, z3, concolic, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-t854z
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-t854z: Go Classify E2E repro, u8 range-bound twin, and executable E2E verification requirements

**Target:** str-t854z (open, P1, "Numeric variables split across solver sorts"). Add a comment. No new issue.

This replaces the earlier audit draft `z3-mixed-int-real-sort-split`, which duplicated str-t854z (checked with `bd show str-t854z` on 2026-09-23). str-t854z already covers the root cause (separate `ints`/`reals` maps in `VarTable`, the hint override in `to_z3_expr`, the Real value overwriting the Int one in `extract_concrete_values`), two direct repros, a mixed-literal property, and a TS concolic fixture. The comment below adds only what str-t854z lacks.

## Comment text

> **Audit 2026-09-22 (finding core-01): additional evidence and verification requirements.**
>
> Code is unchanged since this issue was filed. At audit branch `audit-2026-09-22` (code identical to `56c86168`): `VarTable` is at `shatter-core/src/solver.rs:74-121`, the hint override is at `:500-503`, and `extract_concrete_values` is at `:1023-1066`.
>
> **1. A third symptom: integer range bounds are placed on the Int twin only.** `assert_int_param_ranges` (`solver.rs:172-180`) calls `vars.get_or_create_int(&p.name)`. When the same param also appears in a Real comparison, the Real twin has no range bound. Direct call: `n: u8` with `[n < 1000.5, n > 300.5]`, negating index 1, returned `Sat({"n": Float(0.0)})`. Acceptance addition: `assert_int_param_ranges` constrains the one variable chosen for the param, whatever its sort, and a solver unit test covers this repro.
>
> **2. A `solve_for_new_path` repro in addition to the `solve_constraints` ones.** x: Float, constraints `[x > 0.5 (Float const), NOT(x < 1 (Int const))]`, negating index 1, returned `Ok(Sat({"x": Float(1.5)}))` in 3 of 3 runs. The requested range was 0.5 < x < 1. The issue's parity clause already requires the fix to cover `solve_for_new_path`; this gives it a concrete test.
>
> **3. End-to-end impact on the Go frontend.** The Go frontend emits `{op: gt, const float 0.5}` next to `{op: lt, const int 1}` for one float param. For `func Classify(x float64) string` with nested `x > 0.5` / `x < 1`, `shatter explore mix.go --concolic --max-iterations 40 --clean` reported 23 iterations, 2 paths, stop reason `worklist_exhausted`, and 4/5 lines. `return "low"` was never reached, and no tried input was in (0.5, 1). The default random engine also runs the Z3Solver strategy, and it missed the branch at 100 iterations too.
>
> **4. Acceptance addition: a Go known-answer E2E, verified so that it cannot pass by being skipped.**
> - Add a `Classify`-shaped fixture (nested `x > 0.5` / `x < 1` on a `float64` param, three distinct returns) to `shatter-core/tests/e2e_concolic_go.rs`, next to the TS fixture the issue already requires in `shatter-core/tests/e2e_concolic.rs`. Assert that all three returns are reached under the concolic orchestrator.
> - Fixture location: these suites read fixtures from the external examples repo (`github.com/shatterproof-ai/examples`, found through `SHATTER_EXAMPLES_DIR` or `<tmp>/shatter-examples-main/standalone/{go,ts}` via `scripts/examples_checkout.py`). Use a self-contained fixture instead: an inline source string written to a tempdir, as `e2e_concolic.rs` already does with `TS_CLOSURE_FIXTURE` (`:277-279`). No coordinated examples-repo change is needed then. If the fixture goes in the examples repo instead, land and pin that change first and name the examples commit in the close note.
> - These subprocess tests are `#[ignore]`d (`e2e_concolic.rs` has 26, `e2e_concolic_go.rs` has 23). Plain `cargo test --test e2e_concolic_go` does not run them. Verify with `task e2e-go` and `task e2e-ts` (these run `cargo test --test ... -- --include-ignored`). The close note must quote the lines of test output that show the new test names reported as `ok`, both from a run on current `main` (expected FAIL) and from a run after the fix. A run that does not list the new test names does not count.
>
> **5. Test coverage gap.** The solver proptests (str-r6fr) never generate one param under mixed sorts, and the E2E known-answer fixtures use only int or string params. The mixed-literal property this issue already asks for closes this gap. Its assertion must check that every SAT model satisfies all asserted constraints, not only that the output has the right type.
>
> Related: str-aureo (float-constant encoding; same function `to_z3_expr`, coordinate if both are in flight), str-6ayh, str-r6fr. The audit's concolic early-termination diagnosis (bucket shatter-concolic-and-engine-design) may attribute lost branches to this issue.
