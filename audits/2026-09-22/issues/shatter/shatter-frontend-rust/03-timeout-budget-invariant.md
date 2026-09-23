---
slug: timeout-budget-invariant
kind: new
title: "Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout"
priority: P2
type: bug
labels: [timeout, rust-frontend, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Request timeout (30 s) is not longer than the build timeout (30 s CLI / 120 s frontend fallback): cold Rust builds surface as a generic request timeout

## Problem

For a Rust target, the first execute request compiles the harness with cargo and then runs it, all inside one frontend request. The CLI's per-request timeout defaults to 30 s, the same as the CLI-governed build timeout, and shorter than the frontend's own 120 s fallback. So a cold build under load hits the request timeout before the build timeout. The user sees only `request timed out after 30s` with 0 iterations, and nothing points at `--build-timeout` or `--request-timeout`. Closed str-da35 already noted that cold builds needed manual timeout bumps that "should be auto-set". That was never done.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-cli/src/args.rs:561-563`: `--request-timeout`, `default_value_t = 30`. `args.rs:574-577`: `--build-timeout`, `default_value_t = 30`. The same pair recurs at `args.rs:1020/1030`, `1225/1233`, `1339/1347` and `1400/1410` for the other subcommands.
- `PARITY.md:84`: `SHATTER_BUILD_TIMEOUT` / `--build-timeout` governed default `30` s.
- `shatter-rust/src/executor.rs:1017`: `const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 120;` (changed in eeceb7b4). `PARITY.md:96` still says the Rust fallback is 30 s.
- Audit repro under load (finding frontend-rust-04, not re-run): explore on two trivial Rust functions → `concolic observe failed: frontend error: request timed out after 30s` at 32.7 s with 0 iterations. Re-run with `--request-timeout 180 --build-timeout 170` succeeded.

## Acceptance criteria

- [ ] Enforced invariant: for any request that may build (prepare / first execute of a compiled frontend), the effective request timeout is greater than build_timeout + exec_timeout plus a margin. Either derive it when the user has not set `--request-timeout`, or give build-capable requests their own budget. If the user sets values that violate it, warn.
- [ ] Unit test for the invariant covering defaults, user-set build timeout, and user-set request timeout.
- [ ] A request that times out while a build is in progress produces a diagnostic naming `--build-timeout` / `--request-timeout` (and the env vars), not a bare `request timed out after Ns`. Add a test that forces a short request timeout against a slow build.
- [ ] `PARITY.md:96` corrected to the real Rust fallback (120 s), and the `cli_parity_tests` in `shatter-cli/src/helpers.rs` still pass.
- [ ] Proof at close: `cargo test --test e2e_concolic_rust` passes, plus one cold-cache run (fresh `CARGO_TARGET_DIR`) of a Rust explore with default flags that completes. Paste the command and its outcome into the close note.

## Suggested approach

Compute the request timeout in the CLI wiring from the resolved build and exec timeouts, e.g. `max(request_timeout, build_timeout + exec_timeout + 10)` for prepare/first-execute. Leave the 30 s default for requests that cannot build. Check both explorer paths (random `explorer.rs` and concolic `orchestrator.rs`) and every subcommand that takes these flags, per the parallel-parity rule.

## Out of scope

- Reducing cold-build time itself (prefetch, shared target dirs; see str-jyxr).
- Go/TS timeout defaults, beyond keeping the invariant general.

## Size

S

## References

- Finding frontend-rust-04 (audit 2026-09-22). Old draft: `drafts/shatter-code/60-timeout-budget-invariant.md`.
- Related: str-qe9pp (open; conformance `rust/prepare_supported_rust` times out at the 30 s request timeout, a symptom of this), str-da35 (closed; noted that the values should be auto-set), str-jyxr (open; prefetch-timeout symptom).
