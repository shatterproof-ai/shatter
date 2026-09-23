# shatter-ts handlers.test.ts analyze tests exceed the 30 s jest timeout under machine load

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | typescript,tests,flake,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | gates-05 |

<!-- body -->
## Problem

Under load (>100 on 32 cores) `src/handlers.test.ts` took 679 s with ~10 analyze tests hitting the 30 s jest timeout; an isolated rerun at load ~30 passed 90/90 in 28 s. analyzer.test.ts took 427 s but passed. Gate reliability on the shared machine suffers.

## Current code facts / evidence

- `shatter-ts/src/handlers.test.ts:368,406` (analyze describe block).
- `audits/2026-09-22/gates/check-unit.log:82-280`: `FAIL src/handlers.test.ts (679.399 s)`, 18 `Exceeded timeout` lines.
- Suspected (unverified): a full ts-morph Project is built per test.

## Acceptance criteria

- Profile one analyze test; record where time goes in this issue.
- Shared fixtures (Project built once in `beforeAll`) or equivalent bring the handlers suite well under timeout at load ~100 (measure and record).
- If load-scaled timeouts remain necessary, document them in `docs/perf/gate-budgets.md`.

## Suggested approach

Confirm the per-test Project cost first; only then restructure fixtures.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: gates-05 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
