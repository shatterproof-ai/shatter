---
slug: ts-handlers-test-timeouts
kind: new
title: "TS handlers.test timeouts: diagnose"
priority: P3
type: task
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS handlers.test timeouts: diagnose

## Problem

During the audit's `check-unit` run, with the machine heavily loaded, `shatter-ts/src/handlers.test.ts` took 679 s and 10 distinct tests hit the 30 s jest timeout. An isolated rerun at lower load passed 90/90 in 26 s. `analyzer.test.ts` took 427 s in the same run but passed. The shared machine regularly runs oversubscribed, so timeouts tuned on an idle machine make the TS unit gate unreliable.

The cause is not known. This issue is diagnosis only: build a reproducible recipe, find where the time goes, and recommend a fix. The fix itself is `ts-handlers-timeout-fix`, which this issue blocks.

## Evidence

The audit gate logs (`audits/2026-09-22/gates/*.log`) are gitignored (`.gitignore:6 *.log`) and are not retrievable from any commit, so the relevant excerpts are reproduced here.

- Invocation (from the log header): `env PROPTEST_CASES=256 SHATTER_FUZZ_CASES=1000 SHATTER_FAST_CHECK_NUM_RUNS=default bash scripts/gate-wrapper.sh check task check-unit`, started 2026-09-22T12:52:01-05:00, load average at start `23.69 88.69 113.87` (1/5/15 min) on 32 cores. The TS suite is run by `shatter-ts/Taskfile.yml:53`/`:72` as `SHATTER_EXAMPLES_DIR="$examples_root" npm test -- --runInBand`, so jest already uses a single worker.
- Result lines from `check-unit.log`:
  ```
  FAIL src/handlers.test.ts (679.399 s)
  Test Suites: 1 failed, 20 passed, 21 total
  Tests:       10 failed, 968 passed, 978 total
  ```
  Each failure reports `Exceeded timeout of 30000 ms for a test.` The 10 failing tests:
  - `handleRequest › analyze › returns function_not_found error for missing func…`
  - `handleRequest › analyze › returns all functions when no function name speci…`
  - `handleRequest › instrument › returns instrumentation_failed for missing fun…`
  - `handleRequest › invocation adapter hooks › returns not_supported when adapt…`
  - `handleRequest › invocation adapter hooks › clears cachedAnalyses on shutdow…`
  - `handleRequest › async function execution › executes async function and retu…`
  - `handleRequest › async function execution › executes async function that rej…`
  - `handleRequest › missing browser global classification (str-jeen.30) › …` (3 tests)
- Rerun log (`cd shatter-ts && SHATTER_EXAMPLES_DIR=<examples snapshot 49984f4b> npx jest --runInBand src/handlers.test.ts`, started 2026-09-22T13:43:26-05:00 at load `31.20 35.55 56.58`, ended at load `29.35 34.66 55.72`): `Tests: 90 passed, 90 total`, `Time: 26.11 s, estimated 680 s`, wall 28 s.
- `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`.
- `handlers.test.ts:171-185`: a top-level `beforeAll` already warms one worker thread (handshake plus an analyze call "to fully load the TypeScript compiler (~2-3s cold start)"). Whether a ts-morph Project is rebuilt per test is not verified. Load average alone is not a reproducible condition: it does not say what the competing load was.
- Audit finding gates-05 (verified "partially": the root cause is speculative, P3).

## Acceptance criteria

- [ ] A reproducible load recipe is written into the issue: the exact competing workload (e.g. `stress-ng --cpu <N> --timeout <T>` with N stated relative to `nproc`, and/or a named concurrent build command), the machine's core count, and the exact test command. Running the recipe reproduces at least one `Exceeded timeout` in `handlers.test.ts` in at least 2 of 3 attempts; the attempt outputs are pasted. If no recipe reproduces the failure after a documented good-faith attempt (at least three recipes tried), the issue records that, and `ts-handlers-timeout-fix` is closed as not reproducible with a link.
- [ ] Under the recipe, at least one timing-out test is profiled (e.g. `--cpu-prof`, jest `--logHeapUsage`, or timestamps around worker startup, Project construction, type checking and request queueing), and the issue records where the time goes, with numbers.
- [ ] The issue records a recommended fix for `ts-handlers-timeout-fix` (e.g. shared fixture, reduced per-test work, a load-aware gate budget, or a documented timeout change) and why the profile supports it.

## Out of scope

- Implementing the fix (`ts-handlers-timeout-fix`).
- The general gate concurrency and load budget on the shared machine (docs/perf).

## Priority / type / labels

P3 · task · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `ts-handlers-timeout-fix`.
