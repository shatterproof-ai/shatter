# Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache misses go.sum

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | ci,github-actions,maintenance,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | tests-ci-18 |

<!-- body -->
## Problem

CI annotations warn of deprecations ahead of a runner migration.

## Current code facts / evidence

- Run 35756993223 annotations: 'Node.js 20 is deprecated ... actions/cache@v4, actions/checkout@v4, actions/setup-go@v5, actions/setup-node@v4, arduino/setup-task@v2'; 'The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026'; 'Restore cache failed: Dependencies file is not found ... go.sum'.
- Files: `.github/workflows/ci.yml`, `drift-patrol.yml`, `perf-ci.yml` (all ubuntu-latest).

## Acceptance criteria

- Action majors bumped to Node-24-based versions.
- Runners pinned to ubuntu-24.04 until z3/libclang packages are verified on 26.
- setup-go `cache-dependency-path: shatter-go/go.sum`.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: tests-ci-18 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
