# NOTE on str-rf2v: a fourth, already-diverged SSA flow walker lives in executor.ts

| field | value |
|---|---|
| action | **append comment to existing issue `str-rf2v`** (no new issue) |
| suggested priority for target | P2 |
| source findings | frontend-ts-09 |

<!-- body -->
**Audit 2026-09-22 note** (findings: frontend-ts-09)



### Current code facts
- `shatter-ts/src/executor.ts:1497-1810` `visitStatementsForLoopSnapshots` / `buildLoopSnapshotMutatedExpr` (:1728) / `mergeLoopSnapshotFlowMaps` (:1769) mirror instrumentor.ts:361-632; the merge functions are line-for-line equivalent.
- The executor copy lacks destructuring, while/do/for-in/for-of traversal and closure poisoning.
- Also: the analyzer builder (analyzer.ts:2280) ignores data flow while parity-matrix `ite-symexpr-production-partial` (:1096-1107) claims TS analyze produces ite.

### Proposed changes / acceptance additions
- Widen str-rf2v to one flow-analysis module with hooks (onBranch, onLoopIteration) used by instrumentor, executor and analyzer.
- Implement the program-point fix from the TS flow-map issue (draft 40) there once.
