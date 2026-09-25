---
slug: ts-js-yaml-v4
kind: new
title: "shatter-ts: upgrade js-yaml 3.x to 4.x and drop the hand-written js-yaml.d.ts"
priority: P3
type: chore
labels: [typescript, dependencies, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts: upgrade js-yaml 3.x to 4.x and drop the hand-written js-yaml.d.ts

Split from the earlier combined ts-lifecycle-and-packaging-hygiene draft (audit finding frontend-ts-16), because a dependency major upgrade has its own risk and validation.

## Problem

`shatter-ts` depends on `js-yaml` `^3.14.2` and carries a hand-written type declaration for it. js-yaml 4 changed the API: `safeLoad`/`safeDump` were removed and `load` became safe by default. Staying on 3.x keeps a hand-maintained `.d.ts` that can drift from the library and leaves the frontend on an old major line. It also means the one call site uses 3.x `load`, which by default accepts the `!!js/function` / `!!js/regexp` / `!!js/undefined` types (it builds JS functions from YAML text); the file it parses (`.shatter/config.yaml`) comes from whatever repository Shatter is pointed at.

## Evidence

Verified at 793f2b0b (2026-09-23).

- `shatter-ts/package.json` `dependencies`: `"js-yaml": "^3.14.2"`.
- `shatter-ts/src/js-yaml.d.ts`: hand-written declaration of `load` only ("no `@types/js-yaml` is installed").
- The only call site: `shatter-ts/src/opaque-stub-registry.ts:28` `import { load as loadYaml } from "js-yaml"`, used to read `<projectRoot>/.shatter/config.yaml` (`:264`).
- Audit sources: finding frontend-ts-16; `audits/2026-09-22/areas/frontend-ts.md` F16.

## Acceptance criteria

- [ ] `js-yaml` is `^4` with `@types/js-yaml` as a devDependency; `shatter-ts/src/js-yaml.d.ts` is deleted.
- [ ] The call site in `opaque-stub-registry.ts` (and any other found by `grep -rn js-yaml shatter-ts/src`, listed in the close note) is migrated. 4.x `load` is not given a schema that re-enables the `js` types.
- [ ] A test loads a representative `.shatter/config.yaml` with `ts_runtime_values` entries and asserts the parsed registry is unchanged from 3.x. A second test asserts that a config containing `!!js/function` is rejected with a clear error instead of being turned into a JS function object; it fails on current `main`.
- [ ] `npx jest` in `shatter-ts` passes; `task walkthrough` pass line pasted (the embedded bundle changes); `task affected` `Gates selected` recorded.

## Out of scope

- Other dependency upgrades.
- Packaging (ts-packaging-hygiene) and lifecycle (ts-lifecycle-and-packaging-hygiene).

## Priority / type / size

P3 · chore · size S
