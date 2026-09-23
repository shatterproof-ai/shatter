---
slug: ts-union-discriminant-literals
kind: new
title: "TS analyzer widens discriminated-union tag fields to plain str, so computeArea never gets kind = circle/rectangle/triangle (0/6 branches)"
priority: P2
type: bug
labels: [typescript, frontend-ts, analyzer, examples, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS analyzer widens discriminated-union tag fields to plain `str`, so `computeArea` never gets `kind` = circle/rectangle/triangle (0/6 branches)

(Split from known-answer-ratchet-and-ts-discriminants during the cross-check revision.)

## Problem

`computeArea(shape: Shape)` in the examples repo's `standalone/ts/05-unions.ts` switches on `shape.kind`, a discriminated union of `"circle"`, `"rectangle"` and `"triangle"` variants. The TS analyzer reports the `kind` field of every variant as plain `str`, dropping the literal type. The generator then produces `kind` values such as `""`, `"0"`, `" "` and random strings, never a real tag, and emitted `{"kind":"true","radius":2.0}`. No case matches, so exploration stays at 0/6 branches.

The analyzer already emits `enum_values` for literal-union parameters and string enums (`shatter-ts/src/analyzer.test.ts:792-810`), so the missing piece is the object-field case inside union variants.

## Evidence

- `/home/ketan/project/examples/standalone/ts/05-unions.ts:9-15` (EXPECTED BRANCHES for `computeArea`: six outcomes across three `kind` values) and `:17` onward (`switch (shape.kind)`).
- The audit's analysis output shows `{"kind":"str"}` in all three union variants; its concolic run reached 21 iterations, 1 path, 0/6 branches (`audits/2026-09-22/areas/goals.md` item 7, on branch `audit-2026-09-22` until the audit reports land).
- `shatter-ts/src/protocol.ts:467`: `enum_values?: (string | number | boolean)[]` exists on the type-info shape; `shatter-ts/src/analyzer.test.ts:792` ("emits enum_values for a literal-union alias parameter") and `:803` (string enum parameter) cover parameters only.
- `demo/gauntlet-scan-allowlist.yaml:32-36` allowlists `computeArea` with the reason "coverage limited by union-input synthesis" and no issue id.

## Acceptance criteria

- [ ] For an object type inside a union whose field has a string, number or boolean literal type, the TS analyzer emits that field with its literal value (as `enum_values` with one value, or a const literal type; the close comment says which). A unit test in `shatter-ts/src/analyzer.test.ts` covers `Shape` from `05-unions.ts` and fails on current code.
- [ ] A known-answer test explores `computeArea` with a fixed `--max-iterations` budget and asserts that all three `kind` values appear in recorded inputs and that at least the three non-throwing outcomes are reached (both engines, random and `--concolic`). It fails on current code; record both runs in the close comment.
- [ ] Parity: if the analysis JSON changes shape or gains a field for this case, `protocol/parity-matrix.yaml` and `shatter-ts/CLAUDE.md` are updated, and `task parity` + `task conformance` pass (output recorded). If the Go or Rust analyzers already handle tagged variants, the close comment says so; if not, a follow-up issue is filed and linked.
- [ ] The `computeArea` entry is removed from `demo/gauntlet-scan-allowlist.yaml` if it now passes, or its reason is updated with this issue's id. `task gauntlet` passes (output recorded).
- [ ] `cargo test --test e2e_concolic` (TS E2E) passes with forced execution.

## Out of scope

- `routeRequest` (also allowlisted for union-input synthesis) beyond checking whether this fix helps it; the close comment reports its before/after branch count.
- The known-answer ratchet gate (known-answer-ratchet-and-ts-discriminants).

## Related

known-answer-ratchet-and-ts-discriminants, concolic-early-termination (shatter-concolic-and-engine-design bucket; attributes part of the `computeArea` loss to this). Source finding: goals-07 (confirmed).
