---
slug: tests-leak-tmp-dirs
kind: new
title: "Tests leak dirs into shared /tmp"
priority: P2
type: bug
labels: [tests, tempdir, rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tests leak dirs into shared /tmp

## Problem

Test runs on the shared multi-agent machine keep adding entries under `/tmp` (300+ `shatter-crate-bridge-*` harness dirs, ~7 GB, on 2026-09-23), and the growth is accelerating. Shared `/tmp` state also couples concurrent runs to each other: the same class of coupling caused the `discover_configs` flake that blocked landings on 2026-09-19 (str-dl2pj). Agents have been working around it by hand (`rm -rf /tmp/.shatter` 9 times in 3 sessions, and `SHATTER_ALLOW_HOST_WRITES=1` 20 times) instead of fixing the class.

The entries are not all the same kind of problem. Three classes need different fixes:

1. **Intentional caches placed in a shared location.** The bin-only and crate-bridge harness dirs are deliberately retained, content-keyed build caches. Retaining them is by design; the problem is that, when `SHATTER_HARNESS_CACHE` is unset (as in most test runs), they land directly in the shared `temp_dir()` with no owner, bound or cleanup.
2. **Per-run scratch that is not removed on the success path.** Fixed or pid-keyed test paths that nothing deletes.
3. **Per-run scratch removed only on success.** Cleanup that is skipped when a test panics or fails.

## Evidence

Line numbers are for `origin/main` 70465921 (2026-09-23); `shatter-rust/src/executor.rs` is identical at audit HEAD.

- `shatter-rust/src/executor.rs:1067` `harness_cache_root()` returns `None` unless `SHATTER_HARNESS_CACHE` is set. Production fallbacks when it is unset:
  - `:856` `temp_dir().join("shatter-bin-only-{key:016x}")`: bin-only harness cache (class 1).
  - `:3247` `temp_dir().join("shatter-crate-bridge-{key:016x}")`: crate-bridge harness cache (class 1).
  - `:1119` `make_harness_dir()` → `temp_dir().join("shatter-harness-{id}")`: per-subprocess harness dir, documented (:1105-1108) as removed by `PersistentHarnessManager::close_all()` (removals around :5851-:6085). Whether it survives a test panic is unverified.
- Test-only paths:
  - `:1091-1102` `make_request_scratch()` is `#[cfg(test)]`; it falls back to `temp_dir().join("shatter-exec-{id}")` when `SHATTER_HARNESS_SCRATCH` is unset. (An earlier draft wrongly listed it among production fallbacks.)
  - The `mod tests` block (starts at :7608) has 33 `std::env::temp_dir()` call sites (e.g. :10523 `shatter-test-exec-count`, :10548, :10630, :10779, :10809, :11171, :11248-:11337, :11444, :11580-:11811, :12140-:13342). Some already clean up on success; each needs classifying.
  - Generated harness code writes console capture files under `std::env::temp_dir()` (`:2525`, `:2704`, `let __capture_dir = std::env::temp_dir();`). Whether those files are removed is unverified.
- `SHATTER_HARNESS_CACHE` is process-global. Several tests set it with `std::env::set_var` under `ENV_LOCK` (`:11170-11177`, `:11200-11213`, `:11227-11230`, `:11246-11258`, `:11285`, `:11305-11321`), while other tests reach `harness_cache_root()` through production code paths without taking `ENV_LOCK`. Under `cargo test` (threads in one process) a per-test `set_var` can therefore be observed by an unrelated concurrent test. Setting more env vars per test would widen that race.
- `shatter-core/src/scan_orchestrator.rs` stub-frontend flag files under `temp_dir()`: `shatter-id-mismatch-injected-{pid}`, `shatter-dead-after-handshake-{pid}` and `shatter-execute-exits-twice-{pid}` (main :9670/:9803/:9915; audit HEAD :9489/:9622/:9734).
- Counts on 2026-09-23 (`ls -d /tmp/<prefix>* | wc -l`): `shatter-crate-bridge-*` 318 (170 distinct keys, `du -sch` 7.1 GB), `shatter-id-mismatch-injected-*` 87, `shatter-execute-exits-twice-*` 87, `shatter-dead-after-handshake-*` 87, `shatter-bin-only-*` 52, `shatter-gauntlet.*` 19. At audit time (2026-09-22) there were 127 crate-bridge dirs (6.0 GB). Crate-bridge dirs by mtime: 09-19 30, 09-20 40, 09-21 88, 09-22 57.
- Audit finding sessions-08 (verified, P2).

## Acceptance criteria

- [ ] Every `temp_dir()` use in shatter-rust `executor.rs` (production and `mod tests`), the generated-harness capture dir, and the three scan_orchestrator flag files is classified in the issue as class 1, 2 or 3 (or "cleaned correctly, including on panic"), with the chosen fix per entry.
- [ ] Test isolation does not rely on mutating process-global env per test. Tests get their scratch/cache root by injection (a function parameter or struct field threaded to the code under test, e.g. `*_in(root: &Path)` variants), or by running in a separate process (a subprocess helper, or a gate that runs this crate under nextest's process-per-test model and documents that requirement). Any remaining `set_var` of `SHATTER_HARNESS_CACHE`/`SHATTER_HARNESS_SCRATCH` in tests is either removed or every reader of that variable in tests is covered by the same lock.
- [ ] Class 2/3 paths use `tempfile::TempDir` (or an injected root inside one), so they are removed on success and on panic.
- [ ] Class 1: decide explicitly whether the production fallbacks (`:856`, `:3247`, `:1119`) keep using `temp_dir()` when no cache root is configured, or default to a per-user cache dir (e.g. `$XDG_CACHE_HOME/shatter`) with a size or age bound. Record the decision in the issue. Test runs must not populate the shared fallback either way: tests that exercise these caches pass an injected per-test root.
- [ ] A leak check runs as part of the Rust test gates: each gate runs with a gate-private `TMPDIR` and fails if anything remains in it at exit (or, if the gate cannot own `TMPDIR`, a before/after diff of `/tmp/shatter-*` scoped to that gate). Proof at close (red, then green): show the check failing on a scratch commit that adds a deliberately leaking test, then passing on the final branch.
- [ ] Concurrency proof at close: run `cargo test -p shatter-rust` with default test threads twice in parallel (two processes, same machine, different `TMPDIR`s) and paste that both pass and both leak checks report zero entries. Also run it once under `cargo nextest run -p shatter-rust` and paste the result.
- [ ] Proof for shatter-core: run `cargo test -p shatter-core --lib scan_orchestrator` under a private `TMPDIR` and paste the leak-check output showing zero remaining entries.
- [ ] str-dl2pj is linked as related, with a comment that this issue covers the leak class and dl2pj stays scoped to the `discover_configs` walk-up boundary.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Start with the three scan_orchestrator flag files (small and mechanical; pass the path to the stub via an arg or a per-test env on the child process only, never `set_var` in the parent). For shatter-rust, add root-taking variants of `harness_cache_root`-dependent helpers so tests can inject a `TempDir` without touching global env; `harness_scratch_root()` shows the current env-based shape to replace. Cleaning the existing `/tmp` backlog is a one-off manual step for the operator, not part of the fix.

## Out of scope

- The `discover_configs` walk-up boundary itself (str-dl2pj).
- Gauntlet temp dirs already handled by str-jeen.58/str-jeen.64.
- Changing harness caching semantics for real users beyond the class-1 fallback decision above.

## Priority / type / labels

P2 · bug · tests, tempdir, rust-frontend, audit · Size L

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related (not duplicate): str-dl2pj (open, P1; config discovery boundary; verified open with `bd show` on 2026-09-23). Prior related fixes: str-ri1z, str-jeen.64, str-jeen.58 (closed).
