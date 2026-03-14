# str-3ky9.8: E2E test for dynamic mock branch discovery

## Context

The autonomous external dep mocking epic (str-3ky9) has completed CLI mock_params wiring (str-3ky9.6) and downstream raw_results triple update (str-3ky9.7). The example file `examples/standalone/ts/17-mock-branches.ts` already exists (untracked in main) with three functions designed for mock branch testing. What's missing is E2E tests that verify the full pipeline discovers all branches gated by mock return values.

## Approach

### 1. Copy example file to worktree
- Copy `examples/standalone/ts/17-mock-branches.ts` from main repo to worktree (it's untracked)

### 2. Add helper: `instrument_function_with_mocks`
- File: `shatter-core/tests/e2e_concolic.rs`
- Like `instrument_function()` but accepts `mocks: Vec<MockConfig>` instead of hardcoding `vec![]`

### 3. Add three E2E tests to `shatter-core/tests/e2e_concolic.rs`

**Test A: `concolic_mock_status_branches_discovered`** — classifyStatus
- Analyze → get `analysis.dependencies`
- `generate_auto_mocks(deps)` → auto mock configs
- `build_mock_params(deps, configs)` → mock params
- Instrument with mock configs
- Explore with `ExploreConfig { mock_params, mocks, .. }`
- Assert: discovers at least 3 of 4 branches ("ok", "not-found", "server-error", default)
- Assert: `unique_paths >= 3`

**Test B: `concolic_mock_result_branches_discovered`** — loadOrDefault
- Same flow: analyze → auto-mock → build mock_params → instrument → explore
- Assert: discovers "missing", "empty-config", and "loaded:*" branches
- Assert: `unique_paths >= 2` (existsSync true/false + content branching)

**Test C: `concolic_mock_loop_branches_discovered`** — classifyConfigs
- Same flow with seed inputs including arrays of 2-3 paths
- Assert: discovers at least 2 of 3 branches ("all-comments", "no-comments", "mixed")
- Assert: varied mock returns per call enable loop-iteration-level branching

### Key patterns from existing tests
- Use `examples_dir()` for file resolution
- Use `spawn_ts_frontend()` for subprocess
- Use `analyze_function()` then access `analysis.dependencies`
- Use `return_value_set()` for assertions
- Use `ExploreConfig { ..Default::default() }` with generous limits for mock tests
- Call `frontend.shutdown()` at end

### Critical files
- `shatter-core/tests/e2e_concolic.rs` — add tests + helper
- `examples/standalone/ts/17-mock-branches.ts` — copy to worktree (already written)
- `shatter-core/src/auto_mock.rs` — `generate_auto_mocks()`, `build_mock_params()` (use, don't modify)
- `shatter-core/src/orchestrator.rs` — `ExploreConfig`, `explore()` (use, don't modify)
- `shatter-core/src/protocol.rs` — `MockConfig`, `FunctionAnalysis` (use, don't modify)

## Verification

1. `cargo test --test e2e_concolic` — all new tests pass
2. `cargo clippy -- -D warnings` — no warnings in shatter-core/shatter-cli
3. Run `/pre-completion` skill
