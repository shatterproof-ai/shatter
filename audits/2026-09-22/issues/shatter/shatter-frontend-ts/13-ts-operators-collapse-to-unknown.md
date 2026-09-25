---
slug: ts-operators-collapse-to-unknown
kind: new
title: "TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)"
priority: P3
type: feature
labels: [typescript, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)

## Problem

Branches over many everyday TS expressions produce `unknown` constraints, so the solver cannot steer toward them:

- `??`, `**`, `<<`/`>>`/`>>>`;
- element access (`xs[i]`), `ConditionalExpression` operands;
- template literals, BigInt literals, postfix unary;
- type-only wrappers `as` / `!` / `satisfies` / `<T>x`.

The type-only wrappers are the worst case: they change nothing at runtime, yet they hide a fully resolvable expression. The 2026-09-04 audit listed `??` and `**` (area A5, P3), but they were never filed.

## Evidence

Verified at 56c86168 and re-checked unchanged at 793f2b0b (2026-09-23).

- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` (handled node kinds through about `:1953`) and `:1984-2027` operator mapping (`binaryTokenToOp`), which has no cases for `??`, `**` or shifts. `buildSymExprWithFlow` (`:874`) and the analyzer copy (`analyzer.ts:2280`) have the same gaps.
- The instrumentor has no `isAsExpression`, `isNonNullExpression`, `isSatisfiesExpression` or `isElementAccessExpression` handling (grep finds 0 hits).
- Probes:
  - `const v = a ?? 7; if (v > 3)` -> `{unknown} > 3`;
  - `if (2 ** a! > 8)` -> `unknown > 8`;
  - `if (xs[i] === 7)` -> `unknown === 7`.

  The verifier reproduced the `??` and element-access probes; it did not probe `**` or `!`.
- Core `BinOpKind` (`shatter-core/src/sym_expr.rs:95-114`) has `Shl`, `Shr` and `BitClear` (commented "Go-specific"), `In` and `InstanceOf`. It has **no** power operator and no unsigned right shift. The TS `BinOpKind` union (`shatter-ts/src/protocol.ts:514`) lacks `shl`/`shr` (see str-qwua7.37).
- Core solver limits (`shatter-core/src/solver.rs`): `ConstValue::Null | ConstValue::Undefined => Int(0)` (`:522`), so null, undefined and numeric `0` are indistinguishable to Z3; `BinOpKind::Shl | Shr` (with the other bitwise ops) return `SolverError::Unsupported("bitwise operator ... not yet supported in Z3 solver")` (`:601-608`). The core triage evaluator likewise maps both null and undefined to JSON `null` (`triage.rs:369`).
- Audit sources: finding frontend-ts-14; `audits/2026-09-22/areas/frontend-ts.md` F14.

## Acceptance criteria

- [ ] `as`, `!`, `satisfies` and `<T>x` type assertions (and parentheses) are unwrapped before building, in every builder.
- [ ] `a ?? b` stays `unknown` as a **documented collapse rule**, because the core has no sound null model (null and undefined solve as integer `0`, so `ite(eq(a, null) or eq(a, undefined), b, a)` would pick `b` for `a = 0`). One sound exception is allowed: when the TypeChecker (or a syntactic literal) proves the left operand cannot be null or undefined, `a ?? b` reduces to `a`. A test pins `x ?? 7` with `x: number | undefined` to `unknown` and, if the exception is implemented, `n ?? 7` with `n: number` to `param n`. Real nullish support needs a core null model and is a separate issue.
- [ ] `<<` and `>>` map to the core `Shl`/`Shr` **on the wire**, with `shl`/`shr` added to the TS `BinOpKind`. This is emission only: the Z3 solver rejects `Shl`/`Shr` today (`solver.rs:601-608`), so these constraints will not be solvable. The close note must not claim solver support, and a test asserts that a shift constraint reaches the core as `shl`/`shr` and is reported as unsupported (not a crash or a wrong model). Solver support is a separate core issue.
- [ ] `>>>` and `**` either get a core op wired through Z3 or stay `unknown`, with the choice recorded as a documented collapse rule. Do not map `**` to `Mul`.
- [ ] Element access with a literal key (`o["k"]`, `xs[0]`) becomes a `param.path` segment. A non-literal index stays `unknown`, as a documented rule.
- [ ] Each construct has a unit test in **both** `buildSymExpr` and `buildSymExprWithFlow`, and in the analyzer builder unless ts-flow-analysis-consolidation has deleted it. Each test fails on current `main`.
- [ ] One known-answer E2E case using `as` or `!` unwrapping (not `??` or shifts, which the solver cannot use) shows the concolic engine flipping a branch it could not flip before. Close-time proof: `task --force e2e-ts` (the governed task, not bare `cargo test`); paste that test's line and the `test result:` line. Record `task affected` `Gates selected`.
- [ ] Any new op that reaches the wire is added to `protocol/schemas` and PROTOCOL.md (coordinate with protocol-schemas-reject-real-output), and `task parity` passes.

## Suggested approach

Do this inside the shared builder if ts-flow-analysis-consolidation has landed, so all builders gain the operators at once. Otherwise add each construct to all three builders in one change, and extend the output-equality parity corpus from ts-protocol-and-parity-tests-meaningful. Start with the unwrapping, which is cheap and high-yield.

## Out of scope

- Template literal and string-concatenation solving (Z3 string theory). Keep these `unknown` unless it is trivial.
- BigInt semantics.
- Go/Rust operator coverage.

## Related

- str-qwua7.37 (open; SymExpr construction spec, TS lacks shl/shr), str-rf2v (open), ts-flow-analysis-consolidation (this bucket), str-a4c (closed; added Shl/Shr to BinOpKind for Go, no solver support), protocol-schemas-reject-real-output (shatter-protocol-parity bucket; schemas lack shl/shr).

## Priority / type / size

P3 · feature · size S-M
