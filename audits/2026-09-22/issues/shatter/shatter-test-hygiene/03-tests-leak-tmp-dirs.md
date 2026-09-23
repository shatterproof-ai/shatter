---
slug: tests-leak-tmp-dirs
kind: new
title: "Tests leak per-run directories into shared /tmp (300+ crate-bridge harness dirs, ~7 GB); give tests filesystem isolation and a leak check"
priority: P2
type: bug
labels: [tests, tempdir, rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tests leak per-run directories into shared /tmp (300+ crate-bridge harness dirs, ~7 GB); give tests filesystem isolation and a leak check

## Problem

When `SHATTER_HARNESS_CACHE` is unset, the Rust frontend's harness paths fall back to fixed or keyed names under `std::env::temp_dir()`. Several shatter-core stub-frontend tests also write flag files under `temp_dir()`. Nothing removes these entries, so every test run on the shared multi-agent machine adds to `/tmp`, and the growth is accelerating. Shared `/tmp` state also couples concurrent runs to each other: the same class of coupling caused the `discover_configs` flake that blocked landings on 2026-09-19 (str-dl2pj). Agents have been working around it by hand (`rm -rf /tmp/.shatter` 9 times in 3 sessions, and `SHATTER_ALLOW_HOST_WRITES=1` 20 times) instead of fixing the class.

## Evidence

Line numbers are for `origin/main` 70465921 (2026-09-23).

- `shatter-rust/src/executor.rs:1067` `harness_cache_root()` returns `None` unless `SHATTER_HARNESS_CACHE` is set. Fallbacks:
  - `:856` `std::env::temp_dir().join(format!("shatter-bin-only-{key:016x}"))`. This is the bin-only harness. (Verifier correction: :856 is bin-only, not crate-bridge.)
  - `:3247` `std::env::temp_dir().join(format!("shatter-crate-bridge-{key:016x}"))` is the crate-bridge harness.
  - `:1101` `shatter-exec-{id}` and `:1119` `shatter-harness-{id}` are similar fallbacks.
  - Test-only fixed paths are at `:10523` `shatter-test-exec-count`, `:10548`, `:10630` and `:10779` (`shatter-test-*`).
- `shatter-core/src/scan_orchestrator.rs` test flag files under `temp_dir()`: `:9670` `shatter-id-mismatch-injected-{pid}`, `:9803` `shatter-dead-after-handshake-{pid}` and `:9915` `shatter-execute-exits-twice-{pid}`. These were at :9489/:9622/:9734 at audit HEAD 56c86168.
- Counts on 2026-09-23 (`ls -d /tmp/<prefix>* | wc -l`): `shatter-crate-bridge-*` 318 (170 distinct keys, `du -sch` 7.1 GB), `shatter-id-mismatch-injected-*` 87, `shatter-execute-exits-twice-*` 87, `shatter-dead-after-handshake-*` 87, `shatter-bin-only-*` 52 and `shatter-gauntlet.*` 19. At audit time (2026-09-22) there were 127 crate-bridge dirs (6.0 GB), then 264. Crate-bridge dirs by mtime: 09-19 30, 09-20 40, 09-21 88, 09-22 57.
- Audit finding sessions-08 (verified, P2).

## Acceptance criteria

- [ ] No test writes to a shared `std::env::temp_dir()` path. Rust-frontend tests set `SHATTER_HARNESS_CACHE` (or an equivalent per-test root) to a `tempfile::TempDir`. The shatter-core stub-frontend flag files live in a per-test `TempDir` that is passed to the stub through env. The fixed `shatter-test-*` paths in `executor.rs` tests use `TempDir`.
- [ ] Decide explicitly whether the production fallbacks (`:856`, `:1101`, `:1119`, `:3247`) should keep using `temp_dir()` when no cache root is configured, or default to a per-user cache dir (e.g. `$XDG_CACHE_HOME/shatter`). Record the decision in the issue. If they stay, document the cleanup story.
- [ ] A test-hygiene check fails when a test run leaves new `/tmp/shatter-*` entries. For example, a wrapper that sets a private `TMPDIR` for the gate and asserts it is empty afterwards, or a before/after diff of `/tmp/shatter-*` in drift-patrol or CI.
- [ ] Proof at close: from a clean private `TMPDIR`, run `cargo test -p shatter-rust` and `cargo test -p shatter-core --lib scan_orchestrator` and paste the output of the leak check showing zero new entries. Also show the check failing on a deliberately leaking test before the fix, or on a scratch commit.
- [ ] str-dl2pj is linked as related. Note on it that this issue covers the leak class, and that dl2pj stays scoped to the `discover_configs` walk-up boundary.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Start with the three scan_orchestrator flag files (small and mechanical). Then thread a harness cache root through the shatter-rust test helpers; `harness_scratch_root()` (`SHATTER_HARNESS_SCRATCH`, `#[cfg(test)]`) already exists as a model. For the leak check, running each Rust test gate under a gate-private `TMPDIR` and asserting that it is empty at exit catches every leak class at once, including new ones, without listing prefixes. Cleaning the existing `/tmp` backlog is a one-off manual step for the operator, not part of the fix.

## Out of scope

- The `discover_configs` walk-up boundary itself (str-dl2pj).
- Gauntlet temp dirs already handled by str-jeen.58/str-jeen.64.
- Changing harness caching semantics for real users beyond the fallback decision above.

## Priority / type / labels

P2 · bug · tests, tempdir, rust-frontend, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related (not duplicate): str-dl2pj (open, P1; config discovery boundary). Prior related fixes: str-ri1z, str-jeen.64, str-jeen.58 (closed).
