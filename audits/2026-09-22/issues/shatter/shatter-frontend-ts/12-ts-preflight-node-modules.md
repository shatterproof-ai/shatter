---
slug: ts-preflight-node-modules
kind: new
title: "TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request"
priority: P3
type: bug
labels: [typescript, install, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request

## Problem

The TS frontend's preflight fails whenever `<project_root>/node_modules` is missing, whether or not the project has any dependencies. The core derives `project_root` from the nearest `package.json`, `tsconfig.json`, `go.mod` or `Cargo.toml`. So these all get `preflight_failed: missing_node_modules`, even for a file with no imports:

- a tsconfig-only TS project;
- a `package.json` with no dependencies;
- possibly a `.ts` file inside a Go or Rust repo.

The failure is stored in a single module-level variable. Once set, it fails every later request in the same frontend process, including requests for other roots. The code comment says this stickiness is intentional ("one failure authoritative"). The verifier therefore treats it as a design choice to revisit rather than a bug, and downgraded the finding to P3.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/handlers.ts:190` `let preflightFailure: PreflightFailure | null = null;` (a module-level value).
- `shatter-ts/src/handlers.ts:203-218` `runPreflight(projectRoot)` returns early only when `projectRoot` is null or empty, then fails on a missing `node_modules` (`PREFLIGHT_REASON_MISSING_NODE_MODULES`, `:178`).
- `preflightFailure` is checked by the analyze, instrument, prepare, execute and setup handlers (`:438`, `:503`, `:559`, `:639`, `:880`).
- `shatter-core/src/project.rs:10-15` `MARKERS`: `package.json`, `tsconfig.json` (TypeScript), `go.mod`, `Cargo.toml`.
- Audit probe: analyze with `project_root` set to a directory without `node_modules` returned `preflight_failed: missing_node_modules: .../node_modules` for a file with no imports.
- Not verified end to end through the `shatter` CLI. It is also unverified whether the core ever passes a Go/Cargo root to the TS frontend. Confidence is medium.
- Introduced by str-jeen.26 (closed; node_modules preflight) and str-jeen.40 (closed; `preflight_failed` code). Neither handled dependency-free projects or per-root scoping.
- Audit sources: finding frontend-ts-13 (verifier: partially confirmed, P2 -> P3); `audits/2026-09-22/areas/frontend-ts.md` F13.

## Acceptance criteria

- [ ] `node_modules` is required only when `package.json` declares `dependencies` or `devDependencies`, or the frontend fails lazily on the first `MODULE_NOT_FOUND` with the same `preflight_failed` remediation text.
- [ ] Either the preflight failure is keyed per project root, so a failure for root A does not fail requests for root B, or the "one failure authoritative" design is re-confirmed and the reason recorded in `shatter-ts/CLAUDE.md` next to the preflight section, with a test pinning it.
- [ ] Tests:
  - a zero-dependency TS project (tsconfig-only, and a `package.json` with no deps) runs analyze + explore successfully through the CLI. Paste the `shatter explore` output in the close note;
  - a project with declared deps and no `node_modules` still gets `preflight_failed`.

  The first test fails on current `main`.
- [ ] If the error surface changes, update `shatter-ts/CLAUDE.md` and the parity contract, and run `task parity` + `task conformance`. Record `task affected` `Gates selected`.

## Suggested approach

Read `package.json` once in `runPreflight`. Replace the module-level value with a `Map<root, PreflightFailure>`, unless the maintainer keeps the sticky design.

## Out of scope

- Installing dependencies automatically.
- Go/Rust preflight.

## Priority / type / size

P3 · bug · size S
