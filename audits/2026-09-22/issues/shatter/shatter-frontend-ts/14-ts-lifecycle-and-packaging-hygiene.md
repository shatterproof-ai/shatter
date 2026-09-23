---
slug: ts-lifecycle-and-packaging-hygiene
kind: new
title: "shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, standalone bundle worker name, js-yaml v3"
priority: P3
type: chore
labels: [typescript, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, standalone bundle worker name, js-yaml v3

## Problem

These are small robustness and packaging defects in `shatter-ts`, each independently fixable. None is user-visible today, but some of them hide real problems from the test suite: `forceExit` masks leaked handles.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

1. **Timer leak (frontend-ts-06; verifier P3).**
   - `shatter-ts/src/executor.ts:1274-1281`: `Promise.race([syncResult, new Promise((_, reject) => setTimeout(() => reject(new Error("async execution timed out")), timeoutMs))])`. The timer is never cleared or `unref`'d, so every async execute leaves a pending timer of up to `timeoutMs`.
   - `shatter-ts/jest.config.js:6`: `forceExit: true` ("avoid hanging on worker threads that outlive tests") hides leaked handles like this one.
   - Practical impact is minor, because the frontend is long-lived and each timer is bounded.
2. **Stdin EOF / shutdown drop in-flight responses (frontend-ts-17).**
   - `shatter-ts/src/main.ts:76-79`: `rl.on("close", () => { ...; process.exit(0); })` exits without awaiting in-flight `handleRequest` promises.
   - Audit probes (not re-run by the verifier): piping `handshake, shutdown` then EOF produced no `shutdown_ack`; `shutdown` during an in-flight `instrument` produced `internal_error "Unhandled error: Worker terminated"`.
   - `main.ts:23` hard-codes `"Starting TypeScript frontend (protocol 0.1.0)"`, although `PROTOCOL_VERSION` is imported at `main.ts:13`.
   - Low impact today, because the core waits for each response.
3. **Packaging (frontend-ts-16).**
   - Both `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` exist. The Taskfile installs with npm; `pnpm-lock.yaml` was last touched incidentally in aca09d8b.
   - `shatter-ts/tsconfig.json:16-17` includes `src` and excludes only `src/__fixtures__`, so `tsc` emits all 21 `*.test.ts` into `dist/`.
   - `shatter-ts/package.json:11` `bundle` emits `dist/bundle.js` and `dist/worker-bundle.js`, but `shatter-ts/src/instrumentation-worker.ts:49` resolves `path.join(__dirname, "worker.js")`.
     - *Verifier correction:* `node dist/bundle.js` works in a normal `dist/`, because `tsc` also emits `dist/worker.js`, which the bundle falls back to. Only a standalone bundle (without the tsc output) fails with `Cannot find module .../worker.js` on the first instrument. The CLI's embedded path renames the file correctly (`shatter-cli/build.rs:93`, `embedded_frontend.rs:42`).
   - `package.json` `main`/`bin` point at the unbundled `dist/main.js`.
   - `js-yaml` is `^3.14.2`, with a hand-written `shatter-ts/src/js-yaml.d.ts`.
- Audit sources: findings frontend-ts-06, frontend-ts-16, frontend-ts-17; `audits/2026-09-22/areas/frontend-ts.md` F6/F16/F17.

## Acceptance criteria

- [ ] The async race clears its timer in `finally`, or `unref`s it. `forceExit` is removed from `jest.config.js` after one `npx jest --detectOpenHandles` run whose findings are fixed. The close note pastes that run's summary showing no open handles.
- [ ] On stdin `close` and on `shutdown`, the frontend awaits pending request promises, bounded by a timeout, before exiting. A test pipes `handshake, shutdown`, then EOF, and asserts that `shutdown_ack` is received; it fails on current `main`. The startup banner uses `PROTOCOL_VERSION`.
- [ ] `pnpm-lock.yaml` is deleted (npm is the one package manager).
- [ ] A `tsconfig.build.json` (used by `build`) excludes `**/*.test.ts`. After `task ts:build` there are no `*.test.js` files in `dist/`, while ts-jest typechecking of tests still works.
- [ ] The bundle and worker names agree: the bundle emits `worker.js`, or the worker path is passed explicitly. A standalone bundle (bundle output alone, in an empty directory) completes an `instrument` request. `shatter-cli/build.rs` / `embedded_frontend.rs` are updated if the file name changes.
- [ ] `js-yaml` is 4.x with `@types/js-yaml`, and the hand-written `js-yaml.d.ts` is removed.
- [ ] Record `task affected` `Gates selected`. Because the embedded frontend is touched, also run the walkthrough (`task walkthrough`) and paste its pass line.

## Suggested approach

Make independent small commits, one per item. The timer fix touches the same race as ts-timeout-classification, which adds a dedicated timeout error class. If both are picked up together, do them in one change.

## Out of scope

- Outcome classification (ts-timeout-classification).
- ESLint adoption (str-qwua7.31).
- Changing how the CLI embeds the frontend beyond the worker file name.

## Priority / type / size

P3 · chore · size M
