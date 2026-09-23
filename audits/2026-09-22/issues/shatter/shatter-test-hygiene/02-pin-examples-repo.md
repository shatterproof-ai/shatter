---
slug: pin-examples-repo
kind: new
title: "Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)"
priority: P2
type: task
labels: [testing, examples, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)

## Problem

Unit, E2E, smoke, TS and walkthrough tests read `SHATTER_EXAMPLES_DIR`, a checkout of `shatterproof-ai/examples` that `scripts/examples_checkout.py` resets to `origin/main` whenever it is more than 10 minutes old. No commit is pinned, and no Task fingerprint covers the examples content. As a result:

- the same shatter commit can pass or fail depending on when the gate ran;
- a change to the examples repo can break shatter's gates with no shatter commit to bisect;
- a cached Task result can be served for a gate whose inputs (the examples) have changed, because the only fingerprinted input is the script itself.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `scripts/examples_checkout.py:19` `DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"`, `:20` `DEFAULT_BRANCH = "main"`, `:21` `DEFAULT_DIR = <tempdir>/shatter-examples-main`, `:48` `REFRESH_WINDOW_SECONDS = 600`.
- `scripts/examples_checkout.py:134-137` (`_refresh_checkout_locked`): `fetch origin main`, `checkout main`, `reset --hard origin/main`, `clean -fdx`.
- Consumers of `SHATTER_EXAMPLES_DIR` include `Taskfile.yml` (tasks at :76, :131, :159, :606, :625), `shatter-core/Taskfile.yml` (test-ignored/-fast), `shatter-ts/Taskfile.yml`, `shatter-core/tests/e2e_concolic{,_go,_rust}.rs`, `e2e_llm_oracle.rs`, `bench_frontier_ranking.rs`, `tests/support/rust_frontend_harness.rs`, `shatter-ts/src/{executor,handlers}.test.ts`, `shatter-rust/src/executor.rs` and `scripts/perf_runner.py`.
- The Task `sources:` lists contain `scripts/examples_checkout.py` (e.g. `Taskfile.yml:128,156,598`), but nothing that changes when the examples content changes.
- There is no lock or pin file (`git ls-files | grep -i examples` shows no SHA file).
- Related closed issues: str-35vtk.4 (locking for the shared checkout), str-w5ry (refresh crash) and str-91yri (PR auth). None pins a SHA.
- Audit finding tests-ci-09 (verified).

## Acceptance criteria

- [ ] A tracked lock file (e.g. `examples.lock` holding a full 40-char SHA) pins the examples commit.
- [ ] `examples_checkout.py` checks out exactly that SHA: fetch the SHA if missing, then `checkout --detach <sha>`. It never resets to a moving branch. The 10-minute refresh window either goes away or only governs fetching.
- [ ] Every Task that exports `SHATTER_EXAMPLES_DIR` lists the lock file in its `sources:`. Proof at close: change the SHA in a scratch branch, run `task --dry test-standard` (or the equivalent status query) and show that the task is no longer up to date.
- [ ] A unit test in `scripts/test_walkthrough_examples_checkout.py` (or a sibling test) asserts that the checkout HEAD equals the lock SHA after a run.
- [ ] A short documented bump procedure (README or `docs/`): edit the lock, open a PR, and let the normal gates run. Bumping the pin is an ordinary gated change.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Keep the existing shared-checkout lock and per-consumer snapshot machinery (`SNAPSHOT_CACHE_DIR`, str-35vtk.4). Only replace the branch target with the pinned SHA, and key the snapshot cache on the SHA. Optionally add a `task examples-bump` helper that writes the current `origin/main` SHA into the lock.

## Out of scope

- Moving the examples back in-tree.
- Changing which examples tests use.
- General Task `sources:` coverage (task-sources-cover-real-inputs, shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, examples, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-35vtk.4, str-w5ry, str-91yri (closed); `task-sources-cover-real-inputs`.
