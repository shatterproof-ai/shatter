---
slug: spec-diff-symbolic-region-verdicts
kind: new
title: "spec-diff misses real regressions: report CHANGED when old and new classes cover overlapping input regions with different outcomes (Z3 witness), INCONCLUSIVE when regions are unknown"
priority: P2
type: feature
labels: [spec-diff, spec, solver, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [spec-preconditions-from-path-constraints]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# spec-diff misses real regressions: report CHANGED when old and new classes cover overlapping input regions with different outcomes (Z3 witness), INCONCLUSIVE when regions are unknown

## Problem

`spec-diff` reads two spec files and never executes the target. It can only compare what the files record, and today each class records a branch path, sample statistics and one or a few example inputs. When a behavior change moves inputs from one class to another, the two files usually have no example input in common, so spec-diff cannot state what changed. Two audit cases:

- **grade.** v1: `n < 0` → "invalid", `n < 50` → "fail", else "pass". v2 inserts `n > 100` → "overflow" before the `< 50` test. The only real change is that inputs above 100 now return "overflow" instead of "pass". spec-diff reports `2 added, 1 removed, 1 precondition(s) changed, 1 inconclusive`, and the word `overflow` never appears. v1 recorded "pass" only at input 50, and v2 recorded "overflow" only at 101, so no shared example exists.
- **even/odd swap.** `classifyNumber` with the "positive-even"/"positive-odd" results swapped: spec-diff reports `[PRECOND] Class 3 ... - param[0] == 2 + param[0] == 1` and `[INCONCLUSIVE]`, not a changed outcome. Base recorded "positive-even" at input 2; new recorded it at input 1.

Once spec-preconditions-from-path-constraints lands, each class carries its symbolic `path_condition` with a `complete`/`partial`/`none` status. With complete conditions on both sides, spec-diff can decide the question from the files alone: an old class O and a new class N whose conditions are jointly satisfiable, but whose postconditions differ, are a behavior change for every input in that intersection, and Z3 can produce a concrete witness. For grade, v1 "pass" (`!(n < 0) && !(n < 50)`) intersects v2 "overflow" (`!(n < 0) && n > 100`), witness 101. Where a condition is `partial` or `none`, the region is not known and the verdict must stay INCONCLUSIVE; spec-diff must never report "no change" for it.

Maintainer decision D2 makes spec-diff the only regression tool, so its false negatives are false negatives for Shatter's whole regression story.

## Division of work with str-qwua7.38

- **str-qwua7.38** owns class *pairing*: which old class corresponds to which new class when branch IDs are renumbered, and the rule (added by the audit note qwua7-38-spec-diff-false-negative) that a path match alone must not pair classes whose postconditions differ.
- **This issue** owns *region verdicts*: for every old/new class pair whose path conditions intersect (whether or not the pairing step paired them), report CHANGED with a witness when the postconditions differ. It does not change pairing.
- Whichever lands second rebases onto the other; the second one re-runs both issues' known-answer tests.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/spec_diff.rs:134-141`: classes are paired only by exact `branch_path`; no field of `SpecClass` holds a symbolic condition today.
- `shatter-cli/src/commands/diff.rs` (spec-diff command): reads files only; the help text says "Matched classes are only compared when both sides recorded a comparable canonical example; otherwise the pair is reported as inconclusive".
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/artifact-samples/sd-v1.json`, `sd-v2.json`, `sd-diff.txt` (grade; class examples `[[-1]]`, `[[0]]`, `[[50]]` in v1 and `[[-1]]`, `[[0]]`, `[[101]]`, `[[100]]` in v2); `audits/2026-09-22/goals-runs/regress/base.json`, `new.json` (even/odd swap; "positive-even" example `[[2]]` in base and `[[1]]` in new). Write-up: `audits/2026-09-22/areas/artifacts.md` F5.

## Acceptance criteria

- [ ] For each function present in both files, spec-diff checks every (old class, new class) pair where both `path_condition`s are `complete` and the postconditions differ (using the existing nondeterminism-aware postcondition comparison). If the conjunction of the two conditions is satisfiable under Z3, it reports `[CHANGED]` naming both postconditions, the intersected condition as text, and a Z3 witness input. The exit code is non-zero.
- [ ] If either side of an otherwise overlapping pair is `partial` or `none`, or the two functions' parameter lists differ, the pair is reported `[INCONCLUSIVE]` with the reason (`unknown region`, `signature changed`). Such a pair never produces "no change". A Z3 `unknown` result or timeout is also INCONCLUSIVE.
- [ ] Known-answer test **grade**: TS sources for v1 and v2 (as described above) are checked in as test fixtures; the test explores both with `explore --spec-out` and a fixed `--max-iterations`, runs `spec-diff`, and asserts a `[CHANGED]` row from "pass" to "overflow" whose witness satisfies `n > 100`, and exit non-zero. Close-time proof: the test failing on the code before this issue (with spec-preconditions-from-path-constraints landed) and passing after.
- [ ] Known-answer test **even/odd swap**: same shape for `classifyNumber` with the two positive results swapped; asserts `[CHANGED]` rows between "positive-even" and "positive-odd" with witnesses of the right parity.
- [ ] Negative control: spec-diff of two independent explorations of the same unchanged source reports no `[CHANGED]` row (run it on `classifyNumber` for TS, Go and Rust).
- [ ] Legacy input: spec-diff of a legacy bundle (no `path_condition`, upgraded to status `none` by the shared reader) against a new bundle reports INCONCLUSIVE for overlapping pairs and does not crash.
- [ ] `--json` output carries the verdict, both class labels, the intersected condition and the witness; a round-trip test covers it.
- [ ] `task e2e` passes with forced execution; output recorded in the close comment.

## Suggested approach

Reuse the core Z3 translation of `SymExpr` that the concolic solver already uses. Bound each satisfiability check with a small timeout and treat timeouts as INCONCLUSIVE. Pairs can be pruned by postcondition first (only differing postconditions need a solver call).

## Out of scope

- Pairing renumbered classes (str-qwua7.38).
- Replaying recorded examples against the other version's source. spec-diff stays file-only.
- Producing symbolic conditions (spec-preconditions-from-path-constraints).

## Related

str-qwua7.38, str-nfg4y (canonical-input guard), str-0oc. Blocked by spec-preconditions-from-path-constraints. Audit note on str-qwua7.38: qwua7-38-spec-diff-false-negative. Source findings: artifacts-06, artifacts-08, goals-13.
