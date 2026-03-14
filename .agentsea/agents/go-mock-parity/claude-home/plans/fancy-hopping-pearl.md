# str-b5zn: Add remaining high-value contracts to solver bridge and protocol boundary

## Context

The `contracts` crate is already in use with 3 annotations (1 `#[requires]` on `solve_for_new_path` for `negate_index`, and 1 `#[requires]` + 1 `#[ensures]` on `overlay_solved_values`). The contracts policy identifies 5 qualifying sites. Three remain unimplemented.

Key insight from code review: `SymExpr::Param` uses **string names** (not numeric indices), so the `to_z3_expr` precondition should validate that Param names exist in the param_infos list. However, `to_z3_expr` is a **private** function and receives params indirectly via `VarTable::param_sorts` — the real trust boundary is at `solve_for_new_path` where `param_infos` arrives from the frontend subprocess. The contract on `to_z3_expr` itself would be about ensuring the param_sorts map was correctly built, but since `to_z3_expr` just creates new Z3 variables for unknown params (defaulting to Int sort), an invalid param name doesn't cause silent corruption — it just gets a default sort. The more valuable contract is a postcondition on `solve_for_new_path` checking solved value types match param_infos.

## Plan

### 1. `solve_for_new_path()` postcondition — solved values match ParamInfo types

**File:** `shatter-core/src/solver.rs`

Add `use contracts::ensures;` and a `#[ensures]` on `solve_for_new_path()` that validates: when the result is `Ok(SolveResult::Sat(map))`, every key in `map` that matches a param_info name has a `ConcreteValue` variant compatible with that param's `TypeInfo`.

Helper function `solved_values_match_param_types(result, param_infos) -> bool` to keep the contract expression clean. This checks:
- `ConcreteValue::Int` ↔ `TypeInfo::Int`
- `ConcreteValue::Float` ↔ `TypeInfo::Float`
- `ConcreteValue::Str` ↔ `TypeInfo::Str`
- `ConcreteValue::Bool` ↔ `TypeInfo::Bool`
- Nullable inner types match
- Unknown/unrecognized types pass (no assertion on types Z3 can't represent)

### 2. `to_z3_expr()` precondition — Param names reference known params

**File:** `shatter-core/src/solver.rs`

Since `to_z3_expr` is private and called in a loop, adding a `#[requires]` contract on every call would be expensive and the failure mode isn't truly silent (unknown params just get default Int sort, which will cause a Z3 sort mismatch if the actual type is String).

**Revised approach:** Add a validation check at the `solve_for_new_path` level — before solving, collect all param names from constraints and warn/validate against param_infos. This is lighter-weight and catches the real trust boundary (frontend JSON → solver). Implement as a debug-only assertion helper called before the Z3 solve loop, using `#[cfg(debug_assertions)]` or as a contract precondition on `solve_for_new_path`.

Actually, re-reading the policy: the contract must be at a trust boundary where violation causes **silent corruption**. Unknown param names in constraints → Z3 defaults to Int → if it should be String, Z3 returns a sort mismatch error (not silent). The postcondition (item 1) is the higher-value contract. I'll still add a `#[requires]`-style validation but as a lightweight debug assertion rather than a formal contract, since the failure mode is Z3 error not silent corruption.

### 3. Protocol validation wrappers

**File:** `shatter-core/src/protocol.rs`

Create two public validation functions:

**`validate_execute_result(result: &ExecuteResult) -> bool`** with `#[ensures]`:
- Every `BranchDecision` in `branch_path` has a meaningful constraint (not checking emptiness of branch_path itself — empty is valid for functions with no branches)
- `path_constraints` entries with `Expr` variant have non-trivial expressions

**`validate_analyze_result(functions: &[FunctionAnalysis]) -> bool`** with `#[ensures]`:
- Every `FunctionAnalysis` has non-empty `name`
- `params` count is reasonable (sanity bound)
- `start_line <= end_line`

These are called after deserializing frontend subprocess JSON — the trust boundary is subprocess → core.

## Files to modify

1. `shatter-core/src/solver.rs` — postcondition on `solve_for_new_path`, debug assertion for param name validity
2. `shatter-core/src/protocol.rs` — `validate_execute_result()` and `validate_analyze_result()` with contracts

## Verification

1. `cargo test -p shatter-core` — unit tests pass
2. `cargo clippy -p shatter-core -- -D warnings` — clean
3. `cargo test --test e2e_concolic` — E2E pipeline still works
4. Add tests for validation functions and the postcondition (test that invalid data triggers contract panics in debug builds)
