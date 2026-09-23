# TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | feature |
| priority | P3 |
| labels | typescript,instrumentation,solver,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.37, str-rf2v |
| source findings | frontend-ts-14 |

<!-- body -->
## Problem

Branches over common expressions produce `unknown` constraints, blocking solver-guided exploration.

## Current code facts / evidence

- `shatter-ts/src/instrumentor.ts:1984-2027`, `:1862-1953`.
- `const v = a ?? 7; if (v > 3)` → `{unknown} > 3`; `2 ** a! > 8` → unknown > 8; `xs[i] === 7` → unknown === 7.
- Core BinOpKind has Shl/Shr (str-a4c); TS lacks them (str-qwua7.37).

## Acceptance criteria

- as/!/satisfies/type assertions are unwrapped.
- ?? becomes ite over eq(x, null); ** and shifts map to core ops; element access with a literal key becomes a path segment.
- Tests per construct in both builders.

## Suggested approach

Do inside the shared builder (str-rf2v).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: frontend-ts-14 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.37, str-rf2v
