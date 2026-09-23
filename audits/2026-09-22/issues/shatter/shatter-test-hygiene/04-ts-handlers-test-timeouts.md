---
slug: ts-handlers-test-timeouts
kind: new
title: "shatter-ts handlers.test.ts: 10 tests exceed the 30 s jest timeout under machine load"
priority: P3
type: bug
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts handlers.test.ts: 10 tests exceed the 30 s jest timeout under machine load

## Problem

During the audit's `check-unit` run, with the machine at load average >100 on 32 cores, `shatter-ts/src/handlers.test.ts` took 679 s and 10 distinct tests hit the 30 s jest timeout. An isolated rerun at load ~30 passed 90/90 in 28 s. `analyzer.test.ts` took 427 s in the same run but passed. The shared machine regularly runs 3-6x oversubscribed, so wall-clock timeouts tuned on an idle machine make the TS unit gate unreliable.

## Evidence

- `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`.
- `audits/2026-09-22/gates/check-unit.log:82` shows `FAIL src/handlers.test.ts (679.399 s)`. The log has 18 `Exceeded timeout` lines. The failing-test list at :83-235 (repeated in the summary at :281-433) names 10 distinct tests:
  - `analyze`: 2 tests (stack frames `handlers.test.ts:368`, `:406`, in `describe("analyze")` at :323)
  - `instrument`: 1 test (`:439`, in `describe("instrument")` at :422)
  - `invocation adapter hooks`: 2 tests (describe at :891)
  - `async function execution`: 2 tests (describe at :1069)
  - `missing browser global classification (str-jeen.30)`: 3 tests (describe at :1921)

  The draft said "analyze tests"; the failures span five describe blocks.
- `audits/2026-09-22/gates/ts-handlers-rerun.log` shows `exit=0 wall=28s ... load=29.35`, with jest reporting "Time: 26.11 s, estimated 680 s".
- `handlers.test.ts:171-185`: a top-level `beforeAll` already warms one worker thread (handshake plus an analyze call "to fully load the TypeScript compiler (~2-3s cold start)"). The earlier guess that "a full ts-morph Project is built per test" is not verified. The cost may instead be per-request worker or compile work, or plain CPU starvation.
- Audit finding gates-05 (verified "partially": the root cause is speculative, P3).

## Acceptance criteria

- [ ] Profile at least one of the timing-out tests under induced load (e.g. `stress-ng --cpu 64` or a parallel `cargo build` alongside) and record in this issue where the time goes: worker startup, ts-morph Project construction, type checking, or queueing.
- [ ] Based on that profile, restructure fixtures (e.g. share a Project or worker across a describe block) so the handlers suite completes with no timeouts at load ~100. Paste the measured suite time and load average into the issue.
- [ ] If a load-scaled timeout is still needed after that, it is implemented once (in the jest config, not per test) and documented in `docs/perf/gate-budgets.md` together with the known-load-flake note.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Confirm the cost first. Do not restructure fixtures until the profile shows per-test construction dominates. If the time is CPU starvation of the worker thread rather than redundant work, the right fix may be a jest `maxWorkers` cap for the TS gate under the shared machine budget rather than fixture changes.

## Out of scope

- The general gate concurrency and load budget on the shared machine (docs/perf).
- Other TS test files, unless the profile shows the same cause.

## Priority / type / labels

P3 · bug · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
