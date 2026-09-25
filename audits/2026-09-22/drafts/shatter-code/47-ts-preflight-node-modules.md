# TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | typescript,install,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-jeen.26, str-jeen.40 |
| source findings | frontend-ts-13 |

<!-- body -->
## Problem

A tsconfig-only project or a .ts file with no imports gets `preflight_failed: missing_node_modules`; the failure is a single module-level value, so it also applies to other roots in the same frontend process.

## Current code facts / evidence

- `shatter-ts/src/handlers.ts:190-218` requires node_modules when project_root is non-empty.
- `shatter-core/src/project.rs:10-15` MARKERS include tsconfig.json, go.mod, Cargo.toml.
- Stickiness is intentional per the code comment ('one failure authoritative'); end-to-end CLI impact not verified.

## Acceptance criteria

- node_modules required only when package.json declares dependencies/devDependencies (or fail lazily on MODULE_NOT_FOUND).
- Preflight failure keyed per root (or the design choice is re-confirmed and documented here).
- Test: zero-dependency TS project explores successfully.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-ts-13 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-jeen.26, str-jeen.40
