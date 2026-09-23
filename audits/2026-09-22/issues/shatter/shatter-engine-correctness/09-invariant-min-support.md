---
slug: invariant-min-support
kind: new
title: "Invariant inference over-generalizes: no minimum support at function level, and only 0-anchored numeric templates"
priority: P2
type: bug
labels: [spec, invariants, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Invariant inference over-generalizes: no minimum support at function level, and only 0-anchored numeric templates

## Problem

Function-level invariant detection runs on any number of execution records, including one. `detect_invariants` keeps every template that holds on all specimens, so a single execution yields invariants like `x == <constant>` and `x > 0`, which are then published in the spec. Numeric comparison templates compare only against 0, so real bounds (such as `0 <= x <= 100`) are never inferred, and the output is dominated by trivially true or overfitted facts. Confidence is hard-coded to 1.0, so the spec cannot signal weak support.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/spec.rs:395-400`: `detect_classified_invariants(&all_records, ...)` runs on all records with no minimum count. Per-class inference requires `class_records.len() >= 2` (`:411`).
- `shatter-core/src/invariants.rs:504` `detect_invariants`: keeps any template true on all specimens and returns early only on empty input.
- `shatter-core/src/invariants.rs:661`: `confidence: 1.0` is hard-coded. Removing or computing it is tracked in open str-qwua7.61.
- `shatter-core/src/invariants.rs:253-340`: `NumericComparison` candidates use only `value: 0.0` (`:270`, `:291`, `:312`, `:332`). There are no range or min/max templates.
- `invariants.rs` has 0 `proptest!` blocks. Proptest coverage is tracked in open str-qwua7.47.

## Acceptance criteria

- [ ] A minimum-support threshold (default at least 5 specimens, configurable) applies before an invariant is reported at both function and class level. The default and the config key are documented where spec options are documented.
- [ ] Range templates (observed min/max bounds) are added, or the close note gives explicit reasoning for not adding them.
- [ ] A proptest checks that every inferred invariant holds on all of its source traces, and that no invariant is emitted from fewer than N specimens.
- [ ] A regression test: a function explored with a single execution produces no function-level invariants. At close, show it failing on current `main` and passing after the fix.
- [ ] Spec snapshots that change are regenerated and reviewed in the same change, and the diff is summarized in the close note.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add the support threshold in `spec.rs`, next to the existing per-class `>= 2` check, so both call sites use one constant or config value. Add range templates in `invariants.rs`. Coordinate with str-qwua7.61 on the confidence field: this issue covers the support threshold and templates, and str-qwua7.61 covers the confidence value.

## Out of scope

- Removing or computing `confidence` (str-qwua7.61).
- General proptest coverage of `invariants.rs` beyond the invariant above (str-qwua7.47).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.61, str-qwua7.47.

## References

Audit 2026-09-22 finding core-11 (verified, P2). Source draft: `drafts/shatter-code/20-invariant-min-support-templates.md`.
