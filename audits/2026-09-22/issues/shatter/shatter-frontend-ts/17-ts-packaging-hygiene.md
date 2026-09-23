---
slug: ts-packaging-hygiene
kind: new
title: "shatter-ts packaging: dual npm/pnpm lockfiles, tests emitted to dist/, standalone bundle cannot find its worker"
priority: P3
type: chore
labels: [typescript, packaging, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts packaging: dual npm/pnpm lockfiles, tests emitted to dist/, standalone bundle cannot find its worker

Split from the earlier combined ts-lifecycle-and-packaging-hygiene draft (audit finding frontend-ts-16). Each item below is a separate commit; they share one issue because all three are build-output hygiene in `shatter-ts` with the same validation (build, bundle, walkthrough).

## Problem

- Both `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` exist. The Taskfile installs with npm, so the pnpm lockfile drifts silently.
- `tsc` emits every `*.test.ts` into `dist/`, so test code ships in the build output.
- The `bundle` script emits `dist/bundle.js` and `dist/worker-bundle.js`, but the instrumentation worker is resolved as `worker.js` next to the bundle. A standalone bundle (without the `tsc` output beside it) fails on the first `instrument`.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/package-lock.json` and `shatter-ts/pnpm-lock.yaml` both present; `pnpm-lock.yaml` was last touched incidentally in aca09d8b.
- `shatter-ts/tsconfig.json:16-17`: `"include": ["src"]`, `"exclude": ["node_modules", "dist", "src/__fixtures__"]`. All 21 `src/*.test.ts` compile into `dist/`.
- `shatter-ts/package.json:11` `bundle`: `--outfile=dist/bundle.js` and `--outfile=dist/worker-bundle.js`.
- `shatter-ts/src/instrumentation-worker.ts:49`: `workerPath ?? path.join(__dirname, "worker.js")`.
- *Verifier correction:* `node dist/bundle.js` works in a normal `dist/`, because `tsc` also emits `dist/worker.js`, which the bundle falls back to. Only a standalone bundle fails, with `Cannot find module .../worker.js`. The CLI's embedded path already renames the file (`shatter-cli/build.rs:93` reads `worker-bundle.js`; `shatter-cli/src/embedded_frontend.rs:42` extracts it as `worker.js`), so installed CLIs are not affected.
- Audit sources: finding frontend-ts-16; `audits/2026-09-22/areas/frontend-ts.md` F16.

## Acceptance criteria

- [ ] `pnpm-lock.yaml` is deleted; npm is the one package manager. `shatter-ts/CLAUDE.md` (or README) says so, and `npm ci` in a clean checkout succeeds.
- [ ] A `tsconfig.build.json` used by the `build` script excludes `**/*.test.ts`. After `task ts:build`, `find shatter-ts/dist -name '*.test.js'` prints nothing (paste it), and ts-jest still type-checks the tests (`npx jest` passes).
- [ ] The bundle and worker names agree: the bundle emits `worker.js`, or the worker path is passed explicitly. A test or scripted check copies **only** the bundle output into an empty temp directory, starts it, and completes an `instrument` request; it fails on current `main`. If the file name changes, `shatter-cli/build.rs` and `embedded_frontend.rs` are updated in the same change.
- [ ] Because the embedded frontend is touched, run `task walkthrough` and paste its pass line. Record `task affected` `Gates selected`.

## Out of scope

- Lifecycle fixes (ts-lifecycle-and-packaging-hygiene).
- The js-yaml upgrade (ts-js-yaml-v4).
- Changing how the CLI embeds the frontend beyond the worker file name.
- Pointing `package.json` `main`/`bin` at the bundle (a publishing decision; file separately if wanted).

## Priority / type / size

P3 · chore · size S
