---
slug: ts-handlers-timeout-fix
kind: new
title: "TS handlers.test load timeouts: fix"
priority: P3
type: bug
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ts-handlers-test-timeouts]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS handlers.test load timeouts: fix

## Problem

`shatter-ts/src/handlers.test.ts` hits the 30 s jest timeout (10 tests) when the shared machine is heavily loaded, failing the TS unit gate. The diagnosis issue `ts-handlers-test-timeouts` produces a reproducible load recipe, a profile and a recommended fix. This issue implements that fix. Its acceptance is outcome-based: it does not prescribe a particular mechanism.

## Evidence

See `ts-handlers-test-timeouts` for the log excerpts (679 s suite, 10 `Exceeded timeout of 30000 ms` failures at load ~114 on 32 cores; 26 s clean rerun at load ~30). `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`; `shatter-ts/Taskfile.yml:53`/`:72` already run jest with `--runInBand`. Audit finding gates-05.

## Acceptance criteria

- [ ] The fix is the one recommended by the diagnosis, or the issue explains why a different one was chosen.
- [ ] Under the diagnosis issue's load recipe, the handlers suite passes with no timeouts in 3 of 3 consecutive runs. Paste the suite time and load average of each run, plus the same recipe's failing output on the pre-fix commit (red then green).
- [ ] Without load, the suite passes and its time is not worse than before the fix by more than 20% (paste before/after times).
- [ ] Per-test timeouts are not raised piecemeal. If a timeout change is part of the fix, it is set once (jest config or the TS Task entry) and documented in `docs/perf/gate-budgets.md` with the measured basis.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Other TS test files, unless the diagnosis showed the same cause.
- Machine-wide gate concurrency policy.

## Priority / type / labels

P3 · bug · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `ts-handlers-test-timeouts` (diagnosis).
