# str-jxs0: Extract Z3 constraint solving into Z3SolverStrategy

## Context

Z3 constraint solving is currently hard-wired into the concolic loop in `orchestrator.rs` (lines 569-608). This issue extracts that logic into a `Z3SolverStrategy` struct implementing the `InputStrategy` trait, making it composable with other strategies via `MetaStrategy`. The orchestrator integration (replacing inline code) happens later in str-djqh.

## Files to Modify

| File | Change |
|------|--------|
| `shatter-core/src/orchestrator.rs` | Make 3 helper functions `pub(crate)` |
| `shatter-core/src/strategy.rs` | Add `Z3SolverStrategy` struct, impl, unit tests, proptests |

## Step 1: Make helper functions accessible

In `orchestrator.rs`, change visibility from `fn` to `pub(crate) fn` for:
- `extract_sym_constraints` (line 236)
- `concrete_to_json` (line 266)
- `overlay_solved_values` (line 297)

No logic changes.

## Step 2: Add Z3SolverStrategy to strategy.rs

### Struct

```rust
pub struct Z3SolverStrategy {
    solver_timeout_ms: Option<u64>,
    param_infos: Vec<ParamInfo>,
    pending: VecDeque<Vec<Value>>,
}
```

- `solver_timeout_ms` and `param_infos` passed at construction (both fixed per exploration session)
- `pending` is a queue of solved inputs, filled by `feedback()`, drained by `next()`

### Constructor

```rust
pub fn new(solver_timeout_ms: Option<u64>, param_infos: Vec<ParamInfo>) -> Self
```

### InputStrategy impl

- **`feedback()`**: Extracts constraints via `extract_sym_constraints(result)`, filters to solvable `SymExpr`s, calls `solver::solve_for_new_path()` for each, overlays solutions via `overlay_solved_values()`, pushes to `pending`
- **`next()`**: `self.pending.pop_front()`
- **`name()`**: `"z3_solver"`
- **`estimated_size()`**: `None` (infinite — produces work while unsolved constraints exist)

### Design decisions
- Stall tracking (Unsat/error → frontier) stays in orchestrator — the strategy just produces inputs or nothing
- No changes to the `InputStrategy` trait
- Follows `FuzzerStrategy` pattern: feedback fills queue, next drains it

### Imports to add

```rust
use crate::solver::{self, SolveResult};
use crate::sym_expr::SymExpr;
```

(`ParamInfo` already imported, `VecDeque` already imported)

## Step 3: Unit tests

Add to existing `#[cfg(test)] mod tests` in strategy.rs, reusing `empty_ctx()` helper:

1. **next without feedback → None**
2. **name → "z3_solver"**
3. **estimated_size → None** (infinite)
4. **empty branch_path → no pending**
5. **solvable constraint → queues input** (use `x == 5` taken=true; Z3 negates to `x != 5` which is SAT)
6. **Unknown constraints → no pending**

Key types for test construction:
- `BranchDecision { branch_id: u32, line: u32, taken: bool, constraint: SymConstraint }`
- `SymConstraint::Expr { expr }` / `SymConstraint::Unknown { hint }`
- `SymExpr::Param { name, path }` (no `index` field)

## Step 4: Property-based tests

Add `proptest!` block using shared generators from `test_arbitraries.rs`:

1. **feedback never panics** — feed arbitrary `ExecuteResult` to feedback; must not panic
2. **output preserves input vector length** — any solved inputs must have same length as input

Use `arb_execute_result()`, `arb_param_info()` from `test_arbitraries.rs`.

## Step 5: Quality gates

```bash
cargo test -p shatter-core        # All tests pass
cargo clippy -p shatter-core -- -D warnings  # No warnings
```

## What this does NOT do

- Does not modify the `InputStrategy` trait
- Does not integrate Z3SolverStrategy into the orchestrator's explore loop (that's str-djqh)
- Does not change MetaStrategy registration
- Does not move helper functions to a new module
