# Plan: Shatter Resilience — Timeouts, Memory Limits, Z3 Timeout

## Context

Shatter's defenses against pathological target code are uneven. The Go frontend handles timeouts well, but the TS and Rust frontends ignore `SHATTER_EXEC_TIMEOUT` entirely. The `--timeout-total` scan flag is parsed but never enforced. Z3 has no solver timeout. There are no cross-platform memory guards. This plan addresses all of these gaps.

---

## 1. Language-Specific Default Exec Timeouts

**Go default: 5s, TS default: 15s, Rust default: 5s.** Change the fallback *inside each frontend* (not the CLI). The CLI `--exec-timeout` override still propagates uniformly via the env var.

| File | Change |
|---|---|
| `shatter-go/instrument/executor.go:30` | `10 * time.Second` → `5 * time.Second` |
| `shatter-go/instrument/executor_test.go` | Update `TestExecTimeoutDefaultIs10s` → expect 5s |
| `shatter-ts/src/executor.ts` | Add `getExecTimeoutMs()` reading `SHATTER_EXEC_TIMEOUT`, default 15000ms |
| `shatter-rust/src/handler.rs` | Add `exec_timeout_from_env()`, default 5000ms, store in `Handler` |

## 2. TS Execution Timeout via `vm.runInContext`

Node's `vm.runInContext` accepts `{ timeout: N }` (ms) that throws on expiry via V8's termination mechanism.

**`shatter-ts/src/executor.ts`:**
- Add exported `getExecTimeoutMs()` helper at module top
- Apply `timeout: EXEC_TIMEOUT_MS` to both `vm.runInContext` calls (lines 86, 397)
- In `measureExecution`, the function call happens outside `vm.runInContext`, so the module-load timeout covers module-level hangs; the core's `request_timeout` backstops function-level hangs

**`shatter-ts/src/executor.test.ts`:** Add tests:
- `getExecTimeoutMs` defaults to 15000 when env absent
- `getExecTimeoutMs` parses numeric seconds → ms
- `getExecTimeoutMs` ignores invalid values, returns default
- Module-level infinite loop is aborted by timeout (write temp fixture with `while(true){}`)

## 3. Rust Frontend — Read Exec Timeout

**`shatter-rust/src/handler.rs`:**
- Add `exec_timeout_ms: u64` field to `Handler`
- Add `fn exec_timeout_from_env() -> u64` (mirrors Go pattern), default 5000ms
- Set in `Handler::new()`
- Add unit tests: default, reads env var, ignores invalid/zero/negative

## 4. Z3 Solver Timeout

**How it works:** Z3's Rust crate (`z3 0.19`) provides `Config::set_timeout_msec(ms)` — a context-level timeout that applies to all solvers created within that config. When Z3 hits the deadline, `solver.check()` returns `SatResult::Unknown` with reason `"timeout"`. The existing `check_and_extract` in `solver.rs:675-691` already maps `Unknown` → `SolverError::Unknown(reason)`, so no error-handling changes are needed.

**`shatter-core/src/solver.rs`:**
- Add `solver_timeout_ms: Option<u64>` param to `solve_for_new_path` and `solve_constraints`
- Apply via `cfg.set_timeout_msec(ms)` when `Some`
- Add test: set 1ms timeout on a satisfiable constraint, assert either `Sat` (fast enough) or `Unknown` (timed out) — no panic

**`shatter-core/src/orchestrator.rs:396`:** Pass `solver_timeout_ms` from config through to `solve_for_new_path`

**`shatter-core/src/explorer.rs` `ExploreConfig`:** Add `solver_timeout_ms: Option<u64>` field, default `None`

**`shatter-cli/src/main.rs`:** Add `--solver-timeout <SECS>` (optional) to `Explore`, `Scan`, `Run`. Convert to ms, thread through configs.

## 5. `--timeout-total` Implementation

**`shatter-core/src/scan_orchestrator.rs`:**
- Add `timeout_total: Option<Duration>` to `ScanConfig`
- In `parallel_scan`, record `scan_start = Instant::now()` before layer loop
- At top of each layer iteration, check `scan_start.elapsed() >= total`; if exceeded, push all remaining functions as skipped with reason `"total scan timeout exceeded"` and break
- Layer-boundary check (not mid-layer cancellation) — preserves partial results from current layer

**`shatter-cli/src/main.rs`:**
- Rename `_timeout_total` → `timeout_total` (line 982)
- Pass `Some(Duration::from_secs(timeout_total))` to `ScanConfig` (0 = `None`)

**Test in `scan_orchestrator.rs`:** `parallel_scan` with `timeout_total: Some(Duration::ZERO)` — all functions should appear in `skipped`.

## 6. Cross-Platform Memory Limits

Add `--memory-limit <MB>` (optional, no default) to `Explore`, `Scan`, `Run`, `ExportTests`.

**`shatter-cli/src/main.rs` `frontend_config()`:**
- For TS: insert `--max-old-space-size=<MB>` at position 0 in `config.args` (before script path)
- For Go: push `GOMEMLIMIT=<bytes>` to `config.env_vars`
- Rust frontend: no runtime memory limit available, skip

**Test:** CLI parsing test verifying the flag reaches `FrontendConfig.args`/`env_vars`.

## 7. Integration Tests

| Test | File | What it validates |
|---|---|---|
| `FrontendError::Timeout` | `shatter-core/src/frontend.rs` | Slow frontend script (sleeps 5s) + 100ms `request_timeout` → `Timeout` error |
| TS module timeout | `shatter-ts/src/executor.test.ts` | Fixture with `while(true){}`, 100ms timeout → throw |
| Scan `timeout_total` | `shatter-core/src/scan_orchestrator.rs` | Zero-duration total → all functions skipped |
| Go default change | `shatter-go/instrument/executor_test.go` | Existing test updated to expect 5s |

**New test fixture:** `protocol/slow-frontend.sh` — bash script that sleeps before responding (for `FrontendError::Timeout` test).

## 8. Documentation Updates

**`CLAUDE.md` — add to "Common Task Recipes":**
```
### Frontend timeout contract
All frontends MUST read SHATTER_EXEC_TIMEOUT (seconds) and apply it.
Defaults: Go=5s, TS=15s, Rust=5s.
```

**Per-frontend CLAUDE.md files** — add one-line timeout contract note.

**`demo/walkthrough.sh`** — add stages exercising `--timeout-total` and `--memory-limit`.

---

## Sequencing

Parallel group A (no dependencies): Areas 1+2+3 (frontend timeouts), Area 4 (Z3), Area 8 (docs)
Parallel group B: Area 5 (`timeout_total` in core)
Sequential after A+B: Area 6 (memory limit CLI), Area 7 (integration tests)

## Verification

1. `cargo test` — solver timeout test, scan timeout_total test, frontend timeout test all pass
2. `cargo clippy -- -D warnings` — no new warnings
3. `cd shatter-ts && npm test` — TS timeout tests pass
4. `cd shatter-go && go test ./...` — Go default change test passes
5. `cd shatter-rust && cargo test` — Rust env var tests pass
6. `/walkthrough-review` — new walkthrough stages render correctly
