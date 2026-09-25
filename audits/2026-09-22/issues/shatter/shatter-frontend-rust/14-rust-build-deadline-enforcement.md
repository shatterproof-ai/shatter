---
slug: rust-build-deadline-enforcement
kind: new
title: "shatter-rust build timeout is checked only after cargo exits, and fallback builds each get a fresh budget: no build deadline is actually enforced"
priority: P2
type: bug
labels: [timeout, rust-frontend, harness, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust build timeout is checked only after cargo exits, and fallback builds each get a fresh budget: no build deadline is actually enforced

## Problem

The Rust frontend's build timeout (`--build-timeout` / `SHATTER_BUILD_TIMEOUT`, frontend fallback 120 s) is not a deadline. The harness build runs `cargo build` with blocking `Command::output()` and compares elapsed time to the budget only after cargo has exited. A slow or hung build therefore runs until cargo finishes, however long that takes, and only then turns into a `CompilationFailed` "timed out". A single execute request can also run several builds in sequence (bin-only then crate-bridge routing; whole-file crate-bridge build, then per-candidate and single-function fallbacks), and each build gets the full budget again.

As a result the CLI cannot size its per-request timeout from the build timeout (issue `timeout-budget-invariant`): the real worst-case build time for one request is unbounded.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/executor.rs:2990-3010`: `Command::new("cargo").args(&cargo_args)....output()` (blocking), then `if build_start.elapsed() > build_timeout { return Err(CompilationFailed(..)) }`. No child kill, no wait-with-timeout. The optional `cargo_check_before_build` (`:2988`) runs before it with its own blocking call.
- `executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;`.
- Multiple builds per request: `executor.rs:5535-5861` (crate-bridge whole-file build degrading to per-candidate and single-function fallbacks, see the comments at `:5535`, `:5747`, `:5755`, `:5861`) and `executor.rs:6292-6300` (bin-only first, crate-bridge fallback).

## Acceptance criteria

- [ ] Every cargo invocation the frontend makes for a harness (check, build, fallback builds) runs as a spawned child that is killed when its deadline passes (e.g. `spawn()` + a wait-with-timeout loop or a watchdog thread, with the child's process group killed so rustc children die too). Output is still captured for diagnostics.
- [ ] One aggregate build deadline per execute/prepare request: all builds the request triggers (routing fallback, whole-file, per-candidate, single-function) share the remaining budget instead of each getting a fresh one. When the budget runs out mid-sequence, the request fails with a `CompilationFailed` message that says the build budget was exhausted, names the budget value and `--build-timeout`, and says how many builds were attempted.
- [ ] Test (red on main, green on the branch; paste both into the close note): with a fake `cargo` on `PATH` that sleeps 30 s, and a 2 s build timeout, an execute request returns the budget-exhausted error in under 5 s and leaves no `cargo`/sleep child running.
- [ ] Test: a request whose first build fails fast and whose fallback build is slow is bounded by the one aggregate deadline, not two.
- [ ] `task rust-fe:test` passes, and the Rust E2E suite passes with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing because every case is `#[ignore]`d). Paste the `test result:` line, which must show 0 ignored.

## Suggested approach

Wrap the build commands in one helper `run_cargo_with_deadline(cmd, deadline: Instant)` that spawns, drains stdout/stderr on threads, polls `try_wait`, and kills the process group at the deadline. Thread a `deadline: Instant` computed once per request through the build/fallback functions instead of a `Duration` per call.

## Out of scope

- CLI-side request-timeout sizing and the timeout diagnostic (`timeout-budget-invariant`, blocked by this issue).
- Making builds faster (target-dir sharing, prefetch).

## Size

M

## References

- Split from `timeout-budget-invariant` after the Codex cross-check of the 2026-09-22 audit (finding frontend-rust-04).
- Related: str-da35 (closed; cold builds needed manual timeout bumps).
