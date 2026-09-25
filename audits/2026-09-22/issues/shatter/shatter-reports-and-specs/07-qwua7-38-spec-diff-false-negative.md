---
slug: qwua7-38-spec-diff-false-negative
kind: note-to-existing
title: "Note on str-qwua7.38: exact-BranchPath pairing causes false negatives, not only noise"
priority: P2
type: bug
labels: [spec-diff, audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.38
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.38: exact-BranchPath pairing causes false negatives, not only noise

Target: **str-qwua7.38** (open, P2, "spec-diff: pair classes by behavior when branch paths are renumbered, not by exact BranchPath"). Action: append the comment below. Do not change the priority (stays P2; the audit finding artifacts-06 was filed at P1 and the verifier lowered it to P2 because the tool still exits non-zero in this example).

## Comment text

**Audit 2026-09-22 note (finding artifacts-06): pairing by exact BranchPath also hides real changes.**

This issue describes renumbered branch IDs as noise (unchanged behavior shows up as ADDED + REMOVED). The 2026-09-22 audit found that the same pairing can also pair two different behaviors and hide the real regression.

Weight: under maintainer decision D2 (2026-09-23), the snapshot `shatter diff` command is being retired and `spec-diff` is the only regression tool. A false negative in spec-diff is therefore a false negative in Shatter's whole regression story, which makes this issue more important than when it was filed.

Evidence (files under `audits/2026-09-22/artifact-samples/` on branch `audit-2026-09-22` until the audit reports land):

- v1 `grade`: `n < 0` -> invalid, `n < 50` -> fail, else pass. v2 inserts `n > 100` -> overflow.
- v1 `fail` and v2 `overflow` share `BranchPath [(0,F),(1,T)]`, so they are paired (`sd-v1.json`, `sd-v2.json`).
- `spec-diff` output (`sd-diff.txt`): `Summary: 2 added, 1 removed, 1 precondition(s) changed, 1 class(es) with insufficient comparison evidence`, then `[ADDED] Class 2 — returns "fail"`, `[ADDED] Class 4 — returns "pass"`, `[REMOVED] Class 3 — returns "pass"`, a `[PRECOND]` change and `[INCONCLUSIVE] Class 2`. The word `overflow` never appears, so the real regression (inputs > 100 now return `overflow` instead of `pass`) is not reported. The command still exits non-zero here only because of the unrelated REMOVED class.
- Pairing code today (re-checked 2026-09-23, audit worktree HEAD 793f2b0b, code identical to 56c86168): `shatter-core/src/spec_diff.rs:134-141` builds `old_by_path`/`new_by_path` keyed on `&c.branch_path` and matches with `old_by_path.get(&new_class.branch_path)`; removed classes at `:201`. The `~93-97` line reference in this issue's description is stale.

What spec-diff can and cannot establish from these two files: v1 recorded "pass" only at input 50 and v2 recorded "overflow" only at input 101, and spec-diff does not execute targets. The files alone therefore cannot prove that a specific input moved from "pass" to "overflow". They do prove that v2 has a behavior ("overflow") that no v1 class has, and that the v1 "fail" and v2 "overflow" classes have different postconditions even though their branch paths match.

Proposed additions to this issue's acceptance criteria (pairing only):

- An exact `BranchPath` match must not by itself pair two classes whose postconditions differ (under the existing nondeterminism-aware comparison). Such classes are left unpaired, so the v2 class is reported ADDED and the v1 class REMOVED.
- Known-answer test using `sd-v1.json` / `sd-v2.json` (copy them into the test fixtures): the output contains an `[ADDED]` row for `returns "overflow"`, contains no `[PRECOND]` or `[INCONCLUSIVE]` row that pairs "fail" with "overflow", and the command exits non-zero. It fails on current code (the word `overflow` does not appear); record the failing run.

Not part of this issue: the concrete verdict "inputs > 100 changed from pass to overflow". That needs each class's symbolic input region and a solver check, and is owned by the new audit issue spec-diff-symbolic-region-verdicts (blocked by spec-preconditions-from-path-constraints), which uses the same grade pair as its known-answer test. That issue does not change pairing; whichever of the two lands second rebases and re-runs both sets of tests.
