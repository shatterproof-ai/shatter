---
slug: pin-examples-repo
kind: new
title: "Examples repo test input unpinned"
priority: P2
type: task
labels: [testing, examples, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Examples repo test input unpinned

## Problem

Unit, E2E, smoke, TS and walkthrough tests read `SHATTER_EXAMPLES_DIR`, a checkout of `shatterproof-ai/examples` that `scripts/examples_checkout.py` resets to `origin/main` whenever it is more than 10 minutes old. No commit is pinned, and no Task fingerprint covers the examples content. As a result:

- the same shatter commit can pass or fail depending on when the gate ran;
- a change to the examples repo can break shatter's gates with no shatter commit to bisect;
- a cached Task result can be served for a gate whose inputs (the examples) have changed, because the only fingerprinted input is the script itself.

Pinning only the canonical checkout is not enough. The per-consumer snapshot that tests actually read is published by cloning the canonical checkout's `main` branch, not its HEAD commit, so the snapshot content must be pinned too.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `scripts/examples_checkout.py:19` `DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"`, `:20` `DEFAULT_BRANCH = "main"`, `:21` `DEFAULT_DIR = <tempdir>/shatter-examples-main`, `:48` `REFRESH_WINDOW_SECONDS = 600`.
- `scripts/examples_checkout.py:130-139` (`_refresh_checkout_locked`): returns early if refreshed within the window; otherwise `fetch origin main`, `checkout main`, `reset --hard origin/main`, `clean -fdx`.
- `scripts/examples_checkout.py:188-233` (`_snapshot_shared_checkout_locked`): the snapshot cache directory is keyed by the canonical checkout's `rev-parse HEAD` (:191, :197), but the snapshot itself is made with `git clone --local --no-hardlinks --branch main <checkout>` (:213-226, `--branch` at :220). If the canonical checkout's HEAD is detached at a pin while its local `main` branch points elsewhere, the snapshot published under the pinned SHA's directory contains `main`'s content. Detaching HEAD alone therefore does not pin what consumers read.
- A direct fallback bypasses the script: `shatter-rust/src/executor.rs:11444` (test code) uses `std::env::temp_dir().join("shatter-examples-main")` when `SHATTER_EXAMPLES_DIR` is unset.
- Consumers of `SHATTER_EXAMPLES_DIR` include `Taskfile.yml` (tasks at :76, :131, :159, :606, :625), `shatter-core/Taskfile.yml` (test-ignored/-fast), `shatter-ts/Taskfile.yml`, `shatter-core/tests/e2e_concolic{,_go,_rust}.rs`, `e2e_llm_oracle.rs`, `bench_frontier_ranking.rs`, `tests/support/rust_frontend_harness.rs`, `shatter-ts/src/{executor,handlers}.test.ts`, `shatter-rust/src/executor.rs` and `scripts/perf_runner.py`.
- The Task `sources:` lists contain `scripts/examples_checkout.py` (e.g. `Taskfile.yml:128,156,598`), but nothing that changes when the examples content changes. `test-standard` has no `sources:` of its own; the checksum-cached unit is the internal `workspace-test` task.
- There is no lock or pin file (`git ls-files | grep -i examples` shows no SHA file).
- Related closed issues: str-35vtk.4 (locking for the shared checkout), str-w5ry (refresh crash) and str-91yri (PR auth). None pins a SHA.
- Audit finding tests-ci-09 (verified).

## Acceptance criteria

- [ ] A tracked lock file (e.g. `examples.lock` holding a full 40-char SHA) pins the examples commit.
- [ ] The canonical checkout is brought to exactly that SHA (fetch the SHA if missing, then `checkout --detach <sha>`); nothing resets to a moving branch. The 10-minute refresh window either goes away or only governs whether a fetch is attempted, never which commit is checked out.
- [ ] The published snapshot is created from the pinned SHA, not from a branch name (e.g. clone then `checkout --detach <sha>`, or `git worktree`/`archive` of the SHA), and the snapshot cache is keyed by the pinned SHA.
- [ ] Before returning a path, the script verifies that the returned snapshot's `git rev-parse HEAD` equals the pinned SHA and that `git status --porcelain` in it is empty; on mismatch it fails loudly rather than returning.
- [ ] Unit tests in `scripts/test_walkthrough_examples_checkout.py` (or a sibling test file), using a local bare repo as origin, cover:
  - fresh clone: returned snapshot HEAD == pin;
  - recently refreshed canonical checkout (inside the refresh window) whose local `main` has moved: returned snapshot HEAD == pin, not `main`;
  - two callers with different pins in the same process/cache dir: each gets a snapshot at its own pin;
  - an already-published snapshot directory whose HEAD does not match its key: rejected.
  Proof at close: show at least the "moved local main" test failing against the current script (red), then passing after the fix (green).
- [ ] The `shatter-rust/src/executor.rs:11444` fallback either goes through the pinned checkout or fails with a message telling the developer to set `SHATTER_EXAMPLES_DIR` via the script.
- [ ] Every checksum-cached Task that exports `SHATTER_EXAMPLES_DIR` lists the lock file in its `sources:`. Proof at close: on a scratch branch, change the SHA in the lock and show `task --status workspace-test` (and the other affected cached tasks) exiting non-zero (not up to date), where it exited zero before the change.
- [ ] A short documented bump procedure (README or `docs/`): edit the lock, open a PR, and let the normal gates run. Bumping the pin is an ordinary gated change.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Keep the existing shared-checkout lock and snapshot publication machinery (`SNAPSHOT_CACHE_DIR`, str-35vtk.4). Replace the branch target with the pinned SHA in both the refresh and the snapshot clone. Optionally add a `task examples-bump` helper that writes the current `origin/main` SHA into the lock.

## Out of scope

- Moving the examples back in-tree.
- Changing which examples tests use.
- General Task `sources:` coverage (task-sources-cover-real-inputs, shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, examples, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `cli-output-snapshots`.
- Related: str-35vtk.4, str-w5ry, str-91yri (closed); `task-sources-cover-real-inputs`.
