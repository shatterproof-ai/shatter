# WS-CS Concurrency-Safety Validation

Date: 2026-08-23

## Concurrent smoke and gauntlet run

The acceptance harness launched two `task --force smoke` processes and two
`demo/gauntlet.sh --auto --delay 0 --step-timeout 300` processes at the same
time from commit `87da0048`. Each gauntlet copied its per-run spec, observation,
scan report, and path manifest into a distinct evidence directory before its
private scratch directory was removed.

The exact validator is tracked as `scripts/concurrency-safety-check.sh`. Run it
from any checkout with:

```bash
scripts/concurrency-safety-check.sh
```

It prints the durable run directory and preserves logs, result records, JSON,
Markdown, path manifests, and a summary there for independent inspection.
The final candidate run is preserved at
`/tmp/str-35vtk4-acceptance.5m1c46` on the validation host.

| Run | Exit | Wall time |
|---|---:|---:|
| smoke-1 | 0 | 19s |
| smoke-2 | 0 | 18s |
| gauntlet-1 | 0 | 391s |
| gauntlet-2 | 0 | 391s |

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

- The reusable examples cache serializes clone, validation, refresh, and
  cleanup. Callers receive an immutable local clone keyed by canonical Git
  commit and published atomically while the cache lock is held, so a later
  refresh cannot mutate an in-flight reader even after the ten-minute refresh
  window expires. Calls at the same commit reuse one snapshot. `--no-update`
  also waits for and validates a complete canonical clone under that lock.
- `BinaryRegistry` persistence reloads the on-disk index under a cross-process
  lock and applies only the current registration, preventing a stale instance
  from rolling back a newer value. Lock and temporary files live in a sibling
  state directory outside `binaries/`, where concurrent workspace GC cannot
  remove them. Example and Rapid state-machine regressions cover stale
  same-key updates, unrelated keys, and registration concurrent with GC.
- Gauntlet specifications, observations, and scan reports use one `mktemp`
  directory per run. Caller-owned `XDG_CACHE_HOME`, `GOCACHE`, and
  `CARGO_TARGET_DIR` values are no longer deleted. Walkthrough likewise
  preserves a caller-owned Cargo target directory.
- `build-examples` uses `<worktree>/target/examples-build`, avoiding writes in
  the shared examples checkout without creating a machine-wide Cargo target.
- The concurrency-headline output directory now uses `mktemp -d`, so two
  invocations in the same second cannot share logs.

## Beads hook latency and backup semantics (deferred, str-mpgg1)

An earlier version of this branch installed a `BEADS_HOOK_TIMEOUT=30`
environment block into all five git hooks via `scripts/setup-hooks.sh`,
including `post-merge` and `post-checkout`. That change has been reverted
(str-mpgg1): `AGENTS.md`'s "Leave the managed git hooks alone" rule
explicitly designates `.git/hooks/post-merge` and `post-checkout` as
beads-managed and off-limits to hand-editing, since they are shared across
every worktree via the common git dir. Installing a timeout block into them
— even from a repo-tracked installer script rather than a direct edit to the
live hook files — has the same shared-blast-radius effect that rule guards
against, and doing so here was not a deliberate, reviewed policy exception.

Item 3 of the original issue (beads hook timeout, `backup.git-push`
semantics) is deferred pending an explicit AGENTS.md amendment proposed and
reviewed on its own, rather than landed silently alongside unrelated
concurrency fixes. Investigation notes for that future work:
`backup.git-push: true`'s network I/O risk is really about how often `bd
sync` gets invoked inside hooks (not the state-transition commands like `bd
create`/`update`/`close`), and repo convention already says "sync once at
landing" — so the actual leverage is in hook invocation frequency, not the
backup flag itself. Beads also sets `BD_GIT_HOOK=1` while running managed
hooks, which already suppresses automatic backup from those hook processes.

## Fixed-resource sweep

- `demo/gauntlet-docker.sh` paths are inside a fresh `docker run --rm`
  container for each step and are namespace-isolated.
- `scripts/gate-wrapper.sh` intentionally uses a fixed lock directory and a
  flock-protected timing CSV; those paths coordinate agents rather than hold
  per-run artifacts.
- `/tmp/shatter-examples-main` is a shared cache protected by the checkout
  lock. Consumers reuse immutable commit-keyed clones under
  `/tmp/shatter-examples-snapshots/`; fresh demo checkouts use unique
  `shatter-examples.*` directories.
- `scripts/cleanup.sh --tmp` is an explicit garbage-collection command, not a
  runtime consumer; it must not be run concurrently with active demos, gates,
  or profiling processes that may be reading an examples snapshot.
- No fixed listening ports were found under `demo/` or `scripts/`.
