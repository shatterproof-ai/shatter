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

Re-verified against the audit worktree at 56c86168 (2026-09-23).

- `shatter-ts/src/instrumentor.ts:1862` `buildSymExpr` (handled node kinds through about `:1953`) and `:1984-2027` operator mapping (`binaryTokenToOp`), which has no cases for `??`, `**` or shifts. `buildSymExprWithFlow` (`:874`) and the analyzer copy (`analyzer.ts:2280`) have the same gaps.
- The instrumentor has no `isAsExpression`, `isNonNullExpression`, `isSatisfiesExpression` or `isElementAccessExpression` handling (grep finds 0 hits).
- Probes:
  - `const v = a ?? 7; if (v > 3)` -> `{unknown} > 3`;
  - `if (2 ** a! > 8)` -> `unknown > 8`;
  - `if (xs[i] === 7)` -> `unknown === 7`.

  The verifier reproduced the `??` and element-access probes; it did not probe `**` or `!`.
- Core `BinOpKind` (`shatter-core/src/sym_expr.rs:95-114`) has `Shl`, `Shr` and `BitClear` (commented "Go-specific"), `In` and `InstanceOf`. It has **no** power operator and no unsigned right shift. The TS `BinOpKind` union (`shatter-ts/src/protocol.ts:514`) lacks `shl`/`shr` (see str-qwua7.37).
- Audit sources: finding frontend-ts-14; `audits/2026-09-22/areas/frontend-ts.md` F14.

## Acceptance criteria

- [ ] `as`, `!`, `satisfies` and `<T>x` type assertions (and parentheses) are unwrapped before building, in every builder.
- [ ] `a ?? b` becomes `ite(eq(a, null) or eq(a, undefined), b, a)`, or the closest faithful encoding the core's null model supports. Document the choice next to the builder.
- [ ] `<<` and `>>` map to the core `Shl`/`Shr`, with `shl`/`shr` added to the TS `BinOpKind`.
- [ ] `>>>` and `**` either get a core op wired through Z3 or stay `unknown`, with the choice recorded as a documented collapse rule. Do not map `**` to `Mul`.
- [ ] Element access with a literal key (`o["k"]`, `xs[0]`) becomes a `param.path` segment. A non-literal index stays `unknown`, as a documented rule.
- [ ] Each construct has a unit test in **both** `buildSymExpr` and `buildSymExprWithFlow`, and in the analyzer builder unless str-rf2v has deleted it. Each test fails on current `main`.
- [ ] One known-answer E2E case (`??` or `as`) shows the concolic engine flipping a branch it could not flip before. Run `cargo test -p shatter-core --test e2e_concolic -- --ignored <case>` directly and paste the output. Record `task affected` `Gates selected`.
- [ ] Any new op that reaches the wire is added to `protocol/schemas` and PROTOCOL.md (coordinate with protocol-schemas-reject-real-output), and `task parity` passes.

## Suggested approach

Do this inside the shared builder if str-rf2v's consolidation has landed, so all builders gain the operators at once. Otherwise add each construct to all three builders in one change, and extend the output-equality parity corpus from ts-protocol-and-parity-tests-meaningful. Start with the unwrapping, which is cheap and high-yield.

## Out of scope

- Template literal and string-concatenation solving (Z3 string theory). Keep these `unknown` unless it is trivial.
- BigInt semantics.
- Go/Rust operator coverage.

## Related

- str-qwua7.37 (open; which node kinds are mandatory, TS lacks shl/shr), str-rf2v (open), str-a4c (core shifts), protocol-schemas-reject-real-output (shatter-protocol-parity bucket; schemas lack shl/shr).

## Priority / type / size

P3 · feature · size S-M
