# Spec preconditions are sample statistics (often false or vacuous); use the symbolic path constraints already recorded

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | feature |
| priority | P2 |
| labels | spec,spec-diff,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-0oc, str-dcz, str-qwua7.38 |
| source findings | artifacts-08, goals-13 |

<!-- body -->
## Problem

Class preconditions are derived from observed samples (`AllEqual param[0]==-1`, `param[0] > 0`, `SameType`) rather than from the path condition. They are sometimes false (fmt2's `n>10` class shows `param[0] > 0`, but n=5 throws), and spec-diff reports a swapped mapping as a precondition change plus inconclusive rather than a changed output. str-0oc promised constraint-derived preconditions.

## Current code facts / evidence

- `shatter-core/src/spec.rs:450-524` precondition derivation; `shatter-core/src/equivalence.rs`.
- Each raw_results branch_path carries constraints, e.g. `{op:gt,left:{param n},right:{const 10}}`.
- `audits/2026-09-22/goals-runs/regress/base.json`: class 'returns "negative"' preconditions `[{"AllEqual":{"param_index":0,"value":-1}}]`, sample_count 20.
- After swapping even/odd, `spec-diff` prints `[PRECOND] Class 3 ... - param[0] == 2 + param[0] == 1` and `[INCONCLUSIVE]` instead of 'input 2 now returns positive-odd'.

## Acceptance criteria

- Each class's precondition is the conjunction of its branch constraints rendered as source-like text (e.g. `n < 0`); provenance marks it symbolic.
- Sample statistics move to a separate 'observed inputs' field; preconditions shared by every class are suppressed.
- spec-diff re-executes or cross-references old examples against new classes so the even/odd swap is reported as a changed postcondition for a concrete input.
- Known-answer tests for classifyNumber and fmt2 preconditions.

## Suggested approach

Spec schema bump likely; coordinate with draft 38 (spec shapes) and str-qwua7.38.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: artifacts-08, goals-13 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-0oc, str-dcz, str-qwua7.38
