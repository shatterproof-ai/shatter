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

The crate-bridge driver writes its JSON result line with `println!` to the same stdout the target function writes to. It redirects no file descriptors. The reader in `PersistentHarness::execute` treats the first stdout line as the protocol response. So when a target function calls `println!`, the protocol line is corrupted, and the execution is reported as `internal_error` / `runtime_failed` instead of returning the function's value.

Crate-bridge is not an opt-in mode. It is the automatic fallback whenever the bin-only dispatch path returns `NonExecutable` for a file inside a crate, so real crates hit it without asking. The standalone and dispatch harnesses already redirect fd 1/fd 2 with `dup2`. Only crate-bridge does not.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/executor.rs:5195`: the generated crate-bridge driver emits `println!("{}", serde_json::to_string(&exec_result).unwrap());`, and nothing around it redirects fds.
- `dup2` appears only in the standalone generator (`executor.rs:2541-2561`) and the dispatch generator (`executor.rs:2720-2881`).
- `executor.rs:627-647` `PersistentHarness::execute` reads one line from `response_rx` and runs `serde_json::from_str(&line)`. On failure it returns `OutputParseError("failed to parse execute result: ...\nline: {line}")`.
- `executor.rs:6292-6300`: crate-backed files go to bin-only and fall back to crate_bridge unless the caller requested a harness mode explicitly.
- `protocol/parity-matrix.yaml:492,496` and `shatter-rust/CLAUDE.md:21,23` record only that "crate-bridge does not capture console output". They do not record that user output corrupts the protocol channel.
- Audit probe (finding frontend-rust-01): a crate containing `pub fn noisy(n: i64) -> i64 { println!("hello from user code {n}"); n + 1 }`, executed with `harness_mode: crate_bridge`, returned `error internal_error output parse error: failed to parse execute result: expected value at line 1 column 1 line: hello from user code 1`, with outcome `runtime_failed`. A sibling non-printing function worked after the harness restarted.

## Acceptance criteria

- [ ] The crate-bridge protocol uses a channel user code cannot write to. Either (a) dup the original stdout to a private fd at driver startup and point fd 1 (and fd 2 if needed) at a capture file or `/dev/null` around each call, or (b) prefix protocol lines with a sentinel and have the reader skip lines without it.
- [ ] Regression test, first failing and then passing on the branch: a crate-bridge-routed crate with a printing function returns its value (e.g. `noisy(1) == 2`) with no `internal_error`. Record the failing run's output in the close note.
- [ ] Console output from crate-bridge is either captured as `console_output` or deliberately discarded. Whichever is chosen, `protocol/parity-matrix.yaml` and the side-effect contract in `shatter-rust/CLAUDE.md` state it, and `task parity` plus `task conformance` pass.
- [ ] `cargo test --test e2e_concolic_rust` passes. Add an E2E known-answer case with a printing target in a crate if none of the existing cases routes through crate-bridge.

## Suggested approach

Option (a) matches what the standalone and dispatch harnesses already do. The finding notes that crate-bridge cannot add a `libc` dependency to the user crate, so use `std::os::fd` (`OwnedFd`, `File::from`) with a small `unsafe` `dup`/`dup2` shim via `extern "C"`, or reopen `/dev/stdout` into a private handle before the first call. Option (b) is simpler but still interleaves user output with protocol output on one pipe, and a user line that happens to start with the sentinel would still break it.

## Out of scope

- Capturing other side-effect kinds in crate-bridge.
- Changing the bin-only / crate-bridge routing policy.

## Size

M

## References

- Finding frontend-rust-01 (audit 2026-09-22, `audits/2026-09-22/findings.json`). Old draft: `drafts/shatter-code/58-rust-crate-bridge-stdout.md`.
- Related closed crate-bridge work (not duplicates): str-qsrb, str-70m2.
