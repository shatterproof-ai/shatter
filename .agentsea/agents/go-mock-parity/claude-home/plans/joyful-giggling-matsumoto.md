# str-q33: Self-hosting — Run shatter explore against shatter-core

## Context

This is the "shatter testing shatter" milestone. We validate end-to-end Rust target support by running `shatter explore` against functions extracted from shatter-core itself. The Rust frontend compiles standalone `.rs` files (it can't import from shatter-core directly), so we create self-contained example files containing real shatter-core algorithms with their necessary type definitions inlined.

## Approach

### 1. Create self-hosting example files

Create `examples/rust/src/self_hosting.rs` containing 4 functions extracted from shatter-core, each with its required types inlined. Selected for: pure logic, numeric/enum parameters, interesting branching, no external dependencies.

**Functions:**

| Function | Source | Params | Branches | Why |
|----------|--------|--------|----------|-----|
| `classify_float` | `float_probe::classify` | `(usize, usize, f64)` | 3 (total==0, ratio>=threshold, else) | Core classification with division-by-zero guard |
| `coverage_percentages` | `CoverageMetrics::percentages` | `(usize, usize, usize, usize, usize)` | 2 (total==0 vs compute) | Real percentage logic from coverage reporting |
| `symexpr_ratio` | `CoverageMetrics::symexpr_ratio` | `(usize, usize)` | 2 (total==0 vs compute) | Simpler ratio with same pattern |
| `executions_agree_simple` | inspired by `float_probe::executions_agree` | `(u64, u64, bool, bool, i64, i64)` | ~5 (path hash mismatch, both error, both ok + values match, mixed) | Multi-condition comparison logic |

Each function will be a standalone `pub fn` with inlined return types (primitives, tuples, or small enums defined in the same file).

### 2. Write integration test

Create `shatter-core/tests/self_hosting_explore.rs` following the existing pattern in `rust_explore_integration.rs`:

- Reuse helpers: `spawn_rust_frontend()`, `analyze_function()`, `instrument_function()`, `execute_function_raw()`, `collect_return_values()`
- For each function:
  1. Analyze → verify params and branches detected
  2. Instrument → verify instrumentation succeeds
  3. Execute with targeted inputs → verify all branches discovered
  4. Execute with boundary inputs → verify distinct paths
- Document expected branches and triggering inputs per CLAUDE.md conventions

### 3. Run and document results

- Run the tests, capture discovered paths and coverage
- Document any bugs or limitations found
- Add comments documenting the self-hosting validation

## Files to create/modify

- **Create**: `examples/rust/src/self_hosting.rs` — self-contained shatter-core functions
- **Create**: `shatter-core/tests/self_hosting_explore.rs` — integration tests
- **Modify**: `examples/rust/src/lib.rs` — add `mod self_hosting;`

## Verification

1. `cargo build --manifest-path shatter-rust/Cargo.toml` (frontend must be built)
2. `cargo test --manifest-path shatter-core/Cargo.toml --test self_hosting_explore` (new tests pass)
3. `cargo test --manifest-path shatter-core/Cargo.toml` (existing tests still pass)
4. `cargo clippy --manifest-path shatter-core/Cargo.toml -- -D warnings` (no warnings)
5. Run `/pre-completion` before declaring done
