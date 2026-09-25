---
slug: invariant-min-support
kind: new
title: "Invariant inference over-generalizes: function-level invariants are published from a single execution (no minimum support)"
priority: P2
type: bug
labels: [spec, invariants, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Invariant inference over-generalizes: function-level invariants are published from a single execution (no minimum support)

## Problem

Function-level invariant detection runs on any number of execution records, including one. `detect_invariants` keeps every template that holds on all specimens, so a single execution yields invariants like `x == <constant>` and `x > 0`, which are then published in the spec. Per-class inference already requires at least 2 records, but the function level has no floor at all.

Support is only partly visible to readers. The markdown spec prints `(satisfied/total)` next to each function invariant, so `(1/1)` shows up there. The YAML spec deliberately omits `satisfied_count`/`total_count`, and `confidence` is hard-coded to 1.0, so YAML consumers cannot tell a one-specimen invariant from a well-supported one.

Numeric comparison templates also compare only against 0, so real bounds (such as `0 <= x <= 100`) are never inferred. That is a separate enhancement and is out of scope here (see below).

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/spec.rs:394-400`: `detect_classified_invariants(&all_records, ...)` runs on all records with no minimum count. Per-class inference requires `class_records.len() >= 2` (`:411`).
- `shatter-core/src/invariants.rs:504` `detect_invariants`: keeps any template true on all specimens and returns early only on empty input.
- `shatter-core/src/invariants.rs:661`: `confidence: 1.0` is hard-coded, with `satisfied_count` and `total_count` both set to the specimen count. Removing or computing `confidence` is tracked in open str-qwua7.61.
- `shatter-core/src/spec.rs:568-575`: the markdown renderer prints `[confidence] (satisfied/total)`. The test at `spec.rs:2767-2775` asserts that YAML does **not** contain `satisfied_count` or `total_count`.
- `shatter-core/src/invariants.rs:262-335`: the three `NumericComparison` templates use `value: 0.0` (`:270`, `:291`, `:312`). The fourth entry (`:330-332`) is a `NumericConstant` whose `0.0` is a placeholder set during detection, not another comparison against 0.
- `invariants.rs` has 0 `proptest!` blocks. Open str-qwua7.47 (checked with `bd show` on 2026-09-23) owns the property "every returned invariant holds on the specimens it was inferred from".

## Acceptance criteria

- [ ] One minimum-support threshold applies before an invariant is reported, at both function and class level. The default is 5 specimens and it is configurable. The default and the config key are documented where spec options are documented. The existing per-class `>= 2` check uses the same constant or config value.
- [ ] A proptest: for any record set smaller than the threshold, `detect_classified_invariants` (or the spec-building function that calls it) emits no invariants; for record sets at or above it, the output equals what `detect_invariants` returns today for the same records. The "invariant holds on its source traces" property stays with str-qwua7.47 and is not duplicated here.
- [ ] A regression test: a function explored with a single execution produces no function-level invariants in either the markdown or the YAML spec. At close, quote it failing on current `main` and passing after the fix.
- [ ] Spec snapshots that change are regenerated and reviewed in the same change, and the close note summarizes the diff (which invariants disappeared and why).
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add the support threshold in `spec.rs`, next to the existing per-class `>= 2` check, so both call sites use one constant or config value. Coordinate with str-qwua7.61 on the confidence field: this issue covers the support threshold only.

## Out of scope

- Range or min/max templates. Observed extrema from sampled traces do not establish general bounds, so this needs its own design. File separately if wanted.
- Removing or computing `confidence` (str-qwua7.61).
- Proptest coverage of `invariants.rs`, including the source-trace property (str-qwua7.47).
- Showing support counts in YAML output.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.61 (confidence), str-qwua7.47 (owns the source-trace property).

## References

Audit 2026-09-22 finding core-11 (verified, P2). Source draft: `drafts/shatter-code/20-invariant-min-support-templates.md`.
