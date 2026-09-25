# TS analyzer resolves shadowed callback parameters to the outer function parameter

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | typescript,analyze,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-ts-04 |

<!-- body -->
## Problem

Identifiers are resolved to params by name only, so `xs.filter((x) => x > 5)` inside `shadow(xs, x)` records a condition on the outer `x`.

## Current code facts / evidence

- `shatter-ts/src/analyzer.ts:2288-2293` and `shatter-ts/src/instrumentor.ts:1871-1874`: `paramNames.has(expr.text)`.
- Repro: `shadow(xs, x)` with `xs.filter((x)=>{if(x>5)...})` → analyze condition `{param x} > 5`. Instrument side not verified.

## Acceptance criteria

- Analyzer resolves identifiers via the TypeChecker symbol and compares with the parameter declaration symbol.
- Instrumentor keeps a scope stack of bound names (or uses the same symbol check).
- Test for the shadow repro in both analyze and instrument.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-ts-04 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
