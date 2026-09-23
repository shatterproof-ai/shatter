# Following the Rust-frontend hint still fails on undocumented SHATTER_RUNTIME_PATH; `shatter doctor` reports all green

- Priority: P2
- Type: bug
- Labels: rust-frontend,docs,ux,install
- Tracker action: new issue (partially overlaps str-qwua7.40, .20.2, .60 and .13; this issue owns the runtime-crate failure and doctor coverage)
- Related: str-qwua7.40, str-qwua7.20.2, str-qwua7.60, str-qwua7.13
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed)

<!-- body -->
## Problem
Exploring a `.rs` target outside the shatter source tree:
1. Without `shatter-rust`, explore prints a roughly 600-character contributor-oriented hint twice (about 1.3 KB of stderr).
2. With `shatter-rust` on PATH, as the hint instructs, every function fails with `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`. The error repeats once per function, and the failure table labels the language `any`.
3. `SHATTER_RUNTIME_PATH` is not mentioned in README, QUICKSTART, SPEC or any help text.
4. `shatter doctor` exits 0 and does not check the Rust frontend, the runtime crate, the node/go toolchains, or sandbox/host-write readiness.

## Current code facts
- The hint text comes from `shatter-cli/src/helpers.rs:497`.
- `shatter-cli/src/commands/build_frontend.rs:604-630` vendors the runtime for `build-frontend`.
- `shatter-cli/src/commands/doctor.rs` checks version/hash, config and gitignore only.
- The E2E Rust tests run from the workspace root, where the runtime crate is found automatically, so the out-of-tree path is never exercised.

## Acceptance criteria
- `shatter doctor` reports, each with a one-line fix command: Rust frontend resolution, runtime crate location, the node and go toolchains, and sandbox/host-write readiness. It exits non-zero on a hard failure.
- The missing-frontend hint is at most two lines, printed once per run, and points at `shatter doctor`.
- `SHATTER_RUNTIME_PATH` is documented (README install section plus the env-var table from str-qwua7.20.2) until str-qwua7.60 embeds the runtime.
- A test runs a Rust explore from a temp dir outside the repo, with `SHATTER_RUNTIME_PATH` unset, and asserts the actionable error or success.

## Scope
In: doctor checks, hint text, docs. Out: embedding the Rust frontend (str-qwua7.60).
