---
slug: run-validity-degraded-cause-diagnosis
kind: new
title: "Diagnose why `shatter run` rates an all-TS, fully supported example directory as 'degraded' (61.5% represented source)"
priority: P2
type: task
labels: [report, run, validity, diagnosis, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why `shatter run` rates an all-TS, fully supported example directory as 'degraded'

## Problem

`shatter run .` over a directory of six TS example files, all in a supported language, reported `represented_source_percent=61.5 below high threshold 75.0` and a `degraded` verdict. Either the representation math is wrong (a bug that makes every run look worse than it is) or about 38% of the source lines really are unrepresented (for example module-level code, type declarations, or functions that failed or timed out), in which case the report must say so. Split out of run-report-verdict-and-coverage-metrics during the cross-check, because it is an open-ended investigation that should not hold up the layout fix.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/run.out`: `## Report Validity: degraded`, detail `represented_source_percent=61.5 below high threshold 75.0`.
- The classification is in `shatter-cli/src/commands/run.rs` `classify_validity` (degraded branch at run.rs:1706-1715), fed by `representation_spans` and the run summary built just before `run.rs:673`.
- Source findings: audit 2026-09-22 cli-ux-13. Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13.

## Acceptance criteria

This is a diagnosis issue. It closes with a written finding and, if the math is wrong, a fix with a test.

- [ ] Reproduce on the same six example files (record the examples commit and the command) and capture the JSON `unrepresented_*_lines` buckets.
- [ ] Attribute every unrepresented line to a bucket and a cause (per file, with line ranges), and state it in the close reason.
- [ ] If any attribution is wrong (lines counted as unrepresented that belong to explored functions, or non-executable lines such as imports and type declarations counted in the denominator), fix it here with a unit test on `classify_validity`/representation spans that fails on current main and passes after, and record both runs. If the fix is larger than a focused change, file it as a separate bug with the failing test attached, blocked by nothing, and close this one pointing at it.
- [ ] If the attribution is correct, record that and file (or comment on run-report-verdict-and-coverage-metrics with) the per-bucket wording the plain-language verdict must use for this case.
- [ ] `task affected` passes, with `Gates selected` recorded, if any code changed.

## Related

str-jeen.5 (introduced the validity layer). In this bucket: run-report-verdict-and-coverage-metrics.

## Priority / Type

P2, task (diagnosis). Priority matches the parent finding: a wrong representation figure would mislabel every run.
