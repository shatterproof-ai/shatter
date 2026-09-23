---
slug: rust-tests-offline-silent-pass
kind: new
title: "38 shatter-rust tests print 'skipping' and count as passed when cargo cannot reach the network"
priority: P2
type: bug
labels: [rust-frontend, tests, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# 38 shatter-rust tests print 'skipping' and count as passed when cargo cannot reach the network

## Problem

Many shatter-rust executor tests build fixture crates. When the build fails with an offline-looking error, the test prints `skipping ...` to stderr and returns, and nextest records that as a pass. The nextest profile has `status-level = "fail"`, so the skip message is never shown. A CI run or sandbox without network (or with a cold registry cache) can therefore report `rust-fe:test` green while testing none of the crate-building paths.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/executor.rs:7611` `fn is_offline_compile_error_message(msg: &str) -> bool` matches messages such as `spurious network error` and `Could not resolve host`.
- There are 16 references to it in `executor.rs`: the definition plus 15 match guards of the form `Err(ExecuteError::CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ..."); }` (e.g. around `:8088`, `:8276`). There are 38 `skipping` occurrences in total.
- `.config/nextest-standalone.toml:12` and `:20`: `status-level = "fail"` (with `final-status-level = "slow"` / `"flaky"`), so passing tests' stderr, including the skip line, is not shown.
- Command to see the count: `/usr/bin/grep -c skipping shatter-rust/src/executor.rs` → `38`.

## Acceptance criteria

- [ ] Offline skips are visible as skips, not passes. Either gate the network-dependent tests behind an env var (e.g. `SHATTER_OFFLINE=1` → `#[ignore]`/nextest filterset exclusion), or make the offline branch fail the test when `CI` is set.
- [ ] CI prefetches the fixture crates' dependencies (a `cargo fetch` step, or reusing the prefetch from str-jyxr), so CI never takes the offline path.
- [ ] `rust-fe:test` reports the number of offline skips, and CI asserts it is 0.
- [ ] Proof at close: run `task rust-fe:test` once with networking disabled (e.g. `CARGO_NET_OFFLINE=true` and an empty `CARGO_HOME`), showing the tests are reported as skipped or failed rather than passed, and once normally, showing 0 skips. Paste both outputs into the close note.

## Suggested approach

Replace the 15 inline guards with one helper, e.g. `fn skip_or_fail_offline(msg) -> !`, that panics under `CI` and otherwise prints a marker that the task wrapper counts. Better still, move the network-dependent tests into a nextest group that the offline profile excludes explicitly.

## Out of scope

- General nextest profile changes (see str-ed38.3 and the tests-ci issues in this epic).
- Reducing how many tests build fixture crates.

## Size

S

## References

- Finding frontend-rust-11 (audit 2026-09-22). Old draft: `drafts/shatter-code/63-rust-tests-offline-silent-pass.md`.
- Related: str-jyxr (open, fixture prefetch timeout).
