# NOTE TO APPEND to str-rf2v: TS analyzer ignores data flow, and the parity matrix wrongly says TS analyze produces ite

- Priority: P2
- Type: (existing issue)
- Labels: (existing)
- Tracker action: note to append to str-rf2v (`bd comments add`). Also fix the parity-matrix entry in the same change.
- Related: str-qwua7.24
- Source findings: audit 2026-09-22 frontend-ts-03 (confirmed)

<!-- body -->
Audit 2026-09-22 adds this to the builder-consolidation scope:

- **The analyzer builder ignores the flow map.** For `const y = a*2; if (y > 10)`, analyze emits `{gt, unknown, 10}` while instrument emits `{gt, a*2, 10}`. For `let x=a; if (x>0)`, analyze gives `{gt, unknown, 0}`. Go's analyzer threads a flow map, so TS and Go analyze output differ on the same source. Code: `shatter-ts/src/analyzer.ts:2280` (the non-exported `buildSymExpr`).
- **The parity matrix is wrong.** `protocol/parity-matrix.yaml:1096-1107` (`ite-symexpr-production-partial`) lists `affected_frontends: [rust]` for analyze only, and says TS produces ite via `visitStatementsForDataFlow`. Add `typescript` (analyze) to the entry, or fix the builder first.
- **There is a fourth walker copy to fold in:** `shatter-ts/src/executor.ts:1497-1810` (`visitStatementsForLoopSnapshots`, `buildLoopSnapshotMutatedExpr`, `mergeLoopSnapshotFlowMaps`) duplicates the instrumentor walker and has already diverged (audit finding frontend-ts-09).

Extra acceptance: analyze and instrument produce identical SymExpr for flow-tracked locals on a shared fixture corpus, and the matrix entry is accurate.
