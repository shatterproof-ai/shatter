---
slug: effectiveness-benchmark-holdout
kind: new
title: "Deliver a minimal bug-finding effectiveness benchmark (known-answer + downstream subset) and retire or fix holdout"
priority: P2
type: task
labels: [audit-2026-09-22, effectiveness, benchmark]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Deliver a minimal bug-finding effectiveness benchmark (known-answer + downstream subset) and retire or fix holdout

## Problem

Shatter's effectiveness measurement ("does it find real bugs and behaviours?") has been designed three times and never delivered, so no one can tell whether engine changes improve results. This issue lives in the shatter tracker because `~/project/shatter-effectiveness` has no tracker and holdout's bd has no issues.

- **holdout** (`~/project/holdout`): the last run (`results/2026-04-09T08-34-54/summary.json`, shatter 120ef02a) reports `targets_ok 5`, `targets_failed 7` and `total_branches_covered 294` of `total_branches 294`, which cannot be real. `total_errors 401` equals `total_functions_skipped 401`: fingerprint-match cache skips ("unchanged (fingerprint match)") are counted as errors. It has not run since April.
- **shatter-effectiveness** (`~/project/shatter-effectiveness`): it contains a 1,057-line design (`docs/specs/2026-08-27-effectiveness-benchmark-design.md`) and a 1,260-line 13-task plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`). It holds only `docs/ scripts/ tests/` (a docs-integrity gate) and no `bench/` code. Last commit: 6e234fe, 2026-08-31, "Merge implementation-plan".
- Downstream ≥90% coverage goals (kapow, zolem, pickpackit) have stalled at 18-28% since 2026-07-07 with no metric linking engine work to outcomes (see downstream-coverage-goals-epic).

This is distinct from concolic-vs-default-benchmark, which measures **coverage per explorer**. This issue measures **bug/behaviour-finding effectiveness**. The two may share a harness (corpus checkout, manifest format, run/record scripts).

## Evidence (re-verified 2026-09-23)

```
$ python3 -c "import json;d=json.load(open('holdout/results/2026-04-09T08-34-54/summary.json')); print({k:v for k,v in d.items() if not isinstance(v,(list,dict))})"
{... 'targets_ok': 5, 'targets_failed': 7, 'total_functions_explored': 176, 'total_scope_functions': 577,
 'total_functions_skipped': 401, 'total_branches_covered': 294, 'total_branches': 294, 'total_behaviors': 11454, 'total_errors': 401}
$ git -C shatter-effectiveness log -1 --format='%h %ad %s' --date=short
6e234fe 2026-08-31 Merge implementation-plan: thirteen-task plan for the benchmark
$ ls shatter-effectiveness
docs  scripts  tests
```

## Acceptance criteria

- [ ] Location decision recorded in this issue: shatter-effectiveness, or a `task` target inside shatter. Reasons: tracker, CI access, corpus pinning.
- [ ] A minimal on-demand benchmark exists and records a dated result file per run. It is either:
  - the smallest slice of the effectiveness plan (Task 1 probes plus Tasks 4-5 distill/score over one target), or
  - a known-answer benchmark built from the examples' `EXPECTED BRANCHES` comments plus a seeded-bug subset of one downstream project.

  The metric is bug/behaviour-finding (for example seeded faults detected and expected outcomes found), not raw line coverage.
- [ ] holdout is either archived (README note pointing to the new benchmark) or fixed: the branch totals must be real, and fingerprint-match skips must be counted as skips, not errors.
- [ ] If the harness is shared with concolic-vs-default-benchmark, the shared parts are named in both issues.
- [ ] Proof at close: the command and output of one uncached run, plus the committed or published dated result file.
- [ ] If the location is shatter-effectiveness, a tracker (bd or GitHub Issues) is initialized there and the remaining plan tasks are filed there. This issue closes with links to them.

## Suggested approach

Make the location decision first, then split: (1) the first slice of the benchmark, (2) the holdout disposition. Reuse `scripts/examples_checkout.py` and a pinned examples SHA (pin-examples-repo).

## Out of scope

- The full 13-task plan.
- Coverage comparison between explorers (concolic-vs-default-benchmark).
- Downstream coverage-goal work itself.

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, effectiveness, benchmark
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: concolic-vs-default-benchmark (possible shared harness, different metric), downstream-coverage-goals-epic, pin-examples-repo; kapow-94wr (earlier "eval harness unreliable" symptom); str-jeen.14 (closed, broad-run validation corpus)
- Source findings: goals-10 (draft other-first-party/50)
