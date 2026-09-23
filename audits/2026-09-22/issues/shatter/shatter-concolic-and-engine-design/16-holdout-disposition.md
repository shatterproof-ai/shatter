---
slug: holdout-disposition
kind: new
title: "holdout reports impossible totals (294/294 branches, 401 fingerprint skips counted as errors) and has not run since April; archive or fix it"
priority: P3
type: task
labels: [audit-2026-09-22, effectiveness, benchmark, holdout]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [effectiveness-benchmark-holdout]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# holdout reports impossible totals (294/294 branches, 401 fingerprint skips counted as errors) and has not run since April; archive or fix it

## Problem

`~/project/holdout` is an earlier effectiveness harness. Its last run (`results/2026-04-09T08-34-54/summary.json`, shatter 120ef02a) reports numbers that cannot be real:

- `total_branches_covered 294` of `total_branches 294` (100%), while `targets_failed` is 7 of 12;
- `total_errors 401` equals `total_functions_skipped 401`: functions skipped as "unchanged (fingerprint match)" are counted as errors.

It has not run since April. Anyone reading its results gets a wrong picture of Shatter's effectiveness. This issue is filed in the shatter tracker because holdout's bd has no issues.

## Evidence (re-verified 2026-09-23)

```
$ python3 -c "import json;d=json.load(open('holdout/results/2026-04-09T08-34-54/summary.json')); print({k:v for k,v in d.items() if not isinstance(v,(list,dict))})"
{... 'targets_ok': 5, 'targets_failed': 7, 'total_functions_explored': 176, 'total_scope_functions': 577,
 'total_functions_skipped': 401, 'total_branches_covered': 294, 'total_branches': 294, 'total_behaviors': 11454, 'total_errors': 401}
```

## Acceptance criteria

Exactly one of these:

- [ ] **Archive:** holdout's README states at the top that it is retired, why (the two defects above), and points to the benchmark delivered by effectiveness-benchmark-holdout. Any scheduled or documented invocation of holdout, in shatter docs or elsewhere, is removed or redirected. Proof: the README diff and `grep -rn holdout` over shatter docs showing no live instructions.
- [ ] **Fix:** the branch total is computed from per-target data, so a run with failed targets cannot report 100%; fingerprint-match skips are counted as skips, not errors. A test over a synthetic summary with one failed target and one fingerprint skip asserts both. Proof: the test failing before and passing after, and one fresh run's `summary.json` with plausible totals.

The close note states which option was taken and why.

## Out of scope

- The new benchmark (effectiveness-benchmark-holdout).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, effectiveness, benchmark, holdout
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: effectiveness-benchmark-holdout (the archive option needs its replacement to point to)
- Source findings: goals-10 (split from draft other-first-party/50)
