---
slug: spec-preconditions-from-path-constraints
kind: new
title: "Spec preconditions are sample statistics (often false or vacuous); export each class's symbolic path condition, with explicit unknown parts"
priority: P2
type: feature
labels: [spec, spec-diff, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [spec-json-shapes-compare]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec preconditions are sample statistics (often false or vacuous); export each class's symbolic path condition, with explicit unknown parts

## Problem

Class preconditions come from statistics over the observed samples (`AllEqual param[0] == -1`, `param[0] > 0`, `SameType number`), not from the path condition. They are sometimes false and often vacuous:

- In `fmt2`, the `n > 10` class shows the precondition `param[0] > 0`, although `n = 5` throws. The throw class gets `typeof param[0] == "number"`.
- In `regress/base.json`, class `returns "negative"` has `AllEqual{param_index: 0, value: -1}` with `sample_count: 20`: 20 samples of the same value become an equality precondition.

The executions already record the symbolic constraint of every branch decision, but the spec drops them: equivalence classes are keyed on a `BranchPath` of `(branch_id, taken)` pairs only. This issue carries those constraints into the spec so that each class states the input region it covers. It is also the prerequisite for spec-diff-symbolic-region-verdicts, which uses the regions to report real regressions that spec-diff misses today (maintainer decision D2 makes spec-diff the only regression tool). The closed issue str-0oc promised constraint-derived preconditions; the prior audit (2026-09-04) raised this again.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/equivalence.rs:218` `fn derive_preconditions(all_inputs: &[Vec<serde_json::Value>])` builds `AllEqual` (:236), `AllZero` (:249), `AllPositive` (:255), `AllNegative` (:261) and `SameType` (:272) from input values only.
- `shatter-core/src/equivalence.rs:31-40`: `BranchPath::from_decisions` keeps only `branch_id` and `taken` from each `BranchDecision`; the `constraint` field is discarded when classes are built.
- `shatter-core/src/execution_record.rs:17-25,50-64`: `BranchDecision.constraint` is a `SymConstraint`, which is either `Expr { expr }` or `Unknown { hint }`, and defaults to `Unknown` when a frontend omits it. `taken` records which side ran, so the class condition for a not-taken decision is the negation of `expr`.
- `shatter-core/src/spec.rs:451-518` `build_spec_class` copies `ec.common_preconditions` into `SpecClass.preconditions` (:518). Provenance is `Proven` only if every branch in the path was discovered by Z3.
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/regress/base.json` (AllEqual preconditions with sample_count 20/20/7/13, all provenance `observed`); `audits/2026-09-22/artifact-samples/edge-plain-spec.md` (the fmt2 `param[0] > 0` class); `sd-v2.json` (`fail` gets `SameType number`).

## Contract

- **Retention.** When executions are grouped into a class, the class keeps the ordered list of `(branch_id, taken, SymConstraint)` for its path. If executions in one class recorded different constraints for the same branch step (for example a loop or a frontend that emits `Unknown` on some runs), the class keeps the step as `unknown` with a hint naming the disagreement. Nothing is dropped silently.
- **Negation.** Each step contributes `expr` when `taken` is true and `not(expr)` when false. The conjunction of the steps is the class's `path_condition`.
- **Incomplete knowledge.** A step whose constraint is `Unknown` appears in `path_condition` as an explicit `unknown` conjunct carrying its hint. The class gets `path_condition_status`: `complete` (every step symbolic), `partial` (some unknown) or `none` (all unknown, or no branches). A `partial` condition over-approximates the region and must be labelled as such in every renderer.
- **Rendering.** Markdown and text render `path_condition` as source-like text using parameter names from the analysis (for example `n < 0`, `!(n < 0) && !(n === 0) && n % 2 === 0`), with unknown conjuncts shown as `<unknown: hint>`.
- **Statistics.** The current sample statistics move to a separate `observed_inputs` field and are rendered under that name, not as preconditions. Statistics shared by every class are not rendered.
- **Schema.** This bumps `SPEC_SCHEMA_VERSION` by one and adds an upgrade step to the shared reader from spec-json-shapes-compare (this issue is blocked by it): a legacy bundle upgrades with `path_condition_status: none` and its old `preconditions` moved to `observed_inputs`. The previous-version bundle is added to `shatter-core/tests/fixtures/spec-legacy/` first.

## Acceptance criteria

- [ ] The retention, negation, incomplete-knowledge, rendering, statistics and schema rules above are implemented. The JSON class carries `path_condition` (structured, reusing the `SymExpr` representation) and `path_condition_status`.
- [ ] Known-answer tests pin the path condition of every class of `classifyNumber` for TS, Go and Rust under both engines (random and `--concolic`; CLAUDE.md "parallel parity"), and of `fmt2`. `fmt2` was an audit scratch file (`edge/plain.ts`, not checked in): recreate it as a test fixture from its description in `audits/2026-09-22/areas/artifacts.md` (F2: `n > 10` returns "big", `n < 0` returns "neg", otherwise throws `Error("bad")`); the "big" class must not claim `n > 0`. For each language and engine the test pins what is actually produced, including `partial`/`none` where a frontend emits `Unknown`; the close comment gives the per-language, per-engine status table. Each test fails on current code (no `path_condition`); record both runs.
- [ ] A test with a class whose executions disagree on one step's constraint shows that step as `unknown` and the class as `partial`.
- [ ] A proptest over generated decision lists checks that `path_condition` has exactly one conjunct per branch step, that not-taken steps are negated, and that the status is `complete` if and only if no conjunct is unknown.
- [ ] Legacy reading: `spec-diff` and `compare` on the previous-version fixture against a fresh bundle of the same source give the same verdicts as before this change (spec-diff-symbolic-region-verdicts has not landed yet, so no region verdicts are expected).
- [ ] `task e2e` passes with forced execution (not a cached "up to date"); output recorded in the close comment.

## Suggested approach

Carry the constraint list next to `BranchPath` in the equivalence class (keep `BranchPath` itself as the grouping key so class identity does not change). Write a small `SymExpr` pretty-printer keyed on parameter names from the analysis.

## Out of scope

- Any change to spec-diff verdicts. Region-overlap regression verdicts are spec-diff-symbolic-region-verdicts; behavior pairing of renumbered classes is str-qwua7.38.
- The YAML tag shape (spec-yaml-custom-tags).
- Simplifying or minimizing the conjunction (for example with Z3). A literal conjunction is acceptable.

## Related

str-0oc (closed, promised this), str-dcz, str-qwua7.38. Blocked by spec-json-shapes-compare; blocks spec-diff-symbolic-region-verdicts. Source findings: artifacts-08 (confirmed), goals-13 (confirmed).
