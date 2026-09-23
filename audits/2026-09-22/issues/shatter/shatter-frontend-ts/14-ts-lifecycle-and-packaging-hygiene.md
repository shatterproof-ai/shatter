---
slug: ts-lifecycle-and-packaging-hygiene
kind: new
title: "shatter-ts lifecycle: async timeout timer never cleared (hidden by jest forceExit); shutdown and stdin EOF drop in-flight responses"
priority: P3
type: bug
labels: [typescript, lifecycle, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts lifecycle: async timeout timer never cleared (hidden by jest forceExit); shutdown and stdin EOF drop in-flight responses

Scope note: the slug is kept from the earlier combined draft. The packaging items (dual lockfiles, tests emitted to `dist/`, standalone bundle worker name) moved to ts-packaging-hygiene, and the js-yaml major upgrade moved to ts-js-yaml-v4. This issue is only process-lifecycle correctness.

## Problem

1. **Timer leak.** Every async `execute` starts a `setTimeout` for the harness timeout and never clears it, so each call leaves a pending timer of up to `timeoutMs`. Jest's `forceExit: true` hides leaked handles like this one from the test suite.
2. **Shutdown and EOF drop in-flight work.** Requests are handled concurrently (each stdin line starts its own `handleRequest` promise). The `shutdown` handler terminates the instrumentation worker immediately, so an `instrument` still running in that worker fails with `Worker terminated`. On stdin `close`, the process calls `process.exit(0)` without waiting for pending promises, so responses in flight are lost, including the `shutdown_ack`.

Impact today is low, because the core waits for each response before sending the next request. It becomes real for any pipelined client, and the leaked handles weaken the test suite.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/executor.ts:1272-1282`: `Promise.race([syncResult, new Promise((_, reject) => setTimeout(() => reject(new Error("async execution timed out")), timeoutMs))])`. The timer id is not kept, so it can never be cleared.
- `shatter-ts/jest.config.js:7`: `forceExit: true` ("avoid hanging on worker threads that outlive tests").
- `shatter-ts/src/handlers.ts:1046-1069`: the `shutdown` case clears caches and runs `await _worker.terminate()` (`:1058-1061`) before returning `shutdown_ack`, without waiting for requests still using the worker.
- `shatter-ts/src/main.ts:76-79`: `rl.on("close", () => { ...; process.exit(0); })` exits without awaiting in-flight `handleRequest` promises.
- Audit probes (not re-run by the verifier): piping `handshake, shutdown` then EOF produced no `shutdown_ack`; `shutdown` during an in-flight `instrument` produced `internal_error "Unhandled error: Worker terminated"`.
- `shatter-ts/src/main.ts:23` hard-codes `"Starting TypeScript frontend (protocol 0.1.0)"`, although `PROTOCOL_VERSION` is imported at `main.ts:13`.
- Audit sources: findings frontend-ts-06 and frontend-ts-17; `audits/2026-09-22/areas/frontend-ts.md` F6/F17.

## Acceptance criteria

- [ ] The async race keeps its timer id and calls `clearTimeout` when the race settles either way (for example in `finally`). `unref()` alone does **not** satisfy this: the timer would still stay allocated until it fires. A unit test with jest fake timers asserts that no timer is pending after a fast async execute resolves, and after one that rejects.
- [ ] `forceExit` is removed from `jest.config.js`. The close note pastes the summary of an `npx jest --detectOpenHandles` run in `shatter-ts` showing no open handles; any other leaks it finds are fixed in this issue or filed with ids in the close note.
- [ ] The frontend tracks in-flight request promises. On `shutdown` it stops accepting new work, awaits in-flight requests (bounded by a timeout), and only then terminates the worker and sends `shutdown_ack`. On stdin `close` it awaits in-flight requests (same bound) before exiting.
- [ ] Regression tests that spawn `node dist/main.js`, each red on current `main`:
  - pipe `handshake`, `shutdown`, then EOF; assert `shutdown_ack` is received;
  - send `instrument` for a real fixture and then `shutdown` immediately without waiting; assert the `instrument` response is a success (not `internal_error ... Worker terminated`) and is written **before** `shutdown_ack`.
- [ ] The startup banner uses `PROTOCOL_VERSION`.
- [ ] Record `task affected` `Gates selected`. If the shutdown response ordering is protocol-visible, update `shatter-ts/CLAUDE.md` and run `task conformance`.

## Suggested approach

Keep a `Set<Promise>` of in-flight handlers in `main.ts`. Move the worker termination out of the `shutdown` handler into a drain step that `main.ts` runs after the in-flight set is empty. The timer fix touches the same race as ts-timeout-classification, which adds a dedicated timeout error class; if both are picked up together, do them in one change.

## Out of scope

- Outcome classification (ts-timeout-classification).
- Packaging (ts-packaging-hygiene) and the js-yaml upgrade (ts-js-yaml-v4).
- ESLint adoption (str-qwua7.31).

## Priority / type / size

P3 · bug · size S-M
