# shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, js-yaml v3

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | typescript,cleanup,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-ts-06, frontend-ts-16, frontend-ts-17 |

<!-- body -->
## Problem

Several small robustness/packaging defects in shatter-ts.

## Current code facts / evidence

- `shatter-ts/src/executor.ts:1274-1281` Promise.race with an uncleared setTimeout per async execution; `shatter-ts/jest.config.js:6` `forceExit: true`.
- `shatter-ts/src/main.ts:74-79` `process.exit(0)` on readline close without awaiting in-flight handlers; shutdown during instrument → internal_error 'Worker terminated'; `main.ts:23` hard-codes 'protocol 0.1.0' though PROTOCOL_VERSION is imported.
- `shatter-ts/pnpm-lock.yaml` last touched incidentally (aca09d8b) while the Taskfile uses npm.
- `shatter-ts/tsconfig.json:17` includes src with no test exclude → 21 *.test.js in dist.
- Bundle emits worker-bundle.js; `instrumentation-worker.ts:50` resolves __dirname/worker.js (works only because tsc also emits worker.js).
- js-yaml ^3.14 with a hand-written d.ts.

## Acceptance criteria

- clearTimeout in finally; forceExit removed after a `--detectOpenHandles` run fixes real leaks.
- Pending requests are awaited (time-bounded) on stdin close/shutdown; banner uses PROTOCOL_VERSION.
- pnpm-lock.yaml deleted; tsconfig.build.json excludes tests; bundle and worker names agree; js-yaml 4 + @types.

## Suggested approach

Independent small commits.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-ts-06, frontend-ts-16, frontend-ts-17 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
