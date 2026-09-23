---
slug: run-report-verdict-and-coverage-metrics
kind: new
title: "Run report opens with an unexplained 'degraded' verdict before its H1; explore, scan and run headline three different coverage metrics"
priority: P2
type: bug
labels: [report, ux, run, scan, explore, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Run report opens with an unexplained 'degraded' verdict before its H1; explore, scan and run headline three different coverage metrics

## Problem

**1. The verdict comes before the title and is not explained.** `shatter run .` over a directory of six TS files, all in supported languages, begins its stdout with `## Report Validity: degraded` and a one-row table whose detail reads `represented_source_percent=61.5 below high threshold 75.0`. Only after that does `# Shatter Run Report` appear. The reader sees a verdict, then the title. The verdict uses an internal field name and threshold, and does not say which source is unrepresented or why, in a directory where every file is supported. The recommended action, "Inspect unrepresented_*_lines buckets", names JSON fields the markdown report does not show.

**2. Three commands, three coverage metrics, none of them named consistently.** For the same example files:

- `explore` headlines **line** coverage: "100% coverage (7/7 lines)".
- `scan` headlines **branch** coverage: "Overall coverage (completed-functions subset): 92.5%" (37/40 branches).
- `run` totals **lines**: "80/99 81%".

`run` also uses a different default iteration budget from `scan`. For the same function, `computeStats`, run reports 31% and scan reports 63%. A user comparing commands cannot tell whether the numbers disagree or measure different things.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/run.rs:670-674` renders `render_validity_markdown(...)` and prints it with `print_markdown` *before* `print_summary_report(...)` at `run.rs:679`, which writes the H1.
- `render_validity_markdown` at `shatter-cli/src/commands/run.rs:1989-2010` writes `## Report Validity: {label}` and then a raw Reason/Detail/Recommended-action table.
- The degraded reason text is at `shatter-cli/src/commands/run.rs:1706-1715`: `represented_source_percent={rep_pct:.1} below high threshold {HIGH_REPRESENTATION_PCT:.1}` with "Inspect unrepresented_*_lines buckets ...".
- The H1 `# Shatter Run Report` is written at `shatter-cli/src/commands/scan.rs:2034` (the shared summary writer).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `run.out` has a blank line 1, `## Report Validity: degraded` on line 2, the reason row on line 6, and `# Shatter Run Report` on line 8. The total row is `| **Total** | **40** | **80/99** | **81%** |`, with `computeStats` at `5/16 | 31%`.
  - `ts-scan.out` line 28 reads `Overall coverage (completed-functions subset): 92.5%`.
  - `ts-explore.out` line 21 reads `100% coverage (7/7 lines)`.
- Source findings: audit 2026-09-22 cli-ux-13 (confirmed, P2; the cross-command metric comparison was not re-run by the verifier). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13. This was also 2026-09-04 usability-ui item 16, which was never filed.

## Acceptance criteria

- [ ] The `run` report's first non-blank line is its H1. The validity verdict follows directly under it as one plain-language sentence that states the cause and the affected files or functions. Example: "Validity: degraded. 38% of source lines are in functions that were not explored (3 failed, 2 unsupported); see Unrepresented source below." Internal field names such as `represented_source_percent` appear only in JSON output.
- [ ] The degraded cause in the audit's all-TS repro is identified and either fixed (if the representation math is wrong) or explained (if the lines really are unrepresented, for example as module-level code). Record which one in the close reason.
- [ ] `explore`, `scan` and `run` headline the same named coverage metric, or each headline states which metric it is ("line coverage", "branch coverage"). Pick one metric as the default headline for all three commands and record the choice in SPEC (reporting section) with a §8 changelog row.
- [ ] If `run` and `scan` keep different default iteration budgets, the report states the budget next to the coverage figure. Otherwise align the defaults.
- [ ] A golden or snapshot test covers the top of the `run` markdown report (H1 first, then the verdict sentence) and the headline coverage line of each of explore, scan and run on one shared example, asserting that the metric is named. The H1-first assertion must fail on current main. Existing tests at `run.rs:3490-3513` are updated.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Move the validity block into the summary writer, after the H1. Give `ValidityReason` a `human_summary` alongside the machine `detail`. Add a small shared `CoverageHeadline { metric, covered, total }` type in core that all three commands render through, so the metric name is always printed and cannot drift. Decide the default metric together with branch-metric-counts-sites, which questions what the branch metric counts.

## Out of scope

- The scan report's double H1 (`# Scan Results` then `# Shatter Scan Report`), absolute paths in function columns, and "Interesting Inputs" curation. Those are scan-report-headline-and-paths (bucket shatter-reports-and-specs).
- Changing the validity thresholds.
- Progress output (scan-progress-post-hoc).

## Related

str-jeen.5 (closed; introduced the `report_validity` layer), str-qwua7.57 (terminology unification, not metrics), str-4ad5, branch-metric-counts-sites, scan-report-headline-and-paths.

## Priority / Type

P2, bug.
