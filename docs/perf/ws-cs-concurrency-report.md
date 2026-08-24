# WS-CS Concurrency-Safety Validation

Date: 2026-08-23

## Concurrent smoke and gauntlet run

The acceptance harness launched two `task --force smoke` processes and two
`demo/gauntlet.sh --auto --delay 0 --step-timeout 300` processes at the same
time from commit `aff81c9f`. Each gauntlet copied its per-run spec, observation,
scan report, and path manifest into a distinct evidence directory before its
private scratch directory was removed.

| Run | Exit | Wall time |
|---|---:|---:|
| smoke-1 | 0 | 9s |
| smoke-2 | 0 | 9s |
| gauntlet-1 | 0 | 379s |
| gauntlet-2 | 0 | 373s |

The mechanical validator parsed every preserved JSON artifact, required a
Markdown scan report from each gauntlet, and rejected references to the other
run's gauntlet or examples directory. It passed. The first attempt used the
default 120-second step timeout; both gauntlets reached all 60 steps but their
parallel-scan step timed out under four-way load. The repository's contention
policy calls for a rerun rather than treating load as a logic failure, so the
successful run used a 300-second per-step budget.

That first attempt also exposed a real demo defect: specification-diff inputs
were human-readable Markdown redirected to `.json` files. The gauntlet now
uses `--spec-out`; focused execution confirmed that both outputs parse as JSON
and `spec-diff` accepts them.

## Shared state fixes

- The reusable examples checkout serializes clone, refresh, and cleanup,
  records a ten-minute freshness marker before returning a fresh clone, and
  never recursively acquires its own lock. Read-only consumers avoid resets
  during the freshness window.
- `BinaryRegistry` persistence uses the existing cross-process build lock,
  reloads and merges the on-disk index while locked, and renames a unique
  temporary file. The regression models two stale processes and proves both
  entries survive.
- Gauntlet specifications, observations, and scan reports use one `mktemp`
  directory per run. Caller-owned `XDG_CACHE_HOME`, `GOCACHE`, and
  `CARGO_TARGET_DIR` values are no longer deleted. Walkthrough likewise
  preserves a caller-owned Cargo target directory.
- `build-examples` uses `<worktree>/target/examples-build`, avoiding writes in
  the shared examples checkout without creating a machine-wide Cargo target.
- The concurrency-headline output directory now uses `mktemp -d`, so two
  invocations in the same second cannot share logs.

## Beads hook latency and backup semantics

`scripts/setup-hooks.sh` now installs an idempotent Shatter environment block
immediately after the shebang in all five Beads hooks: `pre-commit`,
`post-merge`, `pre-push`, `post-checkout`, and `prepare-commit-msg`. The block
defaults `BEADS_HOOK_TIMEOUT` to 30 seconds while preserving an operator
override, and precedes the Beads-managed block that consumes the value.

An isolated detached-worktree checkout took 0.23 seconds with hooks disabled.
With the installed hook path restored, the same operation was bounded at about
30.2 seconds when the local Beads post-checkout operation did not return
promptly. This replaces the Beads shim's 300-second default ceiling. The
persistent repository-local `core.hooksPath=/dev/null` bypass found during the
audit was removed; a subsequent pushed feature commit ran and passed the
restored `check-fast` pre-push gate.

`backup.git-push: true` remains unchanged. Beads v0.63.3 and the installed
v1.1.0 shims set `BD_GIT_HOOK=1`, and automatic backup returns early in hook
context, so network backup work is already outside the hook path. Removing the
legacy key would provide no measured hook benefit and could weaken behavior for
older or future clients. Landing remains the explicit point for exporting the
tracker snapshot and pushing it with `main`.

## Fixed-resource sweep

- `demo/gauntlet-docker.sh` paths are inside a fresh `docker run --rm`
  container for each step and are namespace-isolated.
- `scripts/gate-wrapper.sh` intentionally uses a fixed lock directory and a
  flock-protected timing CSV; those paths coordinate agents rather than hold
  per-run artifacts.
- `/tmp/shatter-examples-main` is intentionally shared and protected by the
  checkout lock/freshness protocol. Fresh demo checkouts use unique
  `shatter-examples.*` directories.
- `scripts/cleanup.sh --tmp` is an explicit garbage-collection command, not a
  runtime consumer; it must not be run concurrently with active demos.
- No fixed listening ports were found under `demo/` or `scripts/`.
