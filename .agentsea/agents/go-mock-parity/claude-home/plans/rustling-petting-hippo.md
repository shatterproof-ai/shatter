# str-hsrb: Fallback Boundary Search for Unknown/Opaque Constraints

## Context

When Z3 can't solve a branch constraint (Unknown/opaque constraint, or solver timeout), the orchestrator falls back to `fuzz_inputs()` which blindly mutates values (±1, flip bool, empty string). This is directionless. When we already have concrete inputs that took **both sides** of a branch, we can interpolate between those witnesses using binary-search-style narrowing to find the boundary more effectively.

## Approach

Create a new `boundary_search` module with type-aware interpolation logic. Integrate it into the orchestrator loop as a higher-priority fallback than blind fuzzing, triggered when both sides of a branch have been observed.

## Files to Modify

| File | Change |
|------|--------|
| `shatter-core/src/boundary_search.rs` | **New** — core interpolation logic |
| `shatter-core/src/lib.rs` | Register module |
| `shatter-core/src/orchestrator.rs` | Add `InputSource::BoundarySearch`, `boundary_generated` counter, integrate into loop |
| `shatter-core/src/coverage_metrics.rs` | Add `DiscoveryMethod::BoundarySearch`, update `from_exploration()` |
| `shatter-core/src/pipeline.rs` | Update test fixture for new `ExploreResult` field |

## Implementation Steps

### 1. New module: `boundary_search.rs`

**Constants:**
- `MAX_BOUNDARY_STEPS: usize = 4` — interpolation candidates per branch per round
- `MAX_BOUNDARY_BRANCHES_PER_ROUND: usize = 3` — budget cap
- `FLOAT_CONVERGENCE_EPSILON: f64 = 1e-9`

**Functions:**

```rust
pub fn find_witness_pair(
    raw_results: &[(Vec<Value>, ExecuteResult)],
    branch_id: u32,
) -> Option<(Vec<Value>, Vec<Value>)>
```
Scan `raw_results` for inputs that took opposite sides of `branch_id`. Return `(true_witness, false_witness)`.

```rust
pub fn interpolate_inputs(
    true_witness: &[Value],
    false_witness: &[Value],
    param_infos: &[ParamInfo],
    max_steps: usize,
) -> Vec<Vec<Value>>
```
Per-parameter binary-search interpolation. Skip identical values. Round-robin across differing parameters to stay within budget.

```rust
fn interpolate_value(a: &Value, b: &Value, typ: &TypeInfo, max_steps: usize) -> Vec<Value>
```
- **Int**: midpoints via `(a + b) / 2`, stop when `|a - b| <= 1`
- **Float**: midpoints via `(a + b) / 2.0`, stop when `|a - b| < epsilon`
- **Array**: per-element interpolation on differing elements
- **Object**: per-field interpolation on differing fields
- **Nullable**: if one null / one non-null, return both; otherwise inner type
- **Bool/Str/Complex/Opaque/Unknown**: empty vec (can't meaningfully interpolate)

### 2. Update `InputSource` enum (orchestrator.rs)

Insert `BoundarySearch = 2` between `Fuzzed = 1` and existing `Drilled`. Renumber: `Drilled = 3`, `Z3Solved = 4`, `UserProvided = 5`.

### 3. Update `DiscoveryMethod` enum (coverage_metrics.rs)

Add `BoundarySearch` variant. In `from_exploration()`, count it alongside `Random`/`Drilled` in `random_found` (same bucket — non-solver, non-user discovery).

### 4. Update `ExploreResult` (orchestrator.rs)

Add `pub boundary_generated: usize` field.

### 5. Integrate into orchestrator loop (orchestrator.rs ~610-625)

Replace the existing fuzz fallback with a two-phase approach:
1. **Phase 1**: For Unknown constraints where both branch sides are observed, call `find_witness_pair` + `interpolate_inputs`, push to worklist as `InputSource::BoundarySearch`
2. **Phase 2**: If boundary search wasn't applicable (no opposite witness), fall back to existing `fuzz_inputs()`

Add `InputSource::BoundarySearch => DiscoveryMethod::BoundarySearch` in discovery attribution (~line 530).

### 6. Update pipeline.rs test fixture

Add `boundary_generated: 0` to the `ExploreResult` construction in test.

### 7. Tests

**Unit tests** in `boundary_search.rs`:
- `find_witness_pair` — both sides present, missing side, unknown branch
- `interpolate_int_binary_search` — midpoint correctness
- `interpolate_float_convergence` — epsilon stop condition
- `interpolate_skips_identical` — unchanged params produce no candidates
- `interpolate_respects_max_steps` — budget bound
- `candidates_preserve_vector_length` — output len == input len

**Proptest** in `boundary_search.rs`:
- `interpolate_int_midpoint_in_range` — all midpoints between min(a,b) and max(a,b)
- `interpolate_float_midpoint_in_range` — same for floats (filter NaN)
- `interpolate_preserves_vector_length` — invariant across random inputs
- `interpolate_bounded_output` — never exceeds max_steps candidates

## Parallel Code Path Check

Boundary search is concolic-only (requires branch path + constraint tracking). The random explorer in `explorer.rs` doesn't have this infrastructure — no parity issue.

## Verification

1. `cargo test -p shatter-core` — unit + proptest
2. `cargo clippy -p shatter-core -- -D warnings` — lint clean
3. `cargo test --test e2e_concolic` — pipeline still discovers expected branches (change touches orchestrator loop)
