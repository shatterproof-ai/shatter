# Invariant inference over-generalises: no minimum support at function level, confidence hard-coded 1.0, only 0-anchored templates

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | spec,properties,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.61, str-qwua7.47 |
| source findings | core-11 |

<!-- body -->
## Problem

Function-level invariant detection runs on any number of records, so a single execution yields `x == <const>` and `x > 0`. Numeric templates compare only against 0, and confidence is always 1.0.

## Current code facts / evidence

- `shatter-core/src/spec.rs:395-400` runs `detect_classified_invariants` on all records with no minimum; per-class inference requires >=2 (:411).
- `shatter-core/src/invariants.rs:504-535` keeps any template true on all specimens; returns early only on empty input.
- `invariants.rs:648-665`: `confidence` hard-coded 1.0 (:661) — removal tracked in str-qwua7.61.
- `invariants.rs:253-340` NumericComparison candidates only use value 0.0; no range templates. invariants.rs has 0 `proptest!` blocks.

## Acceptance criteria

- Minimum support (e.g. >=5 specimens, configurable) before an invariant is reported at function or class level.
- Range templates (min/max bounds observed) or explicit reasoning for not adding them.
- Proptest: every inferred invariant holds on all its source traces; no invariant from fewer than N specimens.

## Suggested approach

Coordinate with str-qwua7.61 (confidence removal); this issue covers support threshold + templates.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: core-11 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.61, str-qwua7.47
