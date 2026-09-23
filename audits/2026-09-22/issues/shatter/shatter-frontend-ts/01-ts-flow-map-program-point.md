---
slug: ts-flow-map-program-point
kind: new
title: "TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution"
priority: P1
type: bug
labels: [typescript, instrumentation, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution

## Problem

`instrumentFunction` builds **one** data-flow map for the whole function body and uses that end-of-function map for every branch. Three more defects make it worse:

- A reassignment whose right-hand side resolves to `unknown` does not kill the old binding.
- Loop bodies are walked once, with no widening or havoc. This includes `for` incrementors and mutations inside loop conditions.
- Parameter lookup runs **before** the flow map (`resolveName` at `instrumentor.ts:346-351`, `buildSymExpr` at `:1871-1879`). A reassigned parameter therefore always resolves to the original parameter, and updating the flow map alone cannot fix it.

As a result the `constraint` recorded in `branch_path` can be **false for the inputs that actually ran**. Z3 then negates a constraint that has nothing to do with the branch and never flips it. Core triage (`triage::evaluate_constraint`) also uses these constraints to predict paths, so a wrong constraint can wrongly skip executions. That second effect was not traced end to end.

This is a soundness bug in the main concolic path for TypeScript, not a precision gap.

## Evidence

Line numbers were verified against the audit worktree at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23). No TS production code changed between the 2026-09-04 and 2026-09-22 audits.

- `shatter-ts/src/instrumentor.ts:187`: `const dataFlowMap = buildDataFlowMap(targetFunction, sourceFile, paramNames);` is built once, before instrumenting.
- `shatter-ts/src/instrumentor.ts:1710`: `wrapBranchCondition` calls `buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap)` with that one map for every branch.
- `shatter-ts/src/instrumentor.ts:498` and `:510`: `if (nextExpr.kind !== "unknown") { flowMap.set(...) }`. An unresolvable reassignment leaves the stale binding in place.
- `shatter-ts/src/instrumentor.ts:422-431`: the `for` body and incrementor, and the `while`/`do`/`for-in`/`for-of` bodies, are each visited once through `visitStatementsForDataFlow` / `visitExpressionForDataFlow`, with no havoc of loop-assigned variables. Loop **conditions** are not visited for mutations at all.
- `shatter-ts/src/instrumentor.ts:346-351` (`resolveName`) and `:1871-1873` (`buildSymExpr`) check `paramNames.has(name)` before consulting the flow map.
- Probes (calling `instrumentFunction` directly; 4th argument of `__shatter_branch`):

  ```ts
  export function stale(a: number, b: number) { let x = a; if (x > 0) {...} x = b; if (x > 100) {...} }
  //  branch 0 (x > 0)  -> { bin_op gt, left: param b, right: 0 }   // should be param a
  export function staleUnknown(a: number) { let x = a; x = opaque(); if (x > 10) {...} }
  //  branch 0 (x > 10) -> { bin_op gt, left: param a, right: 10 }  // should be unknown
  export function loopCarried(a: number) { let acc = a; let i = 0; while (i < 3) { acc = acc + 1; i++; } if (acc > 10) ... }
  //  while (i < 3)     -> { lt, left: (0 + 1), right: 3 }           // constant, wrong
  //  acc > 10          -> { gt, left: a + 1, right: 10 }            // actually a + 3
  ```

- Through the real frontend (`node dist/main.js`), `execute` of `stale` with `inputs: [5, 0]`:

  ```json
  "branch_path":[{"branch_id":0,"line":3,"taken":true,
    "constraint":{"kind":"expr","expr":{"kind":"bin_op","op":"gt",
      "left":{"kind":"param","name":"b","path":[]},"right":{"kind":"const","type":"int","value":0}}}}]
  ```

  The branch was taken while its recorded constraint `b > 0` is false for `b = 0`. The audit verifier reproduced `stale` independently; it did not re-run the `staleUnknown`/`loopCarried` probes.
