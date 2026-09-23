---
slug: nextest-ci-profile-and-stale-parity-fallback
kind: new
title: "CI runs plain `cargo test` (nextest `[profile.ci]` is dead config, local and CI runners diverge) and parity-governed keeps a stale 'pending str-7jgm.2' fallback"
priority: P3
type: chore
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI runs plain `cargo test` (nextest `[profile.ci]` is dead config, local and CI runners diverge) and parity-governed keeps a stale 'pending str-7jgm.2' fallback

## Problem

The Rust test tasks use cargo-nextest when it is on PATH and fall back to plain `cargo test` otherwise. CI never installs nextest, so CI and local gates run different test runners, and `.config/nextest.toml`'s `[profile.ci]` (`retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`) is never applied anywhere. Locally, the default profile's `fail-fast = true` means one failing or timed-out test hides the rest of the suite. The audit saw 87 of 3531 tests unrun when `bench_frontier_ranking` timed out, before str-6nul9 excluded that benchmark. In CI, the E2E suites have no per-test timeout at all.

Separately, the `parity-governed` task still carries an `if [ -f scripts/validate-parity.py ] ... else echo "[skip] ... pending str-7jgm.2"` fallback. The script exists and str-7jgm.2 is closed, so the fallback is dead code that would silently turn a deleted script into a skip.

Verifier correction (tests-ci-07): the `rust-frontend-harness` test group (`max-threads = 1`) exists because nextest's process-per-test model defeats the tests' in-process mutexes. Under plain `cargo test` in CI those mutexes do serialize the fixture builds. So "flake serialization only applies locally" is not a problem, and this issue does not claim it. The remaining points are the dead config and the runner divergence.

## Evidence

Re-verified 2026-09-23 against `main` (70465921):

- `.github/workflows/ci.yml`: no cargo-nextest install step and no `NEXTEST_PROFILE` or `--profile ci`. A repo-wide grep finds no reference to the ci profile outside audit drafts.
- `.config/nextest.toml`:
  - `[profile.default]`: `test-threads = 4`, `slow-timeout = { period = "60s", terminate-after = 2 }`, `fail-fast = true`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`, same `rust-frontend-harness` override. Nothing uses it.
- nextest-or-fallback branches: `shatter-core/Taskfile.yml:28-32` (test), `:61-65` (test-ignored), `:88-92` (test-ignored-fast), `shatter-cli/Taskfile.yml:27`, `:46`, `shatter-rust/Taskfile.yml:22`, `shatter-rust-runtime/Taskfile.yml:22`. For example, `:65` falls back to `cargo test -p shatter-core -- --include-ignored --skip bench_frontier_ranking`.
- `Taskfile.yml:265-275` `parity-governed`: `if [ -f scripts/validate-parity.py ]; then python3 scripts/validate-parity.py; else echo "[skip] validate-parity.py not present (pending str-7jgm.2)"; fi`. `scripts/validate-parity.py` exists, and `bd show str-7jgm.2` shows it CLOSED.
- The CI test leaves are currently not executing at all (Task checksum poisoning; see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard`). This issue matters once those land, because CI will then really run the fallback path.

## Acceptance criteria

- [ ] CI installs cargo-nextest (for example `taiki-e/install-action@<pinned>` with `tool: cargo-nextest`) and sets `NEXTEST_PROFILE=ci`, or the Taskfiles pass `--profile ci` when `CI` is set. Alternatively, delete `[profile.ci]` with a comment in `nextest.toml` explaining why CI stays on `cargo test`. Either way, no unused profile remains.
- [ ] The landing gate does not stop at the first failure. Either the profile the gate uses has `fail-fast = false`, or the gate passes `--no-fail-fast`. A failing test's summary lists all failures.
- [ ] If nextest is adopted in CI, the flaky-retry report (`final-status-level = "flaky"`) appears in the CI log. The close reason cites a CI run URL showing the nextest summary line from `core:test-ignored`.
- [ ] The `else` fallback in `parity-governed` is removed, so `python3 scripts/validate-parity.py` runs unconditionally.
- [ ] Proof at close: forced-gate output (`task parity --force`, or with the checksum cleared) showing validate-parity running, plus the CI run URL above.

## Suggested approach

Prefer installing nextest in CI. It removes the local/CI divergence and gives the E2E suites the 120 s per-test terminate-after in CI too. Keep the `cargo test` fallback in the Taskfiles for developer machines without nextest, but make the CI path explicit.

## Out of scope

- Excluding `bench_frontier_ranking` from the gate (done in str-6nul9).
- The Task checksum problem (str-qwua7.3).
- NEXTEST thread budget (str-35vtk.14).

## Dependencies

- None hard-blocking. The work gains most value after the str-qwua7.3 fix (`task-list-json-poisons-checksums`) makes CI test leaves execute.
- Related: str-6nul9 (closed), str-35vtk.7 (closed; wired nextest locally), str-35vtk.14, `ci-executed-leaf-guard`.

Priority: P3 · Type: chore · Labels: ci, quality-gates, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/07, gates-08, tests-ci-07
