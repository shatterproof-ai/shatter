---
slug: timeout-budget-invariant
kind: new
title: "Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout"
priority: P2
type: bug
labels: [timeout, rust-frontend, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [rust-build-deadline-enforcement]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout

## Problem

For a Rust target, the first execute request compiles the harness with cargo and then runs it, all inside one frontend request. The CLI's per-request timeout defaults to 30 s, the same as the CLI-governed build timeout, and shorter than the frontend's own 120 s fallback. So a cold build under load hits the request timeout before the build timeout. The user sees only `request timed out after 30s` with 0 iterations, and nothing points at `--build-timeout` or `--request-timeout`. Closed str-da35 already noted that cold builds needed manual timeout bumps that "should be auto-set". That was never done.

The CLI can only size the request timeout from the build timeout once the frontend actually enforces an aggregate build deadline per request. Today it does not (the build time is checked after cargo exits, and fallback builds each get a fresh budget), which is why this issue is blocked by `rust-build-deadline-enforcement`.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-cli/src/args.rs:561-563`: `--request-timeout`, `default_value_t = 30`. `args.rs:574-577`: `--build-timeout`, `default_value_t = 30`. The same pair recurs at `args.rs:1020/1030`, `1225/1233`, `1339/1347` and `1400/1410` for the other subcommands.
- `PARITY.md:84`: `SHATTER_BUILD_TIMEOUT` / `--build-timeout` governed default `30` s.
- `shatter-rust/src/executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;` (changed in eeceb7b4). `PARITY.md:96` still says the Rust fallback is 30 s.
- `shatter-rust/src/executor.rs:2990-3010`: the build budget is checked only after blocking `Command::output()` returns (see `rust-build-deadline-enforcement`).
- Audit repro under load (finding frontend-rust-04, not re-run): explore on two trivial Rust functions → `concolic observe failed: frontend error: request timed out after 30s` at 32.7 s with 0 iterations. Re-run with `--request-timeout 180 --build-timeout 170` succeeded.

## Acceptance criteria

- [ ] Enforced invariant in the CLI: for any request that may build (prepare / first execute of a compiled frontend), the effective request timeout is greater than build_timeout + exec_timeout + a fixed margin, where build_timeout is the aggregate per-request deadline the frontend enforces after `rust-build-deadline-enforcement`. Either derive it when the user has not set `--request-timeout`, or give build-capable requests their own budget. If the user sets values that violate it, warn once with both values.
- [ ] Unit test for the invariant covering defaults, user-set build timeout, and user-set request timeout, on every subcommand that takes the pair (`args.rs` sites above) and on both explorer paths (random `explorer.rs` and concolic `orchestrator.rs`) per the parallel-parity rule.
- [ ] A request that times out while a build is in progress produces a diagnostic naming `--build-timeout` / `--request-timeout` (and the existing `SHATTER_BUILD_TIMEOUT` env var), not a bare `request timed out after Ns`. Test: a short request timeout against a frontend stub or fake `cargo` that sleeps, asserting the diagnostic text.
- [ ] `PARITY.md:96` corrected to the real Rust fallback (120 s), and the `cli_parity_tests` in `shatter-cli/src/helpers.rs` still pass.
- [ ] Cold-build proof at close, pasted into the close note:
  - The harness build cache must really be cold. Harness builds ignore the caller's `CARGO_TARGET_DIR` and use `SHATTER_HARNESS_CACHE` (`executor.rs:1062-1073`, `standalone_target_dir`), so set `SHATTER_HARNESS_CACHE` to a fresh empty directory (and a fresh `CARGO_TARGET_DIR` for good measure).
  - Run a Rust explore with default timeout flags and `--timing` (or equivalent) and show the `execute.build` phase with a non-trivial duration or cargo `Compiling` lines, proving a build actually happened, and that the explore completed with iterations > 0.
  - The Rust E2E suite with ignored cases included: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` runs nothing; every case is `#[ignore]`d). Paste the `test result:` line showing 0 ignored.

## Suggested approach

Compute the request timeout in the CLI wiring from the resolved build and exec timeouts, e.g. `max(request_timeout, build_timeout + exec_timeout + 10)` for prepare/first-execute. Leave the 30 s default for requests that cannot build. Check both explorer paths and every subcommand that takes these flags.

## Out of scope

- Enforcing the build deadline inside the frontend (`rust-build-deadline-enforcement`).
- Reducing cold-build time itself (shared target dirs, dependency prefetch). str-jyxr is not that work: it is about frontend `generate` requests consuming the input-prefetch budget.
- Go/TS timeout defaults, beyond keeping the invariant general.

## Size

S

## References

- Finding frontend-rust-04 (audit 2026-09-22). Old draft: `drafts/shatter-code/60-timeout-budget-invariant.md`.
- Related: str-qe9pp (open; conformance `rust/prepare_supported_rust` times out at the 30 s request timeout, a symptom of this), str-da35 (closed; noted that the values should be auto-set).
