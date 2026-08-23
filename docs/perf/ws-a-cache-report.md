# WS-A Compilation Cache Validation

Date: 2026-08-22

## Dependency seeding

`scripts/seed-worktree.sh` seeded this issue's fresh linked worktree from the
canonical checkout in 2.06 seconds. The regression test also verifies that
matching lockfiles share file inodes and that a lockfile mismatch runs
`npm ci` in the target worktree.

## Cargo and sccache

`task test-quick` passed after adding the compact development profile to all
three workspace roots. The worktree had no Cargo target directories before the
run. For this validation only, sccache was enabled through the command
environment because machine-level Cargo configuration is operator-managed.

The post-run `sccache --show-stats` result was:

- 2,627 compile requests, 1,955 executed;
- 1,307 cache hits and 636 misses (67% hits among executed requests);
- 697 Rust hits and 610 C/C++ hits;
- zero cache timeouts, read errors, or write errors.

The host has `/usr/bin/sccache`, but no `~/.cargo/config.toml`, mold, or lld.
The operator setup required to make acceleration automatic across worktrees is
documented in `AGENTS.md`; this task intentionally did not mutate user-level
configuration or install system packages.

## Go cache

Inside `shatter-go`, `go env GOCACHE` resolves to the default shared cache
at `/home/ketan/.cache/go-build`. Ordinary Go builds and tests therefore keep
the shared default. Shatter's generated/instrumented builds retain their
intentional workspace-pinned cache behavior.

## Target-directory GC

Before the sweep, `scripts/target-dir-report.sh` found 152,255,740 KiB
(approximately 145 GiB) across registered worktrees. The Bento closure scan
identified 14 fully merged worktrees, then its guarded apply pass preserved
all 14 because each contained uncommitted files. It also pruned one missing
worktree registration whose directory was already absent.

Reclaimed: **0 GiB**. No active, dirty, unmerged, or detached worktree data was
deleted. The report script is now the required first step for future closure
passes so reclaim candidates can be ranked without weakening those safeguards.
