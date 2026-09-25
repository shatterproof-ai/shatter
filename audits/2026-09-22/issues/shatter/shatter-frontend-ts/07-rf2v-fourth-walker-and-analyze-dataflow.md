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

**Target:** `str-rf2v` ("Investigate consolidating TS buildSymExpr implementations (analyzer/instrumentor triplication)", P2, OPEN; live state checked with `bd show str-rf2v` on 2026-09-23).
**Action:** post ONE comment (`bd comments add str-rf2v`). Keep the priority at P2, the title and the description unchanged. The comment adds evidence to the **investigation** and points at the implementation issue drafted alongside it. It does **not** widen str-rf2v into the refactor: str-rf2v's description says "Out of scope: the actual consolidation refactor", and that stays true.
**Ordering:** post after `ts-flow-analysis-consolidation` and `ts-flow-map-program-point` are filed, and substitute their ids for the `<...>` placeholders.

**Existing notes on str-rf2v (from the 2026-09-04 audit)** already reject option (b) and recommend collapsing to one builder with a resolver callback. This comment is consistent with them and adds the walker and analyzer-data-flow facts they did not cover.

## Comment text

> **Audit 2026-09-22 note** (findings frontend-ts-03 and frontend-ts-09; evidence in `audits/2026-09-22/areas/frontend-ts.md` F3/F9, re-checked at 793f2b0b).
>
> Three facts for this investigation. They strengthen the existing note's rejection of option (b) ("document the triplication as load-bearing and add a node-type parity test").
>
> **1. There is a fourth copy: an SSA flow walker in `executor.ts`, and it has already diverged.**
> `shatter-ts/src/executor.ts:1497-1810` contains `visitStatementsForLoopSnapshots` (:1497), `buildLoopSnapshotMutatedExpr` (:1728) and `mergeLoopSnapshotFlowMaps` (:1769). It is a near-copy of the instrumentor walker at `instrumentor.ts:361-632` (`visitStatementsForDataFlow` :361, `mergeFlowMaps` :569). `mergeFlowMaps` and `mergeLoopSnapshotFlowMaps` are line-for-line equivalent. The executor copy has no destructuring (`registerDestructuredBindings`), no `while`/`do`/`for-in`/`for-of` traversal, and no closure poisoning: a grep of 1497-1810 for those constructs finds 0 hits, against 3 in the instrumentor range. Neither this issue ("triplication") nor the parity table in `shatter-ts/CLAUDE.md` mentions it. The root CLAUDE.md "grep for the parallel path" rule names only `buildSymExpr`/`buildSymExprWithFlow`, so copies under other names escape it.
>
> **2. The analyzer's builder ignores data flow.**
> `shatter-ts/src/analyzer.ts:2280` (the non-exported `buildSymExpr`, "local copy for analyzer independence") knows only `paramNames`. For `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` while instrument emits `{gt, a*2, 10}`. For `let x = a; if (x > 0)`, analyze gives `{gt, unknown, 0}`. Go's analyzer threads a flow map, so TS and Go `analyze` differ on the same source, and that divergence is not recorded anywhere.
>
> **3. The parity matrix is wrong about TS.**
> `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says "TypeScript produces ite expressions via data flow analysis in visitStatementsForDataFlow" and lists `affected_frontends: [rust]` for `affected_commands: [analyze]`. TS produces ite only on the instrument/execute path.
>
> **Resolution proposal:** close this investigation through option (a), "file the implementation issue with the design sketch". That implementation issue is drafted as <ts-flow-analysis-consolidation id>: one flow-analysis module (walker plus builder) used by the analyzer, instrumentor and executor, analyze/instrument SymExpr equality on a shared corpus, and the parity-matrix correction above. To close this issue, append the design sketch (resolver-callback builder per the existing note, plus the walker hooks) to that issue and cite it here. The program-point soundness fix is tracked separately in <ts-flow-map-program-point id> and does not wait for the consolidation.
