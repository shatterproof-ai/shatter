---
slug: rf2v-fourth-walker-and-analyze-dataflow
kind: note-to-existing
title: "Note on str-rf2v: a fourth, already-diverged SSA flow walker in executor.ts; the TS analyzer ignores data flow; the parity matrix wrongly says TS analyze produces ite"
priority: P2
type: note
labels: [typescript, parity, audit]
parent_epic: ""
blocked_by: []
existing_id: str-rf2v
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-rf2v

**Target:** `str-rf2v` ("Investigate consolidating TS buildSymExpr implementations (analyzer/instrumentor triplication)", P2, OPEN).
**Action:** post ONE comment (`bd comments add str-rf2v`). Keep the priority at P2. Do not change the title; the comment asks the implementer to widen the scope.
**Ordering:** post after `ts-flow-map-program-point`, `ts-protocol-and-parity-tests-meaningful` and `ts-operators-collapse-to-unknown` are filed, and substitute their ids for the `<...>` placeholders.

## Comment text

> **Audit 2026-09-22 note** (findings frontend-ts-03 and frontend-ts-09; evidence in `audits/2026-09-22/areas/frontend-ts.md` F3/F9, re-checked at 56c86168).
>
> Three additions to the consolidation scope. With them, option (b) in this issue ("document the triplication as load-bearing and add a node-type parity test") is not sufficient.
>
> **1. There is a fourth copy: an SSA flow walker in `executor.ts`, and it has already diverged.**
> `shatter-ts/src/executor.ts:1497-1810` contains `visitStatementsForLoopSnapshots` (:1497), `buildLoopSnapshotMutatedExpr` (:1728) and `mergeLoopSnapshotFlowMaps` (:1769). It is a near-copy of the instrumentor walker at `instrumentor.ts:361-632` (`visitStatementsForDataFlow` :361, `mergeFlowMaps` :569). `mergeFlowMaps` and `mergeLoopSnapshotFlowMaps` are line-for-line equivalent. The executor copy has no destructuring (`registerDestructuredBindings`), no `while`/`do`/`for-in`/`for-of` traversal, and no closure poisoning: a grep of 1497-1810 for those constructs finds 0 hits, against 3 in the instrumentor range. Neither this issue ("triplication") nor the parity table in `shatter-ts/CLAUDE.md` mentions it. The root CLAUDE.md "grep for the parallel path" rule names only `buildSymExpr`/`buildSymExprWithFlow`, so copies under other names escape it.
>
> **2. The analyzer's builder ignores data flow.**
> `shatter-ts/src/analyzer.ts:2280` (the non-exported `buildSymExpr`, "local copy for analyzer independence") knows only `paramNames`. For `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` while instrument emits `{gt, a*2, 10}`. For `let x = a; if (x > 0)`, analyze gives `{gt, unknown, 0}`. Go's analyzer threads a flow map, so TS and Go `analyze` differ on the same source, and that divergence is not recorded anywhere.
>
> **3. The parity matrix is wrong about TS.**
> `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says "TypeScript produces ite expressions via data flow analysis in visitStatementsForDataFlow" and lists `affected_frontends: [rust]` for `affected_commands: [analyze]`. TS produces ite only on the instrument/execute path. Until the analyzer builder uses the flow map, `typescript` belongs in `affected_frontends` for `analyze`, and the description should say so.
>
> **Proposed additions to acceptance:**
> - Widen the deliverable to **one flow-analysis module** (for example `shatter-ts/src/flow-analysis.ts`): a single walker parameterized by hooks (`onBranch`, `onLoopIteration`) and used by the instrumentor, the executor loop snapshots and the analyzer. It is paired with one SymExpr builder whose flow resolution is optional.
> - The program-point fix from <ts-flow-map-program-point id> (snapshot per branch, kill on unknown reassignment, havoc loop-assigned variables) lives in that module once. That fixes the executor copy as well.
> - On a shared fixture corpus of flow-tracked locals (derived locals, if/else reassignment producing ite, loops), analyze and instrument produce **identical** SymExpr for the same condition. A test asserts this.
> - `protocol/parity-matrix.yaml` `ite-symexpr-production-partial` is accurate in the same change: either TS analyze now produces ite, or `typescript` is added to `affected_frontends` for `analyze` with corrected text. Run `task parity`.
> - The builder output-equality test in <ts-protocol-and-parity-tests-meaningful id> covers the analyzer builder, which is exported for tests or deleted by this consolidation.
> - Related: <ts-operators-collapse-to-unknown id> should land in the shared builder, so that operator support cannot diverge again.
