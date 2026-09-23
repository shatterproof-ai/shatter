---
slug: spec-preconditions-from-path-constraints
kind: new
title: "Spec preconditions are sample statistics (often false or vacuous); derive them from the symbolic path constraints already recorded"
priority: P2
type: feature
labels: [spec, spec-diff, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec preconditions are sample statistics (often false or vacuous); derive them from the symbolic path constraints already recorded

## Problem

Class preconditions come from statistics over the observed samples (`AllEqual param[0] == -1`, `param[0] > 0`, `SameType number`), not from the path condition. They are sometimes false and often vacuous:

- In `fmt2`, the `n > 10` class shows the precondition `param[0] > 0`, although `n = 5` throws. The throw class gets `typeof param[0] == "number"`.
- In `regress/base.json`, class `returns "negative"` has `AllEqual{param_index: 0, value: -1}` with `sample_count: 20`: 20 samples of the same value become an equality precondition.

This also weakens `spec-diff`, which is now the only regression tool (maintainer decision D2). After swapping the even/odd outputs of a function, `spec-diff` reports `[PRECOND] Class 3 ... - param[0] == 2 + param[0] == 1` and `[INCONCLUSIVE]`, not "input 2 now returns positive-odd". The closed issue str-0oc promised constraint-derived preconditions; the prior audit (2026-09-04) raised this again.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/equivalence.rs:218` `fn derive_preconditions(all_inputs: &[Vec<serde_json::Value>])` builds `AllEqual` (:236), `AllZero` (:249), `AllPositive` (:255), `AllNegative` (:261) and `SameType` (:272) from input values only.
- `shatter-core/src/spec.rs:451-518` `build_spec_class` copies `ec.common_preconditions` into `SpecClass.preconditions` (:518). Provenance is `Proven` only if every branch in the path was discovered by Z3.
- Every raw result's `branch_path` already carries constraints, for example `{op: gt, left: {param n}, right: {const 10}}`.
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/regress/base.json` (AllEqual preconditions with sample_count 20/20/7/13, all provenance `observed`); `audits/2026-09-22/artifact-samples/edge-plain-spec.md` (the fmt2 `param[0] > 0` class); `sd-v2.json` (`fail` gets `SameType number`). The verifier re-ran `shatter spec-diff base.json new.json` and got the `[PRECOND]` + `[INCONCLUSIVE]` output quoted above.

## Acceptance criteria

- [ ] Each class's precondition is the conjunction of its branch constraints, rendered as source-like text (for example `n < 0`, `n > 10 && n % 2 == 0`). Provenance marks these as symbolic.
- [ ] Sample statistics move to a separate "observed inputs" field. Preconditions shared by every class are suppressed.
- [ ] `spec-diff` re-executes, or cross-references, old examples against the new classes, so the even/odd swap is reported as a changed postcondition for a concrete input (`input 2: "positive-even" -> "positive-odd"`), not as a precondition change plus inconclusive.
- [ ] Known-answer tests pin the preconditions for `classifyNumber` (TS, Go, Rust) and for `fmt2`. They fail on current code; record both runs.
- [ ] A spec-diff known-answer test covers the even/odd swap and fails on current code.
- [ ] The spec schema version is bumped and the spec changelog has a row, and spec-diff still reads old bundles (or says clearly that it cannot).

## Suggested approach

Render the class's recorded path constraints through a small constraint printer (param names from the analysis). Keep the current statistics under a new `observed_inputs` field. Bump the schema once, together with spec-yaml-custom-tags and spec-json-shapes-compare. Coordinate the spec-diff pairing change with str-qwua7.38.

## Out of scope

- Behavior-based class pairing in spec-diff (str-qwua7.38).
- The YAML tag shape (spec-yaml-custom-tags).

## Related

str-0oc (closed, promised this), str-dcz, str-qwua7.38. Source findings: artifacts-08 (confirmed), goals-13 (confirmed).
