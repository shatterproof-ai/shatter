---
slug: ts-shadowed-callback-params
kind: new
title: "TS analyzer resolves shadowed callback parameters to the outer function parameter"
priority: P2
type: bug
labels: [typescript, analyze, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS analyzer resolves shadowed callback parameters to the outer function parameter

## Problem

The SymExpr builders turn an identifier into a function parameter by **name only**. There is no scope check. Inside `shadow(xs, x)`, the callback in `xs.filter((x) => { if (x > 5) ... })` has its own `x`, but its condition is recorded as a condition on the **outer** parameter `x`. So the analyzer reports a branch the outer `x` does not control, and the solver may be handed a wrong constraint.

## Evidence

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/analyzer.ts:2280` (non-exported `buildSymExpr`); `:2289` `if (paramNames.has(expr.text))`.
- `shatter-ts/src/instrumentor.ts:1862` (`buildSymExpr`); `:1872` `if (paramNames.has(expr.text))`. `buildSymExprWithFlow` (`:874`) resolves through a `resolveName` callback that is likewise name-based.
- Probe: `shadow(xs: number[], x: number)` containing `xs.filter((x) => { if (x > 5) ... })`. Analyze records the condition `{param x} > 5`. The audit verifier reproduced this.
- The instrument side was **not** verified. The instrumentor probably does not branch-instrument callback bodies, so there may be nothing to observe there today. It has the same name-only lookup, so it becomes live as soon as callbacks are instrumented.
- Audit sources: finding frontend-ts-04; `audits/2026-09-22/areas/frontend-ts.md` F4.

## Acceptance criteria

- [ ] The analyzer resolves identifiers through the TypeChecker: `checker.getSymbolAtLocation(id)` is compared with the symbol of the parameter declaration. A same-named inner binding is not a parameter; it becomes `unknown`, or a local, according to the builder's rules.
- [ ] The instrumentor gets the same guarantee, through a scope stack of bound names or the same symbol check. This includes `buildSymExprWithFlow`'s `resolveName`, so a flow-map entry for the outer `x` is not used for an inner `x`.
- [ ] Regression tests for the `shadow` repro in analyze, and in instrument if callbacks are instrumented. The analyze test fails on current `main` and passes after the fix. Add variants: arrow-parameter shadowing, a `const x` shadow in a nested block, and a destructured `({ x }) =>` shadow.
- [ ] Record `task affected` `Gates selected` at close. If instrument output changes, run `task --force e2e-ts` (the governed task, not bare `cargo test`) and paste its `test result:` line with a non-zero passed count.

## Suggested approach

For the analyzer, the TypeChecker is already available (`analyzer.ts:362` `const checker = program.getTypeChecker()` in `analyzeFile`). Carry the parameter symbols into the builder instead of a name set. The instrumentor runs as a transformer, possibly without a checker, so a lexical scope stack (push the bound names at each function, arrow or block) is the cheaper route there.

## Out of scope

- Consolidating the builders (ts-flow-analysis-consolidation, str-rf2v). If that shared builder lands first, do the fix there.
- Instrumenting callback bodies as branch sites.

## Priority / type / size

P2 · bug · size S
