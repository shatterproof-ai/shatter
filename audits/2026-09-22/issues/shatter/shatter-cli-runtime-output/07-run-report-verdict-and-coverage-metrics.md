---
slug: run-report-verdict-and-coverage-metrics
kind: new
title: "Run report opens with an unexplained 'degraded' verdict, in internal field names, before its H1"
priority: P2
type: bug
labels: [report, ux, run, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Run report opens with an unexplained 'degraded' verdict, in internal field names, before its H1

## Problem

`shatter run .` over a directory of six TS files, all in supported languages, begins its stdout with `## Report Validity: degraded` and a one-row table whose detail reads `represented_source_percent=61.5 below high threshold 75.0`. Only after that does `# Shatter Run Report` appear. The reader sees a verdict, then the title. The verdict uses an internal field name and threshold, and does not say which source is unrepresented or why, in a directory where every file is supported. The recommended action, "Inspect unrepresented_*_lines buckets", names JSON fields the markdown report does not show.

Scope note: this draft was split during the cross-check (the slug is kept for stability; the metric part moved out). Why the audit's all-TS repro was classified degraded is run-validity-degraded-cause-diagnosis; the three commands headlining three different coverage metrics is coverage-headline-metric-unification. This issue covers the placement and wording of the verdict only.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/run.rs:673-674` renders `render_validity_markdown(...)` and prints it with `print_markdown` *before* `print_summary_report(...)` at `run.rs:679`, which writes the H1.
- `render_validity_markdown` at `shatter-cli/src/commands/run.rs:1989-2010` writes `## Report Validity: {label}` and then a raw Reason/Detail/Recommended-action table.
- The degraded reason text is at `shatter-cli/src/commands/run.rs:1706-1715`: `represented_source_percent={rep_pct:.1} below high threshold {HIGH_REPRESENTATION_PCT:.1}` with "Inspect unrepresented_*_lines buckets ...".
- The H1 `# Shatter Run Report` is written at `shatter-cli/src/commands/scan.rs:2034` (the shared summary writer).
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `run.out` has a blank line 1, `## Report Validity: degraded` on line 2, the reason row on line 6, and `# Shatter Run Report` on line 8.
- Source findings: audit 2026-09-22 cli-ux-13 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F13. This was also 2026-09-04 usability-ui item 16, which was never filed.
- Existing tests for the block: `shatter-cli/src/commands/run.rs:3484-3513` (`render_validity_markdown_emits_verdict_and_reasons`, `..._high_run_collapses_to_no_issues`, `..._escapes_pipe_in_detail`).

## Acceptance criteria

- [ ] The `run` markdown report's first non-blank line is its H1 (`# Shatter Run Report`). The validity verdict follows directly under it.
- [ ] The verdict is one plain-language sentence per reason, built at render time from the reason and the run summary (if stored on `ValidityReason`, it is `#[serde(skip)]`; the machine `code`/`detail`/`recommended_action` stay unchanged in JSON). Example: "Validity: degraded. 38% of source lines are in functions that were not explored (3 failed, 2 unsupported); see Unrepresented source below." The sentence names counts per bucket and the affected files or functions (up to a small limit, then "and N more").
- [ ] The markdown never contains `represented_source_percent`, `unrepresented_*_lines` or other JSON field names. A test asserts this for the degraded, low and high tiers. If the recommended action points the reader at a section, that section exists in the markdown report.
- [ ] JSON output (`report_validity`, `validity_reasons`) is byte-identical before and after for the same run summary (unit test on the serializer), so machine consumers are unaffected.
- [ ] Snapshot test of the top of the `run` markdown report for a fixture summary with one degraded reason: H1 first, then the verdict sentence. The H1-first assertion fails on current main and passes after the fix; record both in the close reason. The existing tests at `run.rs:3484-3513` are updated to the new shape.
- [ ] `task walkthrough` passes and `/walkthrough-review` has been run. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Move the validity block into the summary writer, after the H1 (`scan.rs:2034` is the shared writer), instead of printing it from `run.rs` before calling `print_summary_report`.

## Out of scope

- Why the all-TS repro is degraded (run-validity-degraded-cause-diagnosis).
- Coverage metric naming and unification across commands (coverage-headline-metric-unification).
- The scan report's double H1, absolute paths in function columns, and "Interesting Inputs" curation (scan-report-headline-and-paths, bucket shatter-reports-and-specs).
- Changing the validity thresholds.
- Progress output (scan-progress-post-hoc).

## Related

str-jeen.5 (closed; introduced the `report_validity` layer), str-4ad5, scan-report-headline-and-paths. In this bucket: run-validity-degraded-cause-diagnosis, coverage-headline-metric-unification.

## Priority / Type

P2, bug.
