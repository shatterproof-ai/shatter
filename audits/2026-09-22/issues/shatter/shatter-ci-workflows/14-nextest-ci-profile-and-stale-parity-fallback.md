---
slug: nextest-ci-profile-and-stale-parity-fallback
kind: new
title: "CI runs plain `cargo test` while local gates use nextest; both nextest configs carry an unused `[profile.ci]` and `fail-fast = true`: pick one runner policy and make CI apply it"
priority: P3
type: chore
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI runs plain `cargo test` while local gates use nextest; both nextest configs carry an unused `[profile.ci]` and `fail-fast = true`: pick one runner policy and make CI apply it

(The slug is kept for filer stability. The stale `parity-governed` fallback that this draft also used to cover was split into `parity-governed-stale-fallback`.)

## Problem

The Rust test tasks use cargo-nextest when it is on PATH and fall back to plain `cargo test` otherwise. CI never installs nextest, so CI and local gates run different test runners.

Two nextest config files each define a `[profile.ci]` that nothing selects:

- `.config/nextest.toml`, used by the workspace crates;
- `.config/nextest-standalone.toml`, used explicitly by `shatter-rust` and `shatter-rust-runtime` through `--config-file`.

In both files, `[profile.default]` has `fail-fast = true`, so locally one failing or timed-out test hides the rest of the suite. The audit saw 87 of 3531 tests unrun when `bench_frontier_ranking` timed out, before str-6nul9 excluded that benchmark. In CI, the E2E suites have no per-test timeout at all.

Verifier correction (tests-ci-07): the `rust-frontend-harness` test group (`max-threads = 1`) exists because nextest's process-per-test model defeats the tests' in-process mutexes. Under plain `cargo test` in CI those mutexes do serialize the fixture builds. So "flake serialization only applies locally" is not a problem, and this issue does not claim it. What remains is the dead config, fail-fast, and the runner divergence.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `.github/workflows/ci.yml` has no cargo-nextest install step and no `NEXTEST_PROFILE` or `--profile ci`. Its only explicit cargo test is `cargo test -p shatter-llm` (`:103`). A repo-wide grep finds no reference to the ci profile outside audit drafts.
- `.config/nextest.toml`:
  - `[profile.default]`: `test-threads = 4`, `slow-timeout = { period = "60s", terminate-after = 2 }`, `fail-fast = true`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`, and the same `rust-frontend-harness` override. Nothing selects it.
- `.config/nextest-standalone.toml`:
  - `[profile.default]`: `fail-fast = true`, `final-status-level = "slow"`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`. Nothing selects it.
  - It is consumed by `shatter-rust/Taskfile.yml:24` and `shatter-rust-runtime/Taskfile.yml:24` (`cargo nextest run --config-file ../.config/nextest-standalone.toml`), and listed as a source at root `Taskfile.yml:401`.
- The nextest-or-fallback branches are at `shatter-core/Taskfile.yml:28-32` (test), `:61-65` (test-ignored), `:88-92` (test-ignored-fast), `shatter-cli/Taskfile.yml:27` and `:46`, `shatter-rust/Taskfile.yml:22`, and `shatter-rust-runtime/Taskfile.yml:22`. For example, `:65` falls back to `cargo test -p shatter-core -- --include-ignored --skip bench_frontier_ranking`.
- The CI test leaves are currently not executing at all, because of Task checksum poisoning (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard`). This issue matters once those land, because CI will then really run the fallback path.

## Acceptance criteria

- [ ] **Decide the policy** and write it in a comment at the top of both nextest config files. The options are:
  - **(A) adopt nextest in CI.** CI installs cargo-nextest (for example `taiki-e/install-action@<pinned sha>` with `tool: cargo-nextest`) and selects the `ci` profile for every nextest invocation, workspace and standalone, through `NEXTEST_PROFILE=ci` or by passing `--profile ci` when `CI` is set.
  - **(B) keep `cargo test` in CI.** Delete `[profile.ci]` from **both** files, with a comment explaining why CI stays on `cargo test`.

  Either way, no unused profile remains in either file.
- [ ] **No fail-fast in the gate.** Both default profiles have `fail-fast = false`, or every gate invocation passes `--no-fail-fast` (both nextest and cargo test accept it). Show a forced gate run (`task core:test --force`, or with its checksum cleared) on a scratch branch with two deliberately failing tests, in which the summary lists both failures. Paste the output in the close reason, then revert.
- [ ] **Close-time proof, by option:**
  - (A): cite a `ci.yml` run URL whose log shows nextest's summary line from `core:test-ignored` and from one standalone crate, run under the `ci` profile (the profile name is visible in the nextest header). This requires `ci-executed-leaf-guard` to have landed, so that the leaves execute.
  - (B): cite the diff removing both `[profile.ci]` sections, plus the forced-gate output above. No CI URL is required.

## Suggested approach

(A) is preferred. It removes the local/CI divergence and gives the E2E suites the 120 s per-test terminate-after in CI too. Keep the `cargo test` fallback in the Taskfiles for developer machines without nextest, but make the CI path explicit.

## Out of scope

- The `parity-governed` stale fallback (`parity-governed-stale-fallback`).
- Excluding `bench_frontier_ranking` from the gate (done in str-6nul9).
- The Task checksum problem (str-qwua7.3).
- The NEXTEST thread budget (str-35vtk.14).

## Dependencies

- None hard-blocking. Option (A)'s CI proof needs `ci-executed-leaf-guard` (shatter-gates-integrity bucket) to have landed.
- Related: str-6nul9 (closed), str-35vtk.7 (closed; wired nextest locally), str-35vtk.14, `ci-executed-leaf-guard`, `parity-governed-stale-fallback`.

Priority: P3 · Type: chore · Labels: ci, quality-gates, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/07, gates-08, tests-ci-07
