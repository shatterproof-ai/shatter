# TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | typescript,instrumentation,concolic,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-4kop, str-mu3h, str-rf2v |
| source findings | frontend-ts-01 |

<!-- body -->
## Problem

The instrumentor builds one end-of-function data-flow map and uses it for every branch, keeps stale bindings when a variable is reassigned from an unresolvable expression, and visits loop bodies once. The emitted constraints are then false for the executed path, which misleads the solver.

## Current code facts / evidence

- `shatter-ts/src/instrumentor.ts:187` single `buildDataFlowMap` at finalize; `:1710` uses `ctx.dataFlowMap` for every branch; `:498` skips the set when the new value is unknown (old binding kept); `:422` loop handling.
- Repro: `stale(a,b){let x=a; if(x>0){...} x=b}` → emits `__shatter_branch(0,1,!!(x>0),{gt, param b, 0})`; real execute of stale with [5,0] reports taken=true under `b > 0`.
- `staleUnknown` (`x=opaque()`) records `a > 10`; `loopCarried` records `(0+1) < 3` and `a+1 > 10` (actual a+3).
- The core already has `triage::evaluate_constraint`.

## Acceptance criteria

- Constraints use the flow map snapshot at the branch's program point.
- Reassignment from an unresolvable expression marks the variable unknown; variables assigned in a loop are unknown at the loop head.
- Oracle test (fast-check or corpus): every recorded branch_path constraint evaluated on the concrete inputs equals `taken`.
- Core-side check downgrades constraints inconsistent with `taken` to unknown and counts them in telemetry/artifact stats.

## Suggested approach

Implement in the shared flow walker (see draft 45 / str-rf2v) so it is fixed once.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-ts-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-4kop, str-mu3h, str-rf2v
