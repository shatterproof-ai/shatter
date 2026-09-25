---
slug: core-constraint-consistency-guard
kind: new
title: "Core: check each recorded branch constraint against the concrete execution before solving, with language-aware evaluation that returns indeterminate when unsure"
priority: P2
type: feature
labels: [core, concolic, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core: check each recorded branch constraint against the concrete execution before solving, with language-aware evaluation that returns indeterminate when unsure

## Problem

A frontend can record a branch constraint that is false for the inputs that actually ran (ts-flow-map-program-point shows TS doing this today). The core passes such constraints straight to Z3, which then negates something unrelated to the branch, and to triage, which uses them to predict paths. Nothing in the core notices the contradiction, so a frontend soundness bug becomes silent lost coverage.

The core already has a concrete evaluator, `triage::evaluate_constraint` (`shatter-core/src/triage.rs:316`). It is **not** a safe oracle for every frontend:

- `BinOpKind::Div` / `Mod` evaluate in `f64` (`triage.rs:442-455`), so a Go/Rust integer condition such as `a / 2 == 1` at `a = 3` evaluates to false where the target computed true.
- `ConstValue::Null` and `ConstValue::Undefined` both become JSON `null` (`triage.rs:369`), and `eval_eq` treats `(null, null)` as equal (`triage.rs:473`), so JS `null === undefined` evaluates to true.

An unconditional "downgrade on mismatch" guard built on it would discard correct Go/Rust constraints. This issue was split out of ts-flow-map-program-point (audit draft frontend-ts-01) after the Codex cross-check flagged exactly this.

## Evidence

Verified at 793f2b0b (2026-09-23).

- `shatter-core/src/triage.rs:316` `pub fn evaluate_constraint(expr, params, param_names) -> Option<Value>`; returns `None` for `Unknown` and unsupported ops.
- `shatter-core/src/triage.rs:442-455`: `Div`/`Mod` through `as_f64` and `eval_arith` with `f64` closures.
- `shatter-core/src/triage.rs:369`: `ConstValue::Null | ConstValue::Undefined => Value::Null`.
- `evaluate_constraint` has no caller outside `triage.rs`; the orchestrator uses it only through `TriageState` (`orchestrator.rs:1653`).
- Audit sources: finding frontend-ts-01 (`audits/2026-09-22/areas/frontend-ts.md` F1); Codex cross-check finding 2 (`audits/2026-09-22/issues/crosscheck/shatter-frontend-ts.codex.md`).

## Acceptance criteria

- [ ] An evaluation mode that takes the frontend language (or the per-op semantics it implies) and returns **indeterminate** (`None`) for any sub-expression whose semantics it does not model exactly for that language. At minimum: integer `Div`/`Mod` for Go and Rust (truncating integer semantics, or indeterminate), JS `null` vs `undefined` (distinct, or indeterminate), and any op the evaluator does not implement.
- [ ] Before a recorded `branch_path` constraint reaches the solver, the core evaluates it on the concrete inputs. If the result is determinate and contradicts `taken`, the constraint is replaced by `unknown` for solving and triage, and a counter (for example `inconsistent_constraints`) is incremented in the run stats or artifact. Indeterminate results change nothing.
- [ ] Unit tests (proptest where it fits the invariant "determinate result implies agreement with a reference evaluation"):
  - a deliberately inconsistent TS constraint is downgraded and counted;
  - Go `a / 2 == 1` at `a = 3` with `taken: true` is **not** downgraded;
  - a JS `x === null` constraint at `x = undefined` is not downgraded as inconsistent;
  - an `Unknown` or unsupported-op constraint is left untouched.
- [ ] The counter is visible somewhere a user or gate can read it (run summary, stats JSON or artifact), and the E2E suites for all three frontends report `0` on their fixture corpora once ts-flow-map-program-point has landed. If a non-zero count shows up on Go or Rust fixtures, file each as a frontend bug and cite it in the close note.
- [ ] Close-time proof: `task --force e2e-ts`, `task --force e2e-go` and `task --force e2e-rust` (the governed tasks, not bare `cargo test`), pasting each `test result:` line with a non-zero passed count. Record `task affected` `Gates selected`.

## Suggested approach

Add a language parameter (or a small semantics struct) to the evaluator rather than a second evaluator. Hook the check where the orchestrator takes in execute responses, before constraints are stored for solving and before triage sees them. Keep the random explorer path in mind: if it stores constraints too, apply the same check there (root CLAUDE.md "parallel parity").

## Out of scope

- Fixing the TS flow map (ts-flow-map-program-point).
- Making the evaluator fully precise for every language; indeterminate is an acceptable answer.

## Related

- ts-flow-map-program-point (this bucket): the frontend bug this guards against.
- concolic-early-termination (shatter-concolic-and-engine-design bucket): the counter helps attribute early stops.

## Priority / type / size

P2 · feature · size M
