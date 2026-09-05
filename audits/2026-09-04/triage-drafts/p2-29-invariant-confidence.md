---
repo: shatter
type: bug
priority: 2
labels: core, docs
existing: none
---
# Triage: invariant confidence — implement satisfied/total with a minimum-sample threshold, or remove the claim from SPEC §3.4

Triage: maintainer decision required. Proposed default: **implement `satisfied/total` with a minimum of 3 samples** (option 1).

## Problem
SPEC §3.4 says invariants "are classified with confidence scores (satisfied_count / total_count)". The detector only returns invariants that hold for every specimen, so `confidence` is a hard-coded `1.0` and `satisfied_count == total_count` always. There is no minimum-sample guard, so with two executions `x > 0` is reported as an invariant with confidence 1.0. The spec overstates what the number means.

## Current code facts
- `shatter-core/src/invariants.rs:655-665`: `ClassifiedInvariant { confidence: 1.0, // detect_invariants only returns invariants that hold for all specimens, satisfied_count: total, total_count: total }`.
- `SpecInvariant` (`spec.rs:118`) carries `confidence: ConfidenceLevel`; rendered by `specify --yaml` / `properties` as `property:` lines and compared by `spec-diff` "lost properties".
- Nine `InvariantKind`s (invariants.rs:35) match the SPEC §3.4 table; no implication/disjunction; no cross-param relation beyond `OutputEqualsInput`.
- SPEC.md:648: "Invariants are classified with confidence scores (satisfied_count / total_count) and reported at both function-wide and per-class levels."

## Options
1. **Implement (proposed default)**: `detect_invariants` evaluates every candidate against all specimens and returns those with `satisfied/total ≥ min_confidence` (config `invariants.min_confidence`, default 1.0 so output is unchanged by default) and `total ≥ min_samples` (config `invariants.min_samples`, **default 3**); proptest: reported `confidence == satisfied/total`; no invariant emitted from fewer than `min_samples` specimens; `spec-diff` "lost properties" unaffected at the defaults.
2. **Remove the claim**: SPEC §3.4 rewritten to "invariants are reported only when they hold for all N observed samples; `sample_count` is exported; no statistical confidence is computed"; `confidence` removed from the JSON/YAML or documented as always 1.0.

## Acceptance checks
- Decision recorded; SPEC §3.4 and `invariants.rs` agree; `specify --yaml`/`properties` output covered by a test at the defaults.

## Scope
In: invariants.rs, SPEC §3.4, YAML property rendering. Out: new invariant kinds.

## Size
small (doc) / medium (implementation).

## Provenance
Audit 2026-09-04, section 11, action item 29; evidence audits/2026-09-04/design-foundation.md §1 (Invariant detection), rec 7.
