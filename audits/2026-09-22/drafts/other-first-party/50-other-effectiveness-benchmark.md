# Deliver a minimal effectiveness benchmark (known-answer and downstream subset); retire or fix holdout

## Filing metadata

- tracker/repo: shatter (fallback for other: shatter-effectiveness/holdout have no usable tracker)
- action: create new issue
- type: task
- priority: P2
- labels: audit-2026-09-22, effectiveness
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: goals-10

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes (decision on location explicitly delegated to implementer)
- too_broad: borderline — split after location decision

<!-- BODY -->
## Problem

Shatter's effectiveness measurement has been designed three times and never delivered:

- `~/project/holdout`: the last run (`results/2026-04-09T08-34-54/summary.json`) reports targets_ok 5, targets_failed 7, and `total_branches_covered 294 / total_branches 294`, which cannot be real. `total_errors 401` equals `total_functions_skipped 401`, so fingerprint-match cache skips are counted as errors. Its bd tracker is empty.
- `~/project/shatter-effectiveness`: a 1057-line design (`docs/specs/2026-08-27-effectiveness-benchmark-design.md`) and a 1260-line 13-task plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`). About 15 commits built a docs-integrity landing gate, and there is no `bench/` code. The last commit was 2026-08-31, and the repo has no tracker.

As a result, nobody can tell whether engine changes (for example concolic vs default explorer) improve coverage. Downstream ≥90% coverage goals have stalled at 18-28% since 2026-07-07.

## Acceptance criteria

- [ ] A minimal on-demand benchmark exists. It covers either the smallest slice of the effectiveness plan (Task 1 probes plus Tasks 4-5 distill/score over one target), or a `task`-level known-answer benchmark in shatter built from the examples' `EXPECTED BRANCHES` comments plus one downstream subset. It records a dated number per run.
- [ ] holdout is either archived (README note) or its metric and skip accounting are fixed.
- [ ] The decision on where the benchmark lives (shatter-effectiveness vs shatter) is recorded in this issue.

## Too-broad note

This is a decision plus a first slice. Split it once the location decision is made.

## Source

Shatter audit 2026-09-22 finding goals-10 (`areas/goals.md`).
