---
slug: rust-crate-bridge-stdout
kind: new
title: "Rust crate-bridge harness shares stdout with user code: any target that prints fails as internal_error"
priority: P1
type: bug
labels: [rust-frontend, crate-bridge, harness, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust crate-bridge harness shares stdout with user code: any target that prints fails as internal_error

## Problem

The crate-bridge driver writes its JSON result line with `println!` to the same stdout the target function writes to. It redirects no file descriptors. The reader in `PersistentHarness::execute` treats the first stdout line as the protocol response. So when a target function calls `println!` (or `print!` without a newline), the protocol line is corrupted, and the execution is reported as `internal_error` / `runtime_failed` instead of returning the function's value.

Crate-bridge is not an opt-in mode. It is the automatic fallback whenever the bin-only dispatch path returns `NonExecutable` for a file inside a crate, so real crates hit it without asking. The standalone and dispatch harnesses already redirect fd 1/fd 2 with `libc::dup2`. Only crate-bridge does not.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files; line numbers unchanged since 56c86168):

- `shatter-rust/src/executor.rs:5195`: the generated crate-bridge driver emits `println!("{}", serde_json::to_string(&exec_result).unwrap());`, and nothing around it redirects fds.
- `dup2` appears only in the standalone generator (`executor.rs:2535-2561`) and the dispatch generator (`executor.rs:2714-2881`). Both emit unconditional Unix-only code (`libc::dup`, `std::os::unix::io::AsRawFd`) with no `cfg(windows)` branch, and crate-bridge cannot add a `libc` dependency to the user's crate.
- `executor.rs:627-647` `PersistentHarness::execute` reads one line from `response_rx` and runs `serde_json::from_str(&line)`. On failure it returns `OutputParseError("failed to parse execute result: ...\nline: {line}")`.
- `executor.rs:6292-6300`: crate-backed files go to bin-only and fall back to crate_bridge unless the caller requested a harness mode explicitly.
- `protocol/parity-matrix.yaml:492,496` and `shatter-rust/CLAUDE.md:21,23` record only that "crate-bridge does not capture console output". They do not record that user output corrupts the protocol channel.
- `.github/workflows/release.yml:87-93` ships `shatter-rust.exe` for `x86_64-pc-windows-msvc`, and maintainer decision D1 (2026-09-23) keeps Windows in the release matrix. A POSIX-only fix would leave crate-bridge on Windows either broken or uncompilable.
- Audit probe (finding frontend-rust-01): a crate containing `pub fn noisy(n: i64) -> i64 { println!("hello from user code {n}"); n + 1 }`, executed with `harness_mode: crate_bridge`, returned `error internal_error output parse error: failed to parse execute result: expected value at line 1 column 1 line: hello from user code 1`, with outcome `runtime_failed`. A sibling non-printing function worked after the harness restarted.

## Acceptance criteria

- [ ] Protocol responses travel on a channel that user code cannot write to through `print!`/`println!`/`eprint!`/`std::io::stdout()`. A sentinel or prefix scheme on the shared stdout pipe does **not** satisfy this item: a partial `print!` line concatenates with the response, and user output can imitate any sentinel.
- [ ] The design works on every platform the release matrix ships `shatter-rust` for, including `x86_64-pc-windows-msvc` (D1). The close note names the mechanism per platform (e.g. POSIX `dup`/`dup2` via an `extern "C"` shim on Unix and `SetStdHandle`/a separate handle on Windows, or a platform-neutral response file/pipe whose path the frontend passes to the driver). If the Windows path cannot be exercised in CI yet, the generated driver must at least compile for `x86_64-pc-windows-msvc` (`cargo check --target x86_64-pc-windows-msvc` on a generated crate-bridge crate, output pasted), and the close note links the issue tracking Windows harness execution.
- [ ] Regression tests, each shown failing on main and passing on the branch (paste both runs into the close note), over a crate-bridge-routed crate in one persistent harness process:
  - `println!` before returning: `noisy(1) == 2`, no `internal_error`.
  - `print!` with no trailing newline before returning.
  - user output that is itself valid JSON or looks like an execute result (e.g. `println!("{{\"return_value\":99}}")`), which must not be taken as the response.
  - writes to stderr.
  - three successive requests on the same harness process, alternating printing and non-printing functions, each returning its own correct value (no response shifted onto the next request).
- [ ] Console output from crate-bridge is either captured as `console_output` or deliberately discarded. Whichever is chosen, `protocol/parity-matrix.yaml` and the side-effect contract in `shatter-rust/CLAUDE.md` state it, and `task parity` plus `task conformance` pass.
- [ ] A new known-answer case in `shatter-core/tests/e2e_concolic_rust.rs` routes a printing target through crate-bridge (assert the harness mode in the test). Proof: `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust -- --include-ignored` (the body of `task e2e-rust-governed`). Plain `cargo test --test e2e_concolic_rust` is not proof: every case is `#[ignore]`d and it reports success with 0 run. Paste the `test result:` line; it must show 0 ignored and include the new test.

## Suggested approach

Save the original stdout once at driver startup, point fd 1/fd 2 (or the Windows std handles) at a capture file or null device around each call, and write responses only to the saved handle. The standalone and dispatch generators do the per-call part already on Unix. Because crate-bridge cannot add `libc` to the user's crate, use a small `extern "C"` declaration for `dup`/`dup2` under `cfg(unix)` and the `SetStdHandle`/`GetStdHandle` Win32 calls under `cfg(windows)`, or avoid fd juggling entirely with a response file whose path comes in the request.

## Out of scope

- Capturing other side-effect kinds in crate-bridge.
- Changing the bin-only / crate-bridge routing policy.
- Malformed-request handling in the same generated loop (`executor.rs:5179-5182`), tracked by `rust-runtime-harness-loop`.
- Making the standalone and dispatch harnesses Windows-capable (they are Unix-only today; file separately if not already tracked).

## Size

M

## References

- Finding frontend-rust-01 (audit 2026-09-22, `audits/2026-09-22/findings.json`). Old draft: `drafts/shatter-code/58-rust-crate-bridge-stdout.md`.
- Related closed crate-bridge work (not duplicates): str-qsrb, str-70m2.
- Maintainer decision D1 (Windows and aarch64 releases kept), `audits/2026-09-22.md`.
