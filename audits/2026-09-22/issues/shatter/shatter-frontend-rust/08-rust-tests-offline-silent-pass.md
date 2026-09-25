---
slug: rust-tests-offline-silent-pass
kind: new
title: "shatter-rust tests early-return as passed when a fixture build fails with an offline-looking (or any cargo-mentioning) error"
priority: P2
type: bug
labels: [rust-frontend, tests, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-rust tests early-return as passed when a fixture build fails with an offline-looking (or any cargo-mentioning) error

## Problem

Many shatter-rust executor tests build fixture crates. When the build fails with an error that matches one of two predicates, the test prints `skipping ...` to stderr and returns, and nextest records that as a pass. The nextest profile has `status-level = "fail"`, so the skip message is never shown. A CI run or sandbox without network (or with a cold registry cache) can therefore report `rust-fe:test` green while testing none of the crate-building paths.

The second predicate is much broader than "offline": `cargo_build_unavailable` accepts any error message containing the substring `cargo` or `No such file`. Most real compile failures of a fixture harness mention cargo (paths, `failed to run cargo`, cargo's own output), so some tests may silently pass on genuine regressions, not only when offline.

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/executor.rs:7611` `fn is_offline_compile_error_message(msg)` matches messages such as `spurious network error` and `Could not resolve host`. It guards 15 early-return arms of the form `Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ..."); }` (`executor.rs:8088, 8276, 8317, 8432, 8526, 8627, 8745, 8794, 8902, 8966, 9033, 9093, 9164, 10178, 10588`).
- `executor.rs:11566-11573` `fn cargo_build_unavailable(msg)` returns true if `msg.contains("cargo") || msg.contains("No such file") || ...network strings`. It guards 18 more early-return arms (`executor.rs:7830, 7909, 11611, 11687, 11758, 11857, 12463, 12606, 12672, 12710, 12748, 12842, 12953, 13035, 13124, 13185, 13427, 13438`).
- `shatter-rust/src/handler.rs:1506` and `:2027` add two more `skipping ...: cargo unavailable` early returns in handler tests (the `analyzer.rs:868` hit is only a doc comment). The 38 textual `skipping` occurrences in `executor.rs` are not a count of distinct affected tests.
- `.config/nextest-standalone.toml:12` and `:20`: `status-level = "fail"` (with `final-status-level = "slow"` / `"flaky"`), so passing tests' stderr, including the skip line, is not shown.

## Acceptance criteria

- [ ] Inventory first, recorded in the close note as a table: every test in `shatter-rust/src/**` and `shatter-rust/tests/**` that can return early without asserting (the 33 predicate-guarded arms above, the two `handler.rs` skips, and any other early `return` on an environment condition), with test name and guard.
- [ ] `cargo_build_unavailable`'s bare `"cargo"` and `"No such file"` substrings are removed. A genuine fixture compile error (e.g. a deliberate type error in the fixture) fails the test. Unit test: `cargo_build_unavailable("error[E0308]: mismatched types ... cargo build failed")` is false.
- [ ] Every remaining environment skip goes through one helper that, under `CI` (or a `SHATTER_REQUIRE_NETWORK_TESTS=1` switch the gate sets), panics instead of returning, and otherwise prints a single fixed marker line (e.g. `SHATTER-TEST-SKIP: <test> <reason>`).
- [ ] `rust-fe:test` reports the number of skip markers and fails in CI when it is non-zero. The CI workflow prefetches the fixture crates' dependencies itself (e.g. `cargo fetch --manifest-path` for each fixture, or a warmed `CARGO_HOME` cache step); str-jyxr is unrelated (it is about frontend `generate` requests and the input-prefetch budget) and provides no fixture prefetch.
- [ ] Offline proof at close that reaches the early-return branch rather than failing earlier. An empty `CARGO_HOME` plus offline mode can prevent the outer test crate from compiling at all, which proves nothing. Instead:
  1. Build the test binaries normally: `cargo nextest archive -p shatter-rust --archive-file /tmp/rfe.tar.zst` (or `cargo test -p shatter-rust --no-run` and note the binary path).
  2. Run them with fixture builds forced offline: `CARGO_NET_OFFLINE=true CARGO_HOME=$(mktemp -d) SHATTER_HARNESS_CACHE=$(mktemp -d)` and the archived/prebuilt binary.
  3. Show, on main, the tests reported as passed while stderr (`--no-capture` or `--success-output immediate`) contains the skip lines, i.e. the early-return branch was reached; and on the branch, the same run reporting those tests as failed (with `CI=1`) or explicitly counted as skipped.
  4. One normal networked run showing 0 skip markers.
  Paste all outputs into the close note.

## Suggested approach

Replace both predicates' 33 guards with one helper, e.g. `fn env_skip_or_fail(test: &str, msg: &str)` that matches only well-known network/registry failure strings and panics when `CI` is set. Better still, move the network-dependent tests into a nextest test group that an explicit offline profile excludes, so skips are visible as skips.

## Out of scope

- General nextest profile changes (see str-ed38.3 and the test-hygiene issues in this epic).
- Reducing how many tests build fixture crates.

## Size

M

## References

- Finding frontend-rust-11 (audit 2026-09-22). Old draft: `drafts/shatter-code/63-rust-tests-offline-silent-pass.md`.
