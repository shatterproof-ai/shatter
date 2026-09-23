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

`instrumentFunction` builds **one** data-flow map for the whole function body and uses that end-of-function map for every branch. Two more defects make it worse. A reassignment whose right-hand side resolves to `unknown` does not kill the old binding. Loop bodies are walked once, with no widening or havoc. As a result the `constraint` recorded in `branch_path` can be **false for the inputs that actually ran**. Z3 then negates a constraint that has nothing to do with the branch and never flips it. Core triage (`triage::evaluate_constraint`) also uses these constraints to predict paths, so a wrong constraint can wrongly skip executions. That second effect was not traced end to end.

This is a soundness bug in the main concolic path for TypeScript, not a precision gap.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23). No TS production code changed between the 2026-09-04 and 2026-09-22 audits.

- `shatter-ts/src/instrumentor.ts:187`: `const dataFlowMap = buildDataFlowMap(targetFunction, sourceFile, paramNames);` is built once, before instrumenting.
- `shatter-ts/src/instrumentor.ts:1710`: `wrapBranchCondition` calls `buildSymExpr(condition, ctx.paramNames, ctx.dataFlowMap)` with that one map for every branch.
- `shatter-ts/src/instrumentor.ts:498` and `:510`: `if (nextExpr.kind !== "unknown") { flowMap.set(...) }`. An unresolvable reassignment leaves the stale binding in place.
- `shatter-ts/src/instrumentor.ts:422-431`: `for`/`while`/`do`/`for-in`/`for-of` bodies are visited once through `visitStatementsForDataFlow`, with no havoc of loop-assigned variables.
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
- `shatter-core/src/triage.rs:316` already provides `pub fn evaluate_constraint(...)`, so a core-side consistency check needs no new evaluator.
- No gate catches this. The builder property tests (`shatter-ts/src/property.test.ts:1187`) compare builders in isolation against a fixed `resolveName`. No test checks "constraint evaluated on concrete inputs == taken".
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/findings.json`), `audits/2026-09-22/areas/frontend-ts.md` F1. These paths exist on branch `audit-2026-09-22`.

## Acceptance criteria

- [ ] The constraint for each branch uses the flow map **as of that branch's program point**, not the end-of-function map.
- [ ] Reassigning a variable from an unresolvable expression sets it to `unknown`, so the old binding is gone.
- [ ] Every variable assigned inside a loop body is `unknown` at the loop head and after the loop, unless a sound widening is implemented.
- [ ] The `stale`, `staleUnknown` and `loopCarried` probes above are regression tests: `param a`, `unknown`, and `unknown`/non-constant respectively. Each test fails on current `main` and passes after the fix; the close note shows both runs.
- [ ] **Oracle test** (fast-check over generated inputs, or a fixture corpus with random inputs): execute each function, evaluate every recorded `branch_path[i].constraint` on the concrete inputs, and assert the result equals `taken`. Unknown constraints are skipped. The corpus includes reassignment-after-branch, opaque-call reassignment, loop-carried, and `if`/`else` merges (ite).
- [ ] **Core-side guard:** before a constraint reaches the solver, the core evaluates it with `triage::evaluate_constraint` against the concrete inputs. If the result contradicts `taken`, the constraint is downgraded to `unknown` and counted in telemetry or artifact stats (for example `inconsistent_constraints`). This protects every frontend. A Rust unit test feeds a deliberately inconsistent constraint and asserts the downgrade and the count.
- [ ] Close-time proof: the TS E2E suite was actually executed, not served from the Task cache. Run `cargo test -p shatter-core --test e2e_concolic -- --ignored` directly and paste the summary line. Also run `task affected` and record its `Gates selected` output.

## Suggested approach

1. Snapshot the flow map at each branch during the transformer walk instead of calling `buildDataFlowMap` once at `:187`. Kill bindings on unknown reassignment, and havoc loop-assigned names at the loop head.
2. If str-rf2v's shared flow-analysis module (one walker with `onBranch` / `onLoopIteration` hooks) exists by the time this is picked up, implement the fix there once. That also fixes the executor's loop-snapshot copy (`shatter-ts/src/executor.ts:1497-1810`). Do not block on str-rf2v: fixing the instrumentor walker directly is acceptable, as long as the executor copy gets the same kill/havoc semantics or a follow-up is filed.
3. Add the core-side guard in the orchestrator's constraint intake, reusing `triage::evaluate_constraint`.

## Out of scope

- Consolidating the analyzer, instrumentor and executor builders/walkers (str-rf2v).
- Making the analyzer use data flow (a note on str-rf2v covers this).
- New operator support (ts-operators-collapse-to-unknown).
- Loop widening more precise than havoc.

## Related

- str-4kop (closed; SSA phi/ite for conditional reassignment) and str-mu3h (closed; closures over mutable state become unknown) are neighbouring precision work, not this bug.
- str-rf2v (open; builder consolidation). See the rf2v-fourth-walker-and-analyze-dataflow note in this bucket.
- concolic-early-termination (other bucket): wrong constraints are one plausible contributor.

## Priority / type / size

P1 · bug · size M
