---
slug: ts-flow-analysis-consolidation
kind: new
title: "TS: consolidate the four SymExpr builders / flow walkers (analyzer, instrumentor x2, executor loop snapshots) into one flow-analysis module"
priority: P2
type: task
labels: [typescript, parity, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS: consolidate the four SymExpr builders / flow walkers (analyzer, instrumentor x2, executor loop snapshots) into one flow-analysis module

## Problem

The TS frontend has four hand-maintained implementations of symbolic-expression construction and data-flow walking, and they already produce different output for the same source:

- `shatter-ts/src/analyzer.ts:2280` `buildSymExpr` (parameters only, no data flow);
- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` and `:874` `buildSymExprWithFlow`;
- the instrumentor flow walker `instrumentor.ts:361-632` (`visitStatementsForDataFlow`, `mergeFlowMaps`);
- the executor loop-snapshot walker `shatter-ts/src/executor.ts:1497-1810` (`visitStatementsForLoopSnapshots`, `buildLoopSnapshotMutatedExpr`, `mergeLoopSnapshotFlowMaps`), which lacks destructuring, `while`/`do`/`for-in`/`for-of` traversal and closure poisoning.

str-rf2v is the **investigation** into consolidating them; its description puts the refactor out of scope and asks for option (a), "file the implementation issue with the design sketch". This is that implementation issue, split out of the audit's rf2v note after the Codex cross-check pointed out that widening str-rf2v by comment would contradict its own scope.

## Evidence

Verified at 793f2b0b (2026-09-23).

- The four locations above, and `bd show str-rf2v` (description: "Out of scope: the actual consolidation refactor"; existing 2026-09-04 notes recommend "Collapse to one builder `buildSymExpr(node, resolver)`" and reject option (b)).
- Divergence examples: for `const y = a * 2; if (y > 10)`, analyze emits `{gt, unknown, 10}` and instrument emits `{gt, a*2, 10}`. A grep of `executor.ts:1497-1810` for `registerDestructuredBindings`, `isWhileStatement` or closure poisoning finds 0 hits, against 3 in the instrumentor range.
- `protocol/parity-matrix.yaml:1096-1110` (`ite-symexpr-production-partial`) says TS produces ite through `visitStatementsForDataFlow` and lists `affected_frontends: [rust]` for `analyze`. TS analyze does not produce ite.
- Audit sources: findings frontend-ts-03 and frontend-ts-09; `audits/2026-09-22/areas/frontend-ts.md` F3/F9.

## Acceptance criteria

- [ ] **Parity-matrix correction (may land first, as its own commit):** `ite-symexpr-production-partial` lists `typescript` in `affected_frontends` for `analyze` and says TS produces ite only on instrument/execute, until the analyzer uses the shared module. `task parity` passes. Once the analyzer does produce ite, the entry is updated again in the same change.
- [ ] One flow-analysis module (for example `shatter-ts/src/flow-analysis.ts`) containing one walker parameterized by hooks (for example `onBranch`, `onLoopIteration`) and one SymExpr builder that takes a name resolver. The analyzer, the instrumentor (both paths) and the executor loop snapshots all use it. The old copies are deleted, including the duplicate operator tables (`binaryTokenToOp`/`unaryTokenToOp` vs `mapBinaryOp`/`mapUnaryOp`).
- [ ] The kill/havoc/program-point semantics from ts-flow-map-program-point live in the shared module (if that issue landed first, its regression tests still pass unchanged).
- [ ] **Output-equality test:** on a shared fixture corpus of flow-tracked locals (derived locals, if/else reassignment producing ite, loops, destructuring, closures), analyze and instrument produce **identical** SymExpr for the same condition. The test is red on current `main` for at least the `const y = a * 2` case.
- [ ] The builder output-equality test from ts-protocol-and-parity-tests-meaningful runs against the single builder (or is reduced to what still makes sense) and passes.
- [ ] `shatter-ts/CLAUDE.md` builder-parity section and the root CLAUDE.md "parallel parity" bullet name the new module instead of the deleted builders.
- [ ] Close-time proof: `task --force e2e-ts` (the governed task) with its `test result:` line and a non-zero passed count; `task conformance` output; `task affected` `Gates selected`. If any analyze output changes, regenerate TS conformance goldens and list the changed cases.

## Suggested approach

Follow the existing str-rf2v notes: `buildSymExpr(node, resolver)`, where the resolver is param-only for a plain lookup and flow-map-backed otherwise, and `resolvePropertyChain` consults the resolver for the base. Sequence after str-qwua7.37 (SymExpr construction spec) if it has landed, so the unknown-collapse rule is decided once.

## Out of scope

- New operator support (ts-operators-collapse-to-unknown); land it in the shared builder afterwards.
- Go/Rust builders.

## Related

- str-rf2v (open; the investigation this implements; the rf2v-fourth-walker-and-analyze-dataflow note links the two).
- ts-flow-map-program-point (this bucket; independent soundness fix, not blocked on this).
- ts-protocol-and-parity-tests-meaningful, ts-operators-collapse-to-unknown (this bucket).
- str-qwua7.37 (open; SymExpr construction spec).

## Priority / type / size

P2 · task · size L
