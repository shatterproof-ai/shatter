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
- Pairing code today (re-checked 2026-09-23, HEAD 56c86168): `shatter-core/src/spec_diff.rs:134-141` builds `old_by_path`/`new_by_path` keyed on `&c.branch_path` and matches with `old_by_path.get(&new_class.branch_path)`; removed classes at `:201`. The `~93-97` line reference in this issue's description is stale.

Proposed additions to the acceptance criteria:

- Add this v1/v2 `grade` pair as a known-answer spec-diff test that must report a CHANGED postcondition for inputs > 100 (`pass` -> `overflow`). It fails on current code; record the failing run.
- Behavior pairing must check that paired classes agree on outcomes for shared concrete examples before reporting them as the same class; a path match alone must not suppress a postcondition change.
- Related new issue: spec-preconditions-from-path-constraints (spec-diff reports an even/odd output swap as a precondition change plus inconclusive, the same class of false negative).
