---
slug: explore-function-not-found-diagnostics
kind: new
title: "explore file:missingFn reports an all-zero failure breakdown and does not list available functions"
priority: P3
type: bug
labels: [cli, explore, error-handling, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore file:missingFn reports an all-zero failure breakdown and does not list available functions

## Problem

`shatter explore arithmetic-v1.ts:doesNotExist` prints `[error] Analyze error (FunctionNotFound): Function not found: doesNotExist in arithmetic-v1.ts` and then `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0); no completed functions`. The breakdown has no category for analyze failures, so every counter is 0, and the error does not list the functions that do exist.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/err-nofn.err`.
- `shatter-cli/src/commands/explore.rs` `ExploreFailure::AllAttemptedTargetsFailed { attempted_targets, build_failed, runtime_failed, timed_out }` (returned by `decide_explore_exit_status`, `explore.rs:744`): no analyze/not-found bucket.
- Finding artifacts-16 (P3).

## Acceptance criteria

- [ ] A missing target function is counted in its own category (for example `analyze_failed=1` or `not_found=1`) in the failure summary; the summary never reports `N attempted target(s) failed` with all counters 0. A unit test on `decide_explore_exit_status` covers it.
- [ ] The error lists the exported functions in that file and adds a "did you mean `<name>`?" suggestion when one is close (edit distance). A CLI test on a TS fixture asserts both; it fails on current HEAD.
- [ ] Exit code stays 2.
- [ ] `task affected` passes. The close comment records the gates selected.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.12 (exit codes), str-qwua7.33.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 3.