- No gate catches this. The builder property tests (`shatter-ts/src/property.test.ts:1187`) compare builders in isolation against a fixed `resolveName`. No test checks "constraint evaluated on concrete inputs == taken".
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/findings.json`), `audits/2026-09-22/areas/frontend-ts.md` F1. These paths exist on branch `audit-2026-09-22`.

## Acceptance criteria

- [ ] The constraint for each branch uses the flow map **as of that branch's program point**, not the end-of-function map.
- [ ] Reassigning a variable from an unresolvable expression sets it to `unknown`, so the old binding is gone.
- [ ] Havoc covers **every** mutation site inside a loop: the body, the `for` incrementor, and assignments or updates inside the loop condition (for example `while (x-- > 0)`). Every such variable is `unknown` at the loop head, inside the body, and after the loop, unless a sound widening is implemented.
- [ ] A reassigned parameter resolves through the program-point map, not straight to `param <name>`. Parameter lookup no longer short-circuits the flow map for names that are assigned anywhere in the function.
- [ ] Regression tests, each red on current `main` and green after the fix (the close note pastes both runs, for example `npx jest -t flow-map-program-point` before and after):

  | Function | Branch | Expected constraint left side |
  |---|---|---|
  | `stale` (above) | `x > 0` | `param a` |
  | `staleUnknown` (above) | `x > 10` | `unknown` |
  | `loopCarried` (above) | `while (i < 3)` and `acc > 10` | `unknown` (non-constant) |
  | `forIncr(a, n) { for (let i = a; i < n; i++) { if (i > 5) ... } }` | `i > 5` | `unknown` |
  | `condMut(a) { let x = a; while (x-- > 0) {} if (x > 3) ... }` | `x > 3` | `unknown` |
  | `paramReassign(a, b) { a = b; if (a > 0) ... }` | `a > 0` | `param b` |
  | `paramOpaque(a) { a = opaque(); if (a > 0) ... }` | `a > 0` | `unknown` |

- [ ] **Oracle test in the TS suite** (fast-check over generated inputs, or a fixture corpus with random inputs): execute each function, evaluate every recorded `branch_path[i].constraint` with JS semantics on the concrete inputs, and assert the result equals `taken`. Unknown constraints are skipped. The corpus contains every function in the table above plus `if`/`else` merges (ite). The test demonstrably fails on current `main` (paste the failing counterexample).
- [ ] The executor loop-snapshot walker (`shatter-ts/src/executor.ts:1497-1810`) gets the same kill/havoc semantics in this change, or a follow-up issue is filed and its id is in the close note.
- [ ] Close-time proof: `task --force e2e-ts` (the governed task; do not run the underlying `cargo test` bare, per AGENTS.md "Shared-Machine Resource Etiquette"). Paste the `test result:` summary line and confirm the passed count is non-zero. Record `task affected` `Gates selected`.

## Suggested approach

1. Snapshot the flow map at each branch during the transformer walk instead of calling `buildDataFlowMap` once at `:187`. Kill bindings on unknown reassignment. Before walking a loop, collect every name assigned in its condition, body and incrementor, and havoc those names at the loop head.
2. Collect the set of assigned parameter names up front, and route those names through the flow map (seeded with `param <name>`) instead of the `paramNames` short-circuit.
3. If ts-flow-analysis-consolidation (the shared flow-analysis module) exists when this is picked up, implement the fix there once. Do not block on it.

## Out of scope

- The core-side consistency guard: core-constraint-consistency-guard (this bucket).
- Consolidating the analyzer, instrumentor and executor builders/walkers (ts-flow-analysis-consolidation, str-rf2v).
- New operator support (ts-operators-collapse-to-unknown).
- Loop widening more precise than havoc.

## Related

- str-4kop (closed; SSA phi/ite for conditional reassignment) and str-mu3h (closed; closures over mutable state become unknown) are neighbouring precision work, not this bug.
- str-rf2v (open; builder consolidation investigation) and ts-flow-analysis-consolidation (this bucket).
- core-constraint-consistency-guard (this bucket): the core-side backstop, split out of this draft.
- concolic-early-termination (shatter-concolic-and-engine-design bucket): wrong constraints are one plausible contributor.

## Priority / type / size

P1 · bug · size M
